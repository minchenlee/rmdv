//! Persistent Quick Slot data: one global bank of nine slots.
//!
//! Each slot keeps the workspace root it was assigned in (as
//! [`workspace_key`] writes it) and its path relative to that root, so a
//! slot opens from any workspace and renames inside its workspace still
//! update it. The older per-workspace banks ([`QuickSlotsStore`]) stay in
//! the preferences file unchanged so a downgrade still finds them; the
//! global bank is seeded once from one of them ([`seed_global`]).
//!
//! The UI owns navigation and Iced state; this module deliberately contains
//! only bounded serializable data and path/context helpers so it can be tested
//! without starting a window or touching the user's real configuration.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

pub const SLOT_COUNT: usize = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SlotMode {
    #[default]
    Rendered,
    DocumentMindmap,
    FullMindmap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlotContext {
    #[serde(default)]
    pub mode: SlotMode,
    /// Rendered/document-Mindmap body position as a relative [0, 1] offset.
    #[serde(default)]
    pub body_position: f32,
    /// Document-Mindmap selection, represented by the stable parser block id.
    #[serde(default)]
    pub mindmap_selection: Option<u64>,
    #[serde(default)]
    pub mindmap_panel_open: bool,
    /// Full Mindmap selected-file preview position as a relative [0, 1] offset.
    #[serde(default)]
    pub preview_position: f32,
}

impl Default for SlotContext {
    fn default() -> Self {
        Self {
            mode: SlotMode::Rendered,
            body_position: 0.0,
            mindmap_selection: None,
            mindmap_panel_open: false,
            preview_position: 0.0,
        }
    }
}

impl SlotContext {
    pub fn normalized(mut self) -> Self {
        self.body_position = normalize_position(self.body_position);
        self.preview_position = normalize_position(self.preview_position);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuickSlot {
    /// The workspace root the slot was assigned in, as [`workspace_key`]
    /// wrote it. Every slot of the global bank has one; slots of the older
    /// per-workspace banks have none (their bank's key is their root).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub root: String,
    /// Path relative to `root`. Absolute paths are rejected by
    /// [`relative_path`] and never written by the application.
    #[serde(default)]
    pub relative_path: String,
    #[serde(default)]
    pub context: SlotContext,
}

impl QuickSlot {
    pub fn normalized(mut self) -> Option<Self> {
        self.relative_path = normalize_relative_path(&self.relative_path)?;
        if !self.root.is_empty() && !Path::new(&self.root).is_absolute() {
            return None;
        }
        self.context = self.context.normalized();
        Some(self)
    }

    /// The slot's file, resolved inside its own workspace root.
    pub fn resolve(&self) -> Option<PathBuf> {
        if self.root.is_empty() {
            return None;
        }
        resolve_path(Path::new(&self.root), &self.relative_path)
    }

    /// Whether the slot was assigned in workspace `root`, which must be
    /// canonical as [`workspace_key`] writes it (the app's open workspace
    /// is). It runs on the menu, view, and scroll paths, so it compares the
    /// saved key and never asks the filesystem.
    pub fn in_workspace(&self, root: &Path) -> bool {
        !self.root.is_empty() && Path::new(&self.root) == root
    }

    /// The slot's title: its file name.
    pub fn file_name(&self) -> String {
        Path::new(&self.relative_path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.relative_path.clone())
    }

    /// The name of the workspace folder the slot was assigned in.
    pub fn workspace_name(&self) -> String {
        Path::new(&self.root)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.root.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceSlots {
    #[serde(default)]
    pub slots: Vec<Option<QuickSlot>>,
    #[serde(default)]
    pub active: Option<usize>,
}

impl Default for WorkspaceSlots {
    fn default() -> Self {
        Self {
            slots: vec![None; SLOT_COUNT],
            active: None,
        }
    }
}

impl WorkspaceSlots {
    /// Normalize malformed/older config at the boundary: exactly nine slots,
    /// invalid paths dropped, and active indices constrained to the bank.
    pub fn normalized(mut self) -> Self {
        self.slots.truncate(SLOT_COUNT);
        self.slots.resize(SLOT_COUNT, None);
        self.slots = self
            .slots
            .into_iter()
            .map(|slot| slot.and_then(QuickSlot::normalized))
            .collect();
        if self
            .active
            .is_some_and(|index| index >= SLOT_COUNT || self.occupied(index).is_none())
        {
            self.active = None;
        }
        self
    }

    /// Normalize the global bank: as [`Self::normalized`], and a slot
    /// without an absolute workspace root is dropped (it cannot be opened).
    pub fn normalized_global(mut self) -> Self {
        for slot in self.slots.iter_mut() {
            if slot.as_ref().is_some_and(|slot| slot.root.is_empty()) {
                *slot = None;
            }
        }
        self.normalized()
    }

    pub fn occupied(&self, index: usize) -> Option<&QuickSlot> {
        self.slots.get(index).and_then(Option::as_ref)
    }

    pub fn occupied_mut(&mut self, index: usize) -> Option<&mut QuickSlot> {
        self.slots.get_mut(index).and_then(Option::as_mut)
    }

    pub fn set(&mut self, index: usize, slot: QuickSlot) -> bool {
        if index >= SLOT_COUNT {
            return false;
        }
        self.slots[index] = slot.normalized();
        self.slots[index].is_some()
    }

    pub fn clear(&mut self, index: usize) -> Option<QuickSlot> {
        if index >= SLOT_COUNT {
            return None;
        }
        let previous = self.slots[index].take();
        if self.active == Some(index) {
            self.active = None;
        }
        previous
    }

    pub fn clear_all(&mut self) -> Vec<(usize, QuickSlot)> {
        let mut removed = Vec::new();
        for index in 0..SLOT_COUNT {
            if let Some(slot) = self.clear(index) {
                removed.push((index, slot));
            }
        }
        removed
    }
}

/// The older per-workspace banks, by [`workspace_key`]. Since the global
/// bank they are only read once, to seed it, and are never written, so an
/// older build still finds them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct QuickSlotsStore {
    #[serde(default)]
    pub banks: BTreeMap<String, WorkspaceSlots>,
}

impl QuickSlotsStore {
    pub fn normalized(mut self) -> Self {
        self.banks = self
            .banks
            .into_iter()
            .map(|(root, bank)| (root, bank.normalized()))
            .collect();
        self
    }

    pub fn bank(&self, root: &Path) -> WorkspaceSlots {
        self.banks
            .get(&workspace_key(root))
            .cloned()
            .unwrap_or_default()
            .normalized()
    }

    pub fn put_bank(&mut self, root: &Path, bank: WorkspaceSlots) {
        self.banks.insert(workspace_key(root), bank.normalized());
    }
}

/// The global bank seeded from the older per-workspace banks: the bank of
/// the first workspace in `roots` (most recent first) that holds a slot,
/// each slot given that workspace as its root. Banks are never merged; with
/// no such workspace the global bank starts empty.
pub fn seed_global<'a>(
    store: &QuickSlotsStore,
    roots: impl IntoIterator<Item = &'a Path>,
) -> WorkspaceSlots {
    for root in roots {
        let key = workspace_key(root);
        let Some(bank) = store.banks.get(&key) else {
            continue;
        };
        let mut bank = bank.clone().normalized();
        if bank.slots.iter().all(Option::is_none) {
            continue;
        }
        for slot in bank.slots.iter_mut().flatten() {
            slot.root = key.clone();
        }
        return bank.normalized_global();
    }
    WorkspaceSlots::default()
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClearUndo {
    pub index: usize,
    pub slot: QuickSlot,
    pub additional: Vec<(usize, QuickSlot)>,
}

pub fn normalize_position(position: f32) -> f32 {
    if position.is_finite() {
        position.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Normalize a persisted workspace-relative path without ever permitting an
/// escape component. CurDir segments are harmless but can otherwise make a
/// stored `./notes.md` miss the canonical `notes.md` Quick Add identity.
pub fn normalize_relative_path(relative: &str) -> Option<String> {
    let path = Path::new(relative);
    if path.is_absolute() || relative.is_empty() {
        return None;
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => normalized.push(part),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    (!normalized.as_os_str().is_empty()).then(|| normalized.to_string_lossy().into_owned())
}

/// Canonical workspace identity. Existing roots use their filesystem
/// canonical path; a not-yet-created root retains a stable absolute/relative
/// representation without guessing or migrating another bank.
pub fn workspace_key(root: &Path) -> String {
    std::fs::canonicalize(root)
        .unwrap_or_else(|_| root.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

/// Return a safe relative file path only when the file belongs to the root.
pub fn relative_path(root: &Path, file: &Path) -> Option<String> {
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let file = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    let relative = file.strip_prefix(&root).ok()?;
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return None;
    }
    Some(relative.to_string_lossy().into_owned())
}

pub fn resolve_path(root: &Path, relative: &str) -> Option<PathBuf> {
    let candidate = Path::new(relative);
    if candidate.is_absolute()
        || relative.is_empty()
        || candidate.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return None;
    }
    let root = std::fs::canonicalize(root).ok()?;
    let path = root.join(candidate);

    // Inspect every descendant component with `symlink_metadata`, including
    // components whose targets do not exist yet. `Path::exists()` follows
    // symlinks and therefore skips a dangling ancestor; walking metadata keeps
    // a missing path visibly broken without allowing a later retarget outside
    // the workspace.
    let mut ancestor = path.as_path();
    loop {
        if std::fs::symlink_metadata(ancestor)
            .ok()
            .is_some_and(|metadata| metadata.file_type().is_symlink())
        {
            return None;
        }
        if ancestor == root {
            break;
        }
        ancestor = ancestor.parent()?;
    }

    if let Ok(canonical) = std::fs::canonicalize(&path) {
        return canonical.starts_with(&root).then_some(canonical);
    }

    // A missing slot is still useful to show as visibly broken, but only
    // after the nearest existing ancestor proves that the path cannot escape.
    let mut ancestor = path.as_path();
    while let Some(parent) = ancestor.parent() {
        if parent.exists() {
            let canonical_parent = std::fs::canonicalize(parent).ok()?;
            if !canonical_parent.starts_with(&root) {
                return None;
            }
            return Some(path);
        }
        ancestor = parent;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn test_root(label: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        std::env::temp_dir().join(format!(
            "rmdv-quick-slots-{label}-{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn banks_are_separate_and_always_have_nine_slots() {
        let mut store = QuickSlotsStore::default();
        let first = test_root("first");
        let second = test_root("second");
        let mut bank = WorkspaceSlots::default();
        assert!(bank.set(
            8,
            QuickSlot {
                root: String::new(),
                relative_path: "README.md".into(),
                context: SlotContext::default(),
            }
        ));
        store.put_bank(&first, bank.clone());
        assert_eq!(store.bank(&first), bank);
        assert_eq!(store.bank(&second), WorkspaceSlots::default());
        assert_eq!(store.bank(&first).slots.len(), SLOT_COUNT);
    }

    #[test]
    fn relative_paths_reject_outside_absolute_and_parent_paths() {
        let root = test_root("paths");
        let inside = root.join("docs/file.md");
        assert_eq!(
            relative_path(&root, &inside).as_deref(),
            Some("docs/file.md")
        );
        assert!(relative_path(&root, &root.join("../elsewhere.md")).is_none());
        assert!(resolve_path(&root, "../elsewhere.md").is_none());
        assert!(resolve_path(&root, "/tmp/elsewhere.md").is_none());
    }

    #[cfg(unix)]
    #[test]
    fn resolve_path_rejects_descendant_symlink_escape_and_allows_safe_missing() {
        use std::os::unix::fs::symlink;

        let root = test_root("symlink-root");
        let outside = test_root("symlink-outside");
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.md"), "secret").unwrap();
        symlink(&outside, root.join("escape")).unwrap();

        assert!(resolve_path(&root, "escape/secret.md").is_none());
        assert!(resolve_path(&root, "escape/future.md").is_none());

        let dangling_target = outside.join("nonexistent");
        symlink(&dangling_target, root.join("dangling")).unwrap();
        assert!(resolve_path(&root, "dangling/future.md").is_none());
        std::fs::create_dir_all(&dangling_target).unwrap();
        std::fs::write(dangling_target.join("future.md"), "future").unwrap();
        assert!(resolve_path(&root, "dangling/future.md").is_none());

        assert_eq!(
            resolve_path(&root, "notes/future.md"),
            Some(
                std::fs::canonicalize(&root)
                    .unwrap()
                    .join("notes/future.md"),
            )
        );

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn malformed_and_older_banks_normalize_without_panic() {
        let raw =
            r#"{"slots":[{"relative_path":"/tmp/no.md"},{"relative_path":"ok.md"}],"active":99}"#;
        let bank: WorkspaceSlots = serde_json::from_str(raw).unwrap();
        let bank = bank.normalized();
        assert_eq!(bank.slots.len(), SLOT_COUNT);
        assert!(bank.slots[0].is_none());
        assert!(bank.slots[1].is_some());
        assert_eq!(bank.active, None);
        let empty_active: WorkspaceSlots =
            serde_json::from_str(r#"{"active":0,"slots":[null]}"#).unwrap();
        assert_eq!(empty_active.normalized().active, None);
        let mut empty_active = WorkspaceSlots::default();
        empty_active.active = Some(0);
        assert!(empty_active.clear(0).is_none());
        assert_eq!(empty_active.active, None);
        let _: QuickSlotsStore = serde_json::from_str("{}").unwrap();
    }

    #[test]
    fn cur_dir_segments_normalize_without_relaxing_containment() {
        assert_eq!(
            normalize_relative_path("./notes/./today.md").as_deref(),
            Some("notes/today.md")
        );
        assert!(normalize_relative_path("../today.md").is_none());
        assert!(normalize_relative_path("/tmp/today.md").is_none());
        let slot = QuickSlot {
            root: String::new(),
            relative_path: "./notes/./today.md".into(),
            context: SlotContext::default(),
        }
        .normalized()
        .expect("safe CurDir path");
        assert_eq!(slot.relative_path, "notes/today.md");
    }

    #[test]
    fn clear_and_undo_payload_is_bounded() {
        let mut bank = WorkspaceSlots::default();
        bank.set(
            0,
            QuickSlot {
                root: String::new(),
                relative_path: "a.md".into(),
                context: SlotContext::default(),
            },
        );
        let removed = bank.clear(0).unwrap();
        assert_eq!(removed.relative_path, "a.md");
        assert!(bank.slots.iter().all(Option::is_none));
        let undo = ClearUndo {
            index: 0,
            slot: removed,
            additional: Vec::new(),
        };
        bank.set(undo.index, undo.slot);
        assert!(bank.occupied(0).is_some());
    }

    fn legacy(relative_path: &str) -> QuickSlot {
        QuickSlot {
            root: String::new(),
            relative_path: relative_path.into(),
            context: SlotContext::default(),
        }
    }

    #[test]
    fn seeding_takes_the_first_workspace_with_slots_and_never_merges() {
        let mut store = QuickSlotsStore::default();
        let (empty, first, second) = (
            test_root("seed-empty"),
            test_root("seed-first"),
            test_root("seed-second"),
        );
        store.put_bank(&empty, WorkspaceSlots::default());
        let mut bank = WorkspaceSlots::default();
        bank.set(1, legacy("a.md"));
        bank.active = Some(1);
        store.put_bank(&first, bank);
        let mut bank = WorkspaceSlots::default();
        bank.set(2, legacy("b.md"));
        store.put_bank(&second, bank);
        let before = store.clone();

        let missing = test_root("seed-missing");
        let seeded = seed_global(
            &store,
            [
                missing.as_path(),
                empty.as_path(),
                first.as_path(),
                second.as_path(),
            ],
        );

        let slot = seeded.occupied(1).expect("first bank with a slot");
        assert_eq!(slot.relative_path, "a.md");
        assert_eq!(slot.root, workspace_key(&first));
        assert_eq!(seeded.active, Some(1));
        assert!(seeded.occupied(2).is_none(), "banks are never merged");
        assert_eq!(store, before, "seeding reads the old banks only");
        assert_eq!(
            seed_global(&store, [missing.as_path()]),
            WorkspaceSlots::default()
        );
    }

    #[test]
    fn a_slot_resolves_in_its_own_root_only() {
        let root = test_root("own-root");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.md"), "a").unwrap();
        let slot = QuickSlot {
            root: workspace_key(&root),
            ..legacy("a.md")
        };
        let canonical = std::fs::canonicalize(&root).unwrap();
        assert_eq!(slot.resolve(), Some(canonical.join("a.md")));
        assert!(slot.in_workspace(&canonical));
        assert!(!slot.in_workspace(&test_root("other-root")));
        assert_eq!(slot.file_name(), "a.md");
        assert_eq!(
            slot.workspace_name(),
            canonical.file_name().unwrap().to_string_lossy()
        );
        assert_eq!(legacy("a.md").resolve(), None, "no root: nothing to open");
        assert!(QuickSlot {
            root: "relative/root".into(),
            ..legacy("a.md")
        }
        .normalized()
        .is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    /// `in_workspace` runs on the menu, view, and scroll paths, so it
    /// compares the saved key with the (canonical) open workspace and never
    /// asks the filesystem: an alias of the root does not match.
    #[cfg(unix)]
    #[test]
    fn in_workspace_compares_the_saved_key_without_resolving_aliases() {
        let root = test_root("key-only");
        std::fs::create_dir_all(&root).unwrap();
        let canonical = std::fs::canonicalize(&root).unwrap();
        let alias = test_root("key-only-alias");
        std::os::unix::fs::symlink(&canonical, &alias).unwrap();
        let slot = QuickSlot {
            root: workspace_key(&root),
            ..legacy("a.md")
        };
        assert!(slot.in_workspace(&canonical));
        assert!(!slot.in_workspace(&alias), "the alias is not canonicalized");
        std::fs::remove_file(alias).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}
