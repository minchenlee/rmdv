//! Context menus on outline rows, vault search hits, and file finder rows
//! (MDV-048 P6). The Command Palette lists commands only: its rows have no
//! menu.

use super::group3::tree_fixture;
use super::tree_menu::{menu_line, menu_lines, right_press, right_press_row, runs};
use super::*;

fn labels(app: &App) -> Vec<String> {
    menu_lines(app)
        .into_iter()
        .map(|(label, ..)| label)
        .collect()
}

#[test]
fn an_outline_row_offers_go_to_and_copy_items() {
    let (mut app, dir) = tree_fixture("outline-menu");
    let _ = app.update(Message::FileLoaded(Ok((
        dir.join("alpha.md"),
        "# Outline One\n\ntext\n\n## Outline Two\n\ntext\n\n## Outline Three\n\ntext\n".into(),
    ))));
    app.sidebar_tab = SidebarTab::Outline;
    assert_eq!(app.outline_cursor, 0);
    let mut ui = PointerUi::new();

    let messages = right_press_row(&mut app, &mut ui, "Outline Three");

    assert!(
        has(&messages, |m| matches!(m, Message::OutlineContextMenu(2))),
        "the row asks for its menu: {messages:?}"
    );
    assert_eq!(
        labels(&app),
        ["Go to Heading", "Copy Heading Text", "Copy Link to Heading"]
    );
    let line = app.outline_sections[2].line;
    assert_eq!(
        menu_line(&app, "Go to Heading").2,
        runs(Message::OutlineRowClicked { index: 2, line }),
        "the same as a click"
    );
    assert_eq!(
        menu_line(&app, "Copy Heading Text").2,
        runs(Message::CopyCode("Outline Three".into()))
    );
    assert_eq!(
        menu_line(&app, "Copy Link to Heading").2,
        runs(Message::CopyCode("alpha.md#outline-three".into()))
    );
    assert_eq!(app.outline_cursor, 0, "a right press moves no cursor");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A file finder over `alpha.md`, `beta.md` and `sub/gamma.md`.
fn finder(tag: &str) -> (App, PathBuf) {
    let (mut app, dir) = tree_fixture(tag);
    app.workspace_files = ["alpha.md", "beta.md", "sub/gamma.md"]
        .iter()
        .map(|name| dir.join(name))
        .collect();
    app.workspace_files_rev += 1;
    // The file tree shows the same names; keep it out of the way.
    app.sidebar_open = false;
    let _ = app.update(Message::OpenFileFinder);
    assert_eq!(app.overlay, Overlay::FileFinder);
    (app, dir)
}

#[test]
fn a_file_finder_row_offers_open_reveal_and_copy_path() {
    let (mut app, dir) = finder("finder-menu");
    let beta = dir.join("beta.md");
    let selected = app.overlay_selected;
    let mut ui = PointerUi::new();
    let row = ui.text_bounds(&app, "beta.md").expect("finder row");

    let messages = right_press(&mut app, &mut ui, center(row));

    assert!(
        has(&messages, |m| matches!(
            m,
            Message::FileFinderContextMenu(path) if *path == beta
        )),
        "the row asks for its menu: {messages:?}"
    );
    assert_eq!(labels(&app), ["Open", "Reveal in Finder", "Copy Path"]);
    assert_eq!(
        menu_line(&app, "Open").2,
        runs(Message::OpenFileFinderPath(beta.clone())),
        "the same as a click"
    );
    assert_eq!(
        menu_line(&app, "Reveal in Finder").2,
        runs(Message::RevealPath(beta.clone()))
    );
    assert_eq!(
        menu_line(&app, "Copy Path").2,
        runs(Message::CopyPath(beta.to_str().unwrap().to_owned()))
    );
    assert_eq!(app.overlay, Overlay::FileFinder, "the finder stays open");
    assert_eq!(app.overlay_selected, selected, "the selection stays");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_query_field_under_a_menu_takes_no_typing_or_enter() {
    let (mut app, dir) = finder("finder-menu-keys");
    let mut ui = PointerUi::new();
    let row = ui.text_bounds(&app, "beta.md").expect("finder row");
    let _ = right_press(&mut app, &mut ui, center(row));
    assert!(app.context_menu.is_some());

    // The menu owns the keys; a query message that still arrives does
    // nothing.
    let _ = app.update(Message::OverlayQueryChanged("gam".into()));
    assert_eq!(app.overlay_query, "", "typing goes to the menu");
    let _ = app.update(Message::OverlayConfirm);
    assert_eq!(app.overlay, Overlay::FileFinder, "Enter opens nothing");
    assert!(app.file.is_none());
    assert!(app.context_menu.is_some(), "the menu stays open");
    let _ = std::fs::remove_dir_all(&dir);

    // The vault page's field too.
    let mut app = App::default();
    app.workspace = Some(PathBuf::from("/w"));
    let _ = app.update(Message::OpenVaultSearch);
    let _ = app.update(Message::PointerRightPress(Point::new(10.0, 10.0)));
    app.open_context_menu(vec![MenuEntry::item("Copy", Message::Noop)], None);
    let _ = app.update(Message::VaultQueryChanged("needle".into()));
    assert_eq!(app.vault_query, "");
    let _ = app.update(Message::VaultEnter);
    assert!(app.vault_open);
    assert!(app.context_menu.is_some());
}

#[test]
fn a_vault_hit_offers_open_reveal_and_copy_path() {
    let (mut app, dir) = tree_fixture("vault-menu");
    let beta = dir.join("beta.md");
    let _ = app.update(Message::OpenVaultSearch);
    app.vault_query = "needle".into();
    app.vault_searched_query = Some("needle".into());
    for line in [3, 9] {
        app.vault_results.push(crate::vault_search::VaultHit {
            path: beta.clone(),
            line,
            col_start: 0,
            col_end: 6,
            context: vec![crate::vault_search::ContextLine {
                number: line,
                text: format!("needle on line {line}"),
                is_match: true,
                spans: Vec::new(),
            }],
        });
    }
    app.vault_file_count = 1;
    let cursor = app.vault_cursor;
    let mut ui = PointerUi::new();
    let row = ui
        .find(
            &app,
            Find {
                text: None,
                // The line-number gutter of the second hit.
                contains: Some("    9 "),
                id: None,
                found: None,
                found_text: None,
            },
        )
        .map(|(bounds, _)| bounds)
        .expect("the second hit shows");

    let messages = right_press(&mut app, &mut ui, center(row));

    assert!(
        has(&messages, |m| matches!(m, Message::VaultHitContextMenu(1))),
        "the hit asks for its menu: {messages:?}"
    );
    assert_eq!(labels(&app), ["Open", "Reveal in Finder", "Copy Path"]);
    assert_eq!(
        menu_line(&app, "Open").2,
        runs(Message::VaultOpenHitAt {
            path: beta.clone(),
            line: 9
        }),
        "the same as a click on it"
    );
    assert_eq!(
        menu_line(&app, "Reveal in Finder").2,
        runs(Message::RevealPath(beta.clone()))
    );
    assert_eq!(
        menu_line(&app, "Copy Path").2,
        runs(Message::CopyPath(beta.to_str().unwrap().to_owned()))
    );
    assert!(app.vault_open);
    assert_eq!(app.vault_cursor, cursor, "a right press moves no cursor");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_command_palette_row_has_no_menu() {
    let mut app = App::default();
    let _ = app.update(Message::OpenCommandPalette);
    let mut ui = PointerUi::new();
    let row = ui
        .text_bounds(&app, "Toggle Sidebar  ⌘B")
        .expect("palette row");
    let _ = right_press(&mut app, &mut ui, center(row));
    assert!(app.context_menu.is_none());
    assert_eq!(app.overlay, Overlay::Command, "the palette stays open");
}

#[test]
fn a_vault_hit_menu_opens_the_hit_it_was_opened_on() {
    let (mut app, dir) = tree_fixture("vault-menu-stale");
    let (alpha, beta) = (dir.join("alpha.md"), dir.join("beta.md"));
    let _ = app.update(Message::OpenVaultSearch);
    let hit = |path: &Path, line: u32| crate::vault_search::VaultHit {
        path: path.to_path_buf(),
        line,
        col_start: 0,
        col_end: 6,
        context: Vec::new(),
    };
    app.vault_results = vec![hit(&alpha, 2), hit(&beta, 9)];
    let _ = app.update(Message::PointerRightPress(Point::new(400.0, 300.0)));
    let _ = app.update(Message::VaultHitContextMenu(1));
    assert!(app.context_menu.is_some());

    // A new search lands while the menu is open.
    app.vault_results = vec![hit(&alpha, 2)];
    let follow_ups = ready_outputs(app.update(Message::ContextMenuChoose(0)));
    let mut opened = Vec::new();
    for message in follow_ups {
        opened.extend(ready_outputs(app.update(message)));
    }

    assert!(
        has(
            &opened,
            |m| matches!(m, Message::Open(path) if *path == beta)
        ),
        "Open opens the clicked hit: {opened:?}"
    );
    assert_eq!(app.pending_nav.as_ref().and_then(|nav| nav.line), Some(9));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Esc, as the keyboard reports it.
pub(super) fn escape() -> Event {
    Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
        modified_key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
        physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Escape),
        location: iced::keyboard::Location::Standard,
        modifiers: iced::keyboard::Modifiers::default(),
        text: None,
        repeat: false,
    })
}

#[test]
fn the_find_field_under_a_menu_takes_no_typing_and_keeps_focus() {
    const FIND: &str = "search-input";
    let mut app = long_doc_app();
    let mut ui = PointerUi::new();
    super::group2::apply_ui(&mut app, &mut ui, vec![Message::ToggleSearch]);
    assert_eq!(ui.focused(&app, FIND), Some(true));
    let _ = right_press(&mut app, &mut ui, Point::new(640.0, 400.0));
    assert!(app.context_menu.is_some(), "the reader menu opens");

    let typed = ui.type_char(&app, 'x');
    assert!(
        !has(&typed, |m| matches!(m, Message::QueryChanged(_))),
        "the keys belong to the menu: {typed:?}"
    );
    let _ = app.update(Message::QueryChanged("x".into()));
    assert_eq!(app.query, "", "a query change under the menu does nothing");

    // Esc closes the menu (the key subscription sends the close); the field
    // underneath keeps its focus.
    let messages = ui.send(&app, Point::new(640.0, 400.0), vec![escape()]).0;
    apply(&mut app, messages);
    let _ = app.update(Message::ContextMenuClose);
    assert!(app.context_menu.is_none());
    assert_eq!(ui.focused(&app, FIND), Some(true));
}
