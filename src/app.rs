use crate::ast::{Block, BlockId, Inline};
use crate::icon::{self, ic};
use crate::parser;
use crate::picker::{self, Picker, PickerMode};
use crate::render::Highlight;
use crate::search::{self, MatchPos};
use crate::theme::{self, Palette, ThemeMode, ThemePreset, Typography};
use crate::tree::{self, Node};
use iced::widget::{
    button, column, container, mouse_area, row as irow, scrollable, stack, text, text_input, Column, Space,
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

const SIDEBAR_WIDTH: f32 = 280.0;
const READING_MAX: f32 = 780.0;
const TREE_INDENT: f32 = 14.0;
const SCROLLER_FADE_MS: u64 = 1200;
const SIDEBAR_MIN: f32 = 160.0;
const SIDEBAR_MAX: f32 = 600.0;
const MIND_PANEL_DEFAULT: f32 = 380.0;
const MIND_PANEL_MIN: f32 = 240.0;
const MIND_PANEL_MAX: f32 = 900.0;
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
    /// Toggle visibility of dot-prefixed entries in tree + picker.
    ToggleHidden,
    TreeToggle(PathBuf),
    TreeMove(isize),
    TreeActivate,
    TreeToggleAtCursor,
    ScrollBy(f32),
    ScrollToTop,
    ScrollToBottom,
    ToggleSearch,
    QueryChanged(String),
    NextMatch,
    PrevMatch,
    TreeScrolled(iced::widget::scrollable::Viewport),
    OverlayScrolled(iced::widget::scrollable::Viewport),
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
    ToastExpire(u64),
    ImageFetched(String, Result<Vec<u8>, String>),
    SvgRasterized(String, Result<(Vec<u8>, u32, u32), String>),
    OpenImageZoom(String),
    ToggleViewMode,
    ToggleMindmap,
    MindmapToggleNode(crate::ast::BlockId),
    MindmapSelectLeaf(crate::ast::BlockId),
    MindmapDeselect,
    MindmapNavigate(MindmapDir),
    MindmapToggleSelected,
    MindmapPanelDragStart(f32),
    MindmapPanelDragMove(f32),
    MindmapPanelDragEnd,
    ToggleMindmapAutocenter,
    ToggleMindmapPanel,
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
    /// IPC request from the listener subscription. The sender is wrapped in
    /// `Arc<Mutex<Option<…>>>` so the variant is `Clone` (Iced 0.14 requires
    /// `Message: Clone`). The handler takes the sender out of the mutex once
    /// to reply.
    Ipc(
        crate::ipc::Request,
        std::sync::Arc<std::sync::Mutex<Option<futures::channel::oneshot::Sender<crate::ipc::Response>>>>,
    ),
}

#[derive(Debug, Clone, Default)]
pub struct PendingNav {
    pub line: Option<u32>,
    pub section: Option<String>,
}

pub struct App {
    pub file: Option<PathBuf>,
    pub source: String,
    pub ast: Vec<(BlockId, Block)>,
    pub theme_mode: ThemeMode,
    pub theme_preset: ThemePreset,
    pub palette: Palette,
    pub typography: Typography,
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
    pub tree_cursor: usize,
    pub overlay: Overlay,
    pub overlay_query: String,
    pub overlay_selected: usize,
    pub picker: Option<Picker>,
    pub tree_viewport: Option<iced::widget::scrollable::Viewport>,
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
    pub image_cache: HashMap<String, ImageState>,
    pub zoom_url: Option<String>,
    pub view_mode: ViewMode,
    pub editor: Option<iced::widget::text_editor::Content>,
    pub dirty: bool,
    pub edit_history: Vec<String>,
    pub edit_redo: Vec<String>,
    pub is_data_doc: bool,
    pub folded: HashSet<crate::ast::BlockId>,
    pub hovered_heading: Option<crate::ast::BlockId>,
    pub fold_chord_pending: bool,
    pub mindmap_collapsed: HashSet<crate::ast::BlockId>,
    pub mindmap_panel_open: bool,
    pub mindmap_selected: Option<crate::ast::BlockId>,
    pub mindmap_panel_width: f32,
    pub mindmap_panel_drag: Option<(f32, Option<f32>)>,
    pub mindmap_autocenter: bool,
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
        Self {
            file: None,
            source: String::new(),
            ast: Vec::new(),
            theme_mode: mode,
            theme_preset: preset,
            palette: theme::palette_for(preset),
            typography: Typography::DEFAULT,
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
            tree_cursor: 0,
            overlay: Overlay::None,
            overlay_query: String::new(),
            overlay_selected: 0,
            picker: None,
            tree_viewport: None,
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
            image_cache: HashMap::new(),
            zoom_url: None,
            view_mode: ViewMode::Rendered,
            editor: None,
            dirty: false,
            edit_history: Vec::new(),
            edit_redo: Vec::new(),
            is_data_doc: false,
            folded: HashSet::new(),
            hovered_heading: None,
            fold_chord_pending: false,
            mindmap_collapsed: HashSet::new(),
            mindmap_panel_open: false,
            mindmap_selected: None,
            mindmap_panel_width: MIND_PANEL_DEFAULT,
            mindmap_panel_drag: None,
            mindmap_autocenter: true,
            diagram_cache: crate::diagram::DiagramCache::new(64),
            diagram_theme_id: 0,
            zoom_diagram: None,
            block_lines: Vec::new(),
            pending_nav: None,
            queued_snap: None,
        }
    }
}

