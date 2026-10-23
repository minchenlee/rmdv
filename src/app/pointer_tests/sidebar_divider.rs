//! The sidebar divider (MDV-048 P7): a hover line, a width that persists
//! in prefs, a drag below the minimum that collapses the sidebar (a re-open
//! restores the last width), and a drag that ends wherever the button goes
//! up.

use super::group3::tree_fixture;
use super::*;

fn prefs_path(app: &App) -> PathBuf {
    app.quick_slots_persistence_path
        .clone()
        .expect("the test prefs path")
}

#[test]
fn the_divider_shows_a_one_pixel_line_on_hover() {
    let (app, dir) = tree_fixture("divider-line");
    let mut ui = PointerUi::new();
    let handle = ui.id_bounds(&app, SIDEBAR_DIVIDER_ID).expect("divider");
    let line = ui
        .id_bounds(&app, SIDEBAR_DIVIDER_LINE_ID)
        .expect("the divider's hover line");
    assert_eq!(line.width, 1.0, "a hairline: {line:?}");
    assert!(
        (line.center_x() - handle.center_x()).abs() < 1.0,
        "centered in the divider: {line:?} in {handle:?}"
    );
    for x in [handle.x + 0.5, handle.x + handle.width - 0.5] {
        let (_, cursor) = ui.move_to(&app, Point::new(x, handle.center_y()));
        assert_eq!(cursor, mouse::Interaction::ResizingHorizontally, "at {x}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_dragged_width_is_saved_once_when_the_drag_ends() {
    let (mut app, dir) = tree_fixture("divider-save");
    let path = prefs_path(&app);
    let _ = std::fs::remove_file(&path);
    let _ = app.update(Message::SidebarDragStart);
    let _ = app.update(Message::SidebarDragMove(330.0));
    let _ = app.update(Message::SidebarDragMove(350.0));
    assert!(!path.exists(), "no prefs write while the divider moves");
    let _ = app.update(Message::SidebarDragEnd);
    assert_eq!(app.sidebar_width, 350.0);
    assert_eq!(crate::prefs::load_from(&path).sidebar_width, 350.0);

    let _ = app.update(Message::SidebarResetWidth);
    assert_eq!(
        crate::prefs::load_from(&path).sidebar_width,
        SIDEBAR_WIDTH,
        "a double-click reset is saved too"
    );
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_saved_width_loads_leniently() {
    let load = |json: &str| -> f32 {
        serde_json::from_str::<crate::prefs::Prefs>(json)
            .expect("prefs parse")
            .sidebar_width
    };
    assert_eq!(load("{}"), SIDEBAR_WIDTH, "missing: the default");
    assert_eq!(load(r#"{"sidebar_width":320}"#), 320.0);
    assert_eq!(load(r#"{"sidebar_width":"wide"}"#), SIDEBAR_WIDTH);
    assert_eq!(load(r#"{"sidebar_width":5000}"#), SIDEBAR_MAX);
    assert_eq!(load(r#"{"sidebar_width":10}"#), SIDEBAR_MIN);
    let prefs: crate::prefs::Prefs =
        serde_json::from_str(r#"{"sidebar_width":null,"show_footer":false}"#).unwrap();
    assert_eq!(prefs.sidebar_width, SIDEBAR_WIDTH);
    assert!(!prefs.show_footer, "a bad width keeps the other prefs");
}

#[test]
fn dragging_below_the_minimum_collapses_and_a_reopen_restores_the_width() {
    let (mut app, dir) = tree_fixture("divider-collapse");
    app.sidebar_width = 300.0;
    let _ = app.update(Message::SidebarDragStart);
    let _ = app.update(Message::SidebarDragMove(200.0));
    assert_eq!(app.sidebar_width, 200.0);
    let _ = app.update(Message::SidebarDragMove(40.0));
    assert!(!app.sidebar_open, "below the minimum the sidebar collapses");

    // Back out during the same drag: it opens again under the pointer.
    let _ = app.update(Message::SidebarDragMove(260.0));
    assert!(app.sidebar_open);
    assert_eq!(app.sidebar_width, 260.0);

    let _ = app.update(Message::SidebarDragMove(20.0));
    let _ = app.update(Message::SidebarDragEnd);
    assert!(!app.sidebar_open, "released while collapsed");
    assert_eq!(
        crate::prefs::load_from(&prefs_path(&app)).sidebar_width,
        300.0,
        "the width before the drag is the one kept"
    );
    let _ = app.update(Message::ToggleSidebar);
    assert!(app.sidebar_open);
    assert_eq!(
        app.sidebar_width, 300.0,
        "a re-open restores the last width"
    );
    let _ = std::fs::remove_file(prefs_path(&app));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_drag_ends_on_a_release_anywhere_or_a_lost_release() {
    use iced::mouse::{Button, Event as Mouse};
    let ended =
        |event: Event| matches!(sidebar_drag_message(&event), Some(Message::SidebarDragEnd));
    // The subscription does not look at the pointer: a release outside the
    // window (reported with any position) ends the drag.
    assert!(ended(Event::Mouse(Mouse::ButtonReleased(Button::Left))));
    // A release the window never saw: the next press, or the window
    // losing focus, ends the drag instead of leaving the divider stuck to
    // the pointer.
    assert!(ended(Event::Mouse(Mouse::ButtonPressed(Button::Left))));
    assert!(ended(Event::Window(iced::window::Event::Unfocused)));
    assert!(matches!(
        sidebar_drag_message(&Event::Mouse(Mouse::CursorMoved {
            position: Point::new(-40.0, 10.0)
        })),
        Some(Message::SidebarDragMove(x)) if x == -40.0
    ));
}
