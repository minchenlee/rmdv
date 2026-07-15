//! Filesystem/workspace adapters for the shared mindmap canvas.
//!
//! This module owns path-based identity and visible-graph construction only.
//! It deliberately does not open files, mutate `App`, or reuse document
//! `BlockId` state.

use crate::mindmap::{self, MNode};
use crate::picker::{Entry, Picker, SupportedFileCount};
use crate::tree::Node;
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

    /// Folder choosers can have dozens of immediate siblings. Aligning the
    /// selected root with the first child keeps the start of the ordinary
    /// folder list in view; keyboard selection still autocenters each later
    /// child independently.
    fn finish_picker(mut self, root: WorkspaceNodeId, root_idx: usize) -> WorkspaceGraph {
        let mut y_cursor = mindmap::PAD;
        mindmap::layout(&mut self.nodes, root_idx, &mut y_cursor);
        if let Some(first_child) = self.nodes[root_idx].children.first().copied() {
            self.nodes[root_idx].y = self.nodes[first_child].y;
        }
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

fn picker_folder_label(
    name: &str,
    path: &PathBuf,
    file_counts: &HashMap<PathBuf, SupportedFileCount>,
    unavailable_count_paths: &HashSet<PathBuf>,
    pending_count_path: Option<&PathBuf>,
) -> String {
    if pending_count_path.is_some_and(|pending| pending == path) {
        return format!("{name} · counting files…");
    }
    if unavailable_count_paths.contains(path) {
        return format!("{name} · count unavailable");
    }
    let Some(count) = file_counts.get(path) else {
        return name.to_string();
    };
    if count.capped && count.count == 0 {
        // The entry budget can end before we encounter a supported file. Do
        // not imply an impossible "0+ files" result or that the folder is
        // empty; the count is simply incomplete.
        return format!("{name} · scan limit reached");
    }
    let amount = if count.capped {
        format!("{}+", count.count)
    } else {
        count.count.to_string()
    };
    let noun = if count.count == 1 && !count.capped {
        "file"
    } else {
        "files"
    };
    format!("{name} · {amount} {noun}")
}

/// Adapt the folder-only picker into a shallow mindmap graph. A picker knows
/// only immediate directory entries; selecting/activating a folder changes the
/// picker directory and rebuilds this graph. `file_counts` contains only
/// asynchronously collected, chooser-scoped metadata, so graph construction
/// never walks the filesystem itself.
pub fn from_picker(
    picker: &Picker,
    file_counts: &HashMap<PathBuf, SupportedFileCount>,
    unavailable_count_paths: &HashSet<PathBuf>,
    pending_count_path: Option<&PathBuf>,
) -> WorkspaceGraph {
    let mut builder = Builder {
        nodes: Vec::new(),
        by_id: HashMap::new(),
    };
    let root_path = picker.cwd.clone();
    let root_id = WorkspaceNodeId::Root(root_path.clone());
    let root_name = root_path
        .file_name()
        .map(|part| part.to_string_lossy().into_owned())
        .unwrap_or_else(|| root_path.to_string_lossy().into_owned());
    let root_label = picker_folder_label(
        &root_name,
        &root_path,
        file_counts,
        unavailable_count_paths,
        pending_count_path,
    );
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
        let root_has_known_files = file_counts
            .get(&root_path)
            .is_some_and(|count| count.count > 0 || count.capped);
        let root_is_counting = pending_count_path.is_some_and(|path| path == &root_path);
        let root_count_unavailable = unavailable_count_paths.contains(&root_path);
        if folders.is_empty()
            && !root_has_known_files
            && !root_is_counting
            && !root_count_unavailable
        {
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
                    picker_folder_label(
                        &entry.name,
                        &entry.path,
                        file_counts,
                        unavailable_count_paths,
                        pending_count_path,
                    ),
                    1,
                    false,
                );
                builder.attach(root_idx, child);
            }
        }
    }

    builder.finish_picker(root_id, root_idx)
}

