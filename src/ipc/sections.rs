use crate::ast::{Block, Inline};
use crate::ipc::lines::build_byte_to_line;
use crate::parser;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Section {
    pub level: u8,
    pub title: String,
    pub path: String,
    pub line: u32,
}

pub fn list_sections(src: &str) -> Vec<Section> {
    let (blocks, offsets) = parser::parse(src);
    let table = build_byte_to_line(src);
    let mut stack: Vec<(u8, String)> = Vec::new();
    let mut out = Vec::new();
    for (i, (_, block)) in blocks.iter().enumerate() {
        if let Block::Heading { level, inlines, .. } = block {
            while stack.last().is_some_and(|(l, _)| *l >= *level) {
                stack.pop();
            }
            let title = inline_text(inlines);
            stack.push((*level, title.clone()));
            let path = stack.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join("/");
            let line = table.line_for_byte(offsets[i] as usize);
            out.push(Section { level: *level, title, path, line });
        }
    }
    out
}

/// Find the first section whose path ends with the given path (segment-wise
/// suffix match). Bare title `"Setup"` matches the first heading titled
/// "Setup"; `"Install/Setup"` matches the first whose tail is
/// `Install/Setup`.
pub fn resolve_section_path<'a>(needle: &str, sections: &'a [Section]) -> Option<&'a Section> {
    let needle_segs: Vec<&str> = needle.split('/').filter(|s| !s.is_empty()).collect();
    if needle_segs.is_empty() {
        return None;
    }
    sections.iter().find(|s| {
        let hay: Vec<&str> = s.path.split('/').collect();
        hay.len() >= needle_segs.len()
            && hay[hay.len() - needle_segs.len()..] == needle_segs[..]
    })
}

fn inline_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for i in inlines {
        push_inline(i, &mut out);
    }
    out
}

fn push_inline(i: &Inline, out: &mut String) {
    match i {
        Inline::Text(s) | Inline::Code(s) => out.push_str(s),
        Inline::Emph(c) | Inline::Strong(c) | Inline::Strike(c) => {
            for x in c { push_inline(x, out); }
        }
        Inline::Link { children, .. } => {
            for x in children { push_inline(x, out); }
        }
    }
}
