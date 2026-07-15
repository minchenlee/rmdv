use crate::picker::is_markdown_path;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const WORKSPACE_TREE_MAX_DEPTH: usize = 12;
pub const WORKSPACE_FILE_INDEX_MAX_DEPTH: usize = 8;
pub const WORKSPACE_MAX_FILES: usize = 5_000;
/// Hard filesystem-work budget for one workspace index. Counting every entry,
/// including unsupported files, prevents a wide non-Markdown tree from
/// bypassing the file cap and monopolizing the UI or a background worker.
pub const WORKSPACE_MAX_ENTRIES: usize = 10_000;

#[derive(Debug, Clone)]
pub struct Node {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub children: Vec<Node>,
    /// Recursive supported-file count gathered while this node's subtree is
    /// visited by the one bounded workspace scan. Files leave this as `None`.
    pub recursive_supported_file_count: Option<RecursiveFileCount>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecursiveFileCount {
    Exact(usize),
    LowerBound(usize),
    Unavailable,
}

impl RecursiveFileCount {
    fn counted(self) -> usize {
        match self {
            Self::Exact(count) | Self::LowerBound(count) => count,
            Self::Unavailable => 0,
        }
    }

    fn is_exact(self) -> bool {
        matches!(self, Self::Exact(_))
    }
}

/// One bounded filesystem pass produces both consumers that previously walked
/// the workspace independently: the sidebar/mindmap tree and file-finder list.
#[derive(Debug, Clone)]
pub struct WorkspaceSnapshot {
    pub root: Node,
    pub files: Vec<PathBuf>,
    pub truncated: bool,
}

pub fn build(root: &Path, show_hidden: bool) -> Node {
    build_workspace(root, show_hidden)
        .map(|snapshot| snapshot.root)
        .unwrap_or_else(|_| empty_root(root))
}

pub fn build_workspace(root: &Path, show_hidden: bool) -> Result<WorkspaceSnapshot, String> {
    build_workspace_with_limits(
        root,
        show_hidden,
        WORKSPACE_TREE_MAX_DEPTH,
        WORKSPACE_FILE_INDEX_MAX_DEPTH,
        WORKSPACE_MAX_FILES,
        WORKSPACE_MAX_ENTRIES,
    )
}

fn empty_root(root: &Path) -> Node {
    let name = root
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.to_string_lossy().into_owned());
    Node {
        path: root.to_path_buf(),
        name,
        is_dir: true,
        children: Vec::new(),
        recursive_supported_file_count: None,
    }
}

fn build_workspace_with_limits(
    root: &Path,
    show_hidden: bool,
    tree_max_depth: usize,
    file_index_max_depth: usize,
    max_files: usize,
    max_entries: usize,
) -> Result<WorkspaceSnapshot, String> {
    let mut root_node = empty_root(root);
    let mut files = Vec::new();
    let mut budget = ScanBudget {
        examined_entries: 0,
        supported_files: 0,
        max_entries,
        max_files,
        exhausted: false,
        truncated: false,
    };
    fill_bounded(
        &mut root_node,
        0,
        tree_max_depth,
        file_index_max_depth,
        show_hidden,
        &mut budget,
        &mut files,
    )?;
    // When the budget ends, retain already-discovered directories even if we
    // could not inspect their descendants. This keeps shallow, ordinary
    // folders discoverable instead of falsely presenting them as absent.
    if !budget.truncated {
        prune(&mut root_node);
    }
    files.sort();
    Ok(WorkspaceSnapshot {
        root: root_node,
        files,
        truncated: budget.truncated,
    })
}

struct ScanBudget {
    examined_entries: usize,
    supported_files: usize,
    max_entries: usize,
    max_files: usize,
    /// Hard global entry/file budget exhausted. Unlike a depth or read
    /// interruption, this stops later sibling walks too.
    exhausted: bool,
    truncated: bool,
}

