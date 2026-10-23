use crate::ast::{Block, BlockId, Inline};
use crate::theme::Palette;
use iced::widget::canvas::{self, path, Fill, Path, Stroke, Text};
use iced::{
    keyboard, mouse, touch, window, Color, Point, Rectangle, Renderer, Size, Theme, Vector,
};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

pub const NODE_W: f32 = 260.0;
pub const NODE_H: f32 = 36.0;
pub const X_GAP: f32 = 60.0;
pub const Y_GAP: f32 = 12.0;
pub const PAD: f32 = 32.0;
/// Width of the canvas-to-panel divider: its whole width is the grab area.
pub const PANEL_HANDLE_W: f32 = 9.0;
const FONT_SIZE: f32 = 14.0;
// Hard cap to bound the truncate scan on pathological inputs. The pixel
// budget below will usually cut earlier; this is just a safety lid.
const MAX_CHARS: usize = 120;
// Inner horizontal padding inside a node before text starts butting the edge.
const TEXT_INSET_X: f32 = 14.0;
const ZOOM_MIN: f32 = 0.2;
const ZOOM_MAX: f32 = 4.0;
const KEYBOARD_ZOOM_FACTOR: f32 = 1.25;
const ANIM_DURATION: Duration = Duration::from_millis(220);
const ZOOM_ANIM_DURATION: Duration = Duration::from_millis(140);
/// How far (screen px) the pointer may move between press and release and
/// still count as a click. Beyond it the press becomes a pan.
const CLICK_SLOP: f32 = 4.0;
/// Two clicks on one node within this interval are a double click. It
/// matches Iced's own `mouse::Click`, which the reader uses.
const DOUBLE_CLICK: Duration = Duration::from_millis(300);
/// World distance from a branch node's right edge to its chevron center.
const CHEVRON_INSET: f32 = 12.0;
/// World half-size of the drawn chevron.
const CHEVRON_HALF: f32 = 4.0;
/// Side of the chevron's square hit zone: world units, but never smaller
/// than `CHEVRON_HIT_MIN` screen px, so it stays clickable zoomed out.
const CHEVRON_HIT: f32 = 24.0;
const CHEVRON_HIT_MIN: f32 = 16.0;
/// Screen px of the map that a drag always leaves inside the canvas, so the
/// content cannot be thrown fully off-screen.
const PAN_KEEP_VISIBLE: f32 = 48.0;
/// Screen px at the bottom of the canvas that Zoom to Fit leaves free for
/// the floating hint and zoom buttons.
const FIT_BOTTOM_INSET: f32 = 56.0;

/// What a mind map zoom button asks the canvas to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoomCommand {
    /// Show the whole map (never above 100%).
    Fit,
    In,
    Out,
    /// Back to 100%.
    Reset,
}

/// The document mind map's root node: the document itself. It is
/// selectable and foldable like a heading. The parser derives block ids
/// from a hash, so a real block never takes this value in practice.
pub const DOC_ROOT: BlockId = BlockId(u64::MAX);

/// The latest zoom button press. The canvas keeps its own zoom, so the app
/// bumps `generation` on each press and the canvas applies a request once,
/// on the next redraw. A canvas that first sees a request takes it as
/// already applied, so a newly shown map replays nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ZoomRequest {
    pub generation: u64,
    pub command: Option<ZoomCommand>,
}

