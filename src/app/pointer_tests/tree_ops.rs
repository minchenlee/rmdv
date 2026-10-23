//! File operations from the file tree's context menu (MDV-048 P6): New
//! File, New Folder, inline Rename, and Move to Trash. Each test works in
//! its own temporary workspace.

use super::group2::apply_ui;
use super::group3::{tree_fixture, tree_index};
use super::tree_menu::{menu_line, menu_lines, right_press, right_press_row, runs};
use super::*;

fn toast(app: &App) -> Option<&str> {
    app.toast.as_ref().map(|toast| toast.text.as_str())
}

fn has_row(app: &App, path: &Path) -> bool {
    let rows = tree::flatten_with_files(
        app.workspace_tree.as_ref().unwrap(),
        &app.workspace_sidebar_files,
        &app.expanded,
    );
    rows.iter().any(|row| row.node.path() == path)
}

/// Names in `dir`, sorted, as the file system reports them.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn rename_value(app: &App) -> Option<(&Path, &str)> {
    app.tree_rename
        .as_ref()
        .map(|rename| (rename.path.as_path(), rename.value.as_str()))
}

fn open(app: &mut App, path: &Path) {
    let source = std::fs::read_to_string(path).unwrap();
    let _ = app.update(Message::FileLoaded(Ok((path.to_path_buf(), source))));
    assert_eq!(app.file.as_deref(), Some(path));
}

/// Rename `from` to `to` through the messages the inline editor sends.
fn rename(app: &mut App, from: &Path, to: &str) {
    let _ = app.update(Message::TreeRenameStart(from.to_path_buf()));
    let _ = app.update(Message::TreeRenameInput(to.into()));
    let _ = app.update(Message::TreeRenameCommit);
}

/// Deletes the items this test moved to the Trash when it drops, also when
/// an assertion fails, so a failing run leaves nothing in the user's Trash.
#[derive(Default)]
struct TrashCleanup(Vec<std::path::PathBuf>);

impl TrashCleanup {
    /// The items moved to the Trash since the last call; they are deleted
    /// again on drop.
    fn take(&mut self) -> Vec<std::path::PathBuf> {
        let trashed = take_trashed_for_tests();
        self.0.extend(trashed.iter().cloned());
        trashed
    }
}

impl Drop for TrashCleanup {
    fn drop(&mut self) {
        self.take();
        for trashed in self.0.drain(..) {
            let _ = if trashed.is_dir() {
                std::fs::remove_dir_all(&trashed)
            } else {
                std::fs::remove_file(&trashed)
            };
        }
    }
}

// --- The menu ----------------------------------------------------------------

#[test]
fn the_tree_menus_offer_new_rename_and_trash() {
    let (mut app, dir) = tree_fixture("ops-menu");
    let beta = dir.join("beta.md");
    let sub = dir.join("sub");
    let mut ui = PointerUi::new();

    let _ = right_press_row(&mut app, &mut ui, "beta.md");
    let labels: Vec<String> = menu_lines(&app).into_iter().map(|line| line.0).collect();
    assert_eq!(
        labels[5..],
        ["-", "New File", "New Folder", "Rename", "Move to Trash"]
    );
    // A file's New File goes next to it.
    assert_eq!(
        menu_line(&app, "New File").2,
        runs(Message::TreeNewFile(dir.clone()))
    );
    assert_eq!(
        menu_line(&app, "New Folder").2,
        runs(Message::TreeNewFolder(dir.clone()))
    );
    assert_eq!(
        menu_line(&app, "Rename").2,
        runs(Message::TreeRenameStart(beta.clone()))
    );
    assert_eq!(
        menu_line(&app, "Move to Trash").2,
        runs(Message::TreeTrash(beta))
    );

    // A folder's New File goes inside it.
    let _ = app.update(Message::ContextMenuClose);
    let _ = right_press_row(&mut app, &mut ui, "sub");
    assert_eq!(
        menu_line(&app, "New File").2,
        runs(Message::TreeNewFile(sub.clone()))
    );
    assert_eq!(
        menu_line(&app, "Move to Trash").2,
        runs(Message::TreeTrash(sub))
    );

    // The workspace root can get new items but is not renamed or trashed.
    let _ = app.update(Message::ContextMenuClose);
    let last = ui.text_bounds(&app, "sub").unwrap();
    let _ = right_press(&mut app, &mut ui, Point::new(last.x, last.y + 160.0));
    assert_eq!(
        menu_line(&app, "New Folder").2,
        runs(Message::TreeNewFolder(dir.clone()))
    );
    assert_eq!(menu_line(&app, "Rename").2, None);
    assert_eq!(menu_line(&app, "Move to Trash").2, None);
    let _ = std::fs::remove_dir_all(&dir);
}

