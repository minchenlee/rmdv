//! Hand-rolled common-subset LaTeX-document parser that emits mdv's AST.
//!
//! Covers the academic-paper subset: sectioning, inline formatting, display
//! math (routed to `DiagramKind::Math`), lists, tabular, figures, verbatim,
//! and cross-refs. Never panics — unrecognized constructs degrade gracefully
//! to their textual content.

use crate::ast::{Block, BlockId, DiagramKind, Inline, ListItem};
use std::hash::{DefaultHasher, Hash, Hasher};

pub fn parse(src: &str) -> (Vec<(BlockId, Block)>, Vec<u32>) {
    let body = document_body(src);
    let base = body.as_ptr() as usize - src.as_ptr() as usize;
    let mut p = Parser::new(body, base as u32);
    let blocks = p.parse_blocks();
    let offsets = p.offsets;
    let blocks: Vec<(BlockId, Block)> = blocks
        .into_iter()
        .enumerate()
        .map(|(pos, b)| (block_id(pos, &b), b))
        .collect();
    (blocks, offsets)
}

/// Slice out the `\begin{document}`…`\end{document}` body when present, else
/// the whole input (so bare fragments parse). Returns a sub-slice of `src` so
/// byte offsets stay anchored to the original.
fn document_body(src: &str) -> &str {
    let Some(begin) = src.find("\\begin{document}") else {
        return src;
    };
    let after = begin + "\\begin{document}".len();
    let end = src[after..]
        .find("\\end{document}")
        .map(|e| after + e)
        .unwrap_or(src.len());
    &src[after..end]
}

// ---- id / hash scheme (mirrors parser.rs) ----

