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

#[derive(Clone)]
pub struct MNode {
    pub id: Option<BlockId>,
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
fn fit_label_for_node(s: &str) -> (String, bool) {
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

fn layout(nodes: &mut [MNode], idx: usize, y_cursor: &mut f32) -> f32 {
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

#[derive(Debug, Default)]
pub struct MindmapState {
    pub zoom: f32,
    pub pan: Vector,
    pub drag_origin: Option<Point>,
    pub drag_pan_origin: Vector,
    pub drag_moved: bool,
    pub initialized: bool,
    anim: HashMap<BlockId, Anim>,
    pan_anim: Option<PanAnim>,
    last_selected: Option<BlockId>,
    last_bounds_w: f32,
    last_bounds_h: f32,
    last_panel_open: bool,
    hovered_idx: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
struct PanAnim {
    start: Vector,
    target: Vector,
    start_at: Instant,
}

pub struct MindmapProgram<'a, Message> {
    pub nodes: std::sync::Arc<Vec<MNode>>,
    pub content_size: Size,
    pub palette: Palette,
    pub selected: Option<BlockId>,
    pub panel_open: bool,
    pub panel_width: f32,
    pub autocenter: bool,
    pub on_toggle: Box<dyn Fn(BlockId) -> Message + 'a>,
    pub on_select: Box<dyn Fn(BlockId) -> Message + 'a>,
    pub on_deselect: Message,
}

impl<'a, Message> MindmapProgram<'a, Message> {
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
    fn sync_anim(&self, state: &mut MindmapState) {
        let parents = self.parent_map();
        let now = Instant::now();
        // Build a snapshot of pre-existing current positions so we can read
        // parent's current pos when seeding new children.
        let prev_current: HashMap<BlockId, (f32, f32)> = state
            .anim
            .iter()
            .map(|(k, v)| (*k, (v.current_x, v.current_y)))
            .collect();
        let present: HashSet<BlockId> = self.nodes.iter().filter_map(|n| n.id).collect();
        state.anim.retain(|k, _| present.contains(k));

        for (i, n) in self.nodes.iter().enumerate() {
            let Some(id) = n.id else { continue };
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
                        .and_then(|pi| self.nodes[pi].id)
                        .and_then(|pid| prev_current.get(&pid).copied())
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
    fn step_anim(&self, state: &mut MindmapState) -> bool {
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
    fn sync_focus_pan(&self, state: &mut MindmapState, bounds: Rectangle) {
        let prev_bounds_w = state.last_bounds_w;
        let bounds_changed = (bounds.width - state.last_bounds_w).abs() > 0.5
            || (bounds.height - state.last_bounds_h).abs() > 0.5;
        let sel_changed = self.selected != state.last_selected;
        let panel_just_opened = self.panel_open && !state.last_panel_open;
        let panel_just_closed = !self.panel_open && state.last_panel_open;
        if !sel_changed && !bounds_changed && !panel_just_opened && !panel_just_closed {
            state.last_panel_open = self.panel_open;
            return;
        }
        if !self.autocenter {
            // Keep bookkeeping in sync so re-enable doesn't fire stale change.
            state.last_selected = self.selected;
            state.last_bounds_w = bounds.width;
            state.last_bounds_h = bounds.height;
            state.last_panel_open = self.panel_open;
            return;
        }
        state.last_selected = self.selected;
        state.last_bounds_w = bounds.width;
        state.last_bounds_h = bounds.height;
        let snap = panel_just_opened || panel_just_closed || (sel_changed && self.panel_open);
        state.last_panel_open = self.panel_open;
        if !sel_changed && self.selected.is_none() {
            return;
        }
        let Some(sel) = self.selected else { return };
        let Some((idx, _)) = self
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.id == Some(sel))
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

    fn snap_focus_to_node(&self, state: &mut MindmapState, idx: usize, bounds: Rectangle) {
        let future_open = !self.panel_open;
        let target =
            self.focus_target_for_node(state, idx, bounds, bounds.width, future_open, false);
        state.pan = target;
        state.pan_anim = None;
    }

    fn focus_target_for_node(
        &self,
        state: &MindmapState,
        idx: usize,
        bounds: Rectangle,
        prev_bounds_w: f32,
        panel_just_opened: bool,
        panel_just_closed: bool,
    ) -> Vector {
        let panel_span = self.panel_width + PANEL_HANDLE_W;
        let effective_w = if panel_just_opened && (bounds.width - prev_bounds_w).abs() <= 0.5 {
            (bounds.width - panel_span).max(1.0)
        } else if panel_just_closed && (bounds.width - prev_bounds_w).abs() <= 0.5 {
            bounds.width + panel_span
        } else {
            bounds.width
        };

        // Node center in world space (use animated current pos to avoid jumps).
        let (nx, ny) = self.pos(state, idx);
        let world_cx = nx + NODE_W / 2.0;
        let world_cy = ny + NODE_H / 2.0;
        let z = state.zoom;
        Vector::new(
            effective_w / 2.0 - world_cx * z,
            bounds.height / 2.0 - world_cy * z,
        )
    }

    /// Advance pan animation. Returns true if still running.
    fn step_pan_anim(&self, state: &mut MindmapState) -> bool {
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
    fn pos(&self, state: &MindmapState, idx: usize) -> (f32, f32) {
        let n = &self.nodes[idx];
        match n.id.and_then(|id| state.anim.get(&id)) {
            Some(a) => (a.current_x, a.current_y),
            None => (n.x, n.y),
        }
    }
}

impl<'a, Message: Clone> canvas::Program<Message, Theme, Renderer> for MindmapProgram<'a, Message> {
    type State = MindmapState;

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
                        if let Some(id) = n.id {
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

        // Edges in screen space.
        let edges_path = Path::new(|b| {
            for (i, n) in self.nodes.iter().enumerate() {
                let (nx, ny) = positions[i];
                let px = proj_x(nx + NODE_W);
                let py = proj_y(ny + NODE_H / 2.0);
                for &c in &n.children {
                    let (cnx, cny) = positions[c];
                    let cx = proj_x(cnx);
                    let cy = proj_y(cny + NODE_H / 2.0);
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
        // Per-path append order and the dots' lack of culling are preserved so
        // the produced geometry is identical.
        let mut accent_bg_b = path::Builder::new();
        let mut surface_bg_b = path::Builder::new();
        let mut accent_border_b = path::Builder::new();
        let mut subtle_border_b = path::Builder::new();
        let mut hidden_dots_b = path::Builder::new();
        for (i, n) in self.nodes.iter().enumerate() {
            let (nx, ny) = positions[i];
            if n.has_hidden_children {
                let cx = proj_x(nx + NODE_W) - dot_offset;
                let cy = proj_y(ny + NODE_H / 2.0);
                hidden_dots_b.circle(Point::new(cx, cy), dot_r);
                hidden_dots_b.close();
            }
            let sx = proj_x(nx);
            let sy = proj_y(ny);
            if !visible(sx, sy) {
                continue;
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
        if let Some(sel) = self.selected {
            if let Some((i, _)) = self
                .nodes
                .iter()
                .enumerate()
                .find(|(_, n)| n.id == Some(sel))
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
