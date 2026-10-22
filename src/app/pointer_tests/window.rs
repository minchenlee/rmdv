//! Window and lifecycle pointer paths (MDV-048 P2, group 2): moving the
//! window from the toolbar, close protection, the minimum size, the drop
//! highlight, and the welcome page's recent items.

use super::group3::tree_fixture;
use super::*;
use crate::app::lifecycle::WINDOW_MIN_SIZE;
use crate::icon::ic;
use crate::prefs::ToolbarMode;
use iced::advanced::widget::{Id, Operation};

fn always(mut app: App) -> App {
    let _ = app.update(Message::SetToolbarMode(ToolbarMode::Always));
    app
}

/// A point on the toolbar's blank space: left of the view switch and clear
/// of the file name, the Show Sidebar button, and Back / Forward.
fn blank_toolbar_point(ui: &mut PointerUi, app: &App) -> Point {
    let bar = ui.id_bounds(app, "toolbar").expect("toolbar shown");
    let read = ui.text_bounds(app, "Read").expect("Read segment");
    let name = ui.text_bounds(app, "pointer-long.md");
    // The Forward chevron ends the left controls.
    let left_end = ui
        .text_bounds(app, &ic::CHEVRON_RIGHT.to_string())
        .filter(|b| b.y < bar.y + bar.height)
        .map_or(0.0, |b| b.x + b.width + 16.0);
    let y = bar.y + bar.height / 2.0;
    let x = (0..60)
        .map(|step| (bar.x + 160.0).max(left_end) + step as f32 * 10.0)
        .find(|x| {
            *x < read.x - 20.0
                && !name.is_some_and(|n| *x >= n.x - 8.0 && *x <= n.x + n.width + 8.0)
        })
        .expect("blank toolbar space");
    Point::new(x, y)
}

fn drags(messages: &[Message]) -> bool {
    has(messages, |m| matches!(m, Message::WindowDrag))
}

fn zooms(messages: &[Message]) -> bool {
    has(messages, |m| {
        matches!(m, Message::WindowTitlebarDoubleClick)
    })
}

// --- 1. Window drag -------------------------------------------------------------

#[test]
fn pressing_blank_toolbar_space_moves_the_window() {
    let app = always(long_doc_app());
    let mut ui = PointerUi::new();
    let at = blank_toolbar_point(&mut ui, &app);
    let messages = ui.click(&app, at);
    assert_eq!(drags(&messages), APP_DRAWS_TITLEBAR, "{messages:?}");
    assert!(!zooms(&messages), "one click does not zoom");
}

#[test]
fn toolbar_buttons_do_not_move_the_window() {
    let app = always(long_doc_app());
    let mut ui = PointerUi::new();
    for label in ["Read", "Edit"] {
        let at = center(ui.text_bounds(&app, label).expect("segment"));
        let messages = ui.click(&app, at);
        assert!(!drags(&messages), "{label}: {messages:?}");
    }
    let palette = ui
        .text_bounds(&app, &ic::COMMAND.to_string())
        .expect("palette button");
    let messages = ui.click(&app, center(palette));
    assert!(
        has(&messages, |m| matches!(m, Message::OpenCommandPalette)),
        "{messages:?}"
    );
    assert!(!drags(&messages), "{messages:?}");
}

#[test]
fn double_clicking_blank_toolbar_space_zooms_the_window() {
    let app = always(long_doc_app());
    let mut ui = PointerUi::new();
    let at = blank_toolbar_point(&mut ui, &app);
    let messages = ui.double_click(&app, at);
    assert_eq!(zooms(&messages), APP_DRAWS_TITLEBAR, "{messages:?}");
}

#[test]
fn revealed_hover_toolbar_moves_the_window() {
    let mut app = long_doc_app();
    let _ = app.update(Message::ToolbarHover(true));
    let mut ui = PointerUi::new();
    let at = blank_toolbar_point(&mut ui, &app);
    let messages = ui.click(&app, at);
    assert_eq!(drags(&messages), APP_DRAWS_TITLEBAR, "{messages:?}");
}

