//! Toolbar state: the hover reveal and the view switch.

use super::*;

impl App {
    /// The pointer entered or left the hover toolbar's band. Entering shows
    /// the toolbar at once; leaving hides it after a short delay, which a
    /// return to the band cancels.
    pub(super) fn toolbar_hover(&mut self, entered: bool) -> Task<Message> {
        self.toolbar_hovered = entered;
        self.toolbar_hover_generation = self.toolbar_hover_generation.wrapping_add(1);
        if entered {
            self.toolbar_revealed = true;
            return Task::none();
        }
        if !self.toolbar_revealed {
            return Task::none();
        }
        let generation = self.toolbar_hover_generation;
        Task::perform(
            async { tokio::time::sleep(TOOLBAR_HIDE_DELAY).await },
            move |_| Message::ToolbarHideAfter(generation),
        )
    }

    /// Where the hover band sits in the widget tree, or `None` while it is
    /// not drawn (Always mode, the vault search page). `view` wraps the band
    /// differently with the find bar open, beside the sidebar, and under
    /// whole-window glass; each change builds a new band.
    pub(super) fn toolbar_band_slot(&self) -> Option<ToolbarBandSlot> {
        let full_mindmap = self.full_mindmap.is_some();
        let vault_page = self.vault_open && !self.settings_open && !full_mindmap;
        if self.prefs.toolbar == crate::prefs::ToolbarMode::Always || vault_page {
            return None;
        }
        let sidebar = !full_mindmap && self.sidebar_open && self.workspace.is_some();
        Some(ToolbarBandSlot {
            find_bar: self.search_open && !full_mindmap,
            sidebar,
            glass_corner: sidebar && self.glass() == Glass::Window,
        })
    }

    /// A new band starts unhovered and never reports leaving the old one,
    /// so the hover it held is dropped with it. A band that has the pointer
    /// reports entering on the next event, which reveals the toolbar again.
    pub(super) fn sync_toolbar_band(&mut self) {
        let slot = self.toolbar_band_slot();
        if slot == self.toolbar_band_at {
            return;
        }
        self.toolbar_band_at = slot;
        self.toolbar_hovered = false;
        self.toolbar_revealed = false;
        // Cancels a pending hide.
        self.toolbar_hover_generation = self.toolbar_hover_generation.wrapping_add(1);
    }

    /// A toolbar segment: show `target`, through the same transitions as its
    /// shortcut (⌘E, ⌘M, ⌘⇧M, Esc). Settings and vault search close first
    /// so the chosen view shows.
    pub(super) fn toolbar_view(&mut self, target: ToolbarView) -> Task<Message> {
        let mut tasks = Vec::new();
        self.settings_open = false;
        if self.vault_open {
            tasks.push(self.update(Message::VaultClose));
        }
        if target == ToolbarView::FullMindmap {
            if self.full_mindmap.is_none() {
                tasks.push(self.update(Message::ToggleFullMindmap));
            }
            return Task::batch(tasks);
        }
        if self.full_mindmap.is_some() {
            tasks.push(self.update(Message::ExitFullMindmap));
            if self.full_mindmap.is_some() {
                // The exit waits for a workspace refresh; the reader shows
                // its own view when it lands.
                return Task::batch(tasks);
            }
        }
        let step = match (target, self.view_mode) {
            (ToolbarView::Read, ViewMode::Raw)
            | (ToolbarView::Edit, ViewMode::Rendered | ViewMode::Mindmap) => {
                Some(Message::ToggleViewMode)
            }
            (ToolbarView::Read, ViewMode::Mindmap)
            | (ToolbarView::Mindmap, ViewMode::Rendered | ViewMode::Raw) => {
                Some(Message::ToggleMindmap)
            }
            _ => None,
        };
        if let Some(step) = step {
            tasks.push(self.update(step));
        }
        Task::batch(tasks)
    }
}