fn block_id(pos: usize, b: &Block) -> BlockId {
    let mut h = DefaultHasher::new();
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
            discriminant(kind).hash(h);
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

/// Math diagram hash — same scheme as parser.rs DisplayMath (`2u8` then source)
/// so identical equations dedupe across the markdown and tex pipelines.
fn math_hash(source: &str) -> u64 {
    let mut h = DefaultHasher::new();
    2u8.hash(&mut h);
    source.hash(&mut h);
    h.finish()
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

// ---- parser ----

struct Parser<'a> {
    bytes: &'a [u8],
    src: &'a str,
    pos: usize,
    /// Byte offset of `src` within the original document (for line-nav).
    base: u32,
    offsets: Vec<u32>,
}

/// A buffered top-level block plus the byte offset where it started.
struct Emitted {
    block: Block,
    offset: u32,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str, base: u32) -> Self {
        Parser {
            bytes: src.as_bytes(),
            src,
            pos: 0,
            base,
            offsets: Vec::new(),
        }
    }

    fn parse_blocks(&mut self) -> Vec<Block> {
        let emitted = self.blocks_until(None);
        let mut blocks = Vec::with_capacity(emitted.len());
        for e in emitted {
            self.offsets.push(e.offset);
            blocks.push(e.block);
        }
        blocks
    }

    /// Parse blocks until `stop` (an `\end{env}` env name) or EOF. The
    /// terminator is consumed when matched.
    fn blocks_until(&mut self, stop: Option<&str>) -> Vec<Emitted> {
        let mut out = Vec::new();
        loop {
            self.skip_blank();
            if self.pos >= self.bytes.len() {
                break;
            }
            let start = self.pos;
            // Check for the stopping \end{env}.
            if let Some(env) = stop {
                if self.at_end_env(env) {
                    self.consume_end_env(env);
                    break;
                }
            }
            // A stray \end{..} we are not waiting for: consume + ignore so we
            // don't loop forever on it.
            if self.at_str("\\end{") {
                self.skip_command_token();
                continue;
            }
            if let Some(block) = self.parse_block() {
                out.push(Emitted {
                    block,
                    offset: self.base + start as u32,
                });
            } else if self.pos == start {
                // Defensive: never spin.
                self.pos += 1;
            }
        }
        out
    }

    fn parse_block(&mut self) -> Option<Block> {
        self.skip_blank();
        let c = *self.bytes.get(self.pos)?;
        if c == b'\\' {
            if let Some(b) = self.try_command_block() {
                return Some(b);
            }
        }
        if c == b'%' {
            self.skip_comment();
            return None;
        }
        // Display math openers ($$ / \[) become Diagram blocks.
        if self.at_str("$$") {
            return Some(self.parse_dollar_math());
        }
        if self.at_str("\\[") {
            return Some(self.parse_bracket_math());
        }
        // Otherwise a paragraph of inline content up to a blank line.
        self.parse_paragraph()
    }

    /// Block-level commands and environments. Returns None when the command is
    /// inline (e.g. `\textbf`) so it gets folded into a paragraph instead.
    fn try_command_block(&mut self) -> Option<Block> {
        let save = self.pos;
        let name = self.peek_command_name();
        match name.as_str() {
            "section" => return Some(self.heading(1)),
            "subsection" => return Some(self.heading(2)),
            "subsubsection" => return Some(self.heading(3)),
            "title" => return Some(self.heading(1)),
            "author" | "date" => {
                self.skip_command_token();
                let arg = self.read_group_or_empty();
                return Some(Block::Paragraph(parse_inlines(&arg)));
            }
            "begin" => return self.parse_environment(),
            "hrule" => {
                self.skip_command_token();
                return Some(Block::Rule);
            }
            "rule" => {
                self.skip_command_token();
                let _ = self.read_optional();
                let _ = self.read_group_or_empty();
                let _ = self.read_group_or_empty();
                return Some(Block::Rule);
            }
            "maketitle" | "tableofcontents" | "newpage" | "clearpage" | "bigskip" | "medskip"
            | "smallskip" | "noindent" | "centering" | "hline" | "vfill" | "hfill" => {
                self.skip_command_token();
                return None;
            }
            // Cross-ref anchor / index entry: drop command + its braced arg.
            "label" | "index" => {
                self.skip_command_token();
                let _ = self.read_group_or_empty();
                return None;
            }
            _ => {}
        }
        self.pos = save;
        None
    }

    /// `\section[*]{...}` → heading. Star form is handled identically.
    fn heading(&mut self, level: u8) -> Block {
        self.skip_command_token(); // consumes name + trailing '*'
        let text = self.read_group_or_empty();
        let inlines = parse_inlines(&text);
        let id = slugify(&inline_to_string(&inlines));
        Block::Heading { level, id, inlines }
    }

    fn parse_environment(&mut self) -> Option<Block> {
        self.skip_command_token(); // \begin
        let env = self.read_group_or_empty();
        let env = env.trim();
        match env {
            "itemize" => Some(self.parse_list(false)),
            "enumerate" => Some(self.parse_list(true)),
            "equation" | "equation*" | "align" | "align*" | "gather" | "gather*" | "multline"
            | "multline*" => Some(self.parse_env_math(env)),
            "tabular" => Some(self.parse_tabular()),
            "verbatim" => Some(self.parse_verbatim("verbatim")),
            "lstlisting" => Some(self.parse_verbatim("lstlisting")),
            "figure" => Some(self.parse_figure()),
            "quote" | "quotation" => Some(self.parse_quote(env)),
            _ => {
                // Unknown environment: render its body as paragraph(s); take
                // the first emitted block, dropping any others (best effort).
                let inner = self.blocks_until(Some(env));
                inner.into_iter().next().map(|e| e.block)
            }
        }
    }

    // ---- math ----

    fn parse_dollar_math(&mut self) -> Block {
        self.pos += 2; // $$
        let start = self.pos;
        let end = self.src[start..]
            .find("$$")
            .map(|e| start + e)
            .unwrap_or(self.bytes.len());
        let body = strip_math_meta(&self.src[start..end]);
        self.pos = (end + 2).min(self.bytes.len());
        let source = body.trim().to_string();
        Block::Diagram {
            kind: DiagramKind::Math,
            hash: math_hash(&source),
            source,
        }
    }

    fn parse_bracket_math(&mut self) -> Block {
        self.pos += 2; // \[
        let start = self.pos;
        let end = self.src[start..]
            .find("\\]")
            .map(|e| start + e)
            .unwrap_or(self.bytes.len());
        let body = strip_math_meta(&self.src[start..end]);
        self.pos = (end + 2).min(self.bytes.len());
        let source = body.trim().to_string();
        Block::Diagram {
            kind: DiagramKind::Math,
            hash: math_hash(&source),
            source,
        }
    }

    /// `equation`/`align`/… — the body is passed to iced_math *with* the
    /// `\begin{env}…\end{env}` wrapper, which pulldown-latex parses natively.
    fn parse_env_math(&mut self, env: &str) -> Block {
        let begin = format!("\\end{{{env}}}");
        let start = self.pos;
        let end = self.src[start..]
            .find(&begin)
            .map(|e| start + e)
            .unwrap_or(self.bytes.len());
        let inner = strip_math_meta(&self.src[start..end]);
        self.pos = (end + begin.len()).min(self.bytes.len());
        let source = format!("\\begin{{{env}}}{}\\end{{{env}}}", inner);
        Block::Diagram {
            kind: DiagramKind::Math,
            hash: math_hash(&source),
            source,
        }
    }

    // ---- lists ----

    fn parse_list(&mut self, ordered: bool) -> Block {
        let end_marker = if ordered { "enumerate" } else { "itemize" };
        let mut items: Vec<ListItem> = Vec::new();
        let mut seen_item = false;
        let mut item_text = String::new();
        let mut nested: Vec<Block> = Vec::new();

        loop {
            // Skip only comments here — NOT whitespace, which is meaningful
            // inter-word spacing inside item text (collapsed later in make_item).
            self.skip_comments();
            if self.pos >= self.bytes.len() || self.at_end_env(end_marker) {
                self.consume_end_env(end_marker);
                break;
            }
            if self.at_str("\\item") {
                if seen_item {
                    items.push(make_item(
                        std::mem::take(&mut item_text),
                        std::mem::take(&mut nested),
                    ));
                }
                seen_item = true;
                self.skip_command_token();
                let _ = self.read_optional();
                continue;
            }
            // Nested list inside the current item.
            if self.at_str("\\begin{itemize}") || self.at_str("\\begin{enumerate}") {
                self.skip_command_token(); // \begin
                let env = self.read_group_or_empty();
                let sub_ordered = env.trim() == "enumerate";
                nested.push(self.parse_list(sub_ordered));
                continue;
            }
            // Accumulate raw item text up to the next control point.
            if let Some(ch) =
                self.next_text_char(&["\\item", "\\begin{itemize}", "\\begin{enumerate}", "\\end{"])
            {
                item_text.push_str(&ch);
            } else {
                break;
            }
        }
        if seen_item {
            items.push(make_item(item_text, nested));
        }
        Block::List { ordered, items }
    }

    // ---- tabular ----

    fn parse_tabular(&mut self) -> Block {
        let _ = self.read_optional(); // optional positional arg [t]/[b]
        let _spec = self.read_group_or_empty(); // column spec
        let start = self.pos;
        let end = self.src[start..]
            .find("\\end{tabular}")
            .map(|e| start + e)
            .unwrap_or(self.bytes.len());
        let body = &self.src[start..end];
        self.pos = (end + "\\end{tabular}".len()).min(self.bytes.len());

        let mut rows: Vec<Vec<Vec<Inline>>> = Vec::new();
        for raw_row in split_rows(body) {
            let row = raw_row.trim();
            if row.is_empty() {
                continue;
            }
            let cells: Vec<Vec<Inline>> = split_cells(row)
                .into_iter()
                .map(|c| parse_inlines(&flatten_multicolumn(c.trim())))
                .collect();
            if cells.iter().all(|c| c.is_empty()) {
                continue;
            }
            rows.push(cells);
        }
        let headers = if rows.is_empty() {
            Vec::new()
        } else {
            rows.remove(0)
        };
        Block::Table { headers, rows }
    }

    // ---- verbatim / code ----

    fn parse_verbatim(&mut self, env: &str) -> Block {
        let close = format!("\\end{{{env}}}");
        let _ = self.read_optional(); // lstlisting may carry [options]
                                      // Skip a single leading newline right after \begin{env}.
        if self.at_str("\r\n") {
            self.pos += 2;
        } else if self.bytes.get(self.pos) == Some(&b'\n') {
            self.pos += 1;
        }
        let start = self.pos;
        let end = self.src[start..]
            .find(&close)
            .map(|e| start + e)
            .unwrap_or(self.bytes.len());
        let mut code = self.src[start..end].to_string();
        // Drop a single trailing newline before \end.
        if code.ends_with('\n') {
            code.pop();
            if code.ends_with('\r') {
                code.pop();
            }
        }
        self.pos = (end + close.len()).min(self.bytes.len());
        Block::CodeBlock {
            lang: None,
            code,
            spans: Vec::new(),
        }
    }

    // ---- figure ----

    fn parse_figure(&mut self) -> Block {
        let _ = self.read_optional(); // [htbp] placement
        let start = self.pos;
        let end = self.src[start..]
            .find("\\end{figure}")
            .map(|e| start + e)
            .unwrap_or(self.bytes.len());
        let body = self.src[start..end].to_string();
        self.pos = (end + "\\end{figure}".len()).min(self.bytes.len());

        let url = find_includegraphics(&body).unwrap_or_default();
        Block::Image {
            url,
            alt: String::new(),
        }
    }

    fn parse_quote(&mut self, env: &str) -> Block {
        let inner = self.blocks_until(Some(env));
        Block::Blockquote(inner.into_iter().map(|e| e.block).collect())
    }

    // ---- paragraph ----

    fn parse_paragraph(&mut self) -> Option<Block> {
        let mut buf = String::new();
        loop {
            if self.pos >= self.bytes.len() {
                break;
            }
            // Blank line terminates the paragraph.
            if self.at_blank_line() {
                break;
            }
            if self.bytes[self.pos] == b'%' {
                self.skip_comment();
                continue;
            }
            // A block-level construct mid-text ends the paragraph so it can be
            // handled as its own block on the next loop.
            if self.at_block_boundary() {
                break;
            }
            if let Some(ch) = self.next_text_char(&[]) {
                buf.push_str(&ch);
            } else {
                break;
            }
        }
        let inlines = parse_inlines(buf.trim());
        if inlines.is_empty() {
            None
        } else {
            Some(Block::Paragraph(inlines))
        }
    }

    /// True if the upcoming command/environment must start its own block.
    fn at_block_boundary(&self) -> bool {
        if self.at_str("$$") || self.at_str("\\[") {
            return true;
        }
        if self.at_str("\\begin{") {
            // Peek the env name; math/inline-able envs handled at block level.
            return true;
        }
        if self.at_str("\\end{") {
            return true;
        }
        for kw in [
            "\\section",
            "\\subsection",
            "\\subsubsection",
            "\\title",
            "\\maketitle",
            "\\item",
            "\\hrule",
        ] {
            if self.at_str(kw) {
                return true;
            }
        }
        false
    }

    // ---- low-level scanning ----

    /// Reads one logical text unit (an escape sequence, a control word with its
    /// trailing whitespace, or a single char), appending nothing block-level.
    /// Stops (returns None) when at one of `stops`. Used to slurp raw runs that
    /// `parse_inlines` later interprets.
    fn next_text_char(&mut self, stops: &[&str]) -> Option<String> {
        if self.pos >= self.bytes.len() {
            return None;
        }
        for s in stops {
            if self.at_str(s) {
                return None;
            }
        }
        let c = self.bytes[self.pos];
        if c == b'\\' {
            // Keep the whole control sequence verbatim for the inline pass.
            let start = self.pos;
            self.pos += 1;
            if self.pos < self.bytes.len() {
                let nc = self.bytes[self.pos];
                if nc.is_ascii_alphabetic() {
                    while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_alphabetic()
                    {
                        self.pos += 1;
                    }
                } else {
                    // Escaped symbol or \\ — take exactly one following byte.
                    self.pos += 1;
                }
            }
            // Include the argument group(s) so inline parser sees them whole.
            return Some(self.src[start..self.pos].to_string());
        }
        // Advance one UTF-8 char.
        let ch_len = utf8_len(c);
        let end = (self.pos + ch_len).min(self.bytes.len());
        let s = self.src[self.pos..end].to_string();
        self.pos = end;
        Some(s)
    }

    fn peek_command_name(&self) -> String {
        let mut i = self.pos;
        if self.bytes.get(i) != Some(&b'\\') {
            return String::new();
        }
        i += 1;
        let start = i;
        while i < self.bytes.len() && self.bytes[i].is_ascii_alphabetic() {
            i += 1;
        }
        self.src[start..i].to_string()
    }

    /// Consumes `\name` plus an optional trailing `*`.
    fn skip_command_token(&mut self) {
        if self.bytes.get(self.pos) != Some(&b'\\') {
            return;
        }
        self.pos += 1;
        if self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_alphabetic() {
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_alphabetic() {
                self.pos += 1;
            }
        } else if self.pos < self.bytes.len() {
            // single-char control symbol
            self.pos += 1;
        }
        if self.bytes.get(self.pos) == Some(&b'*') {
            self.pos += 1;
        }
    }

    /// Reads a balanced `{...}` group (leading whitespace allowed), returning
    /// inner text. Returns empty string if no group is present.
    fn read_group_or_empty(&mut self) -> String {
        self.skip_inline_ws();
        if self.bytes.get(self.pos) != Some(&b'{') {
            return String::new();
        }
        self.pos += 1;
        let start = self.pos;
        let mut depth = 1;
        while self.pos < self.bytes.len() {
            let c = self.bytes[self.pos];
            if c == b'\\' {
                self.pos = (self.pos + 2).min(self.bytes.len());
                continue;
            }
            if c == b'{' {
                depth += 1;
            } else if c == b'}' {
                depth -= 1;
                if depth == 0 {
                    let s = self.src[start..self.pos].to_string();
                    self.pos += 1;
                    return s;
                }
            }
            self.pos += 1;
        }
        self.src[start..self.pos].to_string()
    }

    /// Reads an optional `[...]` argument, returning its inner text if present.
    fn read_optional(&mut self) -> Option<String> {
        self.skip_inline_ws();
        if self.bytes.get(self.pos) != Some(&b'[') {
            return None;
        }
        self.pos += 1;
        let start = self.pos;
        while self.pos < self.bytes.len() && self.bytes[self.pos] != b']' {
            self.pos += 1;
        }
        let s = self.src[start..self.pos].to_string();
        if self.pos < self.bytes.len() {
            self.pos += 1;
        }
        Some(s)
    }

    fn at_str(&self, s: &str) -> bool {
        self.src[self.pos.min(self.src.len())..].starts_with(s)
    }

    fn at_end_env(&self, env: &str) -> bool {
        let m = format!("\\end{{{env}}}");
        self.at_str(&m)
    }

    fn consume_end_env(&mut self, env: &str) {
        let m = format!("\\end{{{env}}}");
        if self.at_str(&m) {
            self.pos += m.len();
        }
    }

    fn at_blank_line(&self) -> bool {
        // At current position: a run of inline ws then two newlines.
        let mut i = self.pos;
        // current char must be a newline for a blank-line break
        if self.bytes.get(i) != Some(&b'\n') && self.bytes.get(i) != Some(&b'\r') {
            return false;
        }
        // consume one EOL
        if self.bytes.get(i) == Some(&b'\r') {
            i += 1;
        }
        if self.bytes.get(i) == Some(&b'\n') {
            i += 1;
        }
        // skip inline ws then require another EOL
        while i < self.bytes.len() && (self.bytes[i] == b' ' || self.bytes[i] == b'\t') {
            i += 1;
        }
        matches!(self.bytes.get(i), Some(b'\n') | Some(b'\r'))
    }

    fn skip_blank(&mut self) {
        self.skip_ws_and_comments();
    }

    fn skip_ws_and_comments(&mut self) {
        loop {
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
                self.pos += 1;
            }
            if self.bytes.get(self.pos) == Some(&b'%') {
                self.skip_comment();
                continue;
            }
            break;
        }
    }

    /// Skips comment lines (and the leading whitespace before a comment), but
    /// preserves whitespace that precedes real content.
    fn skip_comments(&mut self) {
        loop {
            let save = self.pos;
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
                self.pos += 1;
            }
            if self.bytes.get(self.pos) == Some(&b'%') {
                self.skip_comment();
                continue;
            }
            self.pos = save;
            break;
        }
    }

    fn skip_inline_ws(&mut self) {
        while matches!(
            self.bytes.get(self.pos),
            Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r')
        ) {
            self.pos += 1;
        }
    }

    fn skip_comment(&mut self) {
        // Consume `%` … to end of line (caller guarantees it is unescaped).
        while self.pos < self.bytes.len() && self.bytes[self.pos] != b'\n' {
            self.pos += 1;
        }
        if self.pos < self.bytes.len() {
            self.pos += 1;
        }
    }
}

