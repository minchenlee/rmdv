use crate::ast::{Block, BlockId, DiagramKind, Inline, ListItem};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::collections::VecDeque;
use std::hash::{DefaultHasher, Hash, Hasher};

pub fn parse(src: &str) -> (Vec<(BlockId, Block)>, Vec<u32>) {
    let src = strip_frontmatter(src);
    // CommonMark's flanking rules refuse to close `*`/`**` when the closing
    // delimiter sits between CJK punctuation (e.g. `）`) and a CJK letter
    // (e.g. `的`) with no surrounding spaces — common in Chinese/Japanese/Korean
    // prose, which doesn't use spaces. Insert a synthetic non-punctuation
    // marker before such a delimiter so the parser closes the emphasis.
    // `inserts` records the
    // byte positions (in the rewritten string) so block offsets can be mapped
    // back to coordinates in the original `src`. See issue #6.
    let (cooked, inserts, synthetic_marker) = preprocess_cjk_emphasis(src);
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_SMART_PUNCTUATION);
    opts.insert(Options::ENABLE_MATH);
    let parser = Parser::new_ext(&cooked, opts).into_offset_iter();
    let mut state = ParseState {
        synthetic_marker,
        ..ParseState::default()
    };
    let mut pending_offset: Option<u32> = None;
    for (ev, range) in parser {
        if matches!(ev, Event::Start(_)) && state.stack.is_empty() {
            pending_offset = Some(range.start as u32);
        }
        let before_len = state.blocks.len();
        let take_offset = pending_offset;
        state.handle(ev, range.start as u32);
        if state.blocks.len() > before_len {
            for _ in before_len..state.blocks.len() {
                let off = state
                    .emitted_offsets
                    .pop_front()
                    .unwrap_or_else(|| take_offset.or(Some(range.start as u32)).unwrap_or(0));
                state.offsets.push(off);
            }
            pending_offset = None;
        }
    }
    // Map block offsets from rewritten coordinates back onto the original `src`.
    if !inserts.is_empty() {
        for off in &mut state.offsets {
            *off = map_offset_back(*off, &inserts);
        }
    }
    let blocks: Vec<(BlockId, Block)> = state
        .blocks
        .into_iter()
        .enumerate()
        .map(|(pos, b)| (block_id(pos, &b), b))
        .collect();
    (blocks, state.offsets)
}

/// Width of the BMP private-use marker in UTF-8.
const SYNTHETIC_MARKER_LEN: u32 = 3;
const PUA_START: u32 = 0xE000;
const PUA_END: u32 = 0xF8FF;
const PUA_COUNT: usize = (PUA_END - PUA_START + 1) as usize;
const PUA_WORDS: usize = PUA_COUNT / u64::BITS as usize;
#[cfg(test)]
const ZWSP: char = '\u{200b}';

/// Translate a byte offset in the marker-injected string back to the
/// equivalent offset in the original source: subtract `SYNTHETIC_MARKER_LEN`
/// for every
/// insertion that occurred strictly before `off`. `inserts` is sorted ascending.
fn map_offset_back(off: u32, inserts: &[u32]) -> u32 {
    let before = inserts.partition_point(|&p| p < off);
    off - before as u32 * SYNTHETIC_MARKER_LEN
}

/// Returns true for punctuation/symbol characters that, in CJK typography,
/// commonly sit immediately before a closing `*`/`**` with no separating space.
/// Deliberately narrow: only East-Asian-width punctuation in the CJK Symbols,
/// Vertical Forms, CJK-compatibility, and Fullwidth/Halfwidth-forms ranges.
/// ASCII punctuation is intentionally excluded so CommonMark semantics for
/// English text (e.g. `**C++**var` staying literal) are unchanged.
fn is_cjk_punct(c: char) -> bool {
    matches!(c,
        // CJK Symbols and Punctuation 。、「」『』《》【】…—— etc. — start at
        // U+3001 to skip U+3000 (ideographic space), which is whitespace and
        // already flanks correctly without our help.
        '\u{3001}'..='\u{303F}'
        | '\u{2018}'..='\u{201F}' // curly quotes ‘’ “” (used as CJK quotes)
        | '\u{2025}'..='\u{2027}' // ‥ … ‧
        | '\u{30FB}'              // ・ katakana middle dot
        | '\u{FE10}'..='\u{FE19}' // vertical forms
        | '\u{FE30}'..='\u{FE4F}' // CJK compatibility forms
        | '\u{FF01}'..='\u{FF60}' // fullwidth forms （）！？，：；～ etc.
        | '\u{FFE0}'..='\u{FFE6}' // fullwidth signs
    )
}

struct MarkerOccupancy {
    words: [u64; PUA_WORDS],
}

impl Default for MarkerOccupancy {
    fn default() -> Self {
        Self {
            words: [0; PUA_WORDS],
        }
    }
}

impl MarkerOccupancy {
    /// Build a fixed-size occupancy map in one forward pass over the source.
    /// Numeric references are decoded with pulldown-cmark's limits (six hex or
    /// seven decimal digits) because they become authored characters in events.
    fn from_source(src: &str) -> Self {
        let mut occupied = Self::default();
        let bytes = src.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'&' {
                if let Some((codepoint, consumed)) = scan_numeric_reference(&bytes[i..]) {
                    occupied.mark_codepoint(codepoint);
                    i += consumed;
                    continue;
                }
            }

            let c = src[i..]
                .chars()
                .next()
                .expect("i always points at a UTF-8 character boundary");
            occupied.mark_codepoint(c as u32);
            i += c.len_utf8();
        }
        occupied
    }

    fn mark_codepoint(&mut self, codepoint: u32) {
        if !(PUA_START..=PUA_END).contains(&codepoint) {
            return;
        }
        let index = (codepoint - PUA_START) as usize;
        self.words[index / u64::BITS as usize] |= 1 << (index % u64::BITS as usize);
    }

    fn first_free(&self) -> Option<char> {
        self.words
            .iter()
            .enumerate()
            .find_map(|(word_index, &word)| {
                if word == u64::MAX {
                    return None;
                }
                let bit = (!word).trailing_zeros() as usize;
                let index = word_index * u64::BITS as usize + bit;
                (index < PUA_COUNT).then(|| {
                    char::from_u32(PUA_START + index as u32)
                        .expect("BMP private-use values are valid scalars")
                })
            })
    }
}

