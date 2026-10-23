//! Reader and editor gestures (MDV-048 P5): pinch and ⌘+scroll change the
//! reader font by the ⌘+ and ⌘− steps, and a trackpad scrolls the editor by
//! the finger distance. The step math is tested in `super::super::gestures`;
//! these tests drive `App::update` and the headless UI.

use super::*;
use iced::keyboard;

fn command_wheel(ui: &mut PointerUi, app: &App, at: Point, y: f32) -> Vec<Message> {
    let mut out = ui.move_to(app, at).0;
    out.extend(
        ui.send(
            app,
            at,
            vec![
                Event::Keyboard(keyboard::Event::ModifiersChanged(
                    keyboard::Modifiers::COMMAND,
                )),
                Event::Mouse(mouse::Event::WheelScrolled {
                    delta: mouse::ScrollDelta::Pixels { x: 0.0, y },
                }),
            ],
        )
        .0,
    );
    out
}

/// A point over the reading column, below the toolbar.
fn over_reader_text(ui: &mut PointerUi, app: &App) -> Point {
    let column = ui.id_bounds(app, "reader-column").expect("reader column");
    Point::new(column.center_x(), 300.0)
}

#[test]
fn command_wheel_over_the_reader_zooms_and_does_not_scroll() {
    let app = long_doc_app();
    let mut ui = PointerUi::new();
    let at = over_reader_text(&mut ui, &app);

    // A plain wheel scrolls, as before.
    let plain = ui.wheel(&app, at);
    assert!(
        has(&plain, |m| matches!(m, Message::BodyScrolled(_))),
        "a plain wheel scrolls the reader: {plain:?}"
    );

    let mut ui = PointerUi::new();
    let zoom = command_wheel(&mut ui, &app, at, 120.0);
    assert!(
        !has(&zoom, |m| matches!(m, Message::BodyScrolled(_))),
        "⌘+wheel must not scroll the reader: {zoom:?}"
    );
    assert!(
        has(
            &zoom,
            |m| matches!(m, Message::ReaderZoomBy(s) if *s > 1.9 && *s < 2.1)
        ),
        "⌘+wheel up asks for two larger steps (120 px): {zoom:?}"
    );

    // ⌘ released: the wheel scrolls again.
    let released = ui
        .send(
            &app,
            at,
            vec![
                Event::Keyboard(keyboard::Event::ModifiersChanged(
                    keyboard::Modifiers::empty(),
                )),
                Event::Mouse(mouse::Event::WheelScrolled {
                    delta: mouse::ScrollDelta::Pixels { x: 0.0, y: -120.0 },
                }),
            ],
        )
        .0;
    assert!(
        !has(&released, |m| matches!(m, Message::ReaderZoomBy(_))),
        "without ⌘ the wheel does not zoom: {released:?}"
    );
}

#[test]
fn reader_zoom_steps_wait_for_the_flush_and_match_the_keys() {
    let mut app = long_doc_app();
    let mut keys = long_doc_app();
    let start = app.font_scale;
    for _ in 0..4 {
        let _ = app.update(Message::ReaderZoomBy(0.7));
    }
    assert_eq!(app.font_scale, start, "no font change before the flush");
    assert!(app.reader_zoom.flush_scheduled());
    let _ = app.update(Message::ReaderZoomFlush);
    let _ = keys.update(Message::FontSizeUp);
    let _ = keys.update(Message::FontSizeUp);
    assert_eq!(
        app.font_scale, keys.font_scale,
        "2.8 steps apply two ⌘+ steps at once"
    );
    assert!(app.typography.body_size > 0.0);
    assert!((app.reader_zoom.pending() - 0.8).abs() < 1e-4);

    // Down by one step, as ⌘−.
    let _ = app.update(Message::ReaderZoomBy(-1.2));
    let _ = app.update(Message::ReaderZoomFlush);
    let _ = keys.update(Message::FontSizeDown);
    assert!((app.font_scale - keys.font_scale).abs() < 1e-6);
}

#[test]
fn reader_zoom_stops_at_the_keyboard_limit() {
    let mut app = long_doc_app();
    for _ in 0..40 {
        let _ = app.update(Message::FontSizeUp);
    }
    let max = app.font_scale;
    let _ = app.update(Message::ReaderZoomBy(1.5));
    let _ = app.update(Message::ReaderZoomFlush);
    assert_eq!(app.font_scale, max, "the same limit as ⌘+");
    assert_eq!(app.reader_zoom.pending(), 0.0, "a limit ends the gesture");
}

