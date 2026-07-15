use crate::ast::{Block, BlockId, Inline};
use crate::theme::Palette;
use iced::widget::canvas::{self, path, Fill, Path, Stroke, Text};
use iced::{mouse, window, Point, Rectangle, Renderer, Size, Theme, Vector};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

pub const NODE_W: f32 = 260.0;
pub const NODE_H: f32 = 36.0;
pub const X_GAP: f32 = 60.0;
pub const Y_GAP: f32 = 12.0;
pub const PAD: f32 = 32.0;
pub const PANEL_HANDLE_W: f32 = 5.0;
const FONT_SIZE: f32 = 14.0;
// Hard cap to bound the truncate scan on pathological inputs. The pixel
// budget below will usually cut earlier; this is just a safety lid.
const MAX_CHARS: usize = 120;
// Inner horizontal padding inside a node before text starts butting the edge.
const TEXT_INSET_X: f32 = 14.0;
const ZOOM_MIN: f32 = 0.2;
const ZOOM_MAX: f32 = 4.0;
const ANIM_DURATION: Duration = Duration::from_millis(220);

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
        id: None,
        label: root_label,
        full_label: doc_title.to_string(),
        truncated: root_trunc,
        level: 0,
        children: Vec::new(),
        has_hidden_children: false,
        x: 0.0,
        y: 0.0,
    });
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
    last_selected: Option<Id>,
    last_bounds_w: f32,
    last_bounds_h: f32,
    last_panel_open: bool,
    /// Full Mindmap increments this generation whenever its visible graph is
    /// rebuilt. Keeping it separate from selection lets the canvas preserve a
    /// selected folder's focus across async child discovery even when the
    /// selected identity itself does not change.
    last_layout_generation: Option<u64>,
    hovered_idx: Option<usize>,
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
            last_selected: None,
            last_bounds_w: 0.0,
            last_bounds_h: 0.0,
            last_panel_open: false,
            last_layout_generation: None,
            hovered_idx: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct PanAnim {
    start: Vector,
    target: Vector,
    start_at: Instant,
}