/// Match pulldown-cmark's numeric character reference grammar closely enough
/// to reserve any BMP private-use scalar it will decode into an event payload.
fn scan_numeric_reference(bytes: &[u8]) -> Option<(u32, usize)> {
    if bytes.get(0..2) != Some(b"&#") {
        return None;
    }

    let mut i = 2;
    let (radix, limit): (u32, usize) = if bytes.get(i).is_some_and(|&b| b | 0x20 == b'x') {
        i += 1;
        (16, 6)
    } else {
        (10, 7)
    };
    let start = i;
    let mut value = 0u32;
    while i - start < limit {
        let Some(&b) = bytes.get(i) else {
            break;
        };
        let digit = match b {
            b'0'..=b'9' => u32::from(b - b'0'),
            b'a'..=b'f' if radix == 16 => u32::from(b - b'a' + 10),
            b'A'..=b'F' if radix == 16 => u32::from(b - b'A' + 10),
            _ => break,
        };
        value = value * radix + digit;
        i += 1;
    }

    (i > start && bytes.get(i) == Some(&b';')).then_some((value, i + 1))
}

/// Insert a synthetic marker where a run of `*` delimiters sits flush against a
/// CJK punctuation character, so CommonMark's flanking rules will open/close
/// the emphasis. Two symmetric placements (see issue #6):
///
/// * **closer:** `CJK-punct` immediately followed by `*…` → marker *before* the
///   run
/// * **opener:** `*…` immediately followed by `CJK-punct` → marker *after* the
///   run
///
/// Fenced code blocks and inline code spans are emitted verbatim so code is
/// never polluted. The marker is selected from the BMP private-use range and is
/// guaranteed not to occur in the source, so cleanup cannot remove authored
/// content such as U+200B. Returns the rewritten text, sorted marker byte
/// positions, and the selected marker. When nothing matches, the input is
/// returned borrowed with an empty insertion list and no marker — the common
/// ASCII / no-`*` case pays only a single scan and no allocation.
fn preprocess_cjk_emphasis(src: &str) -> (std::borrow::Cow<'_, str>, Vec<u32>, Option<char>) {
    // Any rewrite needs both a `*` and a non-ASCII byte somewhere; bail cheaply
    // otherwise (covers virtually all English documents).
    let bytes = src.as_bytes();
    if !bytes.contains(&b'*') || bytes.iter().all(|b| b.is_ascii()) {
        return (std::borrow::Cow::Borrowed(src), Vec::new(), None);
    }

    // Every BMP private-use scalar is three UTF-8 bytes, keeping offset mapping
    // constant. Literal and entity-authored scalars are reserved in one source
    // pass. If the range is full, preserve content and skip the rewrite.
    let Some(synthetic_marker) = MarkerOccupancy::from_source(src).first_free() else {
        return (std::borrow::Cow::Borrowed(src), Vec::new(), None);
    };

    let mut out = String::with_capacity(src.len() + 8);
    let mut inserts: Vec<u32> = Vec::new();
    let mut fence: Option<(char, usize)> = None; // open fence: (marker char, run length)

    for line in src.split_inclusive('\n') {
        // A fenced-code delimiter line is an optionally-indented run of `` ` ``
        // or `~`. An opening fence needs ≥3 chars; a closing fence must match
        // the opener's char and be at least as long. Only a line that actually
        // toggles fence state is emitted verbatim as a delimiter — otherwise a
        // short `` `…` `` / `~…~` line (inline code, strikethrough) would be
        // wrongly skipped.
        let toggles = match (fence, fence_delimiter(line)) {
            (None, Some((_, len))) => len >= 3,
            (Some((fc, flen)), Some((marker, len))) => marker == fc && len >= flen,
            _ => false,
        };
        if toggles {
            fence = match fence {
                None => fence_delimiter(line),
                Some(_) => None,
            };
            out.push_str(line);
            continue;
        }
        if fence.is_some() {
            out.push_str(line);
            continue;
        }
        rewrite_line(line, &mut out, &mut inserts, synthetic_marker);
    }

    if inserts.is_empty() {
        (std::borrow::Cow::Borrowed(src), Vec::new(), None)
    } else {
        (
            std::borrow::Cow::Owned(out),
            inserts,
            Some(synthetic_marker),
        )
    }
}

/// If `line` is a fenced-code delimiter (≤3 leading spaces then a run of ≥1
/// identical `` ` `` or `~`, then only whitespace), return `(marker, run_len)`.
fn fence_delimiter(line: &str) -> Option<(char, usize)> {
    let body = line.trim_end_matches(['\r', '\n']);
    let indent = body.len() - body.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &body[indent..];
    let marker = rest.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }
    let run = rest.chars().take_while(|&c| c == marker).count();
    // Backtick info strings may not contain backticks; for our purposes any
    // trailing text after the run is fine for `~`, and for `` ` `` the run is a
    // valid delimiter as long as the rest has no backtick.
    let after = &rest[run..];
    if marker == '`' && after.contains('`') {
        return None;
    }
    Some((marker, run))
}