// --- New File and New Folder ---------------------------------------------------

#[test]
fn new_file_creates_untitled_and_renames_it_inline() {
    let (mut app, dir) = tree_fixture("ops-new-file");
    let mut ui = PointerUi::new();

    apply_ui(&mut app, &mut ui, vec![Message::TreeNewFile(dir.clone())]);

    let untitled = dir.join("Untitled.md");
    assert_eq!(std::fs::read_to_string(&untitled).ok().as_deref(), Some(""));
    assert!(has_row(&app, &untitled), "the tree shows the new file");
    assert_eq!(
        rename_value(&app),
        Some((untitled.as_path(), "Untitled.md")),
        "the new row is renamed inline"
    );
    assert_eq!(ui.focused(&app, TREE_RENAME_ID), Some(true));
    // The name without its extension is selected: typing replaces it.
    let typed = ui.type_char(&app, 'x');
    assert!(
        has(
            &typed,
            |m| matches!(m, Message::TreeRenameInput(value) if value == "x.md")
        ),
        "{typed:?}"
    );
    assert!(app.file.is_none(), "the new file does not open");

    // The next one gets the next free name.
    let _ = app.update(Message::TreeRenameCancel);
    let _ = app.update(Message::TreeNewFile(dir.clone()));
    assert!(dir.join("Untitled 2.md").is_file());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn new_folder_creates_untitled_folder_inside_and_opens_it() {
    let (mut app, dir) = tree_fixture("ops-new-folder");
    let sub = dir.join("sub");
    assert!(!app.expanded.contains(&sub));

    let _ = app.update(Message::TreeNewFolder(sub.clone()));

    let folder = sub.join("Untitled Folder");
    assert!(folder.is_dir());
    assert!(app.expanded.contains(&sub), "the parent opens to show it");
    assert!(has_row(&app, &folder));
    assert_eq!(
        rename_value(&app),
        Some((folder.as_path(), "Untitled Folder"))
    );
    let _ = app.update(Message::TreeRenameCancel);
    let _ = app.update(Message::TreeNewFolder(sub.clone()));
    assert!(sub.join("Untitled Folder 2").is_dir());
    let _ = app.update(Message::TreeRenameCancel);
    // The empty folders stay in the tree through later scans, so a file can
    // be made inside one.
    assert!(has_row(&app, &folder), "still shown after the next scan");
    let _ = app.update(Message::TreeNewFile(folder.clone()));
    assert!(has_row(&app, &folder.join("Untitled.md")));
    let _ = app.update(Message::TreeRenameCancel);
    rename(&mut app, &sub.join("Untitled Folder 2"), "Empty");
    assert!(
        has_row(&app, &sub.join("Empty")),
        "a renamed empty folder stays"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn new_items_stay_inside_the_workspace() {
    let (mut app, dir) = tree_fixture("ops-new-outside");
    let outside = std::env::temp_dir().join(format!("rmdv-tree-outside-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&outside);
    std::fs::create_dir_all(&outside).unwrap();

    let _ = app.update(Message::TreeNewFile(outside.clone()));
    let _ = app.update(Message::TreeNewFolder(outside.clone()));

    assert!(names(&outside).is_empty(), "nothing is created outside");
    assert!(app.tree_rename.is_none());
    let _ = std::fs::remove_dir_all(&outside);
    let _ = std::fs::remove_dir_all(&dir);
}

// --- Rename --------------------------------------------------------------------

#[test]
fn rename_moves_the_file_and_every_reference_to_it() {
    let (mut app, dir) = tree_fixture("ops-rename-file");
    let beta = dir.join("beta.md");
    let alpha = dir.join("alpha.md");
    let bravo = dir.join("Bravo.md");
    open(&mut app, &beta);
    let _ = app.update(Message::QuickSlotAssignPath(beta.clone()));
    assert_eq!(
        app.quick_slots
            .occupied(0)
            .map(|slot| slot.relative_path.as_str()),
        Some("beta.md")
    );
    open(&mut app, &alpha);
    assert!(app.history.back.iter().any(|entry| entry.path == beta));
    assert!(app.recent.paths.contains(&beta));
    app.tree_cursor = tree_index(&app, &beta);

    rename(&mut app, &beta, "Bravo.md");

    assert!(app.tree_rename.is_none(), "{:?}", toast(&app));
    assert!(bravo.is_file() && !beta.exists());
    assert_eq!(
        app.quick_slots
            .occupied(0)
            .map(|slot| slot.relative_path.as_str()),
        Some("Bravo.md"),
        "its Quick Slot"
    );
    assert!(
        app.history.back.iter().any(|entry| entry.path == bravo)
            && !app.history.back.iter().any(|entry| entry.path == beta),
        "Back returns to it by its new name"
    );
    assert!(app.recent.paths.contains(&bravo), "recent files");
    assert!(!app.recent.paths.contains(&beta));
    let stored = crate::recent::load_from(app.recent_store.as_deref().unwrap());
    assert!(stored.paths.contains(&bravo) && !stored.paths.contains(&beta));
    assert_eq!(
        app.tree_cursor,
        tree_index(&app, &bravo),
        "the tree cursor stays on it"
    );

    // The open document follows its file.
    rename(&mut app, &alpha, "Alpha 2.md");
    assert_eq!(app.file, Some(dir.join("Alpha 2.md")), "the open file");
    assert_eq!(app.error, None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn renaming_a_folder_moves_what_is_inside_it() {
    let (mut app, dir) = tree_fixture("ops-rename-folder");
    let sub = dir.join("sub");
    let gamma = sub.join("gamma.md");
    open(&mut app, &gamma);
    assert!(app.expanded.contains(&sub), "opening gamma reveals sub");
    let _ = app.update(Message::QuickSlotAssignPath(gamma.clone()));

    rename(&mut app, &sub, "Notes");

    let notes = dir.join("Notes");
    assert!(notes.join("gamma.md").is_file() && !sub.exists());
    assert_eq!(app.file, Some(notes.join("gamma.md")));
    assert!(app.expanded.contains(&notes) && !app.expanded.contains(&sub));
    assert_eq!(
        app.quick_slots
            .occupied(0)
            .map(|slot| slot.relative_path.as_str()),
        Some("Notes/gamma.md")
    );
    assert!(has_row(&app, &notes.join("gamma.md")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rename_refuses_bad_names_and_keeps_editing() {
    let (mut app, dir) = tree_fixture("ops-rename-invalid");
    let beta = dir.join("beta.md");
    for (name, says) in [
        ("", "empty"),
        ("   ", "empty"),
        ("a/b.md", "/"),
        ("a\0b.md", "NUL"),
        (".", "."),
        ("..", ".."),
        ("alpha.md", "already exists"),
    ] {
        rename(&mut app, &beta, name);
        assert!(beta.is_file(), "{name:?} leaves the file alone");
        assert_eq!(
            rename_value(&app).map(|(path, value)| (path.to_path_buf(), value.to_owned())),
            Some((beta.clone(), name.to_owned())),
            "{name:?} keeps the editor open to fix it"
        );
        let toast = toast(&app).unwrap_or_default();
        assert!(toast.contains(says), "{name:?}: {toast:?}");
        let _ = app.update(Message::TreeRenameCancel);
    }
    assert_eq!(names(&dir), ["alpha.md", "beta.md", "sub"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rename_can_change_only_the_case() {
    let (mut app, dir) = tree_fixture("ops-rename-case");
    let beta = dir.join("beta.md");
    open(&mut app, &beta);

    rename(&mut app, &beta, "Beta.md");

    assert!(app.tree_rename.is_none(), "{:?}", toast(&app));
    assert_eq!(names(&dir), ["Beta.md", "alpha.md", "sub"]);
    assert_eq!(app.file, Some(dir.join("Beta.md")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rename_with_the_same_name_or_escape_changes_nothing() {
    let (mut app, dir) = tree_fixture("ops-rename-same");
    let beta = dir.join("beta.md");
    rename(&mut app, &beta, "beta.md");
    assert!(app.tree_rename.is_none());
    assert!(beta.is_file());

    let _ = app.update(Message::TreeRenameStart(beta.clone()));
    let _ = app.update(Message::TreeRenameInput("gone.md".into()));
    assert!(matches!(
        tree_rename_key_message(&iced::keyboard::Key::Named(
            iced::keyboard::key::Named::Escape
        )),
        Message::TreeRenameCancel
    ));
    assert!(matches!(
        tree_rename_key_message(&iced::keyboard::Key::Named(
            iced::keyboard::key::Named::ArrowDown
        )),
        Message::Noop
    ));
    let _ = app.update(Message::TreeRenameCancel);
    assert!(app.tree_rename.is_none());
    assert_eq!(names(&dir), ["alpha.md", "beta.md", "sub"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_inline_rename_closes_on_a_press_outside_it() {
    let (mut app, dir) = tree_fixture("ops-rename-blur");
    let beta = dir.join("beta.md");
    let mut ui = PointerUi::new();
    apply_ui(
        &mut app,
        &mut ui,
        vec![Message::TreeRenameStart(beta.clone())],
    );
    let input = ui
        .id_bounds(&app, TREE_RENAME_ID)
        .expect("the rename field shows in the row");
    assert!(input.x < app.sidebar_width);

    // A press inside the field edits it.
    let inside = ui.click(&app, center(input));
    assert!(
        !has(&inside, |m| matches!(m, Message::TreeRenameCancel)),
        "{inside:?}"
    );
    // A press anywhere else cancels.
    let outside = ui.click(&app, Point::new(700.0, 400.0));
    assert!(
        has(&outside, |m| matches!(m, Message::TreeRenameCancel)),
        "{outside:?}"
    );
    apply(&mut app, outside);
    assert!(app.tree_rename.is_none());
    assert!(beta.is_file());

    // Hiding the sidebar ends the rename too.
    let _ = app.update(Message::TreeRenameStart(beta.clone()));
    let _ = app.update(Message::ToggleSidebar);
    assert!(app.tree_rename.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_workspace_root_is_not_renamed() {
    let (mut app, dir) = tree_fixture("ops-rename-root");
    let _ = app.update(Message::TreeRenameStart(dir.clone()));
    assert!(app.tree_rename.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

// --- Move to Trash ---------------------------------------------------------------

#[cfg(target_os = "macos")]
#[test]
fn move_to_trash_moves_the_file_and_says_so() {
    let mut cleanup = TrashCleanup::default();
    let (mut app, dir) = tree_fixture("ops-trash-file");
    let beta = dir.join("beta.md");

    let _ = app.update(Message::TreeTrash(beta.clone()));
    let trashed = cleanup.take();

    assert!(!beta.exists());
    assert_eq!(trashed.len(), 1, "the system Trash received it");
    assert!(trashed[0].exists(), "{trashed:?}");
    assert_eq!(toast(&app), Some("Moved beta.md to Trash"));
    assert!(!has_row(&app, &beta));
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(target_os = "macos")]
#[test]
fn trashing_the_open_document_shows_the_welcome_page() {
    let _cleanup = TrashCleanup::default();
    let (mut app, dir) = tree_fixture("ops-trash-open");
    let sub = dir.join("sub");
    let gamma = sub.join("gamma.md");
    open(&mut app, &gamma);

    let _ = app.update(Message::TreeTrash(sub.clone()));

    assert!(!sub.exists());
    assert!(app.file.is_none(), "the open document closes");
    assert!(app.source.is_empty());
    assert!(app.error.is_none());
    let mut ui = PointerUi::new();
    assert!(
        ui.text_containing(&app, "native markdown viewer").is_some(),
        "the welcome page shows"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(not(target_os = "macos"))]
#[test]
fn move_to_trash_says_it_is_not_supported_here() {
    let (mut app, dir) = tree_fixture("ops-trash-other");
    let beta = dir.join("beta.md");
    let _ = app.update(Message::TreeTrash(beta.clone()));
    assert!(beta.is_file());
    assert!(toast(&app).unwrap_or_default().contains("not supported"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn move_to_trash_refuses_unsaved_edits_and_the_root() {
    let mut cleanup = TrashCleanup::default();
    let (mut app, dir) = tree_fixture("ops-trash-dirty");
    let sub = dir.join("sub");
    let gamma = sub.join("gamma.md");
    open(&mut app, &gamma);
    app.source.push_str("unsaved\n");
    app.dirty = true;

    for target in [gamma.clone(), sub.clone()] {
        let _ = app.update(Message::TreeTrash(target.clone()));
        assert!(gamma.is_file(), "{target:?} with unsaved edits stays");
        assert!(
            toast(&app).unwrap_or_default().contains("unsaved edits"),
            "{:?}",
            toast(&app)
        );
    }
    assert_eq!(app.file.as_deref(), Some(gamma.as_path()));

    let _ = app.update(Message::TreeTrash(dir.clone()));
    assert!(dir.is_dir(), "the workspace root is never trashed");
    assert!(cleanup.take().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

// --- Review fixes ------------------------------------------------------------------

/// Choose the open menu's item labelled `label`, as a click on it does.
fn choose(app: &mut App, ui: &mut PointerUi, label: &str) {
    let index = menu_lines(app)
        .iter()
        .position(|(line, ..)| line == label)
        .unwrap_or_else(|| panic!("no {label:?} in {:?}", menu_lines(app)));
    apply_ui(app, ui, vec![Message::ContextMenuChoose(index)]);
    assert!(app.context_menu.is_none(), "choosing closes the menu");
}

/// Hide the sidebar and show its peek from the left window edge.
fn peek(app: &mut App, ui: &mut PointerUi) {
    if app.sidebar_open {
        let _ = app.update(Message::ToggleSidebar);
    }
    // `right_press` sends no release; a held button keeps the peek away.
    let _ = ui.send(
        app,
        Point::new(900.0, 400.0),
        vec![Event::Mouse(mouse::Event::ButtonReleased(
            mouse::Button::Right,
        ))],
    );
    let _ = ui.move_to(app, Point::new(900.0, 400.0));
    let messages = ui.move_to(app, Point::new(2.0, 400.0)).0;
    apply(app, messages);
    assert!(app.sidebar_peek && !app.sidebar_open, "the tree only peeks");
}

#[test]
fn rename_and_new_file_from_the_peeking_tree_pin_the_sidebar_open() {
    let (mut app, dir) = tree_fixture("ops-peek");
    let beta = dir.join("beta.md");
    let mut ui = PointerUi::new();

    peek(&mut app, &mut ui);
    let _ = right_press_row(&mut app, &mut ui, "beta.md");
    choose(&mut app, &mut ui, "Rename");
    assert_eq!(
        rename_value(&app),
        Some((beta.as_path(), "beta.md")),
        "the rename survives the update that starts it"
    );
    assert!(app.sidebar_open, "pinned open, as Keep Sidebar Open does");
    assert_eq!(ui.focused(&app, TREE_RENAME_ID), Some(true));
    let _ = app.update(Message::TreeRenameCancel);

    peek(&mut app, &mut ui);
    let _ = right_press_row(&mut app, &mut ui, "alpha.md");
    choose(&mut app, &mut ui, "New File");
    let untitled = dir.join("Untitled.md");
    assert!(untitled.is_file());
    assert_eq!(
        rename_value(&app),
        Some((untitled.as_path(), "Untitled.md"))
    );
    assert!(app.sidebar_open);
    assert_eq!(ui.focused(&app, TREE_RENAME_ID), Some(true));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_rename_needs_a_row_to_show_its_field() {
    let (mut app, dir) = tree_fixture("ops-no-row");
    let gamma = dir.join("sub").join("gamma.md");
    assert!(!has_row(&app, &gamma), "sub is collapsed");
    let _ = app.update(Message::TreeRenameStart(gamma.clone()));
    assert_eq!(
        rename_value(&app),
        None,
        "no field, so no rename holds the keys"
    );

    // A new file deeper than the tree shows is made; a toast says where.
    let mut deep = dir.clone();
    for level in 0..=tree::WORKSPACE_TREE_MAX_DEPTH {
        deep.push(format!("d{level}"));
    }
    std::fs::create_dir_all(&deep).unwrap();
    let _ = app.update(Message::TreeNewFile(deep.clone()));
    let untitled = deep.join("Untitled.md");
    assert!(untitled.is_file());
    assert!(!has_row(&app, &untitled), "the tree does not show it");
    assert_eq!(rename_value(&app), None);
    assert!(
        toast(&app)
            .unwrap_or_default()
            .contains("Created Untitled.md"),
        "{:?}",
        toast(&app)
    );

    // A rename whose row goes away in a rescan ends.
    let beta = dir.join("beta.md");
    let _ = app.update(Message::TreeRenameStart(beta.clone()));
    assert!(rename_value(&app).is_some());
    std::fs::remove_file(&beta).unwrap();
    let snapshot = tree::build_workspace(&dir, false).unwrap();
    app.apply_refreshed_workspace_snapshot(dir.clone(), snapshot);
    assert_eq!(rename_value(&app), None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn operations_refuse_paths_through_a_symlinked_folder() {
    let (mut app, dir) = tree_fixture("ops-symlink");
    let outside =
        std::env::temp_dir().join(format!("rmdv-tree-link-target-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&outside);
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("x.md"), "# X\n").unwrap();
    let link = dir.join("link");
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    let x = link.join("x.md");

    let mut cleanup = TrashCleanup::default();
    let _ = app.update(Message::TreeTrash(x.clone()));
    let trashed = cleanup.take();
    assert!(trashed.is_empty(), "nothing goes to the Trash: {trashed:?}");
    assert!(
        toast(&app)
            .unwrap_or_default()
            .contains("outside this workspace"),
        "{:?}",
        toast(&app)
    );
    let _ = app.update(Message::TreeNewFile(link.clone()));
    let _ = app.update(Message::TreeNewFolder(link.clone()));
    assert!(app.tree_rename.is_none());
    // A rename field already open on such a path commits nothing either.
    app.tree_rename = Some(TreeRename {
        path: x.clone(),
        value: "y.md".into(),
    });
    let _ = app.update(Message::TreeRenameCommit);
    assert_eq!(names(&outside), ["x.md"], "nothing changes outside");

    // The link itself is an entry in the workspace: it can be renamed.
    let _ = app.update(Message::TreeRenameCancel);
    let snapshot = tree::build_workspace(&dir, false).unwrap();
    app.apply_refreshed_workspace_snapshot(dir.clone(), snapshot);
    rename(&mut app, &link, "renamed-link");
    assert!(dir.join("renamed-link").join("x.md").is_file());
    assert_eq!(names(&outside), ["x.md"]);
    let _ = std::fs::remove_dir_all(&outside);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_close_confirmation_ends_an_inline_rename() {
    let (mut app, dir) = tree_fixture("ops-close");
    open(&mut app, &dir.join("alpha.md"));
    app.dirty = true;
    let _ = app.update(Message::TreeRenameStart(dir.join("beta.md")));
    assert!(rename_value(&app).is_some());

    let _ = app.update(Message::WindowCloseRequested(iced::window::Id::unique()));

    assert!(app.close_confirm.is_some());
    assert_eq!(rename_value(&app), None, "the confirmation owns the keys");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_rename_field_under_a_menu_takes_no_typing_or_enter() {
    let (mut app, dir) = tree_fixture("ops-rename-menu");
    let beta = dir.join("beta.md");
    let _ = app.update(Message::TreeRenameStart(beta.clone()));
    let _ = app.update(Message::PointerRightPress(Point::new(40.0, 80.0)));
    app.open_context_menu(vec![MenuEntry::item("Copy", Message::Noop)], None);

    let _ = app.update(Message::TreeRenameInput("zz.md".into()));
    let _ = app.update(Message::TreeRenameCommit);

    assert_eq!(rename_value(&app), Some((beta.as_path(), "beta.md")));
    assert!(beta.is_file());
    assert!(app.context_menu.is_some());
    let _ = std::fs::remove_dir_all(&dir);
}
