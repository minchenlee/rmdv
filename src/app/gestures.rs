//! Pointer gestures that the reader and the editor turn into discrete steps
//! (MDV-048 P5): pinch and ⌘+scroll change the reader font size by the
//! keyboard's ⌘+ and ⌘− steps, and a trackpad scrolls the editor by the
//! finger distance.
//!
//! A font step clears the height cache and rebuilds the virtual window, so a
//! gesture never applies one step per event. [`ZoomAccumulator`] adds the
//! deltas up, and one short timer applies the whole steps it holds, at most
//! [`MAX_STEPS_PER_FLUSH`] at a time.

use super::{App, Message};
use iced::Task;
use std::time::{Duration, Instant};

/// Trackpad scroll distance, in logical pixels, for one font step.
pub(crate) const WHEEL_PX_PER_STEP: f32 = 60.0;
/// Mouse wheel notches for one font step.
pub(crate) const WHEEL_LINES_PER_STEP: f32 = 1.0;
/// Pinch magnification for one font step. A step scales the font by 1.1,
/// and a magnification of 0.1 makes a native view 10 % larger.
pub(crate) const PINCH_PER_STEP: f32 = 0.1;
/// A pause this long ends a gesture: what it left over starts over.
pub(crate) const GESTURE_GAP: Duration = Duration::from_millis(250);
/// The timer that applies the steps collected meanwhile.
pub(crate) const FLUSH_DELAY: Duration = Duration::from_millis(60);
/// The most font steps one flush applies; a fast gesture gives a few steps,
/// not a jump to the limit.
pub(crate) const MAX_STEPS_PER_FLUSH: i32 = 2;

/// Collects pinch and ⌘+scroll deltas, in font steps, until a flush.
#[derive(Debug, Default)]
pub(crate) struct ZoomAccumulator {
    /// Font steps collected and not applied yet; a fraction stays between
    /// flushes of one gesture.
    pending: f32,
    /// When the last delta arrived; a longer pause ends the gesture.
    last: Option<Instant>,
    /// A flush timer is running.
    flush_scheduled: bool,
}

impl ZoomAccumulator {
    /// Adds `steps` font steps (positive: larger) at `now`. A pause longer
    /// than [`GESTURE_GAP`] or a change of direction starts over. Returns
    /// `true` when the caller must start the flush timer.
    pub(crate) fn add(&mut self, steps: f32, now: Instant) -> bool {
        if !steps.is_finite() || steps == 0.0 {
            return false;
        }
        let paused = self
            .last
            .is_none_or(|last| now.saturating_duration_since(last) > GESTURE_GAP);
        let reversed = self.pending != 0.0 && self.pending.signum() != steps.signum();
        if paused || reversed {
            self.pending = 0.0;
        }
        self.pending += steps;
        self.last = Some(now);
        !std::mem::replace(&mut self.flush_scheduled, true)
    }

    /// The whole font steps to apply now (at most [`MAX_STEPS_PER_FLUSH`]
    /// either way). The fraction of a step stays for the rest of the
    /// gesture; whole steps over the limit are dropped.
    pub(crate) fn take_steps(&mut self) -> i32 {
        self.flush_scheduled = false;
        let whole = self.pending.trunc();
        self.pending -= whole;
        (whole as i32).clamp(-MAX_STEPS_PER_FLUSH, MAX_STEPS_PER_FLUSH)
    }

    /// Forget the gesture (a font limit, a reset, another document).
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    #[cfg(test)]
    pub(crate) fn pending(&self) -> f32 {
        self.pending
    }

    #[cfg(test)]
    pub(crate) fn flush_scheduled(&self) -> bool {
        self.flush_scheduled
    }
}

/// Font steps for one ⌘+wheel event: scrolling up (away from the user, or
/// two fingers moving down with natural scrolling) makes the font larger,
/// as it zooms the mind map in.
pub(crate) fn wheel_zoom_steps(delta: iced::mouse::ScrollDelta) -> f32 {
    match delta {
        iced::mouse::ScrollDelta::Lines { y, .. } => y / WHEEL_LINES_PER_STEP,
        iced::mouse::ScrollDelta::Pixels { y, .. } => y / WHEEL_PX_PER_STEP,
    }
}

