//! The file tree's context menu (MDV-048 P6): a right press on a file, a
//! folder, or the empty tree space opens a menu for that item. The menu
//! itself (placement, closing, keys) is tested in `context_menu`.

use super::group3::{tree_fixture, tree_index};
use super::*;

/// A right press at `at` through the view; the messages it publishes are
/// applied.
pub(super) fn right_press(app: &mut App, ui: &mut PointerUi, at: Point) -> Vec<Message> {
    let _ = ui.move_to(app, at);
    let messages = ui
        .send(
            app,
            at,
            vec![Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Right,
            ))],
        )
        .0;
    apply(app, messages.clone());
    messages
}

/// Right press the tree row that shows `name`.
pub(super) fn right_press_row(app: &mut App, ui: &mut PointerUi, name: &str) -> Vec<Message> {
    let row = ui.text_bounds(app, name).expect("the tree row shows");
    assert!(row.x < app.sidebar_width, "{name} is a sidebar row");
    right_press(app, ui, center(row))
}

/// The open menu as (label, shortcut, message) lines, each message in its
/// `Debug` form (`Message` has no `PartialEq`); separators are
/// `("-", None, None)`.
pub(super) fn menu_lines(app: &App) -> Vec<(String, Option<&'static str>, Option<String>)> {
    app.context_menu
        .as_ref()
        .expect("a context menu is open")
        .entries
        .iter()
        .map(|entry| match entry {
            MenuEntry::Item {
                label,
                shortcut,
                message,
            } => (
                label.clone(),
                *shortcut,
                message.as_ref().map(|m| format!("{m:?}")),
            ),
            MenuEntry::Separator => ("-".to_owned(), None, None),
        })
        .collect()
}

pub(super) fn menu_line(app: &App, label: &str) -> (String, Option<&'static str>, Option<String>) {
    menu_lines(app)
        .into_iter()
        .find(|(line, _, _)| line == label)
        .unwrap_or_else(|| panic!("no {label:?} in {:?}", menu_lines(app)))
}

/// A message in the form `menu_lines` reports it.
pub(super) fn runs(message: Message) -> Option<String> {
    Some(format!("{message:?}"))
}

fn labels(app: &App) -> Vec<String> {
    menu_lines(app)
        .into_iter()
        .map(|(label, ..)| label)
        .collect()
}

fn text_of(path: &Path) -> String {
    path.to_str().expect("utf-8 path").to_owned()
}

