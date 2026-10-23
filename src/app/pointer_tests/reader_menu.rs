//! The reader's context menu (MDV-048 P6): what a right press on selected
//! text, a link, an image, a code block, a heading, a diagram, a math block,
//! or blank space offers. A right press never changes the selection.

use super::tree_menu::{menu_line, menu_lines, right_press, runs};
use super::*;
use crate::ast::{Block, DiagramKind};

const IMAGE: &str = "https://example.com/pic.png";

/// `a.md` in a workspace, open in the reader, with one element of each
/// kind; the sidebar is closed so the reader has margins on both sides.
fn fixture(tag: &str) -> (App, PathBuf) {
    let dir = std::env::temp_dir().join(format!("rmdv-reader-menu-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("docs")).expect("fixture dir");
    let doc = dir.join("docs").join("a.md");
    std::fs::write(
        &doc,
        format!(
            "# My Heading: Part 2\n\n\
             Some plain words here to select from the reader.\n\n\
             [a link to the notes](notes.md#top)\n\n\
             ```rust\nfn main() {{}}\n```\n\n\
             ![pic]({IMAGE})\n\n\
             ```mermaid\ngraph TD; A-->B\n```\n\n\
             $$\nx^2 + y^2\n$$\n"
        ),
    )
    .expect("a.md");
    let dir = canonicalize_existing_path(dir);
    let doc = dir.join("docs").join("a.md");
    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    let source = std::fs::read_to_string(&doc).expect("a.md");
    let _ = app.update(Message::FileLoaded(Ok((doc, source))));
    let handle = iced::widget::image::Handle::from_rgba(200, 100, vec![200u8; 200 * 100 * 4]);
    app.image_cache
        .insert(IMAGE.to_string(), ImageState::Loaded(handle));
    app.sidebar_open = false;
    (app, dir)
}

/// The bounds of the first top-level block that `pick` accepts.
fn block_bounds(ui: &mut PointerUi, app: &App, pick: impl Fn(&Block) -> bool) -> Rectangle {
    let id = app
        .ast
        .iter()
        .find(|(_, block)| pick(block))
        .map(|(id, _)| *id)
        .expect("the block is in the document");
    ui.find(
        app,
        Find {
            text: None,
            contains: None,
            id: Some(crate::render::block_anchor_id(id)),
            found: None,
            found_text: None,
        },
    )
    .expect("the block is on screen")
    .0
}

fn paragraph(ui: &mut PointerUi, app: &App) -> Rectangle {
    block_bounds(
        ui,
        app,
        |b| matches!(b, Block::Paragraph(inlines) if crate::plain_text::block_text(&Block::Paragraph(inlines.clone())).is_some_and(|t| t.starts_with("Some plain"))),
    )
}

/// A left drag inside the reader from `from` to `to`.
fn drag(app: &mut App, ui: &mut PointerUi, from: Point, to: Point) {
    let _ = ui.move_to(app, from);
    let mut messages = ui
        .send(
            app,
            from,
            vec![Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Left,
            ))],
        )
        .0;
    messages.extend(ui.move_to(app, to).0);
    messages.extend(
        ui.send(
            app,
            to,
            vec![Event::Mouse(mouse::Event::ButtonReleased(
                mouse::Button::Left,
            ))],
        )
        .0,
    );
    apply(app, messages);
}

