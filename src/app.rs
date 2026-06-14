use crate::ast::{Block, BlockId, Inline};
use crate::icon::{self, ic};
use crate::parser;
use crate::picker::{self, Picker, PickerMode};
use crate::render::Highlight;
use crate::search::{self, MatchPos};
use crate::theme::{self, Palette, ThemeMode, ThemePreset, Typography};
use crate::tree::{self, Node};
use iced::widget::{
    button, column, container, mouse_area, row as irow, scrollable, stack, text, text_input,
    Column, Space,
};
use iced::{Background, Border, Color, Element, Length, Padding, Task, Theme};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

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

#[derive(Debug, Clone)]
pub enum ImageState {
    Loading,
    Loaded(iced::widget::image::Handle),
    LoadedSvg {
        svg: iced::widget::svg::Handle,
        bytes: std::sync::Arc<Vec<u8>>,
        /// Rasterized variant for zoom modal (filled lazily on first zoom open).
        raster: Option<iced::widget::image::Handle>,
    },
    Failed,
}

/// Retained heap cost of one image entry: encoded/decoded payload bytes.
/// `svg::Handle::from_memory` keeps its own copy of the SVG payload alongside
/// the `bytes` Arc, hence the ×2.
fn image_state_cost(s: &ImageState) -> usize {
    fn handle_cost(h: &iced::widget::image::Handle) -> usize {
        match h {
            iced::widget::image::Handle::Rgba { pixels, .. } => pixels.len(),
            iced::widget::image::Handle::Bytes(_, bytes) => bytes.len(),
            iced::widget::image::Handle::Path(..) => 0,
        }
    }
    match s {
        ImageState::Loading | ImageState::Failed => 0,
        ImageState::Loaded(h) => handle_cost(h),
        ImageState::LoadedSvg { bytes, raster, .. } => {
            bytes.len() * 2 + raster.as_ref().map(handle_cost).unwrap_or(0)
        }
    }
}

/// Soft byte budget for `ImageCache`. Bounds session-long accumulation of
/// fetched remote images and SVG zoom rasters; generous enough that a single
/// document's images never get evicted in realistic use.
const IMAGE_CACHE_BYTE_BUDGET: usize = 256 * 1024 * 1024;

/// Insertion-ordered image cache with a soft byte budget. Entries were
/// previously kept in a bare `HashMap` for the whole session; `trim` (called
/// after each image load) evicts the oldest entries NOT referenced by the
/// current document, so what's on screen never changes.
#[derive(Debug, Default)]
pub struct ImageCache {
    map: HashMap<String, ImageState>,
    /// Insertion order, oldest first. Only ever holds keys present in `map`.
    order: Vec<String>,
    /// Running total of `image_state_cost` over all entries, so the budget
    /// check on each image load is O(1) instead of a full map walk.
    bytes: usize,
}

impl ImageCache {
    pub fn get(&self, key: &str) -> Option<&ImageState> {
        self.map.get(key)
    }

    /// Mutable access for in-place updates (SVG raster fill). Callers that
    /// grow an entry's payload must re-sync the running cost via `resync_cost`.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut ImageState> {
        self.map.get_mut(key)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }

    pub fn insert(&mut self, key: String, value: ImageState) {
        self.bytes += image_state_cost(&value);
        if let Some(replaced) = self.map.insert(key.clone(), value) {
            self.bytes = self.bytes.saturating_sub(image_state_cost(&replaced));
        } else {
            self.order.push(key);
        }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Total retained payload bytes (tracked incrementally; O(1)).
    pub fn cost_bytes(&self) -> usize {
        self.bytes
    }

    /// Recompute the running cost from scratch. Call after mutating an entry
    /// in place through `get_mut`.
    fn resync_cost(&mut self) {
        self.bytes = self.map.values().map(image_state_cost).sum();
    }

    /// Evict oldest-inserted entries until under `budget`, skipping any key
    /// `keep` returns true for (the current document's images). Zero-cost
    /// entries (`Loading`/`Failed`) are never evicted: dropping them can't
    /// reach the budget, and evicting a `Failed` sentinel would silently
    /// re-enable fetch retries that the old unbounded cache never made.
    pub fn trim(&mut self, budget: usize, keep: impl Fn(&str) -> bool) {
        if self.bytes <= budget {
            return;
        }
        let map = &mut self.map;
        let bytes = &mut self.bytes;
        self.order.retain(|key| {
            if *bytes <= budget || keep(key) {
                return true;
            }
            let cost = map.get(key).map(image_state_cost).unwrap_or(0);
            if cost == 0 {
                return true;
            }
            map.remove(key);
            *bytes = bytes.saturating_sub(cost);
            false
        });
    }
}

const SIDEBAR_WIDTH: f32 = 280.0;
const READING_MAX: f32 = 780.0;
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
const MIND_PANEL_MAX: f32 = 900.0;
/// Window-width fractions cycled by ⌘⌥W: 1/3, 1/2, 3/5.
const MIND_PANEL_FRACS: [f32; 3] = [1.0 / 3.0, 0.5, 0.6];
const MIND_PANEL_MAX_BLOCKS: usize = 80;
const MIND_PANEL_MAX_TEXT_BYTES: usize = 24 * 1024;