/// Rewrite one line (known to be outside a fenced block), appending to `out` and
/// recording marker insertion offsets (into `out`) in `inserts`. Inline code
/// spans (matched backtick runs) are passed through untouched.
fn rewrite_line(line: &str, out: &mut String, inserts: &mut Vec<u32>, synthetic_marker: char) {
    let mut chars = line.char_indices().peekable();
    let mut prev: Option<char> = None;
    let mut inline_code_run: usize = 0; // backtick run length holding an open span; 0 = outside code

    while let Some((_i, c)) = chars.next() {
        if c == '`' {
            let mut run = 1usize;
            while matches!(chars.peek(), Some(&(_, '`'))) {
                run += 1;
                chars.next();
            }
            for _ in 0..run {
                out.push('`');
            }
            if inline_code_run == 0 {
                inline_code_run = run;
            } else if inline_code_run == run {
                inline_code_run = 0;
            }
            prev = Some('`');
            continue;
        }
        if inline_code_run > 0 {
            out.push(c);
            prev = Some(c);
            continue;
        }

        // Closer: CJK punctuation flush against the start of a `*` run.
        if c == '*' && prev.is_some_and(is_cjk_punct) {
            inserts.push(out.len() as u32);
            out.push(synthetic_marker);
            out.push(c);
            // Emit the rest of the `*` run, then, if it abuts CJK punctuation,
            // also apply the opener fix after the run.
            while matches!(chars.peek(), Some(&(_, '*'))) {
                out.push('*');
                chars.next();
            }
            if matches!(chars.peek(), Some(&(_, n)) if is_cjk_punct(n)) {
                inserts.push(out.len() as u32);
                out.push(synthetic_marker);
            }
            prev = Some('*');
            continue;
        }

        // Opener: a `*` run flush against following CJK punctuation (and not
        // already handled by the closer branch above).
        if c == '*' {
            out.push(c);
            while matches!(chars.peek(), Some(&(_, '*'))) {
                out.push('*');
                chars.next();
            }
            if matches!(chars.peek(), Some(&(_, n)) if is_cjk_punct(n)) {
                inserts.push(out.len() as u32);
                out.push(synthetic_marker);
            }
            prev = Some('*');
            continue;
        }

        out.push(c);
        prev = Some(c);
    }
}

fn block_id(pos: usize, b: &Block) -> BlockId {
    let mut h = DefaultHasher::new();
    // Mix position so identical adjacent blocks (e.g. two horizontal rules,
    // duplicated paragraphs) get distinct ids — required for height/widget keying.
    (pos as u64).hash(&mut h);
    fmt_block_for_hash(b, &mut h);
    BlockId(h.finish())
}

fn fmt_block_for_hash<H: Hasher>(b: &Block, h: &mut H) {
    use std::mem::discriminant;
    discriminant(b).hash(h);
    match b {
        Block::Heading { level, id, inlines } => {
            level.hash(h);
            id.hash(h);
            for i in inlines {
                fmt_inline(i, h);
            }
        }
        Block::Paragraph(inlines) => {
            for i in inlines {
                fmt_inline(i, h);
            }
        }
        Block::CodeBlock { lang, code, .. } => {
            lang.hash(h);
            code.hash(h);
        }
        Block::Image { url, alt } => {
            url.hash(h);
            alt.hash(h);
        }
        Block::Blockquote(blocks) => {
            for x in blocks {
                fmt_block_for_hash(x, h);
            }
        }
        Block::List { ordered, items } => {
            ordered.hash(h);
            for it in items {
                it.task.hash(h);
                for x in &it.blocks {
                    fmt_block_for_hash(x, h);
                }
            }
        }
        Block::Table { headers, rows } => {
            for c in headers {
                for i in c {
                    fmt_inline(i, h);
                }
            }
            for r in rows {
                for c in r {
                    for i in c {
                        fmt_inline(i, h);
                    }
                }
            }
        }
        Block::Diagram { kind, source, hash } => {
            std::mem::discriminant(kind).hash(h);
            source.hash(h);
            hash.hash(h);
        }
        Block::Rule => {}
    }
}

fn fmt_inline<H: Hasher>(i: &Inline, h: &mut H) {
    use std::mem::discriminant;
    discriminant(i).hash(h);
    match i {
        Inline::Text(s) | Inline::Code(s) => s.hash(h),
        Inline::Emph(c) | Inline::Strong(c) | Inline::Strike(c) => {
            for x in c {
                fmt_inline(x, h);
            }
        }
        Inline::Link { url, children } => {
            url.hash(h);
            for x in children {
                fmt_inline(x, h);
            }
        }
    }
}

#[derive(Default)]
struct ParseState {
    blocks: Vec<Block>,
    offsets: Vec<u32>,
    emitted_offsets: VecDeque<u32>,
    stack: Vec<Frame>,
    /// Marker selected for this parse's CJK-emphasis rewrite. It is absent from
    /// the original source, so removing it cannot remove authored content.
    synthetic_marker: Option<char>,
}

enum Frame {
    Heading {
        level: u8,
        inlines: Vec<Inline>,
    },
    Paragraph {
        inlines: Vec<Inline>,
        offset: Option<u32>,
    },
    Emph(Vec<Inline>),
    Strong(Vec<Inline>),
    Strike(Vec<Inline>),
    Link {
        url: String,
        children: Vec<Inline>,
    },
    Blockquote(Vec<Block>),
    List {
        ordered: bool,
        items: Vec<ListItem>,
    },
    Item {
        task: Option<bool>,
        blocks: Vec<Block>,
        loose_inlines: Vec<Inline>,
    },
    CodeBlock {
        lang: Option<String>,
        code: String,
    },
    Table {
        headers: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
        in_head: bool,
        current_row: Vec<Vec<Inline>>,
        current_cell: Vec<Inline>,
    },
}

impl ParseState {
    /// Remove this parse's synthetic CJK-emphasis marker from a payload before
    /// it enters the AST. The per-line preprocessor cannot see inside multiline
    /// inline-code, math, or link destinations, so this is the backstop that
    /// keeps the marker out of rendered text, URLs, search, copy, and hashes.
    /// No-op (and allocation-free) when no rewrite happened.
    fn clean_synthetic_marker(&self, s: String) -> String {
        if let Some(marker) = self.synthetic_marker.filter(|&marker| s.contains(marker)) {
            let mut s = s;
            s.retain(|c| c != marker);
            s
        } else {
            s
        }
    }

