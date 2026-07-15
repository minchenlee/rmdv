//! Filesystem/workspace adapters for the shared mindmap canvas.
//!
//! This module owns path-based identity and visible-graph construction only.
//! It deliberately does not open files, mutate `App`, or reuse document
//! `BlockId` state.

use crate::mindmap::{self, MNode};
use crate::tree::{Node, RecursiveFileCount};
use iced::Size;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkspaceStatus {
    Empty,
    Error,
    Truncated,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceNodeKind {
    Root,
    Folder,
    File,
    Empty,
    Error,
    Truncated,
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
    }
}

fn workspace_folder_label(node: &Node, expanded: bool) -> String {
    if expanded {
        return node.name.clone();
    }
    match node.recursive_supported_file_count {
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
pub fn from_tree(tree_root: &Node, expanded: &HashSet<PathBuf>, truncated: bool) -> WorkspaceGraph {
    let mut builder = Builder {
        nodes: Vec::new(),
        by_id: HashMap::new(),
    };
    let root_id = WorkspaceNodeId::Root(tree_root.path.clone());
    let root_expanded = expanded.contains(&tree_root.path);
    let root_label = workspace_folder_label(tree_root, root_expanded);
    let root_idx = builder.push(
        root_id.clone(),
        WorkspaceNodeKind::Root,
        Some(tree_root.path.clone()),
        root_label,
        0,
        !tree_root.children.is_empty() && !root_expanded,
    );

    if tree_root.children.is_empty() {
        let status = if truncated {
            WorkspaceStatus::Truncated
        } else {
            WorkspaceStatus::Empty
        };
        let kind = if truncated {
            WorkspaceNodeKind::Truncated
        } else {
            WorkspaceNodeKind::Empty
        };
        let child = builder.push(
            WorkspaceNodeId::Status(tree_root.path.clone(), status),
            kind,
            None,
            status_label(status, None),
            1,
            false,
        );
        builder.attach(root_idx, child);
    } else if root_expanded {
        for child in &tree_root.children {
            append_tree_node(&mut builder, root_idx, child, expanded, 1);
        }
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
    }

    builder.finish(root_id)
}

fn append_tree_node(
    builder: &mut Builder,
    parent: usize,
    node: &Node,
    expanded: &HashSet<PathBuf>,
    level: u8,
) {
    let (id, kind) = if node.is_dir {
        (
            WorkspaceNodeId::Folder(node.path.clone()),
            WorkspaceNodeKind::Folder,
        )
    } else {
        (
            WorkspaceNodeId::File(node.path.clone()),
            WorkspaceNodeKind::File,
        )
    };
    let is_expanded = node.is_dir && expanded.contains(&node.path);
    let has_hidden_children = node.is_dir && !node.children.is_empty() && !is_expanded;
    let idx = builder.push(
        id,
        kind,
        Some(node.path.clone()),
        workspace_folder_label(node, is_expanded),
        level,
        has_hidden_children,
    );
    builder.attach(parent, idx);
    if node.is_dir && is_expanded {
        for child in &node.children {
            append_tree_node(builder, idx, child, expanded, level.saturating_add(1));
        }
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

    #[test]
    fn expanded_workspace_root_uses_plain_label() {
        let mut root = node(
            "/vault",
            true,
            vec![node("/vault/readme.md", false, vec![])],
        );
        root.recursive_supported_file_count = Some(RecursiveFileCount::LowerBound(5_000));
        let graph = from_tree(&root, &HashSet::from([PathBuf::from("/vault")]), true);
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

        let graph = from_tree(&root, &HashSet::from([PathBuf::from("/vault")]), true);
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
        let collapsed = from_tree(&root, &HashSet::from([PathBuf::from("/vault")]), false);
        let id = WorkspaceNodeId::Folder(PathBuf::from("/vault/notes"));
        let collapsed_label = collapsed.nodes[collapsed.index_of(&id).unwrap()]
            .full_label
            .clone();
        assert_eq!(collapsed_label, "notes · 1 file");

        let expanded = from_tree(
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
        let collapsed = from_tree(&root, &expanded, false);
        let src = WorkspaceNodeId::Folder(PathBuf::from("/vault/src"));
        let app = WorkspaceNodeId::File(PathBuf::from("/vault/src/app.rs"));
        assert!(collapsed.node(&src).unwrap().has_hidden_children);
        assert!(collapsed.node(&app).is_none());

        expanded.insert(PathBuf::from("/vault/src"));
        let open = from_tree(&root, &expanded, false);
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
        assert!(from_tree(&one, &expanded, false).node(&id).is_some());
        assert!(from_tree(&two, &expanded, false).node(&id).is_some());
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
        let graph = from_tree(&root, &HashSet::from([PathBuf::from("/vault")]), false);
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
        let graph = from_tree(&root, &HashSet::from([PathBuf::from("/vault")]), true);

        assert!(graph
            .node(&WorkspaceNodeId::Status(
                PathBuf::from("/vault"),
                WorkspaceStatus::Truncated,
            ))
            .is_some());
    }
}
