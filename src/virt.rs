//! Virtual-scroll machinery: cheap per-block height estimates, a measured-height
//! cache, the fold-aware display list, and the hysteresis window that decides
//! which blocks are actually rendered.
//!
//! Geometry contract shared with `render::render`: the body column lays out the
//! display-list blocks with `BLOCK_GAP_PX` spacing, so the top of display entry
//! `k` sits at `prefix[k]` px from the top of the body column. Spacer heights in
//! `VirtWindow::top_spacer`/`bottom_spacer` are derived from the same prefix
//! sums, which keeps scrollbar geometry stable while only a window of blocks is
//! materialized.

use crate::ast::{Block, BlockId, Inline, ListItem};
use std::collections::{HashMap, HashSet};

const LINE_PX: f32 = 24.0;
const HEADING_PX: [f32; 6] = [44.0, 36.0, 30.0, 26.0, 24.0, 22.0];
pub const BLOCK_GAP_PX: f32 = 14.0;
const CODE_LINE_PX: f32 = 20.0;
const TABLE_ROW_PX: f32 = 28.0;
const PARAGRAPH_CHARS_PER_LINE: f32 = 80.0;

/// Docs with at most this many fold-visible blocks render in full; above it
/// the window + spacers path engages. Keeps small/typical documents on the
/// zero-risk path while bounding widget-tree size for big ones.
pub const VIRT_MIN_BLOCKS: usize = 256;

pub fn estimate_height(b: &Block) -> f32 {
    match b {
        Block::Heading { level, .. } => {
            // Headings are single-line in rmdv; ignore inline length.
            HEADING_PX[((*level as usize).saturating_sub(1)).min(5)]
        }
        Block::Paragraph(inlines) => paragraph_lines(inlines) * LINE_PX,
        Block::CodeBlock { code, .. } => (code.lines().count().max(1) as f32) * CODE_LINE_PX + 16.0,
        Block::Image { .. } => 240.0,
        Block::Diagram { .. } => 200.0,
        Block::Blockquote(blocks) => blocks.iter().map(estimate_height).sum::<f32>() + BLOCK_GAP_PX,
        Block::List { items, .. } => items.iter().map(estimate_item).sum::<f32>(),
        Block::Table { headers: _, rows } => (rows.len() as f32 + 1.0) * TABLE_ROW_PX,
        Block::Rule => 12.0,
    }
}

fn estimate_item(it: &ListItem) -> f32 {
    let inner: f32 = it.blocks.iter().map(estimate_height).sum();
    inner.max(LINE_PX)
}

fn paragraph_lines(inlines: &[Inline]) -> f32 {
    let chars: f32 = inlines.iter().map(inline_chars).sum();
    (chars / PARAGRAPH_CHARS_PER_LINE).ceil().max(1.0)
}

fn inline_chars(i: &Inline) -> f32 {
    match i {
        Inline::Text(s) | Inline::Code(s) => s.chars().count() as f32,
        Inline::Emph(c) | Inline::Strong(c) | Inline::Strike(c) => c.iter().map(inline_chars).sum(),
        Inline::Link { children, .. } => children.iter().map(inline_chars).sum(),
    }
}

#[derive(Default, Clone)]
pub struct HeightCache {
    measured: HashMap<BlockId, f32>,
}

impl HeightCache {
    pub fn get(&self, id: BlockId, b: &Block) -> f32 {
        *self.measured.get(&id).unwrap_or(&estimate_height(b))
    }

    pub fn set_measured(&mut self, id: BlockId, h: f32) {
        self.measured.insert(id, h);
    }

    pub fn retain(&mut self, ids: &HashSet<BlockId>) {
        self.measured.retain(|k, _| ids.contains(k));
    }

    pub fn clear(&mut self) {
        self.measured.clear();
    }
}

/// Indices into `blocks` of every block that is actually rendered given the
/// current fold state. Mirrors the historical fold-skip loop in
/// `render::render`: a folded heading stays visible (it carries the chevron)
/// but hides everything after it until a heading of level <= its own.
pub fn display_list(blocks: &[(BlockId, Block)], folded: &HashSet<BlockId>) -> Vec<usize> {
    let mut out = Vec::with_capacity(blocks.len());
    let mut fold_until: Option<u8> = None;
    for (i, (id, b)) in blocks.iter().enumerate() {
        if let Block::Heading { level, .. } = b {
            let lvl = *level as u8;
            if let Some(thresh) = fold_until {
                if lvl > thresh {
                    continue;
                }
                fold_until = None;
            }
            out.push(i);
            if folded.contains(id) {
                fold_until = Some(lvl);
            }
            continue;
        }
        if fold_until.is_some() {
            continue;
        }
        out.push(i);
    }
    out
}