fn editor_font() -> iced::Font {
    iced::Font {
        family: iced::font::Family::Name("JetBrains Mono"),
        weight: iced::font::Weight::Normal,
        stretch: iced::font::Stretch::Normal,
        style: iced::font::Style::Normal,
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
    OpenWorkspace(PathBuf),
    OpenFolderPicker,
    OpenFileFinder,
    OpenCommandPalette,
    OpenThemePicker,
    OpenVaultSearch,
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
    FileChanged(PathBuf),
    OpenLink(String),
    ToggleTheme,
    SetTheme(ThemePreset),
    SetCustomTheme(String),
    ReloadThemes,
    ThemeFilesChanged,
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
    WindowResized(iced::Size),
    HintSelection,
    FoldChordStart,
    FoldChordCancel,
    FoldToLevel(u8),
    ToggleFold(crate::ast::BlockId),
    HeadingHoverEnter(crate::ast::BlockId),
    HeadingHoverExit(crate::ast::BlockId),
    EditorAction(iced::widget::text_editor::Action),
    SaveFile,
    FileSaved(Result<(), String>),
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

pub struct App {
    pub file: Option<PathBuf>,
    pub source: String,
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
    pub workspace_tree: Option<Node>,
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
    pub tree_viewport: Option<iced::widget::scrollable::Viewport>,
    pub outline_viewport: Option<iced::widget::scrollable::Viewport>,
    /// Latest window size, tracked via `window::resize_events` for ⌘⌥W
    /// fraction-of-window panel sizing. `None` until the first resize event.
    pub window_size: Option<iced::Size>,
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
    pub custom_themes: Vec<crate::theme_load::CustomTheme>,
    pub theme_id: crate::theme::ThemeId,
    pub image_cache: ImageCache,
    pub zoom_url: Option<String>,
    pub view_mode: ViewMode,
    pub editor: Option<iced::widget::text_editor::Content>,
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
    /// Cached mindmap layout, lazily rebuilt from (ast, file, mindmap_collapsed).
    /// Every mutation of those inputs must call `invalidate_mindmap_layout`.
    /// RefCell so `view(&self)` can populate it on first read.
    mindmap_layout: std::cell::RefCell<Option<(std::sync::Arc<Vec<crate::mindmap::MNode>>, iced::Size)>>,
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
    /// Snap-to relative offset queued for the next `update` tick. Used by
    /// `apply_goto` which can't perform scroll math during the IPC handler
    /// without re-entering `update`.
    pub queued_snap: Option<f32>,
    /// Precise-landing companion to `queued_snap`: after the estimate snap,
    /// run a widget operation that centers this block from its real laid-out
    /// bounds (the virt window around it was rebuilt by `apply_goto`).
    pub queued_goto: Option<crate::ast::BlockId>,
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
    /// A downloaded + verified update awaiting user-initiated install. Drives
    /// the update banner. `None` until the background check finds a newer build.
    pub pending_update: Option<crate::update::ReadyUpdate>,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub id: u64,
    pub text: String,
}

impl Default for App {
    fn default() -> Self {
        let mode = ThemeMode::System;
        let preset = theme::resolve_mode(mode);
        // Migrate legacy `mdv` config into `rmdv` before the first read.
        crate::config_migrate::run();
        let prefs = crate::prefs::load();
        Self {
            file: None,
            source: String::new(),
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
            workspace_tree: None,
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
            tree_viewport: None,
            outline_viewport: None,
            window_size: None,
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
            custom_themes: Vec::new(),
            theme_id: crate::theme::ThemeId::Preset(preset),
            image_cache: ImageCache::default(),
            zoom_url: None,
            view_mode: ViewMode::Rendered,
            editor: None,
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
            mindmap_layout: std::cell::RefCell::new(None),
            diagram_cache: crate::diagram::DiagramCache::new(64),
            diagram_theme_id: 0,
            zoom_diagram: None,
            block_lines: Vec::new(),
            pending_nav: None,
            queued_snap: None,
            queued_goto: None,
            virt_window: crate::virt::VirtWindow::default(),
            nav_anchor: None,
            prefs,
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

    fn show_toast(&mut self, text: String) -> Task<Message> {
        self.toast_seq = self.toast_seq.wrapping_add(1);
        let id = self.toast_seq;
        self.toast = Some(Toast { id, text });
        Task::perform(
            async {
                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            },
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
                    Task::perform(load_file(p), Message::FileLoaded)
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
        (app, Task::batch([task, update_check]))
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
        // otherwise no-op. Switch to Rendered (and reveal the target if folded).
        self.view_mode = ViewMode::Rendered;
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
    fn mindmap_layout(&self) -> (std::sync::Arc<Vec<crate::mindmap::MNode>>, iced::Size) {
        let mut cache = self.mindmap_layout.borrow_mut();
        if cache.is_none() {
            let (nodes, size) = crate::mindmap::build_layout(
                &self.ast,
                self.file.as_deref(),
                &self.mindmap_collapsed,
            );
            *cache = Some((std::sync::Arc::new(nodes), size));
        }
        let (nodes, size) = cache.as_ref().unwrap();
        (std::sync::Arc::clone(nodes), *size)
    }

    fn invalidate_mindmap_layout(&self) {
        *self.mindmap_layout.borrow_mut() = None;
    }

    /// Select root's first child if nothing is selected, opening the preview
    /// panel. Called on mindmap toggle-on and on file load while in mindmap
    /// mode, so a freshly opened document focuses its first heading.
    fn mindmap_focus_first_child(&mut self) {
        if self.view_mode != ViewMode::Mindmap || self.mindmap_selected.is_some() {
            return;
        }
        let (nodes, _) = self.mindmap_layout();
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
    /// Images referenced by the current document (or the open zoom modal) are
    /// never evicted, so what's on screen never changes.
    fn trim_image_cache(&mut self) {
        if self.image_cache.cost_bytes() <= IMAGE_CACHE_BYTE_BUDGET {
            return;
        }
        let keep: HashSet<&str> = self
            .ast
            .iter()
            .filter_map(|(_, b)| match b {
                Block::Image { url, .. } => Some(url.as_str()),
                _ => None,
            })
            .chain(self.zoom_url.as_deref())
            .collect();
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

    /// Right-side panel shown in Mindmap mode. Renders a bounded markdown slice
    /// for the selected heading so panel open/redraw cannot rebuild huge trees.
    fn mindmap_panel_view(
        &self,
        pal: &Palette,
        hl: &Highlight,
        recently_scrolled: bool,
        panel_width: f32,
    ) -> Element<'_, Message> {
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
        container(scrolled)
            .width(Length::Fixed(panel_width))
            .height(Length::Fill)
            .center_y(Length::Fill)
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

    fn command_items(&self) -> Vec<(&'static str, Message)> {
        vec![
            ("Open Folder…  ⌘O", Message::OpenFolderPicker),
            ("Find File in Workspace…  ⌘P", Message::OpenFileFinder),
            ("Toggle Sidebar  ⌘B", Message::ToggleSidebar),
            ("Toggle Hidden Files  ⌘⇧.", Message::ToggleHidden),
            ("Find in Document  ⌘F", Message::ToggleSearch),
            ("Search All Files…  ⌘⇧F", Message::OpenVaultSearch),
            ("Toggle Raw/Rendered  ⌘E", Message::ToggleViewMode),
            ("Increase Font Size  ⌘+", Message::FontSizeUp),
            ("Decrease Font Size  ⌘-", Message::FontSizeDown),
            ("Reset Font Size  ⌘0", Message::FontSizeReset),
            ("Toggle Status Footer", Message::ToggleFooter),
            ("Toggle Mindmap  ⌘M", Message::ToggleMindmap),
            ("Toggle Mindmap Panel  ⌘⌥B", Message::ToggleMindmapPanel),
            (
                "Cycle Mindmap Panel Width  ⌘⌥W",
                Message::MindmapCyclePanelWidth,
            ),
            (
                "Toggle Mindmap Auto-Center",
                Message::ToggleMindmapAutocenter,
            ),
            ("Cycle Theme  ⌘T", Message::ToggleTheme),
            ("Pick Theme…", Message::OpenThemePicker),
            ("Reload Custom Themes", Message::ReloadThemes),
            ("Scroll to Top  ⌘↑", Message::ScrollToTop),
            ("Scroll to Bottom  ⌘↓", Message::ScrollToBottom),
            (
                "Toggle Auto-Focus on Agent Nav",
                Message::ToggleAutoFocusOnNav,
            ),
            ("Show Keyboard Shortcuts  ⌘/", Message::ToggleShortcuts),
        ]
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
            Message::Open(p) => Task::perform(load_file(p), Message::FileLoaded),
            Message::OpenWorkspace(p) => {
                self.workspace_files = picker::walk_markdown(&p, 8, 5000, self.show_hidden);
                self.workspace_tree = Some(tree::build(&p, self.show_hidden));
                self.expanded.clear();
                if let Some(t) = &self.workspace_tree {
                    self.expanded.insert(t.path.clone());
                }
                self.workspace = Some(p);
                self.sidebar_open = true;
                self.tree_cursor = 0;
                self.overlay = Overlay::None;
                self.picker = None;
                Task::none()
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
                let next =
                    (self.vault_cursor as isize + d).clamp(0, visible.len() as isize - 1);
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
            Message::ImageFetched(url, Ok(bytes)) => {
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
                if self.is_data_doc {
                    return self.show_toast("Fold for data formats not yet supported".into());
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
                Task::batch([
                    self.measure_window_heights(),
                    self.show_toast(format!("Font {:.0} px", size)),
                ])
            }
            Message::FontSizeDown => {
                let size = self.adjust_font_scale(1.0 / 1.1);
                self.height_cache.clear();
                self.rebuild_virt_here();
                Task::batch([
                    self.measure_window_heights(),
                    self.show_toast(format!("Font {:.0} px", size)),
                ])
            }
            Message::FontSizeReset => {
                self.font_scale = 1.0;
                self.typography = self.typography_base;
                self.height_cache.clear();
                self.rebuild_virt_here();
                Task::batch([
                    self.measure_window_heights(),
                    self.show_toast("Font reset".to_string()),
                ])
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
                let restore = self.restore_body_scroll();
                match self.view_mode {
                    ViewMode::Rendered => {
                        self.editor = Some(iced::widget::text_editor::Content::with_text(
                            self.source.as_str(),
                        ));
                        self.edit_history.clear();
                        self.edit_redo.clear();
                        self.dirty = false;
                        self.view_mode = ViewMode::Raw;
                    }
                    ViewMode::Raw => {
                        if let Some(ed) = self.editor.take() {
                            let text = ed.text();
                            if text != self.source {
                                self.source = text;
                                self.reparse_source();
                            }
                        }
                        self.edit_history.clear();
                        self.edit_redo.clear();
                        self.view_mode = ViewMode::Rendered;
                    }
                    ViewMode::Mindmap => {
                        self.mindmap_panel_drag = None;
                        self.editor = Some(iced::widget::text_editor::Content::with_text(
                            self.source.as_str(),
                        ));
                        self.edit_history.clear();
                        self.edit_redo.clear();
                        self.dirty = false;
                        self.view_mode = ViewMode::Raw;
                    }
                }
                restore
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
                        if let Some(ed) = self.editor.take() {
                            let text = ed.text();
                            if text != self.source {
                                self.source = text;
                                self.reparse_source();
                            }
                        }
                        self.edit_history.clear();
                        self.edit_redo.clear();
                        self.view_mode = ViewMode::Mindmap;
                    }
                    ViewMode::Rendered => self.view_mode = ViewMode::Mindmap,
                }
                // On first open (no selection yet), focus root's first child so
                // arrow nav and the preview panel start at the top heading.
                self.mindmap_focus_first_child();
                restore
            }
            Message::MindmapToggleNode(id) => {
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
                let (nodes, _) = self.mindmap_layout();
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
                    return Task::none();
                };
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
                            tokio::time::sleep(std::time::Duration::from_millis(75)),
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
            Message::WindowResized(size) => {
                self.window_size = Some(size);
                Task::none()
            }
            Message::MindmapCyclePanelWidth => {
                // Open the panel if it was closed so the size change is visible.
                self.mindmap_panel_open = true;
                self.mindmap_panel_drag = None;
                self.mindmap_panel_step = (self.mindmap_panel_step + 1) % MIND_PANEL_FRACS.len();
                let frac = MIND_PANEL_FRACS[self.mindmap_panel_step];
                // Fall back to the default px width until the first resize event
                // gives us a real window width.
                let target = match self.window_size {
                    Some(s) => s.width * frac,
                    None => MIND_PANEL_DEFAULT,
                };
                self.mindmap_panel_width = target.clamp(MIND_PANEL_MIN, MIND_PANEL_MAX);
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
                            let dx = ax - cursor_x;
                            self.mindmap_panel_width =
                                (orig_w + dx).clamp(MIND_PANEL_MIN, MIND_PANEL_MAX);
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
                if let Some(ed) = self.editor.as_mut() {
                    let edits = action.is_edit();
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
                        self.dirty = true;
                    }
                }
                Task::none()
            }
            Message::EditorUndo => {
                if let Some(ed) = self.editor.as_mut() {
                    if let Some(prev) = self.edit_history.pop() {
                        let current = ed.text();
                        self.edit_redo.push(current);
                        *ed = iced::widget::text_editor::Content::with_text(&prev);
                        self.dirty = prev != self.source;
                    }
                }
                Task::none()
            }
            Message::EditorRedo => {
                if let Some(ed) = self.editor.as_mut() {
                    if let Some(next) = self.edit_redo.pop() {
                        let current = ed.text();
                        self.edit_history.push(current);
                        *ed = iced::widget::text_editor::Content::with_text(&next);
                        self.dirty = next != self.source;
                    }
                }
                Task::none()
            }
            Message::SaveFile => {
                let Some(path) = self.file.clone() else {
                    return Task::none();
                };
                let text = match self.editor.as_ref() {
                    Some(ed) => ed.text(),
                    None => self.source.clone(),
                };
                self.source = text.clone();
                self.reparse_source();
                self.dirty = false;
                let prime = self.prime_diagram_cache();
                Task::batch([
                    Task::perform(
                        async move {
                            tokio::fs::write(&path, text)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        Message::FileSaved,
                    ),
                    prime,
                ])
            }
            Message::FileSaved(Ok(())) => self.show_toast("✓ Saved".into()),
            Message::FileSaved(Err(e)) => {
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
                let parent = path.parent().map(|p| p.to_path_buf());
                let load = Task::perform(load_file(path), Message::FileLoaded);
                if let Some(parent) = parent {
                    Task::batch([Task::done(Message::OpenWorkspace(parent)), load])
                } else {
                    load
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
                        return Task::perform(load_file(p), Message::FileLoaded);
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
            Message::FileLoaded(Ok((path, src))) => {
                crate::recent::add(&path);
                if self.workspace.is_none() {
                    if let Some(parent) = path.parent().map(PathBuf::from) {
                        self.workspace_files =
                            picker::walk_markdown(&parent, 8, 5000, self.show_hidden);
                        self.workspace_tree = Some(tree::build(&parent, self.show_hidden));
                        self.expanded.clear();
                        if let Some(t) = &self.workspace_tree {
                            self.expanded.insert(t.path.clone());
                        }
                        self.workspace = Some(parent);
                        self.tree_cursor = 0;
                    }
                }
                // Opening a DIFFERENT file: the body scrollable's offset gets
                // clamped by iced on the next layout, but if the new content
                // fits the viewport no scroll notification ever fires — the
                // stale viewport would poison body-offset math (current-line
                // estimate, virt window). Watcher reloads of the same file
                // keep it, preserving scroll position.
                if self.file.as_deref() != Some(path.as_path()) {
                    self.body_viewport = None;
                }
                self.source = src;
                self.file = Some(path);
                self.outline_cursor = 0;
                self.is_data_doc = data_lang_for(self.file.as_deref()).is_some();
                self.mindmap_collapsed.clear();
                self.mindmap_selected = None;
                self.mindmap_panel_shown = None;
                self.load_ast_from_source();
                self.error = None;
                self.rebuild_matches();
                // Opening a file while in mindmap mode: focus root's first child
                // (file load cleared the selection above).
                self.mindmap_focus_first_child();
                self.reveal_current_file();
                let mut fetches: Vec<Task<Message>> = Vec::new();
                for (_id, b) in &self.ast {
                    if let Block::Image { url, .. } = b {
                        if is_remote_url(url) && !self.image_cache.contains_key(url) {
                            self.image_cache.insert(url.clone(), ImageState::Loading);
                            let u = url.clone();
                            fetches.push(Task::perform(fetch_image(u), |(url, res)| {
                                Message::ImageFetched(url, res)
                            }));
                        }
                    }
                }
                self.refresh_diagram_theme_id();
                let prime = self.prime_diagram_cache();
                let nav_task: Task<Message> = if let Some(nav) = self.pending_nav.take() {
                    // A link `#fragment` resolves to a line via slug matching;
                    // IPC `line`/`section` pass through unchanged.
                    let line = nav
                        .fragment
                        .as_deref()
                        .and_then(|f| {
                            line_for_fragment(&self.source, f, is_tex_path(self.file.as_deref()))
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
                Task::batch(fetches)
            }
            Message::FileChanged(p) => {
                if self.dirty {
                    return self.show_toast("External change ignored (unsaved edits)".into());
                }
                Task::perform(load_file(p), Message::FileLoaded)
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
                let toast = self.show_toast(label);
                if changed {
                    Task::batch([toast, self.prime_diagram_cache()])
                } else {
                    toast
                }
            }
            Message::SetTheme(t) => {
                self.theme_preset = t;
                self.palette = theme::palette_for(t);
                self.theme_id = theme::ThemeId::Preset(t);
                let changed = self.refresh_diagram_theme_id();
                let toast = self.show_toast(t.label().to_string());
                if changed {
                    Task::batch([toast, self.prime_diagram_cache()])
                } else {
                    toast
                }
            }
            Message::SetCustomTheme(slug) => {
                if let Some(t) = self.custom_themes.iter().find(|t| t.slug == slug) {
                    let (palette, typography, label) = (t.palette, t.typography, t.name.clone());
                    self.palette = palette;
                    self.set_typography_base(typography);
                    self.theme_id = theme::ThemeId::Custom(slug.clone());
                    let changed = self.refresh_diagram_theme_id();
                    let toast = self.show_toast(label);
                    if changed {
                        Task::batch([toast, self.prime_diagram_cache()])
                    } else {
                        toast
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
                let toast =
                    self.show_toast(format!("{n} custom theme{}", if n == 1 { "" } else { "s" }));
                if changed {
                    Task::batch([toast, self.prime_diagram_cache()])
                } else {
                    toast
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
                if active_changed && self.refresh_diagram_theme_id() {
                    Task::batch([toast, self.prime_diagram_cache()])
                } else {
                    toast
                }
            }
            Message::ToastExpire(id) => {
                if let Some(t) = &self.toast {
                    if t.id == id {
                        self.toast = None;
                    }
                }
                Task::none()
            }
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
                self.show_hidden = !self.show_hidden;
                // Rebuild tree + workspace_files with the new filter. Keep
                // expanded paths; any node that disappears just won't show.
                if let Some(ws) = self.workspace.clone() {
                    self.workspace_files = picker::walk_markdown(&ws, 8, 5000, self.show_hidden);
                    self.workspace_tree = Some(tree::build(&ws, self.show_hidden));
                }
                // If a picker is open, rebuild its view too.
                if let Some(p) = self.picker.as_mut() {
                    p.show_hidden = self.show_hidden;
                    p.refresh();
                }
                let label = if self.show_hidden {
                    "Hidden files: shown".to_string()
                } else {
                    "Hidden files: hidden".to_string()
                };
                self.show_toast(label)
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
                let len = tree::flatten(root, &self.expanded).len();
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
                let rows = tree::flatten(root, &self.expanded);
                let Some(r) = rows.get(self.tree_cursor) else {
                    return Task::none();
                };
                if r.node.is_dir {
                    let p = r.node.path.clone();
                    if !self.expanded.remove(&p) {
                        self.expanded.insert(p);
                    }
                    Task::none()
                } else {
                    let p = r.node.path.clone();
                    Task::perform(load_file(p), Message::FileLoaded)
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
                let rows = tree::flatten(root, &self.expanded);
                let Some(r) = rows.get(self.tree_cursor) else {
                    return Task::none();
                };
                if r.node.is_dir {
                    let p = r.node.path.clone();
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
                let rows = tree::flatten(root, &self.expanded);
                let Some(r) = rows.get(self.tree_cursor) else {
                    return Task::none();
                };
                let path = r.node.path.display().to_string();
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
                self.virt_window.rebuild(
                    &self.ast,
                    &self.folded,
                    &self.height_cache,
                    f32::MAX,
                    vh,
                );
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
                    return self.measure_window_heights();
                }
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
                self.diagram_cache.put((hash, theme_id), state);
                // A Ready diagram replaces its faded-source placeholder with
                // an image of a different height — refresh measured heights.
                self.measure_window_heights()
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
                    let Some(&dpos) = by_id.get(&bid) else { continue };
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
                        self.view_mode = match mode {
                            Mode::View => ViewMode::Rendered,
                            Mode::Edit => ViewMode::Raw,
                            Mode::Mindmap => ViewMode::Mindmap,
                        };
                        nav_focus = Some(focus);
                        Response::ok(id)
                    }
                    Cmd::OpenFolder { dir } => {
                        follow_up =
                            Task::done(Message::OpenWorkspace(std::path::PathBuf::from(dir)));
                        Response::ok(id)
                    }
                    Cmd::Reveal { file, focus } => {
                        follow_up = Task::perform(
                            load_file(std::path::PathBuf::from(file)),
                            Message::FileLoaded,
                        );
                        nav_focus = Some(focus);
                        Response::ok(id)
                    }
                    Cmd::Open {
                        file,
                        line,
                        section,
                        focus,
                    } => {
                        if self.dirty {
                            Response::err(
                                id,
                                format!(
                                    "unsaved edits in {}; save or discard before opening another",
                                    self.file
                                        .as_ref()
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_default()
                                ),
                            )
                        } else {
                            let path = std::path::PathBuf::from(file);
                            follow_up = Task::perform(load_file(path), Message::FileLoaded);
                            self.pending_nav = Some(PendingNav {
                                line,
                                section,
                                ..Default::default()
                            });
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
                };
                Self::reply(&tx, resp);
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
        let focused = self.search_open;
        let overlay_open = self.overlay != Overlay::None;
        let sidebar_open = self.sidebar_open;
        let outline_active = self.sidebar_open && self.sidebar_tab == SidebarTab::Outline;
        let tree_active =
            self.sidebar_open && self.workspace.is_some() && self.sidebar_tab == SidebarTab::Files;
        let editing = self.view_mode == ViewMode::Raw && self.editor.is_some();
        let fold_chord = self.fold_chord_pending;
        let mindmap = self.view_mode == ViewMode::Mindmap;
        let vault_open = self.vault_open;
        let keys = iced::event::listen_with(|ev, status, _id| {
            let is_keyboard = matches!(
                &ev,
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { .. })
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
                ),
                ev,
            )| {
                use iced::keyboard::{key::Named, Event as KEv, Key};
                let (key, physical, mods) = match ev {
                    iced::Event::Keyboard(KEv::KeyPressed {
                        key,
                        physical_key,
                        modifiers,
                        ..
                    }) => (key, physical_key, modifiers),
                    _ => return Message::Noop,
                };
                let cmd = mods.command() || mods.control();
                // ⌘⌥B: alt+letter on macOS swaps the logical char, so match the
                // physical KeyB code instead of the produced character.
                if cmd && mods.alt() {
                    use iced::keyboard::key::{Code, Physical};
                    if let Physical::Code(Code::KeyB) = physical {
                        return Message::ToggleMindmapPanel;
                    }
                    if let Physical::Code(Code::KeyW) = physical {
                        if mindmap {
                            return Message::MindmapCyclePanelWidth;
                        }
                    }
                    if let Physical::Code(Code::KeyC) = physical {
                        if tree_active {
                            return Message::CopyTreePath;
                        }
                    }
                }
                if fold_chord {
                    if let Key::Character(c) = &key {
                        if let Some(d) = c.chars().next().and_then(|ch| ch.to_digit(10)) {
                            if d <= 6 {
                                return Message::FoldToLevel(d as u8);
                            }
                        }
                    }
                    return Message::FoldChordCancel;
                }
                if let Key::Character(c) = &key {
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
                        "=" | "+" if cmd => return Message::FontSizeUp,
                        "-" if cmd => return Message::FontSizeDown,
                        "0" if cmd => return Message::FontSizeReset,
                        "m" if cmd => return Message::ToggleMindmap,
                        "/" if cmd => return Message::ToggleShortcuts,
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
        let drag = if self.sidebar_drag.is_some() {
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
        let mind_drag = if self.view_mode == ViewMode::Mindmap && self.mindmap_panel_drag.is_some()
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
        let ipc = iced::Subscription::run(ipc_subscription_stream);
        let resize = iced::window::resize_events().map(|(_, size)| Message::WindowResized(size));
        iced::Subscription::batch([
            dnd,
            watcher,
            theme_watcher,
            keys,
            scroller,
            drag,
            mind_drag,
            resize,
            ipc,
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

        let reader: Element<'_, Message> = if self.vault_open {
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
                let (nodes, content_size) = self.mindmap_layout();
                let program = crate::mindmap::MindmapProgram {
                    nodes,
                    content_size,
                    palette: pal,
                    selected: self.mindmap_selected,
                    panel_open: self.mindmap_panel_open,
                    panel_width: self.mindmap_panel_width,
                    autocenter: self.mindmap_autocenter,
                    on_toggle: Box::new(Message::MindmapToggleNode),
                    on_select: Box::new(Message::MindmapSelectLeaf),
                    on_deselect: Message::MindmapDeselect,
                };
                let canvas_el: Element<'_, Message> = iced::widget::canvas(program)
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
                    irow![canvas_el, handle, panel].into()
                } else {
                    canvas_el
                }
            } else if self.view_mode == ViewMode::Raw {
                if let Some(ed) = self.editor.as_ref() {
                    iced::widget::text_editor(ed)
                        .on_action(Message::EditorAction)
                        // Filter cmd/ctrl combos so global shortcuts (⌘B, ⌘T,
                        // ⌘E, ⌘K, ⌘M, ⌘P, ⌘O, etc.) don't ALSO get inserted
                        // as text by the editor. Keep ⌘C/⌘X/⌘V/⌘A/⌘Z/⌘Y for
                        // standard editor bindings — those have explicit
                        // handlers upstream that we want to preserve.
                        .key_binding(|kp| {
                            let cmd_or_ctrl = kp.modifiers.command() || kp.modifiers.control();
                            if cmd_or_ctrl {
                                let keep = matches!(
                                    kp.key.to_latin(kp.physical_key),
                                    Some('c' | 'x' | 'v' | 'a' | 'z' | 'y')
                                );
                                if !keep {
                                    return None;
                                }
                            }
                            iced::widget::text_editor::Binding::from_key_press(kp)
                        })
                        .font(editor_font())
                        .size(self.typography.code_size)
                        .line_height(iced::widget::text::LineHeight::Relative(1.55))
                        .height(Length::Fill)
                        .padding(iced::Padding {
                            top: 48.0,
                            right: 32.0,
                            bottom: 24.0,
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
                        .into()
                } else {
                    text(self.source.as_str())
                        .font(iced::Font::MONOSPACE)
                        .size(self.typography.code_size)
                        .color(pal.fg)
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

        let reader_with_search: Element<'_, Message> = if self.search_open {
            column![
                search_bar_view(&self.query, &self.matches, self.match_idx, pal),
                reader,
            ]
            .into()
        } else {
            reader.into()
        };

        let main_area: Element<'_, Message> = if self.sidebar_open && self.workspace.is_some() {
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
        let main_bg = if self.sidebar_open && self.workspace.is_some() {
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
        let footer_layer: Element<'_, Message> =
            if self.show_footer && self.file.is_some() && self.view_mode != ViewMode::Mindmap {
                status_footer(&self.source, pal)
            } else {
                Space::new().into()
            };
        let base: Element<'_, Message> =
            iced::widget::stack![Element::from(main), footer_layer, overlay_layer].into();
        let toast_layer: Element<'_, Message> = match &self.toast {
            Some(t) => toast_overlay(&t.text, pal),
            None => Space::new().into(),
        };
        let update_layer: Element<'_, Message> = match &self.pending_update {
            Some(u) => update_banner(&u.version, pal),
            None => Space::new().into(),
        };
        iced::widget::stack![base, toast_layer, update_layer].into()
    }
}

/// Bottom-center banner inviting the user to install a downloaded update.
fn update_banner<'a>(version: &str, pal: Palette) -> Element<'a, Message> {
    use iced::widget::{button, container, row, text as text_w};
    let label = text_w(format!("rmdv {version} ready to install"))
        .size(13.5)
        .color(pal.fg);
    let install = button(text_w("Install & Restart").size(13.0))
        .padding([5, 12])
        .on_press(Message::InstallUpdate);
    let later = button(text_w("Later").size(13.0))
        .padding([5, 12])
        .on_press(Message::DismissUpdate);
    let bar = container(
        row![label, install, later]
            .spacing(12)
            .align_y(iced::alignment::Vertical::Center),
    )
    .padding([10, 16])
    .style(move |_| container::Style {
        background: Some(pal.surface.into()),
        border: iced::Border {
            color: pal.rule,
            width: 1.0,
            radius: 10.0.into(),
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
fn status_footer<'a>(source: &str, pal: Palette) -> Element<'a, Message> {
    use iced::widget::{container, text as text_w};
    let words = source.split_whitespace().count();
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

fn toast_overlay<'a>(text: &str, pal: Palette) -> Element<'a, Message> {
    use iced::widget::{container, text as text_w};
    let bubble = container(text_w(text.to_string()).size(13.5).color(pal.fg))
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
            kbd("Open Folder", "⌘O"),
            kbd("Find File in Workspace", "⌘P"),
            kbd("Command Palette", "⌘⇧P"),
            kbd("Toggle Sidebar", "⌘B"),
            kbd("Find in Document", "⌘F"),
            kbd("Cycle Theme", "⌘T"),
            kbd("Edit / Select Text", "⌘E"),
            kbd("Fold to Level (then 0–6)", "⌘K"),
        ]
        .spacing(8)
        .align_x(iced::Alignment::Start)
        .into(),
        pal,
    )
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

fn sidebar_view<'a>(app: &'a App, pal: Palette) -> Element<'a, Message> {
    let recently_scrolled = app
        .last_scroll_at
        .is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(SCROLLER_FADE_MS));
    let ws = app.workspace.as_ref().unwrap();
    let ws_name = ws
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("workspace");
    let title_row = irow![
        text(ws_name.to_string().to_uppercase())
            .size(11)
            .color(pal.muted),
        Space::new().width(Length::Fill),
        iced::widget::tooltip(
            ghost_lu(ic::COMMAND, pal).on_press(Message::OpenCommandPalette),
            container(
                text("⌘⇧P")
                    .size(11)
                    .color(pal.fg)
                    .shaping(iced::widget::text::Shaping::Advanced)
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
            }),
            iced::widget::tooltip::Position::Bottom,
        ),
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center);

    let tab_row = irow![
        sidebar_tab_button(
            "Files",
            app.sidebar_tab == SidebarTab::Files,
            SidebarTab::Files,
            pal
        ),
        sidebar_tab_button(
            "Outline",
            app.sidebar_tab == SidebarTab::Outline,
            SidebarTab::Outline,
            pal
        ),
    ]
    .spacing(6);

    let header = container(column![
        Space::new().height(Length::Fixed(sidebar_titlebar_reserve())),
        container(title_row)
            .padding(Padding {
                top: 0.0,
                right: 14.0,
                bottom: 8.0,
                left: 14.0,
            })
            .width(Length::Fill),
        container(tab_row)
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
        let rows = tree::flatten(tree_root, &app.expanded);
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
fn tree_row_width(node: &Node, depth: usize) -> f32 {
    const CHAR_ADVANCE: f32 = 7.0;
    let indent = TREE_INDENT * depth as f32;
    let chevron = 14.0;
    let leaf = 13.0 + 4.0 + 7.0; // icon + gap before + gap after
    let label = node.name.chars().count() as f32 * CHAR_ADVANCE;
    let padding_h = 16.0; // button padding 8 each side
    indent + chevron + leaf + label + padding_h
}

fn tree_row<'a>(
    node: &'a Node,
    depth: usize,
    expanded: &HashSet<PathBuf>,
    current: Option<&'a PathBuf>,
    is_cursor: bool,
    pal: Palette,
) -> Element<'a, Message> {
    let is_current = !node.is_dir && current.map(|c| c == &node.path).unwrap_or(false);
    let path = node.path.clone();

    // Indent area with vertical guides per ancestor level.
    let mut indent = iced::widget::Row::new();
    for _ in 0..depth {
        indent = indent.push(indent_guide(pal));
    }

    let chevron: Element<'a, Message> = if node.is_dir {
        let open = expanded.contains(&node.path);
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
    } else if node.is_dir {
        pal.fg
    } else {
        pal.muted
    };
    let label_weight = if node.is_dir {
        iced::font::Weight::Medium
    } else {
        iced::font::Weight::Normal
    };
    let mut label_font = iced::Font::with_name("Inter");
    label_font.weight = label_weight;
    let label = text(node.name.as_str())
        .size(13)
        .color(label_color)
        .font(label_font)
        .wrapping(text::Wrapping::None);

    let leaf_icon: Element<'a, Message> = if node.is_dir {
        let open = expanded.contains(&node.path);
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

    let on_press = if node.is_dir {
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
                .on_press(Message::Open(path_clone));
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
        list = list.push(
            container(
                text("Press Enter to search")
                    .color(pal.subtle)
                    .size(13),
            )
            .padding(14),
        );
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

/// Static, read-only keyboard cheatsheet. Grouped by category, no search, no
/// cursor. Esc or backdrop click dismisses (handled by `overlay_frame`).
fn shortcuts_overlay<'a>(pal: Palette) -> Element<'a, Message> {
    // (group title, [(keys, action)]). Hand-authored so we can group by category
    // and include non-command bindings (arrows, Space) the palette omits.
    let groups: [(&str, &[(&str, &str)]); 5] = [
        (
            "File",
            &[
                ("⌘O", "Open Folder"),
                ("⌘P", "Find File in Workspace"),
                ("⌘S", "Save"),
            ],
        ),
        (
            "Navigation",
            &[
                ("⌘F", "Find in Document"),
                ("⌘⇧F", "Search All Files"),
                ("⌘↑", "Scroll to Top"),
                ("⌘↓", "Scroll to Bottom"),
                ("↑ ↓", "Move outline / tree selection"),
                ("Enter", "Jump to selection"),
            ],
        ),
        (
            "View",
            &[
                ("⌘B", "Toggle Sidebar"),
                ("⌘E", "Toggle Raw / Rendered"),
                ("⌘T", "Cycle Theme"),
                ("⌘⇧.", "Toggle Hidden Files"),
                ("⌘+ ⌘-", "Font Size Up / Down"),
                ("⌘0", "Reset Font Size"),
                ("⌘⇧P", "Command Palette"),
            ],
        ),
        (
            "Mindmap",
            &[
                ("⌘M", "Toggle Mindmap"),
                ("⌘⌥B", "Toggle Panel"),
                ("⌘⌥W", "Cycle Panel Width"),
                ("← ↑ → ↓", "Navigate nodes"),
                ("Space", "Fold / unfold node"),
            ],
        ),
        ("Help", &[("⌘/", "Show Shortcuts")]),
    ];

    // Three balanced columns so the sheet is compact and nothing clips:
    // File + Navigation | View | Mindmap + Help (each ≤ 8 rows).
    let columns: [&[(&str, &[(&str, &str)])]; 3] =
        [&groups[0..2], &groups[2..3], &groups[3..5]];

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
/// window is not fullscreen. Iced 0.14 exposes no way to query the current
/// window mode, so we always reserve room for the buttons here — when truly
/// fullscreen the extra ~22px of leading space is unused but harmless.
fn sidebar_titlebar_reserve() -> f32 {
    #[cfg(target_os = "macos")]
    {
        22.0
    }
    #[cfg(not(target_os = "macos"))]
    {
        0.0
    }
}

fn sleek_scrollable_style(
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

fn data_lang_for(path: Option<&std::path::Path>) -> Option<&'static str> {
    let ext = path.and_then(|p| p.extension()).and_then(|e| e.to_str())?;
    match ext.to_ascii_lowercase().as_str() {
        "json" => Some("json"),
        "yaml" | "yml" => Some("yaml"),
        "toml" => Some("toml"),
        _ => None,
    }
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

async fn load_file(p: PathBuf) -> Result<(PathBuf, String), String> {
    let bytes = tokio::fs::read(&p).await.map_err(|e| e.to_string())?;
    let s = String::from_utf8_lossy(&bytes).into_owned();
    Ok((p, s))
}

async fn fetch_image(url: String) -> (String, Result<Vec<u8>, String>) {
    let res = async {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .user_agent("rmdv/0.2")
            .build()
            .map_err(|e| e.to_string())?;
        let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("http {}", resp.status()));
        }
        let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
        Ok::<Vec<u8>, String>(bytes.to_vec())
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
mod tests {
    use super::*;
    use crate::ast::{Block, DiagramKind, ListItem};

    #[test]
    fn diagram_hash_present_finds_nested_list_math() {
        let blocks = vec![(
            crate::ast::BlockId(1),
            Block::List {
                ordered: true,
                items: vec![ListItem {
                    task: None,
                    blocks: vec![Block::Diagram {
                        kind: DiagramKind::Math,
                        source: "x".into(),
                        hash: 42,
                    }],
                }],
            },
        )];

        assert!(diagram_hash_present(&blocks, 42));
    }

    fn loaded_image(bytes: usize) -> ImageState {
        ImageState::Loaded(iced::widget::image::Handle::from_bytes(vec![0u8; bytes]))
    }

    #[test]
    fn image_cache_trim_evicts_oldest_unreferenced_first() {
        let mut cache = ImageCache::default();
        cache.insert("a".into(), loaded_image(400));
        cache.insert("b".into(), loaded_image(400));
        cache.insert("c".into(), loaded_image(400));
        assert_eq!(cache.cost_bytes(), 1200);
        cache.trim(800, |_| false);
        assert!(!cache.contains_key("a"), "oldest entry should evict first");
        assert!(cache.contains_key("b"));
        assert!(cache.contains_key("c"));
    }

    #[test]
    fn image_cache_trim_never_evicts_kept_keys() {
        let mut cache = ImageCache::default();
        cache.insert("current-doc".into(), loaded_image(400));
        cache.insert("old-doc".into(), loaded_image(400));
        cache.trim(0, |k| k == "current-doc");
        assert!(cache.contains_key("current-doc"));
        assert!(!cache.contains_key("old-doc"));
    }

    #[test]
    fn image_cache_trim_noop_under_budget() {
        let mut cache = ImageCache::default();
        cache.insert("a".into(), loaded_image(100));
        cache.insert("b".into(), loaded_image(100));
        cache.trim(1024, |_| false);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn image_cache_reinsert_does_not_duplicate_order() {
        let mut cache = ImageCache::default();
        cache.insert("a".into(), ImageState::Loading);
        cache.insert("a".into(), loaded_image(400));
        cache.insert("b".into(), loaded_image(400));
        cache.trim(500, |_| false);
        // "a" (oldest) evicted exactly once; "b" stays.
        assert!(!cache.contains_key("a"));
        assert!(cache.contains_key("b"));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn image_cache_svg_cost_counts_payload_twice_plus_raster() {
        let state = ImageState::LoadedSvg {
            svg: iced::widget::svg::Handle::from_memory(vec![0u8; 100]),
            bytes: std::sync::Arc::new(vec![0u8; 100]),
            raster: Some(iced::widget::image::Handle::from_rgba(
                5,
                5,
                vec![0u8; 100],
            )),
        };
        assert_eq!(image_state_cost(&state), 300);
    }
}
