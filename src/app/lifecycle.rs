//! The window's lifecycle: its launch settings, closing it, and the recent
//! files and folders it reopens.

use super::*;

/// The smallest window size. A narrower or shorter window crowds the
/// toolbar's controls and the reading column.
pub(super) const WINDOW_MIN_SIZE: iced::Size = iced::Size::new(640.0, 420.0);

/// The main window's settings. `transparent` lets whole-window glass show
/// through (macOS only).
pub fn window_settings(transparent: bool) -> iced::window::Settings {
    #[cfg(target_os = "macos")]
    let platform_specific = iced::window::settings::PlatformSpecific {
        title_hidden: true,
        titlebar_transparent: true,
        fullsize_content_view: true,
    };
    #[cfg(not(target_os = "macos"))]
    let platform_specific = iced::window::settings::PlatformSpecific::default();
    iced::window::Settings {
        platform_specific,
        transparent,
        min_size: Some(WINDOW_MIN_SIZE),
        // The app decides: unsaved edits ask before the window closes.
        exit_on_close_request: false,
        ..Default::default()
    }
}

/// How long the window must stay still before its frame is saved: one
/// prefs write per resize or move, not one per event.
pub(super) const WINDOW_FRAME_SETTLE: std::time::Duration = std::time::Duration::from_millis(500);

/// What the frame's settle timer does when it fires `since` after the
/// latest move or resize: `None` samples and saves now, `Some(wait)` fires
/// again after `wait`.
pub(super) fn window_frame_settle_step(since: std::time::Duration) -> Option<std::time::Duration> {
    (since < WINDOW_FRAME_SETTLE).then(|| WINDOW_FRAME_SETTLE - since)
}

fn window_frame_settle_after(id: iced::window::Id, delay: std::time::Duration) -> Task<Message> {
    Task::perform(
        async move {
            tokio::time::sleep(delay).await;
            id
        },
        Message::WindowFrameSettled,
    )
}

/// Ask the window whether it is full screen or maximized; the answer saves
/// the frame.
fn sample_window_frame(id: iced::window::Id) -> Task<Message> {
    iced::window::mode(id).then(move |mode| {
        let fullscreen = matches!(mode, iced::window::Mode::Fullscreen);
        iced::window::is_maximized(id).map(move |maximized| Message::WindowFrameSampled {
            fullscreen,
            maximized,
        })
    })
}

/// The launch window settings with the saved frame restored. On macOS the
/// frame goes fully onto a screen's visible area (AppKit lists the screens
/// before the window opens); elsewhere no screen list exists yet, so the
/// size is restored and the window is centered.
pub fn launch_window_settings(
    transparent: bool,
    saved: Option<crate::window_frame::WindowFrame>,
) -> iced::window::Settings {
    let screens = saved.and_then(|_| crate::window_frame::visible_screens());
    window_settings_restored(transparent, saved.as_ref(), screens.as_deref())
}

/// [`window_settings`] with `saved` restored against `screens`.
pub(super) fn window_settings_restored(
    transparent: bool,
    saved: Option<&crate::window_frame::WindowFrame>,
    screens: Option<&[crate::window_frame::ScreenRect]>,
) -> iced::window::Settings {
    use crate::window_frame::Placement;
    let mut settings = window_settings(transparent);
    let Some(saved) = saved else {
        return settings;
    };
    let restore = crate::window_frame::restore(
        saved,
        screens,
        (WINDOW_MIN_SIZE.width, WINDOW_MIN_SIZE.height),
    );
    settings.size = iced::Size::new(restore.width, restore.height);
    settings.position = match restore.placement {
        Placement::Centered => iced::window::Position::Centered,
        Placement::At { x, y } => iced::window::Position::Specific(iced::Point::new(x, y)),
    };
    settings.maximized = restore.maximized;
    settings
}

impl App {
    /// The window moved or resized: save its frame once it settles.
    /// A burst of events shares one timer.
    pub(super) fn note_window_frame_change(&mut self, id: iced::window::Id) -> Task<Message> {
        self.window_frame_changed_at = Some(std::time::Instant::now());
        if self.window_frame_settle_armed {
            return Task::none();
        }
        self.window_frame_settle_armed = true;
        window_frame_settle_after(id, WINDOW_FRAME_SETTLE)
    }

    /// The settle timer fired: wait again while the window still changes,
    /// otherwise ask the window for its state.
    pub(super) fn window_frame_settle_fired(&mut self, id: iced::window::Id) -> Task<Message> {
        let since = self
            .window_frame_changed_at
            .map_or(WINDOW_FRAME_SETTLE, |at| at.elapsed());
        match window_frame_settle_step(since) {
            Some(wait) => window_frame_settle_after(id, wait),
            None => {
                self.window_frame_settle_armed = false;
                sample_window_frame(id)
            }
        }
    }

