//! Two-finger swipe between pages (macOS): Back and Forward, as in Safari.
//!
//! Iced 0.14 and winit report a two-finger scroll only as wheel deltas, so
//! a swipe cannot be told apart from a horizontal scroll inside Iced. This
//! bridge watches the scroll events in AppKit, as `native_pinch` watches
//! magnification, and hands a swipe to AppKit's own tracking
//! (`-[NSEvent trackSwipeEventWithOptions:…]`) only when:
//!
//! - the user turned on "Swipe between pages" with two fingers
//!   (`NSEvent.isSwipeTrackingFromScrollEventsEnabled`),
//! - the app armed it: the reader shows a document, no overlay or dialog
//!   is open, and history has a stop that way ([`set_armed`]),
//! - the pointer is over the reader and over nothing that scrolls
//!   horizontally, such as a wide table ([`set_pointer_ok`], from the
//!   reader's wheel gate), and
//! - the gesture's first movement is horizontal.
//!
//! A completed swipe comes back through [`subscription`] as a [`Swipe`].
//! Other platforms keep the plain wheel; nothing here runs there.

#[cfg(not(test))]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU8, Ordering};

/// History has a stop behind (fingers move right).
pub const BACK: u8 = 1;
/// History has a stop ahead (fingers move left).
pub const FORWARD: u8 = 2;

/// A completed swipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Swipe {
    Back,
    Forward,
}

/// What the monitor does with a scroll event of a gesture it has not
/// decided yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// No movement yet: decide on a later event.
    Wait,
    /// This gesture scrolls; it never swipes.
    Scroll,
    /// Let AppKit track this gesture as a swipe that way.
    Track(Swipe),
}

/// Only macOS has the swipe monitor; elsewhere nothing arms a swipe or
/// probes the reader for one.
pub const SUPPORTED: bool = cfg!(target_os = "macos");

static ARMED: AtomicU8 = AtomicU8::new(0);
#[cfg(not(test))]
static POINTER_OK: AtomicBool = AtomicBool::new(false);
// Unit tests run in parallel threads, each with its own headless UI.
#[cfg(test)]
thread_local! {
    static POINTER_OK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The directions a swipe may go now ([`BACK`], [`FORWARD`]); `0` turns
/// swipes off. The app sets it after every update.
pub fn set_armed(directions: u8) {
    ARMED.store(directions, Ordering::Relaxed);
}

/// The directions a swipe may go now.
pub fn armed() -> u8 {
    ARMED.load(Ordering::Relaxed)
}

/// The pointer is over the reader and over nothing that scrolls
/// horizontally.
pub fn set_pointer_ok(ok: bool) {
    #[cfg(not(test))]
    POINTER_OK.store(ok, Ordering::Relaxed);
    #[cfg(test)]
    POINTER_OK.set(ok);
}

// Only the macOS monitor (and tests) read it.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn pointer_ok() -> bool {
    #[cfg(not(test))]
    return POINTER_OK.load(Ordering::Relaxed);
    #[cfg(test)]
    POINTER_OK.get()
}