/// Font steps for one native pinch magnification delta.
pub(crate) fn pinch_zoom_steps(magnification: f32) -> f32 {
    magnification / PINCH_PER_STEP
}

/// Editor lines for one trackpad scroll of `dy` logical pixels: the finger
/// distance over the line height, so the text moves with the fingers.
/// `partial` carries the fraction of a line to the next event and starts
/// over when the direction changes. Positive lines scroll down, as in
/// `text_editor::Action::Scroll`.
pub(crate) fn editor_scroll_lines(partial: &mut f32, dy: f32, line_height: f32) -> i32 {
    if !dy.is_finite() || !line_height.is_finite() || line_height <= 0.0 {
        return 0;
    }
    let lines = -dy / line_height;
    if *partial != 0.0 && partial.signum() != lines.signum() {
        *partial = 0.0;
    }
    let total = *partial + lines;
    let whole = total.trunc();
    *partial = total - whole;
    whole as i32
}

impl App {
    /// Pinch and ⌘+scroll change the font where they act on the reader: a
    /// document on screen and no overlay or dialog over it.
    fn reader_zoom_allowed(&self) -> bool {
        self.reader_shows_document() && !self.history_blocked()
    }

    /// Collect `steps` font steps; the first delta of a batch starts the
    /// flush timer.
    pub(super) fn reader_zoom_by(&mut self, steps: f32) -> Task<Message> {
        if !self.reader_zoom_allowed() {
            self.reader_zoom.clear();
            return Task::none();
        }
        if self.reader_zoom.add(steps, Instant::now()) {
            Task::perform(async { tokio::time::sleep(FLUSH_DELAY).await }, |_| {
                Message::ReaderZoomFlush
            })
        } else {
            Task::none()
        }
    }

    /// Apply the whole steps collected since the timer started, with one
    /// rebuild, the same steps and limits as ⌘+ and ⌘−. Prefs are written
    /// once, [`GESTURE_GAP`] after the last flush that changed the font,
    /// not on every flush: the write syncs the file to disk.
    pub(super) fn reader_zoom_flush(&mut self) -> Task<Message> {
        let steps = self.reader_zoom.take_steps();
        if steps == 0 || !self.reader_zoom_allowed() {
            return Task::none();
        }
        let before = self.font_scale;
        let size = self.step_font_scale(steps);
        if self.font_scale == before {
            // At a limit: the rest of the gesture would only push on it.
            self.reader_zoom.clear();
            return Task::none();
        }
        self.reader_zoom_save = self.reader_zoom_save.wrapping_add(1);
        let seq = self.reader_zoom_save;
        let save = Task::perform(async { tokio::time::sleep(GESTURE_GAP).await }, move |_| {
            Message::ReaderZoomSave(seq)
        });
        Task::batch([self.font_size_changed(format!("Font {:.0} px", size)), save])
    }

