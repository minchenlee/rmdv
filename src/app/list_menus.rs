//! Context menus on list rows (MDV-048 P6): outline rows, vault search
//! hits, and file finder rows. A right press moves no cursor or selection;
//! Open does what a click on the row does. The Command Palette lists
//! commands only, so its rows have no menu.

use super::*;

impl App {
    /// Go to Heading (a click), Copy Heading Text, Copy Link to Heading.
    pub(super) fn open_outline_context_menu(&mut self, index: usize) -> Task<Message> {
        let Some(section) = self.outline_sections.get(index) else {
            return Task::none();
        };
        let entries = vec![
            MenuEntry::item(
                "Go to Heading",
                Message::OutlineRowClicked {
                    index,
                    line: section.line,
                },
            ),
            MenuEntry::item(
                "Copy Heading Text",
                Message::CopyCode(section.title.clone()),
            ),
            menu_item(
                "Copy Link to Heading",
                self.heading_link_for(&section.title).map(Message::CopyCode),
            ),
        ];
        self.open_context_menu(entries, None);
        Task::none()
    }

    pub(super) fn open_vault_hit_context_menu(&mut self, index: usize) -> Task<Message> {
        if !self.vault_open {
            return Task::none();
        }
        let Some(hit) = self.vault_results.get(index) else {
            return Task::none();
        };
        // Open names the hit itself: a newer search can replace the results
        // while the menu is open, and an index would then name another hit.
        let open = Message::VaultOpenHitAt {
            path: hit.path.clone(),
            line: hit.line,
        };
        let entries = file_row_entries(open, &hit.path);
        self.open_context_menu(entries, None);
        Task::none()
    }

    pub(super) fn open_file_finder_context_menu(&mut self, path: PathBuf) -> Task<Message> {
        if self.overlay != Overlay::FileFinder {
            return Task::none();
        }
        let entries = file_row_entries(Message::OpenFileFinderPath(path.clone()), &path);
        self.open_context_menu(entries, None);
        Task::none()
    }
}

/// Open (`open`, what a click does), Reveal in Finder, Copy Path.
fn file_row_entries(open: Message, path: &Path) -> Vec<MenuEntry> {
    vec![
        MenuEntry::item("Open", open),
        MenuEntry::item("Reveal in Finder", Message::RevealPath(path.to_path_buf())),
        menu_item(
            "Copy Path",
            path.to_str().map(|text| Message::CopyPath(text.to_owned())),
        ),
    ]
}
