use crate::picker::is_markdown_path;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
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
    /// File-finder/vault-search index, limited by
    /// `WORKSPACE_FILE_INDEX_MAX_DEPTH` as before.
    pub files: Vec<PathBuf>,
    /// Lightweight file paths for the standard Files sidebar across the full
    /// retained tree depth. This remains bounded by the same global scan and
    /// replaces permanent file `Node`s without reducing sidebar coverage.
    pub sidebar_files: Vec<PathBuf>,
    pub truncated: bool,
}

/// A bounded, non-recursive file listing for one expanded explorer folder.
/// Folder structure and recursive counts remain owned by `WorkspaceSnapshot`;
/// these paths are short-lived UI materialization only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImmediateFilesSnapshot {
    pub files: Vec<PathBuf>,
    pub truncated: bool,
}

pub fn load_immediate_supported_files(
    folder: &Path,
    show_hidden: bool,
) -> Result<ImmediateFilesSnapshot, String> {
    load_immediate_supported_files_with_limits(
        folder,
        show_hidden,
        WORKSPACE_MAX_FILES,
        WORKSPACE_MAX_ENTRIES,
    )
}

fn load_immediate_supported_files_with_limits(
    folder: &Path,
    show_hidden: bool,
    max_files: usize,
    max_entries: usize,
) -> Result<ImmediateFilesSnapshot, String> {
    let read_dir = std::fs::read_dir(folder).map_err(|error| error.to_string())?;
    let mut files = Vec::new();
    let mut truncated = false;

    for (examined, entry) in read_dir.enumerate() {
        if examined >= max_entries {
            truncated = true;
            break;
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                truncated = true;
                continue;
            }
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() || !is_markdown_path(&path) {
            continue;
        }
        if files.len() >= max_files {
            truncated = true;
            break;
        }
        files.push(path);
    }

    files.sort_by(|a, b| {
        let a_name = a
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| a.to_string_lossy().into_owned());
        let b_name = b
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| b.to_string_lossy().into_owned());
        a_name
            .to_lowercase()
            .cmp(&b_name.to_lowercase())
            .then_with(|| a_name.cmp(&b_name))
    });
    Ok(ImmediateFilesSnapshot { files, truncated })
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
    let mut sidebar_files = Vec::new();
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
        &mut sidebar_files,
    )?;
    // When the budget ends, retain already-discovered directories even if we
    // could not inspect their descendants. This keeps shallow, ordinary
    // folders discoverable instead of falsely presenting them as absent.
    if !budget.truncated {
        prune(&mut root_node);
    }
    files.sort();
    sidebar_files.sort();
    Ok(WorkspaceSnapshot {
        root: root_node,
        files,
        sidebar_files,
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
    sidebar_files: &mut Vec<PathBuf>,
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
    let mut local_supported_files = 0;
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
            local_supported_files += 1;
            sidebar_files.push(entry.path.clone());
            if depth < file_index_max_depth {
                indexed_files.push(entry.path.clone());
            }
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
            sidebar_files,
        )?;
        let child_count = child
            .recursive_supported_file_count
            .unwrap_or(RecursiveFileCount::LowerBound(0));
        if !child_count.is_exact() {
            subtree_incomplete = true;
        }
    }
    let recursive_count = local_supported_files
        + dirs
            .iter()
            .filter_map(|child| child.recursive_supported_file_count)
            .map(RecursiveFileCount::counted)
            .sum::<usize>();
    node.children = dirs;
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
            Some(
                RecursiveFileCount::Exact(1..)
                    | RecursiveFileCount::LowerBound(_)
                    | RecursiveFileCount::Unavailable
            )
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
    out.push(Row {
        node: RowNode::Retained(node),
        depth,
    });
    if node.is_dir && expanded.contains(&node.path) {
        for c in &node.children {
            push(c, depth + 1, expanded, out);
        }
    }
}

