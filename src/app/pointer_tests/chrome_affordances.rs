//! Chrome affordances (MDV-048 P7): the status footer pill names what a
//! click does and cycles the statistic it shows (saved in prefs); the
//! shortcut sheet has a close button, runs a row's action on click, and
//! lists the shortcuts the key map really has; the Command Palette draws
//! shortcuts as right-aligned key caps that typing does not match; and the
//! palette, finder, and vault query fields get a clear button.

use super::group2::apply_ui;
use super::group3::{assert_tips, tips, tree_fixture};
use super::*;
use crate::prefs::FooterStat;

fn prefs_path(app: &App) -> PathBuf {
    app.quick_slots_persistence_path
        .clone()
        .expect("the test prefs path")
}

fn short_doc_app() -> App {
    let mut app = App::default();
    let _ = app.update(Message::FileLoaded(Ok((
        PathBuf::from("/tmp/pointer-footer.md"),
        "one two three four five\n\nsix seven\n".to_string(),
    ))));
    assert!(app.show_footer);
    app
}

// --- Footer pill -----------------------------------------------------------

#[test]
fn the_footer_pill_click_cycles_the_statistic_and_saves_it() {
    let mut app = short_doc_app();
    let path = prefs_path(&app);
    let _ = std::fs::remove_file(&path);
    let mut ui = PointerUi::new();
    assert_eq!(app.prefs.footer_stat, FooterStat::WordsAndTime);
    assert!(ui.text_bounds(&app, "7 words · 1 min read").is_some());
    assert_tips(&tips(&mut ui, &app), &["Show Next Statistic"]);

    let characters = format!("{} characters", app.source.chars().count());
    let expected = [
        (FooterStat::Words, "7 words"),
        (FooterStat::ReadingTime, "1 min read"),
        (FooterStat::Characters, characters.as_str()),
        (FooterStat::WordsAndTime, "7 words · 1 min read"),
    ];
    for (stat, label) in expected {
        let pill = ui
            .id_bounds(&app, "status-footer")
            .expect("the footer pill");
        let messages = ui.click(&app, center(pill));
        apply(&mut app, messages);
        assert_eq!(app.prefs.footer_stat, stat);
        assert!(ui.text_bounds(&app, label).is_some(), "shows {label:?}");
        assert_eq!(
            crate::prefs::load_from(&path).footer_stat,
            stat,
            "the choice is saved"
        );
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_footer_statistic_loads_leniently() {
    let load = |json: &str| -> FooterStat {
        serde_json::from_str::<crate::prefs::Prefs>(json)
            .expect("prefs parse")
            .footer_stat
    };
    assert_eq!(load("{}"), FooterStat::WordsAndTime);
    assert_eq!(
        load(r#"{"footer_stat":"characters"}"#),
        FooterStat::Characters
    );
    assert_eq!(load(r#"{"footer_stat":"lines"}"#), FooterStat::WordsAndTime);
    let prefs: crate::prefs::Prefs =
        serde_json::from_str(r#"{"footer_stat":7,"show_footer":false}"#).unwrap();
    assert_eq!(prefs.footer_stat, FooterStat::WordsAndTime);
    assert!(!prefs.show_footer, "a bad statistic keeps the other prefs");
}

// --- Shortcut sheet -----------------------------------------------------------

fn sheet_app() -> App {
    let mut app = App::default();
    let _ = app.update(Message::ToggleShortcuts);
    assert_eq!(app.overlay, Overlay::Shortcuts);
    app
}

#[test]
fn the_shortcut_sheet_has_a_close_button() {
    let mut app = sheet_app();
    let mut ui = PointerUi::new();
    let close = ui.id_bounds(&app, "shortcuts-close").expect("the X button");
    let sheet = ui.id_bounds(&app, "shortcuts-sheet").expect("the sheet");
    assert!(sheet.contains(center(close)), "the X sits on the sheet");
    assert!(
        close.height >= 28.0,
        "a 28 px target like the other ghost buttons"
    );
    let messages = ui.click(&app, center(close));
    apply(&mut app, messages);
    assert_eq!(app.overlay, Overlay::None);
}

#[test]
fn a_sheet_row_runs_its_action_and_a_plain_row_does_nothing() {
    let mut app = sheet_app();
    let sidebar = app.sidebar_open;
    let mut ui = PointerUi::new();
    let row = ui
        .overlay_text_bounds(&app, "Toggle Sidebar")
        .expect("the Toggle Sidebar row");
    let messages = ui.click(&app, center(row));
    apply(&mut app, messages);
    assert_eq!(app.overlay, Overlay::None, "the sheet closes");
    assert_eq!(app.sidebar_open, !sidebar, "and the action runs");

    let mut app = sheet_app();
    let row = ui
        .text_bounds(&app, "Navigate nodes")
        .expect("a row without an action");
    let messages = ui.click(&app, center(row));
    apply(&mut app, messages);
    assert_eq!(app.overlay, Overlay::Shortcuts, "the sheet stays open");
}

#[test]
fn the_sheet_lists_the_shortcuts_the_key_map_has() {
    let rows: Vec<SheetRow> = shortcut_sheet_groups()
        .into_iter()
        .flat_map(|(_, rows)| rows)
        .collect();
    let has_keys = |keys: &str| rows.iter().any(|row| row.keys == keys);

    // Every shortcut the palette names is on the sheet, and a row that runs
    // an action runs the palette's action for those keys.
    let app = App::default();
    let mut palette = app.command_items();
    let mut mindmap = App::default();
    mindmap.view_mode = ViewMode::Mindmap;
    palette.extend(mindmap.command_items());
    for (label, message) in palette {
        let (_, Some(keys)) = split_command_label(label) else {
            continue;
        };
        let keys = if keys.starts_with("⌘K ") {
            "⌘K 0–6"
        } else {
            keys
        };
        let row = rows
            .iter()
            .find(|row| row.keys == keys || row.keys.split(' ').any(|k| k == keys))
            .unwrap_or_else(|| panic!("the sheet misses {keys} ({label})"));
        if let Some(action) = &row.action {
            assert_eq!(
                format!("{action:?}"),
                format!("{message:?}"),
                "{keys}: the row runs what the palette runs"
            );
        }
    }

    // Bindings only the key map has.
    for keys in ["⌘Z ⌘⇧Z", "j k", "Space ⇧Space", "← →", "⌘⇧W", "⌘W"] {
        assert!(has_keys(keys), "the sheet misses {keys}");
    }
    // ⌘S is listed once, not under both File and Edit.
    assert_eq!(rows.iter().filter(|row| row.keys == "⌘S").count(), 1);
}

#[test]
fn every_sheet_row_fits_on_the_sheet() {
    let app = sheet_app();
    let mut ui = PointerUi::new();
    let sheet = ui.id_bounds(&app, "shortcuts-sheet").expect("the sheet");
    for (_, rows) in shortcut_sheet_groups() {
        for row in rows {
            let label = ui
                .text_bounds(&app, row.label)
                .unwrap_or_else(|| panic!("{} is drawn", row.label));
            assert!(
                label.y >= sheet.y && label.y + label.height <= sheet.y + sheet.height,
                "{} at {label:?} is clipped by {sheet:?}",
                row.label
            );
        }
    }
}

// --- Command Palette ------------------------------------------------------------

#[test]
fn the_palette_draws_shortcuts_as_right_aligned_key_caps() {
    let mut app = App::default();
    let _ = app.update(Message::OpenCommandPalette);
    // One row, so its cap is the only "B" in the palette.
    let _ = app.update(Message::OverlayQueryChanged("toggle sidebar".into()));
    assert_eq!(app.filtered_commands().len(), 1);
    let mut ui = PointerUi::new();
    assert!(
        ui.text_bounds(&app, "Toggle Sidebar  ⌘B").is_none(),
        "the shortcut is not part of the label"
    );
    let label = ui
        .overlay_text_bounds(&app, "Toggle Sidebar")
        .expect("the label");
    let input = ui
        .id_bounds(&app, "overlay-input")
        .expect("the query field");
    let cap = ui.overlay_text_bounds(&app, "B").expect("the B key cap");
    assert!(
        (cap.center_y() - label.center_y()).abs() < 4.0,
        "the cap is on the label's row: {cap:?} vs {label:?}"
    );
    assert!(
        cap.x + cap.width > input.x + input.width - 48.0,
        "right-aligned: {cap:?} in {input:?}"
    );
}

#[test]
fn typing_matches_command_names_not_their_shortcuts() {
    let mut app = App::default();
    let _ = app.update(Message::OpenCommandPalette);
    let _ = app.update(Message::OverlayQueryChanged("⌘".into()));
    assert!(app.filtered_commands().is_empty(), "no name has a ⌘");
    let _ = app.update(Message::OverlayQueryChanged("sidebar".into()));
    assert!(app
        .filtered_commands()
        .iter()
        .any(|(label, _, _)| label.starts_with("Toggle Sidebar")));
    // "B" alone matches names with a b, not every ⌘B-style shortcut.
    let _ = app.update(Message::OverlayQueryChanged("⌘B".into()));
    assert!(app.filtered_commands().is_empty());
}

// --- Query clear buttons ------------------------------------------------------------

fn assert_clear_button(
    mut app: App,
    id: &'static str,
    input: &'static str,
    query: fn(&App) -> &str,
) {
    let mut ui = PointerUi::new();
    assert!(
        ui.id_bounds(&app, id).is_none(),
        "no clear button on an empty field"
    );
    match id {
        "vault-query-clear" => {
            let _ = app.update(Message::VaultQueryChanged("needle".into()));
        }
        _ => {
            let _ = app.update(Message::OverlayQueryChanged("needle".into()));
        }
    }
    let clear = ui.id_bounds(&app, id).expect("a clear button");
    let field = ui.id_bounds(&app, input).expect("the field");
    assert!(
        (clear.center_y() - field.center_y()).abs() < 6.0,
        "beside the field: {clear:?} vs {field:?}"
    );
    let messages = ui.click(&app, center(clear));
    apply_ui(&mut app, &mut ui, messages);
    assert_eq!(query(&app), "", "the click clears the query");
    assert_eq!(ui.focused(&app, input), Some(true), "and focuses the field");
    assert!(ui.id_bounds(&app, id).is_none());
}

#[test]
fn the_palette_query_has_a_clear_button() {
    let mut app = App::default();
    let _ = app.update(Message::OpenCommandPalette);
    assert_clear_button(app, "overlay-query-clear", "overlay-input", |app| {
        app.overlay_query.as_str()
    });
}

#[test]
fn the_finder_query_has_a_clear_button() {
    let (mut app, dir) = tree_fixture("clear-finder");
    let _ = app.update(Message::OpenFileFinder);
    assert_eq!(app.overlay, Overlay::FileFinder);
    assert_clear_button(app, "overlay-query-clear", "overlay-input", |app| {
        app.overlay_query.as_str()
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_vault_query_has_a_clear_button() {
    let (mut app, dir) = tree_fixture("clear-vault");
    let _ = app.update(Message::OpenVaultSearch);
    assert!(app.vault_open);
    assert_clear_button(app, "vault-query-clear", "vault-input", |app| {
        app.vault_query.as_str()
    });
    let _ = std::fs::remove_dir_all(&dir);
}
