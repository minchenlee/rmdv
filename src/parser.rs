use crate::ast::{Block, BlockId, DiagramKind, Inline, ListItem};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::hash::{DefaultHasher, Hash, Hasher};

pub fn parse(src: &str) -> (Vec<(BlockId, Block)>, Vec<u32>) {
    let src = strip_frontmatter(src);
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_SMART_PUNCTUATION);
    let parser = Parser::new_ext(src, opts).into_offset_iter();
    let mut state = ParseState::default();
    let mut pending_offset: Option<u32> = None;
    for (ev, range) in parser {
        if matches!(ev, Event::Start(_)) && state.stack.is_empty() {
            pending_offset = Some(range.start as u32);
        }
        let before_len = state.blocks.len();
        let take_offset = pending_offset;
        state.handle(ev);
        if state.blocks.len() > before_len {
            let off = take_offset
                .or(Some(range.start as u32))
                .unwrap_or(0);
            for _ in before_len..state.blocks.len() {
                state.offsets.push(off);
            }
            pending_offset = None;
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
            level.hash(h); id.hash(h);
            for i in inlines { fmt_inline(i, h); }
        }
        Block::Paragraph(inlines) => for i in inlines { fmt_inline(i, h); },
        Block::CodeBlock { lang, code, .. } => { lang.hash(h); code.hash(h); }
        Block::Image { url, alt } => { url.hash(h); alt.hash(h); }
        Block::Blockquote(blocks) => for x in blocks { fmt_block_for_hash(x, h); },
        Block::List { ordered, items } => {
            ordered.hash(h);
            for it in items {
                it.task.hash(h);
                for x in &it.blocks { fmt_block_for_hash(x, h); }
            }
        }
        Block::Table { headers, rows } => {
            for c in headers { for i in c { fmt_inline(i, h); } }
            for r in rows { for c in r { for i in c { fmt_inline(i, h); } } }
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
        Inline::Emph(c) | Inline::Strong(c) | Inline::Strike(c) => for x in c { fmt_inline(x, h); },
        Inline::Link { url, children } => { url.hash(h); for x in children { fmt_inline(x, h); } }
    }
}

#[derive(Default)]
struct ParseState {
    blocks: Vec<Block>,
    offsets: Vec<u32>,
    stack: Vec<Frame>,
}

enum Frame {
    Heading { level: u8, inlines: Vec<Inline> },
    Paragraph(Vec<Inline>),
    Emph(Vec<Inline>),
    Strong(Vec<Inline>),
    Strike(Vec<Inline>),
    Link { url: String, children: Vec<Inline> },
    Blockquote(Vec<Block>),
    List { ordered: bool, items: Vec<ListItem> },
    Item { task: Option<bool>, blocks: Vec<Block>, loose_inlines: Vec<Inline> },
    CodeBlock { lang: Option<String>, code: String },
    Table {
        headers: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
        in_head: bool,
        current_row: Vec<Vec<Inline>>,
        current_cell: Vec<Inline>,
    },
}

