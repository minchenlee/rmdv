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

/// In-document search results, stored as one entry per matching block rather
/// than one per hit: a one-letter query on a large document can hit hundreds
/// of thousands of times. `get(i)` yields the same `MatchPos` sequence the
/// expanded list would, in document order.
#[derive(Debug, Clone, Default)]
pub struct Matches {
    blocks: Vec<usize>,
    /// Running hit total through each entry of `blocks`.
    ends: Vec<usize>,
}

impl Matches {
    pub fn len(&self) -> usize {
        self.ends.last().copied().unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.ends.is_empty()
    }

    pub fn clear(&mut self) {
        self.blocks.clear();
        self.ends.clear();
    }

    pub fn get(&self, index: usize) -> Option<MatchPos> {
        if index >= self.len() {
            return None;
        }
        let entry = self.ends.partition_point(|&end| end <= index);
        let start = entry.checked_sub(1).map_or(0, |prev| self.ends[prev]);
        Some(MatchPos {
            block: self.blocks[entry],
            in_block: index - start,
        })
    }
}

pub fn find_in_blocks(blocks: &[(BlockId, Block)], query: &str) -> Matches {
    let mut out = Matches::default();
    if query.is_empty() {
        return out;
    }
    let lowered = query.to_lowercase();
    // Reused across blocks so a search allocates a handful of times, not
    // twice per block.
    let mut text = String::new();
    let mut lower = String::new();
    let mut total = 0;
    for (bi, (_id, b)) in blocks.iter().enumerate() {
        text.clear();
        push_block_text(b, &mut text);
        let n = count_lowered_into(&text, &lowered, &mut lower);
        if n > 0 {
            total += n;
            out.blocks.push(bi);
            out.ends.push(total);
        }
    }
    out
}

/// [`count_all_lowered`] that lowercases into a caller-owned buffer. ASCII
/// text lowercases byte for byte, and can never contain a non-ASCII needle.
fn count_lowered_into(haystack: &str, lowered_needle: &str, scratch: &mut String) -> usize {
    scratch.clear();
    if haystack.is_ascii() {
        if !lowered_needle.is_ascii() {
            return 0;
        }
        scratch.push_str(haystack);
        scratch.make_ascii_lowercase();
    } else {
        for ch in haystack.chars() {
            scratch.extend(ch.to_lowercase());
        }
    }
    let mut count = 0;
    let mut start = 0;
    while let Some(idx) = scratch[start..].find(lowered_needle) {
        count += 1;
        start = start + idx + lowered_needle.len();
    }
    count
}

#[cfg(test)]
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The expanded per-hit list `find_in_blocks` used to return.
    fn expanded(blocks: &[(BlockId, Block)], query: &str) -> Vec<(usize, usize)> {
        let lowered = query.to_lowercase();
        let mut out = Vec::new();
        for (bi, (_id, b)) in blocks.iter().enumerate() {
            for k in 0..count_all_lowered(&block_text(b), &lowered) {
                out.push((bi, k));
            }
        }
        out
    }

    #[test]
    fn compact_matches_equal_the_expanded_list() {
        let src = "# The Engine\n\nthe THE tHe\n\nno hit here\n\n\
                   - İstanbul ıi straße STRASSE\n- ﬁle and FILE\n\n\
                   ```\nlet the = 1;\n```\n\n| the | x |\n| - | - |\n| a | the |\n";
        let (ast, _) = crate::parser::parse(src);
        for query in [
            "the", "e", "i", "ss", "ß", "İ", "ﬁ", "zzz", "THE", "the the",
        ] {
            let want = expanded(&ast, query);
            let got = find_in_blocks(&ast, query);
            assert_eq!(got.len(), want.len(), "len for {query:?}");
            assert_eq!(got.is_empty(), want.is_empty(), "is_empty for {query:?}");
            let got: Vec<_> = (0..got.len())
                .map(|i| got.get(i).map(|m| (m.block, m.in_block)).unwrap())
                .collect();
            assert_eq!(got, want, "positions for {query:?}");
        }
    }

    #[test]
    fn matches_get_past_the_end_and_clear() {
        let (ast, _) = crate::parser::parse("a a\n\nb\n\na\n");
        let mut m = find_in_blocks(&ast, "a");
        assert_eq!(m.len(), 3);
        assert!(m.get(3).is_none());
        m.clear();
        assert!(m.is_empty());
        assert_eq!(m.len(), 0);
        assert!(m.get(0).is_none());
        assert!(find_in_blocks(&ast, "").is_empty());
    }
}
