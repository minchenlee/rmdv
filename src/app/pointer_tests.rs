//! Pointer (mouse and trackpad) behavior of overlays, lists, the vault page,
//! and the welcome page (MDV-048 P1). Each test builds `App::view` in a
//! headless tiny-skia `UserInterface`; no window opens.

use super::*;
use iced::advanced::widget::{Id, Operation};
use iced::mouse;
use iced::{Event, Point, Rectangle, Size};
use iced_runtime::user_interface::{self, Cache, UserInterface};

const WINDOW: Size = Size::new(1280.0, 800.0);

/// Feeds events to a fresh `UserInterface` built from the current
/// `App::view`, keeping widget state (hover, last cursor position) between
/// calls the way the event loop does.
struct PointerUi {
    renderer: iced::Renderer,
    cache: Option<Cache>,
    size: Size,
}

/// Locates a widget's bounds by the text it shows or by its container id.
struct Find<'a> {
    text: Option<&'a str>,
    contains: Option<&'a str>,
    id: Option<Id>,
    found: Option<Rectangle>,
    found_text: Option<String>,
}

impl Operation for Find<'_> {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if self.found.is_none() && self.id.is_some() && id == self.id.as_ref() {
            self.found = Some(bounds);
        }
    }

    fn text_input(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        _state: &mut dyn iced::advanced::widget::operation::TextInput,
    ) {
        self.container(id, bounds);
    }

    fn text(&mut self, _id: Option<&Id>, bounds: Rectangle, text: &str) {
        if self.found.is_some() {
            return;
        }
        let hit =
            self.text.is_some_and(|t| t == text) || self.contains.is_some_and(|t| text.contains(t));
        if hit {
            self.found = Some(bounds);
            self.found_text = Some(text.to_string());
        }
    }
}

/// Registers the bundled Lucide font once, as `main` does, so toolbar
/// icons measure as they draw. Without it a glyph falls back to whatever
/// system font covers its private-use code point, or to nothing.
fn load_icon_font() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let fonts = iced::advanced::graphics::text::font_system();
        let mut fonts = fonts.write().expect("font system");
        fonts.load_font(std::borrow::Cow::Borrowed(include_bytes!(
            "../assets/fonts/lucide.ttf"
        )));
    });
}

impl PointerUi {
    fn new() -> Self {
        use iced::advanced::renderer::Headless;
        load_icon_font();
        let renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            iced::Font::DEFAULT,
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .expect("headless tiny-skia renderer");
        Self {
            renderer,
            cache: None,
            size: WINDOW,
        }
    }

    /// A window of `size` instead of the default.
    fn sized(size: Size) -> Self {
        Self {
            size,
            ..Self::new()
        }
    }

    /// One batch of events at `at`; returns the published messages and the
    /// resulting mouse interaction (cursor shape).
    fn send(
        &mut self,
        app: &App,
        at: Point,
        events: Vec<Event>,
    ) -> (Vec<Message>, mouse::Interaction) {
        self.send_cursor(app, mouse::Cursor::Available(at), events)
    }

    /// The pointer leaves the window: the event loop then reports no
    /// cursor position.
    fn leave_window(&mut self, app: &App) -> Vec<Message> {
        self.send_cursor(
            app,
            mouse::Cursor::Unavailable,
            vec![Event::Mouse(mouse::Event::CursorLeft)],
        )
        .0
    }

    fn send_cursor(
        &mut self,
        app: &App,
        cursor: mouse::Cursor,
        events: Vec<Event>,
    ) -> (Vec<Message>, mouse::Interaction) {
        let cache = self.cache.take().unwrap_or_default();
        let mut ui = UserInterface::build(app.view(), self.size, cache, &mut self.renderer);
        let mut messages = Vec::new();
        let mut clipboard = iced::advanced::clipboard::Null;
        let (state, _) = ui.update(
            &events,
            cursor,
            &mut self.renderer,
            &mut clipboard,
            &mut messages,
        );
        let interaction = match state {
            user_interface::State::Updated {
                mouse_interaction, ..
            } => mouse_interaction,
            user_interface::State::Outdated => mouse::Interaction::None,
        };
        self.cache = Some(ui.into_cache());
        (messages, interaction)
    }

    fn move_to(&mut self, app: &App, at: Point) -> (Vec<Message>, mouse::Interaction) {
        self.send(
            app,
            at,
            vec![Event::Mouse(mouse::Event::CursorMoved { position: at })],
        )
    }

    /// Two clicks in one pass, so a slow machine cannot stretch them past
    /// the double-click interval.
    fn double_click(&mut self, app: &App, at: Point) -> Vec<Message> {
        let mut out = self.move_to(app, at).0;
        let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let release = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        out.extend(
            self.send(
                app,
                at,
                vec![press.clone(), release.clone(), press, release],
            )
            .0,
        );
        out
    }

    fn click(&mut self, app: &App, at: Point) -> Vec<Message> {
        let mut out = self.move_to(app, at).0;
        out.extend(
            self.send(
                app,
                at,
                vec![Event::Mouse(mouse::Event::ButtonPressed(
                    mouse::Button::Left,
                ))],
            )
            .0,
        );
        out.extend(
            self.send(
                app,
                at,
                vec![Event::Mouse(mouse::Event::ButtonReleased(
                    mouse::Button::Left,
                ))],
            )
            .0,
        );
        out
    }

    fn wheel(&mut self, app: &App, at: Point) -> Vec<Message> {
        let mut out = self.move_to(app, at).0;
        out.extend(
            self.send(
                app,
                at,
                vec![Event::Mouse(mouse::Event::WheelScrolled {
                    delta: mouse::ScrollDelta::Pixels { x: 0.0, y: -120.0 },
                })],
            )
            .0,
        );
        out
    }

    fn find(&mut self, app: &App, mut find: Find<'_>) -> Option<(Rectangle, Option<String>)> {
        let cache = self.cache.take().unwrap_or_default();
        let mut ui = UserInterface::build(app.view(), self.size, cache, &mut self.renderer);
        ui.operate(&self.renderer, &mut find);
        self.cache = Some(ui.into_cache());
        find.found.map(|bounds| (bounds, find.found_text))
    }

    fn text_bounds(&mut self, app: &App, text: &str) -> Option<Rectangle> {
        self.find(
            app,
            Find {
                text: Some(text),
                contains: None,
                id: None,
                found: None,
                found_text: None,
            },
        )
        .map(|(bounds, _)| bounds)
    }

    /// The last widget showing exactly `text`. Overlays draw after the
    /// page under them, so this finds an overlay row whose label the page
    /// (for example the welcome page's shortcut list) shows too.
    fn overlay_text_bounds(&mut self, app: &App, text: &str) -> Option<Rectangle> {
        struct Last<'a> {
            text: &'a str,
            found: Option<Rectangle>,
        }
        impl Operation for Last<'_> {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                operate(self);
            }
            fn text(&mut self, _id: Option<&Id>, bounds: Rectangle, text: &str) {
                if text == self.text {
                    self.found = Some(bounds);
                }
            }
        }
        let mut last = Last { text, found: None };
        let cache = self.cache.take().unwrap_or_default();
        let mut ui = UserInterface::build(app.view(), self.size, cache, &mut self.renderer);
        ui.operate(&self.renderer, &mut last);
        self.cache = Some(ui.into_cache());
        last.found
    }

    fn text_containing(&mut self, app: &App, needle: &str) -> Option<String> {
        self.find(
            app,
            Find {
                text: None,
                contains: Some(needle),
                id: None,
                found: None,
                found_text: None,
            },
        )
        .and_then(|(_, text)| text)
    }

    fn id_bounds(&mut self, app: &App, id: &'static str) -> Option<Rectangle> {
        self.find(
            app,
            Find {
                text: None,
                contains: None,
                id: Some(Id::new(id)),
                found: None,
                found_text: None,
            },
        )
        .map(|(bounds, _)| bounds)
    }
}

