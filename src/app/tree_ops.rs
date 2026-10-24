//! File operations from the file tree's context menu (MDV-048 P6): New
//! File, New Folder, inline Rename, and Move to Trash. Every operation acts
//! inside the workspace root only, also when a symbolic link in the tree
//! leads out of it; every failure says why in a toast.

use super::*;
use std::path::Component;

#[cfg(test)]
thread_local! {
    /// Where this thread's Move to Trash calls put their items.
    static TRASHED: std::cell::RefCell<Vec<PathBuf>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The paths this thread moved to the Trash, so a test can delete them
/// from the Trash again.
#[cfg(test)]
pub(super) fn take_trashed_for_tests() -> Vec<PathBuf> {
    TRASHED.with(|trashed| std::mem::take(&mut *trashed.borrow_mut()))
}

/// The first of `Untitled.md`, `Untitled 2.md`, … (or `Untitled Folder`,
/// `Untitled Folder 2`, …) that does not exist in `dir`.
pub(super) fn untitled_path(dir: &Path, folder: bool) -> PathBuf {
    let (stem, extension) = if folder {
        ("Untitled Folder", "")
    } else {
        ("Untitled", ".md")
    };
    (1..)
        .map(|n| match n {
            1 => dir.join(format!("{stem}{extension}")),
            n => dir.join(format!("{stem} {n}{extension}")),
        })
        .find(|path| std::fs::symlink_metadata(path).is_err())
        .expect("an unused name")
}

/// Why `name` cannot name a file or folder, if it cannot.
pub(super) fn invalid_name_reason(name: &str) -> Option<String> {
    let name = name.trim();
    if name.is_empty() {
        return Some("A name can't be empty".into());
    }
    if name.contains('/') {
        return Some("A name can't contain \u{201c}/\u{201d}".into());
    }
    if name.contains('\0') {
        return Some("A name can't contain a NUL character".into());
    }
    if name == "." || name == ".." {
        return Some(format!("\u{201c}{name}\u{201d} can't be used as a name"));
    }
    None
}

/// `path` after `old` was renamed to `new`: `new` itself, or the same place
/// inside it. `None` when `path` is not `old` or inside it.
pub(super) fn rewire_path(path: &Path, old: &Path, new: &Path) -> Option<PathBuf> {
    let rest = path.strip_prefix(old).ok()?;
    Some(if rest.as_os_str().is_empty() {
        new.to_path_buf()
    } else {
        new.join(rest)
    })
}

/// The part of `name` the rename field selects, in characters: the name
/// without its extension for a file, the whole name for a folder.
pub(super) fn rename_selection(name: &str, is_dir: bool) -> (usize, usize) {
    let stem = match name.rfind('.') {
        Some(dot) if dot > 0 && !is_dir => &name[..dot],
        _ => name,
    };
    // The field counts grapheme clusters; one per char is close enough for
    // file names, and a short selection is never wrong to type over.
    (0, stem.chars().count())
}

/// `path` is the workspace root or inside it.
fn inside_workspace(root: &Path, path: &Path) -> bool {
    path.starts_with(root)
        && !path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
}

/// `dir` resolves, through any symbolic links, to the workspace root or a
/// folder inside it. The tree follows a link to a folder, so a row can name
/// a place outside the root under a path inside it.
fn resolves_inside(root: &Path, dir: &Path) -> bool {
    match (std::fs::canonicalize(root), std::fs::canonicalize(dir)) {
        (Ok(root), Ok(dir)) => dir.starts_with(root),
        _ => false,
    }
}

/// The folder that holds `path` resolves inside the workspace root.
fn parent_resolves_inside(root: &Path, path: &Path) -> bool {
    path.parent()
        .is_some_and(|parent| resolves_inside(root, parent))
}

/// The name a toast uses for `path`.
fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// `a` and `b` name the same file system entry: a case-only rename on a
/// case-insensitive volume.
fn same_entry(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (std::fs::symlink_metadata(a), std::fs::symlink_metadata(b)) {
            (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        let lower = |path: &Path| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_lowercase())
        };
        a.parent() == b.parent() && lower(a) == lower(b)
    }
}