// ---- list item helpers ----

fn make_item(text: String, mut nested: Vec<Block>) -> ListItem {
    let mut blocks: Vec<Block> = Vec::new();
    // LaTeX collapses whitespace runs (incl. newlines/indentation) to one space.
    let collapsed = collapse_ws(text.trim());
    let inlines = parse_inlines(&collapsed);
    if !inlines.is_empty() {
        blocks.push(Block::Paragraph(inlines));
    }
    blocks.append(&mut nested);
    ListItem { task: None, blocks }
}

/// Collapse runs of whitespace to a single space (LaTeX inter-word spacing).
fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_ws = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_ws {
                out.push(' ');
            }
            prev_ws = true;
        } else {
            out.push(c);
            prev_ws = false;
        }
    }
    out
}

// ---- table helpers ----

/// Split tabular body into rows on unescaped `\\`.
fn split_rows(body: &str) -> Vec<String> {
    let bytes = body.as_bytes();
    let mut rows = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && bytes.get(i + 1) == Some(&b'\\') {
            rows.push(body[start..i].to_string());
            i += 2;
            // Skip an optional [len] after \\.
            while bytes.get(i) == Some(&b' ') {
                i += 1;
            }
            if bytes.get(i) == Some(&b'[') {
                while i < bytes.len() && bytes[i] != b']' {
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1;
                }
            }
            start = i;
            continue;
        }
        if bytes[i] == b'\\' {
            i += 2; // skip escaped command/char
            continue;
        }
        i += 1;
    }
    if start < body.len() {
        rows.push(body[start..].to_string());
    }
    rows
}

