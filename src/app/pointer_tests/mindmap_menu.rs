//! Mind map context menus (MDV-048 P6): a right press on a document map
//! node, a JSON/YAML data map node, a Full Mindmap node, or empty canvas.
//! The canvas gesture itself is tested in `crate::mindmap`.

use super::group3::{full_mindmap_app, tree_fixture};
use super::tree_menu::{menu_line, menu_lines, right_press, runs};
use super::*;
use crate::mindmap::{ZoomCommand, DOC_ROOT};

/// A document with nested headings, open in the mind map.
fn document_map(tag: &str, source: &str) -> (App, PathBuf) {
    let (mut app, dir) = tree_fixture(tag);
    let _ = app.update(Message::FileLoaded(Ok((
        dir.join("alpha.md"),
        source.to_owned(),
    ))));
    let _ = app.update(Message::ToggleMindmap);
    assert_eq!(app.view_mode, ViewMode::Mindmap);
    (app, dir)
}

/// The block id of the heading titled `title`.
fn heading(app: &App, title: &str) -> crate::ast::BlockId {
    app.ast
        .iter()
        .find_map(|(id, block)| match block {
            Block::Heading { inlines, .. }
                if crate::ipc::sections::inline_text(inlines) == title =>
            {
                Some(*id)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no heading {title:?}"))
}

/// Open the menu as a right press does: the press, then the canvas's ask.
fn open_menu(app: &mut App, ask: Message) {
    apply(
        app,
        vec![Message::PointerRightPress(Point::new(300.0, 200.0)), ask],
    );
    assert!(app.context_menu.is_some(), "the menu opens");
}

fn labels(app: &App) -> Vec<String> {
    menu_lines(app)
        .into_iter()
        .map(|(label, ..)| label)
        .collect()
}

const NESTED: &str =
    "# Top\n\n## Branch\n\n### Inner\n\n#### Deep\n\n## Other\n\n### Leaf\n\n## Last\n";

#[test]
fn a_document_heading_node_offers_reader_copy_and_fold_items() {
    let (mut app, dir) = document_map("mm-menu-doc", NESTED);
    let branch = heading(&app, "Branch");
    let selected = app.mindmap_selected;

    open_menu(&mut app, Message::MindmapContextMenu(Some(branch)));

    assert_eq!(
        labels(&app),
        [
            "Show in Reader",
            "Copy Heading Text",
            "Copy Link to Heading",
            "-",
            "Collapse All Below",
            "Expand All Below",
        ]
    );
    assert_eq!(
        menu_line(&app, "Show in Reader").2,
        runs(Message::MindmapActivateNode(branch)),
        "the same as a double click"
    );
    assert_eq!(
        menu_line(&app, "Copy Heading Text").2,
        runs(Message::CopyCode("Branch".into()))
    );
    assert_eq!(
        menu_line(&app, "Copy Link to Heading").2,
        runs(Message::CopyCode("alpha.md#branch".into()))
    );
    assert_eq!(
        menu_line(&app, "Collapse All Below").2,
        runs(Message::MindmapCollapseBelow(branch))
    );
    assert_eq!(
        menu_line(&app, "Expand All Below").2,
        None,
        "nothing below is folded yet"
    );
    assert_eq!(
        app.mindmap_selected, selected,
        "a right press selects nothing"
    );

    // A leaf has nothing to fold or unfold.
    let leaf = heading(&app, "Last");
    open_menu(&mut app, Message::MindmapContextMenu(Some(leaf)));
    assert_eq!(menu_line(&app, "Collapse All Below").2, None);
    assert_eq!(menu_line(&app, "Expand All Below").2, None);

    // The root is the document: no heading to copy.
    open_menu(&mut app, Message::MindmapContextMenu(Some(DOC_ROOT)));
    assert_eq!(
        labels(&app),
        [
            "Show in Reader",
            "-",
            "Collapse All Below",
            "Expand All Below"
        ]
    );
    assert_eq!(
        menu_line(&app, "Show in Reader").2,
        runs(Message::MindmapActivateNode(DOC_ROOT))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn collapse_and_expand_all_below_fold_the_whole_subtree() {
    let (mut app, dir) = document_map("mm-menu-fold", NESTED);
    let top = heading(&app, "Top");
    let branch = heading(&app, "Branch");
    let inner = heading(&app, "Inner");
    let other = heading(&app, "Other");
    let deep = heading(&app, "Deep");
    app.mindmap_collapsed.insert(top);
    app.mindmap_selected = Some(deep);

    let _ = app.update(Message::MindmapCollapseBelow(top));
    assert!(
        !app.mindmap_collapsed.contains(&top),
        "the node itself shows its children"
    );
    for id in [branch, inner, other] {
        assert!(
            app.mindmap_collapsed.contains(&id),
            "every branch below folds"
        );
    }
    assert!(
        !app.mindmap_collapsed.contains(&deep),
        "a leaf has nothing to fold"
    );
    assert_eq!(
        app.mindmap_selected,
        Some(top),
        "a selection hidden by the fold moves to the node"
    );
    let (nodes, _, _) = app.mindmap_layout();
    let shown: Vec<_> = nodes.iter().filter_map(|node| node.id).collect();
    assert!(shown.contains(&branch) && !shown.contains(&inner));

    open_menu(&mut app, Message::MindmapContextMenu(Some(top)));
    assert_eq!(
        menu_line(&app, "Collapse All Below").2,
        None,
        "already folded"
    );
    assert_eq!(
        menu_line(&app, "Expand All Below").2,
        runs(Message::MindmapExpandBelow(top))
    );

    app.mindmap_collapsed.insert(top);
    let _ = app.update(Message::MindmapExpandBelow(top));
    assert!(app.mindmap_collapsed.is_empty(), "everything below unfolds");
    let (nodes, _, _) = app.mindmap_layout();
    assert!(nodes.iter().any(|node| node.id == Some(deep)));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_data_map_node_copies_its_key_value_and_path() {
    let (mut app, dir) = tree_fixture("mm-menu-data");
    let _ = app.update(Message::FileLoaded(Ok((
        dir.join("data.json"),
        r#"{"a": {"b": [1, 2, {"c": "x"}], "d e": true}}"#.into(),
    ))));
    let _ = app.update(Message::ToggleMindmap);
    assert!(app.is_data_doc);
    let (nodes, _, paths) = app.mindmap_layout();
    let id_at = |path: &[crate::data_mindmap::PathSeg]| {
        nodes
            .iter()
            .filter_map(|node| node.id)
            .find(|id| paths.get(id).is_some_and(|p| p == path))
            .expect("node at path")
    };
    use crate::data_mindmap::PathSeg::{Index, Key};
    let b = id_at(&[Key("a".into()), Key("b".into())]);

    open_menu(&mut app, Message::MindmapContextMenu(Some(b)));
    assert_eq!(labels(&app), ["Copy Key", "Copy Value", "Copy Path"]);
    assert_eq!(
        menu_line(&app, "Copy Key").2,
        runs(Message::CopyCode("b".into()))
    );
    assert_eq!(
        menu_line(&app, "Copy Value").2,
        runs(Message::MindmapCopyDataValue(b))
    );
    assert_eq!(
        menu_line(&app, "Copy Path").2,
        runs(Message::CopyCode("a.b".into()))
    );
    assert_eq!(
        app.mindmap_data_value(b).as_deref(),
        Some("[\n  1,\n  2,\n  {\n    \"c\": \"x\"\n  }\n]"),
        "the value as JSON text"
    );

    let c = id_at(&[Key("a".into()), Key("b".into()), Index(2), Key("c".into())]);
    open_menu(&mut app, Message::MindmapContextMenu(Some(c)));
    assert_eq!(
        menu_line(&app, "Copy Path").2,
        runs(Message::CopyCode("a.b[2].c".into()))
    );
    assert_eq!(app.mindmap_data_value(c).as_deref(), Some("x"));

    let element = id_at(&[Key("a".into()), Key("b".into()), Index(2)]);
    open_menu(&mut app, Message::MindmapContextMenu(Some(element)));
    assert_eq!(
        menu_line(&app, "Copy Key").2,
        runs(Message::CopyCode("2".into())),
        "an array element's key is its index"
    );

    let spaced = id_at(&[Key("a".into()), Key("d e".into())]);
    open_menu(&mut app, Message::MindmapContextMenu(Some(spaced)));
    assert_eq!(
        menu_line(&app, "Copy Path").2,
        runs(Message::CopyCode("a[\"d e\"]".into()))
    );

    // The root has no key and no path.
    let root = nodes[0].id.expect("root id");
    open_menu(&mut app, Message::MindmapContextMenu(Some(root)));
    assert_eq!(menu_line(&app, "Copy Key").2, None);
    assert_eq!(menu_line(&app, "Copy Path").2, None);
    assert!(menu_line(&app, "Copy Value").2.is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn empty_canvas_offers_fit_and_actual_size() {
    let (mut app, dir) = document_map("mm-menu-canvas", NESTED);
    open_menu(&mut app, Message::MindmapContextMenu(None));
    assert_eq!(labels(&app), ["Zoom to Fit", "Actual Size"]);
    assert_eq!(
        menu_line(&app, "Zoom to Fit").2,
        runs(Message::MindmapZoom(ZoomCommand::Fit))
    );
    assert_eq!(
        menu_line(&app, "Actual Size").2,
        runs(Message::MindmapZoom(ZoomCommand::Reset))
    );
    let _ = std::fs::remove_dir_all(&dir);

    let (mut app, dir) = full_mindmap_app("mm-menu-full-canvas");
    open_menu(&mut app, Message::FullMindmapContextMenu(None));
    assert_eq!(labels(&app), ["Zoom to Fit", "Actual Size"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_full_mindmap_node_offers_open_reveal_copy_and_quick_slot() {
    let (mut app, dir) = full_mindmap_app("mm-menu-full");
    let file = dir.join("alpha.md");
    let file_id = WorkspaceNodeId::File(file.clone());
    let selected = app.full_mindmap.as_ref().unwrap().selected.clone();

    open_menu(
        &mut app,
        Message::FullMindmapContextMenu(Some(file_id.clone())),
    );
    assert_eq!(
        labels(&app),
        [
            "Open",
            "Reveal in Finder",
            "Copy Path",
            "-",
            "Add to Quick Slot"
        ]
    );
    assert_eq!(
        menu_line(&app, "Open").2,
        runs(Message::FullMindmapActivateNode(file_id.clone())),
        "the same as a double click"
    );
    assert_eq!(
        menu_line(&app, "Reveal in Finder").2,
        runs(Message::RevealPath(file.clone()))
    );
    assert_eq!(
        menu_line(&app, "Copy Path").2,
        runs(Message::CopyPath(file.to_str().unwrap().to_owned()))
    );
    assert_eq!(
        menu_line(&app, "Add to Quick Slot").2,
        runs(Message::QuickSlotAssignPath(file.clone()))
    );
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().selected,
        selected,
        "a right press selects nothing"
    );

    // A folder opens as the root; it goes in no Quick Slot.
    let folder = WorkspaceNodeId::Folder(dir.join("sub"));
    open_menu(
        &mut app,
        Message::FullMindmapContextMenu(Some(folder.clone())),
    );
    assert_eq!(labels(&app), ["Open", "Reveal in Finder", "Copy Path"]);
    assert_eq!(
        menu_line(&app, "Open").2,
        runs(Message::FullMindmapActivateNode(folder))
    );

    // The root is open already.
    let root = app.full_mindmap_graph().unwrap().root_id();
    open_menu(&mut app, Message::FullMindmapContextMenu(Some(root)));
    assert_eq!(menu_line(&app, "Open").2, None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_right_press_on_the_mind_map_canvas_opens_its_menu() {
    let (mut app, dir) = document_map("mm-menu-press", NESTED);
    let selected = app.mindmap_selected;
    let collapsed = app.mindmap_collapsed.clone();
    let mut ui = PointerUi::new();
    // Inside the canvas, left of the side panel.
    let at = Point::new(app.sidebar_width + 40.0, 400.0);

    let messages = right_press(&mut app, &mut ui, at);

    assert!(
        has(&messages, |m| matches!(m, Message::MindmapContextMenu(_))),
        "the canvas asks for its menu: {messages:?}"
    );
    assert!(app.context_menu.is_some());
    assert_eq!(app.mindmap_selected, selected);
    assert_eq!(app.mindmap_collapsed, collapsed);
    let _ = std::fs::remove_dir_all(&dir);

    let (mut app, dir) = full_mindmap_app("mm-menu-full-press");
    let mut ui = PointerUi::new();
    let at = Point::new(40.0, 400.0);
    let messages = right_press(&mut app, &mut ui, at);
    assert!(
        has(&messages, |m| matches!(
            m,
            Message::FullMindmapContextMenu(_)
        )),
        "the canvas asks for its menu: {messages:?}"
    );
    assert!(app.context_menu.is_some());
    let _ = std::fs::remove_dir_all(&dir);
}
