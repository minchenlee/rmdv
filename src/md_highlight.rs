//! Line-by-line markdown syntax highlighter for the in-app text editor.
//!
//! Implements iced's `text::Highlighter` trait. Tokenizes per line:
//! headings, fenced code blocks, inline code spans, emph/strong, links,
//! blockquotes, list bullets, and horizontal rules. Colors come from the
//! active `Palette` so highlighting tracks the user's theme.

use iced::advanced::text::highlighter::Format;
use iced::Color;
use std::ops::Range;

use crate::theme::Palette;

#[derive(Clone, PartialEq)]
pub struct Settings {
    pub palette: Palette,
}

#[derive(Clone, Copy)]
pub struct Highlight {
    pub color: Color,
}

impl Highlight {
    pub fn to_format(self) -> Format<iced::Font> {
        Format {
            color: Some(self.color),
            font: None,
        }
    }
}

pub struct MdHighlighter {
    palette: Palette,
    in_fence: bool,
    /// Index of the line we will highlight next.
    line: usize,
}

impl iced::advanced::text::Highlighter for MdHighlighter {
    type Settings = Settings;
    type Highlight = Highlight;
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, Highlight)>;

    fn new(settings: &Self::Settings) -> Self {
        Self {
            palette: settings.palette,
            in_fence: false,
            line: 0,
        }
    }

    fn update(&mut self, new_settings: &Self::Settings) {
        self.palette = new_settings.palette;
        self.in_fence = false;
        self.line = 0;
    }

    fn change_line(&mut self, line: usize) {
        if line < self.line {
            self.line = line;
            // Conservative: re-scan from top requires reset, but the trait
            // only re-feeds lines from `line` onward. For fence tracking
            // accuracy we'd need to remember per-line state. v1 accepts
            // that fence state can drift after edits inside a fence;
            // a full re-enter of the editor refreshes it.
            self.in_fence = false;
        }
    }

    fn current_line(&self) -> usize {
        self.line
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        let toks = tokenize_line(line, &self.palette, &mut self.in_fence);
        self.line = self.line.saturating_add(1);
        toks.into_iter()
    }
}

fn tokenize_line(line: &str, pal: &Palette, in_fence: &mut bool) -> Vec<(Range<usize>, Highlight)> {
    let mut out: Vec<(Range<usize>, Highlight)> = Vec::new();
    let trimmed = line.trim_start();

    // Fenced code block toggle.
    if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
        push(&mut out, 0..line.len(), pal.syntax.punctuation);
        *in_fence = !*in_fence;
        return out;
    }
    if *in_fence {
        push(&mut out, 0..line.len(), pal.syntax.string);
        return out;
    }

    // Headings.
    if let Some(level) = heading_level(trimmed) {
        let lead = line.len() - trimmed.len();
        let hash_end = lead + level;
        push(&mut out, lead..hash_end, pal.syntax.punctuation);
        push(&mut out, hash_end..line.len(), pal.accent);
        return out;
    }

    // Horizontal rule.
    if is_hr(trimmed) {
        push(&mut out, 0..line.len(), pal.syntax.punctuation);
        return out;
    }

    // Blockquote.
    if trimmed.starts_with('>') {
        let lead = line.len() - trimmed.len();
        push(&mut out, lead..lead + 1, pal.syntax.comment);
        scan_inline(line, lead + 1..line.len(), pal, &mut out);
        return out;
    }

    // List bullet.
    if let Some(marker_end) = list_marker_end(line) {
        push(&mut out, 0..marker_end, pal.syntax.keyword);
        scan_inline(line, marker_end..line.len(), pal, &mut out);
        return out;
    }

    scan_inline(line, 0..line.len(), pal, &mut out);
    out
}

fn scan_inline(
    line: &str,
    range: Range<usize>,
    pal: &Palette,
    out: &mut Vec<(Range<usize>, Highlight)>,
) {
    let bytes = line.as_bytes();
    let mut i = range.start;
    while i < range.end {
        let b = bytes[i];
        // Inline code span.
        if b == b'`' {
            if let Some(end) = find_byte(bytes, i + 1, range.end, b'`') {
                push(out, i..end + 1, pal.syntax.string);
                i = end + 1;
                continue;
            }
        }
        // Strong **...** or __...__
        if (b == b'*' || b == b'_') && i + 1 < range.end && bytes[i + 1] == b {
            let marker = b;
            if let Some(end) = find_double(bytes, i + 2, range.end, marker) {
                push(out, i..end + 2, pal.syntax.keyword);
                i = end + 2;
                continue;
            }
        }
        // Emph *...* or _..._
        if b == b'*' || b == b'_' {
            if let Some(end) = find_byte(bytes, i + 1, range.end, b) {
                if end > i + 1 {
                    push(out, i..end + 1, pal.syntax.type_);
                    i = end + 1;
                    continue;
                }
            }
        }
        // Link [text](url) or image ![alt](url).
        if b == b'[' || (b == b'!' && i + 1 < range.end && bytes[i + 1] == b'[') {
            let start = i;
            let after_bang = if b == b'!' { i + 1 } else { i };
            if let Some(close) = find_byte(bytes, after_bang + 1, range.end, b']') {
                if close + 1 < range.end && bytes[close + 1] == b'(' {
                    if let Some(end_url) = find_byte(bytes, close + 2, range.end, b')') {
                        push(out, start..close + 1, pal.syntax.function);
                        push(out, close + 1..end_url + 1, pal.syntax.string);
                        i = end_url + 1;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
}

fn push(out: &mut Vec<(Range<usize>, Highlight)>, range: Range<usize>, color: Color) {
    if range.start < range.end {
        out.push((range, Highlight { color }));
    }
}

fn find_byte(bytes: &[u8], start: usize, end: usize, target: u8) -> Option<usize> {
    let mut i = start;
    while i < end {
        if bytes[i] == target {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_double(bytes: &[u8], start: usize, end: usize, target: u8) -> Option<usize> {
    let mut i = start;
    while i + 1 < end {
        if bytes[i] == target && bytes[i + 1] == target {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn heading_level(line: &str) -> Option<usize> {
    let mut n = 0usize;
    for ch in line.chars() {
        if ch == '#' {
            n += 1
        } else {
            break;
        }
    }
    if n == 0 || n > 6 {
        return None;
    }
    let rest = &line[n..];
    if rest.is_empty() || rest.starts_with(' ') {
        Some(n)
    } else {
        None
    }
}

fn is_hr(line: &str) -> bool {
    let s = line.trim();
    (s.len() >= 3) && s.chars().all(|c| c == '-' || c == '*' || c == '_')
}

fn list_marker_end(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let b = bytes[i];
    if matches!(b, b'-' | b'*' | b'+') && i + 1 < bytes.len() && bytes[i + 1] == b' ' {
        return Some(i + 2);
    }
    // Ordered list: digits then "." or ")" then space.
    let mut j = i;
    while j < bytes.len() && bytes[j].is_ascii_digit() {
        j += 1;
    }
    if j > i
        && j + 1 < bytes.len()
        && (bytes[j] == b'.' || bytes[j] == b')')
        && bytes[j + 1] == b' '
    {
        return Some(j + 2);
    }
    None
}