/// The hit the reader reported for the last right press.
fn hit(messages: &[Message]) -> ReaderHit {
    messages
        .iter()
        .find_map(|m| match m {
            Message::ReaderContextMenu(hit) => Some(hit.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the reader reports the right press: {messages:?}"))
}

fn labels(app: &App) -> Vec<String> {
    menu_lines(app)
        .into_iter()
        .map(|(label, ..)| label)
        .collect()
}

fn close_menu(app: &mut App) {
    let _ = app.update(Message::ContextMenuClose);
}

#[test]
fn a_right_press_on_selected_text_offers_copy_and_keeps_the_selection() {
    let (mut app, dir) = fixture("selection");
    let mut ui = PointerUi::new();
    let text = paragraph(&mut ui, &app);
    let y = text.y + 8.0;
    drag(
        &mut app,
        &mut ui,
        Point::new(text.x + 1.0, y),
        Point::new(text.x + 80.0, y),
    );

    let messages = right_press(&mut app, &mut ui, Point::new(text.x + 30.0, y));
    let first = hit(&messages);
    let selected = first.selection.clone().expect("the selection is reported");
    assert!(selected.starts_with("Some"), "{selected:?}");
    assert!(first.in_selection, "the press is inside the selection");
    assert_eq!(first.link, None);
    assert_eq!(labels(&app)[..2], ["Copy", "Select All"]);
    assert_eq!(
        menu_line(&app, "Copy").2,
        runs(Message::CopyCode(selected.clone()))
    );
    assert_eq!(
        menu_line(&app, "Select All").2,
        runs(Message::SelectAllDocument)
    );
    close_menu(&mut app);

    // The press did not clear the selection; a press outside it still
    // offers Copy, after the items for what is under the pointer.
    let messages = right_press(&mut app, &mut ui, Point::new(text.x + 30.0, y));
    assert_eq!(hit(&messages).selection.as_deref(), Some(selected.as_str()));
    close_menu(&mut app);
    let outside = Point::new(text.x + 300.0, y);
    let messages = right_press(&mut app, &mut ui, outside);
    let second = hit(&messages);
    assert_eq!(second.selection.as_deref(), Some(selected.as_str()));
    assert!(!second.in_selection, "the press is past the selection");
    assert!(
        labels(&app).contains(&"Copy".to_string()),
        "{:?}",
        labels(&app)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_right_press_with_no_selection_disables_copy_and_offers_the_document() {
    let (mut app, dir) = fixture("blank");
    let doc = app.file.clone().expect("a.md is open");
    let mut ui = PointerUi::new();
    let text = paragraph(&mut ui, &app);

    // On plain text, and in the margin beside the reading column.
    for at in [
        Point::new(text.x + 30.0, text.y + 8.0),
        Point::new(24.0, text.y + 8.0),
    ] {
        let messages = right_press(&mut app, &mut ui, at);
        assert!(
            has(&messages, |m| matches!(m, Message::ReaderContextMenu(_))),
            "{at:?}: {messages:?}"
        );
        assert_eq!(
            labels(&app),
            [
                "Copy",
                "Select All",
                "-",
                "Copy Path",
                "Reveal in Finder",
                "-",
                "Read",
                "Edit",
                "Mind Map",
            ],
            "{at:?}"
        );
        assert_eq!(menu_line(&app, "Copy").2, None, "nothing to copy");
        assert_eq!(
            menu_line(&app, "Copy Path").2,
            runs(Message::CopyPath(doc.to_string_lossy().into_owned()))
        );
        assert_eq!(
            menu_line(&app, "Reveal in Finder").2,
            runs(Message::RevealPath(doc.clone()))
        );
        assert_eq!(menu_line(&app, "Read").2, None, "the reader is current");
        assert_eq!(
            menu_line(&app, "Edit"),
            (
                "Edit".to_string(),
                Some("⌘E"),
                runs(Message::ToolbarView(ToolbarView::Edit))
            )
        );
        assert_eq!(
            menu_line(&app, "Mind Map"),
            (
                "Mind Map".to_string(),
                Some("⌘M"),
                runs(Message::ToolbarView(ToolbarView::Mindmap))
            )
        );
        close_menu(&mut app);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_right_press_on_a_link_offers_open_and_copy_link() {
    let (mut app, dir) = fixture("link");
    let mut ui = PointerUi::new();
    let link = block_bounds(
        &mut ui,
        &app,
        |b| matches!(b, Block::Paragraph(inlines) if matches!(inlines.first(), Some(crate::ast::Inline::Link { .. }))),
    );
    let at = Point::new(link.x + 20.0, link.y + 8.0);

    let messages = right_press(&mut app, &mut ui, at);
    assert_eq!(hit(&messages).link.as_deref(), Some("notes.md#top"));
    assert_eq!(labels(&app)[..2], ["Open Link", "Copy Link Address"]);
    assert_eq!(
        menu_line(&app, "Open Link").2,
        runs(Message::OpenLink("notes.md#top".into())),
        "the same path as a click"
    );
    assert_eq!(
        menu_line(&app, "Copy Link Address").2,
        runs(Message::CopyCode("notes.md#top".into()))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_right_press_on_an_image_offers_open_and_copy_image_address() {
    let (mut app, dir) = fixture("image");
    let mut ui = PointerUi::new();
    let image = block_bounds(&mut ui, &app, |b| matches!(b, Block::Image { .. }));

    let messages = right_press(&mut app, &mut ui, image.center());
    assert!(
        has(&messages, |m| matches!(
            m,
            Message::ReaderTarget(ReaderTarget::Image { key, link: None }) if key == IMAGE
        )),
        "{messages:?}"
    );
    assert_eq!(labels(&app)[..2], ["Open Image", "Copy Image Address"]);
    assert_eq!(
        menu_line(&app, "Open Image").2,
        runs(Message::OpenImageZoom(IMAGE.into()))
    );
    assert_eq!(
        menu_line(&app, "Copy Image Address").2,
        runs(Message::CopyCode(IMAGE.into()))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_right_press_on_a_code_block_offers_copy_code() {
    let (mut app, dir) = fixture("code");
    let code = app
        .ast
        .iter()
        .find_map(|(_, b)| match b {
            Block::CodeBlock { code, .. } => Some(code.clone()),
            _ => None,
        })
        .expect("a code block");
    let mut ui = PointerUi::new();
    let bounds = block_bounds(&mut ui, &app, |b| matches!(b, Block::CodeBlock { .. }));

    let _ = right_press(
        &mut app,
        &mut ui,
        Point::new(bounds.x + 40.0, bounds.center_y()),
    );
    assert_eq!(labels(&app)[0], "Copy Code");
    assert_eq!(
        menu_line(&app, "Copy Code").2,
        runs(Message::CopyCode(code))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_right_press_on_a_heading_copies_a_link_to_it() {
    let (mut app, dir) = fixture("heading");
    let mut ui = PointerUi::new();
    let heading = block_bounds(&mut ui, &app, |b| matches!(b, Block::Heading { .. }));

    let _ = right_press(
        &mut app,
        &mut ui,
        Point::new(heading.x + 30.0, heading.center_y()),
    );
    assert_eq!(labels(&app)[0], "Copy Link to Heading");
    // The slug the link resolver matches; the path from the workspace.
    assert_eq!(
        menu_line(&app, "Copy Link to Heading").2,
        runs(Message::CopyCode("docs/a.md#my-heading-part-2".into()))
    );
    assert_eq!(
        line_for_fragment(&app.source, "my-heading-part-2", false),
        Some(1),
        "the copied fragment resolves to the heading"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_right_press_on_a_diagram_or_math_block_copies_its_source() {
    let (mut app, dir) = fixture("diagram");
    let (hash, math) = {
        let mut hash = None;
        let mut math = None;
        for (_, b) in &app.ast {
            if let Block::Diagram {
                kind,
                source,
                hash: h,
            } = b
            {
                match kind {
                    DiagramKind::Math => math = Some(source.clone()),
                    _ => hash = Some(*h),
                }
            }
        }
        (hash.expect("a diagram"), math.expect("a math block"))
    };
    let mut ui = PointerUi::new();

    let diagram = block_bounds(
        &mut ui,
        &app,
        |b| matches!(b, Block::Diagram { kind, .. } if *kind != DiagramKind::Math),
    );
    let _ = right_press(
        &mut app,
        &mut ui,
        Point::new(diagram.x + 40.0, diagram.center_y()),
    );
    assert_eq!(labels(&app)[0], "Copy Source");
    assert_eq!(
        menu_line(&app, "Copy Source").2,
        runs(Message::CopyDiagramSource(hash))
    );
    close_menu(&mut app);

    let formula = block_bounds(&mut ui, &app, |b| {
        matches!(
            b,
            Block::Diagram {
                kind: DiagramKind::Math,
                ..
            }
        )
    });
    let _ = right_press(&mut app, &mut ui, formula.center());
    assert_eq!(labels(&app)[0], "Copy Source");
    assert_eq!(
        menu_line(&app, "Copy Source").2,
        runs(Message::CopyCode(math))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_right_press_keeps_the_whole_document_selected_and_copy_copies_it() {
    let (mut app, dir) = fixture("select-all");
    let mut ui = PointerUi::new();
    let _ = app.update(Message::SelectAllDocument);
    assert!(app.reader_select_all);
    let right = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right));
    assert!(
        select_all_ends_on(&right).is_none(),
        "a right press does not end the whole-document selection"
    );
    let left = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
    assert!(select_all_ends_on(&left).is_some(), "a click still does");

    let text = paragraph(&mut ui, &app);
    let _ = right_press(&mut app, &mut ui, Point::new(text.x + 30.0, text.y + 8.0));
    assert!(app.reader_select_all, "the right press keeps it");
    assert_eq!(menu_line(&app, "Copy").2, runs(Message::CopyDocument));
    assert!(
        !app.select_all_clear_listens(),
        "a press that closes the menu does not end it either"
    );
    close_menu(&mut app);
    assert!(app.select_all_clear_listens());
    let _ = std::fs::remove_dir_all(&dir);
}