/// `prefix[k]` = y of the top of display entry `k` relative to the body column
/// top; `prefix[display.len()]` = total tracked height (including one trailing
/// gap, see `VirtWindow::total_height`).
fn prefix_sums(blocks: &[(BlockId, Block)], display: &[usize], cache: &HeightCache) -> Vec<f32> {
    let mut prefix = Vec::with_capacity(display.len() + 1);
    let mut y = 0.0f32;
    prefix.push(0.0);
    for &i in display {
        let (id, b) = &blocks[i];
        y += cache.get(*id, b) + BLOCK_GAP_PX;
        prefix.push(y);
    }
    prefix
}

/// The state behind windowed rendering. Rebuilt in `App::update` (never in
/// `view`) whenever the document, folds, heights, or scroll band change;
/// `render::render` only reads it.
#[derive(Default, Clone)]
pub struct VirtWindow {
    /// Fold-visible block indices into the AST.
    pub display: Vec<usize>,
    /// Cumulative y positions over `display`; len = display.len() + 1.
    pub prefix: Vec<f32>,
    /// Rendered slice of `display`: `[range.0, range.1)`.
    pub range: (usize, usize),
    /// Scroll band (body-relative px) within which `range` stays valid.
    /// Offsets outside it trigger a rebuild (hysteresis: the rendered window
    /// is padded well past the band so fast scrolling never outruns content).
    pub band: (f32, f32),
    /// False = doc small enough to render fully (range covers all of display).
    pub active: bool,
}

impl VirtWindow {
    /// Recompute display list, prefix sums, and the rendered window around
    /// `offset_y` (body-relative px of the viewport top).
    pub fn rebuild(
        &mut self,
        blocks: &[(BlockId, Block)],
        folded: &HashSet<BlockId>,
        cache: &HeightCache,
        offset_y: f32,
        viewport_h: f32,
    ) {
        self.display = display_list(blocks, folded);
        self.prefix = prefix_sums(blocks, &self.display, cache);
        self.rebuild_at_current_shape(offset_y, viewport_h);
    }

    /// True when `offset_y` (body-relative px) left the band the current
    /// window was built for.
    pub fn needs_rebuild(&self, offset_y: f32) -> bool {
        self.active && (offset_y < self.band.0 || offset_y > self.band.1)
    }

    /// Position of AST block `ast_idx` in the display list (None if hidden
    /// under a fold). `display` is ascending, so binary search.
    pub fn display_pos(&self, ast_idx: usize) -> Option<usize> {
        self.display.binary_search(&ast_idx).ok()
    }

    /// Estimated y of the top of display entry `dpos`, body-relative.
    pub fn block_top(&self, dpos: usize) -> f32 {
        self.prefix.get(dpos).copied().unwrap_or(0.0)
    }

    /// Estimated height of display entry `dpos` (without trailing gap).
    pub fn block_height(&self, dpos: usize) -> f32 {
        match (self.prefix.get(dpos), self.prefix.get(dpos + 1)) {
            (Some(a), Some(b)) => (b - a - BLOCK_GAP_PX).max(0.0),
            _ => 0.0,
        }
    }

    /// Estimated total body-column height (matches a full render's height:
    /// every block plus a gap between consecutive ones, no trailing gap).
    pub fn total_height(&self) -> f32 {
        (self.prefix.last().copied().unwrap_or(0.0) - BLOCK_GAP_PX).max(0.0)
    }

    /// Height of the spacer standing in for blocks above the window. None when
    /// the window starts at the top. Sized so that with the column's own gap
    /// after the spacer, the first rendered block lands exactly at
    /// `prefix[range.0]`.
    pub fn top_spacer(&self) -> Option<f32> {
        if !self.active || self.range.0 == 0 {
            return None;
        }
        Some((self.prefix[self.range.0] - BLOCK_GAP_PX).max(0.0))
    }

    /// Height of the spacer standing in for blocks below the window. None when
    /// the window reaches the end.
    pub fn bottom_spacer(&self) -> Option<f32> {
        if !self.active || self.range.1 >= self.display.len() {
            return None;
        }
        let tail = self.prefix[self.display.len()] - self.prefix[self.range.1];
        Some((tail - BLOCK_GAP_PX).max(0.0))
    }