/// Split a row into cells on unescaped `&`, dropping `\hline`.
fn split_cells(row: &str) -> Vec<String> {
    let row = row.replace("\\hline", "");
    let bytes = row.as_bytes();
    let mut cells = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'&' {
            cells.push(row[start..i].to_string());
            start = i + 1;
        }
        i += 1;
    }
    cells.push(row[start..].to_string());
    cells
}

/// `\multicolumn{n}{a}{text}` → its `text`, discarding span/alignment.
fn flatten_multicolumn(cell: &str) -> String {
    let Some(idx) = cell.find("\\multicolumn") else {
        return cell.to_string();
    };
    let rest = &cell[idx + "\\multicolumn".len()..];
    // Read three brace groups; return the third.
    let mut s = rest;
    let mut last = String::new();
    for _ in 0..3 {
        let g = read_leading_group(s);
        match g {
            Some((inner, after)) => {
                last = inner;
                s = after;
            }
            None => break,
        }
    }
    let prefix = &cell[..idx];
    format!("{prefix}{last}{s}")
}

/// Reads a leading `{...}` group from `s` (after optional whitespace).
fn read_leading_group(s: &str) -> Option<(String, &str)> {
    let t = s.trim_start();
    let b = t.as_bytes();
    if b.first() != Some(&b'{') {
        return None;
    }
    let mut depth = 1;
    let mut i = 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth -= 1;
                i += 1;
                if depth == 0 {
                    return Some((t[1..i - 1].to_string(), &t[i..]));
                }
            }
            _ => i += 1,
        }
    }
    Some((t[1..].to_string(), ""))
}

