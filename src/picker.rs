use std::path::{Path, PathBuf};

/// A bounded count of files rmdv can open inside a folder.
///
/// This deliberately mirrors the workspace tree's filtering rules.  The cap
/// keeps folder-selection previews useful without turning a quick peek into an
/// unbounded filesystem walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupportedFileCount {
    pub count: usize,
    pub capped: bool,
}

pub const FOLDER_FILE_COUNT_MAX_DEPTH: usize = 12;
pub const FOLDER_FILE_COUNT_MAX_FILES: usize = 5_000;
/// Maximum directory entries examined for one chooser preview. This bounds
/// background work even when a tree contains only unsupported files.
pub const FOLDER_FILE_COUNT_MAX_ENTRIES: usize = 10_000;

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_md: bool,
}

#[derive(Debug, Clone)]
pub struct Picker {
    pub cwd: PathBuf,
    pub entries: Vec<Entry>,
    pub selected: usize,
    pub error: Option<String>,
    pub mode: PickerMode,
    pub show_hidden: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerMode {
    /// Pick any folder or markdown file (used when no workspace yet).
    OpenAny,
    /// Pick only folders (workspace root selection).
    Folder,
}

impl Picker {
    pub fn new(start: Option<PathBuf>, mode: PickerMode, show_hidden: bool) -> Self {
        let cwd = start
            .or_else(|| dirs::home_dir())
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("/"));
        let mut p = Self {
            cwd,
            entries: Vec::new(),
            selected: 0,
            error: None,
            mode,
            show_hidden,
        };
        p.refresh();
        p
    }

    pub fn refresh(&mut self) {
        self.entries.clear();
        self.selected = 0;
        self.error = None;
        match std::fs::read_dir(&self.cwd) {
            Ok(rd) => {
                let mut items: Vec<Entry> = rd
                    .filter_map(|e| e.ok())
                    .filter_map(|e| {
                        let path = e.path();
                        let name = e.file_name().to_string_lossy().into_owned();
                        // Always skip .git (huge, never wanted).
                        if name == ".git" {
                            return None;
                        }
                        // Other dot-entries gated by toggle.
                        if !self.show_hidden && name.starts_with('.') {
                            return None;
                        }
                        let is_dir = path.is_dir();
                        let is_md = !is_dir && is_markdown_path(&path);
                        match self.mode {
                            PickerMode::OpenAny => {
                                if !is_dir && !is_md {
                                    return None;
                                }
                            }
                            PickerMode::Folder => {
                                if !is_dir {
                                    return None;
                                }
                            }
                        }
                        Some(Entry {
                            name,
                            path,
                            is_dir,
                            is_md,
                        })
                    })
                    .collect();
                items.sort_by(|a, b| match (a.is_dir, b.is_dir) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                });
                self.entries = items;
            }
            Err(e) => {
                self.error = Some(e.to_string());
            }
        }
    }

    pub fn navigate_to(&mut self, p: PathBuf) {
        self.cwd = p;
        self.refresh();
    }

    pub fn parent(&mut self) {
        if let Some(parent) = self.cwd.parent() {
            self.cwd = parent.to_path_buf();
            self.refresh();
        }
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.entries.is_empty() {
            return;
        }
        let len = self.entries.len() as isize;
        let next = (self.selected as isize + delta).rem_euclid(len);
        self.selected = next as usize;
    }

    pub fn home() -> Option<PathBuf> {
        dirs::home_dir()
    }

    pub fn breadcrumbs(&self) -> Vec<(String, PathBuf)> {
        let mut out = Vec::new();
        let mut acc = PathBuf::new();
        for c in self.cwd.components() {
            acc.push(c.as_os_str());
            let label = match c {
                std::path::Component::RootDir => continue,
                std::path::Component::Normal(s) => s.to_string_lossy().into_owned(),
                std::path::Component::Prefix(p) => p.as_os_str().to_string_lossy().into_owned(),
                _ => continue,
            };
            out.push((label, acc.clone()));
        }
        out
    }
}

pub fn is_markdown_path(p: &Path) -> bool {
    let ext = p.extension().and_then(|s| s.to_str());
    // PDFs are viewable (extracted to markdown) only when the `pdf` feature is
    // compiled in, so only surface them in the explorer then.
    #[cfg(feature = "pdf")]
    if ext.is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
        return true;
    }
    matches!(
        ext,
        Some("md")
            | Some("markdown")
            | Some("tex")
            | Some("json")
            | Some("yaml")
            | Some("yml")
            | Some("toml")
    )
}

