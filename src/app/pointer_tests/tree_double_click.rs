//! Double clicks in the sidebar (MDV-048 P6): a file row opens the file and
//! gives the keyboard to the reader, a folder row toggles once, and the
//! sidebar divider goes back to its default width.

use super::group3::tree_fixture;
use super::*;

/// Apply `messages` one by one; returns what their tasks yield at once.
fn run(app: &mut App, messages: Vec<Message>) -> Vec<Message> {
    let mut outputs = Vec::new();
    for message in messages {
        outputs.extend(ready_outputs(app.update(message)));
    }
    outputs
}

fn row_clicks(messages: &[Message]) -> usize {
    messages
        .iter()
        .filter(|m| matches!(m, Message::TreeRowClicked { .. }))
        .count()
}

#[test]
fn double_clicking_a_file_opens_it_and_gives_the_reader_the_keys() {
    let (mut app, dir) = tree_fixture("dbl-file");
    let alpha = dir.join("alpha.md");
    assert!(app.tree_keys_active(), "the tree has the arrow keys");
    let mut ui = PointerUi::new();
    let row = ui.text_bounds(&app, "alpha.md").expect("alpha row");

    // What one click costs: a single file load.
    let (mut single, single_dir) = tree_fixture("dbl-file-single");
    let before = single.file_refresh_generation;
    let _ = single.update(Message::TreeRowClicked {
        path: single_dir.join("alpha.md"),
        is_dir: false,
    });
    let one_load = single.file_refresh_generation - before;
    assert!(one_load > 0);
    let _ = std::fs::remove_dir_all(&single_dir);

    let before = app.file_refresh_generation;
    let messages = ui.double_click(&app, center(row));
    assert_eq!(row_clicks(&messages), 2, "{messages:?}");
    let _ = run(&mut app, messages);

    assert_eq!(
        app.file_refresh_generation - before,
        one_load,
        "one load, not two"
    );
    assert_eq!(
        app.generic_load.as_ref().map(|(_, path)| path),
        Some(&alpha),
        "alpha is loading"
    );
    assert!(
        !app.tree_keys_active(),
        "arrows, Space and Enter go to the reader"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_tree_takes_the_keys_back() {
    let (mut app, dir) = tree_fixture("dbl-back");
    let alpha = dir.join("alpha.md");
    let beta = dir.join("beta.md");
    let double = |app: &mut App| {
        for _ in 0..2 {
            let _ = app.update(Message::TreeRowClicked {
                path: alpha.clone(),
                is_dir: false,
            });
        }
        assert!(!app.tree_keys_active());
    };

    // A single click on a row.
    double(&mut app);
    let _ = app.update(Message::TreeRowClicked {
        path: beta.clone(),
        is_dir: false,
    });
    assert!(app.tree_keys_active(), "a click in the tree");

    // ← (Files tab), as before.
    app.tree_last_click = None;
    double(&mut app);
    let _ = app.update(Message::SetSidebarTab(SidebarTab::Files));
    assert!(app.tree_keys_active(), "the ← key");

    // Hiding and showing the sidebar.
    app.tree_last_click = None;
    double(&mut app);
    let _ = app.update(Message::ToggleSidebar);
    let _ = app.update(Message::ToggleSidebar);
    assert!(app.sidebar_open);
    assert!(app.tree_keys_active(), "a sidebar shown again");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn double_clicking_a_folder_toggles_it_once() {
    let (mut app, dir) = tree_fixture("dbl-folder");
    let sub = dir.join("sub");
    let mut ui = PointerUi::new();
    let row = ui.text_bounds(&app, "sub").expect("folder row");

    let messages = ui.double_click(&app, center(row));
    assert_eq!(row_clicks(&messages), 2, "{messages:?}");
    let _ = run(&mut app, messages);
    assert!(app.expanded.contains(&sub), "open, not opened and closed");
    assert!(
        app.tree_keys_active(),
        "a folder keeps the keys in the tree"
    );

    let messages = ui.double_click(&app, center(row));
    let _ = run(&mut app, messages);
    assert!(
        !app.expanded.contains(&sub),
        "the next double click closes it"
    );

    // Two slow clicks are two clicks.
    let click = Message::TreeRowClicked {
        path: sub.clone(),
        is_dir: true,
    };
    let _ = app.update(click.clone());
    assert!(app.expanded.contains(&sub));
    let long_ago = std::time::Instant::now().checked_sub(std::time::Duration::from_secs(60));
    if let (Some((_, at)), Some(long_ago)) = (app.tree_last_click.as_mut(), long_ago) {
        *at = long_ago;
        let _ = app.update(click);
        assert!(!app.expanded.contains(&sub), "a slow second click toggles");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn double_clicking_the_sidebar_divider_resets_its_width() {
    let (mut app, dir) = tree_fixture("dbl-divider");
    app.sidebar_width = 420.0;
    let mut ui = PointerUi::new();
    let divider = ui
        .id_bounds(&app, SIDEBAR_DIVIDER_ID)
        .expect("the sidebar divider");
    let (_, cursor) = ui.move_to(&app, center(divider));
    assert_eq!(cursor, mouse::Interaction::ResizingHorizontally);

    let messages = ui.double_click(&app, center(divider));
    assert!(
        has(&messages, |m| matches!(m, Message::SidebarResetWidth)),
        "{messages:?}"
    );
    apply(&mut app, messages);

    assert_eq!(app.sidebar_width, SIDEBAR_WIDTH);
    assert!(
        app.sidebar_drag.is_none(),
        "the double click's own press does not keep dragging"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
