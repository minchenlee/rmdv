//! Navigation history (MDV-048 P3, group 1): the Back and Forward stacks,
//! the mouse side buttons, ⌘[ and ⌘], and the toolbar chevrons.

use super::*;
use std::time::Duration;

/// Where the reader sits in the default window.
const READER: Point = Point::new(700.0, 420.0);

/// A workspace with a long `a.md`, short `b.md` and `c.md`, and `doc.md`
/// with a heading far down to jump to.
fn fixture(tag: &str) -> (App, PathBuf) {
    let dir = std::env::temp_dir().join(format!("rmdv-history-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("fixture dir");
    let mut long = String::from("# A\n\n");
    for i in 0..200 {
        long.push_str(&format!(
            "Paragraph {i} with enough words to fill a reader line here.\n\n"
        ));
    }
    std::fs::write(dir.join("a.md"), &long).expect("a.md");
    std::fs::write(dir.join("b.md"), "# B\n\nshort\n").expect("b.md");
    std::fs::write(dir.join("c.md"), "# C\n\nshort\n").expect("c.md");
    let mut doc = String::from("# Top\n\n[jump](#target)\n\n");
    doc.push_str(&long);
    doc.push_str("## Target\n\nThe end.\n");
    std::fs::write(dir.join("doc.md"), &doc).expect("doc.md");
    let dir = canonicalize_existing_path(dir);
    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    (app, dir)
}

/// Runs a task the way the runtime does: applies the messages it yields
/// (and theirs) and runs its widget operations on `ui`. A timer that does
/// not fire within a short wait (a toast's expiry) is dropped.
fn run(app: &mut App, ui: &mut PointerUi, task: Task<Message>) {
    use iced::futures::StreamExt;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let Some(mut stream) = iced_runtime::task::into_stream(task) else {
        return;
    };
    loop {
        let next = runtime.block_on(async {
            tokio::time::timeout(Duration::from_millis(200), stream.next()).await
        });
        match next {
            // A toast's expiry timer runs on the wall clock; a slow machine
            // would clear the toast before the test reads it.
            Ok(Some(iced_runtime::Action::Output(Message::ToastExpire(_)))) => {}
            Ok(Some(iced_runtime::Action::Output(message))) => {
                let task = app.update(message);
                run(app, ui, task);
            }
            Ok(Some(iced_runtime::Action::Widget(operation))) => {
                // As the runtime does: operate, then finish, which hands a
                // result (a measured scroll target) back to the task.
                use iced::advanced::widget::operation::Outcome;
                let mut current = Some(operation);
                while let Some(mut operation) = current.take() {
                    ui.operate(app, operation.as_mut());
                    if let Outcome::Chain(next) = operation.finish() {
                        current = Some(next);
                    }
                }
            }
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => break,
        }
    }
}

fn send(app: &mut App, ui: &mut PointerUi, message: Message) {
    let task = app.update(message);
    run(app, ui, task);
}

fn open(app: &mut App, ui: &mut PointerUi, path: &Path) {
    send(app, ui, Message::Open(path.to_path_buf()));
    assert_eq!(app.file.as_deref(), Some(path), "{path:?} opened");
    settle(app, ui);
}

/// A few frames: the reader reports its viewport and a pending scroll
/// restore lands, as it does after the runtime redraws.
fn settle(app: &mut App, ui: &mut PointerUi) {
    for _ in 0..4 {
        let (messages, _) = ui.send(
            app,
            READER,
            vec![Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            ))],
        );
        for message in messages {
            send(app, ui, message);
        }
    }
}

fn scroll_down(app: &mut App, ui: &mut PointerUi, steps: usize) {
    for _ in 0..steps {
        let messages = ui.wheel(app, READER);
        for message in messages {
            send(app, ui, message);
        }
    }
    settle(app, ui);
}

fn offset(app: &App) -> f32 {
    app.body_viewport
        .as_ref()
        .map_or(0.0, |viewport| viewport.absolute_offset().y)
}

fn back(app: &mut App, ui: &mut PointerUi) {
    send(app, ui, Message::HistoryGo(HistoryDir::Back));
    settle(app, ui);
}

fn forward(app: &mut App, ui: &mut PointerUi) {
    send(app, ui, Message::HistoryGo(HistoryDir::Forward));
    settle(app, ui);
}

fn toast_text(app: &App) -> String {
    app.toast
        .as_ref()
        .map(|t| t.text.clone())
        .unwrap_or_default()
}