    /// The frame to save now, or `None` to keep the saved one. Full screen
    /// and a minimized (zero-size) window keep it; a maximized window keeps
    /// the normal frame under the zoom and only marks it maximized. With no
    /// normal frame saved yet, a maximized window saves the launch size,
    /// centered: its zoomed size must not become the normal frame.
    fn current_window_frame(
        &self,
        fullscreen: bool,
        maximized: bool,
    ) -> Option<crate::window_frame::WindowFrame> {
        if fullscreen {
            return None;
        }
        if maximized {
            let launch = window_settings(false).size;
            return Some(match self.prefs.window {
                Some(saved) => crate::window_frame::WindowFrame {
                    maximized: true,
                    ..saved
                },
                None => crate::window_frame::WindowFrame {
                    width: launch.width,
                    height: launch.height,
                    x: None,
                    y: None,
                    maximized: true,
                },
            });
        }
        // A minimized window on Windows reports 0×0 at -32000, -32000.
        let size = self
            .window_size
            .filter(|size| size.width >= 1.0 && size.height >= 1.0)?;
        Some(crate::window_frame::WindowFrame {
            width: size.width,
            height: size.height,
            x: self.window_position.map(|p| p.x),
            y: self.window_position.map(|p| p.y),
            maximized,
        })
    }

    /// The window opened on a monitor of this size: shrink it to fit when
    /// it is larger (see [`crate::window_frame::fit_on_monitor`]).
    pub(super) fn fit_window_on_monitor(
        &mut self,
        id: iced::window::Id,
        monitor: Option<iced::Size>,
    ) -> Task<Message> {
        let (Some(monitor), Some(size)) = (monitor, self.window_size) else {
            return Task::none();
        };
        let Some(fit) = crate::window_frame::fit_on_monitor(
            (size.width, size.height),
            self.window_position.map(|p| (p.x, p.y)),
            (monitor.width, monitor.height),
            (WINDOW_MIN_SIZE.width, WINDOW_MIN_SIZE.height),
        ) else {
            return Task::none();
        };
        let resize = iced::window::resize(id, iced::Size::new(fit.width, fit.height));
        match fit.position {
            Some((x, y)) => {
                Task::batch([resize, iced::window::move_to(id, iced::Point::new(x, y))])
            }
            None => resize,
        }
    }

    /// Save the window's current frame when it changed, unless it is full
    /// screen.
    pub(super) fn save_window_frame(&mut self, fullscreen: bool, maximized: bool) {
        if !fullscreen {
            self.window_maximized = maximized;
        }
        let Some(frame) = self.current_window_frame(fullscreen, maximized) else {
            return;
        };
        if self.prefs.window != Some(frame) {
            self.prefs.window = Some(frame);
            self.save_prefs();
        }
    }
}

impl App {
    /// The red close button or ⌘Q: close at once, or ask first while the
    /// document has unsaved edits. `window` is `None` for a quit.
    pub(super) fn request_window_close(
        &mut self,
        window: Option<iced::window::Id>,
    ) -> Task<Message> {
        if self.window_closing {
            return Task::none();
        }
        if self.dirty {
            self.close_confirm = Some(CloseConfirm { window });
            return Task::none();
        }
        self.close_window_now(window)
    }

    /// Save, then close once that write succeeds.
    pub(super) fn close_confirm_save(&mut self) -> Task<Message> {
        let Some(confirm) = self.close_confirm.take() else {
            return Task::none();
        };
        if !self.dirty {
            return self.close_window_now(confirm.window);
        }
        if self.file.is_none() {
            // Nothing to save to; keep the edits and the window.
            return Task::none();
        }
        let save = self.update(Message::SaveFile);
        // `SaveFile` adopts the text it writes as `source`.
        self.close_after_save = Some(CloseAfterSave {
            window: confirm.window,
            text: self.source.clone(),
        });
        save
    }

    pub(super) fn close_confirm_discard(&mut self) -> Task<Message> {
        match self.close_confirm.take() {
            Some(confirm) => self.close_window_now(confirm.window),
            None => Task::none(),
        }
    }

    /// The save a close waited on succeeded: close, unless edits made
    /// meanwhile are still unsaved.
    pub(super) fn close_after_save_settled(&mut self) -> Task<Message> {
        let Some(pending) = self.close_after_save.take() else {
            return Task::none();
        };
        if self.dirty {
            return Task::none();
        }
        self.close_window_now(pending.window)
    }

    /// Close the window now: keep the Quick Slot reading position, then
    /// close. The last window closing ends the app.
    pub(super) fn close_window_now(&mut self, window: Option<iced::window::Id>) -> Task<Message> {
        self.close_confirm = None;
        self.close_after_save = None;
        self.window_closing = true;
        // A close cannot wait for the settle timer or ask the window, so it
        // saves the latest frame with the last known state.
        self.save_window_frame(self.window_fullscreen, self.window_maximized);
        let checkpoint = self.checkpoint_active_quick_slot();
        // The checkpoint mutates the in-memory slot synchronously. Persist it
        // before returning the close task so the debounce cannot lose the
        // latest reading position when the process exits immediately.
        self.persist_quick_slots_now();
        self.invalidate_pending_quick_slot_restore();
        let close = match window {
            Some(id) => iced::window::close(id),
            None => iced::window::latest().and_then(iced::window::close),
        };
        Task::batch([checkpoint, close])
    }
}

impl App {
    /// Record a file opened now in the recent list.
    pub(super) fn remember_recent_file(&mut self, path: &Path) {
        if let Some(store) = self.recent_store.as_deref() {
            self.recent = crate::recent::add_file_to(store, path).existing();
        }
    }

    /// Record a folder opened now as the workspace in the recent list.
    pub(super) fn remember_recent_folder(&mut self, path: &Path) {
        if let Some(store) = self.recent_store.as_deref() {
            self.recent = crate::recent::add_folder_to(store, path).existing();
        }
    }
}