/// Messages a task yields without waiting (for example `Task::done`, which
/// yields once before its value). Polls a few times with a no-op waker inside
/// a Tokio context, so pending timers are created but never awaited.
fn ready_outputs(task: Task<Message>) -> Vec<Message> {
    use iced::futures::task::{noop_waker, Context, Poll};
    use iced::futures::StreamExt;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("tokio runtime");
    let _guard = runtime.enter();
    let mut out = Vec::new();
    let Some(mut stream) = iced_runtime::task::into_stream(task) else {
        return out;
    };
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    for _ in 0..8 {
        match stream.poll_next_unpin(&mut cx) {
            Poll::Ready(Some(iced_runtime::Action::Output(message))) => out.push(message),
            Poll::Ready(Some(_)) | Poll::Pending => {}
            Poll::Ready(None) => break,
        }
    }
    out
}

/// Applies published messages and the messages their tasks yield at once
/// (one level, like `Task::done` follow-ups).
fn apply(app: &mut App, messages: Vec<Message>) {
    for message in messages {
        let task = app.update(message);
        for follow_up in ready_outputs(task) {
            let _ = app.update(follow_up);
        }
    }
}

fn has(messages: &[Message], pred: impl Fn(&Message) -> bool) -> bool {
    messages.iter().any(pred)
}

fn center(bounds: Rectangle) -> Point {
    bounds.center()
}

fn long_doc_app() -> App {
    let mut source = String::new();
    for i in 0..200 {
        source.push_str(&format!(
            "Paragraph {i} with enough words to fill a reader line here.\n\n"
        ));
    }
    let mut app = App::default();
    let _ = app.update(Message::FileLoaded(Ok((
        std::path::PathBuf::from("/tmp/pointer-long.md"),
        source,
    ))));
    app
}

fn command_index(app: &App, label: &str) -> usize {
    app.filtered_commands()
        .iter()
        .position(|(l, _, _)| *l == label)
        .expect("command label")
}

// --- 1. Palette and theme row clicks behave like Enter ----------------------

#[test]
fn palette_row_click_closes_overlay_and_runs_command() {
    let mut app = App::default();
    let _ = app.update(Message::OpenCommandPalette);
    let sidebar_before = app.sidebar_open;
    let mut ui = PointerUi::new();
    let row = ui
        .overlay_text_bounds(&app, "Toggle Sidebar")
        .expect("palette row is rendered");

    let messages = ui.click(&app, center(row));
    apply(&mut app, messages);

    assert_eq!(app.overlay, Overlay::None, "a row click closes the palette");
    assert_ne!(app.sidebar_open, sidebar_before, "the command still runs");
}

#[test]
fn theme_row_click_applies_theme_and_closes_overlay() {
    let mut app = App::default();
    let _ = app.update(Message::OpenThemePicker);
    let target = ThemePreset::Nord;
    assert_ne!(app.theme_preset, target);
    let mut ui = PointerUi::new();
    let row = ui
        .text_bounds(&app, target.label())
        .expect("theme row is rendered");

    let messages = ui.click(&app, center(row));
    apply(&mut app, messages);

    assert_eq!(app.overlay, Overlay::None, "a row click closes the picker");
    assert_eq!(app.theme_preset, target, "the clicked theme is applied");
}

// --- 2. Panels are opaque; the scrim blocks hover and wheel -----------------

#[test]
fn clicking_blank_panel_space_keeps_overlays_open() {
    // Shortcut sheet: the whole card is static text.
    let mut app = App::default();
    let _ = app.update(Message::ToggleShortcuts);
    let mut ui = PointerUi::new();
    let title = ui
        .text_bounds(&app, "Navigation")
        .expect("sheet group title");
    let messages = ui.click(&app, center(title));
    assert!(
        !has(&messages, |m| matches!(m, Message::CloseOverlay)),
        "a click on the shortcut sheet must not close it: {messages:?}"
    );

    // Palette: the list padding beside a row is panel space, not scrim.
    let mut app = App::default();
    let _ = app.update(Message::OpenCommandPalette);
    let mut ui = PointerUi::new();
    let row_text = ui
        .text_bounds(&app, "Open Settings")
        .expect("first palette row");
    // Row text starts 12 px inside the row; the list pads rows by 8 px.
    let blank = Point::new(row_text.x - 12.0 - 4.0, row_text.center_y());
    let messages = ui.click(&app, blank);
    assert!(
        !has(&messages, |m| matches!(m, Message::CloseOverlay)),
        "a click on blank palette space must not close it: {messages:?}"
    );

    // The scrim outside the panel still closes it.
    let messages = ui.click(&app, Point::new(20.0, 20.0));
    assert!(
        has(&messages, |m| matches!(m, Message::CloseOverlay)),
        "a click on the scrim closes the palette: {messages:?}"
    );
}

#[test]
fn clicking_quick_slot_confirm_card_keeps_it_open() {
    let mut app = App::default();
    app.quick_slot_replace = Some(QuickSlotReplace {
        index: 0,
        current: "a.md".into(),
        incoming: "b.md".into(),
        path: None,
    });
    let mut ui = PointerUi::new();
    let title = ui
        .text_bounds(&app, "Replace Quick Slot 1?")
        .expect("confirm title");
    let messages = ui.click(&app, center(title));
    assert!(
        !has(&messages, |m| matches!(m, Message::QuickSlotReplaceCancel)),
        "a click on the card must not cancel: {messages:?}"
    );
    let messages = ui.click(&app, Point::new(20.0, 20.0));
    assert!(has(&messages, |m| matches!(
        m,
        Message::QuickSlotReplaceCancel
    )));
}

#[test]
fn overlay_scrim_blocks_hover_and_wheel_to_the_reader() {
    let mut app = long_doc_app();
    let mut ui = PointerUi::new();
    // Reader text above the palette panel (the panel starts at y = 190).
    let over_text = ui
        .text_bounds(
            &app,
            "Paragraph 0 with enough words to fill a reader line here.",
        )
        .map(center)
        .unwrap_or(Point::new(320.0, 70.0));
    assert!(over_text.y < 180.0, "probe point must sit above the panel");
    let (_, reader_cursor) = ui.move_to(&app, over_text);

    let _ = app.update(Message::OpenCommandPalette);
    let mut ui = PointerUi::new();
    let (_, cursor) = ui.move_to(&app, over_text);
    assert_ne!(
        cursor, reader_cursor,
        "the reader's cursor shape must not show through the scrim"
    );
    let messages = ui.wheel(&app, over_text);
    assert!(
        !has(&messages, |m| matches!(m, Message::BodyScrolled(_))),
        "wheel over the scrim must not scroll the reader: {messages:?}"
    );

    // Scrollables inside the panel still scroll.
    let row = ui
        .text_bounds(&app, "Open Settings")
        .expect("first palette row");
    let messages = ui.wheel(&app, center(row));
    assert!(
        has(&messages, |m| matches!(m, Message::OverlayScrolled(_))),
        "the palette list keeps wheel scrolling: {messages:?}"
    );
}