/// Decide a gesture from its first scroll deltas (`dx` positive: the
/// content moves right, so the fingers swipe toward the previous page).
pub fn decide(dx: f64, dy: f64, directions: u8, pointer_ok: bool) -> Decision {
    if !dx.is_finite() || !dy.is_finite() || (dx == 0.0 && dy == 0.0) {
        return Decision::Wait;
    }
    if dx.abs() <= dy.abs() || !pointer_ok {
        return Decision::Scroll;
    }
    let (swipe, bit) = if dx > 0.0 {
        (Swipe::Back, BACK)
    } else {
        (Swipe::Forward, FORWARD)
    };
    if directions & bit == 0 {
        Decision::Scroll
    } else {
        Decision::Track(swipe)
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{armed, decide, pointer_ok, Decision, Swipe};
    use block2::RcBlock;
    use futures::stream::{self, BoxStream};
    use futures::StreamExt;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, Bool};
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSEvent, NSEventMask, NSEventPhase, NSEventSwipeTrackingOptions};
    use std::cell::{Cell, RefCell};
    use std::ptr::NonNull;
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::sync::{Mutex, OnceLock};
    use tokio::sync::mpsc::{self, Receiver, Sender};

    const NONE: u8 = 0;
    const BACK: u8 = 1;
    const FORWARD: u8 = 2;

    struct Channel {
        wake: Sender<()>,
        receiver: Mutex<Option<Receiver<()>>>,
        /// The last completed swipe not yet delivered.
        pending: AtomicU8,
    }

    impl Channel {
        fn send(&self, swipe: Swipe) {
            let code = match swipe {
                Swipe::Back => BACK,
                Swipe::Forward => FORWARD,
            };
            self.pending.store(code, Ordering::Release);
            // Capacity one: a single wake-up drains the pending swipe.
            let _ = self.wake.try_send(());
        }

        fn take(&self) -> Option<Swipe> {
            match self.pending.swap(NONE, Ordering::AcqRel) {
                BACK => Some(Swipe::Back),
                FORWARD => Some(Swipe::Forward),
                _ => None,
            }
        }
    }

    static CHANNEL: OnceLock<Channel> = OnceLock::new();
    thread_local! {
        // As in `native_pinch`: AppKit's monitor token retains the block;
        // keep it on the main thread for the process lifetime.
        static MONITOR: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
        // The scroll gesture now is decided: it swipes or it scrolls.
        static DECIDED: Cell<bool> = const { Cell::new(false) };
    }

    /// Register the scroll monitor while on the main thread, after Iced
    /// created the window (see `native_pinch::install`). Unit tests run off
    /// the main thread and skip it.
    pub fn install() {
        if MainThreadMarker::new().is_none() || MONITOR.with(|slot| slot.borrow().is_some()) {
            return;
        }
        let channel = CHANNEL.get_or_init(|| {
            let (wake, receiver) = mpsc::channel(1);
            Channel {
                wake,
                receiver: Mutex::new(Some(receiver)),
                pending: AtomicU8::new(NONE),
            }
        });
        let block = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            // SAFETY: AppKit passes a non-null event for a local monitor, and
            // the callback returns this same still-valid event to AppKit.
            consider(unsafe { event.as_ref() }, channel);
            event.as_ptr()
        });
        // SAFETY: The callback returns the exact event pointer AppKit gave us,
        // satisfying the method's valid-pointer contract.
        let monitor = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::ScrollWheel, &block)
        };
        if let Some(monitor) = monitor {
            MONITOR.with(|slot| *slot.borrow_mut() = Some(monitor));
        } else {
            eprintln!("rmdv: could not install macOS swipe monitor");
        }
    }

    /// Decide each trackpad gesture once, on its first movement. The event
    /// always goes on to Iced; a gesture that does not swipe scrolls as
    /// before.
    fn consider(event: &NSEvent, channel: &'static Channel) {
        let phase = event.phase();
        if phase == NSEventPhase::MayBegin || phase == NSEventPhase::Began {
            DECIDED.set(false);
        }
        // Mouse wheels and momentum scrolls carry no gesture phase.
        let gesture = phase == NSEventPhase::Began || phase == NSEventPhase::Changed;
        if !gesture || event.momentumPhase() != NSEventPhase::None || DECIDED.get() {
            return;
        }
        let decision = decide(
            event.scrollingDeltaX(),
            event.scrollingDeltaY(),
            armed(),
            pointer_ok(),
        );
        let swipe = match decision {
            Decision::Wait => return,
            Decision::Scroll => {
                DECIDED.set(true);
                return;
            }
            Decision::Track(swipe) => swipe,
        };
        DECIDED.set(true);
        if !NSEvent::isSwipeTrackingFromScrollEventsEnabled() {
            return;
        }
        track(event, swipe, channel);
    }

    /// Hand the gesture to AppKit's swipe tracking. It reports `Ended` when
    /// the user completed the swipe and `Cancelled` when it springs back.
    fn track(event: &NSEvent, swipe: Swipe, channel: &'static Channel) {
        let sent = Cell::new(false);
        let handler = RcBlock::new(
            move |_amount: f64, phase: NSEventPhase, _complete: Bool, _stop: NonNull<Bool>| {
                if phase == NSEventPhase::Ended && !sent.replace(true) {
                    channel.send(swipe);
                }
            },
        );
        // The direction locks to the first movement, and the amount stays
        // within one page either way.
        event.trackSwipeEventWithOptions_dampenAmountThresholdMin_max_usingHandler(
            NSEventSwipeTrackingOptions::LockDirection
                | NSEventSwipeTrackingOptions::ClampGestureAmount,
            -1.0,
            1.0,
            &handler,
        );
    }

    pub fn subscription() -> iced::Subscription<Swipe> {
        if CHANNEL.get().is_some() {
            iced::Subscription::run(events)
        } else {
            iced::Subscription::none()
        }
    }

    fn events() -> BoxStream<'static, Swipe> {
        let receiver = CHANNEL
            .get()
            .and_then(|channel| channel.receiver.lock().ok()?.take());
        match receiver {
            Some(receiver) => stream::unfold(receiver, |mut receiver| async {
                loop {
                    receiver.recv().await?;
                    if let Some(swipe) = CHANNEL.get().and_then(Channel::take) {
                        return Some((swipe, receiver));
                    }
                }
            })
            .boxed(),
            None => stream::empty().boxed(),
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn install() {}

    pub fn subscription() -> iced::Subscription<super::Swipe> {
        iced::Subscription::none()
    }
}

pub use imp::{install, subscription};

#[cfg(test)]
mod tests {
    use super::*;

    const BOTH: u8 = BACK | FORWARD;

    #[test]
    fn a_horizontal_start_swipes_the_way_the_fingers_move() {
        assert_eq!(decide(6.0, 1.0, BOTH, true), Decision::Track(Swipe::Back));
        assert_eq!(
            decide(-6.0, -2.0, BOTH, true),
            Decision::Track(Swipe::Forward)
        );
    }

    #[test]
    fn a_vertical_start_scrolls_and_no_movement_waits() {
        assert_eq!(decide(0.0, 0.0, BOTH, true), Decision::Wait);
        assert_eq!(decide(2.0, 5.0, BOTH, true), Decision::Scroll);
        assert_eq!(
            decide(3.0, -3.0, BOTH, true),
            Decision::Scroll,
            "a tie scrolls"
        );
        assert_eq!(decide(f64::NAN, 0.0, BOTH, true), Decision::Wait);
    }

    #[test]
    fn no_stop_that_way_or_a_horizontal_scroller_keeps_the_scroll() {
        assert_eq!(decide(6.0, 0.0, FORWARD, true), Decision::Scroll);
        assert_eq!(decide(-6.0, 0.0, BACK, true), Decision::Scroll);
        assert_eq!(decide(6.0, 0.0, 0, true), Decision::Scroll);
        assert_eq!(
            decide(6.0, 0.0, BOTH, false),
            Decision::Scroll,
            "over a wide table the table scrolls"
        );
    }
}
