//! Sidebar peek (MDV-048 polish): while the sidebar is hidden, the pointer at
//! the left window edge shows it over the content, without moving the
//! reader. It hides a short delay after the pointer leaves it. A peek never
//! changes `sidebar_open`, so the keyboard keeps going where it went before.

use super::*;

/// How long the peek stays after the pointer leaves it.
pub(super) const SIDEBAR_PEEK_HIDE_DELAY: std::time::Duration = TOOLBAR_HIDE_DELAY;

impl App {
    /// The hidden sidebar may peek: a workspace is open, the sidebar is
    /// hidden, and no Full Mindmap, Settings page, overlay, or confirmation
    /// card covers the window.
    pub(super) fn sidebar_peek_allowed(&self) -> bool {
        self.workspace.is_some()
            && !self.sidebar_open
            && self.full_mindmap.is_none()
            && self.overlay == Overlay::None
            && !self.settings_open
            && self.quick_slot_replace.is_none()
            && self.close_confirm.is_none()
    }

    /// The pointer entered or left the peek (its edge band or the peeking
    /// sidebar). Entering the band shows the peek at once; leaving the peek
    /// hides it after a short delay, which a return cancels.
    ///
    /// Only the peek itself confirms the hover. A new peek reports the
    /// pointer on its first event, and it reports a leave only after that,
    /// so a pointer that leaves the window across the band never produces
    /// a leave. The band's entry therefore already starts the hide delay,
    /// which the peek's own report cancels.
    pub(super) fn sidebar_peek_hover(&mut self, entered: bool) -> Task<Message> {
        if !self.sidebar_peek_allowed() {
            return Task::none();
        }
        // The peek reports the pointer once more when it is already known.
        if entered && self.sidebar_peek && self.sidebar_peek_hovered {
            return Task::none();
        }
        // An entry while the peek shows comes from the peek itself; an entry
        // while it is hidden comes from the band.
        let from_band = entered && !self.sidebar_peek;
        self.sidebar_peek_hovered = entered && !from_band;
        self.sidebar_peek_generation = self.sidebar_peek_generation.wrapping_add(1);
        if from_band {
            self.sidebar_peek = true;
        } else if entered || !self.sidebar_peek {
            return Task::none();
        }
        let generation = self.sidebar_peek_generation;
        Task::perform(
            async { tokio::time::sleep(SIDEBAR_PEEK_HIDE_DELAY).await },
            move |_| Message::SidebarPeekHideAfter(generation),
        )
    }

    /// The hide delay ran out. A delay from before the pointer came back does
    /// nothing.
    pub(super) fn sidebar_peek_hide_after(&mut self, generation: u64) {
        if generation == self.sidebar_peek_generation && !self.sidebar_peek_hovered {
            self.sidebar_peek = false;
        }
    }

    /// Drops the peek once it may no longer show: ⌘B or Keep Sidebar Open
    /// pinned the sidebar, or Full Mindmap, an overlay, or Settings opened.
    pub(super) fn sync_sidebar_peek(&mut self) {
        if self.sidebar_peek && !self.sidebar_peek_allowed() {
            self.sidebar_peek = false;
            self.sidebar_peek_hovered = false;
            // Cancels a pending hide.
            self.sidebar_peek_generation = self.sidebar_peek_generation.wrapping_add(1);
        }
    }
}