// --- 3. Hover moves the selection only after real pointer motion ------------

#[test]
fn palette_hover_moves_selection_after_pointer_motion() {
    let mut app = App::default();
    let _ = app.update(Message::OpenCommandPalette);
    let label = "Find in Document  ⌘F";
    let target = command_index(&app, label);
    let mut ui = PointerUi::new();
    let row = ui
        .overlay_text_bounds(&app, split_command_label(label).0)
        .expect("palette row");

    // The first pointer event after the overlay opens only sets a baseline:
    // a pointer resting where the list appears must not steal the selection.
    let messages = ui.move_to(&app, center(row)).0;
    apply(&mut app, messages);
    assert_eq!(app.overlay_selected, 0);

    let messages = ui
        .move_to(&app, Point::new(row.center_x() + 6.0, row.center_y()))
        .0;
    apply(&mut app, messages);
    assert_eq!(app.overlay_selected, target, "hover moved the selection");

    // Arrow keys move on; a still pointer does not take it back.
    let _ = app.update(Message::OverlayMove(1));
    assert_eq!(app.overlay_selected, target + 1);
    let messages = ui
        .move_to(&app, Point::new(row.center_x() + 6.0, row.center_y()))
        .0;
    apply(&mut app, messages);
    assert_eq!(app.overlay_selected, target + 1);
}

// --- 4. Folder picker: rows click and double-click -------------------------

mod picker;

// --- 5. Vault search close and search buttons --------------------------------

#[test]
fn vault_page_has_close_and_search_buttons() {
    let mut app = App::default();
    app.vault_open = true;
    app.vault_query = "needle".into();
    let mut ui = PointerUi::new();

    let search = ui
        .id_bounds(&app, "vault-search-button")
        .expect("vault header has a search button");
    let messages = ui.click(&app, center(search));
    assert!(
        has(&messages, |m| matches!(m, Message::VaultRunSearch)),
        "{messages:?}"
    );

    let close = ui
        .id_bounds(&app, "vault-close-button")
        .expect("vault header has a close button");
    let messages = ui.click(&app, center(close));
    assert!(
        has(&messages, |m| matches!(m, Message::VaultClose)),
        "{messages:?}"
    );
    apply(&mut app, messages);
    assert!(!app.vault_open);
}

// --- 6. File finder selection stays in the rendered rows ----------------------

fn finder_app(files: usize) -> App {
    let mut app = App::default();
    app.workspace = Some(std::path::PathBuf::from("/w"));
    app.workspace_files = (0..files)
        .map(|i| std::path::PathBuf::from(format!("/w/f{i:03}.md")))
        .collect();
    app.workspace_files_rev += 1;
    let _ = app.update(Message::OpenFileFinder);
    app
}

#[test]
fn file_finder_selection_stays_within_rendered_rows() {
    let mut app = finder_app(150);
    let _ = app.update(Message::OverlayMove(1000));
    assert_eq!(app.overlay_selected, 79, "only 80 rows are rendered");

    let mut ui = PointerUi::new();
    assert_eq!(
        ui.text_containing(&app, "keep typing to narrow").as_deref(),
        Some("70 more — keep typing to narrow")
    );

    // Past the 200-result cap the count is a lower bound.
    let app = finder_app(250);
    let mut ui = PointerUi::new();
    assert_eq!(
        ui.text_containing(&app, "keep typing to narrow").as_deref(),
        Some("120+ more — keep typing to narrow")
    );

    // Under the cap there is no hint.
    let app = finder_app(10);
    let mut ui = PointerUi::new();
    assert!(ui.text_containing(&app, "keep typing to narrow").is_none());
}

// --- 7. Welcome shortcut rows are buttons --------------------------------------

#[test]
fn welcome_shortcut_rows_send_their_shortcut_message() {
    let rows: [(&str, &str, Message); 9] = [
        ("Open Folder", "⌘O", Message::OpenFolderPicker),
        ("Find File in Workspace", "⌘P", Message::OpenFileFinder),
        ("Command Palette", "⌘⇧P", Message::OpenCommandPalette),
        ("Toggle Sidebar", "⌘B", Message::ToggleSidebar),
        ("Find in Document", "⌘F", Message::ToggleSearch),
        ("Cycle Theme", "⌘T", Message::ToggleTheme),
        ("Edit / Select Text", "⌘E", Message::WelcomeNeedsFile),
        ("Fold to Level (then 0–6)", "⌘K", Message::WelcomeNeedsFile),
        ("Full Mindmap Mode", "⌘⇧M", Message::ToggleFullMindmap),
    ];
    let app = App::default();
    for (label, key, expected) in rows {
        let mut ui = PointerUi::new();
        assert!(
            ui.text_bounds(&app, key).is_some(),
            "the {key} hint stays visible"
        );
        let bounds = ui.text_bounds(&app, label).expect("welcome row label");
        let messages = ui.click(&app, center(bounds));
        let expected = format!("{expected:?}");
        assert!(
            messages.iter().any(|m| format!("{m:?}") == expected),
            "{label} must send {expected}, got {messages:?}"
        );
    }
}

#[test]
fn hover_selection_ignores_rows_that_scroll_under_a_still_pointer() {
    let mut app = App::default();
    let _ = app.update(Message::OpenCommandPalette);

    // Baseline: the first pointer report is not motion.
    let _ = app.update(Message::ListPointerMoved(Point::new(10.0, 10.0)));
    let _ = app.update(Message::OverlayHover(3));
    assert_eq!(app.overlay_selected, 0);

    let _ = app.update(Message::ListPointerMoved(Point::new(10.0, 14.0)));
    let _ = app.update(Message::OverlayHover(3));
    assert_eq!(app.overlay_selected, 3);

    // Arrow keys scroll the list; the row now under the still pointer
    // reports a hover without pointer motion.
    let _ = app.update(Message::OverlayMove(2));
    assert_eq!(app.overlay_selected, 5);
    let _ = app.update(Message::OverlayHover(4));
    assert_eq!(
        app.overlay_selected, 5,
        "a still pointer keeps the selection"
    );

    // Real motion takes it again.
    let _ = app.update(Message::ListPointerMoved(Point::new(10.0, 20.0)));
    let _ = app.update(Message::OverlayHover(4));
    assert_eq!(app.overlay_selected, 4);
}

#[test]
fn vault_hover_moves_cursor_only_after_pointer_motion() {
    let mut app = App::default();
    app.workspace = Some(std::path::PathBuf::from("/w"));
    let _ = app.update(Message::OpenVaultSearch);
    for line in 1..=3 {
        app.vault_results.push(crate::vault_search::VaultHit {
            path: std::path::PathBuf::from("/w/a.md"),
            line,
            col_start: 0,
            col_end: 1,
            context: Vec::new(),
        });
    }

    let _ = app.update(Message::ListPointerMoved(Point::new(5.0, 5.0)));
    let _ = app.update(Message::VaultHover(2));
    assert_eq!(app.vault_cursor, 0, "baseline report is not motion");

    let _ = app.update(Message::ListPointerMoved(Point::new(5.0, 9.0)));
    let _ = app.update(Message::VaultHover(2));
    assert_eq!(app.vault_cursor, 2);

    let _ = app.update(Message::VaultMove(-1));
    let _ = app.update(Message::VaultHover(2));
    assert_eq!(app.vault_cursor, 1, "a still pointer keeps the cursor");

    // A stale index past the visible hits is ignored.
    let _ = app.update(Message::ListPointerMoved(Point::new(5.0, 30.0)));
    let _ = app.update(Message::VaultHover(9));
    assert_eq!(app.vault_cursor, 1);
}

