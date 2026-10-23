//! The mind map pointer model (MDV-048 P4): what a click, a chevron, and a
//! double click do in the document mind map and in Full Mindmap. The canvas
//! gesture itself is tested in `crate::mindmap`; these tests cover what the
//! app does with the messages the canvas publishes.

use super::group3::{assert_tips, full_mindmap_app, tips, tree_fixture};
use super::*;
use crate::icon::ic;
use crate::mindmap::{ZoomCommand, DOC_ROOT};

#[test]
fn a_full_mindmap_click_selects_and_opens_the_panel() {
    let (mut app, dir) = full_mindmap_app("p4-click-panel");
    assert!(!app.full_mindmap.as_ref().unwrap().panel_open);

    // Keyboard selection leaves the panel closed, as before.
    let _ = app.update(Message::FullMindmapNavigate(MindmapDir::Right));
    assert!(!app.full_mindmap.as_ref().unwrap().panel_open);

    let folder = WorkspaceNodeId::Folder(dir.join("sub"));
    let expanded_before = app.full_mindmap.as_ref().unwrap().expanded.clone();
    let _ = app.update(Message::FullMindmapPointerSelect(folder.clone()));
    let full = app.full_mindmap.as_ref().unwrap();
    assert!(full.panel_open, "a click opens the panel");
    assert_eq!(full.selected.as_ref(), Some(&folder));
    assert_eq!(
        full.expanded, expanded_before,
        "a click on a folder does not fold it"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_document_mindmap_branch_click_opens_the_panel_without_folding() {
    let (mut app, dir) = tree_fixture("p4-doc-branch");
    let _ = app.update(Message::FileLoaded(Ok((
        dir.join("alpha.md"),
        "# Top\n\n## Child\n\ntext\n".into(),
    ))));
    let _ = app.update(Message::ToggleMindmap);
    let (nodes, _, _) = app.mindmap_layout();
    let branch = nodes
        .iter()
        .find(|node| !node.children.is_empty() && node.id.is_some())
        .and_then(|node| node.id)
        .expect("a branch heading");
    let _ = app.update(Message::MindmapSelectLeaf(branch));
    assert!(app.mindmap_panel_open);
    assert_eq!(app.mindmap_selected, Some(branch));
    assert_eq!(app.mindmap_panel_shown, Some(branch));
    assert!(!app.mindmap_collapsed.contains(&branch));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_full_mindmap_double_click_does_what_enter_does() {
    // A file opens, as Enter does, even when the first click's selection
    // has not landed.
    let (mut app, dir) = full_mindmap_app("p4-activate-file");
    let file = dir.join("alpha.md");
    let _ = app.update(Message::FullMindmapActivateNode(WorkspaceNodeId::File(
        file.clone(),
    )));
    let full = app.full_mindmap.as_ref().unwrap();
    assert_eq!(full.selected, Some(WorkspaceNodeId::File(file.clone())));
    assert_eq!(
        full.pending_open.as_ref().map(|open| open.path.as_path()),
        Some(file.as_path()),
        "the file opens"
    );
    let _ = std::fs::remove_dir_all(&dir);

    // A folder becomes the root.
    let (mut app, dir) = full_mindmap_app("p4-activate-folder");
    let folder = dir.join("sub");
    let _ = app.update(Message::FullMindmapPointerSelect(WorkspaceNodeId::Folder(
        folder.clone(),
    )));
    let _ = app.update(Message::FullMindmapActivateNode(WorkspaceNodeId::Folder(
        folder.clone(),
    )));
    assert_eq!(
        app.full_mindmap
            .as_ref()
            .and_then(|full| full.pending_workspace_load.as_ref())
            .map(|load| load.path.as_path()),
        Some(folder.as_path())
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn one_click_on_more_shows_the_next_page() {
    let dir = std::env::temp_dir().join(format!("rmdv-p4-more-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let dir = canonicalize_existing_path(dir);
    let folder = |name: &str| crate::tree::Node {
        path: dir.join(name),
        name: name.into(),
        is_dir: true,
        children: Vec::new(),
        recursive_supported_file_count: Some(crate::tree::RecursiveFileCount::Exact(1)),
    };
    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.full_mindmap = Some(App::new_full_mindmap_state());
    {
        let full = app.full_mindmap.as_mut().unwrap();
        full.expanded.insert(dir.clone());
        full.materialized_folders.insert(
            dir.clone(),
            crate::workspace_mindmap::MaterializedFolder::Loaded {
                folders: std::sync::Arc::new(vec![folder("a"), folder("b")]),
                files: std::sync::Arc::new(Vec::new()),
                recursive_supported_file_count: crate::tree::RecursiveFileCount::Exact(2),
                truncated: false,
                shown: 1,
            },
        );
    }
    let more = WorkspaceNodeId::Status(
        dir.clone(),
        crate::workspace_mindmap::WorkspaceStatus::MoreItems,
    );
    assert!(app.full_mindmap_graph().unwrap().node(&more).is_some());

    let _ = app.update(Message::FullMindmapPointerSelect(more.clone()));
    let graph = app.full_mindmap_graph().unwrap();
    assert!(graph.node(&more).is_none(), "the page is shown");
    assert!(graph
        .node(&WorkspaceNodeId::Folder(dir.join("b")))
        .is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

/// The tree fixture with a two-heading document open in the mind map.
fn document_mindmap_app(tag: &str) -> (App, PathBuf) {
    let (mut app, dir) = tree_fixture(tag);
    let _ = app.update(Message::FileLoaded(Ok((
        dir.join("alpha.md"),
        "# Top\n\n## Child\n\ntext\n".into(),
    ))));
    let _ = app.update(Message::ToggleMindmap);
    assert_eq!(app.view_mode, ViewMode::Mindmap);
    (app, dir)
}

#[test]
fn zoom_buttons_send_their_command_and_leave_the_canvas_its_events() {
    let (mut app, dir) = document_mindmap_app("p4-zoom-buttons");
    assert!(
        app.mindmap_selected.is_some(),
        "the first heading is selected"
    );
    let mut ui = PointerUi::new();
    assert_tips(
        &tips(&mut ui, &app),
        &["Zoom to Fit", "Zoom Out  −", "Actual Size  0", "Zoom In  ="],
    );

    let plus = ui
        .text_bounds(&app, &ic::PLUS.to_string())
        .expect("zoom in button");
    let messages = ui.click(&app, center(plus));
    assert!(
        has(&messages, |m| matches!(
            m,
            Message::MindmapZoom(ZoomCommand::In)
        )),
        "{messages:?}"
    );
    assert!(
        !has(&messages, |m| matches!(m, Message::MindmapDeselect)),
        "the canvas under the button sees no click"
    );
    apply(&mut app, messages);
    assert_eq!(app.mindmap_zoom_request.generation, 1);
    assert_eq!(app.mindmap_zoom_request.command, Some(ZoomCommand::In));
    let fit = ui
        .text_bounds(&app, &ic::SCAN.to_string())
        .expect("zoom to fit button");
    let messages = ui.click(&app, center(fit));
    apply(&mut app, messages);
    assert_eq!(app.mindmap_zoom_request.command, Some(ZoomCommand::Fit));
    assert_eq!(app.mindmap_zoom_request.generation, 2);
    let reset = ui.text_bounds(&app, "100%").expect("100% button");
    let messages = ui.click(&app, center(reset));
    apply(&mut app, messages);
    assert_eq!(app.mindmap_zoom_request.command, Some(ZoomCommand::Reset));

    // Just above the buttons the canvas still owns the pointer: it shows
    // its grab cursor and an empty-canvas click clears the selection.
    let probe = Point::new(plus.center_x(), plus.y - 40.0);
    let (_, cursor) = ui.move_to(&app, probe);
    assert_eq!(cursor, mouse::Interaction::Grab);
    let messages = ui.click(&app, probe);
    assert!(
        has(&messages, |m| matches!(m, Message::MindmapDeselect)),
        "{messages:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_zoom_buttons_drive_their_own_canvas() {
    let (mut app, dir) = full_mindmap_app("p4-fmm-zoom");
    let mut ui = PointerUi::new();
    let minus = ui
        .text_bounds(&app, &ic::MINUS.to_string())
        .expect("zoom out button");
    let messages = ui.click(&app, center(minus));
    apply(&mut app, messages);
    assert_eq!(
        app.full_mindmap_zoom_request.command,
        Some(ZoomCommand::Out)
    );
    assert_eq!(
        app.mindmap_zoom_request,
        crate::mindmap::ZoomRequest::default(),
        "the document map's zoom is left alone"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_empty_canvas_click_clears_the_selection_but_keeps_the_panel() {
    let (mut app, dir) = document_mindmap_app("p4-deselect-keeps-panel");
    assert!(app.mindmap_panel_open && app.mindmap_selected.is_some());
    let _ = app.update(Message::MindmapDeselect);
    assert_eq!(app.mindmap_selected, None);
    assert_eq!(app.mindmap_panel_shown, None);
    assert!(app.mindmap_panel_open, "deselect is not close");
    let _ = std::fs::remove_dir_all(&dir);

    let (mut app, dir) = full_mindmap_app("p4-fmm-deselect-keeps-panel");
    let file = WorkspaceNodeId::File(dir.join("alpha.md"));
    let _ = app.update(Message::FullMindmapPointerSelect(file));
    let _ = app.update(Message::FullMindmapDeselect);
    let full = app.full_mindmap.as_ref().unwrap();
    assert_eq!(full.selected, None);
    assert!(full.panel_open, "deselect is not close");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_panel_close_button_hides_each_mind_map_panel() {
    let (mut app, dir) = document_mindmap_app("p4-panel-close");
    assert!(app.mindmap_panel_open);
    let mut ui = PointerUi::new();
    assert_tips(&tips(&mut ui, &app), &["Hide Panel  ⌘⌥B"]);
    let close = ui
        .text_bounds(&app, &ic::X.to_string())
        .expect("panel close button");
    assert!(close.x > 1280.0 - app.mindmap_panel_width, "{close:?}");
    let messages = ui.click(&app, center(close));
    assert!(
        has(&messages, |m| matches!(m, Message::ToggleMindmapPanel)),
        "{messages:?}"
    );
    apply(&mut app, messages);
    assert!(!app.mindmap_panel_open);
    assert!(
        app.mindmap_selected.is_some(),
        "closing keeps the selection"
    );
    assert!(ui.text_bounds(&app, &ic::X.to_string()).is_none());
    let _ = std::fs::remove_dir_all(&dir);

    let (mut app, dir) = full_mindmap_app("p4-fmm-panel-close");
    let file = WorkspaceNodeId::File(dir.join("alpha.md"));
    let _ = app.update(Message::FullMindmapPointerSelect(file.clone()));
    assert!(app.full_mindmap.as_ref().unwrap().panel_open);
    let mut ui = PointerUi::new();
    let close = ui
        .text_bounds(&app, &ic::X.to_string())
        .expect("Full Mindmap panel close button");
    let messages = ui.click(&app, center(close));
    apply(&mut app, messages);
    let full = app.full_mindmap.as_ref().unwrap();
    assert!(
        !full.panel_open,
        "the pointer closes the Full Mindmap panel"
    );
    assert_eq!(full.selected.as_ref(), Some(&file));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Full Mindmap with `alpha.md` (rewritten to `source`) selected by the
/// pointer and its preview parsed into the panel.
fn full_mindmap_preview_app(tag: &str, source: &str) -> (App, PathBuf) {
    let (mut app, dir) = full_mindmap_app(tag);
    let file = dir.join("alpha.md");
    std::fs::write(&file, source).expect("alpha.md");
    // Show the root's files.
    loop {
        let requests: Vec<_> = app
            .full_mindmap
            .as_ref()
            .map(|full| full.pending_folder_loads.values().cloned().collect())
            .unwrap_or_default();
        if requests.is_empty() {
            break;
        }
        for request in requests {
            let snapshot =
                tree::load_expanded_folder(&request.folder, request.show_hidden).unwrap();
            let _ = app.update(Message::FullMindmapFolderLoaded {
                request: request.clone(),
                result: Ok((request.folder.clone(), snapshot)),
            });
        }
    }
    let _ = app.update(Message::FullMindmapPointerSelect(WorkspaceNodeId::File(
        file.clone(),
    )));
    let settle = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_preview_settle.clone())
        .expect("a preview settle");
    let _ = app.update(Message::FullMindmapPreviewSettle { request: settle });
    let request = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_preview.clone())
        .expect("a preview read");
    let _ = app.update(Message::FullMindmapPreviewLoaded {
        request: request.clone(),
        result: Ok((file.clone(), source.to_string())),
    });
    let parsed = parse_full_mindmap_preview_blocking(file.clone(), source.to_string());
    let _ = app.update(Message::FullMindmapPreviewParsed {
        request,
        result: Ok(parsed),
    });
    assert!(matches!(
        app.full_mindmap.as_ref().unwrap().preview,
        FullMindmapPreview::Document { .. }
    ));
    (app, dir)
}

#[test]
fn a_full_mindmap_preview_copy_button_copies() {
    let (app, dir) =
        full_mindmap_preview_app("p4-preview-copy", "# Alpha\n\n```rust\nlet x = 1;\n```\n");
    let mut ui = PointerUi::new();
    let copy = ui
        .text_bounds(&app, &ic::COPY.to_string())
        .expect("the code block's copy button");
    let messages = ui.click(&app, center(copy));
    assert!(
        has(
            &messages,
            |m| matches!(m, Message::CopyCode(code) if code.contains("let x = 1;"))
        ),
        "{messages:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_preview_messages_keep_their_action() {
    let base = PathBuf::from("/notes/alpha.md");
    let (blocks, _) = crate::parser::parse("```mermaid\ngraph TD; A-->B\n```\n");
    let hash = blocks
        .iter()
        .find_map(|(_, block)| match block {
            crate::ast::Block::Diagram { hash, .. } => Some(*hash),
            _ => None,
        })
        .expect("a diagram block");
    let map = |message| App::full_mindmap_preview_message(message, &base, &blocks);

    assert!(matches!(
        map(Message::OpenLink("beta.md#part".into())),
        Message::FullMindmapPreviewLink { base: b, url } if b == base && url == "beta.md#part"
    ));
    assert!(matches!(map(Message::CopyCode("x".into())), Message::CopyCode(s) if s == "x"));
    // The preview's own diagram source, not the open document's.
    assert!(matches!(
        map(Message::CopyDiagramSource(hash)),
        Message::CopyCode(s) if s.contains("A-->B")
    ));
    assert!(matches!(map(Message::DiagramZoom(hash)), Message::DiagramZoom(h) if h == hash));
    assert!(matches!(
        map(Message::OpenImageZoom("/notes/a.png".into())),
        Message::OpenImageZoom(u) if u == "/notes/a.png"
    ));
    assert!(matches!(
        map(Message::TableScrolled),
        Message::TableScrolled
    ));
    // Folding stays a reader action.
    assert!(matches!(
        map(Message::ToggleFold(crate::ast::BlockId(1))),
        Message::Noop
    ));
}

#[test]
fn a_full_mindmap_preview_link_resolves_from_the_previewed_file() {
    let (mut app, dir) = full_mindmap_app("p4-preview-link");
    let base = dir.join("alpha.md");
    let _ = app.update(Message::FullMindmapPreviewLink {
        base: base.clone(),
        url: "sub/gamma.md#gamma".into(),
    });
    assert_eq!(
        app.full_mindmap
            .as_ref()
            .and_then(|full| full.pending_open.as_ref())
            .map(|open| open.path.clone()),
        Some(dir.join("sub").join("gamma.md")),
        "the link opens relative to the previewed file"
    );
    // The heading waits for that open to be accepted.
    assert_eq!(
        app.full_mindmap
            .as_ref()
            .and_then(|full| full.link_nav.as_ref())
            .map(|(path, nav)| (path.clone(), nav.fragment.clone())),
        Some((dir.join("sub").join("gamma.md"), Some("gamma".into())))
    );

    let (mut app, dir2) = full_mindmap_app("p4-preview-link-missing");
    let _ = app.update(Message::FullMindmapPreviewLink {
        base: dir2.join("alpha.md"),
        url: "nope.md".into(),
    });
    assert!(app
        .toast
        .as_ref()
        .is_some_and(|toast| toast.text.contains("nope.md")));
    assert!(app
        .full_mindmap
        .as_ref()
        .is_some_and(|full| full.pending_open.is_none()));
    let _ = app.update(Message::FullMindmapPreviewLink {
        base: dir2.join("alpha.md"),
        url: "https://example.com/doc".into(),
    });
    assert_eq!(
        app.external_opens.last().map(|(url, _)| url.as_str()),
        Some("https://example.com/doc"),
        "a web link goes to the browser"
    );
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&dir2);
}

#[test]
fn dragging_a_mind_map_panel_divider_stops_at_a_max_width() {
    let (mut app, dir) = document_mindmap_app("p4-divider-max");
    app.window_size = Some(iced::Size::new(1280.0, 800.0));
    let _ = app.update(Message::MindmapPanelDragStart(0.0));
    let _ = app.update(Message::MindmapPanelDragMove(1000.0));
    let _ = app.update(Message::MindmapPanelDragMove(-2000.0));
    let _ = app.update(Message::MindmapPanelDragEnd);
    assert!(
        app.mindmap_panel_width <= 1280.0 * MIND_PANEL_MAX_FRAC + 0.01,
        "{}",
        app.mindmap_panel_width
    );
    let _ = std::fs::remove_dir_all(&dir);

    let (mut app, dir) = full_mindmap_app("p4-fmm-divider-max");
    app.window_size = Some(iced::Size::new(1280.0, 800.0));
    let _ = app.update(Message::FullMindmapTogglePanel);
    let _ = app.update(Message::FullMindmapPanelDragStart(0.0));
    let _ = app.update(Message::FullMindmapPanelDragMove(1000.0));
    let _ = app.update(Message::FullMindmapPanelDragMove(-2000.0));
    let width = app.full_mindmap.as_ref().unwrap().panel_width;
    assert!(width <= 1280.0 * MIND_PANEL_MAX_FRAC + 0.01, "{width}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The divider's hit area: wide enough to grab, with the resize cursor
/// at both edges, and a double click puts the default width back.
fn check_divider(app: &mut App, id: &'static str, width: impl Fn(&App) -> f32) {
    let mut ui = PointerUi::new();
    let handle = ui.id_bounds(app, id).expect("panel divider");
    assert!(handle.width >= 9.0, "a wide hit area: {handle:?}");
    for x in [handle.x + 1.0, handle.x + handle.width - 1.0] {
        let (_, cursor) = ui.move_to(app, Point::new(x, handle.center_y()));
        assert_eq!(cursor, mouse::Interaction::ResizingHorizontally, "at {x}");
    }
    assert_ne!(width(app), MIND_PANEL_DEFAULT);
    let messages = ui.double_click(app, center(handle));
    apply(app, messages);
    assert_eq!(
        width(app),
        MIND_PANEL_DEFAULT,
        "a double click resets the width"
    );
}

#[test]
fn a_mind_map_panel_divider_is_easy_to_grab_and_resets_on_double_click() {
    let (mut app, dir) = document_mindmap_app("p4-divider-reset");
    app.mindmap_panel_width = 600.0;
    check_divider(&mut app, "mindmap-panel-divider", |app| {
        app.mindmap_panel_width
    });
    let _ = std::fs::remove_dir_all(&dir);

    let (mut app, dir) = full_mindmap_app("p4-fmm-divider-reset");
    let _ = app.update(Message::FullMindmapTogglePanel);
    app.full_mindmap.as_mut().unwrap().panel_width = 600.0;
    check_divider(&mut app, "full-mindmap-panel-divider", |app| {
        app.full_mindmap.as_ref().unwrap().panel_width
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_document_root_selects_the_whole_document() {
    let (mut app, dir) = document_mindmap_app("p4-root-select");
    let (nodes, _, _) = app.mindmap_layout();
    assert_eq!(
        nodes[0].id,
        Some(DOC_ROOT),
        "the root is a node, not canvas"
    );
    let first_heading = nodes[nodes[0].children[0]].id;

    // A click on the root selects it, and the panel shows the document
    // from its top.
    let _ = app.update(Message::MindmapSelectLeaf(DOC_ROOT));
    assert_eq!(app.mindmap_selected, Some(DOC_ROOT));
    assert_eq!(app.mindmap_panel_shown, Some(DOC_ROOT));
    assert!(app.mindmap_panel_open);
    assert_eq!(
        app.mindmap_panel_range(DOC_ROOT),
        Some((0, app.ast.len(), false))
    );

    // Its chevron folds every heading, and unfolds them again.
    let _ = app.update(Message::MindmapToggleNode(DOC_ROOT));
    assert_eq!(app.mindmap_layout().0.len(), 1);
    let _ = app.update(Message::MindmapToggleNode(DOC_ROOT));
    assert_eq!(app.mindmap_layout().0.len(), nodes.len());

    // The arrow keys keep their meaning: Right goes to the first heading,
    // and Left from the root enters Full Mindmap. (Right schedules the
    // panel settle timer, which needs a runtime.)
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _guard = runtime.enter();
    let _ = app.update(Message::MindmapNavigate(MindmapDir::Right));
    assert_eq!(app.mindmap_selected, first_heading);
    let _ = app.update(Message::MindmapSelectLeaf(DOC_ROOT));
    let _ = app.update(Message::MindmapNavigate(MindmapDir::Left));
    assert!(
        app.full_mindmap.is_some(),
        "Left from the root enters Full Mindmap"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn left_from_a_top_heading_still_enters_full_mindmap() {
    let (mut app, dir) = document_mindmap_app("p4-root-left-boundary");
    let (nodes, _, _) = app.mindmap_layout();
    let top = nodes[nodes[0].children[0]].id;
    assert_eq!(app.mindmap_selected, top);
    let _ = app.update(Message::MindmapNavigate(MindmapDir::Left));
    assert!(app.full_mindmap.is_some(), "Left never selects the root");
    assert_ne!(app.mindmap_selected, Some(DOC_ROOT));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The zoom of the first mind map canvas state in `tree`, if any.
fn canvas_zoom<Id: 'static>(tree: &mut iced::advanced::widget::Tree) -> Option<&mut f32> {
    use iced::advanced::widget::tree::State;
    if let State::Some(state) = &mut tree.state {
        if let Some(state) = state.downcast_mut::<crate::mindmap::MindmapState<Id>>() {
            return Some(&mut state.zoom);
        }
    }
    tree.children.iter_mut().find_map(canvas_zoom::<Id>)
}

/// Showing or hiding the panel keeps the canvas widget at the same tree
/// position, so its zoom, pan and pointer focus survive the change.
fn check_canvas_survives<Id: 'static>(app: &mut App, toggle: impl Fn() -> Message) {
    use iced::advanced::widget::Tree;
    let view = app.view();
    let mut tree = Tree::new(view.as_widget());
    drop(view);
    *canvas_zoom::<Id>(&mut tree).expect("a mind map canvas") = 2.5;
    for _ in 0..2 {
        let _ = app.update(toggle());
        let view = app.view();
        tree.diff(view.as_widget());
        drop(view);
        assert_eq!(
            canvas_zoom::<Id>(&mut tree).copied(),
            Some(2.5),
            "the panel toggle keeps the canvas state"
        );
    }
}

#[test]
fn showing_or_hiding_a_mind_map_panel_keeps_the_canvas_state() {
    let (mut app, dir) = document_mindmap_app("p4-panel-keeps-canvas");
    assert!(app.mindmap_panel_open);
    check_canvas_survives::<crate::ast::BlockId>(&mut app, || Message::ToggleMindmapPanel);
    let _ = std::fs::remove_dir_all(&dir);

    let (mut app, dir) = full_mindmap_app("p4-fmm-panel-keeps-canvas");
    assert!(!app.full_mindmap.as_ref().unwrap().panel_open);
    check_canvas_survives::<WorkspaceNodeId>(&mut app, || Message::FullMindmapTogglePanel);
    let _ = std::fs::remove_dir_all(&dir);
}

/// iced sends the second press of a double click as a press and then a
/// double click, and the button is still down: the pointer jitter before
/// the release must not drag the reset width back.
#[test]
fn a_divider_double_click_reset_survives_pointer_jitter() {
    let (mut app, dir) = document_mindmap_app("p4-divider-reset-jitter");
    app.window_size = Some(iced::Size::new(1280.0, 800.0));
    app.mindmap_panel_width = 600.0;
    let _ = app.update(Message::MindmapPanelDragStart(680.0));
    let _ = app.update(Message::MindmapPanelResetWidth);
    let _ = app.update(Message::MindmapPanelDragMove(681.0));
    let _ = app.update(Message::MindmapPanelDragMove(682.0));
    let _ = app.update(Message::MindmapPanelDragEnd);
    assert_eq!(app.mindmap_panel_width, MIND_PANEL_DEFAULT);
    let _ = std::fs::remove_dir_all(&dir);

    let (mut app, dir) = full_mindmap_app("p4-fmm-divider-reset-jitter");
    app.window_size = Some(iced::Size::new(1280.0, 800.0));
    let _ = app.update(Message::FullMindmapTogglePanel);
    app.full_mindmap.as_mut().unwrap().panel_width = 600.0;
    let _ = app.update(Message::FullMindmapPanelDragStart(680.0));
    let _ = app.update(Message::FullMindmapPanelResetWidth);
    let _ = app.update(Message::FullMindmapPanelDragMove(681.0));
    let _ = app.update(Message::FullMindmapPanelDragMove(682.0));
    let _ = app.update(Message::FullMindmapPanelDragEnd);
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().panel_width,
        MIND_PANEL_DEFAULT
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Opens `name` from Full Mindmap as Enter does and completes its read.
/// Returns what the accepted load asks for at once.
fn activate_and_load(app: &mut App, path: &Path, source: &str) -> Vec<Message> {
    let _ = app.update(Message::FullMindmapActivateNode(WorkspaceNodeId::File(
        path.to_path_buf(),
    )));
    let request = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_open.clone())
        .expect("an open request");
    ready_outputs(app.update(Message::FullMindmapFileLoaded {
        request,
        result: Ok((path.to_path_buf(), source.to_string())),
    }))
}

fn has_goto(messages: &[Message]) -> bool {
    has(
        messages,
        |m| matches!(m, Message::Ipc(request, _) if matches!(request.cmd, crate::ipc::Cmd::Goto { .. })),
    )
}

#[test]
fn a_preview_link_fragment_applies_only_to_its_own_accepted_open() {
    let link = |app: &mut App, dir: &Path| {
        let _ = app.update(Message::FullMindmapPreviewLink {
            base: dir.join("alpha.md"),
            url: "sub/gamma.md#part".into(),
        });
    };
    let beta = "# Beta\n\ntext\n\n## Part\n\nmore\n";

    // Accepted: the fragment scrolls the file the link opened.
    let (mut app, dir) = full_mindmap_app("p4-link-nav-accepted");
    let gamma = dir.join("sub").join("gamma.md");
    link(&mut app, &dir);
    let request = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_open.clone())
        .expect("the link opens gamma");
    let outputs = ready_outputs(app.update(Message::FullMindmapFileLoaded {
        request,
        result: Ok((gamma, "# Gamma\n\n## Part\n".into())),
    }));
    assert!(has_goto(&outputs), "{outputs:?}");
    let _ = std::fs::remove_dir_all(&dir);

    // Blocked by unsaved edits: a later open of another file does not
    // jump to the link's heading.
    let (mut app, dir) = full_mindmap_app("p4-link-nav-dirty");
    app.dirty = true;
    link(&mut app, &dir);
    assert!(app.full_mindmap.as_ref().unwrap().pending_open.is_none());
    app.dirty = false;
    let outputs = activate_and_load(&mut app, &dir.join("beta.md"), beta);
    assert!(!has_goto(&outputs), "blocked: {outputs:?}");
    let _ = std::fs::remove_dir_all(&dir);

    // A failed read leaves nothing either.
    let (mut app, dir) = full_mindmap_app("p4-link-nav-error");
    link(&mut app, &dir);
    let request = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_open.clone())
        .expect("the link opens gamma");
    let _ = app.update(Message::FullMindmapFileLoaded {
        request,
        result: Err("unreadable".into()),
    });
    let outputs = activate_and_load(&mut app, &dir.join("beta.md"), beta);
    assert!(!has_goto(&outputs), "failed: {outputs:?}");
    let _ = std::fs::remove_dir_all(&dir);

    // Superseded by Enter on another file before the read returns.
    let (mut app, dir) = full_mindmap_app("p4-link-nav-superseded");
    link(&mut app, &dir);
    let outputs = activate_and_load(&mut app, &dir.join("beta.md"), beta);
    assert!(!has_goto(&outputs), "superseded: {outputs:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_narrow_mind_map_keeps_its_zoom_buttons_whole() {
    let (mut app, dir) = document_mindmap_app("p4-narrow-zoom-buttons");
    // The canvas then starts at the window's left edge.
    app.sidebar_open = false;
    // Wide: the hints and the buttons both show.
    let mut wide = PointerUi::sized(Size::new(1600.0, 700.0));
    assert!(wide.text_bounds(&app, "move").is_some(), "hints show");
    assert!(wide.text_bounds(&app, &ic::PLUS.to_string()).is_some());
    let mut ui = PointerUi::sized(Size::new(1000.0, 700.0));
    // Narrow the canvas to 300 px with the panel.
    let divider = ui
        .id_bounds(&app, "mindmap-panel-divider")
        .expect("panel divider");
    app.mindmap_panel_width += divider.x - 300.0;
    let divider = ui
        .id_bounds(&app, "mindmap-panel-divider")
        .expect("panel divider");
    assert!(divider.x < 340.0, "a narrow canvas: {divider:?}");
    for code in [ic::SCAN, ic::MINUS, ic::PLUS] {
        let glyph = ui
            .text_bounds(&app, &code.to_string())
            .expect("zoom button");
        assert!(
            glyph.width > 0.0 && glyph.x + glyph.width <= divider.x,
            "{code:?} is whole inside the canvas: {glyph:?}, canvas ends at {}",
            divider.x
        );
    }
    let reset = ui.text_bounds(&app, "100%").expect("100% button");
    assert!(reset.width > 0.0 && reset.x + reset.width <= divider.x);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_panel_close_button_does_not_cover_panel_text() {
    let (mut app, dir) = tree_fixture("p4-panel-close-clear");
    let mut source = String::from("# Top\n\n");
    for i in 0..60 {
        source.push_str(&format!(
            "Line{i} has enough words in it to wrap across the whole panel width easily.\n\n"
        ));
    }
    let _ = app.update(Message::FileLoaded(Ok((dir.join("alpha.md"), source))));
    let _ = app.update(Message::ToggleMindmap);
    let _ = app.update(Message::MindmapSelectLeaf(DOC_ROOT));
    assert!(app.mindmap_panel_open);
    let mut ui = PointerUi::new();
    let close = ui
        .text_bounds(&app, &ic::X.to_string())
        .expect("panel close button");
    // The close button's whole hit area, not just its glyph.
    let close = Rectangle {
        x: close.x - 8.0,
        y: close.y - 8.0,
        width: close.width + 16.0,
        height: close.height + 16.0,
    };
    let text = ui
        .id_bounds(&app, MINDMAP_PANEL_TEXT_ID)
        .expect("the panel's text area");
    assert!(
        text.y < close.y + close.height,
        "long content starts at the top: {text:?}"
    );
    assert!(
        text.x + text.width <= close.x,
        "text stays clear of the close button: {text:?} vs {close:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Where a document mind map node's label starts on screen, with the
/// sidebar closed so the canvas starts at the window's left edge. Before any
/// redraw the canvas has zoom 1 and no pan, so a node sits at the canvas
/// corner plus its layout position; the canvas top is found by its grab
/// cursor.
fn document_node_point(ui: &mut PointerUi, app: &App, node: &crate::mindmap::MNode) -> Point {
    let top = (0..200)
        .map(|y| y as f32)
        .find(|&y| ui.move_to(app, Point::new(4.0, y)).1 == mouse::Interaction::Grab)
        .expect("the canvas top");
    let at = Point::new(node.x + 40.0, top + node.y + crate::mindmap::NODE_H / 2.0);
    assert_eq!(
        ui.move_to(app, at).1,
        mouse::Interaction::Pointer,
        "the pointer is on {:?}",
        node.label
    );
    at
}

#[test]
fn a_document_mindmap_double_click_reads_the_heading() {
    let (mut app, dir) = document_mindmap_app("p4-doc-activate");
    app.sidebar_open = false;
    let (nodes, _, _) = app.mindmap_layout();
    let child = nodes
        .iter()
        .find(|node| node.label == "Child")
        .expect("the Child heading");
    let child_id = child.id.expect("a heading id");
    let mut ui = PointerUi::new();
    let at = document_node_point(&mut ui, &app, child);
    let messages = ui.double_click(&app, at);
    assert!(
        has(&messages, |m| matches!(
            m,
            Message::MindmapActivateNode(id) if *id == child_id
        )),
        "a double click activates the heading: {messages:?}"
    );
    apply(&mut app, messages);
    assert_eq!(app.view_mode, ViewMode::Rendered, "the Reader shows");
    let block = app
        .ast
        .iter()
        .position(|(id, _)| *id == child_id)
        .expect("the heading block");
    assert_eq!(
        app.nav_anchor,
        Some(block),
        "the reader scrolls to the heading"
    );
    // Back returns to the mind map, as after a jump to a #fragment.
    let _ = app.update(Message::HistoryGo(HistoryDir::Back));
    assert_eq!(app.view_mode, ViewMode::Mindmap);
    let _ = std::fs::remove_dir_all(&dir);

    // The root: the Reader from the top of the document.
    let (mut app, dir) = document_mindmap_app("p4-doc-activate-root");
    app.sidebar_open = false;
    let (nodes, _, _) = app.mindmap_layout();
    let mut ui = PointerUi::new();
    let at = document_node_point(&mut ui, &app, &nodes[0]);
    let messages = ui.double_click(&app, at);
    assert!(
        has(&messages, |m| matches!(
            m,
            Message::MindmapActivateNode(DOC_ROOT)
        )),
        "{messages:?}"
    );
    let follow_ups = ready_outputs(app.update(Message::MindmapActivateNode(DOC_ROOT)));
    assert_eq!(app.view_mode, ViewMode::Rendered);
    assert!(
        has(&follow_ups, |m| matches!(m, Message::ScrollToTop)),
        "the Reader starts at the top: {follow_ups:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_data_mindmap_double_click_stays_in_the_mind_map() {
    let (mut app, dir) = tree_fixture("p4-data-activate");
    let _ = app.update(Message::FileLoaded(Ok((
        dir.join("data.json"),
        "{\"a\": {\"b\": 1}}".into(),
    ))));
    let _ = app.update(Message::ToggleMindmap);
    assert!(app.is_data_doc);
    assert_eq!(app.view_mode, ViewMode::Mindmap);
    let (nodes, _, _) = app.mindmap_layout();
    let id = nodes.iter().find_map(|node| node.id).expect("a data node");
    let _ = app.update(Message::MindmapActivateNode(id));
    assert_eq!(app.view_mode, ViewMode::Mindmap);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_document_mindmap_double_click_drops_a_pending_scroll_restore() {
    // A Quick Slot or Back restore still waits for the Reader to report a
    // scrollable viewport. The heading jump replaces it: the first report
    // of the new Reader must not scroll back to the saved position.
    for target in ["Child", "root"] {
        let (mut app, dir) = document_mindmap_app("p4-doc-activate-restore");
        let (nodes, _, _) = app.mindmap_layout();
        let id = if target == "root" {
            DOC_ROOT
        } else {
            nodes
                .iter()
                .find(|node| node.label == target)
                .and_then(|node| node.id)
                .expect("the Child heading")
        };
        app.quick_slot_body_restore = Some(0.7);
        app.history.arriving = Some(0.7);
        app.quick_slot_preview_restore = Some(0.4);
        let _ = app.update(Message::MindmapActivateNode(id));
        assert_eq!(app.view_mode, ViewMode::Rendered, "{target}");
        assert_eq!(app.quick_slot_body_restore, None, "{target}");
        assert_eq!(app.history.arriving, None, "{target}");
        assert_eq!(app.quick_slot_preview_restore, None, "{target}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn a_document_mindmap_activation_reads_its_own_block_or_nothing() {
    let (mut app, dir) = document_mindmap_app("p4-doc-activate-block");
    let child = app
        .ast
        .iter()
        .position(|(_, block)| matches!(block, crate::ast::Block::Heading { level: 2, .. }))
        .expect("the Child heading");
    let child_id = app.ast[child].0;
    let paragraph_id = app.ast[child + 1].0;

    // A block that is not a heading has no mind map node to read: no
    // history stop, no checkpoint, the mind map stays.
    let back = app.history.back.len();
    let _ = app.update(Message::MindmapActivateNode(paragraph_id));
    assert_eq!(app.view_mode, ViewMode::Mindmap);
    assert_eq!(app.history.back.len(), back, "no stop for a no-op");

    // A heading and the paragraph after it can start on the same line (a
    // .tex `\section{A} text`). The heading block is read, not whichever
    // block a line lookup finds.
    let line = app.block_lines[child];
    app.block_lines[child + 1] = line;
    let _ = app.update(Message::MindmapActivateNode(child_id));
    assert_eq!(app.view_mode, ViewMode::Rendered, "the Reader shows");
    assert_eq!(app.nav_anchor, Some(child), "the heading is the target");
    assert_eq!(app.history.back.len(), back + 1, "one stop for the jump");
    let _ = std::fs::remove_dir_all(&dir);
}

// --- Wheel and drag (MDV-048 P5) -----------------------------------------

fn settings_row_index(row: SettingsRow) -> usize {
    SettingsRow::visible()
        .iter()
        .position(|r| *r == row)
        .expect("row is visible on this platform")
}

#[test]
fn scroll_pans_setting_switches_persists_and_has_a_command() {
    let mut app = App::default();
    let isolated = app.quick_slots_persistence_path.clone().unwrap();
    let saved = |path: &Path| crate::prefs::load_from(path).mindmap_scroll_pans;
    // Off by default: scrolling zooms, as before.
    assert!(!app.prefs.mindmap_scroll_pans);

    let _ = app.update(Message::OpenSettings);
    let _ = app.update(Message::SettingsCursor(settings_row_index(
        SettingsRow::MindmapScrollPans,
    )));
    let _ = app.update(Message::SettingsStep(1));
    assert!(app.prefs.mindmap_scroll_pans && saved(&isolated));
    let _ = app.update(Message::SettingsStep(1));
    assert!(app.prefs.mindmap_scroll_pans, "→ again keeps it on");
    let _ = app.update(Message::SettingsActivate);
    assert!(!app.prefs.mindmap_scroll_pans && !saved(&isolated));

    let command = app
        .command_items()
        .into_iter()
        .find(|(label, _)| label.contains("Scroll to Pan"))
        .expect("a Command Palette entry");
    assert!(matches!(command.1, Message::ToggleMindmapScrollPans));
    let _ = app.update(command.1);
    assert!(app.prefs.mindmap_scroll_pans && saved(&isolated));
}

#[test]
fn scroll_pans_setting_row_has_a_clickable_switch() {
    let mut app = App::default();
    let _ = app.update(Message::OpenSettings);
    // A tall window shows every row without scrolling.
    let mut ui = PointerUi::sized(Size::new(1280.0, 2000.0));
    let label = ui
        .text_bounds(&app, "Scroll pans the mind map")
        .expect("the Settings page shows the row");
    assert!(label.y + label.height < 2000.0, "{label:?}");
    let index = settings_row_index(SettingsRow::MindmapScrollPans);
    let row = ui
        .find(
            &app,
            Find {
                text: None,
                contains: None,
                id: Some(App::settings_row_id(index)),
                found: None,
                found_text: None,
            },
        )
        .map(|(bounds, _)| bounds)
        .expect("the row container");
    // The 36 px switch sits at the right end of the row, inside its 14 px
    // padding.
    let switch = Point::new(row.x + row.width - 14.0 - 18.0, row.center_y());
    let messages = ui.click(&app, switch);
    assert!(
        has(&messages, |m| matches!(m, Message::ToggleMindmapScrollPans)),
        "{messages:?}"
    );
}

/// The first mind map canvas state in `tree`, if any.
fn canvas_state<Id: 'static>(
    tree: &mut iced::advanced::widget::Tree,
) -> Option<&mut crate::mindmap::MindmapState<Id>> {
    use iced::advanced::widget::tree::State;
    if let State::Some(state) = &mut tree.state {
        if let Some(state) = state.downcast_mut::<crate::mindmap::MindmapState<Id>>() {
            return Some(state);
        }
    }
    tree.children.iter_mut().find_map(canvas_state::<Id>)
}

/// One event straight into the view's widget tree, kept in `tree` the way
/// the event loop keeps it, with the pointer at `at`.
fn send_to_tree(
    ui: &PointerUi,
    app: &App,
    tree: &mut iced::advanced::widget::Tree,
    at: Point,
    event: iced::Event,
) -> Vec<Message> {
    use iced::advanced::{clipboard, layout, Layout, Shell};
    let mut view = app.view();
    tree.diff(view.as_widget());
    let node = view.as_widget_mut().layout(
        tree,
        &ui.renderer,
        &layout::Limits::new(Size::ZERO, ui.size),
    );
    let mut messages = Vec::new();
    let mut shell = Shell::new(&mut messages);
    view.as_widget_mut().update(
        tree,
        &event,
        Layout::new(&node),
        mouse::Cursor::Available(at),
        &ui.renderer,
        &mut clipboard::Null,
        &mut shell,
        &Rectangle::with_size(ui.size),
    );
    drop(view);
    messages
}

/// The canvas pan before and after one two-finger scroll at `at`.
fn scroll_canvas<Id: 'static>(app: &App, at: Point) -> (iced::Vector, iced::Vector) {
    let ui = PointerUi::new();
    let view = app.view();
    let mut tree = iced::advanced::widget::Tree::new(view.as_widget());
    drop(view);
    let moved = iced::Event::Mouse(mouse::Event::CursorMoved { position: at });
    let _ = send_to_tree(&ui, app, &mut tree, at, moved);
    let before = canvas_state::<Id>(&mut tree)
        .expect("a mind map canvas")
        .pan;
    let scroll = iced::Event::Mouse(mouse::Event::WheelScrolled {
        delta: mouse::ScrollDelta::Pixels { x: 6.0, y: -10.0 },
    });
    let _ = send_to_tree(&ui, app, &mut tree, at, scroll);
    let after = canvas_state::<Id>(&mut tree)
        .expect("a mind map canvas")
        .pan;
    (before, after)
}

#[test]
fn scroll_pans_reaches_both_mind_map_canvases() {
    let (mut app, dir) = document_mindmap_app("p5-scroll-pans-doc");
    let mut ui = PointerUi::new();
    let plus = ui
        .text_bounds(&app, &ic::PLUS.to_string())
        .expect("zoom in button");
    let probe = Point::new(plus.center_x(), plus.y - 40.0);
    let (before, after) = scroll_canvas::<crate::ast::BlockId>(&app, probe);
    assert_eq!(before, after, "off: a scroll zooms and pans nothing");
    let _ = app.update(Message::ToggleMindmapScrollPans);
    let (before, after) = scroll_canvas::<crate::ast::BlockId>(&app, probe);
    assert_eq!(after - before, iced::Vector::new(6.0, -10.0), "on: it pans");
    let _ = std::fs::remove_dir_all(&dir);

    let (mut app, dir) = full_mindmap_app("p5-scroll-pans-fmm");
    let minus = ui
        .text_bounds(&app, &ic::MINUS.to_string())
        .expect("zoom out button");
    let probe = Point::new(minus.center_x(), minus.y - 40.0);
    let (before, after) = scroll_canvas::<WorkspaceNodeId>(&app, probe);
    assert_eq!(before, after, "off: a scroll zooms and pans nothing");
    let _ = app.update(Message::ToggleMindmapScrollPans);
    let (before, after) = scroll_canvas::<WorkspaceNodeId>(&app, probe);
    assert_eq!(after - before, iced::Vector::new(6.0, -10.0), "on: it pans");
    let _ = std::fs::remove_dir_all(&dir);
}