    /// Rebuild the window centered on AST block `ast_idx` (used by goto/search
    /// jumps so the target is materialized before the precise scroll op runs).
    /// Folds may have changed (callers unfold targets first), so the display
    /// list is refreshed before locating the target.
    pub fn rebuild_around(
        &mut self,
        blocks: &[(BlockId, Block)],
        folded: &HashSet<BlockId>,
        cache: &HeightCache,
        ast_idx: usize,
        viewport_h: f32,
    ) {
        self.display = display_list(blocks, folded);
        self.prefix = prefix_sums(blocks, &self.display, cache);
        let dpos = self.display.binary_search(&ast_idx).unwrap_or_else(|i| i);
        let offset = self.block_top(dpos) - viewport_h.max(400.0) * 0.38;
        self.rebuild_at_current_shape(offset.max(0.0), viewport_h);
    }

    /// Re-window over the already-computed display/prefix.
    fn rebuild_at_current_shape(&mut self, offset_y: f32, viewport_h: f32) {
        self.active = self.display.len() > VIRT_MIN_BLOCKS;
        if !self.active {
            self.range = (0, self.display.len());
            self.band = (f32::NEG_INFINITY, f32::INFINITY);
            return;
        }
        let vh = viewport_h.max(400.0);
        let pad = pad_px(vh);
        let offset_y = offset_y.clamp(0.0, self.total_height());
        let lo = offset_y - pad;
        let hi = offset_y + vh + pad;
        let start = self.prefix[1..].partition_point(|&bottom| bottom < lo);
        let end = self.prefix[..self.display.len()].partition_point(|&top| top <= hi);
        self.range = (
            start.min(self.display.len()),
            end.max(start).min(self.display.len()),
        );
        self.band = (offset_y - pad * 0.5, offset_y + pad * 0.5);
    }

    /// Display entry whose span contains `offset_y` (body-relative px).
    pub fn display_pos_at(&self, offset_y: f32) -> Option<usize> {
        if self.display.is_empty() {
            return None;
        }
        let k = self.prefix[1..].partition_point(|&bottom| bottom <= offset_y);
        Some(k.min(self.display.len() - 1))
    }
}