impl ZoomRequest {
    pub fn next(self, command: ZoomCommand) -> Self {
        Self {
            generation: self.generation.wrapping_add(1),
            command: Some(command),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Anim {
    start_x: f32,
    start_y: f32,
    target_x: f32,
    target_y: f32,
    current_x: f32,
    current_y: f32,
    start_at: Instant,
    done: bool,
}

fn ease_out_cubic(t: f32) -> f32 {
    let u = 1.0 - t;
    1.0 - u * u * u
}

/// A laid-out canvas node. The document mindmap uses the default `BlockId`
/// identity; other tree-shaped navigators can supply a distinct identity type
/// without borrowing document collapse or selection state.
#[derive(Clone)]
pub struct MNode<Id = BlockId> {
    pub id: Option<Id>,
    pub label: String,
    pub full_label: String,
    pub truncated: bool,
    pub level: u8,
    pub children: Vec<usize>,
    pub has_hidden_children: bool,
    pub x: f32,
    pub y: f32,
}

fn inlines_to_string(items: &[Inline], out: &mut String) {
    for i in items {
        match i {
            Inline::Text(t) | Inline::Code(t) => out.push_str(t),
            Inline::Emph(c) | Inline::Strong(c) | Inline::Strike(c) => inlines_to_string(c, out),
            Inline::Link { children, .. } => inlines_to_string(children, out),
        }
    }
}

/// Approximate pixel advance of `c` at the given font size. CJK / wide glyphs
/// roughly occupy an em, Latin and punctuation roughly half-em. This is a
/// heuristic; good enough to avoid label-overflow without importing a shaper.
fn glyph_advance_at(c: char, size: f32) -> f32 {
    let cp = c as u32;
    let wide = matches!(
        cp,
        0x1100..=0x115F
            | 0x2E80..=0x303E
            | 0x3041..=0x33FF
            | 0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xA000..=0xA4CF
            | 0xAC00..=0xD7A3
            | 0xF900..=0xFAFF
            | 0xFE30..=0xFE4F
            | 0xFF00..=0xFF60
            | 0xFFE0..=0xFFE6
            | 0x1F300..=0x1FAFF
    );
    if wide {
        size
    } else if c.is_whitespace() {
        size * 0.32
    } else if c.is_ascii_digit() || c.is_ascii_uppercase() {
        size * 0.62
    } else {
        size * 0.55
    }
}

fn text_width_at(s: &str, size: f32) -> f32 {
    s.chars().map(|c| glyph_advance_at(c, size)).sum()
}

/// Truncate `s` to fit `max_width` at the given font size. If it already
/// fits, returns the original. Otherwise drops from the end and appends `…`.
fn fit_label_at(s: &str, max_width: f32, size: f32) -> (String, bool) {
    if text_width_at(s, size) <= max_width {
        return (s.to_string(), false);
    }
    let ellipsis_w = glyph_advance_at('…', size);
    let budget = (max_width - ellipsis_w).max(0.0);
    let mut acc = String::new();
    let mut w = 0.0;
    for c in s.chars().take(MAX_CHARS) {
        let adv = glyph_advance_at(c, size);
        if w + adv > budget {
            break;
        }
        acc.push(c);
        w += adv;
    }
    while acc.ends_with(' ') {
        acc.pop();
    }
    acc.push('…');
    (acc, true)
}

/// Like [`fit_label_at`], but drops from the start and keeps the end, so a
/// long path keeps its file name.
fn fit_tail_at(s: &str, max_width: f32, size: f32) -> (String, bool) {
    if text_width_at(s, size) <= max_width {
        return (s.to_string(), false);
    }
    let budget = (max_width - glyph_advance_at('…', size)).max(0.0);
    let mut kept = Vec::new();
    let mut w = 0.0;
    for c in s.chars().rev() {
        let adv = glyph_advance_at(c, size);
        if w + adv > budget {
            break;
        }
        kept.push(c);
        w += adv;
    }
    let mut out = String::from('…');
    out.extend(kept.into_iter().rev());
    (out, true)
}

/// Smallest label size worth drawing; below it the text is hidden.
const LABEL_FONT_MIN: f32 = 8.0;

/// Screen font size for node labels at zoom `z`: `FONT_SIZE * z` rounded down
/// to a whole pixel, so a label fitted at `FONT_SIZE` still fits its scaled
/// node and the glyph atlas only sees whole-pixel sizes (sub-pixel sizes
/// re-rasterize glyphs on every zoom event). `None` when the text would be
/// too small to read.
fn label_font_size(z: f32) -> Option<f32> {
    let size = (FONT_SIZE * z + 0.001).floor();
    (size >= LABEL_FONT_MIN).then_some(size)
}

/// Fit `s` into a node's inner width at the unified `FONT_SIZE`. Truncates
/// with `…` if it overflows.
pub(crate) fn fit_label_for_node(s: &str) -> (String, bool) {
    let max = (NODE_W - TEXT_INSET_X * 2.0).max(1.0);
    fit_label_at(s, max, FONT_SIZE)
}

pub fn build_tree(
    ast: &[(BlockId, Block)],
    doc_title: &str,
    collapsed: &HashSet<BlockId>,
) -> Vec<MNode> {
    let mut nodes: Vec<MNode> = Vec::new();
    let (root_label, root_trunc) = fit_label_for_node(doc_title);
    nodes.push(MNode {
        id: Some(DOC_ROOT),
        label: root_label,
        full_label: doc_title.to_string(),
        truncated: root_trunc,
        level: 0,
        children: Vec::new(),
        has_hidden_children: false,
        x: 0.0,
        y: 0.0,
    });
    if collapsed.contains(&DOC_ROOT) {
        // A folded root hides every heading, as a folded heading hides its
        // subtree.
        nodes[0].has_hidden_children = ast
            .iter()
            .any(|(_, block)| matches!(block, Block::Heading { .. }));
        return nodes;
    }
    let mut stack: Vec<(u8, usize)> = vec![(0, 0)];
    let mut skip_under: Option<u8> = None;

    for (id, block) in ast {
        if let Block::Heading { level, inlines, .. } = block {
            let lvl = *level;
            if let Some(skip_lvl) = skip_under {
                if lvl > skip_lvl {
                    if let Some(&(top_lvl, top_idx)) = stack.last() {
                        if lvl == top_lvl + 1 || top_lvl < skip_lvl {}
                        let _ = top_idx;
                    }
                    // Mark parent as having hidden children. Find collapsed ancestor.
                    if let Some(parent_idx) = stack
                        .iter()
                        .rev()
                        .find(|&&(l, _)| l == skip_lvl)
                        .map(|&(_, i)| i)
                    {
                        nodes[parent_idx].has_hidden_children = true;
                    }
                    continue;
                } else {
                    skip_under = None;
                }
            }

            let mut full = String::new();
            inlines_to_string(inlines, &mut full);
            let full = full.trim().to_string();
            if full.is_empty() {
                continue;
            }
            let (label, truncated) = fit_label_for_node(&full);
            while let Some(&(top_lvl, _)) = stack.last() {
                if top_lvl >= lvl {
                    stack.pop();
                } else {
                    break;
                }
            }
            let parent_idx = stack.last().map(|&(_, i)| i).unwrap_or(0);
            let idx = nodes.len();
            nodes.push(MNode {
                id: Some(*id),
                label,
                full_label: full,
                truncated,
                level: lvl,
                children: Vec::new(),
                has_hidden_children: false,
                x: 0.0,
                y: 0.0,
            });
            nodes[parent_idx].children.push(idx);
            stack.push((lvl, idx));

            if collapsed.contains(id) {
                skip_under = Some(lvl);
            }
        }
    }
    nodes
}

/// Return branching heading IDs whose descendants should be hidden to show at
/// most `depth` structural node levels below the synthetic document root.
/// Heading ranks still control horizontal layout; skipped ranks do not change
/// the parent/child depth used by this operation.
pub fn collapsed_for_depth(ast: &[(BlockId, Block)], depth: u8) -> HashSet<BlockId> {
    if depth == 0 {
        return HashSet::new();
    }

    fn collect(
        nodes: &[MNode],
        index: usize,
        structural_depth: u8,
        target_depth: u8,
        collapsed: &mut HashSet<BlockId>,
    ) {
        let node = &nodes[index];
        if structural_depth >= target_depth && !node.children.is_empty() {
            if let Some(id) = node.id {
                collapsed.insert(id);
            }
        }
        for &child in &node.children {
            collect(
                nodes,
                child,
                structural_depth.saturating_add(1),
                target_depth,
                collapsed,
            );
        }
    }

    let nodes = build_tree(ast, "", &HashSet::new());
    let mut collapsed = HashSet::new();
    if !nodes.is_empty() {
        collect(&nodes, 0, 0, depth, &mut collapsed);
    }
    collapsed
}

pub(crate) fn layout<Id>(nodes: &mut [MNode<Id>], idx: usize, y_cursor: &mut f32) -> f32 {
    let kids = nodes[idx].children.clone();
    let x = PAD + nodes[idx].level as f32 * (NODE_W + X_GAP);
    nodes[idx].x = x;
    if kids.is_empty() {
        let y = *y_cursor;
        nodes[idx].y = y;
        *y_cursor += NODE_H + Y_GAP;
        return y + NODE_H / 2.0;
    }
    let mut first_mid = 0.0;
    let mut last_mid = 0.0;
    for (i, &c) in kids.iter().enumerate() {
        let mid = layout(nodes, c, y_cursor);
        if i == 0 {
            first_mid = mid;
        }
        last_mid = mid;
    }
    let center = (first_mid + last_mid) / 2.0;
    nodes[idx].y = center - NODE_H / 2.0;
    center
}

pub fn build_layout(
    ast: &[(BlockId, Block)],
    file: Option<&std::path::Path>,
    collapsed: &HashSet<BlockId>,
) -> (Vec<MNode>, Size) {
    let title = file
        .and_then(|p| p.file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Document".into());
    let mut nodes = build_tree(ast, &title, collapsed);
    let mut y_cursor: f32 = PAD;
    layout(&mut nodes, 0, &mut y_cursor);
    let max_level = nodes.iter().map(|n| n.level).max().unwrap_or(0) as f32;
    let width = PAD * 2.0 + NODE_W + max_level * (NODE_W + X_GAP);
    let height = y_cursor + PAD;
    (nodes, Size::new(width, height))
}

#[derive(Debug)]
pub struct MindmapState<Id = BlockId> {
    pub zoom: f32,
    pub pan: Vector,
    pub drag_origin: Option<Point>,
    pub drag_pan_origin: Vector,
    pub drag_moved: bool,
    pub initialized: bool,
    anim: HashMap<Id, Anim>,
    pan_anim: Option<PanAnim>,
    zoom_anim: Option<ZoomAnim>,
    last_selected: Option<Id>,
    /// The node whose viewport focus is requested by the owning navigator.
    /// Document mindmaps keep this equal to `selected`; Full Mindmap can keep
    /// an explicit request while an async graph materializes.
    last_focus: Option<Id>,
    last_bounds_w: f32,
    last_bounds_h: f32,
    last_panel_open: bool,
    /// Full Mindmap increments this generation whenever its visible graph is
    /// rebuilt. Keeping it separate from selection lets the canvas preserve a
    /// selected folder's focus across async child discovery even when the
    /// selected identity itself does not change.
    last_layout_generation: Option<u64>,
    /// The node list `anim` was last synced against. Node lists are immutable
    /// behind their `Arc`, so a redraw with the same list has nothing to sync.
    /// A `Weak` keeps the allocation from being reused by a different list.
    anim_nodes: Option<std::sync::Weak<Vec<MNode<Id>>>>,
    hovered_idx: Option<usize>,
    /// The node under the left-button press, and whether the press hit its
    /// chevron. Its action runs on release, and only when the pointer did
    /// not move past [`CLICK_SLOP`].
    pressed: Option<(Id, bool)>,
    /// When, on which node, and where on screen the last single click
    /// landed, for double clicks.
    last_click: Option<(Instant, Id, Point)>,
    /// The node the pointer last clicked. When the app then focuses it, the
    /// canvas keeps it where it is on screen instead of centering it. A key
    /// press or a click on empty canvas hands focus back to centering.
    pointer_focus: Option<Id>,
    touch_points: Vec<(touch::Finger, Point)>,
    touch_span: Option<f32>,
    native_pinch_log: Option<f64>,
    /// The zoom request generation this canvas last applied.
    zoom_request_seen: Option<u64>,
}

impl<Id> Default for MindmapState<Id> {
    fn default() -> Self {
        Self {
            zoom: 0.0,
            pan: Vector::default(),
            drag_origin: None,
            drag_pan_origin: Vector::default(),
            drag_moved: false,
            initialized: false,
            anim: HashMap::new(),
            pan_anim: None,
            zoom_anim: None,
            last_selected: None,
            last_focus: None,
            last_bounds_w: 0.0,
            last_bounds_h: 0.0,
            last_panel_open: false,
            last_layout_generation: None,
            anim_nodes: None,
            hovered_idx: None,
            pressed: None,
            pointer_focus: None,
            last_click: None,
            touch_points: Vec::new(),
            touch_span: None,
            native_pinch_log: None,
            zoom_request_seen: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct PanAnim {
    start: Vector,
    target: Vector,
    start_at: Instant,
}

/// A short, retargetable animation for both pieces of the canvas transform.
/// Interpolating pan with zoom keeps the point under the input anchor still.
#[derive(Debug, Clone, Copy)]
struct ZoomAnim {
    start_zoom: f32,
    target_zoom: f32,
    start_pan: Vector,
    target_pan: Vector,
    start_at: Instant,
}

pub struct MindmapProgram<'a, Id, Message> {
    pub nodes: std::sync::Arc<Vec<MNode<Id>>>,
    pub content_size: Size,
    pub palette: Palette,
    /// Canvas fill; clear when the window glass shows through the reader.
    pub background: Color,
    pub selected: Option<Id>,
    /// Optional viewport-focus request. When omitted, `selected` remains the
    /// focus target for backwards-compatible document mindmap behavior.
    pub focus: Option<Id>,
    pub panel_open: bool,
    pub panel_width: f32,
    pub autocenter: bool,
    /// Optional source-generation signal for navigators whose visible graph
    /// can be rebuilt without changing selection. Both document and Full
    /// Mindmap supply their own independent graph generations.
    pub layout_generation: Option<u64>,
    /// Bare graph zoom keys belong to the canvas only while no higher-level
    /// input surface (for example search or an overlay) owns the keyboard.
    pub keyboard_zoom_enabled: bool,
    /// Cumulative macOS magnification input. Iced 0.14 drops Winit's native
    /// `PinchGesture`, so the app supplies it through a small platform bridge.
    /// A cumulative log lets the canvas apply every queued pinch delta even
    /// when multiple native events arrive before the next redraw.
    pub native_pinch_log: f64,
    /// The latest zoom button press; see [`ZoomRequest`].
    pub zoom_request: ZoomRequest,
    pub on_toggle: Box<dyn Fn(Id) -> Message + 'a>,
    pub on_select: Box<dyn Fn(Id) -> Message + 'a>,
    /// A double click on a node: what Enter does there. `None` makes a
    /// double click two single clicks.
    pub on_activate: Option<Box<dyn Fn(Id) -> Message + 'a>>,
    /// Text for the hover tooltip of any node, for example a Full Mindmap
    /// node's full path. Without it the tooltip shows only a truncated
    /// label in full.
    pub hover_text: Option<Box<dyn Fn(&Id) -> Option<String> + 'a>>,
    pub on_deselect: Message,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyboardZoom {
    In,
    Out,
    Reset,
}

fn keyboard_zoom(event: &iced::Event) -> Option<KeyboardZoom> {
    use keyboard::key::{Code, Physical};

    let iced::Event::Keyboard(keyboard::Event::KeyPressed {
        modified_key,
        physical_key,
        modifiers,
        ..
    }) = event
    else {
        return None;
    };
    // The document Mindmap panel retains ⌘/Ctrl +, −, and 0 for reader
    // typography. Graph zoom is deliberately graph-scoped and unmodified.
    // Both `=` and `+` share the same physical key on common layouts.
    if modifiers.command() || modifiers.control() || modifiers.alt() {
        return None;
    }

    match physical_key {
        Physical::Code(Code::Equal) => Some(KeyboardZoom::In),
        Physical::Code(Code::NumpadAdd | Code::NumpadEqual) => Some(KeyboardZoom::In),
        Physical::Code(Code::Minus) if !modifiers.shift() => Some(KeyboardZoom::Out),
        Physical::Code(Code::NumpadSubtract) => Some(KeyboardZoom::Out),
        Physical::Code(Code::Digit0) if !modifiers.shift() => Some(KeyboardZoom::Reset),
        Physical::Code(Code::Numpad0) => Some(KeyboardZoom::Reset),
        _ => match modified_key.as_ref() {
            keyboard::Key::Character("=" | "+") => Some(KeyboardZoom::In),
            keyboard::Key::Character("-") => Some(KeyboardZoom::Out),
            keyboard::Key::Character("0") => Some(KeyboardZoom::Reset),
            _ => None,
        },
    }
}

/// The current zoom; 1 before the canvas has seen its first event.
fn zoom_of<Id>(state: &MindmapState<Id>) -> f32 {
    if state.initialized {
        state.zoom
    } else {
        1.0
    }
}

/// A branch node carries a chevron that folds and unfolds it. Leaves and
/// nodes without an id have none.
fn has_chevron<Id>(node: &MNode<Id>) -> bool {
    node.id.is_some() && (!node.children.is_empty() || node.has_hidden_children)
}

/// How a chevron is inked; chevrons of one ink share one stroked path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChevronInk {
    /// A folded branch: "›".
    Folded,
    /// An open branch: "⌄".
    Open,
    /// Either glyph on the accent-filled root, in the root label's color.
    OnRoot,
}

impl ChevronInk {
    const ALL: [ChevronInk; 3] = [ChevronInk::Folded, ChevronInk::Open, ChevronInk::OnRoot];

    fn of<Id>(node: &MNode<Id>) -> Self {
        if node.level == 0 {
            ChevronInk::OnRoot
        } else if node.has_hidden_children {
            ChevronInk::Folded
        } else {
            ChevronInk::Open
        }
    }

    fn color(self, palette: &Palette) -> Color {
        match self {
            ChevronInk::Folded => palette.accent,
            ChevronInk::Open => palette.muted,
            ChevronInk::OnRoot => palette.bg,
        }
    }
}

fn canvas_center(bounds: Rectangle) -> Point {
    Point::new(bounds.width / 2.0, bounds.height / 2.0)
}

/// Set an absolute zoom while preserving the world point under `anchor`.
/// Returns whether the scale actually changed after clamping.
#[cfg(test)]
fn set_zoom_about<Id>(state: &mut MindmapState<Id>, anchor: Point, target_zoom: f32) -> bool {
    let old_zoom = state.zoom;
    let new_zoom = target_zoom.clamp(ZOOM_MIN, ZOOM_MAX);
    if !new_zoom.is_finite() || (new_zoom - old_zoom).abs() < f32::EPSILON {
        return false;
    }
    let world_x = (anchor.x - state.pan.x) / old_zoom;
    let world_y = (anchor.y - state.pan.y) / old_zoom;
    state.zoom = new_zoom;
    state.pan.x = anchor.x - world_x * new_zoom;
    state.pan.y = anchor.y - world_y * new_zoom;
    true
}

#[cfg(test)]
fn zoom_by<Id>(state: &mut MindmapState<Id>, anchor: Point, factor: f32) -> bool {
    if !factor.is_finite() || factor <= 0.0 {
        return false;
    }
    set_zoom_about(state, anchor, state.zoom * factor)
}

/// Advance a scheduled zoom to its current rendered transform. Returns whether
/// another frame is needed to finish it.
fn step_zoom_animation<Id>(state: &mut MindmapState<Id>) -> bool {
    let Some(animation) = state.zoom_anim else {
        return false;
    };
    let elapsed = Instant::now()
        .duration_since(animation.start_at)
        .as_secs_f32();
    let t = (elapsed / ZOOM_ANIM_DURATION.as_secs_f32()).clamp(0.0, 1.0);
    let k = ease_out_cubic(t);
    state.zoom = animation.start_zoom + (animation.target_zoom - animation.start_zoom) * k;
    state.pan.x = animation.start_pan.x + (animation.target_pan.x - animation.start_pan.x) * k;
    state.pan.y = animation.start_pan.y + (animation.target_pan.y - animation.start_pan.y) * k;
    if t >= 1.0 {
        state.zoom = animation.target_zoom;
        state.pan = animation.target_pan;
        state.zoom_anim = None;
        false
    } else {
        true
    }
}

/// Animate toward an absolute zoom while preserving the world point below the
/// input anchor. User zoom input supersedes automatic selection re-centering.
fn animate_zoom_to<Id>(state: &mut MindmapState<Id>, anchor: Point, target_zoom: f32) -> bool {
    if !target_zoom.is_finite() {
        return false;
    }
    let target_zoom = target_zoom.clamp(ZOOM_MIN, ZOOM_MAX);
    let prior_target = state.zoom_anim.map(|animation| animation.target_zoom);
    let was_running = step_zoom_animation(state);
    if (target_zoom - state.zoom).abs() < f32::EPSILON {
        state.zoom_anim = None;
        return false;
    }
    if prior_target.is_some_and(|prior| (prior - target_zoom).abs() < f32::EPSILON) {
        return was_running;
    }
    let world_x = (anchor.x - state.pan.x) / state.zoom;
    let world_y = (anchor.y - state.pan.y) / state.zoom;
    let target_pan = Vector::new(
        anchor.x - world_x * target_zoom,
        anchor.y - world_y * target_zoom,
    );
    state.pan_anim = None;
    state.zoom_anim = Some(ZoomAnim {
        start_zoom: state.zoom,
        target_zoom,
        start_pan: state.pan,
        target_pan,
        start_at: Instant::now(),
    });
    true
}

/// Schedule a relative zoom. Repeated wheel, pinch, or key events compound
/// from the last requested target so the animation never loses input distance.
fn animate_zoom_by<Id>(state: &mut MindmapState<Id>, anchor: Point, factor: f32) -> bool {
    if !factor.is_finite() || factor <= 0.0 {
        return false;
    }
    let base_zoom = state
        .zoom_anim
        .map(|animation| animation.target_zoom)
        .unwrap_or(state.zoom);
    animate_zoom_to(state, anchor, base_zoom * factor)
}

/// Animate to an absolute zoom and pan, for a target that is not anchored
/// at one point (Zoom to Fit).
fn animate_view_to<Id>(state: &mut MindmapState<Id>, target_zoom: f32, target_pan: Vector) -> bool {
    if !target_zoom.is_finite() || !target_pan.x.is_finite() || !target_pan.y.is_finite() {
        return false;
    }
    let target_zoom = target_zoom.clamp(ZOOM_MIN, ZOOM_MAX);
    let _ = step_zoom_animation(state);
    if (target_zoom - state.zoom).abs() < f32::EPSILON
        && (target_pan.x - state.pan.x).abs() < 0.5
        && (target_pan.y - state.pan.y).abs() < 0.5
    {
        state.zoom_anim = None;
        return false;
    }
    state.pan_anim = None;
    state.zoom_anim = Some(ZoomAnim {
        start_zoom: state.zoom,
        target_zoom,
        start_pan: state.pan,
        target_pan,
        start_at: Instant::now(),
    });
    true
}

/// Clamp one pan axis so at least [`PAN_KEEP_VISIBLE`] px (or all of a
/// smaller map) stays inside a `view`-long canvas. A pan that is already
/// outside the range (after a zoom) only may move back toward it.
fn clamp_pan_axis(pan: f32, origin: f32, content: f32, view: f32) -> f32 {
    let keep = PAN_KEEP_VISIBLE.min(content).min(view);
    let mut lo = keep - content;
    let mut hi = view - keep;
    if lo > hi {
        std::mem::swap(&mut lo, &mut hi);
    }
    pan.clamp(lo.min(origin), hi.max(origin))
}

fn touch_point_in_canvas(bounds: Rectangle, position: Point) -> Option<Point> {
    let local = Point::new(position.x - bounds.x, position.y - bounds.y);
    (local.x >= 0.0 && local.x <= bounds.width && local.y >= 0.0 && local.y <= bounds.height)
        .then_some(local)
}

fn touch_geometry(points: &[(touch::Finger, Point)]) -> Option<(f32, Point)> {
    if points.len() < 2 {
        return None;
    }
    let first = points[0].1;
    let second = points[1].1;
    let dx = second.x - first.x;
    let dy = second.y - first.y;
    let span = (dx * dx + dy * dy).sqrt();
    let center = Point::new((first.x + second.x) / 2.0, (first.y + second.y) / 2.0);
    Some((span, center))
}

impl<'a, Id, Message> MindmapProgram<'a, Id, Message>
where
    Id: Clone + Eq + std::hash::Hash,
{
    /// Compute parent index for each node.
    fn parent_map(&self) -> Vec<Option<usize>> {
        let mut parents = vec![None; self.nodes.len()];
        for (i, n) in self.nodes.iter().enumerate() {
            for &c in &n.children {
                parents[c] = Some(i);
            }
        }
        parents
    }

    /// Update `anim` entries to reflect new targets. New nodes spawn at parent's current pos.
    fn sync_anim(&self, state: &mut MindmapState<Id>) {
        let synced = state
            .anim_nodes
            .as_ref()
            .is_some_and(|nodes| std::ptr::eq(nodes.as_ptr(), std::sync::Arc::as_ptr(&self.nodes)));
        if synced {
            return;
        }
        state.anim_nodes = Some(std::sync::Arc::downgrade(&self.nodes));
        let parents = self.parent_map();
        let now = Instant::now();
        // Build a snapshot of pre-existing current positions so we can read
        // parent's current pos when seeding new children.
        let prev_current: HashMap<Id, (f32, f32)> = state
            .anim
            .iter()
            .map(|(k, v)| (k.clone(), (v.current_x, v.current_y)))
            .collect();
        let present: HashSet<Id> = self.nodes.iter().filter_map(|n| n.id.clone()).collect();
        state.anim.retain(|k, _| present.contains(k));

        for (i, n) in self.nodes.iter().enumerate() {
            let Some(id) = n.id.clone() else { continue };
            let target = (n.x, n.y);
            match state.anim.get_mut(&id) {
                Some(a) => {
                    if (a.target_x - target.0).abs() > 0.001
                        || (a.target_y - target.1).abs() > 0.001
                    {
                        a.start_x = a.current_x;
                        a.start_y = a.current_y;
                        a.target_x = target.0;
                        a.target_y = target.1;
                        a.start_at = now;
                        a.done = false;
                    }
                }
                None => {
                    // New node — seed from parent's current pos if known, else target.
                    let (sx, sy) = parents[i]
                        .and_then(|pi| self.nodes[pi].id.as_ref())
                        .and_then(|pid| prev_current.get(pid).copied())
                        .unwrap_or(target);
                    state.anim.insert(
                        id,
                        Anim {
                            start_x: sx,
                            start_y: sy,
                            target_x: target.0,
                            target_y: target.1,
                            current_x: sx,
                            current_y: sy,
                            start_at: now,
                            done: false,
                        },
                    );
                }
            }
        }
    }

    /// Advance all animations. Returns true if any still running.
    fn step_anim(&self, state: &mut MindmapState<Id>) -> bool {
        let now = Instant::now();
        let mut running = false;
        for a in state.anim.values_mut() {
            if a.done {
                continue;
            }
            let elapsed = now.duration_since(a.start_at).as_secs_f32();
            let dur = ANIM_DURATION.as_secs_f32();
            let t = (elapsed / dur).clamp(0.0, 1.0);
            let k = ease_out_cubic(t);
            a.current_x = a.start_x + (a.target_x - a.start_x) * k;
            a.current_y = a.start_y + (a.target_y - a.start_y) * k;
            if t >= 1.0 {
                a.current_x = a.target_x;
                a.current_y = a.target_y;
                a.done = true;
            } else {
                running = true;
            }
        }
        running
    }

    /// Detect selection change; if newly focused, start a pan animation that
    /// centers the selected node in the visible bounds (keeping current zoom).
    fn sync_focus_pan(&self, state: &mut MindmapState<Id>, bounds: Rectangle) {
        let prev_bounds_w = state.last_bounds_w;
        let bounds_changed = (bounds.width - state.last_bounds_w).abs() > 0.5
            || (bounds.height - state.last_bounds_h).abs() > 0.5;
        let sel_changed = self.selected != state.last_selected;
        let focus = self.focus.as_ref().or(self.selected.as_ref());
        let focus_changed = focus != state.last_focus.as_ref();
        let layout_changed = self
            .layout_generation
            .is_some_and(|generation| state.last_layout_generation != Some(generation));
        let panel_just_opened = self.panel_open && !state.last_panel_open;
        let panel_just_closed = !self.panel_open && state.last_panel_open;
        if !sel_changed
            && !focus_changed
            && !bounds_changed
            && !layout_changed
            && !panel_just_opened
            && !panel_just_closed
        {
            state.last_panel_open = self.panel_open;
            state.last_layout_generation = self.layout_generation;
            return;
        }
        if !self.autocenter {
            // Keep bookkeeping in sync so re-enable doesn't fire stale change.
            state.last_selected = self.selected.clone();
            state.last_focus = focus.cloned();
            state.last_bounds_w = bounds.width;
            state.last_bounds_h = bounds.height;
            state.last_panel_open = self.panel_open;
            state.last_layout_generation = self.layout_generation;
            return;
        }
        state.last_selected = self.selected.clone();
        state.last_bounds_w = bounds.width;
        state.last_bounds_h = bounds.height;
        state.last_layout_generation = self.layout_generation;
        // A graph rebuild may move the selected node or introduce a newly
        // selected child from a Loading status. Animate the transform to the
        // rebuilt layout target so the node stays in focus while its position
        // animation settles. Existing selection changes retain the historical
        // immediate snap behavior.
        let snap = panel_just_opened
            || panel_just_closed
            || ((sel_changed || focus_changed) && self.panel_open && !layout_changed);
        state.last_panel_open = self.panel_open;
        if !sel_changed && !focus_changed && !layout_changed && focus.is_none() {
            return;
        }
        let Some(sel) = focus else {
            return;
        };
        let Some((idx, _)) = self
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.id.as_ref() == Some(sel))
        else {
            // An explicit Full Mindmap focus request can arrive before its
            // file/folder is materialized in the accepted graph. Do not mark
            // that request consumed yet: doing so poisons `last_focus`, and a
            // later graph relayout with the same generation would see no
            // change and leave the canvas centered on the old root. Keep the
            // previous focus identity until the requested node is actually
            // present, then the next redraw will center it precisely.
            return;
        };
        // Only a present node can consume the focus identity. This ordering is
        // deliberate: stale/partial graph frames cannot make a later accepted
        // materialization fall through to the workspace root.
        state.last_focus = focus.cloned();
        if state.pointer_focus.as_ref() == Some(sel) {
            let effective_w =
                self.effective_width(bounds, prev_bounds_w, panel_just_opened, panel_just_closed);
            if let Some(target) = self.keep_in_place_target(state, idx, effective_w, bounds.height)
            {
                // No jump on click: the node stays under the pointer. When a
                // relayout moves it, pan along with its node animation (same
                // start, duration, and easing) so its screen position holds.
                if (target.x - state.pan.x).abs() < 0.5 && (target.y - state.pan.y).abs() < 0.5 {
                    return;
                }
                let start_at = self.nodes[idx]
                    .id
                    .as_ref()
                    .and_then(|id| state.anim.get(id))
                    .filter(|anim| !anim.done)
                    .map(|anim| anim.start_at);
                match start_at {
                    Some(start_at) => {
                        state.pan_anim = Some(PanAnim {
                            start: state.pan,
                            target,
                            start_at,
                        });
                    }
                    None => {
                        state.pan = target;
                        state.pan_anim = None;
                    }
                }
                return;
            }
        }
        let target = self.focus_target_for_node(
            state,
            idx,
            bounds,
            prev_bounds_w,
            panel_just_opened,
            panel_just_closed,
            // Explicit navigator focus is a precise request, not merely a
            // selection hint. Use the accepted node's final layout even when
            // the graph cache generation did not change between a partial
            // frame and materialization; otherwise the node animation's
            // parent seed can leave the canvas centered on the root/sibling.
            layout_changed || (self.focus.is_some() && focus_changed),
        );
        // If already on target, skip.
        if (target.x - state.pan.x).abs() < 0.5 && (target.y - state.pan.y).abs() < 0.5 {
            return;
        }
        if snap {
            state.pan = target;
            state.pan_anim = None;
        } else {
            state.pan_anim = Some(PanAnim {
                start: state.pan,
                target,
                start_at: Instant::now(),
            });
        }
    }

    /// The canvas width once a panel change has landed. A panel toggle may
    /// reach the canvas one frame before its narrower (or wider) bounds do.
    fn effective_width(
        &self,
        bounds: Rectangle,
        prev_bounds_w: f32,
        panel_just_opened: bool,
        panel_just_closed: bool,
    ) -> f32 {
        let panel_span = self.panel_width + PANEL_HANDLE_W;
        if panel_just_opened && (bounds.width - prev_bounds_w).abs() <= 0.5 {
            (bounds.width - panel_span).max(1.0)
        } else if panel_just_closed && (bounds.width - prev_bounds_w).abs() <= 0.5 {
            bounds.width + panel_span
        } else {
            bounds.width
        }
    }

    /// The pan that keeps a clicked node where it is on screen while it
    /// moves to its layout position, or `None` when it is not fully visible
    /// in `effective_w` (for example under a panel that is opening).
    fn keep_in_place_target(
        &self,
        state: &MindmapState<Id>,
        idx: usize,
        effective_w: f32,
        height: f32,
    ) -> Option<Vector> {
        let (cx, cy) = self.pos(state, idx);
        let z = state.zoom;
        let sx = cx * z + state.pan.x;
        let sy = cy * z + state.pan.y;
        let visible =
            sx >= 0.0 && sy >= 0.0 && sx + NODE_W * z <= effective_w && sy + NODE_H * z <= height;
        if !visible {
            return None;
        }
        let node = &self.nodes[idx];
        Some(Vector::new(
            state.pan.x + (cx - node.x) * z,
            state.pan.y + (cy - node.y) * z,
        ))
    }

    fn focus_target_for_node(
        &self,
        state: &MindmapState<Id>,
        idx: usize,
        bounds: Rectangle,
        prev_bounds_w: f32,
        panel_just_opened: bool,
        panel_just_closed: bool,
        prefer_layout_position: bool,
    ) -> Vector {
        let effective_w =
            self.effective_width(bounds, prev_bounds_w, panel_just_opened, panel_just_closed);

        // For a rebuilt graph, use the node's final layout position. New
        // children are seeded from their parent for the 220 ms node animation;
        // using that transient seed here would center the parent/root instead
        // of the selected child. Ordinary selection keeps the animated-current
        // behavior to avoid changing existing document-mindmap motion.
        let (nx, ny) = if prefer_layout_position {
            let node = &self.nodes[idx];
            (node.x, node.y)
        } else {
            self.pos(state, idx)
        };
        let world_cx = nx + NODE_W / 2.0;
        let world_cy = ny + NODE_H / 2.0;
        let z = state.zoom;
        Vector::new(
            effective_w / 2.0 - world_cx * z,
            bounds.height / 2.0 - world_cy * z,
        )
    }

    /// Advance pan animation. Returns true if still running.
    fn step_pan_anim(&self, state: &mut MindmapState<Id>) -> bool {
        let Some(p) = state.pan_anim else {
            return false;
        };
        let now = Instant::now();
        let elapsed = now.duration_since(p.start_at).as_secs_f32();
        let dur = ANIM_DURATION.as_secs_f32();
        let t = (elapsed / dur).clamp(0.0, 1.0);
        let k = ease_out_cubic(t);
        state.pan.x = p.start.x + (p.target.x - p.start.x) * k;
        state.pan.y = p.start.y + (p.target.y - p.start.y) * k;
        if t >= 1.0 {
            state.pan = p.target;
            state.pan_anim = None;
            false
        } else {
            true
        }
    }

    /// The node drawn with the hover fill.
    /// Only clickable nodes get it, and not while a drag pans the canvas.
    fn hover_highlight(&self, state: &MindmapState<Id>) -> Option<usize> {
        if state.drag_origin.is_some() && state.drag_moved {
            return None;
        }
        state
            .hovered_idx
            .filter(|&i| self.nodes.get(i).is_some_and(|n| n.id.is_some()))
    }

    /// The hover tooltip for node `idx`: the navigator's `hover_text`, or
    /// else a truncated label in full.
    fn tooltip_text(&self, idx: usize) -> Option<String> {
        let n = self.nodes.get(idx)?;
        self.hover_text
            .as_ref()
            .zip(n.id.as_ref())
            .and_then(|(text, id)| text(id))
            .or_else(|| (n.truncated && !n.full_label.is_empty()).then(|| n.full_label.clone()))
    }

    /// The node whose box contains the canvas point `pos`, at its rendered
    /// (animated) position.
    fn node_at(&self, state: &MindmapState<Id>, pos: Point) -> Option<usize> {
        let z = zoom_of(state);
        let wx = (pos.x - state.pan.x) / z;
        let wy = (pos.y - state.pan.y) / z;
        (0..self.nodes.len()).find(|&i| {
            let (nx, ny) = self.pos(state, i);
            wx >= nx && wx <= nx + NODE_W && wy >= ny && wy <= ny + NODE_H
        })
    }

    /// Screen center of node `i`'s chevron.
    fn chevron_center(&self, state: &MindmapState<Id>, i: usize) -> Point {
        let (nx, ny) = self.pos(state, i);
        let z = zoom_of(state);
        Point::new(
            (nx + NODE_W - CHEVRON_INSET) * z + state.pan.x,
            (ny + NODE_H / 2.0) * z + state.pan.y,
        )
    }

    fn on_chevron(&self, state: &MindmapState<Id>, i: usize, pos: Point) -> bool {
        if !has_chevron(&self.nodes[i]) {
            return false;
        }
        let center = self.chevron_center(state, i);
        let half = (CHEVRON_HIT * zoom_of(state)).max(CHEVRON_HIT_MIN) / 2.0;
        (pos.x - center.x).abs() <= half && (pos.y - center.y).abs() <= half
    }

    /// What a pointer at `pos` targets: a node, and whether it is that
    /// node's chevron. A node's own box wins over a neighbor's chevron zone,
    /// which may reach past its node when zoomed far out.
    fn hit(&self, state: &MindmapState<Id>, pos: Point) -> Option<(usize, bool)> {
        if let Some(i) = self.node_at(state, pos) {
            return Some((i, self.on_chevron(state, i, pos)));
        }
        // At this zoom and closer, every chevron zone lies inside its node.
        if zoom_of(state) * CHEVRON_HIT >= CHEVRON_HIT_MIN {
            return None;
        }
        (0..self.nodes.len())
            .find(|&i| self.on_chevron(state, i, pos))
            .map(|i| (i, true))
    }

    /// Get rendered (animated) position for a node.
    fn pos(&self, state: &MindmapState<Id>, idx: usize) -> (f32, f32) {
        let n = &self.nodes[idx];
        match n.id.as_ref().and_then(|id| state.anim.get(id)) {
            Some(a) => (a.current_x, a.current_y),
            None => (n.x, n.y),
        }
    }

    /// Apply a zoom button press the canvas has not seen yet. Returns
    /// whether an animation started.
    fn sync_zoom_request(&self, state: &mut MindmapState<Id>, bounds: Rectangle) -> bool {
        let request = self.zoom_request;
        let Some(seen) = state.zoom_request_seen.replace(request.generation) else {
            // A newly shown canvas takes the current request as applied.
            return false;
        };
        if seen == request.generation {
            return false;
        }
        let anchor = canvas_center(bounds);
        match request.command {
            Some(ZoomCommand::In) => animate_zoom_by(state, anchor, KEYBOARD_ZOOM_FACTOR),
            Some(ZoomCommand::Out) => animate_zoom_by(state, anchor, 1.0 / KEYBOARD_ZOOM_FACTOR),
            Some(ZoomCommand::Reset) => animate_zoom_to(state, anchor, 1.0),
            Some(ZoomCommand::Fit) => {
                let (zoom, pan) = self.fit_view(bounds);
                animate_view_to(state, zoom, pan)
            }
            None => false,
        }
    }

    /// The zoom and pan that show the whole map, centered, never above 100%.
    fn fit_view(&self, bounds: Rectangle) -> (f32, Vector) {
        let content_w = self.content_size.width.max(1.0);
        let content_h = self.content_size.height.max(1.0);
        let usable_h = (bounds.height - FIT_BOTTOM_INSET).max(bounds.height * 0.5);
        let zoom = (bounds.width / content_w)
            .min(usable_h / content_h)
            .clamp(ZOOM_MIN, 1.0);
        let pan = Vector::new(
            (bounds.width - content_w * zoom) / 2.0,
            (usable_h - content_h * zoom) / 2.0,
        );
        (zoom, pan)
    }

    /// Apply a macOS trackpad pinch that arrived through the native bridge.
    /// We only zoom when the current pointer is over this canvas, so a pinch
    /// over an adjacent panel never changes the graph behind it.
    fn sync_native_pinch(
        &self,
        state: &mut MindmapState<Id>,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> bool {
        let Some(previous_log) = state.native_pinch_log.replace(self.native_pinch_log) else {
            // Establish the baseline when a canvas is first shown. A newly
            // mounted canvas should not replay pinch history from an earlier
            // document or Full Mindmap session.
            return false;
        };
        let delta = self.native_pinch_log - previous_log;
        if !delta.is_finite() || delta.abs() < f64::EPSILON {
            return false;
        }
        let Some(anchor) = cursor.position_in(bounds) else {
            return false;
        };
        animate_zoom_by(state, anchor, delta.exp() as f32)
    }

    /// Handle a two-finger touchscreen pinch. Native macOS trackpads use the
    /// bridge above; Winit forwards physical touch contacts through Iced.
    fn handle_touch_pinch(
        &self,
        state: &mut MindmapState<Id>,
        event: &iced::Event,
        bounds: Rectangle,
    ) -> bool {
        let iced::Event::Touch(event) = event else {
            return false;
        };
        match event {
            touch::Event::FingerPressed { id, position } => {
                let Some(position) = touch_point_in_canvas(bounds, *position) else {
                    return false;
                };
                if state.touch_points.is_empty() {
                    // A direct gesture should take over from any just-finished
                    // wheel or keyboard transition immediately.
                    state.zoom_anim = None;
                    state.pan_anim = None;
                }
                if let Some((_, existing)) = state
                    .touch_points
                    .iter_mut()
                    .find(|(finger, _)| finger == id)
                {
                    *existing = position;
                } else {
                    state.touch_points.push((*id, position));
                }
                state.touch_span = touch_geometry(&state.touch_points).map(|(span, _)| span);
                true
            }
            touch::Event::FingerMoved { id, position } => {
                let Some((_, existing)) = state
                    .touch_points
                    .iter_mut()
                    .find(|(finger, _)| finger == id)
                else {
                    return false;
                };
                // Keep following an existing contact even when it moves just
                // outside the canvas, which avoids a jump at the edge.
                *existing = Point::new(position.x - bounds.x, position.y - bounds.y);
                let previous = state.touch_span;
                let geometry = touch_geometry(&state.touch_points);
                state.touch_span = geometry.map(|(span, _)| span);
                if let (Some(previous), Some((span, center))) = (previous, geometry) {
                    if previous > f32::EPSILON && span > f32::EPSILON {
                        let _ = animate_zoom_by(state, center, span / previous);
                    }
                }
                true
            }
            touch::Event::FingerLifted { id, .. } | touch::Event::FingerLost { id, .. } => {
                let previous_len = state.touch_points.len();
                state.touch_points.retain(|(finger, _)| finger != id);
                state.touch_span = touch_geometry(&state.touch_points).map(|(span, _)| span);
                state.touch_points.len() != previous_len
            }
        }
    }
}

impl<'a, Id, Message: Clone> MindmapProgram<'a, Id, Message>
where
    Id: Clone + Eq + std::hash::Hash,
{
    /// The left button came up at `now`, at `at` inside the canvas (`None`
    /// when it came up outside). A still release on a node is its click;
    /// `now` is a parameter so tests can time double clicks on a test clock.
    fn release(
        &self,
        state: &mut MindmapState<Id>,
        now: Instant,
        at: Option<Point>,
    ) -> Option<canvas::Action<Message>> {
        let origin = state.drag_origin.take()?;
        // A release outside the canvas, or far from the press, is not a
        // click even when no move event came in between.
        let moved = std::mem::take(&mut state.drag_moved)
            || !at.is_some_and(|at| (at.x - origin.x).hypot(at.y - origin.y) <= CLICK_SLOP);
        let pressed = state.pressed.take();
        // Only a quick second click on the same node's body is a double
        // click; a drag, a chevron, or empty canvas in between starts over.
        let previous = state.last_click.take();
        if moved {
            // One frame to bring back the hover fill the drag hid.
            return Some(canvas::Action::request_redraw().and_capture());
        }
        // A quick second click where the first one landed is a double
        // click even when the first click moved its node away: the panel it
        // opened can bring the node into view, so the second press finds
        // empty canvas or another node.
        if let (Some(activate), Some((_, first, _))) = (
            self.on_activate.as_ref(),
            previous.as_ref().filter(|(at, _, point)| {
                now.saturating_duration_since(*at) <= DOUBLE_CLICK
                    && (origin.x - point.x).hypot(origin.y - point.y) <= CLICK_SLOP
            }),
        ) {
            if pressed.as_ref().map(|(id, _)| id) != Some(first)
                && self.nodes.iter().any(|n| n.id.as_ref() == Some(first))
            {
                state.pointer_focus = Some(first.clone());
                return Some(canvas::Action::publish(activate(first.clone())).and_capture());
            }
        }
        let Some((id, chevron)) = pressed else {
            // A still click on empty canvas.
            state.pointer_focus = None;
            if self.selected.is_some() {
                return Some(canvas::Action::publish(self.on_deselect.clone()).and_capture());
            }
            return Some(canvas::Action::capture());
        };
        if !self.nodes.iter().any(|n| n.id.as_ref() == Some(&id)) {
            return Some(canvas::Action::capture());
        }
        // The chevron folds a branch; the rest of a node selects it
        // (and the app opens the panel), as the arrow keys do. The
        // focus sync on the next redraw keeps the node in place, or
        // brings it into view if the panel would cover it.
        state.pointer_focus = Some(id.clone());
        let msg = if chevron {
            (self.on_toggle)(id)
        } else if let Some(activate) = self.on_activate.as_ref().filter(|_| {
            previous.as_ref().is_some_and(|(at, prev, _)| {
                *prev == id && now.saturating_duration_since(*at) <= DOUBLE_CLICK
            })
        }) {
            activate(id)
        } else {
            state.last_click = Some((now, id.clone(), origin));
            (self.on_select)(id)
        };
        Some(canvas::Action::publish(msg).and_capture())
    }
}

impl<'a, Id, Message: Clone> canvas::Program<Message, Theme, Renderer>
    for MindmapProgram<'a, Id, Message>
where
    Id: Clone + Eq + std::hash::Hash + 'static,
{
    type State = MindmapState<Id>;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        if !state.initialized {
            state.zoom = 1.0;
            state.initialized = true;
        }
        if let iced::Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) = event {
            // Keyboard navigation centers its node again. Modifier keys
            // alone (for example holding ⌘) navigate nowhere.
            use keyboard::key::Named;
            if !matches!(
                key,
                keyboard::Key::Named(Named::Shift | Named::Control | Named::Alt | Named::Super)
            ) {
                state.pointer_focus = None;
            }
        }
        match event {
            iced::Event::Keyboard(_) if self.keyboard_zoom_enabled => {
                let Some(action) = keyboard_zoom(event) else {
                    return None;
                };
                let anchor = canvas_center(bounds);
                match action {
                    KeyboardZoom::In => {
                        let _ = animate_zoom_by(state, anchor, KEYBOARD_ZOOM_FACTOR);
                    }
                    KeyboardZoom::Out => {
                        let _ = animate_zoom_by(state, anchor, 1.0 / KEYBOARD_ZOOM_FACTOR);
                    }
                    KeyboardZoom::Reset => {
                        let _ = animate_zoom_to(state, anchor, 1.0);
                    }
                }
                Some(canvas::Action::request_redraw().and_capture())
            }
            iced::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let Some(pos) = cursor.position_in(bounds) else {
                    return None;
                };
                let factor = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => (*y * 0.05).exp(),
                    mouse::ScrollDelta::Pixels { y, .. } => (*y * 0.0025).exp(),
                };
                let _ = animate_zoom_by(state, pos, factor);
                Some(canvas::Action::request_redraw().and_capture())
            }
            iced::Event::Touch(_) => self
                .handle_touch_pinch(state, event, bounds)
                .then(|| canvas::Action::request_redraw().and_capture()),
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(pos) = cursor.position_in(bounds) else {
                    return None;
                };
                // Every press may become a drag, even on a node. The node's
                // action waits for a release without movement.
                state.pressed = self
                    .hit(state, pos)
                    .and_then(|(i, chevron)| self.nodes[i].id.clone().map(|id| (id, chevron)));
                state.drag_origin = Some(pos);
                state.drag_pan_origin = state.pan;
                state.drag_moved = false;
                // Direct dragging wins over any in-flight programmatic motion.
                state.pan_anim = None;
                state.zoom_anim = None;
                Some(canvas::Action::capture())
            }
            iced::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(origin) = state.drag_origin {
                    let Some(pos) = cursor.position_in(bounds) else {
                        // The press left the canvas, so it is no longer a
                        // click. Outside the canvas nothing pans.
                        state.drag_moved = true;
                        return None;
                    };
                    let dx = pos.x - origin.x;
                    let dy = pos.y - origin.y;
                    if !state.drag_moved && dx.hypot(dy) <= CLICK_SLOP {
                        // Still a click: the canvas stays put.
                        return Some(canvas::Action::capture());
                    }
                    state.drag_moved = true;
                    let z = zoom_of(state);
                    let origin = state.drag_pan_origin;
                    state.pan.x = clamp_pan_axis(
                        origin.x + dx,
                        origin.x,
                        self.content_size.width * z,
                        bounds.width,
                    );
                    state.pan.y = clamp_pan_axis(
                        origin.y + dy,
                        origin.y,
                        self.content_size.height * z,
                        bounds.height,
                    );
                    return Some(canvas::Action::request_redraw().and_capture());
                }
                // Hover tracking: highlight the node under the cursor so we
                // can show a tooltip for truncated labels in draw().
                let new_hover = cursor
                    .position_in(bounds)
                    .and_then(|pos| self.node_at(state, pos));
                if new_hover != state.hovered_idx {
                    state.hovered_idx = new_hover;
                    return Some(canvas::Action::request_redraw());
                }
                None
            }
            iced::Event::Mouse(mouse::Event::CursorLeft) => {
                if state.hovered_idx.is_some() {
                    state.hovered_idx = None;
                    return Some(canvas::Action::request_redraw());
                }
                None
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                self.release(state, Instant::now(), cursor.position_in(bounds))
            }
            iced::Event::Window(window::Event::RedrawRequested(_)) => {
                self.sync_anim(state);
                let pinch_zoomed = self.sync_native_pinch(state, bounds, cursor);
                let _ = self.sync_zoom_request(state, bounds);
                let zoom_running = step_zoom_animation(state);
                if !zoom_running {
                    self.sync_focus_pan(state, bounds);
                }
                let node_running = self.step_anim(state);
                let pan_running = if state.zoom_anim.is_none() {
                    self.step_pan_anim(state)
                } else {
                    false
                };
                // The node list, a zoom, or a pan can move a different node
                // under a still pointer. This frame draws anyway, so the
                // hover follows without asking for another one.
                state.hovered_idx = cursor
                    .position_in(bounds)
                    .and_then(|pos| self.node_at(state, pos));
                if node_running || pan_running || zoom_running || pinch_zoomed {
                    Some(canvas::Action::request_redraw())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        // Background.
        let bg_path = Path::rectangle(Point::ORIGIN, bounds.size());
        frame.fill(&bg_path, self.background);

        let z = if state.initialized { state.zoom } else { 1.0 };
        let pan = state.pan;

        // We deliberately do NOT use frame.scale(z): iced_wgpu re-rasterizes
        // glyphs whenever the current transform scale changes (see
        // iced_wgpu::geometry::fill_text: size = Pixels(text.size.0 * scale_y)).
        // Re-shaping every glyph on every wheel event killed scroll-zoom perf.
        // Instead, project everything to screen space manually here; keep text
        // at a constant font size so the glyph atlas is reused across frames.

        // Project a world point to screen space.
        let proj_x = |x: f32| x * z + pan.x;
        let proj_y = |y: f32| y * z + pan.y;
        let s_w = NODE_W * z;
        let s_h = NODE_H * z;
        let radius = (8.0 * z).min(s_w.min(s_h) * 0.5).max(0.0);
        let chevron_half = (CHEVRON_HALF * z).max(2.5);
        let chevron_inset = CHEVRON_INSET * z;

        // View-frustum culling: skip nodes fully outside bounds. Use a small
        // margin so partly-visible nodes still render.
        let view = Rectangle::new(Point::ORIGIN, bounds.size());
        let visible = |sx: f32, sy: f32| -> bool {
            !(sx + s_w < view.x - 1.0
                || sy + s_h < view.y - 1.0
                || sx > view.x + view.width + 1.0
                || sy > view.y + view.height + 1.0)
        };

        // Animated positions (lerp current → target). Root falls back to layout coords.
        let positions: Vec<(f32, f32)> =
            (0..self.nodes.len()).map(|i| self.pos(state, i)).collect();

        let edge_stroke = Stroke::default()
            .with_color(self.palette.subtle)
            .with_width(1.5);

        // Edges in screen space. Each curve's control points all lie inside
        // the AABB of its two endpoints (controls are (mx,py)/(mx,cy) with
        // mx between px and cx), and a bezier stays inside its control hull,
        // so an endpoint-AABB test against the viewport is an exact
        // conservative cull. Margin covers the stroke width.
        let edges_path = Path::new(|b| {
            for (i, n) in self.nodes.iter().enumerate() {
                let (nx, ny) = positions[i];
                let px = proj_x(nx + NODE_W);
                let py = proj_y(ny + NODE_H / 2.0);
                for &c in &n.children {
                    let (cnx, cny) = positions[c];
                    let cx = proj_x(cnx);
                    let cy = proj_y(cny + NODE_H / 2.0);
                    let (x0, x1) = if px <= cx { (px, cx) } else { (cx, px) };
                    let (y0, y1) = if py <= cy { (py, cy) } else { (cy, py) };
                    if x1 < view.x - 2.0
                        || y1 < view.y - 2.0
                        || x0 > view.x + view.width + 2.0
                        || y0 > view.y + view.height + 2.0
                    {
                        continue;
                    }
                    let mx = (px + cx) / 2.0;
                    b.move_to(Point::new(px, py));
                    b.bezier_curve_to(Point::new(mx, py), Point::new(mx, cy), Point::new(cx, cy));
                }
            }
        });
        frame.stroke(&edges_path, edge_stroke);

        let accent_fill = Fill::from(self.palette.accent);
        let surface_fill = Fill::from(self.palette.surface);
        let accent_border_stroke = Stroke::default()
            .with_color(self.palette.accent)
            .with_width(1.0);
        let subtle_border_stroke = Stroke::default()
            .with_color(self.palette.subtle)
            .with_width(1.0);

        // One pass over nodes feeding all six paths. Per-path append order
        // is preserved. Chevrons share the node's cull test: the chevron
        // center sits 12·z inside the right edge and reaches at most
        // max(4·z, 2.5) from it, so it lies within the 1 px cull margin.
        let mut accent_bg_b = path::Builder::new();
        let mut surface_bg_b = path::Builder::new();
        let mut accent_border_b = path::Builder::new();
        let mut subtle_border_b = path::Builder::new();
        // Folded branches show "›", open ones "⌄"; one path per ink.
        let mut chevrons_b = ChevronInk::ALL.map(|_| path::Builder::new());
        for (i, n) in self.nodes.iter().enumerate() {
            let (nx, ny) = positions[i];
            let sx = proj_x(nx);
            let sy = proj_y(ny);
            if !visible(sx, sy) {
                continue;
            }
            if has_chevron(n) {
                let cx = sx + s_w - chevron_inset;
                let cy = sy + s_h / 2.0;
                let h = chevron_half;
                let ink = ChevronInk::of(n);
                let b = &mut chevrons_b[ink as usize];
                if n.has_hidden_children {
                    b.move_to(Point::new(cx - h / 2.0, cy - h));
                    b.line_to(Point::new(cx + h / 2.0, cy));
                    b.line_to(Point::new(cx - h / 2.0, cy + h));
                } else {
                    b.move_to(Point::new(cx - h, cy - h / 2.0));
                    b.line_to(Point::new(cx, cy + h / 2.0));
                    b.line_to(Point::new(cx + h, cy - h / 2.0));
                }
            }
            if n.level == 0 {
                append_rounded_rect(&mut accent_bg_b, sx, sy, s_w, s_h, radius);
            } else {
                append_rounded_rect(&mut surface_bg_b, sx, sy, s_w, s_h, radius);
            }
            if n.level == 0 || n.has_hidden_children {
                append_rounded_rect(&mut accent_border_b, sx, sy, s_w, s_h, radius);
            } else {
                append_rounded_rect(&mut subtle_border_b, sx, sy, s_w, s_h, radius);
            }
        }
        let accent_backgrounds = accent_bg_b.build();
        let surface_backgrounds = surface_bg_b.build();
        let accent_borders = accent_border_b.build();
        let subtle_borders = subtle_border_b.build();
        let chevrons = chevrons_b.map(path::Builder::build);

        // Nodes.
        frame.fill(&surface_backgrounds, surface_fill);
        frame.fill(&accent_backgrounds, accent_fill);
        // One extra path for the node under the pointer. Hover changes
        // request a single redraw; a still pointer requests none.
        if let Some(i) = self.hover_highlight(state) {
            let (nx, ny) = positions[i];
            let (sx, sy) = (proj_x(nx), proj_y(ny));
            if visible(sx, sy) {
                let hover_bg = Path::new(|b| append_rounded_rect(b, sx, sy, s_w, s_h, radius));
                frame.fill(&hover_bg, self.palette.hover());
            }
        }
        frame.stroke(&subtle_borders, subtle_border_stroke);
        frame.stroke(&accent_borders, accent_border_stroke);

        // Selection ring (drawn over borders for the leaf currently open in the panel).
        if let Some(sel) = self.selected.as_ref() {
            if let Some((i, _)) = self
                .nodes
                .iter()
                .enumerate()
                .find(|(_, n)| n.id.as_ref() == Some(sel))
            {
                let (nx, ny) = positions[i];
                let sx = proj_x(nx);
                let sy = proj_y(ny);
                if visible(sx, sy) {
                    let ring_inset = 1.0;
                    let ring = Path::new(|b| {
                        append_rounded_rect(
                            b,
                            sx - ring_inset,
                            sy - ring_inset,
                            s_w + ring_inset * 2.0,
                            s_h + ring_inset * 2.0,
                            radius + ring_inset,
                        );
                    });
                    let ring_stroke = Stroke::default()
                        .with_color(self.palette.accent)
                        .with_width(2.5);
                    frame.stroke(&ring, ring_stroke);
                }
            }
        }

        // Text scales with the nodes, snapped to a short size ladder so the
        // glyph atlas stays warm; it is hidden once it would be unreadable.
        if let Some(label_size) = label_font_size(z) {
            for (i, n) in self.nodes.iter().enumerate() {
                let (nx, ny) = positions[i];
                let sx = proj_x(nx);
                let sy = proj_y(ny);
                if !visible(sx, sy) {
                    continue;
                }
                let text_color = if n.level == 0 {
                    self.palette.bg
                } else {
                    self.palette.fg
                };
                frame.fill_text(Text {
                    content: n.label.clone(),
                    position: Point::new(sx + s_w / 2.0, sy + s_h / 2.0),
                    color: text_color,
                    size: iced::Pixels(label_size),
                    align_x: iced::alignment::Horizontal::Center.into(),
                    align_y: iced::alignment::Vertical::Center.into(),
                    ..Text::default()
                });
            }
        }
        let chevron_stroke = |color| {
            Stroke::default()
                .with_color(color)
                .with_width(1.5)
                .with_line_cap(canvas::LineCap::Round)
                .with_line_join(canvas::LineJoin::Round)
        };
        for (ink, chevrons) in ChevronInk::ALL.into_iter().zip(&chevrons) {
            frame.stroke(chevrons, chevron_stroke(ink.color(&self.palette)));
        }

        // Tooltip: the navigator's hover text, or a truncated label in full.
        let tooltip = state
            .hovered_idx
            .and_then(|idx| self.tooltip_text(idx).map(|text| (idx, text)));
        if let Some((idx, tip_text)) = tooltip {
            let (nx, ny) = self.pos(state, idx);
            let nsx = proj_x(nx);
            let nsy = proj_y(ny);
            let tip_pad_x = 8.0;
            let tip_pad_y = 5.0;
            // A path wider than the canvas keeps its end.
            let max_text_w = (bounds.width - 12.0 - tip_pad_x * 2.0).max(1.0);
            let (tip_text, _) = fit_tail_at(&tip_text, max_text_w, FONT_SIZE);
            let tip_w =
                (text_width_at(&tip_text, FONT_SIZE) + tip_pad_x * 2.0).min(bounds.width - 12.0);
            let tip_h = FONT_SIZE + tip_pad_y * 2.0;
            let mut tip_x = nsx + s_w / 2.0 - tip_w / 2.0;
            let mut tip_y = nsy - tip_h - 6.0;
            if tip_y < 4.0 {
                tip_y = nsy + s_h + 6.0;
            }
            tip_x = tip_x.clamp(4.0, (bounds.width - tip_w - 4.0).max(4.0));
            let tip_radius = crate::theme::radius::SM.min(tip_h * 0.5);
            let tip_bg = Path::new(|b| {
                append_rounded_rect(b, tip_x, tip_y, tip_w, tip_h, tip_radius);
            });
            frame.fill(&tip_bg, Fill::from(self.palette.surface));
            let tip_border = Stroke::default()
                .with_color(self.palette.rule)
                .with_width(1.0);
            frame.stroke(&tip_bg, tip_border);
            frame.fill_text(Text {
                content: tip_text,
                position: Point::new(tip_x + tip_w / 2.0, tip_y + tip_h / 2.0),
                color: self.palette.fg,
                size: iced::Pixels(FONT_SIZE),
                align_x: iced::alignment::Horizontal::Center.into(),
                align_y: iced::alignment::Vertical::Center.into(),
                ..Text::default()
            });
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.drag_origin.is_some() && state.drag_moved {
            return mouse::Interaction::Grabbing;
        }
        let Some(pos) = cursor.position_in(bounds) else {
            return mouse::Interaction::default();
        };
        if self
            .hit(state, pos)
            .is_some_and(|(i, _)| self.nodes[i].id.is_some())
        {
            return mouse::Interaction::Pointer;
        }
        mouse::Interaction::Grab
    }
}

fn append_rounded_rect(b: &mut path::Builder, x: f32, y: f32, w: f32, h: f32, r: f32) {
    b.move_to(Point::new(x + r, y));
    b.line_to(Point::new(x + w - r, y));
    b.quadratic_curve_to(Point::new(x + w, y), Point::new(x + w, y + r));
    b.line_to(Point::new(x + w, y + h - r));
    b.quadratic_curve_to(Point::new(x + w, y + h), Point::new(x + w - r, y + h));
    b.line_to(Point::new(x + r, y + h));
    b.quadratic_curve_to(Point::new(x, y + h), Point::new(x, y + h - r));
    b.line_to(Point::new(x, y + r));
    b.quadratic_curve_to(Point::new(x, y), Point::new(x + r, y));
    b.close();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_font_scales_with_zoom_and_fits_the_node() {
        assert_eq!(label_font_size(1.0), Some(FONT_SIZE));
        // Too small to read: no text instead of glyphs spilling past the node.
        assert_eq!(label_font_size(0.5), None);
        assert_eq!(label_font_size(ZOOM_MIN), None);

        let label_w = NODE_W - TEXT_INSET_X * 2.0;
        let mut z = ZOOM_MIN;
        while z <= ZOOM_MAX {
            if let Some(size) = label_font_size(z) {
                // A label fitted at FONT_SIZE still fits the scaled node.
                assert!(label_w * size / FONT_SIZE <= label_w * z + 0.01, "z={z}");
                assert!(size <= NODE_H * z, "z={z}");
                // And it tracks the zoom within one pixel, so zooming feels
                // continuous instead of stepping between coarse sizes.
                assert!(size > FONT_SIZE * z - 1.0, "z={z} size={size}");
            }
            z += 0.01;
        }
        assert!(label_font_size(ZOOM_MAX).unwrap() >= FONT_SIZE * 3.0);
    }

    fn heading(id: u64, level: u8, label: &str) -> (BlockId, Block) {
        (
            BlockId(id),
            Block::Heading {
                level,
                id: label.to_lowercase(),
                inlines: vec![Inline::Text(label.to_string())],
            },
        )
    }

    #[test]
    fn the_document_root_is_a_node_that_selects_and_folds() {
        let ast = vec![heading(1, 1, "Top"), heading(2, 2, "Child")];
        let nodes = build_tree(&ast, "Document", &HashSet::new());
        assert_eq!(nodes[0].id, Some(DOC_ROOT), "the root can be selected");
        assert!(has_chevron(&nodes[0]), "the root folds like a branch");

        let folded = build_tree(&ast, "Document", &HashSet::from([DOC_ROOT]));
        assert_eq!(folded.len(), 1, "a folded root hides every heading");
        assert!(folded[0].has_hidden_children);
        assert!(folded[0].children.is_empty());

        // A document with no headings has nothing to fold.
        let empty = build_tree(&[], "Document", &HashSet::from([DOC_ROOT]));
        assert!(!empty[0].has_hidden_children);
        assert!(!has_chevron(&empty[0]));
    }

    #[test]
    fn the_root_chevron_contrasts_with_the_root_fill() {
        let ast = vec![heading(1, 1, "Top"), heading(2, 2, "Child")];
        let open = build_tree(&ast, "Document", &HashSet::new());
        let folded = build_tree(&ast, "Document", &HashSet::from([DOC_ROOT]));
        for palette in [Palette::ONE_DARK, Palette::ONE_LIGHT] {
            for root in [&open[0], &folded[0]] {
                // The root is filled with the accent, so its chevron takes
                // the root label's color.
                assert_eq!(ChevronInk::of(root).color(&palette), palette.bg);
            }
            // Other branches keep accent when folded and muted when open.
            assert_eq!(ChevronInk::of(&open[1]).color(&palette), palette.muted);
            let folded_child = build_tree(&ast, "Document", &HashSet::from([BlockId(1)]));
            assert_eq!(
                ChevronInk::of(&folded_child[1]).color(&palette),
                palette.accent
            );
        }
    }

    #[test]
    fn document_tree_keeps_block_ids_and_collapse_behavior() {
        let ast = vec![
            heading(1, 1, "Top"),
            heading(2, 2, "Child"),
            heading(3, 1, "Next"),
        ];
        let collapsed = HashSet::from([BlockId(1)]);
        let nodes = build_tree(&ast, "Document", &collapsed);

        assert_eq!(nodes[0].id, Some(DOC_ROOT));
        assert_eq!(nodes[0].children.len(), 2);
        assert_eq!(nodes[1].id, Some(BlockId(1)));
        assert!(nodes[1].has_hidden_children);
        assert!(nodes.iter().all(|node| node.id != Some(BlockId(2))));
        assert_eq!(nodes[2].id, Some(BlockId(3)));
    }

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    struct PathId(String);

    fn sample_nodes<Id: Clone>(root: Id, child: Id) -> Vec<MNode<Id>> {
        vec![
            MNode {
                id: Some(root),
                label: "root".into(),
                full_label: "root".into(),
                truncated: false,
                level: 0,
                children: vec![1],
                has_hidden_children: false,
                x: 0.0,
                y: 0.0,
            },
            MNode {
                id: Some(child),
                label: "child".into(),
                full_label: "child".into(),
                truncated: false,
                level: 1,
                children: Vec::new(),
                has_hidden_children: false,
                x: 0.0,
                y: 0.0,
            },
        ]
    }

    #[test]
    fn generic_layout_accepts_non_copy_workspace_identity() {
        let mut document = sample_nodes(BlockId(1), BlockId(2));
        let mut workspace = sample_nodes(
            PathId("/project".into()),
            PathId("/project/readme.md".into()),
        );
        let mut document_y = PAD;
        let mut workspace_y = PAD;
        layout(&mut document, 0, &mut document_y);
        layout(&mut workspace, 0, &mut workspace_y);

        assert_eq!(document_y, workspace_y);
        assert_eq!(document[0].x, workspace[0].x);
        assert_eq!(document[0].y, workspace[0].y);
        assert_eq!(document[1].x, workspace[1].x);
        assert_eq!(document[1].y, workspace[1].y);
        let _state = MindmapState::<PathId>::default();
    }

    fn canvas_program(
        nodes: Vec<MNode<PathId>>,
        selected: PathId,
        generation: u64,
    ) -> MindmapProgram<'static, PathId, ()> {
        MindmapProgram {
            nodes: std::sync::Arc::new(nodes),
            content_size: Size::new(1200.0, 900.0),
            palette: Palette::ONE_DARK,
            background: Palette::ONE_DARK.bg,
            selected: Some(selected.clone()),
            focus: Some(selected),
            panel_open: true,
            panel_width: 360.0,
            autocenter: true,
            layout_generation: Some(generation),
            keyboard_zoom_enabled: false,
            native_pinch_log: 0.0,
            zoom_request: ZoomRequest::default(),
            on_toggle: Box::new(|_| ()),
            on_select: Box::new(|_| ()),
            on_activate: None,
            hover_text: None,
            on_deselect: (),
        }
    }

    fn folder_graph(child: Option<PathId>) -> Vec<MNode<PathId>> {
        let mut nodes = vec![
            MNode {
                id: Some(PathId("/Users/me".into())),
                label: "me".into(),
                full_label: "me".into(),
                truncated: false,
                level: 0,
                children: vec![1, 2],
                has_hidden_children: false,
                x: 0.0,
                y: 0.0,
            },
            MNode {
                id: Some(PathId("/Users/me/Documents".into())),
                label: "Documents".into(),
                full_label: "Documents".into(),
                truncated: false,
                level: 1,
                children: Vec::new(),
                has_hidden_children: false,
                x: 0.0,
                y: 0.0,
            },
            MNode {
                id: Some(PathId("/Users/me/Downloads".into())),
                label: "Downloads".into(),
                full_label: "Downloads".into(),
                truncated: false,
                level: 1,
                children: Vec::new(),
                has_hidden_children: false,
                x: 0.0,
                y: 0.0,
            },
        ];
        if let Some(child) = child {
            nodes[1].children.push(3);
            nodes.push(MNode {
                id: Some(child),
                label: "Notes".into(),
                full_label: "Notes".into(),
                truncated: false,
                level: 2,
                children: Vec::new(),
                has_hidden_children: false,
                x: 0.0,
                y: 0.0,
            });
            nodes[1].children.push(4);
            nodes.push(MNode {
                id: Some(PathId("/Users/me/Documents/Archive".into())),
                label: "Archive".into(),
                full_label: "Archive".into(),
                truncated: false,
                level: 2,
                children: Vec::new(),
                has_hidden_children: false,
                x: 0.0,
                y: 0.0,
            });
        }
        let mut y_cursor = PAD;
        layout(&mut nodes, 0, &mut y_cursor);
        nodes
    }

    #[test]
    fn sync_anim_runs_once_per_node_list() {
        let documents = PathId("/Users/me/Documents".into());
        let program = canvas_program(folder_graph(None), documents.clone(), 1);
        let mut state = MindmapState::default();
        program.sync_anim(&mut state);
        assert_eq!(state.anim.len(), program.nodes.len());

        // A redraw with the same list must not rebuild animation state.
        state.anim.clear();
        program.sync_anim(&mut state);
        assert!(state.anim.is_empty());

        // A rebuilt list syncs again, even with identical contents.
        let rebuilt = canvas_program(folder_graph(None), documents, 1);
        rebuilt.sync_anim(&mut state);
        assert_eq!(state.anim.len(), rebuilt.nodes.len());
    }

    fn nested_shell_graph(shell: bool) -> Vec<MNode<PathId>> {
        let mut nodes = vec![
            MNode {
                id: Some(PathId("/Users/me".into())),
                label: "me".into(),
                full_label: "me".into(),
                truncated: false,
                level: 0,
                children: vec![1],
                has_hidden_children: false,
                x: 0.0,
                y: 0.0,
            },
            MNode {
                id: Some(PathId("/Users/me/Documents".into())),
                label: "Documents".into(),
                full_label: "Documents".into(),
                truncated: false,
                level: 1,
                children: Vec::new(),
                has_hidden_children: false,
                x: 0.0,
                y: 0.0,
            },
        ];
        if shell {
            nodes[1].children.push(2);
            nodes.push(MNode {
                id: Some(PathId("/Users/me/Documents/Archive".into())),
                label: "Archive".into(),
                full_label: "Archive".into(),
                truncated: false,
                level: 2,
                children: Vec::new(),
                has_hidden_children: false,
                x: 0.0,
                y: 0.0,
            });
        }
        let mut y_cursor = PAD;
        layout(&mut nodes, 0, &mut y_cursor);
        nodes
    }

    #[test]
    fn full_mindmap_async_expansion_refocuses_selected_folder_after_relayout() {
        let root = PathId("/Users/me".into());
        let documents = PathId("/Users/me/Documents".into());
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(1000.0, 800.0));
        let initial = canvas_program(folder_graph(None), documents.clone(), 1);
        let mut state = MindmapState::default();
        state.zoom = 1.0;

        // Entry establishes the deliberate initial folder focus. Mark the
        // initial node animation settled before the async expansion arrives.
        initial.sync_anim(&mut state);
        initial.sync_focus_pan(&mut state, bounds);
        for anim in state.anim.values_mut() {
            anim.current_x = anim.target_x;
            anim.current_y = anim.target_y;
            anim.done = true;
        }
        state.pan_anim = None;

        let accepted = canvas_program(
            folder_graph(Some(PathId("/Users/me/Documents/Notes".into()))),
            documents,
            2,
        );
        accepted.sync_anim(&mut state);
        accepted.sync_focus_pan(&mut state, bounds);

        let selected_idx = accepted
            .nodes
            .iter()
            .position(|node| node.id.as_ref() == accepted.selected.as_ref())
            .unwrap();
        let target = accepted.focus_target_for_node(
            &state,
            selected_idx,
            bounds,
            bounds.width,
            false,
            false,
            true,
        );
        let pan_target = state
            .pan_anim
            .expect("relayout should preserve focus")
            .target;
        assert_eq!(pan_target, target);

        let root_idx = accepted
            .nodes
            .iter()
            .position(|node| node.id.as_ref() == Some(&root))
            .unwrap();
        let root_target = accepted.focus_target_for_node(
            &state,
            root_idx,
            bounds,
            bounds.width,
            false,
            false,
            true,
        );
        assert_ne!(
            pan_target, root_target,
            "async relayout must not refocus root"
        );
    }

    #[test]
    fn explicit_canvas_focus_request_targets_requested_node() {
        let root = PathId("/Users/me".into());
        let child = PathId("/Users/me/Documents".into());
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(1000.0, 800.0));
        let mut program = canvas_program(folder_graph(Some(child.clone())), root.clone(), 1);
        // Selection and viewport focus are separate contracts: an async
        // navigator may keep its selection ring while explicitly requesting
        // the newly materialized file/folder as the canvas focus target.
        program.focus = Some(child.clone());
        let mut state = MindmapState::default();
        state.zoom = 1.0;
        program.sync_anim(&mut state);
        program.sync_focus_pan(&mut state, bounds);

        let child_idx = program
            .nodes
            .iter()
            .position(|node| node.id.as_ref() == Some(&child))
            .unwrap();
        // The first redraw snaps because the canvas panel has just opened;
        // later redraws animate the same target. Either way the observable
        // pan must be computed from the explicit focus request.
        let pan_target = state.pan;
        let child_target =
            program.focus_target_for_node(&state, child_idx, bounds, 0.0, true, false, true);
        assert_eq!(pan_target, child_target);

        let root_idx = program
            .nodes
            .iter()
            .position(|node| node.id.as_ref() == Some(&root))
            .unwrap();
        let root_target =
            program.focus_target_for_node(&state, root_idx, bounds, 0.0, true, false, false);
        assert_ne!(pan_target, root_target, "focus must not fall back to root");
    }

    #[test]
    fn explicit_focus_waits_for_materialized_node_without_poisoning_canvas_state() {
        let root = PathId("/Users/me".into());
        let file = PathId("/Users/me/Documents/Notes.md".into());
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(1000.0, 800.0));
        let mut state = MindmapState::default();
        state.zoom = 1.0;

        // The workspace root is rendered first while the accepted branch
        // listing is still in flight.
        let initial = canvas_program(folder_graph(None), root.clone(), 1);
        initial.sync_anim(&mut state);
        initial.sync_focus_pan(&mut state, bounds);
        state.pan_anim = None;

        // The navigator has already accepted the file selection/focus intent,
        // but one transient graph frame still lacks the file node. A focus
        // request must remain pending rather than being marked as consumed.
        let pending = canvas_program(folder_graph(None), file.clone(), 1);
        pending.sync_anim(&mut state);
        pending.sync_focus_pan(&mut state, bounds);

        // The final accepted graph materializes the exact file without any
        // additional selection or layout-generation change. This is the
        // manual root-jump path: the actual pan target must now be the file.
        let accepted = canvas_program(folder_graph(Some(file.clone())), file.clone(), 1);
        accepted.sync_anim(&mut state);
        accepted.sync_focus_pan(&mut state, bounds);
        let file_idx = accepted
            .nodes
            .iter()
            .position(|node| node.id.as_ref() == Some(&file))
            .unwrap();
        let target = accepted.focus_target_for_node(
            &state,
            file_idx,
            bounds,
            bounds.width,
            false,
            false,
            true,
        );
        let pan_target = state.pan_anim.map(|anim| anim.target).unwrap_or(state.pan);
        assert_eq!(pan_target, target, "accepted file must own the canvas pan");

        let root_idx = accepted
            .nodes
            .iter()
            .position(|node| node.id.as_ref() == Some(&root))
            .unwrap();
        let root_target = accepted.focus_target_for_node(
            &state,
            root_idx,
            bounds,
            bounds.width,
            false,
            false,
            true,
        );
        assert_ne!(
            pan_target, root_target,
            "a missing-node frame must not poison focus"
        );
    }