fn fill_bounded(
    node: &mut Node,
    depth: usize,
    tree_max_depth: usize,
    file_index_max_depth: usize,
    show_hidden: bool,
    budget: &mut ScanBudget,
    indexed_files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if depth >= tree_max_depth {
        node.recursive_supported_file_count = Some(RecursiveFileCount::LowerBound(0));
        budget.truncated = true;
        return Ok(());
    }
    if budget.exhausted {
        node.recursive_supported_file_count = Some(RecursiveFileCount::LowerBound(0));
        return Ok(());
    }
    let rd = match std::fs::read_dir(&node.path) {
        Ok(rd) => rd,
        Err(error) if depth == 0 => return Err(error.to_string()),
        Err(_) => {
            node.recursive_supported_file_count = Some(RecursiveFileCount::Unavailable);
            return Ok(());
        }
    };
    struct ScanEntry {
        path: PathBuf,
        name: String,
        is_dir: bool,
        is_hidden: bool,
    }

    let mut entries = Vec::new();
    let mut subtree_incomplete = false;
    for entry in rd {
        if budget.examined_entries >= budget.max_entries {
            budget.exhausted = true;
            budget.truncated = true;
            subtree_incomplete = true;
            break;
        }
        budget.examined_entries += 1;
        let e = match entry {
            Ok(entry) => entry,
            Err(_) => {
                budget.truncated = true;
                subtree_incomplete = true;
                continue;
            }
        };
        let name = e.file_name().to_string_lossy().into_owned();
        // Always skip massive build/vcs caches.
        if name == "node_modules" || name == "target" || name == ".git" {
            continue;
        }
        // Other dot-entries gated on the toggle.
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        let path = e.path();
        entries.push(ScanEntry {
            is_dir: path.is_dir(),
            is_hidden: name.starts_with('.'),
            path,
            name,
        });
    }

    // Among entries admitted by the hard read budget, ordinary entries always
    // precede optional dot entries. This prevents traversal of an admitted
    // hidden cache from consuming the remaining budget before Documents. A
    // directory with >10k immediate entries is still explicitly truncated.
    entries.sort_by(|a, b| {
        a.is_hidden
            .cmp(&b.is_hidden)
            .then_with(|| b.is_dir.cmp(&a.is_dir))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    let mut dirs: Vec<Node> = Vec::new();
    let mut files: Vec<Node> = Vec::new();
    for entry in entries {
        if entry.is_dir {
            dirs.push(Node {
                path: entry.path,
                name: entry.name,
                is_dir: true,
                children: Vec::new(),
                recursive_supported_file_count: None,
            });
        } else if is_markdown_path(&entry.path) {
            if budget.supported_files >= budget.max_files {
                budget.exhausted = true;
                budget.truncated = true;
                subtree_incomplete = true;
                break;
            }
            budget.supported_files += 1;
            if depth < file_index_max_depth {
                indexed_files.push(entry.path.clone());
            }
            files.push(Node {
                path: entry.path,
                name: entry.name,
                is_dir: false,
                children: Vec::new(),
                recursive_supported_file_count: None,
            });
            if budget.supported_files >= budget.max_files {
                budget.exhausted = true;
                budget.truncated = true;
                subtree_incomplete = true;
                break;
            }
        }
    }

    // Discover all immediate siblings before descending. This prevents one
    // early subtree from making later top-level folders disappear entirely.
    for child in &mut dirs {
        fill_bounded(
            child,
            depth + 1,
            tree_max_depth,
            file_index_max_depth,
            show_hidden,
            budget,
            indexed_files,
        )?;
        let child_count = child
            .recursive_supported_file_count
            .unwrap_or(RecursiveFileCount::LowerBound(0));
        if !child_count.is_exact() {
            subtree_incomplete = true;
        }
    }
    let recursive_count = files.len()
        + dirs
            .iter()
            .filter_map(|child| child.recursive_supported_file_count)
            .map(RecursiveFileCount::counted)
            .sum::<usize>();
    node.children = dirs;
    node.children.extend(files);
    node.recursive_supported_file_count = Some(if subtree_incomplete {
        RecursiveFileCount::LowerBound(recursive_count)
    } else {
        RecursiveFileCount::Exact(recursive_count)
    });
    Ok(())
}

/// Remove dirs with no markdown descendants.
fn prune(node: &mut Node) -> bool {
    if !node.is_dir {
        return true;
    }
    node.children.retain_mut(|c| prune(c));
    !node.children.is_empty()
        || matches!(
            node.recursive_supported_file_count,
            Some(RecursiveFileCount::LowerBound(_) | RecursiveFileCount::Unavailable)
        )
}

/// Flatten tree into visible rows respecting `expanded` set. Root not shown.
pub fn flatten<'a>(root: &'a Node, expanded: &HashSet<PathBuf>) -> Vec<Row<'a>> {
    let mut out = Vec::new();
    for child in &root.children {
        push(child, 0, expanded, &mut out);
    }
    out
}