#[test]
fn right_clicking_a_file_row_opens_its_menu_without_moving_the_cursor() {
    let (mut app, dir) = tree_fixture("menu-file");
    let beta = dir.join("beta.md");
    let alpha = dir.join("alpha.md");
    app.tree_cursor = tree_index(&app, &alpha);
    let mut ui = PointerUi::new();

    let messages = right_press_row(&mut app, &mut ui, "beta.md");

    assert!(
        has(&messages, |m| matches!(m, Message::PointerRightPress(_))),
        "{messages:?}"
    );
    assert!(
        has(&messages, |m| matches!(
            m,
            Message::TreeContextMenu(Some((path, false))) if *path == beta
        )),
        "the row asks for its menu: {messages:?}"
    );
    let menu = app.context_menu.as_ref().expect("the menu opens");
    assert_eq!(menu.tree_target.as_deref(), Some(beta.as_path()));
    assert_eq!(
        app.tree_cursor,
        tree_index(&app, &alpha),
        "a right press does not move the tree cursor"
    );
    assert!(app.file.is_none(), "a right press opens nothing");
    assert_eq!(
        labels(&app)[..5],
        [
            "Open",
            "Reveal in Finder",
            "Copy Path",
            "Copy Relative Path",
            "Add to Quick Slot",
        ]
    );
    assert_eq!(menu_line(&app, "Open").2, runs(Message::Open(beta.clone())));
    assert_eq!(
        menu_line(&app, "Reveal in Finder").2,
        runs(Message::RevealPath(beta.clone()))
    );
    assert_eq!(
        menu_line(&app, "Copy Path").2,
        runs(Message::CopyPath(text_of(&beta)))
    );
    assert_eq!(
        menu_line(&app, "Copy Relative Path").2,
        runs(Message::CopyPath("beta.md".into()))
    );
    assert_eq!(
        menu_line(&app, "Add to Quick Slot").2,
        runs(Message::QuickSlotAssignPath(beta.clone()))
    );
    // Beta is not the focused file: ⌘⌥R / ⌘⌥C would act on something else.
    assert_eq!(menu_line(&app, "Reveal in Finder").1, None);
    assert_eq!(menu_line(&app, "Copy Path").1, None);

    // Choosing Open runs it and closes the menu.
    let index = labels(&app).iter().position(|l| l == "Open").unwrap();
    let task = app.update(Message::ContextMenuChoose(index));
    assert!(app.context_menu.is_none());
    let outputs = ready_outputs(task);
    assert!(
        matches!(outputs.as_slice(), [Message::Open(path)] if *path == beta),
        "{outputs:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_focused_file_row_menu_shows_the_reveal_and_copy_shortcuts() {
    let (mut app, dir) = tree_fixture("menu-focused");
    let alpha = dir.join("alpha.md");
    let _ = app.update(Message::FileLoaded(Ok((alpha.clone(), "# Alpha\n".into()))));
    assert_eq!(app.focused_file_path(), Some(alpha));
    let mut ui = PointerUi::new();

    let _ = right_press_row(&mut app, &mut ui, "alpha.md");

    assert_eq!(menu_line(&app, "Reveal in Finder").1, Some("⌘⌥R"));
    assert_eq!(menu_line(&app, "Copy Path").1, Some("⌘⌥C"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn right_clicking_a_folder_row_offers_folder_actions() {
    let (mut app, dir) = tree_fixture("menu-folder");
    let sub = dir.join("sub");
    let mut ui = PointerUi::new();

    let messages = right_press_row(&mut app, &mut ui, "sub");

    assert!(
        has(&messages, |m| matches!(
            m,
            Message::TreeContextMenu(Some((path, true))) if *path == sub
        )),
        "{messages:?}"
    );
    assert!(
        !app.expanded.contains(&sub),
        "a right press does not toggle"
    );
    let labels = labels(&app);
    assert!(!labels.iter().any(|l| l == "Open"), "{labels:?}");
    assert!(
        !labels.iter().any(|l| l == "Add to Quick Slot"),
        "{labels:?}"
    );
    assert_eq!(
        menu_line(&app, "Reveal in Finder").2,
        runs(Message::RevealPath(sub.clone()))
    );
    assert_eq!(
        menu_line(&app, "Copy Relative Path").2,
        runs(Message::CopyPath("sub".into()))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn right_clicking_empty_tree_space_opens_the_workspace_menu() {
    let (mut app, dir) = tree_fixture("menu-root");
    let mut ui = PointerUi::new();
    let last = ui.text_bounds(&app, "sub").expect("the last row");
    let below = Point::new(last.x, last.y + 160.0);

    let messages = right_press(&mut app, &mut ui, below);

    assert!(
        has(&messages, |m| matches!(m, Message::TreeContextMenu(None))),
        "the empty space asks for the workspace menu: {messages:?}"
    );
    let menu = app.context_menu.as_ref().expect("the menu opens");
    assert_eq!(menu.tree_target, None);
    assert_eq!(
        menu_line(&app, "Reveal in Finder").2,
        runs(Message::RevealPath(dir.clone()))
    );
    assert_eq!(
        menu_line(&app, "Copy Path").2,
        runs(Message::CopyPath(text_of(&dir)))
    );
    assert_eq!(
        menu_line(&app, "Copy Relative Path").2,
        None,
        "the root has no relative path"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn copy_path_copies_the_text_and_says_so() {
    let (mut app, dir) = tree_fixture("menu-copy");
    let _ = app.update(Message::CopyPath("sub/gamma.md".into()));
    assert_eq!(
        app.toast.as_ref().map(|toast| toast.text.as_str()),
        Some("Path copied")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn slot(app: &App, index: usize) -> Option<&str> {
    app.quick_slots
        .occupied(index)
        .map(|slot| slot.relative_path.as_str())
}

fn occupy(app: &mut App, index: usize, relative_path: &str) {
    assert!(app.quick_slots.set(
        index,
        crate::quick_slots::QuickSlot {
            relative_path: relative_path.into(),
            context: crate::quick_slots::SlotContext::default(),
        },
    ));
}

#[test]
fn add_to_quick_slot_takes_the_first_free_slot() {
    let (mut app, dir) = tree_fixture("menu-slot-free");
    occupy(&mut app, 0, "alpha.md");

    let _ = app.update(Message::QuickSlotAssignPath(
        dir.join("sub").join("gamma.md"),
    ));

    assert_eq!(slot(&app, 1), Some("sub/gamma.md"));
    assert_eq!(slot(&app, 0), Some("alpha.md"));
    assert!(app.file.is_none(), "the file does not open");
    assert_eq!(app.quick_slots.active, None, "no open file, no active slot");
    assert_eq!(
        app.toast.as_ref().map(|toast| toast.text.as_str()),
        Some("Quick Slot 2 assigned")
    );

    // A file already in a slot stays where it is.
    let _ = app.update(Message::QuickSlotAssignPath(dir.join("alpha.md")));
    assert_eq!(slot(&app, 0), Some("alpha.md"));
    assert_eq!(slot(&app, 2), None);
    assert_eq!(
        app.toast.as_ref().map(|toast| toast.text.as_str()),
        Some("alpha.md is already in Quick Slot 1")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn add_to_quick_slot_with_every_slot_full_asks_to_replace_the_last() {
    let (mut app, dir) = tree_fixture("menu-slot-full");
    for index in 0..crate::quick_slots::SLOT_COUNT {
        occupy(&mut app, index, &format!("other-{index}.md"));
    }
    let beta = dir.join("beta.md");

    let _ = app.update(Message::QuickSlotAssignPath(beta.clone()));

    let last = crate::quick_slots::SLOT_COUNT - 1;
    let pending = app
        .quick_slot_replace
        .clone()
        .expect("a full bank asks before replacing");
    assert_eq!(pending.index, last);
    assert_eq!(pending.incoming, "beta.md");
    assert_eq!(slot(&app, last), Some("other-8.md"));

    let _ = app.update(Message::QuickSlotReplaceConfirm);
    assert_eq!(slot(&app, last), Some("beta.md"));
    assert!(app.file.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn add_to_quick_slot_for_the_open_file_keeps_its_place_and_activates() {
    let (mut app, dir) = tree_fixture("menu-slot-open");
    let alpha = dir.join("alpha.md");
    let _ = app.update(Message::FileLoaded(Ok((alpha.clone(), "# Alpha\n".into()))));

    let _ = app.update(Message::QuickSlotAssignPath(alpha));

    assert_eq!(slot(&app, 0), Some("alpha.md"));
    assert_eq!(app.quick_slots.active, Some(0), "the open file's slot");
    let _ = std::fs::remove_dir_all(&dir);
}
