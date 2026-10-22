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

/// An assignment that waits for the user to confirm replacing an occupied slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct QuickSlotReplace {
    pub(super) index: usize,
    /// Workspace-relative path the slot holds now.
    pub(super) current: String,
    /// Workspace-relative path that replaces it.
    pub(super) incoming: String,
}

/// Tells real pointer motion over a list from the row hovers that scrolling
/// under a still pointer causes, so hover moves the keyboard selection only
/// when the pointer moves.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct ListPointer {
    /// Latest pointer position over the list surface.
    last: Option<iced::Point>,
    /// The position a row hover or a keyboard move last consumed.
    consumed: Option<iced::Point>,
}

impl ListPointer {
    /// Record the pointer over the list surface. The first report after the
    /// list appears is a baseline, not motion: the pointer may rest where the
    /// list opened.
    pub(super) fn moved(&mut self, at: iced::Point) {
        if self.last.is_none() {
            self.consumed = Some(at);
        }
        self.last = Some(at);
    }

    /// Whether the pointer moved since the last hover or keyboard move.
    pub(super) fn take_motion(&mut self) -> bool {
        let moved = self.last != self.consumed;
        self.consumed = self.last;
        moved
    }

    /// The keyboard moved the selection; motion so far is spent.
    pub(super) fn settle(&mut self) {
        self.consumed = self.last;
    }
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
    /// A failure report: it stays as long as an action toast and can be
    /// closed.
    pub error: bool,
}

#[derive(Debug, Clone)]
pub struct ToastAction {
    pub label: String,
    pub message: Message,
}

/// File-finder results for one query over one workspace file list. `App`
/// keeps the latest so view and key handling reuse it instead of rescoring
/// every workspace file on each frame.
#[derive(Debug, Clone)]
pub(super) struct FileFinderMemo {
    pub(super) query: String,
    pub(super) root: Option<PathBuf>,
    pub(super) files_rev: u64,
    pub(super) files_len: usize,
    pub(super) results: Vec<(PathBuf, String, i32)>,
}