    /// The trailing prefs write of a reader zoom gesture: only for the
    /// latest flush, so a gesture writes once after it pauses.
    pub(super) fn reader_zoom_save(&mut self, seq: u64) -> Task<Message> {
        if seq == self.reader_zoom_save {
            self.save_prefs();
        }
        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::mouse::ScrollDelta;

    fn ms(start: Instant, millis: u64) -> Instant {
        start + Duration::from_millis(millis)
    }

    #[test]
    fn steps_apply_only_after_a_whole_threshold() {
        let t = Instant::now();
        let mut zoom = ZoomAccumulator::default();
        assert!(zoom.add(0.5, t), "the first delta starts the flush timer");
        assert_eq!(zoom.take_steps(), 0, "half a step changes nothing");
        assert!(zoom.add(0.6, ms(t, 10)));
        assert_eq!(zoom.take_steps(), 1);
        assert!(
            (zoom.pending() - 0.1).abs() < 1e-4,
            "the fraction stays for the rest of the gesture: {}",
            zoom.pending()
        );
    }

    #[test]
    fn a_fast_gesture_gives_a_few_steps_and_one_timer() {
        let t = Instant::now();
        let mut zoom = ZoomAccumulator::default();
        let started: Vec<bool> = (0..10).map(|i| zoom.add(0.5, ms(t, i))).collect();
        assert_eq!(
            started.iter().filter(|s| **s).count(),
            1,
            "one timer until the flush: {started:?}"
        );
        assert!(zoom.flush_scheduled());
        assert_eq!(zoom.take_steps(), MAX_STEPS_PER_FLUSH);
        assert!(!zoom.flush_scheduled());
        assert_eq!(
            zoom.pending(),
            0.0,
            "whole steps over the limit are dropped"
        );
        assert!(zoom.add(-3.4, ms(t, 20)));
        assert_eq!(zoom.take_steps(), -MAX_STEPS_PER_FLUSH);
        assert!((zoom.pending() + 0.4).abs() < 1e-4);
    }

    #[test]
    fn a_change_of_direction_or_a_pause_starts_over() {
        let t = Instant::now();
        let mut zoom = ZoomAccumulator::default();
        zoom.add(0.8, t);
        zoom.add(-0.3, ms(t, 10));
        assert!(
            (zoom.pending() + 0.3).abs() < 1e-4,
            "a reversed gesture drops what the other way collected: {}",
            zoom.pending()
        );
        zoom.add(-0.6, ms(t, 20));
        let late = ms(t, 20) + GESTURE_GAP + Duration::from_millis(1);
        zoom.add(-0.3, late);
        assert!(
            (zoom.pending() + 0.3).abs() < 1e-4,
            "a pause ends the gesture: {}",
            zoom.pending()
        );
        zoom.add(f32::NAN, late);
        zoom.add(0.0, late);
        assert!(
            (zoom.pending() + 0.3).abs() < 1e-4,
            "no-op deltas are ignored"
        );
    }

    #[test]
    fn editor_trackpad_scroll_follows_the_finger() {
        let line = 20.15;
        let mut partial = 0.0;
        // Fingers up (content up, scroll down): y is negative.
        assert_eq!(editor_scroll_lines(&mut partial, -3.0 * line, line), 3);
        assert!(partial.abs() < 1e-4);
        // Small deltas add up to whole lines.
        let lines: i32 = (0..10)
            .map(|_| editor_scroll_lines(&mut partial, -line / 4.0, line))
            .sum();
        assert_eq!(lines, 2, "ten quarter lines are two lines and a half");
        assert!((partial - 0.5).abs() < 1e-3, "{partial}");
        // Turning back drops the half line the other way.
        assert_eq!(editor_scroll_lines(&mut partial, line * 0.75, line), 0);
        assert!((partial + 0.75).abs() < 1e-3, "{partial}");
        assert_eq!(editor_scroll_lines(&mut partial, line * 0.5, line), -1);
        // Iced's own speed would be 3 · 20.15 / 4 ≈ 15 lines for the first
        // scroll; a bad line height scrolls nothing.
        assert_eq!(editor_scroll_lines(&mut partial, -40.0, 0.0), 0);
        assert_eq!(editor_scroll_lines(&mut partial, f32::NAN, line), 0);
    }

    #[test]
    fn wheel_and_pinch_deltas_convert_to_font_steps() {
        assert_eq!(
            wheel_zoom_steps(ScrollDelta::Pixels {
                x: 0.0,
                y: WHEEL_PX_PER_STEP
            }),
            1.0
        );
        assert_eq!(
            wheel_zoom_steps(ScrollDelta::Pixels {
                x: 40.0,
                y: -WHEEL_PX_PER_STEP / 2.0
            }),
            -0.5,
            "only the vertical scroll zooms"
        );
        assert_eq!(wheel_zoom_steps(ScrollDelta::Lines { x: 0.0, y: 1.0 }), 1.0);
        assert_eq!(
            wheel_zoom_steps(ScrollDelta::Lines { x: 0.0, y: -2.0 }),
            -2.0
        );
        assert!((pinch_zoom_steps(0.05) - 0.5).abs() < 1e-6);
        assert!((pinch_zoom_steps(-0.2) + 2.0).abs() < 1e-6);
    }
}