fn push<'a>(node: &'a Node, depth: usize, expanded: &HashSet<PathBuf>, out: &mut Vec<Row<'a>>) {
    out.push(Row { node, depth });
    if node.is_dir && expanded.contains(&node.path) {
        for c in &node.children {
            push(c, depth + 1, expanded, out);
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Row<'a> {
    pub node: &'a Node,
    pub depth: usize,
}

/// Set containing every ancestor path (within root) needed to reveal `target`.
pub fn ancestors_of(root: &Path, target: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut cur = target.parent();
    while let Some(p) = cur {
        out.push(p.to_path_buf());
        if p == root {
            break;
        }
        cur = p.parent();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(label: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("rmdv-tree-{label}-{}-{stamp}", std::process::id()))
    }

    #[test]
    fn workspace_snapshot_builds_tree_and_file_index_in_one_bounded_pass() {
        let root = test_dir("snapshot");
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        std::fs::write(root.join("readme.md"), "# Root\n").unwrap();
        std::fs::write(root.join("docs/guide.md"), "# Guide\n").unwrap();
        std::fs::write(root.join("target/generated.md"), "# Generated\n").unwrap();
        std::fs::write(root.join(".hidden/secret.md"), "# Secret\n").unwrap();

        let snapshot = build_workspace(&root, false).unwrap();

        assert!(!snapshot.truncated);
        assert_eq!(snapshot.files.len(), 2);
        assert!(snapshot.files.contains(&root.join("readme.md")));
        assert!(snapshot.files.contains(&root.join("docs/guide.md")));
        assert_eq!(snapshot.root.children.len(), 2);
        assert!(snapshot
            .root
            .children
            .iter()
            .any(|node| node.name == "docs"));
        assert!(snapshot
            .root
            .children
            .iter()
            .any(|node| node.name == "readme.md"));
        assert_eq!(
            snapshot.root.recursive_supported_file_count,
            Some(RecursiveFileCount::Exact(2))
        );
        assert_eq!(
            snapshot
                .root
                .children
                .iter()
                .find(|node| node.name == "docs")
                .and_then(|node| node.recursive_supported_file_count),
            Some(RecursiveFileCount::Exact(1))
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn workspace_snapshot_stops_at_entry_budget() {
        let root = test_dir("entry-cap");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("one.bin"), "").unwrap();
        std::fs::write(root.join("two.bin"), "").unwrap();

        let snapshot = build_workspace_with_limits(&root, false, 12, 8, 5_000, 1).unwrap();

        assert!(snapshot.truncated);
        assert!(snapshot.files.is_empty());
        assert_eq!(
            snapshot.root.recursive_supported_file_count,
            Some(RecursiveFileCount::LowerBound(0))
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn workspace_snapshot_marks_the_exact_file_cap_incomplete() {
        let root = test_dir("file-cap");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("one.md"), "# One\n").unwrap();

        let snapshot = build_workspace_with_limits(&root, false, 12, 8, 1, 10).unwrap();

        assert!(snapshot.truncated);
        assert_eq!(
            snapshot.root.recursive_supported_file_count,
            Some(RecursiveFileCount::LowerBound(1))
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn workspace_snapshot_marks_only_interrupted_subtrees_as_lower_bounds() {
        let root = test_dir("subtree-cap");
        std::fs::create_dir_all(root.join("a-exact")).unwrap();
        std::fs::create_dir_all(root.join("b-interrupted")).unwrap();
        std::fs::write(root.join("a-exact/one.md"), "# One\n").unwrap();
        std::fs::write(root.join("b-interrupted/two.md"), "# Two\n").unwrap();
        std::fs::write(root.join("b-interrupted/three.md"), "# Three\n").unwrap();

        let snapshot = build_workspace_with_limits(&root, false, 12, 8, 2, 20).unwrap();
        let exact = snapshot
            .root
            .children
            .iter()
            .find(|node| node.name == "a-exact")
            .unwrap();
        let interrupted = snapshot
            .root
            .children
            .iter()
            .find(|node| node.name == "b-interrupted")
            .unwrap();
        assert_eq!(
            exact.recursive_supported_file_count,
            Some(RecursiveFileCount::Exact(1))
        );
        assert_eq!(
            interrupted.recursive_supported_file_count,
            Some(RecursiveFileCount::LowerBound(1))
        );
        assert_eq!(
            snapshot.root.recursive_supported_file_count,
            Some(RecursiveFileCount::LowerBound(2))
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn workspace_snapshot_surfaces_unreadable_root() {
        let missing = test_dir("missing");
        assert!(build_workspace(&missing, false).is_err());
    }

    #[test]
    fn hidden_workspace_entries_are_additive_and_do_not_crowd_documents() {
        let root = test_dir("hidden-union");
        let documents = root.join("Documents");
        let hidden = root.join(".cache");
        std::fs::create_dir_all(&documents).unwrap();
        std::fs::create_dir_all(&hidden).unwrap();
        std::fs::write(documents.join("visible.md"), "# Visible\n").unwrap();
        for index in 0..12 {
            std::fs::write(hidden.join(format!("cache-{index}.bin")), "").unwrap();
        }
        std::fs::write(hidden.join("hidden.md"), "# Hidden\n").unwrap();

        let hidden_off = build_workspace_with_limits(&root, false, 12, 8, 20, 8).unwrap();
        assert!(hidden_off.files.contains(&documents.join("visible.md")));
        assert!(!hidden_off.files.contains(&hidden.join("hidden.md")));

        let complete_hidden_on = build_workspace(&root, true).unwrap();
        assert!(complete_hidden_on
            .files
            .contains(&documents.join("visible.md")));
        assert!(complete_hidden_on.files.contains(&hidden.join("hidden.md")));

        let hidden_on = build_workspace_with_limits(&root, true, 12, 8, 20, 8).unwrap();
        assert!(hidden_on.files.contains(&documents.join("visible.md")));
        assert!(hidden_on
            .root
            .children
            .iter()
            .any(|node| node.path == documents));
        assert!(hidden_on.truncated);

        let _ = std::fs::remove_dir_all(root);
    }
}
