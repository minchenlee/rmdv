//! Filesystem/workspace adapters for the shared mindmap canvas.
//!
//! This module owns path-based identity and visible-graph construction only.
//! It deliberately does not open files, mutate `App`, or reuse document
//! `BlockId` state.

use crate::mindmap::{self, MNode};
use crate::tree::{Node, RecursiveFileCount};
use iced::Size;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkspaceStatus {
    Empty,
    Error,
    Truncated,
    LoadingFiles,
    BranchTruncated,
}

/// Stable, filesystem-scoped canvas identity. These values never enter the
/// document mindmap's `BlockId` sets or preview-panel maps.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WorkspaceNodeId {
    Root(PathBuf),
    Folder(PathBuf),
    File(PathBuf),
    Status(PathBuf, WorkspaceStatus),
}

impl WorkspaceNodeId {
    /// Return the filesystem path represented by this graph identity.
    pub fn path(&self) -> &Path {
        match self {
            Self::Root(path) | Self::Folder(path) | Self::File(path) => path,
            Self::Status(path, _) => path,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceNodeKind {
    Root,
    Folder,
    File,
    Empty,
    Error,
    Truncated,
    Loading,
}

#[derive(Debug, Clone)]
pub enum MaterializedFolder {
    Loaded {
        folders: Arc<Vec<Node>>,
        files: Arc<Vec<PathBuf>>,
        recursive_supported_file_count: RecursiveFileCount,
        truncated: bool,
    },
    Error(String),
}

#[derive(Debug, Clone)]
pub struct WorkspaceNode {
    pub id: WorkspaceNodeId,
    pub kind: WorkspaceNodeKind,
    pub path: Option<PathBuf>,
    pub visible_children: usize,
    pub has_hidden_children: bool,
}

/// A visible, laid-out adapter graph plus domain metadata for app actions.
#[derive(Clone)]
pub struct WorkspaceGraph {
    pub nodes: Arc<Vec<MNode<WorkspaceNodeId>>>,
    pub content_size: Size,
    root: WorkspaceNodeId,
    by_id: HashMap<WorkspaceNodeId, WorkspaceNode>,
}

impl WorkspaceGraph {
    pub fn root_id(&self) -> WorkspaceNodeId {
        self.root.clone()
    }

    pub fn node(&self, id: &WorkspaceNodeId) -> Option<&WorkspaceNode> {
        self.by_id.get(id)
    }

    pub fn index_of(&self, id: &WorkspaceNodeId) -> Option<usize> {
        self.nodes
            .iter()
            .position(|node| node.id.as_ref() == Some(id))
    }

    fn parent_indices(&self) -> Vec<Option<usize>> {
        let mut parents = vec![None; self.nodes.len()];
        for (parent, node) in self.nodes.iter().enumerate() {
            for &child in &node.children {
                parents[child] = Some(parent);
            }
        }
        parents
    }

    pub fn parent(&self, id: &WorkspaceNodeId) -> Option<WorkspaceNodeId> {
        let idx = self.index_of(id)?;
        let parent = self.parent_indices().get(idx).copied().flatten()?;
        self.nodes.get(parent)?.id.clone()
    }

    /// Find the closest visible folder/root ancestor for a path that was
    /// removed from the accepted graph (for example, an unknown shell that a
    /// bounded retry proved exactly empty). The graph, rather than the source
    /// tree, is authoritative here: omitted exact-empty ancestors are skipped
    /// and the workspace root is returned only when it is the first visible
    /// ancestor.
    pub fn nearest_visible_ancestor(&self, path: &Path) -> Option<WorkspaceNodeId> {
        let root_path = self.node(&self.root)?.path.as_deref()?;
        let mut ancestor = path.parent();
        while let Some(candidate) = ancestor {
            if !candidate.starts_with(root_path) {
                break;
            }
            let id = if candidate == root_path {
                self.root.clone()
            } else {
                WorkspaceNodeId::Folder(candidate.to_path_buf())
            };
            if self.node(&id).is_some() {
                return Some(id);
            }
            if candidate == root_path {
                break;
            }
            ancestor = candidate.parent();
        }
        None
    }

    pub fn first_child(&self, id: &WorkspaceNodeId) -> Option<WorkspaceNodeId> {
        let idx = self.index_of(id)?;
        let child = *self.nodes.get(idx)?.children.first()?;
        self.nodes.get(child)?.id.clone()
    }

    pub fn sibling(&self, id: &WorkspaceNodeId, delta: isize) -> Option<WorkspaceNodeId> {
        let idx = self.index_of(id)?;
        let parent = self.parent_indices().get(idx).copied().flatten()?;
        let siblings = &self.nodes.get(parent)?.children;
        let pos = siblings.iter().position(|&child| child == idx)? as isize;
        let target = pos + delta;
        if target < 0 {
            return None;
        }
        let sibling = *siblings.get(target as usize)?;
        self.nodes.get(sibling)?.id.clone()
    }
}

struct Builder {
    nodes: Vec<MNode<WorkspaceNodeId>>,
    by_id: HashMap<WorkspaceNodeId, WorkspaceNode>,
}

impl Builder {
    fn push(
        &mut self,
        id: WorkspaceNodeId,
        kind: WorkspaceNodeKind,
        path: Option<PathBuf>,
        full_label: String,
        level: u8,
        has_hidden_children: bool,
    ) -> usize {
        let (label, truncated) = mindmap::fit_label_for_node(&full_label);
        let idx = self.nodes.len();
        self.nodes.push(MNode {
            id: Some(id.clone()),
            label,
            full_label,
            truncated,
            level,
            children: Vec::new(),
            has_hidden_children,
            x: 0.0,
            y: 0.0,
        });
        self.by_id.insert(
            id.clone(),
            WorkspaceNode {
                id,
                kind,
                path,
                visible_children: 0,
                has_hidden_children,
            },
        );
        idx
    }

    fn attach(&mut self, parent: usize, child: usize) {
        self.nodes[parent].children.push(child);
        if let Some(parent_id) = self.nodes[parent].id.as_ref() {
            if let Some(info) = self.by_id.get_mut(parent_id) {
                info.visible_children += 1;
            }
        }
    }

    fn finish(mut self, root: WorkspaceNodeId) -> WorkspaceGraph {
        let mut y_cursor = mindmap::PAD;
        mindmap::layout(&mut self.nodes, 0, &mut y_cursor);
        let max_level = self.nodes.iter().map(|node| node.level).max().unwrap_or(0) as f32;
        let width =
            mindmap::PAD * 2.0 + mindmap::NODE_W + max_level * (mindmap::NODE_W + mindmap::X_GAP);
        WorkspaceGraph {
            nodes: Arc::new(self.nodes),
            content_size: Size::new(width, y_cursor + mindmap::PAD),
            root,
            by_id: self.by_id,
        }
    }
}

fn status_label(status: WorkspaceStatus, detail: Option<&str>) -> String {
    match (status, detail) {
        (WorkspaceStatus::Empty, _) => "Empty folder".to_string(),
        (WorkspaceStatus::Error, Some(error)) => format!("⚠ {error}"),
        (WorkspaceStatus::Error, None) => "⚠ Couldn't read folder".to_string(),
        (WorkspaceStatus::Truncated, _) => "More files not indexed".to_string(),
        (WorkspaceStatus::LoadingFiles, _) => "Loading files…".to_string(),
        (WorkspaceStatus::BranchTruncated, _) => "More items not indexed".to_string(),
    }
}

fn has_discoverable_children(node: &Node) -> bool {
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

fn effective_folder_count(
    node: &Node,
    materialized: &HashMap<PathBuf, MaterializedFolder>,
) -> Option<RecursiveFileCount> {
    match materialized.get(&node.path) {
        Some(MaterializedFolder::Loaded {
            recursive_supported_file_count,
            ..
        }) => Some(*recursive_supported_file_count),
        // A branch-local verification that cannot read its folder is still a
        // visible folder with an unavailable count. Do not let the retained
        // LowerBound(0) shell continue to masquerade as "scan limit reached".
        Some(MaterializedFolder::Error(_)) => Some(RecursiveFileCount::Unavailable),
        _ => node.recursive_supported_file_count,
    }
}

fn should_show_folder(node: &Node, materialized: &HashMap<PathBuf, MaterializedFolder>) -> bool {
    node.is_dir
        && !matches!(
            effective_folder_count(node, materialized),
            Some(RecursiveFileCount::Exact(0))
        )
}

fn workspace_folder_label(
    node: &Node,
    expanded: bool,
    materialized: &HashMap<PathBuf, MaterializedFolder>,
) -> String {
    if expanded {
        return node.name.clone();
    }
    match effective_folder_count(node, materialized) {
        Some(RecursiveFileCount::Exact(count)) => {
            let noun = if count == 1 { "file" } else { "files" };
            format!("{} · {count} {noun}", node.name)
        }
        Some(RecursiveFileCount::LowerBound(0)) => {
            format!("{} · scan limit reached", node.name)
        }
        Some(RecursiveFileCount::LowerBound(count)) => {
            format!("{} · {count}+ files", node.name)
        }
        Some(RecursiveFileCount::Unavailable) => {
            format!("{} · count unavailable", node.name)
        }
        None => node.name.clone(),
    }
}

/// Adapt the existing prebuilt workspace tree. The caller owns expansion;
/// folders outside `expanded` remain present but contribute no visible children
/// and advertise their hidden descendants to the shared canvas.
/// Adapt the retained workspace tree without delayed verification hiding.
/// Kept as the compatibility entry point for document/sidebar tests and
/// callers that do not own Full Mindmap verification state.
pub fn from_tree(
    tree_root: &Node,
    expanded: &HashSet<PathBuf>,
    materialized: &HashMap<PathBuf, MaterializedFolder>,
    pending: &HashSet<PathBuf>,
    truncated: bool,
) -> WorkspaceGraph {
    from_tree_with_hidden(
        tree_root,
        expanded,
        materialized,
        pending,
        &HashSet::new(),
        truncated,
    )
}

/// Adapt the retained workspace tree while omitting unresolved LowerBound(0)
/// folders owned by a fixed Full Mindmap verification wave.
pub fn from_tree_with_hidden(
    tree_root: &Node,
    expanded: &HashSet<PathBuf>,
    materialized: &HashMap<PathBuf, MaterializedFolder>,
    pending: &HashSet<PathBuf>,
    hidden_unverified: &HashSet<PathBuf>,
    truncated: bool,
) -> WorkspaceGraph {
    let mut builder = Builder {
        nodes: Vec::new(),
        by_id: HashMap::new(),
    };
    let root_id = WorkspaceNodeId::Root(tree_root.path.clone());
    let root_expanded = expanded.contains(&tree_root.path);
    let root_label = workspace_folder_label(tree_root, root_expanded, materialized);
    let root_idx = builder.push(
        root_id.clone(),
        WorkspaceNodeKind::Root,
        Some(tree_root.path.clone()),
        root_label,
        0,
        has_discoverable_children(tree_root) && !root_expanded,
    );

    if root_expanded {
        for child in visible_folder_children(tree_root, materialized) {
            append_tree_node(
                &mut builder,
                root_idx,
                child,
                expanded,
                materialized,
                pending,
                hidden_unverified,
                1,
            );
        }
        append_materialized_children(
            &mut builder,
            root_idx,
            &tree_root.path,
            materialized,
            pending,
            1,
        );
        if truncated {
            let status = WorkspaceStatus::Truncated;
            let child = builder.push(
                WorkspaceNodeId::Status(tree_root.path.clone(), status),
                WorkspaceNodeKind::Truncated,
                None,
                status_label(status, None),
                1,
                false,
            );
            builder.attach(root_idx, child);
        }
        if builder.nodes[root_idx].children.is_empty()
            && matches!(
                effective_folder_count(tree_root, materialized),
                Some(RecursiveFileCount::Exact(0))
            )
        {
            let status = WorkspaceStatus::Empty;
            let child = builder.push(
                WorkspaceNodeId::Status(tree_root.path.clone(), status),
                WorkspaceNodeKind::Empty,
                None,
                status_label(status, None),
                1,
                false,
            );
            builder.attach(root_idx, child);
        }
    }

    builder.finish(root_id)
}

fn append_tree_node(
    builder: &mut Builder,
    parent: usize,
    node: &Node,
    expanded: &HashSet<PathBuf>,
    materialized: &HashMap<PathBuf, MaterializedFolder>,
    pending: &HashSet<PathBuf>,
    hidden_unverified: &HashSet<PathBuf>,
    level: u8,
) {
    // Production snapshots retain folders only, but test/fallback snapshots
    // may still carry a file child. Ignore it here rather than manufacturing a
    // folder identity or panicking while a user expands that branch.
    if !node.is_dir {
        return;
    }
    if !should_show_folder(node, materialized) || hidden_unverified.contains(&node.path) {
        return;
    }
    let id = WorkspaceNodeId::Folder(node.path.clone());
    let kind = WorkspaceNodeKind::Folder;
    let is_expanded = expanded.contains(&node.path);
    let has_hidden_children = has_discoverable_children(node) && !is_expanded;
    let idx = builder.push(
        id,
        kind,
        Some(node.path.clone()),
        workspace_folder_label(node, is_expanded, materialized),
        level,
        has_hidden_children,
    );
    builder.attach(parent, idx);
    if is_expanded {
        for child in visible_folder_children(node, materialized) {
            append_tree_node(
                builder,
                idx,
                child,
                expanded,
                materialized,
                pending,
                hidden_unverified,
                level.saturating_add(1),
            );
        }
        append_materialized_children(
            builder,
            idx,
            &node.path,
            materialized,
            pending,
            level.saturating_add(1),
        );
    }
}

fn visible_folder_children<'a>(
    node: &'a Node,
    materialized: &'a HashMap<PathBuf, MaterializedFolder>,
) -> &'a [Node] {
    match materialized.get(&node.path) {
        Some(MaterializedFolder::Loaded { folders, .. }) => folders.as_slice(),
        _ => &node.children,
    }
}

fn append_materialized_children(
    builder: &mut Builder,
    parent: usize,
    folder: &PathBuf,
    materialized: &HashMap<PathBuf, MaterializedFolder>,
    pending: &HashSet<PathBuf>,
    level: u8,
) {
    match materialized.get(folder) {
        Some(MaterializedFolder::Loaded {
            files, truncated, ..
        }) => {
            for path in files.iter() {
                let label = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_string_lossy().into_owned());
                let child = builder.push(
                    WorkspaceNodeId::File(path.clone()),
                    WorkspaceNodeKind::File,
                    Some(path.clone()),
                    label,
                    level,
                    false,
                );
                builder.attach(parent, child);
            }
            if *truncated {
                let status = WorkspaceStatus::BranchTruncated;
                let child = builder.push(
                    WorkspaceNodeId::Status(folder.clone(), status),
                    WorkspaceNodeKind::Truncated,
                    None,
                    status_label(status, None),
                    level,
                    false,
                );
                builder.attach(parent, child);
            }
        }
        Some(MaterializedFolder::Error(error)) => {
            let status = WorkspaceStatus::Error;
            let child = builder.push(
                WorkspaceNodeId::Status(folder.clone(), status),
                WorkspaceNodeKind::Error,
                None,
                status_label(status, Some(error)),
                level,
                false,
            );
            builder.attach(parent, child);
        }
        None if pending.contains(folder) => {
            let status = WorkspaceStatus::LoadingFiles;
            let child = builder.push(
                WorkspaceNodeId::Status(folder.clone(), status),
                WorkspaceNodeKind::Loading,
                None,
                status_label(status, None),
                level,
                false,
            );
            builder.attach(parent, child);
        }
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(path: &str, is_dir: bool, children: Vec<Node>) -> Node {
        let path = PathBuf::from(path);
        let count = children
            .iter()
            .map(|child| match child.recursive_supported_file_count {
                Some(RecursiveFileCount::Exact(count)) => count,
                _ if !child.is_dir => 1,
                _ => 0,
            })
            .sum();
        Node {
            name: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned()),
            path,
            is_dir,
            children,
            recursive_supported_file_count: is_dir.then_some(RecursiveFileCount::Exact(count)),
        }
    }

    fn test_graph(root: &Node, expanded: &HashSet<PathBuf>, truncated: bool) -> WorkspaceGraph {
        fn collect(node: &Node, out: &mut HashMap<PathBuf, MaterializedFolder>) {
            if !node.is_dir {
                return;
            }
            let files = node
                .children
                .iter()
                .filter(|child| !child.is_dir)
                .map(|child| child.path.clone())
                .collect::<Vec<_>>();
            out.insert(
                node.path.clone(),
                MaterializedFolder::Loaded {
                    folders: Arc::new(
                        node.children
                            .iter()
                            .filter(|child| child.is_dir)
                            .map(skeleton)
                            .collect(),
                    ),
                    files: Arc::new(files),
                    recursive_supported_file_count: node
                        .recursive_supported_file_count
                        .unwrap_or(RecursiveFileCount::Unavailable),
                    truncated: false,
                },
            );
            for child in node.children.iter().filter(|child| child.is_dir) {
                collect(child, out);
            }
        }

        fn skeleton(node: &Node) -> Node {
            Node {
                path: node.path.clone(),
                name: node.name.clone(),
                is_dir: node.is_dir,
                children: node
                    .children
                    .iter()
                    .filter(|child| child.is_dir)
                    .map(skeleton)
                    .collect(),
                recursive_supported_file_count: node.recursive_supported_file_count,
            }
        }

        let mut materialized = HashMap::new();
        collect(root, &mut materialized);
        from_tree(
            &skeleton(root),
            expanded,
            &materialized,
            &HashSet::new(),
            truncated,
        )
    }

    #[test]
    fn expanded_workspace_root_uses_plain_label() {
        let mut root = node(
            "/vault",
            true,
            vec![node("/vault/readme.md", false, vec![])],
        );
        root.recursive_supported_file_count = Some(RecursiveFileCount::LowerBound(5_000));
        let graph = test_graph(&root, &HashSet::from([PathBuf::from("/vault")]), true);
        let label = graph
            .index_of(&graph.root_id())
            .and_then(|index| graph.nodes.get(index))
            .map(|node| node.full_label.as_str());
        assert_eq!(label, Some("vault"));
    }

    #[test]
    fn collapsed_folders_render_exact_lower_bound_limit_and_unavailable_counts() {
        let mut exact = node("/vault/exact", true, vec![]);
        exact.recursive_supported_file_count = Some(RecursiveFileCount::Exact(1));
        let mut lower = node("/vault/lower", true, vec![]);
        lower.recursive_supported_file_count = Some(RecursiveFileCount::LowerBound(12));
        let mut limit = node("/vault/limit", true, vec![]);
        limit.recursive_supported_file_count = Some(RecursiveFileCount::LowerBound(0));
        let mut unavailable = node("/vault/unavailable", true, vec![]);
        unavailable.recursive_supported_file_count = Some(RecursiveFileCount::Unavailable);
        let root = node("/vault", true, vec![exact, lower, limit, unavailable]);

        let graph = test_graph(&root, &HashSet::from([PathBuf::from("/vault")]), true);
        let label = |path: &str| {
            graph
                .index_of(&WorkspaceNodeId::Folder(PathBuf::from(path)))
                .and_then(|index| graph.nodes.get(index))
                .map(|node| node.full_label.clone())
        };
        assert_eq!(label("/vault/exact").as_deref(), Some("exact · 1 file"));
        assert_eq!(label("/vault/lower").as_deref(), Some("lower · 12+ files"));
        assert_eq!(
            label("/vault/limit").as_deref(),
            Some("limit · scan limit reached")
        );
        assert_eq!(
            label("/vault/unavailable").as_deref(),
            Some("unavailable · count unavailable")
        );
    }

    #[test]
    fn expanded_folder_uses_plain_label_after_collapsed_count() {
        let mut folder = node(
            "/vault/notes",
            true,
            vec![node("/vault/notes/guide.md", false, vec![])],
        );
        folder.recursive_supported_file_count = Some(RecursiveFileCount::Exact(1));
        let root = node("/vault", true, vec![folder]);
        let collapsed = test_graph(&root, &HashSet::from([PathBuf::from("/vault")]), false);
        let id = WorkspaceNodeId::Folder(PathBuf::from("/vault/notes"));
        let collapsed_label = collapsed.nodes[collapsed.index_of(&id).unwrap()]
            .full_label
            .clone();
        assert_eq!(collapsed_label, "notes · 1 file");

        let expanded = test_graph(
            &root,
            &HashSet::from([PathBuf::from("/vault"), PathBuf::from("/vault/notes")]),
            false,
        );
        assert_eq!(
            expanded.nodes[expanded.index_of(&id).unwrap()].full_label,
            "notes"
        );
    }

    #[test]
    fn expansion_hides_only_descendants_of_collapsed_folder() {
        let root = node(
            "/vault",
            true,
            vec![
                node(
                    "/vault/src",
                    true,
                    vec![node("/vault/src/app.rs", false, vec![])],
                ),
                node("/vault/readme.md", false, vec![]),
            ],
        );
        let mut expanded = HashSet::from([PathBuf::from("/vault")]);
        let collapsed = test_graph(&root, &expanded, false);
        let src = WorkspaceNodeId::Folder(PathBuf::from("/vault/src"));
        let app = WorkspaceNodeId::File(PathBuf::from("/vault/src/app.rs"));
        assert!(collapsed.node(&src).unwrap().has_hidden_children);
        assert!(collapsed.node(&app).is_none());

        expanded.insert(PathBuf::from("/vault/src"));
        let open = test_graph(&root, &expanded, false);
        assert!(!open.node(&src).unwrap().has_hidden_children);
        assert!(open.node(&app).is_some());
    }

    #[test]
    fn path_identity_survives_unrelated_sibling_insertion() {
        let one = node(
            "/vault",
            true,
            vec![node(
                "/vault/src",
                true,
                vec![node("/vault/src/app.rs", false, vec![])],
            )],
        );
        let two = node(
            "/vault",
            true,
            vec![
                node(
                    "/vault/docs",
                    true,
                    vec![node("/vault/docs/guide.md", false, vec![])],
                ),
                node(
                    "/vault/src",
                    true,
                    vec![node("/vault/src/app.rs", false, vec![])],
                ),
            ],
        );
        let expanded = HashSet::from([PathBuf::from("/vault"), PathBuf::from("/vault/src")]);
        let id = WorkspaceNodeId::File(PathBuf::from("/vault/src/app.rs"));
        assert!(test_graph(&one, &expanded, false).node(&id).is_some());
        assert!(test_graph(&two, &expanded, false).node(&id).is_some());
    }

    #[test]
    fn graph_navigation_follows_parent_child_and_sibling_relationships() {
        let root = node(
            "/vault",
            true,
            vec![
                node("/vault/a.md", false, vec![]),
                node("/vault/b.md", false, vec![]),
            ],
        );
        let graph = test_graph(&root, &HashSet::from([PathBuf::from("/vault")]), false);
        let root_id = graph.root_id();
        let a = WorkspaceNodeId::File(PathBuf::from("/vault/a.md"));
        let b = WorkspaceNodeId::File(PathBuf::from("/vault/b.md"));
        assert_eq!(graph.first_child(&root_id), Some(a.clone()));
        assert_eq!(graph.parent(&a), Some(root_id));
        assert_eq!(graph.sibling(&a, 1), Some(b));
    }

    #[test]
    fn truncated_workspace_has_an_explicit_status_node() {
        let root = node(
            "/vault",
            true,
            vec![node("/vault/readme.md", false, vec![])],
        );
        let graph = test_graph(&root, &HashSet::from([PathBuf::from("/vault")]), true);

        assert!(graph
            .node(&WorkspaceNodeId::Status(
                PathBuf::from("/vault"),
                WorkspaceStatus::Truncated,
            ))
            .is_some());
    }

    #[test]
    fn expanded_folder_shows_stable_loading_then_only_its_accepted_files() {
        let mut folder = node("/vault/notes", true, vec![]);
        folder.recursive_supported_file_count = Some(RecursiveFileCount::Exact(1));
        let root = node("/vault", true, vec![folder]);
        let expanded = HashSet::from([PathBuf::from("/vault"), PathBuf::from("/vault/notes")]);
        let pending = HashSet::from([PathBuf::from("/vault/notes")]);
        let loading = from_tree(&root, &expanded, &HashMap::new(), &pending, false);
        assert!(loading
            .node(&WorkspaceNodeId::Status(
                PathBuf::from("/vault/notes"),
                WorkspaceStatus::LoadingFiles,
            ))
            .is_some());
        assert!(loading
            .node(&WorkspaceNodeId::File(PathBuf::from("/vault/notes/a.md")))
            .is_none());

        let materialized = HashMap::from([(
            PathBuf::from("/vault/notes"),
            MaterializedFolder::Loaded {
                folders: Arc::new(Vec::new()),
                files: Arc::new(vec![PathBuf::from("/vault/notes/a.md")]),
                recursive_supported_file_count: RecursiveFileCount::Exact(1),
                truncated: false,
            },
        )]);
        let loaded = from_tree(&root, &expanded, &materialized, &HashSet::new(), false);
        assert!(loaded
            .node(&WorkspaceNodeId::File(PathBuf::from("/vault/notes/a.md")))
            .is_some());
        assert!(loaded
            .node(&WorkspaceNodeId::Status(
                PathBuf::from("/vault/notes"),
                WorkspaceStatus::LoadingFiles,
            ))
            .is_none());
    }

    #[test]
    fn collapsed_folder_never_exposes_retained_materialized_files() {
        let mut folder = node("/vault/notes", true, vec![]);
        folder.recursive_supported_file_count = Some(RecursiveFileCount::Exact(1));
        let root = node("/vault", true, vec![folder]);
        let materialized = HashMap::from([(
            PathBuf::from("/vault/notes"),
            MaterializedFolder::Loaded {
                folders: Arc::new(Vec::new()),
                files: Arc::new(vec![PathBuf::from("/vault/notes/a.md")]),
                recursive_supported_file_count: RecursiveFileCount::Exact(1),
                truncated: false,
            },
        )]);
        let graph = from_tree(
            &root,
            &HashSet::from([PathBuf::from("/vault")]),
            &materialized,
            &HashSet::new(),
            false,
        );
        assert!(graph
            .node(&WorkspaceNodeId::File(PathBuf::from("/vault/notes/a.md")))
            .is_none());
        assert!(
            graph
                .node(&WorkspaceNodeId::Folder(PathBuf::from("/vault/notes")))
                .unwrap()
                .has_hidden_children
        );
    }

    #[test]
    fn exact_empty_folders_are_hidden_while_interrupted_unknowns_remain() {
        let mut empty = node("/vault/empty", true, vec![]);
        empty.recursive_supported_file_count = Some(RecursiveFileCount::Exact(0));
        let mut unknown = node("/vault/unknown", true, vec![]);
        unknown.recursive_supported_file_count = Some(RecursiveFileCount::LowerBound(0));
        let root = node("/vault", true, vec![empty, unknown]);

        let graph = from_tree(
            &root,
            &HashSet::from([PathBuf::from("/vault")]),
            &HashMap::new(),
            &HashSet::new(),
            true,
        );

        assert!(graph
            .node(&WorkspaceNodeId::Folder(PathBuf::from("/vault/empty")))
            .is_none());
        assert!(graph
            .node(&WorkspaceNodeId::Folder(PathBuf::from("/vault/unknown")))
            .is_some());
    }

    #[test]
    fn delayed_reveal_hides_only_unresolved_zero_lower_bounds() {
        let mut pending = node("/vault/pending", true, vec![]);
        pending.recursive_supported_file_count = Some(RecursiveFileCount::LowerBound(0));
        let mut positive = node("/vault/positive", true, vec![]);
        positive.recursive_supported_file_count = Some(RecursiveFileCount::LowerBound(2));
        let mut unavailable = node("/vault/unavailable", true, vec![]);
        unavailable.recursive_supported_file_count = Some(RecursiveFileCount::Unavailable);
        let root = node("/vault", true, vec![pending, positive, unavailable]);
        let hidden = HashSet::from([PathBuf::from("/vault/pending")]);

        let graph = from_tree_with_hidden(
            &root,
            &HashSet::from([PathBuf::from("/vault")]),
            &HashMap::new(),
            &HashSet::new(),
            &hidden,
            false,
        );

        assert!(graph
            .node(&WorkspaceNodeId::Folder(PathBuf::from("/vault/pending")))
            .is_none());
        assert_eq!(
            graph
                .node(&WorkspaceNodeId::Folder(PathBuf::from("/vault/positive")))
                .map(|node| node.kind.clone()),
            Some(WorkspaceNodeKind::Folder)
        );
        assert!(graph
            .node(&WorkspaceNodeId::Folder(PathBuf::from(
                "/vault/unavailable"
            )))
            .is_some());
    }

    #[test]
    fn verification_error_relabels_zero_lower_bound_as_unavailable() {
        let mut shell = node("/vault/locked", true, vec![]);
        shell.recursive_supported_file_count = Some(RecursiveFileCount::LowerBound(0));
        let root = node("/vault", true, vec![shell]);
        let materialized = HashMap::from([(
            PathBuf::from("/vault/locked"),
            MaterializedFolder::Error("permission denied".into()),
        )]);
        let graph = from_tree(
            &root,
            &HashSet::from([PathBuf::from("/vault")]),
            &materialized,
            &HashSet::new(),
            false,
        );
        let id = WorkspaceNodeId::Folder(PathBuf::from("/vault/locked"));
        let idx = graph.index_of(&id).expect("error folder remains visible");
        assert_eq!(graph.nodes[idx].full_label, "locked · count unavailable");
    }

    #[test]
    fn completed_lazy_scan_removes_a_previously_unknown_exact_empty_folder() {
        let mut shell = node("/vault/unknown", true, vec![]);
        shell.recursive_supported_file_count = Some(RecursiveFileCount::LowerBound(0));
        let root = node("/vault", true, vec![shell]);
        let materialized = HashMap::from([(
            PathBuf::from("/vault/unknown"),
            MaterializedFolder::Loaded {
                folders: Arc::new(Vec::new()),
                files: Arc::new(Vec::new()),
                recursive_supported_file_count: RecursiveFileCount::Exact(0),
                truncated: false,
            },
        )]);

        let graph = from_tree(
            &root,
            &HashSet::from([PathBuf::from("/vault"), PathBuf::from("/vault/unknown")]),
            &materialized,
            &HashSet::new(),
            true,
        );
        assert!(graph
            .node(&WorkspaceNodeId::Folder(PathBuf::from("/vault/unknown")))
            .is_none());
    }

    #[test]
    fn removed_shell_uses_nearest_visible_ancestor_before_root_fallback() {
        let shell_path = PathBuf::from("/vault/Documents/Shopee");
        let documents_path = PathBuf::from("/vault/Documents");
        let mut shell = node(shell_path.to_str().unwrap(), true, vec![]);
        shell.recursive_supported_file_count = Some(RecursiveFileCount::LowerBound(0));
        let mut documents = node(documents_path.to_str().unwrap(), true, vec![shell.clone()]);
        documents.recursive_supported_file_count = Some(RecursiveFileCount::LowerBound(0));
        let mut root = node("/vault", true, vec![documents]);
        root.recursive_supported_file_count = Some(RecursiveFileCount::LowerBound(0));
        let materialized = HashMap::from([(
            shell_path.clone(),
            MaterializedFolder::Loaded {
                folders: Arc::new(Vec::new()),
                files: Arc::new(Vec::new()),
                recursive_supported_file_count: RecursiveFileCount::Exact(0),
                truncated: false,
            },
        )]);
        let graph = from_tree(
            &root,
            &HashSet::from([PathBuf::from("/vault"), documents_path.clone(), shell_path]),
            &materialized,
            &HashSet::new(),
            true,
        );
        assert_eq!(
            graph.nearest_visible_ancestor(PathBuf::from("/vault/Documents/Shopee").as_path()),
            Some(WorkspaceNodeId::Folder(documents_path.clone()))
        );

        let mut direct_shell = node("/vault/direct", true, vec![]);
        direct_shell.recursive_supported_file_count = Some(RecursiveFileCount::LowerBound(0));
        let mut direct_root = node("/vault", true, vec![direct_shell]);
        direct_root.recursive_supported_file_count = Some(RecursiveFileCount::LowerBound(0));
        let direct_materialized = HashMap::from([(
            PathBuf::from("/vault/direct"),
            MaterializedFolder::Loaded {
                folders: Arc::new(Vec::new()),
                files: Arc::new(Vec::new()),
                recursive_supported_file_count: RecursiveFileCount::Exact(0),
                truncated: false,
            },
        )]);
        let direct_graph = from_tree(
            &direct_root,
            &HashSet::from([PathBuf::from("/vault"), PathBuf::from("/vault/direct")]),
            &direct_materialized,
            &HashSet::new(),
            true,
        );
        assert_eq!(
            direct_graph.nearest_visible_ancestor(PathBuf::from("/vault/direct").as_path()),
            Some(direct_graph.root_id())
        );
    }

    #[test]
    fn loaded_interrupted_branch_renders_discovered_folders_files_and_status() {
        let mut shell = node("/vault/limited", true, vec![]);
        shell.recursive_supported_file_count = Some(RecursiveFileCount::LowerBound(0));
        let root = node("/vault", true, vec![shell]);
        let mut notes = node("/vault/limited/notes", true, vec![]);
        notes.recursive_supported_file_count = Some(RecursiveFileCount::Exact(1));
        let materialized = HashMap::from([(
            PathBuf::from("/vault/limited"),
            MaterializedFolder::Loaded {
                folders: Arc::new(vec![notes]),
                files: Arc::new(vec![PathBuf::from("/vault/limited/readme.md")]),
                recursive_supported_file_count: RecursiveFileCount::LowerBound(2),
                truncated: true,
            },
        )]);

        let graph = from_tree(
            &root,
            &HashSet::from([PathBuf::from("/vault"), PathBuf::from("/vault/limited")]),
            &materialized,
            &HashSet::new(),
            true,
        );

        assert!(graph
            .node(&WorkspaceNodeId::Folder(PathBuf::from(
                "/vault/limited/notes"
            )))
            .is_some());
        assert!(graph
            .node(&WorkspaceNodeId::File(PathBuf::from(
                "/vault/limited/readme.md"
            )))
            .is_some());
        assert!(graph
            .node(&WorkspaceNodeId::Status(
                PathBuf::from("/vault/limited"),
                WorkspaceStatus::BranchTruncated,
            ))
            .is_some());
    }

    #[test]
    fn exact_branch_reuses_retained_folders_with_indexed_immediate_files() {
        let mut nested = node("/vault/notes/nested", true, vec![]);
        nested.recursive_supported_file_count = Some(RecursiveFileCount::Exact(1));
        let notes = node("/vault/notes", true, vec![nested.clone()]);
        let root = node("/vault", true, vec![notes]);
        let materialized = HashMap::from([(
            PathBuf::from("/vault/notes"),
            MaterializedFolder::Loaded {
                folders: Arc::new(vec![nested]),
                files: Arc::new(vec![PathBuf::from("/vault/notes/readme.md")]),
                recursive_supported_file_count: RecursiveFileCount::Exact(2),
                truncated: false,
            },
        )]);
        let graph = from_tree(
            &root,
            &HashSet::from([PathBuf::from("/vault"), PathBuf::from("/vault/notes")]),
            &materialized,
            &HashSet::new(),
            false,
        );

        assert!(graph
            .node(&WorkspaceNodeId::Folder(PathBuf::from(
                "/vault/notes/nested"
            )))
            .is_some());
        assert!(graph
            .node(&WorkspaceNodeId::File(PathBuf::from(
                "/vault/notes/readme.md"
            )))
            .is_some());
    }

    #[test]
    fn interrupted_branch_without_discoveries_has_truthful_terminal_status() {
        let mut shell = node("/vault/limited", true, vec![]);
        shell.recursive_supported_file_count = Some(RecursiveFileCount::LowerBound(0));
        let root = node("/vault", true, vec![shell]);
        let materialized = HashMap::from([(
            PathBuf::from("/vault/limited"),
            MaterializedFolder::Loaded {
                folders: Arc::new(Vec::new()),
                files: Arc::new(Vec::new()),
                recursive_supported_file_count: RecursiveFileCount::LowerBound(0),
                truncated: true,
            },
        )]);

        let graph = from_tree(
            &root,
            &HashSet::from([PathBuf::from("/vault"), PathBuf::from("/vault/limited")]),
            &materialized,
            &HashSet::new(),
            true,
        );
        let status = WorkspaceNodeId::Status(
            PathBuf::from("/vault/limited"),
            WorkspaceStatus::BranchTruncated,
        );
        assert_eq!(
            graph
                .index_of(&status)
                .and_then(|index| graph.nodes.get(index))
                .map(|node| node.full_label.as_str()),
            Some("More items not indexed")
        );
    }
}
