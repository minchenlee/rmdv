//! The shared context menu (MDV-048 P6): where it opens, what closes it,
//! what it swallows, the keyboard, and hover. The menu is driven through
//! `App::view` in the headless UI; the surfaces that open real menus (the
//! file tree) have their own tests.

use super::*;

/// The window-edge margin `context_menu_origin` keeps.
const MARGIN: f32 = 6.0;

fn entries() -> Vec<MenuEntry> {
    vec![
        MenuEntry::item("Show Outline", Message::SetSidebarTab(SidebarTab::Outline))
            .shortcut(Some("→")),
        MenuEntry::disabled("Nothing Here"),
        MenuEntry::Separator,
        MenuEntry::item("Keyboard Shortcuts", Message::ToggleShortcuts),
    ]
}

/// A right press at `at` through the view, then a menu opened by it.
fn open_at(app: &mut App, ui: &mut PointerUi, at: Point) {
    let messages = ui
        .send(
            app,
            at,
            vec![Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Right,
            ))],
        )
        .0;
    assert!(
        has(
            &messages,
            |m| matches!(m, Message::PointerRightPress(p) if *p == at)
        ),
        "the root reports the right press in window coordinates: {messages:?}"
    );
    apply(app, messages);
    app.open_context_menu(entries(), None);
}

fn menu_bounds(ui: &mut PointerUi, app: &App) -> Rectangle {
    ui.id_bounds(app, CONTEXT_MENU_ID)
        .expect("the context menu shows")
}

fn press(button: mouse::Button) -> Vec<Event> {
    vec![
        Event::Mouse(mouse::Event::ButtonPressed(button)),
        Event::Mouse(mouse::Event::ButtonReleased(button)),
    ]
}

#[test]
fn a_context_menu_opens_at_the_right_press() {
    let mut app = long_doc_app();
    let mut ui = PointerUi::new();
    let at = Point::new(400.0, 200.0);
    open_at(&mut app, &mut ui, at);
    let bounds = menu_bounds(&mut ui, &app);
    assert_eq!((bounds.x, bounds.y), (401.0, 201.0), "just below-right");
    let size = app.context_menu.as_ref().unwrap().size();
    assert_eq!(
        (bounds.width, bounds.height),
        (size.width, size.height),
        "the drawn menu has the size placement assumes"
    );
}

#[test]
fn a_context_menu_flips_and_stays_inside_the_window() {
    let mut app = long_doc_app();
    let mut ui = PointerUi::new();
    let at = Point::new(WINDOW.width - 10.0, WINDOW.height - 10.0);
    open_at(&mut app, &mut ui, at);
    let bounds = menu_bounds(&mut ui, &app);
    assert!(
        bounds.x + bounds.width <= at.x && bounds.y + bounds.height <= at.y,
        "flipped to the left and above the press: {bounds:?}"
    );
    assert!(bounds.x >= MARGIN && bounds.y >= MARGIN);

    // A window too small for the menu: it keeps to the margins.
    let menu = iced::Size::new(232.0, 300.0);
    let small = iced::Size::new(200.0, 150.0);
    let origin = context_menu_origin(Point::new(100.0, 100.0), menu, small);
    assert_eq!(origin, Point::new(MARGIN, MARGIN));
    // A press near the left/top edge never pushes the menu outside.
    let origin = context_menu_origin(Point::new(0.0, 0.0), menu, WINDOW);
    assert!(origin.x >= MARGIN && origin.y >= MARGIN, "{origin:?}");
    // Room below and right: no flip.
    let origin = context_menu_origin(Point::new(50.0, 60.0), menu, WINDOW);
    assert_eq!(origin, Point::new(51.0, 61.0));
}

#[test]
fn a_press_outside_the_menu_closes_it_and_goes_no_further() {
    let mut app = long_doc_app();
    let mut ui = PointerUi::new();
    let keyboard_button = ui
        .text_bounds(&app, &crate::icon::ic::KEYBOARD.to_string())
        .expect("the keyboard shortcuts button");
    for button in [
        mouse::Button::Left,
        mouse::Button::Right,
        mouse::Button::Middle,
    ] {
        open_at(&mut app, &mut ui, Point::new(400.0, 200.0));
        let target = center(keyboard_button);
        let _ = ui.move_to(&app, target);
        let messages = ui.send(&app, target, press(button)).0;
        assert!(
            has(&messages, |m| matches!(m, Message::ContextMenuClose)),
            "{button:?}: {messages:?}"
        );
        assert!(
            !has(&messages, |m| matches!(m, Message::ToggleShortcuts)),
            "{button:?}: the press stops at the menu's backdrop: {messages:?}"
        );
        apply(&mut app, messages);
        assert!(app.context_menu.is_none(), "{button:?} closes the menu");
        assert_eq!(app.overlay, Overlay::None);
    }
}