#[test]
fn sidebar_header_moves_the_window() {
    let (app, dir) = tree_fixture("window-drag-sidebar");
    let mut ui = PointerUi::new();
    let title = ui
        .text_containing(&app, "RMDV-TREE-WINDOW-DRAG-SIDEBAR")
        .and_then(|label| ui.text_bounds(&app, &label))
        .expect("workspace title");
    // The workspace title and the strip above it.
    for at in [
        center(title),
        Point::new(title.x + 4.0, (title.y - 6.0).max(1.0)),
    ] {
        let messages = ui.click(&app, at);
        assert_eq!(drags(&messages), APP_DRAWS_TITLEBAR, "{at:?}: {messages:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A task that would reach the window (its stream exists) or none at all.
fn reaches_window(task: Task<Message>) -> bool {
    iced_runtime::task::into_stream(task).is_some()
}

#[test]
fn window_drag_and_zoom_act_only_outside_fullscreen() {
    let mut app = long_doc_app();
    assert!(reaches_window(app.update(Message::WindowDrag)));
    assert!(reaches_window(
        app.update(Message::WindowTitlebarDoubleClick)
    ));
    app.window_fullscreen = true;
    assert!(!reaches_window(app.update(Message::WindowDrag)));
    assert!(!reaches_window(
        app.update(Message::WindowTitlebarDoubleClick)
    ));
}

// --- 3. Window minimum size -----------------------------------------------------

#[test]
fn window_keeps_a_minimum_size() {
    for transparent in [false, true] {
        let settings = window_settings(transparent);
        assert_eq!(settings.min_size, Some(Size::new(640.0, 420.0)));
        assert!(
            settings.size.width >= 640.0 && settings.size.height >= 420.0,
            "the default size fits the minimum: {:?}",
            settings.size
        );
    }
}

// --- 2. Close protection --------------------------------------------------------

/// `long_doc_app` in edit mode with one typed character.
fn dirty_app() -> App {
    let mut app = long_doc_app();
    let _ = app.update(Message::ToggleViewMode);
    let _ = app.update(Message::EditorAction(
        iced::widget::text_editor::Action::Edit(iced::widget::text_editor::Edit::Insert('x')),
    ));
    assert!(app.dirty);
    app
}

#[test]
fn window_waits_for_the_app_before_closing() {
    assert!(!window_settings(false).exit_on_close_request);
}

#[test]
fn closing_a_clean_window_closes_at_once() {
    let mut app = long_doc_app();
    let task = app.update(Message::WindowCloseRequested(iced::window::Id::unique()));
    assert!(app.close_confirm.is_none());
    assert!(app.window_closing);
    assert!(reaches_window(task));
}

#[test]
fn closing_with_unsaved_edits_asks_first() {
    let mut app = dirty_app();
    let id = iced::window::Id::unique();
    let _ = app.update(Message::WindowCloseRequested(id));
    assert_eq!(app.close_confirm, Some(CloseConfirm { window: Some(id) }));
    assert!(!app.window_closing);

    // Cancel keeps the window and the edits.
    let _ = app.update(Message::CloseConfirmCancel);
    assert!(app.close_confirm.is_none());
    assert!(!app.window_closing);
    assert!(app.dirty);

    // Don't Save closes without writing.
    let _ = app.update(Message::WindowCloseRequested(id));
    let task = app.update(Message::CloseConfirmDiscard);
    assert!(app.close_confirm.is_none());
    assert!(app.window_closing);
    assert!(reaches_window(task));
}

#[test]
fn quit_with_unsaved_edits_asks_first() {
    let mut app = dirty_app();
    let _ = app.update(Message::QuitRequested);
    assert_eq!(app.close_confirm, Some(CloseConfirm { window: None }));
    assert!(!app.window_closing);

    let mut clean = long_doc_app();
    let _ = clean.update(Message::QuitRequested);
    assert!(clean.close_confirm.is_none());
    assert!(clean.window_closing);
}

#[test]
fn save_in_the_close_confirmation_closes_only_after_the_save_succeeds() {
    let mut app = dirty_app();
    let _ = app.update(Message::WindowCloseRequested(iced::window::Id::unique()));
    let _ = app.update(Message::CloseConfirmSave);
    assert!(app.close_confirm.is_none(), "the confirmation gives way");
    assert!(!app.window_closing, "the write has not landed yet");
    let saved_source = app.source.clone();

    let _ = app.update(Message::FileSaved {
        result: Ok(()),
        saved_source,
    });
    assert!(!app.dirty);
    assert!(app.window_closing, "a successful save closes the window");
}

#[test]
fn a_failed_save_keeps_the_window_open() {
    let mut app = dirty_app();
    let _ = app.update(Message::WindowCloseRequested(iced::window::Id::unique()));
    let _ = app.update(Message::CloseConfirmSave);
    let saved_source = app.source.clone();
    let _ = app.update(Message::FileSaved {
        result: Err("disk full".into()),
        saved_source: saved_source.clone(),
    });
    assert!(!app.window_closing);
    assert!(app.dirty);
    assert!(app.close_after_save.is_none(), "the close is dropped");

    // A later save the user makes on their own does not close the window.
    let _ = app.update(Message::FileSaved {
        result: Ok(()),
        saved_source,
    });
    assert!(!app.window_closing);
}

#[test]
fn close_confirmation_owns_the_keyboard() {
    use iced::keyboard::{key::Named, Key, Modifiers};
    let key = |key: Key, modifiers: Modifiers| close_confirm_key_message(&key, modifiers);
    assert!(matches!(
        key(Key::Named(Named::Escape), Modifiers::empty()),
        Message::CloseConfirmCancel
    ));
    assert!(matches!(
        key(Key::Named(Named::Enter), Modifiers::empty()),
        Message::CloseConfirmSave
    ));
    assert!(matches!(
        key(Key::Character("d".into()), Modifiers::COMMAND),
        Message::CloseConfirmDiscard
    ));
    assert!(matches!(
        key(Key::Character("d".into()), Modifiers::empty()),
        Message::Noop
    ));
    assert!(matches!(
        key(Key::Character("s".into()), Modifiers::COMMAND),
        Message::Noop
    ));

    // Typing does not reach the editor under the confirmation.
    let mut app = dirty_app();
    let _ = app.update(Message::WindowCloseRequested(iced::window::Id::unique()));
    let before = app.editor.as_ref().unwrap().text();
    let _ = app.update(Message::EditorAction(
        iced::widget::text_editor::Action::Edit(iced::widget::text_editor::Edit::Insert('y')),
    ));
    assert_eq!(app.editor.as_ref().unwrap().text(), before);
}

#[test]
fn close_confirmation_buttons_and_backdrop() {
    let mut app = dirty_app();
    let _ = app.update(Message::WindowCloseRequested(iced::window::Id::unique()));
    let mut ui = PointerUi::new();
    assert!(
        ui.text_containing(&app, "pointer-long.md").is_some(),
        "the confirmation names the file"
    );
    for (label, expect) in [
        ("Save", "CloseConfirmSave"),
        ("Don't Save", "CloseConfirmDiscard"),
        ("Cancel", "CloseConfirmCancel"),
    ] {
        let bounds = ui
            .text_bounds(&app, label)
            .unwrap_or_else(|| panic!("{label} button"));
        let messages = ui.click(&app, center(bounds));
        assert!(
            messages.iter().any(|m| format!("{m:?}") == expect),
            "{label}: {messages:?}"
        );
    }
    // The backdrop cancels; the card's blank space does not.
    let messages = ui.click(&app, Point::new(20.0, WINDOW.height - 20.0));
    assert!(
        has(&messages, |m| matches!(m, Message::CloseConfirmCancel)),
        "{messages:?}"
    );
    let title = ui
        .text_containing(&app, "Save changes")
        .and_then(|t| ui.text_bounds(&app, &t))
        .expect("title");
    let messages = ui.click(
        &app,
        Point::new(title.x + title.width + 30.0, title.y + 4.0),
    );
    assert!(messages.is_empty(), "{messages:?}");
}

// --- 4. Drop highlight ----------------------------------------------------------

#[test]
fn dragging_files_over_the_window_shows_the_drop_highlight() {
    use iced::window::Event as W;
    let hovered = file_drag_message(W::FileHovered(PathBuf::from("/tmp/a.md")));
    assert!(
        matches!(hovered, Some(Message::DropHover(true))),
        "{hovered:?}"
    );
    let left = file_drag_message(W::FilesHoveredLeft);
    assert!(matches!(left, Some(Message::DropHover(false))), "{left:?}");
    assert!(file_drag_message(W::Focused).is_none());

    let mut app = long_doc_app();
    let mut ui = PointerUi::new();
    assert!(ui.text_bounds(&app, "Open in rmdv").is_none());
    let _ = app.update(Message::DropHover(true));
    assert!(app.drop_hover);
    let label = ui
        .text_bounds(&app, "Open in rmdv")
        .expect("the drop label shows");
    assert!(
        (label.center_x() - WINDOW.width / 2.0).abs() < 2.0,
        "centered: {label:?}"
    );
    let frame = ui.id_bounds(&app, "drop-highlight").expect("drop frame");
    assert_eq!(frame.size(), WINDOW, "the frame covers the window");

    let _ = app.update(Message::DropHover(false));
    assert!(!app.drop_hover);
    let mut ui = PointerUi::new();
    assert!(ui.text_bounds(&app, "Open in rmdv").is_none());
}

#[test]
fn dropping_a_file_clears_the_drop_highlight() {
    let dir = std::env::temp_dir().join(format!("rmdv-drop-clear-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut app = App::default();
    app.drop_hover = true;
    apply(&mut app, vec![dropped_path_message(dir.clone())]);
    assert!(!app.drop_hover);
    assert!(app.workspace.is_some(), "the drop still opens the folder");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn drop_highlight_takes_no_pointer_input() {
    let mut app = long_doc_app();
    app.drop_hover = true;
    let mut ui = PointerUi::new();
    let text_cursor = (0..20).any(|step| {
        let at = Point::new(400.0 + step as f32 * 20.0, 300.0);
        ui.move_to(&app, at).1 == mouse::Interaction::Text
    });
    assert!(text_cursor, "the reader keeps its cursor under the frame");
}

// --- 5. Recent items on the welcome page ----------------------------------------

#[test]
fn welcome_page_lists_recent_folders_and_files_and_opens_them() {
    let dir = std::env::temp_dir().join(format!("rmdv-welcome-recent-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let dir = canonicalize_existing_path(dir);
    let notes = dir.join("notes");
    std::fs::create_dir_all(&notes).unwrap();
    let file = dir.join("plan.md");
    std::fs::write(&file, "# Plan\n").unwrap();

    let mut app = App::default();
    let store = app.recent_store.clone().expect("private store");
    // An older entry that no longer exists is skipped.
    std::fs::write(
        &store,
        format!(
            r#"{{ "paths": ["{}"], "folders": ["{}"] }}"#,
            dir.join("gone.md").display(),
            dir.join("gone").display()
        ),
    )
    .unwrap();
    app.remember_recent_file(&file);
    app.remember_recent_folder(&notes);
    assert_eq!(app.recent.paths, vec![file.clone()]);
    assert_eq!(app.recent.folders, vec![notes.clone()]);
    let saved = crate::recent::load_from(&store);
    assert_eq!(saved.paths.len(), 2, "the store keeps what it had");

    // Opening a folder on purpose records it; a file's implied parent does not.
    let other = dir.join("other");
    std::fs::create_dir_all(&other).unwrap();
    let _ = app.update(Message::OpenWorkspace(other.clone()));
    assert_eq!(app.recent.folders.first(), Some(&other));
    let mut fresh = App::default();
    let _ = fresh.update(Message::FileLoaded(Ok((file.clone(), "# Plan\n".into()))));
    assert_eq!(fresh.recent.paths, vec![file.clone()]);
    assert!(
        fresh.recent.folders.is_empty(),
        "{:?}",
        fresh.recent.folders
    );

    let mut app = App::default();
    let store = app.recent_store.clone().unwrap();
    let _ = crate::recent::add_file_to(&store, &file);
    app.remember_recent_folder(&notes);
    let mut ui = PointerUi::new();
    assert!(ui.text_bounds(&app, "gone.md").is_none());
    let folder_row = ui.text_bounds(&app, "notes").expect("recent folder row");
    let file_row = ui.text_bounds(&app, "plan.md").expect("recent file row");
    assert!(ui.text_bounds(&app, "Recent Folders").is_some());
    assert!(ui.text_bounds(&app, "Recent Files").is_some());

    let messages = ui.click(&app, center(file_row));
    assert!(
        has(
            &messages,
            |m| matches!(m, Message::OpenFileFinderPath(p) if *p == file)
        ),
        "{messages:?}"
    );
    let messages = ui.click(&app, center(folder_row));
    assert!(
        has(
            &messages,
            |m| matches!(m, Message::OpenFileFinderPath(p) if *p == notes)
        ),
        "{messages:?}"
    );
    apply(&mut app, messages);
    assert_eq!(app.workspace.as_deref(), Some(notes.as_path()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn welcome_page_without_recent_items_shows_no_recent_section() {
    let app = App::default();
    let mut ui = PointerUi::new();
    assert!(ui.text_bounds(&app, "Recent Files").is_none());
    assert!(ui.text_bounds(&app, "Recent Folders").is_none());
}

// --- Review fixes ---------------------------------------------------------------

#[test]
fn disabled_toolbar_controls_do_not_move_or_zoom_the_window() {
    // A clean file: Save is disabled.
    let app = always(long_doc_app());
    let mut ui = PointerUi::new();
    let save = ui
        .text_bounds(&app, &ic::SAVE.to_string())
        .expect("Save button");
    let mut messages = ui.click(&app, center(save));
    messages.extend(ui.click(&app, center(save)));
    assert!(!drags(&messages), "{messages:?}");
    assert!(!zooms(&messages), "{messages:?}");

    // No file: Read, Edit, and Mind Map are disabled.
    let app = always(App::default());
    let mut ui = PointerUi::new();
    for label in ["Read", "Edit", "Mind Map"] {
        let at = center(ui.text_bounds(&app, label).expect("segment"));
        let messages = ui.double_click(&app, at);
        assert!(!drags(&messages), "{label}: {messages:?}");
        assert!(!zooms(&messages), "{label}: {messages:?}");
    }
}

#[test]
fn window_drag_area_acts_only_where_rmdv_draws_the_titlebar() {
    use iced::advanced::renderer::Headless;
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        iced::Font::DEFAULT,
        iced::Pixels(16.0),
        Some("tiny-skia"),
    ))
    .expect("headless tiny-skia renderer");
    let size = Size::new(200.0, 40.0);
    let at = Point::new(100.0, 20.0);
    let press = |titlebar_hidden: bool, renderer: &mut iced::Renderer| {
        let area: Element<'_, Message> = titlebar_drag_area(
            Space::new().width(Length::Fill).height(Length::Fill),
            titlebar_hidden,
        );
        let mut ui = UserInterface::build(area, size, Cache::default(), renderer);
        let mut messages = Vec::new();
        let _ = ui.update(
            &[
                Event::Mouse(mouse::Event::CursorMoved { position: at }),
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            ],
            mouse::Cursor::Available(at),
            renderer,
            &mut iced::advanced::clipboard::Null,
            &mut messages,
        );
        messages
    };
    assert!(drags(&press(true, &mut renderer)));
    assert!(
        press(false, &mut renderer).is_empty(),
        "under a native title bar the chrome does not move the window"
    );
    assert_eq!(APP_DRAWS_TITLEBAR, cfg!(target_os = "macos"));
}

#[test]
fn installing_an_update_with_unsaved_edits_asks_to_save_first() {
    let mut app = dirty_app();
    app.pending_update = Some(crate::update::ReadyUpdate {
        version: "9.9.9".into(),
        notes_url: None,
        artifact: std::env::temp_dir().join("rmdv-no-such-update.tar.gz"),
        sha256: String::new(),
    });
    let _ = app.update(Message::InstallUpdate);
    assert!(app.pending_update.is_some(), "the update stays offered");
    let toast = app.toast.as_ref().expect("a toast explains");
    assert!(toast.text.contains("unsaved edits"), "{}", toast.text);
    assert!(
        matches!(
            toast.action.as_ref().map(|a| &a.message),
            Some(Message::SaveFile)
        ),
        "the toast offers Save"
    );
}

fn ipc_close(app: &mut App) -> crate::ipc::Response {
    let (tx, mut rx) = futures::channel::oneshot::channel();
    let _ = app.update(Message::Ipc(
        crate::ipc::Request {
            id: 7,
            cmd: crate::ipc::Cmd::Close,
        },
        std::sync::Arc::new(std::sync::Mutex::new(Some(tx))),
    ));
    rx.try_recv()
        .ok()
        .flatten()
        .expect("the close request is answered")
}

#[test]
fn ipc_close_asks_before_dropping_unsaved_edits() {
    let mut app = dirty_app();
    let response = ipc_close(&mut app);
    assert_eq!(app.close_confirm, Some(CloseConfirm { window: None }));
    assert!(!app.window_closing);
    assert!(!response.ok, "{response:?}");

    let mut clean = long_doc_app();
    let response = ipc_close(&mut clean);
    assert!(response.ok, "{response:?}");
    assert!(clean.close_confirm.is_none());
    assert!(clean.window_closing);
}

/// The scroll translation of the scrollable that holds `target`; operations
/// see unscrolled bounds.
fn scroll_translation(ui: &mut PointerUi, app: &App, target: Rectangle) -> iced::Vector {
    use iced::advanced::widget::operation::Scrollable;
    struct Probe {
        target: Rectangle,
        found: iced::Vector,
    }
    impl Operation for Probe {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }

        fn scrollable(
            &mut self,
            _id: Option<&Id>,
            _bounds: Rectangle,
            content_bounds: Rectangle,
            translation: iced::Vector,
            _state: &mut dyn Scrollable,
        ) {
            if content_bounds.contains(self.target.center()) {
                self.found = translation;
            }
        }
    }
    let mut probe = Probe {
        target,
        found: iced::Vector::ZERO,
    };
    ui.operate(app, &mut probe);
    probe.found
}

#[test]
fn welcome_recent_rows_stay_reachable_in_the_smallest_window() {
    let (mut app, dir) = tree_fixture("welcome-min");
    // Outside the workspace, so the sidebar tree does not list the name.
    let elsewhere =
        std::env::temp_dir().join(format!("rmdv-welcome-min-recent-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&elsewhere);
    std::fs::create_dir_all(elsewhere.join("recent-folder-row")).unwrap();
    let elsewhere = canonicalize_existing_path(elsewhere);
    let file = elsewhere.join("recent-file-row.md");
    std::fs::write(&file, "# Recent\n").unwrap();
    app.remember_recent_folder(&elsewhere.join("recent-folder-row"));
    app.remember_recent_file(&file);
    assert!(
        app.file.is_none() && app.sidebar_open,
        "welcome beside the sidebar"
    );
    let natural = PointerUi::new()
        .text_bounds(&app, "recent-file-row.md")
        .expect("recent file row in a large window");
    let mut ui = PointerUi::sized(WINDOW_MIN_SIZE);
    let reader_x = app.sidebar_width;
    let probe = Point::new((reader_x + WINDOW_MIN_SIZE.width) / 2.0, 200.0);
    let _ = ui.move_to(&app, probe);
    let inside = |b: Rectangle| {
        b.x >= reader_x
            && b.x + b.width <= WINDOW_MIN_SIZE.width
            && b.y >= 0.0
            && b.y + b.height <= WINDOW_MIN_SIZE.height
    };
    let shown = |ui: &mut PointerUi| {
        let row = ui
            .text_bounds(&app, "recent-file-row.md")
            .expect("recent file row");
        let scrolled = scroll_translation(ui, &app, row);
        Rectangle {
            x: row.x - scrolled.x,
            y: row.y - scrolled.y,
            ..row
        }
    };
    let mut row = shown(&mut ui);
    for _ in 0..12 {
        if inside(row) {
            break;
        }
        let _ = ui.wheel(&app, probe);
        row = shown(&mut ui);
    }
    assert!(inside(row), "the row scrolls into the reader: {row:?}");
    assert!(
        (row.width - natural.width).abs() < 1.0,
        "the name is not squeezed: {row:?} vs {natural:?}"
    );
    let messages = ui.click(&app, center(row));
    assert!(
        has(
            &messages,
            |m| matches!(m, Message::OpenFileFinderPath(p) if *p == file)
        ),
        "{messages:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&elsewhere);
}

// --- Review screenshots ---------------------------------------------------------

#[test]
#[ignore = "writes review screenshots; set RMDV_WINDOW_SHOTS"]
fn window_screenshots() {
    use iced::advanced::renderer::Headless;
    let Some(dir) = std::env::var_os("RMDV_WINDOW_SHOTS").map(PathBuf::from) else {
        return;
    };
    std::fs::create_dir_all(&dir).expect("shot dir");
    {
        let fonts = iced::advanced::graphics::text::font_system();
        let mut fonts = fonts.write().expect("font system");
        for font in crate::fonts::BUNDLED {
            fonts.load_font(std::borrow::Cow::Borrowed(font));
        }
    }
    let scale = 2.0;
    let shoot = |app: &App, name: &str| {
        let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            iced::Font::with_name("Inter"),
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .expect("headless tiny-skia renderer");
        let mut ui = UserInterface::build(app.view(), WINDOW, Cache::default(), &mut renderer);
        let pal = app.palette;
        ui.draw(
            &mut renderer,
            &app.theme(),
            &iced::advanced::renderer::Style { text_color: pal.fg },
            mouse::Cursor::Unavailable,
        );
        let (w, h) = (
            (WINDOW.width * scale) as u32,
            (WINDOW.height * scale) as u32,
        );
        let rgba = renderer.screenshot(iced::Size::new(w, h), scale, pal.bg);
        let path = dir.join(name);
        image::RgbaImage::from_raw(w, h, rgba)
            .expect("rgba buffer")
            .save(&path)
            .expect("write png");
        eprintln!("wrote {}", path.display());
    };
    let ws = std::env::temp_dir().join(format!("rmdv-window-shots-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&ws);
    std::fs::create_dir_all(ws.join("notes")).unwrap();
    std::fs::write(ws.join("plan.md"), "# Plan\n").unwrap();
    std::fs::write(ws.join("notes").join("ideas.md"), "# Ideas\n").unwrap();
    for (preset, tag) in [
        (crate::theme::ThemePreset::OneLight, "light"),
        (crate::theme::ThemePreset::OneDark, "dark"),
    ] {
        let mut app = App::default();
        let _ = app.update(Message::SetTheme(preset));
        let _ = app.update(Message::SetGlass(crate::macos_vibrancy::Glass::Off));
        app.remember_recent_file(&ws.join("notes").join("ideas.md"));
        app.remember_recent_file(&ws.join("plan.md"));
        app.remember_recent_folder(&ws.join("notes"));
        app.remember_recent_folder(&ws);
        app.toast = None;
        shoot(&app, &format!("welcome-recent-{tag}.png"));
        app.drop_hover = true;
        shoot(&app, &format!("drop-highlight-{tag}.png"));

        let mut app = dirty_app();
        let _ = app.update(Message::SetTheme(preset));
        let _ = app.update(Message::SetGlass(crate::macos_vibrancy::Glass::Off));
        let _ = app.update(Message::WindowCloseRequested(iced::window::Id::unique()));
        app.toast = None;
        shoot(&app, &format!("close-confirm-{tag}.png"));
    }
    let _ = std::fs::remove_dir_all(&ws);
}