pub struct MindmapProgram<'a, Id, Message> {
    pub nodes: std::sync::Arc<Vec<MNode<Id>>>,
    pub content_size: Size,
    pub palette: Palette,
    pub selected: Option<Id>,
    pub panel_open: bool,
    pub panel_width: f32,
    pub autocenter: bool,
    /// Optional source-generation signal for navigators whose visible graph
    /// can be rebuilt without changing selection. Document mindmaps leave it
    /// unset; Full Mindmap supplies its graph generation.
    pub layout_generation: Option<u64>,
    pub on_toggle: Box<dyn Fn(Id) -> Message + 'a>,
    pub on_select: Box<dyn Fn(Id) -> Message + 'a>,
    pub on_deselect: Message,
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
        let layout_changed = self
            .layout_generation
            .is_some_and(|generation| state.last_layout_generation != Some(generation));
        let panel_just_opened = self.panel_open && !state.last_panel_open;
        let panel_just_closed = !self.panel_open && state.last_panel_open;
        if !sel_changed
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
            || (sel_changed && self.panel_open && !layout_changed);
        state.last_panel_open = self.panel_open;
        if !sel_changed && !layout_changed && self.selected.is_none() {
            return;
        }
        let Some(sel) = self.selected.as_ref() else {
            return;
        };
        let Some((idx, _)) = self
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.id.as_ref() == Some(sel))
        else {
            return;
        };
        let target = self.focus_target_for_node(
            state,
            idx,
            bounds,
            prev_bounds_w,
            panel_just_opened,
            panel_just_closed,
            layout_changed,
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

    fn snap_focus_to_node(&self, state: &mut MindmapState<Id>, idx: usize, bounds: Rectangle) {
        let future_open = !self.panel_open;
        let target =
            self.focus_target_for_node(state, idx, bounds, bounds.width, future_open, false, false);
        state.pan = target;
        state.pan_anim = None;
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
        let panel_span = self.panel_width + PANEL_HANDLE_W;
        let effective_w = if panel_just_opened && (bounds.width - prev_bounds_w).abs() <= 0.5 {
            (bounds.width - panel_span).max(1.0)
        } else if panel_just_closed && (bounds.width - prev_bounds_w).abs() <= 0.5 {
            bounds.width + panel_span
        } else {
            bounds.width
        };

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

    /// Get rendered (animated) position for a node.
    fn pos(&self, state: &MindmapState<Id>, idx: usize) -> (f32, f32) {
        let n = &self.nodes[idx];
        match n.id.as_ref().and_then(|id| state.anim.get(id)) {
            Some(a) => (a.current_x, a.current_y),
            None => (n.x, n.y),
        }
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
        match event {
            iced::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let Some(pos) = cursor.position_in(bounds) else {
                    return None;
                };
                let factor = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => (*y * 0.05).exp(),
                    mouse::ScrollDelta::Pixels { y, .. } => (*y * 0.0025).exp(),
                };
                let old_zoom = state.zoom;
                let new_zoom = (old_zoom * factor).clamp(ZOOM_MIN, ZOOM_MAX);
                // Zoom around cursor: keep world point under cursor fixed.
                let world_x = (pos.x - state.pan.x) / old_zoom;
                let world_y = (pos.y - state.pan.y) / old_zoom;
                state.zoom = new_zoom;
                state.pan.x = pos.x - world_x * new_zoom;
                state.pan.y = pos.y - world_y * new_zoom;
                Some(canvas::Action::request_redraw().and_capture())
            }
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(pos) = cursor.position_in(bounds) else {
                    return None;
                };
                // Hit-test in world space against animated positions.
                let wx = (pos.x - state.pan.x) / state.zoom;
                let wy = (pos.y - state.pan.y) / state.zoom;
                for (i, n) in self.nodes.iter().enumerate() {
                    let (nx, ny) = self.pos(state, i);
                    if wx >= nx && wx <= nx + NODE_W && wy >= ny && wy <= ny + NODE_H {
                        if let Some(id) = n.id.clone() {
                            let msg = if n.children.is_empty() && !n.has_hidden_children {
                                if self.autocenter {
                                    self.snap_focus_to_node(state, i, bounds);
                                }
                                (self.on_select)(id)
                            } else {
                                (self.on_toggle)(id)
                            };
                            return Some(canvas::Action::publish(msg).and_capture());
                        }
                        break;
                    }
                }
                // Begin pan drag (and remember press point so a stationary release
                // can be treated as a click-on-empty deselect).
                state.drag_origin = Some(pos);
                state.drag_pan_origin = state.pan;
                state.drag_moved = false;
                // Cancel any in-flight programmatic pan animation.
                state.pan_anim = None;
                Some(canvas::Action::capture())
            }
            iced::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(origin) = state.drag_origin {
                    let Some(pos) = cursor.position_in(bounds) else {
                        return None;
                    };
                    let dx = pos.x - origin.x;
                    let dy = pos.y - origin.y;
                    if dx.abs() > 3.0 || dy.abs() > 3.0 {
                        state.drag_moved = true;
                    }
                    state.pan.x = state.drag_pan_origin.x + dx;
                    state.pan.y = state.drag_pan_origin.y + dy;
                    return Some(canvas::Action::request_redraw().and_capture());
                }
                // Hover tracking: highlight the node under the cursor so we
                // can show a tooltip for truncated labels in draw().
                let new_hover = cursor.position_in(bounds).and_then(|pos| {
                    let wx = (pos.x - state.pan.x) / state.zoom;
                    let wy = (pos.y - state.pan.y) / state.zoom;
                    self.nodes.iter().enumerate().find_map(|(i, _)| {
                        let (nx, ny) = self.pos(state, i);
                        if wx >= nx && wx <= nx + NODE_W && wy >= ny && wy <= ny + NODE_H {
                            Some(i)
                        } else {
                            None
                        }
                    })
                });
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
                if state.drag_origin.is_some() {
                    let moved = state.drag_moved;
                    state.drag_origin = None;
                    state.drag_moved = false;
                    if !moved && self.selected.is_some() {
                        return Some(
                            canvas::Action::publish(self.on_deselect.clone()).and_capture(),
                        );
                    }
                    return Some(canvas::Action::capture());
                }
                None
            }
            iced::Event::Window(window::Event::RedrawRequested(_)) => {
                self.sync_anim(state);
                self.sync_focus_pan(state, bounds);
                let node_running = self.step_anim(state);
                let pan_running = self.step_pan_anim(state);
                if node_running || pan_running {
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
        frame.fill(&bg_path, self.palette.bg);

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
        let dot_r = (3.0 * z).max(1.0);
        let dot_offset = 10.0 * z;

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

        // One pass over nodes feeding all five paths (was five full passes).
        // Per-path append order is preserved. Dots share the node's cull test:
        // the dot center sits 10·z inside the right edge with radius
        // max(3·z, 1) ≤ 10·z for all z ≥ ZOOM_MIN, so the dot always lies
        // inside the node's AABB and culling it with the node is exact.
        let mut accent_bg_b = path::Builder::new();
        let mut surface_bg_b = path::Builder::new();
        let mut accent_border_b = path::Builder::new();
        let mut subtle_border_b = path::Builder::new();
        let mut hidden_dots_b = path::Builder::new();
        for (i, n) in self.nodes.iter().enumerate() {
            let (nx, ny) = positions[i];
            let sx = proj_x(nx);
            let sy = proj_y(ny);
            if !visible(sx, sy) {
                continue;
            }
            if n.has_hidden_children {
                let cx = proj_x(nx + NODE_W) - dot_offset;
                let cy = proj_y(ny + NODE_H / 2.0);
                hidden_dots_b.circle(Point::new(cx, cy), dot_r);
                hidden_dots_b.close();
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
        let hidden_dots = hidden_dots_b.build();

        // Nodes.
        frame.fill(&surface_backgrounds, surface_fill);
        frame.fill(&accent_backgrounds, accent_fill);
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

        // Text: only draw when the rect is large enough to contain it.
        // Uniform font size keeps the glyph atlas warm across zoom levels.
        let min_visible_height = FONT_SIZE * 1.1;
        if s_h >= min_visible_height {
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
                    size: iced::Pixels(FONT_SIZE),
                    align_x: iced::alignment::Horizontal::Center.into(),
                    align_y: iced::alignment::Vertical::Center.into(),
                    ..Text::default()
                });
            }
        }
        frame.fill(&hidden_dots, accent_fill);

        // Tooltip: show full label only when hovering a truncated node.
        if let Some(idx) = state.hovered_idx {
            if let Some(n) = self.nodes.get(idx) {
                if n.truncated && !n.full_label.is_empty() {
                    let (nx, ny) = self.pos(state, idx);
                    let nsx = proj_x(nx);
                    let nsy = proj_y(ny);
                    let tip_text = n.full_label.clone();
                    let tip_pad_x = 8.0;
                    let tip_pad_y = 5.0;
                    let tip_w = (text_width_at(&tip_text, FONT_SIZE) + tip_pad_x * 2.0)
                        .min(bounds.width - 12.0);
                    let tip_h = FONT_SIZE + tip_pad_y * 2.0;
                    let mut tip_x = nsx + s_w / 2.0 - tip_w / 2.0;
                    let mut tip_y = nsy - tip_h - 6.0;
                    if tip_y < 4.0 {
                        tip_y = nsy + s_h + 6.0;
                    }
                    tip_x = tip_x.clamp(4.0, (bounds.width - tip_w - 4.0).max(4.0));
                    let tip_radius = 6.0_f32.min(tip_h * 0.5);
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
            }
        }

        let _ = self.content_size;
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.drag_origin.is_some() {
            return mouse::Interaction::Grabbing;
        }
        let Some(pos) = cursor.position_in(bounds) else {
            return mouse::Interaction::default();
        };
        let z = if state.initialized { state.zoom } else { 1.0 };
        let wx = (pos.x - state.pan.x) / z;
        let wy = (pos.y - state.pan.y) / z;
        for (i, n) in self.nodes.iter().enumerate() {
            let (nx, ny) = self.pos(state, i);
            if n.id.is_some() && wx >= nx && wx <= nx + NODE_W && wy >= ny && wy <= ny + NODE_H {
                return mouse::Interaction::Pointer;
            }
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
    fn document_tree_keeps_block_ids_and_collapse_behavior() {
        let ast = vec![
            heading(1, 1, "Top"),
            heading(2, 2, "Child"),
            heading(3, 1, "Next"),
        ];
        let collapsed = HashSet::from([BlockId(1)]);
        let nodes = build_tree(&ast, "Document", &collapsed);

        assert_eq!(nodes[0].id, None);
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
            selected: Some(selected),
            panel_open: true,
            panel_width: 360.0,
            autocenter: true,
            layout_generation: Some(generation),
            on_toggle: Box::new(|_| ()),
            on_select: Box::new(|_| ()),
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
}