// ---- figure helper ----

fn find_includegraphics(body: &str) -> Option<String> {
    let idx = body.find("\\includegraphics")?;
    let rest = &body[idx + "\\includegraphics".len()..];
    let rest = skip_optional(rest);
    read_leading_group(rest).map(|(inner, _)| inner.trim().to_string())
}

fn skip_optional(s: &str) -> &str {
    let t = s.trim_start();
    if !t.starts_with('[') {
        return t;
    }
    match t.find(']') {
        Some(i) => &t[i + 1..],
        None => t,
    }
}

// ---- math body cleanup ----

/// Strips `\label{..}` and `\tag{..}` from a math body (they'd error in
/// pulldown-latex).
fn strip_math_meta(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if let Some(after) = match_macro_with_arg(&body[i..], "\\label")
            .or_else(|| match_macro_with_arg(&body[i..], "\\tag"))
        {
            i += after;
            continue;
        }
        let ch_len = utf8_len(bytes[i]);
        out.push_str(&body[i..(i + ch_len).min(body.len())]);
        i += ch_len;
    }
    out
}

/// If `s` begins with `macro{...}` (optional ws between), returns the number of
/// bytes the whole `macro{...}` occupies.
fn match_macro_with_arg(s: &str, name: &str) -> Option<usize> {
    if !s.starts_with(name) {
        return None;
    }
    let rest = &s[name.len()..];
    let trimmed = rest.trim_start();
    let ws = rest.len() - trimmed.len();
    let (_, after) = read_leading_group(trimmed)?;
    Some(name.len() + ws + (trimmed.len() - after.len()))
}

// ---- inline parsing ----

/// Parses a raw text run into inline nodes.
pub fn parse_inlines(s: &str) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut text = String::new();
    let bytes = s.as_bytes();
    let mut i = 0;

    macro_rules! flush {
        () => {
            if !text.is_empty() {
                out.push(Inline::Text(std::mem::take(&mut text)));
            }
        };
    }

    while i < s.len() {
        let c = bytes[i];
        if c == b'%' {
            // Comment to end of line.
            while i < s.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // Inline math literal `$...$` (not display $$).
        if c == b'$' && bytes.get(i + 1) != Some(&b'$') {
            if let Some(end_rel) = find_unescaped_dollar(&s[i + 1..]) {
                let end = i + 1 + end_rel;
                text.push_str(&s[i..=end]); // keep delimiters
                i = end + 1;
                continue;
            }
        }
        // Inline math `\(...\)` literal → re-emit as `$...$`.
        if s[i..].starts_with("\\(") {
            if let Some(rel) = s[i + 2..].find("\\)") {
                let inner = &s[i + 2..i + 2 + rel];
                text.push('$');
                text.push_str(inner);
                text.push('$');
                i = i + 2 + rel + 2;
                continue;
            }
        }
        if c == b'~' {
            text.push(' ');
            i += 1;
            continue;
        }
        if c == b'\\' {
            let (inl, consumed, literal) = parse_control(&s[i..]);
            if let Some(lit) = literal {
                text.push_str(&lit);
            } else if let Some(node) = inl {
                flush!();
                out.push(node);
            }
            i += consumed;
            continue;
        }
        if c == b'{' || c == b'}' {
            // Bare braces are grouping in plain text — drop them.
            i += 1;
            continue;
        }
        let ch_len = utf8_len(c);
        text.push_str(&s[i..(i + ch_len).min(s.len())]);
        i += ch_len;
    }
    flush!();
    out
}