#[test]
fn the_wheel_hover_and_cursor_stop_at_an_open_menu() {
    let mut app = long_doc_app();
    let mut ui = PointerUi::new();
    // Reader text, left of where the menu opens.
    let over_text = Point::new(500.0, 300.0);
    let (_, reader_cursor) = ui.move_to(&app, over_text);
    assert_ne!(reader_cursor, mouse::Interaction::Idle);
    let messages = ui.wheel(&app, over_text);
    assert!(
        has(&messages, |m| matches!(m, Message::BodyScrolled(_))),
        "the wheel scrolls the reader before the menu opens: {messages:?}"
    );

    open_at(&mut app, &mut ui, Point::new(900.0, 500.0));
    let (_, cursor) = ui.move_to(&app, over_text);
    assert_eq!(
        cursor,
        mouse::Interaction::Idle,
        "no reader cursor under the menu"
    );
    let messages = ui.wheel(&app, over_text);
    assert!(
        !has(&messages, |m| matches!(m, Message::BodyScrolled(_))),
        "the reader does not scroll: {messages:?}"
    );
    assert!(app.context_menu.is_some(), "the wheel keeps the menu open");
    // The wheel over the menu itself stops there as well.
    let menu = menu_bounds(&mut ui, &app);
    let messages = ui.wheel(&app, center(menu));
    assert!(!has(&messages, |m| matches!(m, Message::BodyScrolled(_))));
}

#[test]
fn context_menu_keys_move_choose_and_close() {
    use iced::keyboard::{key::Named, Key};
    assert!(matches!(
        context_menu_key_message(&Key::Named(Named::ArrowDown)),
        Message::ContextMenuMove(1)
    ));
    assert!(matches!(
        context_menu_key_message(&Key::Named(Named::ArrowUp)),
        Message::ContextMenuMove(-1)
    ));
    assert!(matches!(
        context_menu_key_message(&Key::Named(Named::Enter)),
        Message::ContextMenuActivate
    ));
    assert!(matches!(
        context_menu_key_message(&Key::Named(Named::Escape)),
        Message::ContextMenuClose
    ));
    assert!(matches!(
        context_menu_key_message(&Key::Character("j".into())),
        Message::Noop
    ));

    let mut app = long_doc_app();
    app.right_press_at = Some(Point::new(300.0, 300.0));
    app.open_context_menu(entries(), None);
    let highlighted = |app: &App| app.context_menu.as_ref().and_then(|m| m.highlighted);
    // Down from nothing: the first item; the disabled item and the
    // separator are skipped; the end wraps to the start.
    let _ = app.update(Message::ContextMenuMove(1));
    assert_eq!(highlighted(&app), Some(0));
    let _ = app.update(Message::ContextMenuMove(1));
    assert_eq!(highlighted(&app), Some(3));
    let _ = app.update(Message::ContextMenuMove(1));
    assert_eq!(highlighted(&app), Some(0));
    let _ = app.update(Message::ContextMenuMove(-1));
    assert_eq!(highlighted(&app), Some(3));

    // Enter runs the highlighted item after the menu closes.
    apply(&mut app, vec![Message::ContextMenuActivate]);
    assert!(app.context_menu.is_none());
    assert_eq!(app.overlay, Overlay::Shortcuts, "the item ran");
    let _ = app.update(Message::CloseOverlay);

    // Up from nothing: the last item. Esc closes without running it.
    app.open_context_menu(entries(), None);
    let _ = app.update(Message::ContextMenuMove(-1));
    assert_eq!(highlighted(&app), Some(3));
    let _ = app.update(Message::ContextMenuClose);
    assert!(app.context_menu.is_none());
    assert_eq!(app.overlay, Overlay::None);

    // Enter with nothing highlighted closes.
    app.open_context_menu(entries(), None);
    apply(&mut app, vec![Message::ContextMenuActivate]);
    assert!(app.context_menu.is_none());
    assert_eq!(app.overlay, Overlay::None);
}