/// Flatten the retained folder skeleton for the standard Files sidebar and
/// splice bounded indexed file paths beneath their visible parent. This keeps
/// Full Mindmap's retained tree folder-only without removing ordinary sidebar
/// file rows or allocating a second permanent file-node tree.
pub fn flatten_with_files<'a>(
    root: &'a Node,
    files: &'a [PathBuf],
    expanded: &HashSet<PathBuf>,
) -> Vec<Row<'a>> {
    let mut files_by_parent: HashMap<&'a Path, Vec<&'a Path>> = HashMap::new();
    for file in files {
        if let Some(parent) = file.parent() {
            files_by_parent.entry(parent).or_default().push(file);
        }
    }
    for siblings in files_by_parent.values_mut() {
        siblings.sort_by(|a, b| sidebar_file_order(a, b));
    }

    let mut out = Vec::new();
    for child in &root.children {
        push_with_files(child, 0, expanded, &files_by_parent, &mut out);
    }
    append_indexed_files(&root.path, 0, &files_by_parent, &mut out);
    out
}

fn push_with_files<'a>(
    node: &'a Node,
    depth: usize,
    expanded: &HashSet<PathBuf>,
    files_by_parent: &HashMap<&'a Path, Vec<&'a Path>>,
    out: &mut Vec<Row<'a>>,
) {
    out.push(Row {
        node: RowNode::Retained(node),
        depth,
    });
    if node.is_dir && expanded.contains(&node.path) {
        for child in &node.children {
            push_with_files(child, depth + 1, expanded, files_by_parent, out);
        }
        append_indexed_files(&node.path, depth + 1, files_by_parent, out);
    }
}

fn append_indexed_files<'a>(
    parent: &Path,
    depth: usize,
    files_by_parent: &HashMap<&'a Path, Vec<&'a Path>>,
    out: &mut Vec<Row<'a>>,
) {
    if let Some(files) = files_by_parent.get(parent) {
        out.extend(files.iter().map(|path| Row {
            node: RowNode::IndexedFile(path),
            depth,
        }));
    }
}

fn sidebar_file_order(a: &Path, b: &Path) -> std::cmp::Ordering {
    let a_name = a
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| a.as_os_str().to_string_lossy());
    let b_name = b
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| b.as_os_str().to_string_lossy());
    a_name
        .starts_with('.')
        .cmp(&b_name.starts_with('.'))
        .then_with(|| a_name.to_lowercase().cmp(&b_name.to_lowercase()))
        .then_with(|| a_name.cmp(&b_name))
}

#[derive(Debug, Clone, Copy)]
pub struct Row<'a> {
    pub node: RowNode<'a>,
    pub depth: usize,
}

