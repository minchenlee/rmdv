use crate::ast::{Block, BlockId, Inline};
use crate::icon::{self, ic};
use crate::parser;
use crate::picker::{self, Picker, PickerMode};
use crate::render::Highlight;
use crate::search::{self, MatchPos};
use crate::theme::{self, Palette, ThemeMode, ThemePreset, Typography};
use crate::tree::{self, Node};
use crate::workspace_mindmap::{self, WorkspaceGraph, WorkspaceNodeId, WorkspaceNodeKind};
use iced::widget::{
    button, column, container, mouse_area, row as irow, scrollable, stack, text, text_input,
    Column, Space,
};
use iced::{Background, Border, Color, Element, Length, Padding, Task, Theme};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, OnceLock,
};

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

fn full_mindmap_space_message() -> Message {
    Message::FullMindmapToggleSelected
}

fn full_mindmap_preview_scroll_tag(full: &FullMindmapState) -> (u64, Option<PathBuf>, u64) {
    if let Some(request) = full.preview_identity.as_ref() {
        return (
            full.preview_namespace,
            Some(request.path.clone()),
            request.id,
        );
    }
    if let Some(request) = full.pending_preview.as_ref() {
        return (
            full.preview_namespace,
            Some(request.path.clone()),
            request.id,
        );
    }
    if let Some(request) = full.pending_preview_settle.as_ref() {
        return (
            full.preview_namespace,
            Some(request.path.clone()),
            request.id,
        );
    }
    (full.preview_namespace, None, 0)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFullMindmapOpen {
    pub id: u64,
    pub path: PathBuf,
}

/// Read-only side-panel preview identity. It is separate from a pending open:
/// selecting nodes must never replace the current document or consume its
/// dirty-file guard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFullMindmapPreview {
    pub id: u64,
    pub path: PathBuf,
}

/// Preview-owned asset identity. The namespace survives Full Mindmap
/// re-entry, while the request id changes for every accepted file preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullMindmapPreviewIdentity {
    pub namespace: u64,
    pub request: PendingFullMindmapPreview,
}

/// Ownership of one image/diagram operation in one materialized virtual
/// range. The preview request alone is insufficient: a late completion from
/// an older range must not mutate the current wave's Loading/Pending sentinel
/// or terminal-failure state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullMindmapPreviewAssetIdentity {
    pub preview: FullMindmapPreviewIdentity,
    pub range: (usize, usize),
    pub wave: u64,
}

/// Identity for the short settle window before a selected Full Mindmap file
/// starts its complete read-only preview. This is separate from
/// `PendingFullMindmapPreview`: no file read owns the navigator until this
/// debounce message is accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFullMindmapPreviewSettle {
    pub id: u64,
    pub path: PathBuf,
}

/// Identity for a bounded workspace index requested by Full Mindmap. The
/// result is applied only while this exact request still owns the navigator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFullMindmapWorkspaceLoad {
    pub id: u64,
    pub path: PathBuf,
    pub select_root: bool,
    /// Folder-picker fallback can request that a file open begin only after its
    /// parent workspace index is ready, avoiding a synchronous scan race.
    pub open_after: Option<PathBuf>,
    /// Hidden-entry refreshes keep the user's current expansion and selection
    /// after the background snapshot replaces the tree.
    pub preserve_navigation: bool,
    /// Returning to Files with a changed hidden-file filter waits for this
    /// snapshot, then reveals the refreshed sidebar directly.
    pub return_to_files_after: bool,
    /// Normal Esc/toggle exits also wait for a stale hidden sidebar snapshot,
    /// but restore the prior underlying surface instead of forcing Files.
    pub exit_after_refresh: bool,
}

/// Identity for one expanded folder's bounded branch-local discovery. The
/// workspace/filter tuple prevents a late completion from another explorer
/// generation from repopulating a collapsed or replaced branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFullMindmapFolderLoad {
    pub id: u64,
    pub workspace_root: PathBuf,
    pub folder: PathBuf,
    pub show_hidden: bool,
}

/// Identity for one candidate in a fixed delayed-reveal verification wave.
/// Unlike ordinary expanded-folder materialization, these requests are owned
/// by both the wave and the parent expansion generation. A result from a
/// collapsed/re-rooted/refiltered navigator therefore cannot reveal a shell
/// in a newer graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFullMindmapVerification {
    pub id: u64,
    pub wave_id: u64,
    pub workspace_root: PathBuf,
    pub parent: PathBuf,
    pub parent_expansion_generation: u64,
    pub folder: PathBuf,
    pub show_hidden: bool,
}

/// Fixed, bounded verification ownership. `candidates` is snapshotted at wave
/// start, so `total` never grows while completions arrive. At most
/// `FULL_MINDMAP_VERIFICATION_CONCURRENCY` requests are in flight and the
/// candidate cap leaves excess LowerBound(0) shells visible with their truthful
/// scan-limit label instead of silently dropping them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullMindmapVerificationWave {
    pub id: u64,
    pub workspace_root: PathBuf,
    pub show_hidden: bool,
    pub parent_expansion_generation: u64,
    pub candidates: Vec<PathBuf>,
    pub parent_by_candidate: HashMap<PathBuf, PathBuf>,
    pub next_index: usize,
    pub in_flight: HashSet<PathBuf>,
    pub request_ids: HashMap<PathBuf, u64>,
    pub checked: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FullMindmapProgress {
    pub wave_id: u64,
    pub checked: usize,
    pub total: usize,
}

#[derive(Debug, Clone)]
enum FullMindmapPreviewAsset {
    Image(String),
    Diagram {
        hash: u64,
        kind: crate::ast::DiagramKind,
        source: String,
    },
}

/// Compact worker-built asset index keyed by top-level preview block. The UI
/// only clones descriptors for the currently materialized virtual range;
/// nested assets in off-screen blocks never trigger an O(all-blocks) scan.
#[derive(Debug, Clone, Default)]
pub struct FullMindmapPreviewAssetIndex {
    by_block: HashMap<BlockId, Vec<FullMindmapPreviewAsset>>,
}

#[derive(Debug, Clone)]
pub enum FullMindmapPreview {
    None,
    Loading(PathBuf),
    Document {
        path: PathBuf,
        blocks: Vec<(BlockId, Block)>,
        truncated: bool,
        /// Estimate display/prefix shape built by the preview worker. `None`
        /// is retained for small test fixtures and legacy callers; runtime
        /// complete previews always carry the worker-built shape.
        shape: Option<Arc<crate::virt::VirtShape>>,
        /// Worker-built compact asset descriptors. `None` is retained for
        /// hand-authored fixtures; the runtime parser always provides it.
        assets: Option<Arc<FullMindmapPreviewAssetIndex>>,
    },
    Data {
        path: PathBuf,
        source: String,
        truncated: bool,
    },
    Error {
        path: PathBuf,
        error: String,
    },
}

/// App-level workspace navigator state. This is intentionally distinct from
/// document `ViewMode::Mindmap` and every `mindmap_*` document field.
#[derive(Clone)]
pub struct FullMindmapState {
    pub selected: Option<WorkspaceNodeId>,
    pub expanded: HashSet<PathBuf>,
    /// Visibility is an explicit user choice and must not change when focus
    /// moves between workspace nodes.
    pub panel_open: bool,
    pub panel_width: f32,
    /// Current step in the Full Mindmap ⌘⌥W width cycle. Kept separate from
    /// document Mindmap state so the two modes cannot affect one another.
    pub panel_step: usize,
    pub panel_drag: Option<(f32, Option<f32>)>,
    pub pending_open: Option<PendingFullMindmapOpen>,
    pub pending_preview_settle: Option<PendingFullMindmapPreviewSettle>,
    pub pending_preview: Option<PendingFullMindmapPreview>,
    pub pending_workspace_load: Option<PendingFullMindmapWorkspaceLoad>,
    pub pending_folder_loads: HashMap<PathBuf, PendingFullMindmapFolderLoad>,
    /// One fixed delayed-reveal wave owns unresolved LowerBound(0) shells at
    /// the currently visible expanded frontier. The wave is canceled whenever
    /// expansion/root/filter/mode ownership changes.
    pub verification_wave: Option<FullMindmapVerificationWave>,
    /// Newly materialized child shells cannot be appended to an active wave's
    /// fixed candidate denominator. They remain hidden here until that wave
    /// drains, then a fresh fixed snapshot verifies them.
    pub verification_followup_pending: bool,
    pub verification_hidden: HashSet<PathBuf>,
    /// Increments whenever the expanded frontier changes. Verification
    /// requests carry the parent generation to reject stale completions.
    pub expansion_generation: u64,
    pub materialized_folders: HashMap<PathBuf, workspace_mindmap::MaterializedFolder>,
    /// A current or previously selected file is selected only after its parent
    /// listing proves that the file is still visible under the accepted filter.
    pub deferred_file_selection: Option<PathBuf>,
    /// Exact canvas focus request for the workspace navigator. It is kept
    /// separate from `selected` so async materialization can be observed and
    /// guarded without letting a stale completion refocus the root/sibling.
    pub focus_request: Option<WorkspaceNodeId>,
    pub preview: FullMindmapPreview,
    /// Accepted preview request identity. It is separate from the local
    /// measurement generation so background workspace requests cannot rebuild
    /// an unchanged preview child tree.
    pub preview_identity: Option<PendingFullMindmapPreview>,
    pub preview_namespace: u64,
    pub preview_shape_ready: bool,
    /// Full Mindmap preview virtualization is completely separate from the
    /// document reader's window, viewport, and measured-height cache. A
    /// selected file therefore cannot move or resize the document underneath
    /// the navigator, and a later document load cannot invalidate this panel.
    pub preview_window: crate::virt::VirtWindow,
    pub preview_viewport: Option<iced::widget::scrollable::Viewport>,
    pub preview_height_cache: crate::virt::HeightCache,
    /// At most one preview measurement operation may be in flight. Scroll
    /// events can arrive faster than widget-tree operations complete; the
    /// guard coalesces those events instead of queueing an unbounded stream.
    pub preview_measurement_pending: bool,
    /// Materialized display range captured when the pending measurement was
    /// dispatched. If scrolling moves to a different range before completion,
    /// the accepted result schedules one follow-up for that current range.
    pub preview_measurement_range: Option<(usize, usize)>,
    pub preview_measurement_generation: Option<u64>,
    /// Monotonic identity for preview selection/parse/layout work. Height
    /// measurements carry it so a late operation cannot alter a new file.
    pub preview_generation: u64,
    /// Cooperative cancellation for stale preview reads/parses. The worker
    /// checks this token between bounded chunks and before expensive parsing.
    pub preview_work_epoch: Arc<AtomicU64>,
    /// Per-navigator parse capacity. Obsolete previews can never occupy a
    /// process-global slot or create an unbounded spawn_blocking backlog;
    /// waiting tasks poll the same epoch and abandon promptly when superseded.
    pub preview_parse_gate: Arc<tokio::sync::Semaphore>,
    /// One shared latest-only settle worker per Full Mindmap instance. The
    /// watch channel replaces requests instead of spawning one uncancellable
    /// sleep per key repeat.
    pub preview_settle_tx: tokio::sync::watch::Sender<Option<PendingFullMindmapPreviewSettle>>,
    pub preview_settle_worker_started: bool,
    /// Remote image loads dispatched by the preview. Values retain the exact
    /// request that owns each loading sentinel: an old completion must not
    /// remove or overwrite a newer preview's same-URL operation.
    pub preview_loading_images: HashMap<String, FullMindmapPreviewAssetIdentity>,
    /// Diagram renders use the same ownership rule because the cache key is
    /// only `(content hash, theme)` and can be reused by A -> B previews.
    pub preview_pending_diagrams: HashMap<(u64, u32), FullMindmapPreviewAssetIdentity>,
    /// Remote/diagram keys used by the accepted preview, including entries
    /// that completed. Legacy document-only completion messages carry no
    /// request identity, so these sets let us keep them from replacing an
    /// active preview-owned result while still reusing loaded cache entries.
    pub preview_asset_images: HashSet<String>,
    /// Image failures are terminal for the current preview identity and
    /// visible-range wave. A later prime may continue with other assets, but
    /// it must not immediately delete `Failed` and retry the same URL forever.
    /// The set is cleared when the identity/range/theme retry boundary changes.
    pub preview_failed_images: HashSet<String>,
    pub preview_asset_diagrams: HashSet<(u64, u32)>,
    pub preview_asset_cursor: usize,
    pub preview_asset_cursor_tag: Option<(u64, u64, (usize, usize))>,
    pub preview_asset_wave_id: u64,
    pub load_error: Option<String>,
    /// Visible workspace graphs are rebuilt only after their source/expansion
    /// changes, not every `view()` frame.
    layout_cache: std::cell::RefCell<Option<std::sync::Arc<WorkspaceGraph>>>,
    /// Generation of the visible workspace graph. The shared canvas uses this
    /// to preserve focus when async folder discovery rebuilds the graph without
    /// changing the selected path.
    pub layout_generation: u64,
}

mod image_cache;
use image_cache::IMAGE_CACHE_BYTE_BUDGET;
pub use image_cache::{ImageCache, ImageState};

const SIDEBAR_WIDTH: f32 = 280.0;
const QUICK_SLOTS_RAIL_GAP: f32 = 10.0;
const QUICK_SLOTS_RAIL_REVEAL_DELAY_MS: u64 = 500;
const READING_MAX: f32 = 780.0;
const KEYBOARD_BUTTON_HEIGHT: f32 = 26.0; // 14px text at 1.3 line-height + 4px vertical padding on each side.
const KEYBOARD_BUTTON_BOTTOM_PAD: f32 = 12.0;
const KEYBOARD_BUTTON_FOOTER_BOTTOM_PAD: f32 = 44.0;
const ZEN_EDITOR_OVERLAY_CLEARANCE: f32 = 2.0;
/// Top padding of the body scrollable's content (`Padding::from([56, 32])` in
/// `view`). Virt-window math works in body-relative px; every conversion from
/// scrollable offsets must subtract this.
const BODY_TOP_PAD: f32 = 56.0;
const TREE_INDENT: f32 = 14.0;
const SCROLLER_FADE_MS: u64 = 1200;
const SIDEBAR_MIN: f32 = 160.0;
const SIDEBAR_MAX: f32 = 600.0;
const MIND_PANEL_DEFAULT: f32 = 380.0;
const MIND_PANEL_MIN: f32 = 240.0;
/// Window-width fractions cycled by ⌘⌥W: 1/3, 1/2, 2/3.
const MIND_PANEL_FRACS: [f32; 3] = [1.0 / 3.0, 0.5, 2.0 / 3.0];
const MIND_PANEL_MAX_BLOCKS: usize = 80;
const MIND_PANEL_MAX_TEXT_BYTES: usize = 24 * 1024;
/// Kept only for the stale-worker regression fixture. Runtime preview parsing
/// is always dispatched to the guarded worker, regardless of source length.
#[cfg(test)]
const FULL_MINDMAP_PREVIEW_STALE_SOURCE_BYTES: usize = 64 * 1024;
const FULL_MINDMAP_PREVIEW_CANCELLED: &str = "full mindmap preview superseded";
const FULL_MINDMAP_PREVIEW_READ_CHUNK_BYTES: usize = 64 * 1024;
/// The document Mindmap panel keeps its existing rendered-content debounce
/// cadence, while Full Mindmap previews use a longer quiet window so rapid
/// workspace navigation does not start unnecessary reads.
const MINDMAP_PANEL_SETTLE_MS: u64 = 75;
const FULL_MINDMAP_PREVIEW_SETTLE_MS: u64 = 300;
/// Maximum asset descriptors dispatched by one visible-range pass. Further
/// blocks are picked up by a later scroll/remeasure pass, keeping task and
/// cache pressure bounded for asset-heavy Markdown.
const FULL_MINDMAP_PREVIEW_ASSET_BATCH: usize = 64;
/// Delayed-reveal verification is deliberately small and fixed. Four bounded
/// filesystem workers keep the UI responsive; the candidate cap bounds both
/// queue memory and total extra scans. Candidates beyond the cap stay visible
/// with their `scan limit reached` lower-bound label.
const FULL_MINDMAP_VERIFICATION_CONCURRENCY: usize = 4;
const FULL_MINDMAP_VERIFICATION_MAX_CANDIDATES: usize = 256;

/// Keep the raw editor's final line above the floating keyboard shortcut
/// button. Its footer-aware bottom offset is also the clearance needed for
/// the status footer beneath it.
fn zen_editor_bottom_inset(footer_visible: bool) -> f32 {
    let shortcut_bottom_pad = if footer_visible {
        KEYBOARD_BUTTON_FOOTER_BOTTOM_PAD
    } else {
        KEYBOARD_BUTTON_BOTTOM_PAD
    };
    shortcut_bottom_pad + KEYBOARD_BUTTON_HEIGHT + ZEN_EDITOR_OVERLAY_CLEARANCE
}

fn mindmap_panel_width_for_step(step: usize, window_size: Option<iced::Size>) -> f32 {
    window_size
        .map(|size| size.width * MIND_PANEL_FRACS[step % MIND_PANEL_FRACS.len()])
        .unwrap_or(MIND_PANEL_DEFAULT)
        .max(MIND_PANEL_MIN)
}

fn mindmap_panel_width_for_drag(origin: f32, anchor: f32, cursor_x: f32) -> f32 {
    (origin + anchor - cursor_x).max(MIND_PANEL_MIN)
}

fn editor_font() -> iced::Font {
    iced::Font {
        family: iced::font::Family::Name("JetBrains Mono"),
        weight: iced::font::Weight::Normal,
        stretch: iced::font::Stretch::Normal,
        style: iced::font::Style::Normal,
    }
}

mod keys;
use keys::*;

fn quick_slots_rail_left_offset(sidebar_open: bool, sidebar_width: f32, full_mindmap: bool) -> f32 {
    if sidebar_open && !full_mindmap {
        sidebar_width + QUICK_SLOTS_RAIL_GAP
    } else {
        QUICK_SLOTS_RAIL_GAP
    }
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

#[derive(Debug, Clone)]
pub enum Message {
    Open(PathBuf),
    /// File-finder activation. Full Mindmap Mode intercepts this narrow path
    /// so fallback opens retain its async dirty-file safety contract.
    OpenFileFinderPath(PathBuf),
    OpenWorkspace(PathBuf),
    OpenFolderPicker,
    OpenFileFinder,
    OpenCommandPalette,
    OpenThemePicker,
    OpenVaultSearch,
    Refresh,
    RevealFileInFinder,
    CopyFilePath,
    VaultQueryChanged(String),
    /// Run the search for the current query (Enter when the query has changed).
    VaultRunSearch,
    /// Enter in the vault page: search if the query was edited, else open the hit.
    VaultEnter,
    VaultSearchDone(crate::vault_search::VaultResults),
    VaultMove(isize),
    VaultOpenSelected,
    VaultOpenHit(usize),
    VaultToggleFile(PathBuf),
    VaultClose,
    /// Apply a measured absolute scroll offset to the vault results page.
    VaultScrollTo(f32),
    ToggleShortcuts,
    /// Activate the slot addressed by its zero-based rail index.
    QuickSlotActivate(usize),
    /// Assign/overwrite the slot with the current reading context.
    QuickSlotAssign(usize),
    QuickSlotClear(usize),
    QuickSlotClearAll,
    QuickSlotUndo,
    QuickSlotNew,
    QuickSlotClose,
    QuickSlotCloseWindow,
    QuickSlotCycle(i8),
    /// Primary modifier press/release controls the transient rail.
    QuickSlotsModifier(bool),
    /// Delayed reveal for the transient rail; the generation ignores a
    /// modifier release or a newer press that superseded this timer.
    QuickSlotsModifierReveal(u64),
    /// Debounced persistence generation for ordinary scrolling checkpoints.
    QuickSlotsPersist(u64),
    /// Internal handoff after entering/exiting Full Mindmap for a slot.
    QuickSlotRestorePending,
    /// Identity-bearing file read for a non-Full-Mindmap slot activation.
    QuickSlotFileLoaded {
        index: usize,
        slot: crate::quick_slots::QuickSlot,
        result: Result<(PathBuf, String), String>,
    },
    CloseOverlay,
    PickerNavigate(PathBuf),
    PickerParent,
    PickerHome,
    PickerSelectFolderHere,
    /// Picker chose a file: open it AND select its parent as workspace.
    PickerOpenFile(PathBuf),
    OverlayQueryChanged(String),
    OverlayMove(isize),
    OverlayConfirm,
    OverlayDescend,
    FileLoaded(Result<(PathBuf, String), String>),
    FileLoadCompleted {
        generation: u64,
        result: Result<(PathBuf, String), String>,
    },
    RefreshFileLoaded {
        request: PendingRefreshFile,
        result: Result<(PathBuf, String), String>,
    },
    RefreshWorkspaceLoaded {
        request: PendingRefreshWorkspace,
        result: Result<(PathBuf, tree::WorkspaceSnapshot), String>,
    },
    FileChanged(PathBuf),
    /// Identity-bearing completion for a file watcher reload. A watcher read
    /// must not share the generic navigation completion path: an explicit
    /// slot/manual open may own the reader by the time it finishes.
    FileChangedLoaded {
        request: PendingWatcherReload,
        result: Result<(PathBuf, String), String>,
    },
    CheckClipboardCopy(String),
    ClipboardCopyChecked {
        expected: String,
        actual: Option<String>,
    },
    OpenLink(String),
    ToggleTheme,
    SetTheme(ThemePreset),
    SetCustomTheme(String),
    ReloadThemes,
    ThemeFilesChanged,
    OpenThemesDir,
    ToggleSidebar,
    SetSidebarTab(SidebarTab),
    /// Toggle visibility of dot-prefixed entries in tree + picker.
    ToggleHidden,
    TreeToggle(PathBuf),
    TreeMove(isize),
    TreeActivate,
    OutlineMove(isize),
    OutlineActivate,
    ScrollToLine(u32),
    TreeToggleAtCursor,
    CopyTreePath,
    ScrollBy(f32),
    ScrollToTop,
    ScrollToBottom,
    ToggleSearch,
    QueryChanged(String),
    NextMatch,
    PrevMatch,
    TreeScrolled(iced::widget::scrollable::Viewport),
    OutlineScrolled(iced::widget::scrollable::Viewport),
    OverlayScrolled(iced::widget::scrollable::Viewport),
    VaultScrolled(iced::widget::scrollable::Viewport),
    BodyScrolled(iced::widget::scrollable::Viewport),
    TableScrolled,
    ScrollerTick,
    CopyCode(String),
    SidebarDragStart,
    SidebarDragMove(f32),
    SidebarDragEnd,
    /// Deferred body scroll restore. Emitted after a toggle that reinitialises
    /// the body scrollable. `RestoreBodySnap(y)` uses relative offset [0..1];
    /// `RestoreBodyScroll(y)` uses absolute px offset.
    RestoreBodySnap(f32),
    RestoreBodyScroll(f32),
    /// Real laid-out heights for windowed blocks, harvested by a widget
    /// operation after a virt-window rebuild. Feeds `HeightCache` so prefix
    /// estimates converge to real geometry. The `f32` is the body offset at
    /// dispatch time: scroll-anchoring compensation is only valid if the
    /// viewport hasn't moved since (a nav jump in between would make the
    /// compensation fight the landing).
    BlockHeightsMeasured(Vec<(crate::ast::BlockId, f32)>, f32),
    ToastExpire(u64),
    /// Install the CLI symlink from the packaged app.
    InstallCli,
    /// Result of the asynchronous CLI installation attempt.
    CliInstallFinished(Result<(), String>),
    /// An update was downloaded + verified and is ready to install.
    UpdateAvailable(crate::update::ReadyUpdate),
    /// User confirmed install: self-replace + relaunch.
    InstallUpdate,
    /// User dismissed the update banner.
    DismissUpdate,
    ImageFetched(String, Result<Vec<u8>, String>),
    SvgRasterized(String, Result<(Vec<u8>, u32, u32), String>),
    OpenImageZoom(String),
    ToggleViewMode,
    FontSizeUp,
    FontSizeDown,
    FontSizeReset,
    ToggleFooter,
    ToggleMindmap,
    /// A macOS magnification delta from the native event bridge. The shared
    /// canvas consumes the cumulative stream on its next redraw.
    MindmapNativePinch(f32),
    MindmapToggleNode(crate::ast::BlockId),
    MindmapSelectLeaf(crate::ast::BlockId),
    MindmapDeselect,
    MindmapNavigate(MindmapDir),
    MindmapPanelSettle(u64),
    MindmapToggleSelected,
    MindmapPanelDragStart(f32),
    MindmapPanelDragMove(f32),
    MindmapPanelDragEnd,
    ToggleMindmapAutocenter,
    ToggleMindmapPanel,
    MindmapCyclePanelWidth,
    ToggleFullMindmap,
    ExitFullMindmap,
    FullMindmapToggleNode(WorkspaceNodeId),
    FullMindmapSelectNode(WorkspaceNodeId),
    FullMindmapDeselect,
    FullMindmapNavigate(MindmapDir),
    FullMindmapDiveWorkspace(WorkspaceNodeId),
    FullMindmapActivate,
    FullMindmapToggleSelected,
    FullMindmapSelectRoot,
    FullMindmapSetRoot(PathBuf),
    FullMindmapWorkspaceParent,
    FullMindmapReturnToFiles,
    FullMindmapTogglePanel,
    FullMindmapCyclePanelWidth,
    FullMindmapPanelDragStart(f32),
    FullMindmapPanelDragMove(f32),
    FullMindmapPanelDragEnd,
    FullMindmapFileLoaded {
        request: PendingFullMindmapOpen,
        result: Result<(PathBuf, String), String>,
    },
    FullMindmapPreviewSettle {
        request: PendingFullMindmapPreviewSettle,
    },
    FullMindmapPreviewLoaded {
        request: PendingFullMindmapPreview,
        result: Result<(PathBuf, String), String>,
    },
    FullMindmapPreviewParsed {
        request: PendingFullMindmapPreview,
        result: Result<FullMindmapPreview, String>,
    },
    /// Scroll and measured-height feedback for the Full Mindmap preview panel.
    /// These messages are deliberately not `BodyScrolled`/
    /// `BlockHeightsMeasured`: the panel owns a different viewport and cache.
    FullMindmapPreviewScrolled {
        namespace: u64,
        path: Option<PathBuf>,
        identity: u64,
        viewport: iced::widget::scrollable::Viewport,
    },
    FullMindmapPreviewBlockHeightsMeasured {
        path: PathBuf,
        namespace: u64,
        identity: u64,
        generation: u64,
        measured: Vec<(crate::ast::BlockId, f32)>,
        at_offset: f32,
    },
    FullMindmapPreviewImageFetched {
        identity: FullMindmapPreviewIdentity,
        range: (usize, usize),
        wave: u64,
        url: String,
        result: Result<Vec<u8>, String>,
    },
    FullMindmapPreviewAssetWave {
        identity: FullMindmapPreviewIdentity,
        range: (usize, usize),
        wave: u64,
    },
    FullMindmapWorkspaceLoaded {
        request: PendingFullMindmapWorkspaceLoad,
        result: Result<(PathBuf, tree::WorkspaceSnapshot), String>,
    },
    FullMindmapFolderLoaded {
        request: PendingFullMindmapFolderLoad,
        result: Result<(PathBuf, tree::ExpandedFolderSnapshot), String>,
    },
    FullMindmapVerificationLoaded {
        request: PendingFullMindmapVerification,
        result: Result<(PathBuf, tree::ExpandedFolderSnapshot), String>,
    },
    WindowResized(iced::window::Id, iced::Size),
    WindowUnfocused(iced::window::Id),
    RefreshWindowMode(iced::window::Id),
    RefreshWindowModeSettled(iced::window::Id),
    WindowModeChanged(iced::window::Mode),
    /// Palette command: capture the window to a timestamped PNG on the Desktop.
    TakeScreenshot,
    /// Async result of a `Cmd::Screenshot` / `TakeScreenshot` capture. Encodes
    /// to PNG, writes the file at `pending_screenshot.0`, and either replies via
    /// the stashed IPC sender or shows a toast.
    ScreenshotCaptured(iced::window::Screenshot),
    HintSelection,
    FoldChordStart,
    FoldChordCancel,
    FoldToLevel(u8),
    ToggleFold(crate::ast::BlockId),
    HeadingHoverEnter(crate::ast::BlockId),
    HeadingHoverExit(crate::ast::BlockId),
    EditorAction(iced::widget::text_editor::Action),
    SaveFile,
    FileSaved {
        result: Result<(), String>,
        saved_source: String,
    },
    EditorUndo,
    EditorRedo,
    /// Zoom a rendered diagram into the image-viewer overlay. Looks up the
    /// `Ready` SVG bytes by content-hash and opens [`Overlay::ImageZoom`].
    DiagramZoom(u64),
    /// Copy a diagram's raw source to the system clipboard.
    CopyDiagramSource(u64),
    /// Result of an async diagram render dispatched by `prime_diagram_cache`.
    /// `theme_id` is the snapshot at dispatch time — stale results are dropped.
    DiagramRendered {
        hash: u64,
        theme_id: u32,
        result: Result<crate::diagram::RenderOutput, String>,
    },
    FullMindmapPreviewDiagramRendered {
        identity: FullMindmapPreviewIdentity,
        range: (usize, usize),
        wave: u64,
        hash: u64,
        theme_id: u32,
        result: Result<crate::diagram::RenderOutput, String>,
    },
    Noop,
    /// Toggle the `auto_focus_on_nav` preference and persist it.
    ToggleAutoFocusOnNav,
    /// IPC request from the listener subscription. The sender is wrapped in
    /// `Arc<Mutex<Option<…>>>` so the variant is `Clone` (Iced 0.14 requires
    /// `Message: Clone`). The handler takes the sender out of the mutex once
    /// to reply.
    Ipc(
        crate::ipc::Request,
        std::sync::Arc<
            std::sync::Mutex<Option<futures::channel::oneshot::Sender<crate::ipc::Response>>>,
        >,
    ),
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
struct PendingIpcFileOpen {
    path: PathBuf,
    nav: Option<PendingNav>,
}

#[derive(Debug, Clone, PartialEq)]
struct PendingQuickSlotRestore {
    generation: u64,
    index: usize,
    slot: crate::quick_slots::QuickSlot,
    root_key: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PendingWatcherReload {
    generation: u64,
    path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRefreshFile {
    id: u64,
    path: PathBuf,
    generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRefreshWorkspace {
    id: u64,
    path: PathBuf,
    show_hidden: bool,
}

#[derive(Debug, Clone)]
struct RefreshTracker {
    id: u64,
    has_workspace: bool,
    has_file: bool,
    file_skip_reason: Option<FileRefreshSkipReason>,
    file_done: bool,
    workspace_done: bool,
    file_error: Option<String>,
    workspace_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileRefreshSkipReason {
    UnsavedEdits,
    DocumentChanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZenRestoreState {
    pub sidebar_open: bool,
    pub search_open: bool,
}

pub struct App {
    pub file: Option<PathBuf>,
    pub source: String,
    /// Whitespace-separated word count of `source`, refreshed on every parse
    /// so the footer never scans the whole document inside `view()`.
    source_words: usize,
    pub ast: Vec<(BlockId, Block)>,
    pub theme_mode: ThemeMode,
    pub theme_preset: ThemePreset,
    pub palette: Palette,
    pub typography: Typography,
    /// Theme-provided typography before the user's font-zoom factor is applied.
    /// `typography` = `typography_base.scaled(font_scale)`.
    pub typography_base: Typography,
    pub font_scale: f32,
    pub show_footer: bool,
    pub error: Option<String>,
    pub query: String,
    pub matches: Vec<MatchPos>,
    pub match_idx: usize,
    pub search_open: bool,
    pub workspace: Option<PathBuf>,
    pub workspace_files: Vec<PathBuf>,
    /// Bounded lightweight path index used only to reconstruct ordinary Files
    /// sidebar rows across the full retained tree depth. Cmd+P and vault search
    /// continue to use `workspace_files` and its historical shallower depth.
    pub workspace_sidebar_files: tree::SidebarFileIndex,
    pub workspace_tree: Option<Node>,
    /// Filter used to produce the stored workspace snapshot. Full Mindmap may
    /// change `show_hidden` while the Files sidebar remains obscured.
    pub workspace_snapshot_show_hidden: bool,
    /// True when the bounded workspace index stopped at its entry/file budget.
    pub workspace_truncated: bool,
    /// Whether dot-prefixed dirs/files appear in the tree, picker, and
    /// workspace_files walk. Toggled by `Message::ToggleHidden` (⌘⇧.).
    /// `.git`/node_modules/target are always filtered regardless.
    pub show_hidden: bool,
    pub expanded: HashSet<PathBuf>,
    pub sidebar_open: bool,
    pub sidebar_tab: SidebarTab,
    pub tree_cursor: usize,
    pub outline_cursor: usize,
    /// Heading outline, rebuilt in `load_ast_from_source` when the document
    /// changes. Cached so the Outline sidebar (rendered every frame) and arrow
    /// nav don't re-parse the whole source per event.
    pub outline_sections: Vec<crate::ipc::sections::Section>,
    pub overlay: Overlay,
    pub overlay_query: String,
    pub overlay_selected: usize,
    /// Vault search results page (Zed-style) — full reader-area, not an overlay.
    /// Shown when `vault_open`; workspace-level, so it renders even with no file.
    pub vault_open: bool,
    pub vault_query: String,
    /// The query the currently-displayed results were searched for. `None` until
    /// the first search. Enter searches when this differs from `vault_query`
    /// (query edited), otherwise opens the selected hit.
    pub vault_searched_query: Option<String>,
    pub vault_results: Vec<crate::vault_search::VaultHit>,
    /// Distinct files in `vault_results`, computed when results change so the
    /// vault page doesn't re-scan the hit list every frame.
    pub vault_file_count: usize,
    pub vault_truncated: bool,
    /// Monotonic request counter; a `VaultSearchDone` whose seq != this is stale.
    pub vault_seq: u64,
    /// Cursor over the *visible* (non-collapsed) flattened match list.
    pub vault_cursor: usize,
    /// Files whose result group the user has folded.
    pub vault_collapsed: HashSet<PathBuf>,
    pub vault_viewport: Option<iced::widget::scrollable::Viewport>,
    pub picker: Option<Picker>,
    /// Opt-in workspace navigator. Kept separate from the document's
    /// `ViewMode::Mindmap` state so entering/exiting it cannot disturb an open
    /// document mindmap or Zen editor.
    pub full_mindmap: Option<FullMindmapState>,
    /// Monotonic across Full Mindmap sessions so a result from a mode that was
    /// exited cannot collide with a new same-path request after re-entry.
    pub full_mindmap_request_seq: u64,
    pub tree_viewport: Option<iced::widget::scrollable::Viewport>,
    pub outline_viewport: Option<iced::widget::scrollable::Viewport>,
    /// Latest window size, tracked via `window::resize_events` for ⌘⌥W
    /// fraction-of-window panel sizing. `None` until the first resize event.
    pub window_size: Option<iced::Size>,
    /// True when the window is in native fullscreen, where macOS hides the
    /// traffic-light buttons and the sidebar header needs no reserved gap.
    pub window_fullscreen: bool,
    pub overlay_viewport: Option<iced::widget::scrollable::Viewport>,
    pub body_viewport: Option<iced::widget::scrollable::Viewport>,
    pub last_body_range: std::cell::Cell<(usize, usize)>,
    #[allow(dead_code)]
    pub first_frame_at: Option<std::time::Instant>,
    pub last_scroll_at: Option<std::time::Instant>,
    pub sidebar_width: f32,
    pub sidebar_drag: Option<f32>,
    pub(crate) hl_cache: crate::highlight::HlCache,
    pub(crate) height_cache: crate::virt::HeightCache,
    pub toast: Option<Toast>,
    pub toast_seq: u64,
    refresh_seq: u64,
    /// Invalidates a file-refresh read when the visible document or an
    /// overlapping save advances while that read is in flight.
    file_refresh_generation: u64,
    pending_refresh: Option<RefreshTracker>,
    pending_refresh_file: Option<PendingRefreshFile>,
    pending_refresh_workspace: Option<PendingRefreshWorkspace>,
    pending_refresh_full_mindmap_workspace: Option<PendingFullMindmapWorkspaceLoad>,
    pending_clipboard_copy: Option<String>,
    /// Persistent neutral progress feedback for the active Full Mindmap
    /// verification wave. Kept separate from attention/error toasts so a
    /// blocked action never loses its own priority or expiry timing.
    pub full_mindmap_progress: Option<FullMindmapProgress>,
    pub custom_themes: Vec<crate::theme_load::CustomTheme>,
    pub theme_id: crate::theme::ThemeId,
    pub image_cache: ImageCache,
    pub zoom_url: Option<String>,
    pub view_mode: ViewMode,
    pub editor: Option<iced::widget::text_editor::Content>,
    pub zen_restore: Option<ZenRestoreState>,
    /// Last document text known to have been persisted successfully. `source`
    /// may contain an unsaved Zen edit after switching back to rendered mode.
    pub saved_source: String,
    pub dirty: bool,
    pub edit_history: crate::history::SnapshotStack,
    pub edit_redo: crate::history::SnapshotStack,
    pub is_data_doc: bool,
    pub folded: HashSet<crate::ast::BlockId>,
    pub hovered_heading: Option<crate::ast::BlockId>,
    pub fold_chord_pending: bool,
    pub mindmap_collapsed: HashSet<crate::ast::BlockId>,
    pub mindmap_panel_open: bool,
    pub mindmap_selected: Option<crate::ast::BlockId>,
    /// What the preview panel actually renders. Lags `mindmap_selected` by a
    /// short debounce during arrow-key navigation: rebuilding the rendered
    /// slice (shaping + highlighting up to MIND_PANEL_MAX_BLOCKS) on every
    /// key-repeat press churns multi-MB allocations per frame.
    pub mindmap_panel_shown: Option<crate::ast::BlockId>,
    /// Generation counter pairing debounce timers with the latest selection;
    /// a stale timer's `MindmapPanelSettle` is ignored.
    mindmap_panel_settle_gen: u64,
    pub mindmap_panel_width: f32,
    /// Current step in the ⌘⌥W width cycle (indexes `MIND_PANEL_FRACS`).
    pub mindmap_panel_step: usize,
    pub mindmap_panel_drag: Option<(f32, Option<f32>)>,
    pub mindmap_autocenter: bool,
    /// Cumulative macOS magnification delivered by the native pinch bridge.
    /// The canvas records its own baseline when mounted, so this is only an
    /// input stream and does not persist a document's zoom level.
    pub mindmap_native_pinch_log: f64,
    /// Full Mindmap has a distinct canvas state and therefore its own input
    /// stream. Keeping them separate prevents a mode switch from replaying a
    /// gesture into the other graph.
    pub full_mindmap_native_pinch_log: f64,
    /// Cached mindmap layout, lazily rebuilt from (ast, file, mindmap_collapsed).
    /// Every mutation of those inputs must invalidate it or atomically replace
    /// it with a layout built from the new state.
    /// RefCell so `view(&self)` can populate it on first read.
    mindmap_layout: std::cell::RefCell<
        Option<(
            std::sync::Arc<Vec<crate::mindmap::MNode>>,
            iced::Size,
            std::sync::Arc<
                std::collections::HashMap<crate::ast::BlockId, Vec<crate::data_mindmap::PathSeg>>,
            >,
        )>,
    >,
    /// Changes whenever the Document Mindmap graph is invalidated. The canvas
    /// uses this to re-center the focused node after collapse/expand relayouts.
    mindmap_layout_generation: std::cell::Cell<u64>,
    /// Pretty-printed subtree for the data-doc mindmap leaf panel, keyed by the
    /// shown node id. Recomputed when `mindmap_panel_shown` changes; cleared by
    /// `invalidate_mindmap_layout`.
    mindmap_data_panel: std::cell::RefCell<Option<(crate::ast::BlockId, String)>>,
    /// Last `(root, file) -> relative path` answer for the active Quick Slot.
    /// The scroll checkpoint and the slot rail ask for the same pair on every
    /// event, and each fresh answer costs two `canonicalize` calls. Cleared
    /// whenever a file (re)loads.
    quick_slot_relative_memo: std::cell::RefCell<Option<(PathBuf, PathBuf, Option<String>)>>,
    /// T3 — diagram render cache. T4 will populate it from a pre-walk +
    /// `iced::Task::perform` of `diagram::render_blocking`.
    pub diagram_cache: crate::diagram::DiagramCache,
    /// Stable digest of the current palette. Refreshed on every theme change
    /// so the diagram cache (keyed on `(hash, theme_id)`) is invalidated for
    /// the new palette automatically.
    pub diagram_theme_id: u32,
    /// Pre-rasterized image::Handle of the diagram currently shown in the
    /// zoom overlay. `None` when overlay shows a normal raster/svg image.
    /// Using image::Handle lets the zoom modal reuse iced's built-in
    /// `image::viewer` for scroll-to-zoom + drag-to-pan + escape-to-close
    /// parity with normal images. Handle clones are cheap (Arc inside).
    pub zoom_diagram: Option<iced::widget::image::Handle>,
    /// Line numbers (0-based) for each block in `ast`, parallel to `ast`.
    /// Built from `parser::parse`'s byte-offset return via `build_byte_to_line`.
    pub block_lines: Vec<u32>,
    /// Set by IPC `Open { line, section }` so the subsequent `FileLoaded`
    /// can finish navigation once the AST/block_lines exist.
    pub pending_nav: Option<PendingNav>,
    /// IPC file activation waits here while Full Mindmap exits and, when
    /// needed, reconciles a stale hidden-file workspace snapshot.
    pending_ipc_file_open: Option<PendingIpcFileOpen>,
    /// Snap-to relative offset queued for the next `update` tick. Used by
    /// `apply_goto` which can't perform scroll math during the IPC handler
    /// without re-entering `update`.
    pub queued_snap: Option<f32>,
    /// Precise-landing companion to `queued_snap`: after the estimate snap,
    /// run a widget operation that centers this block from its real laid-out
    /// bounds (the virt window around it was rebuilt by `apply_goto`).
    pub queued_goto: Option<crate::ast::BlockId>,
    /// In-flight screenshot: target PNG path + an optional deferred IPC reply
    /// sender. `Cmd::Screenshot` stashes `Some(tx)` so the client blocks until
    /// the file is written; the palette command stashes `None` (toast only).
    /// `Message::ScreenshotCaptured` writes the file and replies if a sender
    /// is present.
    pub pending_screenshot: Option<(
        std::path::PathBuf,
        Option<
            std::sync::Arc<
                std::sync::Mutex<Option<futures::channel::oneshot::Sender<crate::ipc::Response>>>,
            >,
        >,
    )>,
    /// Windowed-rendering state for the body (display list, prefix sums,
    /// rendered range + hysteresis band). Rebuilt only in `update` — on doc
    /// load/reparse, fold changes, font changes, measured-height feedback,
    /// goto jumps, and scroll-band exits — and read by `view`/`render`.
    pub(crate) virt_window: crate::virt::VirtWindow,
    /// AST index of an in-flight navigation target. While set, a band-exit
    /// rebuild recenters the window on the target instead of the raw offset,
    /// so the estimate snap can't evict the block the precise scroll op needs.
    /// Cleared on the first scroll event after the jump.
    pub(crate) nav_anchor: Option<usize>,
    /// User preferences (persisted to `~/.config/rmdv/prefs.json`).
    pub prefs: crate::prefs::Prefs,
    /// The currently selected workspace bank. It is normalized to nine slots
    /// at every workspace boundary and persisted back into `prefs.json`.
    pub quick_slots: crate::quick_slots::WorkspaceSlots,
    quick_slots_root: Option<PathBuf>,
    quick_slots_undo: Option<crate::quick_slots::ClearUndo>,
    pub quick_slots_modifier_held: bool,
    quick_slots_rail_revealed: bool,
    quick_slots_modifier_generation: u64,
    quick_slots_persist_generation: u64,
    quick_slots_persist_pending: bool,
    quick_slot_activation_generation: u64,
    pending_quick_slot_restore: Option<PendingQuickSlotRestore>,
    quick_slot_preview_restore_guard: Option<PendingQuickSlotRestore>,
    quick_slot_body_restore: Option<f32>,
    quick_slot_preview_restore: Option<f32>,
    quick_slots_persistence_path: Option<PathBuf>,
    watcher_generation: u64,
    pending_watcher_reload: Option<PendingWatcherReload>,
    /// A downloaded + verified update awaiting user-initiated install. Drives
    /// the update banner. `None` until the background check finds a newer build.
    pub pending_update: Option<crate::update::ReadyUpdate>,
}

#[cfg(test)]
fn test_quick_slots_persistence_path() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    std::env::temp_dir().join(format!(
        "rmdv-app-quick-slots-test-{}-{}.json",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ))
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

impl Default for App {
    fn default() -> Self {
        let mode = ThemeMode::System;
        let preset = theme::resolve_mode(mode);
        // Migrate legacy `mdv` config into `rmdv` before the first read.
        #[cfg(not(test))]
        crate::config_migrate::run();
        #[cfg(not(test))]
        let prefs = crate::prefs::load();
        #[cfg(test)]
        let prefs = crate::prefs::Prefs::default();
        Self {
            file: None,
            source: String::new(),
            source_words: 0,
            ast: Vec::new(),
            theme_mode: mode,
            theme_preset: preset,
            palette: theme::palette_for(preset),
            typography: Typography::DEFAULT,
            typography_base: Typography::DEFAULT,
            font_scale: 1.0,
            show_footer: prefs.show_footer,
            error: None,
            query: String::new(),
            matches: Vec::new(),
            match_idx: 0,
            search_open: false,
            workspace: None,
            workspace_files: Vec::new(),
            workspace_sidebar_files: tree::SidebarFileIndex::default(),
            workspace_tree: None,
            workspace_snapshot_show_hidden: false,
            workspace_truncated: false,
            show_hidden: false,
            expanded: HashSet::new(),
            sidebar_open: false,
            sidebar_tab: SidebarTab::Files,
            tree_cursor: 0,
            outline_cursor: 0,
            outline_sections: Vec::new(),
            overlay: Overlay::None,
            overlay_query: String::new(),
            overlay_selected: 0,
            vault_open: false,
            vault_query: String::new(),
            vault_searched_query: None,
            vault_results: Vec::new(),
            vault_file_count: 0,
            vault_truncated: false,
            vault_seq: 0,
            vault_cursor: 0,
            vault_collapsed: HashSet::new(),
            vault_viewport: None,
            picker: None,
            full_mindmap: None,
            full_mindmap_request_seq: 0,
            tree_viewport: None,
            outline_viewport: None,
            window_size: None,
            window_fullscreen: false,
            overlay_viewport: None,
            body_viewport: None,
            last_body_range: std::cell::Cell::new((0, 0)),
            first_frame_at: None,
            last_scroll_at: None,
            sidebar_width: SIDEBAR_WIDTH,
            sidebar_drag: None,
            hl_cache: crate::highlight::HlCache::default(),
            height_cache: crate::virt::HeightCache::default(),
            toast: None,
            toast_seq: 0,
            refresh_seq: 0,
            file_refresh_generation: 0,
            pending_refresh: None,
            pending_refresh_file: None,
            pending_refresh_workspace: None,
            pending_refresh_full_mindmap_workspace: None,
            pending_clipboard_copy: None,
            full_mindmap_progress: None,
            custom_themes: Vec::new(),
            theme_id: crate::theme::ThemeId::Preset(preset),
            image_cache: ImageCache::default(),
            zoom_url: None,
            view_mode: ViewMode::Rendered,
            editor: None,
            zen_restore: None,
            saved_source: String::new(),
            dirty: false,
            edit_history: crate::history::SnapshotStack::default(),
            edit_redo: crate::history::SnapshotStack::default(),
            is_data_doc: false,
            folded: HashSet::new(),
            hovered_heading: None,
            fold_chord_pending: false,
            mindmap_collapsed: HashSet::new(),
            mindmap_panel_open: false,
            mindmap_selected: None,
            mindmap_panel_shown: None,
            mindmap_panel_settle_gen: 0,
            mindmap_panel_width: MIND_PANEL_DEFAULT,
            mindmap_panel_step: 0,
            mindmap_panel_drag: None,
            mindmap_autocenter: true,
            mindmap_native_pinch_log: 0.0,
            full_mindmap_native_pinch_log: 0.0,
            mindmap_layout: std::cell::RefCell::new(None),
            mindmap_layout_generation: std::cell::Cell::new(0),
            mindmap_data_panel: std::cell::RefCell::new(None),
            quick_slot_relative_memo: std::cell::RefCell::new(None),
            diagram_cache: crate::diagram::DiagramCache::new(64),
            diagram_theme_id: 0,
            zoom_diagram: None,
            block_lines: Vec::new(),
            pending_nav: None,
            pending_ipc_file_open: None,
            queued_snap: None,
            queued_goto: None,
            pending_screenshot: None,
            virt_window: crate::virt::VirtWindow::default(),
            nav_anchor: None,
            prefs,
            quick_slots: crate::quick_slots::WorkspaceSlots::default(),
            quick_slots_root: None,
            quick_slots_undo: None,
            quick_slots_modifier_held: false,
            quick_slots_rail_revealed: false,
            quick_slots_modifier_generation: 0,
            quick_slots_persist_generation: 0,
            quick_slots_persist_pending: false,
            quick_slot_activation_generation: 0,
            pending_quick_slot_restore: None,
            quick_slot_preview_restore_guard: None,
            quick_slot_body_restore: None,
            quick_slot_preview_restore: None,
            quick_slots_persistence_path: {
                #[cfg(test)]
                {
                    Some(test_quick_slots_persistence_path())
                }
                #[cfg(not(test))]
                {
                    None
                }
            },
            watcher_generation: 0,
            pending_watcher_reload: None,
            pending_update: None,
        }
    }
}

impl App {
    /// Record a new theme-provided typography base and re-apply the current
    /// font-zoom factor on top of it.
    fn set_typography_base(&mut self, base: Typography) {
        self.typography_base = base;
        self.typography = base.scaled(self.font_scale);
    }

    /// Adjust the font-zoom factor (clamped) and rebuild `typography` from the
    /// current theme base. Returns the resulting body size for the toast.
    fn adjust_font_scale(&mut self, factor: f32) -> f32 {
        self.font_scale = (self.font_scale * factor).clamp(0.6, 2.2);
        self.typography = self.typography_base.scaled(self.font_scale);
        self.typography.body_size
    }

    fn quick_slots_workspace_root(&self) -> Option<&std::path::Path> {
        self.workspace
            .as_deref()
            .or(self.quick_slots_root.as_deref())
    }

    fn load_quick_slots_for_workspace(&mut self, root: &std::path::Path) {
        let same = self.quick_slots_root.as_ref().is_some_and(|current| {
            crate::quick_slots::workspace_key(current) == crate::quick_slots::workspace_key(root)
        });
        if !same {
            self.invalidate_pending_quick_slot_restore();
            self.persist_quick_slots_now();
            self.quick_slots = self.prefs.quick_slots.bank(root);
            self.quick_slots_root = Some(root.to_path_buf());
            self.quick_slots_undo = None;
            self.quick_slots_persist_pending = false;
        }
    }

    fn persist_quick_slots_now(&mut self) {
        let Some(root) = self.quick_slots_root.clone() else {
            return;
        };
        self.quick_slots_persist_pending = false;
        self.prefs
            .quick_slots
            .put_bank(&root, self.quick_slots.clone());
        if let Some(path) = self.quick_slots_persistence_path.as_deref() {
            crate::prefs::save_to(path, &self.prefs);
        } else {
            crate::prefs::save(&self.prefs);
        }
    }

    fn invalidate_pending_watcher_reload(&mut self) {
        self.watcher_generation = self.watcher_generation.wrapping_add(1);
        self.pending_watcher_reload = None;
    }

    fn begin_watcher_reload(&mut self, path: PathBuf) -> Option<PendingWatcherReload> {
        if self.file.as_ref() != Some(&path)
            || self.pending_quick_slot_restore.is_some()
            || self.quick_slot_preview_restore_guard.is_some()
        {
            return None;
        }
        self.watcher_generation = self.watcher_generation.wrapping_add(1);
        let request = PendingWatcherReload {
            generation: self.watcher_generation,
            path,
        };
        self.pending_watcher_reload = Some(request.clone());
        Some(request)
    }

    fn watcher_reload_is_current(&self, request: &PendingWatcherReload) -> bool {
        self.watcher_generation == request.generation
            && self.pending_watcher_reload.as_ref() == Some(request)
            && self.file.as_ref() == Some(&request.path)
            && self.pending_quick_slot_restore.is_none()
            && self.quick_slot_preview_restore_guard.is_none()
    }

    fn invalidate_pending_quick_slot_restore(&mut self) {
        self.quick_slot_activation_generation =
            self.quick_slot_activation_generation.wrapping_add(1);
        self.invalidate_pending_watcher_reload();
        self.finish_pending_quick_slot_restore();
        self.quick_slot_body_restore = None;
        self.quick_slot_preview_restore = None;
    }

    fn finish_pending_quick_slot_restore(&mut self) {
        self.pending_quick_slot_restore = None;
        self.quick_slot_preview_restore_guard = None;
    }

    fn begin_pending_quick_slot_restore(
        &mut self,
        index: usize,
        slot: crate::quick_slots::QuickSlot,
    ) -> Option<PendingQuickSlotRestore> {
        let root_key = crate::quick_slots::workspace_key(self.quick_slots_workspace_root()?);
        self.quick_slot_activation_generation =
            self.quick_slot_activation_generation.wrapping_add(1);
        self.invalidate_pending_watcher_reload();
        self.quick_slot_preview_restore_guard = None;
        self.quick_slot_body_restore = None;
        self.quick_slot_preview_restore = None;
        let pending = PendingQuickSlotRestore {
            generation: self.quick_slot_activation_generation,
            index,
            slot,
            root_key,
        };
        self.pending_quick_slot_restore = Some(pending.clone());
        Some(pending)
    }

    fn quick_slot_restore_is_current(&self, pending: &PendingQuickSlotRestore) -> bool {
        self.quick_slot_activation_generation == pending.generation
            && self.quick_slots.active == Some(pending.index)
            && self
                .quick_slots_workspace_root()
                .is_some_and(|root| crate::quick_slots::workspace_key(root) == pending.root_key)
            && self.quick_slots.occupied(pending.index) == Some(&pending.slot)
    }

    fn take_current_quick_slot_preview_restore(&mut self) -> Option<f32> {
        let Some(guard) = self.quick_slot_preview_restore_guard.clone() else {
            self.quick_slot_preview_restore = None;
            return None;
        };
        if !self.quick_slot_restore_is_current(&guard) {
            self.invalidate_pending_quick_slot_restore();
            return None;
        }
        self.quick_slot_preview_restore_guard = None;
        self.quick_slot_preview_restore.take()
    }

    /// Debounce context checkpoints so rapid scrolling owns at most one
    /// sleeping task and one follow-up write. The in-memory slot always holds
    /// the latest accepted position even while persistence is pending.
    fn schedule_quick_slots_persist(&mut self) -> Task<Message> {
        self.quick_slots_persist_generation = self.quick_slots_persist_generation.wrapping_add(1);
        if self.quick_slots_persist_pending {
            return Task::none();
        }
        self.quick_slots_persist_pending = true;
        let generation = self.quick_slots_persist_generation;
        Task::perform(
            async move {
                tokio::time::sleep(std::time::Duration::from_millis(220)).await;
                generation
            },
            Message::QuickSlotsPersist,
        )
    }

    fn slot_position(viewport: Option<&iced::widget::scrollable::Viewport>) -> f32 {
        let Some(viewport) = viewport else {
            return 0.0;
        };
        let max = (viewport.content_bounds().height - viewport.bounds().height).max(0.0);
        if max <= 0.0 {
            return 0.0;
        }
        crate::quick_slots::normalize_position(viewport.absolute_offset().y / max)
    }

    fn full_mindmap_selected_file(&self) -> Option<PathBuf> {
        self.full_mindmap.as_ref().and_then(|full| {
            full.selected.as_ref().and_then(|selected| match selected {
                WorkspaceNodeId::File(path) => Some(path.clone()),
                _ => None,
            })
        })
    }

    /// Capture only a reading surface. Raw/Zen intentionally maps to the last
    /// persisted reader surface (Rendered), never to unsaved editor text.
    fn current_quick_slot_context(&self) -> Option<(PathBuf, crate::quick_slots::SlotContext)> {
        let root = self.quick_slots_workspace_root()?;
        if self.full_mindmap.is_some() {
            let path = self.full_mindmap_selected_file()?;
            self.quick_slot_relative_path(root, &path)?;
            return Some((
                path,
                crate::quick_slots::SlotContext {
                    mode: crate::quick_slots::SlotMode::FullMindmap,
                    preview_position: Self::slot_position(
                        self.full_mindmap
                            .as_ref()
                            .and_then(|full| full.preview_viewport.as_ref()),
                    ),
                    ..Default::default()
                },
            ));
        }
        let path = self.file.clone()?;
        self.quick_slot_relative_path(root, &path)?;
        let mode = if self.view_mode == ViewMode::Mindmap {
            crate::quick_slots::SlotMode::DocumentMindmap
        } else {
            crate::quick_slots::SlotMode::Rendered
        };
        Some((
            path,
            crate::quick_slots::SlotContext {
                mode,
                body_position: Self::slot_position(self.body_viewport.as_ref()),
                mindmap_selection: self.mindmap_selected.map(|id| id.0),
                mindmap_panel_open: self.mindmap_panel_open,
                ..Default::default()
            },
        ))
    }

    fn quick_slot_relative_path(&self, root: &Path, path: &Path) -> Option<String> {
        let mut memo = self.quick_slot_relative_memo.borrow_mut();
        if let Some((memo_root, memo_path, relative)) = memo.as_ref() {
            if memo_root == root && memo_path == path {
                return relative.clone();
            }
        }
        let relative = crate::quick_slots::relative_path(root, path);
        *memo = Some((root.to_path_buf(), path.to_path_buf(), relative.clone()));
        relative
    }

    fn quick_slot_activation_is_current(
        &self,
        index: usize,
        slot: &crate::quick_slots::QuickSlot,
        path: &std::path::Path,
    ) -> bool {
        if self.quick_slots.active != Some(index) || self.editor.is_some() {
            return false;
        }
        let path = canonicalize_existing_path(path.to_path_buf());
        let same_path = |current: Option<PathBuf>| {
            current.is_some_and(|current| canonicalize_existing_path(current) == path)
        };
        match slot.context.mode {
            crate::quick_slots::SlotMode::FullMindmap => {
                self.full_mindmap.is_some() && same_path(self.full_mindmap_selected_file())
            }
            crate::quick_slots::SlotMode::DocumentMindmap => {
                self.full_mindmap.is_none()
                    && self.view_mode == ViewMode::Mindmap
                    && same_path(self.file.clone())
            }
            crate::quick_slots::SlotMode::Rendered => {
                self.full_mindmap.is_none()
                    && self.view_mode == ViewMode::Rendered
                    && same_path(self.file.clone())
            }
        }
    }

    fn checkpoint_active_quick_slot(&mut self) -> Task<Message> {
        let Some(index) = self.quick_slots.active else {
            return Task::none();
        };
        // Entering Full Mindmap from a document preserves the active slot's
        // Rendered/DocumentMindmap context. Only a slot that was originally
        // created for Full Mindmap may be checkpointed from the navigator;
        // otherwise selecting a parent folder would silently rewrite a file
        // slot so its next activation stayed in Full Mindmap.
        if self.full_mindmap.is_some()
            && self
                .quick_slots
                .occupied(index)
                .is_some_and(|slot| slot.context.mode != crate::quick_slots::SlotMode::FullMindmap)
        {
            return Task::none();
        }
        let Some((path, context)) = self.current_quick_slot_context() else {
            return Task::none();
        };
        let Some(root) = self.quick_slots_workspace_root() else {
            return Task::none();
        };
        let Some(relative) = self.quick_slot_relative_path(root, &path) else {
            return Task::none();
        };
        let owns = self
            .quick_slots
            .occupied(index)
            .is_some_and(|slot| slot.relative_path == relative);
        if !owns {
            return Task::none();
        }
        if let Some(slot) = self.quick_slots.occupied_mut(index) {
            slot.context = context.normalized();
        }
        self.schedule_quick_slots_persist()
    }

    /// Manual navigation that leaves the active slot's file has no current
    /// target to keep active. Invalidate older debounced writes and persist the
    /// cleared marker immediately so a late completion cannot resurrect it.
    fn clear_active_quick_slot(&mut self) {
        if self.quick_slots.active.is_none() {
            return;
        }
        self.quick_slots.active = None;
        self.quick_slots_persist_generation = self.quick_slots_persist_generation.wrapping_add(1);
        self.persist_quick_slots_now();
    }

    fn quick_slot_path(&self, index: usize) -> Option<(crate::quick_slots::QuickSlot, PathBuf)> {
        let slot = self.quick_slots.occupied(index)?.clone();
        let root = self.quick_slots_workspace_root()?;
        let path = crate::quick_slots::resolve_path(root, &slot.relative_path)?;
        Some((slot, path))
    }

    fn quick_slot_restore_position(&self, position: f32) -> Task<Message> {
        let Some(viewport) = self.body_viewport.as_ref() else {
            return Task::none();
        };
        let max = (viewport.content_bounds().height - viewport.bounds().height).max(0.0);
        if max <= 0.0 {
            return Task::none();
        }
        iced::widget::operation::scroll_to(
            Self::scroll_id(),
            iced::widget::scrollable::AbsoluteOffset {
                x: 0.0,
                y: (crate::quick_slots::normalize_position(position) * max).max(0.0),
            },
        )
    }

    fn quick_slot_restore_preview_position(&self, position: f32) -> Task<Message> {
        let Some(viewport) = self
            .full_mindmap
            .as_ref()
            .and_then(|full| full.preview_viewport.as_ref())
        else {
            return Task::none();
        };
        let max = (viewport.content_bounds().height - viewport.bounds().height).max(0.0);
        iced::widget::operation::scroll_to(
            Self::full_mindmap_preview_scroll_id(),
            iced::widget::scrollable::AbsoluteOffset {
                x: 0.0,
                y: crate::quick_slots::normalize_position(position) * max,
            },
        )
    }

    fn apply_quick_slot_restore_after_file(
        &mut self,
        path: &std::path::Path,
        slot: crate::quick_slots::QuickSlot,
    ) -> Task<Message> {
        self.view_mode = match slot.context.mode {
            crate::quick_slots::SlotMode::DocumentMindmap => ViewMode::Mindmap,
            crate::quick_slots::SlotMode::Rendered | crate::quick_slots::SlotMode::FullMindmap => {
                ViewMode::Rendered
            }
        };
        self.editor = None;
        self.zen_restore = None;
        self.mindmap_selected = slot
            .context
            .mindmap_selection
            .map(crate::ast::BlockId)
            .filter(|id| self.ast.iter().any(|(block_id, _)| block_id == id));
        self.mindmap_panel_shown = self.mindmap_selected;
        self.mindmap_panel_open = slot.context.mindmap_panel_open;
        self.finish_pending_quick_slot_restore();
        self.quick_slot_body_restore = Some(slot.context.body_position);
        if path == self.file.as_deref().unwrap_or(path) {
            let max = self
                .body_viewport
                .as_ref()
                .map(|viewport| {
                    (viewport.content_bounds().height - viewport.bounds().height).max(0.0)
                })
                .unwrap_or(0.0);
            if max > 0.0 {
                self.quick_slot_body_restore = None;
                return self.quick_slot_restore_position(slot.context.body_position);
            }
        }
        Task::none()
    }

    fn begin_quick_slot_activation(&mut self, index: usize) -> Task<Message> {
        let Some(slot) = self.quick_slots.occupied(index).cloned() else {
            return self.show_toast(format!("Quick Slot {} is empty", index + 1));
        };
        let Some(root) = self.quick_slots_workspace_root() else {
            return self.show_toast(format!("Quick Slot {} is missing", index + 1));
        };
        let Some(path) = crate::quick_slots::resolve_path(root, &slot.relative_path) else {
            if self.dirty {
                return self.show_toast(self.unsaved_edits_open_message());
            }
            self.invalidate_pending_quick_slot_restore();
            self.quick_slots.active = Some(index);
            self.persist_quick_slots_now();
            return self.show_toast(format!(
                "Quick Slot {} is missing: {}",
                index + 1,
                slot.relative_path
            ));
        };
        // A clean Zen editor still needs the active slot chord/button to leave
        // the editing surface and restore its persisted reading mode. The
        // active marker can also survive manual navigation, so suppress a
        // duplicate only when the current file and reading surface match the
        // saved target (using canonical paths for aliases).
        if path.is_file() && self.quick_slot_activation_is_current(index, &slot, &path) {
            return Task::none();
        }
        if self.dirty {
            return self.show_toast(self.unsaved_edits_open_message());
        }
        if !path.is_file() {
            self.invalidate_pending_quick_slot_restore();
            self.quick_slots.active = Some(index);
            self.persist_quick_slots_now();
            return self.show_toast(format!(
                "Quick Slot {} is missing: {}",
                index + 1,
                slot.relative_path
            ));
        }
        // Do not checkpoint the current UI back into the target when the
        // active slot is being reactivated. In particular, clean Zen maps to
        // `Rendered`, which would overwrite a same-slot Mindmap context
        // before its saved target is restored. Switching to another slot
        // still checkpoints the outgoing slot.
        let checkpoint = if self.quick_slots.active == Some(index) {
            Task::none()
        } else {
            self.checkpoint_active_quick_slot()
        };
        self.quick_slots.active = Some(index);
        // Persist the active marker immediately; ordinary scroll checkpoints
        // remain debounced, but a restart should reopen the selected bank
        // state even if no viewport event arrives before exit.
        self.persist_quick_slots_now();
        match slot.context.mode {
            crate::quick_slots::SlotMode::FullMindmap => {
                if self.view_mode == ViewMode::Raw || self.editor.is_some() {
                    self.leave_zen_edit_mode(false);
                }
                self.begin_pending_quick_slot_restore(index, slot);
                if self.full_mindmap.is_none() {
                    let enter = self.enter_full_mindmap_at(None);
                    Task::batch([
                        checkpoint,
                        enter,
                        Task::done(Message::QuickSlotRestorePending),
                    ])
                } else {
                    Task::batch([checkpoint, Task::done(Message::QuickSlotRestorePending)])
                }
            }
            crate::quick_slots::SlotMode::Rendered
            | crate::quick_slots::SlotMode::DocumentMindmap => {
                let Some(pending) = self.begin_pending_quick_slot_restore(index, slot) else {
                    return Task::none();
                };
                let load = {
                    let slot = pending.slot.clone();
                    Task::perform(load_file(path), move |result| {
                        Message::QuickSlotFileLoaded {
                            index: pending.index,
                            slot,
                            result,
                        }
                    })
                };
                if self.full_mindmap.is_some() {
                    let exit = self.exit_full_mindmap(false);
                    if self.full_mindmap.is_none() {
                        self.supersede_file_loads();
                        Task::batch([checkpoint, exit, load])
                    } else {
                        Task::batch([checkpoint, exit])
                    }
                } else {
                    self.supersede_file_loads();
                    Task::batch([checkpoint, load])
                }
            }
        }
    }

    fn assign_quick_slot(&mut self, index: usize) -> Task<Message> {
        if index >= crate::quick_slots::SLOT_COUNT {
            return Task::none();
        }
        if self.full_mindmap.is_none() && self.workspace.is_none() {
            return self.show_toast("Open a workspace before assigning Quick Slots".into());
        }
        if self.dirty {
            return self.show_toast(self.unsaved_edits_open_message());
        }
        let Some((path, context)) = self.current_quick_slot_context() else {
            return self.show_toast("Open a file before assigning a Quick Slot".into());
        };
        let Some(root) = self.quick_slots_workspace_root() else {
            return self.show_toast("Open a workspace before assigning Quick Slots".into());
        };
        let Some(relative_path) = crate::quick_slots::relative_path(root, &path) else {
            return self
                .show_toast("Quick Slots only store files inside the active workspace".into());
        };
        let slot = crate::quick_slots::QuickSlot {
            relative_path,
            context: context.normalized(),
        };
        self.invalidate_pending_quick_slot_restore();
        self.quick_slots.set(index, slot);
        self.quick_slots.active = Some(index);
        self.quick_slots_undo = None;
        self.persist_quick_slots_now();
        self.show_toast(format!("Quick Slot {} assigned", index + 1))
    }

    fn new_quick_slot(&mut self) -> Task<Message> {
        if self.full_mindmap.is_none() && self.workspace.is_none() {
            return self.show_toast("Open a workspace before assigning Quick Slots".into());
        }
        if self.dirty {
            return self.show_toast(self.unsaved_edits_open_message());
        }
        let Some((path, _context)) = self.current_quick_slot_context() else {
            return self.show_toast("Open a file before assigning a Quick Slot".into());
        };
        let Some(root) = self.quick_slots_workspace_root() else {
            return self.show_toast("Open a workspace before assigning Quick Slots".into());
        };
        let Some(relative_path) = crate::quick_slots::relative_path(root, &path) else {
            return self
                .show_toast("Quick Slots only store files inside the active workspace".into());
        };
        let matching = (0..crate::quick_slots::SLOT_COUNT).find(|&index| {
            self.quick_slots
                .occupied(index)
                .and_then(|slot| crate::quick_slots::normalize_relative_path(&slot.relative_path))
                .is_some_and(|slot_path| slot_path == relative_path)
        });
        if let Some(index) = matching {
            return self.begin_quick_slot_activation(index);
        }
        let Some(index) = (0..crate::quick_slots::SLOT_COUNT)
            .find(|&index| self.quick_slots.occupied(index).is_none())
        else {
            return self.show_toast("Quick Slots are full".into());
        };
        self.assign_quick_slot(index)
    }

    fn clear_quick_slot(&mut self, index: usize) -> Task<Message> {
        if self.dirty {
            return self.show_toast(self.unsaved_edits_open_message());
        }
        let Some(slot) = self.quick_slots.clear(index) else {
            return self.show_toast(format!("Quick Slot {} is empty", index + 1));
        };
        self.invalidate_pending_quick_slot_restore();
        self.quick_slots_undo = Some(crate::quick_slots::ClearUndo {
            index,
            slot,
            additional: Vec::new(),
        });
        self.persist_quick_slots_now();
        self.show_toast_with_action(
            format!("Quick Slot {} cleared", index + 1),
            Some(ToastAction {
                label: "Undo".into(),
                message: Message::QuickSlotUndo,
            }),
        )
    }

    fn clear_all_quick_slots(&mut self) -> Task<Message> {
        if self.dirty {
            return self.show_toast(self.unsaved_edits_open_message());
        }
        let removed = self.quick_slots.clear_all();
        if removed.is_empty() {
            return self.show_toast("Quick Slots are already empty".into());
        }
        self.invalidate_pending_quick_slot_restore();
        // Clear All is one bounded undo action. Keep the first slot in named
        // fields and retain the remaining eight entries in the same payload.
        let mut removed = removed.into_iter();
        let (index, slot) = removed.next().expect("non-empty clear-all");
        self.quick_slots_undo = Some(crate::quick_slots::ClearUndo {
            index,
            slot,
            additional: removed.collect(),
        });
        self.persist_quick_slots_now();
        self.show_toast_with_action(
            "All Quick Slots cleared".into(),
            Some(ToastAction {
                label: "Undo".into(),
                message: Message::QuickSlotUndo,
            }),
        )
    }

    fn undo_quick_slot_clear(&mut self) -> Task<Message> {
        let Some(undo) = self.quick_slots_undo.take() else {
            return self.show_toast("Nothing to undo".into());
        };
        self.invalidate_pending_quick_slot_restore();
        self.quick_slots.set(undo.index, undo.slot);
        for (index, slot) in undo.additional {
            self.quick_slots.set(index, slot);
        }
        self.quick_slots.active = Some(undo.index);
        self.persist_quick_slots_now();
        self.show_toast(format!("Quick Slot {} restored", undo.index + 1))
    }

    fn close_active_quick_slot(&mut self) -> Task<Message> {
        let Some(active) = self.quick_slots.active else {
            return self.show_toast("No active Quick Slot".into());
        };
        if self.dirty {
            return self.show_toast(self.unsaved_edits_open_message());
        }
        let checkpoint = self.checkpoint_active_quick_slot();
        self.invalidate_pending_quick_slot_restore();
        let _removed = self.quick_slots.clear(active);
        self.quick_slots_undo = None;
        self.persist_quick_slots_now();
        let next = ((active + 1)..crate::quick_slots::SLOT_COUNT)
            .chain((0..active).rev())
            .find(|&index| {
                self.quick_slot_path(index)
                    .is_some_and(|(_, path)| path.is_file())
            });
        let activate = next.map_or_else(Task::none, |index| {
            Task::done(Message::QuickSlotActivate(index))
        });
        Task::batch([
            checkpoint,
            activate,
            self.show_toast("Quick Slot closed".into()),
        ])
    }

    fn close_quick_slot_window(&mut self) -> Task<Message> {
        if self.dirty {
            return self.show_toast(self.unsaved_edits_open_message());
        }
        let checkpoint = self.checkpoint_active_quick_slot();
        // The checkpoint mutates the in-memory slot synchronously. Persist it
        // before returning the close task so the debounce cannot lose the
        // latest reading position when the process exits immediately.
        self.persist_quick_slots_now();
        self.invalidate_pending_quick_slot_restore();
        Task::batch([
            checkpoint,
            iced::window::latest().and_then(iced::window::close),
        ])
    }

    fn apply_pending_quick_slot_restore(&mut self) -> Task<Message> {
        let Some(pending) = self.pending_quick_slot_restore.clone() else {
            return Task::none();
        };
        if !self.quick_slot_restore_is_current(&pending) {
            self.invalidate_pending_quick_slot_restore();
            return Task::none();
        }
        let index = pending.index;
        let slot = pending.slot.clone();
        let Some(root) = self.quick_slots_workspace_root() else {
            self.invalidate_pending_quick_slot_restore();
            return Task::none();
        };
        let Some(path) = crate::quick_slots::resolve_path(root, &slot.relative_path) else {
            self.invalidate_pending_quick_slot_restore();
            return Task::none();
        };
        if !path.is_file() {
            self.invalidate_pending_quick_slot_restore();
            return self.show_toast(format!("Quick Slot {} is missing", index + 1));
        }
        // A Full Mindmap root may still be materializing. Keep the identity-
        // bearing restore pending until that accepted snapshot is installed;
        // applying it against the provisional state would be overwritten by
        // `reset_full_mindmap_workspace` during snapshot replacement.
        if self
            .full_mindmap
            .as_ref()
            .is_some_and(|full| full.pending_workspace_load.is_some())
        {
            return Task::none();
        }
        if slot.context.mode == crate::quick_slots::SlotMode::FullMindmap {
            let Some(full) = self.full_mindmap.as_mut() else {
                return Task::none();
            };
            full.deferred_file_selection = Some(path.clone());
            full.selected = Some(WorkspaceNodeId::File(path.clone()));
            full.focus_request = Some(WorkspaceNodeId::File(path.clone()));
            self.quick_slot_preview_restore = Some(slot.context.preview_position);
            self.quick_slot_preview_restore_guard = Some(pending.clone());
            self.pending_quick_slot_restore = None;
            let already_ready = full.pending_preview.is_none()
                && matches!(
                    &full.preview,
                    FullMindmapPreview::Document { path: current, .. }
                        | FullMindmapPreview::Data { path: current, .. }
                        if current == &path
                );
            let preview = self.schedule_full_mindmap_preview(Some(path));
            let restore = if already_ready
                && self
                    .full_mindmap
                    .as_ref()
                    .is_some_and(|full| full.preview_viewport.is_some())
            {
                self.take_current_quick_slot_preview_restore()
                    .map_or_else(Task::none, |position| {
                        self.quick_slot_restore_preview_position(position)
                    })
            } else {
                Task::none()
            };
            return Task::batch([
                preview,
                restore,
                self.show_toast(format!("Quick Slot {}", index + 1)),
            ]);
        }
        if self.file.as_ref() == Some(&path) {
            return self.apply_quick_slot_restore_after_file(&path, slot);
        }
        let load = Task::perform(load_file(path), move |result| {
            Message::QuickSlotFileLoaded {
                index,
                slot,
                result,
            }
        });
        load
    }

    /// Return the current source in an Arc so read-only Full Mindmap parsing
    /// does not clone a `String` on the update thread. The worker performs the
    /// one owned conversion required by the parser off-thread.
    fn source_snapshot_for_preview(&self) -> Arc<str> {
        Arc::from(self.source.as_str())
    }

    fn sync_editor_to_source(&mut self) -> bool {
        let Some(text) = self.editor.as_ref().map(|editor| editor.text()) else {
            return false;
        };
        if text == self.source {
            self.dirty = self.source != self.saved_source;
            return false;
        }
        self.bump_file_refresh_generation();
        self.source = text;
        self.reparse_source();
        self.dirty = self.source != self.saved_source;
        true
    }

    fn enter_zen_edit_mode(&mut self) -> Task<Message> {
        if self.file.is_none() {
            return Task::none();
        }
        if is_pdf_path(self.file.as_deref()) {
            return self.show_toast("PDFs are view-only".into());
        }
        if self.zen_restore.is_none() {
            self.zen_restore = Some(ZenRestoreState {
                sidebar_open: self.sidebar_open,
                search_open: self.search_open,
            });
        }
        self.sidebar_open = false;
        self.search_open = false;
        self.overlay = Overlay::None;
        self.mindmap_panel_drag = None;
        self.editor = Some(iced::widget::text_editor::Content::with_text(
            self.source.as_str(),
        ));
        self.edit_history.clear();
        self.edit_redo.clear();
        self.view_mode = ViewMode::Raw;
        Task::none()
    }

    fn leave_zen_edit_mode(&mut self, sync_editor: bool) {
        if sync_editor {
            self.sync_editor_to_source();
        }
        self.editor = None;
        self.edit_history.clear();
        self.edit_redo.clear();
        self.view_mode = ViewMode::Rendered;
        self.restore_zen_chrome();
    }

    fn exit_zen_edit_mode(&mut self) -> Task<Message> {
        self.leave_zen_edit_mode(true);
        self.restore_body_scroll()
    }

    fn restore_zen_chrome(&mut self) {
        if let Some(restore) = self.zen_restore.take() {
            self.sidebar_open = restore.sidebar_open;
            self.search_open = restore.search_open;
        }
    }

    fn unsaved_edits_open_message(&self) -> String {
        format!(
            "unsaved edits in {}; save or discard before opening another",
            self.file
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        )
    }

    fn block_file_open_if_dirty(&mut self) -> Option<Task<Message>> {
        if self.dirty {
            Some(self.show_toast(self.unsaved_edits_open_message()))
        } else {
            None
        }
    }

    fn cancel_refresh_tracking(&mut self) {
        self.pending_refresh = None;
        self.pending_refresh_file = None;
        self.pending_refresh_workspace = None;
        self.pending_refresh_full_mindmap_workspace = None;
    }

    fn bump_file_refresh_generation(&mut self) {
        self.file_refresh_generation = self.file_refresh_generation.wrapping_add(1);
    }

    /// A Quick Slot read is the newest file intent: older generic loads and an
    /// in-flight refresh must not land after it and override the slot.
    fn supersede_file_loads(&mut self) {
        self.cancel_refresh_tracking();
        self.bump_file_refresh_generation();
    }

    fn load_file_unless_dirty(&mut self, path: PathBuf) -> Task<Message> {
        self.cancel_refresh_tracking();
        if let Some(blocked) = self.block_file_open_if_dirty() {
            return blocked;
        }
        let checkpoint = self.checkpoint_active_quick_slot();
        self.invalidate_pending_quick_slot_restore();
        Task::batch([checkpoint, self.begin_generic_file_load(path)])
    }

    fn begin_generic_file_load(&mut self, path: PathBuf) -> Task<Message> {
        self.cancel_refresh_tracking();
        self.bump_file_refresh_generation();
        let generation = self.file_refresh_generation;
        Task::perform(load_file(path), move |result| Message::FileLoadCompleted {
            generation,
            result,
        })
    }

    /// Apply file contents after the caller has established async ownership
    /// and rechecked the dirty guard. Refresh uses this directly so the common
    /// state transition does not cancel its still-pending workspace leg.
    fn apply_loaded_file(&mut self, path: PathBuf, src: String) -> Task<Message> {
        self.quick_slot_relative_memo.replace(None);
        let pending_candidate = self.pending_quick_slot_restore.clone();
        let pending_slot_restore = pending_candidate.clone().filter(|pending| {
            self.quick_slot_restore_is_current(pending)
                && self
                    .quick_slots_workspace_root()
                    .and_then(|root| {
                        crate::quick_slots::resolve_path(root, &pending.slot.relative_path)
                    })
                    .is_some_and(|target| target == path)
        });
        if pending_candidate.is_some() && pending_slot_restore.is_none() {
            self.invalidate_pending_quick_slot_restore();
        }
        let switching_file = self.file.as_deref() != Some(path.as_path());
        let refresh_full_mindmap_preview = self.full_mindmap.as_ref().is_some_and(|full| {
            matches!(
                full.selected.as_ref(),
                Some(WorkspaceNodeId::File(selected)) if selected == &path
            )
        });
        self.bump_file_refresh_generation();
        crate::recent::add(&path);
        if switching_file && pending_slot_restore.is_none() {
            // Manual/file-finder navigation leaves the old slot checkpointed
            // but unslotted; only an explicit Quick Slot activation may make
            // the new file active.
            self.clear_active_quick_slot();
        }
        if self.view_mode == ViewMode::Raw || self.editor.is_some() {
            self.leave_zen_edit_mode(false);
        }
        if self.workspace.is_none() {
            if let Some(parent) = path.parent().map(PathBuf::from) {
                self.set_workspace(parent, false);
            }
        }
        // Opening a DIFFERENT file: the body scrollable's offset gets clamped
        // by iced on the next layout, but if the new content fits the viewport
        // no scroll notification ever fires — the stale viewport would poison
        // body-offset math. Watcher reloads of the same file keep it.
        if self.file.as_deref() != Some(path.as_path()) {
            self.body_viewport = None;
        }
        self.source = src;
        self.saved_source = self.source.clone();
        self.file = Some(path.clone());
        self.dirty = false;
        self.outline_cursor = 0;
        self.is_data_doc = data_lang_for(self.file.as_deref()).is_some();
        self.mindmap_collapsed.clear();
        self.mindmap_selected = None;
        self.mindmap_panel_shown = None;
        self.load_ast_from_source();
        self.error = None;
        self.rebuild_matches();
        self.mindmap_focus_first_child();
        self.reveal_current_file();
        let mut fetches: Vec<Task<Message>> = Vec::new();
        for (_id, block) in &self.ast {
            if let Block::Image { url, .. } = block {
                if is_remote_url(url) && !self.image_cache.contains_key(url) {
                    self.image_cache.insert(url.clone(), ImageState::Loading);
                    let url = url.clone();
                    fetches.push(Task::perform(fetch_image(url), |(url, result)| {
                        Message::ImageFetched(url, result)
                    }));
                }
            }
        }
        self.refresh_diagram_theme_id();
        let prime = self.prime_diagram_cache();
        let nav_task: Task<Message> = if let Some(nav) = self.pending_nav.take() {
            let line = nav
                .fragment
                .as_deref()
                .and_then(|fragment| {
                    line_for_fragment(&self.source, fragment, is_tex_path(self.file.as_deref()))
                })
                .or(nav.line);
            Task::done(Message::Ipc(
                crate::ipc::Request {
                    id: 0,
                    cmd: crate::ipc::Cmd::Goto {
                        line,
                        section: nav.section,
                        focus: crate::ipc::FocusBehavior::Default,
                    },
                },
                std::sync::Arc::new(std::sync::Mutex::new(None)),
            ))
        } else {
            Task::none()
        };
        fetches.push(prime);
        fetches.push(nav_task);
        let slot_restore = pending_slot_restore.map_or_else(Task::none, |pending| {
            self.apply_quick_slot_restore_after_file(&path, pending.slot)
        });
        fetches.push(slot_restore);
        if refresh_full_mindmap_preview {
            let source = self.source_snapshot_for_preview();
            fetches.push(self.begin_full_mindmap_preview_source(path, source));
        }
        Task::batch(fetches)
    }

    fn new_full_mindmap_state() -> FullMindmapState {
        static NEXT_PREVIEW_NAMESPACE: AtomicU64 = AtomicU64::new(1);
        let preview_namespace = NEXT_PREVIEW_NAMESPACE.fetch_add(1, Ordering::Relaxed);
        let (preview_settle_tx, _) =
            tokio::sync::watch::channel::<Option<PendingFullMindmapPreviewSettle>>(None);
        FullMindmapState {
            selected: None,
            expanded: HashSet::new(),
            panel_open: false,
            panel_width: MIND_PANEL_DEFAULT,
            panel_step: 0,
            panel_drag: None,
            pending_open: None,
            pending_preview_settle: None,
            pending_preview: None,
            pending_workspace_load: None,
            pending_folder_loads: HashMap::new(),
            verification_wave: None,
            verification_followup_pending: false,
            verification_hidden: HashSet::new(),
            expansion_generation: 0,
            materialized_folders: HashMap::new(),
            deferred_file_selection: None,
            focus_request: None,
            preview: FullMindmapPreview::None,
            preview_identity: None,
            preview_namespace,
            preview_shape_ready: false,
            preview_window: crate::virt::VirtWindow::default(),
            preview_viewport: None,
            preview_height_cache: crate::virt::HeightCache::default(),
            preview_measurement_pending: false,
            preview_measurement_range: None,
            preview_measurement_generation: None,
            preview_generation: 0,
            preview_work_epoch: Arc::new(AtomicU64::new(0)),
            preview_parse_gate: Arc::new(tokio::sync::Semaphore::new(3)),
            preview_settle_tx,
            preview_settle_worker_started: false,
            preview_loading_images: HashMap::new(),
            preview_pending_diagrams: HashMap::new(),
            preview_asset_images: HashSet::new(),
            preview_failed_images: HashSet::new(),
            preview_asset_diagrams: HashSet::new(),
            preview_asset_cursor: 0,
            preview_asset_cursor_tag: None,
            preview_asset_wave_id: 0,
            load_error: None,
            layout_cache: std::cell::RefCell::new(None),
            layout_generation: 0,
        }
    }

    fn full_mindmap_start_folder(&self) -> Option<PathBuf> {
        self.workspace.clone().or_else(|| {
            self.file
                .as_ref()
                .and_then(|file| file.parent().map(PathBuf::from))
        })
    }

    fn full_mindmap_graph(&self) -> Option<std::sync::Arc<WorkspaceGraph>> {
        let full = self.full_mindmap.as_ref()?;
        if let Some(graph) = full.layout_cache.borrow().as_ref() {
            return Some(std::sync::Arc::clone(graph));
        }
        let graph = std::sync::Arc::new(workspace_mindmap::from_tree_with_hidden(
            self.workspace_tree.as_ref()?,
            &full.expanded,
            &full.materialized_folders,
            &full.pending_folder_loads.keys().cloned().collect(),
            &full.verification_hidden,
            self.workspace_truncated,
        ));
        *full.layout_cache.borrow_mut() = Some(std::sync::Arc::clone(&graph));
        Some(graph)
    }

    fn invalidate_full_mindmap_layout(&mut self) {
        if let Some(full) = self.full_mindmap.as_mut() {
            // Invalidate the cache and publish a distinct focus-generation
            // signal. A selected folder can survive a graph rebuild, so the
            // canvas cannot infer this intent from selection identity alone.
            full.layout_generation = full.layout_generation.wrapping_add(1);
            *full.layout_cache.borrow_mut() = None;
        }
    }

    /// Cancel all delayed-reveal ownership. The spawned filesystem futures
    /// cannot be force-stopped by Iced, so clearing the wave and hidden set is
    /// the cancellation boundary; completion handlers then fail the strict
    /// identity check and cannot reveal stale nodes.
    fn cancel_full_mindmap_verification(&mut self) {
        let had_hidden = self.full_mindmap.as_ref().is_some_and(|full| {
            full.verification_wave.is_some() || !full.verification_hidden.is_empty()
        });
        if let Some(full) = self.full_mindmap.as_mut() {
            full.verification_wave = None;
            full.verification_followup_pending = false;
            full.verification_hidden.clear();
        }
        self.full_mindmap_progress = None;
        if had_hidden {
            self.invalidate_full_mindmap_layout();
        }
    }

    fn bump_full_mindmap_expansion_generation(&mut self) {
        if let Some(full) = self.full_mindmap.as_mut() {
            full.expansion_generation = full.expansion_generation.wrapping_add(1);
        }
    }

    fn full_mindmap_folder_count(
        &self,
        path: &std::path::Path,
    ) -> Option<tree::RecursiveFileCount> {
        let full = self.full_mindmap.as_ref()?;
        if let Some(materialized) = full.materialized_folders.get(path) {
            return Some(match materialized {
                workspace_mindmap::MaterializedFolder::Verified {
                    recursive_supported_file_count,
                    ..
                } => *recursive_supported_file_count,
                workspace_mindmap::MaterializedFolder::Loaded {
                    recursive_supported_file_count,
                    ..
                } => *recursive_supported_file_count,
                workspace_mindmap::MaterializedFolder::Error(_) => {
                    tree::RecursiveFileCount::Unavailable
                }
            });
        }
        // A lazily expanded parent owns shallow child skeletons that are no
        // longer reachable through the retained workspace tree. Resolve the
        // child's count from that in-memory branch before giving up.
        for materialized in full.materialized_folders.values() {
            if let workspace_mindmap::MaterializedFolder::Loaded { folders, .. } = materialized {
                if let Some(node) = folders.iter().find(|node| node.path == path) {
                    return node.recursive_supported_file_count;
                }
            }
        }
        self.workspace_tree
            .as_ref()
            .and_then(|root| tree::find_folder(root, path))
            .and_then(|node| node.recursive_supported_file_count)
    }

    /// Snapshot unresolved LowerBound(0) folders that are actually visible
    /// beneath an expanded parent. The graph is authoritative: collapsed
    /// descendants and folders hidden by an older wave never enter the queue.
    fn full_mindmap_verification_candidates(&self) -> (Vec<PathBuf>, HashMap<PathBuf, PathBuf>) {
        let Some(full) = self.full_mindmap.as_ref() else {
            return (Vec::new(), HashMap::new());
        };
        let Some(graph) = self.full_mindmap_graph() else {
            return (Vec::new(), HashMap::new());
        };
        let expanded = full.expanded.clone();
        let mut candidates = Vec::new();
        let mut parent_by_candidate = HashMap::new();
        for node in graph.nodes.iter() {
            let Some(WorkspaceNodeId::Folder(path)) = node.id.as_ref() else {
                continue;
            };
            // The acted-on folder itself stays visible while its explicit
            // expansion owns a branch-local load. Delayed verification is for
            // unresolved children on the newly expanded frontier; hiding the
            // parent would remove the user's selection and suppress its
            // Loading files status.
            if expanded.contains(path) {
                continue;
            }
            // Count/status facts already accepted for this path (including a
            // count-only verification result) are authoritative until the
            // branch is explicitly expanded; do not rescan them on every
            // sibling materialization.
            if full.materialized_folders.contains_key(path) {
                continue;
            }
            if !matches!(
                self.full_mindmap_folder_count(path),
                Some(tree::RecursiveFileCount::LowerBound(0))
            ) {
                continue;
            }
            let Some(parent) = graph.parent(&WorkspaceNodeId::Folder(path.clone())) else {
                continue;
            };
            if !matches!(
                parent,
                WorkspaceNodeId::Root(_) | WorkspaceNodeId::Folder(_)
            ) || !expanded.contains(parent.path())
            {
                continue;
            }
            if parent_by_candidate
                .insert(path.clone(), parent.path().to_path_buf())
                .is_none()
            {
                candidates.push(path.clone());
            }
        }
        // Graph order is stable and follows the user's visible tree. Keep the
        // first fixed prefix; excess candidates remain visible with the
        // retained scan-limit label rather than being silently dropped.
        candidates.truncate(FULL_MINDMAP_VERIFICATION_MAX_CANDIDATES);
        parent_by_candidate.retain(|path, _| candidates.contains(path));
        (candidates, parent_by_candidate)
    }

    /// Keep newly materialized frontier shells hidden without mutating the
    /// active wave's fixed candidate list/denominator. When that wave drains,
    /// the pending shells are included by a fresh fixed snapshot. If no wave
    /// is active, start that snapshot immediately in this same update so the
    /// shells cannot flash for one rendered frame.
    fn schedule_full_mindmap_verification_followup(&mut self) -> Task<Message> {
        let active = self
            .full_mindmap
            .as_ref()
            .is_some_and(|full| full.verification_wave.is_some());
        if !active {
            return self.begin_full_mindmap_verification_wave();
        }

        let (candidates, _) = self.full_mindmap_verification_candidates();
        let active_candidates = self
            .full_mindmap
            .as_ref()
            .and_then(|full| full.verification_wave.as_ref())
            .map(|wave| wave.candidates.iter().collect::<HashSet<_>>())
            .unwrap_or_default();
        let new_candidates = candidates
            .into_iter()
            .filter(|path| {
                // A verification result already owns only count/status for
                // this path; it must not be scheduled repeatedly on every
                // sibling materialization.
                !self
                    .full_mindmap
                    .as_ref()
                    .is_some_and(|full| full.materialized_folders.contains_key(path))
                    && !active_candidates.contains(path)
            })
            .collect::<Vec<_>>();
        if new_candidates.is_empty() {
            return Task::none();
        }
        if let Some(full) = self.full_mindmap.as_mut() {
            full.verification_hidden.extend(new_candidates);
            full.verification_followup_pending = true;
        }
        self.invalidate_full_mindmap_layout();
        Task::none()
    }

    /// Start one fixed wave. The denominator is frozen before any worker is
    /// launched and therefore cannot change when a result exposes new shells.
    fn begin_full_mindmap_verification_wave(&mut self) -> Task<Message> {
        let Some(workspace_root) = self.workspace.clone() else {
            self.cancel_full_mindmap_verification();
            return Task::none();
        };
        if self.workspace_snapshot_show_hidden != self.show_hidden {
            self.cancel_full_mindmap_verification();
            return Task::none();
        }
        // Rebuild the candidate snapshot from the currently visible frontier,
        // never from a graph whose old wave still hides shells.
        self.cancel_full_mindmap_verification();
        let (candidates, parent_by_candidate) = self.full_mindmap_verification_candidates();
        if candidates.is_empty() {
            return Task::none();
        }
        self.full_mindmap_request_seq = self.full_mindmap_request_seq.wrapping_add(1);
        let wave_id = self.full_mindmap_request_seq;
        let show_hidden = self.show_hidden;
        let parent_expansion_generation = self
            .full_mindmap
            .as_ref()
            .map(|full| full.expansion_generation)
            .unwrap_or_default();
        let total = candidates.len();
        if let Some(full) = self.full_mindmap.as_mut() {
            full.verification_hidden = candidates.iter().cloned().collect();
            full.verification_wave = Some(FullMindmapVerificationWave {
                id: wave_id,
                workspace_root,
                show_hidden,
                parent_expansion_generation,
                candidates,
                parent_by_candidate,
                next_index: 0,
                in_flight: HashSet::new(),
                request_ids: HashMap::new(),
                checked: 0,
            });
        }
        self.full_mindmap_progress = Some(FullMindmapProgress {
            wave_id,
            checked: 0,
            total,
        });
        self.invalidate_full_mindmap_layout();
        self.launch_full_mindmap_verification_tasks()
    }

    /// Fill the fixed worker window from the wave's immutable candidate queue.
    fn launch_full_mindmap_verification_tasks(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        loop {
            let next = {
                let Some(full) = self.full_mindmap.as_mut() else {
                    break;
                };
                let Some(wave) = full.verification_wave.as_mut() else {
                    break;
                };
                if wave.in_flight.len() >= FULL_MINDMAP_VERIFICATION_CONCURRENCY
                    || wave.next_index >= wave.candidates.len()
                {
                    break;
                }
                let folder = wave.candidates[wave.next_index].clone();
                wave.next_index += 1;
                wave.in_flight.insert(folder.clone());
                Some((
                    wave.id,
                    wave.workspace_root.clone(),
                    wave.show_hidden,
                    wave.parent_expansion_generation,
                    wave.parent_by_candidate
                        .get(&folder)
                        .cloned()
                        .unwrap_or_else(|| wave.workspace_root.clone()),
                    folder,
                ))
            };
            let Some((
                wave_id,
                workspace_root,
                show_hidden,
                parent_expansion_generation,
                parent,
                folder,
            )) = next
            else {
                break;
            };
            self.full_mindmap_request_seq = self.full_mindmap_request_seq.wrapping_add(1);
            let request = PendingFullMindmapVerification {
                id: self.full_mindmap_request_seq,
                wave_id,
                workspace_root,
                parent,
                parent_expansion_generation,
                folder: folder.clone(),
                show_hidden,
            };
            if let Some(full) = self.full_mindmap.as_mut() {
                if let Some(wave) = full.verification_wave.as_mut() {
                    wave.request_ids.insert(folder.clone(), request.id);
                }
            }
            tasks.push(Task::perform(
                load_full_mindmap_folder(folder, show_hidden),
                move |result| Message::FullMindmapVerificationLoaded { request, result },
            ));
        }
        Task::batch(tasks)
    }

    fn handle_full_mindmap_verification_loaded(
        &mut self,
        request: PendingFullMindmapVerification,
        result: Result<(PathBuf, tree::ExpandedFolderSnapshot), String>,
    ) -> Task<Message> {
        let current = self.workspace.as_ref() == Some(&request.workspace_root)
            && self.show_hidden == request.show_hidden
            && self.workspace_snapshot_show_hidden == request.show_hidden
            && self.full_mindmap.as_ref().is_some_and(|full| {
                full.expansion_generation == request.parent_expansion_generation
                    && full.expanded.contains(&request.parent)
                    && full.verification_wave.as_ref().is_some_and(|wave| {
                        wave.id == request.wave_id
                            && wave.workspace_root == request.workspace_root
                            && wave.show_hidden == request.show_hidden
                            && wave.parent_expansion_generation
                                == request.parent_expansion_generation
                            && wave.candidates.contains(&request.folder)
                            && wave.in_flight.contains(&request.folder)
                            && wave.request_ids.get(&request.folder) == Some(&request.id)
                    })
            });
        if !current {
            return Task::none();
        }

        let mut accepted_exact_empty = false;
        let materialized = match result {
            Ok((path, snapshot)) if path == request.folder => {
                accepted_exact_empty = matches!(
                    snapshot.recursive_supported_file_count,
                    tree::RecursiveFileCount::Exact(0)
                );
                workspace_mindmap::MaterializedFolder::Verified {
                    recursive_supported_file_count: snapshot.recursive_supported_file_count,
                    truncated: snapshot.truncated,
                }
            }
            Ok((path, _)) => workspace_mindmap::MaterializedFolder::Error(format!(
                "Verified unexpected folder: {}",
                path.display()
            )),
            Err(error) => workspace_mindmap::MaterializedFolder::Error(error),
        };

        let (done, followup_pending) = if let Some(full) = self.full_mindmap.as_mut() {
            full.verification_hidden.remove(&request.folder);
            full.materialized_folders
                .insert(request.folder.clone(), materialized);
            let wave = full
                .verification_wave
                .as_mut()
                .expect("verified request owns a wave");
            wave.in_flight.remove(&request.folder);
            wave.request_ids.remove(&request.folder);
            // Each accepted result advances exactly once. This monotonic
            // counter drives both the determinate bar and remaining label.
            wave.checked = wave.checked.saturating_add(1);
            let done = wave.checked >= wave.candidates.len() && wave.in_flight.is_empty();
            let followup_pending = done && full.verification_followup_pending;
            if done {
                full.verification_wave = None;
                full.verification_followup_pending = false;
                if !followup_pending {
                    full.verification_hidden.clear();
                    self.full_mindmap_progress = None;
                }
            } else {
                self.full_mindmap_progress = Some(FullMindmapProgress {
                    wave_id: wave.id,
                    checked: wave.checked,
                    total: wave.candidates.len(),
                });
            }
            (done, followup_pending)
        } else {
            return Task::none();
        };
        self.invalidate_full_mindmap_layout();
        if accepted_exact_empty {
            self.normalize_full_mindmap_workspace();
        }
        if done {
            if followup_pending {
                self.schedule_full_mindmap_verification_followup()
            } else {
                Task::none()
            }
        } else {
            self.launch_full_mindmap_verification_tasks()
        }
    }

    fn enter_full_mindmap(&mut self) -> Task<Message> {
        self.enter_full_mindmap_at(None)
    }

    /// Enter Full Mindmap, optionally forcing a fresh workspace root. The
    /// document-Mindmap boundary uses the override when the current file is
    /// outside the existing workspace; ordinary Full Mindmap entry preserves
    /// its already-indexed workspace exactly as before.
    fn enter_full_mindmap_at(&mut self, forced_start: Option<PathBuf>) -> Task<Message> {
        self.cancel_refresh_tracking();
        self.overlay = Overlay::None;
        if self.full_mindmap.is_some() {
            self.cancel_full_mindmap_verification();
            // Re-entry replaces the navigator state. Bump the preview epoch
            // before dropping the old state so any active read/parse exits at
            // its next cooperative checkpoint instead of blocking new work.
            self.reset_full_mindmap_preview_window();
        }
        self.full_mindmap = Some(Self::new_full_mindmap_state());
        if forced_start.is_none() && self.workspace_tree.is_some() {
            self.reset_full_mindmap_workspace();
            let verification = self.begin_full_mindmap_verification_wave();
            let expanded = self.begin_full_mindmap_expanded_folder_loads();
            Task::batch([verification, expanded])
        } else {
            let start = forced_start
                .or_else(|| self.full_mindmap_start_folder())
                .or_else(Picker::home);
            start.map_or_else(Task::none, |path| {
                self.begin_full_mindmap_workspace_load(path, false, None, false, false, false)
            })
        }
    }

    /// Return to the workspace navigator from document-level Mindmap. Keep an
    /// existing workspace only when its accepted bounded index contains the
    /// current file; otherwise adopt the file's parent (or Home) through the
    /// same background workspace-load path used by every other Full Mindmap
    /// root change.
    fn enter_full_mindmap_for_current_file(&mut self) -> Task<Message> {
        let Some(file) = self.file.clone() else {
            return Task::none();
        };
        let in_workspace = self
            .workspace
            .as_ref()
            .is_some_and(|root| file.starts_with(root))
            && self.workspace_files.contains(&file);
        let forced_start = (!in_workspace)
            .then(|| file.parent().map(PathBuf::from).or_else(Picker::home))
            .flatten();
        // The document-Mindmap root-left gesture replaces the reader surface
        // with Full Mindmap before the workspace helper can observe it. Capture
        // the outgoing document context at this boundary; the helper's later
        // Full Mindmap checkpoint is a no-op because the new navigator has no
        // selected preview yet.
        let checkpoint = self.checkpoint_active_quick_slot();
        Task::batch([checkpoint, self.enter_full_mindmap_at(forced_start)])
    }

    /// Exit Full Mindmap without exposing a workspace snapshot built under a
    /// different hidden-file filter. Normal exits restore the prior surface;
    /// the explicit Files action additionally opens the sidebar.
    fn exit_full_mindmap(&mut self, return_to_files: bool) -> Task<Message> {
        let stale_workspace =
            self.full_mindmap.is_some() && self.workspace_snapshot_show_hidden != self.show_hidden;
        if stale_workspace {
            if let Some(path) = self.workspace.clone() {
                return self.begin_full_mindmap_workspace_load(
                    path,
                    false,
                    None,
                    true,
                    return_to_files,
                    true,
                );
            }
        }
        self.finish_full_mindmap_exit(return_to_files)
    }

    fn finish_full_mindmap_exit(&mut self, return_to_files: bool) -> Task<Message> {
        self.cancel_refresh_tracking();
        self.cancel_full_mindmap_verification();
        self.reset_full_mindmap_preview_window();
        self.quick_slot_preview_restore = None;
        self.full_mindmap = None;
        if return_to_files {
            self.sidebar_open = true;
            self.sidebar_tab = SidebarTab::Files;
            self.reveal_current_file();
        }
        self.restore_body_scroll()
    }

    /// Start an IPC file activation only after Full Mindmap has stopped owning
    /// the reader surface. A normal exit clears the navigator synchronously;
    /// when the hidden-file snapshot is stale, `exit_full_mindmap` completes
    /// through `FullMindmapWorkspaceLoaded` and consumes the same pending open
    /// there.
    fn begin_ipc_file_open(&mut self, path: PathBuf, nav: Option<PendingNav>) -> Task<Message> {
        // An IPC/file-finder navigation supersedes any in-flight slot load;
        // its completion must not later reopen the older slot target.
        let path = canonicalize_existing_path(path);
        // Capture the outgoing reading context before IPC navigation changes
        // the visible file (or asks Full Mindmap to relinquish its preview).
        // `checkpoint_active_quick_slot` mutates the in-memory slot
        // synchronously; the returned task only debounces persistence.
        let checkpoint = self.checkpoint_active_quick_slot();
        self.invalidate_pending_quick_slot_restore();
        if self.full_mindmap.is_none() {
            self.pending_nav = nav;
            return Task::batch([checkpoint, self.begin_generic_file_load(path)]);
        }

        // Do not let an older generic load consume the navigation intended for
        // this deferred request while the navigator is still visible.
        self.pending_nav = None;
        self.pending_ipc_file_open = Some(PendingIpcFileOpen { path, nav });
        let exit = self.exit_full_mindmap(false);
        if self.full_mindmap.is_none() {
            Task::batch([checkpoint, self.start_pending_ipc_file_open(exit)])
        } else {
            Task::batch([checkpoint, exit])
        }
    }

    fn start_pending_ipc_file_open(&mut self, cleanup: Task<Message>) -> Task<Message> {
        let Some(PendingIpcFileOpen { path, nav }) = self.pending_ipc_file_open.take() else {
            return cleanup;
        };
        self.cancel_refresh_tracking();
        self.pending_nav = nav;
        let load = self.begin_generic_file_load(path);
        Task::batch([cleanup, load])
    }

    /// Start (or refresh) the workspace phase without touching sidebar state.
    fn reset_full_mindmap_workspace(&mut self) {
        let Some(tree) = self.workspace_tree.as_ref() else {
            return;
        };
        let root = tree.path.clone();
        let current_file = self
            .file
            .as_ref()
            .filter(|file| file.starts_with(&root))
            .cloned();
        let ancestors = current_file
            .as_ref()
            .map(|file| tree::ancestors_of(&root, file))
            .unwrap_or_default();
        self.reset_full_mindmap_preview_window();
        {
            let Some(full) = self.full_mindmap.as_mut() else {
                return;
            };
            full.expanded.clear();
            full.expanded.insert(root.clone());
            for ancestor in ancestors {
                full.expanded.insert(ancestor);
            }
            let root_id = WorkspaceNodeId::Root(root);
            full.selected = Some(root_id.clone());
            // The document-Mindmap root-Left bridge already knows which file
            // must own the first Full Mindmap viewport. Keep that request
            // alive before the branch listing exposes the file node; the
            // canvas will deliberately defer consuming it until the rendered
            // WorkspaceGraph contains the exact file. Without this early
            // identity, the first root frame becomes the only focus event and
            // a later materialization can leave the viewport at the root.
            full.focus_request = Some(
                current_file
                    .as_ref()
                    .map(|path| WorkspaceNodeId::File(path.clone()))
                    .unwrap_or(root_id),
            );
            full.panel_drag = None;
            // Rebuilding the workspace means navigation has changed intent. An
            // in-flight file read from the prior workspace must not later exit
            // the navigator into that old document.
            full.pending_open = None;
            full.pending_preview_settle = None;
            full.pending_preview = None;
            full.pending_workspace_load = None;
            full.pending_folder_loads.clear();
            full.verification_wave = None;
            full.verification_followup_pending = false;
            full.verification_hidden.clear();
            full.expansion_generation = full.expansion_generation.wrapping_add(1);
            full.materialized_folders.clear();
            full.deferred_file_selection = current_file;
            full.preview = FullMindmapPreview::None;
            full.load_error = None;
            full.layout_generation = full.layout_generation.wrapping_add(1);
            *full.layout_cache.borrow_mut() = None;
        }
        self.full_mindmap_progress = None;
        self.invalidate_full_mindmap_layout();
    }

    /// Keep a Full Mindmap selection valid after rebuilding the source tree
    /// (for example after toggling hidden files) without sharing sidebar state.
    fn normalize_full_mindmap_workspace(&mut self) {
        let selected = match self.full_mindmap.as_ref() {
            Some(FullMindmapState { selected, .. }) => selected.clone(),
            _ => return,
        };
        if selected.is_none() {
            // Explicit canvas deselection is a valid latest navigation state.
            // A preserve-navigation snapshot refresh may replace graph data,
            // but it must not manufacture a root selection/focus afterward.
            if let Some(full) = self.full_mindmap.as_mut() {
                full.focus_request = None;
            }
            return;
        }
        let Some(graph) = self.full_mindmap_graph() else {
            return;
        };
        let deferred = self
            .full_mindmap
            .as_ref()
            .and_then(|full| full.deferred_file_selection.as_ref());
        let deferred_file = selected
            .as_ref()
            .is_some_and(|id| matches!(id, WorkspaceNodeId::File(path) if deferred == Some(path)));
        let next = selected
            .clone()
            .filter(|id| graph.node(id).is_some() || deferred_file)
            .or_else(|| {
                selected
                    .as_ref()
                    .and_then(|id| graph.nearest_visible_ancestor(id.path()))
            })
            .unwrap_or_else(|| graph.root_id());
        let selection_changed = self
            .full_mindmap
            .as_ref()
            .is_some_and(|full| full.selected.as_ref() != Some(&next));
        // A bridge entry records a file focus before its parent listing is
        // accepted. Normalization may run for an intermediate root/sibling
        // snapshot, but it must not replace that still-pending file request
        // with the currently visible root. Explicit user navigation clears
        // `deferred_file_selection` first, so this preservation cannot block
        // a newer selection.
        let preserve_deferred_file_focus = self.full_mindmap.as_ref().is_some_and(|full| {
            matches!(
                    (&full.focus_request, &full.deferred_file_selection),
                    (
                        Some(WorkspaceNodeId::File(focus)),
                        Some(deferred),
                    ) if focus == deferred
            )
        });
        if selection_changed {
            self.reset_full_mindmap_preview_window();
        }
        if let Some(full) = self.full_mindmap.as_mut() {
            if selection_changed {
                full.pending_preview_settle = None;
                full.pending_preview = None;
                full.preview = FullMindmapPreview::None;
            }
            if !preserve_deferred_file_focus && full.focus_request.as_ref() != Some(&next) {
                full.focus_request = Some(next.clone());
            }
            full.selected = Some(next);
        }
    }

    fn replace_workspace_snapshot(&mut self, path: PathBuf, snapshot: tree::WorkspaceSnapshot) {
        let path = canonicalize_existing_path(path);
        let root_changed = self.quick_slots_workspace_root().is_some_and(|current| {
            crate::quick_slots::workspace_key(current) != crate::quick_slots::workspace_key(&path)
        });
        if root_changed {
            self.invalidate_pending_quick_slot_restore();
        }
        let _ = self.checkpoint_active_quick_slot();
        self.persist_quick_slots_now();
        self.workspace_files = snapshot.files;
        self.workspace_sidebar_files = snapshot.sidebar_files;
        self.workspace_tree = Some(snapshot.root);
        self.workspace_snapshot_show_hidden = self.show_hidden;
        self.workspace_truncated = snapshot.truncated;
        self.workspace = Some(path);
        if let Some(root) = self.workspace.clone() {
            self.load_quick_slots_for_workspace(&root);
        }
    }

    fn apply_workspace_snapshot(
        &mut self,
        path: PathBuf,
        snapshot: tree::WorkspaceSnapshot,
        open_sidebar: bool,
    ) {
        self.replace_workspace_snapshot(path, snapshot);
        self.expanded.clear();
        if let Some(tree) = &self.workspace_tree {
            self.expanded.insert(tree.path.clone());
        }
        if open_sidebar {
            self.sidebar_open = true;
        }
        self.tree_cursor = 0;
        self.overlay = Overlay::None;
        self.picker = None;
        if self.full_mindmap.is_some() {
            self.reset_full_mindmap_workspace();
            self.normalize_full_mindmap_workspace();
        }
    }

    fn apply_refreshed_workspace_snapshot(
        &mut self,
        path: PathBuf,
        snapshot: tree::WorkspaceSnapshot,
    ) {
        let expanded = self.expanded.clone();
        self.replace_workspace_snapshot(path, snapshot);
        let root = self
            .workspace_tree
            .as_ref()
            .expect("workspace snapshot was just installed")
            .path
            .clone();
        let retained: HashSet<PathBuf> = expanded
            .into_iter()
            .filter(|folder| {
                *folder == root
                    || self
                        .workspace_tree
                        .as_ref()
                        .is_some_and(|tree| tree::find_folder(tree, folder).is_some())
            })
            .collect();
        self.expanded = retained;
        self.expanded.insert(root);
        let row_count = self
            .workspace_tree
            .as_ref()
            .map(|tree| {
                tree::flatten_with_files(tree, &self.workspace_sidebar_files, &self.expanded).len()
            })
            .unwrap_or(0);
        self.tree_cursor = self.tree_cursor.min(row_count.saturating_sub(1));
        self.reveal_current_file();
        self.error = None;
    }

    fn begin_refresh_workspace_load(&mut self, path: PathBuf, id: u64) -> Task<Message> {
        let request = PendingRefreshWorkspace {
            id,
            path: path.clone(),
            show_hidden: self.show_hidden,
        };
        self.pending_refresh_workspace = Some(request.clone());
        Task::perform(
            load_workspace_snapshot(path, request.show_hidden),
            move |result| Message::RefreshWorkspaceLoaded { request, result },
        )
    }

    fn handle_refresh_workspace_loaded(
        &mut self,
        request: PendingRefreshWorkspace,
        result: Result<(PathBuf, tree::WorkspaceSnapshot), String>,
    ) -> Task<Message> {
        let current = self.pending_refresh_workspace.as_ref() == Some(&request)
            && self
                .pending_refresh
                .as_ref()
                .is_some_and(|refresh| refresh.id == request.id)
            && self.full_mindmap.is_none()
            && self.workspace.as_deref() == Some(request.path.as_path())
            && self.show_hidden == request.show_hidden;
        if !current {
            // A newer refresh, navigation, or hidden-file filter owns the
            // current workspace. This completion must not overwrite it.
            return Task::none();
        }

        self.pending_refresh_workspace = None;
        let workspace_error = match result {
            Ok((path, snapshot)) if path == request.path => {
                self.apply_refreshed_workspace_snapshot(path, snapshot);
                None
            }
            Ok((path, _)) => {
                let error = format!("Loaded unexpected folder: {}", path.display());
                self.error = Some(error.clone());
                Some(error)
            }
            Err(error) => {
                let message = format!("Couldn't refresh {}: {error}", request.path.display());
                self.error = Some(message.clone());
                Some(message)
            }
        };
        if let Some(refresh) = self.pending_refresh.as_mut() {
            if refresh.id == request.id {
                refresh.workspace_done = true;
                refresh.workspace_error = workspace_error;
            }
        }
        self.finish_refresh()
    }

    fn refresh_completion_label(tracker: &RefreshTracker) -> String {
        let mut errors = Vec::new();
        if let Some(error) = &tracker.workspace_error {
            errors.push(format!("Folder refresh failed: {error}"));
        }
        if let Some(error) = &tracker.file_error {
            errors.push(format!("File refresh failed: {error}"));
        }
        if !errors.is_empty() {
            return errors.join("; ");
        }

        if let Some(reason) = tracker.file_skip_reason {
            let reason = match reason {
                FileRefreshSkipReason::UnsavedEdits => "unsaved edits",
                FileRefreshSkipReason::DocumentChanged => "document changed",
            };
            return if tracker.has_workspace {
                format!("Folder refreshed; file refresh skipped ({reason})")
            } else {
                format!("File refresh skipped ({reason})")
            };
        }

        match (tracker.has_workspace, tracker.has_file) {
            (true, true) => "File and folder refreshed".to_string(),
            (true, false) => "Folder refreshed".to_string(),
            (false, true) => "File refreshed".to_string(),
            (false, false) => "Nothing to refresh".to_string(),
        }
    }

    fn finish_refresh(&mut self) -> Task<Message> {
        let Some(tracker) = self.pending_refresh.as_ref() else {
            return Task::none();
        };
        if !tracker.file_done || !tracker.workspace_done {
            return Task::none();
        }
        let tracker = self
            .pending_refresh
            .take()
            .expect("refresh tracker was just checked");
        self.pending_refresh_file = None;
        self.pending_refresh_workspace = None;
        self.pending_refresh_full_mindmap_workspace = None;
        let label = Self::refresh_completion_label(&tracker);
        if tracker.workspace_error.is_some() || tracker.file_error.is_some() {
            // A successful second leg may have cleared `self.error` while the
            // first leg's failure was still waiting for the transaction to
            // finish. Restore the aggregate failure as the final state.
            self.error = Some(label.clone());
        } else {
            // A successful retry owns the refresh error surface. This matters
            // in Full Mindmap, whose workspace snapshot path does not pass
            // through the ordinary helpers that already clear `self.error`.
            self.error = None;
        }
        self.show_toast(label)
    }

    fn refresh_status(&mut self) -> Task<Message> {
        self.cancel_refresh_tracking();
        self.refresh_seq = self.refresh_seq.wrapping_add(1);
        let refresh_id = self.refresh_seq;
        let has_workspace = self.workspace.is_some();
        let has_file = self.file.is_some();
        if has_file {
            // Refresh is the latest file-read intent. A generic load that
            // started before this command must not later replace its result.
            self.bump_file_refresh_generation();
        }
        let file_skip_reason =
            (has_file && self.dirty).then_some(FileRefreshSkipReason::UnsavedEdits);
        let mut tasks = Vec::new();
        let mut workspace_done = !has_workspace;

        if let Some(path) = self.workspace.clone() {
            if self.full_mindmap.is_some() {
                let exit_after_refresh = self.pending_ipc_file_open.is_some();
                tasks.push(self.begin_full_mindmap_workspace_load(
                    path,
                    false,
                    None,
                    true,
                    false,
                    exit_after_refresh,
                ));
                self.pending_refresh_full_mindmap_workspace = self
                    .full_mindmap
                    .as_ref()
                    .and_then(|full| full.pending_workspace_load.clone());
                workspace_done = self.pending_refresh_full_mindmap_workspace.is_none();
            } else {
                tasks.push(self.begin_refresh_workspace_load(path, refresh_id));
                workspace_done = false;
            }
        }

        let mut file_done = !has_file || file_skip_reason.is_some();
        if has_file && !self.dirty {
            if let Some(path) = self.file.clone() {
                let request = PendingRefreshFile {
                    id: refresh_id,
                    path: path.clone(),
                    generation: self.file_refresh_generation,
                };
                self.pending_refresh_file = Some(request.clone());
                file_done = false;
                tasks.push(Task::perform(load_file(path), move |result| {
                    Message::RefreshFileLoaded { request, result }
                }));
            }
        }

        self.pending_refresh = Some(RefreshTracker {
            id: refresh_id,
            has_workspace,
            has_file,
            file_skip_reason,
            file_done,
            workspace_done,
            file_error: None,
            workspace_error: None,
        });
        tasks.push(self.finish_refresh());
        Task::batch(tasks)
    }

    fn set_workspace(&mut self, path: PathBuf, open_sidebar: bool) {
        let path = canonicalize_existing_path(path);
        match tree::build_workspace(&path, self.show_hidden) {
            Ok(snapshot) => self.apply_workspace_snapshot(path, snapshot, open_sidebar),
            Err(error) => {
                let message = format!("Couldn't index {}: {error}", path.display());
                if let Some(full) = self.full_mindmap.as_mut() {
                    full.load_error = Some(message);
                } else {
                    self.error = Some(message);
                }
            }
        }
    }

    /// Full Mindmap workspace changes are intentionally background-only. A
    /// project root can contain thousands of unrelated entries; indexing it on
    /// the Iced update thread would freeze navigation and could exhaust memory.
    fn begin_full_mindmap_workspace_load(
        &mut self,
        path: PathBuf,
        select_root: bool,
        open_after: Option<PathBuf>,
        preserve_navigation: bool,
        return_to_files_after: bool,
        exit_after_refresh: bool,
    ) -> Task<Message> {
        let path = canonicalize_existing_path(path);
        self.cancel_refresh_tracking();
        // Full Mindmap workspace changes replace the selected preview/root;
        // preserve the outgoing slot context before any request state moves.
        // The non-Full-Mindmap path is covered by `set_workspace`'s snapshot
        // replacement, which checkpoints before swapping the active root.
        let checkpoint = if self.full_mindmap.is_some() {
            self.checkpoint_active_quick_slot()
        } else {
            Task::none()
        };
        if (self.pending_quick_slot_restore.is_some()
            || self.quick_slot_preview_restore_guard.is_some())
            && self.quick_slots_workspace_root().is_some_and(|current| {
                crate::quick_slots::workspace_key(current)
                    != crate::quick_slots::workspace_key(&path)
            })
        {
            self.invalidate_pending_quick_slot_restore();
        }
        if self.full_mindmap.is_none() {
            self.set_workspace(path, true);
            return Task::none();
        }
        let already_pending = self.full_mindmap.as_ref().is_some_and(|full| {
            full.pending_workspace_load.as_ref().is_some_and(|pending| {
                pending.path == path
                    && pending.select_root == select_root
                    && pending.open_after == open_after
                    && pending.preserve_navigation == preserve_navigation
                    && pending.return_to_files_after == return_to_files_after
                    && pending.exit_after_refresh == exit_after_refresh
            })
        });
        if already_pending {
            return checkpoint;
        }
        // Root/filter changes supersede every delayed-reveal worker. The
        // request identity check also guards futures that cannot be aborted.
        self.cancel_full_mindmap_verification();
        self.bump_full_mindmap_expansion_generation();
        if !preserve_navigation {
            self.reset_full_mindmap_preview_window();
        }
        self.full_mindmap_request_seq = self.full_mindmap_request_seq.wrapping_add(1);
        let request = PendingFullMindmapWorkspaceLoad {
            id: self.full_mindmap_request_seq,
            path: path.clone(),
            select_root,
            open_after,
            preserve_navigation,
            return_to_files_after,
            exit_after_refresh,
        };
        let full = self.full_mindmap.as_mut().expect("checked above");
        full.pending_workspace_load = Some(request.clone());
        // A new refresh owns the status line. Do not leave an error from a
        // prior failed snapshot visible after a later refresh succeeds.
        full.load_error = None;
        if preserve_navigation {
            if let Some(WorkspaceNodeId::File(path)) = full.selected.as_ref() {
                full.deferred_file_selection = Some(path.clone());
            }
        } else {
            full.deferred_file_selection = None;
        }
        full.pending_folder_loads.clear();
        full.materialized_folders.clear();
        if !preserve_navigation {
            full.pending_open = None;
        }
        if !preserve_navigation {
            full.pending_preview_settle = None;
            full.pending_preview = None;
            full.load_error = None;
            full.preview = FullMindmapPreview::None;
            full.preview_shape_ready = false;
        }
        self.invalidate_full_mindmap_layout();
        let show_hidden = self.show_hidden;
        Task::batch([
            checkpoint,
            Task::perform(load_workspace_snapshot(path, show_hidden), move |result| {
                Message::FullMindmapWorkspaceLoaded { request, result }
            }),
        ])
    }

    fn begin_full_mindmap_expanded_folder_loads(&mut self) -> Task<Message> {
        let folders = self
            .full_mindmap
            .as_ref()
            .map(|full| full.expanded.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        Task::batch(
            folders
                .into_iter()
                .map(|folder| self.begin_full_mindmap_folder_load(folder)),
        )
    }

    fn begin_full_mindmap_folder_load(&mut self, folder: PathBuf) -> Task<Message> {
        let folder = canonicalize_existing_path(folder);
        let Some(workspace_root) = self.workspace.clone() else {
            return Task::none();
        };
        let eligible = self.full_mindmap.as_ref().is_some_and(|full| {
            full.expanded.contains(&folder)
                && folder.starts_with(&workspace_root)
                && !full
                    .materialized_folders
                    .get(&folder)
                    .is_some_and(|materialized| {
                        matches!(
                            materialized,
                            workspace_mindmap::MaterializedFolder::Loaded { .. }
                                | workspace_mindmap::MaterializedFolder::Error(_)
                        )
                    })
                && !full.pending_folder_loads.contains_key(&folder)
                // A delayed-reveal wave owns unresolved shells, including an
                // expanded ancestor. Avoid a duplicate branch scan whose
                // completion could race the fixed wave's result.
                && !full.verification_hidden.contains(&folder)
        });
        if !eligible || self.workspace_snapshot_show_hidden != self.show_hidden {
            return Task::none();
        }

        // An exact retained node already owns complete folder structure and
        // the standard sidebar's pre-grouped immediate paths. Reuse those
        // bounded in-memory facts; only interrupted/unreadable or lazily
        // discovered shallow nodes need another filesystem pass.
        let retained_snapshot = self
            .workspace_tree
            .as_ref()
            .and_then(|root| tree::find_folder(root, &folder))
            .filter(|node| {
                matches!(
                    node.recursive_supported_file_count,
                    Some(tree::RecursiveFileCount::Exact(_))
                )
            })
            .map(|node| {
                let folders = node
                    .children
                    .iter()
                    .cloned()
                    .map(|mut child| {
                        child.children.clear();
                        child
                    })
                    .collect();
                let files = self.workspace_sidebar_files.files_for(&folder).to_vec();
                let count = node
                    .recursive_supported_file_count
                    .expect("filtered exact retained folder");
                (folders, files, count)
            });

        self.full_mindmap_request_seq = self.full_mindmap_request_seq.wrapping_add(1);
        let request = PendingFullMindmapFolderLoad {
            id: self.full_mindmap_request_seq,
            workspace_root,
            folder: folder.clone(),
            show_hidden: self.show_hidden,
        };
        let full = self.full_mindmap.as_mut().expect("eligible Full Mindmap");
        full.pending_folder_loads
            .insert(folder.clone(), request.clone());
        full.materialized_folders.remove(&folder);
        self.invalidate_full_mindmap_layout();
        if let Some((folders, files, recursive_supported_file_count)) = retained_snapshot {
            let result = Ok((
                folder,
                tree::ExpandedFolderSnapshot {
                    folders,
                    files,
                    recursive_supported_file_count,
                    truncated: false,
                },
            ));
            Task::done(Message::FullMindmapFolderLoaded { request, result })
        } else {
            Task::perform(
                load_full_mindmap_folder(folder, request.show_hidden),
                move |result| Message::FullMindmapFolderLoaded { request, result },
            )
        }
    }

    fn evict_full_mindmap_folder(&mut self, folder: &std::path::Path) {
        if let Some(full) = self.full_mindmap.as_mut() {
            full.expansion_generation = full.expansion_generation.wrapping_add(1);
            full.expanded.retain(|path| !path.starts_with(folder));
            full.pending_folder_loads
                .retain(|path, _| !path.starts_with(folder));
            full.materialized_folders
                .retain(|path, _| !path.starts_with(folder));
            if full
                .deferred_file_selection
                .as_ref()
                .is_some_and(|path| path.starts_with(folder))
            {
                full.deferred_file_selection = None;
            }
        }
        self.invalidate_full_mindmap_layout();
    }

    /// Drop logical ownership of any Full Mindmap preview work. The spawned
    /// timer/read cannot always be aborted, so its request identity remains
    /// the final stale-result guard in the message handlers.
    fn cancel_full_mindmap_preview(&mut self) {
        self.reset_full_mindmap_preview_window();
        self.quick_slot_preview_restore = None;
        if let Some(full) = self.full_mindmap.as_mut() {
            full.pending_preview_settle = None;
            full.pending_preview = None;
            full.preview = FullMindmapPreview::None;
        }
    }

    /// Full Mindmap-only file load. The wrapper retains request identity so a
    /// late result cannot close the navigator after a newer request supersedes
    /// it, and its error has a navigator-local home instead of `App::error`.
    fn begin_full_mindmap_open(&mut self, path: PathBuf) -> Task<Message> {
        self.cancel_refresh_tracking();
        if let Some(blocked) = self.block_file_open_if_dirty() {
            return blocked;
        }
        let path = canonicalize_existing_path(path);
        self.invalidate_pending_quick_slot_restore();
        if self.full_mindmap.is_none() {
            return self.load_file_unless_dirty(path);
        }
        // Deliberate Full Mindmap activation supersedes a deferred IPC open.
        self.pending_ipc_file_open = None;
        let pending_refresh = self
            .full_mindmap
            .as_ref()
            .and_then(|full| full.pending_workspace_load.clone())
            .filter(|request| request.preserve_navigation);
        if let Some(request) = pending_refresh {
            // A hidden-filter refresh and file read must not race: accepting
            // the file first would exit Full Mindmap and make the snapshot
            // completion stale. Supersede the spawned refresh with a new
            // request whose accepted completion starts the file read.
            if request.exit_after_refresh {
                // Esc/toggle/Return to Files already owns the terminal intent.
                // A queued activation must not turn that exit into a file open.
                let checkpoint = self.checkpoint_active_quick_slot();
                self.cancel_full_mindmap_preview();
                return checkpoint;
            }
            // Enter is deliberate activation even while a refresh is queued:
            // save the outgoing slot context, then cancel the preview.
            let checkpoint = self.checkpoint_active_quick_slot();
            self.cancel_full_mindmap_preview();
            let load = self.begin_full_mindmap_workspace_load(
                request.path,
                request.select_root,
                Some(path),
                true,
                request.return_to_files_after,
                false,
            );
            return Task::batch([checkpoint, load]);
        }
        // Enter is deliberate activation: cancel any read-only preview settle
        // or in-flight preview only after its outgoing slot context is saved.
        let checkpoint = self.checkpoint_active_quick_slot();
        self.cancel_full_mindmap_preview();
        // Full Mindmap file activation is app-owned navigation away from the
        // current preview. Preserve its outgoing Quick Slot context before
        // replacing the pending preview/open request.
        self.full_mindmap_request_seq = self.full_mindmap_request_seq.wrapping_add(1);
        let request = PendingFullMindmapOpen {
            id: self.full_mindmap_request_seq,
            path: path.clone(),
        };
        let full = self.full_mindmap.as_mut().expect("checked above");
        full.pending_open = Some(request.clone());
        full.load_error = None;
        Task::batch([
            checkpoint,
            Task::perform(load_file(path), move |result| {
                Message::FullMindmapFileLoaded { request, result }
            }),
        ])
    }

    /// Select a workspace node and independently settle a bounded, read-only
    /// preview when it is a file. Preview loads deliberately bypass the dirty
    /// guard because they never alter the current document.
    fn select_full_mindmap_node(&mut self, id: WorkspaceNodeId) -> Task<Message> {
        let checkpoint = self.checkpoint_active_quick_slot();
        self.invalidate_pending_quick_slot_restore();
        // A manual canvas selection supersedes any pending slot-specific
        // preview snap. Slot activation sets the snap directly and does not
        // route through this selection handler.
        self.quick_slot_preview_restore = None;
        let node = self
            .full_mindmap_graph()
            .and_then(|graph| graph.node(&id).cloned());
        let preview_path = node
            .as_ref()
            .and_then(|node| {
                matches!(&node.kind, WorkspaceNodeKind::File)
                    .then(|| node.path.clone())
                    .flatten()
            })
            .or_else(|| match &id {
                WorkspaceNodeId::File(path)
                    if self.workspace_files.contains(path)
                        && self
                            .workspace
                            .as_ref()
                            .is_some_and(|root| path.starts_with(root)) =>
                {
                    Some(path.clone())
                }
                _ => None,
            });
        let active_matches = self.quick_slots.active.is_some_and(|index| {
            let Some(path) = preview_path.as_ref() else {
                return false;
            };
            let Some(root) = self.quick_slots_workspace_root() else {
                return false;
            };
            let Some(relative) = crate::quick_slots::relative_path(root, path) else {
                return false;
            };
            self.quick_slots
                .occupied(index)
                .is_some_and(|slot| slot.relative_path == relative)
        });
        if self.quick_slots.active.is_some() && !active_matches {
            // Manual canvas navigation is no longer viewing the active slot's
            // file. Clear only the active marker; the bookmarked slot itself
            // remains intact for later activation.
            self.quick_slots.active = None;
            self.persist_quick_slots_now();
        }
        if let Some(full) = self.full_mindmap.as_mut() {
            full.deferred_file_selection = match &id {
                WorkspaceNodeId::File(path) if node.is_none() => Some(path.clone()),
                _ => None,
            };
            full.selected = Some(id.clone());
            full.focus_request = Some(id);
            // A background hidden-entry refresh describes the same workspace
            // and intentionally captures navigation at completion time. Keep
            // it alive across ordinary selection changes; project switches
            // and parent loads still retain their existing cancellation path.
            if full
                .pending_workspace_load
                .as_ref()
                .is_some_and(|request| !request.preserve_navigation)
            {
                full.pending_workspace_load = None;
            }
            full.load_error = None;
        }
        // `schedule_full_mindmap_preview` owns the preview reset boundary. Do
        // not reset here: same-path reselection must preserve a ready preview
        // and, more importantly, must not cancel its in-flight latest request
        // before the same-path ownership check runs.
        let preview = self.schedule_full_mindmap_preview(preview_path);
        Task::batch([
            checkpoint,
            preview,
            iced::widget::operation::scroll_to(
                Self::full_mindmap_preview_scroll_id(),
                iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: 0.0 },
            ),
        ])
    }

    /// Queue a short settle window for a file selected by keyboard or canvas.
    /// The preview remains in a neutral loading state while the user is still
    /// moving; only the accepted settle message starts the bounded filesystem
    /// read and parser. Every newer selection replaces this identity, so a
    /// timer that cannot be aborted is harmless when it eventually fires.
    fn schedule_full_mindmap_preview(&mut self, path: Option<PathBuf>) -> Task<Message> {
        let Some(path) = path else {
            self.cancel_full_mindmap_preview();
            return Task::none();
        };
        let Some(full) = self.full_mindmap.as_ref() else {
            return Task::none();
        };
        let already_owned = full
            .pending_preview_settle
            .as_ref()
            .is_some_and(|pending| pending.path == path)
            || full
                .pending_preview
                .as_ref()
                .is_some_and(|pending| pending.path == path);
        let already_ready = full.pending_preview.is_none()
            && matches!(
                &full.preview,
                FullMindmapPreview::Document { path: current, .. }
                    | FullMindmapPreview::Data { path: current, .. }
                    if current == &path
            );
        if already_owned {
            return Task::none();
        }
        if already_ready {
            // Re-selection must not reread/reparse a retained preview, but a
            // soft cache trim may have evicted one of its remote assets. The
            // preview identity remains stable while this lightweight pass
            // reclaims only missing image/diagram work.
            return self.prime_full_mindmap_preview_assets();
        }

        // Selection changed: reset only preview-owned virtualization and
        // measurement state before the new settle/read request takes over.
        self.reset_full_mindmap_preview_window();
        self.full_mindmap_request_seq = self.full_mindmap_request_seq.wrapping_add(1);
        let request = PendingFullMindmapPreviewSettle {
            id: self.full_mindmap_request_seq,
            path: path.clone(),
        };
        // Subscribe before publishing the request. The persistent worker is
        // normally started lazily on the first file selection; publishing to
        // the watch channel first would let a newly-created receiver start at
        // the already-published version and miss the settle wake-up if the
        // stream waits for `changed()` before inspecting its current value.
        let settle_worker = self.start_full_mindmap_preview_settle_worker();
        let full = self.full_mindmap.as_mut().expect("checked above");
        // A different selection supersedes both an older timer and an older
        // read. The old future may still complete, but its request no longer
        // matches `pending_preview` and is ignored.
        full.pending_preview_settle = Some(request.clone());
        full.pending_preview = None;
        full.preview = FullMindmapPreview::Loading(path);
        let _ = full.preview_settle_tx.send(Some(request));
        settle_worker
    }

    /// Start the one persistent latest-only debounce worker for this navigator
    /// instance. Every subsequent selection replaces the watch value; stale
    /// sleeps are abandoned by the worker rather than accumulating one task per
    /// key repeat.
    fn start_full_mindmap_preview_settle_worker(&mut self) -> Task<Message> {
        let Some(full) = self.full_mindmap.as_mut() else {
            return Task::none();
        };
        if full.preview_settle_worker_started {
            return Task::none();
        }
        full.preview_settle_worker_started = true;
        let receiver = full.preview_settle_tx.subscribe();
        // Keep one stream alive for the navigator lifetime. A one-shot task
        // can resolve its settle message before update() handles it; if a
        // newer selection resets the watch in that gap, the next request
        // would otherwise send into a receiver with no live worker and stay
        // on `Loading preview…` forever.
        Task::run(full_mindmap_preview_settle_stream(receiver), |request| {
            Message::FullMindmapPreviewSettle { request }
        })
    }

    fn begin_full_mindmap_preview(&mut self, path: Option<PathBuf>) -> Task<Message> {
        let Some(path) = path else {
            self.reset_full_mindmap_preview_window();
            if let Some(full) = self.full_mindmap.as_mut() {
                full.pending_preview_settle = None;
                full.pending_preview = None;
                full.preview = FullMindmapPreview::None;
            }
            return Task::none();
        };
        let already_ready = self.full_mindmap.as_ref().is_some_and(|full| {
            full.pending_preview
                .as_ref()
                .is_some_and(|pending| pending.path == path)
                || (full.pending_preview.is_none()
                    && matches!(
                    &full.preview,
                    FullMindmapPreview::Document { path: current, .. }
                        | FullMindmapPreview::Data { path: current, .. }
                        if current == &path
                    ))
        });
        if already_ready {
            return Task::none();
        }
        if self.full_mindmap.is_none() {
            return Task::none();
        }
        self.full_mindmap_request_seq = self.full_mindmap_request_seq.wrapping_add(1);
        let request = PendingFullMindmapPreview {
            id: self.full_mindmap_request_seq,
            path: path.clone(),
        };
        let (preview_work_epoch, preview_work_cancel) = self
            .full_mindmap
            .as_ref()
            .map(|full| {
                (
                    full.preview_work_epoch.load(Ordering::Acquire),
                    Arc::clone(&full.preview_work_epoch),
                )
            })
            .expect("checked above");
        let full = self.full_mindmap.as_mut().expect("checked above");
        // This direct path bypasses the settle timer; clear the watch value
        // as well as the compatibility field so an old worker cannot emit its
        // request again after this accepted read starts.
        let _ = full.preview_settle_tx.send(None);
        full.pending_preview_settle = None;
        full.pending_preview = Some(request.clone());
        full.preview_identity = Some(request.clone());
        full.preview_measurement_generation = None;
        full.preview = FullMindmapPreview::Loading(path.clone());
        Task::perform(
            load_full_mindmap_preview_guarded(path, preview_work_cancel, preview_work_epoch),
            move |result| Message::FullMindmapPreviewLoaded { request, result },
        )
    }

    /// Start parsing an already-open document for the initial Full Mindmap
    /// preview without doing parser/highlighter work on the update thread.
    /// Unlike selection-driven previews this intentionally skips the 300 ms
    /// settle because the document is already the accepted current file.
    fn begin_full_mindmap_preview_source(
        &mut self,
        path: PathBuf,
        source: Arc<str>,
    ) -> Task<Message> {
        if self.full_mindmap.is_none() {
            return Task::none();
        }
        let keep_quick_slot_restore =
            self.quick_slot_preview_restore_guard
                .as_ref()
                .is_some_and(|guard| {
                    self.quick_slot_restore_is_current(guard)
                        && self
                            .quick_slots_workspace_root()
                            .and_then(|root| {
                                crate::quick_slots::resolve_path(root, &guard.slot.relative_path)
                            })
                            .is_some_and(|target| target == path)
                });
        if !keep_quick_slot_restore {
            // Source reparses for a different/stale identity must invalidate
            // the position and its guard together. A current slot restore for
            // this same file survives until the accepted parse can consume it.
            self.quick_slot_preview_restore = None;
            self.quick_slot_preview_restore_guard = None;
        }
        // A source-backed reparse is a new accepted preview even when the
        // path is unchanged. Clear old measurement ownership/window shape so
        // pending g1 results cannot poison the new parse's geometry.
        self.reset_full_mindmap_preview_window();
        let full = self.full_mindmap.as_mut().expect("checked above");
        self.full_mindmap_request_seq = self.full_mindmap_request_seq.wrapping_add(1);
        let request = PendingFullMindmapPreview {
            id: self.full_mindmap_request_seq,
            path: path.clone(),
        };
        let epoch = full.preview_work_epoch.load(Ordering::Acquire);
        let cancel = Arc::clone(&full.preview_work_epoch);
        let parse_gate = Arc::clone(&full.preview_parse_gate);
        // Source-backed previews also bypass debounce. Explicitly cancel any
        // prior watch value before assigning this accepted identity.
        let _ = full.preview_settle_tx.send(None);
        full.pending_preview_settle = None;
        full.pending_preview = Some(request.clone());
        full.preview_identity = Some(request.clone());
        full.preview_measurement_generation = None;
        full.preview = FullMindmapPreview::Loading(path);
        Task::perform(
            parse_full_mindmap_preview_guarded(
                request.path.clone(),
                source,
                cancel,
                epoch,
                parse_gate,
            ),
            move |result| Message::FullMindmapPreviewParsed { request, result },
        )
    }

    /// Drop only Full Mindmap preview layout/scroll ownership. The document
    /// reader's `virt_window`, viewport, and height cache are intentionally
    /// untouched. A generation bump makes already-dispatched measurement
    /// operations harmless when selection changes.
    fn reset_full_mindmap_preview_window(&mut self) {
        let stale_loading = self
            .full_mindmap
            .as_mut()
            .map(|full| full.preview_loading_images.drain().collect::<Vec<_>>())
            .unwrap_or_default();
        let stale_diagrams = self
            .full_mindmap
            .as_ref()
            .map(|full| {
                full.preview_pending_diagrams
                    .keys()
                    .copied()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for (url, _owner) in stale_loading {
            if matches!(self.image_cache.get(&url), Some(ImageState::Loading)) {
                self.image_cache.remove(&url);
            }
        }
        for key in stale_diagrams {
            if matches!(
                self.diagram_cache.peek(&key),
                Some(crate::diagram::DiagramState::Pending)
            ) {
                self.diagram_cache.remove(&key);
            }
        }
        if let Some(full) = self.full_mindmap.as_mut() {
            let _ = full.preview_settle_tx.send(None);
            full.preview_generation = full.preview_generation.wrapping_add(1);
            full.preview_window = crate::virt::VirtWindow::default();
            full.preview_viewport = None;
            full.preview_height_cache.clear();
            full.preview_measurement_pending = false;
            full.preview_measurement_range = None;
            full.preview_measurement_generation = None;
            full.preview_identity = None;
            full.preview_shape_ready = false;
            full.preview_work_epoch.fetch_add(1, Ordering::AcqRel);
            full.preview_pending_diagrams.clear();
            full.preview_asset_images.clear();
            full.preview_failed_images.clear();
            full.preview_asset_diagrams.clear();
            full.preview_asset_cursor = 0;
            full.preview_asset_cursor_tag = None;
            full.preview_asset_wave_id = full.preview_asset_wave_id.wrapping_add(1);
        }
    }

    fn full_mindmap_preview_body_offset(full: &FullMindmapState) -> f32 {
        full.preview_viewport
            .as_ref()
            .map(|viewport| viewport.absolute_offset().y.max(0.0))
            .unwrap_or(0.0)
    }

    /// Rebuild the preview's shared virtual window from its own viewport and
    /// measured-height cache. This mirrors `rebuild_virt_here` but never reads
    /// or mutates document state.
    fn rebuild_full_mindmap_preview_here(&mut self) {
        let Some(full) = self.full_mindmap.as_mut() else {
            return;
        };
        let offset = Self::full_mindmap_preview_body_offset(full);
        let viewport_h = full
            .preview_viewport
            .as_ref()
            .map(|viewport| viewport.bounds().height)
            .or(self.window_size.map(|size| size.height))
            .unwrap_or(1000.0);
        if matches!(full.preview, FullMindmapPreview::Document { .. }) {
            if !full.preview_shape_ready {
                // Move the worker-built shape out of the enum so the normal
                // runtime handoff is O(1); the Arc is uniquely owned here.
                let shape = match &mut full.preview {
                    FullMindmapPreview::Document { shape, .. } => shape.take(),
                    _ => None,
                };
                if let Some(shape) = shape {
                    full.preview_window
                        .install_shape_arc(shape, offset, viewport_h);
                } else if let FullMindmapPreview::Document { blocks, .. } = &full.preview {
                    // Small in-memory fixtures and older callers may not carry
                    // worker geometry. Runtime complete previews always do.
                    full.preview_window.rebuild(
                        blocks,
                        &HashSet::new(),
                        &full.preview_height_cache,
                        offset,
                        viewport_h,
                    );
                }
                full.preview_shape_ready = true;
            } else {
                full.preview_window
                    .rebuild_at_current_shape_for_preview(offset, viewport_h);
            }
        } else {
            full.preview_window = crate::virt::VirtWindow::default();
            full.preview_shape_ready = false;
        }
    }

    fn refresh_full_mindmap_preview_heights(&mut self) -> Task<Message> {
        if let Some(full) = self.full_mindmap.as_mut() {
            full.preview_generation = full.preview_generation.wrapping_add(1);
            full.preview_height_cache.clear();
            full.preview_window.clear_height_adjustments();
            if !full.preview_measurement_pending {
                full.preview_measurement_range = None;
                full.preview_measurement_generation = None;
            }
        }
        self.rebuild_full_mindmap_preview_here();
        Task::batch([
            self.prime_full_mindmap_preview_assets(),
            self.measure_full_mindmap_preview_heights(),
        ])
    }

    /// Dispatch a widget operation that measures only the materialized
    /// preview blocks. The path and generation travel with the result so a
    /// late operation cannot feed the document cache or a newer selection.
    fn measure_full_mindmap_preview_heights(&mut self) -> Task<Message> {
        let Some(full) = self.full_mindmap.as_mut() else {
            return Task::none();
        };
        let FullMindmapPreview::Document { path, blocks, .. } = &full.preview else {
            return Task::none();
        };
        if !full.preview_window.active || full.preview_measurement_pending {
            return Task::none();
        }
        let (start, end) = full.preview_window.range;
        let targets: HashMap<iced::widget::Id, crate::ast::BlockId> = full.preview_window.display
            [start.min(full.preview_window.display.len())
                ..end.min(full.preview_window.display.len())]
            .iter()
            .filter_map(|&idx| blocks.get(idx).map(|(id, _)| *id))
            .filter(|id| !full.preview_height_cache.is_measured(*id))
            .map(|id| (crate::render::block_anchor_id(id), id))
            .collect();
        if targets.is_empty() {
            return Task::none();
        }
        full.preview_measurement_pending = true;
        full.preview_measurement_range = Some(full.preview_window.range);
        full.preview_measurement_generation = Some(full.preview_generation);
        measure_full_mindmap_preview_block_heights(
            path.clone(),
            full.preview_namespace,
            full.preview_identity
                .as_ref()
                .map(|request| request.id)
                .unwrap_or(0),
            full.preview_generation,
            targets,
            Self::full_mindmap_preview_body_offset(full),
        )
    }

    fn show_toast(&mut self, text: String) -> Task<Message> {
        self.show_toast_with_action(text, None)
    }

    fn show_cli_install_prompt(&mut self) -> Task<Message> {
        self.show_toast_with_action(
            "Install the rmdv CLI to use rmdv from Terminal".to_string(),
            Some(ToastAction {
                label: "Install CLI".to_string(),
                message: Message::InstallCli,
            }),
        )
    }

    fn show_toast_with_action(
        &mut self,
        text: String,
        action: Option<ToastAction>,
    ) -> Task<Message> {
        self.toast_seq = self.toast_seq.wrapping_add(1);
        let id = self.toast_seq;
        let duration = if action.is_some() {
            std::time::Duration::from_secs(8)
        } else {
            std::time::Duration::from_millis(1500)
        };
        self.toast = Some(Toast { id, text, action });
        Task::perform(
            async move { tokio::time::sleep(duration).await },
            move |_| Message::ToastExpire(id),
        )
    }

    fn scroll_id() -> iced::widget::Id {
        iced::widget::Id::new("body")
    }
    fn tree_scroll_id() -> iced::widget::Id {
        iced::widget::Id::new("tree")
    }
    fn outline_scroll_id() -> iced::widget::Id {
        iced::widget::Id::new("outline")
    }
    fn overlay_scroll_id() -> iced::widget::Id {
        iced::widget::Id::new("overlay")
    }
    /// Stable identity for the Full Mindmap read-only preview. This is kept
    /// distinct from the document body scrollable so selecting a file never
    /// reuses or mutates document scroll state.
    fn full_mindmap_preview_scroll_id() -> iced::widget::Id {
        iced::widget::Id::new("full-mindmap-preview")
    }
    fn search_input_id() -> iced::widget::Id {
        iced::widget::Id::new("search-input")
    }
    fn overlay_input_id() -> iced::widget::Id {
        iced::widget::Id::new("overlay-input")
    }
    fn vault_input_id() -> iced::widget::Id {
        iced::widget::Id::new("vault-input")
    }
    fn vault_scroll_id() -> iced::widget::Id {
        iced::widget::Id::new("vault")
    }
    /// Stable id for the n-th visible match block, used to scroll to the cursor
    /// by measured bounds (blocks vary in height, so estimation can't track it).
    fn vault_match_anchor_id(vis_idx: usize) -> iced::widget::Id {
        iced::widget::Id::from(format!("vault-match-{vis_idx}"))
    }

    /// Indices into `vault_results` for matches whose file group is expanded.
    /// The page cursor and `↑↓` nav operate over this list.
    fn vault_visible_matches(&self) -> Vec<usize> {
        self.vault_results
            .iter()
            .enumerate()
            .filter(|(_, h)| !self.vault_collapsed.contains(&h.path))
            .map(|(i, _)| i)
            .collect()
    }

    /// Scroll the results page so the cursor's match block is visible. Blocks
    /// vary in height (variable context lines, file headers, wrapped lines), so
    /// estimation can't track them — instead measure the block's real laid-out
    /// bounds by id and scroll just enough to bring it on screen.
    fn scroll_vault_to_cursor(&self) -> Task<Message> {
        let visible = self.vault_visible_matches();
        if visible.is_empty() {
            return Task::none();
        }
        scroll_vault_to_match(self.vault_cursor)
    }

    /// Edge-scroll the sidebar tree to the cursor. Takes the flattened row
    /// count from the caller (`TreeMove` already flattens for clamping) so the
    /// tree isn't flattened twice per keystroke.
    fn scroll_tree_to_cursor_with_len(&self, total: usize) -> Task<Message> {
        const ROW_H: f32 = 26.0;
        if total == 0 {
            return Task::none();
        }
        edge_scroll(
            Self::tree_scroll_id(),
            self.tree_viewport.as_ref(),
            self.tree_cursor,
            total,
            ROW_H,
        )
    }

    fn scroll_outline_to_cursor(&self) -> Task<Message> {
        // Row height matches `outline_row`'s fixed height.
        const ROW_H: f32 = 26.0;
        let total = self.outline_sections.len();
        if total == 0 {
            return Task::none();
        }
        edge_scroll(
            Self::outline_scroll_id(),
            self.outline_viewport.as_ref(),
            self.outline_cursor,
            total,
            ROW_H,
        )
    }

    fn scroll_overlay_to_cursor(&self) -> Task<Message> {
        let len = match self.overlay {
            Overlay::FileFinder => self.filtered_files().len(),
            Overlay::Command => self.filtered_commands().len(),
            Overlay::ThemePicker => self.filtered_themes().len(),
            Overlay::FolderPicker => self.picker.as_ref().map(|p| p.entries.len()).unwrap_or(0),
            Overlay::None | Overlay::ImageZoom | Overlay::Shortcuts => 0,
        };
        self.scroll_overlay_to_cursor_with_len(len)
    }

    /// `scroll_overlay_to_cursor` for callers that already computed the
    /// filtered list length this update (`OverlayMove` does, every arrow key).
    fn scroll_overlay_to_cursor_with_len(&self, len: usize) -> Task<Message> {
        let (total, row_h) = match self.overlay {
            // FileFinder renders at most 80 rows; scroll math matches.
            Overlay::FileFinder => (len.min(80), 32.0),
            Overlay::Command | Overlay::ThemePicker => (len, 32.0),
            Overlay::FolderPicker => (len, 33.0),
            Overlay::None | Overlay::ImageZoom | Overlay::Shortcuts => (0, 32.0),
        };
        if total == 0 {
            return Task::none();
        }
        edge_scroll(
            Self::overlay_scroll_id(),
            self.overlay_viewport.as_ref(),
            self.overlay_selected,
            total,
            row_h,
        )
    }

    pub fn new(initial: Option<PathBuf>) -> (Self, Task<Message>) {
        // Iced 0.14 does not forward macOS `PinchGesture` events. Register a
        // tiny process-local bridge while we're still on the app's main thread.
        crate::native_pinch::install();
        // Finder delivers document opens through AppKit rather than argv.
        crate::macos_open::install();
        let mut app = Self::default();
        let mut errs = Vec::new();
        let mut combined = crate::theme_load::bundled().clone();
        combined.extend(crate::theme_load::discover(&mut errs));
        combined.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        app.custom_themes = combined;
        if !errs.is_empty() && app.error.is_none() {
            app.error = Some(format!("theme load: {}", errs.join("; ")));
        }
        let task = match initial {
            Some(p) => {
                if p.is_dir() {
                    Task::done(Message::OpenWorkspace(p))
                } else {
                    app.begin_generic_file_load(p)
                }
            }
            None => Task::none(),
        };
        // Background update check on launch. A failed/absent manifest is a
        // silent no-op (maps to DismissUpdate, which clears nothing).
        let update_check = Task::perform(crate::update::check_and_download(), |res| match res {
            Ok(Some(ready)) => Message::UpdateAvailable(ready),
            _ => Message::DismissUpdate,
        });
        let cli_prompt =
            if crate::cli_install::should_offer() && !crate::cli_install::is_installed() {
                app.show_cli_install_prompt()
            } else {
                Task::none()
            };
        (app, Task::batch([task, update_check, cli_prompt]))
    }

    /// Returns the next theme in cycle order: all built-in presets followed
    /// by every loaded custom theme, then wraps. Tuple = (id, display label,
    /// palette, optional typography override).
    fn next_theme(
        &self,
    ) -> (
        theme::ThemeId,
        String,
        theme::Palette,
        Option<theme::Typography>,
    ) {
        let mut cycle: Vec<(
            theme::ThemeId,
            String,
            theme::Palette,
            Option<theme::Typography>,
        )> = theme::ThemePreset::ALL
            .iter()
            .map(|p| {
                (
                    theme::ThemeId::Preset(*p),
                    p.label().to_string(),
                    theme::palette_for(*p),
                    None,
                )
            })
            .collect();
        for t in &self.custom_themes {
            cycle.push((
                theme::ThemeId::Custom(t.slug.clone()),
                t.name.clone(),
                t.palette,
                Some(t.typography),
            ));
        }
        let idx = cycle
            .iter()
            .position(|(id, _, _, _)| id == &self.theme_id)
            .unwrap_or(usize::MAX);
        let next = if idx == usize::MAX {
            0
        } else {
            (idx + 1) % cycle.len()
        };
        cycle.swap_remove(next)
    }

    pub fn is_dark(&self) -> bool {
        match &self.theme_id {
            crate::theme::ThemeId::Preset(p) => p.is_dark(),
            crate::theme::ThemeId::Custom(slug) => self
                .custom_themes
                .iter()
                .find(|t| &t.slug == slug)
                .map(|t| t.dark)
                .unwrap_or_else(|| self.theme_preset.is_dark()),
        }
    }

    pub fn title(&self) -> String {
        match &self.file {
            Some(p) => format!(
                "rmdv — {}",
                p.file_name().and_then(|n| n.to_str()).unwrap_or("?")
            ),
            None => "rmdv".into(),
        }
    }

    pub fn theme(&self) -> Theme {
        if self.is_dark() {
            Theme::Dark
        } else {
            Theme::Light
        }
    }

    /// Re-snap the body scrollable to its last known offset. Iced 0.14 keys
    /// scrollable widget state by tree position, so wrapping/unwrapping the
    /// reader (search bar toggle, sidebar toggle) reinitialises the state and
    /// snaps to the top. Call this after any toggle that changes the reader's
    /// place in the tree.
    fn restore_body_scroll(&self) -> Task<Message> {
        let Some(v) = self.body_viewport.as_ref() else {
            return Task::none();
        };
        let content_h = v.content_bounds().height;
        let view_h = v.bounds().height;
        if content_h <= view_h {
            return Task::none();
        }
        let rel = v.absolute_offset().y / (content_h - view_h);
        Task::done(Message::RestoreBodySnap(rel.clamp(0.0, 1.0)))
    }

    /// Body-relative scroll offset (px past the top of the rendered column).
    fn body_offset(&self) -> f32 {
        self.body_viewport
            .as_ref()
            .map(|v| (v.absolute_offset().y - BODY_TOP_PAD).max(0.0))
            .unwrap_or(0.0)
    }

    /// Body viewport height, falling back to the window height before the
    /// first scroll event has reported real bounds.
    fn body_viewport_h(&self) -> f32 {
        self.body_viewport
            .as_ref()
            .map(|v| v.bounds().height)
            .or(self.window_size.map(|s| s.height))
            .unwrap_or(1000.0)
    }

    /// Rebuild the virt window around the current scroll position.
    fn rebuild_virt_here(&mut self) {
        let offset = self.body_offset();
        let vh = self.body_viewport_h();
        self.virt_window
            .rebuild(&self.ast, &self.folded, &self.height_cache, offset, vh);
    }

    /// Rebuild the virt window centered on an AST block (goto/search jumps),
    /// so the target is materialized before a precise scroll operation runs.
    fn rebuild_virt_around_block(&mut self, ast_idx: usize) {
        let vh = self.body_viewport_h();
        self.virt_window
            .rebuild_around(&self.ast, &self.folded, &self.height_cache, ast_idx, vh);
    }

    /// Widget operation harvesting real laid-out heights for the windowed
    /// blocks. Dispatch after window rebuilds (NOT from the measurement
    /// handler itself — that would loop).
    fn measure_window_heights(&self) -> Task<Message> {
        if !self.virt_window.active {
            return Task::none();
        }
        let (s, e) = self.virt_window.range;
        let targets: std::collections::HashMap<iced::widget::Id, crate::ast::BlockId> = self
            .virt_window
            .display[s.min(self.virt_window.display.len())..e.min(self.virt_window.display.len())]
            .iter()
            .filter_map(|&i| self.ast.get(i).map(|(id, _)| *id))
            .map(|id| (crate::render::block_anchor_id(id), id))
            .collect();
        measure_block_heights(targets, self.body_offset())
    }

    fn scroll_to_current_match(&mut self) -> Task<Message> {
        let Some(m) = self.matches.get(self.match_idx) else {
            return Task::none();
        };
        let block_idx = m.block;
        let Some((id, _)) = self.ast.get(block_idx) else {
            return Task::none();
        };
        let id = *id;
        // The match may sit under a folded heading, whose block container is
        // then absent from the widget tree — the scroll Operation would find
        // nothing and silently no-op. Reveal it first.
        self.unfold_to_reveal(block_idx);
        // Materialize the target before the scroll operation traverses the
        // tree — an off-window block has no widget for the op to find. No
        // measure pass here: it would compute scroll-anchoring against the
        // pre-jump offset and fight the landing; the post-landing BodyScrolled
        // band-exit measures instead.
        self.rebuild_virt_around_block(block_idx);
        self.nav_anchor = Some(block_idx);
        // Use real laid-out widget bounds via the scroll operation rather than
        // height estimates, which diverge from the actual layout (code blocks,
        // images, diagrams, math) and left the match offscreen.
        scroll_block_to_center(id)
    }

    /// Remove fold state on every heading whose collapsed range hides the block
    /// at `block_idx`, so a search/nav target under (possibly nested) folds is
    /// actually rendered. Mirrors the fold logic in `render::render`: a folded
    /// heading hides following blocks until a heading of level ≤ its own.
    fn unfold_to_reveal(&mut self, block_idx: usize) {
        if self.folded.is_empty() || block_idx >= self.ast.len() {
            return;
        }
        // Stack of (heading_level, heading_id, is_folded) enclosing block_idx.
        let mut ancestors: Vec<(u8, crate::ast::BlockId, bool)> = Vec::new();
        for (i, (id, b)) in self.ast.iter().enumerate() {
            if i == block_idx {
                break;
            }
            if let Block::Heading { level, .. } = b {
                let lvl = *level as u8;
                while ancestors.last().is_some_and(|(l, _, _)| *l >= lvl) {
                    ancestors.pop();
                }
                ancestors.push((lvl, *id, self.folded.contains(id)));
            }
        }
        // Any folded heading on the ancestor path hides block_idx; reveal them.
        for (_, id, folded) in ancestors {
            if folded {
                self.folded.remove(&id);
            }
        }
    }

    fn scroll_to_line_top(&mut self, line: u32) -> Task<Message> {
        let Some(idx) = crate::ipc::lines::block_for_line(line, &self.block_lines) else {
            return Task::none();
        };
        let Some((id, Block::Heading { .. })) = self.ast.get(idx) else {
            return Task::none();
        };
        let id = *id;
        // The scroll Operation walks the body scrollable, which only exists in
        // Rendered view — outline/fragment nav fired from Raw or Mindmap would
        // otherwise no-op. Leave Zen through the normal cleanup path so editor
        // text and chrome state do not remain stranded.
        if self.view_mode == ViewMode::Raw {
            self.leave_zen_edit_mode(true);
        } else {
            self.view_mode = ViewMode::Rendered;
        }
        self.unfold_to_reveal(idx);
        self.rebuild_virt_around_block(idx);
        self.nav_anchor = Some(idx);
        scroll_block_to_top(id)
    }

    fn synthesize_data_ast(&mut self) -> Option<Vec<(crate::ast::BlockId, Block)>> {
        let lang = data_lang_for(self.file.as_deref())?;
        let code = prettify_data(lang, &self.source);
        let spans = self.hl_cache.highlight(lang, &code);
        let block = Block::CodeBlock {
            lang: Some(lang.to_string()),
            code,
            spans,
        };
        Some(vec![(crate::ast::BlockId(0), block)])
    }

    /// Cached `mindmap::build_layout` result, rebuilt on first read after an
    /// invalidation. Pure function of (ast, file, mindmap_collapsed); see the
    /// field doc on `mindmap_layout` for the invalidation contract.
    fn mindmap_layout(
        &self,
    ) -> (
        std::sync::Arc<Vec<crate::mindmap::MNode>>,
        iced::Size,
        std::sync::Arc<
            std::collections::HashMap<crate::ast::BlockId, Vec<crate::data_mindmap::PathSeg>>,
        >,
    ) {
        let mut cache = self.mindmap_layout.borrow_mut();
        if cache.is_none() {
            let (nodes, size, paths) = if self.is_data_doc {
                let lang = data_lang_for(self.file.as_deref()).unwrap_or("json");
                crate::data_mindmap::build_layout(
                    &self.source,
                    lang,
                    self.file.as_deref(),
                    &self.mindmap_collapsed,
                )
            } else {
                let (nodes, size) = crate::mindmap::build_layout(
                    &self.ast,
                    self.file.as_deref(),
                    &self.mindmap_collapsed,
                );
                (nodes, size, std::collections::HashMap::new())
            };
            *cache = Some((std::sync::Arc::new(nodes), size, std::sync::Arc::new(paths)));
        }
        let (nodes, size, paths) = cache.as_ref().unwrap();
        (
            std::sync::Arc::clone(nodes),
            *size,
            std::sync::Arc::clone(paths),
        )
    }

    fn invalidate_mindmap_layout(&self) {
        *self.mindmap_layout.borrow_mut() = None;
        *self.mindmap_data_panel.borrow_mut() = None;
        self.mindmap_layout_generation
            .set(self.mindmap_layout_generation.get().wrapping_add(1));
    }

    fn replace_mindmap_layout(
        &self,
        nodes: Vec<crate::mindmap::MNode>,
        size: iced::Size,
        paths: std::collections::HashMap<crate::ast::BlockId, Vec<crate::data_mindmap::PathSeg>>,
    ) -> std::sync::Arc<Vec<crate::mindmap::MNode>> {
        let nodes = std::sync::Arc::new(nodes);
        *self.mindmap_layout.borrow_mut() = Some((
            std::sync::Arc::clone(&nodes),
            size,
            std::sync::Arc::new(paths),
        ));
        *self.mindmap_data_panel.borrow_mut() = None;
        self.mindmap_layout_generation
            .set(self.mindmap_layout_generation.get().wrapping_add(1));
        nodes
    }

    /// Select root's first child if nothing is selected, opening the preview
    /// panel. Called on mindmap toggle-on and on file load while in mindmap
    /// mode, so a freshly opened document focuses its first heading.
    fn mindmap_focus_first_child(&mut self) {
        if self.view_mode != ViewMode::Mindmap || self.mindmap_selected.is_some() {
            return;
        }
        let (nodes, _, _) = self.mindmap_layout();
        if let Some(id) = nodes
            .first()
            .and_then(|root| root.children.first().copied())
            .and_then(|idx| nodes[idx].id)
        {
            self.mindmap_selected = Some(id);
            self.mindmap_panel_shown = Some(id);
            self.mindmap_panel_open = true;
        }
    }

    fn reparse_source(&mut self) {
        self.load_ast_from_source();
        self.rebuild_matches();
    }

    /// Parse `self.source` into `self.ast` (+ `block_lines`), dispatching by file
    /// type: structured-data files (json/yaml/toml) synthesize a single code
    /// block, `.tex` goes through the LaTeX parser, everything else is markdown.
    /// Shared by `reparse_source` (post-edit) and the `FileLoaded` handler so a
    /// `.tex` file can't render correctly on load then revert to markdown on edit.
    fn load_ast_from_source(&mut self) {
        // Covers every `self.ast` write below; `self.file` and
        // `mindmap_collapsed` writes in FileLoaded happen before this call.
        self.invalidate_mindmap_layout();
        self.source_words = self.source.split_whitespace().count();
        if let Some(ast) = self.synthesize_data_ast() {
            self.ast = ast;
            // Data docs are one synthesized block at line 1; reset block_lines
            // and the outline so a stale map from a prior file can't misroute
            // line-nav.
            self.block_lines = vec![1];
            self.outline_sections.clear();
            self.rebuild_virt_here();
            return;
        }
        let is_tex = is_tex_path(self.file.as_deref());
        let (mut parsed, block_offsets) = if is_tex {
            crate::tex::parse(&self.source)
        } else {
            parser::parse(&self.source)
        };
        for (_id, b) in parsed.iter_mut() {
            if let Block::CodeBlock {
                lang: Some(l),
                code,
                spans,
            } = b
            {
                if spans.is_empty() {
                    *spans = self.hl_cache.highlight(l, code);
                }
            }
        }
        let table = crate::ipc::lines::build_byte_to_line(&self.source);
        self.block_lines = block_offsets
            .iter()
            .map(|&b| table.line_for_byte(b as usize))
            .collect();
        self.ast = parsed;
        // Reuse the parse + byte-to-line table from above instead of letting
        // list_sections_for re-run both on the same source. Valid only while
        // the span-fill loop above never adds/removes blocks:
        debug_assert!(self.ast.len() == block_offsets.len());
        self.outline_sections =
            crate::ipc::sections::list_sections_from_ast(&self.ast, &block_offsets, &table);
        // New AST → new display list/prefix sums. BlockIds are content-hashed,
        // so measured heights survive for unchanged blocks across reparses.
        self.rebuild_virt_here();
    }

    /// Evict oldest fetched images once the cache exceeds its byte budget.
    /// Images referenced by the current document, visible Full Mindmap preview
    /// wave, or open zoom modal are never evicted, so what's on screen never
    /// changes.
    fn trim_image_cache(&mut self) {
        if self.image_cache.cost_bytes() <= IMAGE_CACHE_BYTE_BUDGET {
            return;
        }
        let mut keep: HashSet<String> = self
            .ast
            .iter()
            .filter_map(|(_, b)| match b {
                Block::Image { url, .. } => Some(url.clone()),
                _ => None,
            })
            .chain(self.zoom_url.clone())
            .collect();
        if let Some(full) = self.full_mindmap.as_ref() {
            // Keep only assets from the current materialized preview wave and
            // in-flight ownership. The worker index may cover thousands of
            // off-screen URLs; retaining all of them would defeat the cache
            // byte budget and recreate the original memory spike.
            keep.extend(full.preview_asset_images.iter().cloned());
            keep.extend(full.preview_loading_images.keys().cloned());
        }
        self.image_cache
            .trim(IMAGE_CACHE_BYTE_BUDGET, |k| keep.contains(k));
    }

    /// Walk the current AST and dispatch a background render for every
    /// `Block::Diagram` whose `(hash, theme_id)` is not yet in the cache.
    /// Inserts `Pending` placeholders so the render path doesn't re-dispatch
    /// the same hash on every redraw. Returns a `Task::batch` of in-flight
    /// render futures.
    fn prime_diagram_cache(&mut self) -> Task<Message> {
        let theme_id = self.diagram_theme_id;
        let palette = self.palette;
        // Editor font carries through to mermaid/dot output for visual parity.
        let font_family = "JetBrains Mono".to_string();
        // Dedupe by hash so duplicate diagram blocks share a single task.
        let mut seen: std::collections::HashSet<u64> = std::collections::HashSet::new();
        let mut tasks: Vec<Task<Message>> = Vec::new();
        let mut pending_inserts: Vec<(u64, crate::ast::DiagramKind, String)> = Vec::new();
        // Diagram/math blocks can be nested inside list items, blockquotes and
        // table cells, so walk the tree rather than just the top level.
        fn collect_diagrams<'a>(
            b: &'a Block,
            out: &mut Vec<(u64, crate::ast::DiagramKind, String)>,
        ) {
            match b {
                Block::Diagram { hash, kind, source } => {
                    out.push((*hash, kind.clone(), source.clone()))
                }
                Block::Blockquote(blocks) => {
                    for inner in blocks {
                        collect_diagrams(inner, out);
                    }
                }
                Block::List { items, .. } => {
                    for item in items {
                        for inner in &item.blocks {
                            collect_diagrams(inner, out);
                        }
                    }
                }
                _ => {}
            }
        }
        let mut found: Vec<(u64, crate::ast::DiagramKind, String)> = Vec::new();
        for (_id, b) in &self.ast {
            collect_diagrams(b, &mut found);
        }
        for (hash, kind, source) in found {
            if !seen.insert(hash) {
                continue;
            }
            if self.diagram_cache.peek(&(hash, theme_id)).is_some() {
                continue;
            }
            pending_inserts.push((hash, kind, source));
        }
        for (hash, kind, source) in pending_inserts {
            self.diagram_cache
                .put((hash, theme_id), crate::diagram::DiagramState::Pending);
            let ff = font_family.clone();
            tasks.push(Task::perform(
                crate::diagram::render_blocking_async(kind, source, palette, ff),
                move |result| Message::DiagramRendered {
                    hash,
                    theme_id,
                    result,
                },
            ));
        }
        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }

    /// Drop Loading/Pending ownership from an older virtual range before a
    /// new visible wave primes. The old futures may still complete, but their
    /// range-tagged messages can no longer remove the new wave's sentinels.
    /// This also prevents hung off-screen image requests from consuming the
    /// current wave's 64-operation cap forever.
    fn reconcile_full_mindmap_preview_asset_wave(
        &mut self,
        asset_identity: &FullMindmapPreviewAssetIdentity,
    ) {
        let stale_images = self
            .full_mindmap
            .as_ref()
            .map(|full| {
                full.preview_loading_images
                    .iter()
                    .filter(|(_, owner)| *owner != asset_identity)
                    .map(|(url, _)| url.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let stale_diagrams = self
            .full_mindmap
            .as_ref()
            .map(|full| {
                full.preview_pending_diagrams
                    .iter()
                    .filter(|(_, owner)| *owner != asset_identity)
                    .map(|(key, _)| *key)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for url in stale_images {
            if let Some(full) = self.full_mindmap.as_mut() {
                full.preview_loading_images.remove(&url);
            }
            if matches!(self.image_cache.get(&url), Some(ImageState::Loading)) {
                self.image_cache.remove(&url);
            }
        }
        for key in stale_diagrams {
            if let Some(full) = self.full_mindmap.as_mut() {
                full.preview_pending_diagrams.remove(&key);
            }
            if matches!(
                self.diagram_cache.peek(&key),
                Some(crate::diagram::DiagramState::Pending)
            ) {
                self.diagram_cache.remove(&key);
            }
        }
    }

    /// Prime the same remote-image and diagram caches used by normal document
    /// rendering, but tag every asynchronous result with this preview's
    /// namespace/request identity. Stale preview assets are therefore dropped
    /// without mutating a newer file's lifecycle.
    fn prime_full_mindmap_preview_assets(&mut self) -> Task<Message> {
        let Some(full) = self.full_mindmap.as_ref() else {
            return Task::none();
        };
        let Some(request) = full.preview_identity.as_ref() else {
            return Task::none();
        };
        let FullMindmapPreview::Document { blocks, assets, .. } = &full.preview else {
            return Task::none();
        };
        let identity = FullMindmapPreviewIdentity {
            namespace: full.preview_namespace,
            request: request.clone(),
        };
        let range = full.preview_window.range;
        let tag = (identity.namespace, identity.request.id, range);
        let cursor = if full.preview_asset_cursor_tag == Some(tag) {
            full.preview_asset_cursor
        } else {
            0
        };
        let display_len = full.preview_window.display.len();
        let visible_start = range.0.min(display_len);
        let visible_end = range.1.min(display_len);
        // Copy only the currently materialized virtual slice. Cloning the
        // worker-built display vector here would reintroduce an O(all-blocks)
        // UI allocation on every scroll/asset completion.
        let visible_indices = full.preview_window.display[visible_start..visible_end].to_vec();
        let visible_ids = visible_indices
            .iter()
            .filter_map(|&idx| blocks.get(idx).map(|(id, _)| *id))
            .collect::<Vec<_>>();
        let index = assets.clone();
        let visible_blocks = if index.is_none() {
            visible_indices
                .iter()
                .filter_map(|&idx| blocks.get(idx).cloned())
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let mut total = 0usize;
        let mut skipped = 0usize;
        let mut descriptors = Vec::new();
        if let Some(index) = &index {
            total = visible_ids
                .iter()
                .filter_map(|id| index.by_block.get(id))
                .map(Vec::len)
                .sum();
            'indexed: for id in &visible_ids {
                let Some(assets) = index.by_block.get(id) else {
                    continue;
                };
                for asset in assets {
                    if skipped < cursor {
                        skipped += 1;
                        continue;
                    }
                    if descriptors.len() >= FULL_MINDMAP_PREVIEW_ASSET_BATCH {
                        break 'indexed;
                    }
                    skipped += 1;
                    descriptors.push(asset.clone());
                }
            }
        } else {
            for (_, block) in &visible_blocks {
                let mut images = Vec::new();
                let mut diagrams = Vec::new();
                collect_preview_assets(block, &mut images, &mut diagrams);
                let assets = images
                    .into_iter()
                    .map(FullMindmapPreviewAsset::Image)
                    .chain(diagrams.into_iter().map(|(hash, kind, source)| {
                        FullMindmapPreviewAsset::Diagram { hash, kind, source }
                    }))
                    .collect::<Vec<_>>();
                total = total.saturating_add(assets.len());
                for asset in assets {
                    if skipped < cursor {
                        skipped += 1;
                        continue;
                    }
                    if descriptors.len() >= FULL_MINDMAP_PREVIEW_ASSET_BATCH {
                        skipped += 1;
                        continue;
                    }
                    skipped += 1;
                    descriptors.push(asset);
                }
            }
        }

        // A changed visible range starts a fresh lazy wave and drops off-screen
        // keep-set ownership. Subsequent waves for the same tag append only
        // descriptors from this range.
        if let Some(full) = self.full_mindmap.as_mut() {
            if full.preview_asset_cursor_tag != Some(tag) {
                full.preview_asset_wave_id = full.preview_asset_wave_id.wrapping_add(1);
                full.preview_asset_images.clear();
                full.preview_failed_images.clear();
                full.preview_asset_diagrams.clear();
                full.preview_asset_cursor_tag = Some(tag);
            }
        }

        let mut tasks = Vec::new();
        let mut seen_images = HashSet::new();
        let mut seen_diagrams = HashSet::new();
        let mut consumed = 0usize;
        let mut dispatched = 0usize;
        let mut blocked = false;
        let theme_id = self.diagram_theme_id;
        let palette = self.palette;
        let asset_identity = FullMindmapPreviewAssetIdentity {
            preview: identity.clone(),
            range,
            wave: self
                .full_mindmap
                .as_ref()
                .map(|full| full.preview_asset_wave_id)
                .unwrap_or(0),
        };
        self.reconcile_full_mindmap_preview_asset_wave(&asset_identity);
        for asset in descriptors {
            match asset {
                FullMindmapPreviewAsset::Image(url) => {
                    if !is_remote_url(&url) || !seen_images.insert(url.clone()) {
                        consumed += 1;
                        continue;
                    }
                    if let Some(full) = self.full_mindmap.as_mut() {
                        full.preview_asset_images.insert(url.clone());
                    }
                    let preview_failed = self
                        .full_mindmap
                        .as_ref()
                        .is_some_and(|full| full.preview_failed_images.contains(&url));
                    if preview_failed {
                        consumed += 1;
                        continue;
                    }
                    let cache_is_loading =
                        matches!(self.image_cache.get(&url), Some(ImageState::Loading));
                    let cache_is_loaded = matches!(
                        self.image_cache.get(&url),
                        Some(ImageState::Loaded(_) | ImageState::LoadedSvg { .. })
                    );
                    let preview_owns_loading = self.full_mindmap.as_ref().is_some_and(|full| {
                        full.preview_loading_images
                            .get(&url)
                            .is_some_and(|owner| owner == &asset_identity)
                    });
                    if cache_is_loaded || (cache_is_loading && preview_owns_loading) {
                        consumed += 1;
                        continue;
                    }
                    if self.full_mindmap.as_ref().is_some_and(|full| {
                        full.preview_loading_images.len() >= FULL_MINDMAP_PREVIEW_ASSET_BATCH
                    }) {
                        blocked = true;
                        break;
                    }
                    if self.image_cache.contains_key(&url) && !preview_owns_loading {
                        self.image_cache.remove(&url);
                    }
                    self.image_cache.insert(url.clone(), ImageState::Loading);
                    if let Some(full) = self.full_mindmap.as_mut() {
                        full.preview_loading_images
                            .insert(url.clone(), asset_identity.clone());
                    }
                    let task_identity = identity.clone();
                    let task_url = url.clone();
                    tasks.push(Task::perform(
                        fetch_image(task_url.clone()),
                        move |(_, result)| Message::FullMindmapPreviewImageFetched {
                            identity: task_identity.clone(),
                            range,
                            wave: asset_identity.wave,
                            url: task_url.clone(),
                            result,
                        },
                    ));
                    consumed += 1;
                    dispatched += 1;
                }
                FullMindmapPreviewAsset::Diagram { hash, kind, source } => {
                    let key = (hash, theme_id);
                    if !seen_diagrams.insert(hash) {
                        consumed += 1;
                        continue;
                    }
                    if let Some(full) = self.full_mindmap.as_mut() {
                        full.preview_asset_diagrams.insert(key);
                    }
                    let cache_is_pending = matches!(
                        self.diagram_cache.peek(&key),
                        Some(crate::diagram::DiagramState::Pending)
                    );
                    let preview_owns_pending = self.full_mindmap.as_ref().is_some_and(|full| {
                        full.preview_pending_diagrams
                            .get(&key)
                            .is_some_and(|owner| owner == &asset_identity)
                    });
                    if self.diagram_cache.peek(&key).is_some()
                        && (!cache_is_pending || preview_owns_pending)
                    {
                        consumed += 1;
                        continue;
                    }
                    if self.full_mindmap.as_ref().is_some_and(|full| {
                        full.preview_pending_diagrams.len() >= FULL_MINDMAP_PREVIEW_ASSET_BATCH
                    }) {
                        blocked = true;
                        break;
                    }
                    if cache_is_pending && !preview_owns_pending {
                        self.diagram_cache.remove(&key);
                    }
                    self.diagram_cache
                        .put(key, crate::diagram::DiagramState::Pending);
                    if let Some(full) = self.full_mindmap.as_mut() {
                        full.preview_pending_diagrams
                            .insert(key, asset_identity.clone());
                    }
                    let task_identity = identity.clone();
                    tasks.push(Task::perform(
                        crate::diagram::render_blocking_async(
                            kind,
                            source,
                            palette,
                            "JetBrains Mono".into(),
                        ),
                        move |result| Message::FullMindmapPreviewDiagramRendered {
                            identity: task_identity.clone(),
                            range,
                            wave: asset_identity.wave,
                            hash,
                            theme_id,
                            result,
                        },
                    ));
                    consumed += 1;
                    dispatched += 1;
                }
            }
        }
        let next_cursor = cursor.saturating_add(consumed);
        let has_more = next_cursor < total;
        if let Some(full) = self.full_mindmap.as_mut() {
            full.preview_asset_cursor = next_cursor;
        }
        let next_wave = if has_more && dispatched == 0 && !blocked {
            Task::done(Message::FullMindmapPreviewAssetWave {
                identity,
                range,
                wave: asset_identity.wave,
            })
        } else {
            Task::none()
        };
        if tasks.is_empty() {
            next_wave
        } else {
            Task::batch([Task::batch(tasks), next_wave])
        }
    }

    /// Theme ids are part of diagram cache identity. When the palette changes,
    /// discard only preview-owned Pending work from the old theme and rebuild
    /// the retained preview's diagram asset set under the new key immediately.
    /// Loaded image assets remain reusable across theme changes.
    fn refresh_full_mindmap_preview_assets_for_theme(&mut self) -> Task<Message> {
        let stale = self
            .full_mindmap
            .as_ref()
            .map(|full| {
                full.preview_pending_diagrams
                    .keys()
                    .filter(|(_, theme_id)| *theme_id != self.diagram_theme_id)
                    .copied()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for key in stale {
            if matches!(
                self.diagram_cache.peek(&key),
                Some(crate::diagram::DiagramState::Pending)
            ) {
                self.diagram_cache.remove(&key);
            }
        }
        let theme_id = self.diagram_theme_id;
        if let Some(full) = self.full_mindmap.as_mut() {
            full.preview_pending_diagrams
                .retain(|(_, id), _| *id == theme_id);
            full.preview_asset_diagrams
                .retain(|(_, id)| *id == theme_id);
            full.preview_failed_images.clear();
            full.preview_asset_cursor = 0;
            full.preview_asset_cursor_tag = None;
        }
        self.prime_full_mindmap_preview_assets()
    }

    /// Recompute `diagram_theme_id` from current palette. Returns true iff
    /// the id changed (i.e. palette actually differs). Callers can skip
    /// `prime_diagram_cache` when this returns false — palette unchanged
    /// means existing cache entries are still valid.
    fn refresh_diagram_theme_id(&mut self) -> bool {
        let new_id = crate::diagram::theme_id(&self.palette);
        if new_id == self.diagram_theme_id {
            false
        } else {
            self.diagram_theme_id = new_id;
            true
        }
    }

    fn rebuild_matches(&mut self) {
        self.matches = search::find_in_blocks(&self.ast, &self.query);
        self.match_idx = 0;
    }

    pub fn blocks(&self) -> impl Iterator<Item = &Block> {
        self.ast.iter().map(|(_, b)| b)
    }

    fn open_overlay(&mut self, kind: Overlay) {
        self.overlay = kind;
        self.overlay_query.clear();
        self.overlay_selected = 0;
        self.overlay_viewport = None;
        if kind == Overlay::FolderPicker {
            let start = self.workspace.clone().or_else(|| {
                self.file
                    .as_ref()
                    .and_then(|p| p.parent().map(|x| x.to_path_buf()))
            });
            self.picker = Some(Picker::new(start, PickerMode::OpenAny, self.show_hidden));
        } else {
            self.picker = None;
        }
    }

    fn mindmap_panel_range(&self, target: BlockId) -> Option<(usize, usize, bool)> {
        let mut start = None;
        let mut natural_end = self.ast.len();
        for (i, (id, b)) in self.ast.iter().enumerate() {
            if start.is_none() {
                if *id == target && matches!(b, Block::Heading { .. }) {
                    start = Some(i);
                }
            } else if matches!(b, Block::Heading { .. }) {
                natural_end = i;
                break;
            }
        }

        let start = start?;
        let mut end = natural_end;
        let mut text_bytes = 0usize;
        for i in start..natural_end {
            let block_count = i - start + 1;
            text_bytes = text_bytes.saturating_add(block_text_bytes(&self.ast[i].1));
            if block_count >= MIND_PANEL_MAX_BLOCKS || text_bytes >= MIND_PANEL_MAX_TEXT_BYTES {
                end = i + 1;
                break;
            }
        }
        Some((start, end, end < natural_end))
    }

    /// Leaf panel for data-doc mindmaps: pretty-print the selected node's
    /// subtree and render it through the shared data code-block view. The pretty
    /// string is cached in `mindmap_data_panel` so it is computed at most once
    /// per selection change (mirrors the markdown panel's settle behavior).
    fn mindmap_data_panel_view(
        &self,
        pal: &Palette,
        recently_scrolled: bool,
        panel_width: f32,
    ) -> Element<'_, Message> {
        let pal_c = *pal;
        // Refresh the cached pretty string if the shown node changed.
        if let Some(target) = self.mindmap_panel_shown {
            let needs = self
                .mindmap_data_panel
                .borrow()
                .as_ref()
                .map(|(id, _)| *id != target)
                .unwrap_or(true);
            if needs {
                let (_, _, paths) = self.mindmap_layout();
                let lang = data_lang_for(self.file.as_deref()).unwrap_or("json");
                let pretty = paths
                    .get(&target)
                    .and_then(|p| crate::data_mindmap::subtree_pretty(&self.source, lang, p))
                    .unwrap_or_default();
                *self.mindmap_data_panel.borrow_mut() = Some((target, pretty));
            }
        }

        let pretty_owned: Option<String> = self
            .mindmap_data_panel
            .borrow()
            .as_ref()
            .filter(|(_, p)| !p.is_empty())
            .map(|(_, p)| p.clone());
        let content: Element<'_, Message> = match pretty_owned {
            Some(pretty) => crate::render::data_view_owned(pretty, pal, &self.typography),
            None => container(
                text("Select a node to see its value")
                    .color(pal.muted)
                    .size(13),
            )
            .padding(24)
            .into(),
        };

        let scrolled = scrollable(container(content).padding(Padding::from([24, 24])))
            .height(Length::Shrink)
            .direction(slim_scroll_direction())
            .style(move |_, status| sleek_scrollable_style(status, pal_c, recently_scrolled));
        container(scrolled)
            .width(Length::Fixed(panel_width))
            .height(Length::Fill)
            .center_y(Length::Fill)
            .style(move |_| container::Style {
                background: Some(pal_c.surface.into()),
                ..Default::default()
            })
            .into()
    }

    /// Right-side panel shown in Mindmap mode. Renders a bounded markdown slice
    /// for the selected heading so panel open/redraw cannot rebuild huge trees.
    fn mindmap_panel_view(
        &self,
        pal: &Palette,
        hl: &Highlight,
        recently_scrolled: bool,
        panel_width: f32,
    ) -> Element<'_, Message> {
        if self.is_data_doc {
            return self.mindmap_data_panel_view(pal, recently_scrolled, panel_width);
        }
        let pal_c = *pal;
        let content: Element<'_, Message> = match self.mindmap_panel_shown {
            None => container(
                text("Click a leaf heading to see its content")
                    .color(pal.muted)
                    .size(13),
            )
            .padding(24)
            .into(),
            Some(target) => match self.mindmap_panel_range(target) {
                Some((s, end, truncated)) => {
                    let mut col = Column::new().spacing(12).push(crate::render::render(
                        &self.ast[s..end],
                        pal,
                        &self.typography,
                        hl,
                        // Bounded slice in its own scrollable — never windowed.
                        None,
                        &self.image_cache,
                        self.file.as_deref(),
                        &self.folded,
                        self.hovered_heading,
                        &self.diagram_cache,
                        self.diagram_theme_id,
                        true,
                        (0, 0),
                        recently_scrolled,
                    ));
                    if truncated {
                        col = col.push(
                            container(
                                text("Panel preview truncated for performance")
                                    .color(pal.muted)
                                    .size(12),
                            )
                            .padding(Padding::from([8, 0])),
                        );
                    }
                    col.into()
                }
                None => container(text("Heading not found").color(pal.muted).size(13))
                    .padding(24)
                    .into(),
            },
        };
        // Center content vertically when it fits; scroll from the top when it
        // overflows. The scrollable measures the inner column's natural height:
        // a Fill-height wrapper would clamp to the viewport and kill scrolling,
        // so instead we anchor the column and let the outer container center it.
        let scrolled = scrollable(container(content).padding(Padding::from([24, 24])))
            .height(Length::Shrink)
            .direction(slim_scroll_direction())
            .style(move |_, status| sleek_scrollable_style(status, pal_c, recently_scrolled));
        // Scrollable fills the available height (short content stays centered via
        // center_y; long content scrolls). Keyboard hints live on the map canvas,
        // not in this content panel.
        let body = container(scrolled)
            .height(Length::Fill)
            .center_y(Length::Fill);
        container(body)
            .width(Length::Fixed(panel_width))
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(pal_c.surface.into()),
                border: Border {
                    color: pal_c.rule,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .into()
    }

    /// Full-window workspace navigator. It deliberately reads only
    /// `full_mindmap` and `workspace_mindmap` state; document mindmap layout,
    /// collapse, selection, and preview state stay untouched underneath.
    fn full_mindmap_view(&self, pal: Palette, recently_scrolled: bool) -> Element<'_, Message> {
        let Some(full) = self.full_mindmap.as_ref() else {
            return Space::new().into();
        };
        let Some(graph) = self.full_mindmap_graph() else {
            if let Some(load) = &full.pending_workspace_load {
                return container(
                    column![
                        text("Indexing workspace…").size(14).color(pal.fg),
                        text(load.path.display().to_string()).size(12).color(pal.muted),
                        text("Large folders are indexed in the background with a fixed safety limit.")
                            .size(12)
                            .color(pal.muted),
                    ]
                    .spacing(10),
                )
                .padding(24)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into();
            }
            return container(
                text("Workspace navigator unavailable — press Esc to return")
                    .size(14)
                    .color(pal.muted),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into();
        };

        let program = crate::mindmap::MindmapProgram::<WorkspaceNodeId, Message> {
            nodes: graph.nodes.clone(),
            content_size: graph.content_size,
            palette: pal,
            selected: full.selected.clone(),
            // Keep the explicit navigator request distinct from the selected
            // ring. The shared canvas uses `selected` as a compatibility
            // fallback only when no request exists; a present request always
            // targets the accepted node's final layout position.
            focus: full.focus_request.clone(),
            panel_open: full.panel_open,
            panel_width: full.panel_width,
            autocenter: true,
            layout_generation: Some(full.layout_generation),
            keyboard_zoom_enabled: self.overlay == Overlay::None,
            native_pinch_log: self.full_mindmap_native_pinch_log,
            on_toggle: Box::new(Message::FullMindmapToggleNode),
            on_select: Box::new(Message::FullMindmapSelectNode),
            on_deselect: Message::FullMindmapDeselect,
        };
        let canvas: Element<'_, Message> = iced::widget::canvas(program)
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
        let selected_is_file = full
            .selected
            .as_ref()
            .and_then(|id| graph.node(id))
            .is_some_and(|node| node.kind == WorkspaceNodeKind::File)
            && (full.pending_workspace_load.is_none()
                || full
                    .pending_workspace_load
                    .as_ref()
                    .is_some_and(|request| request.preserve_navigation));
        let hint_items: &[(&str, &str)] = match selected_is_file {
            true => &[("←↑→↓", "move"), ("= / −", "zoom"), ("Enter", "open")],
            false => &[
                ("←↑→↓", "move"),
                ("Space", "fold"),
                ("= / −", "zoom"),
                ("Enter", "root"),
            ],
        };
        let canvas: Element<'_, Message> = stack![canvas, floating_mindmap_hint(hint_items, pal)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
        let body: Element<'_, Message> = if full.panel_open {
            irow![
                canvas,
                full_mindmap_panel_resize_handle(pal),
                self.full_mindmap_panel_view(&graph, pal, full.panel_width, recently_scrolled),
            ]
            .into()
        } else {
            canvas
        };
        container(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(pal.bg.into()),
                ..Default::default()
            })
            .into()
    }

    fn full_mindmap_panel_view(
        &self,
        graph: &WorkspaceGraph,
        pal: Palette,
        panel_width: f32,
        recently_scrolled: bool,
    ) -> Element<'_, Message> {
        let Some(full) = self.full_mindmap.as_ref() else {
            return Space::new().into();
        };
        let selected = full.selected.as_ref().and_then(|id| graph.node(id));
        let label = full
            .selected
            .as_ref()
            .and_then(|id| graph.index_of(id))
            .and_then(|idx| graph.nodes.get(idx))
            .map(|node| node.full_label.clone())
            .unwrap_or_else(|| "Select a folder or file".to_string());
        let selected_path = selected.and_then(|node| node.path.as_ref());
        let selected_is_file = selected.is_some_and(|node| node.kind == WorkspaceNodeKind::File)
            && (full.pending_workspace_load.is_none()
                || full
                    .pending_workspace_load
                    .as_ref()
                    .is_some_and(|request| request.preserve_navigation));
        let preview_widget_generation = (
            full.preview_identity
                .as_ref()
                .or_else(|| full.pending_preview.as_ref())
                .map(|request| request.id)
                .or_else(|| {
                    full.pending_preview_settle
                        .as_ref()
                        .map(|request| request.id)
                })
                .unwrap_or(0),
            full.preview_generation,
        );
        let content: Element<'_, Message> =
            if selected_is_file {
                let preview: Element<'_, Message> = match &full.preview {
                    FullMindmapPreview::Loading(path) if Some(path) == selected_path => {
                        container(text("Loading preview…").size(13).color(pal.muted))
                            .padding(24)
                            .into()
                    }
                    FullMindmapPreview::Document {
                        path,
                        blocks,
                        truncated,
                        ..
                    } if Some(path) == selected_path => {
                        let rendered = crate::render::render(
                            blocks,
                            &pal,
                            &self.typography,
                            &Highlight::default(),
                            Some(&full.preview_window),
                            &self.image_cache,
                            Some(path.as_path()),
                            &HashSet::new(),
                            None,
                            &self.diagram_cache,
                            self.diagram_theme_id,
                            false,
                            preview_widget_generation,
                            recently_scrolled,
                        )
                        .map(|message| match message {
                            Message::TableScrolled => Message::TableScrolled,
                            _ => Message::Noop,
                        });
                        let mut preview = Column::new().push(rendered);
                        if *truncated {
                            preview = preview.push(
                                text("Preview truncated for performance")
                                    .size(12)
                                    .color(pal.muted),
                            );
                        }
                        preview.into()
                    }
                    FullMindmapPreview::Data {
                        path,
                        source,
                        truncated,
                    } if Some(path) == selected_path => {
                        let mut preview = Column::new().push(
                            crate::render::data_view_owned(source.clone(), &pal, &self.typography)
                                .map(|_| Message::Noop),
                        );
                        if *truncated {
                            preview = preview.push(
                                text("Preview truncated for performance")
                                    .size(12)
                                    .color(pal.muted),
                            );
                        }
                        preview.into()
                    }
                    FullMindmapPreview::Error { path, error } if Some(path) == selected_path => {
                        container(text(error.clone()).size(13).color(pal.accent))
                            .padding(24)
                            .into()
                    }
                    _ => container(text("Loading preview…").size(13).color(pal.muted))
                        .padding(24)
                        .into(),
                };
                // The label/path header and its fixed 20 px top breathing room
                // live outside the scrollable. The preview body therefore starts
                // at exact offset zero, regardless of label/path wrapping.
                let preview_header = container(
                    column![
                        text(label).size(14).color(pal.fg),
                        text(
                            selected_path
                                .map(|path| path.display().to_string())
                                .unwrap_or_default()
                        )
                        .size(12)
                        .color(pal.muted),
                    ]
                    .spacing(10),
                )
                .padding(Padding {
                    top: 20.0,
                    right: 18.0,
                    bottom: 10.0,
                    left: 18.0,
                })
                .width(Length::Fill);
                let scroll_tag = full_mindmap_preview_scroll_tag(full);
                let preview_scroll = scrollable(container(preview).padding(Padding {
                    top: 0.0,
                    right: 18.0,
                    bottom: 20.0,
                    left: 18.0,
                }))
                .id(Self::full_mindmap_preview_scroll_id())
                .on_scroll(move |viewport| Message::FullMindmapPreviewScrolled {
                    namespace: scroll_tag.0,
                    path: scroll_tag.1.clone(),
                    identity: scroll_tag.2,
                    viewport,
                })
                .height(Length::Fill)
                .direction(slim_scroll_direction())
                .style(move |_, status| sleek_scrollable_style(status, pal, recently_scrolled));
                column![preview_header, preview_scroll,].spacing(0).into()
            } else if let Some(load) = &full.pending_workspace_load {
                container(
                    column![
                    text("Indexing project…").size(14).color(pal.fg),
                    text(load.path.display().to_string()).size(12).color(pal.muted),
                    text("Large folders are indexed in the background with a fixed safety limit.")
                        .size(12)
                        .color(pal.muted),
                ]
                    .spacing(10),
                )
                .padding(24)
                .center_y(Length::Fill)
                .into()
            } else {
                let hint = match selected.map(|node| &node.kind) {
                    Some(WorkspaceNodeKind::Empty) => "No supported files are visible here.",
                    Some(WorkspaceNodeKind::Error) => "Choose another readable folder.",
                    Some(WorkspaceNodeKind::Truncated) => {
                        "Some files were omitted to keep navigation responsive."
                    }
                    _ => "Select a file to preview its content.",
                };
                container(text(hint).size(13).color(pal.muted))
                    .padding(24)
                    .center_y(Length::Fill)
                    .into()
            };
        let content: Element<'_, Message> = if let Some(error) = full.load_error.as_ref() {
            column![text(error.clone()).size(12).color(pal.accent), content,]
                .spacing(10)
                .into()
        } else {
            content
        };
        let body: Element<'_, Message> = if selected_is_file {
            content
        } else {
            let scroll_tag = full_mindmap_preview_scroll_tag(full);
            scrollable(container(content).padding(Padding::from([20, 18])))
                .id(Self::full_mindmap_preview_scroll_id())
                .on_scroll(move |viewport| Message::FullMindmapPreviewScrolled {
                    namespace: scroll_tag.0,
                    path: scroll_tag.1.clone(),
                    identity: scroll_tag.2,
                    viewport,
                })
                .height(Length::Fill)
                .direction(slim_scroll_direction())
                // Folder/status content keeps Iced's established default
                // scrollbar catalog; only the file preview below uses the
                // ordinary Markdown transparent-rail style.
                .style(scrollable::default)
                .into()
        };
        container(body)
            .width(Length::Fixed(panel_width))
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(pal.surface.into()),
                border: Border {
                    color: pal.rule,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .into()
    }

    fn command_items(&self) -> Vec<(&'static str, Message)> {
        let panel_toggle = if self.full_mindmap.is_some() {
            Message::FullMindmapTogglePanel
        } else {
            Message::ToggleMindmapPanel
        };
        let panel_width = if self.full_mindmap.is_some() {
            Message::FullMindmapCyclePanelWidth
        } else {
            Message::MindmapCyclePanelWidth
        };
        let mut items = vec![
            ("Open Folder…  ⌘O", Message::OpenFolderPicker),
            ("Refresh File / Folder  ⌘R", Message::Refresh),
            ("Reveal File in Finder  ⌘⌥R", Message::RevealFileInFinder),
            ("Copy Focused File Path  ⌘⌥C", Message::CopyFilePath),
            ("Find File in Workspace…  ⌘P", Message::OpenFileFinder),
            ("Toggle Sidebar  ⌘B", Message::ToggleSidebar),
            ("Toggle Hidden Files  ⌘⇧.", Message::ToggleHidden),
            ("Find in Document  ⌘F", Message::ToggleSearch),
            ("Search All Files…  ⌘⇧F", Message::OpenVaultSearch),
            ("Toggle Zen Edit  ⌘E", Message::ToggleViewMode),
            ("Increase Font Size  ⌘+", Message::FontSizeUp),
            ("Decrease Font Size  ⌘-", Message::FontSizeDown),
            ("Reset Font Size  ⌘0", Message::FontSizeReset),
            ("Toggle Status Footer", Message::ToggleFooter),
            ("Toggle Mindmap  ⌘M", Message::ToggleMindmap),
            ("Toggle Full Mindmap Mode  ⌘⇧M", Message::ToggleFullMindmap),
            ("Toggle Mindmap Panel  ⌘⌥B", panel_toggle),
            ("Cycle Mindmap Panel Width  ⌘⌥W", panel_width),
            (
                "Toggle Mindmap Auto-Center",
                Message::ToggleMindmapAutocenter,
            ),
            ("Cycle Theme  ⌘T", Message::ToggleTheme),
            ("Pick Theme…", Message::OpenThemePicker),
            ("Reload Custom Themes", Message::ReloadThemes),
            ("Open Themes Folder", Message::OpenThemesDir),
            ("Scroll to Top  Home / g", Message::ScrollToTop),
            ("Scroll to Bottom  End / G", Message::ScrollToBottom),
            (
                "Toggle Auto-Focus on Agent Nav",
                Message::ToggleAutoFocusOnNav,
            ),
            ("Show Keyboard Shortcuts  ⌘/", Message::ToggleShortcuts),
            ("Clear All Quick Slots", Message::QuickSlotClearAll),
            ("Take Screenshot", Message::TakeScreenshot),
        ];
        if self.view_mode == ViewMode::Mindmap && self.full_mindmap.is_none() {
            items.extend([
                (
                    "Mindmap: Show All Node Levels  ⌘K 0",
                    Message::FoldToLevel(0),
                ),
                ("Mindmap: Show 1 Node Level  ⌘K 1", Message::FoldToLevel(1)),
                ("Mindmap: Show 2 Node Levels  ⌘K 2", Message::FoldToLevel(2)),
                ("Mindmap: Show 3 Node Levels  ⌘K 3", Message::FoldToLevel(3)),
                ("Mindmap: Show 4 Node Levels  ⌘K 4", Message::FoldToLevel(4)),
                ("Mindmap: Show 5 Node Levels  ⌘K 5", Message::FoldToLevel(5)),
                ("Mindmap: Show 6 Node Levels  ⌘K 6", Message::FoldToLevel(6)),
            ]);
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        if crate::cli_install::should_offer() {
            items.push(("Install CLI", Message::InstallCli));
        }
        items
    }

    fn filtered_files(&self) -> Vec<(PathBuf, String, i32)> {
        let root = self.workspace.as_ref();
        let mut scored: Vec<(PathBuf, String, i32)> = self
            .workspace_files
            .iter()
            .filter_map(|p| {
                let rel = root
                    .and_then(|r| p.strip_prefix(r).ok())
                    .map(|x| x.to_string_lossy().into_owned())
                    .unwrap_or_else(|| p.to_string_lossy().into_owned());
                let s = picker::fuzzy_score(&self.overlay_query, &rel)?;
                Some((p.clone(), rel, s))
            })
            .collect();
        scored.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.cmp(&b.1)));
        scored.truncate(200);
        scored
    }

    fn filtered_commands(&self) -> Vec<(&'static str, Message, i32)> {
        let mut scored: Vec<(&'static str, Message, i32)> = self
            .command_items()
            .into_iter()
            .filter_map(|(label, msg)| {
                let s = picker::fuzzy_score(&self.overlay_query, label)?;
                Some((label, msg, s))
            })
            .collect();
        scored.sort_by(|a, b| b.2.cmp(&a.2));
        scored
    }

    fn filtered_themes(&self) -> Vec<ThemeEntry> {
        let mut out: Vec<ThemeEntry> = ThemePreset::ALL
            .into_iter()
            .map(ThemeEntry::Preset)
            .chain(
                self.custom_themes
                    .iter()
                    .map(|t| ThemeEntry::Custom(t.slug.clone(), t.name.clone(), t.palette)),
            )
            .filter(|t| {
                if self.overlay_query.is_empty() {
                    true
                } else {
                    picker::fuzzy_score(&self.overlay_query, t.label()).is_some()
                }
            })
            .collect();
        let _ = &mut out;
        out
    }

    fn reveal_current_file(&mut self) {
        let (Some(ws), Some(file)) = (self.workspace.as_ref(), self.file.as_ref()) else {
            return;
        };
        for a in tree::ancestors_of(ws, file) {
            self.expanded.insert(a);
        }
    }

    fn reveal_file_in_finder(path: &Path) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open")
                .arg("-R")
                .arg(path)
                .spawn()
                .map(|_| ())
                .map_err(|error| format!("couldn't launch Finder: {error}"))
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = path;
            Err("Finder reveal is only supported on macOS".to_string())
        }
    }

    fn focused_file_path(&self) -> Option<PathBuf> {
        if let Some(WorkspaceNodeId::File(path)) = self
            .full_mindmap
            .as_ref()
            .and_then(|full| full.selected.as_ref())
        {
            return Some(path.clone());
        }

        let files_sidebar_active = self.sidebar_open
            && self.workspace.is_some()
            && self.sidebar_tab == SidebarTab::Files
            && self.full_mindmap.is_none();
        if files_sidebar_active {
            if let Some(root) = &self.workspace_tree {
                let rows =
                    tree::flatten_with_files(root, &self.workspace_sidebar_files, &self.expanded);
                if let Some(row) = rows.get(self.tree_cursor) {
                    if !row.node.is_dir() {
                        return Some(row.node.path().to_path_buf());
                    }
                }
            }
        }

        self.file.clone()
    }

    fn reply(
        tx: &std::sync::Arc<
            std::sync::Mutex<Option<futures::channel::oneshot::Sender<crate::ipc::Response>>>,
        >,
        resp: crate::ipc::Response,
    ) {
        if let Some(sender) = tx.lock().ok().and_then(|mut g| g.take()) {
            let _ = sender.send(resp);
        }
    }

    pub fn update(&mut self, msg: Message) -> Task<Message> {
        if let Some(rel) = self.queued_snap.take() {
            // Drain any pending IPC-driven scroll BEFORE dispatching the new
            // message so the snap lands before further state mutation.
            // The new message is requeued via a follow-up task.
            let mut tasks = vec![Task::done(Message::RestoreBodySnap(rel))];
            if let Some(id) = self.queued_goto.take() {
                // Precise pass: the estimate snap above lands near the target;
                // this op re-lands it from real laid-out bounds (the block is
                // materialized — apply_goto rebuilt the window around it).
                tasks.push(scroll_block_to_center(id));
            }
            tasks.push(Task::done(msg));
            return Task::batch(tasks);
        }
        match msg {
            Message::Open(p) => self.load_file_unless_dirty(p),
            Message::Refresh => self.refresh_status(),
            Message::RevealFileInFinder => {
                let Some(path) = self.focused_file_path() else {
                    return self.show_toast("No file open".into());
                };
                let label = match Self::reveal_file_in_finder(&path) {
                    Ok(()) => "Revealed in Finder".to_string(),
                    Err(error) => error,
                };
                self.show_toast(label)
            }
            Message::CopyFilePath => {
                let Some(path) = self.focused_file_path() else {
                    return self.show_toast("No file open".into());
                };
                let Some(expected) = path.to_str().map(str::to_owned) else {
                    return self.show_toast("File path cannot be copied as text".into());
                };
                self.pending_clipboard_copy = Some(expected.clone());
                let toast = self.show_toast("Copying file path…".into());
                let verify_expected = expected.clone();
                let verify = Task::perform(
                    async {
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    },
                    move |_| Message::CheckClipboardCopy(verify_expected),
                );
                Task::batch([
                    iced::clipboard::write::<Message>(expected.clone()),
                    verify,
                    toast,
                ])
            }
            Message::CheckClipboardCopy(expected) => {
                if self.pending_clipboard_copy.as_deref() != Some(expected.as_str()) {
                    return Task::none();
                }
                iced::clipboard::read().map(move |actual| Message::ClipboardCopyChecked {
                    expected: expected.clone(),
                    actual,
                })
            }
            Message::ClipboardCopyChecked { expected, actual } => {
                if self.pending_clipboard_copy.as_deref() != Some(expected.as_str()) {
                    return Task::none();
                }
                self.pending_clipboard_copy = None;
                if actual.as_deref() == Some(expected.as_str()) {
                    self.show_toast("File path copied".into())
                } else {
                    self.show_toast("Couldn't copy file path".into())
                }
            }
            Message::OpenFileFinderPath(p) => {
                self.overlay = Overlay::None;
                if self.full_mindmap.is_some() {
                    self.begin_full_mindmap_open(p)
                } else {
                    self.load_file_unless_dirty(p)
                }
            }
            Message::OpenWorkspace(p) => {
                self.cancel_refresh_tracking();
                // A workspace selected through Full Mindmap Mode should not
                // silently alter the hidden sidebar's open/closed preference.
                if self.full_mindmap.is_some() {
                    let exit_after_refresh = self.pending_ipc_file_open.is_some();
                    self.begin_full_mindmap_workspace_load(
                        p,
                        false,
                        None,
                        false,
                        false,
                        exit_after_refresh,
                    )
                } else {
                    self.set_workspace(p, true);
                    Task::none()
                }
            }
            Message::OpenFolderPicker => {
                self.open_overlay(Overlay::FolderPicker);
                Task::none()
            }
            Message::OpenFileFinder => {
                if self.workspace.is_some() {
                    self.open_overlay(Overlay::FileFinder);
                } else {
                    self.open_overlay(Overlay::FolderPicker);
                }
                iced::widget::operation::focus(Self::overlay_input_id())
            }
            Message::OpenCommandPalette => {
                self.open_overlay(Overlay::Command);
                iced::widget::operation::focus(Self::overlay_input_id())
            }
            Message::OpenThemePicker => {
                self.open_overlay(Overlay::ThemePicker);
                iced::widget::operation::focus(Self::overlay_input_id())
            }
            Message::OpenVaultSearch => {
                if self.workspace.is_none() {
                    // No folder open: pick one first.
                    self.open_overlay(Overlay::FolderPicker);
                    return iced::widget::operation::focus(Self::overlay_input_id());
                }
                self.vault_open = true;
                self.vault_query.clear();
                self.vault_searched_query = None;
                self.vault_results.clear();
                self.vault_file_count = 0;
                self.vault_truncated = false;
                self.vault_cursor = 0;
                self.vault_collapsed.clear();
                self.vault_viewport = None;
                // Bump seq so any in-flight `run` from a prior open is dropped
                // by the `VaultSearchDone` seq guard instead of repopulating
                // the freshly-blank page.
                self.vault_seq += 1;
                iced::widget::operation::focus(Self::vault_input_id())
            }
            Message::VaultQueryChanged(q) => {
                // Typing only updates the query text; the search runs on Enter
                // (VaultRunSearch) so we don't re-scan the vault per keystroke.
                self.vault_query = q;
                Task::none()
            }
            Message::VaultEnter => {
                // Enter searches when the query was edited since the last search,
                // otherwise opens the hit the cursor is on.
                if self.vault_searched_query.as_deref() == Some(self.vault_query.as_str()) {
                    Task::done(Message::VaultOpenSelected)
                } else {
                    Task::done(Message::VaultRunSearch)
                }
            }
            Message::VaultRunSearch => {
                self.vault_cursor = 0;
                self.vault_seq += 1;
                self.vault_searched_query = Some(self.vault_query.clone());
                let seq = self.vault_seq;
                let files = self.workspace_files.clone();
                let query = self.vault_query.clone();
                Task::perform(
                    crate::vault_search::run(files, query, seq),
                    Message::VaultSearchDone,
                )
            }
            Message::VaultSearchDone(r) => {
                // Drop stale results whose query was superseded mid-scan.
                if r.seq == self.vault_seq {
                    self.vault_results = r.hits;
                    // Hits arrive grouped by file, so distinct files = number
                    // of adjacent path runs (same walk the view used to do).
                    let mut last: Option<&std::path::Path> = None;
                    let mut n = 0;
                    for h in &self.vault_results {
                        if last != Some(h.path.as_path()) {
                            n += 1;
                            last = Some(h.path.as_path());
                        }
                    }
                    self.vault_file_count = n;
                    self.vault_truncated = r.truncated;
                    self.vault_cursor = 0;
                    // New result set: drop the stale viewport so virtualization
                    // renders from the top, and scroll the list back to 0.
                    self.vault_viewport = None;
                    return iced::widget::operation::scroll_to(
                        Self::vault_scroll_id(),
                        iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: 0.0 },
                    );
                }
                Task::none()
            }
            Message::VaultMove(d) => {
                let visible = self.vault_visible_matches();
                if visible.is_empty() {
                    return Task::none();
                }
                let next = (self.vault_cursor as isize + d).clamp(0, visible.len() as isize - 1);
                self.vault_cursor = next as usize;
                self.scroll_vault_to_cursor()
            }
            Message::VaultToggleFile(path) => {
                // Remember which hit the cursor pointed at so it tracks that
                // match across the visible-list shift (collapsing a group above
                // the cursor otherwise silently re-targets it).
                let anchor = self.vault_visible_matches().get(self.vault_cursor).copied();
                if !self.vault_collapsed.remove(&path) {
                    self.vault_collapsed.insert(path);
                }
                let visible = self.vault_visible_matches();
                self.vault_cursor = anchor
                    .and_then(|hi| visible.iter().position(|&v| v == hi))
                    .unwrap_or_else(|| {
                        // Anchored hit is now hidden: clamp to the last visible.
                        self.vault_cursor.min(visible.len().saturating_sub(1))
                    });
                Task::none()
            }
            Message::VaultOpenSelected => {
                // Resolve the cursor to a hit index and share VaultOpenHit's path.
                match self.vault_visible_matches().get(self.vault_cursor).copied() {
                    Some(hi) => Task::done(Message::VaultOpenHit(hi)),
                    None => Task::none(),
                }
            }
            Message::VaultOpenHit(idx) => {
                if let Some(hit) = self.vault_results.get(idx).cloned() {
                    if let Some(blocked) = self.block_file_open_if_dirty() {
                        return blocked;
                    }
                    self.vault_open = false;
                    self.pending_nav = Some(PendingNav {
                        line: Some(hit.line),
                        ..Default::default()
                    });
                    return Task::done(Message::Open(hit.path));
                }
                Task::none()
            }
            Message::VaultClose => {
                self.vault_open = false;
                Task::none()
            }
            Message::VaultScrollTo(y) => iced::widget::operation::scroll_to(
                Self::vault_scroll_id(),
                iced::widget::scrollable::AbsoluteOffset { x: 0.0, y },
            ),
            Message::ToggleShortcuts => {
                if self.overlay == Overlay::Shortcuts {
                    self.overlay = Overlay::None;
                } else {
                    self.open_overlay(Overlay::Shortcuts);
                }
                Task::none()
            }
            Message::QuickSlotsModifier(held) => {
                if held {
                    if self.quick_slots_modifier_held {
                        return Task::none();
                    }

                    self.quick_slots_modifier_held = true;
                    self.quick_slots_rail_revealed = false;
                    self.quick_slots_modifier_generation =
                        self.quick_slots_modifier_generation.wrapping_add(1);
                    let generation = self.quick_slots_modifier_generation;
                    Task::perform(
                        async move {
                            tokio::time::sleep(std::time::Duration::from_millis(
                                QUICK_SLOTS_RAIL_REVEAL_DELAY_MS,
                            ))
                            .await;
                            generation
                        },
                        Message::QuickSlotsModifierReveal,
                    )
                } else {
                    self.quick_slots_modifier_held = false;
                    self.quick_slots_rail_revealed = false;
                    self.quick_slots_modifier_generation =
                        self.quick_slots_modifier_generation.wrapping_add(1);
                    Task::none()
                }
            }
            Message::QuickSlotsModifierReveal(generation) => {
                if self.quick_slots_modifier_held
                    && generation == self.quick_slots_modifier_generation
                {
                    self.quick_slots_rail_revealed = true;
                }
                Task::none()
            }
            Message::QuickSlotActivate(index) => self.begin_quick_slot_activation(index),
            Message::QuickSlotAssign(index) => self.assign_quick_slot(index),
            Message::QuickSlotClear(index) => self.clear_quick_slot(index),
            Message::QuickSlotClearAll => self.clear_all_quick_slots(),
            Message::QuickSlotUndo => self.undo_quick_slot_clear(),
            Message::QuickSlotNew => self.new_quick_slot(),
            Message::QuickSlotClose => self.close_active_quick_slot(),
            Message::QuickSlotCloseWindow => self.close_quick_slot_window(),
            Message::QuickSlotCycle(delta) => {
                let active = self.quick_slots.active;
                let start = active.unwrap_or(if delta < 0 {
                    crate::quick_slots::SLOT_COUNT - 1
                } else {
                    0
                }) as isize;
                let root = self.quick_slots_workspace_root().map(PathBuf::from);
                let steps: Vec<usize> = if active.is_some() {
                    (1..=crate::quick_slots::SLOT_COUNT).collect()
                } else {
                    (0..crate::quick_slots::SLOT_COUNT).collect()
                };
                let next = steps
                    .into_iter()
                    .map(|step| {
                        (start + isize::from(delta) * step as isize)
                            .rem_euclid(crate::quick_slots::SLOT_COUNT as isize)
                            as usize
                    })
                    .find(|&index| {
                        self.quick_slots.occupied(index).is_some_and(|slot| {
                            root.as_ref()
                                .and_then(|root| {
                                    crate::quick_slots::resolve_path(root, &slot.relative_path)
                                })
                                .is_some_and(|path| path.is_file())
                        })
                    });
                match next {
                    Some(index) => self.begin_quick_slot_activation(index),
                    None => self.show_toast("No other valid Quick Slot".into()),
                }
            }
            Message::QuickSlotsPersist(generation) => {
                if generation < self.quick_slots_persist_generation {
                    self.quick_slots_persist_pending = false;
                    return self.schedule_quick_slots_persist();
                }
                self.persist_quick_slots_now();
                Task::none()
            }
            Message::QuickSlotRestorePending => self.apply_pending_quick_slot_restore(),
            Message::QuickSlotFileLoaded {
                index,
                slot,
                result,
            } => {
                let current = self
                    .pending_quick_slot_restore
                    .as_ref()
                    .is_some_and(|pending| {
                        pending.index == index
                            && pending.slot == slot
                            && self.quick_slot_restore_is_current(pending)
                    });
                if !current {
                    return Task::none();
                }
                match result {
                    Ok((path, source)) => {
                        let expected = self.quick_slots_workspace_root().and_then(|root| {
                            crate::quick_slots::resolve_path(root, &slot.relative_path)
                        });
                        if expected.as_ref() != Some(&path) {
                            self.invalidate_pending_quick_slot_restore();
                            return Task::none();
                        }
                        if self.dirty {
                            self.invalidate_pending_quick_slot_restore();
                            return self.show_toast(self.unsaved_edits_open_message());
                        }
                        self.update(Message::FileLoaded(Ok((path, source))))
                    }
                    Err(error) => {
                        self.invalidate_pending_quick_slot_restore();
                        self.error = Some(error);
                        Task::none()
                    }
                }
            }
            Message::CloseOverlay => {
                let was_zoom = self.overlay == Overlay::ImageZoom;
                self.overlay = Overlay::None;
                self.picker = None;
                self.zoom_url = None;
                self.zoom_diagram = None;
                if was_zoom {
                    self.restore_body_scroll()
                } else {
                    Task::none()
                }
            }
            Message::FullMindmapPreviewImageFetched {
                identity,
                range,
                wave,
                url,
                result,
            } => {
                let asset_identity = FullMindmapPreviewAssetIdentity {
                    preview: identity.clone(),
                    range,
                    wave,
                };
                let valid = self.full_mindmap.as_ref().is_some_and(|full| {
                    full.preview_namespace == identity.namespace
                        && full.preview_identity.as_ref() == Some(&identity.request)
                        && full.preview_window.range == range
                        && full.preview_asset_wave_id == wave
                        && full.selected.as_ref().is_some_and(|selected| {
                            matches!(selected, WorkspaceNodeId::File(path) if path == &identity.request.path)
                        })
                });
                let owns_loading = self
                    .full_mindmap
                    .as_ref()
                    .and_then(|full| full.preview_loading_images.get(&url))
                    .is_some_and(|owner| owner == &asset_identity);
                if owns_loading {
                    if let Some(full) = self.full_mindmap.as_mut() {
                        full.preview_loading_images.remove(&url);
                    }
                }
                if !valid {
                    // Only the operation that inserted this Loading sentinel
                    // may remove it. A stale A completion must not delete B's
                    // same-URL request after B has taken ownership.
                    if owns_loading
                        && matches!(self.image_cache.get(&url), Some(ImageState::Loading))
                    {
                        self.image_cache.remove(&url);
                    }
                    return Task::none();
                }
                // A duplicate/current-identity completion that no longer
                // owns the Loading sentinel cannot claim the shared cache or
                // alter the terminal-failure set.
                if !owns_loading {
                    return Task::none();
                }
                match result {
                    Ok(bytes) => {
                        let state = if is_svg_bytes(&bytes)
                            || url.to_ascii_lowercase().ends_with(".svg")
                        {
                            let arc = std::sync::Arc::new(bytes);
                            let svg = iced::widget::svg::Handle::from_memory(arc.as_ref().clone());
                            ImageState::LoadedSvg {
                                svg,
                                bytes: arc,
                                raster: None,
                            }
                        } else {
                            ImageState::Loaded(iced::widget::image::Handle::from_bytes(bytes))
                        };
                        self.image_cache.insert(url, state);
                        self.trim_image_cache();
                        let assets = self.prime_full_mindmap_preview_assets();
                        Task::batch([
                            assets,
                            self.measure_full_mindmap_preview_heights(),
                            self.measure_window_heights(),
                        ])
                    }
                    Err(_) => {
                        if owns_loading {
                            if let Some(full) = self.full_mindmap.as_mut() {
                                full.preview_failed_images.insert(url.clone());
                            }
                        }
                        self.image_cache.insert(url, ImageState::Failed);
                        self.prime_full_mindmap_preview_assets()
                    }
                }
            }
            Message::FullMindmapPreviewAssetWave {
                identity,
                range,
                wave,
            } => {
                let valid = self.full_mindmap.as_ref().is_some_and(|full| {
                    full.preview_namespace == identity.namespace
                        && full.preview_identity.as_ref() == Some(&identity.request)
                        && matches!(&full.preview, FullMindmapPreview::Document { .. })
                        && full.preview_window.range == range
                        && full.preview_asset_wave_id == wave
                });
                if valid {
                    self.prime_full_mindmap_preview_assets()
                } else {
                    Task::none()
                }
            }
            Message::ImageFetched(url, Ok(bytes)) => {
                // The legacy document fetch message has no request identity.
                // If a Full Mindmap preview owns this URL's pending load,
                // terminal failure, or retained result, let the preview lane
                // remain authoritative; otherwise an old document fetch can
                // overwrite a newer preview operation in the shared cache.
                let preview_pending = self
                    .full_mindmap
                    .as_ref()
                    .is_some_and(|full| full.preview_loading_images.contains_key(&url));
                let preview_failed = self
                    .full_mindmap
                    .as_ref()
                    .is_some_and(|full| full.preview_failed_images.contains(&url));
                let preview_has_result = self.full_mindmap.as_ref().is_some_and(|full| {
                    full.preview_asset_images.contains(&url)
                        && self.image_cache.get(&url).is_some_and(|state| {
                            matches!(state, ImageState::Loaded(_) | ImageState::LoadedSvg { .. })
                        })
                });
                if preview_pending || preview_failed || preview_has_result {
                    return Task::none();
                }
                let state = if is_svg_bytes(&bytes) || url.to_ascii_lowercase().ends_with(".svg") {
                    let arc = std::sync::Arc::new(bytes);
                    let svg = iced::widget::svg::Handle::from_memory(arc.as_ref().clone());
                    ImageState::LoadedSvg {
                        svg,
                        bytes: arc,
                        raster: None,
                    }
                } else {
                    let handle = iced::widget::image::Handle::from_bytes(bytes);
                    ImageState::Loaded(handle)
                };
                self.image_cache.insert(url, state);
                self.trim_image_cache();
                // Loaded image replaces a one-line placeholder — re-measure.
                self.measure_window_heights()
            }
            Message::SvgRasterized(key, Ok(rgba_bytes_w_h)) => {
                let (rgba, w, h) = rgba_bytes_w_h;
                let handle = iced::widget::image::Handle::from_rgba(w, h, rgba);
                if let Some(entry) = self.image_cache.get_mut(&key) {
                    if let ImageState::LoadedSvg { raster, .. } = entry {
                        *raster = Some(handle);
                    }
                    self.image_cache.resync_cost();
                } else {
                    self.image_cache.insert(key, ImageState::Loaded(handle));
                }
                self.trim_image_cache();
                Task::none()
            }
            Message::SvgRasterized(key, Err(_)) => {
                self.image_cache.insert(key, ImageState::Failed);
                Task::none()
            }
            Message::ImageFetched(url, Err(_)) => {
                let preview_pending = self
                    .full_mindmap
                    .as_ref()
                    .is_some_and(|full| full.preview_loading_images.contains_key(&url));
                let preview_failed = self
                    .full_mindmap
                    .as_ref()
                    .is_some_and(|full| full.preview_failed_images.contains(&url));
                let preview_has_result = self.full_mindmap.as_ref().is_some_and(|full| {
                    full.preview_asset_images.contains(&url)
                        && self.image_cache.get(&url).is_some_and(|state| {
                            matches!(state, ImageState::Loaded(_) | ImageState::LoadedSvg { .. })
                        })
                });
                if preview_pending || preview_failed || preview_has_result {
                    return Task::none();
                }
                self.image_cache.insert(url, ImageState::Failed);
                Task::none()
            }
            Message::HintSelection => {
                return self.show_toast("Press ⌘E to edit & select text".into());
            }
            Message::FoldChordStart => {
                self.fold_chord_pending = true;
                return self.show_toast("Fold: press 0-6 …".into());
            }
            Message::FoldChordCancel => {
                self.fold_chord_pending = false;
                Task::none()
            }
            Message::FoldToLevel(n) => {
                self.fold_chord_pending = false;
                // Node-depth folding belongs to the open document, including
                // structured-data Mindmaps. Full Mindmap has independent
                // workspace expansion state and must never mutate the hidden
                // document.
                if self.full_mindmap.is_some() {
                    return Task::none();
                }
                if self.view_mode == ViewMode::Mindmap {
                    let nodes = if self.is_data_doc {
                        let lang = data_lang_for(self.file.as_deref()).unwrap_or("json");
                        let (nodes, size, paths, collapsed) =
                            crate::data_mindmap::build_layout_for_depth(
                                &self.source,
                                lang,
                                self.file.as_deref(),
                                n,
                            );
                        self.mindmap_collapsed = collapsed;
                        self.replace_mindmap_layout(nodes, size, paths)
                    } else {
                        self.mindmap_collapsed = crate::mindmap::collapsed_for_depth(&self.ast, n);
                        self.invalidate_mindmap_layout();
                        self.mindmap_layout().0
                    };
                    let selected_is_visible = self
                        .mindmap_selected
                        .is_some_and(|selected| nodes.iter().any(|node| node.id == Some(selected)));
                    if !selected_is_visible {
                        let next = nodes
                            .first()
                            .and_then(|root| root.children.first())
                            .and_then(|index| nodes.get(*index))
                            .and_then(|node| node.id);
                        self.mindmap_selected = next;
                        self.mindmap_panel_shown =
                            self.mindmap_panel_open.then_some(next).flatten();
                    }
                    return Task::none();
                }
                if self.is_data_doc {
                    return self.show_toast("Fold levels are available in Mindmap mode".into());
                }
                self.folded.clear();
                if n > 0 {
                    for (id, b) in &self.ast {
                        if let Block::Heading { level, .. } = b {
                            if *level as u8 >= n {
                                self.folded.insert(*id);
                            }
                        }
                    }
                }
                self.rebuild_virt_here();
                self.measure_window_heights()
            }
            Message::ToggleFold(id) => {
                if self.folded.contains(&id) {
                    self.folded.remove(&id);
                    self.rebuild_virt_here();
                    return self.measure_window_heights();
                }
                let mut parent_level: Option<u8> = None;
                let mut new_folds: Vec<crate::ast::BlockId> = Vec::new();
                for (bid, b) in &self.ast {
                    if let Block::Heading { level, .. } = b {
                        let lvl = *level as u8;
                        if let Some(pl) = parent_level {
                            if lvl <= pl {
                                break;
                            }
                            new_folds.push(*bid);
                        } else if *bid == id {
                            parent_level = Some(lvl);
                        }
                    }
                }
                if parent_level.is_some() {
                    self.folded.insert(id);
                    for bid in new_folds {
                        self.folded.insert(bid);
                    }
                }
                self.rebuild_virt_here();
                self.measure_window_heights()
            }
            Message::HeadingHoverEnter(id) => {
                self.hovered_heading = Some(id);
                Task::none()
            }
            Message::HeadingHoverExit(id) => {
                if self.hovered_heading == Some(id) {
                    self.hovered_heading = None;
                }
                Task::none()
            }
            Message::FontSizeUp => {
                let size = self.adjust_font_scale(1.1);
                self.height_cache.clear();
                self.rebuild_virt_here();
                let preview_measure = self.refresh_full_mindmap_preview_heights();
                Task::batch([
                    self.measure_window_heights(),
                    preview_measure,
                    self.show_toast(format!("Font {:.0} px", size)),
                ])
            }
            Message::FontSizeDown => {
                let size = self.adjust_font_scale(1.0 / 1.1);
                self.height_cache.clear();
                self.rebuild_virt_here();
                let preview_measure = self.refresh_full_mindmap_preview_heights();
                Task::batch([
                    self.measure_window_heights(),
                    preview_measure,
                    self.show_toast(format!("Font {:.0} px", size)),
                ])
            }
            Message::FontSizeReset => {
                self.font_scale = 1.0;
                self.typography = self.typography_base;
                self.height_cache.clear();
                self.rebuild_virt_here();
                let preview_measure = self.refresh_full_mindmap_preview_heights();
                Task::batch([
                    self.measure_window_heights(),
                    preview_measure,
                    self.show_toast("Font reset".to_string()),
                ])
            }
            Message::MindmapNativePinch(delta) => {
                // The native monitor runs for the whole app. Only forward a
                // finite magnification delta while a graph owns the visible
                // surface; search and overlays retain their normal input.
                if delta.is_finite() && self.overlay == Overlay::None {
                    if self.full_mindmap.is_some() {
                        self.full_mindmap_native_pinch_log += f64::from(delta);
                    } else if self.view_mode == ViewMode::Mindmap && !self.search_open {
                        self.mindmap_native_pinch_log += f64::from(delta);
                    }
                }
                Task::none()
            }
            Message::ToggleFooter => {
                self.show_footer = !self.show_footer;
                self.prefs.show_footer = self.show_footer;
                crate::prefs::save(&self.prefs);
                self.show_toast(
                    if self.show_footer {
                        "Footer shown"
                    } else {
                        "Footer hidden"
                    }
                    .to_string(),
                )
            }
            Message::ToggleViewMode => {
                if self.file.is_none() {
                    return Task::none();
                }
                let checkpoint = self.checkpoint_active_quick_slot();
                match self.view_mode {
                    ViewMode::Raw => Task::batch([checkpoint, self.exit_zen_edit_mode()]),
                    ViewMode::Rendered | ViewMode::Mindmap => {
                        Task::batch([checkpoint, self.enter_zen_edit_mode()])
                    }
                }
            }
            Message::ToggleMindmap => {
                if self.file.is_none() {
                    return Task::none();
                }
                let restore = self.restore_body_scroll();
                match self.view_mode {
                    ViewMode::Mindmap => {
                        self.mindmap_panel_drag = None;
                        self.view_mode = ViewMode::Rendered;
                    }
                    ViewMode::Raw => {
                        self.sync_editor_to_source();
                        self.editor = None;
                        self.edit_history.clear();
                        self.edit_redo.clear();
                        self.restore_zen_chrome();
                        self.view_mode = ViewMode::Mindmap;
                    }
                    ViewMode::Rendered => self.view_mode = ViewMode::Mindmap,
                }
                // On first open (no selection yet), focus root's first child so
                // arrow nav and the preview panel start at the top heading.
                self.mindmap_focus_first_child();
                let checkpoint = self.checkpoint_active_quick_slot();
                Task::batch([checkpoint, restore])
            }
            Message::ToggleFullMindmap => {
                // Manual Full Mindmap navigation supersedes a slot activation
                // that has not completed yet; stale file/preview results must
                // not mutate the newly chosen surface.
                let checkpoint = self.checkpoint_active_quick_slot();
                self.invalidate_pending_quick_slot_restore();
                if self.full_mindmap.is_some() {
                    Task::batch([checkpoint, self.exit_full_mindmap(false)])
                } else {
                    Task::batch([checkpoint, self.enter_full_mindmap()])
                }
            }
            Message::ExitFullMindmap => {
                let checkpoint = self.checkpoint_active_quick_slot();
                self.invalidate_pending_quick_slot_restore();
                Task::batch([checkpoint, self.exit_full_mindmap(false)])
            }
            Message::FullMindmapToggleNode(id) => {
                let checkpoint = self.checkpoint_active_quick_slot();
                self.invalidate_pending_quick_slot_restore();
                let workspace_path = self.full_mindmap_graph().and_then(|graph| {
                    graph.node(&id).and_then(|node| {
                        matches!(
                            node.kind,
                            WorkspaceNodeKind::Root | WorkspaceNodeKind::Folder
                        )
                        .then(|| node.path.clone())
                        .flatten()
                    })
                });
                let mut load = None;
                let mut collapse = None;
                self.reset_full_mindmap_preview_window();
                if let Some(full) = self.full_mindmap.as_mut() {
                    full.deferred_file_selection = None;
                    full.selected = Some(id.clone());
                    full.focus_request = Some(id.clone());
                    full.pending_preview_settle = None;
                    full.pending_preview = None;
                    full.preview = FullMindmapPreview::None;
                    full.load_error = None;
                    if full
                        .pending_workspace_load
                        .as_ref()
                        .is_some_and(|request| !request.preserve_navigation)
                    {
                        full.pending_workspace_load = None;
                    }
                    if let Some(path) = workspace_path.clone() {
                        if full.expanded.contains(&path) {
                            collapse = Some(path);
                        } else {
                            full.expanded.insert(path.clone());
                            load = Some(path);
                        }
                    }
                }
                if !matches!(&id, WorkspaceNodeId::File(_)) {
                    self.clear_active_quick_slot();
                }
                if workspace_path.is_some() {
                    self.cancel_full_mindmap_verification();
                    if load.is_some() {
                        self.bump_full_mindmap_expansion_generation();
                    }
                }
                self.invalidate_full_mindmap_layout();
                if let Some(path) = collapse {
                    self.evict_full_mindmap_folder(&path);
                    // Cancellation clears the old hidden set. Re-snapshot the
                    // remaining expanded frontier now, otherwise unresolved
                    // shells under unrelated expanded parents flash visible
                    // after this branch collapses.
                    Task::batch([checkpoint, self.begin_full_mindmap_verification_wave()])
                } else {
                    let navigation = load.map_or_else(Task::none, |path| {
                        let verification = self.begin_full_mindmap_verification_wave();
                        let branch = self.begin_full_mindmap_folder_load(path);
                        Task::batch([verification, branch])
                    });
                    Task::batch([checkpoint, navigation])
                }
            }
            Message::FullMindmapSelectNode(id) => self.select_full_mindmap_node(id),
            Message::FullMindmapDeselect => {
                let checkpoint = self.checkpoint_active_quick_slot();
                self.invalidate_pending_quick_slot_restore();
                self.reset_full_mindmap_preview_window();
                if let Some(full) = self.full_mindmap.as_mut() {
                    full.selected = None;
                    full.focus_request = None;
                    full.deferred_file_selection = None;
                    full.panel_drag = None;
                    full.pending_preview_settle = None;
                    full.pending_preview = None;
                    full.preview = FullMindmapPreview::None;
                    if full
                        .pending_workspace_load
                        .as_ref()
                        .is_some_and(|request| !request.preserve_navigation)
                    {
                        full.pending_workspace_load = None;
                    }
                }
                self.clear_active_quick_slot();
                self.invalidate_full_mindmap_layout();
                checkpoint
            }
            Message::FullMindmapNavigate(dir) => {
                enum Navigation {
                    Select(WorkspaceNodeId),
                    Dive(WorkspaceNodeId),
                    WorkspaceParent,
                    None,
                }

                let navigation = (|| {
                    let full = self.full_mindmap.as_ref()?;
                    let graph = self.full_mindmap_graph()?;
                    let current = full.selected.clone().unwrap_or_else(|| graph.root_id());
                    let nav = match dir {
                        MindmapDir::Up => graph
                            .sibling(&current, -1)
                            .map(Navigation::Select)
                            .unwrap_or(Navigation::None),
                        MindmapDir::Down => graph
                            .sibling(&current, 1)
                            .map(Navigation::Select)
                            .unwrap_or(Navigation::None),
                        MindmapDir::Left => graph
                            .parent(&current)
                            .map_or_else(|| Navigation::WorkspaceParent, Navigation::Select),
                        MindmapDir::Right => {
                            if graph
                                .node(&current)
                                .is_some_and(|node| node.has_hidden_children)
                            {
                                Navigation::Dive(current)
                            } else {
                                graph
                                    .first_child(&current)
                                    .map(Navigation::Select)
                                    .unwrap_or(Navigation::None)
                            }
                        }
                    };
                    Some(nav)
                })()
                .unwrap_or(Navigation::None);

                match navigation {
                    Navigation::Select(id) => self.update(Message::FullMindmapSelectNode(id)),
                    Navigation::Dive(id) => self.update(Message::FullMindmapDiveWorkspace(id)),
                    Navigation::WorkspaceParent => self.update(Message::FullMindmapWorkspaceParent),
                    Navigation::None => Task::none(),
                }
            }
            Message::FullMindmapDiveWorkspace(id) => {
                let Some(graph) = self.full_mindmap_graph() else {
                    return Task::none();
                };
                let Some(node) = graph.node(&id) else {
                    return Task::none();
                };
                if !matches!(
                    node.kind,
                    WorkspaceNodeKind::Root | WorkspaceNodeKind::Folder
                ) {
                    return Task::none();
                }
                self.invalidate_pending_quick_slot_restore();
                let child = if node.has_hidden_children {
                    if let Some(path) = node.path.clone() {
                        self.cancel_full_mindmap_verification();
                        if let Some(full) = self.full_mindmap.as_mut() {
                            full.expanded.insert(path.clone());
                            full.expansion_generation = full.expansion_generation.wrapping_add(1);
                        }
                        self.invalidate_full_mindmap_layout();
                        let verification = self.begin_full_mindmap_verification_wave();
                        let load = self.begin_full_mindmap_folder_load(path);
                        let select = self
                            .full_mindmap_graph()
                            .and_then(|expanded| expanded.first_child(&id))
                            .map_or_else(Task::none, |child| {
                                self.update(Message::FullMindmapSelectNode(child))
                            });
                        return Task::batch([verification, load, select]);
                    } else {
                        None
                    }
                } else {
                    graph.first_child(&id)
                };
                child.map_or_else(Task::none, |child| {
                    self.update(Message::FullMindmapSelectNode(child))
                })
            }
            Message::FullMindmapActivate => {
                let action = (|| {
                    let full = self.full_mindmap.as_ref()?;
                    let graph = self.full_mindmap_graph()?;
                    let selected = full.selected.clone().unwrap_or_else(|| graph.root_id());
                    if let WorkspaceNodeId::File(path) = &selected {
                        if full.deferred_file_selection.as_ref() == Some(path) {
                            return Some(Message::OpenFileFinderPath(path.clone()));
                        }
                    }
                    let node = graph.node(&selected)?;
                    match node.kind {
                        WorkspaceNodeKind::Root | WorkspaceNodeKind::Folder => {
                            node.path.clone().map(Message::FullMindmapSetRoot)
                        }
                        WorkspaceNodeKind::File => {
                            node.path.clone().map(Message::OpenFileFinderPath)
                        }
                        WorkspaceNodeKind::Empty
                        | WorkspaceNodeKind::Error
                        | WorkspaceNodeKind::Truncated
                        | WorkspaceNodeKind::Loading => None,
                    }
                })();
                action.map_or_else(Task::none, |message| self.update(message))
            }
            Message::FullMindmapToggleSelected => {
                let id = self
                    .full_mindmap
                    .as_ref()
                    .and_then(|full| full.selected.clone());
                let can_toggle = self.full_mindmap_graph().is_some_and(|graph| {
                    id.as_ref().is_some_and(|id| {
                        graph.node(id).is_some_and(|node| {
                            matches!(
                                node.kind,
                                WorkspaceNodeKind::Root | WorkspaceNodeKind::Folder
                            )
                        })
                    })
                });
                if can_toggle {
                    if let Some(id) = id {
                        return self.update(Message::FullMindmapToggleNode(id));
                    }
                }
                Task::none()
            }
            Message::FullMindmapSelectRoot => {
                if let Some(graph) = self.full_mindmap_graph() {
                    return self.update(Message::FullMindmapSelectNode(graph.root_id()));
                }
                Task::none()
            }
            Message::FullMindmapSetRoot(path) => {
                if self.workspace.as_ref() == Some(&path) {
                    return Task::none();
                }
                let exit_after_refresh = self.pending_ipc_file_open.is_some();
                self.begin_full_mindmap_workspace_load(
                    path,
                    true,
                    None,
                    false,
                    false,
                    exit_after_refresh,
                )
            }
            Message::FullMindmapWorkspaceParent => {
                let root = self
                    .workspace
                    .clone()
                    .filter(|_| self.full_mindmap.is_some());
                let Some(root) = root else {
                    return Task::none();
                };
                let Some(parent) = root.parent().map(PathBuf::from) else {
                    return Task::none();
                };
                self.invalidate_pending_quick_slot_restore();
                let exit_after_refresh = self.pending_ipc_file_open.is_some();
                self.begin_full_mindmap_workspace_load(
                    parent,
                    true,
                    None,
                    false,
                    false,
                    exit_after_refresh,
                )
            }
            Message::FullMindmapReturnToFiles => {
                // Files is app-owned navigation away from the active preview;
                // persist its outgoing Full Mindmap context before the exit or
                // hidden-filter refresh takes ownership of the state.
                let checkpoint = self.checkpoint_active_quick_slot();
                self.invalidate_pending_quick_slot_restore();
                Task::batch([checkpoint, self.exit_full_mindmap(true)])
            }
            Message::FullMindmapTogglePanel => {
                if let Some(full) = self.full_mindmap.as_mut() {
                    full.panel_open = !full.panel_open;
                    if !full.panel_open {
                        full.panel_drag = None;
                    }
                }
                Task::none()
            }
            Message::FullMindmapCyclePanelWidth => {
                let window_size = self.window_size;
                if let Some(full) = self.full_mindmap.as_mut() {
                    // This explicit panel-width action reveals the resized result.
                    full.panel_open = true;
                    full.panel_drag = None;
                    full.panel_step = (full.panel_step + 1) % MIND_PANEL_FRACS.len();
                    full.panel_width = mindmap_panel_width_for_step(full.panel_step, window_size);
                }
                self.refresh_full_mindmap_preview_heights()
            }
            Message::FullMindmapPanelDragStart(_) => {
                if let Some(full) = self.full_mindmap.as_mut() {
                    full.panel_drag = Some((full.panel_width, None));
                }
                Task::none()
            }
            Message::FullMindmapPanelDragMove(cursor_x) => {
                if let Some(full) = self.full_mindmap.as_mut() {
                    if let Some((origin, anchor)) = full.panel_drag {
                        match anchor {
                            None => full.panel_drag = Some((origin, Some(cursor_x))),
                            Some(anchor) => {
                                full.panel_width =
                                    mindmap_panel_width_for_drag(origin, anchor, cursor_x);
                            }
                        }
                    }
                }
                self.refresh_full_mindmap_preview_heights()
            }
            Message::FullMindmapPanelDragEnd => {
                if let Some(full) = self.full_mindmap.as_mut() {
                    full.panel_drag = None;
                }
                Task::none()
            }
            Message::FullMindmapFileLoaded { request, result } => {
                let current = self.full_mindmap.as_ref().is_some_and(|full| {
                    full.pending_open
                        .as_ref()
                        .is_some_and(|pending| pending == &request)
                });
                if !current {
                    // A newer request replaced this one, or the user left Full
                    // Mindmap Mode while it was in flight.
                    return Task::none();
                }
                if self.dirty {
                    if let Some(full) = self.full_mindmap.as_mut() {
                        if full.pending_open.as_ref() == Some(&request) {
                            full.pending_open = None;
                        }
                    }
                    self.pending_nav = None;
                    self.invalidate_pending_quick_slot_restore();
                    return self.show_toast(self.unsaved_edits_open_message());
                }
                match result {
                    Err(error) => {
                        if let Some(full) = self.full_mindmap.as_mut() {
                            if full.pending_open.as_ref() == Some(&request) {
                                full.pending_open = None;
                                full.load_error = Some(error);
                            }
                        }
                        Task::none()
                    }
                    Ok((path, source)) if path == request.path => {
                        // Delegate synchronously to the established load path:
                        // the dirty recheck above and this call are one update,
                        // so a late completion cannot discard an intervening
                        // editor change. Clearing the navigator only after the
                        // request was accepted preserves its dirty safety.
                        self.cancel_full_mindmap_verification();
                        self.pending_ipc_file_open = None;
                        self.full_mindmap = None;
                        // Full Mindmap file activation always hands the file
                        // back to the document mindmap, whose normal load path
                        // then focuses the first content child. Let that path
                        // clean up a pre-existing Raw/Zen editor first; its
                        // leave_zen_edit_mode helper restores Rendered, so the
                        // bridge reapplies Mindmap only after delegation.
                        let task = self.update(Message::FileLoaded(Ok((path, source))));
                        self.view_mode = ViewMode::Mindmap;
                        self.mindmap_focus_first_child();
                        task
                    }
                    Ok((path, _)) => {
                        if let Some(full) = self.full_mindmap.as_mut() {
                            if full.pending_open.as_ref() == Some(&request) {
                                full.pending_open = None;
                                full.load_error =
                                    Some(format!("Loaded unexpected file: {}", path.display()));
                            }
                        }
                        Task::none()
                    }
                }
            }
            Message::FullMindmapPreviewSettle { request } => {
                let current = self.full_mindmap.as_ref().is_some_and(|full| {
                    full.pending_preview_settle.as_ref() == Some(&request)
                        && matches!(
                            full.selected.as_ref(),
                            Some(WorkspaceNodeId::File(path)) if path == &request.path
                        )
                });
                if !current {
                    // A newer selection, folder/root/filter change, or mode
                    // exit superseded this timer. No read may begin here.
                    return self.start_full_mindmap_preview_settle_worker();
                }
                if let Some(full) = self.full_mindmap.as_mut() {
                    full.pending_preview_settle = None;
                }
                self.begin_full_mindmap_preview(Some(request.path))
            }
            Message::FullMindmapPreviewLoaded { request, result } => {
                let current = self.full_mindmap.as_ref().is_some_and(|full| {
                    full.pending_preview
                        .as_ref()
                        .is_some_and(|pending| pending == &request)
                });
                if !current {
                    // Selection, folder, workspace, or mode changed while the
                    // read was running. A preview result must never affect the
                    // current document or a newer selection.
                    return Task::none();
                }
                match result {
                    Ok((path, source)) if path == request.path => {
                        // Every Markdown parse/highlight runs away from the
                        // update thread. A short file can still contain
                        // hundreds of blocks; byte size is not a safe proxy
                        // for parser cost or widget-tree size.
                        let Some((epoch, cancel, parse_gate)) =
                            self.full_mindmap.as_ref().map(|full| {
                                (
                                    full.preview_work_epoch.load(Ordering::Acquire),
                                    Arc::clone(&full.preview_work_epoch),
                                    Arc::clone(&full.preview_parse_gate),
                                )
                            })
                        else {
                            return Task::none();
                        };
                        return Task::perform(
                            parse_full_mindmap_preview_guarded(
                                path,
                                Arc::from(source),
                                cancel,
                                epoch,
                                parse_gate,
                            ),
                            move |result| Message::FullMindmapPreviewParsed { request, result },
                        );
                    }
                    Ok((path, _)) => {
                        if let Some(full) = self.full_mindmap.as_mut() {
                            if full.pending_preview.as_ref() == Some(&request) {
                                full.pending_preview = None;
                                full.preview = FullMindmapPreview::Error {
                                    path: request.path,
                                    error: format!(
                                        "Preview loaded unexpected file: {}",
                                        path.display()
                                    ),
                                };
                            }
                        }
                        if self.quick_slot_preview_restore_guard.is_some() {
                            self.invalidate_pending_quick_slot_restore();
                        }
                    }
                    Err(error) => {
                        if error == FULL_MINDMAP_PREVIEW_CANCELLED {
                            // A newer selection already invalidated the
                            // request in normal operation. Keep this guard so
                            // a cooperative worker cancellation can never
                            // replace a current loading state with an error.
                            return Task::none();
                        }
                        if let Some(full) = self.full_mindmap.as_mut() {
                            if full.pending_preview.as_ref() == Some(&request) {
                                full.pending_preview = None;
                                full.preview = FullMindmapPreview::Error {
                                    path: request.path,
                                    error,
                                };
                            }
                        }
                        if self.quick_slot_preview_restore_guard.is_some() {
                            self.invalidate_pending_quick_slot_restore();
                        }
                    }
                }
                Task::none()
            }
            Message::FullMindmapPreviewParsed { request, result } => {
                let current = self
                    .full_mindmap
                    .as_ref()
                    .is_some_and(|full| full.pending_preview.as_ref() == Some(&request));
                if !current {
                    return Task::none();
                }
                match result {
                    Ok(preview) => {
                        if let Some(full) = self.full_mindmap.as_mut() {
                            if full.pending_preview.as_ref() == Some(&request) {
                                full.pending_preview = None;
                                full.preview_shape_ready = false;
                                full.preview = preview;
                            }
                        }
                        // The accepted parse is the first point at which the
                        // full block list exists. Build its own shared
                        // virtual window and measure only that window after
                        // the next layout pass.
                        self.rebuild_full_mindmap_preview_here();
                        let assets = self.prime_full_mindmap_preview_assets();
                        let restore_position = if self
                            .full_mindmap
                            .as_ref()
                            .is_some_and(|full| full.preview_viewport.is_some())
                        {
                            self.take_current_quick_slot_preview_restore()
                        } else {
                            None
                        };
                        let restore = restore_position.map_or_else(Task::none, |position| {
                            self.quick_slot_restore_preview_position(position)
                        });
                        Task::batch([assets, self.measure_full_mindmap_preview_heights(), restore])
                    }
                    Err(error) => {
                        if error == FULL_MINDMAP_PREVIEW_CANCELLED {
                            return Task::none();
                        }
                        if let Some(full) = self.full_mindmap.as_mut() {
                            if full.pending_preview.as_ref() == Some(&request) {
                                full.pending_preview = None;
                                full.preview = FullMindmapPreview::Error {
                                    path: request.path,
                                    error,
                                };
                            }
                        }
                        if self.quick_slot_preview_restore_guard.is_some() {
                            self.invalidate_pending_quick_slot_restore();
                        }
                        Task::none()
                    }
                }
            }
            Message::FullMindmapFolderLoaded { request, result } => {
                let current = self.workspace.as_ref() == Some(&request.workspace_root)
                    && self.show_hidden == request.show_hidden
                    && self.workspace_snapshot_show_hidden == request.show_hidden
                    && self.full_mindmap.as_ref().is_some_and(|full| {
                        full.expanded.contains(&request.folder)
                            && full
                                .pending_folder_loads
                                .get(&request.folder)
                                .is_some_and(|pending| pending == &request)
                    });
                if !current {
                    return Task::none();
                }

                let mut accepted_exact_empty = false;
                let accepted_files = match result {
                    Ok((path, snapshot)) if path == request.folder => {
                        accepted_exact_empty = matches!(
                            snapshot.recursive_supported_file_count,
                            tree::RecursiveFileCount::Exact(0)
                        );
                        let folders = std::sync::Arc::new(snapshot.folders);
                        let files = std::sync::Arc::new(snapshot.files);
                        if let Some(full) = self.full_mindmap.as_mut() {
                            full.pending_folder_loads.remove(&request.folder);
                            full.materialized_folders.insert(
                                request.folder.clone(),
                                workspace_mindmap::MaterializedFolder::Loaded {
                                    folders,
                                    files: std::sync::Arc::clone(&files),
                                    recursive_supported_file_count: snapshot
                                        .recursive_supported_file_count,
                                    truncated: snapshot.truncated,
                                },
                            );
                        }
                        Some(files)
                    }
                    Ok((path, _)) => {
                        if let Some(full) = self.full_mindmap.as_mut() {
                            full.pending_folder_loads.remove(&request.folder);
                            full.materialized_folders.insert(
                                request.folder.clone(),
                                workspace_mindmap::MaterializedFolder::Error(format!(
                                    "Loaded unexpected folder: {}",
                                    path.display()
                                )),
                            );
                        }
                        None
                    }
                    Err(error) => {
                        if let Some(full) = self.full_mindmap.as_mut() {
                            full.pending_folder_loads.remove(&request.folder);
                            full.materialized_folders.insert(
                                request.folder.clone(),
                                workspace_mindmap::MaterializedFolder::Error(error),
                            );
                        }
                        None
                    }
                };
                self.invalidate_full_mindmap_layout();
                if accepted_exact_empty {
                    // A formerly unknown shell is no longer a valid visible
                    // selection once its bounded retry proves it exact-empty.
                    self.normalize_full_mindmap_workspace();
                }
                // The branch snapshot may have introduced a new expanded
                // frontier of LowerBound(0) child shells after the current
                // verification wave froze its denominator. Keep those shells
                // hidden and either queue a follow-up behind the active wave
                // or start a fresh fixed wave immediately when idle.
                let followup = self.schedule_full_mindmap_verification_followup();

                let deferred = self.full_mindmap.as_ref().and_then(|full| {
                    full.deferred_file_selection
                        .as_ref()
                        .filter(|path| path.parent() == Some(request.folder.as_path()))
                        .cloned()
                });
                if let (Some(files), Some(path)) = (accepted_files.as_ref(), deferred) {
                    if files.contains(&path) {
                        if let Some(full) = self.full_mindmap.as_mut() {
                            full.deferred_file_selection = None;
                            let file_id = WorkspaceNodeId::File(path.clone());
                            full.selected = Some(file_id.clone());
                            full.focus_request = Some(file_id);
                        }
                        if self.file.as_ref() == Some(&path) {
                            let source = self.source_snapshot_for_preview();
                            let preview =
                                self.begin_full_mindmap_preview_source(path.clone(), source);
                            return Task::batch([followup, preview]);
                        }
                        return Task::batch([
                            followup,
                            self.begin_full_mindmap_preview(Some(path)),
                        ]);
                    }
                    if let Some(full) = self.full_mindmap.as_mut() {
                        full.deferred_file_selection = None;
                    }
                    self.normalize_full_mindmap_workspace();
                }

                let loading_id = WorkspaceNodeId::Status(
                    request.folder.clone(),
                    workspace_mindmap::WorkspaceStatus::LoadingFiles,
                );
                let selected_loading = self
                    .full_mindmap
                    .as_ref()
                    .is_some_and(|full| full.selected.as_ref() == Some(&loading_id));
                if selected_loading {
                    let parent_id = if self.workspace.as_ref() == Some(&request.folder) {
                        WorkspaceNodeId::Root(request.folder)
                    } else {
                        WorkspaceNodeId::Folder(request.folder)
                    };
                    if let Some(child) = self
                        .full_mindmap_graph()
                        .and_then(|graph| graph.first_child(&parent_id))
                    {
                        return Task::batch([followup, self.select_full_mindmap_node(child)]);
                    }
                    return Task::batch([followup, self.select_full_mindmap_node(parent_id)]);
                }
                followup
            }
            Message::FullMindmapVerificationLoaded { request, result } => {
                self.handle_full_mindmap_verification_loaded(request, result)
            }
            Message::RefreshWorkspaceLoaded { request, result } => {
                self.handle_refresh_workspace_loaded(request, result)
            }
            Message::FullMindmapWorkspaceLoaded { request, result } => {
                let current = self.full_mindmap.as_ref().is_some_and(|full| {
                    full.pending_workspace_load
                        .as_ref()
                        .is_some_and(|pending| pending == &request)
                });
                if !current {
                    // The navigator exited or a newer folder choice superseded
                    // this bounded index while it was running.
                    return Task::none();
                }
                let refresh_id = self
                    .pending_refresh_full_mindmap_workspace
                    .as_ref()
                    .filter(|pending| *pending == &request)
                    .and_then(|_| self.pending_refresh.as_ref().map(|refresh| refresh.id));
                let mut refresh_error = None;
                let open_after = request.open_after.clone();
                let mut followup = Task::none();
                match result {
                    Ok((path, snapshot)) if path == request.path => {
                        if request.preserve_navigation {
                            // Same-workspace refreshes replace only snapshot
                            // data. File preview/open requests and the latest
                            // Full Mindmap navigation remain independently
                            // owned and valid across this completion.
                            self.replace_workspace_snapshot(path.clone(), snapshot);
                            if let Some(full) = self.full_mindmap.as_mut() {
                                full.pending_workspace_load = None;
                            }
                            self.invalidate_full_mindmap_layout();
                            self.normalize_full_mindmap_workspace();
                        } else {
                            self.apply_workspace_snapshot(path.clone(), snapshot, false);
                        }
                        if request.exit_after_refresh {
                            self.cancel_full_mindmap_verification();
                            self.reset_full_mindmap_preview_window();
                            self.full_mindmap = None;
                            if request.return_to_files_after {
                                self.sidebar_open = true;
                                self.sidebar_tab = SidebarTab::Files;
                                self.reveal_current_file();
                            }
                            followup = if self.pending_quick_slot_restore.is_some() {
                                Task::done(Message::QuickSlotRestorePending)
                            } else {
                                self.start_pending_ipc_file_open(self.restore_body_scroll())
                            };
                        } else if !request.preserve_navigation && request.select_root {
                            self.reset_full_mindmap_preview_window();
                            if let Some(full) = self.full_mindmap.as_mut() {
                                let root_id = WorkspaceNodeId::Root(path.clone());
                                full.selected = Some(root_id.clone());
                                full.focus_request = Some(root_id);
                                full.expanded.clear();
                                full.expanded.insert(path);
                                full.pending_open = None;
                                full.pending_preview_settle = None;
                                full.pending_preview = None;
                                full.preview = FullMindmapPreview::None;
                            }
                            self.invalidate_full_mindmap_layout();
                        }
                        if !request.exit_after_refresh {
                            if let Some(file) = open_after.clone() {
                                followup = self.begin_full_mindmap_open(file);
                            }
                        }
                        if !request.exit_after_refresh {
                            let verification = self.begin_full_mindmap_verification_wave();
                            followup = if open_after.is_none() {
                                Task::batch([
                                    followup,
                                    verification,
                                    self.begin_full_mindmap_expanded_folder_loads(),
                                ])
                            } else {
                                Task::batch([followup, verification])
                            };
                        }
                        if !request.exit_after_refresh && self.pending_quick_slot_restore.is_some()
                        {
                            // The accepted snapshot may have replaced the
                            // provisional navigator state. Retry the pending
                            // slot only after that replacement is complete.
                            followup = Task::batch([
                                followup,
                                Task::done(Message::QuickSlotRestorePending),
                            ]);
                        }
                    }
                    Ok((path, _)) => {
                        let message = format!("Indexed unexpected folder: {}", path.display());
                        refresh_error = Some(message.clone());
                        if request.preserve_navigation {
                            self.show_hidden = self.workspace_snapshot_show_hidden;
                        }
                        if request.exit_after_refresh {
                            self.error = Some(message);
                            let cleanup =
                                self.finish_full_mindmap_exit(request.return_to_files_after);
                            followup = self.start_pending_ipc_file_open(cleanup);
                        } else if let Some(full) = self.full_mindmap.as_mut() {
                            full.pending_workspace_load = None;
                            full.load_error = Some(message);
                        }
                        if request.preserve_navigation && !request.exit_after_refresh {
                            followup = self.begin_full_mindmap_expanded_folder_loads();
                        }
                    }
                    Err(error) => {
                        let message = format!("Couldn't index {}: {error}", request.path.display());
                        refresh_error = Some(message.clone());
                        // The existing workspace snapshot is still filtered
                        // under the previous value. Revert the UI preference so
                        // every accepted failure leaves those two facts aligned.
                        if request.preserve_navigation {
                            self.show_hidden = self.workspace_snapshot_show_hidden;
                        }
                        if request.exit_after_refresh {
                            // Exit is terminal user intent. Do not trap the user
                            // in Full Mindmap merely because reconciliation
                            // failed; promote the error before removing its
                            // navigator-local error surface.
                            self.error = Some(message);
                            let cleanup =
                                self.finish_full_mindmap_exit(request.return_to_files_after);
                            followup = self.start_pending_ipc_file_open(cleanup);
                        } else if let Some(full) = self.full_mindmap.as_mut() {
                            full.pending_workspace_load = None;
                            full.load_error = Some(message);
                        }
                        if request.preserve_navigation && !request.exit_after_refresh {
                            followup = self.begin_full_mindmap_expanded_folder_loads();
                        }
                    }
                }
                let refresh_toast = if let Some(refresh_id) = refresh_id {
                    self.pending_refresh_full_mindmap_workspace = None;
                    if let Some(refresh) = self.pending_refresh.as_mut() {
                        if refresh.id == refresh_id {
                            refresh.workspace_done = true;
                            refresh.workspace_error = refresh_error;
                        }
                    }
                    self.finish_refresh()
                } else {
                    Task::none()
                };
                self.invalidate_full_mindmap_layout();
                Task::batch([followup, refresh_toast])
            }
            Message::MindmapToggleNode(id) => {
                self.mindmap_selected = Some(id);
                if self.mindmap_panel_open {
                    self.mindmap_panel_shown = Some(id);
                }
                if self.mindmap_collapsed.contains(&id) {
                    self.mindmap_collapsed.remove(&id);
                } else {
                    self.mindmap_collapsed.insert(id);
                }
                self.invalidate_mindmap_layout();
                Task::none()
            }
            Message::MindmapSelectLeaf(id) => {
                self.mindmap_selected = Some(id);
                self.mindmap_panel_shown = Some(id);
                self.mindmap_panel_open = true;
                Task::none()
            }
            Message::MindmapDeselect => {
                self.mindmap_selected = None;
                self.mindmap_panel_shown = None;
                self.mindmap_panel_open = false;
                self.mindmap_panel_drag = None;
                Task::none()
            }
            Message::MindmapNavigate(dir) => {
                let (nodes, _, _) = self.mindmap_layout();
                // Build parent index.
                let mut parents: Vec<Option<usize>> = vec![None; nodes.len()];
                for (i, n) in nodes.iter().enumerate() {
                    for &c in &n.children {
                        parents[c] = Some(i);
                    }
                }
                // Current index: selected blockid, else first heading.
                let cur = self
                    .mindmap_selected
                    .and_then(|id| nodes.iter().position(|n| n.id == Some(id)))
                    .or_else(|| {
                        // No selection: pick root's first child if any.
                        nodes
                            .first()
                            .and_then(|root| root.children.first().copied())
                    });
                let Some(cur_idx) = cur else {
                    if dir == MindmapDir::Left {
                        return self.enter_full_mindmap_for_current_file();
                    }
                    return Task::none();
                };
                // The document root is represented by an unselectable node
                // (`id == None`). Left at that boundary (including the
                // no-selection/root semantic above) hands navigation to the
                // current file's Full Mindmap workspace.
                if dir == MindmapDir::Left
                    && parents[cur_idx].map_or(true, |parent| nodes[parent].id.is_none())
                {
                    return self.enter_full_mindmap_for_current_file();
                }
                let next_idx: Option<usize> = match dir {
                    MindmapDir::Down | MindmapDir::Up => (|| -> Option<usize> {
                        let parent = parents[cur_idx]?;
                        let kids = &nodes[parent].children;
                        let pos = kids.iter().position(|&i| i == cur_idx)?;
                        match dir {
                            MindmapDir::Down => kids.get(pos + 1).copied(),
                            MindmapDir::Up => {
                                if pos == 0 {
                                    None
                                } else {
                                    Some(kids[pos - 1])
                                }
                            }
                            _ => unreachable!(),
                        }
                    })(),
                    MindmapDir::Left => parents[cur_idx].filter(|&p| nodes[p].id.is_some()),
                    MindmapDir::Right => {
                        let n = &nodes[cur_idx];
                        if !n.children.is_empty() {
                            n.children.first().copied()
                        } else if n.has_hidden_children {
                            // First Right on a collapsed node expands it (and
                            // invalidates the layout cache); we keep using the
                            // pre-expand `nodes` for the rest of this handler
                            // and return None, so the SECOND Right press sees
                            // the rebuilt layout's children and descends.
                            if let Some(id) = n.id {
                                self.mindmap_collapsed.remove(&id);
                                self.invalidate_mindmap_layout();
                            }
                            None
                        } else {
                            None
                        }
                    }
                };
                if let Some(idx) = next_idx {
                    if let Some(id) = nodes[idx].id {
                        self.mindmap_selected = Some(id);
                        self.mindmap_panel_open = true;
                        // Debounce the panel rebuild: the selection ring moves
                        // immediately, but the rendered slice only updates once
                        // navigation pauses, so key-repeat doesn't re-shape the
                        // panel content on every press.
                        self.mindmap_panel_settle_gen =
                            self.mindmap_panel_settle_gen.wrapping_add(1);
                        let settle_gen = self.mindmap_panel_settle_gen;
                        return Task::perform(
                            tokio::time::sleep(std::time::Duration::from_millis(
                                MINDMAP_PANEL_SETTLE_MS,
                            )),
                            move |_| Message::MindmapPanelSettle(settle_gen),
                        );
                    }
                }
                Task::none()
            }
            Message::MindmapPanelSettle(settle_gen) => {
                if settle_gen == self.mindmap_panel_settle_gen {
                    self.mindmap_panel_shown = self.mindmap_selected;
                }
                Task::none()
            }
            Message::ToggleMindmapPanel => {
                self.mindmap_panel_open = !self.mindmap_panel_open;
                if !self.mindmap_panel_open {
                    self.mindmap_panel_drag = None;
                } else {
                    // Re-opening shows the current selection without waiting
                    // for a (possibly never-firing) settle timer.
                    self.mindmap_panel_shown = self.mindmap_selected;
                }
                Task::none()
            }
            Message::WindowResized(id, size) => {
                self.window_size = Some(size);
                // The preview viewport height can change without a scroll
                // event (for example when a window is tiled). Re-window and
                // remeasure its retained layout deterministically.
                let preview_geometry = self.refresh_full_mindmap_preview_heights();
                Task::batch([
                    refresh_window_mode_after_native_transition(id),
                    preview_geometry,
                ])
            }
            Message::WindowUnfocused(id) => {
                self.quick_slots_modifier_held = false;
                self.quick_slots_rail_revealed = false;
                self.quick_slots_modifier_generation =
                    self.quick_slots_modifier_generation.wrapping_add(1);
                refresh_window_mode_after_native_transition(id)
            }
            Message::RefreshWindowMode(id) => {
                // AppKit local monitors must be registered after Iced has
                // created the first NSWindow. Registering during `App::new`
                // leaves the macOS event loop alive without ever creating a
                // visible window on current macOS releases.
                crate::native_pinch::install();
                refresh_window_mode_after_native_transition(id)
            }
            Message::RefreshWindowModeSettled(id) => refresh_window_mode(id),
            Message::WindowModeChanged(mode) => {
                self.window_fullscreen = matches!(mode, iced::window::Mode::Fullscreen);
                Task::none()
            }
            Message::TakeScreenshot => {
                self.overlay = Overlay::None;
                let dir = dirs::desktop_dir()
                    .or_else(dirs::home_dir)
                    .unwrap_or_else(|| std::path::PathBuf::from("."));
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let path = dir.join(format!("rmdv-screenshot-{stamp}.png"));
                self.pending_screenshot = Some((path, None));
                iced::window::latest()
                    .and_then(iced::window::screenshot)
                    .map(Message::ScreenshotCaptured)
            }
            Message::ScreenshotCaptured(shot) => {
                if let Some((path, tx)) = self.pending_screenshot.take() {
                    let saved = image::RgbaImage::from_raw(
                        shot.size.width,
                        shot.size.height,
                        shot.rgba.to_vec(),
                    )
                    .ok_or_else(|| "screenshot buffer size mismatch".to_string())
                    .and_then(|img| {
                        img.save(&path)
                            .map_err(|e| format!("write {}: {e}", path.display()))
                    });
                    match tx {
                        // IPC capture: reply over the socket, no toast.
                        Some(tx) => {
                            let resp = match &saved {
                                Ok(()) => crate::ipc::Response::ok_with(
                                    1,
                                    serde_json::json!({
                                        "path": path.to_string_lossy(),
                                        "width": shot.size.width,
                                        "height": shot.size.height,
                                    }),
                                ),
                                Err(e) => crate::ipc::Response::err(1, e.clone()),
                            };
                            Self::reply(&tx, resp);
                            Task::none()
                        }
                        // Palette capture: surface the result as a toast.
                        None => {
                            let msg = match &saved {
                                Ok(()) => format!(
                                    "Saved screenshot to {}",
                                    path.file_name()
                                        .map(|n| n.to_string_lossy().into_owned())
                                        .unwrap_or_else(|| path.to_string_lossy().into_owned())
                                ),
                                Err(e) => format!("Screenshot failed: {e}"),
                            };
                            self.show_toast(msg)
                        }
                    }
                } else {
                    Task::none()
                }
            }
            Message::MindmapCyclePanelWidth => {
                // Open the panel if it was closed so the size change is visible.
                self.mindmap_panel_open = true;
                self.mindmap_panel_drag = None;
                self.mindmap_panel_step = (self.mindmap_panel_step + 1) % MIND_PANEL_FRACS.len();
                self.mindmap_panel_width =
                    mindmap_panel_width_for_step(self.mindmap_panel_step, self.window_size);
                Task::none()
            }
            Message::MindmapToggleSelected => {
                if let Some(id) = self.mindmap_selected {
                    if self.mindmap_collapsed.contains(&id) {
                        self.mindmap_collapsed.remove(&id);
                    } else {
                        self.mindmap_collapsed.insert(id);
                    }
                    self.invalidate_mindmap_layout();
                }
                Task::none()
            }
            Message::MindmapPanelDragStart(_) => {
                self.mindmap_panel_drag = Some((self.mindmap_panel_width, None));
                Task::none()
            }
            Message::MindmapPanelDragMove(cursor_x) => {
                if let Some((orig_w, anchor)) = self.mindmap_panel_drag {
                    match anchor {
                        None => {
                            self.mindmap_panel_drag = Some((orig_w, Some(cursor_x)));
                        }
                        Some(ax) => {
                            self.mindmap_panel_width =
                                mindmap_panel_width_for_drag(orig_w, ax, cursor_x);
                        }
                    }
                }
                Task::none()
            }
            Message::MindmapPanelDragEnd => {
                self.mindmap_panel_drag = None;
                Task::none()
            }
            Message::ToggleMindmapAutocenter => {
                self.mindmap_autocenter = !self.mindmap_autocenter;
                let label = if self.mindmap_autocenter {
                    "Mindmap auto-center: on"
                } else {
                    "Mindmap auto-center: off"
                };
                self.show_toast(label.into())
            }
            Message::EditorAction(action) => {
                let edits = action.is_edit();
                if edits {
                    self.bump_file_refresh_generation();
                }
                if let Some(ed) = self.editor.as_mut() {
                    if edits {
                        let prev = ed.text();
                        if self.edit_history.push_if_changed(prev) {
                            if self.edit_history.len() > 200 {
                                self.edit_history.drop_oldest();
                            }
                            self.edit_redo.clear();
                        }
                    }
                    ed.perform(action);
                    if edits {
                        self.dirty = ed.text() != self.saved_source;
                    }
                }
                Task::none()
            }
            Message::EditorUndo => {
                let mut changed = false;
                if let Some(ed) = self.editor.as_mut() {
                    if let Some(prev) = self.edit_history.pop() {
                        let current = ed.text();
                        self.edit_redo.push(current);
                        *ed = iced::widget::text_editor::Content::with_text(&prev);
                        self.dirty = prev != self.saved_source;
                        changed = true;
                    }
                }
                if changed {
                    self.bump_file_refresh_generation();
                }
                Task::none()
            }
            Message::EditorRedo => {
                let mut changed = false;
                if let Some(ed) = self.editor.as_mut() {
                    if let Some(next) = self.edit_redo.pop() {
                        let current = ed.text();
                        self.edit_history.push(current);
                        *ed = iced::widget::text_editor::Content::with_text(&next);
                        self.dirty = next != self.saved_source;
                        changed = true;
                    }
                }
                if changed {
                    self.bump_file_refresh_generation();
                }
                Task::none()
            }
            Message::SaveFile => {
                let Some(path) = self.file.clone() else {
                    return Task::none();
                };
                // A write can race a refresh read even when the visible text
                // is unchanged. Advance ownership before dispatching it so a
                // pre-save read cannot be accepted after this save settles.
                self.bump_file_refresh_generation();
                let text = match self.editor.as_ref() {
                    Some(ed) => ed.text(),
                    None => self.source.clone(),
                };
                self.source = text.clone();
                self.reparse_source();
                self.dirty = self.source != self.saved_source;
                let saved_source = text.clone();
                let prime = self.prime_diagram_cache();
                Task::batch([
                    Task::perform(
                        async move {
                            tokio::fs::write(&path, text)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        move |result| Message::FileSaved {
                            result,
                            saved_source,
                        },
                    ),
                    prime,
                ])
            }
            Message::FileSaved {
                result: Ok(()),
                saved_source,
            } => {
                self.bump_file_refresh_generation();
                // An older write may finish after a newer save was queued. Only
                // advance the persisted baseline when this is still the source
                // shown by the app; otherwise the newer write owns the state.
                if self.source == saved_source {
                    self.saved_source = saved_source;
                    let current = self
                        .editor
                        .as_ref()
                        .map(|ed| ed.text())
                        .unwrap_or_else(|| self.source.clone());
                    self.dirty = current != self.saved_source;
                }
                self.show_toast("✓ Saved".into())
            }
            Message::FileSaved {
                result: Err(e),
                saved_source,
            } => {
                self.bump_file_refresh_generation();
                // Keep the guard armed if the failed write is still the active
                // document state. A later save may already have superseded it.
                if self.source == saved_source {
                    self.dirty = true;
                }
                self.error = Some(format!("save failed: {e}"));
                Task::none()
            }
            Message::OpenImageZoom(url) => {
                let raster_task = match self.image_cache.get(&url) {
                    Some(ImageState::LoadedSvg {
                        bytes,
                        raster: None,
                        ..
                    }) => {
                        let key = url.clone();
                        let bytes = bytes.clone();
                        Some(Task::perform(
                            async move { rasterize_svg(&bytes) },
                            move |res| Message::SvgRasterized(key.clone(), res),
                        ))
                    }
                    None if url.to_ascii_lowercase().ends_with(".svg") => {
                        // Local svg path not yet in cache; load+raster.
                        let key = url.clone();
                        let path = std::path::PathBuf::from(&url);
                        self.image_cache.insert(url.clone(), ImageState::Loading);
                        Some(Task::perform(
                            async move {
                                let bytes =
                                    tokio::fs::read(&path).await.map_err(|e| e.to_string())?;
                                rasterize_svg(&bytes)
                            },
                            move |res| Message::SvgRasterized(key.clone(), res),
                        ))
                    }
                    _ => None,
                };
                self.zoom_url = Some(url);
                self.zoom_diagram = None;
                self.overlay = Overlay::ImageZoom;
                let restore = self.restore_body_scroll();
                match raster_task {
                    Some(t) => Task::batch([restore, t]),
                    None => restore,
                }
            }
            Message::PickerNavigate(p) => {
                if let Some(pk) = self.picker.as_mut() {
                    if p.is_dir() {
                        pk.navigate_to(p);
                        self.overlay_selected = 0;
                        // Leaf folder (no subfolders, readable): treat the
                        // navigation as a workspace pick. Saves the user an
                        // extra Space/Enter on dead-end folders.
                        if pk.entries.is_empty() && pk.error.is_none() {
                            let cwd = pk.cwd.clone();
                            self.overlay = Overlay::None;
                            self.picker = None;
                            return Task::done(Message::OpenWorkspace(cwd));
                        }
                    }
                }
                Task::none()
            }
            Message::PickerParent => {
                if let Some(pk) = self.picker.as_mut() {
                    pk.parent();
                    self.overlay_selected = 0;
                }
                Task::none()
            }
            Message::PickerHome => {
                if let Some(home) = Picker::home() {
                    if let Some(pk) = self.picker.as_mut() {
                        pk.navigate_to(home);
                    }
                }
                Task::none()
            }
            Message::PickerSelectFolderHere => {
                if let Some(pk) = &self.picker {
                    let p = pk.cwd.clone();
                    return Task::done(Message::OpenWorkspace(p));
                }
                Task::none()
            }
            Message::PickerOpenFile(path) => {
                self.overlay = Overlay::None;
                self.picker = None;
                self.cancel_refresh_tracking();
                if let Some(blocked) = self.block_file_open_if_dirty() {
                    return blocked;
                }
                self.invalidate_pending_watcher_reload();
                let parent = path.parent().map(|p| p.to_path_buf());
                if self.full_mindmap.is_some() {
                    // Deliberate picker activation supersedes an IPC open that
                    // was waiting for Full Mindmap to exit.
                    self.pending_ipc_file_open = None;
                    // Preserve the picker contract without synchronously
                    // indexing the file's parent on the UI thread. The file
                    // read starts only after that bounded index is ready. The
                    // workspace helper owns the single outgoing Full
                    // Mindmap checkpoint at this transition boundary.
                    if let Some(parent) = parent {
                        return self.begin_full_mindmap_workspace_load(
                            parent,
                            false,
                            Some(path),
                            false,
                            false,
                            false,
                        );
                    }
                    // With no parent directory to index, this wrapper owns
                    // the single checkpoint before its guarded file read.
                    return self.begin_full_mindmap_open(path);
                }
                // The picker owns this file load rather than routing through
                // `load_file_unless_dirty`; checkpoint the outgoing slot
                // before the generic load can replace it.
                let checkpoint = self.checkpoint_active_quick_slot();
                let load = self.begin_generic_file_load(path);
                if let Some(parent) = parent {
                    Task::batch([checkpoint, Task::done(Message::OpenWorkspace(parent)), load])
                } else {
                    Task::batch([checkpoint, load])
                }
            }
            Message::OverlayQueryChanged(q) => {
                self.overlay_query = q;
                self.overlay_selected = 0;
                Task::none()
            }
            Message::OverlayMove(d) => {
                let len = match self.overlay {
                    Overlay::FileFinder => self.filtered_files().len(),
                    Overlay::Command => self.filtered_commands().len(),
                    Overlay::ThemePicker => self.filtered_themes().len(),
                    Overlay::FolderPicker => {
                        self.picker.as_ref().map(|p| p.entries.len()).unwrap_or(0)
                    }
                    Overlay::None | Overlay::ImageZoom | Overlay::Shortcuts => 0,
                };
                if len == 0 {
                    return Task::none();
                }
                let next = (self.overlay_selected as isize + d).clamp(0, len as isize - 1);
                self.overlay_selected = next as usize;
                self.scroll_overlay_to_cursor_with_len(len)
            }
            Message::OverlayConfirm => match self.overlay {
                Overlay::FileFinder => {
                    let files = self.filtered_files();
                    if let Some((p, _, _)) = files.get(self.overlay_selected).cloned() {
                        self.overlay = Overlay::None;
                        return if self.full_mindmap.is_some() {
                            self.begin_full_mindmap_open(p)
                        } else {
                            self.load_file_unless_dirty(p)
                        };
                    }
                    Task::none()
                }
                Overlay::Command => {
                    let cmds = self.filtered_commands();
                    if let Some((_, msg, _)) = cmds.get(self.overlay_selected).cloned() {
                        self.overlay = Overlay::None;
                        return Task::done(msg);
                    }
                    Task::none()
                }
                Overlay::ThemePicker => {
                    let themes = self.filtered_themes();
                    if let Some(t) = themes.get(self.overlay_selected).cloned() {
                        self.overlay = Overlay::None;
                        return Task::done(t.message());
                    }
                    Task::none()
                }
                Overlay::FolderPicker => {
                    if let Some(pk) = self.picker.as_ref() {
                        if let Some(e) = pk.entries.get(self.overlay_selected).cloned() {
                            if e.is_dir {
                                self.overlay = Overlay::None;
                                self.picker = None;
                                return Task::done(Message::OpenWorkspace(e.path));
                            } else if e.is_md {
                                return Task::done(Message::PickerOpenFile(e.path));
                            }
                        }
                    }
                    Task::none()
                }
                Overlay::None | Overlay::ImageZoom | Overlay::Shortcuts => Task::none(),
            },
            Message::OverlayDescend => {
                if self.overlay == Overlay::FolderPicker {
                    if let Some(pk) = self.picker.as_mut() {
                        if let Some(e) = pk.entries.get(self.overlay_selected).cloned() {
                            if e.is_dir {
                                pk.navigate_to(e.path);
                                self.overlay_selected = 0;
                                // Leaf folder: auto-open as workspace.
                                if pk.entries.is_empty() && pk.error.is_none() {
                                    let cwd = pk.cwd.clone();
                                    self.overlay = Overlay::None;
                                    self.picker = None;
                                    return Task::done(Message::OpenWorkspace(cwd));
                                }
                                return self.scroll_overlay_to_cursor();
                            } else if e.is_md {
                                return Task::done(Message::PickerOpenFile(e.path));
                            }
                        } else if pk.entries.is_empty() && pk.error.is_none() {
                            let cwd = pk.cwd.clone();
                            self.overlay = Overlay::None;
                            self.picker = None;
                            return Task::done(Message::OpenWorkspace(cwd));
                        }
                    }
                }
                Task::none()
            }
            Message::FileLoadCompleted { generation, result } => {
                if generation != self.file_refresh_generation {
                    // Refresh, save, edit, or a newer generic load superseded
                    // this read before it reached the update loop.
                    return Task::none();
                }
                self.update(Message::FileLoaded(result))
            }
            Message::RefreshFileLoaded { request, result } => {
                let current = self.pending_refresh_file.as_ref() == Some(&request)
                    && self
                        .pending_refresh
                        .as_ref()
                        .is_some_and(|refresh| refresh.id == request.id)
                    && self.file.as_deref() == Some(request.path.as_path());
                if !current {
                    // A newer navigation or refresh owns the current file.
                    // This completion must not overwrite it.
                    return Task::none();
                }
                self.pending_refresh_file = None;
                let skip_reason = if self.dirty {
                    Some(FileRefreshSkipReason::UnsavedEdits)
                } else if request.generation != self.file_refresh_generation {
                    Some(FileRefreshSkipReason::DocumentChanged)
                } else {
                    None
                };
                if let Some(reason) = skip_reason {
                    if let Some(refresh) = self.pending_refresh.as_mut() {
                        refresh.file_done = true;
                        refresh.file_skip_reason = Some(reason);
                    }
                    return self.finish_refresh();
                }
                match result {
                    Ok((path, source)) if path == request.path => {
                        let load = self.apply_loaded_file(path, source);
                        if let Some(refresh) = self.pending_refresh.as_mut() {
                            refresh.file_done = true;
                        }
                        let toast = self.finish_refresh();
                        Task::batch([load, toast])
                    }
                    Ok((path, _)) => {
                        let error = format!("Loaded unexpected file: {}", path.display());
                        self.error = Some(error.clone());
                        if let Some(refresh) = self.pending_refresh.as_mut() {
                            refresh.file_done = true;
                            refresh.file_error = Some(error);
                        }
                        self.finish_refresh()
                    }
                    Err(error) => {
                        self.error = Some(error.clone());
                        if let Some(refresh) = self.pending_refresh.as_mut() {
                            refresh.file_done = true;
                            refresh.file_error = Some(error);
                        }
                        self.finish_refresh()
                    }
                }
            }
            Message::FileLoaded(Ok((path, src))) => {
                if self.dirty {
                    // An IPC/link/vault open can queue navigation before its
                    // asynchronous read returns. Do not let that stale target
                    // affect the next successful open after this one is blocked.
                    self.pending_nav = None;
                    self.invalidate_pending_quick_slot_restore();
                    return self.show_toast(self.unsaved_edits_open_message());
                }
                // Non-refresh loads supersede a refresh transaction. The
                // refresh-owned path calls `apply_loaded_file` directly after
                // validating its own request and therefore keeps its workspace
                // leg alive.
                self.cancel_refresh_tracking();
                self.apply_loaded_file(path, src)
            }
            Message::FileChanged(p) => {
                self.cancel_refresh_tracking();
                self.bump_file_refresh_generation();
                if self.dirty {
                    return self.show_toast("External change ignored (unsaved edits)".into());
                }
                let p = canonicalize_existing_path(p);
                let Some(request) = self.begin_watcher_reload(p.clone()) else {
                    return Task::none();
                };
                Task::perform(load_file(p), move |result| Message::FileChangedLoaded {
                    request,
                    result,
                })
            }
            Message::FileChangedLoaded { request, result } => {
                if !self.watcher_reload_is_current(&request) {
                    return Task::none();
                }
                self.pending_watcher_reload = None;
                if self.dirty {
                    return self.show_toast("External change ignored (unsaved edits)".into());
                }
                match result {
                    Ok((path, source)) if path == request.path => {
                        self.update(Message::FileLoaded(Ok((path, source))))
                    }
                    Ok(_) => Task::none(),
                    Err(error) => self.update(Message::FileLoaded(Err(error))),
                }
            }
            Message::OpenLink(url) => {
                // Split off a `#fragment` suffix (heading anchor).
                let (target, fragment) = match url.split_once('#') {
                    Some((t, f)) => (t, Some(f)),
                    None => (url.as_str(), None),
                };
                // Bare `#fragment`: navigate within the current document.
                if target.is_empty() {
                    let is_tex = is_tex_path(self.file.as_deref());
                    if let Some(line) =
                        fragment.and_then(|f| line_for_fragment(&self.source, f, is_tex))
                    {
                        return Task::done(goto_line_message(line));
                    }
                    return Task::none();
                }
                // Local markdown file: open it in-app, then navigate to the
                // fragment (if any) once it has loaded.
                if !is_external_link(target) {
                    if let Some(path) = resolve_image_path(target, self.file.as_deref()) {
                        let is_md = path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                            e.eq_ignore_ascii_case("md")
                                || e.eq_ignore_ascii_case("markdown")
                                || e.eq_ignore_ascii_case("tex")
                        });
                        if is_md && path.is_file() {
                            if let Some(blocked) = self.block_file_open_if_dirty() {
                                return blocked;
                            }
                            if let Some(f) = fragment {
                                self.pending_nav = Some(PendingNav {
                                    fragment: Some(f.to_string()),
                                    ..Default::default()
                                });
                            }
                            return Task::done(Message::Open(path));
                        }
                    }
                }
                let _ = open::that_detached(&url);
                Task::none()
            }
            Message::FileLoaded(Err(e)) => {
                self.cancel_refresh_tracking();
                self.pending_nav = None;
                self.error = Some(e);
                Task::none()
            }
            Message::ToggleTheme => {
                let (next_id, label, pal, typo) = self.next_theme();
                self.theme_id = next_id.clone();
                if let theme::ThemeId::Preset(p) = next_id {
                    self.theme_preset = p;
                }
                self.palette = pal;
                if let Some(t) = typo {
                    self.set_typography_base(t);
                }
                let changed = self.refresh_diagram_theme_id();
                let preview_geometry = self.refresh_full_mindmap_preview_heights();
                let toast = self.show_toast(label);
                if changed {
                    let preview_assets = self.refresh_full_mindmap_preview_assets_for_theme();
                    Task::batch([
                        toast,
                        preview_geometry,
                        self.prime_diagram_cache(),
                        preview_assets,
                    ])
                } else {
                    Task::batch([toast, preview_geometry])
                }
            }
            Message::SetTheme(t) => {
                self.theme_preset = t;
                self.palette = theme::palette_for(t);
                self.theme_id = theme::ThemeId::Preset(t);
                let changed = self.refresh_diagram_theme_id();
                let preview_geometry = self.refresh_full_mindmap_preview_heights();
                let toast = self.show_toast(t.label().to_string());
                if changed {
                    let preview_assets = self.refresh_full_mindmap_preview_assets_for_theme();
                    Task::batch([
                        toast,
                        preview_geometry,
                        self.prime_diagram_cache(),
                        preview_assets,
                    ])
                } else {
                    Task::batch([toast, preview_geometry])
                }
            }
            Message::SetCustomTheme(slug) => {
                if let Some(t) = self.custom_themes.iter().find(|t| t.slug == slug) {
                    let (palette, typography, label) = (t.palette, t.typography, t.name.clone());
                    self.palette = palette;
                    self.set_typography_base(typography);
                    self.theme_id = theme::ThemeId::Custom(slug.clone());
                    let changed = self.refresh_diagram_theme_id();
                    let preview_geometry = self.refresh_full_mindmap_preview_heights();
                    let toast = self.show_toast(label);
                    if changed {
                        let preview_assets = self.refresh_full_mindmap_preview_assets_for_theme();
                        Task::batch([
                            toast,
                            preview_geometry,
                            self.prime_diagram_cache(),
                            preview_assets,
                        ])
                    } else {
                        Task::batch([toast, preview_geometry])
                    }
                } else {
                    Task::none()
                }
            }
            Message::ReloadThemes => {
                let mut errs = Vec::new();
                let mut combined = crate::theme_load::bundled().clone();
                combined.extend(crate::theme_load::discover(&mut errs));
                combined.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                self.custom_themes = combined;
                if let theme::ThemeId::Custom(slug) = self.theme_id.clone() {
                    if let Some(t) = self.custom_themes.iter().find(|t| t.slug == slug) {
                        let (palette, typography) = (t.palette, t.typography);
                        self.palette = palette;
                        self.set_typography_base(typography);
                    }
                }
                let n = self.custom_themes.len();
                if !errs.is_empty() {
                    self.error = Some(format!("theme load: {}", errs.join("; ")));
                }
                let changed = self.refresh_diagram_theme_id();
                let preview_geometry = self.refresh_full_mindmap_preview_heights();
                let toast =
                    self.show_toast(format!("{n} custom theme{}", if n == 1 { "" } else { "s" }));
                if changed {
                    let preview_assets = self.refresh_full_mindmap_preview_assets_for_theme();
                    Task::batch([
                        toast,
                        preview_geometry,
                        self.prime_diagram_cache(),
                        preview_assets,
                    ])
                } else {
                    Task::batch([toast, preview_geometry])
                }
            }
            Message::ThemeFilesChanged => {
                let mut errs = Vec::new();
                let before = self.custom_themes.len();
                let mut combined = crate::theme_load::bundled().clone();
                combined.extend(crate::theme_load::discover(&mut errs));
                combined.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                self.custom_themes = combined;
                let after = self.custom_themes.len();
                let active_changed = if let theme::ThemeId::Custom(slug) = self.theme_id.clone() {
                    if let Some(t) = self.custom_themes.iter().find(|t| t.slug == slug) {
                        let (palette, typography) = (t.palette, t.typography);
                        self.palette = palette;
                        self.set_typography_base(typography);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };
                if !errs.is_empty() {
                    self.error = Some(format!("theme load: {}", errs.join("; ")));
                }
                let toast = if active_changed {
                    self.show_toast("theme reloaded".to_string())
                } else if before != after {
                    self.show_toast(format!(
                        "{after} custom theme{}",
                        if after == 1 { "" } else { "s" }
                    ))
                } else {
                    Task::none()
                };
                let preview_geometry = active_changed
                    .then(|| self.refresh_full_mindmap_preview_heights())
                    .unwrap_or_else(Task::none);
                if active_changed && self.refresh_diagram_theme_id() {
                    let preview_assets = self.refresh_full_mindmap_preview_assets_for_theme();
                    Task::batch([
                        toast,
                        preview_geometry,
                        self.prime_diagram_cache(),
                        preview_assets,
                    ])
                } else {
                    Task::batch([toast, preview_geometry])
                }
            }
            Message::OpenThemesDir => match crate::theme_load::ensure_themes_dir() {
                Ok(dir) => match open::that_detached(&dir) {
                    Ok(()) => self.show_toast("opened themes folder".to_string()),
                    Err(e) => {
                        self.error = Some(format!("open themes folder: {e}"));
                        Task::none()
                    }
                },
                Err(e) => {
                    self.error = Some(format!("themes folder: {e}"));
                    Task::none()
                }
            },
            Message::ToastExpire(id) => {
                if let Some(t) = &self.toast {
                    if t.id == id {
                        self.toast = None;
                    }
                }
                Task::none()
            }
            Message::InstallCli => {
                let install =
                    Task::perform(crate::cli_install::install(), Message::CliInstallFinished);
                Task::batch([self.show_toast("Installing rmdv CLI…".to_string()), install])
            }
            Message::CliInstallFinished(result) => match result {
                Ok(()) => {
                    let location = crate::cli_install::install_path()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "the user CLI directory".to_string());
                    self.show_toast(format!("rmdv CLI installed in {location}"))
                }
                Err(error) => self.show_toast_with_action(
                    format!("Could not install rmdv CLI: {error}"),
                    Some(ToastAction {
                        label: "Try Again".to_string(),
                        message: Message::InstallCli,
                    }),
                ),
            },
            Message::UpdateAvailable(ready) => {
                self.pending_update = Some(ready);
                Task::none()
            }
            Message::DismissUpdate => {
                self.pending_update = None;
                Task::none()
            }
            Message::InstallUpdate => {
                if let Some(ready) = &self.pending_update {
                    // apply() relaunches + exits on success; only returns on error.
                    if let Err(e) = crate::update::apply(ready) {
                        self.pending_update = None;
                        return self.show_toast(format!("Update failed: {e}"));
                    }
                }
                Task::none()
            }
            Message::ToggleSidebar => {
                self.sidebar_open = !self.sidebar_open;
                self.restore_body_scroll()
            }
            Message::SetSidebarTab(tab) => {
                self.sidebar_tab = tab;
                Task::none()
            }
            Message::ToggleHidden => {
                let full_active = self.full_mindmap.is_some();
                if !full_active {
                    self.cancel_refresh_tracking();
                }
                self.show_hidden = !self.show_hidden;
                let workspace = self.workspace.clone();
                let pending_workspace = self
                    .full_mindmap
                    .as_ref()
                    .and_then(|full| full.pending_workspace_load.clone());
                // Outside Full Mindmap, preserve the existing sidebar behavior.
                // Full Mindmap refreshes its potentially large workspace only
                // through the stale-safe background loader below.
                if !full_active {
                    if let Some(ws) = workspace.as_ref() {
                        match tree::build_workspace(&ws, self.show_hidden) {
                            Ok(snapshot) => {
                                self.workspace_files = snapshot.files;
                                self.workspace_sidebar_files = snapshot.sidebar_files;
                                self.workspace_tree = Some(snapshot.root);
                                self.workspace_snapshot_show_hidden = self.show_hidden;
                                self.workspace_truncated = snapshot.truncated;
                            }
                            Err(error) => {
                                self.error =
                                    Some(format!("Couldn't refresh {}: {error}", ws.display()));
                            }
                        }
                    }
                }
                // If a picker is open, rebuild its view too.
                if let Some(p) = self.picker.as_mut() {
                    p.show_hidden = self.show_hidden;
                    p.refresh();
                }
                if let Some(full) = self.full_mindmap.as_mut() {
                    full.pending_workspace_load = None;
                }
                self.invalidate_full_mindmap_layout();
                let label = if self.show_hidden {
                    "Hidden files: shown".to_string()
                } else {
                    "Hidden files: hidden".to_string()
                };
                let workspace_refresh = if full_active {
                    if let Some(request) = pending_workspace {
                        self.begin_full_mindmap_workspace_load(
                            request.path,
                            request.select_root,
                            request.open_after,
                            request.preserve_navigation,
                            request.return_to_files_after,
                            request.exit_after_refresh,
                        )
                    } else {
                        workspace.map_or_else(Task::none, |path| {
                            self.begin_full_mindmap_workspace_load(
                                path, false, None, true, false, false,
                            )
                        })
                    }
                } else {
                    Task::none()
                };
                Task::batch([workspace_refresh, self.show_toast(label)])
            }
            Message::TreeToggle(p) => {
                if !self.expanded.remove(&p) {
                    self.expanded.insert(p);
                }
                Task::none()
            }
            Message::TreeMove(d) => {
                let Some(root) = &self.workspace_tree else {
                    return Task::none();
                };
                let len =
                    tree::flatten_with_files(root, &self.workspace_sidebar_files, &self.expanded)
                        .len();
                if len == 0 {
                    return Task::none();
                }
                let len_i = len as isize;
                self.tree_cursor = ((self.tree_cursor as isize + d).rem_euclid(len_i)) as usize;
                self.scroll_tree_to_cursor_with_len(len)
            }
            Message::TreeActivate => {
                let Some(root) = &self.workspace_tree else {
                    return Task::none();
                };
                let rows =
                    tree::flatten_with_files(root, &self.workspace_sidebar_files, &self.expanded);
                let Some(r) = rows.get(self.tree_cursor) else {
                    return Task::none();
                };
                if r.node.is_dir() {
                    let p = r.node.path().to_path_buf();
                    if !self.expanded.remove(&p) {
                        self.expanded.insert(p);
                    }
                    Task::none()
                } else {
                    let p = r.node.path().to_path_buf();
                    self.load_file_unless_dirty(p)
                }
            }
            Message::OutlineMove(d) => {
                let len = self.outline_sections.len();
                if len == 0 {
                    return Task::none();
                }
                let len_i = len as isize;
                self.outline_cursor =
                    ((self.outline_cursor as isize + d).rem_euclid(len_i)) as usize;
                self.scroll_outline_to_cursor()
            }
            Message::OutlineActivate => {
                let Some(s) = self.outline_sections.get(self.outline_cursor) else {
                    return Task::none();
                };
                self.scroll_to_line_top(s.line)
            }
            Message::ScrollToLine(line) => self.scroll_to_line_top(line),
            Message::TreeToggleAtCursor => {
                let Some(root) = &self.workspace_tree else {
                    return Task::none();
                };
                let rows =
                    tree::flatten_with_files(root, &self.workspace_sidebar_files, &self.expanded);
                let Some(r) = rows.get(self.tree_cursor) else {
                    return Task::none();
                };
                if r.node.is_dir() {
                    let p = r.node.path().to_path_buf();
                    if !self.expanded.remove(&p) {
                        self.expanded.insert(p);
                    }
                }
                Task::none()
            }
            Message::CopyTreePath => {
                let Some(root) = &self.workspace_tree else {
                    return Task::none();
                };
                let rows =
                    tree::flatten_with_files(root, &self.workspace_sidebar_files, &self.expanded);
                let Some(r) = rows.get(self.tree_cursor) else {
                    return Task::none();
                };
                let path = r.node.path().display().to_string();
                let toast = self.show_toast("Path copied".into());
                Task::batch([iced::clipboard::write::<Message>(path), toast])
            }
            Message::ScrollBy(dy) => iced::widget::operation::scroll_by(
                Self::scroll_id(),
                iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: dy },
            ),
            Message::ScrollToTop => {
                // Pre-position the window so the jump never lands on a spacer.
                // No measure pass: it would anchor against the pre-jump offset.
                let vh = self.body_viewport_h();
                self.virt_window
                    .rebuild(&self.ast, &self.folded, &self.height_cache, 0.0, vh);
                iced::widget::operation::scroll_to(
                    Self::scroll_id(),
                    iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: 0.0 },
                )
            }
            Message::ScrollToBottom => {
                let vh = self.body_viewport_h();
                self.virt_window
                    .rebuild(&self.ast, &self.folded, &self.height_cache, f32::MAX, vh);
                iced::widget::operation::scroll_to(
                    Self::scroll_id(),
                    iced::widget::scrollable::AbsoluteOffset {
                        x: 0.0,
                        y: f32::MAX,
                    },
                )
            }
            Message::ToggleSearch => {
                self.search_open = !self.search_open;
                if !self.search_open {
                    self.query.clear();
                    self.matches.clear();
                    self.match_idx = 0;
                    self.restore_body_scroll()
                } else {
                    Task::batch([
                        iced::widget::operation::focus(Self::search_input_id()),
                        self.restore_body_scroll(),
                    ])
                }
            }
            Message::QueryChanged(q) => {
                self.query = q;
                self.rebuild_matches();
                self.scroll_to_current_match()
            }
            Message::NextMatch => {
                if !self.matches.is_empty() {
                    self.match_idx = (self.match_idx + 1) % self.matches.len();
                }
                self.scroll_to_current_match()
            }
            Message::PrevMatch => {
                if !self.matches.is_empty() {
                    self.match_idx = (self.match_idx + self.matches.len() - 1) % self.matches.len();
                }
                self.scroll_to_current_match()
            }
            Message::TreeScrolled(v) => {
                self.tree_viewport = Some(v);
                self.last_scroll_at = Some(std::time::Instant::now());
                Task::none()
            }
            Message::OutlineScrolled(v) => {
                self.outline_viewport = Some(v);
                self.last_scroll_at = Some(std::time::Instant::now());
                Task::none()
            }
            Message::OverlayScrolled(v) => {
                self.overlay_viewport = Some(v);
                self.last_scroll_at = Some(std::time::Instant::now());
                Task::none()
            }
            Message::VaultScrolled(v) => {
                self.vault_viewport = Some(v);
                self.last_scroll_at = Some(std::time::Instant::now());
                Task::none()
            }
            Message::FullMindmapPreviewScrolled {
                namespace,
                path,
                identity,
                viewport: v,
            } => {
                let valid_identity = self.full_mindmap.as_ref().is_some_and(|full| {
                    let (expected_namespace, expected_path, expected_id) =
                        full_mindmap_preview_scroll_tag(full);
                    expected_namespace == namespace
                        && expected_id == identity
                        && expected_path == path
                });
                if !valid_identity {
                    // A late scroll event from the previous file/Full Mindmap
                    // instance must not seed this preview's viewport offset.
                    return Task::none();
                }
                let bounds_changed = self.full_mindmap.as_ref().is_some_and(|full| {
                    full.preview_viewport
                        .as_ref()
                        .is_some_and(|previous| previous.bounds().size() != v.bounds().size())
                });
                if bounds_changed {
                    if let Some(full) = self.full_mindmap.as_mut() {
                        full.preview_generation = full.preview_generation.wrapping_add(1);
                        full.preview_height_cache.clear();
                        // A viewport-bounds event is itself a geometry
                        // invalidation; do not let sparse corrections from
                        // the old width/height distort the new prefix.
                        full.preview_window.clear_height_adjustments();
                        full.preview_measurement_pending = false;
                        full.preview_measurement_range = None;
                        full.preview_measurement_generation = None;
                    }
                }
                if let Some(full) = self.full_mindmap.as_mut() {
                    full.preview_viewport = Some(v);
                }
                let preview_restore = self.take_current_quick_slot_preview_restore();
                self.last_scroll_at = Some(std::time::Instant::now());
                let checkpoint = if preview_restore.is_some() {
                    Task::none()
                } else {
                    self.checkpoint_active_quick_slot()
                };
                let needs_rebuild = self.full_mindmap.as_ref().is_some_and(|full| {
                    matches!(full.preview, FullMindmapPreview::Document { .. })
                        && (bounds_changed
                            || full
                                .preview_window
                                .needs_rebuild(Self::full_mindmap_preview_body_offset(full)))
                });
                if !needs_rebuild {
                    let restore = preview_restore.map_or_else(Task::none, |position| {
                        self.quick_slot_restore_preview_position(position)
                    });
                    return Task::batch([checkpoint, restore]);
                }
                self.rebuild_full_mindmap_preview_here();
                let restore = preview_restore.map_or_else(Task::none, |position| {
                    self.quick_slot_restore_preview_position(position)
                });
                Task::batch([
                    checkpoint,
                    self.prime_full_mindmap_preview_assets(),
                    self.measure_full_mindmap_preview_heights(),
                    restore,
                ])
            }
            Message::BodyScrolled(v) => {
                // A width change reflows text, invalidating measured heights;
                // a height change alters the window padding. Either way the
                // window must be rebuilt around the (possibly new) offset.
                let bounds_changed = self
                    .body_viewport
                    .as_ref()
                    .is_some_and(|p| p.bounds().size() != v.bounds().size());
                if bounds_changed {
                    self.height_cache.clear();
                }
                self.body_viewport = Some(v);
                self.last_scroll_at = Some(std::time::Instant::now());
                let checkpoint = if self.quick_slot_body_restore.is_some() {
                    Task::none()
                } else {
                    self.checkpoint_active_quick_slot()
                };
                let restore = self.quick_slot_body_restore.and_then(|position| {
                    let max = self
                        .body_viewport
                        .as_ref()
                        .map(|viewport| {
                            (viewport.content_bounds().height - viewport.bounds().height).max(0.0)
                        })
                        .unwrap_or(0.0);
                    if max > 0.0 {
                        self.quick_slot_body_restore = None;
                        Some(self.quick_slot_restore_position(position))
                    } else {
                        None
                    }
                });
                let offset = self.body_offset();
                let anchor = self.nav_anchor.take();
                if bounds_changed || self.virt_window.needs_rebuild(offset) {
                    // A scroll event during an in-flight nav jump comes from
                    // the estimate snap; keep the target materialized for the
                    // precise scroll op instead of windowing the raw offset.
                    match anchor {
                        Some(idx) => self.rebuild_virt_around_block(idx),
                        None => self.rebuild_virt_here(),
                    }
                    return Task::batch([
                        checkpoint,
                        restore.unwrap_or_else(Task::none),
                        self.measure_window_heights(),
                    ]);
                }
                Task::batch([checkpoint, restore.unwrap_or_else(Task::none)])
            }
            Message::TableScrolled => {
                self.last_scroll_at = Some(std::time::Instant::now());
                Task::none()
            }
            Message::CopyCode(s) => {
                let toast = self.show_toast("Copied".into());
                Task::batch([iced::clipboard::write::<Message>(s), toast])
            }
            Message::DiagramZoom(hash) => {
                // Only zoom Ready diagrams. We have to scan all theme_id keys
                // because cache may hold a stale entry under an old theme_id;
                // we want the one matching the current palette.
                let key = (hash, self.diagram_theme_id);
                let handle = match self.diagram_cache.peek(&key) {
                    Some(crate::diagram::DiagramState::Ready { inline, .. }) => {
                        Some(inline.clone())
                    }
                    _ => None,
                };
                if let Some(h) = handle {
                    self.zoom_diagram = Some(h);
                    self.zoom_url = None;
                    self.overlay = Overlay::ImageZoom;
                    self.restore_body_scroll()
                } else {
                    Task::none()
                }
            }
            Message::CopyDiagramSource(hash) => {
                let src = self.ast.iter().find_map(|(_, b)| match b {
                    Block::Diagram {
                        hash: h, source, ..
                    } if *h == hash => Some(source.clone()),
                    _ => None,
                });
                match src {
                    Some(s) => {
                        let toast = self.show_toast("Copied".into());
                        Task::batch([iced::clipboard::write::<Message>(s), toast])
                    }
                    None => Task::none(),
                }
            }
            Message::DiagramRendered {
                hash,
                theme_id,
                result,
            } => {
                let key = (hash, theme_id);
                // Legacy document diagram completions have no request
                // identity. Keep a Full Mindmap preview's Pending/Ready
                // ownership authoritative for a shared cache key; the
                // preview completion will also refresh its own measurements.
                let preview_pending = self
                    .full_mindmap
                    .as_ref()
                    .is_some_and(|full| full.preview_pending_diagrams.contains_key(&key));
                let preview_has_result = self.full_mindmap.as_ref().is_some_and(|full| {
                    full.preview_asset_diagrams.contains(&key)
                        && self.diagram_cache.peek(&key).is_some_and(|state| {
                            !matches!(state, crate::diagram::DiagramState::Pending)
                        })
                });
                if preview_pending || preview_has_result {
                    return Task::none();
                }
                // Drop stale results — theme changed mid-render, or AST
                // re-parsed away the source block.
                if theme_id != self.diagram_theme_id {
                    return Task::none();
                }
                let still_present = diagram_hash_present(&self.ast, hash);
                if !still_present {
                    return Task::none();
                }
                let state = match result {
                    Ok(out) => {
                        let crate::diagram::RenderOutput { svg, rgba, w, h } = out;
                        let inline = iced::widget::image::Handle::from_rgba(w, h, rgba);
                        crate::diagram::DiagramState::Ready {
                            inline,
                            source_bytes: std::sync::Arc::new(svg),
                            device_w: w,
                        }
                    }
                    Err(msg) => crate::diagram::DiagramState::Err(msg),
                };
                self.diagram_cache.put(key, state);
                // A Ready diagram replaces its faded-source placeholder with
                // an image of a different height — refresh measured heights.
                self.measure_window_heights()
            }
            Message::FullMindmapPreviewDiagramRendered {
                identity,
                range,
                wave,
                hash,
                theme_id,
                result,
            } => {
                let asset_identity = FullMindmapPreviewAssetIdentity {
                    preview: identity.clone(),
                    range,
                    wave,
                };
                let valid = self.full_mindmap.as_ref().is_some_and(|full| {
                    full.preview_namespace == identity.namespace
                        && full.preview_identity.as_ref() == Some(&identity.request)
                        && matches!(
                            &full.preview,
                            FullMindmapPreview::Document { path, .. }
                                if path == &identity.request.path
                        )
                        && full.preview_window.range == range
                        && full.preview_asset_wave_id == wave
                        && self.diagram_theme_id == theme_id
                });
                let key = (hash, theme_id);
                let owns_pending = self
                    .full_mindmap
                    .as_ref()
                    .and_then(|full| full.preview_pending_diagrams.get(&key))
                    .is_some_and(|owner| owner == &asset_identity);
                if owns_pending {
                    if let Some(full) = self.full_mindmap.as_mut() {
                        full.preview_pending_diagrams.remove(&key);
                    }
                }
                if !valid {
                    if owns_pending
                        && matches!(
                            self.diagram_cache.peek(&key),
                            Some(crate::diagram::DiagramState::Pending)
                        )
                    {
                        self.diagram_cache.remove(&key);
                    }
                    return Task::none();
                }
                // A duplicate/current-identity completion that no longer
                // owns the Pending sentinel cannot overwrite a newer Ready
                // or Err result in the shared cache.
                if !owns_pending {
                    return Task::none();
                }
                let state = match result {
                    Ok(out) => {
                        let crate::diagram::RenderOutput { svg, rgba, w, h } = out;
                        crate::diagram::DiagramState::Ready {
                            inline: iced::widget::image::Handle::from_rgba(w, h, rgba),
                            source_bytes: std::sync::Arc::new(svg),
                            device_w: w,
                        }
                    }
                    Err(msg) => crate::diagram::DiagramState::Err(msg),
                };
                self.diagram_cache.put(key, state);
                // A shared cache result can affect both panels. Keep the
                // preview lane's identity check above, then refresh whichever
                // document layout is currently visible as well.
                let assets = self.prime_full_mindmap_preview_assets();
                Task::batch([
                    assets,
                    self.measure_full_mindmap_preview_heights(),
                    self.measure_window_heights(),
                ])
            }
            Message::SidebarDragStart => {
                self.sidebar_drag = Some(self.sidebar_width);
                Task::none()
            }
            Message::SidebarDragMove(x) => {
                if self.sidebar_drag.is_some() {
                    self.sidebar_width = x.clamp(SIDEBAR_MIN, SIDEBAR_MAX);
                }
                Task::none()
            }
            Message::SidebarDragEnd => {
                self.sidebar_drag = None;
                Task::none()
            }
            Message::ScrollerTick => {
                if let Some(t) = self.last_scroll_at {
                    if t.elapsed() >= std::time::Duration::from_millis(SCROLLER_FADE_MS) {
                        self.last_scroll_at = None;
                    }
                }
                Task::none()
            }
            Message::RestoreBodySnap(y) => iced::widget::operation::snap_to(
                Self::scroll_id(),
                iced::widget::scrollable::RelativeOffset { x: 0.0, y },
            ),
            Message::RestoreBodyScroll(y) => iced::widget::operation::scroll_to(
                Self::scroll_id(),
                iced::widget::scrollable::AbsoluteOffset { x: 0.0, y },
            ),
            Message::BlockHeightsMeasured(measured, at_offset) => {
                let body_off = self.body_offset();
                // Compensation is anchored to the offset the measurement was
                // dispatched at; if the viewport moved since (nav jump, user
                // scroll), a scroll_by would fight that movement — skip it.
                let offset_stable = (body_off - at_offset).abs() <= 1.0;
                let (s, e) = self.virt_window.range;
                let mut by_id: HashMap<crate::ast::BlockId, usize> = HashMap::new();
                for (k, &i) in self.virt_window.display
                    [s.min(self.virt_window.display.len())..e.min(self.virt_window.display.len())]
                    .iter()
                    .enumerate()
                {
                    if let Some((bid, _)) = self.ast.get(i) {
                        by_id.insert(*bid, s + k);
                    }
                }
                let mut delta_above = 0.0f32;
                let mut any = false;
                for (bid, h) in measured {
                    let Some(&dpos) = by_id.get(&bid) else {
                        continue;
                    };
                    let old = self.virt_window.block_height(dpos);
                    if (h - old).abs() <= 0.5 {
                        continue;
                    }
                    any = true;
                    // Estimate error in blocks fully above the viewport shifts
                    // everything on screen once corrected; track it so the
                    // offset can be compensated (scroll anchoring).
                    if self.virt_window.block_top(dpos) + old <= body_off {
                        delta_above += h - old;
                    }
                    self.height_cache.set_measured(bid, h);
                }
                if !any {
                    return Task::none();
                }
                self.rebuild_virt_here();
                if offset_stable && delta_above.abs() > 0.5 {
                    return iced::widget::operation::scroll_by(
                        Self::scroll_id(),
                        iced::widget::scrollable::AbsoluteOffset {
                            x: 0.0,
                            y: delta_above,
                        },
                    );
                }
                Task::none()
            }
            Message::FullMindmapPreviewBlockHeightsMeasured {
                path,
                namespace,
                identity,
                generation,
                measured,
                at_offset,
            } => {
                let valid = self.full_mindmap.as_ref().is_some_and(|full| {
                    full.preview_namespace == namespace
                        && full
                            .preview_identity
                            .as_ref()
                            .is_some_and(|request| request.id == identity && request.path == path)
                        && full.preview_generation == generation
                        && matches!(
                            full.selected.as_ref(),
                            Some(WorkspaceNodeId::File(selected)) if selected == &path
                        )
                        && matches!(
                            &full.preview,
                            FullMindmapPreview::Document { path: preview_path, .. }
                                if preview_path == &path
                        )
                });
                if !valid {
                    // Geometry/theme changes invalidate an operation without
                    // invalidating the preview identity. Only the exact
                    // request+generation that owns the coalescing slot may
                    // release it. In particular, an old g1 result cannot
                    // clear a newer g2 operation that has already replaced
                    // the slot.
                    let owns_slot = self.full_mindmap.as_ref().is_some_and(|full| {
                        full.preview_measurement_pending
                            && full.preview_measurement_generation == Some(generation)
                            && full.preview_namespace == namespace
                            && full.preview_identity.as_ref().is_some_and(|request| {
                                request.id == identity && request.path == path
                            })
                    });
                    if owns_slot {
                        if let Some(full) = self.full_mindmap.as_mut() {
                            full.preview_measurement_pending = false;
                            full.preview_measurement_range = None;
                            full.preview_measurement_generation = None;
                        }
                        self.rebuild_full_mindmap_preview_here();
                        return Task::batch([
                            self.prime_full_mindmap_preview_assets(),
                            self.measure_full_mindmap_preview_heights(),
                        ]);
                    }
                    // The panel was reset or a newer file owns the viewport;
                    // this result must not touch either preview state.
                    return Task::none();
                }
                if let Some(full) = self.full_mindmap.as_mut() {
                    full.preview_measurement_pending = false;
                    full.preview_measurement_generation = None;
                }
                let body_off = self
                    .full_mindmap
                    .as_ref()
                    .map(Self::full_mindmap_preview_body_offset)
                    .unwrap_or(0.0);
                let offset_stable = (body_off - at_offset).abs() <= 1.0;
                let mut delta_above = 0.0f32;
                let mut any = false;
                let mut range_changed = false;
                if let Some(full) = self.full_mindmap.as_mut() {
                    range_changed =
                        full.preview_measurement_range.take() != Some(full.preview_window.range);
                    let (start, end) = full.preview_window.range;
                    let display = full.preview_window.display.clone();
                    let blocks = match &full.preview {
                        FullMindmapPreview::Document { blocks, .. } => blocks,
                        _ => return Task::none(),
                    };
                    let mut by_id: HashMap<crate::ast::BlockId, usize> = HashMap::new();
                    for (offset, &idx) in display[start.min(display.len())..end.min(display.len())]
                        .iter()
                        .enumerate()
                    {
                        if let Some((id, _)) = blocks.get(idx) {
                            by_id.insert(*id, start + offset);
                        }
                    }
                    for (id, height) in measured {
                        let Some(&dpos) = by_id.get(&id) else {
                            continue;
                        };
                        if !height.is_finite() || height <= 0.0 {
                            continue;
                        }
                        let old = full.preview_window.block_height(dpos);
                        // Record every finite layout result even when it is
                        // already close to the estimate; otherwise an
                        // accurate estimate is repeatedly redispatched after
                        // each window rebuild.
                        full.preview_height_cache.set_measured(id, height);
                        if (height - old).abs() <= 0.5 {
                            continue;
                        }
                        any = true;
                        if full.preview_window.block_top(dpos) + old <= body_off {
                            delta_above += height - old;
                        }
                        full.preview_window.apply_height_delta(dpos, height - old);
                    }
                }
                if !any {
                    return if range_changed {
                        // The operation completed for an older materialized
                        // range while the viewport moved. Re-measure the
                        // current range once; the pending guard prevents a
                        // completion loop when the widget tree has no IDs.
                        Task::batch([
                            self.prime_full_mindmap_preview_assets(),
                            self.measure_full_mindmap_preview_heights(),
                        ])
                    } else {
                        Task::none()
                    };
                }
                self.rebuild_full_mindmap_preview_here();
                let assets = self.prime_full_mindmap_preview_assets();
                let followup = if range_changed {
                    self.measure_full_mindmap_preview_heights()
                } else {
                    Task::none()
                };
                if offset_stable && delta_above.abs() > 0.5 {
                    let scroll = iced::widget::operation::scroll_by(
                        Self::full_mindmap_preview_scroll_id(),
                        iced::widget::scrollable::AbsoluteOffset {
                            x: 0.0,
                            y: delta_above,
                        },
                    );
                    return Task::batch([assets, scroll, followup]);
                }
                Task::batch([assets, followup])
            }
            Message::Noop => Task::none(),
            Message::ToggleAutoFocusOnNav => {
                self.prefs.auto_focus_on_nav = !self.prefs.auto_focus_on_nav;
                crate::prefs::save(&self.prefs);
                let state = if self.prefs.auto_focus_on_nav {
                    "on"
                } else {
                    "off"
                };
                return self.show_toast(format!("Auto-focus on agent nav: {state}"));
            }
            Message::Ipc(req, tx) => {
                use crate::ipc::{Cmd, FocusBehavior, Mode, Response};
                let id = req.id;
                let mut follow_up: Task<Message> = Task::none();
                // Tracks whether the handler should chain a focus-raise after
                // the response. `Some(true)` = force raise, `Some(false)` =
                // explicit suppress, `None` = not a nav command.
                let mut nav_focus: Option<FocusBehavior> = None;
                // Screenshot replies only after the file is written, so its
                // handler stashes the sender and suppresses the sync reply.
                let mut defer_reply = false;
                let resp = match req.cmd {
                    Cmd::Current => {
                        let mode = match self.view_mode {
                            ViewMode::Rendered => "view",
                            ViewMode::Raw => "edit",
                            ViewMode::Mindmap => "mindmap",
                        };
                        let body = serde_json::json!({
                            "file": self.file.as_ref().map(|p| p.to_string_lossy().into_owned()),
                            "line": current_line_estimate(self),
                            "mode": mode,
                            "folder": self.workspace.as_ref().map(|p| p.to_string_lossy().into_owned()),
                        });
                        Response::ok_with(id, body)
                    }
                    Cmd::Focus => {
                        follow_up =
                            iced::window::latest().and_then(|wid| iced::window::gain_focus(wid));
                        Response::ok(id)
                    }
                    Cmd::Close => {
                        follow_up = iced::window::latest().and_then(|wid| iced::window::close(wid));
                        Response::ok(id)
                    }
                    Cmd::Mode { mode, focus } => {
                        let is_pdf = is_pdf_path(self.file.as_deref());
                        match mode {
                            Mode::View => {
                                if self.view_mode == ViewMode::Raw {
                                    follow_up = self.exit_zen_edit_mode();
                                } else {
                                    self.view_mode = ViewMode::Rendered;
                                }
                            }
                            // PDFs are view-only: coerce edit requests to View.
                            Mode::Edit if is_pdf => self.view_mode = ViewMode::Rendered,
                            Mode::Edit => {
                                follow_up = self.enter_zen_edit_mode();
                            }
                            Mode::Mindmap => {
                                if self.view_mode == ViewMode::Raw {
                                    self.sync_editor_to_source();
                                    self.editor = None;
                                    self.edit_history.clear();
                                    self.edit_redo.clear();
                                    self.restore_zen_chrome();
                                }
                                self.view_mode = ViewMode::Mindmap;
                            }
                        }
                        nav_focus = Some(focus);
                        Response::ok(id)
                    }
                    Cmd::OpenFolder { dir } => {
                        follow_up =
                            Task::done(Message::OpenWorkspace(std::path::PathBuf::from(dir)));
                        Response::ok(id)
                    }
                    Cmd::Reveal { file, focus } => {
                        if self.dirty {
                            Response::err(id, self.unsaved_edits_open_message())
                        } else {
                            follow_up =
                                self.begin_ipc_file_open(std::path::PathBuf::from(file), None);
                            nav_focus = Some(focus);
                            Response::ok(id)
                        }
                    }
                    Cmd::Open {
                        file,
                        line,
                        section,
                        focus,
                    } => {
                        if self.dirty {
                            Response::err(id, self.unsaved_edits_open_message())
                        } else {
                            let path = std::path::PathBuf::from(file);
                            follow_up = self.begin_ipc_file_open(
                                path,
                                Some(PendingNav {
                                    line,
                                    section,
                                    ..Default::default()
                                }),
                            );
                            nav_focus = Some(focus);
                            Response::ok(id)
                        }
                    }
                    Cmd::Goto {
                        line,
                        section,
                        focus,
                    } => {
                        nav_focus = Some(focus);
                        apply_goto(self, id, line, section)
                    }
                    Cmd::Screenshot { path } => {
                        // Capture is async: stash the path + sender, fire the
                        // window screenshot, and reply once the PNG is written.
                        self.pending_screenshot = Some((
                            std::path::PathBuf::from(path),
                            Some(std::sync::Arc::clone(&tx)),
                        ));
                        follow_up = iced::window::latest()
                            .and_then(iced::window::screenshot)
                            .map(Message::ScreenshotCaptured);
                        defer_reply = true;
                        Response::ok(id)
                    }
                    Cmd::Resize { width, height } => {
                        follow_up = iced::window::latest().and_then(move |wid| {
                            iced::window::resize(wid, iced::Size::new(width as f32, height as f32))
                        });
                        Response::ok(id)
                    }
                    Cmd::Theme { slug } => match theme::preset_by_slug(&slug) {
                        Some(preset) => {
                            follow_up = Task::done(Message::SetTheme(preset));
                            Response::ok(id)
                        }
                        None => Response::err(id, format!("unknown theme: {slug}")),
                    },
                    Cmd::DemoBanner { version } => {
                        // Fake a ready update purely to render the banner. The
                        // artifact path is empty, so "Install" would no-op — this
                        // is for demos/screenshots only.
                        self.pending_update = Some(crate::update::ReadyUpdate {
                            version,
                            notes_url: None,
                            artifact: std::path::PathBuf::new(),
                            sha256: String::new(),
                        });
                        Response::ok(id)
                    }
                };
                if !defer_reply {
                    Self::reply(&tx, resp);
                }
                let should_focus = match nav_focus {
                    Some(FocusBehavior::Force) => true,
                    Some(FocusBehavior::Suppress) => false,
                    Some(FocusBehavior::Default) => self.prefs.auto_focus_on_nav,
                    None => false,
                };
                if should_focus {
                    let raise =
                        iced::window::latest().and_then(|wid| iced::window::gain_focus(wid));
                    follow_up = Task::batch([follow_up, raise]);
                }
                return follow_up;
            }
        }
    }

    pub fn subscription(&self) -> iced::Subscription<Message> {
        let dnd = iced::event::listen_with(|ev, _status, _id| match ev {
            iced::Event::Window(iced::window::Event::FileDropped(path)) => {
                Some(Message::Open(path))
            }
            _ => None,
        });
        let watcher = crate::watch::watch_subscription(self.file.clone()).map(Message::FileChanged);
        let theme_watcher =
            crate::theme_watch::watch_subscription().map(|()| Message::ThemeFilesChanged);
        let full_mindmap = self.full_mindmap.is_some();
        let focused = self.search_open && !full_mindmap;
        let overlay_open = self.overlay != Overlay::None;
        let sidebar_open = self.sidebar_open && !full_mindmap;
        let outline_active =
            self.sidebar_open && self.sidebar_tab == SidebarTab::Outline && !full_mindmap;
        let tree_active = self.sidebar_open
            && self.workspace.is_some()
            && self.sidebar_tab == SidebarTab::Files
            && !full_mindmap;
        let editing = self.view_mode == ViewMode::Raw && self.editor.is_some() && !full_mindmap;
        let fold_chord = self.fold_chord_pending && !full_mindmap;
        let mindmap = self.view_mode == ViewMode::Mindmap && !full_mindmap;
        let vault_open = self.vault_open && !full_mindmap;
        let quick_slots_allowed =
            quick_slots_shortcuts_enabled(overlay_open, vault_open, focused, editing, self.dirty);
        let keys = iced::event::listen_with(|ev, status, _id| {
            let is_keyboard = matches!(
                &ev,
                iced::Event::Keyboard(
                    iced::keyboard::Event::KeyPressed { .. }
                        | iced::keyboard::Event::KeyReleased { .. }
                        | iced::keyboard::Event::ModifiersChanged(_)
                )
            );
            if !is_keyboard {
                return None;
            }
            // Always surface keyboard events even when a child widget captured them,
            // so global shortcuts (Cmd+E, Cmd+S, Esc, etc.) still fire while the
            // text_editor is focused. We rely on the sub handler's modifier-aware
            // checks to avoid stealing plain typing keys.
            let _ = status;
            Some(ev)
        })
        .with((
            focused,
            overlay_open,
            tree_active,
            outline_active,
            sidebar_open,
            editing,
            fold_chord,
            mindmap,
            vault_open,
            full_mindmap,
            quick_slots_allowed,
        ))
        .map(
            |(
                (
                    focused,
                    overlay_open,
                    tree_active,
                    outline_active,
                    sidebar_open,
                    editing,
                    fold_chord,
                    mindmap,
                    vault_open,
                    full_mindmap,
                    quick_slots_allowed,
                ),
                ev,
            )| {
                use iced::keyboard::{key::Named, Event as KEv, Key};
                if let iced::Event::Keyboard(KEv::ModifiersChanged(modifiers)) = ev {
                    return Message::QuickSlotsModifier(quick_slots_primary_modifier(modifiers));
                }
                let (key, modified_key, physical, mods, released) = match ev {
                    iced::Event::Keyboard(KEv::KeyPressed {
                        key,
                        modified_key,
                        physical_key,
                        modifiers,
                        ..
                    }) => (key, modified_key, physical_key, modifiers, false),
                    iced::Event::Keyboard(KEv::KeyReleased {
                        key,
                        modified_key,
                        physical_key,
                        modifiers,
                        ..
                    }) => (key, modified_key, physical_key, modifiers, true),
                    _ => return Message::Noop,
                };
                if is_primary_modifier_key(&key) {
                    return Message::QuickSlotsModifier(!released);
                }
                let cmd = mods.command() || mods.control();
                // Quick Slot chords are physical-key based so alternate
                // layouts cannot turn a digit/arrow into a different action.
                // Never steal overlay/editor/vault input.
                if !released {
                    // A pending ⌘K fold chord owns the next key, so ⌘K then
                    // ⌘1 still folds instead of activating slot 1.
                    let surface_allowed = !overlay_open && !vault_open && !focused && !fold_chord;
                    if let Some(message) = quick_slot_physical_message(
                        physical,
                        mods,
                        quick_slots_allowed,
                        editing,
                        surface_allowed,
                    ) {
                        return message;
                    }
                }
                if released {
                    return Message::Noop;
                }
                if reader_font_shortcuts_enabled(
                    full_mindmap,
                    mindmap,
                    overlay_open,
                    focused,
                    fold_chord,
                ) {
                    if let Some(message) = reader_font_size_shortcut(&modified_key, mods) {
                        return message;
                    }
                }
                // Keep the full navigator's activation separate from document
                // ⌘M. Physical matching handles layouts that emit a shifted
                // character differently on macOS.
                if cmd && mods.shift() {
                    use iced::keyboard::key::{Code, Physical};
                    if let Physical::Code(Code::KeyM) = physical {
                        return Message::ToggleFullMindmap;
                    }
                }
                if is_shortcuts_key(&key, physical, mods) {
                    return Message::ToggleShortcuts;
                }
                if is_refresh_key(&key, physical, mods) {
                    return Message::Refresh;
                }
                if is_reveal_file_key(&key, physical, mods) {
                    return Message::RevealFileInFinder;
                }
                if is_copy_file_path_key(&key, physical, mods) {
                    return Message::CopyFilePath;
                }
                // ⌘⌥B: alt+letter on macOS swaps the logical char, so match the
                // physical KeyB code instead of the produced character.
                if cmd && mods.alt() {
                    use iced::keyboard::key::{Code, Physical};
                    if let Physical::Code(Code::KeyB) = physical {
                        return if full_mindmap {
                            Message::FullMindmapTogglePanel
                        } else {
                            Message::ToggleMindmapPanel
                        };
                    }
                    if let Physical::Code(Code::KeyW) = physical {
                        if full_mindmap {
                            return Message::FullMindmapCyclePanelWidth;
                        }
                        if mindmap {
                            return Message::MindmapCyclePanelWidth;
                        }
                    }
                }
                if fold_chord {
                    if let Some(message) = fold_level_shortcut(&key) {
                        return message;
                    }
                    return Message::FoldChordCancel;
                }
                if let Key::Character(c) = &key {
                    // Full Mindmap owns the window but retains the useful
                    // global/fallback commands. Other document-only chords do
                    // nothing while its surface is visible.
                    if full_mindmap {
                        return match c.as_str() {
                            "p" if cmd && mods.shift() => Message::OpenCommandPalette,
                            "P" if cmd => Message::OpenCommandPalette,
                            "p" if cmd => Message::OpenFileFinder,
                            "o" if cmd => Message::OpenFolderPicker,
                            "." if cmd && mods.shift() => Message::ToggleHidden,
                            ">" if cmd => Message::ToggleHidden,
                            "t" if cmd => Message::ToggleTheme,
                            "s" if cmd => Message::SaveFile,
                            _ => Message::Noop,
                        };
                    }
                    // Vault search page owns the screen: only ⌘⇧F (re-open,
                    // idempotent) passes; every other ⌘-shortcut would mutate
                    // state under the page, so swallow it.
                    if vault_open {
                        if (c.as_str() == "f" || c.as_str() == "F") && cmd && mods.shift() {
                            return Message::OpenVaultSearch;
                        }
                        return Message::Noop;
                    }
                    match c.as_str() {
                        "p" if cmd && mods.shift() => return Message::OpenCommandPalette,
                        "P" if cmd => return Message::OpenCommandPalette,
                        "p" if cmd => return Message::OpenFileFinder,
                        "k" if cmd && !editing => return Message::FoldChordStart,
                        "o" if cmd => return Message::OpenFolderPicker,
                        "b" if cmd => return Message::ToggleSidebar,
                        // ⌘⇧. — toggle hidden files. Match both '.' and '>'
                        // since shift+. produces '>' on many layouts.
                        "." if cmd && mods.shift() => return Message::ToggleHidden,
                        ">" if cmd => return Message::ToggleHidden,
                        // ⌘⇧F — vault-wide search. Match both 'f'+shift and the
                        // capital 'F' some layouts emit; ordered before ⌘F.
                        "f" | "F" if cmd && mods.shift() => return Message::OpenVaultSearch,
                        "f" if cmd => return Message::ToggleSearch,
                        "t" if cmd => return Message::ToggleTheme,
                        "e" if cmd => return Message::ToggleViewMode,
                        "m" if cmd => return Message::ToggleMindmap,
                        "c" if cmd && !editing && !overlay_open => return Message::HintSelection,
                        "s" if cmd => return Message::SaveFile,
                        "z" if cmd && editing && mods.shift() => return Message::EditorRedo,
                        "z" if cmd && editing => return Message::EditorUndo,
                        "y" if cmd && editing => return Message::EditorRedo,
                        _ => {}
                    }
                }
                // Vault search page owns Esc/arrows/Enter while open. The query
                // text_input keeps focus but doesn't consume these, so they're
                // handled here at the app key layer (like the overlay did).
                if vault_open {
                    return match key {
                        Key::Named(Named::Escape) => Message::VaultClose,
                        Key::Named(Named::ArrowDown) => Message::VaultMove(1),
                        Key::Named(Named::ArrowUp) => Message::VaultMove(-1),
                        Key::Named(Named::Enter) => Message::VaultEnter,
                        _ => Message::Noop,
                    };
                }
                if matches!(&key, Key::Named(Named::Escape)) {
                    if overlay_open {
                        return Message::CloseOverlay;
                    }
                    if focused {
                        return Message::ToggleSearch;
                    }
                    if editing {
                        return Message::ToggleViewMode;
                    }
                }
                if overlay_open {
                    return match key {
                        Key::Named(Named::ArrowDown) => Message::OverlayMove(1),
                        Key::Named(Named::ArrowUp) => Message::OverlayMove(-1),
                        Key::Named(Named::Enter) => Message::OverlayConfirm,
                        Key::Named(Named::Space) => Message::OverlayDescend,
                        Key::Named(Named::ArrowRight) => Message::OverlayDescend,
                        Key::Named(Named::ArrowLeft) => Message::PickerParent,
                        _ => Message::Noop,
                    };
                }
                if full_mindmap {
                    return match key {
                        Key::Named(Named::Escape) => Message::ExitFullMindmap,
                        Key::Named(Named::ArrowDown) => {
                            Message::FullMindmapNavigate(MindmapDir::Down)
                        }
                        Key::Named(Named::ArrowUp) => Message::FullMindmapNavigate(MindmapDir::Up),
                        Key::Named(Named::ArrowLeft) => {
                            Message::FullMindmapNavigate(MindmapDir::Left)
                        }
                        Key::Named(Named::ArrowRight) => {
                            Message::FullMindmapNavigate(MindmapDir::Right)
                        }
                        Key::Named(Named::Space) => full_mindmap_space_message(),
                        Key::Named(Named::Enter) if cmd => Message::FullMindmapActivate,
                        Key::Named(Named::Enter) => Message::FullMindmapActivate,
                        Key::Named(Named::Home) if cmd => Message::FullMindmapSelectRoot,
                        Key::Named(Named::Home) => Message::FullMindmapSelectRoot,
                        _ => Message::Noop,
                    };
                }
                if focused {
                    if matches!(&key, Key::Named(Named::Enter)) {
                        return if mods.shift() {
                            Message::PrevMatch
                        } else {
                            Message::NextMatch
                        };
                    }
                    return Message::Noop;
                }
                if editing {
                    return Message::Noop;
                }
                let m: Option<Message> = match key {
                    // Sidebar wins arrow keys when open: keyboard file nav
                    // takes priority over mindmap node nav (handled below).
                    Key::Named(Named::ArrowDown)
                        if mindmap && !overlay_open && !tree_active && !outline_active =>
                    {
                        Some(Message::MindmapNavigate(MindmapDir::Down))
                    }
                    Key::Named(Named::ArrowUp)
                        if mindmap && !overlay_open && !tree_active && !outline_active =>
                    {
                        Some(Message::MindmapNavigate(MindmapDir::Up))
                    }
                    Key::Named(Named::ArrowLeft)
                        if mindmap && !overlay_open && !tree_active && !outline_active =>
                    {
                        Some(Message::MindmapNavigate(MindmapDir::Left))
                    }
                    Key::Named(Named::ArrowRight)
                        if mindmap && !overlay_open && !tree_active && !outline_active =>
                    {
                        Some(Message::MindmapNavigate(MindmapDir::Right))
                    }
                    Key::Named(Named::Space)
                        if mindmap && !overlay_open && !tree_active && !outline_active =>
                    {
                        Some(Message::MindmapToggleSelected)
                    }
                    Key::Named(Named::ArrowDown) if tree_active => Some(Message::TreeMove(1)),
                    Key::Named(Named::ArrowUp) if tree_active => Some(Message::TreeMove(-1)),
                    Key::Named(Named::ArrowDown) if outline_active => Some(Message::OutlineMove(1)),
                    Key::Named(Named::ArrowUp) if outline_active => Some(Message::OutlineMove(-1)),
                    Key::Named(Named::ArrowLeft) if sidebar_open => {
                        Some(Message::SetSidebarTab(SidebarTab::Files))
                    }
                    Key::Named(Named::ArrowRight) if sidebar_open => {
                        Some(Message::SetSidebarTab(SidebarTab::Outline))
                    }
                    Key::Named(Named::Enter) if tree_active => Some(Message::TreeActivate),
                    Key::Named(Named::Space) if tree_active => Some(Message::TreeActivate),
                    Key::Named(Named::Enter) if outline_active => Some(Message::OutlineActivate),
                    Key::Named(Named::Space) if outline_active => Some(Message::OutlineActivate),
                    Key::Named(Named::ArrowDown) if mods.command() => Some(Message::ScrollToBottom),
                    Key::Named(Named::ArrowUp) if mods.command() => Some(Message::ScrollToTop),
                    Key::Named(Named::ArrowDown) => Some(Message::ScrollBy(40.0)),
                    Key::Named(Named::ArrowUp) => Some(Message::ScrollBy(-40.0)),
                    Key::Named(Named::Space) if mods.shift() => Some(Message::ScrollBy(-400.0)),
                    Key::Named(Named::Space) => Some(Message::ScrollBy(400.0)),
                    Key::Named(Named::PageDown) => Some(Message::ScrollBy(400.0)),
                    Key::Named(Named::PageUp) => Some(Message::ScrollBy(-400.0)),
                    Key::Named(Named::Home) => Some(Message::ScrollToTop),
                    Key::Named(Named::End) => Some(Message::ScrollToBottom),
                    Key::Character(c) => match c.as_str() {
                        "j" => Some(Message::ScrollBy(40.0)),
                        "k" => Some(Message::ScrollBy(-40.0)),
                        "g" => Some(Message::ScrollToTop),
                        "G" => Some(Message::ScrollToBottom),
                        _ => None,
                    },
                    _ => None,
                };
                m.unwrap_or(Message::Noop)
            },
        );
        let scroller = if self.last_scroll_at.is_some() {
            iced::time::every(std::time::Duration::from_millis(150)).map(|_| Message::ScrollerTick)
        } else {
            iced::Subscription::none()
        };
        let drag = if self.sidebar_drag.is_some() && !full_mindmap {
            iced::event::listen_with(|ev, _status, _id| match ev {
                iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                    Some(Message::SidebarDragMove(position.x))
                }
                iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                    iced::mouse::Button::Left,
                )) => Some(Message::SidebarDragEnd),
                _ => None,
            })
        } else {
            iced::Subscription::none()
        };
        let mind_drag = if !full_mindmap
            && self.view_mode == ViewMode::Mindmap
            && self.mindmap_panel_drag.is_some()
        {
            iced::event::listen_with(|ev, _status, _id| match ev {
                iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                    Some(Message::MindmapPanelDragMove(position.x))
                }
                iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                    iced::mouse::Button::Left,
                )) => Some(Message::MindmapPanelDragEnd),
                _ => None,
            })
        } else {
            iced::Subscription::none()
        };
        let full_mind_drag = if self
            .full_mindmap
            .as_ref()
            .is_some_and(|full| full.panel_drag.is_some())
        {
            iced::event::listen_with(|ev, _status, _id| match ev {
                iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                    Some(Message::FullMindmapPanelDragMove(position.x))
                }
                iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                    iced::mouse::Button::Left,
                )) => Some(Message::FullMindmapPanelDragEnd),
                _ => None,
            })
        } else {
            iced::Subscription::none()
        };
        let ipc = iced::Subscription::run(ipc_subscription_stream);
        let native_pinch = crate::native_pinch::subscription().map(Message::MindmapNativePinch);
        let macos_open = crate::macos_open::subscription().map(Message::OpenFileFinderPath);
        let window_events = iced::window::events().filter_map(|(id, event)| match event {
            iced::window::Event::Unfocused => Some(Message::WindowUnfocused(id)),
            iced::window::Event::Opened { .. }
            | iced::window::Event::Focused
            | iced::window::Event::Moved(_)
            | iced::window::Event::Rescaled(_) => Some(Message::RefreshWindowMode(id)),
            iced::window::Event::Resized(size) => Some(Message::WindowResized(id, size)),
            _ => None,
        });
        iced::Subscription::batch([
            dnd,
            watcher,
            theme_watcher,
            keys,
            scroller,
            drag,
            mind_drag,
            full_mind_drag,
            window_events,
            ipc,
            native_pinch,
            macos_open,
        ])
    }

    pub fn view(&self) -> Element<'_, Message> {
        {
            use std::sync::OnceLock;
            // Print first_view BEFORE the font-load block so the timing reflects
            // when the window can actually paint (font load runs lazily after).
            static BENCH: OnceLock<bool> = OnceLock::new();
            if *BENCH.get_or_init(|| std::env::var_os("RMDV_BENCH_STARTUP").is_some()) {
                static FIRST: OnceLock<()> = OnceLock::new();
                FIRST.get_or_init(|| {
                    if let Some(d) = crate::bench::since_process_start() {
                        eprintln!("startup: first_view={:?}", d);
                    }
                });
            }
            // Deferred from main(): first view pays ~270ms font scan instead of blocking window paint.
            static FONTS_LOADED: OnceLock<()> = OnceLock::new();
            FONTS_LOADED.get_or_init(|| {
                let fs = iced::advanced::graphics::text::font_system();
                if let Ok(mut guard) = fs.write() {
                    guard.raw().db_mut().load_system_fonts();
                }
                if std::env::var_os("RMDV_BENCH_STARTUP").is_some() {
                    if let Some(d) = crate::bench::since_process_start() {
                        eprintln!("startup: fonts_loaded={:?}", d);
                    }
                }
            });
        }
        let pal = self.palette;
        let recently_scrolled = self
            .last_scroll_at
            .is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(SCROLLER_FADE_MS));
        let full_mindmap = self.full_mindmap.is_some();
        let footer_visible = !full_mindmap
            && self.show_footer
            && self.file.is_some()
            && self.view_mode != ViewMode::Mindmap;

        let reader: Element<'_, Message> = if full_mindmap {
            self.full_mindmap_view(pal, recently_scrolled)
        } else if self.vault_open {
            // Workspace-level page — renders before the file/welcome checks so
            // it works with no document open.
            vault_search_page(
                &self.vault_query,
                self.vault_searched_query.as_deref(),
                &self.vault_results,
                self.vault_file_count,
                self.vault_cursor,
                self.vault_truncated,
                &self.vault_collapsed,
                self.workspace.as_deref(),
                self.vault_viewport.as_ref(),
                pal,
            )
        } else if let Some(err) = &self.error {
            centered_card(
                column![
                    text("Couldn't open file").size(20).color(pal.fg),
                    text(err.clone()).color(pal.muted).size(13),
                    Space::new().height(8),
                    primary_button("Open Folder", pal).on_press(Message::OpenFolderPicker),
                ]
                .spacing(10)
                .align_x(iced::Alignment::Center)
                .into(),
                pal,
            )
        } else if self.file.is_none() {
            welcome_view(pal)
        } else {
            let hl = Highlight {
                query: self.query.clone(),
                current_block: self.matches.get(self.match_idx).map(|m| m.block),
                current_in_block: self
                    .matches
                    .get(self.match_idx)
                    .map(|m| m.in_block)
                    .unwrap_or(0),
            };
            let body: Element<'_, Message> = if self.view_mode == ViewMode::Mindmap {
                let (nodes, content_size, _) = self.mindmap_layout();
                let program = crate::mindmap::MindmapProgram {
                    nodes,
                    content_size,
                    palette: pal,
                    selected: self.mindmap_selected,
                    // Document Mindmap keeps its historical selection-driven
                    // animated positioning. Full Mindmap passes a separate
                    // explicit focus request above.
                    focus: None,
                    panel_open: self.mindmap_panel_open,
                    panel_width: self.mindmap_panel_width,
                    autocenter: self.mindmap_autocenter,
                    layout_generation: Some(self.mindmap_layout_generation.get()),
                    keyboard_zoom_enabled: self.overlay == Overlay::None
                        && !self.search_open
                        && !self.fold_chord_pending,
                    native_pinch_log: self.mindmap_native_pinch_log,
                    on_toggle: Box::new(Message::MindmapToggleNode),
                    on_select: Box::new(Message::MindmapSelectLeaf),
                    on_deselect: Message::MindmapDeselect,
                };
                let canvas_el: Element<'_, Message> = iced::widget::canvas(program)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into();
                let canvas_with_hint: Element<'_, Message> = stack![
                    canvas_el,
                    floating_mindmap_hint(
                        &[("←↑→↓", "move"), ("Space", "fold"), ("= / −", "zoom"),],
                        pal,
                    ),
                ]
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
                if self.mindmap_panel_open {
                    let panel = self.mindmap_panel_view(
                        &pal,
                        &hl,
                        recently_scrolled,
                        self.mindmap_panel_width,
                    );
                    let handle = mindmap_panel_resize_handle(pal);
                    irow![canvas_with_hint, handle, panel].into()
                } else {
                    canvas_with_hint
                }
            } else if self.view_mode == ViewMode::Raw {
                if let Some(ed) = self.editor.as_ref() {
                    let editor = iced::widget::text_editor(ed)
                        .on_action(Message::EditorAction)
                        // Filter cmd/ctrl combos so global shortcuts (⌘B, ⌘T,
                        // ⌘E, ⌘K, ⌘M, ⌘P, ⌘O, etc.) don't ALSO get inserted
                        // as text by the editor. Keep ⌘C/⌘X/⌘V/⌘A/⌘Z/⌘Y for
                        // standard editor bindings — those have explicit
                        // handlers upstream that we want to preserve.
                        .key_binding(editor_key_binding)
                        .font(editor_font())
                        .size(self.typography.code_size)
                        .line_height(iced::widget::text::LineHeight::Relative(1.55))
                        .height(Length::Fill)
                        .padding(iced::Padding {
                            top: 0.0,
                            right: 32.0,
                            bottom: zen_editor_bottom_inset(footer_visible),
                            left: 64.0,
                        })
                        .highlight_with::<crate::md_highlight::MdHighlighter>(
                            crate::md_highlight::Settings { palette: pal },
                            |hl, _theme| hl.to_format(),
                        )
                        .style(move |_, _| iced::widget::text_editor::Style {
                            background: pal.bg.into(),
                            border: Border {
                                color: iced::Color::TRANSPARENT,
                                width: 0.0,
                                radius: 0.0.into(),
                            },
                            placeholder: pal.subtle,
                            value: pal.fg,
                            selection: pal.selection,
                        })
                        .height(Length::Fill);
                    container(
                        container(editor)
                            .width(Length::Fill)
                            .height(Length::Fill)
                            .max_width(READING_MAX),
                    )
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .center_x(Length::Fill)
                    .into()
                } else {
                    let fallback = text(self.source.as_str())
                        .font(iced::Font::MONOSPACE)
                        .size(self.typography.code_size)
                        .color(pal.fg);
                    container(
                        container(fallback)
                            .width(Length::Fill)
                            .max_width(READING_MAX),
                    )
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .center_x(Length::Fill)
                    .into()
                }
            } else if self.is_data_doc {
                if let Some((_, Block::CodeBlock { code, spans, .. })) = self.ast.first() {
                    crate::render::data_view(code, spans, &pal, &self.typography)
                } else {
                    crate::render::render(
                        &self.ast,
                        &pal,
                        &self.typography,
                        &hl,
                        Some(&self.virt_window),
                        &self.image_cache,
                        self.file.as_deref(),
                        &self.folded,
                        self.hovered_heading,
                        &self.diagram_cache,
                        self.diagram_theme_id,
                        true,
                        (0, 0),
                        recently_scrolled,
                    )
                }
            } else {
                crate::render::render(
                    &self.ast,
                    &pal,
                    &self.typography,
                    &hl,
                    Some(&self.virt_window),
                    &self.image_cache,
                    self.file.as_deref(),
                    &self.folded,
                    self.hovered_heading,
                    &self.diagram_cache,
                    self.diagram_theme_id,
                    true,
                    (0, 0),
                    recently_scrolled,
                )
            };
            if self.view_mode == ViewMode::Raw || self.view_mode == ViewMode::Mindmap {
                container(body)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(move |_| container::Style {
                        background: Some(pal.bg.into()),
                        ..Default::default()
                    })
                    .into()
            } else {
                scrollable(
                    container(container(body).max_width(READING_MAX).width(Length::Fill))
                        .padding(Padding::from([56, 32]))
                        .center_x(Length::Fill)
                        .width(Length::Fill),
                )
                .id(Self::scroll_id())
                .height(Length::Fill)
                .on_scroll(Message::BodyScrolled)
                .direction(slim_scroll_direction())
                .style(move |_, status| sleek_scrollable_style(status, pal, recently_scrolled))
                .into()
            }
        };

        let reader_with_search: Element<'_, Message> = if self.search_open && !full_mindmap {
            column![
                search_bar_view(&self.query, &self.matches, self.match_idx, pal),
                reader,
            ]
            .into()
        } else {
            reader.into()
        };

        let main_area: Element<'_, Message> =
            if !full_mindmap && self.sidebar_open && self.workspace.is_some() {
                // View panel paints its own rounded background. iced 0.14 doesn't
                // mask child draws to the radius, but background fill does respect
                // it — so the corner pixels outside the radius are transparent and
                // show the sidebar-colored area behind. Reader content has enough
                // padding that no text falls into the corner curve.
                irow![
                    sidebar_view(self, pal),
                    sidebar_resize_handle(pal),
                    container(reader_with_search)
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .style(move |_| container::Style {
                            background: Some(pal.bg.into()),
                            border: Border {
                                color: Color::TRANSPARENT,
                                width: 0.0,
                                radius: iced::border::top_left(24),
                            },
                            ..Default::default()
                        }),
                ]
                .into()
            } else {
                container(reader_with_search)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            };

        // When the sidebar is open, the area "behind" the view panel's rounded
        // top-left corner needs to look like sidebar, so the cutout pixels
        // outside the reader's rounded background pick up sidebar color.
        let main_bg = if !full_mindmap && self.sidebar_open && self.workspace.is_some() {
            pal.sidebar
        } else {
            pal.bg
        };
        let main = container(main_area)
            .style(move |_| container::Style {
                background: Some(main_bg.into()),
                ..Default::default()
            })
            .width(Length::Fill)
            .height(Length::Fill);

        let overlay_layer: Element<'_, Message> = match self.overlay {
            Overlay::None => Space::new().into(),
            Overlay::FolderPicker => {
                folder_picker_overlay(self.picker.as_ref(), self.overlay_selected, pal)
            }
            Overlay::FileFinder => {
                let files = self.filtered_files();
                file_finder_overlay(&self.overlay_query, files, self.overlay_selected, pal)
            }
            Overlay::Command => {
                let cmds = self.filtered_commands();
                command_overlay(&self.overlay_query, cmds, self.overlay_selected, pal)
            }
            Overlay::ThemePicker => {
                let themes = self.filtered_themes();
                theme_overlay(
                    &self.overlay_query,
                    themes,
                    self.overlay_selected,
                    self.theme_id.clone(),
                    pal,
                )
            }
            Overlay::Shortcuts => shortcuts_overlay(pal),
            Overlay::ImageZoom => image_zoom_overlay(
                self.zoom_url.as_deref(),
                self.zoom_diagram.as_ref(),
                &self.image_cache,
                pal,
            ),
        };
        // Status footer floats over the reader (content scrolls behind it),
        // pinned bottom-right. Shown for any open document except mindmap.
        let footer_layer: Element<'_, Message> = if footer_visible {
            status_footer(self.source_words, pal)
        } else {
            Space::new().into()
        };
        // Floating cheatsheet button, bottom-right of the active reader or mindmap.
        // Sits just above the word-count pill when the footer is visible; drops to
        // the corner when it's off. Hidden only while an overlay is open.
        let kb_button_layer: Element<'_, Message> = if self.overlay == Overlay::None {
            let bottom_pad = if footer_visible {
                KEYBOARD_BUTTON_FOOTER_BOTTOM_PAD
            } else {
                KEYBOARD_BUTTON_BOTTOM_PAD
            };
            container(iced::widget::tooltip(
                ghost_lu(ic::KEYBOARD, pal).on_press(Message::ToggleShortcuts),
                container(text("Keyboard shortcuts  ⌘/").size(12).color(pal.fg))
                    .padding(Padding::from([4, 8]))
                    .style(move |_| container::Style {
                        background: Some(pal.surface.into()),
                        border: Border {
                            color: pal.rule,
                            width: 1.0,
                            radius: 6.0.into(),
                        },
                        ..Default::default()
                    }),
                iced::widget::tooltip::Position::Left,
            ))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding {
                top: 0.0,
                right: 12.0,
                bottom: bottom_pad,
                left: 0.0,
            })
            .align_x(iced::alignment::Horizontal::Right)
            .align_y(iced::alignment::Vertical::Bottom)
            .into()
        } else {
            Space::new().into()
        };
        let quick_slots_layer: Element<'_, Message> = if quick_slots_rail_visible(
            self.quick_slots_rail_revealed,
            self.overlay != Overlay::None,
            self.vault_open,
            self.search_open,
            self.view_mode == ViewMode::Raw && self.editor.is_some(),
            self.dirty,
        ) {
            quick_slots_rail(
                self,
                pal,
                quick_slots_rail_left_offset(
                    self.sidebar_open && self.workspace.is_some(),
                    self.sidebar_width,
                    full_mindmap,
                ),
            )
        } else {
            Space::new().into()
        };
        let base: Element<'_, Message> = iced::widget::stack![
            Element::from(main),
            footer_layer,
            kb_button_layer,
            quick_slots_layer,
            overlay_layer
        ]
        .into();
        let toast_layer: Element<'_, Message> = match &self.toast {
            Some(t) => toast_overlay(t, pal),
            None => Space::new().into(),
        };
        // Progress has its own neutral layer and never replaces the ordinary
        // toast. The stack order keeps attention/error feedback above this
        // persistent status while preserving each toast's expiry timing.
        let progress_layer: Element<'_, Message> = match &self.full_mindmap_progress {
            Some(progress) if self.full_mindmap.is_some() => {
                full_mindmap_progress_overlay(progress, pal)
            }
            _ => Space::new().into(),
        };
        let update_layer: Element<'_, Message> = match &self.pending_update {
            Some(u) => update_banner(&u.version, pal),
            None => Space::new().into(),
        };
        iced::widget::stack![base, progress_layer, toast_layer, update_layer].into()
    }
}

/// Bottom-center banner inviting the user to install a downloaded update.
fn update_banner<'a>(version: &str, pal: Palette) -> Element<'a, Message> {
    use iced::widget::{button, container, row, text as text_w};
    // A small accent dot signals "something new", matching the warm accent the
    // rest of the UI uses for its single highlight color.
    let dot = container(Space::new().width(7).height(7)).style(move |_| container::Style {
        background: Some(pal.accent.into()),
        border: Border {
            radius: 999.0.into(),
            ..Default::default()
        },
        ..Default::default()
    });
    let label = text_w(format!("rmdv {version} ready to install"))
        .size(13.0)
        .color(pal.fg);
    // Primary action uses the shared accent-pill button; "Later" is a quiet
    // ghost so the two read as primary/secondary, not two competing buttons.
    let install = primary_button("Install & Restart", pal).on_press(Message::InstallUpdate);
    let later = button(text_w("Later").size(13.0).color(pal.muted))
        .padding(Padding::from([8, 14]))
        .style(move |_, status| button::Style {
            background: match status {
                button::Status::Hovered | button::Status::Pressed => {
                    Some(Background::Color(pal.surface_alt))
                }
                _ => None,
            },
            text_color: pal.muted,
            border: Border {
                color: pal.rule,
                width: 1.0,
                radius: 999.0.into(),
            },
            ..Default::default()
        })
        .on_press(Message::DismissUpdate);
    let bar = container(
        row![dot, label, Space::new().width(8), install, later]
            .spacing(10)
            .align_y(iced::alignment::Vertical::Center),
    )
    .padding(Padding::from([8, 10]))
    .style(move |_| container::Style {
        background: Some(pal.surface.into()),
        border: iced::Border {
            color: pal.rule,
            width: 1.0,
            radius: 12.0.into(),
        },
        text_color: Some(pal.fg),
        ..Default::default()
    });
    container(bar)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(iced::Padding {
            bottom: 24.0,
            ..iced::Padding::ZERO
        })
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Bottom)
        .into()
}

/// Bottom status bar: word count + estimated reading time (~200 wpm).
fn status_footer<'a>(words: usize, pal: Palette) -> Element<'a, Message> {
    use iced::widget::{container, text as text_w};
    let minutes = ((words as f32) / 200.0).ceil().max(1.0) as usize;
    let label = format!(
        "{} word{} · {} min read",
        words,
        if words == 1 { "" } else { "s" },
        minutes
    );
    // Translucent pill so document content remains visible scrolling behind it.
    let mut pill_bg = pal.bg;
    pill_bg.a = 0.82;
    let pill = container(text_w(label).size(12.0).color(pal.muted))
        .padding([4, 12])
        .style(move |_| container::Style {
            background: Some(pill_bg.into()),
            border: iced::Border {
                color: pal.rule,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        });
    // Float bottom-right over the reader; content scrolls underneath.
    container(pill)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([10, 14])
        .align_x(iced::alignment::Horizontal::Right)
        .align_y(iced::alignment::Vertical::Bottom)
        .into()
}

fn toast_overlay<'a>(toast: &Toast, pal: Palette) -> Element<'a, Message> {
    use iced::widget::{button, container, text as text_w};
    let mut content = irow![text_w(toast.text.clone()).size(13.5).color(pal.fg)]
        .spacing(10)
        .align_y(iced::alignment::Vertical::Center);
    if let Some(action) = &toast.action {
        let action_button = button(text_w(action.label.clone()).size(12.5).color(pal.accent_fg))
            .padding(Padding::from([5, 10]))
            .style(move |_, _| button::Style {
                background: Some(Background::Color(pal.accent)),
                text_color: pal.accent_fg,
                border: Border {
                    radius: 999.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .on_press(action.message.clone());
        content = content.push(action_button);
    }
    let bubble = container(content)
        .padding([8, 14])
        .style(move |_| container::Style {
            background: Some(pal.surface.into()),
            border: iced::Border {
                color: pal.rule,
                width: 1.0,
                radius: 8.0.into(),
            },
            text_color: Some(pal.fg),
            ..Default::default()
        });
    container(bubble)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([18, 0])
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Top)
        .into()
}

/// Persistent, neutral Full Mindmap verification feedback. It intentionally
/// shares the toast position but sits beneath the ordinary toast layer, so a
/// blocked/error toast remains readable and keeps its own expiry deadline.
fn full_mindmap_progress_overlay<'a>(
    progress: &FullMindmapProgress,
    pal: Palette,
) -> Element<'a, Message> {
    use iced::widget::{container, text as text_w};
    let total = progress.total.max(1);
    let checked = progress.checked.min(progress.total);
    let remaining = progress.total.saturating_sub(checked);
    let ratio = (checked as f32 / total as f32).clamp(0.0, 1.0);
    let filled = ((ratio * 1000.0).round() as u16).max(if ratio > 0.0 { 1 } else { 0 });
    let unfilled = 1000u16.saturating_sub(filled);
    let bar = irow![
        container(Space::new())
            .width(Length::FillPortion(filled))
            .height(Length::Fixed(4.0))
            .style(move |_| container::Style {
                background: Some(pal.muted.into()),
                ..Default::default()
            }),
        container(Space::new())
            .width(Length::FillPortion(unfilled.max(1)))
            .height(Length::Fixed(4.0))
            .style(move |_| container::Style {
                background: Some(pal.rule.into()),
                ..Default::default()
            }),
    ]
    .spacing(0);
    let bubble = container(
        column![
            text_w(format!(
                "Verifying folders · {checked}/{} checked · {remaining} remaining",
                progress.total
            ))
            .size(13.0)
            .color(pal.fg),
            container(bar)
                .width(Length::Fill)
                .clip(true)
                .style(move |_| container::Style {
                    background: Some(pal.rule.into()),
                    border: Border {
                        radius: 999.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
        ]
        .spacing(6),
    )
    .width(Length::Fixed(320.0))
    .padding([8, 14])
    .style(move |_| container::Style {
        background: Some(pal.surface.into()),
        border: Border {
            color: pal.rule,
            width: 1.0,
            radius: 8.0.into(),
        },
        text_color: Some(pal.fg),
        ..Default::default()
    });
    container(bubble)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([18, 0])
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Top)
        .into()
}

fn image_zoom_overlay<'a>(
    url: Option<&'a str>,
    diagram: Option<&iced::widget::image::Handle>,
    cache: &ImageCache,
    pal: Palette,
) -> Element<'a, Message> {
    use iced::widget::image::viewer;
    let mk_viewer = |h: iced::widget::image::Handle| -> Element<'a, Message> {
        viewer(h)
            .min_scale(0.25)
            .max_scale(10.0)
            .scale_step(0.18)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    };
    // Diagram overrides image source when set — DiagramZoom clears zoom_url.
    // Reuses image::viewer for scroll-zoom + drag-pan + escape-close parity
    // with normal images.
    let inner: Element<'a, Message> = if let Some(handle) = diagram {
        mk_viewer(handle.clone())
    } else {
        match url {
            Some(u) => match cache.get(u) {
                Some(ImageState::Loaded(h)) => mk_viewer(h.clone()),
                Some(ImageState::LoadedSvg {
                    raster: Some(h), ..
                }) => mk_viewer(h.clone()),
                Some(ImageState::LoadedSvg { raster: None, .. }) | Some(ImageState::Loading) => {
                    text("rendering…").color(pal.muted).into()
                }
                Some(ImageState::Failed) => text("image unavailable").color(pal.muted).into(),
                None => {
                    // Local raster path (cache only stores svg/remote). Use direct viewer.
                    let p = std::path::PathBuf::from(u);
                    mk_viewer(iced::widget::image::Handle::from_path(p))
                }
            },
            None => text("").into(),
        }
    };
    let scrim = container(
        container(inner)
            .padding(8)
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_| container::Style {
        background: Some(Color { a: 0.85, ..pal.bg }.into()),
        ..Default::default()
    });
    // Click background scrim → close. Pointer cursor would mislead since
    // most of the surface is the viewer (which handles its own drags).
    let scrim_click = mouse_area(scrim).on_press(Message::CloseOverlay);
    // Top-right close button. Sits on its own mouse_area so a click on the
    // X always fires CloseOverlay (independent of the scrim mouse_area
    // beneath it in the stack).
    let close_btn_inner = container(crate::icon::glyph(crate::icon::ic::X, 16.0, pal.fg))
        .padding(Padding::from([6, 8]))
        .style(move |_| container::Style {
            background: Some(
                Color {
                    a: 0.75,
                    ..pal.code_bg
                }
                .into(),
            ),
            border: iced::Border {
                color: pal.code_border,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        });
    let close_btn = mouse_area(close_btn_inner)
        .interaction(iced::mouse::Interaction::Pointer)
        .on_press(Message::CloseOverlay);
    let close_overlay = container(close_btn)
        .padding(Padding::from([14, 16]))
        .align_x(iced::alignment::Horizontal::Right)
        .align_y(iced::alignment::Vertical::Top)
        .width(Length::Fill)
        .height(Length::Fill);
    stack![scrim_click, close_overlay].into()
}

fn inline_text_bytes(items: &[Inline]) -> usize {
    items
        .iter()
        .map(|i| match i {
            Inline::Text(t) | Inline::Code(t) => t.len(),
            Inline::Emph(c) | Inline::Strong(c) | Inline::Strike(c) => inline_text_bytes(c),
            Inline::Link { children, url } => inline_text_bytes(children).saturating_add(url.len()),
        })
        .sum()
}

fn block_text_bytes(block: &Block) -> usize {
    match block {
        Block::Heading { inlines, .. } | Block::Paragraph(inlines) => inline_text_bytes(inlines),
        Block::CodeBlock { code, .. } => code.len(),
        Block::Blockquote(blocks) => blocks.iter().map(block_text_bytes).sum(),
        Block::List { items, .. } => items
            .iter()
            .flat_map(|item| item.blocks.iter())
            .map(block_text_bytes)
            .sum(),
        Block::Table { headers, rows } => headers
            .iter()
            .chain(rows.iter().flat_map(|row| row.iter()))
            .map(|cells| inline_text_bytes(cells))
            .sum(),
        Block::Image { url, alt } => url.len().saturating_add(alt.len()),
        Block::Diagram { source, .. } => source.len(),
        Block::Rule => 0,
    }
}

fn edge_scroll(
    id: iced::widget::Id,
    viewport: Option<&iced::widget::scrollable::Viewport>,
    cursor: usize,
    total: usize,
    row_h: f32,
) -> Task<Message> {
    // List inside scrollable has small top/bottom padding (~6-8px each). Pad cur_bot
    // so the bottom edge of the *last* row is fully revealed instead of clipped.
    const PAD: f32 = 8.0;
    let Some(v) = viewport else {
        if total <= 1 {
            return Task::none();
        }
        let y = (cursor as f32 / (total - 1) as f32).clamp(0.0, 1.0);
        return iced::widget::operation::snap_to(
            id,
            iced::widget::scrollable::RelativeOffset { x: 0.0, y },
        );
    };
    let cur_top = cursor as f32 * row_h;
    let cur_bot = cur_top + row_h + PAD;
    let off = v.absolute_offset();
    let view_top = off.y;
    let view_h = v.bounds().height;
    let view_bot = view_top + view_h;
    let new_y = if cur_top < view_top {
        cur_top
    } else if cur_bot > view_bot {
        cur_bot - view_h
    } else {
        return Task::none();
    };
    iced::widget::operation::scroll_to(
        id,
        iced::widget::scrollable::AbsoluteOffset {
            x: 0.0,
            y: new_y.max(0.0),
        },
    )
}

fn scroll_block_to_top(id: crate::ast::BlockId) -> Task<Message> {
    struct ScrollBlockToTop {
        body_id: iced::widget::Id,
        target_id: iced::widget::Id,
        content_top: Option<f32>,
        target_top: Option<f32>,
    }

    impl iced::advanced::widget::Operation<Message> for ScrollBlockToTop {
        fn traverse(
            &mut self,
            operate: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation<Message>),
        ) {
            operate(self);
        }

        fn scrollable(
            &mut self,
            id: Option<&iced::widget::Id>,
            _bounds: iced::Rectangle,
            content_bounds: iced::Rectangle,
            _translation: iced::Vector,
            _state: &mut dyn iced::advanced::widget::operation::Scrollable,
        ) {
            if id == Some(&self.body_id) {
                self.content_top = Some(content_bounds.y);
            }
        }

        fn container(&mut self, id: Option<&iced::widget::Id>, bounds: iced::Rectangle) {
            if id == Some(&self.target_id) {
                if let Some(content_top) = self.content_top {
                    self.target_top = Some((bounds.y - content_top).max(0.0));
                }
            }
        }

        fn finish(&self) -> iced::advanced::widget::operation::Outcome<Message> {
            self.target_top
                .map_or(iced::advanced::widget::operation::Outcome::None, |y| {
                    iced::advanced::widget::operation::Outcome::Some(Message::RestoreBodyScroll(y))
                })
        }
    }

    iced::advanced::widget::operate(ScrollBlockToTop {
        body_id: App::scroll_id(),
        target_id: crate::render::block_anchor_id(id),
        content_top: None,
        target_top: None,
    })
}

/// Scroll the body so the given block lands slightly above center, using real
/// laid-out widget bounds (not height estimates). Used by find/highlight nav so
/// the matched word is always actually visible.
fn scroll_block_to_center(id: crate::ast::BlockId) -> Task<Message> {
    struct ScrollBlockToCenter {
        body_id: iced::widget::Id,
        target_id: iced::widget::Id,
        content_top: Option<f32>,
        view_h: f32,
        target_y: Option<f32>,
    }

    impl iced::advanced::widget::Operation<Message> for ScrollBlockToCenter {
        fn traverse(
            &mut self,
            operate: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation<Message>),
        ) {
            operate(self);
        }

        fn scrollable(
            &mut self,
            id: Option<&iced::widget::Id>,
            bounds: iced::Rectangle,
            content_bounds: iced::Rectangle,
            _translation: iced::Vector,
            _state: &mut dyn iced::advanced::widget::operation::Scrollable,
        ) {
            if id == Some(&self.body_id) {
                self.content_top = Some(content_bounds.y);
                self.view_h = bounds.height;
            }
        }

        fn container(&mut self, id: Option<&iced::widget::Id>, bounds: iced::Rectangle) {
            if id == Some(&self.target_id) {
                if let Some(content_top) = self.content_top {
                    let block_top = bounds.y - content_top;
                    // Place block slightly above center so following context shows.
                    let y = block_top + bounds.height * 0.5 - self.view_h * 0.38;
                    self.target_y = Some(y.max(0.0));
                }
            }
        }

        fn finish(&self) -> iced::advanced::widget::operation::Outcome<Message> {
            self.target_y
                .map_or(iced::advanced::widget::operation::Outcome::None, |y| {
                    iced::advanced::widget::operation::Outcome::Some(Message::RestoreBodyScroll(y))
                })
        }
    }

    iced::advanced::widget::operate(ScrollBlockToCenter {
        body_id: App::scroll_id(),
        target_id: crate::render::block_anchor_id(id),
        content_top: None,
        view_h: 0.0,
        target_y: None,
    })
}

/// Harvest real laid-out heights for the given anchored block containers.
/// Feeds the virt-window `HeightCache` so prefix estimates converge.
fn measure_block_heights(
    targets: HashMap<iced::widget::Id, crate::ast::BlockId>,
    at_offset: f32,
) -> Task<Message> {
    struct MeasureHeights {
        targets: HashMap<iced::widget::Id, crate::ast::BlockId>,
        at_offset: f32,
        out: Vec<(crate::ast::BlockId, f32)>,
    }

    impl iced::advanced::widget::Operation<Message> for MeasureHeights {
        fn traverse(
            &mut self,
            operate: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation<Message>),
        ) {
            operate(self);
        }

        fn container(&mut self, id: Option<&iced::widget::Id>, bounds: iced::Rectangle) {
            if let Some(bid) = id.and_then(|i| self.targets.get(i)) {
                self.out.push((*bid, bounds.height));
            }
        }

        fn finish(&self) -> iced::advanced::widget::operation::Outcome<Message> {
            if self.out.is_empty() {
                iced::advanced::widget::operation::Outcome::None
            } else {
                iced::advanced::widget::operation::Outcome::Some(Message::BlockHeightsMeasured(
                    self.out.clone(),
                    self.at_offset,
                ))
            }
        }
    }

    if targets.is_empty() {
        return Task::none();
    }
    iced::advanced::widget::operate(MeasureHeights {
        targets,
        at_offset,
        out: Vec::new(),
    })
}

/// Preview-owned counterpart to `measure_block_heights`. The widget operation
/// is identical, but carries the selected path and preview generation so the
/// update handler can reject a result after navigation changed the panel.
fn measure_full_mindmap_preview_block_heights(
    path: PathBuf,
    namespace: u64,
    identity: u64,
    generation: u64,
    targets: HashMap<iced::widget::Id, crate::ast::BlockId>,
    at_offset: f32,
) -> Task<Message> {
    struct MeasurePreviewHeights {
        path: PathBuf,
        namespace: u64,
        identity: u64,
        generation: u64,
        targets: HashMap<iced::widget::Id, crate::ast::BlockId>,
        at_offset: f32,
        out: Vec<(crate::ast::BlockId, f32)>,
    }

    impl iced::advanced::widget::Operation<Message> for MeasurePreviewHeights {
        fn traverse(
            &mut self,
            operate: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation<Message>),
        ) {
            operate(self);
        }

        fn container(&mut self, id: Option<&iced::widget::Id>, bounds: iced::Rectangle) {
            if let Some(block_id) = id.and_then(|widget_id| self.targets.get(widget_id)) {
                self.out.push((*block_id, bounds.height));
            }
        }

        fn finish(&self) -> iced::advanced::widget::operation::Outcome<Message> {
            iced::advanced::widget::operation::Outcome::Some(
                Message::FullMindmapPreviewBlockHeightsMeasured {
                    path: self.path.clone(),
                    namespace: self.namespace,
                    identity: self.identity,
                    generation: self.generation,
                    measured: self.out.clone(),
                    at_offset: self.at_offset,
                },
            )
        }
    }

    if targets.is_empty() {
        return Task::none();
    }
    iced::advanced::widget::operate(MeasurePreviewHeights {
        path,
        namespace,
        identity,
        generation,
        targets,
        at_offset,
        out: Vec::new(),
    })
}

/// Scroll the vault results page just enough to bring the cursor's match block
/// fully into view, measuring its real bounds (blocks have variable height).
/// Only moves when the block is off-screen, like a code editor's cursor follow.
fn scroll_vault_to_match(vis_idx: usize) -> Task<Message> {
    struct ScrollVaultToMatch {
        scroll_id: iced::widget::Id,
        target_id: iced::widget::Id,
        content_top: Option<f32>,
        view_top: f32,
        view_h: f32,
        target_y: Option<f32>,
    }

    impl iced::advanced::widget::Operation<Message> for ScrollVaultToMatch {
        fn traverse(
            &mut self,
            operate: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation<Message>),
        ) {
            operate(self);
        }

        fn scrollable(
            &mut self,
            id: Option<&iced::widget::Id>,
            bounds: iced::Rectangle,
            content_bounds: iced::Rectangle,
            translation: iced::Vector,
            _state: &mut dyn iced::advanced::widget::operation::Scrollable,
        ) {
            if id == Some(&self.scroll_id) {
                self.content_top = Some(content_bounds.y);
                self.view_top = translation.y;
                self.view_h = bounds.height;
            }
        }

        fn container(&mut self, id: Option<&iced::widget::Id>, bounds: iced::Rectangle) {
            if id == Some(&self.target_id) {
                if let Some(content_top) = self.content_top {
                    const PAD: f32 = 12.0;
                    let block_top = bounds.y - content_top;
                    let block_bot = block_top + bounds.height;
                    let view_top = self.view_top;
                    let view_bot = view_top + self.view_h;
                    let y = if block_top < view_top {
                        block_top - PAD
                    } else if block_bot > view_bot {
                        // Reveal the block's bottom; if taller than the viewport,
                        // pin its top so the match line stays visible.
                        let candidate = block_bot - self.view_h + PAD;
                        candidate.min(block_top - PAD)
                    } else {
                        return; // already fully visible — don't move
                    };
                    self.target_y = Some(y.max(0.0));
                }
            }
        }

        fn finish(&self) -> iced::advanced::widget::operation::Outcome<Message> {
            self.target_y
                .map_or(iced::advanced::widget::operation::Outcome::None, |y| {
                    iced::advanced::widget::operation::Outcome::Some(Message::VaultScrollTo(y))
                })
        }
    }

    iced::advanced::widget::operate(ScrollVaultToMatch {
        scroll_id: App::vault_scroll_id(),
        target_id: App::vault_match_anchor_id(vis_idx),
        content_top: None,
        view_top: 0.0,
        view_h: 0.0,
        target_y: None,
    })
}

fn welcome_view<'a>(pal: Palette) -> Element<'a, Message> {
    let kbd = |label: &'static str, key: &'static str| {
        irow![
            container(
                text(key)
                    .size(12)
                    .color(pal.fg)
                    .shaping(iced::widget::text::Shaping::Advanced)
            )
            .padding(Padding::from([2, 7]))
            .style(move |_| container::Style {
                background: Some(pal.surface_alt.into()),
                border: Border {
                    color: pal.rule,
                    width: 1.0,
                    radius: 5.0.into(),
                },
                ..Default::default()
            }),
            text(label).size(13).color(pal.muted).font(iced::Font {
                family: iced::font::Family::Name("JetBrains Mono"),
                ..iced::Font::DEFAULT
            }),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center)
    };
    centered_card(
        column![
            text("rmdv").size(40).color(pal.fg),
            text("Lightweight, beautiful, native markdown viewer")
                .size(14)
                .color(pal.muted),
            Space::new().height(22),
            primary_button("Browse a Project as Mindmap", pal).on_press(Message::ToggleFullMindmap),
            Space::new().height(8),
            kbd("Open Folder", "⌘O"),
            kbd("Find File in Workspace", "⌘P"),
            kbd("Command Palette", "⌘⇧P"),
            kbd("Toggle Sidebar", "⌘B"),
            kbd("Find in Document", "⌘F"),
            kbd("Cycle Theme", "⌘T"),
            kbd("Edit / Select Text", "⌘E"),
            kbd("Fold to Level (then 0–6)", "⌘K"),
            kbd("Full Mindmap Mode", "⌘⇧M"),
        ]
        .spacing(8)
        .align_x(iced::Alignment::Start)
        .into(),
        pal,
    )
}

/// Transient modifier-driven Quick Slot rail. It deliberately has no enclosing
/// panel: the nine fixed controls float at the supplied left offset and inherit
/// the current theme roles used by the rest of the application.
fn quick_slots_rail<'a>(app: &'a App, pal: Palette, left_offset: f32) -> Element<'a, Message> {
    let mut controls = Column::new().spacing(6).align_x(iced::Alignment::Start);
    for index in 0..crate::quick_slots::SLOT_COUNT {
        let occupied = app.quick_slots.occupied(index);
        let missing = occupied.is_some_and(|slot| {
            app.quick_slots_workspace_root()
                .and_then(|root| crate::quick_slots::resolve_path(root, &slot.relative_path))
                .map_or(true, |path| !path.is_file())
        });
        let active = app.quick_slots.active == Some(index);
        let assignable = app.current_quick_slot_context().is_some();
        let label = container(
            text((index + 1).to_string())
                .size(12)
                .font(editor_font())
                .color(if active {
                    pal.accent_fg
                } else if missing {
                    pal.accent
                } else {
                    pal.fg
                }),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center);
        let mut slot_button = button(label)
            .width(Length::Fixed(24.0))
            .height(Length::Fixed(24.0))
            .padding(Padding::ZERO)
            .style(move |_, status| {
                let background = if active {
                    Some(Background::Color(pal.accent))
                } else {
                    match status {
                        button::Status::Hovered | button::Status::Pressed => {
                            Some(Background::Color(pal.surface_alt))
                        }
                        _ => Some(Background::Color(pal.surface)),
                    }
                };
                button::Style {
                    background,
                    text_color: if active { pal.accent_fg } else { pal.fg },
                    border: Border {
                        color: if missing {
                            pal.accent
                        } else if occupied.is_some() {
                            pal.accent
                        } else {
                            pal.rule
                        },
                        width: 1.0,
                        radius: 5.0.into(),
                    },
                    ..Default::default()
                }
            });
        slot_button = if occupied.is_some() {
            slot_button.on_press(Message::QuickSlotActivate(index))
        } else if assignable {
            slot_button.on_press(Message::QuickSlotAssign(index))
        } else {
            slot_button
        };
        let slot_view: Element<'a, Message> = if let Some(slot) = occupied {
            let filename = if missing {
                format!("Missing: {}", slot.relative_path)
            } else {
                std::path::Path::new(&slot.relative_path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_owned)
                    .unwrap_or_else(|| slot.relative_path.clone())
            };
            let details = container(
                text(filename)
                    .size(12)
                    .color(if missing { pal.accent } else { pal.fg })
                    .wrapping(iced::widget::text::Wrapping::None),
            )
            .width(Length::Fixed(220.0))
            .height(Length::Fixed(24.0))
            .padding(Padding::from([0, 6]))
            .align_y(iced::alignment::Vertical::Center)
            .clip(true)
            .style(move |_| container::Style {
                background: Some(pal.surface.into()),
                border: Border {
                    color: pal.rule,
                    width: 1.0,
                    radius: 6.0.into(),
                },
                ..Default::default()
            });
            irow![slot_button, details]
                .spacing(6)
                .align_y(iced::Alignment::Center)
                .into()
        } else {
            irow![slot_button].into()
        };
        controls = controls.push(slot_view);
    }
    container(controls)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: left_offset,
        })
        .align_x(iced::alignment::Horizontal::Left)
        .align_y(iced::alignment::Vertical::Center)
        .into()
}

fn search_bar_view<'a>(
    query: &'a str,
    matches: &'a [MatchPos],
    idx: usize,
    pal: Palette,
) -> Element<'a, Message> {
    let counter = if matches.is_empty() {
        if query.is_empty() {
            String::new()
        } else {
            "0/0".into()
        }
    } else {
        format!("{}/{}", idx + 1, matches.len())
    };
    container(
        irow![
            text("Find").size(12).color(pal.subtle),
            text_input("type to search…", query)
                .id(App::search_input_id())
                .on_input(Message::QueryChanged)
                .padding(Padding::from([6, 10]))
                .size(13)
                .style(move |_, _| iced::widget::text_input::Style {
                    background: pal.surface_alt.into(),
                    border: Border {
                        color: pal.rule,
                        width: 1.0,
                        radius: 999.0.into(),
                    },
                    icon: pal.muted,
                    placeholder: pal.subtle,
                    value: pal.fg,
                    selection: pal.selection,
                })
                .width(Length::Fill),
            text(counter).color(pal.muted).size(12),
            ghost_lu(ic::CHEVRON_LEFT, pal).on_press(Message::PrevMatch),
            ghost_lu(ic::CHEVRON_RIGHT, pal).on_press(Message::NextMatch),
            ghost_lu(ic::X, pal).on_press(Message::ToggleSearch),
        ]
        .padding(Padding::from([8, 14]))
        .spacing(10)
        .align_y(iced::Alignment::Center),
    )
    .style(move |_| container::Style {
        background: Some(pal.surface.into()),
        border: Border {
            color: pal.rule,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    })
    .width(Length::Fill)
    .into()
}

fn refresh_window_mode(id: iced::window::Id) -> Task<Message> {
    iced::window::mode(id).map(Message::WindowModeChanged)
}

/// Sample the window mode now and again after the native transition settles.
///
/// macOS native fullscreen enter/exit animates; the resize/focus event that
/// triggers a refresh can fire *before* the mode flips, so a single immediate
/// query can read the stale (pre-transition) mode on exit. The delayed second
/// query lands after the animation completes and corrects the flag, restoring
/// the windowed header reserve. See the fullscreen-exit relayout bug.
fn refresh_window_mode_after_native_transition(id: iced::window::Id) -> Task<Message> {
    // Sample immediately, then again after the native animation could plausibly
    // have settled. Two delayed samples (250ms + 600ms) because a single fixed
    // delay can still land before a slow fullscreen-exit animation finishes.
    let delayed = |ms: u64| {
        Task::perform(
            async move {
                tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
                id
            },
            Message::RefreshWindowModeSettled,
        )
    };
    Task::batch([refresh_window_mode(id), delayed(250), delayed(600)])
}

fn sidebar_view<'a>(app: &'a App, pal: Palette) -> Element<'a, Message> {
    let recently_scrolled = app
        .last_scroll_at
        .is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(SCROLLER_FADE_MS));
    let ws = app.workspace.as_ref().unwrap();
    let ws_name = ws
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("workspace");
    // Title row: workspace name on the left; the Files/Outline tabs, command-palette
    // button, and sidebar-collapse button pinned to the right beside each other.
    let kbd_pill = |label: &'static str, pal: Palette| {
        container(
            text(label)
                .size(11)
                .color(pal.fg)
                .shaping(iced::widget::text::Shaping::Advanced),
        )
        .padding(Padding::from([4, 8]))
        .style(move |_| container::Style {
            background: Some(pal.surface_alt.into()),
            border: Border {
                color: pal.rule,
                width: 1.0,
                radius: 5.0.into(),
            },
            ..Default::default()
        })
    };
    let switch_tip = |inner: Element<'a, Message>| {
        iced::widget::tooltip(
            inner,
            kbd_pill("← → switch", pal),
            iced::widget::tooltip::Position::Bottom,
        )
    };
    // Only the (variable-length) title may be clipped on a narrow sidebar — the
    // control cluster stays unclipped so the collapse button is never cut off.
    let title_label = container(
        text(ws_name.to_string().to_uppercase())
            .size(11)
            .color(pal.muted)
            // No word-wrap: a long workspace name must truncate (clip below),
            // not wrap onto a second line and grow the header row.
            .wrapping(iced::widget::text::Wrapping::None),
    )
    .width(Length::Fill)
    .clip(true);
    let controls = irow![
        switch_tip(sidebar_tab_button(
            "Files",
            app.sidebar_tab == SidebarTab::Files,
            SidebarTab::Files,
            pal
        )),
        switch_tip(sidebar_tab_button(
            "Outline",
            app.sidebar_tab == SidebarTab::Outline,
            SidebarTab::Outline,
            pal
        )),
        iced::widget::tooltip(
            ghost_lu(ic::COMMAND, pal).on_press(Message::OpenCommandPalette),
            kbd_pill("⌘⇧P", pal),
            iced::widget::tooltip::Position::Bottom,
        ),
        iced::widget::tooltip(
            ghost_lu(ic::PANEL_LEFT_CLOSE, pal).on_press(Message::ToggleSidebar),
            kbd_pill("⌘B", pal),
            iced::widget::tooltip::Position::Bottom,
        ),
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center);
    let title_row = irow![title_label, controls]
        .spacing(6)
        .align_y(iced::Alignment::Center);

    let header = container(column![
        Space::new().height(Length::Fixed(sidebar_titlebar_reserve_for_fullscreen(
            app.window_fullscreen,
        ))),
        container(title_row)
            .padding(Padding {
                top: 0.0,
                right: 14.0,
                bottom: 8.0,
                left: 14.0,
            })
            .width(Length::Fill),
    ])
    .width(Length::Fill);

    let body: Element<'a, Message> = match app.sidebar_tab {
        SidebarTab::Files => sidebar_files_body(app, pal, recently_scrolled),
        SidebarTab::Outline => sidebar_outline_body(app, pal, recently_scrolled),
    };

    container(column![header, body])
        .width(Length::Fixed(app.sidebar_width))
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(pal.sidebar.into()),
            ..Default::default()
        })
        .into()
}

fn sidebar_tab_button<'a>(
    label: &'a str,
    active: bool,
    tab: SidebarTab,
    pal: Palette,
) -> Element<'a, Message> {
    button(
        text(label)
            .size(11)
            .color(if active { pal.fg } else { pal.muted }),
    )
    .padding(Padding::from([3, 9]))
    .style(move |_, status| {
        let bg = if active {
            Some(Background::Color(pal.surface_alt))
        } else {
            match status {
                button::Status::Hovered => Some(Background::Color(pal.surface_alt)),
                _ => None,
            }
        };
        button::Style {
            background: bg,
            text_color: pal.fg,
            border: Border {
                color: if active { pal.rule } else { Color::TRANSPARENT },
                width: 1.0,
                radius: 5.0.into(),
            },
            ..Default::default()
        }
    })
    .on_press(Message::SetSidebarTab(tab))
    .into()
}

fn sidebar_files_body<'a>(
    app: &'a App,
    pal: Palette,
    recently_scrolled: bool,
) -> Element<'a, Message> {
    // Measure longest row so we can pin the Column to a Fixed width. With
    // `Direction::Both`, an unsized Column collapses to its widest *Shrink*
    // child — which would shrink the selection ring to text width. Setting an
    // explicit width lets each row's `Length::Fill` stretch to it, giving a
    // full-width focus ring AND horizontal scroll when names overflow.
    // Approach mirrors Zed's project panel.
    let mut list = Column::new().spacing(0).padding(Padding::from([4, 4]));
    let mut content_w = app.sidebar_width - 12.0; // minus scrollbar gutter
    if let Some(tree_root) = &app.workspace_tree {
        let rows = tree::flatten_with_files(tree_root, &app.workspace_sidebar_files, &app.expanded);
        let current = app.file.as_ref();
        let cursor = app.tree_cursor;
        for r in rows.iter() {
            let w = tree_row_width(r.node, r.depth);
            if w > content_w {
                content_w = w;
            }
        }
        for (i, r) in rows.iter().enumerate() {
            let row_el = tree_row(r.node, r.depth, &app.expanded, current, i == cursor, pal);
            list = list.push(row_el);
        }
    }
    let list = list.width(Length::Fixed(content_w));
    // Nested single-axis scrollables: inner handles vertical, outer handles
    // horizontal. Iced 0.14's `Direction::Both` allows diagonal scrolling,
    // which feels wrong for file trees — Zed and VS Code lock to one axis at
    // a time. Splitting them lets macOS trackpad gestures route naturally:
    // dominant-Y events hit the inner, dominant-X events bubble to the outer.
    let inner = scrollable(list)
        .id(App::tree_scroll_id())
        .width(Length::Fixed(content_w))
        .height(Length::Fill)
        .on_scroll(Message::TreeScrolled)
        .direction(slim_scroll_direction())
        .style(move |_, status| sleek_scrollable_style(status, pal, recently_scrolled));
    scrollable(inner)
        .height(Length::Fill)
        .direction(slim_scroll_direction_horizontal())
        .style(move |_, status| sleek_scrollable_style(status, pal, recently_scrolled))
        .into()
}

fn sidebar_outline_body<'a>(
    app: &'a App,
    pal: Palette,
    recently_scrolled: bool,
) -> Element<'a, Message> {
    let sections = &app.outline_sections;
    let mut list = Column::new().spacing(0).padding(Padding::from([4, 4]));
    if sections.is_empty() {
        list = list.push(
            container(text("No headings").size(12).color(pal.muted))
                .padding(Padding::from([8, 10])),
        );
    } else {
        for (i, s) in sections.iter().enumerate() {
            list = list.push(outline_row(s, i == app.outline_cursor, pal));
        }
    }
    scrollable(list.width(Length::Fill))
        .id(App::outline_scroll_id())
        .height(Length::Fill)
        .on_scroll(Message::OutlineScrolled)
        .direction(slim_scroll_direction())
        .style(move |_, status| sleek_scrollable_style(status, pal, recently_scrolled))
        .into()
}

fn outline_row<'a>(
    s: &crate::ipc::sections::Section,
    is_cursor: bool,
    pal: Palette,
) -> Element<'a, Message> {
    let indent = TREE_INDENT * (s.level.saturating_sub(1)) as f32;
    let weight = if s.level <= 1 {
        iced::font::Weight::Medium
    } else {
        iced::font::Weight::Normal
    };
    let mut font = iced::Font::with_name("Inter");
    font.weight = weight;
    let label = text(s.title.clone())
        .size(13)
        .color(if s.level <= 1 { pal.fg } else { pal.muted })
        .font(font)
        .wrapping(text::Wrapping::None);
    let content =
        irow![Space::new().width(Length::Fixed(indent)), label].align_y(iced::Alignment::Center);
    button(content)
        .padding(Padding::from([4, 8]))
        .width(Length::Fill)
        .height(Length::Fixed(26.0))
        .style(move |_, status| button::Style {
            background: if is_cursor {
                Some(Background::Color(pal.tree_selected_bg))
            } else {
                match status {
                    button::Status::Hovered => Some(Background::Color(pal.surface_alt)),
                    _ => None,
                }
            },
            text_color: pal.fg,
            border: Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .on_press(goto_line_message(s.line))
        .into()
}

fn mindmap_panel_resize_handle<'a>(pal: Palette) -> Element<'a, Message> {
    mouse_area(
        container(
            Space::new()
                .width(Length::Fixed(crate::mindmap::PANEL_HANDLE_W))
                .height(Length::Fill),
        )
        .style(move |_| container::Style {
            background: Some(pal.bg.into()),
            ..Default::default()
        })
        .height(Length::Fill),
    )
    .interaction(iced::mouse::Interaction::ResizingHorizontally)
    .on_press(Message::MindmapPanelDragStart(0.0))
    .on_release(Message::MindmapPanelDragEnd)
    .into()
}

fn full_mindmap_panel_resize_handle<'a>(pal: Palette) -> Element<'a, Message> {
    mouse_area(
        container(
            Space::new()
                .width(Length::Fixed(crate::mindmap::PANEL_HANDLE_W))
                .height(Length::Fill),
        )
        .style(move |_| container::Style {
            background: Some(pal.bg.into()),
            ..Default::default()
        })
        .height(Length::Fill),
    )
    .interaction(iced::mouse::Interaction::ResizingHorizontally)
    .on_press(Message::FullMindmapPanelDragStart(0.0))
    .on_release(Message::FullMindmapPanelDragEnd)
    .into()
}

fn sidebar_resize_handle<'a>(pal: Palette) -> Element<'a, Message> {
    mouse_area(
        container(Space::new().width(Length::Fixed(5.0)).height(Length::Fill))
            .style(move |_| container::Style {
                background: Some(pal.sidebar.into()),
                ..Default::default()
            })
            .height(Length::Fill),
    )
    .interaction(iced::mouse::Interaction::ResizingHorizontally)
    .on_press(Message::SidebarDragStart)
    .on_release(Message::SidebarDragEnd)
    .into()
}

/// Estimate the pixel width a [`tree_row`] needs at the given depth. Used to
/// size the surrounding Column so the focus ring fills the sidebar width AND
/// horizontal scroll kicks in when names overflow. Approximation uses an
/// average advance for Inter @ 13px; exact metrics aren't worth the cost of a
/// glyph-shaping pass on every render.
fn tree_row_width(node: tree::RowNode<'_>, depth: usize) -> f32 {
    const CHAR_ADVANCE: f32 = 7.0;
    let indent = TREE_INDENT * depth as f32;
    let chevron = 14.0;
    let leaf = 13.0 + 4.0 + 7.0; // icon + gap before + gap after
    let label = node.name().chars().count() as f32 * CHAR_ADVANCE;
    let padding_h = 16.0; // button padding 8 each side
    indent + chevron + leaf + label + padding_h
}

fn tree_row<'a>(
    node: tree::RowNode<'a>,
    depth: usize,
    expanded: &HashSet<PathBuf>,
    current: Option<&'a PathBuf>,
    is_cursor: bool,
    pal: Palette,
) -> Element<'a, Message> {
    let is_dir = node.is_dir();
    let node_path = node.path();
    let is_current = !is_dir && current.map(|c| c.as_path() == node_path).unwrap_or(false);
    let path = node_path.to_path_buf();

    // Indent area with vertical guides per ancestor level.
    let mut indent = iced::widget::Row::new();
    for _ in 0..depth {
        indent = indent.push(indent_guide(pal));
    }

    let chevron: Element<'a, Message> = if is_dir {
        let open = expanded.contains(node_path);
        let g = if open {
            ic::CHEVRON_DOWN
        } else {
            ic::CHEVRON_RIGHT
        };
        icon::glyph(g, 12.0, pal.subtle).into()
    } else {
        Space::new().width(12.0).into()
    };

    let label_color = if is_current {
        pal.fg
    } else if is_dir {
        pal.fg
    } else {
        pal.muted
    };
    let label_weight = if is_dir {
        iced::font::Weight::Medium
    } else {
        iced::font::Weight::Normal
    };
    let mut label_font = iced::Font::with_name("Inter");
    label_font.weight = label_weight;
    let label = text(node.name().into_owned())
        .size(13)
        .color(label_color)
        .font(label_font)
        .wrapping(text::Wrapping::None);

    let leaf_icon: Element<'a, Message> = if is_dir {
        let open = expanded.contains(node_path);
        let g = if open { ic::FOLDER_OPEN } else { ic::FOLDER };
        icon::glyph(g, 13.0, pal.subtle).into()
    } else {
        icon::glyph(ic::FILE_TEXT, 13.0, pal.subtle).into()
    };
    let content = irow![
        indent,
        container(chevron).width(Length::Fixed(14.0)),
        Space::new().width(4.0),
        leaf_icon,
        Space::new().width(7.0),
        label,
    ]
    .align_y(iced::Alignment::Center)
    .spacing(0);

    let on_press = if is_dir {
        Message::TreeToggle(path)
    } else {
        Message::Open(path)
    };

    button(content)
        .padding(Padding::from([4, 8]))
        .width(Length::Fill)
        .height(Length::Fixed(26.0))
        .style(move |_, status| {
            let bg = if is_current {
                Some(Background::Color(pal.tree_selected_bg))
            } else if is_cursor {
                Some(Background::Color(pal.surface_alt))
            } else {
                match status {
                    button::Status::Hovered => Some(Background::Color(pal.surface_alt)),
                    _ => None,
                }
            };
            // Selection and keyboard cursor: background fill only, no ring.
            button::Style {
                background: bg,
                text_color: pal.fg,
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: 6.0.into(),
                },
                ..Default::default()
            }
        })
        .on_press(on_press)
        .into()
}

fn indent_guide<'a>(pal: Palette) -> Element<'a, Message> {
    container(
        container(Space::new().height(Length::Fill))
            .width(Length::Fixed(1.0))
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(pal.indent_guide.into()),
                ..Default::default()
            }),
    )
    .width(Length::Fixed(TREE_INDENT))
    .height(Length::Fixed(26.0))
    .center_x(Length::Fixed(TREE_INDENT))
    .into()
}

fn primary_button<'a>(label: &'a str, pal: Palette) -> button::Button<'a, Message> {
    button(text(label).size(13))
        .padding(Padding::from([8, 14]))
        .style(move |_, status| {
            let bg = match status {
                button::Status::Hovered => Color {
                    a: 0.92,
                    ..pal.accent
                },
                button::Status::Pressed => Color {
                    a: 0.80,
                    ..pal.accent
                },
                _ => pal.accent,
            };
            button::Style {
                background: Some(Background::Color(bg)),
                text_color: pal.accent_fg,
                border: Border {
                    radius: 999.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
}

fn ghost_lu<'a>(code: char, pal: Palette) -> button::Button<'a, Message> {
    button(icon::glyph(code, 14.0, pal.muted))
        .padding(Padding::from([4, 8]))
        .style(move |_, status| button::Style {
            background: match status {
                button::Status::Hovered => Some(Background::Color(pal.surface_alt)),
                _ => None,
            },
            text_color: pal.muted,
            border: Border {
                radius: 999.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
}

fn centered_card<'a>(content: Element<'a, Message>, pal: Palette) -> Element<'a, Message> {
    container(
        container(content)
            .padding(Padding::from([40, 56]))
            .style(move |_| container::Style {
                background: Some(pal.surface.into()),
                border: Border {
                    color: pal.rule,
                    width: 1.0,
                    radius: 16.0.into(),
                },
                shadow: iced::Shadow {
                    color: Color::from_rgba(0.0, 0.0, 0.0, 0.18),
                    offset: iced::Vector::new(0.0, 8.0),
                    blur_radius: 30.0,
                },
                ..Default::default()
            }),
    )
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .into()
}

fn folder_picker_overlay<'a>(
    pk: Option<&'a Picker>,
    selected: usize,
    pal: Palette,
) -> Element<'a, Message> {
    let panel: Element<'a, Message> = if let Some(pk) = pk {
        let crumbs = pk.breadcrumbs();
        let mut crumb_row = iced::widget::Row::new()
            .spacing(2)
            .align_y(iced::Alignment::Center);
        crumb_row = crumb_row.push(ghost_lu(ic::HOME, pal).on_press(Message::PickerHome));
        crumb_row = crumb_row.push(ghost_lu(ic::ARROW_UP, pal).on_press(Message::PickerParent));
        crumb_row = crumb_row.push(Space::new().width(8));
        for (label, path) in crumbs.iter() {
            crumb_row = crumb_row.push(text("/").color(pal.subtle).size(12));
            let label = label.clone();
            let path = path.clone();
            crumb_row = crumb_row.push(
                button(text(label).size(12).color(pal.fg))
                    .padding(Padding::from([3, 6]))
                    .style(move |_, status| button::Style {
                        background: match status {
                            button::Status::Hovered => Some(Background::Color(pal.surface_alt)),
                            _ => None,
                        },
                        text_color: pal.fg,
                        border: Border {
                            radius: 6.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .on_press(Message::PickerNavigate(path)),
            );
        }
        let header = container(crumb_row)
            .padding(Padding::from([10, 14]))
            .width(Length::Fill);

        let mut list = Column::new().spacing(1).padding(Padding::from([6, 8]));
        if let Some(err) = &pk.error {
            list = list.push(text(err.clone()).color(pal.muted).size(13));
        } else if pk.entries.is_empty() {
            list =
                list.push(container(text("Empty folder").color(pal.subtle).size(13)).padding(14));
        } else {
            for (i, e) in pk.entries.iter().enumerate() {
                let is_sel = i == selected;
                let path_clone = e.path.clone();
                let name = e.name.clone();
                let glyph = if e.is_dir { ic::FOLDER } else { ic::FILE_TEXT };
                let on_press = if e.is_dir {
                    Message::PickerNavigate(path_clone)
                } else {
                    Message::PickerOpenFile(path_clone)
                };
                let row = button(
                    irow![
                        icon::glyph(glyph, 13.0, pal.subtle),
                        text(name).size(13).color(pal.fg),
                    ]
                    .spacing(10)
                    .align_y(iced::Alignment::Center),
                )
                .padding(Padding::from([7, 12]))
                .width(Length::Fill)
                .height(Length::Fixed(32.0))
                .style(move |_, status| button::Style {
                    background: match (is_sel, status) {
                        (true, _) => Some(Background::Color(pal.surface_alt)),
                        (_, button::Status::Hovered) => Some(Background::Color(pal.surface_alt)),
                        _ => None,
                    },
                    text_color: pal.fg,
                    border: Border {
                        radius: 6.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .on_press(on_press);
                list = list.push(row);
            }
        }
        let body = scrollable(list)
            .id(App::overlay_scroll_id())
            .height(Length::Fill)
            .on_scroll(Message::OverlayScrolled)
            .direction(slim_scroll_direction())
            .style(move |_, status| sleek_scrollable_style(status, pal, true));

        let footer = picker_hint_footer(pal);
        column![header, body, footer].into()
    } else {
        text("No picker").into()
    };

    overlay_frame(panel, pal, 640.0, 560.0)
}

fn file_finder_overlay<'a>(
    query: &'a str,
    files: Vec<(PathBuf, String, i32)>,
    selected: usize,
    pal: Palette,
) -> Element<'a, Message> {
    let input = container(
        text_input("Find file… (fuzzy)", query)
            .id(App::overlay_input_id())
            .on_input(Message::OverlayQueryChanged)
            .on_submit(Message::OverlayConfirm)
            .padding(Padding::from([10, 14]))
            .size(14)
            .style(move |_, _| iced::widget::text_input::Style {
                background: Color::TRANSPARENT.into(),
                border: Border::default(),
                icon: pal.muted,
                placeholder: pal.subtle,
                value: pal.fg,
                selection: pal.selection,
            }),
    );

    let mut list = Column::new().spacing(0).padding(Padding::from([6, 8]));
    if files.is_empty() {
        list = list.push(container(text("No matches").color(pal.subtle).size(13)).padding(14));
    } else {
        for (i, (p, rel, _)) in files.into_iter().enumerate().take(80) {
            let is_sel = i == selected;
            let path_clone = p.clone();
            let parent = std::path::Path::new(&rel)
                .parent()
                .map(|x| x.to_string_lossy().into_owned())
                .unwrap_or_default();
            let name = std::path::Path::new(&rel)
                .file_name()
                .map(|x| x.to_string_lossy().into_owned())
                .unwrap_or_else(|| rel.clone());
            let inner = irow![
                text(name).size(13).color(pal.fg),
                Space::new().width(8),
                text(parent).size(12).color(pal.subtle),
            ]
            .align_y(iced::Alignment::Center);
            let row = button(inner)
                .padding(Padding::from([7, 12]))
                .width(Length::Fill)
                .height(Length::Fixed(32.0))
                .style(move |_, status| button::Style {
                    background: match (is_sel, status) {
                        (true, _) => Some(Background::Color(pal.surface_alt)),
                        (_, button::Status::Hovered) => Some(Background::Color(pal.surface_alt)),
                        _ => None,
                    },
                    text_color: pal.fg,
                    border: Border {
                        radius: 6.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .on_press(Message::OpenFileFinderPath(path_clone));
            list = list.push(row);
        }
    }
    let body = scrollable(list)
        .id(App::overlay_scroll_id())
        .on_scroll(Message::OverlayScrolled)
        .height(Length::Fill)
        .direction(slim_scroll_direction())
        .style(move |_, status| sleek_scrollable_style(status, pal, true));

    let divider = container(Space::new().height(1.0))
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(pal.rule.into()),
            ..Default::default()
        });

    overlay_frame(column![input, divider, body].into(), pal, 600.0, 460.0)
}

/// Vault-wide search results page (Zed-style). Fills the reader area. Query bar
/// plus match count on top, results grouped under collapsible file headers.
/// Each match shows surrounding context lines with line numbers and the matched
/// span highlighted. Arrow keys move the cursor over visible matches,
/// Enter/click open the file at the line, Esc exits.
#[allow(clippy::too_many_arguments)] // cohesive view fn; splitting args adds noise
fn vault_search_page<'a>(
    query: &'a str,
    searched_query: Option<&str>,
    hits: &'a [crate::vault_search::VaultHit],
    // Distinct files in `hits`; computed once in VaultSearchDone.
    file_count: usize,
    cursor: usize,
    truncated: bool,
    collapsed: &HashSet<PathBuf>,
    workspace: Option<&std::path::Path>,
    viewport: Option<&iced::widget::scrollable::Viewport>,
    pal: Palette,
) -> Element<'a, Message> {
    // The displayed results reflect `searched_query`; if the live `query` has
    // since been edited, prompt for Enter rather than showing a stale count.
    let edited = searched_query != Some(query);
    let count_text = if query.is_empty() {
        String::new()
    } else if edited {
        "press Enter to search".to_string()
    } else if truncated {
        format!("{}+ matches (refine query)", crate::vault_search::MAX_HITS)
    } else {
        format!("{} matches in {} files", hits.len(), file_count)
    };
    let bar = container(
        irow![
            text_input("Search all files… (press Enter)", query)
                .id(App::vault_input_id())
                .on_input(Message::VaultQueryChanged)
                .on_submit(Message::VaultEnter)
                .padding(Padding::from([8, 12]))
                .size(14)
                .style(move |_, _| iced::widget::text_input::Style {
                    background: Color::TRANSPARENT.into(),
                    border: Border::default(),
                    icon: pal.muted,
                    placeholder: pal.subtle,
                    value: pal.fg,
                    selection: pal.selection,
                }),
            text(count_text).size(12).color(pal.subtle),
        ]
        .spacing(12)
        .align_y(iced::Alignment::Center),
    )
    .padding(Padding::from([6, 14]))
    .width(Length::Fill);

    let mut list = Column::new().spacing(0).padding(Padding::from([6, 10]));
    if query.is_empty() {
        list = list.push(
            container(
                text("Type a query and press Enter to search every file")
                    .color(pal.subtle)
                    .size(13),
            )
            .padding(14),
        );
    } else if edited {
        // Query typed but not yet searched (search runs on Enter, not per key).
        list = list
            .push(container(text("Press Enter to search").color(pal.subtle).size(13)).padding(14));
    } else if hits.is_empty() {
        list = list.push(container(text("No matches").color(pal.subtle).size(13)).padding(14));
    } else {
        // Virtualized results list. Walk all hits once to build a flat row model
        // (one Header per file run, one Hit per visible match) with estimated
        // heights, then render only the rows intersecting the viewport plus an
        // overscan — and always the cursor row, so the cursor-follow scroll
        // Operation can measure its real bounds by anchor id. Skipped rows above
        // and below collapse into `Space` of their summed estimated heights so
        // the scrollbar extent and positions stay correct.
        enum Row {
            Header {
                path: PathBuf,
                run_count: usize,
                folded: bool,
            },
            Hit {
                hi: usize,
                vis_idx: usize,
            },
        }

        // Exact per-row heights. Context lines are fixed single-line rows
        // (SIZE * LINE_H, no wrapping), so these estimates match the real layout
        // — which keeps the virtualization spacers from drifting against the
        // measured scroll offset.
        const LINE_PX: f32 = 12.5 * 1.4; // context_line_row fixed height
        const ROW_GAP: f32 = 1.0; // Column::spacing(1) between context lines
        const ROW_PAD_H: f32 = 12.0; // hit button padding (6 top + 6 bottom)
        const HEADER_H: f32 = 12.0 + 13.0 * 1.3 + 2.0; // header button: pad + 13px line
        let hit_height = |hi: usize| -> f32 {
            let n = hits[hi].context.len() as f32;
            ROW_PAD_H + n * LINE_PX + (n - 1.0).max(0.0) * ROW_GAP
        };

        // Build the row model in file-walk order.
        let mut rows: Vec<Row> = Vec::new();
        let mut vis = 0usize;
        let mut idx = 0usize;
        while idx < hits.len() {
            let path = hits[idx].path.clone();
            let folded = collapsed.contains(&path);
            let run_start = idx;
            while idx < hits.len() && hits[idx].path == path {
                idx += 1;
            }
            let run_count = idx - run_start;
            rows.push(Row::Header {
                path,
                run_count,
                folded,
            });
            if folded {
                continue;
            }
            for hi in run_start..run_start + run_count {
                rows.push(Row::Hit { hi, vis_idx: vis });
                vis += 1;
            }
        }

        // Cumulative tops + total height from the estimates.
        let mut tops: Vec<f32> = Vec::with_capacity(rows.len());
        let mut y = 0.0f32;
        for r in &rows {
            tops.push(y);
            y += match r {
                Row::Header { .. } => HEADER_H,
                Row::Hit { hi, .. } => hit_height(*hi),
            };
        }
        let total_h = y;

        // Viewport window in content coordinates (fall back to "render all" until
        // the first scroll event lands a viewport).
        let virtualize = std::env::var("RMDV_NO_VIRT").is_err();
        let (win_top, win_bot) = match (virtualize, viewport) {
            (true, Some(vp)) => {
                let off = vp.absolute_offset().y;
                let vh = vp.bounds().height;
                const OVERSCAN: f32 = 600.0;
                (off - OVERSCAN, off + vh + OVERSCAN)
            }
            _ => (0.0, total_h),
        };

        let row_h = |i: usize| -> f32 {
            match &rows[i] {
                Row::Header { .. } => HEADER_H,
                Row::Hit { hi, .. } => hit_height(*hi),
            }
        };
        let in_window = |i: usize| -> bool {
            let top = tops[i];
            let bot = top + row_h(i);
            bot >= win_top && top <= win_bot
        };

        // Render with a leading spacer for skipped rows, the windowed rows, and a
        // trailing spacer. The cursor's hit row is force-rendered even if off the
        // window so its anchor id exists for measurement.
        let mut skipped_above = 0.0f32;
        let mut pending_below = 0.0f32;
        let mut started = false;
        for (i, r) in rows.iter().enumerate() {
            let is_cursor_row = matches!(r, Row::Hit { vis_idx, .. } if *vis_idx == cursor);
            let render = in_window(i) || is_cursor_row;
            if !render {
                if started {
                    pending_below += row_h(i);
                } else {
                    skipped_above += row_h(i);
                }
                continue;
            }
            if !started {
                if skipped_above > 0.0 {
                    list = list.push(Space::new().height(skipped_above));
                }
                started = true;
            } else if pending_below > 0.0 {
                // Reclaim a gap created by jumping to the cursor row out of window.
                list = list.push(Space::new().height(pending_below));
                pending_below = 0.0;
            }

            match r {
                Row::Header {
                    path,
                    run_count,
                    folded,
                } => {
                    let rel = workspace
                        .and_then(|ws| path.strip_prefix(ws).ok())
                        .unwrap_or(path)
                        .to_string_lossy()
                        .into_owned();
                    let chevron = if *folded {
                        icon::ic::CHEVRON_RIGHT
                    } else {
                        icon::ic::CHEVRON_DOWN
                    };
                    let header_label = if *folded {
                        format!("{rel}  ({run_count})")
                    } else {
                        rel
                    };
                    let header_row = irow![
                        icon::glyph(chevron, 13.0, pal.accent),
                        text(header_label).size(13).color(pal.accent),
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center);
                    let path = path.clone();
                    let header = button(header_row)
                        .padding(Padding::from([6, 8]))
                        .width(Length::Fill)
                        .style(move |_, status| button::Style {
                            background: match status {
                                button::Status::Hovered => Some(Background::Color(pal.surface_alt)),
                                _ => None,
                            },
                            text_color: pal.accent,
                            border: Border {
                                radius: 5.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        })
                        .on_press(Message::VaultToggleFile(path));
                    list = list.push(header);
                }
                Row::Hit { hi, vis_idx } => {
                    let hi = *hi;
                    let is_cursor = *vis_idx == cursor;
                    let hit = &hits[hi];
                    let mut block = Column::new().spacing(1);
                    for cl in &hit.context {
                        block = block.push(context_line_row(
                            cl,
                            hit.col_start,
                            hit.col_end,
                            is_cursor,
                            pal,
                        ));
                    }
                    let row = button(block)
                        .padding(Padding::from([6, 8]))
                        .width(Length::Fill)
                        .style(move |_, status| button::Style {
                            background: match (is_cursor, status) {
                                (true, _) => Some(Background::Color(pal.surface_alt)),
                                (_, button::Status::Hovered) => {
                                    Some(Background::Color(pal.code_bg))
                                }
                                _ => None,
                            },
                            text_color: pal.fg,
                            border: Border {
                                radius: 5.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        })
                        .on_press(Message::VaultOpenHit(hi));
                    // Stable id so cursor-follow can scroll by measured bounds.
                    let row = container(row)
                        .id(App::vault_match_anchor_id(*vis_idx))
                        .width(Length::Fill);
                    list = list.push(row);
                }
            }
        }
        // Trailing spacer for everything skipped after the last rendered row.
        if pending_below > 0.0 {
            list = list.push(Space::new().height(pending_below));
        }
    }

    // Constrain the results column to a comfortable reading width so long lines
    // wrap instead of sprawling edge-to-edge; centre it in the viewport.
    let list = container(container(list).max_width(1100.0).width(Length::Fill))
        .width(Length::Fill)
        .align_x(iced::Alignment::Center);

    let body = scrollable(list)
        .id(App::vault_scroll_id())
        .on_scroll(Message::VaultScrolled)
        .height(Length::Fill)
        .width(Length::Fill)
        .direction(slim_scroll_direction())
        .style(move |_, status| sleek_scrollable_style(status, pal, true));

    let divider = container(Space::new().height(1.0))
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(pal.rule.into()),
            ..Default::default()
        });

    let footer = container(
        text("↑↓ move · ⏎ open · esc exit")
            .size(11)
            .color(pal.subtle),
    )
    .padding(Padding::from([6, 14]))
    .width(Length::Fill);

    container(column![bar, divider, body, footer])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(pal.bg.into()),
            ..Default::default()
        })
        .into()
}

/// One context line: a fixed-width line-number gutter + the source line as
/// markdown-highlighted `rich_text`. On the match line the matched character
/// span gets a highlight background; context lines are dimmed. Long lines wrap
/// within the filled text column (no horizontal blow-out).
fn context_line_row<'a>(
    cl: &'a crate::vault_search::ContextLine,
    col_start: usize,
    col_end: usize,
    is_cursor: bool,
    pal: Palette,
) -> Element<'a, Message> {
    use iced::widget::{rich_text, span};

    const SIZE: f32 = 12.5;
    const LINE_H: f32 = 1.4;

    // Plain (shrink-height) gutter text; a fixed-width box would let the row's
    // height be driven by container sizing rather than the text line.
    let gutter = text(format!("{:>5} ", cl.number))
        .size(SIZE)
        .line_height(LINE_H)
        .font(iced::Font::MONOSPACE)
        .color(pal.subtle);

    // Byte range of the match within this line, for the highlight background.
    let (mb_start, mb_end) = if cl.is_match {
        let s = byte_index_for_char(&cl.text, col_start);
        let e = byte_index_for_char(&cl.text, col_end);
        (s, e)
    } else {
        (0, 0)
    };
    let match_bg = if is_cursor {
        pal.match_current_bg
    } else {
        pal.match_bg
    };

    // Build (byte-range, color) segments from the highlight spans: highlighted
    // ranges get their style colour, gaps get the base colour. Then overlay the
    // match window by splitting any segment that straddles it.
    let line = &cl.text;
    let base_color = if cl.is_match { pal.fg } else { pal.muted };
    let mut segs: Vec<(usize, usize, iced::Color)> = Vec::new();
    let mut cursor = 0usize;
    for sp in &cl.spans {
        let r = sp.range.clone();
        // Spans may overlap (highlight() emits nested captures); drop any that
        // starts inside a range already claimed, like the code-block renderer.
        if r.start < cursor || r.start >= line.len() {
            continue;
        }
        let end = r.end.min(line.len());
        if r.start > cursor {
            segs.push((cursor, r.start, base_color));
        }
        if end > r.start {
            segs.push((r.start, end, crate::render::style_color(sp.style, &pal)));
        }
        cursor = end;
    }
    if cursor < line.len() {
        segs.push((cursor, line.len(), base_color));
    }

    let mut rt: Vec<iced::advanced::text::Span<'a, Message, iced::Font>> = Vec::new();
    for (lo, hi, color) in segs {
        // Split this segment on the match window so the overlap carries match_bg.
        let parts: [(usize, usize, Option<iced::Color>); 3] =
            if cl.is_match && mb_end > lo && mb_start < hi {
                [
                    (lo, mb_start.max(lo), None),
                    (mb_start.max(lo), mb_end.min(hi), Some(match_bg)),
                    (mb_end.min(hi), hi, None),
                ]
            } else {
                [(lo, hi, None), (hi, hi, None), (hi, hi, None)]
            };
        for (a, b, bg) in parts {
            if a >= b {
                continue;
            }
            let mut s = span(&line[a..b])
                .font(iced::Font::MONOSPACE)
                .size(SIZE)
                .line_height(LINE_H)
                .color(color);
            if let Some(c) = bg {
                s = s.background(c);
            }
            rt.push(s);
        }
    }
    if rt.is_empty() {
        rt.push(
            span(" ")
                .font(iced::Font::MONOSPACE)
                .size(SIZE)
                .line_height(LINE_H)
                .color(base_color),
        );
    }

    // Single visual line per source line (Zed-style): no wrapping, so every row
    // is exactly one line tall. This keeps long / CJK / table lines from blowing
    // the row height up vertically AND makes the virtualization height estimate
    // exact. Overflow past the column width is clipped by the parent.
    let body = rich_text(rt)
        .size(SIZE)
        .line_height(LINE_H)
        .wrapping(iced::widget::text::Wrapping::None)
        .width(Length::Fill);

    irow![gutter, body]
        .width(Length::Fill)
        .height(Length::Fixed(SIZE * LINE_H))
        .spacing(4)
        .align_y(iced::Alignment::Center)
        .clip(true)
        .into()
}

/// Byte offset of the `n`-th char in `s` (clamped to `s.len()`).
fn byte_index_for_char(s: &str, n: usize) -> usize {
    s.char_indices().nth(n).map(|(b, _)| b).unwrap_or(s.len())
}

const QUICK_SLOT_SHORTCUT_HINTS: &[(&str, &str)] = &[
    ("⌘1–9", "Activate slots 1 to 9"),
    ("⌘N", "Add current file to next empty slot"),
    ("⌘↑", "Previous slot (outside Zen)"),
    ("⌘↓", "Next slot (outside Zen)"),
    ("⌘W", "Close active slot"),
    ("⌘⇧W", "Close window"),
];

/// Static, read-only keyboard cheatsheet. Grouped by category, no search, no
/// cursor. Esc or backdrop click dismisses (handled by `overlay_frame`).
fn shortcuts_overlay<'a>(pal: Palette) -> Element<'a, Message> {
    // (group title, [(keys, action)]). Hand-authored so we can group by category
    // and include non-command bindings (arrows, Space) the palette omits.
    let groups: [(&str, &[(&str, &str)]); 7] = [
        (
            "File",
            &[
                ("⌘O", "Open Folder"),
                ("⌘R", "Refresh File / Folder"),
                ("⌘⌥R", "Reveal File in Finder"),
                ("⌘⌥C", "Copy Focused File Path"),
                ("⌘P", "Find File in Workspace"),
                ("⌘S", "Save"),
            ],
        ),
        (
            "Navigation",
            &[
                ("⌘F", "Find in Document"),
                ("⌘⇧F", "Search All Files"),
                ("Home / g", "Reader top (outside Zen)"),
                ("End / G", "Reader bottom (outside Zen)"),
                ("↑ ↓", "Move outline / tree selection"),
                ("Enter", "Jump to selection"),
            ],
        ),
        (
            "View",
            &[
                ("⌘B", "Toggle Sidebar"),
                ("⌘E", "Toggle Zen Edit"),
                ("Esc", "Exit Zen Edit"),
                ("⌘T", "Cycle Theme"),
                ("⌘⇧.", "Toggle Hidden Files"),
                ("⌘+ ⌘-", "Reader Font Size Up / Down"),
                ("⌘0", "Reset Reader Font Size"),
                ("⌘⇧P", "Command Palette"),
            ],
        ),
        (
            "Edit",
            &[
                ("⌘← ⌘→", "Zen line start / end"),
                ("⌘↑ ⌘↓", "Zen document start / end"),
                ("⌘S", "Save"),
            ],
        ),
        (
            "Mindmap",
            &[
                ("⌘M", "Toggle Mindmap"),
                ("⌘K 0–6", "Show Through Node Level"),
                ("⌘⌥B", "Toggle Panel"),
                ("⌘⌥W", "Cycle Panel Width"),
                ("= −", "Zoom Graph In / Out"),
                ("0", "Reset Graph Zoom (100%)"),
                ("← ↑ → ↓", "Navigate nodes"),
                ("Space", "Fold / unfold node"),
            ],
        ),
        ("Quick Slots", QUICK_SLOT_SHORTCUT_HINTS),
        ("Help", &[("⌘/", "Show Shortcuts")]),
    ];

    // Three balanced columns so the sheet is compact and nothing clips:
    // File + Quick Slots | Navigation + Mindmap | View + Edit + Help.
    let columns = [
        vec![groups[0], groups[5]],
        vec![groups[1], groups[4]],
        vec![groups[2], groups[3], groups[6]],
    ];

    let mut cols = irow![].spacing(24);
    for col_groups in columns {
        let mut col = Column::new().spacing(2).width(Length::Fixed(300.0));
        for (gi, (title, rows)) in col_groups.iter().enumerate() {
            let top = if gi == 0 { 0.0 } else { 18.0 };
            let mut header_font = iced::Font::with_name("Inter");
            header_font.weight = iced::font::Weight::Semibold;
            col = col.push(
                container(text(*title).size(11).color(pal.muted).font(header_font)).padding(
                    Padding {
                        top,
                        bottom: 5.0,
                        left: 2.0,
                        right: 0.0,
                    },
                ),
            );
            for (keys, action) in rows.iter() {
                let row = irow![
                    container(key_caps(keys, pal)).width(Length::Fixed(118.0)),
                    text(*action).size(13).color(pal.fg),
                ]
                .spacing(12)
                .align_y(iced::Alignment::Center);
                col = col.push(container(row).padding(Padding::from([4, 2])));
            }
        }
        cols = cols.push(col);
    }

    let card = container(cols).padding(Padding::from([34, 40]));

    // Dedicated frame: vertically centered (equal top/bottom margin). Scrim
    // darkness matches the command palette (`overlay_frame`).
    let panel = container(card)
        .max_width(1060.0)
        .max_height(520.0)
        .style(move |_| container::Style {
            background: Some(pal.surface.into()),
            border: Border {
                color: pal.rule,
                width: 1.0,
                radius: 16.0.into(),
            },
            shadow: iced::Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.35),
                offset: iced::Vector::new(0.0, 18.0),
                blur_radius: 60.0,
            },
            ..Default::default()
        });

    let scrim = mouse_area(
        container(Space::new().width(Length::Fill).height(Length::Fill))
            .style(|_| container::Style {
                background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.18))),
                ..Default::default()
            })
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .on_press(Message::CloseOverlay);

    let centered = container(panel)
        .padding(Padding::from([60, 40]))
        .center_x(Length::Fill)
        .center_y(Length::Fill);

    stack![scrim, centered].into()
}

/// Render a shortcut string as a row of square key caps. Space-separated
/// combos; within a combo each character gets its own square box, except
/// multi-letter words (e.g. `Enter`, `Space`) which stay as one wider cap.
fn key_caps<'a>(keys: &str, pal: Palette) -> Element<'a, Message> {
    let mut row = irow![].spacing(4).align_y(iced::Alignment::Center);
    for combo in keys.split(' ').filter(|s| !s.is_empty()) {
        let is_word = combo.chars().count() > 1 && combo.chars().all(|c| c.is_ascii_alphabetic());
        let caps: Vec<String> = if is_word {
            vec![combo.to_string()]
        } else {
            combo.chars().map(|c| c.to_string()).collect()
        };
        for cap in caps {
            let multi = cap.chars().count() > 1;
            let cap_text = text(cap).size(12).color(pal.fg).font(editor_font());
            // Square (24x24) for single glyphs; wider but same height for words.
            let w = if multi {
                Length::Shrink
            } else {
                Length::Fixed(24.0)
            };
            row = row.push(
                container(cap_text)
                    .width(w)
                    .height(Length::Fixed(24.0))
                    .padding(if multi {
                        Padding::from([0, 8])
                    } else {
                        Padding::ZERO
                    })
                    .align_x(iced::alignment::Horizontal::Center)
                    .align_y(iced::alignment::Vertical::Center)
                    .style(move |_| container::Style {
                        background: Some(pal.surface_alt.into()),
                        border: Border {
                            color: pal.rule,
                            width: 1.0,
                            radius: 5.0.into(),
                        },
                        ..Default::default()
                    }),
            );
        }
    }
    row.into()
}

fn command_overlay<'a>(
    query: &'a str,
    cmds: Vec<(&'static str, Message, i32)>,
    selected: usize,
    pal: Palette,
) -> Element<'a, Message> {
    let input = container(
        text_input("Run a command…", query)
            .id(App::overlay_input_id())
            .on_input(Message::OverlayQueryChanged)
            .on_submit(Message::OverlayConfirm)
            .padding(Padding::from([10, 14]))
            .size(14)
            .style(move |_, _| iced::widget::text_input::Style {
                background: Color::TRANSPARENT.into(),
                border: Border::default(),
                icon: pal.muted,
                placeholder: pal.subtle,
                value: pal.fg,
                selection: pal.selection,
            }),
    );

    let mut list = Column::new().spacing(0).padding(Padding::from([6, 8]));
    if cmds.is_empty() {
        list = list.push(container(text("No commands").color(pal.subtle).size(13)).padding(14));
    } else {
        for (i, (label, msg, _)) in cmds.into_iter().enumerate() {
            let is_sel = i == selected;
            let row = button(text(label).size(13).color(pal.fg))
                .padding(Padding::from([7, 12]))
                .width(Length::Fill)
                .height(Length::Fixed(32.0))
                .style(move |_, status| button::Style {
                    background: match (is_sel, status) {
                        (true, _) => Some(Background::Color(pal.surface_alt)),
                        (_, button::Status::Hovered) => Some(Background::Color(pal.surface_alt)),
                        _ => None,
                    },
                    text_color: pal.fg,
                    border: Border {
                        radius: 6.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .on_press(msg);
            list = list.push(row);
        }
    }

    let body = scrollable(list)
        .id(App::overlay_scroll_id())
        .on_scroll(Message::OverlayScrolled)
        .height(Length::Fill)
        .direction(slim_scroll_direction())
        .style(move |_, status| sleek_scrollable_style(status, pal, true));

    let divider = container(Space::new().height(1.0))
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(pal.rule.into()),
            ..Default::default()
        });

    overlay_frame(column![input, divider, body].into(), pal, 560.0, 420.0)
}

fn theme_overlay<'a>(
    query: &'a str,
    themes: Vec<ThemeEntry>,
    selected: usize,
    current: theme::ThemeId,
    pal: Palette,
) -> Element<'a, Message> {
    let input = container(
        text_input("Pick theme…", query)
            .id(App::overlay_input_id())
            .on_input(Message::OverlayQueryChanged)
            .on_submit(Message::OverlayConfirm)
            .padding(Padding::from([10, 14]))
            .size(14)
            .style(move |_, _| iced::widget::text_input::Style {
                background: Color::TRANSPARENT.into(),
                border: Border::default(),
                icon: pal.muted,
                placeholder: pal.subtle,
                value: pal.fg,
                selection: pal.selection,
            }),
    );

    let mut list = Column::new().spacing(0).padding(Padding::from([6, 8]));
    for (i, t) in themes.into_iter().enumerate() {
        let is_sel = i == selected;
        let is_current = t.matches_current(&current);
        let swatch_pal = t.palette();
        let swatch = container(
            Space::new()
                .width(Length::Fixed(14.0))
                .height(Length::Fixed(14.0)),
        )
        .style(move |_| container::Style {
            background: Some(swatch_pal.accent.into()),
            border: Border {
                color: swatch_pal.rule,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        });
        let bg_swatch = container(
            Space::new()
                .width(Length::Fixed(14.0))
                .height(Length::Fixed(14.0)),
        )
        .style(move |_| container::Style {
            background: Some(swatch_pal.bg.into()),
            border: Border {
                color: swatch_pal.rule,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        });
        let label = t.label().to_string();
        let msg = t.message();
        let marker: Element<'a, Message> = if is_current {
            icon::glyph(ic::CHECK, 12.0, pal.accent).into()
        } else {
            Space::new().width(12.0).into()
        };
        let row = button(
            irow![
                marker,
                Space::new().width(4),
                bg_swatch,
                Space::new().width(2),
                swatch,
                Space::new().width(8),
                text(label).size(13).color(pal.fg),
            ]
            .align_y(iced::Alignment::Center),
        )
        .padding(Padding::from([7, 12]))
        .width(Length::Fill)
        .style(move |_, status| button::Style {
            background: match (is_sel, status) {
                (true, _) => Some(Background::Color(pal.surface_alt)),
                (_, button::Status::Hovered) => Some(Background::Color(pal.surface_alt)),
                _ => None,
            },
            text_color: pal.fg,
            border: Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .on_press(msg);
        list = list.push(row);
    }

    let body = scrollable(list)
        .id(App::overlay_scroll_id())
        .on_scroll(Message::OverlayScrolled)
        .height(Length::Fill)
        .direction(slim_scroll_direction())
        .style(move |_, status| sleek_scrollable_style(status, pal, true));

    let divider = container(Space::new().height(1.0))
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(pal.rule.into()),
            ..Default::default()
        });

    overlay_frame(column![input, divider, body].into(), pal, 480.0, 420.0)
}

fn picker_hint_footer<'a>(pal: Palette) -> Element<'a, Message> {
    let hint = |k: &'static str, label: &'static str| -> Element<'a, Message> {
        irow![
            container(text(k).size(11).color(pal.fg))
                .padding(Padding::from([2, 6]))
                .style(move |_| container::Style {
                    background: Some(pal.surface_alt.into()),
                    border: Border {
                        color: pal.rule,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                }),
            Space::new().width(6),
            text(label).size(11).color(pal.subtle),
        ]
        .align_y(iced::Alignment::Center)
        .into()
    };
    let divider = container(Space::new().height(1.0))
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(pal.rule.into()),
            ..Default::default()
        });
    let row = irow![
        hint("↑↓", "navigate"),
        Space::new().width(14),
        hint("←", "up"),
        Space::new().width(14),
        hint("→", "descend"),
        Space::new().width(14),
        hint("␣", "descend / open"),
        Space::new().width(14),
        hint("↵", "open"),
        Space::new().width(Length::Fill),
        hint("⎋", "close"),
    ]
    .align_y(iced::Alignment::Center);
    column![
        divider,
        container(row)
            .padding(Padding::from([8, 14]))
            .width(Length::Fill),
    ]
    .into()
}

/// A compact inline row of `key — label` hint pills, matching the picker footer
/// style (surface_alt cap, rule border, subtle label). Reused for floating mind
/// map hints and the sidebar tab-row hint.
fn hint_pills<'a>(items: &[(&'a str, &'a str)], pal: Palette) -> Element<'a, Message> {
    let mut row = irow![].align_y(iced::Alignment::Center);
    for (i, (k, label)) in items.iter().enumerate() {
        if i > 0 {
            row = row.push(Space::new().width(12));
        }
        let pill = irow![
            container(text(k.to_string()).size(11).color(pal.fg))
                .padding(Padding::from([2, 6]))
                .style(move |_| container::Style {
                    background: Some(pal.surface_alt.into()),
                    border: Border {
                        color: pal.rule,
                        width: 1.0,
                        radius: 4.0.into(),
                    },
                    ..Default::default()
                }),
            Space::new().width(6),
            text(label.to_string()).size(11).color(pal.subtle),
        ]
        .align_y(iced::Alignment::Center);
        row = row.push(pill);
    }
    row.into()
}

/// Floating mind map keyboard hint. It sits over the canvas so side panels
/// remain dedicated to their selected document or folder preview.
fn floating_mindmap_hint<'a>(items: &[(&'a str, &'a str)], pal: Palette) -> Element<'a, Message> {
    let island = container(hint_pills(items, pal))
        .padding(Padding::from([8, 16]))
        .clip(true)
        .style(move |_| container::Style {
            background: Some(pal.surface.into()),
            border: Border {
                color: pal.rule,
                width: 1.0,
                radius: 10.0.into(),
            },
            shadow: iced::Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.22),
                offset: iced::Vector::new(0.0, 5.0),
                blur_radius: 14.0,
            },
            ..Default::default()
        });

    container(island)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            top: 0.0,
            right: 0.0,
            bottom: 16.0,
            left: 0.0,
        })
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Bottom)
        .into()
}

fn overlay_frame<'a>(
    content: Element<'a, Message>,
    pal: Palette,
    max_w: f32,
    max_h: f32,
) -> Element<'a, Message> {
    let panel = container(content)
        .max_width(max_w)
        .max_height(max_h)
        .width(Length::Fill)
        .height(Length::Fill)
        .clip(true)
        .style(move |_| container::Style {
            background: Some(pal.surface.into()),
            border: Border {
                color: pal.rule,
                width: 1.0,
                radius: 14.0.into(),
            },
            shadow: iced::Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.28),
                offset: iced::Vector::new(0.0, 14.0),
                blur_radius: 50.0,
            },
            ..Default::default()
        });

    let scrim = mouse_area(
        container(Space::new().width(Length::Fill).height(Length::Fill))
            .style(|_| container::Style {
                background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.18))),
                ..Default::default()
            })
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .on_press(Message::CloseOverlay);

    let centered = container(panel)
        .padding(Padding::from([80, 40]))
        .center_x(Length::Fill)
        .align_y(iced::alignment::Vertical::Top);

    iced::widget::stack![scrim, centered].into()
}

fn slim_scroll_direction() -> scrollable::Direction {
    scrollable::Direction::Vertical(
        scrollable::Scrollbar::new()
            .width(6.0)
            .scroller_width(6.0)
            .margin(2.0),
    )
}

fn slim_scroll_direction_horizontal() -> scrollable::Direction {
    scrollable::Direction::Horizontal(
        scrollable::Scrollbar::new()
            .width(6.0)
            .scroller_width(6.0)
            .margin(2.0),
    )
}

/// Sidebar header padding. On macOS we use `fullsize_content_view`, so the
/// traffic-light buttons overlay the top-left of the client area whenever the
/// window is not fullscreen. In fullscreen the buttons are hidden, so the large
/// reserve collapses to a small top margin for breathing room.
fn sidebar_titlebar_reserve_for_fullscreen(fullscreen: bool) -> f32 {
    #[cfg(target_os = "macos")]
    {
        if fullscreen {
            10.0
        } else {
            22.0
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = fullscreen;
        0.0
    }
}

pub(crate) fn sleek_scrollable_style(
    status: scrollable::Status,
    pal: Palette,
    recently_scrolled: bool,
) -> scrollable::Style {
    let scroller_color = match status {
        scrollable::Status::Dragged { .. } => pal.scroller_hover,
        scrollable::Status::Hovered {
            is_vertical_scrollbar_hovered: true,
            ..
        }
        | scrollable::Status::Hovered {
            is_horizontal_scrollbar_hovered: true,
            ..
        } => pal.scroller_hover,
        _ if recently_scrolled => pal.scroller_hover,
        _ => Color::TRANSPARENT,
    };
    let rail = scrollable::Rail {
        background: None,
        border: Border {
            radius: 8.0.into(),
            ..Default::default()
        },
        scroller: scrollable::Scroller {
            background: Background::Color(scroller_color),
            border: Border {
                radius: 8.0.into(),
                ..Default::default()
            },
        },
    };
    scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
        auto_scroll: scrollable::AutoScroll {
            background: Background::Color(Color::TRANSPARENT),
            border: Border::default(),
            shadow: iced::Shadow::default(),
            icon: Color::TRANSPARENT,
        },
    }
}

/// True for `.tex` files, which route through the LaTeX parser instead of
/// the markdown one.
fn is_tex_path(path: Option<&std::path::Path>) -> bool {
    path.and_then(|p| p.extension())
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("tex"))
}

fn canonicalize_existing_path(path: PathBuf) -> PathBuf {
    std::fs::canonicalize(&path).unwrap_or(path)
}

/// PDFs are extracted to markdown for viewing only; their source isn't editable
/// text, so edit mode (⌘E / `ViewMode::Raw`) is disabled for them.
fn is_pdf_path(path: Option<&std::path::Path>) -> bool {
    path.and_then(|p| p.extension())
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

fn data_lang_for(path: Option<&std::path::Path>) -> Option<&'static str> {
    let ext = path.and_then(|p| p.extension()).and_then(|e| e.to_str())?;
    match ext.to_ascii_lowercase().as_str() {
        "json" => Some("json"),
        "yaml" | "yml" => Some("yaml"),
        "toml" => Some("toml"),
        _ => None,
    }
}

fn collect_preview_assets(
    block: &Block,
    images: &mut Vec<String>,
    diagrams: &mut Vec<(u64, crate::ast::DiagramKind, String)>,
) {
    match block {
        Block::Image { url, .. } => images.push(url.clone()),
        Block::Diagram { hash, kind, source } => {
            diagrams.push((*hash, kind.clone(), source.clone()))
        }
        Block::Blockquote(blocks) => {
            for block in blocks {
                collect_preview_assets(block, images, diagrams);
            }
        }
        Block::List { items, .. } => {
            for item in items {
                for block in &item.blocks {
                    collect_preview_assets(block, images, diagrams);
                }
            }
        }
        _ => {}
    }
}

fn build_full_mindmap_preview_asset_index(
    blocks: &[(BlockId, Block)],
) -> FullMindmapPreviewAssetIndex {
    let mut by_block = HashMap::new();
    for (id, block) in blocks {
        let mut images = Vec::new();
        let mut diagrams = Vec::new();
        collect_preview_assets(block, &mut images, &mut diagrams);
        let mut assets = Vec::with_capacity(images.len() + diagrams.len());
        assets.extend(images.into_iter().map(FullMindmapPreviewAsset::Image));
        assets.extend(
            diagrams
                .into_iter()
                .map(|(hash, kind, source)| FullMindmapPreviewAsset::Diagram {
                    hash,
                    kind,
                    source,
                }),
        );
        if !assets.is_empty() {
            by_block.insert(*id, assets);
        }
    }
    FullMindmapPreviewAssetIndex { by_block }
}

fn prettify_data(lang: &str, src: &str) -> String {
    if lang == "json" {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(src) {
            if let Ok(s) = serde_json::to_string_pretty(&v) {
                return s;
            }
        }
    }
    src.to_string()
}

fn truncate_preview_source(mut source: String) -> (String, bool) {
    if source.len() <= MIND_PANEL_MAX_TEXT_BYTES {
        return (source, false);
    }
    let mut end = MIND_PANEL_MAX_TEXT_BYTES;
    while !source.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    source.truncate(end);
    (source, true)
}

/// Parse and highlight a complete preview away from the Iced update thread.
/// The static highlighter cache is mutex-protected, so worker results remain
/// independent from the current document's mutable `HlCache`.
fn parse_full_mindmap_preview_blocking(path: PathBuf, source: String) -> FullMindmapPreview {
    if let Some(lang) = data_lang_for(Some(&path)) {
        let (source, source_truncated) = truncate_preview_source(source);
        let (pretty, pretty_truncated) = truncate_preview_source(prettify_data(lang, &source));
        return FullMindmapPreview::Data {
            path,
            source: pretty,
            truncated: source_truncated || pretty_truncated,
        };
    }

    let (mut blocks, _) = if is_tex_path(Some(&path)) {
        crate::tex::parse(&source)
    } else {
        parser::parse(&source)
    };
    for (_, block) in &mut blocks {
        if let Block::CodeBlock {
            lang: Some(lang),
            code,
            spans,
        } = block
        {
            if spans.is_empty() {
                *spans = crate::highlight::highlight(lang, code);
            }
        }
    }
    let shape = Arc::new(crate::virt::VirtWindow::shape(
        &blocks,
        &HashSet::new(),
        &crate::virt::HeightCache::default(),
    ));
    let assets = Arc::new(build_full_mindmap_preview_asset_index(&blocks));
    FullMindmapPreview::Document {
        path,
        blocks,
        truncated: false,
        shape: Some(shape),
        assets: Some(assets),
    }
}

fn full_mindmap_preview_settle_stream(
    updates: tokio::sync::watch::Receiver<Option<PendingFullMindmapPreviewSettle>>,
) -> impl futures::Stream<Item = PendingFullMindmapPreviewSettle> {
    futures::stream::unfold((updates, None), |(mut updates, last_emitted)| async move {
        loop {
            let request = loop {
                if let Some(request) = updates.borrow().clone() {
                    if last_emitted.as_ref() != Some(&request) {
                        break request;
                    }
                }
                if updates.changed().await.is_err() {
                    return None;
                }
            };
            let timer = tokio::time::sleep(std::time::Duration::from_millis(
                FULL_MINDMAP_PREVIEW_SETTLE_MS,
            ));
            tokio::pin!(timer);
            loop {
                tokio::select! {
                    changed = updates.changed() => {
                        if changed.is_err() {
                            return None;
                        }
                        // The watch value is the only settle owner. Restart
                        // the quiet window around whatever request is current.
                        break;
                    }
                    _ = &mut timer => {
                        if updates.borrow().as_ref() == Some(&request) {
                            return Some((request.clone(), (updates, Some(request))));
                        }
                        break;
                    }
                }
            }
        }
    })
}

fn full_mindmap_preview_work_gate() -> &'static tokio::sync::Semaphore {
    static GATE: OnceLock<tokio::sync::Semaphore> = OnceLock::new();
    GATE.get_or_init(|| tokio::sync::Semaphore::const_new(1))
}

fn preview_work_is_current(cancel: &AtomicU64, epoch: u64) -> bool {
    cancel.load(Ordering::Acquire) == epoch
}

async fn acquire_full_mindmap_preview_work(
    cancel: &Arc<AtomicU64>,
    epoch: u64,
) -> Result<tokio::sync::SemaphorePermit<'static>, String> {
    loop {
        if !preview_work_is_current(cancel, epoch) {
            return Err(FULL_MINDMAP_PREVIEW_CANCELLED.to_string());
        }
        match full_mindmap_preview_work_gate().try_acquire() {
            Ok(permit) => return Ok(permit),
            Err(tokio::sync::TryAcquireError::NoPermits) => {
                tokio::time::sleep(std::time::Duration::from_millis(8)).await;
            }
            Err(tokio::sync::TryAcquireError::Closed) => {
                return Err("preview worker gate closed".to_string());
            }
        }
    }
}

fn read_full_mindmap_preview_source_blocking(
    path: PathBuf,
    max_bytes: Option<usize>,
    cancel: Option<Arc<AtomicU64>>,
    epoch: u64,
) -> Result<String, String> {
    use std::io::Read;

    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    let mut chunk = vec![0u8; FULL_MINDMAP_PREVIEW_READ_CHUNK_BYTES];
    loop {
        if cancel
            .as_ref()
            .is_some_and(|token| !preview_work_is_current(token, epoch))
        {
            return Err(FULL_MINDMAP_PREVIEW_CANCELLED.to_string());
        }
        let read = file.read(&mut chunk).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
        if let Some(limit) = max_bytes {
            if bytes.len() >= limit {
                bytes.truncate(limit);
                break;
            }
        }
    }
    if cancel
        .as_ref()
        .is_some_and(|token| !preview_work_is_current(token, epoch))
    {
        return Err(FULL_MINDMAP_PREVIEW_CANCELLED.to_string());
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Complete read with a single cooperative worker. A newer selection bumps
/// the token; stale queued work exits before taking the gate and an active
/// read checks between bounded chunks, so rapid navigation cannot retain a
/// backlog of large sources.
async fn load_full_mindmap_preview_guarded(
    path: PathBuf,
    cancel: Arc<AtomicU64>,
    epoch: u64,
) -> Result<(PathBuf, String), String> {
    #[cfg(feature = "pdf")]
    if path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
    {
        return Err("PDF preview unavailable — press Enter to open".to_string());
    }
    let _permit = acquire_full_mindmap_preview_work(&cancel, epoch).await?;
    let max_bytes = data_lang_for(Some(&path)).map(|_| MIND_PANEL_MAX_TEXT_BYTES + 1);
    let read_path = path.clone();
    let read_cancel = Arc::clone(&cancel);
    let source = tokio::task::spawn_blocking(move || {
        read_full_mindmap_preview_source_blocking(read_path, max_bytes, Some(read_cancel), epoch)
    })
    .await
    .map_err(|error| error.to_string())??;
    if !preview_work_is_current(&cancel, epoch) {
        return Err(FULL_MINDMAP_PREVIEW_CANCELLED.to_string());
    }
    Ok((path, source))
}

/// Parse and highlight off the update thread. Parsing uses a small
/// per-navigator capacity (separate from the read gate): an obsolete large
/// parse may finish in parallel with the current selection without
/// monopolizing a process-global slot, while bounded permits prevent a rapid
/// sequence from filling the blocking pool. Epoch checks before and after the
/// operation reject stale results.
async fn parse_full_mindmap_preview_guarded(
    path: PathBuf,
    source: Arc<str>,
    cancel: Arc<AtomicU64>,
    epoch: u64,
    parse_gate: Arc<tokio::sync::Semaphore>,
) -> Result<FullMindmapPreview, String> {
    if !preview_work_is_current(&cancel, epoch) {
        return Err(FULL_MINDMAP_PREVIEW_CANCELLED.to_string());
    }
    let parse_permit = loop {
        if !preview_work_is_current(&cancel, epoch) {
            return Err(FULL_MINDMAP_PREVIEW_CANCELLED.to_string());
        }
        match Arc::clone(&parse_gate).try_acquire_owned() {
            Ok(permit) => break permit,
            Err(tokio::sync::TryAcquireError::NoPermits) => {
                tokio::time::sleep(std::time::Duration::from_millis(8)).await;
            }
            Err(tokio::sync::TryAcquireError::Closed) => {
                return Err("preview parse gate closed".to_string());
            }
        }
    };
    let parse_cancel = Arc::clone(&cancel);
    tokio::task::spawn_blocking(move || {
        let _parse_permit = parse_permit;
        if !preview_work_is_current(&parse_cancel, epoch) {
            return Err(FULL_MINDMAP_PREVIEW_CANCELLED.to_string());
        }
        let preview = parse_full_mindmap_preview_blocking(path, source.to_string());
        if !preview_work_is_current(&parse_cancel, epoch) {
            return Err(FULL_MINDMAP_PREVIEW_CANCELLED.to_string());
        }
        Ok(preview)
    })
    .await
    .map_err(|error| error.to_string())?
}

async fn load_file(p: PathBuf) -> Result<(PathBuf, String), String> {
    let p = canonicalize_existing_path(p);
    #[cfg(feature = "pdf")]
    if p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
    {
        let path = p.clone();
        // PDFium is sync + holds a process-global lock; run off the async runtime.
        let md = tokio::task::spawn_blocking(move || crate::pdf::pdf_to_markdown(&path))
            .await
            .map_err(|e| e.to_string())??;
        return Ok((p, md));
    }
    let bytes = read_document_bytes(&p).await?;
    Ok((p, document_text(bytes)))
}

/// Largest document the main viewer will read. Files past this are refused
/// before reading instead of being pulled whole into memory.
const MAX_DOCUMENT_BYTES: u64 = 64 * 1024 * 1024;

async fn read_document_bytes(p: &Path) -> Result<Vec<u8>, String> {
    use tokio::io::AsyncReadExt;

    let file = tokio::fs::File::open(p).await.map_err(|e| e.to_string())?;
    let len = file.metadata().await.map_err(|e| e.to_string())?.len();
    if len > MAX_DOCUMENT_BYTES {
        return Err(too_large_message(len));
    }
    // The size can change between the check and the read; never read past
    // the cap either way.
    let mut bytes = Vec::with_capacity(len as usize);
    file.take(MAX_DOCUMENT_BYTES + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_DOCUMENT_BYTES {
        return Err(too_large_message(bytes.len() as u64));
    }
    Ok(bytes)
}

fn too_large_message(len: u64) -> String {
    format!(
        "File is too large to open ({} MB; the limit is {} MB)",
        len / (1024 * 1024),
        MAX_DOCUMENT_BYTES / (1024 * 1024)
    )
}

/// Decode without copying valid UTF-8; only invalid input pays for a lossy
/// conversion.
fn document_text(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes)
        .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
}

/// Read the complete Full Mindmap side-panel source on a worker. PDFs remain
/// openable with Enter but are intentionally not converted just for a preview.
#[cfg(test)]
async fn load_full_mindmap_preview(p: PathBuf) -> Result<(PathBuf, String), String> {
    #[cfg(feature = "pdf")]
    if p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
    {
        return Err("PDF preview unavailable — press Enter to open".to_string());
    }
    let source = if data_lang_for(Some(&p)).is_some() {
        let path = p.clone();
        tokio::task::spawn_blocking(move || {
            use std::io::Read;

            let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
            let mut limited = file.take((MIND_PANEL_MAX_TEXT_BYTES + 1) as u64);
            let mut bytes = Vec::with_capacity(MIND_PANEL_MAX_TEXT_BYTES + 1);
            limited
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            Ok::<String, String>(String::from_utf8_lossy(&bytes).into_owned())
        })
        .await
        .map_err(|error| error.to_string())??
    } else {
        tokio::fs::read(&p)
            .await
            .map_err(|error| error.to_string())
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())?
    };
    Ok((p, source))
}

/// Build the tree and file-finder index together on a blocking worker. The
/// tree-side entry/file budgets guarantee a very large project cannot grow
/// this task without bound.
async fn load_workspace_snapshot(
    path: PathBuf,
    show_hidden: bool,
) -> Result<(PathBuf, tree::WorkspaceSnapshot), String> {
    let scan_path = path.clone();
    let snapshot =
        tokio::task::spawn_blocking(move || tree::build_workspace(&scan_path, show_hidden))
            .await
            .map_err(|error| error.to_string())??;
    Ok((path, snapshot))
}

async fn load_full_mindmap_folder(
    path: PathBuf,
    show_hidden: bool,
) -> Result<(PathBuf, tree::ExpandedFolderSnapshot), String> {
    let scan_path = path.clone();
    let snapshot =
        tokio::task::spawn_blocking(move || tree::load_expanded_folder(&scan_path, show_hidden))
            .await
            .map_err(|error| error.to_string())??;
    Ok((path, snapshot))
}

/// Remote images larger than this are refused instead of buffered whole.
const MAX_REMOTE_IMAGE_BYTES: usize = 25 * 1024 * 1024;
/// Concurrent remote image downloads; a document with many images queues the
/// rest instead of opening one connection per image at once.
const REMOTE_IMAGE_CONCURRENCY: usize = 4;

/// One HTTP client for every image fetch, so TLS setup and pooled connections
/// are reused. Iced drives all tasks on a single runtime, which the pool needs.
static IMAGE_CLIENT: std::sync::LazyLock<Result<reqwest::Client, String>> =
    std::sync::LazyLock::new(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .user_agent(concat!("rmdv/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| e.to_string())
    });

static IMAGE_FETCH_PERMITS: std::sync::LazyLock<tokio::sync::Semaphore> =
    std::sync::LazyLock::new(|| tokio::sync::Semaphore::new(REMOTE_IMAGE_CONCURRENCY));

async fn fetch_image(url: String) -> (String, Result<Vec<u8>, String>) {
    let res = async {
        let client = IMAGE_CLIENT.as_ref().map_err(Clone::clone)?;
        let _permit = IMAGE_FETCH_PERMITS
            .acquire()
            .await
            .map_err(|e| e.to_string())?;
        let mut resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("http {}", resp.status()));
        }
        let too_large = || format!("image larger than {} MB", MAX_REMOTE_IMAGE_BYTES >> 20);
        let declared = resp.content_length().unwrap_or(0) as usize;
        if declared > MAX_REMOTE_IMAGE_BYTES {
            return Err(too_large());
        }
        // Stream so an undeclared or lying length still stops at the cap.
        let mut bytes = Vec::with_capacity(declared);
        while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
            if bytes.len() + chunk.len() > MAX_REMOTE_IMAGE_BYTES {
                return Err(too_large());
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok::<Vec<u8>, String>(bytes)
    }
    .await;
    (url, res)
}

/// Rasterize SVG bytes to RGBA. Target ~2048px on the longer side.
pub fn rasterize_svg(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
    use resvg::tiny_skia;
    use resvg::usvg;
    const TARGET: f32 = 2048.0;
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_data(bytes, &opt).map_err(|e| e.to_string())?;
    let sz = tree.size();
    let (w, h) = (sz.width(), sz.height());
    if w <= 0.0 || h <= 0.0 {
        return Err("svg has zero size".into());
    }
    let scale = (TARGET / w.max(h)).max(1.0);
    let pw = (w * scale).round() as u32;
    let ph = (h * scale).round() as u32;
    let mut pixmap = tiny_skia::Pixmap::new(pw, ph).ok_or("pixmap alloc failed")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok((pixmap.take(), pw, ph))
}

pub fn is_svg_bytes(b: &[u8]) -> bool {
    let head = &b[..b.len().min(512)];
    let s = std::str::from_utf8(head).unwrap_or("");
    let s = s.trim_start();
    s.starts_with("<svg") || s.starts_with("<?xml")
}

pub fn is_remote_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

/// True for links that should hand off to the OS rather than open in-app:
/// remote URLs and any scheme:// / mailto:-style target.
pub fn is_external_link(s: &str) -> bool {
    is_remote_url(s) || s.contains("://") || s.starts_with("mailto:") || s.starts_with("tel:")
}

/// GitHub-style heading slug: lowercase, runs of space/`-`/`_` collapse to a
/// single `-`, other punctuation dropped, leading/trailing `-` trimmed. A
/// single space and a run of spaces both yield one `-` so hand-written anchors
/// (`#results-discussion`) match titles with incidental double spacing.
pub fn slugify(title: &str) -> String {
    let mut out = String::new();
    let mut pending_sep = false;
    for c in title.chars() {
        if c.is_alphanumeric() {
            if pending_sep && !out.is_empty() {
                out.push('-');
            }
            pending_sep = false;
            out.extend(c.to_lowercase());
        } else if c == ' ' || c == '-' || c == '_' {
            pending_sep = true;
        }
    }
    out
}

/// Resolve a link `#fragment` to a heading line in `src`, matching the
/// GitHub-style slug of each heading title. `is_tex` selects the LaTeX parser
/// so `.tex` documents' `\section{}` headings are seen.
pub fn line_for_fragment(src: &str, fragment: &str, is_tex: bool) -> Option<u32> {
    let want = slugify(fragment);
    crate::ipc::sections::list_sections_for(src, is_tex)
        .into_iter()
        .find(|s| slugify(&s.title) == want)
        .map(|s| s.line)
}

pub fn resolve_image_path(url: &str, current_file: Option<&std::path::Path>) -> Option<PathBuf> {
    let p = std::path::Path::new(url);
    if p.is_absolute() {
        return Some(p.to_path_buf());
    }
    let base = current_file.and_then(|f| f.parent())?;
    Some(base.join(url))
}

/// Build the in-app navigation message used by link anchors and outline clicks.
fn goto_line_message(line: u32) -> Message {
    Message::ScrollToLine(line)
}

fn apply_goto(
    app: &mut App,
    id: u64,
    line: Option<u32>,
    section: Option<String>,
) -> crate::ipc::Response {
    use crate::ipc::Response;
    if app.ast.is_empty() {
        return Response::err(id, "no file open");
    }
    let target_line = if let Some(sec) = section {
        let sections = &app.outline_sections;
        match crate::ipc::sections::resolve_section_path(&sec, sections) {
            Some(s) => s.line,
            None => return Response::err(id, format!("section \"{sec}\" not found")),
        }
    } else if let Some(l) = line {
        let max_line = app.block_lines.last().copied().unwrap_or(1);
        if l > max_line.saturating_add(1000) {
            return Response::err(
                id,
                format!("line {l} out of range (file ends near line {max_line})"),
            );
        }
        l
    } else {
        return Response::err(id, "goto requires --line or --section");
    };

    let Some(idx) = crate::ipc::lines::block_for_line(target_line, &app.block_lines) else {
        return Response::err(id, "no blocks");
    };
    // Reveal a fold-hidden target (mirrors search nav) and materialize its
    // window so the precise scroll op queued below can find its widget.
    app.unfold_to_reveal(idx);
    app.rebuild_virt_around_block(idx);
    let Some(dpos) = app.virt_window.display_pos(idx) else {
        return Response::err(id, "could not locate block");
    };
    let block_top = app.virt_window.block_top(dpos);
    let block_h = app.virt_window.block_height(dpos);
    // Body estimate + the scrollable's vertical content padding (56 top/bottom).
    let estimated_h = app.virt_window.total_height() + 2.0 * BODY_TOP_PAD;
    let (content_h, view_h) = app
        .body_viewport
        .as_ref()
        .map(|v| {
            (
                v.content_bounds().height.max(estimated_h),
                v.bounds().height,
            )
        })
        .unwrap_or((estimated_h, 0.0));
    let max_scroll = (content_h - view_h).max(1.0);
    let target = BODY_TOP_PAD + block_top + block_h * 0.5 - view_h * 0.38;
    let rel = (target / max_scroll).clamp(0.0, 1.0);
    app.queued_snap = Some(rel);
    app.queued_goto = app.ast.get(idx).map(|(bid, _)| *bid);
    app.nav_anchor = Some(idx);
    crate::ipc::Response::ok(id)
}

fn current_line_estimate(app: &App) -> Option<u32> {
    let v = app.body_viewport.as_ref()?;
    let content_h = v.content_bounds().height;
    let view_h = v.bounds().height;
    if content_h <= view_h {
        return app.block_lines.first().copied();
    }
    let body_off = (v.absolute_offset().y - BODY_TOP_PAD).max(0.0);
    let dpos = app.virt_window.display_pos_at(body_off)?;
    let ast_idx = *app.virt_window.display.get(dpos)?;
    app.block_lines.get(ast_idx).copied()
}

fn diagram_hash_present(blocks: &[(crate::ast::BlockId, Block)], hash: u64) -> bool {
    blocks
        .iter()
        .any(|(_, block)| block_contains_diagram_hash(block, hash))
}

fn block_contains_diagram_hash(block: &Block, hash: u64) -> bool {
    match block {
        Block::Diagram { hash: h, .. } => *h == hash,
        Block::Blockquote(blocks) => blocks
            .iter()
            .any(|block| block_contains_diagram_hash(block, hash)),
        Block::List { items, .. } => items.iter().any(|item| {
            item.blocks
                .iter()
                .any(|block| block_contains_diagram_hash(block, hash))
        }),
        _ => false,
    }
}

fn ipc_subscription_stream() -> impl iced::futures::Stream<Item = Message> {
    iced::stream::channel(
        64,
        |mut out: futures::channel::mpsc::Sender<Message>| async move {
            let listener = match crate::ipc::server::acquire() {
                Ok(l) => l,
                Err(_) => return,
            };
            let (tx, mut rx) = futures::channel::mpsc::channel::<crate::ipc::server::Pending>(64);
            tokio::spawn(crate::ipc::server::run(listener, tx));
            use futures::SinkExt;
            use futures::StreamExt;
            while let Some((req, reply)) = rx.next().await {
                let wrapped = std::sync::Arc::new(std::sync::Mutex::new(Some(reply)));
                if out.send(Message::Ipc(req, wrapped)).await.is_err() {
                    break;
                }
            }
        },
    )
}

#[cfg(test)]
mod tests;
