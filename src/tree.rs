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
    prune(&mut root_node);
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
    if depth >= tree_max_depth || budget.truncated {
        return Ok(());
    }
    let rd = match std::fs::read_dir(&node.path) {
        Ok(rd) => rd,
        Err(error) if depth == 0 => return Err(error.to_string()),
        Err(_) => return Ok(()),
    };
    let mut dirs: Vec<Node> = Vec::new();
    let mut files: Vec<Node> = Vec::new();
    for entry in rd {
        if budget.examined_entries >= budget.max_entries {
            budget.truncated = true;
            break;
        }
        budget.examined_entries += 1;
        let Ok(e) = entry else {
            continue;
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
        let p = e.path();
        if p.is_dir() {
            let mut child = Node {
                path: p,
                name,
                is_dir: true,
                children: Vec::new(),
            };
            fill_bounded(
                &mut child,
                depth + 1,
                tree_max_depth,
                file_index_max_depth,
                show_hidden,
                budget,
                indexed_files,
            )?;
            dirs.push(child);
        } else if is_markdown_path(&p) {
            if budget.supported_files >= budget.max_files {
                budget.truncated = true;
                break;
            }
            budget.supported_files += 1;
            if depth < file_index_max_depth {
                indexed_files.push(p.clone());
            }
            files.push(Node {
                path: p,
                name,
                is_dir: false,
                children: Vec::new(),
            });
        }
        if budget.truncated {
            break;
        }
    }
    dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    node.children = dirs;
    node.children.extend(files);
    Ok(())
}

/// Remove dirs with no markdown descendants.
fn prune(node: &mut Node) -> bool {
    if !node.is_dir {
        return true;
    }
    node.children.retain_mut(|c| prune(c));
    !node.children.is_empty()
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
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn workspace_snapshot_surfaces_unreadable_root() {
        let missing = test_dir("missing");
        assert!(build_workspace(&missing, false).is_err());
    }
}
