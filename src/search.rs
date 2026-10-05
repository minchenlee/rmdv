use crate::ast::{Block, BlockId, Inline, ListItem};

/// Case-insensitive substring search. Returns byte offsets **into `haystack`**
/// (the original string), so callers can index `haystack` directly.
///
/// `to_lowercase()` is not length-preserving for some scalars (Turkish `İ`,
/// `ẞ`, ligatures), so a lowercased copy's offsets do not map onto the
/// original. ASCII text lowercases byte-for-byte and needs no translation;
/// otherwise matches found in the lowercased copy are walked back to the start
/// of the source char that produced them. Peak extra memory is one lowercased
/// copy of `haystack`. O(n), non-overlapping (matches the previous contract).
pub fn find_all(haystack: &str, needle: &str) -> Vec<usize> {
    if needle.is_empty() {
        return Vec::new();
    }
    let n = needle.to_lowercase();

    if haystack.is_ascii() {
        // A non-ASCII needle can never occur in all-ASCII lowercased text.
        if !n.is_ascii() {
            return Vec::new();
        }
        return match_starts(&haystack.to_ascii_lowercase(), &n);
    }

    let mut h = String::with_capacity(haystack.len());
    for ch in haystack.chars() {
        h.extend(ch.to_lowercase());
    }
    let lowered = match_starts(&h, &n);
    if lowered.is_empty() {
        return lowered;
    }
    drop(h);

    // Translate the ascending lowercased positions in one pass: each belongs to
    // the source char whose lowercased bytes cover it.
    let mut out = Vec::with_capacity(lowered.len());
    let mut pending = lowered.into_iter().peekable();
    let mut lc_end = 0;
    for (orig_off, ch) in haystack.char_indices() {
        lc_end += ch.to_lowercase().map(char::len_utf8).sum::<usize>();
        while pending.next_if(|&pos| pos < lc_end).is_some() {
            out.push(orig_off);
        }
        if pending.peek().is_none() {
            break;
        }
    }
    out
}

/// Non-overlapping start offsets of `needle` in `haystack`.
fn match_starts(haystack: &str, needle: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut start = 0;
    while let Some(idx) = haystack[start..].find(needle) {
        let pos = start + idx;
        out.push(pos);
        start = pos + needle.len();
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