    #[test]
    fn full_mindmap_rendered_workspace_graph_focuses_file_after_materialization() {
        use crate::tree::{Node, RecursiveFileCount};
        use crate::workspace_mindmap::{self, MaterializedFolder, WorkspaceNodeId};
        use std::collections::HashMap;
        use std::path::PathBuf;
        use std::sync::Arc;

        let root = PathBuf::from("/Users/me/project");
        let documents = root.join("Documents");
        let file = documents.join("Notes.md");
        let tree = Node {
            path: root.clone(),
            name: "project".into(),
            is_dir: true,
            children: vec![Node {
                path: documents.clone(),
                name: "Documents".into(),
                is_dir: true,
                children: Vec::new(),
                recursive_supported_file_count: Some(RecursiveFileCount::Exact(1)),
            }],
            recursive_supported_file_count: Some(RecursiveFileCount::Exact(1)),
        };
        let expanded = HashSet::from([root.clone(), documents.clone()]);
        let pending = HashSet::new();
        let empty_materialized = HashMap::new();
        let initial_graph = workspace_mindmap::from_tree_with_hidden(
            &tree,
            &expanded,
            &empty_materialized,
            &pending,
            &HashSet::new(),
            false,
        );
        let mut accepted_materialized = HashMap::new();
        accepted_materialized.insert(
            documents.clone(),
            MaterializedFolder::Loaded {
                folders: Arc::new(Vec::new()),
                files: Arc::new(vec![file.clone()]),
                recursive_supported_file_count: RecursiveFileCount::Exact(1),
                truncated: false,
                shown: crate::tree::DIRECT_PAGE,
            },
        );
        let accepted_graph = workspace_mindmap::from_tree_with_hidden(
            &tree,
            &expanded,
            &accepted_materialized,
            &pending,
            &HashSet::new(),
            false,
        );
        let root_id = WorkspaceNodeId::Root(root);
        let file_id = WorkspaceNodeId::File(file);
        let program = |graph: &workspace_mindmap::WorkspaceGraph,
                       selected: WorkspaceNodeId|
         -> MindmapProgram<'static, WorkspaceNodeId, ()> {
            MindmapProgram {
                nodes: graph.nodes.clone(),
                content_size: graph.content_size,
                palette: Palette::ONE_DARK,
                background: Palette::ONE_DARK.bg,
                selected: Some(selected),
                focus: None,
                panel_open: true,
                panel_width: 360.0,
                autocenter: true,
                layout_generation: Some(1),
                keyboard_zoom_enabled: false,
                native_pinch_log: 0.0,
                zoom_request: ZoomRequest::default(),
                on_toggle: Box::new(|_| ()),
                on_select: Box::new(|_| ()),
                on_activate: None,
                hover_text: None,
                on_deselect: (),
            }
        };
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(1000.0, 800.0));
        let mut state = MindmapState::default();
        state.zoom = 1.0;
        let initial = program(&initial_graph, root_id.clone());
        initial.sync_anim(&mut state);
        initial.sync_focus_pan(&mut state, bounds);
        state.pan_anim = None;

        // Selection/focus is accepted before one stale graph frame has the
        // file node. The final graph below is the actual workspace adapter
        // output rendered by Full Mindmap after branch materialization.
        let pending_program = {
            let mut pending = program(&initial_graph, file_id.clone());
            pending.focus = Some(file_id.clone());
            pending
        };
        pending_program.sync_anim(&mut state);
        pending_program.sync_focus_pan(&mut state, bounds);

        let accepted_program = {
            let mut accepted = program(&accepted_graph, file_id.clone());
            accepted.focus = Some(file_id.clone());
            accepted
        };
        accepted_program.sync_anim(&mut state);
        accepted_program.sync_focus_pan(&mut state, bounds);
        let file_idx = accepted_program
            .nodes
            .iter()
            .position(|node| node.id.as_ref() == Some(&file_id))
            .unwrap();
        let file_target = accepted_program.focus_target_for_node(
            &state,
            file_idx,
            bounds,
            bounds.width,
            false,
            false,
            true,
        );
        let pan_target = state.pan_anim.map(|anim| anim.target).unwrap_or(state.pan);
        assert_eq!(pan_target, file_target);
        let root_idx = accepted_program
            .nodes
            .iter()
            .position(|node| node.id.as_ref() == Some(&root_id))
            .unwrap();
        let root_target = accepted_program.focus_target_for_node(
            &state,
            root_idx,
            bounds,
            bounds.width,
            false,
            false,
            true,
        );
        assert_ne!(pan_target, root_target);
    }

    #[test]
    fn full_mindmap_right_status_replacement_targets_new_first_child() {
        let root = PathId("/Users/me".into());
        let status = PathId("/Users/me/Documents::loading".into());
        let child = PathId("/Users/me/Documents/Notes".into());
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(1000.0, 800.0));

        let mut pending_nodes = folder_graph(None);
        let status_idx = pending_nodes.len();
        pending_nodes[1].children.push(status_idx);
        pending_nodes.push(MNode {
            id: Some(status.clone()),
            label: "Loading files…".into(),
            full_label: "Loading files…".into(),
            truncated: false,
            level: 2,
            children: Vec::new(),
            has_hidden_children: false,
            x: 0.0,
            y: 0.0,
        });
        let pending = canvas_program(pending_nodes, status, 1);
        let mut state = MindmapState::default();
        state.zoom = 1.0;
        pending.sync_anim(&mut state);
        pending.sync_focus_pan(&mut state, bounds);
        for anim in state.anim.values_mut() {
            anim.current_x = anim.target_x;
            anim.current_y = anim.target_y;
            anim.done = true;
        }
        state.pan_anim = None;

        let accepted = canvas_program(
            folder_graph(Some(child)),
            PathId("/Users/me/Documents/Notes".into()),
            2,
        );
        accepted.sync_anim(&mut state);
        accepted.sync_focus_pan(&mut state, bounds);

        let child_idx = accepted
            .nodes
            .iter()
            .position(|node| node.id.as_ref() == accepted.selected.as_ref())
            .unwrap();
        let child_target = accepted.focus_target_for_node(
            &state,
            child_idx,
            bounds,
            bounds.width,
            false,
            false,
            true,
        );
        let pan_target = state
            .pan_anim
            .expect("new child should receive focus")
            .target;
        assert_eq!(pan_target, child_target);

        let root_idx = accepted
            .nodes
            .iter()
            .position(|node| node.id.as_ref() == Some(&root))
            .unwrap();
        let root_target = accepted.focus_target_for_node(
            &state,
            root_idx,
            bounds,
            bounds.width,
            false,
            false,
            true,
        );
        assert_ne!(
            pan_target, root_target,
            "Right must focus the accepted first child"
        );
    }

    #[test]
    fn full_mindmap_exact_empty_shell_relayout_focuses_nearest_ancestor() {
        let root = PathId("/Users/me".into());
        let documents = PathId("/Users/me/Documents".into());
        let shell = PathId("/Users/me/Documents/Archive".into());
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(1000.0, 800.0));
        let initial = canvas_program(nested_shell_graph(true), shell, 1);
        let mut state = MindmapState::default();
        state.zoom = 1.0;
        initial.sync_anim(&mut state);
        initial.sync_focus_pan(&mut state, bounds);
        for anim in state.anim.values_mut() {
            anim.current_x = anim.target_x;
            anim.current_y = anim.target_y;
            anim.done = true;
        }
        state.pan_anim = None;

        let accepted = canvas_program(nested_shell_graph(false), documents, 2);
        accepted.sync_anim(&mut state);
        accepted.sync_focus_pan(&mut state, bounds);

        let selected_idx = accepted
            .nodes
            .iter()
            .position(|node| node.id.as_ref() == accepted.selected.as_ref())
            .unwrap();
        let pan_target = state
            .pan_anim
            .expect("ancestor should receive relayout focus")
            .target;
        let ancestor_target = accepted.focus_target_for_node(
            &state,
            selected_idx,
            bounds,
            bounds.width,
            false,
            false,
            true,
        );
        assert_eq!(pan_target, ancestor_target);

        let root_idx = accepted
            .nodes
            .iter()
            .position(|node| node.id.as_ref() == Some(&root))
            .unwrap();
        let root_target = accepted.focus_target_for_node(
            &state,
            root_idx,
            bounds,
            bounds.width,
            false,
            false,
            true,
        );
        assert_ne!(
            pan_target, root_target,
            "removing a nested shell must not refocus the workspace root"
        );
    }

    fn key_pressed(
        key: iced::keyboard::Key,
        physical_key: iced::keyboard::key::Physical,
        modifiers: iced::keyboard::Modifiers,
    ) -> iced::Event {
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key,
            location: iced::keyboard::Location::Standard,
            modifiers,
            text: None,
            repeat: false,
        })
    }

    #[test]
    fn keyboard_zoom_uses_unmodified_graph_keys() {
        use iced::keyboard::key::{Code, Physical};
        use iced::keyboard::{Key, Modifiers};

        assert_eq!(
            keyboard_zoom(&key_pressed(
                Key::Character("+".into()),
                Physical::Code(Code::Equal),
                Modifiers::SHIFT,
            )),
            Some(KeyboardZoom::In)
        );
        assert_eq!(
            keyboard_zoom(&key_pressed(
                Key::Character("-".into()),
                Physical::Code(Code::NumpadSubtract),
                Modifiers::NONE,
            )),
            Some(KeyboardZoom::Out)
        );
        assert_eq!(
            keyboard_zoom(&key_pressed(
                Key::Character("0".into()),
                Physical::Code(Code::Digit0),
                Modifiers::NONE,
            )),
            Some(KeyboardZoom::Reset)
        );
        assert_eq!(
            keyboard_zoom(&key_pressed(
                Key::Character("+".into()),
                Physical::Code(Code::Equal),
                Modifiers::COMMAND | Modifiers::SHIFT,
            )),
            None
        );
        assert_eq!(
            keyboard_zoom(&key_pressed(
                Key::Character("=".into()),
                Physical::Code(Code::Equal),
                Modifiers::NONE,
            )),
            Some(KeyboardZoom::In)
        );
    }

    #[test]
    fn zoom_keeps_the_world_point_under_its_anchor() {
        let mut state = MindmapState::<BlockId>::default();
        state.zoom = 1.25;
        state.pan = Vector::new(18.0, -11.0);
        let anchor = Point::new(82.0, 47.0);
        let before = (
            (anchor.x - state.pan.x) / state.zoom,
            (anchor.y - state.pan.y) / state.zoom,
        );

        assert!(zoom_by(&mut state, anchor, 1.1));
        let after = (
            (anchor.x - state.pan.x) / state.zoom,
            (anchor.y - state.pan.y) / state.zoom,
        );

        assert!((before.0 - after.0).abs() < 0.0001);
        assert!((before.1 - after.1).abs() < 0.0001);
    }

    #[test]
    fn zoom_animation_keeps_the_anchor_fixed_and_compounds_requests() {
        let mut state = MindmapState::<BlockId>::default();
        state.zoom = 1.25;
        state.pan = Vector::new(18.0, -11.0);
        let anchor = Point::new(82.0, 47.0);
        let before = (
            (anchor.x - state.pan.x) / state.zoom,
            (anchor.y - state.pan.y) / state.zoom,
        );

        assert!(animate_zoom_by(&mut state, anchor, KEYBOARD_ZOOM_FACTOR));
        assert!(animate_zoom_by(&mut state, anchor, KEYBOARD_ZOOM_FACTOR));
        assert!(
            (state.zoom_anim.expect("animation").target_zoom - 1.25 * 1.25 * 1.25).abs() < 0.0001
        );

        state.zoom_anim.as_mut().expect("animation").start_at =
            Instant::now() - Duration::from_millis(70);
        assert!(step_zoom_animation(&mut state));
        let midpoint = (
            (anchor.x - state.pan.x) / state.zoom,
            (anchor.y - state.pan.y) / state.zoom,
        );
        assert!((before.0 - midpoint.0).abs() < 0.0001);
        assert!((before.1 - midpoint.1).abs() < 0.0001);

        state.zoom_anim.as_mut().expect("animation").start_at =
            Instant::now() - Duration::from_millis(200);
        assert!(!step_zoom_animation(&mut state));
        let after = (
            (anchor.x - state.pan.x) / state.zoom,
            (anchor.y - state.pan.y) / state.zoom,
        );
        assert!((before.0 - after.0).abs() < 0.0001);
        assert!((before.1 - after.1).abs() < 0.0001);
    }

    #[test]
    fn zoom_reset_retargets_an_in_flight_transition() {
        let mut state = MindmapState::<BlockId>::default();
        state.zoom = 1.0;
        let anchor = Point::new(50.0, 50.0);

        assert!(animate_zoom_by(&mut state, anchor, KEYBOARD_ZOOM_FACTOR));
        assert!(state.zoom_anim.is_some());
        state.zoom_anim.as_mut().expect("animation").start_at =
            Instant::now() - Duration::from_millis(70);
        assert!(animate_zoom_to(&mut state, anchor, 1.0));
        assert_eq!(state.zoom_anim.expect("reset animation").target_zoom, 1.0);
        state.zoom_anim.as_mut().expect("reset animation").start_at =
            Instant::now() - Duration::from_millis(200);
        assert!(!step_zoom_animation(&mut state));
        assert_eq!(state.zoom, 1.0);
        assert!(state.zoom_anim.is_none());
    }

    #[test]
    fn zoom_clamps_and_reset_returns_to_one_hundred_percent() {
        let mut state = MindmapState::<BlockId>::default();
        state.zoom = 1.0;
        let anchor = Point::new(50.0, 50.0);

        assert!(zoom_by(&mut state, anchor, 100.0));
        assert_eq!(state.zoom, ZOOM_MAX);
        assert!(zoom_by(&mut state, anchor, 0.0001));
        assert_eq!(state.zoom, ZOOM_MIN);
        assert!(set_zoom_about(&mut state, anchor, 1.0));
        assert_eq!(state.zoom, 1.0);
    }

    // --- Pointer model (MDV-048 P4) -------------------------------------

    /// What a pointer test program publishes.
    #[derive(Debug, Clone, PartialEq)]
    enum Ev {
        Toggle(PathId),
        Select(PathId),
        Activate(PathId),
        Deselect,
    }

    fn pointer_program(
        nodes: Vec<MNode<PathId>>,
        selected: Option<PathId>,
    ) -> MindmapProgram<'static, PathId, Ev> {
        MindmapProgram {
            nodes: std::sync::Arc::new(nodes),
            content_size: Size::new(1200.0, 900.0),
            palette: Palette::ONE_DARK,
            background: Palette::ONE_DARK.bg,
            selected,
            focus: None,
            panel_open: false,
            panel_width: 360.0,
            autocenter: true,
            layout_generation: Some(1),
            keyboard_zoom_enabled: false,
            native_pinch_log: 0.0,
            zoom_request: ZoomRequest::default(),
            on_toggle: Box::new(Ev::Toggle),
            on_select: Box::new(Ev::Select),
            on_activate: Some(Box::new(Ev::Activate)),
            hover_text: None,
            on_deselect: Ev::Deselect,
        }
    }

    const POINTER_BOUNDS: Rectangle = Rectangle {
        x: 0.0,
        y: 0.0,
        width: 1000.0,
        height: 800.0,
    };

    /// Sends one event at `at`; returns the published message.
    fn pointer_send(
        program: &MindmapProgram<'static, PathId, Ev>,
        state: &mut MindmapState<PathId>,
        event: iced::Event,
        at: Point,
    ) -> Option<Ev> {
        canvas::Program::update(
            program,
            state,
            &event,
            POINTER_BOUNDS,
            mouse::Cursor::Available(at),
        )
        .and_then(|action| action.into_inner().0)
    }

    fn press_at(
        program: &MindmapProgram<'static, PathId, Ev>,
        state: &mut MindmapState<PathId>,
        at: Point,
    ) -> Option<Ev> {
        pointer_send(
            program,
            state,
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            at,
        )
    }

    fn move_at(
        program: &MindmapProgram<'static, PathId, Ev>,
        state: &mut MindmapState<PathId>,
        at: Point,
    ) -> Option<Ev> {
        pointer_send(
            program,
            state,
            iced::Event::Mouse(mouse::Event::CursorMoved { position: at }),
            at,
        )
    }

    fn release_at(
        program: &MindmapProgram<'static, PathId, Ev>,
        state: &mut MindmapState<PathId>,
        at: Point,
    ) -> Option<Ev> {
        pointer_send(
            program,
            state,
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            at,
        )
    }

    fn node_idx(program: &MindmapProgram<'static, PathId, Ev>, path: &str) -> usize {
        program
            .nodes
            .iter()
            .position(|node| node.id.as_ref().is_some_and(|id| id.0 == path))
            .expect("node in graph")
    }

    /// Screen center of a node at zoom 1 and no pan.
    fn node_center(program: &MindmapProgram<'static, PathId, Ev>, path: &str) -> Point {
        let node = &program.nodes[node_idx(program, path)];
        Point::new(node.x + NODE_W / 2.0, node.y + NODE_H / 2.0)
    }

    const DOWNLOADS: &str = "/Users/me/Downloads";
    const DOCUMENTS: &str = "/Users/me/Documents";

    fn notes_graph() -> Vec<MNode<PathId>> {
        folder_graph(Some(PathId("/Users/me/Documents/Notes".into())))
    }

    #[test]
    fn a_node_acts_on_release_and_a_drag_from_it_pans() {
        let mut program = pointer_program(notes_graph(), None);
        // Measure the pointer alone, without the focus pan.
        program.autocenter = false;
        let mut state = MindmapState::default();
        let leaf = node_center(&program, DOWNLOADS);

        assert_eq!(press_at(&program, &mut state, leaf), None, "press waits");
        assert_eq!(
            release_at(&program, &mut state, leaf),
            Some(Ev::Select(PathId(DOWNLOADS.into()))),
            "a still release is the click"
        );

        // A small wobble is still a click.
        assert_eq!(press_at(&program, &mut state, leaf), None);
        let wobble = Point::new(leaf.x + 2.0, leaf.y + 1.0);
        assert_eq!(move_at(&program, &mut state, wobble), None);
        assert_eq!(state.pan, Vector::ZERO, "a wobble does not pan");
        assert!(release_at(&program, &mut state, wobble).is_some());

        // A drag that starts on a node pans the canvas and clicks nothing.
        let pan_before = state.pan;
        assert_eq!(press_at(&program, &mut state, leaf), None);
        let far = Point::new(leaf.x + 40.0, leaf.y + 10.0);
        assert_eq!(move_at(&program, &mut state, far), None);
        assert_eq!(release_at(&program, &mut state, far), None);
        assert_eq!(
            state.pan,
            Vector::new(pan_before.x + 40.0, pan_before.y + 10.0)
        );
    }

    #[test]
    fn a_press_that_leaves_the_canvas_or_ends_far_away_is_not_a_click() {
        let mut program = pointer_program(notes_graph(), None);
        program.autocenter = false;
        let mut state = live_state();
        let leaf = node_center(&program, DOWNLOADS);
        // Past the right edge, for example over the panel.
        let outside = Point::new(POINTER_BOUNDS.width + 40.0, leaf.y);

        // A fast move leaves the canvas before it passes the slop inside.
        assert_eq!(press_at(&program, &mut state, leaf), None);
        assert_eq!(move_at(&program, &mut state, outside), None);
        assert_eq!(
            release_at(&program, &mut state, outside),
            None,
            "a release outside the canvas clicks nothing"
        );
        assert_eq!(state.pan, Vector::ZERO, "outside the canvas nothing pans");

        // Out and back to the press point: the press already left.
        assert_eq!(press_at(&program, &mut state, leaf), None);
        assert_eq!(move_at(&program, &mut state, outside), None);
        let _ = move_at(&program, &mut state, leaf);
        assert_eq!(release_at(&program, &mut state, leaf), None);

        // A release far from the press with no move event in between.
        assert_eq!(press_at(&program, &mut state, leaf), None);
        assert_eq!(
            release_at(&program, &mut state, Point::new(leaf.x + 40.0, leaf.y)),
            None,
            "a far release inside the canvas is not a click"
        );
        assert_eq!(press_at(&program, &mut state, leaf), None);
        assert_eq!(release_at(&program, &mut state, outside), None);

        // Empty canvas: a release outside does not deselect.
        let selected = pointer_program(notes_graph(), Some(PathId(DOWNLOADS.into())));
        let empty = Point::new(5.0, 5.0);
        assert_eq!(press_at(&selected, &mut state, empty), None);
        assert_eq!(release_at(&selected, &mut state, outside), None);

        // A still click still selects.
        assert_eq!(
            click_at(&program, &mut state, leaf),
            Some(Ev::Select(PathId(DOWNLOADS.into())))
        );
    }

    /// Screen center of a branch node's chevron at zoom 1 and no pan.
    fn chevron_point(program: &MindmapProgram<'static, PathId, Ev>, path: &str) -> Point {
        let node = &program.nodes[node_idx(program, path)];
        Point::new(node.x + NODE_W - CHEVRON_INSET, node.y + NODE_H / 2.0)
    }

    fn click_at(
        program: &MindmapProgram<'static, PathId, Ev>,
        state: &mut MindmapState<PathId>,
        at: Point,
    ) -> Option<Ev> {
        assert_eq!(press_at(program, state, at), None);
        release_at(program, state, at)
    }

    #[test]
    fn a_click_selects_a_branch_and_its_chevron_toggles_it() {
        let mut program = pointer_program(notes_graph(), None);
        program.autocenter = false;
        let mut state = MindmapState::default();
        let documents = PathId(DOCUMENTS.into());

        let body = node_center(&program, DOCUMENTS);
        assert_eq!(
            click_at(&program, &mut state, body),
            Some(Ev::Select(documents.clone())),
            "the node body selects a branch"
        );
        let chevron = chevron_point(&program, DOCUMENTS);
        assert_eq!(
            click_at(&program, &mut state, chevron),
            Some(Ev::Toggle(documents.clone())),
            "the chevron folds it"
        );
        // Leaves have no chevron: the same spot selects.
        let leaf = chevron_point(&program, DOWNLOADS);
        assert_eq!(
            click_at(&program, &mut state, leaf),
            Some(Ev::Select(PathId(DOWNLOADS.into())))
        );
        assert_eq!(
            canvas::Program::mouse_interaction(
                &program,
                &state,
                POINTER_BOUNDS,
                mouse::Cursor::Available(chevron)
            ),
            mouse::Interaction::Pointer
        );

        // Zoomed far out, the chevron keeps a 16 px target.
        state.zoom = ZOOM_MIN;
        let node = &program.nodes[node_idx(&program, DOCUMENTS)];
        let center = Point::new(
            (node.x + NODE_W - CHEVRON_INSET) * ZOOM_MIN,
            (node.y + NODE_H / 2.0) * ZOOM_MIN,
        );
        let edge = Point::new(center.x, center.y + 7.5);
        assert_eq!(
            click_at(&program, &mut state, edge),
            Some(Ev::Toggle(documents))
        );
    }

    /// A canvas state as after its first event: zoom 1, no pan.
    fn live_state() -> MindmapState<PathId> {
        MindmapState {
            zoom: 1.0,
            initialized: true,
            ..MindmapState::default()
        }
    }

    /// Runs the redraw-time sync and finishes every animation.
    fn settle(
        program: &MindmapProgram<'static, PathId, Ev>,
        state: &mut MindmapState<PathId>,
        bounds: Rectangle,
    ) {
        program.sync_anim(state);
        program.sync_focus_pan(state, bounds);
        let done = Instant::now() - Duration::from_millis(1000);
        for anim in state.anim.values_mut() {
            anim.start_at = done;
        }
        if let Some(pan) = state.pan_anim.as_mut() {
            pan.start_at = done;
        }
        program.step_anim(state);
        program.step_pan_anim(state);
    }

    /// The program the app builds after a click selected `selected` and
    /// opened the panel.
    fn after_click(
        nodes: Vec<MNode<PathId>>,
        selected: &str,
        generation: u64,
    ) -> MindmapProgram<'static, PathId, Ev> {
        let mut program = pointer_program(nodes, Some(PathId(selected.into())));
        program.panel_open = true;
        program.layout_generation = Some(generation);
        program
    }

    /// Canvas bounds once the 360 px panel and its handle take their share.
    const NARROWED: Rectangle = Rectangle {
        x: 0.0,
        y: 0.0,
        width: 1000.0 - 360.0 - PANEL_HANDLE_W,
        height: 800.0,
    };

    fn screen_of(
        program: &MindmapProgram<'static, PathId, Ev>,
        state: &MindmapState<PathId>,
        path: &str,
    ) -> Point {
        let (x, y) = program.pos(state, node_idx(program, path));
        Point::new(x * state.zoom + state.pan.x, y * state.zoom + state.pan.y)
    }

    #[test]
    fn a_click_on_a_visible_node_keeps_the_canvas_still() {
        let before = pointer_program(notes_graph(), None);
        let mut state = live_state();
        settle(&before, &mut state, POINTER_BOUNDS);
        let pan = state.pan;

        let leaf = node_center(&before, DOWNLOADS);
        assert!(click_at(&before, &mut state, leaf).is_some());
        // The app selects the node and opens the panel, which narrows the
        // canvas. The node stays where it was clicked.
        let after = after_click(notes_graph(), DOWNLOADS, 1);
        settle(&after, &mut state, NARROWED);
        assert_eq!(state.pan, pan, "no jump on click");

        // Keyboard navigation still centers its node.
        let key = iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::ArrowUp),
            modified_key: keyboard::Key::Named(keyboard::key::Named::ArrowUp),
            physical_key: keyboard::key::Physical::Code(keyboard::key::Code::ArrowUp),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::NONE,
            text: None,
            repeat: false,
        });
        let _ = pointer_send(&after, &mut state, key, leaf);
        let keyed = after_click(notes_graph(), DOCUMENTS, 1);
        settle(&keyed, &mut state, NARROWED);
        let center = screen_of(&keyed, &state, DOCUMENTS);
        assert!(
            (center.x + NODE_W / 2.0 - NARROWED.width / 2.0).abs() < 0.5,
            "keyboard selection centers: {center:?}"
        );
    }

    #[test]
    fn a_click_on_a_node_the_panel_would_cover_brings_it_into_view() {
        let before = pointer_program(notes_graph(), None);
        let mut state = live_state();
        settle(&before, &mut state, POINTER_BOUNDS);
        // Push Downloads toward the right edge, where the panel opens.
        state.pan = Vector::new(250.0, 0.0);
        let leaf = screen_of(&before, &state, DOWNLOADS);
        let at = Point::new(leaf.x + 20.0, leaf.y + NODE_H / 2.0);
        assert!(click_at(&before, &mut state, at).is_some());
        let after = after_click(notes_graph(), DOWNLOADS, 1);
        settle(&after, &mut state, NARROWED);
        let shown = screen_of(&after, &state, DOWNLOADS);
        assert!(
            shown.x >= 0.0 && shown.x + NODE_W <= NARROWED.width,
            "the node is fully visible beside the panel: {shown:?}"
        );
    }

    #[test]
    fn folding_from_the_chevron_keeps_the_node_under_the_pointer() {
        let before = pointer_program(notes_graph(), None);
        let mut state = live_state();
        settle(&before, &mut state, POINTER_BOUNDS);
        let clicked = screen_of(&before, &state, DOCUMENTS);
        let chevron = chevron_point(&before, DOCUMENTS);
        assert_eq!(
            click_at(&before, &mut state, chevron),
            Some(Ev::Toggle(PathId(DOCUMENTS.into())))
        );
        // Folding Documents moves it in the rebuilt layout.
        let mut folded = folder_graph(None);
        folded[1].has_hidden_children = true;
        let after = after_click(folded, DOCUMENTS, 2);
        after.sync_anim(&mut state);
        let moved = &after.nodes[node_idx(&after, DOCUMENTS)];
        assert_ne!(moved.y, clicked.y, "the fixture must move the node");
        settle(&after, &mut state, NARROWED);
        let shown = screen_of(&after, &state, DOCUMENTS);
        assert!(
            (shown.x - clicked.x).abs() < 0.5 && (shown.y - clicked.y).abs() < 0.5,
            "the folded node stays put: {clicked:?} -> {shown:?}"
        );
    }

    /// A click whose release happens at `now`, so the double-click
    /// interval is measured on a test clock, not the wall clock.
    fn click_at_time(
        program: &MindmapProgram<'static, PathId, Ev>,
        state: &mut MindmapState<PathId>,
        at: Point,
        now: Instant,
    ) -> Option<Ev> {
        assert_eq!(press_at(program, state, at), None);
        program
            .release(state, now, Some(at))
            .and_then(|action| action.into_inner().0)
    }

    #[test]
    fn a_double_click_survives_the_opening_panel_moving_its_node() {
        let before = pointer_program(notes_graph(), None);
        let mut state = live_state();
        settle(&before, &mut state, POINTER_BOUNDS);
        // Downloads straddles the edge the panel opens to: its left half
        // stays on the canvas, the right half goes under the panel.
        let laid_out = &before.nodes[node_idx(&before, DOWNLOADS)];
        state.pan = Vector::new(NARROWED.width - NODE_W / 2.0 - laid_out.x, 0.0);
        let leaf = screen_of(&before, &state, DOWNLOADS);
        let at = Point::new(leaf.x + 20.0, leaf.y + NODE_H / 2.0);
        assert!(at.x < NARROWED.width, "the click stays on the canvas");
        let downloads = PathId(DOWNLOADS.into());
        let t0 = Instant::now();
        assert_eq!(
            click_at_time(&before, &mut state, at, t0),
            Some(Ev::Select(downloads.clone()))
        );
        // The app selects it and opens the panel; the redraw brings the
        // node into view, away from the pointer.
        let after = after_click(notes_graph(), DOWNLOADS, 1);
        settle(&after, &mut state, NARROWED);
        assert_ne!(
            after.node_at(&state, at),
            Some(node_idx(&after, DOWNLOADS)),
            "the fixture must move the node from under the pointer"
        );
        assert_eq!(
            click_at_time(&after, &mut state, at, t0 + Duration::from_millis(150)),
            Some(Ev::Activate(downloads.clone())),
            "the second click where the first landed still activates"
        );

        // Too slow, it is a click on empty canvas again.
        let t1 = t0 + Duration::from_secs(4);
        let mut state = live_state();
        settle(&before, &mut state, POINTER_BOUNDS);
        state.pan = Vector::new(NARROWED.width - NODE_W / 2.0 - laid_out.x, 0.0);
        let _ = click_at_time(&before, &mut state, at, t1);
        settle(&after, &mut state, NARROWED);
        assert_eq!(
            click_at_time(
                &after,
                &mut state,
                at,
                t1 + DOUBLE_CLICK + Duration::from_millis(1)
            ),
            Some(Ev::Deselect)
        );
    }

    #[test]
    fn a_double_click_activates_and_a_slow_second_click_selects() {
        let mut program = pointer_program(notes_graph(), None);
        program.autocenter = false;
        let mut state = live_state();
        let leaf = node_center(&program, DOWNLOADS);
        let other = node_center(&program, DOCUMENTS);
        let downloads = PathId(DOWNLOADS.into());
        let t0 = Instant::now();
        let ms = Duration::from_millis;

        assert_eq!(
            click_at_time(&program, &mut state, leaf, t0),
            Some(Ev::Select(downloads.clone()))
        );
        assert_eq!(
            click_at_time(&program, &mut state, leaf, t0 + ms(150)),
            Some(Ev::Activate(downloads.clone())),
            "a quick second click on the same node activates it"
        );
        // A third quick click starts over instead of activating again.
        assert_eq!(
            click_at_time(&program, &mut state, leaf, t0 + ms(250)),
            Some(Ev::Select(downloads.clone()))
        );

        // Too slow: two single clicks.
        let t1 = t0 + ms(2_000);
        assert_eq!(
            click_at_time(&program, &mut state, leaf, t1),
            Some(Ev::Select(downloads.clone()))
        );
        assert_eq!(
            click_at_time(&program, &mut state, leaf, t1 + DOUBLE_CLICK + ms(1)),
            Some(Ev::Select(downloads.clone()))
        );

        // Two different nodes: two single clicks.
        let t2 = t1 + ms(4_000);
        assert!(matches!(
            click_at_time(&program, &mut state, other, t2),
            Some(Ev::Select(_))
        ));
        assert_eq!(
            click_at_time(&program, &mut state, leaf, t2 + ms(100)),
            Some(Ev::Select(downloads.clone()))
        );

        // A chevron click never counts toward a double click.
        let t3 = t2 + ms(4_000);
        let chevron = chevron_point(&program, DOCUMENTS);
        assert!(matches!(
            click_at_time(&program, &mut state, chevron, t3),
            Some(Ev::Toggle(_))
        ));
        assert!(matches!(
            click_at_time(&program, &mut state, other, t3 + ms(100)),
            Some(Ev::Select(_))
        ));

        // Without an activate action a double click is two selections.
        program.on_activate = None;
        let t4 = t3 + ms(4_000);
        assert!(click_at_time(&program, &mut state, leaf, t4).is_some());
        assert_eq!(
            click_at_time(&program, &mut state, leaf, t4 + ms(100)),
            Some(Ev::Select(downloads))
        );
    }

    #[test]
    fn a_double_click_activates_a_document_heading_and_the_root() {
        #[derive(Debug, Clone, PartialEq)]
        enum DocEv {
            Toggle(BlockId),
            Select(BlockId),
            Activate(BlockId),
            Deselect,
        }
        let ast = vec![heading(1, 1, "Top"), heading(2, 2, "Child")];
        let (nodes, content_size) = build_layout(&ast, None, &HashSet::new());
        let program = MindmapProgram {
            nodes: std::sync::Arc::new(nodes),
            content_size,
            palette: Palette::ONE_DARK,
            background: Palette::ONE_DARK.bg,
            selected: None,
            focus: None,
            panel_open: false,
            panel_width: 360.0,
            autocenter: false,
            layout_generation: Some(1),
            keyboard_zoom_enabled: false,
            native_pinch_log: 0.0,
            zoom_request: ZoomRequest::default(),
            on_toggle: Box::new(DocEv::Toggle),
            on_select: Box::new(DocEv::Select),
            on_activate: Some(Box::new(DocEv::Activate)),
            hover_text: None,
            on_deselect: DocEv::Deselect,
        };
        let mut state = MindmapState {
            zoom: 1.0,
            initialized: true,
            ..MindmapState::default()
        };
        let mut click = |id: BlockId, now: Instant| {
            let node = program
                .nodes
                .iter()
                .find(|node| node.id == Some(id))
                .expect("node");
            // The label, clear of a branch's chevron.
            let at = Point::new(node.x + 40.0, node.y + NODE_H / 2.0);
            let pressed = canvas::Program::update(
                &program,
                &mut state,
                &iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                POINTER_BOUNDS,
                mouse::Cursor::Available(at),
            );
            assert!(pressed.is_some_and(|action| action.into_inner().0.is_none()));
            program
                .release(&mut state, now, Some(at))
                .and_then(|action| action.into_inner().0)
        };
        let t0 = Instant::now();
        let quick = t0 + Duration::from_millis(150);
        let child = BlockId(2);
        assert_eq!(click(child, t0), Some(DocEv::Select(child)));
        assert_eq!(click(child, quick), Some(DocEv::Activate(child)));
        let later = t0 + Duration::from_secs(5);
        assert_eq!(click(DOC_ROOT, later), Some(DocEv::Select(DOC_ROOT)));
        assert_eq!(
            click(DOC_ROOT, later + Duration::from_millis(150)),
            Some(DocEv::Activate(DOC_ROOT))
        );
    }

    #[test]
    fn hover_fills_a_clickable_node_and_a_still_pointer_redraws_nothing() {
        let program = pointer_program(notes_graph(), None);
        let mut state = live_state();
        let leaf = node_center(&program, DOWNLOADS);
        let hover = |state: &mut MindmapState<PathId>, at: Point| {
            canvas::Program::update(
                &program,
                state,
                &iced::Event::Mouse(mouse::Event::CursorMoved { position: at }),
                POINTER_BOUNDS,
                mouse::Cursor::Available(at),
            )
            .map(|action| action.into_inner().1)
        };

        assert_eq!(
            hover(&mut state, leaf),
            Some(window::RedrawRequest::NextFrame),
            "entering a node redraws once"
        );
        assert_eq!(
            program.hover_highlight(&state),
            Some(node_idx(&program, DOWNLOADS))
        );
        let inside = Point::new(leaf.x + 30.0, leaf.y);
        assert_eq!(
            hover(&mut state, inside),
            None,
            "moving within it: no redraw"
        );
        assert_eq!(
            hover(&mut state, inside),
            None,
            "a still pointer: no redraw"
        );

        // Empty canvas and the document's unselectable root get no fill.
        assert_eq!(
            hover(&mut state, Point::new(5.0, 5.0)),
            Some(window::RedrawRequest::NextFrame)
        );
        assert_eq!(program.hover_highlight(&state), None);
        let mut doc_nodes = notes_graph();
        doc_nodes[0].id = None;
        let doc = pointer_program(doc_nodes, None);
        let mut doc_state = live_state();
        let root = Point::new(doc.nodes[0].x + 20.0, doc.nodes[0].y + 10.0);
        let _ = move_at(&doc, &mut doc_state, root);
        assert_eq!(doc.hover_highlight(&doc_state), None);

        // A drag shows the grab, not a hover fill.
        let _ = move_at(&program, &mut state, leaf);
        let _ = press_at(&program, &mut state, leaf);
        let _ = move_at(&program, &mut state, Point::new(leaf.x + 30.0, leaf.y));
        assert_eq!(program.hover_highlight(&state), None);
    }

    /// One redraw with the pointer at `at`; returns its redraw request.
    fn redraw_at(
        program: &MindmapProgram<'static, PathId, Ev>,
        state: &mut MindmapState<PathId>,
        at: Point,
    ) -> Option<window::RedrawRequest> {
        canvas::Program::update(
            program,
            state,
            &iced::Event::Window(window::Event::RedrawRequested(Instant::now())),
            POINTER_BOUNDS,
            mouse::Cursor::Available(at),
        )
        .map(|action| action.into_inner().1)
        .filter(|request| *request != window::RedrawRequest::Wait)
    }

    #[test]
    fn the_hover_follows_the_node_under_a_still_pointer() {
        // The node list changes under a still pointer: a folder gains
        // children, and the node at the hovered index moves away.
        let before = pointer_program(folder_graph(None), None);
        let mut state = live_state();
        settle(&before, &mut state, POINTER_BOUNDS);
        let at = node_center(&before, DOWNLOADS);
        let _ = move_at(&before, &mut state, at);
        let downloads = node_idx(&before, DOWNLOADS);
        assert_eq!(before.hover_highlight(&state), Some(downloads));
        let mut after = pointer_program(notes_graph(), None);
        after.autocenter = false;
        settle(&after, &mut state, POINTER_BOUNDS);
        let under = after.node_at(&state, at);
        assert_ne!(under, Some(node_idx(&after, DOWNLOADS)), "it moved");
        assert_eq!(
            redraw_at(&after, &mut state, at),
            None,
            "the hover fix costs no extra frame"
        );
        assert_eq!(after.hover_highlight(&state), under);

        // A zoom button zooms about the canvas center, not the pointer.
        let mut program = pointer_program(notes_graph(), None);
        program.autocenter = false;
        let mut state = live_state();
        settle(&program, &mut state, POINTER_BOUNDS);
        let at = node_center(&program, DOWNLOADS);
        let _ = move_at(&program, &mut state, at);
        let _ = redraw_at(&program, &mut state, at);
        for _ in 0..3 {
            program.zoom_request = program.zoom_request.next(ZoomCommand::In);
            let _ = redraw_at(&program, &mut state, at);
            finish_zoom(&mut state);
        }
        let _ = redraw_at(&program, &mut state, at);
        let under = program.node_at(&state, at);
        assert_ne!(under, Some(node_idx(&program, DOWNLOADS)), "it moved");
        assert_eq!(program.hover_highlight(&state), under);

        // A drag hides the fill; its release redraws once to bring it back.
        let _ = press_at(&program, &mut state, at);
        let far = Point::new(at.x + 60.0, at.y);
        let _ = move_at(&program, &mut state, far);
        let released = canvas::Program::update(
            &program,
            &mut state,
            &iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            POINTER_BOUNDS,
            mouse::Cursor::Available(far),
        )
        .map(|action| action.into_inner().1);
        assert_eq!(released, Some(window::RedrawRequest::NextFrame));
    }

    #[test]
    fn a_hover_text_tooltip_shows_for_every_node() {
        let mut program = pointer_program(notes_graph(), None);
        let downloads = node_idx(&program, DOWNLOADS);
        assert_eq!(
            program.tooltip_text(downloads),
            None,
            "short labels need none"
        );
        program.hover_text = Some(Box::new(|id: &PathId| Some(id.0.clone())));
        assert_eq!(program.tooltip_text(downloads).as_deref(), Some(DOWNLOADS));

        // A path too wide for the canvas keeps its end, where the name is.
        let long = format!("/{}/notes.md", "very-long-folder".repeat(20));
        let (fitted, cut) = fit_tail_at(&long, 300.0, FONT_SIZE);
        assert!(cut && fitted.starts_with('…') && fitted.ends_with("/notes.md"));
        assert!(text_width_at(&fitted, FONT_SIZE) <= 300.0);
    }

    #[test]
    fn touch_geometry_uses_two_finger_distance_and_midpoint() {
        let points = vec![
            (touch::Finger(1), Point::new(10.0, 20.0)),
            (touch::Finger(2), Point::new(13.0, 24.0)),
        ];
        let (span, center) = touch_geometry(&points).expect("two contacts");

        assert_eq!(span, 5.0);
        assert_eq!(center, Point::new(11.5, 22.0));
    }

    /// One redraw of the canvas, the event that applies a zoom request.
    fn redraw(program: &MindmapProgram<'static, PathId, Ev>, state: &mut MindmapState<PathId>) {
        let _ = canvas::Program::update(
            program,
            state,
            &iced::Event::Window(window::Event::RedrawRequested(Instant::now())),
            POINTER_BOUNDS,
            mouse::Cursor::Unavailable,
        );
    }

    /// Ends any zoom animation at its target.
    fn finish_zoom(state: &mut MindmapState<PathId>) {
        if let Some(animation) = state.zoom_anim.as_mut() {
            animation.start_at = Instant::now() - Duration::from_secs(1);
        }
        step_zoom_animation(state);
    }

    fn press_zoom(
        program: &mut MindmapProgram<'static, PathId, Ev>,
        state: &mut MindmapState<PathId>,
        command: ZoomCommand,
    ) {
        program.zoom_request = program.zoom_request.next(command);
        redraw(program, state);
        finish_zoom(state);
    }

    #[test]
    fn zoom_buttons_fit_step_and_reset_the_canvas() {
        let mut program = pointer_program(notes_graph(), None);
        program.autocenter = false;
        program.content_size = Size::new(2000.0, 400.0);
        let mut state = live_state();

        // A canvas that first sees a request replays nothing.
        program.zoom_request = ZoomRequest::default().next(ZoomCommand::In);
        redraw(&program, &mut state);
        finish_zoom(&mut state);
        assert_eq!(state.zoom, 1.0, "no replay on a new canvas");

        press_zoom(&mut program, &mut state, ZoomCommand::Fit);
        let fit = (1000.0f32 / 2000.0).min((800.0 - FIT_BOTTOM_INSET) / 400.0);
        assert!((state.zoom - fit).abs() < 1e-4, "fit zoom {}", state.zoom);
        let usable_h = 800.0 - FIT_BOTTOM_INSET;
        assert!((state.pan.x - (1000.0 - 2000.0 * fit) / 2.0).abs() < 0.5);
        assert!((state.pan.y - (usable_h - 400.0 * fit) / 2.0).abs() < 0.5);

        // Without a new press a redraw leaves the view alone.
        let before = (state.zoom, state.pan);
        redraw(&program, &mut state);
        finish_zoom(&mut state);
        assert_eq!((state.zoom, state.pan), before);

        press_zoom(&mut program, &mut state, ZoomCommand::In);
        assert!((state.zoom - fit * KEYBOARD_ZOOM_FACTOR).abs() < 1e-4);
        press_zoom(&mut program, &mut state, ZoomCommand::Out);
        assert!((state.zoom - fit).abs() < 1e-4);
        press_zoom(&mut program, &mut state, ZoomCommand::Reset);
        assert!((state.zoom - 1.0).abs() < 1e-4);

        // A small map fits at 100%, not blown up.
        program.content_size = Size::new(300.0, 200.0);
        press_zoom(&mut program, &mut state, ZoomCommand::Out);
        press_zoom(&mut program, &mut state, ZoomCommand::Fit);
        assert!((state.zoom - 1.0).abs() < 1e-4);
    }

    #[test]
    fn a_drag_keeps_part_of_the_map_on_screen() {
        let mut program = pointer_program(notes_graph(), None);
        program.autocenter = false;
        program.content_size = Size::new(600.0, 300.0);
        let mut state = live_state();

        // Up and left as far as the canvas allows.
        let low = Point::new(990.0, 790.0);
        let high = Point::new(10.0, 10.0);
        assert_eq!(press_at(&program, &mut state, low), None);
        let _ = move_at(&program, &mut state, high);
        let _ = release_at(&program, &mut state, high);
        assert!(
            state.pan.x + 600.0 >= PAN_KEEP_VISIBLE - 0.01
                && state.pan.y + 300.0 >= PAN_KEEP_VISIBLE - 0.01,
            "the map's far corner stays on screen: {:?}",
            state.pan
        );

        // Then down and right, twice.
        for _ in 0..2 {
            assert_eq!(press_at(&program, &mut state, high), None);
            let _ = move_at(&program, &mut state, low);
            let _ = release_at(&program, &mut state, low);
        }
        assert!(
            state.pan.x <= 1000.0 - PAN_KEEP_VISIBLE + 0.01
                && state.pan.y <= 800.0 - PAN_KEEP_VISIBLE + 0.01,
            "the map's near corner stays on screen: {:?}",
            state.pan
        );

        // A drag that starts out of range (after a zoom) never jumps.
        state.pan = Vector::new(-2000.0, 0.0);
        let from = Point::new(500.0, 400.0);
        assert_eq!(press_at(&program, &mut state, from), None);
        let _ = move_at(&program, &mut state, Point::new(510.0, 400.0));
        assert_eq!(state.pan, Vector::new(-1990.0, 0.0));
        let _ = release_at(&program, &mut state, Point::new(510.0, 400.0));
    }
}
