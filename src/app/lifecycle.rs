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