#[test]
fn a_reader_zoom_gesture_writes_prefs_once_after_it_ends() {
    let mut app = long_doc_app();
    let path = app
        .quick_slots_persistence_path
        .clone()
        .expect("the test prefs path");
    let _ = std::fs::remove_file(&path);
    let start = app.font_scale;
    for _ in 0..3 {
        let _ = app.update(Message::ReaderZoomBy(1.0));
        let _ = app.update(Message::ReaderZoomFlush);
    }
    assert!(app.font_scale > start);
    assert!(
        !path.exists(),
        "no synchronous prefs write per flush during the gesture"
    );

    // The trailing write: a stale timer does nothing, the latest writes.
    let latest = app.reader_zoom_save;
    let _ = app.update(Message::ReaderZoomSave(latest - 1));
    assert!(!path.exists(), "a timer of an earlier flush");
    let _ = app.update(Message::ReaderZoomSave(latest));
    assert_eq!(crate::prefs::load_from(&path).font_scale, app.font_scale);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn pinch_over_the_reader_collects_font_steps() {
    let mut app = long_doc_app();
    let _ = app.update(Message::MindmapNativePinch(0.25));
    assert!(
        (app.reader_zoom.pending() - 2.5).abs() < 1e-4,
        "a 0.25 pinch is 2.5 font steps: {}",
        app.reader_zoom.pending()
    );
    assert!(app.reader_zoom.flush_scheduled());
    assert_eq!(app.mindmap_native_pinch_log, 0.0);

    // An overlay owns the gesture.
    let mut app = long_doc_app();
    let _ = app.update(Message::OpenCommandPalette);
    let _ = app.update(Message::MindmapNativePinch(0.25));
    let _ = app.update(Message::ReaderZoomBy(3.0));
    assert_eq!(app.reader_zoom.pending(), 0.0);
    let start = app.font_scale;
    let _ = app.update(Message::ReaderZoomFlush);
    assert_eq!(app.font_scale, start);

    // The mind map keeps its own pinch.
    let mut app = long_doc_app();
    let _ = app.update(Message::ToggleMindmap);
    assert_eq!(app.view_mode, ViewMode::Mindmap);
    let _ = app.update(Message::MindmapNativePinch(0.25));
    assert_eq!(app.reader_zoom.pending(), 0.0);
    assert!((app.mindmap_native_pinch_log - 0.25).abs() < 1e-6);
}

fn editor_app() -> App {
    let mut app = long_doc_app();
    let _ = app.update(Message::ToggleViewMode);
    assert_eq!(app.view_mode, ViewMode::Raw);
    assert!(app.editor.is_some());
    app
}

fn scroll_lines(messages: &[Message]) -> Vec<i32> {
    messages
        .iter()
        .filter_map(|m| match m {
            Message::EditorAction(iced::widget::text_editor::Action::Scroll { lines }) => {
                Some(*lines)
            }
            _ => None,
        })
        .collect()
}

fn wheel_at(ui: &mut PointerUi, app: &App, at: Point, delta: mouse::ScrollDelta) -> Vec<Message> {
    let mut out = ui.move_to(app, at).0;
    out.extend(
        ui.send(
            app,
            at,
            vec![Event::Mouse(mouse::Event::WheelScrolled { delta })],
        )
        .0,
    );
    out
}

#[test]
fn trackpad_scrolls_the_editor_by_the_finger_distance() {
    let app = editor_app();
    let line = app.typography.code_size * 1.55;
    let at = Point::new(640.0, 400.0);
    let mut ui = PointerUi::new();
    let messages = wheel_at(
        &mut ui,
        &app,
        at,
        mouse::ScrollDelta::Pixels {
            x: 0.0,
            y: -3.0 * line,
        },
    );
    assert_eq!(
        scroll_lines(&messages),
        vec![3],
        "three lines of finger travel scroll three lines, not ~15: {messages:?}"
    );

    // Less than a line waits for more.
    let messages = wheel_at(
        &mut ui,
        &app,
        at,
        mouse::ScrollDelta::Pixels {
            x: 0.0,
            y: -0.4 * line,
        },
    );
    assert!(scroll_lines(&messages).is_empty(), "{messages:?}");
    let messages = wheel_at(
        &mut ui,
        &app,
        at,
        mouse::ScrollDelta::Pixels {
            x: 0.0,
            y: -0.7 * line,
        },
    );
    assert_eq!(scroll_lines(&messages), vec![1]);

    // A mouse wheel notch keeps the editor's own speed (4 lines).
    let messages = wheel_at(
        &mut ui,
        &app,
        at,
        mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 },
    );
    assert_eq!(scroll_lines(&messages), vec![4], "{messages:?}");
}

