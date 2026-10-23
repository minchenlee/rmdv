//! Hit targets and button states (MDV-048 P7): ghost buttons, sidebar tabs,
//! and Quick Slot chips are at least 28 px tall; ghost buttons show a
//! pressed state; a toast's action button answers hover and press; a
//! Settings switch row toggles from anywhere on the row.

use super::group3::tree_fixture;
use super::slot_menu::slots_app;
use super::*;

const MIN_TARGET: f32 = 28.0;

#[test]
fn ghost_buttons_and_sidebar_tabs_are_at_least_28_px_tall() {
    let (app, dir) = tree_fixture("targets-sidebar");
    let mut ui = PointerUi::new();
    for id in ["sidebar-hide", "sidebar-tab-files", "sidebar-tab-outline"] {
        let bounds = ui.id_bounds(&app, id).expect(id);
        assert!(bounds.height >= MIN_TARGET, "{id}: {bounds:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn quick_slot_chips_are_at_least_28_px_tall() {
    let (mut app, dir) = slots_app("targets-slots");
    app.quick_slots_rail_revealed = true;
    let mut ui = PointerUi::new();
    // Slot 2 is empty: its row is the chip alone.
    let chip = ui.id_bounds(&app, "quick-slot-2").expect("slot 2");
    assert!(chip.height >= MIN_TARGET, "{chip:?}");
    assert!(chip.width >= MIN_TARGET, "{chip:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ghost_buttons_show_a_pressed_state() {
    let pal = App::default().palette;
    let pressed = ghost_style(button::Status::Pressed, pal);
    assert_eq!(pressed.background, Some(Background::Color(pal.active())));
    assert_ne!(
        pressed.background,
        ghost_style(button::Status::Hovered, pal).background
    );
    assert_eq!(ghost_style(button::Status::Active, pal).background, None);
}

#[test]
fn toast_action_buttons_answer_hover_and_press() {
    let pal = App::default().palette;
    let rest = toast_action_style(button::Status::Active, pal).background;
    let hover = toast_action_style(button::Status::Hovered, pal).background;
    let press = toast_action_style(button::Status::Pressed, pal).background;
    assert_ne!(rest, hover, "hover changes the fill");
    assert_ne!(hover, press, "press changes it again");
    assert_ne!(rest, press);
}

#[test]
fn a_settings_switch_row_toggles_from_its_label() {
    let mut app = App::default();
    let _ = app.update(Message::ToggleSettings);
    assert!(app.settings_open);
    // Tall enough that the Reading rows need no scrolling.
    let mut ui = PointerUi::sized(Size::new(1280.0, 1600.0));
    let shown = app.show_footer;
    let label = ui
        .text_bounds(&app, "Status footer")
        .expect("the row label");
    let messages = ui.click(&app, center(label));
    apply(&mut app, messages);
    assert_eq!(app.show_footer, !shown, "a click on the label toggles");

    // A click on the switch itself toggles once, not twice.
    let hint = ui
        .text_bounds(&app, "Word count and reading time below the document.")
        .expect("the row hint");
    let row_y = hint.y;
    let window_w = 1280.0;
    let mut toggled = None;
    for x in (900..1260).step_by(4) {
        let (_, cursor) = ui.move_to(&app, Point::new(x as f32, row_y));
        if cursor == mouse::Interaction::Pointer {
            toggled = Some(x as f32);
            break;
        }
    }
    let x = toggled.expect("the switch is under the pointer somewhere right of the label");
    assert!(x < window_w);
    let before = app.show_footer;
    let messages = ui.click(&app, Point::new(x, row_y));
    apply(&mut app, messages);
    assert_eq!(app.show_footer, !before, "one toggle per click");
    let _ = std::fs::remove_file(app.quick_slots_persistence_path.clone().unwrap());
}
