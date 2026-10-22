//! Small state types shared by the app's update and view code.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Rendered,
    Raw,
    Mindmap,
}

/// The views the toolbar's segmented control switches between.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarView {
    Read,
    Edit,
    Mindmap,
    FullMindmap,
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

/// A window close that waits for the user to save or drop unsaved edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CloseConfirm {
    /// The window to close; `None` for a quit (⌘Q), which closes the latest.
    pub(super) window: Option<iced::window::Id>,
}

/// A close confirmed with Save: it goes ahead once the write of `text`
/// succeeds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CloseAfterSave {
    pub(super) window: Option<iced::window::Id>,
    pub(super) text: String,
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

/// How `view` wraps the hover toolbar's band (`App::toolbar_band_slot`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolbarBandSlot {
    pub find_bar: bool,
    pub sidebar: bool,
    pub glass_corner: bool,
}

/// The most stops Back keeps; the oldest drop off first.
pub(super) const HISTORY_LIMIT: usize = 100;

/// One stop in navigation history: a file, where its reader was scrolled
/// to, and the reading surface it showed.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct HistoryEntry {
    pub(super) path: PathBuf,
    /// Body scroll position, normalized to 0–1 like a Quick Slot's.
    pub(super) position: f32,
    /// `Rendered` or `Mindmap`; Zen edit maps to `Rendered`.
    pub(super) mode: ViewMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryDir {
    Back,
    Forward,
}

/// Back and Forward stacks, like a browser's. A new navigation clears
/// Forward; a workspace change clears both.
#[derive(Debug, Default)]
pub(super) struct NavHistory {
    pub(super) back: Vec<HistoryEntry>,
    pub(super) forward: Vec<HistoryEntry>,
    /// A Back or Forward whose file is still loading. The stacks move only
    /// when that load lands, so a newer open that supersedes it records as
    /// an ordinary navigation.
    pub(super) pending: Option<(HistoryDir, PathBuf)>,
    /// The reader position a Back or Forward is scrolling the open file
    /// to, until the reader reports a viewport of its own. A stop left
    /// before then keeps this position, not the top.
    pub(super) arriving: Option<f32>,
}

impl NavHistory {
    pub(super) fn stack(&self, dir: HistoryDir) -> &Vec<HistoryEntry> {
        match dir {
            HistoryDir::Back => &self.back,
            HistoryDir::Forward => &self.forward,
        }
    }

    /// The stack `dir` takes from and the one it gives the current stop to.
    pub(super) fn stacks_mut(
        &mut self,
        dir: HistoryDir,
    ) -> (&mut Vec<HistoryEntry>, &mut Vec<HistoryEntry>) {
        match dir {
            HistoryDir::Back => (&mut self.back, &mut self.forward),
            HistoryDir::Forward => (&mut self.forward, &mut self.back),
        }
    }

    pub(super) fn push(stack: &mut Vec<HistoryEntry>, entry: HistoryEntry) {
        stack.push(entry);
        if stack.len() > HISTORY_LIMIT {
            stack.remove(0);
        }
    }
}

/// The link under the pointer, and the destination its pill shows.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct HoveredLink {
    /// The link as written; a leave for another link leaves this one shown.
    pub(super) url: String,
    pub(super) label: String,
    /// The widget that reported the hover; only its leave hides the pill.
    pub(super) source: u64,
}

/// What the reader shows under the pointer. When it changes (another
/// file, another view, a modal, a page over the reader), a link's pill no
/// longer matches what is under the pointer. A scroll is not a change:
/// the link widgets report leaving when content moves off the pointer.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct LinkPage {
    pub(super) file: Option<PathBuf>,
    pub(super) view_mode: ViewMode,
    pub(super) full_mindmap: bool,
    pub(super) overlay: Overlay,
    /// Settings, vault search and the error card replace the reader.
    pub(super) settings_open: bool,
    pub(super) vault_open: bool,
    pub(super) error: bool,
}