// --- Two-finger swipe between pages (macOS bridge) -------------------------

/// A reader with a twelve-column table (wider than the reading column, so
/// it scrolls horizontally) above a long run of paragraphs.
fn wide_table_app() -> App {
    let cols = 12;
    let mut source = String::from("|");
    for c in 0..cols {
        source.push_str(&format!(" Column {c} heading |"));
    }
    source.push_str("\n|");
    for _ in 0..cols {
        source.push_str(" --- |");
    }
    source.push('\n');
    for r in 0..6 {
        source.push('|');
        for c in 0..cols {
            source.push_str(&format!(" cell {r}-{c} with words |"));
        }
        source.push('\n');
    }
    source.push('\n');
    for i in 0..200 {
        source.push_str(&format!(
            "Paragraph {i} with enough words to fill a reader line here.\n\n"
        ));
    }
    let mut app = App::default();
    let _ = app.update(Message::FileLoaded(Ok((
        std::path::PathBuf::from("/tmp/pointer-wide-table.md"),
        source,
    ))));
    app
}

/// The heights (y, every 8 px) where the probe finds a horizontal
/// scroller under the pointer at `x`.
fn hscroll_rows(ui: &mut PointerUi, app: &App, x: f32) -> Vec<f32> {
    (5..100)
        .map(|i| i as f32 * 8.0)
        .filter(|y| {
            let mut probe = HScrollProbe::new();
            ui.operate(app, &mut probe);
            probe.covers(Point::new(x, *y))
        })
        .collect()
}

#[test]
fn the_probe_finds_a_wide_table_under_the_pointer_and_follows_the_scroll() {
    let app = wide_table_app();
    let mut ui = PointerUi::new();
    let column = ui.id_bounds(&app, "reader-column").expect("reader column");
    let x = column.center_x();
    let rows = hscroll_rows(&mut ui, &app, x);
    assert!(
        !rows.is_empty() && rows.len() < 40,
        "the table near the top scrolls horizontally: {rows:?}"
    );
    let below = rows.last().copied().unwrap_or_default() + 120.0;
    let mut probe = HScrollProbe::new();
    ui.operate(&app, &mut probe);
    assert!(
        !probe.covers(Point::new(x, below)),
        "the paragraphs below do not"
    );

    // Scrolled down, the table has left the top of the reader.
    let _ = ui.send(
        &app,
        Point::new(x, 400.0),
        vec![Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Pixels { x: 0.0, y: -1500.0 },
        })],
    );
    assert_eq!(hscroll_rows(&mut ui, &app, x), Vec::<f32>::new());

    // A document without a table has nothing to scroll sideways.
    let plain = long_doc_app();
    let mut ui = PointerUi::new();
    assert_eq!(hscroll_rows(&mut ui, &plain, x), Vec::<f32>::new());
}

#[test]
fn the_swipe_probe_walks_the_reader_once_per_layout_not_per_move() {
    use crate::app::view::PROBE_WALKS;
    if !crate::native_swipe::SUPPORTED {
        return;
    }
    let mut app = wide_table_app();
    app.history.back.push(HistoryEntry {
        path: PathBuf::from("/tmp/pointer-earlier.md"),
        position: 0.0,
        mode: ViewMode::Rendered,
    });
    assert!(app.reader_swipe_probe());
    let mut ui = PointerUi::new();
    let column = ui.id_bounds(&app, "reader-column").expect("reader column");
    let x = column.center_x();
    let rows = hscroll_rows(&mut ui, &app, x);
    let table_y = rows[rows.len() / 2];
    let text_y = rows.last().copied().unwrap_or_default() + 120.0;
    let moves = |y: f32| -> Vec<Event> {
        (0..20)
            .map(|i| {
                Event::Mouse(mouse::Event::CursorMoved {
                    position: Point::new(x + i as f32, y),
                })
            })
            .collect()
    };

    PROBE_WALKS.set(0);
    ui.send(&app, Point::new(x + 19.0, text_y), moves(text_y));
    assert!(
        crate::native_swipe::pointer_ok(),
        "over the paragraphs a swipe may start"
    );
    ui.send(&app, Point::new(x + 19.0, table_y), moves(table_y));
    assert!(
        !crate::native_swipe::pointer_ok(),
        "over the wide table the table scrolls"
    );
    assert!(
        PROBE_WALKS.get() <= 2,
        "one walk per layout, not one per move: {}",
        PROBE_WALKS.get()
    );
}