#[test]
fn file_finder_more_label_counts_hidden_results() {
    assert_eq!(file_finder_more_label(80), None);
    assert_eq!(
        file_finder_more_label(81).as_deref(),
        Some("1 more — keep typing to narrow")
    );
    assert_eq!(
        file_finder_more_label(FILE_FINDER_RESULT_CAP).as_deref(),
        Some("120+ more — keep typing to narrow")
    );
}

#[test]
fn welcome_rows_that_need_a_file_say_so() {
    // Edit and Fold act on an open document; the welcome page has none.
    for label in ["Edit / Select Text", "Fold to Level (then 0–6)"] {
        let mut app = App::default();
        let mut ui = PointerUi::new();
        let bounds = ui.text_bounds(&app, label).expect("welcome row label");
        let messages = ui.click(&app, center(bounds));
        apply(&mut app, messages);
        assert!(
            app.toast
                .as_ref()
                .is_some_and(|t| t.text == "Open a file first"),
            "{label} must explain why it does nothing, got {:?}",
            app.toast.as_ref().map(|t| &t.text)
        );
        assert!(!app.fold_chord_pending, "{label} arms no fold chord");
    }
}

fn nudge_list_pointer(app: &mut App) {
    // Baseline, then real motion that no row consumed (blank list space).
    let _ = app.update(Message::ListPointerMoved(Point::new(40.0, 300.0)));
    let _ = app.update(Message::ListPointerMoved(Point::new(40.0, 304.0)));
}

#[test]
fn new_vault_results_do_not_take_the_cursor_from_a_still_pointer() {
    let mut app = App::default();
    app.workspace = Some(std::path::PathBuf::from("/w"));
    let _ = app.update(Message::OpenVaultSearch);
    nudge_list_pointer(&mut app);

    let _ = app.update(Message::VaultQueryChanged("a".into()));
    let _ = app.update(Message::VaultRunSearch);
    let hits = (1..=3)
        .map(|line| crate::vault_search::VaultHit {
            path: std::path::PathBuf::from("/w/a.md"),
            line,
            col_start: 0,
            col_end: 1,
            context: Vec::new(),
        })
        .collect();
    let _ = app.update(Message::VaultSearchDone(
        crate::vault_search::VaultResults {
            seq: app.vault_seq,
            hits,
            truncated: false,
        },
    ));
    // The new row under the parked pointer reports a hover with no motion.
    let _ = app.update(Message::VaultHover(2));
    assert_eq!(app.vault_cursor, 0, "Enter still opens the first match");
}

#[test]
fn narrowing_an_overlay_query_does_not_take_the_selection_from_a_still_pointer() {
    let mut app = App::default();
    let _ = app.update(Message::OpenCommandPalette);
    nudge_list_pointer(&mut app);

    let _ = app.update(Message::OverlayQueryChanged("t".into()));
    let _ = app.update(Message::OverlayHover(1));
    assert_eq!(app.overlay_selected, 0, "the first match stays selected");
}

#[test]
fn moving_inside_the_selected_row_publishes_no_row_hover() {
    let mut app = App::default();
    let _ = app.update(Message::OpenCommandPalette);
    let first = split_command_label(app.filtered_commands()[0].0).0;
    let mut ui = PointerUi::new();
    let row = ui
        .overlay_text_bounds(&app, first)
        .expect("first palette row");
    assert_eq!(app.overlay_selected, 0);

    let mut messages = ui.move_to(&app, center(row)).0;
    messages.extend(
        ui.move_to(&app, Point::new(row.center_x() + 4.0, row.center_y()))
            .0,
    );
    assert!(
        !has(&messages, |m| matches!(m, Message::OverlayHover(_))),
        "the selected row needs no hover report, got {messages:?}"
    );

    // Other rows still report hovers.
    let second = split_command_label(app.filtered_commands()[1].0).0;
    let row = ui
        .overlay_text_bounds(&app, second)
        .expect("second palette row");
    let messages = ui.move_to(&app, center(row)).0;
    assert!(has(&messages, |m| matches!(m, Message::OverlayHover(1))));
}

// --- Group 2: find bar, toasts, error card, edit focus, file drop -----------

mod group2 {
    use super::*;
    use crate::icon::ic;
    use iced::advanced::widget::operation::Focusable;
    use iced::keyboard;
    use std::time::Duration;

    /// Reports whether the focusable widget with `id` is focused.
    struct FocusProbe {
        id: Id,
        focused: Option<bool>,
    }

    impl Operation for FocusProbe {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }

        fn focusable(&mut self, id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
            if id == Some(&self.id) {
                self.focused = Some(state.is_focused());
            }
        }
    }

    impl PointerUi {
        pub(super) fn operate(&mut self, app: &App, operation: &mut dyn Operation) {
            let cache = self.cache.take().unwrap_or_default();
            let mut ui = UserInterface::build(app.view(), self.size, cache, &mut self.renderer);
            ui.operate(&self.renderer, operation);
            self.cache = Some(ui.into_cache());
        }

        pub(super) fn focused(&mut self, app: &App, id: &'static str) -> Option<bool> {
            let mut probe = FocusProbe {
                id: Id::new(id),
                focused: None,
            };
            self.operate(app, &mut probe);
            probe.focused
        }

        pub(super) fn type_char(&mut self, app: &App, c: char) -> Vec<Message> {
            let event = Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Character(c.to_string().into()),
                modified_key: keyboard::Key::Character(c.to_string().into()),
                physical_key: keyboard::key::Physical::Code(keyboard::key::Code::KeyX),
                location: keyboard::Location::Standard,
                modifiers: keyboard::Modifiers::default(),
                text: Some(c.to_string().into()),
                repeat: false,
            });
            self.send(app, Point::new(640.0, 400.0), vec![event]).0
        }
    }

    /// Messages and widget operations a task yields without waiting.
    fn drain(task: Task<Message>) -> (Vec<Message>, Vec<Box<dyn Operation>>) {
        use iced::futures::task::{noop_waker, Context, Poll};
        use iced::futures::StreamExt;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("tokio runtime");
        let _guard = runtime.enter();
        let mut messages = Vec::new();
        let mut operations = Vec::new();
        let Some(mut stream) = iced_runtime::task::into_stream(task) else {
            return (messages, operations);
        };
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        for _ in 0..8 {
            match stream.poll_next_unpin(&mut cx) {
                Poll::Ready(Some(iced_runtime::Action::Output(message))) => messages.push(message),
                Poll::Ready(Some(iced_runtime::Action::Widget(operation))) => {
                    operations.push(operation)
                }
                Poll::Ready(Some(_)) | Poll::Pending => {}
                Poll::Ready(None) => break,
            }
        }
        (messages, operations)
    }

    /// Like `apply`, and also runs the widget operations (focus, scroll) the
    /// tasks ask for on `ui`, as the runtime does after an update.
    pub(super) fn apply_ui(app: &mut App, ui: &mut PointerUi, messages: Vec<Message>) {
        for message in messages {
            let (follow_ups, operations) = drain(app.update(message));
            for mut operation in operations {
                ui.operate(app, operation.as_mut());
            }
            for follow_up in follow_ups {
                let (_, operations) = drain(app.update(follow_up));
                for mut operation in operations {
                    ui.operate(app, operation.as_mut());
                }
            }
        }
    }

    fn glyph_bounds(ui: &mut PointerUi, app: &App, glyph: char) -> Rectangle {
        ui.text_bounds(app, &glyph.to_string())
            .unwrap_or_else(|| panic!("glyph {glyph:?} is rendered"))
    }

    // --- 1. Edit mode focuses the editor ------------------------------------

    #[test]
    fn entering_edit_mode_focuses_the_editor_so_typing_works() {
        let mut app = long_doc_app();
        let mut ui = PointerUi::new();
        let _ = ui.move_to(&app, Point::new(640.0, 400.0));

        apply_ui(&mut app, &mut ui, vec![Message::ToggleViewMode]);
        assert!(app.editor.is_some(), "⌘E enters edit mode");
        assert_eq!(ui.focused(&app, "zen-editor"), Some(true));

        let typed = ui.type_char(&app, 'x');
        assert!(
            has(&typed, |m| matches!(m, Message::EditorAction(_))),
            "a key typed right after ⌘E reaches the editor: {typed:?}"
        );
    }

    #[test]
    fn clicking_the_margin_beside_the_editor_focuses_it() {
        let mut app = long_doc_app();
        let mut ui = PointerUi::new();
        let _ = ui.move_to(&app, Point::new(640.0, 400.0));
        let _ = app.update(Message::ToggleViewMode);
        assert!(app.editor.is_some());
        // The editor column is READING_MAX wide and centered; x = 24 is in
        // the empty left margin.
        assert!(WINDOW.width - READING_MAX > 48.0);

        let messages = ui.click(&app, Point::new(24.0, 400.0));
        apply_ui(&mut app, &mut ui, messages);

        assert_eq!(ui.focused(&app, "zen-editor"), Some(true));
    }

    // --- 2. Find bar keeps the input focused after chevron clicks -----------

    #[test]
    fn find_chevron_clicks_return_focus_to_the_search_input() {
        let mut app = long_doc_app();
        let mut ui = PointerUi::new();
        let _ = ui.move_to(&app, Point::new(640.0, 400.0));
        apply_ui(
            &mut app,
            &mut ui,
            vec![
                Message::ToggleSearch,
                Message::QueryChanged("Paragraph".into()),
            ],
        );
        assert_eq!(ui.focused(&app, "search-input"), Some(true));

        for glyph in [ic::CHEVRON_RIGHT, ic::CHEVRON_LEFT] {
            let bounds = glyph_bounds(&mut ui, &app, glyph);
            let messages = ui.click(&app, center(bounds));
            assert!(
                has(&messages, |m| matches!(m, Message::FindStepClicked(_))),
                "{messages:?}"
            );
            apply_ui(&mut app, &mut ui, messages);
            assert_eq!(
                ui.focused(&app, "search-input"),
                Some(true),
                "after the {glyph:?} chevron, typing and Enter go to the input"
            );
        }
    }

    // --- 3. Toasts: Save action, longer errors, close button ----------------

    #[test]
    fn dirty_guard_toast_has_a_save_button() {
        let mut app = long_doc_app();
        app.dirty = true;
        let _ = app.update(Message::Open(PathBuf::from("/tmp/pointer-other.md")));
        assert!(app.toast.as_ref().is_some_and(|t| t.text.contains("⌘S")));

        let mut ui = PointerUi::new();
        let save = ui
            .text_bounds(&app, "Save")
            .expect("the guard toast has a Save button");
        let messages = ui.click(&app, center(save));
        assert!(
            has(&messages, |m| matches!(m, Message::SaveFile)),
            "{messages:?}"
        );
    }

    #[test]
    fn error_toast_stays_longer_and_has_a_close_button() {
        let mut app = long_doc_app();
        app.pending_clipboard_copy = Some("/tmp/x".into());
        let _ = app.update(Message::ClipboardCopyChecked {
            expected: "/tmp/x".into(),
            actual: None,
        });
        let toast = app.toast.clone().expect("error toast");
        assert_eq!(toast_duration(&toast), Duration::from_secs(8));

        let mut ui = PointerUi::new();
        let close = ui
            .id_bounds(&app, "toast-close")
            .expect("an error toast has a close button");
        let messages = ui.click(&app, center(close));
        apply(&mut app, messages);
        assert!(app.toast.is_none(), "the close button hides the toast");
    }

    #[test]
    fn info_toast_stays_short_without_a_close_button() {
        let mut app = long_doc_app();
        app.pending_clipboard_copy = Some("/tmp/x".into());
        let _ = app.update(Message::ClipboardCopyChecked {
            expected: "/tmp/x".into(),
            actual: Some("/tmp/x".into()),
        });
        let toast = app.toast.clone().expect("info toast");
        assert_eq!(toast_duration(&toast), Duration::from_millis(1500));
        let mut ui = PointerUi::new();
        assert!(ui.id_bounds(&app, "toast-close").is_none());
    }

    // --- 4. Open-file error card: Dismiss and Reveal in Finder --------------

    #[test]
    fn open_error_card_offers_dismiss_and_reveal() {
        let mut app = long_doc_app();
        let missing = std::env::temp_dir()
            .join("rmdv-pointer-missing")
            .join("gone.md");
        let _ = app.update(Message::Open(missing.clone()));
        let generation = app.file_refresh_generation;
        let _ = app.update(Message::FileLoadCompleted {
            generation,
            result: Err("No such file or directory".into()),
        });
        assert!(app.error.is_some());

        let mut ui = PointerUi::new();
        let reveal = ui
            .text_bounds(&app, "Reveal in Finder")
            .expect("the error card has Reveal in Finder");
        let messages = ui.click(&app, center(reveal));
        assert!(
            has(
                &messages,
                |m| matches!(m, Message::RevealPath(p) if *p == missing)
            ),
            "Reveal targets the file that failed to open: {messages:?}"
        );

        let dismiss = ui
            .text_bounds(&app, "Dismiss")
            .expect("the error card has Dismiss");
        let messages = ui.click(&app, center(dismiss));
        apply(&mut app, messages);
        assert!(app.error.is_none());
        assert!(
            ui.text_bounds(&app, "Couldn't open file").is_none(),
            "the card is gone"
        );
        assert_eq!(
            app.file.as_deref(),
            Some(Path::new("/tmp/pointer-long.md")),
            "the open document shows again"
        );
    }

    #[test]
    fn reveal_target_falls_back_to_the_nearest_existing_folder() {
        let base = std::env::temp_dir();
        let missing = base.join("rmdv-pointer-missing").join("gone.md");
        assert_eq!(reveal_target(&missing), base);
        assert_eq!(reveal_target(&base), base);
    }

    // --- 5. Dropped files and folders ---------------------------------------

    fn drop_fixture(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("rmdv-drop-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("fixture dir");
        let a = dir.join("a.md");
        let b = dir.join("b.md");
        std::fs::write(&a, "# alpha\n").expect("a.md");
        std::fs::write(&b, "# beta\n").expect("b.md");
        let dir = canonicalize_existing_path(dir);
        (dir.clone(), dir.join("a.md"), dir.join("b.md"))
    }

    #[test]
    fn dropping_a_folder_opens_it_as_the_workspace() {
        let (dir, ..) = drop_fixture("folder");
        let mut app = App::default();
        apply(&mut app, vec![dropped_path_message(dir.clone())]);
        assert_eq!(app.workspace.as_deref(), Some(dir.as_path()));
        assert!(app.error.is_none(), "{:?}", app.error);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dropping_a_folder_over_settings_shows_the_workspace() {
        let (dir, ..) = drop_fixture("settings");
        let mut app = App::default();
        let _ = app.update(Message::OpenSettings);
        assert!(app.settings_open);
        apply(&mut app, vec![dropped_path_message(dir.clone())]);
        assert_eq!(app.workspace.as_deref(), Some(dir.as_path()));
        assert!(!app.settings_open, "the dropped folder is not hidden");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dropping_a_file_closes_an_open_overlay() {
        let (dir, a, b) = drop_fixture("overlay");
        let mut app = App::default();
        let _ = app.update(Message::FileLoaded(Ok((a, "# alpha\n".into()))));
        let _ = app.update(Message::OpenCommandPalette);
        assert_ne!(app.overlay, Overlay::None);
        let _ = app.update(dropped_path_message(b));
        assert_eq!(app.overlay, Overlay::None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dropping_with_unsaved_zen_edits_keeps_the_edits() {
        use iced::widget::text_editor::{Action, Edit, Motion};
        let (dir, a, b) = drop_fixture("dirty");
        let mut app = App::default();
        let _ = app.update(Message::FileLoaded(Ok((a.clone(), "# alpha\n".into()))));
        let _ = app.update(Message::ToggleViewMode);
        let _ = app.update(Message::EditorAction(Action::Move(Motion::DocumentEnd)));
        let _ = app.update(Message::EditorAction(Action::Edit(Edit::Insert('z'))));
        assert!(app.dirty);
        let edited = app.editor.as_ref().map(|ed| ed.text()).expect("editor");

        apply(&mut app, vec![dropped_path_message(b)]);
        assert_eq!(app.file.as_deref(), Some(a.as_path()), "the file stays");
        assert!(app.dirty);
        assert_eq!(
            app.editor.as_ref().map(|ed| ed.text()),
            Some(edited.clone())
        );
        assert!(
            app.toast
                .as_ref()
                .and_then(|t| t.action.as_ref())
                .is_some_and(|action| matches!(action.message, Message::SaveFile)),
            "the guard toast offers Save"
        );

        let (other, ..) = drop_fixture("dirty-folder");
        apply(&mut app, vec![dropped_path_message(other.clone())]);
        assert_eq!(app.workspace.as_deref(), Some(other.as_path()));
        assert_eq!(app.file.as_deref(), Some(a.as_path()));
        assert!(app.dirty);
        assert_eq!(app.editor.as_ref().map(|ed| ed.text()), Some(edited));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&other);
    }
}

// --- Group 3: sidebar, reader, tooltips, Full Mindmap exit ------------------

mod group3 {
    use super::*;

    /// A workspace with two files and a folder holding a third, opened with
    /// the Files sidebar showing.
    pub(super) fn tree_fixture(tag: &str) -> (App, PathBuf) {
        let dir = std::env::temp_dir().join(format!("rmdv-tree-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).expect("fixture dir");
        std::fs::write(dir.join("alpha.md"), "# Alpha\n").expect("alpha.md");
        std::fs::write(dir.join("beta.md"), "# Beta\n").expect("beta.md");
        std::fs::write(dir.join("sub").join("gamma.md"), "# Gamma\n").expect("gamma.md");
        let dir = canonicalize_existing_path(dir);
        let mut app = App::default();
        app.set_workspace(dir.clone(), true);
        app.sidebar_tab = SidebarTab::Files;
        (app, dir)
    }

    pub(super) fn tree_index(app: &App, path: &Path) -> usize {
        let rows = tree::flatten_with_files(
            app.workspace_tree.as_ref().unwrap(),
            &app.workspace_sidebar_files,
            &app.expanded,
        );
        rows.iter()
            .position(|row| row.node.path() == path)
            .expect("row is in the tree")
    }

    // --- 1. Row clicks move the tree and outline cursors ---------------------

    #[test]
    fn clicking_a_tree_file_moves_the_cursor_and_focus_to_it() {
        let (mut app, dir) = tree_fixture("click-file");
        let beta = dir.join("beta.md");
        let _ = app.update(Message::TreeMove(1));
        assert!(app.tree_cursor_shown, "arrow keys show the cursor");
        let before = app.tree_cursor;
        let mut ui = PointerUi::new();
        let row = ui.text_bounds(&app, "beta.md").expect("beta row");
        assert!(row.x < app.sidebar_width);

        let messages = ui.click(&app, center(row));
        apply(&mut app, messages);

        assert_ne!(before, tree_index(&app, &beta));
        assert_eq!(
            app.tree_cursor,
            tree_index(&app, &beta),
            "cursor follows the click"
        );
        assert!(!app.tree_cursor_shown, "a click hides the keyboard cursor");
        // The click's open completes; Reveal/Copy follow the open file.
        let _ = app.update(Message::FileLoaded(Ok((beta.clone(), "# Beta\n".into()))));
        assert_eq!(app.focused_file_path(), Some(beta), "Reveal/Copy act on it");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clicking_a_tree_folder_moves_the_cursor_and_toggles_it() {
        let (mut app, dir) = tree_fixture("click-folder");
        let sub = dir.join("sub");
        let alpha = dir.join("alpha.md");
        // Keyboard cursor on a file: Reveal would act on it.
        app.tree_cursor = tree_index(&app, &alpha);
        app.tree_cursor_shown = true;
        let mut ui = PointerUi::new();
        let row = ui.text_bounds(&app, "sub").expect("folder row");

        let messages = ui.click(&app, center(row));
        apply(&mut app, messages);

        assert!(app.expanded.contains(&sub), "the folder opens");
        assert_eq!(app.tree_cursor, tree_index(&app, &sub));
        assert!(!app.tree_cursor_shown);
        assert_ne!(
            app.focused_file_path(),
            Some(alpha),
            "the old cursor row no longer drives Reveal/Copy"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clicking_an_outline_row_moves_the_outline_cursor() {
        let (mut app, dir) = tree_fixture("click-outline");
        let _ = app.update(Message::FileLoaded(Ok((
            dir.join("alpha.md"),
            "# Outline One\n\ntext\n\n## Outline Two\n\ntext\n\n## Outline Three\n\ntext\n".into(),
        ))));
        app.sidebar_tab = SidebarTab::Outline;
        assert_eq!(app.outline_cursor, 0);
        let mut ui = PointerUi::new();
        let row = ui.text_bounds(&app, "Outline Three").expect("outline row");
        assert!(
            row.x < app.sidebar_width,
            "the sidebar row, not the heading"
        );

        let messages = ui.click(&app, center(row));
        apply(&mut app, messages);

        assert_eq!(app.outline_cursor, 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- 4. Icon-button tooltips name the action and its shortcut ----------

    /// Tooltip labels built for the current `view`.
    pub(super) fn tips(ui: &mut PointerUi, app: &App) -> Vec<String> {
        let _ = tip_log::take();
        let _ = ui.text_bounds(app, "\u{0}");
        tip_log::take()
    }

    pub(super) fn assert_tips(tips: &[String], expected: &[&str]) {
        for label in expected {
            assert!(
                tips.iter().any(|tip| tip == label),
                "missing tooltip {label:?} in {tips:?}"
            );
        }
    }

    #[test]
    fn action_tip_label_joins_action_and_shortcut() {
        assert_eq!(
            tip_log::label("Command Palette", Some("⌘⇧P")),
            "Command Palette  ⌘⇧P"
        );
        assert_eq!(tip_log::label("Copy Code", None), "Copy Code");
    }

    #[test]
    fn sidebar_header_tooltips_name_their_actions() {
        let (app, dir) = tree_fixture("tips-sidebar");
        let mut ui = PointerUi::new();
        assert_tips(
            &tips(&mut ui, &app),
            &[
                "Command Palette  ⌘⇧P",
                "Settings  ⌘,",
                "Hide Sidebar  ⌘B",
                "Switch Tab  ← →",
            ],
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_bar_tooltips_name_their_actions() {
        let mut app = long_doc_app();
        let _ = app.update(Message::ToggleSearch);
        let mut ui = PointerUi::new();
        assert_tips(
            &tips(&mut ui, &app),
            &["Previous Match  ⇧↩", "Next Match  ↩", "Close Find  Esc"],
        );
    }

    #[test]
    fn picker_and_viewer_tooltips_name_their_actions() {
        let dir = std::env::temp_dir().join(format!("rmdv-tips-picker-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("child")).unwrap();
        let mut app = App::default();
        app.overlay = Overlay::FolderPicker;
        app.picker = Some(Picker::new(Some(dir.clone()), PickerMode::OpenAny, false));
        let mut ui = PointerUi::new();
        assert_tips(&tips(&mut ui, &app), &["Home Folder", "Parent Folder  ←"]);
        let _ = std::fs::remove_dir_all(&dir);

        let mut app = long_doc_app();
        let _ = app.update(Message::OpenImageZoom("/tmp/rmdv-tips.png".into()));
        assert_eq!(app.overlay, Overlay::ImageZoom);
        let mut ui = PointerUi::new();
        assert_tips(&tips(&mut ui, &app), &["Close  Esc"]);

        let mut app = App::default();
        let _ = app.update(Message::OpenSettings);
        let mut ui = PointerUi::new();
        assert_tips(&tips(&mut ui, &app), &["Close Settings  Esc"]);
    }

    #[test]
    fn code_copy_button_has_a_tooltip() {
        let mut app = App::default();
        let _ = app.update(Message::FileLoaded(Ok((
            PathBuf::from("/tmp/pointer-code.md"),
            "# Code\n\n```rust\nfn main() {}\n```\n".into(),
        ))));
        let mut ui = PointerUi::new();
        assert_tips(&tips(&mut ui, &app), &["Copy Code"]);
    }

    // --- 2. Heading rows: only the chevron folds -----------------------------

    fn heading_app() -> App {
        let mut app = App::default();
        let _ = app.update(Message::FileLoaded(Ok((
            PathBuf::from("/tmp/pointer-heading.md"),
            "# Fold Me\n\nbody text\n\n## Next Part\n\nmore text\n".into(),
        ))));
        app
    }

    fn glyph(code: char) -> String {
        code.to_string()
    }

    /// Bounds of the first heading's fold chevron (drawn even while hidden).
    fn chevron_bounds(ui: &mut PointerUi, app: &App, code: char) -> Rectangle {
        ui.text_bounds(app, &glyph(code)).expect("fold chevron")
    }

    #[test]
    fn clicking_blank_heading_space_does_not_fold() {
        let mut app = heading_app();
        let mut ui = PointerUi::new();
        let chevron = chevron_bounds(&mut ui, &app, crate::icon::ic::CHEVRON_DOWN);
        // Between the short heading text and the chevron at the row's end.
        let blank = Point::new(chevron.x - 150.0, chevron.center_y());
        let messages = ui.move_to(&app, blank).0;
        apply(&mut app, messages);

        let (_, cursor) = ui.move_to(&app, blank);
        assert_ne!(
            cursor,
            mouse::Interaction::Pointer,
            "blank heading space does not look clickable"
        );
        let messages = ui.click(&app, blank);
        assert!(
            !has(&messages, |m| matches!(m, Message::ToggleFold(_))),
            "a click right of the heading text must not fold: {messages:?}"
        );
    }

    #[test]
    fn heading_chevron_has_a_24px_hit_target_and_a_tooltip() {
        let mut app = heading_app();
        let mut ui = PointerUi::new();
        let chevron = chevron_bounds(&mut ui, &app, crate::icon::ic::CHEVRON_DOWN);
        // Hover shows the chevron.
        let messages = ui.move_to(&app, chevron.center()).0;
        apply(&mut app, messages);
        assert!(
            tips(&mut ui, &app).iter().any(|t| t == "Collapse section"),
            "the chevron names its action"
        );

        // 10 px off the glyph's center, inside a 24x24 target.
        let near = Point::new(chevron.center_x() - 10.0, chevron.center_y() + 10.0);
        let (_, cursor) = ui.move_to(&app, near);
        assert_eq!(cursor, mouse::Interaction::Pointer);
        let messages = ui.click(&app, near);
        assert!(
            has(&messages, |m| matches!(m, Message::ToggleFold(_))),
            "the chevron folds: {messages:?}"
        );
        apply(&mut app, messages);
        assert!(
            tips(&mut ui, &app).iter().any(|t| t == "Expand section"),
            "a folded heading offers Expand"
        );
    }

    // --- 3. Reader images show the zoom-in cursor ----------------------------

    #[test]
    fn reader_images_show_the_zoom_in_cursor() {
        let url = "https://example.com/pointer.png";
        let mut app = App::default();
        let _ = app.update(Message::FileLoaded(Ok((
            PathBuf::from("/tmp/pointer-image.md"),
            format!("Intro\n\n![a picture]({url})\n\nOutro\n"),
        ))));
        let handle = iced::widget::image::Handle::from_rgba(200, 100, vec![200u8; 200 * 100 * 4]);
        app.image_cache
            .insert(url.to_string(), ImageState::Loaded(handle));
        let image_id = app
            .ast
            .iter()
            .find(|(_, block)| matches!(block, crate::ast::Block::Image { .. }))
            .map(|(id, _)| *id)
            .expect("image block");
        let mut ui = PointerUi::new();
        let (bounds, _) = ui
            .find(
                &app,
                Find {
                    text: None,
                    contains: None,
                    id: Some(crate::render::block_anchor_id(image_id)),
                    found: None,
                    found_text: None,
                },
            )
            .expect("image block anchor");
        let over = Point::new(bounds.x + 20.0, bounds.center_y());

        let (_, cursor) = ui.move_to(&app, over);
        assert_eq!(cursor, mouse::Interaction::ZoomIn);
        let messages = ui.click(&app, over);
        assert!(has(&messages, |m| matches!(
            m,
            Message::OpenImageZoom(u) if u == url
        )));
    }

    // --- 5. Full Mindmap Back, parent-folder, and panel buttons ----------------

    /// The tree fixture in Full Mindmap with its workspace graph loaded.
    pub(super) fn full_mindmap_app(tag: &str) -> (App, PathBuf) {
        let (mut app, dir) = tree_fixture(tag);
        let _ = app.update(Message::ToggleFullMindmap);
        // The open workspace's snapshot may be reused without a new index.
        if let Some(request) = app
            .full_mindmap
            .as_ref()
            .and_then(|full| full.pending_workspace_load.clone())
        {
            let snapshot = tree::build_workspace(&request.path, app.show_hidden).unwrap();
            let _ = app.update(Message::FullMindmapWorkspaceLoaded {
                request: request.clone(),
                result: Ok((request.path.clone(), snapshot)),
            });
        }
        assert!(app.full_mindmap_graph().is_some(), "graph is ready");
        (app, dir)
    }

    #[test]
    fn full_mindmap_back_pill_exits_with_and_without_a_graph() {
        // Before any graph exists (indexing, or the index failed).
        let (mut app, dir) = tree_fixture("fmm-back-no-graph");
        let _ = app.update(Message::ToggleFullMindmap);
        assert!(app.full_mindmap.is_some());
        app.workspace_tree = None;
        *app.full_mindmap.as_ref().unwrap().layout_cache.borrow_mut() = None;
        assert!(app.full_mindmap_graph().is_none(), "no graph to draw");
        let mut ui = PointerUi::new();
        let back = ui
            .text_bounds(&app, "Back")
            .expect("Back pill without a graph");
        let messages = ui.click(&app, center(back));
        assert!(has(&messages, |m| matches!(m, Message::ExitFullMindmap)));
        apply(&mut app, messages);
        assert!(app.full_mindmap.is_none(), "Back leaves Full Mindmap");
        let _ = std::fs::remove_dir_all(&dir);

        // With the graph drawn.
        let (mut app, dir) = full_mindmap_app("fmm-back-ready");
        let mut ui = PointerUi::new();
        let back = ui.text_bounds(&app, "Back").expect("Back pill");
        assert!(
            back.x < 200.0 && back.y < 120.0,
            "the pill sits top-left: {back:?}"
        );
        let messages = ui.click(&app, center(back));
        apply(&mut app, messages);
        assert!(app.full_mindmap.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn full_mindmap_parent_and_panel_buttons_send_their_messages() {
        let (mut app, dir) = full_mindmap_app("fmm-buttons");
        let mut ui = PointerUi::new();
        assert_tips(
            &tips(&mut ui, &app),
            &["Back to Reader  Esc", "Parent Folder", "Show Panel  ⌘⌥B"],
        );

        let parent = ui
            .text_bounds(&app, &glyph(crate::icon::ic::FOLDER_UP))
            .expect("parent folder button");
        let messages = ui.click(&app, center(parent));
        assert!(
            has(&messages, |m| matches!(
                m,
                Message::FullMindmapWorkspaceParent
            )),
            "{messages:?}"
        );

        let panel = ui
            .text_bounds(&app, &glyph(crate::icon::ic::PANEL_RIGHT_OPEN))
            .expect("panel toggle button");
        let messages = ui.click(&app, center(panel));
        apply(&mut app, messages);
        assert!(app.full_mindmap.as_ref().unwrap().panel_open);
        assert_tips(&tips(&mut ui, &app), &["Hide Panel  ⌘⌥B"]);
        assert!(ui
            .text_bounds(&app, &glyph(crate::icon::ic::PANEL_RIGHT_CLOSE))
            .is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn full_mindmap_nav_layer_leaves_the_canvas_its_events() {
        let (app, dir) = full_mindmap_app("fmm-pass-through");
        let mut ui = PointerUi::new();
        let back = ui.text_bounds(&app, "Back").expect("Back pill");
        // Beside and below the pill, over blank canvas.
        for probe in [
            Point::new(back.x + 260.0, back.center_y()),
            Point::new(back.x, back.y + 80.0),
        ] {
            let (_, cursor) = ui.move_to(&app, probe);
            assert_eq!(
                cursor,
                mouse::Interaction::Grab,
                "the canvas still owns {probe:?}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- Review fixes ------------------------------------------------------

    #[test]
    fn a_hidden_tree_cursor_leaves_reveal_and_copy_on_the_open_file() {
        let (mut app, dir) = tree_fixture("hidden-cursor");
        let alpha = dir.join("alpha.md");
        let beta = dir.join("beta.md");
        let _ = app.update(Message::TreeRowClicked {
            path: alpha.clone(),
            is_dir: false,
        });
        assert_eq!(app.tree_cursor, tree_index(&app, &alpha));
        assert!(!app.tree_cursor_shown);
        // beta opens some other way (finder, link, Quick Slot).
        let _ = app.update(Message::FileLoaded(Ok((beta.clone(), "# Beta\n".into()))));
        assert_eq!(app.file.as_deref(), Some(beta.as_path()));
        assert!(app.sidebar_open && app.sidebar_tab == SidebarTab::Files);

        assert_eq!(
            app.focused_file_path(),
            Some(beta),
            "Reveal/Copy act on the open file, not the hidden cursor row"
        );

        // Copy Tree Path shows the row it copied.
        let _ = app.update(Message::CopyTreePath);
        assert!(app.tree_cursor_shown, "the copied row is shown");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reader_tooltips_wait_before_they_open() {
        let mut app = App::default();
        let _ = app.update(Message::FileLoaded(Ok((
            PathBuf::from("/tmp/pointer-tip-delay.md"),
            "# Heading\n\ntext\n\n```rust\nfn main() {}\n```\n".into(),
        ))));
        let mut ui = PointerUi::new();
        let _ = tip_log::take_delays();
        let _ = ui.text_bounds(&app, "\u{0}");
        let delays = tip_log::take_delays();
        for label in ["Copy Code", "Collapse section"] {
            let delay = delays
                .iter()
                .find(|(tip, _)| tip == label)
                .map(|(_, delay)| *delay)
                .unwrap_or_else(|| panic!("missing tooltip {label:?} in {delays:?}"));
            assert!(
                delay >= std::time::Duration::from_millis(300),
                "{label} opens after a sweep-proof delay, got {delay:?}"
            );
        }
        // Chrome icon buttons keep their instant tooltips.
        assert!(delays
            .iter()
            .filter(|(tip, _)| tip.starts_with("Hide Sidebar") || tip.starts_with("Settings"))
            .all(|(_, delay)| delay.is_zero()));
    }
}

// --- P2: the top toolbar ------------------------------------------------------

mod toolbar;

// --- P2 group 2: window and lifecycle ---------------------------------------

mod window;

// --- P3: navigation history ---------------------------------------------------

mod history;

// --- P3 group 2: links --------------------------------------------------------

mod links;

// --- Polish: sidebar peek at the left window edge --------------------------

mod sidebar_peek;

// --- P4: the mind map pointer model -------------------------------------------

mod mindmap;

// --- P5: the image and diagram viewer ---------------------------------------

mod viewer;

// --- P5: reader and editor gestures -------------------------------------------

mod gestures;

// --- P6: the shared context menu --------------------------------------------

mod context_menu;

// --- P6: the file tree context menu -----------------------------------------

mod tree_menu;

// --- P6: file operations from the file tree --------------------------------

mod tree_ops;

// --- P6: double clicks in the sidebar -----------------------------------------

mod tree_double_click;

// --- P6: the reader's context menu -------------------------------------------

mod reader_menu;

// --- P6: the Zen editor's context menu -----------------------------------------

mod editor_menu;

// --- P6: mind map context menus ---------------------------------------------

mod mindmap_menu;

// --- P6: outline, vault, and finder context menus -----------------------------

mod list_menus;

// --- P6: the Quick Slot context menu ----------------------------------------

mod slot_menu;

// --- P7: reader selection with the pointer ------------------------------------

mod reader_select;

// --- P7: reader affordances (copy buttons, checkboxes, scrollbar, tables) ----

mod reader_affordances;

// --- P7: the outline follows the reader -----------------------------------

mod outline_spy;

// --- P7: the sidebar divider ------------------------------------------------

mod sidebar_divider;

// --- P7: the Quick Slots popover --------------------------------------------

mod slot_popover;

// --- P7: hit targets and button states ----------------------------------------

mod hit_targets;

// --- P7: footer pill, shortcut sheet, palette caps, and clear buttons -----

mod chrome_affordances;

// --- P8: window frame persistence ------------------------------------------

mod window_frame;
