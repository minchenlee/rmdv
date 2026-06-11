use mdv::ast::{Block, BlockId, Inline};
use mdv::virt::{HeightCache, VirtWindow, BLOCK_GAP_PX, VIRT_MIN_BLOCKS};
use std::collections::HashSet;

fn make_blocks(n: usize) -> Vec<(BlockId, Block)> {
    (0..n)
        .map(|i| {
            (
                BlockId(i as u64),
                Block::Paragraph(vec![Inline::Text(format!("block {}", i))]),
            )
        })
        .collect()
}

#[test]
fn window_padding_extends_past_viewport() {
    let blocks = make_blocks(VIRT_MIN_BLOCKS * 4);
    let cache = HeightCache::default();
    let mut w = VirtWindow::default();
    let offset = 8_000.0;
    let vh = 400.0;
    w.rebuild(&blocks, &HashSet::new(), &cache, offset, vh);
    assert!(w.active);
    let (s, e) = w.range;
    assert!(w.prefix[s] < offset, "window must start above the viewport");
    assert!(
        w.prefix[e] > offset + vh,
        "window must end below the viewport"
    );
}

#[test]
fn measured_height_overrides_estimate() {
    let blocks = make_blocks(VIRT_MIN_BLOCKS * 2);
    let mut cache = HeightCache::default();
    cache.set_measured(BlockId(0), 9999.0);
    let mut w = VirtWindow::default();
    w.rebuild(&blocks, &HashSet::new(), &cache, 5_000.0, 400.0);
    // First block alone is 9999px tall, so offset 5000 falls inside block 0
    // and the window must still include it.
    assert_eq!(w.range.0, 0);
    assert_eq!(w.prefix[1], 9999.0 + BLOCK_GAP_PX);
}

#[test]
fn hysteresis_band_avoids_rebuild_for_small_deltas() {
    let blocks = make_blocks(VIRT_MIN_BLOCKS * 4);
    let cache = HeightCache::default();
    let mut w = VirtWindow::default();
    w.rebuild(&blocks, &HashSet::new(), &cache, 8_000.0, 400.0);
    let range = w.range;
    assert!(!w.needs_rebuild(8_050.0), "wheel-sized delta stays in band");
    assert!(w.needs_rebuild(50_000.0), "jump exits the band");
    // No rebuild happened — range untouched.
    assert_eq!(w.range, range);
}

#[test]
fn spacers_preserve_total_scroll_geometry() {
    let blocks = make_blocks(VIRT_MIN_BLOCKS * 4);
    let cache = HeightCache::default();
    let mut w = VirtWindow::default();
    w.rebuild(&blocks, &HashSet::new(), &cache, 8_000.0, 400.0);
    let (s, e) = w.range;
    let top = w.top_spacer().expect("window starts mid-doc");
    let bottom = w.bottom_spacer().expect("window ends mid-doc");
    let windowed = w.prefix[e] - w.prefix[s];
    let assembled = top + BLOCK_GAP_PX + windowed + bottom;
    assert!(
        (assembled - w.total_height()).abs() < 0.5,
        "spacer + window height must equal a full render's height"
    );
}