#[derive(Debug, Clone, Copy)]
pub enum RowNode<'a> {
    Retained(&'a Node),
    IndexedFile(&'a Path),
}

impl<'a> RowNode<'a> {
    pub fn path(self) -> &'a Path {
        match self {
            Self::Retained(node) => &node.path,
            Self::IndexedFile(path) => path,
        }
    }

    pub fn name(self) -> Cow<'a, str> {
        match self {
            Self::Retained(node) => Cow::Borrowed(&node.name),
            Self::IndexedFile(path) => path
                .file_name()
                .map(|name| name.to_string_lossy())
                .unwrap_or_else(|| path.as_os_str().to_string_lossy()),
        }
    }

    pub fn is_dir(self) -> bool {
        matches!(self, Self::Retained(node) if node.is_dir)
    }
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
        assert_eq!(snapshot.root.children.len(), 1);
        assert!(snapshot
            .root
            .children
            .iter()
            .any(|node| node.name == "docs"));
        assert!(snapshot.root.children.iter().all(|node| node.is_dir));
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
    fn immediate_file_load_is_non_recursive_hidden_aware_and_bounded() {
        let root = test_dir("immediate");
        std::fs::create_dir_all(root.join("nested")).unwrap();
        std::fs::write(root.join("b.md"), "# B\n").unwrap();
        std::fs::write(root.join("a.md"), "# A\n").unwrap();
        std::fs::write(root.join(".secret.md"), "# Secret\n").unwrap();
        std::fs::write(root.join("ignored.txt"), "nope\n").unwrap();
        std::fs::write(root.join("nested/deep.md"), "# Deep\n").unwrap();

        let visible = load_immediate_supported_files_with_limits(&root, false, 10, 10).unwrap();
        assert_eq!(visible.files, vec![root.join("a.md"), root.join("b.md")]);
        assert!(!visible.truncated);

        let hidden = load_immediate_supported_files_with_limits(&root, true, 10, 10).unwrap();
        assert_eq!(hidden.files.len(), 3);
        assert!(hidden.files.contains(&root.join(".secret.md")));
        assert!(!hidden.files.contains(&root.join("nested/deep.md")));

        let bounded = load_immediate_supported_files_with_limits(&root, true, 1, 10).unwrap();
        assert_eq!(bounded.files.len(), 1);
        assert!(bounded.truncated);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sidebar_rows_splice_indexed_files_under_only_visible_parents() {
        let root = test_dir("sidebar-rows");
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        std::fs::write(root.join("b.md"), "# B\n").unwrap();
        std::fs::write(root.join("a.md"), "# A\n").unwrap();
        std::fs::write(root.join(".secret.md"), "# Secret\n").unwrap();
        std::fs::write(root.join("docs/nested.md"), "# Nested\n").unwrap();
        std::fs::write(root.join(".hidden/inside.md"), "# Inside\n").unwrap();

        let visible = build_workspace(&root, false).unwrap();
        assert!(visible.root.children.iter().all(|node| node.is_dir));
        let collapsed = flatten_with_files(
            &visible.root,
            &visible.sidebar_files,
            &HashSet::from([root.clone()]),
        );
        assert_eq!(
            collapsed
                .iter()
                .map(|row| row.node.name().into_owned())
                .collect::<Vec<_>>(),
            vec!["docs", "a.md", "b.md"]
        );
        assert!(!collapsed
            .iter()
            .any(|row| row.node.path() == root.join("docs/nested.md")));

        let expanded = flatten_with_files(
            &visible.root,
            &visible.sidebar_files,
            &HashSet::from([root.clone(), root.join("docs")]),
        );
        assert_eq!(
            expanded
                .iter()
                .map(|row| (row.node.name().into_owned(), row.depth))
                .collect::<Vec<_>>(),
            vec![
                ("docs".into(), 0),
                ("nested.md".into(), 1),
                ("a.md".into(), 0),
                ("b.md".into(), 0),
            ]
        );

        let shown = build_workspace(&root, true).unwrap();
        let shown_rows = flatten_with_files(
            &shown.root,
            &shown.sidebar_files,
            &HashSet::from([root.clone()]),
        );
        assert!(shown_rows
            .iter()
            .any(|row| row.node.path() == root.join(".hidden")));
        assert!(shown_rows
            .iter()
            .any(|row| row.node.path() == root.join(".secret.md")));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sidebar_paths_keep_tree_depth_without_broadening_file_finder_depth() {
        let root = test_dir("sidebar-depth");
        let deep_folder = root.join("one/two");
        let deep_file = deep_folder.join("deep.md");
        std::fs::create_dir_all(&deep_folder).unwrap();
        std::fs::write(&deep_file, "# Deep\n").unwrap();

        let snapshot = build_workspace_with_limits(&root, false, 4, 2, 100, 100).unwrap();
        assert!(!snapshot.files.contains(&deep_file));
        assert!(snapshot.sidebar_files.contains(&deep_file));
        assert!(snapshot.root.children.iter().all(|node| node.is_dir));

        let rows = flatten_with_files(
            &snapshot.root,
            &snapshot.sidebar_files,
            &HashSet::from([root.clone(), root.join("one"), deep_folder.clone()]),
        );
        assert!(rows.iter().any(|row| row.node.path() == deep_file));

        std::fs::remove_dir_all(root).unwrap();
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