/// Handles a control sequence at the start of `s`. Returns
/// `(maybe_inline_node, bytes_consumed, maybe_literal_text)`. When `literal`
/// is Some it should be appended to the running text buffer instead.
fn parse_control(s: &str) -> (Option<Inline>, usize, Option<String>) {
    let bytes = s.as_bytes();
    debug_assert_eq!(bytes[0], b'\\');
    // Escaped symbol: \$ \% \& \# \_ \{ \} and \\ line break.
    if let Some(&c) = bytes.get(1) {
        match c {
            b'$' | b'%' | b'&' | b'#' | b'_' | b'{' | b'}' => {
                return (None, 2, Some((c as char).to_string()));
            }
            b'\\' => return (None, 2, Some("\n".to_string())),
            b',' | b' ' => return (None, 2, Some(" ".to_string())),
            _ => {}
        }
    }
    let name = control_word(s);
    let after_name = 1 + name.len();
    let consume_to = |after: &str| s.len() - after.len();

    match name.as_str() {
        "textbf" | "bf" | "textsf" => {
            let (arg, after) = take_arg(&s[after_name..]);
            (
                Some(Inline::Strong(parse_inlines(&arg))),
                consume_to(after),
                None,
            )
        }
        "textit" | "emph" | "it" => {
            let (arg, after) = take_arg(&s[after_name..]);
            (
                Some(Inline::Emph(parse_inlines(&arg))),
                consume_to(after),
                None,
            )
        }
        "texttt" => {
            let (arg, after) = take_arg(&s[after_name..]);
            (
                Some(Inline::Code(strip_inline_to_text(&arg))),
                consume_to(after),
                None,
            )
        }
        "sout" => {
            let (arg, after) = take_arg(&s[after_name..]);
            (
                Some(Inline::Strike(parse_inlines(&arg))),
                consume_to(after),
                None,
            )
        }
        "verb" => {
            // \verb<delim>...<delim>
            if let Some(&d) = bytes.get(after_name) {
                let rest = &s[after_name + 1..];
                if let Some(end) = rest.find(d as char) {
                    let code = rest[..end].to_string();
                    let total = after_name + 1 + end + 1;
                    return (Some(Inline::Code(code)), total, None);
                }
            }
            (None, after_name, None)
        }
        "href" => {
            let (url, a1) = take_arg(&s[after_name..]);
            let (txt, a2) = take_arg(a1);
            (
                Some(Inline::Link {
                    url: url.trim().to_string(),
                    children: parse_inlines(&txt),
                }),
                consume_to(a2),
                None,
            )
        }
        "url" => {
            let (url, after) = take_arg(&s[after_name..]);
            let url = url.trim().to_string();
            (
                Some(Inline::Link {
                    children: vec![Inline::Text(url.clone())],
                    url,
                }),
                consume_to(after),
                None,
            )
        }
        "ref" | "pageref" => {
            let (k, after) = take_arg(&s[after_name..]);
            (None, consume_to(after), Some(k.trim().to_string()))
        }
        "eqref" => {
            let (k, after) = take_arg(&s[after_name..]);
            (None, consume_to(after), Some(format!("({})", k.trim())))
        }
        "cite" | "citep" | "citet" => {
            let (k, after) = take_arg(&s[after_name..]);
            (None, consume_to(after), Some(format!("[{}]", k.trim())))
        }
        "footnote" => {
            let (txt, after) = take_arg(&s[after_name..]);
            (None, consume_to(after), Some(format!(" ({})", txt.trim())))
        }
        "paragraph" | "subparagraph" => {
            let (txt, after) = take_arg(&s[after_name..]);
            (
                Some(Inline::Strong(parse_inlines(&txt))),
                consume_to(after),
                None,
            )
        }
        // Cross-ref anchors / index entries: consume the braced arg, emit nothing.
        "label" | "index" => {
            let (_arg, after) = take_arg(&s[after_name..]);
            (None, consume_to(after), None)
        }
        // No-content layout commands: drop the command entirely.
        "maketitle" | "centering" | "noindent" | "bigskip" | "medskip" | "smallskip"
        | "clearpage" | "newpage" | "vfill" | "hfill" => (None, after_name, None),
        "" => {
            // A lone backslash followed by punctuation already handled above;
            // here just drop it.
            (None, 1, None)
        }
        _ => {
            // Unknown macro: if it has a brace arg, surface the arg as text;
            // otherwise drop the command (and any trailing star).
            let mut after = &s[after_name..];
            if bytes.get(after_name) == Some(&b'*') {
                after = &s[after_name + 1..];
            }
            let trimmed = after.trim_start();
            if trimmed.starts_with('{') {
                let (arg, rest) = take_arg(after);
                // Re-parse the arg as inlines, but we must return a single
                // node; wrap multiple into… we only return literal text here.
                let inner = inline_to_string(&parse_inlines(&arg));
                (None, consume_to(rest), Some(inner))
            } else {
                (None, consume_to(after), None)
            }
        }
    }
}

fn control_word(s: &str) -> String {
    let b = s.as_bytes();
    let mut i = 1;
    while i < b.len() && b[i].is_ascii_alphabetic() {
        i += 1;
    }
    s[1..i].to_string()
}

/// Reads a `{...}` argument (skipping leading ws). Returns inner + remainder.
/// If no brace, takes the next single token (char or control word).
fn take_arg(s: &str) -> (String, &str) {
    let t = s.trim_start();
    if let Some((inner, after)) = read_leading_group(t) {
        return (inner, after);
    }
    // No group: single following char (common with \bf-style — but those are
    // switches; here we just grab nothing to stay safe).
    (String::new(), t)
}

fn strip_inline_to_text(s: &str) -> String {
    inline_to_string(&parse_inlines(s))
}

fn find_unescaped_dollar(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' {
            i += 2;
            continue;
        }
        if b[i] == b'$' {
            return Some(i);
        }
        i += 1;
    }
    None
}

// ---- shared helpers ----

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

