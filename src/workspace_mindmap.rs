//! Filesystem/workspace adapters for the shared mindmap canvas.
//!
//! This module owns path-based identity and visible-graph construction only.
//! It deliberately does not open files, mutate `App`, or reuse document
//! `BlockId` state.

use crate::mindmap::{self, MNode};
use crate::picker::{Entry, Picker};
use crate::tree::Node;
use iced::Size;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkspaceStatus {
    Empty,
    Error,
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
    pub nodes: Vec<MNode<WorkspaceNodeId>>,
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
            nodes: self.nodes,
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
    }
}

/// Adapt the folder-only picker into a shallow mindmap graph. A picker knows
/// only immediate directory entries; selecting/activating a folder changes the
/// picker directory and rebuilds this graph.
pub fn from_picker(picker: &Picker) -> WorkspaceGraph {
    let mut builder = Builder {
        nodes: Vec::new(),
        by_id: HashMap::new(),
    };
    let root_path = picker.cwd.clone();
    let root_id = WorkspaceNodeId::Root(root_path.clone());
    let root_label = root_path
        .file_name()
        .map(|part| part.to_string_lossy().into_owned())
        .unwrap_or_else(|| root_path.to_string_lossy().into_owned());
    let root_idx = builder.push(
        root_id.clone(),
        WorkspaceNodeKind::Root,
        Some(root_path.clone()),
        root_label,
        0,
        false,
    );

    if let Some(error) = picker.error.as_deref() {
        let status = WorkspaceStatus::Error;
        let child = builder.push(
            WorkspaceNodeId::Status(root_path, status),
            WorkspaceNodeKind::Error,
            None,
            status_label(status, Some(error)),
            1,
            false,
        );
        builder.attach(root_idx, child);
    } else {
        let folders: Vec<&Entry> = picker.entries.iter().filter(|entry| entry.is_dir).collect();
        if folders.is_empty() {
            let status = WorkspaceStatus::Empty;
            let child = builder.push(
                WorkspaceNodeId::Status(root_path, status),
                WorkspaceNodeKind::Empty,
                None,
                status_label(status, None),
                1,
                false,
            );
            builder.attach(root_idx, child);
        } else {
            for entry in folders {
                let child = builder.push(
                    WorkspaceNodeId::Folder(entry.path.clone()),
                    WorkspaceNodeKind::Folder,
                    Some(entry.path.clone()),
                    entry.name.clone(),
                    1,
                    false,
                );
                builder.attach(root_idx, child);
            }
        }
    }

    builder.finish(root_id)
}

/// Adapt the existing prebuilt workspace tree. The caller owns expansion;
/// folders outside `expanded` remain present but contribute no visible children
/// and advertise their hidden descendants to the shared canvas.
pub fn from_tree(tree_root: &Node, expanded: &HashSet<PathBuf>) -> WorkspaceGraph {
    let mut builder = Builder {
        nodes: Vec::new(),
        by_id: HashMap::new(),
    };
    let root_id = WorkspaceNodeId::Root(tree_root.path.clone());
    let root_expanded = expanded.contains(&tree_root.path);
    let root_idx = builder.push(
        root_id.clone(),
        WorkspaceNodeKind::Root,
        Some(tree_root.path.clone()),
        tree_root.name.clone(),
        0,
        !tree_root.children.is_empty() && !root_expanded,
    );

    if tree_root.children.is_empty() {
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
    } else if root_expanded {
        for child in &tree_root.children {
            append_tree_node(&mut builder, root_idx, child, expanded, 1);
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
        node.name.clone(),
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
    use crate::picker::PickerMode;

    fn node(path: &str, is_dir: bool, children: Vec<Node>) -> Node {
        let path = PathBuf::from(path);
        Node {
            name: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned()),
            path,
            is_dir,
            children,
        }
    }

    #[test]
    fn picker_graph_shows_only_folders_and_preserves_paths() {
        let picker = Picker {
            cwd: PathBuf::from("/home/user"),
            entries: vec![
                Entry {
                    name: "project".into(),
                    path: PathBuf::from("/home/user/project"),
                    is_dir: true,
                    is_md: false,
                },
                Entry {
                    name: "note.md".into(),
                    path: PathBuf::from("/home/user/note.md"),
                    is_dir: false,
                    is_md: true,
                },
            ],
            selected: 0,
            error: None,
            mode: PickerMode::Folder,
            show_hidden: false,
        };

        let graph = from_picker(&picker);
        assert_eq!(graph.nodes.len(), 2);
        assert!(graph
            .node(&WorkspaceNodeId::Folder(PathBuf::from(
                "/home/user/project"
            )))
            .is_some());
        assert!(graph
            .node(&WorkspaceNodeId::File(PathBuf::from("/home/user/note.md")))
            .is_none());
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
        let collapsed = from_tree(&root, &expanded);
        let src = WorkspaceNodeId::Folder(PathBuf::from("/vault/src"));
        let app = WorkspaceNodeId::File(PathBuf::from("/vault/src/app.rs"));
        assert!(collapsed.node(&src).unwrap().has_hidden_children);
        assert!(collapsed.node(&app).is_none());

        expanded.insert(PathBuf::from("/vault/src"));
        let open = from_tree(&root, &expanded);
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
        assert!(from_tree(&one, &expanded).node(&id).is_some());
        assert!(from_tree(&two, &expanded).node(&id).is_some());
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
        let graph = from_tree(&root, &HashSet::from([PathBuf::from("/vault")]));
        let root_id = graph.root_id();
        let a = WorkspaceNodeId::File(PathBuf::from("/vault/a.md"));
        let b = WorkspaceNodeId::File(PathBuf::from("/vault/b.md"));
        assert_eq!(graph.first_child(&root_id), Some(a.clone()));
        assert_eq!(graph.parent(&a), Some(root_id));
        assert_eq!(graph.sibling(&a, 1), Some(b));
    }

    #[test]
    fn empty_and_error_picker_graphs_have_recovery_status_nodes() {
        let mut picker = Picker::new(Some(PathBuf::from("/tmp")), PickerMode::Folder, false);
        picker.entries.clear();
        picker.error = None;
        let empty = from_picker(&picker);
        assert!(empty
            .node(&WorkspaceNodeId::Status(
                picker.cwd.clone(),
                WorkspaceStatus::Empty
            ))
            .is_some());

        picker.error = Some("permission denied".into());
        let error = from_picker(&picker);
        assert!(error
            .node(&WorkspaceNodeId::Status(
                picker.cwd.clone(),
                WorkspaceStatus::Error
            ))
            .is_some());
    }
}
