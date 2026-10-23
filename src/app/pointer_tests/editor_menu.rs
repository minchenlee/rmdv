//! The Zen editor's context menu (MDV-048 P6): Cut, Copy, Paste, and
//! Select All. The editor keeps its focus and selection, and the manual
//! undo stack covers Cut and Paste.

use super::group2::apply_ui;
use super::tree_menu::{menu_line, menu_lines, right_press, runs};
use super::*;
use iced::widget::text_editor::{Action, Edit};

const EDITOR: &str = "zen-editor";

/// The long document in the Zen editor, focused.
fn editing() -> (App, PointerUi) {
    let mut app = long_doc_app();
    let mut ui = PointerUi::new();
    let _ = ui.move_to(&app, Point::new(640.0, 400.0));
    apply_ui(&mut app, &mut ui, vec![Message::ToggleViewMode]);
    assert_eq!(ui.focused(&app, EDITOR), Some(true));
    (app, ui)
}

fn editor_text(app: &App) -> String {
    app.editor.as_ref().expect("the editor is open").text()
}

fn editor_selection(app: &App) -> Option<String> {
    app.editor.as_ref().expect("the editor is open").selection()
}

fn labels(app: &App) -> Vec<String> {
    menu_lines(app)
        .into_iter()
        .map(|(label, ..)| label)
        .collect()
}

/// Choose the menu item labelled `label`, as a click on it does.
fn choose(app: &mut App, ui: &mut PointerUi, label: &str) {
    let index = labels(app)
        .iter()
        .position(|l| l == label)
        .unwrap_or_else(|| panic!("no {label:?} in {:?}", labels(app)));
    apply_ui(app, ui, vec![Message::ContextMenuChoose(index)]);
    assert!(app.context_menu.is_none(), "choosing closes the menu");
}

#[test]
fn a_right_press_in_the_editor_offers_cut_copy_paste_and_select_all() {
    let (mut app, mut ui) = editing();

    let messages = right_press(&mut app, &mut ui, Point::new(640.0, 400.0));
    assert!(
        has(&messages, |m| matches!(m, Message::EditorContextMenu)),
        "{messages:?}"
    );
    assert_eq!(labels(&app), ["Cut", "Copy", "Paste", "-", "Select All"]);
    assert_eq!(menu_line(&app, "Cut"), ("Cut".into(), Some("⌘X"), None));
    assert_eq!(menu_line(&app, "Copy"), ("Copy".into(), Some("⌘C"), None));
    assert_eq!(
        menu_line(&app, "Paste"),
        ("Paste".into(), Some("⌘V"), runs(Message::EditorMenuPaste))
    );
    assert_eq!(
        menu_line(&app, "Select All"),
        (
            "Select All".into(),
            Some("⌘A"),
            runs(Message::EditorAction(Action::SelectAll))
        )
    );
    assert_eq!(
        ui.focused(&app, EDITOR),
        Some(true),
        "the editor keeps focus"
    );
    let _ = app.update(Message::ContextMenuClose);

    // With a selection, Cut and Copy work, and the press keeps it.
    let _ = app.update(Message::EditorAction(Action::SelectWord));
    assert_eq!(editor_selection(&app).as_deref(), Some("Paragraph"));
    let _ = right_press(&mut app, &mut ui, Point::new(640.0, 400.0));
    assert_eq!(menu_line(&app, "Cut").2, runs(Message::EditorMenuCut));
    assert_eq!(menu_line(&app, "Copy").2, runs(Message::EditorMenuCopy));
    assert_eq!(editor_selection(&app).as_deref(), Some("Paragraph"));
    assert_eq!(ui.focused(&app, EDITOR), Some(true));

    // Select All selects the whole text.
    choose(&mut app, &mut ui, "Select All");
    assert_eq!(editor_selection(&app), Some(editor_text(&app)));
    assert_eq!(ui.focused(&app, EDITOR), Some(true));
}

#[test]
fn cut_and_paste_from_the_menu_edit_the_text_and_undo_restores_it() {
    let (mut app, mut ui) = editing();
    let original = editor_text(&app);
    let _ = app.update(Message::EditorAction(Action::SelectWord));

    let _ = right_press(&mut app, &mut ui, Point::new(640.0, 400.0));
    choose(&mut app, &mut ui, "Copy");
    assert_eq!(editor_text(&app), original, "Copy changes nothing");
    assert_eq!(editor_selection(&app).as_deref(), Some("Paragraph"));

    let _ = right_press(&mut app, &mut ui, Point::new(640.0, 400.0));
    choose(&mut app, &mut ui, "Cut");
    let cut = editor_text(&app);
    assert_eq!(cut, original.replacen("Paragraph", "", 1), "Cut deletes it");
    assert!(app.dirty);
    assert_eq!(ui.focused(&app, EDITOR), Some(true));
    let _ = app.update(Message::EditorUndo);
    assert_eq!(editor_text(&app), original, "undo brings it back");
    assert!(!app.dirty);

    // Paste inserts the clipboard text at the cursor; undo removes it.
    let _ = app.update(Message::EditorAction(Action::Move(
        iced::widget::text_editor::Motion::DocumentStart,
    )));
    let _ = app.update(Message::EditorPasted(Some("Hello ".into())));
    assert_eq!(editor_text(&app), format!("Hello {original}"));
    let _ = app.update(Message::EditorUndo);
    assert_eq!(editor_text(&app), original);
    let _ = app.update(Message::EditorPasted(None));
    assert_eq!(
        editor_text(&app),
        original,
        "an empty clipboard pastes nothing"
    );
}

#[test]
fn keys_typed_while_the_editor_menu_is_open_do_not_edit() {
    let (mut app, mut ui) = editing();
    let original = editor_text(&app);
    let _ = right_press(&mut app, &mut ui, Point::new(640.0, 400.0));
    assert!(app.context_menu.is_some());

    // The menu layer keeps the keys from the focused editor, and an editor
    // action that still arrives edits nothing while the menu is open.
    let typed = ui.type_char(&app, 'x');
    apply(&mut app, typed);
    let _ = app.update(Message::EditorAction(Action::Edit(Edit::Enter)));
    assert_eq!(editor_text(&app), original);
}

#[test]
fn esc_closes_the_editor_menu_and_the_editor_keeps_focus() {
    let (mut app, mut ui) = editing();
    let _ = right_press(&mut app, &mut ui, Point::new(640.0, 400.0));
    assert!(app.context_menu.is_some());

    let messages = ui
        .send(
            &app,
            Point::new(640.0, 400.0),
            vec![super::list_menus::escape()],
        )
        .0;
    apply(&mut app, messages);
    // What the key subscription sends for Esc.
    let _ = app.update(Message::ContextMenuClose);

    assert!(app.context_menu.is_none());
    assert_eq!(ui.focused(&app, EDITOR), Some(true), "typing still goes in");
}
