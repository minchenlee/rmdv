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

/// Whether a link to `path` opens inside rmdv: the sidebar's own test
/// ([`crate::picker::is_markdown_path`]), so a file the tree lists opens
/// from a link too. The extension's case does not matter (`README.MD`).
pub(super) fn link_opens_in_app(path: &std::path::Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    crate::picker::is_markdown_path(&path.with_extension(ext.to_ascii_lowercase()))
}

/// What the link pill shows for `url`: a web link or a `#fragment` as
/// written; a local link as the file it resolves to, relative to the
/// workspace (or from `~` outside it), keeping its `#fragment`. Long labels
/// keep their start and end.
pub(super) fn link_destination_label(
    url: &str,
    current_file: Option<&std::path::Path>,
    workspace: Option<&std::path::Path>,
) -> String {
    let (target, fragment) = match url.split_once('#') {
        Some((target, fragment)) => (target, Some(fragment)),
        None => (url, None),
    };
    let label = match resolve_image_path(target, current_file) {
        Some(path) if !target.is_empty() && !is_external_link(target) => {
            let path = normalize_lexically(&path);
            let mut label = match workspace.and_then(|root| path.strip_prefix(root).ok()) {
                Some(rest) => rest.display().to_string(),
                None => match dirs::home_dir().and_then(|h| path.strip_prefix(h).ok()) {
                    Some(rest) => format!("~/{}", rest.display()),
                    None => path.display().to_string(),
                },
            };
            if let Some(fragment) = fragment {
                label.push('#');
                label.push_str(fragment);
            }
            label
        }
        _ => url.to_string(),
    };
    shorten_middle(&label, LINK_LABEL_MAX_CHARS)
}

const LINK_LABEL_MAX_CHARS: usize = 120;

fn shorten_middle(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    let tail = max / 3;
    let head = max - tail - 1;
    let mut out: String = s.chars().take(head).collect();
    out.push('…');
    out.extend(s.chars().skip(count - tail));
    out
}

/// `a/./b/../c` as `a/c`, without touching the filesystem.
fn normalize_lexically(path: &std::path::Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// `open -g <url>`: opens a link without bringing its app to the front.
/// `None` for a target that `open` would read as an option.
#[cfg(target_os = "macos")]
pub(super) fn background_open_command(target: &str) -> Option<std::process::Command> {
    if !target.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    let mut command = std::process::Command::new("open");
    command.arg("-g").arg(target);
    Some(command)
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