/// Move `path` to the system Trash (macOS: `NSFileManager`, so Finder can
/// put it back, and no Finder window takes focus).
#[cfg(target_os = "macos")]
fn trash_item(path: &Path) -> Result<(), String> {
    use objc2_foundation::{NSFileManager, NSString, NSURL};
    let text = path.to_str().ok_or("the path is not valid UTF-8")?;
    let url = NSURL::fileURLWithPath(&NSString::from_str(text));
    let mut resulting = None;
    NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&url, Some(&mut resulting))
        .map_err(|error| error.localizedDescription().to_string())?;
    #[cfg(test)]
    if let Some(trashed) = resulting.as_ref().and_then(|url| url.path()) {
        TRASHED.with(|list| list.borrow_mut().push(PathBuf::from(trashed.to_string())));
    }
    let _ = resulting;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn trash_item(path: &Path) -> Result<(), String> {
    let _ = path;
    Err("Move to Trash is not supported on this platform".into())
}

impl App {
    /// The workspace root, while the file tree can change files.
    fn tree_ops_root(&self) -> Option<PathBuf> {
        if self.full_mindmap.is_some() {
            return None;
        }
        self.workspace.clone()
    }

    /// Put the folders New Folder made back into a freshly scanned tree.
    /// A folder that is gone is forgotten.
    pub(super) fn keep_new_folders_in_tree(&mut self) {
        if self.tree_new_folders.is_empty() {
            return;
        }
        self.tree_new_folders.retain(|folder| folder.is_dir());
        // Parents first, so a new folder inside a new folder finds its row.
        self.tree_new_folders
            .sort_by_key(|folder| folder.components().count());
        let Some(tree) = self.workspace_tree.as_mut() else {
            return;
        };
        for folder in &self.tree_new_folders {
            let hidden = folder
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with('.'));
            if self.show_hidden || !hidden {
                tree::insert_empty_folder(tree, folder);
            }
        }
    }

    /// The arrow keys, Space, and Enter move and open in the file tree.
    pub(super) fn tree_keys_active(&self) -> bool {
        self.file_tree_shown() && !self.reader_keys
    }

    /// The Files sidebar is on screen.
    pub(super) fn file_tree_shown(&self) -> bool {
        self.sidebar_open
            && self.workspace.is_some()
            && self.sidebar_tab == SidebarTab::Files
            && self.full_mindmap.is_none()
    }

    pub(super) fn tree_rows(&self) -> Vec<PathBuf> {
        self.workspace_tree
            .as_ref()
            .map(|root| {
                tree::flatten_with_files(root, &self.workspace_sidebar_files, &self.expanded)
                    .iter()
                    .map(|row| row.node.path().to_path_buf())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Index the workspace again after a change made here, keeping open
    /// folders. A refresh still in flight read the old tree; it is dropped.
    ///
    /// Synchronous on purpose: New File and New Folder rename the new row
    /// in place at once, and Rename and Trash must not leave a stale row
    /// that opens a missing file. The scan is bounded by the tree budgets;
    /// at the 10,000-entry budget it took 21–26 ms on a local SSD
    /// (2026-10-24), once per user action. See MDV-048's decisions.
    fn reindex_workspace(&mut self) -> Task<Message> {
        let Some(root) = self.workspace.clone() else {
            return Task::none();
        };
        self.cancel_refresh_tracking();
        match tree::build_workspace(&root, self.show_hidden) {
            Ok(snapshot) => {
                let error = self.error.take();
                let error_path = self.error_path.clone();
                self.apply_refreshed_workspace_snapshot(root, snapshot);
                // A file's error card is not the tree's to clear.
                self.error = error;
                self.error_path = error_path;
                Task::none()
            }
            Err(error) => {
                self.show_error_toast(format!("Couldn't refresh {}: {error}", root.display()))
            }
        }
    }

    /// New File / New Folder in `folder`: create the first free untitled
    /// name, show its row, and rename it in place.
    pub(super) fn create_tree_item(&mut self, folder: PathBuf, is_dir: bool) -> Task<Message> {
        let Some(root) = self.tree_ops_root() else {
            return Task::none();
        };
        let kind = if is_dir { "folder" } else { "file" };
        if !inside_workspace(&root, &folder) || !folder.is_dir() {
            return self.show_error_toast(format!(
                "Couldn't create a {kind}: {} is not a folder in this workspace",
                folder.display()
            ));
        }
        if !resolves_inside(&root, &folder) {
            return self.show_error_toast(format!(
                "Couldn't create a {kind}: {} is outside this workspace",
                display_name(&folder)
            ));
        }
        let path = untitled_path(&folder, is_dir);
        let created = if is_dir {
            std::fs::create_dir(&path)
        } else {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map(|_| ())
        };
        if let Err(error) = created {
            return self
                .show_error_toast(format!("Couldn't create {}: {error}", display_name(&path)));
        }
        if is_dir {
            self.tree_new_folders.push(path.clone());
        }
        for ancestor in tree::ancestors_of(&root, &path) {
            self.expanded.insert(ancestor);
        }
        let reindex = self.reindex_workspace();
        // The new row takes the cursor, as Finder selects a new item, and
        // scrolls into view for its rename field.
        let rows = self.tree_rows();
        let scroll = match rows.iter().position(|row| *row == path) {
            Some(index) => {
                self.tree_cursor = index;
                self.scroll_tree_to_cursor_with_len(rows.len())
            }
            None => Task::none(),
        };
        let rename = self.begin_tree_rename(path.clone());
        if self
            .tree_rename
            .as_ref()
            .is_some_and(|rename| rename.path == path)
        {
            return Task::batch([reindex, scroll, rename]);
        }
        // No row shows the new item (it is deeper than the tree goes, or past
        // its scan budget), so it cannot be renamed in place; say where it is.
        let place = folder
            .strip_prefix(&root)
            .ok()
            .filter(|relative| !relative.as_os_str().is_empty())
            .map(|relative| relative.display().to_string())
            .unwrap_or_else(|| display_name(&root));
        let toast = self.show_toast(format!("Created {} in {place}", display_name(&path)));
        Task::batch([reindex, scroll, rename, toast])
    }

    /// Rename `path` in place: its row shows a field with the name, the
    /// part before the extension selected. Nothing happens when no row
    /// shows `path`.
    pub(super) fn begin_tree_rename(&mut self, path: PathBuf) -> Task<Message> {
        let Some(root) = self.tree_ops_root() else {
            return Task::none();
        };
        if path == root || !inside_workspace(&root, &path) {
            return Task::none();
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return self.show_error_toast(format!(
                "Couldn't rename {}: the name is not valid UTF-8",
                path.display()
            ));
        };
        if !parent_resolves_inside(&root, &path) {
            return self.show_error_toast(format!(
                "Couldn't rename {}: it is outside this workspace",
                display_name(&path)
            ));
        }
        // The field lives in the item's row. Without a row there is nothing
        // to type into, and nothing would give the keys back.
        if !self.tree_rows().contains(&path) {
            return Task::none();
        }
        // A peeking tree hides once the pointer leaves it, and the rename
        // ends with the tree: pin the sidebar open, as Keep Sidebar Open does.
        let pin = if self.sidebar_open {
            Task::none()
        } else {
            self.sidebar_open = true;
            self.restore_body_scroll()
        };
        let (start, end) = rename_selection(name, path.is_dir());
        self.tree_rename = Some(TreeRename {
            value: name.to_owned(),
            path,
        });
        let id = iced::widget::Id::new(TREE_RENAME_ID);
        let focus = iced::widget::operation::focus(id.clone())
            .chain(iced::widget::operation::select_range(id, start, end));
        Task::batch([pin, focus])
    }

    /// Enter in the rename field. A bad or taken name keeps the field open
    /// with a toast that says why.
    pub(super) fn commit_tree_rename(&mut self) -> Task<Message> {
        let Some(rename) = self.tree_rename.clone() else {
            return Task::none();
        };
        let Some(root) = self.tree_ops_root() else {
            self.tree_rename = None;
            return Task::none();
        };
        let old = rename.path;
        let name = rename.value.trim();
        if old.file_name().and_then(|current| current.to_str()) == Some(name) {
            self.tree_rename = None;
            return Task::none();
        }
        if let Some(reason) = invalid_name_reason(name) {
            return self.show_error_toast(reason);
        }
        let Some(parent) = old
            .parent()
            .filter(|_| old != root && inside_workspace(&root, &old))
        else {
            self.tree_rename = None;
            return Task::none();
        };
        if !resolves_inside(&root, parent) {
            self.tree_rename = None;
            return self.show_error_toast(format!(
                "Couldn't rename {}: it is outside this workspace",
                display_name(&old)
            ));
        }
        let new = parent.join(name);
        if std::fs::symlink_metadata(&new).is_ok() && !same_entry(&old, &new) {
            return self.show_error_toast(format!("\u{201c}{name}\u{201d} already exists"));
        }
        self.tree_rename = None;
        if let Err(error) = std::fs::rename(&old, &new) {
            return self
                .show_error_toast(format!("Couldn't rename {}: {error}", display_name(&old)));
        }
        let cursor = self.tree_rows().get(self.tree_cursor).cloned();
        self.rewire_renamed(&old, &new);
        let reindex = self.reindex_workspace();
        if let Some(cursor) = cursor.map(|row| rewire_path(&row, &old, &new).unwrap_or(row)) {
            if let Some(index) = self.tree_rows().iter().position(|row| *row == cursor) {
                self.tree_cursor = index;
            }
        }
        reindex
    }

    /// Point everything that names `old` (or something inside it) at `new`:
    /// the open document, open folders, history, recent files, and Quick
    /// Slots.
    fn rewire_renamed(&mut self, old: &Path, new: &Path) {
        let rewire = |path: &Path| rewire_path(path, old, new);
        if let Some(file) = self.file.as_deref().and_then(rewire) {
            self.file = Some(file);
            self.quick_slot_relative_memo.replace(None);
            // A watcher read of the old path must not land on the new one.
            self.invalidate_pending_watcher_reload();
        }
        self.expanded = self
            .expanded
            .drain()
            .map(|path| rewire(&path).unwrap_or(path))
            .collect();
        for folder in self.tree_new_folders.iter_mut() {
            if let Some(path) = rewire_path(folder, old, new) {
                *folder = path;
            }
        }
        self.vault_collapsed = self
            .vault_collapsed
            .drain()
            .map(|path| rewire(&path).unwrap_or(path))
            .collect();
        for entry in self
            .history
            .back
            .iter_mut()
            .chain(self.history.forward.iter_mut())
        {
            if let Some(path) = rewire(&entry.path) {
                entry.path = path;
            }
        }
        if let Some((_, pending)) = self.history.pending.as_mut() {
            if let Some(path) = rewire(pending) {
                *pending = path;
            }
        }
        if let Some(store) = self.recent_store.as_deref() {
            self.recent = crate::recent::rename_in(store, old, new).existing();
        }
        self.rewire_quick_slots(old, new);
    }

    /// A slot under `old` follows it to `new`. Each slot is matched inside
    /// its own workspace root, so a slot from another workspace with the
    /// same relative path stays as it is.
    fn rewire_quick_slots(&mut self, old: &Path, new: &Path) {
        let rewire = |slot: &mut crate::quick_slots::QuickSlot| {
            if slot.root.is_empty() {
                return false;
            }
            let root = Path::new(&slot.root);
            let (Ok(old), Ok(new)) = (old.strip_prefix(root), new.strip_prefix(root)) else {
                return false;
            };
            let moved = crate::quick_slots::normalize_relative_path(&slot.relative_path)
                .and_then(|relative| rewire_path(Path::new(&relative), old, new));
            match moved {
                Some(path) => {
                    slot.relative_path = path.to_string_lossy().into_owned();
                    true
                }
                None => false,
            }
        };
        let mut changed = false;
        for index in 0..crate::quick_slots::SLOT_COUNT {
            if let Some(slot) = self.quick_slots.occupied_mut(index) {
                changed |= rewire(slot);
            }
        }
        if let Some(undo) = self.quick_slots_undo.as_mut() {
            rewire(&mut undo.slot);
            for (_, slot) in undo.additional.iter_mut() {
                rewire(slot);
            }
        }
        if changed {
            self.persist_quick_slots_now();
        }
    }

    /// Move to Trash. Refused while the item is, or holds, the open document
    /// with unsaved edits. Trashing the open document closes it.
    pub(super) fn trash_tree_item(&mut self, path: PathBuf) -> Task<Message> {
        let Some(root) = self.tree_ops_root() else {
            return Task::none();
        };
        if path == root || !inside_workspace(&root, &path) {
            return Task::none();
        }
        let name = display_name(&path);
        if !parent_resolves_inside(&root, &path) {
            return self.show_error_toast(format!(
                "Couldn't move {name} to Trash: it is outside this workspace"
            ));
        }
        let holds_open = self
            .file
            .as_deref()
            .is_some_and(|file| file.starts_with(&path));
        if holds_open && self.dirty {
            let file = self.file.as_deref().map(display_name).unwrap_or_default();
            return self.show_unsaved_edits_toast(format!(
                "unsaved edits in {file}; save or discard before moving {name} to Trash"
            ));
        }
        if let Err(error) = trash_item(&path) {
            return self.show_error_toast(format!("Couldn't move {name} to Trash: {error}"));
        }
        if self
            .tree_rename
            .as_ref()
            .is_some_and(|rename| rename.path.starts_with(&path))
        {
            self.tree_rename = None;
        }
        if holds_open {
            self.close_document();
        }
        let reindex = self.reindex_workspace();
        Task::batch([reindex, self.show_toast(format!("Moved {name} to Trash"))])
    }

    /// Close the open document and show the welcome page.
    fn close_document(&mut self) {
        self.supersede_file_loads();
        self.invalidate_pending_watcher_reload();
        self.invalidate_pending_quick_slot_restore();
        self.clear_active_quick_slot();
        if self.view_mode == ViewMode::Raw || self.editor.is_some() {
            self.leave_zen_edit_mode(false);
        }
        self.view_mode = ViewMode::Rendered;
        self.file = None;
        self.source.clear();
        self.saved_source.clear();
        self.dirty = false;
        self.body_viewport = None;
        self.is_data_doc = false;
        self.reader_select_all = false;
        self.mindmap_collapsed.clear();
        self.mindmap_selected = None;
        self.mindmap_panel_shown = None;
        self.quick_slot_relative_memo.replace(None);
        self.load_ast_from_source();
        self.rebuild_matches();
        self.error = None;
        self.error_path = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rmdv-ops-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn untitled_names_count_up_past_existing_ones() {
        let dir = scratch("untitled");
        assert_eq!(untitled_path(&dir, false), dir.join("Untitled.md"));
        assert_eq!(untitled_path(&dir, true), dir.join("Untitled Folder"));
        std::fs::write(dir.join("Untitled.md"), "").unwrap();
        std::fs::create_dir(dir.join("Untitled Folder")).unwrap();
        assert_eq!(untitled_path(&dir, false), dir.join("Untitled 2.md"));
        assert_eq!(untitled_path(&dir, true), dir.join("Untitled Folder 2"));
        std::fs::write(dir.join("Untitled 2.md"), "").unwrap();
        assert_eq!(untitled_path(&dir, false), dir.join("Untitled 3.md"));
        // A folder named like the next file still takes its name.
        std::fs::create_dir(dir.join("Untitled 3.md")).unwrap();
        assert_eq!(untitled_path(&dir, false), dir.join("Untitled 4.md"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn names_must_be_one_real_path_component() {
        for bad in ["", "  ", ".", "..", "a/b", "/", "a\0b"] {
            assert!(invalid_name_reason(bad).is_some(), "{bad:?}");
        }
        for good in ["notes.md", ".hidden", "a b", "日記.md", "...", "a\\b"] {
            assert_eq!(invalid_name_reason(good), None, "{good:?}");
        }
    }

    #[test]
    fn rewire_follows_a_rename_into_folders_but_not_siblings() {
        let old = Path::new("/w/notes");
        let new = Path::new("/w/archive");
        assert_eq!(rewire_path(old, old, new), Some(new.to_path_buf()));
        assert_eq!(
            rewire_path(Path::new("/w/notes/a/b.md"), old, new),
            Some(PathBuf::from("/w/archive/a/b.md"))
        );
        assert_eq!(rewire_path(Path::new("/w/notes-old/b.md"), old, new), None);
        assert_eq!(rewire_path(Path::new("/w/other.md"), old, new), None);
        assert_eq!(rewire_path(Path::new("/w"), old, new), None);
    }

    #[test]
    fn rename_selects_the_name_without_its_extension() {
        assert_eq!(rename_selection("notes.md", false), (0, 5));
        assert_eq!(rename_selection("archive.tar.gz", false), (0, 11));
        assert_eq!(rename_selection(".hidden", false), (0, 7));
        assert_eq!(rename_selection("README", false), (0, 6));
        assert_eq!(rename_selection("日記.md", false), (0, 2));
        assert_eq!(
            rename_selection("v1.2", true),
            (0, 4),
            "a folder: all of it"
        );
    }
}
