//! Path, URL, and link helpers shared by loading, rendering, and IPC.

use super::*;

/// True for `.tex` files, which route through the LaTeX parser instead of
/// the markdown one.
pub(super) fn is_tex_path(path: Option<&std::path::Path>) -> bool {
    path.and_then(|p| p.extension())
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("tex"))
}

pub(super) fn canonicalize_existing_path(path: PathBuf) -> PathBuf {
    std::fs::canonicalize(&path).unwrap_or(path)
}

/// PDFs are extracted to markdown for viewing only; their source isn't editable
/// text, so edit mode (⌘E / `ViewMode::Raw`) is disabled for them.
pub(super) fn is_pdf_path(path: Option<&std::path::Path>) -> bool {
    path.and_then(|p| p.extension())
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

pub(super) fn data_lang_for(path: Option<&std::path::Path>) -> Option<&'static str> {
    let ext = path.and_then(|p| p.extension()).and_then(|e| e.to_str())?;
    match ext.to_ascii_lowercase().as_str() {
        "json" => Some("json"),
        "yaml" | "yml" => Some("yaml"),
        "toml" => Some("toml"),
        _ => None,
    }
}

pub fn is_remote_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

/// True for links that should hand off to the OS rather than open in-app:
/// remote URLs and any scheme:// / mailto:-style target.
pub fn is_external_link(s: &str) -> bool {
    is_remote_url(s) || s.contains("://") || s.starts_with("mailto:") || s.starts_with("tel:")
}

/// GitHub-style heading slug: lowercase, runs of space/`-`/`_` collapse to a
/// single `-`, other punctuation dropped, leading/trailing `-` trimmed. A
/// single space and a run of spaces both yield one `-` so hand-written anchors
/// (`#results-discussion`) match titles with incidental double spacing.
pub fn slugify(title: &str) -> String {
    let mut out = String::new();
    let mut pending_sep = false;
    for c in title.chars() {
        if c.is_alphanumeric() {
            if pending_sep && !out.is_empty() {
                out.push('-');
            }
            pending_sep = false;
            out.extend(c.to_lowercase());
        } else if c == ' ' || c == '-' || c == '_' {
            pending_sep = true;
        }
    }
    out
}

/// Resolve a link `#fragment` to a heading line in `src`, matching the
/// GitHub-style slug of each heading title. `is_tex` selects the LaTeX parser
/// so `.tex` documents' `\section{}` headings are seen.
pub fn line_for_fragment(src: &str, fragment: &str, is_tex: bool) -> Option<u32> {
    let want = slugify(fragment);
    crate::ipc::sections::list_sections_for(src, is_tex)
        .into_iter()
        .find(|s| slugify(&s.title) == want)
        .map(|s| s.line)
}

pub fn resolve_image_path(url: &str, current_file: Option<&std::path::Path>) -> Option<PathBuf> {
    let p = std::path::Path::new(url);
    if p.is_absolute() {
        return Some(p.to_path_buf());
    }
    let base = current_file.and_then(|f| f.parent())?;
    Some(base.join(url))
}