fn pad_px(viewport_h: f32) -> f32 {
    (viewport_h * 1.5).max(1200.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Block;

    fn para(chars: usize) -> Block {
        Block::Paragraph(vec![Inline::Text("x".repeat(chars))])
    }

    fn make_paragraphs(n: u64) -> Vec<(BlockId, Block)> {
        (0..n).map(|i| (BlockId(i), para(80))).collect()
    }

    fn heading(level: u8) -> Block {
        Block::Heading {
            level,
            id: String::new(),
            inlines: vec![Inline::Text("h".into())],
        }
    }

    #[test]
    fn display_list_without_folds_is_identity() {
        let blocks = make_paragraphs(5);
        let d = display_list(&blocks, &HashSet::new());
        assert_eq!(d, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn display_list_hides_folded_section_but_keeps_heading() {
        // h1, p, h2, p, h1, p — folding the h2 hides only its paragraph.
        let blocks: Vec<(BlockId, Block)> = vec![
            (BlockId(0), heading(1)),
            (BlockId(1), para(10)),
            (BlockId(2), heading(2)),
            (BlockId(3), para(10)),
            (BlockId(4), heading(1)),
            (BlockId(5), para(10)),
        ];
        let mut folded = HashSet::new();
        folded.insert(BlockId(2));
        let d = display_list(&blocks, &folded);
        assert_eq!(d, vec![0, 1, 2, 4, 5]);
    }

    #[test]
    fn display_list_nested_folds() {
        // Folding an h1 hides nested h2 + content until the next h1.
        let blocks: Vec<(BlockId, Block)> = vec![
            (BlockId(0), heading(1)),
            (BlockId(1), para(10)),
            (BlockId(2), heading(2)),
            (BlockId(3), para(10)),
            (BlockId(4), heading(1)),
        ];
        let mut folded = HashSet::new();
        folded.insert(BlockId(0));
        let d = display_list(&blocks, &folded);
        assert_eq!(d, vec![0, 4]);
    }

    #[test]
    fn small_doc_is_inactive_and_renders_fully() {
        let blocks = make_paragraphs(10);
        let mut w = VirtWindow::default();
        w.rebuild(
            &blocks,
            &HashSet::new(),
            &HeightCache::default(),
            0.0,
            800.0,
        );
        assert!(!w.active);
        assert_eq!(w.range, (0, 10));
        assert_eq!(w.top_spacer(), None);
        assert_eq!(w.bottom_spacer(), None);
        assert!(!w.needs_rebuild(1_000_000.0));
    }

    #[test]
    fn big_doc_windows_and_spacers_cover_the_rest() {
        let blocks = make_paragraphs(1000);
        let mut w = VirtWindow::default();
        let cache = HeightCache::default();
        w.rebuild(&blocks, &HashSet::new(), &cache, 20_000.0, 800.0);
        assert!(w.active);
        let (s, e) = w.range;
        assert!(s > 0, "window must not start at the top");
        assert!(e < 1000, "window must not reach the end");
        assert!(e > s);
        // Spacer + gap accounting: first rendered block top == prefix[s].
        let top = w.top_spacer().unwrap();
        assert!((top + BLOCK_GAP_PX - w.prefix[s]).abs() < 0.01);
        // Total height = spacers + windowed blocks + gaps.
        let windowed: f32 = w.prefix[e] - w.prefix[s];
        let bottom = w.bottom_spacer().unwrap();
        let assembled = top + BLOCK_GAP_PX + windowed + bottom;
        assert!((assembled - (w.prefix[1000] - BLOCK_GAP_PX)).abs() < 0.5);
    }

    #[test]
    fn window_covers_viewport_plus_padding() {
        let blocks = make_paragraphs(1000);
        let mut w = VirtWindow::default();
        let cache = HeightCache::default();
        let offset = 20_000.0;
        let vh = 800.0;
        w.rebuild(&blocks, &HashSet::new(), &cache, offset, vh);
        let (s, e) = w.range;
        assert!(w.prefix[s] <= offset, "window starts above the viewport");
        assert!(w.prefix[e] >= offset + vh, "window ends below the viewport");
        // Hysteresis: stays valid within the band, rebuilds outside it.
        assert!(!w.needs_rebuild(offset));
        assert!(!w.needs_rebuild(offset + 100.0));
        assert!(w.needs_rebuild(offset + 100_000.0));
        assert!(w.needs_rebuild(0.0));
    }

    #[test]
    fn offset_past_end_clamps() {
        let blocks = make_paragraphs(1000);
        let mut w = VirtWindow::default();
        w.rebuild(
            &blocks,
            &HashSet::new(),
            &HeightCache::default(),
            10_000_000.0,
            800.0,
        );
        let (s, e) = w.range;
        assert!(s < e, "clamped window must still render the tail");
        assert_eq!(e, 1000);
    }

    #[test]
    fn measured_heights_shift_prefix() {
        let blocks = make_paragraphs(300);
        let mut cache = HeightCache::default();
        cache.set_measured(BlockId(0), 500.0);
        let mut w = VirtWindow::default();
        w.rebuild(&blocks, &HashSet::new(), &cache, 0.0, 800.0);
        assert_eq!(w.prefix[1], 500.0 + BLOCK_GAP_PX);
    }

    #[test]
    fn rebuild_around_materializes_target() {
        let blocks = make_paragraphs(2000);
        let mut w = VirtWindow::default();
        let cache = HeightCache::default();
        let folded = HashSet::new();
        w.rebuild(&blocks, &folded, &cache, 0.0, 800.0);
        w.rebuild_around(&blocks, &folded, &cache, 1500, 800.0);
        let dpos = w.display_pos(1500).unwrap();
        let (s, e) = w.range;
        assert!(s <= dpos && dpos < e, "target must be inside the window");
    }

    #[test]
    fn display_pos_at_maps_offsets_to_entries() {
        let blocks = make_paragraphs(100);
        let mut w = VirtWindow::default();
        let cache = HeightCache::default();
        w.rebuild(&blocks, &HashSet::new(), &cache, 0.0, 800.0);
        assert_eq!(w.display_pos_at(0.0), Some(0));
        let top_of_5 = w.prefix[5];
        assert_eq!(w.display_pos_at(top_of_5 + 1.0), Some(5));
        assert_eq!(w.display_pos_at(1e9), Some(99));
    }

    #[test]
    fn estimated_heights_per_block_kind() {
        let h = estimate_height(&heading(1));
        assert_eq!(h, HEADING_PX[0]);
        assert_eq!(estimate_height(&para(160)), 2.0 * LINE_PX);
        assert_eq!(estimate_height(&Block::Rule), 12.0);
    }
}
