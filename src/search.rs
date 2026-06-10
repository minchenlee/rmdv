use crate::ast::{Block, BlockId, Inline, ListItem};

/// Case-insensitive substring search. Returns byte offsets **into `haystack`**
/// (the original string), so callers can index `haystack` directly.
///
/// `to_lowercase()` is not length-preserving for some scalars (Turkish `İ`,
/// `ẞ`, ligatures), so a lowercased copy's offsets do not map onto the
/// original. We lowercase once but keep an offset map from each lowercased
/// byte back to the originating char's start in `haystack`, then translate the
/// match positions back. O(n), non-overlapping (matches the previous contract).
pub fn find_all(haystack: &str, needle: &str) -> Vec<usize> {
    if needle.is_empty() {
        return Vec::new();
    }
    let n = needle.to_lowercase();

    // Lowercase the haystack, recording for each resulting byte the original
    // byte offset of the source char that produced it.
    let mut h = String::with_capacity(haystack.len());
    let mut map: Vec<usize> = Vec::with_capacity(haystack.len());
    for (orig_off, ch) in haystack.char_indices() {
        for lc in ch.to_lowercase() {
            let before = h.len();
            h.push(lc);
            for _ in before..h.len() {
                map.push(orig_off);
            }
        }
    }

    let mut out = Vec::new();
    let mut start = 0;
    while let Some(idx) = h[start..].find(&n) {
        let lc_pos = start + idx;
        out.push(map[lc_pos]);
        start = lc_pos + n.len();
    }
    out
}

/// `find_all(haystack, needle).len()` without building the offset map or the
/// match Vec. Takes the needle already lowercased so per-block callers lower
/// it once. Same lowercasing + non-overlapping scan, so counts are identical.
pub fn count_all_lowered(haystack: &str, lowered_needle: &str) -> usize {
    if lowered_needle.is_empty() {
        return 0;
    }
    let mut h = String::with_capacity(haystack.len());
    for ch in haystack.chars() {
        h.extend(ch.to_lowercase());
    }
    let mut count = 0;
    let mut start = 0;
    while let Some(idx) = h[start..].find(lowered_needle) {
        count += 1;
        start = start + idx + lowered_needle.len();
    }
    count
}

#[derive(Debug, Clone, Copy)]
pub struct MatchPos {
    pub block: usize,
    pub in_block: usize,
}

pub fn find_in_blocks(blocks: &[(BlockId, Block)], query: &str) -> Vec<MatchPos> {
    if query.is_empty() {
        return Vec::new();
    }
    let lowered = query.to_lowercase();
    let mut out = Vec::new();
    for (bi, (_id, b)) in blocks.iter().enumerate() {
        let text = block_text(b);
        let n = count_all_lowered(&text, &lowered);
        for k in 0..n {
            out.push(MatchPos {
                block: bi,
                in_block: k,
            });
        }
    }
    out
}

fn block_text(b: &Block) -> String {
    let mut s = String::new();
    push_block_text(b, &mut s);
    s
}

fn push_block_text(b: &Block, out: &mut String) {
    match b {
        Block::Heading { inlines, .. } | Block::Paragraph(inlines) => {
            for i in inlines {
                push_inline_text(i, out);
            }
            out.push('\n');
        }
        Block::CodeBlock { code, .. } => {
            out.push_str(code);
            out.push('\n');
        }
        Block::Blockquote(blocks) => {
            for x in blocks {
                push_block_text(x, out);
            }
        }
        Block::List { items, .. } => {
            for it in items {
                push_list_item(it, out);
            }
        }
        Block::Table { headers, rows } => {
            for cell in headers {
                for i in cell {
                    push_inline_text(i, out);
                }
                out.push(' ');
            }
            out.push('\n');
            for r in rows {
                for cell in r {
                    for i in cell {
                        push_inline_text(i, out);
                    }
                    out.push(' ');
                }
                out.push('\n');
            }
        }
        Block::Image { alt, .. } => {
            out.push_str(alt);
            out.push('\n');
        }
        Block::Diagram { source, .. } => {
            out.push_str(source);
            out.push('\n');
        }
        Block::Rule => {}
    }
}

fn push_list_item(it: &ListItem, out: &mut String) {
    for b in &it.blocks {
        push_block_text(b, out);
    }
}

fn push_inline_text(i: &Inline, out: &mut String) {
    match i {
        Inline::Text(t) | Inline::Code(t) => out.push_str(t),
        Inline::Emph(c) | Inline::Strong(c) | Inline::Strike(c) => {
            for x in c {
                push_inline_text(x, out);
            }
        }
        Inline::Link { children, .. } => {
            for x in children {
                push_inline_text(x, out);
            }
        }
    }
}