impl App {
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
    fn overlay_scroll_id() -> iced::widget::Id {
        iced::widget::Id::new("overlay")
    }
    fn search_input_id() -> iced::widget::Id {
        iced::widget::Id::new("search-input")
    }
    fn overlay_input_id() -> iced::widget::Id {
        iced::widget::Id::new("overlay-input")
    }

    fn scroll_tree_to_cursor(&self) -> Task<Message> {
        const ROW_H: f32 = 26.0;
        let Some(root) = &self.workspace_tree else {
            return Task::none();
        };
        let total = tree::flatten(root, &self.expanded).len();
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

    fn scroll_overlay_to_cursor(&self) -> Task<Message> {
        let (total, row_h) = match self.overlay {
            Overlay::FileFinder => (self.filtered_files().len().min(80), 32.0),
            Overlay::Command => (self.filtered_commands().len(), 32.0),
            Overlay::ThemePicker => (self.filtered_themes().len(), 32.0),
            Overlay::FolderPicker => (
                self.picker.as_ref().map(|p| p.entries.len()).unwrap_or(0),
                33.0,
            ),
            Overlay::None | Overlay::ImageZoom => (0, 32.0),
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
        (app, task)
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
                "mdv — {}",
                p.file_name().and_then(|n| n.to_str()).unwrap_or("?")
            ),
            None => "mdv".into(),
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

    fn scroll_to_current_match(&self) -> Task<Message> {
        if self.matches.is_empty() || self.ast.is_empty() {
            return Task::none();
        }
        let m = self.matches[self.match_idx];
        let Some((block_top, block_h)) =
            crate::virt::estimated_block_position(&self.ast, &self.height_cache, m.block)
        else {
            return Task::none();
        };
        let estimated_h = crate::virt::estimated_content_height(&self.ast, &self.height_cache);
        let (content_h, view_h) = self
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
        // Place the matched block slightly above center, using document-position
        // estimates instead of block index so tall blocks don't skew the jump.
        let target = block_top + block_h * 0.5 - view_h * 0.38;
        let rel = (target / max_scroll).clamp(0.0, 1.0);
        Task::done(Message::RestoreBodySnap(rel))
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

    fn reparse_source(&mut self) {
        if let Some(ast) = self.synthesize_data_ast() {
            self.ast = ast;
            self.rebuild_matches();
            return;
        }
        let (mut parsed, block_offsets) = parser::parse(&self.source);
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
        self.rebuild_matches();
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
        for (_id, b) in &self.ast {
            if let Block::Diagram { hash, kind, source } = b {
                if !seen.insert(*hash) {
                    continue;
                }
                if self.diagram_cache.peek(&(*hash, theme_id)).is_some() {
                    continue;
                }
                pending_inserts.push((*hash, kind.clone(), source.clone()));
            }
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
        let content: Element<'_, Message> = match self.mindmap_selected {
            None => container(
                text("Click a leaf heading to see its content")
                    .color(pal.muted)
                    .size(13),
            )
            .padding(24)
            .into(),
            Some(target) => {
                match self.mindmap_panel_range(target) {
                    Some((s, end, truncated)) => {
                        let mut col = Column::new().spacing(12).push(crate::render::render(
                            &self.ast[s..end],
                            pal,
                            &self.typography,
                            hl,
                            self.body_viewport.as_ref(),
                            &self.height_cache,
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
                }
            }
        };
        let scrolled = scrollable(container(content).padding(Padding::from([24, 24])))
            .height(Length::Fill)
            .direction(slim_scroll_direction())
            .style(move |_, status| sleek_scrollable_style(status, pal_c, recently_scrolled));
        container(scrolled)
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
        vec![
            ("Open Folder…  ⌘O", Message::OpenFolderPicker),
            ("Find File in Workspace…  ⌘P", Message::OpenFileFinder),
            ("Toggle Sidebar  ⌘B", Message::ToggleSidebar),
            ("Toggle Hidden Files  ⌘⇧.", Message::ToggleHidden),
            ("Find in Document  ⌘F", Message::ToggleSearch),
            ("Toggle Raw/Rendered  ⌘E", Message::ToggleViewMode),
            ("Toggle Mindmap  ⌘M", Message::ToggleMindmap),
            ("Toggle Mindmap Panel  ⌘⌥B", Message::ToggleMindmapPanel),
            ("Toggle Mindmap Auto-Center", Message::ToggleMindmapAutocenter),
            ("Cycle Theme  ⌘T", Message::ToggleTheme),
            ("Pick Theme…", Message::OpenThemePicker),
            ("Reload Custom Themes", Message::ReloadThemes),
            ("Scroll to Top  ⌘↑", Message::ScrollToTop),
            ("Scroll to Bottom  ⌘↓", Message::ScrollToBottom),
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
        tx: &std::sync::Arc<std::sync::Mutex<Option<futures::channel::oneshot::Sender<crate::ipc::Response>>>>,
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
            return Task::batch([
                Task::done(Message::RestoreBodySnap(rel)),
                Task::done(msg),
            ]);
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
                Task::none()
            }
            Message::SvgRasterized(key, Ok(rgba_bytes_w_h)) => {
                let (rgba, w, h) = rgba_bytes_w_h;
                let handle = iced::widget::image::Handle::from_rgba(w, h, rgba);
                if let Some(entry) = self.image_cache.get_mut(&key) {
                    if let ImageState::LoadedSvg { raster, .. } = entry {
                        *raster = Some(handle);
                    }
                } else {
                    self.image_cache.insert(key, ImageState::Loaded(handle));
                }
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
                Task::none()
            }
            Message::ToggleFold(id) => {
                if self.folded.contains(&id) {
                    self.folded.remove(&id);
                    return Task::none();
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
                Task::none()
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
                restore
            }
            Message::MindmapToggleNode(id) => {
                if self.mindmap_collapsed.contains(&id) {
                    self.mindmap_collapsed.remove(&id);
                } else {
                    self.mindmap_collapsed.insert(id);
                }
                Task::none()
            }
            Message::MindmapSelectLeaf(id) => {
                self.mindmap_selected = Some(id);
                self.mindmap_panel_open = true;
                Task::none()
            }
            Message::MindmapDeselect => {
                self.mindmap_selected = None;
                self.mindmap_panel_open = false;
                self.mindmap_panel_drag = None;
                Task::none()
            }
            Message::MindmapNavigate(dir) => {
                let (nodes, _) = crate::mindmap::build_layout(
                    &self.ast,
                    self.file.as_deref(),
                    &self.mindmap_collapsed,
                );
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
                            // Expand the collapsed node, then on next press right will descend.
                            if let Some(id) = n.id {
                                self.mindmap_collapsed.remove(&id);
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
                    }
                }
                Task::none()
            }
            Message::ToggleMindmapPanel => {
                self.mindmap_panel_open = !self.mindmap_panel_open;
                if !self.mindmap_panel_open {
                    self.mindmap_panel_drag = None;
                }
                Task::none()
            }
            Message::MindmapToggleSelected => {
                if let Some(id) = self.mindmap_selected {
                    if self.mindmap_collapsed.contains(&id) {
                        self.mindmap_collapsed.remove(&id);
                    } else {
                        self.mindmap_collapsed.insert(id);
                    }
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
                        let push = self.edit_history.last().map(|s| s != &prev).unwrap_or(true);
                        if push {
                            self.edit_history.push(prev);
                            if self.edit_history.len() > 200 {
                                self.edit_history.remove(0);
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
                    Overlay::None | Overlay::ImageZoom => 0,
                };
                if len == 0 {
                    return Task::none();
                }
                let next = (self.overlay_selected as isize + d).clamp(0, len as isize - 1);
                self.overlay_selected = next as usize;
                self.scroll_overlay_to_cursor()
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
                Overlay::None | Overlay::ImageZoom => Task::none(),
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
                self.source = src;
                self.file = Some(path);
                self.is_data_doc = data_lang_for(self.file.as_deref()).is_some();
                self.mindmap_collapsed.clear();
                self.mindmap_selected = None;
                if let Some(ast) = self.synthesize_data_ast() {
                    self.ast = ast;
                } else {
                    let (mut parsed, block_offsets) = parser::parse(&self.source);
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
                }
                self.error = None;
                self.rebuild_matches();
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
                    Task::done(Message::Ipc(
                        crate::ipc::Request {
                            id: 0,
                            cmd: crate::ipc::Cmd::Goto {
                                line: nav.line,
                                section: nav.section,
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
                    self.typography = t;
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
                    self.palette = t.palette;
                    self.typography = t.typography;
                    self.theme_id = theme::ThemeId::Custom(slug.clone());
                    let label = t.name.clone();
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
                        self.palette = t.palette;
                        self.typography = t.typography;
                    }
                }
                let n = self.custom_themes.len();
                if !errs.is_empty() {
                    self.error = Some(format!("theme load: {}", errs.join("; ")));
                }
                let changed = self.refresh_diagram_theme_id();
                let toast = self.show_toast(format!(
                    "{n} custom theme{}",
                    if n == 1 { "" } else { "s" }
                ));
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
                        self.palette = t.palette;
                        self.typography = t.typography;
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
            Message::ToggleSidebar => {
                self.sidebar_open = !self.sidebar_open;
                self.restore_body_scroll()
            }
            Message::ToggleHidden => {
                self.show_hidden = !self.show_hidden;
                // Rebuild tree + workspace_files with the new filter. Keep
                // expanded paths; any node that disappears just won't show.
                if let Some(ws) = self.workspace.clone() {
                    self.workspace_files =
                        picker::walk_markdown(&ws, 8, 5000, self.show_hidden);
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
                self.scroll_tree_to_cursor()
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
            Message::ScrollBy(dy) => iced::widget::operation::scroll_by(
                Self::scroll_id(),
                iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: dy },
            ),
            Message::ScrollToTop => iced::widget::operation::scroll_to(
                Self::scroll_id(),
                iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: 0.0 },
            ),
            Message::ScrollToBottom => iced::widget::operation::scroll_to(
                Self::scroll_id(),
                iced::widget::scrollable::AbsoluteOffset {
                    x: 0.0,
                    y: f32::MAX,
                },
            ),
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
            Message::OverlayScrolled(v) => {
                self.overlay_viewport = Some(v);
                self.last_scroll_at = Some(std::time::Instant::now());
                Task::none()
            }
            Message::BodyScrolled(v) => {
                self.body_viewport = Some(v);
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
                    Block::Diagram { hash: h, source, .. } if *h == hash => {
                        Some(source.clone())
                    }
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
            Message::DiagramRendered { hash, theme_id, result } => {
                // Drop stale results — theme changed mid-render, or AST
                // re-parsed away the source block.
                if theme_id != self.diagram_theme_id {
                    return Task::none();
                }
                let still_present = self.ast.iter().any(|(_, b)| matches!(
                    b,
                    Block::Diagram { hash: h, .. } if *h == hash
                ));
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
                        }
                    }
                    Err(msg) => crate::diagram::DiagramState::Err(msg),
                };
                self.diagram_cache.put((hash, theme_id), state);
                Task::none()
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
            Message::Noop => Task::none(),
            Message::Ipc(req, tx) => {
                use crate::ipc::{Cmd, Mode, Response};
                let id = req.id;
                let mut follow_up: Task<Message> = Task::none();
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
                        follow_up = iced::window::latest()
                            .and_then(|wid| iced::window::gain_focus(wid));
                        Response::ok(id)
                    }
                    Cmd::Close => {
                        follow_up = iced::window::latest()
                            .and_then(|wid| iced::window::close(wid));
                        Response::ok(id)
                    }
                    Cmd::Mode { mode } => {
                        self.view_mode = match mode {
                            Mode::View => ViewMode::Rendered,
                            Mode::Edit => ViewMode::Raw,
                            Mode::Mindmap => ViewMode::Mindmap,
                        };
                        Response::ok(id)
                    }
                    Cmd::OpenFolder { dir } => {
                        follow_up = Task::done(Message::OpenWorkspace(std::path::PathBuf::from(dir)));
                        Response::ok(id)
                    }
                    Cmd::Reveal { file } => {
                        follow_up = Task::perform(
                            load_file(std::path::PathBuf::from(file)),
                            Message::FileLoaded,
                        );
                        Response::ok(id)
                    }
                    Cmd::Open { file, line, section } => {
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
                            self.pending_nav = Some(PendingNav { line, section });
                            Response::ok(id)
                        }
                    }
                    Cmd::Goto { line, section } => apply_goto(self, id, line, section),
                };
                Self::reply(&tx, resp);
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
        let tree_active = self.sidebar_open && self.workspace.is_some();
        let editing = self.view_mode == ViewMode::Raw && self.editor.is_some();
        let fold_chord = self.fold_chord_pending;
        let mindmap = self.view_mode == ViewMode::Mindmap;
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
            editing,
            fold_chord,
            mindmap,
        ))
        .map(
            |((focused, overlay_open, tree_active, editing, fold_chord, mindmap), ev)| {
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
                    Key::Named(Named::ArrowDown) if mindmap && !overlay_open => {
                        Some(Message::MindmapNavigate(MindmapDir::Down))
                    }
                    Key::Named(Named::ArrowUp) if mindmap && !overlay_open => {
                        Some(Message::MindmapNavigate(MindmapDir::Up))
                    }
                    Key::Named(Named::ArrowLeft) if mindmap && !overlay_open => {
                        Some(Message::MindmapNavigate(MindmapDir::Left))
                    }
                    Key::Named(Named::ArrowRight) if mindmap && !overlay_open => {
                        Some(Message::MindmapNavigate(MindmapDir::Right))
                    }
                    Key::Named(Named::Space) if mindmap && !overlay_open => {
                        Some(Message::MindmapToggleSelected)
                    }
                    Key::Named(Named::ArrowDown) if tree_active => Some(Message::TreeMove(1)),
                    Key::Named(Named::ArrowUp) if tree_active => Some(Message::TreeMove(-1)),
                    Key::Named(Named::Enter) if tree_active => Some(Message::TreeActivate),
                    Key::Named(Named::Space) if tree_active => Some(Message::TreeActivate),
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
        iced::Subscription::batch([dnd, watcher, theme_watcher, keys, scroller, drag, mind_drag, ipc])
    }

    pub fn view(&self) -> Element<'_, Message> {
        {
            use std::sync::OnceLock;
            // Print first_view BEFORE the font-load block so the timing reflects
            // when the window can actually paint (font load runs lazily after).
            static BENCH: OnceLock<bool> = OnceLock::new();
            if *BENCH.get_or_init(|| std::env::var_os("MDV_BENCH_STARTUP").is_some()) {
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
                if std::env::var_os("MDV_BENCH_STARTUP").is_some() {
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

        let reader: Element<'_, Message> = if let Some(err) = &self.error {
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
                let (nodes, content_size) = crate::mindmap::build_layout(
                    &self.ast,
                    self.file.as_deref(),
                    &self.mindmap_collapsed,
                );
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
                            let cmd_or_ctrl =
                                kp.modifiers.command() || kp.modifiers.control();
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
                        self.body_viewport.as_ref(),
                        &self.height_cache,
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
                    self.body_viewport.as_ref(),
                    &self.height_cache,
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
            Overlay::ImageZoom => image_zoom_overlay(
                self.zoom_url.as_deref(),
                self.zoom_diagram.as_ref(),
                &self.image_cache,
                pal,
            ),
        };
        let base: Element<'_, Message> =
            iced::widget::stack![Element::from(main), overlay_layer].into();
        let toast_layer: Element<'_, Message> = match &self.toast {
            Some(t) => toast_overlay(&t.text, pal),
            None => Space::new().into(),
        };
        iced::widget::stack![base, toast_layer].into()
    }
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
    cache: &HashMap<String, ImageState>,
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
    let close_btn_inner = container(
        crate::icon::glyph(crate::icon::ic::X, 16.0, pal.fg),
    )
    .padding(Padding::from([6, 8]))
    .style(move |_| container::Style {
        background: Some(Color { a: 0.75, ..pal.code_bg }.into()),
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
            text("mdv").size(40).color(pal.fg),
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
                .on_submit(Message::NextMatch)
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
    let header = container(
        irow![
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
        .padding(sidebar_header_padding())
        .spacing(6)
        .align_y(iced::Alignment::Center),
    )
    .width(Length::Fill);

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
    let body = scrollable(inner)
        .height(Length::Fill)
        .direction(slim_scroll_direction_horizontal())
        .style(move |_, status| sleek_scrollable_style(status, pal, recently_scrolled));

    container(column![header, body])
        .width(Length::Fixed(app.sidebar_width))
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(pal.sidebar.into()),
            ..Default::default()
        })
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
            let show_border = is_current || is_cursor;
            button::Style {
                background: bg,
                text_color: pal.fg,
                border: Border {
                    color: if show_border {
                        pal.tree_selected_border
                    } else {
                        Color::TRANSPARENT
                    },
                    width: if show_border { 1.0 } else { 0.0 },
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
fn sidebar_header_padding() -> Padding {
    #[cfg(target_os = "macos")]
    {
        // Traffic-light buttons sit ~6px from top, ~12px tall → reserve top
        // ~22px to vertically center the label with them. Left ~72px clears
        // the third button + a small gutter.
        Padding {
            top: 22.0,
            right: 14.0,
            bottom: 8.0,
            left: 72.0,
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Padding::from([10, 14])
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
            .user_agent("mdv/0.2")
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

pub fn resolve_image_path(url: &str, current_file: Option<&std::path::Path>) -> Option<PathBuf> {
    let p = std::path::Path::new(url);
    if p.is_absolute() {
        return Some(p.to_path_buf());
    }
    let base = current_file.and_then(|f| f.parent())?;
    Some(base.join(url))
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
        let sections = crate::ipc::sections::list_sections(&app.source);
        match crate::ipc::sections::resolve_section_path(&sec, &sections) {
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
    let Some((block_top, block_h)) =
        crate::virt::estimated_block_position(&app.ast, &app.height_cache, idx)
    else {
        return Response::err(id, "could not locate block");
    };
    let estimated_h = crate::virt::estimated_content_height(&app.ast, &app.height_cache);
    let (content_h, view_h) = app
        .body_viewport
        .as_ref()
        .map(|v| (v.content_bounds().height.max(estimated_h), v.bounds().height))
        .unwrap_or((estimated_h, 0.0));
    let max_scroll = (content_h - view_h).max(1.0);
    let target = block_top + block_h * 0.5 - view_h * 0.38;
    let rel = (target / max_scroll).clamp(0.0, 1.0);
    app.queued_snap = Some(rel);
    crate::ipc::Response::ok(id)
}

fn current_line_estimate(app: &App) -> Option<u32> {
    let v = app.body_viewport.as_ref()?;
    let content_h = v.content_bounds().height;
    let view_h = v.bounds().height;
    if content_h <= view_h {
        return app.block_lines.first().copied();
    }
    let rel = v.absolute_offset().y / (content_h - view_h);
    let est_total = crate::virt::estimated_content_height(&app.ast, &app.height_cache).max(1.0);
    let target_px = rel * est_total;
    let mut best: Option<u32> = None;
    for (i, _) in app.ast.iter().enumerate() {
        if let Some((top, _)) = crate::virt::estimated_block_position(&app.ast, &app.height_cache, i) {
            if top <= target_px {
                best = app.block_lines.get(i).copied();
            } else {
                break;
            }
        }
    }
    best
}

fn ipc_subscription_stream() -> impl iced::futures::Stream<Item = Message> {
    iced::stream::channel(64, |mut out: futures::channel::mpsc::Sender<Message>| async move {
        let listener = match crate::ipc::server::acquire() {
            Ok(l) => l,
            Err(_) => return,
        };
        let (tx, mut rx) = futures::channel::mpsc::channel::<crate::ipc::server::Pending>(64);
        tokio::spawn(crate::ipc::server::run(listener, tx));
        use futures::StreamExt;
        use futures::SinkExt;
        while let Some((req, reply)) = rx.next().await {
            let wrapped = std::sync::Arc::new(std::sync::Mutex::new(Some(reply)));
            if out.send(Message::Ipc(req, wrapped)).await.is_err() {
                break;
            }
        }
    })
}