/// Adapt the existing prebuilt workspace tree. The caller owns expansion;
/// folders outside `expanded` remain present but contribute no visible children
/// and advertise their hidden descendants to the shared canvas.
pub fn from_tree(
    tree_root: &Node,
    expanded: &HashSet<PathBuf>,
    truncated: bool,
    supported_file_count: Option<SupportedFileCount>,
) -> WorkspaceGraph {
    let mut builder = Builder {
        nodes: Vec::new(),
        by_id: HashMap::new(),
    };
    let root_id = WorkspaceNodeId::Root(tree_root.path.clone());
    let root_expanded = expanded.contains(&tree_root.path);
    let root_label = supported_file_count.map_or_else(
        || tree_root.name.clone(),
        |count| {
            let counts = HashMap::from([(tree_root.path.clone(), count)]);
            picker_folder_label(
                &tree_root.name,
                &tree_root.path,
                &counts,
                &HashSet::new(),
                None,
            )
        },
    );
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

        let graph = from_picker(&picker, &HashMap::new(), &HashSet::new(), None);
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
    fn picker_root_focus_starts_at_first_ordinary_folder() {
        let picker = Picker {
            cwd: PathBuf::from("/Users/example"),
            entries: vec![
                Entry {
                    name: "Documents".into(),
                    path: PathBuf::from("/Users/example/Documents"),
                    is_dir: true,
                    is_md: false,
                },
                Entry {
                    name: ".cache".into(),
                    path: PathBuf::from("/Users/example/.cache"),
                    is_dir: true,
                    is_md: false,
                },
            ],
            selected: 0,
            error: None,
            mode: PickerMode::Folder,
            show_hidden: true,
        };
        let graph = from_picker(&picker, &HashMap::new(), &HashSet::new(), None);
        let root = graph.index_of(&graph.root_id()).unwrap();
        let documents = graph
            .index_of(&WorkspaceNodeId::Folder(PathBuf::from(
                "/Users/example/Documents",
            )))
            .unwrap();
        assert_eq!(graph.nodes[root].y, graph.nodes[documents].y);
    }

    #[test]
    fn workspace_root_retains_bounded_supported_file_count() {
        let root = node(
            "/vault",
            true,
            vec![node("/vault/readme.md", false, vec![])],
        );
        let graph = from_tree(
            &root,
            &HashSet::from([PathBuf::from("/vault")]),
            true,
            Some(SupportedFileCount {
                count: 5_000,
                capped: true,
            }),
        );
        let label = graph
            .index_of(&graph.root_id())
            .and_then(|index| graph.nodes.get(index))
            .map(|node| node.full_label.as_str());
        assert_eq!(label, Some("vault · 5000+ files"));
    }

    #[test]
    fn capped_zero_file_count_is_rendered_as_incomplete_not_zero_plus() {
        let path = PathBuf::from("/home/user/wide-folder");
        let counts = HashMap::from([(
            path.clone(),
            SupportedFileCount {
                count: 0,
                capped: true,
            },
        )]);

        assert_eq!(
            picker_folder_label("wide-folder", &path, &counts, &HashSet::new(), None),
            "wide-folder · scan limit reached"
        );
    }

    #[test]
    fn unavailable_file_count_is_not_rendered_as_empty() {
        let path = PathBuf::from("/home/user/restricted");
        let unavailable = HashSet::from([path.clone()]);

        assert_eq!(
            picker_folder_label("restricted", &path, &HashMap::new(), &unavailable, None,),
            "restricted · count unavailable"
        );
    }

    #[test]
    fn unavailable_root_count_does_not_add_an_empty_folder_status() {
        let mut picker = Picker::new(
            Some(PathBuf::from("/tmp/rmdv-unavailable-root")),
            PickerMode::Folder,
            false,
        );
        picker.entries.clear();
        picker.error = None;
        let unavailable = HashSet::from([picker.cwd.clone()]);

        let graph = from_picker(&picker, &HashMap::new(), &unavailable, None);
        let root = WorkspaceNodeId::Root(picker.cwd.clone());
        let root_label = graph
            .index_of(&root)
            .and_then(|index| graph.nodes.get(index))
            .map(|node| node.full_label.as_str());
        assert_eq!(
            root_label,
            Some("rmdv-unavailable-root · count unavailable")
        );
        assert!(graph
            .node(&WorkspaceNodeId::Status(
                picker.cwd.clone(),
                WorkspaceStatus::Empty
            ))
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
        let collapsed = from_tree(&root, &expanded, false, None);
        let src = WorkspaceNodeId::Folder(PathBuf::from("/vault/src"));
        let app = WorkspaceNodeId::File(PathBuf::from("/vault/src/app.rs"));
        assert!(collapsed.node(&src).unwrap().has_hidden_children);
        assert!(collapsed.node(&app).is_none());

        expanded.insert(PathBuf::from("/vault/src"));
        let open = from_tree(&root, &expanded, false, None);
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
        assert!(from_tree(&one, &expanded, false, None).node(&id).is_some());
        assert!(from_tree(&two, &expanded, false, None).node(&id).is_some());
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
        let graph = from_tree(
            &root,
            &HashSet::from([PathBuf::from("/vault")]),
            false,
            None,
        );
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
        let empty = from_picker(&picker, &HashMap::new(), &HashSet::new(), None);
        assert!(empty
            .node(&WorkspaceNodeId::Status(
                picker.cwd.clone(),
                WorkspaceStatus::Empty
            ))
            .is_some());

        picker.error = Some("permission denied".into());
        let error = from_picker(&picker, &HashMap::new(), &HashSet::new(), None);
        assert!(error
            .node(&WorkspaceNodeId::Status(
                picker.cwd.clone(),
                WorkspaceStatus::Error
            ))
            .is_some());
    }

    #[test]
    fn truncated_workspace_has_an_explicit_status_node() {
        let root = node(
            "/vault",
            true,
            vec![node("/vault/readme.md", false, vec![])],
        );
        let graph = from_tree(&root, &HashSet::from([PathBuf::from("/vault")]), true, None);

        assert!(graph
            .node(&WorkspaceNodeId::Status(
                PathBuf::from("/vault"),
                WorkspaceStatus::Truncated,
            ))
            .is_some());
    }
}
