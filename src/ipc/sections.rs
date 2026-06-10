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

/// Markdown sections (CLI/IPC default). For type-aware parsing (`.tex`), use
/// [`list_sections_for`].
pub fn list_sections(src: &str) -> Vec<Section> {
    list_sections_for(src, false)
}

/// Build the heading outline, dispatching to the LaTeX parser when `is_tex` so
/// `.tex` documents (whose AST is built by `crate::tex::parse`) produce the same
/// headings the body renders — markdown-parsing raw LaTeX yields none.
pub fn list_sections_for(src: &str, is_tex: bool) -> Vec<Section> {
    let (blocks, offsets) = if is_tex {
        crate::tex::parse(src)
    } else {
        parser::parse(src)
    };
    let table = build_byte_to_line(src);
    list_sections_from_ast(&blocks, &offsets, &table)
}

/// Same outline as [`list_sections_for`], but from an already-parsed AST so
/// callers that just parsed the document (e.g. `load_ast_from_source`) don't
/// pay a second full parse + byte-to-line scan.
pub fn list_sections_from_ast(
    blocks: &[(crate::ast::BlockId, Block)],
    offsets: &[u32],
    table: &crate::ipc::lines::ByteToLine,
) -> Vec<Section> {
    let mut stack: Vec<(u8, String)> = Vec::new();
    let mut out = Vec::new();
    for (i, (_, block)) in blocks.iter().enumerate() {
        if let Block::Heading { level, inlines, .. } = block {
            while stack.last().is_some_and(|(l, _)| *l >= *level) {
                stack.pop();
            }
            let title = inline_text(inlines);
            stack.push((*level, title.clone()));
            let path = stack
                .iter()
                .map(|(_, t)| t.as_str())
                .collect::<Vec<_>>()
                .join("/");
            let line = table.line_for_byte(offsets[i] as usize);
            out.push(Section {
                level: *level,
                title,
                path,
                line,
            });
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
        hay.len() >= needle_segs.len() && hay[hay.len() - needle_segs.len()..] == needle_segs[..]
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
            for x in c {
                push_inline(x, out);
            }
        }
        Inline::Link { children, .. } => {
            for x in children {
                push_inline(x, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_ast_matches_from_source() {
        let md = "# A\ntext\n\n```rust\nfn x() {}\n```\n\n## B\n\n### C\nmore\n\n## D";
        let (blocks, offsets) = parser::parse(md);
        let table = build_byte_to_line(md);
        assert_eq!(
            list_sections_from_ast(&blocks, &offsets, &table),
            list_sections_for(md, false)
        );
        let tex = "\\section{Intro}\nbody\n\\subsection{Detail}\nmore";
        let (blocks, offsets) = crate::tex::parse(tex);
        let table = build_byte_to_line(tex);
        assert_eq!(
            list_sections_from_ast(&blocks, &offsets, &table),
            list_sections_for(tex, true)
        );
    }

    #[test]
    fn tex_sections_use_latex_parser() {
        // Markdown parsing of LaTeX finds no headings; the tex path must.
        let src = "\\section{Intro}\nbody\n\\subsection{Detail}\nmore";
        assert!(list_sections_for(src, false).is_empty());
        let tex = list_sections_for(src, true);
        let titles: Vec<&str> = tex.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, vec!["Intro", "Detail"]);
        assert_eq!(tex[1].level, 2);
    }
}
