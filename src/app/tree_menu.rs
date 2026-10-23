//! The file tree's context menu (MDV-048 P6): a right press on a file, a
//! folder, or the empty tree space (the workspace root) opens a menu for
//! that item. The press does not move the tree cursor; the row shows a ring
//! while its menu is open.

use super::*;

impl App {
    /// Open the menu for `target` (`(path, is_dir)`), or for the workspace
    /// root when `None`.
    pub(super) fn open_tree_context_menu(
        &mut self,
        target: Option<(PathBuf, bool)>,
    ) -> Task<Message> {
        let Some(root) = self.workspace.clone() else {
            return Task::none();
        };
        let entries = match &target {
            Some((path, is_dir)) => {
                // ⌘⌥R and ⌘⌥C act on the focused file: show them only on its
                // row, where they do the same thing.
                let focused = !is_dir && self.focused_file_path().as_ref() == Some(path);
                tree_item_menu(&root, path, *is_dir, focused)
            }
            None => tree_item_menu(&root, &root, true, false),
        };
        self.open_context_menu(entries, target.map(|(path, _)| path));
        Task::none()
    }
}

/// The entries for `path` in the workspace at `root`; `path == root` is the
/// empty tree space.
fn tree_item_menu(root: &Path, path: &Path, is_dir: bool, focused: bool) -> Vec<MenuEntry> {
    let is_root = path == root;
    let copy = |label: &str, text: Option<String>| match text {
        Some(text) => MenuEntry::item(label, Message::CopyPath(text)),
        None => MenuEntry::disabled(label),
    };
    let relative = (!is_root)
        .then(|| path.strip_prefix(root).ok())
        .flatten()
        .and_then(Path::to_str)
        .map(str::to_owned);
    let mut entries = Vec::new();
    if !is_dir {
        entries.push(MenuEntry::item("Open", Message::Open(path.to_path_buf())));
    }
    entries.push(
        MenuEntry::item("Reveal in Finder", Message::RevealPath(path.to_path_buf()))
            .shortcut(focused.then_some("⌘⌥R")),
    );
    entries.push(
        copy("Copy Path", path.to_str().map(str::to_owned)).shortcut(focused.then_some("⌘⌥C")),
    );
    entries.push(copy("Copy Relative Path", relative));
    if !is_dir {
        entries.push(MenuEntry::item(
            "Add to Quick Slot",
            Message::QuickSlotAssignPath(path.to_path_buf()),
        ));
    }
    // New items go inside a folder, or next to a file.
    let folder = if is_dir {
        path
    } else {
        path.parent().unwrap_or(root)
    };
    entries.push(MenuEntry::Separator);
    entries.push(MenuEntry::item(
        "New File",
        Message::TreeNewFile(folder.to_path_buf()),
    ));
    entries.push(MenuEntry::item(
        "New Folder",
        Message::TreeNewFolder(folder.to_path_buf()),
    ));
    // The workspace root itself is not renamed or trashed from its tree.
    if is_root {
        entries.push(MenuEntry::disabled("Rename"));
        entries.push(MenuEntry::disabled("Move to Trash"));
    } else {
        entries.push(MenuEntry::item(
            "Rename",
            Message::TreeRenameStart(path.to_path_buf()),
        ));
        entries.push(MenuEntry::item(
            "Move to Trash",
            Message::TreeTrash(path.to_path_buf()),
        ));
    }
    entries
}