    fn handle(&mut self, ev: Event<'_>, offset: u32) {
        match ev {
            Event::Start(tag) => self.start(tag, offset),
            Event::End(tag) => self.end(tag, offset),
            Event::Text(s) => {
                let s = self.clean_synthetic_marker(s.into_string());
                self.push_inline(Inline::Text(s), offset, true)
            }
            Event::Code(s) => {
                let s = self.clean_synthetic_marker(s.into_string());
                self.push_inline(Inline::Code(s), offset, true)
            }
            Event::SoftBreak | Event::HardBreak => {
                self.push_inline(Inline::Text("\n".into()), offset, false)
            }
            // Block-only math scope: display math becomes a Math diagram block;
            // inline math renders as literal `$…$` source text for now. The hash
            // must be computed over the cleaned source so cache keys are stable.
            Event::DisplayMath(s) => {
                let source = self.clean_synthetic_marker(s.into_string());
                let mut h = DefaultHasher::new();
                2u8.hash(&mut h);
                source.hash(&mut h);
                self.push_display_math(
                    Block::Diagram {
                        kind: DiagramKind::Math,
                        source,
                        hash: h.finish(),
                    },
                    offset,
                );
            }
            Event::InlineMath(s) => {
                let s = self.clean_synthetic_marker(s.into_string());
                self.push_inline(Inline::Text(format!("${}$", s)), offset, true)
            }
            Event::Rule => self.push_block(Block::Rule),
            Event::TaskListMarker(checked) => {
                if let Some(Frame::Item { task, .. }) = self.stack.last_mut() {
                    *task = Some(checked);
                }
            }
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>, offset: u32) {
        match tag {
            Tag::Heading { level, .. } => {
                self.stack.push(Frame::Heading {
                    level: heading_level(level),
                    inlines: Vec::new(),
                });
            }
            Tag::Paragraph => self.stack.push(Frame::Paragraph {
                inlines: Vec::new(),
                offset: Some(offset),
            }),
            Tag::Emphasis => self.stack.push(Frame::Emph(Vec::new())),
            Tag::Strong => self.stack.push(Frame::Strong(Vec::new())),
            Tag::Strikethrough => self.stack.push(Frame::Strike(Vec::new())),
            Tag::Link { dest_url, .. } => {
                let url = self.clean_synthetic_marker(dest_url.into_string());
                self.stack.push(Frame::Link {
                    url,
                    children: Vec::new(),
                });
            }
            Tag::BlockQuote(_) => self.stack.push(Frame::Blockquote(Vec::new())),
            Tag::List(start) => self.stack.push(Frame::List {
                ordered: start.is_some(),
                items: Vec::new(),
            }),
            Tag::Item => self.stack.push(Frame::Item {
                task: None,
                blocks: Vec::new(),
                loose_inlines: Vec::new(),
            }),
            Tag::CodeBlock(kind) => {
                let lang = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(s) if !s.is_empty() => {
                        Some(s.into_string())
                    }
                    _ => None,
                };
                self.stack.push(Frame::CodeBlock {
                    lang,
                    code: String::new(),
                });
            }
            Tag::Image { dest_url, .. } => {
                let url = self.clean_synthetic_marker(dest_url.into_string());
                self.push_block(Block::Image {
                    url,
                    alt: String::new(),
                });
            }
            Tag::Table(_) => self.stack.push(Frame::Table {
                headers: Vec::new(),
                rows: Vec::new(),
                in_head: false,
                current_row: Vec::new(),
                current_cell: Vec::new(),
            }),
            Tag::TableHead => {
                if let Some(Frame::Table { in_head, .. }) = self.stack.last_mut() {
                    *in_head = true;
                }
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd, offset: u32) {
        match tag {
            TagEnd::TableCell => {
                if let Some(Frame::Table {
                    current_cell,
                    current_row,
                    ..
                }) = self.stack.last_mut()
                {
                    current_row.push(std::mem::take(current_cell));
                }
                return;
            }
            TagEnd::TableRow => {
                if let Some(Frame::Table {
                    current_row, rows, ..
                }) = self.stack.last_mut()
                {
                    rows.push(std::mem::take(current_row));
                }
                return;
            }
            TagEnd::TableHead => {
                if let Some(Frame::Table {
                    in_head,
                    current_row,
                    headers,
                    ..
                }) = self.stack.last_mut()
                {
                    *headers = std::mem::take(current_row);
                    *in_head = false;
                }
                return;
            }
            _ => {}
        }
        let Some(frame) = self.stack.pop() else {
            return;
        };
        match frame {
            Frame::Heading { level, inlines } => {
                let id = slugify(&inline_to_string(&inlines));
                self.push_block(Block::Heading { level, id, inlines });
            }
            // Drop paragraphs left empty after display math split their
            // buffered inline content into sibling blocks.
            Frame::Paragraph { inlines, .. } if inlines.is_empty() => {}
            Frame::Paragraph {
                inlines,
                offset: Some(block_offset),
            } => self.push_block_with_offset(Block::Paragraph(inlines), block_offset),
            Frame::Paragraph {
                inlines,
                offset: None,
            } => self.push_block(Block::Paragraph(inlines)),
            Frame::Emph(children) => self.push_inline(Inline::Emph(children), offset, true),
            Frame::Strong(children) => self.push_inline(Inline::Strong(children), offset, true),
            Frame::Strike(children) => self.push_inline(Inline::Strike(children), offset, true),
            Frame::Link { url, children } => {
                self.push_inline(Inline::Link { url, children }, offset, true)
            }
            Frame::Blockquote(blocks) => self.push_block(Block::Blockquote(blocks)),
            Frame::List { ordered, items } => self.push_block(Block::List { ordered, items }),
            Frame::Item {
                task,
                mut blocks,
                loose_inlines,
            } => {
                if !loose_inlines.is_empty() {
                    blocks.push(Block::Paragraph(loose_inlines));
                }
                if let Some(Frame::List { items, .. }) = self.stack.last_mut() {
                    items.push(ListItem { task, blocks });
                }
            }
            Frame::CodeBlock { lang, code } => match lang.as_deref() {
                Some("mermaid") => {
                    let mut h = DefaultHasher::new();
                    0u8.hash(&mut h);
                    code.hash(&mut h);
                    self.push_block(Block::Diagram {
                        kind: DiagramKind::Mermaid,
                        source: code,
                        hash: h.finish(),
                    });
                }
                Some("dot") | Some("graphviz") => {
                    let mut h = DefaultHasher::new();
                    1u8.hash(&mut h);
                    code.hash(&mut h);
                    self.push_block(Block::Diagram {
                        kind: DiagramKind::Dot,
                        source: code,
                        hash: h.finish(),
                    });
                }
                _ => self.push_block(Block::CodeBlock {
                    lang,
                    code,
                    spans: Vec::new(),
                }),
            },
            Frame::Table { headers, rows, .. } => {
                self.push_block(Block::Table { headers, rows });
            }
        }
    }

    fn push_inline(&mut self, inl: Inline, src_offset: u32, anchors_block_offset: bool) {
        if let Some(Frame::Table { current_cell, .. }) = self.stack.last_mut() {
            current_cell.push(inl);
            return;
        }
        if let Some(top) = self.stack.last_mut() {
            match top {
                Frame::Heading { inlines, .. }
                | Frame::Emph(inlines)
                | Frame::Strong(inlines)
                | Frame::Strike(inlines) => inlines.push(inl),
                Frame::Paragraph { inlines, offset } => {
                    if anchors_block_offset && offset.is_none() {
                        *offset = Some(src_offset);
                    }
                    inlines.push(inl);
                }
                Frame::Link { children, .. } => children.push(inl),
                Frame::CodeBlock { code, .. } => {
                    if let Inline::Text(t) = inl {
                        code.push_str(&t);
                    }
                }
                Frame::Item { loose_inlines, .. } => loose_inlines.push(inl),
                _ => {}
            }
        }
    }

    fn push_block(&mut self, b: Block) {
        match self.stack.last_mut() {
            Some(Frame::Blockquote(blocks)) => blocks.push(b),
            Some(Frame::Item {
                blocks,
                loose_inlines,
                ..
            }) => {
                if !loose_inlines.is_empty() {
                    blocks.push(Block::Paragraph(std::mem::take(loose_inlines)));
                }
                blocks.push(b);
            }
            _ => self.blocks.push(b),
        }
    }

    fn push_block_with_offset(&mut self, b: Block, offset: u32) {
        match self.stack.last_mut() {
            Some(Frame::Blockquote(blocks)) => blocks.push(b),
            Some(Frame::Item {
                blocks,
                loose_inlines,
                ..
            }) => {
                if !loose_inlines.is_empty() {
                    blocks.push(Block::Paragraph(std::mem::take(loose_inlines)));
                }
                blocks.push(b);
            }
            _ => {
                self.blocks.push(b);
                self.emitted_offsets.push_back(offset);
            }
        }
    }

    fn push_display_math(&mut self, b: Block, offset: u32) {
        let paragraph = match self.stack.last_mut() {
            Some(Frame::Paragraph {
                inlines,
                offset: paragraph_offset,
            }) => Some((std::mem::take(inlines), paragraph_offset.take())),
            _ => None,
        };

        if let Some((inlines, paragraph_offset)) = paragraph {
            if !inlines.is_empty() {
                self.push_block_before_top_frame(
                    Block::Paragraph(inlines),
                    paragraph_offset.unwrap_or(offset),
                );
            }
            self.push_block_before_top_frame(b, offset);
            return;
        }

        self.push_block(b);
    }

    fn push_block_before_top_frame(&mut self, b: Block, offset: u32) {
        let end = self.stack.len().saturating_sub(1);
        let target = (0..end)
            .rev()
            .find(|&i| matches!(self.stack[i], Frame::Blockquote(_) | Frame::Item { .. }));

        match target {
            Some(i) => match &mut self.stack[i] {
                Frame::Blockquote(blocks) => blocks.push(b),
                Frame::Item {
                    blocks,
                    loose_inlines,
                    ..
                } => {
                    if !loose_inlines.is_empty() {
                        blocks.push(Block::Paragraph(std::mem::take(loose_inlines)));
                    }
                    blocks.push(b);
                }
                _ => unreachable!(),
            },
            None => {
                self.blocks.push(b);
                self.emitted_offsets.push_back(offset);
            }
        }
    }
}

fn strip_frontmatter(src: &str) -> &str {
    let trimmed = src.trim_start_matches('\u{feff}');
    if !trimmed.starts_with("---") {
        return src;
    }
    let after = &trimmed[3..];
    let rest = after
        .strip_prefix("\r\n")
        .or_else(|| after.strip_prefix('\n'));
    let Some(rest) = rest else { return src };
    let mut idx = 0;
    for line in rest.split_inclusive('\n') {
        let l = line.trim_end_matches(['\r', '\n']);
        if l == "---" || l == "..." {
            return &rest[idx + line.len()..];
        }
        idx += line.len();
    }
    src
}

fn heading_level(l: HeadingLevel) -> u8 {
    match l {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn inline_to_string(v: &[Inline]) -> String {
    let mut s = String::new();
    for i in v {
        stringify_inline(i, &mut s);
    }
    s
}

fn stringify_inline(i: &Inline, s: &mut String) {
    match i {
        Inline::Text(t) | Inline::Code(t) => s.push_str(t),
        Inline::Emph(c) | Inline::Strong(c) | Inline::Strike(c) => {
            for x in c {
                stringify_inline(x, s);
            }
        }
        Inline::Link { children, .. } => {
            for x in children {
                stringify_inline(x, s);
            }
        }
    }
}

fn slugify(s: &str) -> String {
    s.chars()
        .filter_map(|c| {
            if c.is_alphanumeric() {
                Some(c.to_ascii_lowercase())
            } else if c.is_whitespace() {
                Some('-')
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Collect the rendered (text) content of every Strong span in the parse.
    fn strong_texts(src: &str) -> Vec<String> {
        let (blocks, _) = parse(src);
        let mut out = Vec::new();
        for (_, b) in &blocks {
            if let Block::Paragraph(inlines) | Block::Heading { inlines, .. } = b {
                collect_strong(inlines, &mut out);
            }
        }
        out
    }

    fn emph_texts(src: &str) -> Vec<String> {
        let (blocks, _) = parse(src);
        let mut out = Vec::new();
        for (_, b) in &blocks {
            if let Block::Paragraph(inlines) | Block::Heading { inlines, .. } = b {
                collect_emph(inlines, &mut out);
            }
        }
        out
    }

    fn collect_strong(inlines: &[Inline], out: &mut Vec<String>) {
        for i in inlines {
            match i {
                Inline::Strong(c) => out.push(inline_to_string(c)),
                Inline::Emph(c) | Inline::Strike(c) | Inline::Link { children: c, .. } => {
                    collect_strong(c, out)
                }
                _ => {}
            }
        }
    }

    fn collect_emph(inlines: &[Inline], out: &mut Vec<String>) {
        for i in inlines {
            match i {
                Inline::Emph(c) => out.push(inline_to_string(c)),
                Inline::Strong(c) | Inline::Strike(c) | Inline::Link { children: c, .. } => {
                    collect_emph(c, out)
                }
                _ => {}
            }
        }
    }

    #[test]
    fn cjk_strong_after_fullwidth_paren() {
        // Issue #6: `）**的` — closing `**` is preceded by a CJK punctuation
        // and followed by a CJK letter, which CommonMark flanking rules refuse
        // to close. rmdv should still render it bold.
        let src = "這個模型改善了**卷積神經網路（CNN）**的特徵提取效率。";
        assert_eq!(strong_texts(src), vec!["卷積神經網路（CNN）".to_string()]);
    }

    #[test]
    fn cjk_emph_after_fullwidth_paren() {
        // Same failure mode for single-asterisk emphasis.
        let src = "前綴*卷積（X）*的後綴";
        assert_eq!(emph_texts(src), vec!["卷積（X）".to_string()]);
    }

    #[test]
    fn cjk_strong_after_ideographic_period() {
        // CJK full stop 。 (U+3002) before the closing `**`.
        let src = "說明**重點。**接著繼續";
        assert_eq!(strong_texts(src), vec!["重點。".to_string()]);
    }

    #[test]
    fn cjk_strong_after_closing_quote() {
        // Right corner bracket 」 before the close.
        let src = "他說**「你好」**然後離開";
        assert_eq!(strong_texts(src), vec!["「你好」".to_string()]);
    }

    #[test]
    fn ascii_bold_unaffected() {
        // Plain ASCII bold still works and is byte-identical (no marker
        // injected, borrowed fast path).
        let src = "this is **bold** text";
        assert_eq!(strong_texts(src), vec!["bold".to_string()]);
        let (cooked, inserts, marker) = preprocess_cjk_emphasis(src);
        assert!(matches!(cooked, std::borrow::Cow::Borrowed(_)));
        assert!(inserts.is_empty());
        assert!(marker.is_none());
    }

    #[test]
    fn ascii_punct_before_close_not_rewritten() {
        // CommonMark intentionally leaves `**(foo)**bar` literal (punct before,
        // letter after, no space). We must NOT change ASCII semantics: no marker
        // is inserted, so it stays literal exactly as upstream / CommonMark.
        let src = "a **(foo)**bar";
        let (_, inserts, _) = preprocess_cjk_emphasis(src);
        assert!(
            inserts.is_empty(),
            "ASCII punctuation must not trigger a rewrite"
        );
        assert_eq!(strong_texts(src), Vec::<String>::new());
    }

    #[test]
    fn fenced_code_block_untouched() {
        // Asterisks after CJK punctuation inside a fenced block must not get a
        // marker — code is rendered verbatim.
        let src = "```\n（X）**的\n```\n";
        let (_, inserts, _) = preprocess_cjk_emphasis(src);
        assert!(inserts.is_empty(), "fenced code must not be rewritten");
    }

    #[test]
    fn inline_code_untouched() {
        // Inline code span content is verbatim too.
        let src = "看這個 `（X）**的` 範例";
        let (_, inserts, _) = preprocess_cjk_emphasis(src);
        assert!(inserts.is_empty(), "inline code must not be rewritten");
    }

    #[test]
    fn cjk_emphasis_outside_code_still_rewritten_with_code_present() {
        // A real emphasis outside code is rewritten even when an inline code
        // span containing `*` precedes it on the same line.
        let src = "`a*b` 與**重點（X）**的對比";
        assert_eq!(strong_texts(src), vec!["重點（X）".to_string()]);
    }

    #[test]
    fn block_offsets_map_to_original_coordinates() {
        // Marker injection must not corrupt the byte offsets parse() returns:
        // each block's offset must point at the right line in the ORIGINAL src.
        // First line is a heading, second a paragraph that triggers a rewrite.
        let src = "# 標題\n\n這是**內容（X）**的段落\n";
        let (blocks, offsets) = parse(src);
        assert_eq!(blocks.len(), offsets.len());
        // Offsets must be non-decreasing and land within the original source.
        assert!(offsets.windows(2).all(|w| w[0] <= w[1]));
        assert!(offsets.iter().all(|&o| (o as usize) <= src.len()));
        // The paragraph block's offset must point at the paragraph's start
        // ("這是…"), i.e. the byte index of "這" in the ORIGINAL string.
        let para_start = src.find("這是").unwrap() as u32;
        let para_off = *offsets.last().unwrap();
        assert_eq!(
            para_off, para_start,
            "paragraph offset must map back to original coordinates"
        );
        // And it actually parsed as Strong.
        assert_eq!(strong_texts(src), vec!["內容（X）".to_string()]);
    }

    #[test]
    fn cjk_strong_opener_followed_by_punct() {
        // Issue #6 mirror case: the OPENING `**` is immediately followed by CJK
        // punctuation (`**「`), which also fails CommonMark flanking. Needs a
        // a marker after the opener AND before the closer.
        let src = "他說**「你好，世界」**然後離開";
        assert_eq!(strong_texts(src), vec!["「你好，世界」".to_string()]);
    }

    #[test]
    fn tilde_fence_untouched() {
        // `~~~`-fenced blocks are skipped just like ```-fenced ones.
        let src = "~~~\n（X）**的\n~~~\n";
        let (_, inserts, _) = preprocess_cjk_emphasis(src);
        assert!(inserts.is_empty(), "~~~ fenced code must not be rewritten");
    }

    #[test]
    fn strikethrough_line_still_rewritten() {
        // A line starting with `~` for strikethrough must NOT be mistaken for a
        // fence delimiter; emphasis on it is still rewritten.
        let src = "~~刪除~~ 與**重點（X）**的對比";
        assert_eq!(strong_texts(src), vec!["重點（X）".to_string()]);
    }

    #[test]
    fn offsets_with_opener_insert_map_back() {
        // Opener-side inserts shift bytes too; offsets must still map back.
        let src = "# 標題\n\n他說**「話」**然後\n";
        let (blocks, offsets) = parse(src);
        assert_eq!(blocks.len(), offsets.len());
        assert!(offsets.windows(2).all(|w| w[0] <= w[1]));
        assert!(offsets.iter().all(|&o| (o as usize) <= src.len()));
        let para_start = src.find("他說").unwrap() as u32;
        assert_eq!(*offsets.last().unwrap(), para_start);
    }

    #[test]
    fn no_synthetic_marker_leaks_into_text() {
        // The injected marker must be stripped from rendered/searchable text.
        let src = "這是**內容（X）**的段落";
        assert!(!any_synthetic_marker_in_ast(src));
    }

    #[test]
    fn authored_zwsp_survives_cjk_emphasis_rewrite() {
        let src = "原文\u{200b}保留，並且**內容（X）**的格式正確";
        let (blocks, _) = parse(src);
        let rendered = blocks
            .iter()
            .find_map(|(_, block)| match block {
                Block::Paragraph(inlines) => Some(inline_to_string(inlines)),
                _ => None,
            })
            .expect("paragraph");

        assert_eq!(rendered.matches(ZWSP).count(), 1);
        assert!(rendered.contains("原文\u{200b}保留"));
        assert_eq!(strong_texts(src), vec!["內容（X）".to_string()]);
    }

    #[test]
    fn authored_literal_pua_survives_marker_collision() {
        let src = "保留\u{E000}字元，並且**內容（X）**的格式正確";
        let (_, _, marker) = preprocess_cjk_emphasis(src);
        assert_eq!(marker, Some('\u{E001}'));

        let (blocks, _) = parse(src);
        let rendered = blocks
            .iter()
            .find_map(|(_, block)| match block {
                Block::Paragraph(inlines) => Some(inline_to_string(inlines)),
                _ => None,
            })
            .expect("paragraph");
        assert!(rendered.contains('\u{E000}'));
        assert_eq!(strong_texts(src), vec!["內容（X）".to_string()]);
    }

    #[test]
    fn decoded_pua_entity_survives_cjk_emphasis_rewrite() {
        let src = "保留 &#xE000; 字元，並且**內容（X）**的格式正確";
        let (_, _, marker) = preprocess_cjk_emphasis(src);
        assert_eq!(marker, Some('\u{E001}'));

        let (blocks, _) = parse(src);
        let rendered = blocks
            .iter()
            .find_map(|(_, block)| match block {
                Block::Paragraph(inlines) => Some(inline_to_string(inlines)),
                _ => None,
            })
            .expect("paragraph");
        assert!(rendered.contains('\u{E000}'));
        assert_eq!(strong_texts(src), vec!["內容（X）".to_string()]);
    }

    #[test]
    fn decoded_pua_entity_survives_in_link_destination() {
        let src = "[連結](https://example.com/a&#X0E000;b/路徑（X）**的)";
        let (blocks, _) = parse(src);
        let url = blocks.iter().find_map(|(_, block)| match block {
            Block::Paragraph(inlines) => inlines.iter().find_map(|inline| match inline {
                Inline::Link { url, .. } => Some(url.as_str()),
                _ => None,
            }),
            _ => None,
        });

        assert_eq!(url, Some("https://example.com/a\u{E000}b/路徑（X）**的"));
    }

    #[test]
    fn marker_occupancy_uses_one_pass_fixed_map() {
        let src = "\u{E000} &#xE001; &#X00E002; &#0057347;";
        assert_eq!(
            MarkerOccupancy::from_source(src).first_free(),
            Some('\u{E004}')
        );
    }

    #[test]
    fn all_pua_scalars_fall_back_without_marker() {
        let mut src = String::with_capacity(PUA_COUNT * 3 + 32);
        for codepoint in PUA_START..=PUA_END {
            src.push(char::from_u32(codepoint).unwrap());
        }
        src.push_str(" 並且**內容（X）**的格式正確");

        assert!(MarkerOccupancy::from_source(&src).first_free().is_none());
        let (cooked, inserts, marker) = preprocess_cjk_emphasis(&src);
        assert!(matches!(cooked, std::borrow::Cow::Borrowed(_)));
        assert!(inserts.is_empty());
        assert!(marker.is_none());
    }

    #[test]
    fn synthetic_marker_is_removed_from_link_destination() {
        let expected = "https://example.com/路徑（X）**的";
        let src = format!("[連結]({expected})");
        let (blocks, _) = parse(&src);
        let url = blocks.iter().find_map(|(_, block)| match block {
            Block::Paragraph(inlines) => inlines.iter().find_map(|inline| match inline {
                Inline::Link { url, .. } => Some(url.as_str()),
                _ => None,
            }),
            _ => None,
        });

        assert_eq!(url, Some(expected));
    }

    #[test]
    fn synthetic_marker_is_removed_from_image_destination() {
        let expected = "https://example.com/圖片（X）**的.png";
        let src = format!("![圖片]({expected})");
        let (blocks, _) = parse(&src);
        let url = blocks.iter().find_map(|(_, block)| match block {
            Block::Image { url, .. } => Some(url.as_str()),
            _ => None,
        });

        assert_eq!(url, Some(expected));
    }

    /// True if a BMP private-use marker appears anywhere in the parsed AST.
    /// Test inputs do not author private-use scalars, so any match leaked from
    /// preprocessing.
    fn any_synthetic_marker_in_ast(src: &str) -> bool {
        let (blocks, _) = parse(src);
        blocks.iter().any(|(_, b)| block_has_synthetic_marker(b))
    }

    fn has_synthetic_marker(s: &str) -> bool {
        s.chars().any(|c| matches!(c, '\u{E000}'..='\u{F8FF}'))
    }

    fn block_has_synthetic_marker(b: &Block) -> bool {
        match b {
            Block::Paragraph(inl) | Block::Heading { inlines: inl, .. } => {
                inl.iter().any(inline_has_synthetic_marker)
            }
            Block::CodeBlock { code, .. } => has_synthetic_marker(code),
            Block::Diagram { source, .. } => has_synthetic_marker(source),
            Block::Blockquote(bs) => bs.iter().any(block_has_synthetic_marker),
            Block::List { items, .. } => items
                .iter()
                .any(|it| it.blocks.iter().any(block_has_synthetic_marker)),
            Block::Table { headers, rows } => {
                headers.iter().flatten().any(inline_has_synthetic_marker)
                    || rows
                        .iter()
                        .flatten()
                        .flatten()
                        .any(inline_has_synthetic_marker)
            }
            _ => false,
        }
    }

    fn inline_has_synthetic_marker(i: &Inline) -> bool {
        match i {
            Inline::Text(s) | Inline::Code(s) => has_synthetic_marker(s),
            Inline::Emph(c) | Inline::Strong(c) | Inline::Strike(c) => {
                c.iter().any(inline_has_synthetic_marker)
            }
            Inline::Link { url, children } => {
                has_synthetic_marker(url) || children.iter().any(inline_has_synthetic_marker)
            }
        }
    }

    #[test]
    fn no_synthetic_marker_leaks_into_multiline_inline_code() {
        // An inline code span that spans a soft break: the per-line preprocessor
        // can't see it's inside code, so it may inject a marker that pulldown
        // emits as Event::Code. That marker must still be stripped.
        let src = "`\n（X）**字`";
        assert!(
            !any_synthetic_marker_in_ast(src),
            "marker must not leak into multi-line inline code"
        );
    }

    #[test]
    fn no_synthetic_marker_leaks_into_display_math() {
        // Display math `$$…$$` contains `*` as LaTeX, not emphasis. No marker
        // may reach the math source or its content hash.
        let src = "$$（X）*的*$$";
        assert!(!any_synthetic_marker_in_ast(src));
        // The content hash must match the same math written without any CJK-punct
        // trigger — i.e. the hash is computed over clean source.
        let polluted = parse(src);
        let clean = parse("$$ x *y* $$");
        let polluted_hash = polluted.0.iter().find_map(|(_, b)| match b {
            Block::Diagram { hash, .. } => Some(*hash),
            _ => None,
        });
        let clean_hash = clean.0.iter().find_map(|(_, b)| match b {
            Block::Diagram { hash, .. } => Some(*hash),
            _ => None,
        });
        assert!(polluted_hash.is_some() && clean_hash.is_some());
        // (Different content → different hash; the point is the polluted hash is
        // over clean "（X）*的*", without the marker. Recompute the expected.)
        let mut h = DefaultHasher::new();
        2u8.hash(&mut h);
        "（X）*的*".hash(&mut h);
        assert_eq!(
            polluted_hash.unwrap(),
            h.finish(),
            "display-math hash must be over marker-free source"
        );
    }

    #[test]
    fn no_synthetic_marker_leaks_into_inline_math() {
        // Inline math renders as literal `$…$` text; no marker may appear in it.
        let src = "計算 $x（X）*的*$ 完成";
        assert!(
            !any_synthetic_marker_in_ast(src),
            "marker must not leak into inline math text"
        );
    }

    #[test]
    fn map_offset_back_boundary() {
        // An insertion at byte 5 (rewritten coords) means a marker occupies
        // [5, 8). An offset exactly at 5 must NOT shift back (the marker starts
        // at/after it); only insertions strictly before it count.
        assert_eq!(map_offset_back(5, &[5]), 5);
        assert_eq!(map_offset_back(6, &[5]), 3); // 6 - 3
        assert_eq!(map_offset_back(4, &[5]), 4); // before the insertion, unchanged
        assert_eq!(map_offset_back(20, &[5, 10, 15]), 20 - 9);
        assert_eq!(map_offset_back(10, &[5, 10, 15]), 10 - 3); // one strictly before
    }

    #[test]
    fn degenerate_asterisks_do_not_panic() {
        // Trailing/leading `*` runs and punct-adjacent stars at string edges
        // must never panic or produce out-of-bounds offsets.
        for s in [
            "*",
            "**",
            "（X）*",
            "（X）**",
            "*（",
            "**（",
            "（）*的*（）",
        ] {
            let _ = preprocess_cjk_emphasis(s);
            let (blocks, offsets) = parse(s);
            assert_eq!(blocks.len(), offsets.len());
            assert!(offsets.iter().all(|&o| (o as usize) <= s.len()));
        }
    }

    #[test]
    fn ideographic_space_does_not_trigger_rewrite() {
        // U+3000 is whitespace; `*` adjacent to it already flanks correctly, so
        // we must not waste a marker there. Bold still renders (via the space).
        let src = "標題　**內容**　結束"; // 　 is U+3000
        let (_, inserts, _) = preprocess_cjk_emphasis(src);
        assert!(
            inserts.is_empty(),
            "ideographic space must not trigger a rewrite"
        );
        assert_eq!(strong_texts(src), vec!["內容".to_string()]);
    }

    #[test]
    fn crlf_line_endings() {
        let src = "標題\r\n\r\n這是**內容（X）**的\r\n";
        let (blocks, offsets) = parse(src);
        assert_eq!(blocks.len(), offsets.len());
        assert!(offsets.iter().all(|&o| (o as usize) <= src.len()));
        assert_eq!(strong_texts(src), vec!["內容（X）".to_string()]);
    }
}
