//! Sidebar peek (MDV-048 polish): while the sidebar is hidden, the pointer at
//! the left window edge shows it over the content; it hides a short delay
//! after the pointer leaves it.

use super::group3::{assert_tips, full_mindmap_app, tips, tree_fixture};
use super::toolbar::wait_outputs;
use super::*;
use iced::Vector;

/// In the left-edge band, below the toolbar row.
const EDGE: Point = Point::new(2.0, 400.0);
/// Over the reader, right of any sidebar width.
const READER: Point = Point::new(900.0, 400.0);

/// `tree_fixture` with a long `alpha.md` open and the sidebar hidden.
fn peek_app(tag: &str) -> (App, PathBuf) {
    let (mut app, dir) = tree_fixture(tag);
    let mut source = String::new();
    for i in 0..200 {
        source.push_str(&format!(
            "Paragraph {i} with enough words to fill a reader line here.\n\n"
        ));
    }
    let _ = app.update(Message::FileLoaded(Ok((dir.join("alpha.md"), source))));
    let _ = app.update(Message::ToggleSidebar);
    assert!(!app.sidebar_open, "the sidebar starts hidden");
    (app, dir)
}

fn peek_count(messages: &[Message], entered: bool) -> usize {
    messages
        .iter()
        .filter(|m| matches!(m, Message::SidebarPeek(e) if *e == entered))
        .count()
}

/// The reader's scroll area: its bounds and its content bounds.
struct BodyBounds(Option<(Rectangle, Rectangle)>);

impl Operation for BodyBounds {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        content_bounds: Rectangle,
        _translation: Vector,
        _state: &mut dyn iced::advanced::widget::operation::Scrollable,
    ) {
        if self.0.is_none() && id == Some(&App::scroll_id()) {
            self.0 = Some((bounds, content_bounds));
        }
    }
}

fn body_bounds(ui: &mut PointerUi, app: &App) -> (Rectangle, Rectangle) {
    let mut probe = BodyBounds(None);
    ui.operate(app, &mut probe);
    probe.0.expect("the reader is drawn")
}

/// Moves to the edge and applies what that publishes.
fn reveal(app: &mut App, ui: &mut PointerUi) {
    let _ = ui.move_to(app, READER);
    let (messages, _) = ui.move_to(app, EDGE);
    assert_eq!(peek_count(&messages, true), 1, "{messages:?}");
    apply(app, messages);
    assert!(app.sidebar_peek, "the peek shows");
}

