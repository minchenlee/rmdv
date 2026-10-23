//! The Quick Slots pointer entry (MDV-048 P7): a toolbar button pins the
//! same rail the ⌘ hold shows, as a popover with the P6 chip menus. An
//! outside press, Esc, a second click, or choosing a slot closes it; the ⌘
//! hold display is unchanged. Empty slots that cannot be assigned look
//! disabled.

use super::group3::{assert_tips, tips, tree_fixture};
use super::slot_menu::slots_app;
use super::*;

const POPOVER: &str = "quick-slots-popover";
const BUTTON: &str = "toolbar-quick-slots";

fn unrevealed(tag: &str) -> (App, PathBuf) {
    let (mut app, dir) = slots_app(tag);
    app.quick_slots_rail_revealed = false;
    (app, dir)
}

fn click_button(app: &mut App, ui: &mut PointerUi) {
    let button = ui.id_bounds(app, BUTTON).expect("the toolbar button");
    let messages = ui.click(app, center(button));
    apply(app, messages);
}

#[test]
fn the_toolbar_button_names_quick_slots_and_toggles_the_popover() {
    let (mut app, dir) = unrevealed("slot-popover-toggle");
    let mut ui = PointerUi::new();
    assert_tips(&tips(&mut ui, &app), &["Quick Slots  ⌘1–9"]);
    assert!(ui.id_bounds(&app, POPOVER).is_none(), "closed at first");

    click_button(&mut app, &mut ui);
    assert!(app.quick_slots_pinned, "the click pins the rail");
    let popover = ui.id_bounds(&app, POPOVER).expect("the popover shows");
    let chip = ui.text_bounds(&app, "3").expect("slot 3's chip");
    assert!(
        popover.contains(center(chip)),
        "the chips sit in the popover"
    );

    click_button(&mut app, &mut ui);
    assert!(!app.quick_slots_pinned, "a second click closes it");
    assert!(ui.id_bounds(&app, POPOVER).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_outside_press_or_esc_closes_the_popover() {
    let (mut app, dir) = unrevealed("slot-popover-close");
    let mut ui = PointerUi::new();
    click_button(&mut app, &mut ui);
    assert!(app.quick_slots_pinned);
    let messages = ui.click(&app, Point::new(900.0, 600.0));
    apply(&mut app, messages);
    assert!(!app.quick_slots_pinned, "a press outside closes it");

    use iced::keyboard::{key::Named, Key};
    let esc = Key::Named(Named::Escape);
    assert!(matches!(
        quick_slots_popover_key_message(&esc, true),
        Some(Message::QuickSlotsPopover(false))
    ));
    assert!(quick_slots_popover_key_message(&esc, false).is_none());
    assert!(quick_slots_popover_key_message(&Key::Character("a".into()), true).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn choosing_a_slot_closes_the_popover_and_a_right_press_keeps_it() {
    let (mut app, dir) = unrevealed("slot-popover-choose");
    let mut ui = PointerUi::new();
    click_button(&mut app, &mut ui);

    // The P6 chip menu opens from the popover and keeps it open.
    let chip = ui.text_bounds(&app, "3").expect("slot 3's chip");
    let messages = super::tree_menu::right_press(&mut app, &mut ui, center(chip));
    assert!(has(&messages, |m| matches!(
        m,
        Message::QuickSlotContextMenu(2)
    )));
    assert!(app.quick_slots_pinned, "a right press is inside");
    let _ = app.update(Message::ContextMenuClose);

    let messages = ui.click(&app, center(chip));
    assert!(
        has(&messages, |m| matches!(m, Message::QuickSlotActivate(2))),
        "{messages:?}"
    );
    apply(&mut app, messages);
    assert!(!app.quick_slots_pinned, "choosing a slot closes it");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_modifier_hold_display_is_unchanged() {
    let (mut app, dir) = unrevealed("slot-popover-hold");
    let mut ui = PointerUi::new();
    let _ = app.update(Message::QuickSlotsModifier(true));
    let generation = app.quick_slots_modifier_generation;
    let _ = app.update(Message::QuickSlotsModifierReveal(generation));
    assert!(app.quick_slots_rail_revealed);
    assert!(!app.quick_slots_pinned);
    assert!(
        ui.id_bounds(&app, POPOVER).is_none(),
        "the ⌘ hold shows the bare rail, not the popover"
    );
    assert!(ui.text_bounds(&app, "3").is_some(), "the rail shows");
    let _ = app.update(Message::QuickSlotsModifier(false));
    assert!(!app.quick_slots_rail_revealed);
    assert!(ui.text_bounds(&app, "3").is_none(), "and hides on release");

    // A ⌘ press and release while the popover is open leaves it open.
    click_button(&mut app, &mut ui);
    let _ = app.update(Message::QuickSlotsModifier(true));
    let _ = app.update(Message::QuickSlotsModifier(false));
    assert!(app.quick_slots_pinned);
    assert!(ui.id_bounds(&app, POPOVER).is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn empty_slots_that_cannot_be_assigned_look_disabled() {
    let pal = App::default().palette;
    let empty = quick_slot_chip_style(SlotChipLook::Empty, button::Status::Active, pal);
    let disabled = quick_slot_chip_style(SlotChipLook::Unassignable, button::Status::Disabled, pal);
    assert_eq!(disabled.text_color, pal.subtle, "dimmed number");
    assert_ne!(disabled.text_color, empty.text_color);
    assert_ne!(
        disabled.background, empty.background,
        "no chip fill for a slot that does nothing"
    );
    // Hovering it changes nothing.
    let hovered = quick_slot_chip_style(SlotChipLook::Unassignable, button::Status::Hovered, pal);
    assert_eq!(hovered.background, disabled.background);

    // No file open: nothing to assign, so the popover's empty chips are
    // disabled and a click does nothing.
    let (mut app, dir) = tree_fixture("slot-popover-disabled");
    let mut ui = PointerUi::new();
    click_button(&mut app, &mut ui);
    let chip = ui.text_bounds(&app, "5").expect("slot 5's chip");
    let messages = ui.click(&app, center(chip));
    assert!(
        !has(&messages, |m| matches!(m, Message::QuickSlotAssign(_))),
        "{messages:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_popover_closes_for_good_when_the_find_bar_takes_the_keys() {
    let (mut app, dir) = unrevealed("slot-popover-find");
    let mut ui = PointerUi::new();
    click_button(&mut app, &mut ui);
    assert!(app.quick_slots_pinned);
    let _ = app.update(Message::ToggleSearch);
    assert!(app.search_open);
    assert!(!app.quick_slots_pinned, "the find bar closes the popover");
    // Closing the find bar does not bring it back, and one click on the
    // toolbar button opens it again.
    let _ = app.update(Message::ToggleSearch);
    assert!(!app.search_open);
    assert!(ui.id_bounds(&app, POPOVER).is_none(), "it stays closed");
    click_button(&mut app, &mut ui);
    assert!(app.quick_slots_pinned, "one click opens it");

    // The vault page does the same.
    let _ = app.update(Message::OpenVaultSearch);
    assert!(app.vault_open);
    assert!(!app.quick_slots_pinned, "the vault page closes the popover");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_press_on_the_popover_padding_stays_in_the_popover() {
    let (mut app, dir) = unrevealed("slot-popover-padding");
    let mut ui = PointerUi::new();
    click_button(&mut app, &mut ui);
    let popover = ui.id_bounds(&app, POPOVER).expect("the popover shows");
    let padding = Point::new(popover.x + 2.0, popover.center_y());
    let chip = ui.text_bounds(&app, "3").expect("slot 3's chip");
    assert!(
        padding.x < chip.x,
        "{padding:?} is in the padding left of {chip:?}"
    );
    let messages = super::tree_menu::right_press(&mut app, &mut ui, padding);
    assert!(
        !has(&messages, |m| matches!(m, Message::ReaderContextMenu(_))),
        "the reader behind the popover does not get the press: {messages:?}"
    );
    let left = ui.click(&app, padding);
    assert!(
        !has(&left, |m| matches!(m, Message::QuickSlotsPopover(false))),
        "a press inside keeps it open: {left:?}"
    );
    apply(&mut app, left);
    assert!(app.quick_slots_pinned);
    let _ = std::fs::remove_dir_all(&dir);
}
