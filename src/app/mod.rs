use crate::ast::{Block, BlockId, Inline};
use crate::icon::{self, ic};
use crate::parser;
use crate::picker::{self, Picker, PickerMode};
use crate::render::Highlight;
use crate::search::{self, Matches};
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

mod image_cache;
use image_cache::IMAGE_CACHE_BYTE_BUDGET;
pub use image_cache::{ImageCache, ImageState};

const SIDEBAR_WIDTH: f32 = 280.0;
const QUICK_SLOTS_RAIL_GAP: f32 = 10.0;
const QUICK_SLOTS_RAIL_REVEAL_DELAY_MS: u64 = 500;
/// In-document search runs on every keystroke below this source size and is
/// debounced above it, where one search no longer fits inside a frame.
const SEARCH_DEBOUNCE_MIN_BYTES: usize = 1024 * 1024;
const SEARCH_DEBOUNCE_MS: u64 = 80;
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
/// The document Mindmap panel keeps its existing rendered-content debounce
/// cadence, while Full Mindmap previews use a longer quiet window so rapid
/// workspace navigation does not start unnecessary reads.
const MINDMAP_PANEL_SETTLE_MS: u64 = 75;
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

mod dispatch;
mod full_mindmap;
mod full_mindmap_state;
mod full_mindmap_view;
mod ipc_handler;
mod keys;
mod loading;
mod message;
mod paths;
mod preview_worker;
mod scroll_tasks;
mod slots;
mod types;
mod view;
use full_mindmap_state::*;

pub use full_mindmap_state::{
    FullMindmapPreview, FullMindmapPreviewAssetIdentity, FullMindmapPreviewAssetIndex,
    FullMindmapPreviewIdentity, FullMindmapProgress, FullMindmapState, FullMindmapVerificationWave,
    PendingFullMindmapFolderLoad, PendingFullMindmapOpen, PendingFullMindmapPreview,
    PendingFullMindmapPreviewSettle, PendingFullMindmapVerification,
    PendingFullMindmapWorkspaceLoad,
};
use ipc_handler::*;
use keys::*;
use loading::*;
pub use loading::{is_svg_bytes, rasterize_svg};
pub use message::Message;
use paths::*;
pub use paths::{is_external_link, is_remote_url, line_for_fragment, resolve_image_path, slugify};
use preview_worker::*;
use scroll_tasks::*;
use types::*;
pub use types::{
    MindmapDir, Overlay, PendingNav, PendingRefreshFile, PendingRefreshWorkspace,
    PendingWatcherReload, SidebarTab, ThemeEntry, Toast, ToastAction, ViewMode, ZenRestoreState,
};
pub(crate) use view::sleek_scrollable_style;
use view::*;

fn quick_slots_rail_left_offset(sidebar_open: bool, sidebar_width: f32, full_mindmap: bool) -> f32 {
    if sidebar_open && !full_mindmap {
        sidebar_width + QUICK_SLOTS_RAIL_GAP
    } else {
        QUICK_SLOTS_RAIL_GAP
    }
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
    pub matches: Matches,
    pub match_idx: usize,
    /// Bumped per keystroke on large documents; a debounced search only runs
    /// if no newer keystroke arrived (see [`SEARCH_DEBOUNCE_MIN_BYTES`]).
    search_generation: u64,
    /// The query changed but `matches` has not been rebuilt for it yet.
    search_pending: bool,
    pub search_open: bool,
    pub workspace: Option<PathBuf>,
    /// Replace only together with a `workspace_files_rev` bump, so cached
    /// file-finder results are not reused for a different list.
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
    /// Bumped whenever `workspace_files` is replaced; keys `file_finder_memo`.
    workspace_files_rev: u64,
    /// Latest file-finder results; see `filtered_files`.
    file_finder_memo: std::cell::RefCell<Option<FileFinderMemo>>,
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
            matches: Matches::default(),
            match_idx: 0,
            search_generation: 0,
            search_pending: false,
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
            workspace_files_rev: 0,
            file_finder_memo: std::cell::RefCell::new(None),
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

    /// Write `prefs` to the isolated test path when one is set, otherwise to
    /// the user's config.
    fn save_prefs(&self) {
        if let Some(path) = self.quick_slots_persistence_path.as_deref() {
            crate::prefs::save_to(path, &self.prefs);
        } else {
            crate::prefs::save(&self.prefs);
        }
    }

    /// Remember the active theme so the next launch reopens with it.
    fn persist_theme(&mut self) {
        self.prefs.theme = Some(self.theme_id.slug());
        self.save_prefs();
    }

    /// Re-apply the theme saved by [`Self::persist_theme`]. Presets win over a
    /// custom theme with the same slug, matching `rmdv theme <slug>`. An
    /// unknown slug (e.g. a deleted custom theme) keeps the system default.
    fn restore_saved_theme(&mut self) {
        let Some(slug) = self.prefs.theme.clone() else {
            return;
        };
        if let Some(preset) = theme::preset_by_slug(&slug) {
            self.theme_preset = preset;
            self.palette = theme::palette_for(preset);
            self.theme_id = theme::ThemeId::Preset(preset);
        } else if let Some(t) = self.custom_themes.iter().find(|t| t.slug == slug) {
            let (palette, typography) = (t.palette, t.typography);
            self.palette = palette;
            self.set_typography_base(typography);
            self.theme_id = theme::ThemeId::Custom(slug);
        } else {
            return;
        }
        self.refresh_diagram_theme_id();
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
        self.workspace_files_rev = self.workspace_files_rev.wrapping_add(1);
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
        app.restore_saved_theme();
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
        self.search_pending = false;
    }

    /// Run a search the debounce is still holding back, so Enter right after
    /// typing navigates the new query's results.
    fn flush_pending_search(&mut self) {
        if self.search_pending {
            self.rebuild_matches();
        }
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

    /// The top 200 workspace files for the file-finder query. View and key
    /// handling call this on every frame and keypress, so the last answer is
    /// kept until the query, the workspace root, or the file list changes.
    fn filtered_files(&self) -> Vec<(PathBuf, String, i32)> {
        let root = self.workspace.as_ref();
        if let Some(memo) = self.file_finder_memo.borrow().as_ref() {
            if memo.query == self.overlay_query
                && memo.root.as_ref() == root
                && memo.files_rev == self.workspace_files_rev
                && memo.files_len == self.workspace_files.len()
            {
                return memo.results.clone();
            }
        }
        let query = self.overlay_query.to_lowercase();
        let mut scored: Vec<(PathBuf, String, i32)> = self
            .workspace_files
            .iter()
            .filter_map(|p| {
                let rel = root
                    .and_then(|r| p.strip_prefix(r).ok())
                    .map(|x| x.to_string_lossy().into_owned())
                    .unwrap_or_else(|| p.to_string_lossy().into_owned());
                let s = picker::fuzzy_score_lowered(&query, &rel)?;
                Some((p.clone(), rel, s))
            })
            .collect();
        scored.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.cmp(&b.1)));
        scored.truncate(200);
        self.file_finder_memo.replace(Some(FileFinderMemo {
            query: self.overlay_query.clone(),
            root: root.cloned(),
            files_rev: self.workspace_files_rev,
            files_len: self.workspace_files.len(),
            results: scored.clone(),
        }));
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
                            radius: theme::radius::MD.into(),
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

#[cfg(test)]
mod tests;