#[test]
fn left_edge_reveals_the_sidebar_over_the_content() {
    let (mut app, dir) = peek_app("reveal");
    let mut ui = PointerUi::new();
    let _ = ui.move_to(&app, READER);
    assert!(ui.text_bounds(&app, "beta.md").is_none(), "hidden at first");
    let before = body_bounds(&mut ui, &app);

    let (messages, _) = ui.move_to(&app, EDGE);
    assert_eq!(peek_count(&messages, true), 1, "{messages:?}");
    apply(&mut app, messages);
    assert!(!app.sidebar_open, "a peek does not open the sidebar");
    let row = ui.text_bounds(&app, "beta.md").expect("the tree shows");
    assert!(row.x < app.sidebar_width, "{row:?}");
    assert_eq!(
        body_bounds(&mut ui, &app),
        before,
        "the reader does not reflow"
    );

    // The band alone does not confirm the hover. The peek reports the
    // pointer on it once, on its first event, and that cancels the band's
    // pending hide; plain moves inside it then publish nothing.
    let band_generation = app.sidebar_peek_generation;
    let (messages, _) = ui.move_to(&app, Point::new(4.0, 400.0));
    assert_eq!(peek_count(&messages, true), 1, "{messages:?}");
    assert_eq!(peek_count(&messages, false), 0, "{messages:?}");
    apply(&mut app, messages);
    let _ = app.update(Message::SidebarPeekHideAfter(band_generation));
    assert!(app.sidebar_peek, "the peek's hover cancels the band's hide");
    let generation = app.sidebar_peek_generation;
    let _ = app.update(Message::SidebarPeek(true));
    assert_eq!(
        app.sidebar_peek_generation, generation,
        "a repeat is a no-op"
    );
    for x in [10.0, 60.0, 120.0] {
        let (messages, _) = ui.move_to(&app, Point::new(x, 400.0));
        assert!(
            !has(&messages, |m| matches!(m, Message::SidebarPeek(_))),
            "no message on plain moves: {messages:?}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn leaving_the_peek_hides_it_after_the_delay() {
    let (mut app, dir) = peek_app("leave");
    let mut ui = PointerUi::new();
    reveal(&mut app, &mut ui);
    let _ = ui.move_to(&app, Point::new(60.0, 400.0));

    let (messages, _) = ui.move_to(&app, READER);
    assert_eq!(peek_count(&messages, false), 1, "{messages:?}");
    let mut hide = Vec::new();
    for message in messages {
        hide.extend(wait_outputs(app.update(message)));
    }
    assert!(
        ui.text_bounds(&app, "beta.md").is_some(),
        "stays during the delay"
    );
    assert!(
        has(&hide, |m| matches!(m, Message::SidebarPeekHideAfter(_))),
        "the delay ends in a hide message: {hide:?}"
    );
    apply(&mut app, hide);
    assert!(!app.sidebar_peek);
    assert!(
        ui.text_bounds(&app, "beta.md").is_none(),
        "hidden after the delay"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_peek_hides_when_the_pointer_leaves_the_window_across_the_band() {
    // The pointer slides out of the window across the band. The band
    // reports it, then the window loses the pointer before the new peek
    // could see it, so the peek never reports a leave of its own.
    let (mut app, dir) = peek_app("leave-window");
    let mut ui = PointerUi::new();
    let _ = ui.move_to(&app, READER);
    let (messages, _) = ui.move_to(&app, EDGE);
    assert_eq!(peek_count(&messages, true), 1, "{messages:?}");
    let mut hide = Vec::new();
    for message in messages {
        hide.extend(wait_outputs(app.update(message)));
    }
    assert!(app.sidebar_peek, "the peek shows");
    for message in ui.leave_window(&app) {
        hide.extend(wait_outputs(app.update(message)));
    }
    assert!(
        has(&hide, |m| matches!(m, Message::SidebarPeekHideAfter(_))),
        "a hide is scheduled: {hide:?}"
    );
    apply(&mut app, hide);
    assert!(!app.sidebar_peek, "the peek does not stay open");
    assert!(
        ui.text_bounds(&app, "beta.md").is_none(),
        "the reader is clear again"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn returning_to_the_peek_cancels_the_pending_hide() {
    let (mut app, dir) = peek_app("stale");
    let _ = app.update(Message::SidebarPeek(true));
    let _ = app.update(Message::SidebarPeek(false));
    let stale = app.sidebar_peek_generation;
    let _ = app.update(Message::SidebarPeek(true));
    let _ = app.update(Message::SidebarPeekHideAfter(stale));
    assert!(app.sidebar_peek, "a stale hide does nothing");
    let _ = app.update(Message::SidebarPeek(false));
    let current = app.sidebar_peek_generation;
    let _ = app.update(Message::SidebarPeekHideAfter(current));
    assert!(!app.sidebar_peek);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn clicking_a_file_in_the_peek_opens_it_and_keeps_the_peek() {
    let (mut app, dir) = peek_app("open");
    let mut ui = PointerUi::new();
    reveal(&mut app, &mut ui);
    let row = ui.text_bounds(&app, "beta.md").expect("beta row");
    let messages = ui.click(&app, center(row));
    assert!(
        has(&messages, |m| matches!(
            m,
            Message::TreeRowClicked { path, is_dir: false } if path.ends_with("beta.md")
        )),
        "{messages:?}"
    );
    apply(&mut app, messages);
    let beta = dir.join("beta.md");
    let _ = app.update(Message::FileLoaded(Ok((beta.clone(), "# Beta\n".into()))));
    assert_eq!(app.file.as_deref(), Some(beta.as_path()));
    assert!(
        app.sidebar_peek,
        "the peek stays while the pointer is on it"
    );
    assert!(!app.sidebar_open);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn keep_sidebar_open_pins_the_peek() {
    let (mut app, dir) = peek_app("pin");
    let mut ui = PointerUi::new();
    reveal(&mut app, &mut ui);
    assert_tips(&tips(&mut ui, &app), &["Keep Sidebar Open  ⌘B"]);
    let button = ui
        .id_bounds(&app, "sidebar-keep-open")
        .expect("the header button");
    assert!(button.x < app.sidebar_width, "{button:?}");
    let messages = ui.click(&app, center(button));
    assert!(
        has(&messages, |m| matches!(m, Message::ToggleSidebar)),
        "{messages:?}"
    );
    apply(&mut app, messages);
    assert!(app.sidebar_open, "pinned");
    assert!(!app.sidebar_peek, "no longer a peek");
    assert_tips(&tips(&mut ui, &app), &["Hide Sidebar  ⌘B"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn command_b_while_peeking_opens_the_sidebar_as_from_hidden() {
    let (mut app, dir) = peek_app("cmd-b");
    let _ = app.update(Message::SidebarPeek(true));
    assert!(app.sidebar_peek);
    let _ = app.update(Message::ToggleSidebar);
    assert!(app.sidebar_open);
    assert!(!app.sidebar_peek);
    // ⌘B again hides it, as before; no peek comes back on its own.
    let _ = app.update(Message::ToggleSidebar);
    assert!(!app.sidebar_open);
    assert!(!app.sidebar_peek);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_peek_has_no_divider_to_drag() {
    let (mut app, dir) = peek_app("divider");
    let mut ui = PointerUi::new();
    reveal(&mut app, &mut ui);
    let at = Point::new(app.sidebar_width + 2.0, 400.0);
    let (_, interaction) = ui.move_to(&app, at);
    assert_ne!(interaction, mouse::Interaction::ResizingHorizontally);
    let messages = ui.click(&app, at);
    assert!(
        !has(&messages, |m| matches!(m, Message::SidebarDragStart)),
        "{messages:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_band_starts_below_the_toolbar_row() {
    let (app, dir) = peek_app("top");
    let mut ui = PointerUi::new();
    let _ = ui.move_to(&app, READER);
    let (messages, _) = ui.move_to(&app, Point::new(2.0, 20.0));
    assert_eq!(peek_count(&messages, true), 0, "{messages:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn no_peek_without_a_workspace() {
    let mut app = App::default();
    assert!(app.workspace.is_none());
    let mut ui = PointerUi::new();
    let _ = ui.move_to(&app, READER);
    let (messages, _) = ui.move_to(&app, EDGE);
    assert_eq!(peek_count(&messages, true), 0, "{messages:?}");
    let _ = app.update(Message::SidebarPeek(true));
    assert!(!app.sidebar_peek);
}

#[test]
fn no_peek_in_full_mindmap() {
    let (mut app, dir) = full_mindmap_app("peek-full");
    let _ = app.update(Message::ToggleSidebar);
    app.sidebar_open = false;
    let mut ui = PointerUi::new();
    let _ = ui.move_to(&app, READER);
    let (messages, _) = ui.move_to(&app, EDGE);
    assert_eq!(peek_count(&messages, true), 0, "{messages:?}");
    let _ = app.update(Message::SidebarPeek(true));
    assert!(!app.sidebar_peek);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn no_peek_while_a_button_is_held() {
    let (app, dir) = peek_app("held");
    let mut ui = PointerUi::new();
    let _ = ui.move_to(&app, READER);
    let _ = ui.send(
        &app,
        READER,
        vec![Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Left,
        ))],
    );
    // A drag-select that reaches the edge.
    let (messages, _) = ui.move_to(&app, Point::new(200.0, 400.0));
    assert_eq!(peek_count(&messages, true), 0);
    let (messages, _) = ui.move_to(&app, EDGE);
    assert_eq!(peek_count(&messages, true), 0, "{messages:?}");
    let _ = ui.send(
        &app,
        EDGE,
        vec![Event::Mouse(mouse::Event::ButtonReleased(
            mouse::Button::Left,
        ))],
    );
    let _ = ui.move_to(&app, READER);
    let (messages, _) = ui.move_to(&app, EDGE);
    assert_eq!(
        peek_count(&messages, true),
        1,
        "after the release: {messages:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_release_lost_to_a_window_drag_does_not_block_the_peek() {
    // A window drag can swallow the button release (winit documents this
    // for macOS). The band then asks the system whether a button is down.
    use crate::app::view::TEST_BUTTONS_DOWN;
    let (app, dir) = peek_app("drag");
    let mut ui = PointerUi::new();
    let _ = ui.move_to(&app, READER);
    let _ = ui.send(
        &app,
        READER,
        vec![Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Left,
        ))],
    );
    // The drag is still on: no peek.
    TEST_BUTTONS_DOWN.with(|down| down.set(Some(true)));
    let (messages, _) = ui.move_to(&app, EDGE);
    assert_eq!(peek_count(&messages, true), 0, "{messages:?}");
    let _ = ui.move_to(&app, READER);
    // The drag ended, but its release never arrived.
    TEST_BUTTONS_DOWN.with(|down| down.set(Some(false)));
    let (messages, _) = ui.move_to(&app, EDGE);
    TEST_BUTTONS_DOWN.with(|down| down.set(None));
    assert_eq!(
        peek_count(&messages, true),
        1,
        "no button is down: {messages:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_peek_closes_for_overlays_and_full_mindmap() {
    let (mut app, dir) = peek_app("close");
    let _ = app.update(Message::SidebarPeek(true));
    let _ = app.update(Message::OpenCommandPalette);
    assert!(!app.sidebar_peek, "an overlay closes the peek");
    let _ = app.update(Message::CloseOverlay);

    let _ = app.update(Message::SidebarPeek(true));
    assert!(app.sidebar_peek);
    let _ = app.update(Message::ToggleFullMindmap);
    assert!(!app.sidebar_peek, "Full Mindmap closes the peek");
    let _ = std::fs::remove_dir_all(&dir);
}