#[test]
fn clicking_an_item_runs_it_and_a_disabled_item_does_nothing() {
    let mut app = long_doc_app();
    let mut ui = PointerUi::new();
    open_at(&mut app, &mut ui, Point::new(400.0, 200.0));
    let disabled = ui.text_bounds(&app, "Nothing Here").expect("disabled item");
    let messages = ui.click(&app, center(disabled));
    assert!(
        !has(&messages, |m| matches!(
            m,
            Message::ContextMenuClose | Message::ContextMenuChoose(_)
        )),
        "{messages:?}"
    );
    apply(&mut app, messages);
    assert!(app.context_menu.is_some(), "a disabled item keeps the menu");

    let item = ui
        .text_bounds(&app, "Keyboard Shortcuts")
        .expect("enabled item");
    let messages = ui.click(&app, center(item));
    assert!(has(&messages, |m| matches!(
        m,
        Message::ContextMenuChoose(3)
    )));
    apply(&mut app, messages);
    assert!(app.context_menu.is_none(), "choosing closes the menu");
    assert_eq!(app.overlay, Overlay::Shortcuts, "and runs the item");
}

#[test]
fn hover_highlights_an_item_only_after_the_pointer_moves() {
    let mut app = long_doc_app();
    let mut ui = PointerUi::new();
    open_at(&mut app, &mut ui, Point::new(400.0, 200.0));
    let first = ui.text_bounds(&app, "Show Outline").expect("first item");
    let highlighted = |app: &App| app.context_menu.as_ref().and_then(|m| m.highlighted);

    // The first report over the menu is where the pointer rests.
    let messages = ui.move_to(&app, center(first)).0;
    apply(&mut app, messages);
    assert_eq!(
        highlighted(&app),
        None,
        "a still pointer highlights nothing"
    );

    let messages = ui
        .move_to(&app, Point::new(center(first).x + 3.0, center(first).y))
        .0;
    apply(&mut app, messages);
    assert_eq!(highlighted(&app), Some(0), "motion highlights the item");

    // The keyboard moves on; the pointer, still, does not take it back.
    let _ = app.update(Message::ContextMenuMove(1));
    assert_eq!(highlighted(&app), Some(3));
    let messages = ui
        .send(
            &app,
            Point::new(center(first).x + 3.0, center(first).y),
            vec![],
        )
        .0;
    apply(&mut app, messages);
    assert_eq!(highlighted(&app), Some(3));
}

#[test]
fn moves_in_the_highlighted_item_and_the_wheel_publish_nothing() {
    let mut app = long_doc_app();
    let mut ui = PointerUi::new();
    open_at(&mut app, &mut ui, Point::new(400.0, 200.0));
    let first = center(ui.text_bounds(&app, "Show Outline").expect("first item"));
    for x in [first.x, first.x + 3.0] {
        let messages = ui.move_to(&app, Point::new(x, first.y)).0;
        apply(&mut app, messages);
    }
    assert_eq!(
        app.context_menu.as_ref().and_then(|m| m.highlighted),
        Some(0)
    );

    // Each message rebuilds the whole view; the highlighted item stays quiet.
    let messages = ui.move_to(&app, Point::new(first.x + 6.0, first.y)).0;
    assert!(messages.is_empty(), "{messages:?}");
    let wheel = || {
        vec![Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Pixels { x: 0.0, y: -120.0 },
        })]
    };
    let menu = center(menu_bounds(&mut ui, &app));
    let over_menu = ui.send(&app, menu, wheel()).0;
    assert!(over_menu.is_empty(), "{over_menu:?}");
    let over_backdrop = ui.send(&app, Point::new(900.0, 600.0), wheel()).0;
    assert!(over_backdrop.is_empty(), "{over_backdrop:?}");
    assert!(app.context_menu.is_some());
}

#[test]
fn a_context_menu_closes_on_unfocus_resize_and_view_changes() {
    let mut app = long_doc_app();
    let reopen = |app: &mut App| {
        app.right_press_at = Some(Point::new(300.0, 300.0));
        app.open_context_menu(entries(), None);
    };
    let window = iced::window::Id::unique();
    let cases: Vec<(&str, Message)> = vec![
        ("unfocus", Message::WindowUnfocused(window)),
        (
            "resize",
            Message::WindowResized(window, iced::Size::new(900.0, 700.0)),
        ),
        ("overlay", Message::OpenCommandPalette),
        ("view mode", Message::ToggleViewMode),
        ("sidebar", Message::ToggleSidebar),
    ];
    for (name, message) in cases {
        reopen(&mut app);
        let _ = app.update(message);
        assert!(app.context_menu.is_none(), "{name} closes the menu");
        // Back to the reader for the next case.
        let _ = app.update(Message::CloseOverlay);
        if app.view_mode != ViewMode::Rendered {
            let _ = app.update(Message::ToggleViewMode);
        }
    }
    // Messages that change nothing under the menu keep it open.
    reopen(&mut app);
    let _ = app.update(Message::Noop);
    let _ = app.update(Message::ScrollerTick);
    let _ = app.update(Message::PointerRightPress(Point::new(10.0, 10.0)));
    assert!(app.context_menu.is_some());
}
