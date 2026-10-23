//! The Quick Slot context menu (MDV-048 P6): a right press on a slot chip
//! of the rail (shown while ⌘ is held) offers Open, Replace with Current
//! File, Clear and Clear All; an empty slot offers Assign Current File.

use super::group3::tree_fixture;
use super::tree_menu::{menu_line, menu_lines, right_press, runs};
use super::*;

fn labels(app: &App) -> Vec<String> {
    menu_lines(app)
        .into_iter()
        .map(|(label, ..)| label)
        .collect()
}

pub(super) fn occupy(app: &mut App, index: usize, relative_path: &str) {
    assert!(app.quick_slots.set(
        index,
        crate::quick_slots::QuickSlot {
            relative_path: relative_path.into(),
            context: crate::quick_slots::SlotContext::default(),
        },
    ));
}

/// The tree fixture with `alpha.md` open, `alpha.md` in slot 1 and
/// `beta.md` in slot 3, and the rail shown.
pub(super) fn slots_app(tag: &str) -> (App, PathBuf) {
    let (mut app, dir) = tree_fixture(tag);
    let _ = app.update(Message::FileLoaded(Ok((
        dir.join("alpha.md"),
        "# Alpha\n".into(),
    ))));
    occupy(&mut app, 0, "alpha.md");
    occupy(&mut app, 2, "beta.md");
    app.quick_slots_rail_revealed = true;
    (app, dir)
}

/// Right press the rail chip of slot `index`.
fn right_press_slot(app: &mut App, ui: &mut PointerUi, index: usize) -> Vec<Message> {
    let chip = ui
        .text_bounds(app, &(index + 1).to_string())
        .expect("the slot chip shows");
    right_press(app, ui, center(chip))
}

#[test]
fn a_slot_chip_offers_open_replace_clear_and_clear_all() {
    let (mut app, dir) = slots_app("slot-menu-occupied");
    let mut ui = PointerUi::new();

    let messages = right_press_slot(&mut app, &mut ui, 2);

    assert!(
        has(&messages, |m| matches!(m, Message::QuickSlotContextMenu(2))),
        "the chip asks for its menu: {messages:?}"
    );
    assert_eq!(
        labels(&app),
        [
            "Open",
            "Replace with Current File",
            "Clear",
            "-",
            "Clear All"
        ]
    );
    assert_eq!(
        menu_line(&app, "Open"),
        (
            "Open".into(),
            Some("⌘3"),
            runs(Message::QuickSlotActivate(2))
        )
    );
    assert_eq!(
        menu_line(&app, "Replace with Current File"),
        (
            "Replace with Current File".into(),
            Some("⌥⌘3"),
            runs(Message::QuickSlotAssign(2))
        ),
        "the existing replace flow, with its confirmation"
    );
    assert_eq!(menu_line(&app, "Clear").2, runs(Message::QuickSlotClear(2)));
    assert_eq!(
        menu_line(&app, "Clear All").2,
        runs(Message::QuickSlotClearAll)
    );
    assert!(app.file.as_ref().is_some_and(|f| f.ends_with("alpha.md")));

    // The slot that holds the open file has nothing to replace.
    let _ = app.update(Message::ContextMenuClose);
    let _ = right_press_slot(&mut app, &mut ui, 0);
    assert_eq!(menu_line(&app, "Replace with Current File").2, None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_empty_slot_offers_assign_current_file() {
    let (mut app, dir) = slots_app("slot-menu-empty");
    let mut ui = PointerUi::new();

    let messages = right_press_slot(&mut app, &mut ui, 1);

    assert!(
        has(&messages, |m| matches!(m, Message::QuickSlotContextMenu(1))),
        "{messages:?}"
    );
    assert_eq!(labels(&app), ["Assign Current File", "-", "Clear All"]);
    assert_eq!(
        menu_line(&app, "Assign Current File"),
        (
            "Assign Current File".into(),
            Some("⌥⌘2"),
            runs(Message::QuickSlotAssign(1))
        )
    );
    let _ = std::fs::remove_dir_all(&dir);

    // No open file: nothing to assign. No slot in use: nothing to clear.
    let (mut app, dir) = tree_fixture("slot-menu-none");
    let _ = app.update(Message::PointerRightPress(Point::new(20.0, 300.0)));
    let _ = app.update(Message::QuickSlotContextMenu(4));
    assert_eq!(menu_line(&app, "Assign Current File").2, None);
    assert_eq!(menu_line(&app, "Clear All").2, None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn clear_from_the_menu_can_be_undone() {
    let (mut app, dir) = slots_app("slot-menu-clear");
    let mut ui = PointerUi::new();
    let _ = right_press_slot(&mut app, &mut ui, 2);
    let clear = labels(&app)
        .iter()
        .position(|label| label == "Clear")
        .expect("Clear item");

    apply(&mut app, vec![Message::ContextMenuChoose(clear)]);

    assert!(app.quick_slots.occupied(2).is_none(), "the slot is cleared");
    let toast = app.toast.as_ref().expect("a toast");
    assert_eq!(toast.text, "Quick Slot 3 cleared");
    assert!(toast
        .action
        .as_ref()
        .is_some_and(|action| matches!(action.message, Message::QuickSlotUndo)));
    let _ = app.update(Message::QuickSlotUndo);
    assert!(app.quick_slots.occupied(2).is_some(), "undo restores it");

    // Clear All, then one undo restores every slot.
    let _ = right_press_slot(&mut app, &mut ui, 0);
    let clear_all = labels(&app)
        .iter()
        .position(|label| label == "Clear All")
        .expect("Clear All item");
    apply(&mut app, vec![Message::ContextMenuChoose(clear_all)]);
    assert!(app.quick_slots.occupied(0).is_none());
    assert!(app.quick_slots.occupied(2).is_none());
    assert_eq!(
        app.toast.as_ref().map(|t| t.text.as_str()),
        Some("All Quick Slots cleared")
    );
    let _ = app.update(Message::QuickSlotUndo);
    assert!(app.quick_slots.occupied(0).is_some());
    assert!(app.quick_slots.occupied(2).is_some());
    let _ = std::fs::remove_dir_all(&dir);
}