/// Count supported files below `root` without allocating their paths.
///
/// The depth, hidden-file, and large-directory exclusions intentionally match
/// `tree::build`, so the chooser's count describes the files that would become
/// navigable after choosing the folder as a workspace.
pub fn count_supported_files(
    root: &Path,
    max_depth: usize,
    max_files: usize,
    max_entries: usize,
    show_hidden: bool,
) -> Result<SupportedFileCount, String> {
    let mut count = 0;
    let mut examined_entries = 0;
    let mut stack = vec![(root.to_path_buf(), 0usize)];

    while let Some((dir, depth)) = stack.pop() {
        if depth >= max_depth {
            continue;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if depth == 0 => return Err(error.to_string()),
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            if examined_entries >= max_entries {
                return Ok(SupportedFileCount {
                    count,
                    capped: true,
                });
            }
            examined_entries += 1;
            let name = entry.file_name().to_string_lossy().into_owned();
            if matches!(name.as_str(), ".git" | "node_modules" | "target")
                || (!show_hidden && name.starts_with('.'))
            {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                stack.push((path, depth + 1));
            } else if is_markdown_path(&path) {
                count += 1;
                if count >= max_files {
                    return Ok(SupportedFileCount {
                        count,
                        capped: true,
                    });
                }
            }
        }
    }

    Ok(SupportedFileCount {
        count,
        capped: false,
    })
}

/// Walk a workspace folder gathering all markdown files (limited depth + count).
pub fn walk_markdown(
    root: &Path,
    max_depth: usize,
    max_files: usize,
    show_hidden: bool,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        if out.len() >= max_files {
            break;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut sub = Vec::new();
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name == ".git" || name == "node_modules" || name == "target" {
                continue;
            }
            if !show_hidden && name.starts_with('.') {
                continue;
            }
            let p = e.path();
            if p.is_dir() {
                if depth + 1 < max_depth {
                    sub.push((p, depth + 1));
                }
            } else if is_markdown_path(&p) {
                out.push(p);
                if out.len() >= max_files {
                    break;
                }
            }
        }
        sub.reverse();
        stack.extend(sub);
    }
    out.sort();
    out
}

pub fn fuzzy_score(query: &str, candidate: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let q = query.to_lowercase();
    let c = candidate.to_lowercase();
    let mut q_iter = q.chars().peekable();
    let mut score = 0i32;
    let mut prev_match = -1i32;
    let mut prev_char = ' ';
    for (i, ch) in c.chars().enumerate() {
        if let Some(qc) = q_iter.peek() {
            if *qc == ch {
                let i = i as i32;
                score += 10;
                if prev_match >= 0 && i == prev_match + 1 {
                    score += 15; // contiguous bonus
                }
                if prev_char == '/'
                    || prev_char == '_'
                    || prev_char == '-'
                    || prev_char == ' '
                    || i == 0
                {
                    score += 8; // boundary bonus
                }
                prev_match = i;
                q_iter.next();
            }
        }
        prev_char = ch;
    }
    if q_iter.peek().is_some() {
        return None;
    }
    score -= candidate.len() as i32 / 4;
    Some(score)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[cfg(feature = "pdf")]
    #[test]
    fn pdf_is_listed_when_feature_on() {
        assert!(is_markdown_path(Path::new("doc.pdf")));
        assert!(is_markdown_path(Path::new("DOC.PDF"))); // case-insensitive
    }

    #[cfg(not(feature = "pdf"))]
    #[test]
    fn pdf_not_listed_when_feature_off() {
        assert!(!is_markdown_path(Path::new("doc.pdf")));
    }

    #[test]
    fn markdown_and_data_listed_but_binary_not() {
        assert!(is_markdown_path(Path::new("a.md")));
        assert!(is_markdown_path(Path::new("a.yaml")));
        assert!(!is_markdown_path(Path::new("a.bin")));
        assert!(!is_markdown_path(Path::new("a.png")));
    }

    #[test]
    fn supported_file_count_is_recursive_bounded_and_respects_filters() {
        let root = std::env::temp_dir().join(format!(
            "rmdv-picker-file-count-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let nested = root.join("nested");
        let cap_dir = root.join("cap");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir_all(&cap_dir).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::write(root.join("readme.md"), "# Root\n").unwrap();
        std::fs::write(nested.join("data.json"), "{}\n").unwrap();
        std::fs::write(root.join(".hidden/secret.md"), "# Hidden\n").unwrap();
        std::fs::write(root.join("target/generated.md"), "# Generated\n").unwrap();
        std::fs::write(cap_dir.join("one.bin"), "").unwrap();
        std::fs::write(cap_dir.join("two.bin"), "").unwrap();

        assert_eq!(
            count_supported_files(
                &root,
                FOLDER_FILE_COUNT_MAX_DEPTH,
                10,
                FOLDER_FILE_COUNT_MAX_ENTRIES,
                false,
            )
            .unwrap(),
            SupportedFileCount {
                count: 2,
                capped: false
            }
        );
        assert_eq!(
            count_supported_files(
                &root,
                FOLDER_FILE_COUNT_MAX_DEPTH,
                1,
                FOLDER_FILE_COUNT_MAX_ENTRIES,
                false,
            )
            .unwrap(),
            SupportedFileCount {
                count: 1,
                capped: true
            }
        );
        assert_eq!(
            count_supported_files(&cap_dir, FOLDER_FILE_COUNT_MAX_DEPTH, 10, 1, false).unwrap(),
            SupportedFileCount {
                count: 0,
                capped: true,
            }
        );
        assert!(count_supported_files(
            &root.join("missing"),
            FOLDER_FILE_COUNT_MAX_DEPTH,
            10,
            FOLDER_FILE_COUNT_MAX_ENTRIES,
            false,
        )
        .is_err());

        let _ = std::fs::remove_dir_all(root);
    }
}