#[test]
fn a_swipe_armed_under_a_still_pointer_sees_what_is_under_it() {
    use crate::native_swipe::{pointer_ok, set_pointer_ok};
    if !crate::native_swipe::SUPPORTED {
        return;
    }
    let redraw = || {
        vec![Event::Window(iced::window::Event::RedrawRequested(
            std::time::Instant::now(),
        ))]
    };
    let earlier = HistoryEntry {
        path: PathBuf::from("/tmp/pointer-earlier.md"),
        position: 0.0,
        mode: ViewMode::Rendered,
    };

    // Over text with nothing to go back to; a link then adds a Back stop
    // while the pointer rests. The redraw after that update refreshes it.
    let mut app = long_doc_app();
    let mut ui = PointerUi::new();
    let at = over_reader_text(&mut ui, &app);
    ui.move_to(&app, at);
    set_pointer_ok(false);
    app.history.back.push(earlier.clone());
    assert!(app.reader_swipe_probe());
    ui.send(&app, at, redraw());
    assert!(pointer_ok(), "the first swipe after arming may go Back");

    // The pointer rests over a wide table when a swipe becomes armed.
    let mut app = wide_table_app();
    let mut ui = PointerUi::new();
    let column = ui.id_bounds(&app, "reader-column").expect("reader column");
    let rows = hscroll_rows(&mut ui, &app, column.center_x());
    let at = Point::new(column.center_x(), rows[rows.len() / 2]);
    ui.move_to(&app, at);
    set_pointer_ok(true);
    app.history.back.push(earlier);
    ui.send(&app, at, redraw());
    assert!(!pointer_ok(), "the table keeps its sideways scroll");
}

fn swipe_fixture(tag: &str) -> (App, PathBuf) {
    let dir = std::env::temp_dir().join(format!("rmdv-swipe-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("fixture dir");
    let earlier = dir.join("earlier.md");
    std::fs::write(&earlier, "# Earlier\n").expect("earlier.md");
    let mut app = long_doc_app();
    app.history.back.push(HistoryEntry {
        path: earlier.clone(),
        position: 0.0,
        mode: ViewMode::Rendered,
    });
    (app, earlier)
}

#[test]
fn a_swipe_is_armed_only_on_the_reader_toward_a_stop() {
    use crate::native_swipe::{BACK, FORWARD};
    let (mut app, earlier) = swipe_fixture("arm");
    assert_eq!(app.swipe_directions(), BACK);
    app.history.forward.push(HistoryEntry {
        path: earlier,
        position: 0.0,
        mode: ViewMode::Rendered,
    });
    assert_eq!(app.swipe_directions(), BACK | FORWARD);

    let _ = app.update(Message::OpenCommandPalette);
    assert_eq!(app.swipe_directions(), 0, "an overlay owns the pointer");
    let _ = app.update(Message::CloseOverlay);
    assert_eq!(app.swipe_directions(), BACK | FORWARD);

    let _ = app.update(Message::ToggleMindmap);
    assert_eq!(app.swipe_directions(), 0, "the mind map keeps its scroll");
    let _ = app.update(Message::ToggleMindmap);
    let _ = app.update(Message::ToggleViewMode);
    assert_eq!(app.view_mode, ViewMode::Raw);
    assert_eq!(app.swipe_directions(), 0, "the editor keeps its scroll");

    let empty = long_doc_app();
    assert_eq!(empty.swipe_directions(), 0, "no stop either way");
}

#[test]
fn the_reader_probes_for_a_swipe_only_where_one_can_start() {
    let (app, _) = swipe_fixture("probe");
    assert_eq!(
        app.reader_swipe_probe(),
        cfg!(target_os = "macos"),
        "only macOS has the swipe monitor"
    );
    assert!(!long_doc_app().reader_swipe_probe(), "no stop either way");
}

#[test]
fn a_swipe_goes_back_like_the_back_button() {
    let (mut app, earlier) = swipe_fixture("go");
    let _ = app.update(Message::NativeSwipe(HistoryDir::Forward));
    assert!(app.history.pending.is_none(), "no stop ahead");

    let _ = app.update(Message::OpenCommandPalette);
    let _ = app.update(Message::NativeSwipe(HistoryDir::Back));
    assert!(app.history.pending.is_none(), "not under an overlay");
    let _ = app.update(Message::CloseOverlay);

    let _ = app.update(Message::NativeSwipe(HistoryDir::Back));
    assert_eq!(
        app.history.pending,
        Some((HistoryDir::Back, earlier)),
        "the swipe starts the Back load"
    );
}