fn utf8_len(first: u8) -> usize {
    if first < 0x80 {
        1
    } else if first >> 5 == 0b110 {
        2
    } else if first >> 4 == 0b1110 {
        3
    } else {
        4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocks(src: &str) -> Vec<Block> {
        parse(src).0.into_iter().map(|(_, b)| b).collect()
    }

    #[test]
    fn section_heading() {
        let b = blocks("\\section{Intro}");
        assert_eq!(b.len(), 1);
        match &b[0] {
            Block::Heading { level, inlines, .. } => {
                assert_eq!(*level, 1);
                assert_eq!(inlines, &vec![Inline::Text("Intro".into())]);
            }
            other => panic!("expected heading, got {other:?}"),
        }
    }

    #[test]
    fn subsection_levels() {
        assert!(matches!(
            blocks("\\subsection{A}")[0],
            Block::Heading { level: 2, .. }
        ));
        assert!(matches!(
            blocks("\\subsubsection{A}")[0],
            Block::Heading { level: 3, .. }
        ));
        // star form
        assert!(matches!(
            blocks("\\section*{A}")[0],
            Block::Heading { level: 1, .. }
        ));
    }

    #[test]
    fn textbf_strong() {
        let b = blocks("This is \\textbf{bold} text.");
        match &b[0] {
            Block::Paragraph(inl) => {
                assert!(inl.iter().any(|i| matches!(i, Inline::Strong(c)
                    if c == &vec![Inline::Text("bold".into())])));
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn itemize_unordered() {
        let b = blocks("\\begin{itemize}\\item a\\item b\\end{itemize}");
        match &b[0] {
            Block::List { ordered, items } => {
                assert!(!ordered);
                assert_eq!(items.len(), 2);
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn enumerate_ordered() {
        let b = blocks("\\begin{enumerate}\\item a\\item b\\end{enumerate}");
        match &b[0] {
            Block::List { ordered, items } => {
                assert!(*ordered);
                assert_eq!(items.len(), 2);
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn nested_list() {
        let src = "\\begin{itemize}\\item a\\begin{enumerate}\\item x\\item y\\end{enumerate}\\item b\\end{itemize}";
        let b = blocks(src);
        match &b[0] {
            Block::List { items, .. } => {
                assert_eq!(items.len(), 2);
                // first item should contain a nested ordered list block
                let has_nested = items[0]
                    .blocks
                    .iter()
                    .any(|bl| matches!(bl, Block::List { ordered: true, .. }));
                assert!(has_nested, "first item should nest an enumerate");
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn dollar_display_math() {
        let b = blocks("$$x^2$$");
        match &b[0] {
            Block::Diagram { kind, source, .. } => {
                assert_eq!(*kind, DiagramKind::Math);
                assert_eq!(source, "x^2");
            }
            other => panic!("expected math diagram, got {other:?}"),
        }
    }

    #[test]
    fn align_env_preserves_wrapper() {
        let b = blocks("\\begin{align} a &= b \\\\ c &= d \\end{align}");
        match &b[0] {
            Block::Diagram { kind, source, .. } => {
                assert_eq!(*kind, DiagramKind::Math);
                assert!(
                    source.contains("\\begin{align}"),
                    "wrapper must be preserved, got: {source}"
                );
                assert!(source.contains("\\end{align}"));
            }
            other => panic!("expected math diagram, got {other:?}"),
        }
    }

    #[test]
    fn bracket_math_trimmed() {
        let b = blocks("\\[ y \\]");
        match &b[0] {
            Block::Diagram { kind, source, .. } => {
                assert_eq!(*kind, DiagramKind::Math);
                assert_eq!(source, "y");
            }
            other => panic!("expected math diagram, got {other:?}"),
        }
    }

    #[test]
    fn inline_math_literal() {
        let b = blocks("Energy $x$ here.");
        match &b[0] {
            Block::Paragraph(inl) => {
                let joined = inline_to_string(inl);
                assert!(joined.contains("$x$"), "got: {joined}");
                // Must be a Text node, not a diagram/code.
                assert!(inl.iter().all(|i| matches!(i, Inline::Text(_))));
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn tabular_table() {
        let b = blocks("\\begin{tabular}{ll} a & b \\\\ c & d \\end{tabular}");
        match &b[0] {
            Block::Table { headers, rows } => {
                assert_eq!(inline_to_string(&headers[0]), "a");
                assert_eq!(inline_to_string(&headers[1]), "b");
                assert_eq!(rows.len(), 1);
                assert_eq!(inline_to_string(&rows[0][0]), "c");
                assert_eq!(inline_to_string(&rows[0][1]), "d");
            }
            other => panic!("expected table, got {other:?}"),
        }
    }

    #[test]
    fn figure_includegraphics() {
        let src = "\\begin{figure}\\centering\\includegraphics[width=0.5\\textwidth]{fig.png}\\caption{x}\\end{figure}";
        let b = blocks(src);
        let img = b.iter().find_map(|bl| match bl {
            Block::Image { url, .. } => Some(url.clone()),
            _ => None,
        });
        assert_eq!(img.as_deref(), Some("fig.png"));
    }

    #[test]
    fn includegraphics_no_opts() {
        let b = blocks("\\begin{figure}\\includegraphics{fig.png}\\end{figure}");
        assert!(b
            .iter()
            .any(|bl| matches!(bl, Block::Image { url, .. } if url == "fig.png")));
    }

    #[test]
    fn comment_stripped_and_escaped_dollar() {
        let b = blocks("Cost is \\$5 % this is a comment\nper unit.");
        let joined = match &b[0] {
            Block::Paragraph(inl) => inline_to_string(inl),
            other => panic!("expected paragraph, got {other:?}"),
        };
        assert!(joined.contains("$5"), "got: {joined}");
        assert!(!joined.contains("comment"), "comment leaked: {joined}");
    }

    #[test]
    fn unknown_macro_graceful() {
        let b = blocks("Start \\foobar{hello} end.");
        let joined = match &b[0] {
            Block::Paragraph(inl) => inline_to_string(inl),
            other => panic!("expected paragraph, got {other:?}"),
        };
        assert!(joined.contains("hello"), "got: {joined}");
    }

    #[test]
    fn skips_preamble_before_document() {
        let src = "\\documentclass{article}\n\\usepackage{amsmath}\n\\begin{document}\n\\section{Body}\n\\end{document}";
        let b = blocks(src);
        assert_eq!(b.len(), 1);
        assert!(matches!(&b[0], Block::Heading { level: 1, .. }));
    }

    #[test]
    fn fragment_without_document() {
        // No \begin{document}: parse whole input.
        let b = blocks("\\section{Frag}\nbody text");
        assert!(b.iter().any(|bl| matches!(bl, Block::Heading { .. })));
        assert!(b.iter().any(|bl| matches!(bl, Block::Paragraph(_))));
    }

    #[test]
    fn href_link() {
        let b = blocks("See \\href{http://x.com}{click} now.");
        let link = match &b[0] {
            Block::Paragraph(inl) => inl.iter().find_map(|i| match i {
                Inline::Link { url, children } => Some((url.clone(), children.clone())),
                _ => None,
            }),
            other => panic!("expected paragraph, got {other:?}"),
        };
        let (url, children) = link.expect("link node");
        assert_eq!(url, "http://x.com");
        assert_eq!(children, vec![Inline::Text("click".into())]);
    }

    #[test]
    fn url_link() {
        let b = blocks("\\url{http://y.com}");
        match &b[0] {
            Block::Paragraph(inl) => {
                assert!(inl.iter().any(|i| matches!(i, Inline::Link { url, children }
                    if url == "http://y.com" && children == &vec![Inline::Text("http://y.com".into())])));
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn offsets_invariants() {
        let src = "\\section{A}\n\nFirst para.\n\n\\section{B}\n\nSecond para.\n\n$$z$$";
        let (blocks, offsets) = parse(src);
        assert_eq!(blocks.len(), offsets.len());
        assert!(
            offsets.windows(2).all(|w| w[0] <= w[1]),
            "offsets must be non-decreasing: {offsets:?}"
        );
        // Real offsets, not all zero.
        assert!(offsets.iter().any(|&o| o > 0));
    }

    #[test]
    fn offsets_skip_preamble() {
        let src = "\\documentclass{article}\n\\begin{document}\n\\section{Body}\n\\end{document}";
        let (_blocks, offsets) = parse(src);
        // The heading lives well past the preamble, so its offset is large.
        assert!(offsets[0] as usize > src.find("\\section").unwrap() - 1);
    }

    #[test]
    fn verbatim_block() {
        let src = "\\begin{verbatim}\nraw $not math$ & x\n\\end{verbatim}";
        let b = blocks(src);
        match &b[0] {
            Block::CodeBlock { lang, code, .. } => {
                assert!(lang.is_none());
                assert!(code.contains("raw $not math$"), "got: {code:?}");
            }
            other => panic!("expected code block, got {other:?}"),
        }
    }

    #[test]
    fn lstlisting_block() {
        let src = "\\begin{lstlisting}[language=Rust]\nfn main() {}\n\\end{lstlisting}";
        let b = blocks(src);
        assert!(b
            .iter()
            .any(|bl| matches!(bl, Block::CodeBlock { code, .. } if code.contains("fn main"))));
    }

    #[test]
    fn equation_env_math() {
        let b = blocks("\\begin{equation}\nE = mc^2 \\label{eq:e}\n\\end{equation}");
        match &b[0] {
            Block::Diagram { kind, source, .. } => {
                assert_eq!(*kind, DiagramKind::Math);
                assert!(source.contains("\\begin{equation}"));
                assert!(
                    !source.contains("\\label"),
                    "label must be stripped: {source}"
                );
            }
            other => panic!("expected math, got {other:?}"),
        }
    }

    #[test]
    fn texttt_inline_code() {
        let b = blocks("Run \\texttt{ls -la} now.");
        match &b[0] {
            Block::Paragraph(inl) => {
                assert!(inl
                    .iter()
                    .any(|i| matches!(i, Inline::Code(c) if c == "ls -la")));
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn verb_inline_code() {
        let b = blocks("Type \\verb|x = 1| please.");
        match &b[0] {
            Block::Paragraph(inl) => {
                assert!(inl
                    .iter()
                    .any(|i| matches!(i, Inline::Code(c) if c == "x = 1")));
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn cite_and_ref_as_text() {
        let b = blocks("See \\cite{foo} and \\ref{sec:bar}.");
        let joined = match &b[0] {
            Block::Paragraph(inl) => inline_to_string(inl),
            other => panic!("expected paragraph, got {other:?}"),
        };
        assert!(joined.contains("[foo]"), "got: {joined}");
        assert!(joined.contains("sec:bar"), "got: {joined}");
    }

    #[test]
    fn hrule_rule() {
        let b = blocks("\\hrule");
        assert!(matches!(b[0], Block::Rule));
    }

    #[test]
    fn quote_blockquote() {
        let b = blocks("\\begin{quote}A wise saying.\\end{quote}");
        match &b[0] {
            Block::Blockquote(inner) => {
                assert!(inner.iter().any(|bl| matches!(bl, Block::Paragraph(_))));
            }
            other => panic!("expected blockquote, got {other:?}"),
        }
    }

    #[test]
    fn paragraph_split_on_blank_line() {
        let b = blocks("First paragraph.\n\nSecond paragraph.");
        let paras: Vec<_> = b
            .iter()
            .filter(|bl| matches!(bl, Block::Paragraph(_)))
            .collect();
        assert_eq!(paras.len(), 2);
    }

    #[test]
    fn ids_position_distinct() {
        // Two identical rules must get distinct ids (position-mixed).
        let pairs = parse("\\hrule\n\n\\hrule").0;
        assert_eq!(pairs.len(), 2);
        assert_ne!(pairs[0].0, pairs[1].0);
    }

    #[test]
    fn never_panics_on_malformed() {
        // Unbalanced braces / stray ends must not panic.
        let _ = parse("\\textbf{unclosed \\section{a} $$ x ");
        let _ = parse("\\end{itemize} stray");
        let _ = parse("\\begin{tabular}{ll} a & b");
        let _ = parse("{{{}}}\\href{u}");
    }

    #[test]
    fn paragraph_command_strong() {
        let b = blocks("\\paragraph{Note} this matters.");
        match &b[0] {
            Block::Paragraph(inl) => {
                assert!(matches!(inl.first(), Some(Inline::Strong(_))));
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn item_preserves_word_spaces() {
        let b = blocks("\\begin{itemize}\\item alpha beta\\item gamma delta\\end{itemize}");
        match &b[0] {
            Block::List { items, .. } => {
                assert_eq!(items.len(), 2);
                assert_eq!(item_text(&items[0]), "alpha beta");
                assert_eq!(item_text(&items[1]), "gamma delta");
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    fn item_text(it: &ListItem) -> String {
        match it.blocks.first() {
            Some(Block::Paragraph(inl)) => inline_to_string(inl),
            _ => String::new(),
        }
    }

    #[test]
    fn label_dropped_from_text() {
        let b = blocks("\\label{foo} hello");
        match &b[0] {
            Block::Paragraph(inl) => {
                let joined = inline_to_string(inl);
                assert!(!joined.contains("foo"), "label key leaked: {joined:?}");
                assert_eq!(joined.trim(), "hello");
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
    }
}
