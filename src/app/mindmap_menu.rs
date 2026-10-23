//! Mind map context menus (MDV-048 P6): a right press on a node of the
//! document map, the JSON/YAML data map, or Full Mindmap, or on empty
//! canvas. The canvas reports the press without starting a drag or
//! selecting anything.

use super::*;
use crate::ast::BlockId;
use crate::mindmap::{ZoomCommand, DOC_ROOT};

impl App {
    /// The document or data map's menu for node `target`, or for empty
    /// canvas when `None`.
    pub(super) fn open_mindmap_context_menu(&mut self, target: Option<BlockId>) -> Task<Message> {
        if self.view_mode != ViewMode::Mindmap || self.full_mindmap.is_some() {
            return Task::none();
        }
        let entries = match target {
            None => canvas_entries(),
            Some(id) if self.is_data_doc => self.data_node_entries(id),
            Some(id) => self.document_node_entries(id),
        };
        self.open_context_menu(entries, None);
        Task::none()
    }

    /// Full Mindmap's menu for node `target`, or for empty canvas when
    /// `None`. A status row ("More…", "Loading…") gets the canvas menu.
    pub(super) fn open_full_mindmap_context_menu(
        &mut self,
        target: Option<WorkspaceNodeId>,
    ) -> Task<Message> {
        if self.full_mindmap.is_none() {
            return Task::none();
        }
        let entries = match target {
            Some(
                id @ (WorkspaceNodeId::Root(_)
                | WorkspaceNodeId::Folder(_)
                | WorkspaceNodeId::File(_)),
            ) => workspace_node_entries(id),
            Some(WorkspaceNodeId::Status(..)) | None => canvas_entries(),
        };
        self.open_context_menu(entries, None);
        Task::none()
    }

    fn document_node_entries(&self, id: BlockId) -> Vec<MenuEntry> {
        let show = vec![MenuEntry::item(
            "Show in Reader",
            Message::MindmapActivateNode(id),
        )];
        // The root is the document, not a heading.
        let copy = if id == DOC_ROOT {
            Vec::new()
        } else {
            vec![
                menu_item(
                    "Copy Heading Text",
                    self.heading_text(id).map(Message::CopyCode),
                ),
                menu_item(
                    "Copy Link to Heading",
                    self.heading_link(id).map(Message::CopyCode),
                ),
            ]
        };
        let (collapse, expand) = match crate::mindmap::subtree_below(&self.ast, id) {
            Some((all, branches)) => {
                let folded = |id: &BlockId| self.mindmap_collapsed.contains(id);
                let can_collapse = !branches.is_empty()
                    && (folded(&id) || branches.iter().any(|branch| !folded(branch)));
                let can_expand = all.iter().any(folded);
                (
                    can_collapse.then_some(Message::MindmapCollapseBelow(id)),
                    can_expand.then_some(Message::MindmapExpandBelow(id)),
                )
            }
            None => (None, None),
        };
        let fold = vec![
            menu_item("Collapse All Below", collapse),
            menu_item("Expand All Below", expand),
        ];
        let mut first = show;
        first.extend(copy);
        menu_sections(vec![first, fold])
    }

    fn data_node_entries(&self, id: BlockId) -> Vec<MenuEntry> {
        let (_, _, paths) = self.mindmap_layout();
        let path = paths.get(&id);
        let key = path.and_then(|path| crate::data_mindmap::path_key(path));
        let path_text = path
            .map(|path| crate::data_mindmap::path_text(path))
            .filter(|text| !text.is_empty());
        vec![
            menu_item("Copy Key", key.map(Message::CopyCode)),
            menu_item(
                "Copy Value",
                path.is_some().then_some(Message::MindmapCopyDataValue(id)),
            ),
            menu_item("Copy Path", path_text.map(Message::CopyCode)),
        ]
    }

    /// A data map node's value: a string as it is, anything else as
    /// JSON, YAML, or TOML text. Parsed when asked, not on every press.
    pub(super) fn mindmap_data_value(&self, id: BlockId) -> Option<String> {
        if !self.is_data_doc {
            return None;
        }
        let (_, _, paths) = self.mindmap_layout();
        let lang = data_lang_for(self.file.as_deref())?;
        crate::data_mindmap::subtree_pretty(&self.source, lang, paths.get(&id)?)
            .map(|text| text.trim_end_matches('\n').to_owned())
    }

    /// Show `id`'s children, with every branch below it folded.
    pub(super) fn mindmap_collapse_below(&mut self, id: BlockId) -> Task<Message> {
        if self.view_mode != ViewMode::Mindmap || self.is_data_doc {
            return Task::none();
        }
        let Some((all, branches)) = crate::mindmap::subtree_below(&self.ast, id) else {
            return Task::none();
        };
        self.mindmap_collapsed.remove(&id);
        self.mindmap_collapsed.extend(branches);
        // A selection the fold hides moves to the node, as a fold by its
        // chevron selects the node.
        if self
            .mindmap_selected
            .is_some_and(|selected| selected != id && all.contains(&selected))
        {
            self.mindmap_selected = Some(id);
            if self.mindmap_panel_open {
                self.mindmap_panel_shown = Some(id);
            }
        }
        self.invalidate_mindmap_layout();
        Task::none()
    }

    /// Unfold `id` and every branch below it.
    pub(super) fn mindmap_expand_below(&mut self, id: BlockId) -> Task<Message> {
        if self.view_mode != ViewMode::Mindmap || self.is_data_doc {
            return Task::none();
        }
        let Some((all, _)) = crate::mindmap::subtree_below(&self.ast, id) else {
            return Task::none();
        };
        for node in &all {
            self.mindmap_collapsed.remove(node);
        }
        self.invalidate_mindmap_layout();
        Task::none()
    }
}

/// Empty canvas: the zoom buttons' Fit and 100%.
fn canvas_entries() -> Vec<MenuEntry> {
    vec![
        MenuEntry::item("Zoom to Fit", Message::MindmapZoom(ZoomCommand::Fit)),
        MenuEntry::item("Actual Size", Message::MindmapZoom(ZoomCommand::Reset))
            .shortcut(Some("0")),
    ]
}

/// A Full Mindmap folder or file. Open does what a double click does: a
/// file opens, a folder becomes the root. The root is open already.
fn workspace_node_entries(id: WorkspaceNodeId) -> Vec<MenuEntry> {
    let path = id.path().to_path_buf();
    let open = match &id {
        WorkspaceNodeId::Root(_) => None,
        _ => Some(Message::FullMindmapActivateNode(id.clone())),
    };
    let first = vec![
        menu_item("Open", open),
        MenuEntry::item("Reveal in Finder", Message::RevealPath(path.clone())),
        menu_item(
            "Copy Path",
            path.to_str().map(|text| Message::CopyPath(text.to_owned())),
        ),
    ];
    let slot = match id {
        WorkspaceNodeId::File(_) => vec![MenuEntry::item(
            "Add to Quick Slot",
            Message::QuickSlotAssignPath(path),
        )],
        _ => Vec::new(),
    };
    menu_sections(vec![first, slot])
}
