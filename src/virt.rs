//! Virtual-scroll math: cheap height estimates per block, visible-range
//! computation, and a simple cache keyed by BlockId.

use crate::ast::{Block, BlockId, Inline, ListItem};
use std::collections::HashMap;

const LINE_PX: f32 = 24.0;
const HEADING_PX: [f32; 6] = [44.0, 36.0, 30.0, 26.0, 24.0, 22.0];
const BLOCK_GAP_PX: f32 = 14.0;
const CODE_LINE_PX: f32 = 20.0;
const TABLE_ROW_PX: f32 = 28.0;
const PARAGRAPH_CHARS_PER_LINE: f32 = 80.0;

pub fn estimate_height(b: &Block) -> f32 {
    match b {
        Block::Heading { level, .. } => {
            // Headings are single-line in mdv; ignore inline length.
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

#[derive(Default)]
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

    pub fn retain(&mut self, ids: &std::collections::HashSet<BlockId>) {
        self.measured.retain(|k, _| ids.contains(k));
    }

    pub fn clear(&mut self) {
        self.measured.clear();
    }
}

/// Returns inclusive `[start, end)` block index range that intersects the viewport
/// (offset_y = scroll position in px, viewport_h = visible height in px),
/// padded by `pad` blocks on each side.
pub fn visible_range(
    blocks: &[(BlockId, Block)],
    cache: &HeightCache,
    offset_y: f32,
    viewport_h: f32,
    pad: usize,
) -> (usize, usize) {
    let mut y = 0.0;
    let mut start = blocks.len();
    let mut end = blocks.len();
    for (i, (id, b)) in blocks.iter().enumerate() {
        let h = cache.get(*id, b) + BLOCK_GAP_PX;
        if start == blocks.len() && y + h >= offset_y {
            start = i;
        }
        if y > offset_y + viewport_h {
            end = i;
            break;
        }
        y += h;
    }
    let s = start.saturating_sub(pad);
    let e = (end + pad).min(blocks.len());
    (s, e)
}

pub fn estimated_block_position(
    blocks: &[(BlockId, Block)],
    cache: &HeightCache,
    block_idx: usize,
) -> Option<(f32, f32)> {
    let (_, (id, block)) = blocks.iter().enumerate().find(|(i, _)| *i == block_idx)?;
    let top = blocks
        .iter()
        .take(block_idx)
        .map(|(id, block)| cache.get(*id, block) + BLOCK_GAP_PX)
        .sum();
    Some((top, cache.get(*id, block)))
}

pub fn estimated_content_height(blocks: &[(BlockId, Block)], cache: &HeightCache) -> f32 {
    blocks
        .iter()
        .map(|(id, block)| cache.get(*id, block) + BLOCK_GAP_PX)
        .sum::<f32>()
        .max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Block;

    #[test]
    fn empty_doc_has_empty_range() {
        let cache = HeightCache::default();
        let r = visible_range(&[], &cache, 0.0, 800.0, 5);
        assert_eq!(r, (0, 0));
    }

    #[test]
    fn fits_in_viewport_returns_full_range() {
        let blocks = vec![
            (
                BlockId(1),
                Block::Paragraph(vec![Inline::Text("hi".into())]),
            ),
            (
                BlockId(2),
                Block::Paragraph(vec![Inline::Text("ok".into())]),
            ),
        ];
        let cache = HeightCache::default();
        let (s, e) = visible_range(&blocks, &cache, 0.0, 800.0, 0);
        assert_eq!(s, 0);
        assert_eq!(e, blocks.len());
    }

    #[test]
    fn deep_offset_small_viewport_returns_non_empty_range() {
        let mut blocks = Vec::new();
        for i in 0..500 {
            blocks.push((
                BlockId(i),
                Block::Paragraph(vec![Inline::Text("x".repeat(80))]),
            ));
        }
        let cache = HeightCache::default();
        // Offset far into the document, tiny viewport.
        let (s, e) = visible_range(&blocks, &cache, 10_000.0, 50.0, 0);
        assert!(s < blocks.len(), "start must land on a real block");
        assert!(e > s, "end must be after start");
        assert!(e <= blocks.len());
    }

    #[test]
    fn offset_past_document_end_collapses_to_end() {
        let blocks = make_paragraphs(10);
        let cache = HeightCache::default();
        let (s, e) = visible_range(&blocks, &cache, 1_000_000.0, 800.0, 0);
        // Degenerate but should not panic; both indices clamp to len.
        assert_eq!(s, blocks.len());
        assert_eq!(e, blocks.len());
    }

    fn make_paragraphs(n: u64) -> Vec<(BlockId, Block)> {
        (0..n)
            .map(|i| {
                (
                    BlockId(i),
                    Block::Paragraph(vec![Inline::Text("hi".into())]),
                )
            })
            .collect()
    }

    #[test]
    fn skips_blocks_above_viewport() {
        let mut blocks = Vec::new();
        for i in 0..200 {
            blocks.push((
                BlockId(i),
                Block::Paragraph(vec![Inline::Text("x".repeat(80))]),
            ));
        }
        let cache = HeightCache::default();
        let (s, _) = visible_range(&blocks, &cache, 5_000.0, 800.0, 0);
        assert!(s > 0, "should skip blocks above offset");
    }

    #[test]
    fn estimated_position_uses_cumulative_heights() {
        let blocks = make_paragraphs(3);
        let mut cache = HeightCache::default();
        cache.set_measured(BlockId(0), 100.0);
        cache.set_measured(BlockId(1), 200.0);

        let (top, height) = estimated_block_position(&blocks, &cache, 2).unwrap();

        assert_eq!(top, 100.0 + BLOCK_GAP_PX + 200.0 + BLOCK_GAP_PX);
        assert_eq!(height, estimate_height(&blocks[2].1));
    }

    #[test]
    fn estimated_content_height_includes_gaps() {
        let blocks = make_paragraphs(2);
        let mut cache = HeightCache::default();
        cache.set_measured(BlockId(0), 50.0);
        cache.set_measured(BlockId(1), 75.0);

        assert_eq!(
            estimated_content_height(&blocks, &cache),
            50.0 + BLOCK_GAP_PX + 75.0 + BLOCK_GAP_PX
        );
    }
}