impl ParseState {
    fn handle(&mut self, ev: Event<'_>) {
        match ev {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(s) => self.push_inline(Inline::Text(s.into_string())),
            Event::Code(s) => self.push_inline(Inline::Code(s.into_string())),
            Event::SoftBreak | Event::HardBreak => self.push_inline(Inline::Text("\n".into())),
            Event::Rule => self.push_block(Block::Rule),
            Event::TaskListMarker(checked) => {
                if let Some(Frame::Item { task, .. }) = self.stack.last_mut() {
                    *task = Some(checked);
                }
            }
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Heading { level, .. } => {
                self.stack.push(Frame::Heading { level: heading_level(level), inlines: Vec::new() });
            }
            Tag::Paragraph => self.stack.push(Frame::Paragraph(Vec::new())),
            Tag::Emphasis => self.stack.push(Frame::Emph(Vec::new())),
            Tag::Strong => self.stack.push(Frame::Strong(Vec::new())),
            Tag::Strikethrough => self.stack.push(Frame::Strike(Vec::new())),
            Tag::Link { dest_url, .. } => self.stack.push(Frame::Link { url: dest_url.into_string(), children: Vec::new() }),
            Tag::BlockQuote(_) => self.stack.push(Frame::Blockquote(Vec::new())),
            Tag::List(start) => self.stack.push(Frame::List { ordered: start.is_some(), items: Vec::new() }),
            Tag::Item => self.stack.push(Frame::Item { task: None, blocks: Vec::new(), loose_inlines: Vec::new() }),
            Tag::CodeBlock(kind) => {
                let lang = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(s) if !s.is_empty() => Some(s.into_string()),
                    _ => None,
                };
                self.stack.push(Frame::CodeBlock { lang, code: String::new() });
            }
            Tag::Image { dest_url, .. } => {
                self.push_block(Block::Image { url: dest_url.into_string(), alt: String::new() });
            }
            Tag::Table(_) => self.stack.push(Frame::Table {
                headers: Vec::new(), rows: Vec::new(), in_head: false,
                current_row: Vec::new(), current_cell: Vec::new(),
            }),
            Tag::TableHead => {
                if let Some(Frame::Table { in_head, .. }) = self.stack.last_mut() {
                    *in_head = true;
                }
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::TableCell => {
                if let Some(Frame::Table { current_cell, current_row, .. }) = self.stack.last_mut() {
                    current_row.push(std::mem::take(current_cell));
                }
                return;
            }
            TagEnd::TableRow => {
                if let Some(Frame::Table { current_row, rows, .. }) = self.stack.last_mut() {
                    rows.push(std::mem::take(current_row));
                }
                return;
            }
            TagEnd::TableHead => {
                if let Some(Frame::Table { in_head, current_row, headers, .. }) = self.stack.last_mut() {
                    *headers = std::mem::take(current_row);
                    *in_head = false;
                }
                return;
            }
            _ => {}
        }
        let Some(frame) = self.stack.pop() else { return; };
        match frame {
            Frame::Heading { level, inlines } => {
                let id = slugify(&inline_to_string(&inlines));
                self.push_block(Block::Heading { level, id, inlines });
            }
            Frame::Paragraph(inlines) => self.push_block(Block::Paragraph(inlines)),
            Frame::Emph(children) => self.push_inline(Inline::Emph(children)),
            Frame::Strong(children) => self.push_inline(Inline::Strong(children)),
            Frame::Strike(children) => self.push_inline(Inline::Strike(children)),
            Frame::Link { url, children } => self.push_inline(Inline::Link { url, children }),
            Frame::Blockquote(blocks) => self.push_block(Block::Blockquote(blocks)),
            Frame::List { ordered, items } => self.push_block(Block::List { ordered, items }),
            Frame::Item { task, mut blocks, loose_inlines } => {
                if !loose_inlines.is_empty() {
                    blocks.insert(0, Block::Paragraph(loose_inlines));
                }
                if let Some(Frame::List { items, .. }) = self.stack.last_mut() {
                    items.push(ListItem { task, blocks });
                }
            }
            Frame::CodeBlock { lang, code } => {
                match lang.as_deref() {
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
                    _ => self.push_block(Block::CodeBlock { lang, code, spans: Vec::new() }),
                }
            }
            Frame::Table { headers, rows, .. } => {
                self.push_block(Block::Table { headers, rows });
            }
        }
    }

    fn push_inline(&mut self, inl: Inline) {
        if let Some(Frame::Table { current_cell, .. }) = self.stack.last_mut() {
            current_cell.push(inl);
            return;
        }
        if let Some(top) = self.stack.last_mut() {
            match top {
                Frame::Heading { inlines, .. }
                | Frame::Paragraph(inlines)
                | Frame::Emph(inlines)
                | Frame::Strong(inlines)
                | Frame::Strike(inlines) => inlines.push(inl),
                Frame::Link { children, .. } => children.push(inl),
                Frame::CodeBlock { code, .. } => {
                    if let Inline::Text(t) = inl { code.push_str(&t); }
                }
                Frame::Item { loose_inlines, .. } => loose_inlines.push(inl),
                _ => {}
            }
        }
    }

    fn push_block(&mut self, b: Block) {
        match self.stack.last_mut() {
            Some(Frame::Blockquote(blocks)) | Some(Frame::Item { blocks, .. }) => blocks.push(b),
            _ => self.blocks.push(b),
        }
    }
}

fn strip_frontmatter(src: &str) -> &str {
    let trimmed = src.trim_start_matches('\u{feff}');
    if !trimmed.starts_with("---") {
        return src;
    }
    let after = &trimmed[3..];
    let rest = after.strip_prefix("\r\n").or_else(|| after.strip_prefix('\n'));
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
    for i in v { stringify_inline(i, &mut s); }
    s
}

fn stringify_inline(i: &Inline, s: &mut String) {
    match i {
        Inline::Text(t) | Inline::Code(t) => s.push_str(t),
        Inline::Emph(c) | Inline::Strong(c) | Inline::Strike(c) => {
            for x in c { stringify_inline(x, s); }
        }
        Inline::Link { children, .. } => {
            for x in children { stringify_inline(x, s); }
        }
    }
}

fn slugify(s: &str) -> String {
    s.chars()
        .filter_map(|c| {
            if c.is_alphanumeric() { Some(c.to_ascii_lowercase()) }
            else if c.is_whitespace() { Some('-') }
            else { None }
        })
        .collect()
}
