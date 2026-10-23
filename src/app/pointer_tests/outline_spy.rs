//! Outline scroll-spy (MDV-048 P7): the outline marks the section that holds
//! the reader's top line, and its list scrolls to keep that row in view
//! unless the pointer is over the outline.

use super::history::{run, send, settle};
use super::*;

/// Where the reader sits beside the open sidebar.
const READER: Point = Point::new(800.0, 420.0);

/// A workspace whose `doc.md` has `sections` headings, each followed by
/// enough paragraphs to fill more than a screen between two headings.
fn fixture(tag: &str, sections: usize) -> (App, PathBuf) {
    let dir = std::env::temp_dir().join(format!("rmdv-spy-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("fixture dir");
    let mut doc = String::new();
    for s in 0..sections {
        doc.push_str(&format!("## Section {s}\n\n"));
        for p in 0..6 {
            doc.push_str(&format!(
                "Paragraph {s}.{p} with enough words to fill a reader line here and a bit more.\n\n"
            ));
        }
    }
    std::fs::write(dir.join("doc.md"), &doc).expect("doc.md");
    let dir = canonicalize_existing_path(dir);
    let mut app = App::default();
    app.set_workspace(dir.clone(), true);
    app.sidebar_tab = SidebarTab::Outline;
    let mut ui = PointerUi::new();
    send(&mut app, &mut ui, Message::Open(dir.join("doc.md")));
    settle(&mut app, &mut ui);
    (app, dir)
}

/// The section whose heading is the last one at or above the reader's top
/// line, computed from the outline and the reader's block positions.
fn section_at_top(app: &App) -> Option<usize> {
    let top = app.body_offset();
    app.outline_sections.iter().rposition(|section| {
        let block = crate::ipc::lines::block_for_line(section.line, &app.block_lines).unwrap();
        let dpos = app.virt_window.display_pos(block).unwrap();
        app.virt_window.block_top(dpos) <= top + 1.0
    })
}

fn wheel_reader(app: &mut App, ui: &mut PointerUi, steps: usize) {
    let _ = ui.move_to(app, READER);
    let wheel = Event::Mouse(mouse::Event::WheelScrolled {
        delta: mouse::ScrollDelta::Pixels { x: 0.0, y: -120.0 },
    });
    for _ in 0..steps {
        let (messages, _) = ui.send(app, READER, vec![wheel.clone()]);
        for message in messages {
            send(app, ui, message);
        }
    }
    settle(app, ui);
}

fn outline_offset(app: &App) -> f32 {
    app.outline_viewport
        .as_ref()
        .map_or(0.0, |viewport| viewport.absolute_offset().y)
}

#[test]
fn scrolling_the_reader_marks_the_section_at_its_top_line() {
    let (mut app, dir) = fixture("mark", 12);
    let mut ui = PointerUi::new();
    assert_eq!(app.outline_cursor, 0);
    wheel_reader(&mut app, &mut ui, 30);
    let expected = section_at_top(&app).expect("a section above the top line");
    assert!(
        expected >= 2,
        "the reader passed a few sections: {expected}"
    );
    assert_eq!(
        app.outline_cursor, expected,
        "the outline marks the section that holds the reader's top line"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_outline_list_follows_the_marked_section() {
    let (mut app, dir) = fixture("follow", 80);
    let mut ui = PointerUi::new();
    wheel_reader(&mut app, &mut ui, 400);
    let marked = app.outline_cursor;
    assert!(marked > 40, "deep in the document: section {marked}");
    let viewport = app.outline_viewport.as_ref().expect("outline scrolled");
    let row_top = marked as f32 * SIDEBAR_ROW_H;
    let top = viewport.absolute_offset().y;
    assert!(
        row_top >= top - 1.0 && row_top + SIDEBAR_ROW_H <= top + viewport.bounds().height + 1.0,
        "the marked row {marked} (y {row_top}) is inside the outline viewport ({top}, height {})",
        viewport.bounds().height
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_outline_list_stays_put_while_the_pointer_is_over_it() {
    let (mut app, dir) = fixture("hover", 80);
    let mut ui = PointerUi::new();
    let row = ui.text_bounds(&app, "Section 3").expect("outline row");
    assert!(row.x < app.sidebar_width, "the sidebar row");
    let (messages, _) = ui.move_to(&app, center(row));
    apply(&mut app, messages);
    let before = outline_offset(&app);
    // The keyboard scrolls the reader while the pointer rests over the
    // outline. Each step's frame keeps the pointer there.
    for _ in 0..120 {
        let task = app.update(Message::ScrollBy(400.0));
        run(&mut app, &mut ui, task);
        let (messages, _) = ui.send(
            &app,
            center(row),
            vec![Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            ))],
        );
        for message in messages {
            let task = app.update(message);
            run(&mut app, &mut ui, task);
        }
    }
    assert!(app.outline_hovered, "the pointer is still over the outline");
    assert!(
        app.outline_cursor > 40,
        "the mark moved: {}",
        app.outline_cursor
    );
    assert!(
        (outline_offset(&app) - before).abs() < 1.0,
        "the outline list did not scroll under the pointer: {before} -> {}",
        outline_offset(&app)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_clicked_section_that_cannot_reach_the_top_stays_marked() {
    let (mut app, dir) = fixture("hold", 12);
    let mut ui = PointerUi::new();
    let last = app.outline_sections.len() - 1;
    let line = app.outline_sections[last].line;
    send(
        &mut app,
        &mut ui,
        Message::OutlineRowClicked { index: last, line },
    );
    settle(&mut app, &mut ui);
    let at_top = section_at_top(&app).expect("a section at the top");
    assert!(
        at_top < last,
        "the last section is too short to reach the top ({at_top})"
    );
    assert_eq!(app.outline_cursor, last, "the clicked section stays marked");

    // Scrolling back up past it hands the mark back to the spy.
    wheel_up(&mut app, &mut ui, 40);
    assert_eq!(Some(app.outline_cursor), section_at_top(&app));
    let _ = std::fs::remove_dir_all(&dir);
}

fn wheel_up(app: &mut App, ui: &mut PointerUi, steps: usize) {
    let _ = ui.move_to(app, READER);
    let wheel = Event::Mouse(mouse::Event::WheelScrolled {
        delta: mouse::ScrollDelta::Pixels { x: 0.0, y: 120.0 },
    });
    for _ in 0..steps {
        let (messages, _) = ui.send(app, READER, vec![wheel.clone()]);
        for message in messages {
            send(app, ui, message);
        }
    }
    settle(app, ui);
}

/// The marked row lies inside the outline list's viewport.
fn marked_row_in_view(app: &App) -> bool {
    let Some(viewport) = app.outline_viewport.as_ref() else {
        return false;
    };
    let row_top = app.outline_cursor as f32 * SIDEBAR_ROW_H;
    let top = viewport.absolute_offset().y;
    row_top >= top - 1.0 && row_top + SIDEBAR_ROW_H <= top + viewport.bounds().height + 1.0
}

#[test]
fn the_outline_shown_again_scrolls_to_the_marked_section() {
    let (mut app, dir) = fixture("shown-again", 80);
    let mut ui = PointerUi::new();
    // The reader scrolls while the Files tab is up.
    send(&mut app, &mut ui, Message::SetSidebarTab(SidebarTab::Files));
    wheel_reader(&mut app, &mut ui, 400);
    assert!(app.outline_cursor > 40, "section {}", app.outline_cursor);
    send(
        &mut app,
        &mut ui,
        Message::SetSidebarTab(SidebarTab::Outline),
    );
    settle(&mut app, &mut ui);
    assert!(
        marked_row_in_view(&app),
        "the Outline tab opens on row {} ({:?})",
        app.outline_cursor,
        app.outline_viewport.as_ref().map(|v| v.absolute_offset())
    );

    // The same after the sidebar was hidden while the reader scrolled.
    send(&mut app, &mut ui, Message::ToggleSidebar);
    wheel_up(&mut app, &mut ui, 200);
    send(&mut app, &mut ui, Message::ToggleSidebar);
    settle(&mut app, &mut ui);
    assert!(
        marked_row_in_view(&app),
        "the sidebar opens on row {} ({:?})",
        app.outline_cursor,
        app.outline_viewport.as_ref().map(|v| v.absolute_offset())
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_outline_removed_under_the_pointer_does_not_stay_hovered() {
    let (mut app, dir) = fixture("unhover", 12);
    let mut ui = PointerUi::new();
    let hover = |app: &mut App, ui: &mut PointerUi| {
        let row = ui.text_bounds(app, "Section 3").expect("outline row");
        let (messages, _) = ui.move_to(app, center(row));
        apply(app, messages);
        assert!(app.outline_hovered);
    };
    // Zen hides the sidebar with the pointer over the outline.
    hover(&mut app, &mut ui);
    send(&mut app, &mut ui, Message::ToggleViewMode);
    assert!(!app.outline_hovered, "Zen took the outline away");
    send(&mut app, &mut ui, Message::ToggleViewMode);
    // Full Mindmap does too.
    hover(&mut app, &mut ui);
    send(&mut app, &mut ui, Message::ToggleFullMindmap);
    assert!(app.full_mindmap.is_some());
    assert!(!app.outline_hovered, "Full Mindmap took the outline away");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_scrollbar_strip_removed_under_the_pointer_does_not_stay_hovered() {
    let (mut app, dir) = fixture("strip", 12);
    let mut ui = PointerUi::new();
    send(&mut app, &mut ui, Message::ReaderScrollbarHover(true));
    assert!(app.reader_scroller.hovered);
    send(&mut app, &mut ui, Message::ToggleMindmap);
    assert_eq!(app.view_mode, ViewMode::Mindmap);
    assert!(
        !app.reader_scroller.hovered,
        "the mind map took the strip away"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
