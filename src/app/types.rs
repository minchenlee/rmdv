//! Small state types shared by the app's update and view code.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Rendered,
    Raw,
    Mindmap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarTab {
    Files,
    Outline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MindmapDir {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    None,
    FolderPicker,
    FileFinder,
    Command,
    ThemePicker,
    ImageZoom,
    Shortcuts,
}

#[derive(Debug, Clone)]
pub enum ThemeEntry {
    Preset(ThemePreset),
    /// (slug, display name, palette)
    Custom(String, String, theme::Palette),
}

impl ThemeEntry {
    pub fn label(&self) -> &str {
        match self {
            ThemeEntry::Preset(p) => p.label(),
            ThemeEntry::Custom(_, n, _) => n,
        }
    }
    pub fn message(&self) -> Message {
        match self {
            ThemeEntry::Preset(p) => Message::SetTheme(*p),
            ThemeEntry::Custom(s, _, _) => Message::SetCustomTheme(s.clone()),
        }
    }
    pub fn palette(&self) -> theme::Palette {
        match self {
            ThemeEntry::Preset(p) => theme::palette_for(*p),
            ThemeEntry::Custom(_, _, pal) => *pal,
        }
    }
    pub fn matches_current(&self, current: &theme::ThemeId) -> bool {
        match (self, current) {
            (ThemeEntry::Preset(p), theme::ThemeId::Preset(c)) => p == c,
            (ThemeEntry::Custom(s, _, _), theme::ThemeId::Custom(c)) => s == c,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PendingNav {
    pub line: Option<u32>,
    pub section: Option<String>,
    /// Link `#fragment` anchor, resolved by GitHub-style slug once the target
    /// file has loaded. Distinct from `section` (exact-title IPC matching).
    pub fragment: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct PendingIpcFileOpen {
    pub(super) path: PathBuf,
    pub(super) nav: Option<PendingNav>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct PendingQuickSlotRestore {
    pub(super) generation: u64,
    pub(super) index: usize,
    pub(super) slot: crate::quick_slots::QuickSlot,
    pub(super) root_key: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PendingWatcherReload {
    pub(super) generation: u64,
    pub(super) path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRefreshFile {
    pub(super) id: u64,
    pub(super) path: PathBuf,
    pub(super) generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRefreshWorkspace {
    pub(super) id: u64,
    pub(super) path: PathBuf,
    pub(super) show_hidden: bool,
}

#[derive(Debug, Clone)]
pub(super) struct RefreshTracker {
    pub(super) id: u64,
    pub(super) has_workspace: bool,
    pub(super) has_file: bool,
    pub(super) file_skip_reason: Option<FileRefreshSkipReason>,
    pub(super) file_done: bool,
    pub(super) workspace_done: bool,
    pub(super) file_error: Option<String>,
    pub(super) workspace_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FileRefreshSkipReason {
    UnsavedEdits,
    DocumentChanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZenRestoreState {
    pub sidebar_open: bool,
    pub search_open: bool,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub id: u64,
    pub text: String,
    pub action: Option<ToastAction>,
}

#[derive(Debug, Clone)]
pub struct ToastAction {
    pub label: String,
    pub message: Message,
}