// --- 1. History stack ----------------------------------------------------------

#[test]
fn back_returns_to_the_previous_file_at_its_scroll_offset_and_forward_undoes_it() {
    let (mut app, dir) = fixture("back");
    let mut ui = PointerUi::new();
    let (a, b) = (dir.join("a.md"), dir.join("b.md"));
    open(&mut app, &mut ui, &a);
    scroll_down(&mut app, &mut ui, 12);
    let saved = offset(&app);
    assert!(saved > 400.0, "a.md scrolled: {saved}");

    open(&mut app, &mut ui, &b);
    back(&mut app, &mut ui);
    assert_eq!(app.file.as_deref(), Some(a.as_path()), "Back opens a.md");
    let restored = offset(&app);
    assert!(
        (restored - saved).abs() < 40.0,
        "Back lands at the saved offset: saved {saved}, restored {restored}"
    );

    forward(&mut app, &mut ui);
    assert_eq!(app.file.as_deref(), Some(b.as_path()), "Forward opens b.md");
    back(&mut app, &mut ui);
    assert_eq!(app.file.as_deref(), Some(a.as_path()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn back_to_a_file_left_at_the_top_lands_at_the_top() {
    let (mut app, dir) = fixture("top");
    let mut ui = PointerUi::new();
    let (a, doc) = (dir.join("a.md"), dir.join("doc.md"));
    open(&mut app, &mut ui, &a);
    assert!(offset(&app) < 1.0, "a.md at the top");
    open(&mut app, &mut ui, &doc);
    scroll_down(&mut app, &mut ui, 12);
    let scrolled = offset(&app);
    assert!(scrolled > 400.0, "doc.md scrolled: {scrolled}");

    back(&mut app, &mut ui);
    assert_eq!(app.file.as_deref(), Some(a.as_path()), "Back opens a.md");
    let restored = offset(&app);
    assert!(
        restored < 40.0,
        "Back lands at the top a.md was left at, not doc.md's offset: {restored}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_second_back_before_the_first_lands_keeps_the_stop_position() {
    let (mut app, dir) = fixture("quick");
    let mut ui = PointerUi::new();
    let (a, b, c, doc) = (
        dir.join("a.md"),
        dir.join("b.md"),
        dir.join("c.md"),
        dir.join("doc.md"),
    );
    open(&mut app, &mut ui, &c);
    open(&mut app, &mut ui, &a);
    scroll_down(&mut app, &mut ui, 12);
    let saved = offset(&app);
    assert!(saved > 400.0, "a.md scrolled: {saved}");
    open(&mut app, &mut ui, &b);
    open(&mut app, &mut ui, &doc);
    back(&mut app, &mut ui);
    assert_eq!(app.file.as_deref(), Some(b.as_path()));

    // Two quick presses: the second comes before a.md reports its viewport.
    send(&mut app, &mut ui, Message::HistoryGo(HistoryDir::Back));
    assert_eq!(app.file.as_deref(), Some(a.as_path()));
    back(&mut app, &mut ui);
    assert_eq!(app.file.as_deref(), Some(c.as_path()));
    // Positions are a fraction of the scroll range.
    let kept = app.history.forward.last().map(|e| e.position);
    assert!(
        kept.is_some_and(|p| p > 0.05),
        "a.md waits on Forward below the top: {kept:?}"
    );

    forward(&mut app, &mut ui);
    assert_eq!(app.file.as_deref(), Some(a.as_path()));
    let again = offset(&app);
    assert!(
        (again - saved).abs() < 40.0,
        "Forward lands where a.md was read: saved {saved}, again {again}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn back_restores_the_mind_map_view_a_file_was_left_in() {
    let (mut app, dir) = fixture("mode");
    let mut ui = PointerUi::new();
    let (a, b) = (dir.join("a.md"), dir.join("b.md"));
    open(&mut app, &mut ui, &a);
    send(&mut app, &mut ui, Message::ToggleMindmap);
    assert_eq!(app.view_mode, ViewMode::Mindmap);
    send(&mut app, &mut ui, Message::ToggleMindmap);
    open(&mut app, &mut ui, &b);
    // Leave b.md in the mind map, go back to a.md, then forward.
    send(&mut app, &mut ui, Message::ToggleMindmap);
    back(&mut app, &mut ui);
    assert_eq!(app.file.as_deref(), Some(a.as_path()));
    assert_eq!(app.view_mode, ViewMode::Rendered, "a.md was left in Read");
    forward(&mut app, &mut ui);
    assert_eq!(app.file.as_deref(), Some(b.as_path()));
    assert_eq!(
        app.view_mode,
        ViewMode::Mindmap,
        "b.md was left in the mind map"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_new_navigation_clears_forward() {
    let (mut app, dir) = fixture("clear-forward");
    let mut ui = PointerUi::new();
    let (a, b, c) = (dir.join("a.md"), dir.join("b.md"), dir.join("c.md"));
    open(&mut app, &mut ui, &a);
    open(&mut app, &mut ui, &b);
    back(&mut app, &mut ui);
    assert_eq!(app.history.forward.len(), 1, "b.md waits on Forward");
    open(&mut app, &mut ui, &c);
    assert!(
        app.history.forward.is_empty(),
        "opening c.md clears Forward"
    );
    forward(&mut app, &mut ui);
    assert_eq!(
        app.file.as_deref(),
        Some(c.as_path()),
        "nothing to go forward to"
    );
    back(&mut app, &mut ui);
    assert_eq!(app.file.as_deref(), Some(a.as_path()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn history_keeps_at_most_one_hundred_back_entries() {
    let mut app = App::default();
    for i in 0..130 {
        let _ = app.update(Message::FileLoaded(Ok((
            PathBuf::from(format!("/tmp/rmdv-history-bound-{i}.md")),
            format!("# {i}\n"),
        ))));
    }
    assert_eq!(app.history.back.len(), HISTORY_LIMIT);
    assert_eq!(
        app.history.back.last().map(|e| e.path.clone()),
        Some(PathBuf::from("/tmp/rmdv-history-bound-128.md")),
        "the newest entries stay"
    );
    assert_eq!(
        app.history.back.first().map(|e| e.path.clone()),
        Some(PathBuf::from("/tmp/rmdv-history-bound-29.md")),
        "the oldest drop off"
    );
}

#[test]
fn reloading_the_same_file_records_nothing() {
    let (mut app, dir) = fixture("reload");
    let mut ui = PointerUi::new();
    let a = dir.join("a.md");
    open(&mut app, &mut ui, &a);
    open(&mut app, &mut ui, &a);
    send(&mut app, &mut ui, Message::FileChanged(a.clone()));
    assert!(app.history.back.is_empty(), "{:?}", app.history.back);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_fragment_link_is_a_stop_that_back_returns_from() {
    let (mut app, dir) = fixture("fragment");
    let mut ui = PointerUi::new();
    let doc = dir.join("doc.md");
    open(&mut app, &mut ui, &doc);
    scroll_down(&mut app, &mut ui, 3);
    let saved = offset(&app);
    send(&mut app, &mut ui, Message::OpenLink("#target".into()));
    settle(&mut app, &mut ui);
    let jumped = offset(&app);
    assert!(
        jumped > saved + 1000.0,
        "the link jumps: {saved} -> {jumped}"
    );
    assert_eq!(app.history.back.len(), 1, "the jump is a stop");

    back(&mut app, &mut ui);
    assert_eq!(app.file.as_deref(), Some(doc.as_path()));
    let restored = offset(&app);
    assert!(
        (restored - saved).abs() < 40.0,
        "Back returns to where the link was: saved {saved}, restored {restored}"
    );
    forward(&mut app, &mut ui);
    let again = offset(&app);
    assert!(
        (again - jumped).abs() < 80.0,
        "Forward returns to the target: {jumped} vs {again}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_link_to_another_file_and_a_quick_slot_open_are_stops() {
    let (mut app, dir) = fixture("sources");
    let mut ui = PointerUi::new();
    let (a, b, c) = (dir.join("a.md"), dir.join("b.md"), dir.join("c.md"));
    std::fs::write(&a, "# A\n\n[to b](b.md)\n").expect("a.md");
    open(&mut app, &mut ui, &a);
    send(&mut app, &mut ui, Message::OpenLink("b.md".into()));
    assert_eq!(
        app.file.as_deref(),
        Some(b.as_path()),
        "the link opens b.md"
    );
    // Assign c.md to slot 1 from c.md, return to b.md, and open the slot.
    open(&mut app, &mut ui, &c);
    send(&mut app, &mut ui, Message::QuickSlotAssign(0));
    back(&mut app, &mut ui);
    assert_eq!(app.file.as_deref(), Some(b.as_path()));
    send(&mut app, &mut ui, Message::QuickSlotActivate(0));
    settle(&mut app, &mut ui);
    assert_eq!(
        app.file.as_deref(),
        Some(c.as_path()),
        "the slot opens c.md"
    );
    back(&mut app, &mut ui);
    assert_eq!(
        app.file.as_deref(),
        Some(b.as_path()),
        "Back leaves the slot"
    );
    back(&mut app, &mut ui);
    assert_eq!(app.file.as_deref(), Some(a.as_path()), "and then the link");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn back_with_unsaved_edits_keeps_them_and_offers_save() {
    let (mut app, dir) = fixture("dirty");
    let mut ui = PointerUi::new();
    let (a, b) = (dir.join("a.md"), dir.join("b.md"));
    open(&mut app, &mut ui, &a);
    open(&mut app, &mut ui, &b);
    send(&mut app, &mut ui, Message::ToggleViewMode);
    send(
        &mut app,
        &mut ui,
        Message::EditorAction(iced::widget::text_editor::Action::Edit(
            iced::widget::text_editor::Edit::Insert('x'),
        )),
    );
    assert!(app.dirty);
    back(&mut app, &mut ui);
    assert_eq!(app.file.as_deref(), Some(b.as_path()), "b.md stays open");
    assert!(app.dirty && app.editor.is_some(), "the edits stay");
    let action = app.toast.as_ref().and_then(|t| t.action.as_ref());
    assert!(
        action.is_some_and(|a| matches!(a.message, Message::SaveFile)),
        "the toast offers Save: {:?}",
        app.toast
    );
    assert_eq!(app.history.back.len(), 1, "history is unchanged");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn back_skips_a_deleted_file_with_a_toast() {
    let (mut app, dir) = fixture("missing");
    let mut ui = PointerUi::new();
    let (a, b, c) = (dir.join("a.md"), dir.join("b.md"), dir.join("c.md"));
    open(&mut app, &mut ui, &a);
    open(&mut app, &mut ui, &b);
    open(&mut app, &mut ui, &c);
    std::fs::remove_file(&b).expect("delete b.md");
    back(&mut app, &mut ui);
    assert_eq!(app.file.as_deref(), Some(a.as_path()), "Back skips b.md");
    assert!(toast_text(&app).contains("b.md"), "{:?}", app.toast);

    // Only missing files left: stay, and say so.
    let (mut app, dir2) = fixture("missing-only");
    let mut ui = PointerUi::new();
    let (a, b) = (dir2.join("a.md"), dir2.join("b.md"));
    open(&mut app, &mut ui, &a);
    open(&mut app, &mut ui, &b);
    std::fs::remove_file(&a).expect("delete a.md");
    back(&mut app, &mut ui);
    assert_eq!(app.file.as_deref(), Some(b.as_path()), "b.md stays open");
    assert!(toast_text(&app).contains("a.md"), "{:?}", app.toast);
    assert!(app.history.back.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&dir2);
}

#[test]
fn opening_another_workspace_clears_history() {
    let (mut app, dir) = fixture("workspace");
    let mut ui = PointerUi::new();
    let (a, b) = (dir.join("a.md"), dir.join("b.md"));
    open(&mut app, &mut ui, &a);
    open(&mut app, &mut ui, &b);
    back(&mut app, &mut ui);
    assert!(!app.history.forward.is_empty());
    let other = dir.join("other");
    std::fs::create_dir_all(&other).expect("other dir");
    std::fs::write(other.join("x.md"), "# X\n").expect("x.md");
    send(&mut app, &mut ui, Message::OpenWorkspace(other.clone()));
    assert_eq!(
        app.workspace.as_deref(),
        Some(canonicalize_existing_path(other).as_path())
    );
    assert!(app.history.back.is_empty() && app.history.forward.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

// --- 2. Mouse side buttons -----------------------------------------------------

#[test]
fn mouse_side_buttons_go_back_and_forward() {
    let press = |button| iced::Event::Mouse(mouse::Event::ButtonPressed(button));
    assert!(matches!(
        mouse_history_message(&press(mouse::Button::Back)),
        Some(Message::HistoryGo(HistoryDir::Back))
    ));
    assert!(matches!(
        mouse_history_message(&press(mouse::Button::Forward)),
        Some(Message::HistoryGo(HistoryDir::Forward))
    ));
    for other in [
        press(mouse::Button::Left),
        press(mouse::Button::Right),
        press(mouse::Button::Middle),
        press(mouse::Button::Other(7)),
        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Back)),
    ] {
        assert!(mouse_history_message(&other).is_none(), "{other:?}");
    }
}

#[test]
fn back_waits_while_a_modal_is_open() {
    let (mut app, dir) = fixture("modal");
    let mut ui = PointerUi::new();
    let (a, b) = (dir.join("a.md"), dir.join("b.md"));
    open(&mut app, &mut ui, &a);
    open(&mut app, &mut ui, &b);

    send(&mut app, &mut ui, Message::OpenCommandPalette);
    back(&mut app, &mut ui);
    assert_eq!(
        app.file.as_deref(),
        Some(b.as_path()),
        "the palette is open"
    );
    assert_eq!(app.overlay, Overlay::Command, "and stays open");
    send(&mut app, &mut ui, Message::CloseOverlay);

    // Slot 1 holds b.md; assigning a.md to it asks first.
    send(&mut app, &mut ui, Message::QuickSlotAssign(0));
    open(&mut app, &mut ui, &a);
    send(&mut app, &mut ui, Message::QuickSlotAssign(0));
    assert!(app.quick_slot_replace.is_some(), "the replace confirmation");
    back(&mut app, &mut ui);
    assert_eq!(
        app.file.as_deref(),
        Some(a.as_path()),
        "it owns the pointer"
    );
    send(&mut app, &mut ui, Message::QuickSlotReplaceCancel);

    back(&mut app, &mut ui);
    assert_eq!(
        app.file.as_deref(),
        Some(b.as_path()),
        "no modal: Back works"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// --- 3. ⌘[ and ⌘] ----------------------------------------------------------------

#[test]
fn command_brackets_go_back_and_forward() {
    use iced::keyboard::key::{Code, Physical};
    use iced::keyboard::{Key, Modifiers};
    let ch = |c: &str| Key::Character(c.into());
    let cmd = Modifiers::COMMAND;
    let back = |m: Option<Message>| matches!(m, Some(Message::HistoryGo(HistoryDir::Back)));
    let fwd = |m: Option<Message>| matches!(m, Some(Message::HistoryGo(HistoryDir::Forward)));
    assert!(back(history_key_message(
        &ch("["),
        Physical::Code(Code::BracketLeft),
        cmd
    )));
    assert!(fwd(history_key_message(
        &ch("]"),
        Physical::Code(Code::BracketRight),
        cmd
    )));
    // A layout with another character on the bracket keys.
    assert!(back(history_key_message(
        &ch("ü"),
        Physical::Code(Code::BracketLeft),
        cmd
    )));
    assert!(fwd(history_key_message(
        &ch("+"),
        Physical::Code(Code::BracketRight),
        cmd
    )));
    // Without ⌘, or with Shift or Option, the keys stay free.
    for mods in [
        Modifiers::empty(),
        cmd | Modifiers::SHIFT,
        cmd | Modifiers::ALT,
    ] {
        assert!(
            history_key_message(&ch("["), Physical::Code(Code::BracketLeft), mods).is_none(),
            "{mods:?}"
        );
    }
    assert!(history_key_message(&ch("p"), Physical::Code(Code::KeyP), cmd).is_none());
}

#[test]
fn palette_and_shortcut_sheet_list_go_back_and_go_forward() {
    let app = App::default();
    let items = app.command_items();
    assert!(
        items.iter().any(|(label, m)| *label == "Go Back  ⌘["
            && matches!(m, Message::HistoryGo(HistoryDir::Back))),
        "palette has Go Back"
    );
    assert!(
        items.iter().any(|(label, m)| *label == "Go Forward  ⌘]"
            && matches!(m, Message::HistoryGo(HistoryDir::Forward))),
        "palette has Go Forward"
    );

    let mut app = App::default();
    let _ = app.update(Message::ToggleShortcuts);
    let mut ui = PointerUi::new();
    assert!(ui.text_bounds(&app, "Go Back / Forward").is_some());
    let back = ui.text_bounds(&app, "[").expect("the [ key cap");
    assert!(back.y + back.height < 800.0, "on screen: {back:?}");
}

// --- 4. Toolbar chevrons -----------------------------------------------------------

/// Bounds of `code` drawn in the toolbar strip (the reader may draw the
/// same glyph lower down).
fn toolbar_glyph(ui: &mut PointerUi, app: &App, code: char) -> Option<Rectangle> {
    struct All {
        text: String,
        found: Vec<Rectangle>,
    }
    impl Operation for All {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
        fn text(&mut self, _id: Option<&Id>, bounds: Rectangle, text: &str) {
            if text == self.text {
                self.found.push(bounds);
            }
        }
    }
    let mut all = All {
        text: code.to_string(),
        found: Vec::new(),
    };
    ui.operate(app, &mut all);
    all.found.into_iter().find(|b| b.y < TOOLBAR_H)
}

fn history_clicks(messages: &[Message]) -> Option<HistoryDir> {
    messages.iter().find_map(|m| match m {
        Message::HistoryGo(dir) => Some(*dir),
        _ => None,
    })
}

fn moves_window(messages: &[Message]) -> bool {
    has(messages, |m| {
        matches!(m, Message::WindowDrag | Message::WindowTitlebarDoubleClick)
    })
}

#[test]
fn toolbar_chevrons_go_back_and_forward_and_name_their_shortcuts() {
    use super::group3::{assert_tips, tips};
    use crate::icon::ic;
    let (mut app, dir) = fixture("toolbar");
    let mut ui = PointerUi::new();
    let _ = app.update(Message::SetToolbarMode(crate::prefs::ToolbarMode::Always));
    let (a, b) = (dir.join("a.md"), dir.join("b.md"));
    open(&mut app, &mut ui, &a);
    open(&mut app, &mut ui, &b);
    assert_tips(&tips(&mut ui, &app), &["Go Back  ⌘[", "Go Forward  ⌘]"]);
    let back_at = toolbar_glyph(&mut ui, &app, ic::CHEVRON_LEFT).expect("a Back chevron");
    let forward_at = toolbar_glyph(&mut ui, &app, ic::CHEVRON_RIGHT).expect("a Forward chevron");
    assert!(back_at.x < forward_at.x, "Back sits left of Forward");

    // Nothing ahead: Forward is disabled and does not move the window.
    let mut messages = ui.click(&app, center(forward_at));
    messages.extend(ui.click(&app, center(forward_at)));
    assert_eq!(history_clicks(&messages), None, "{messages:?}");
    assert!(!moves_window(&messages), "{messages:?}");

    let messages = ui.click(&app, center(back_at));
    assert_eq!(history_clicks(&messages), Some(HistoryDir::Back));
    assert!(!moves_window(&messages), "{messages:?}");
    for message in messages {
        send(&mut app, &mut ui, message);
    }
    settle(&mut app, &mut ui);
    assert_eq!(app.file.as_deref(), Some(a.as_path()));

    // Now Back is the disabled one.
    let back_at = toolbar_glyph(&mut ui, &app, ic::CHEVRON_LEFT).expect("Back");
    let forward_at = toolbar_glyph(&mut ui, &app, ic::CHEVRON_RIGHT).expect("Forward");
    let mut messages = ui.click(&app, center(back_at));
    messages.extend(ui.click(&app, center(back_at)));
    assert_eq!(history_clicks(&messages), None, "{messages:?}");
    assert!(!moves_window(&messages), "{messages:?}");
    let messages = ui.click(&app, center(forward_at));
    assert_eq!(history_clicks(&messages), Some(HistoryDir::Forward));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn toolbar_chevrons_show_disabled_without_history() {
    use crate::icon::ic;
    let mut app = App::default();
    let _ = app.update(Message::SetToolbarMode(crate::prefs::ToolbarMode::Always));
    let mut ui = PointerUi::new();
    for code in [ic::CHEVRON_LEFT, ic::CHEVRON_RIGHT] {
        let at = toolbar_glyph(&mut ui, &app, code).expect("chevron shown");
        let mut messages = ui.click(&app, center(at));
        messages.extend(ui.click(&app, center(at)));
        assert_eq!(history_clicks(&messages), None, "{messages:?}");
        assert!(!moves_window(&messages), "{messages:?}");
    }
}

#[test]
fn back_from_full_mindmap_leaves_it_for_the_previous_file() {
    let (mut app, dir) = fixture("full");
    let mut ui = PointerUi::new();
    let (a, b) = (dir.join("a.md"), dir.join("b.md"));
    open(&mut app, &mut ui, &a);
    open(&mut app, &mut ui, &b);
    send(&mut app, &mut ui, Message::ToggleFullMindmap);
    assert!(app.full_mindmap.is_some());
    back(&mut app, &mut ui);
    assert!(app.full_mindmap.is_none(), "Back leaves Full Mindmap");
    assert_eq!(app.file.as_deref(), Some(a.as_path()));
    let _ = std::fs::remove_dir_all(&dir);
}
