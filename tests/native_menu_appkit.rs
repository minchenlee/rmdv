//! The native menu bar against real AppKit menus (MDV-048 P8). AppKit
//! menus need the main thread, so this target has no test harness: `main`
//! runs each check. Nothing here opens a window or installs the menu.

#[cfg(target_os = "macos")]
mod appkit {
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{msg_send, ClassType, MainThreadMarker};
    use objc2_app_kit::{
        NSApplication, NSControlStateValueOff, NSControlStateValueOn, NSEvent,
        NSEventModifierFlags, NSEventType, NSMenu, NSMenuDelegate,
    };
    use objc2_foundation::{NSPoint, NSString};
    use rmdv::native_menu::{self, Action, Entry, Menu, Native, Role, Shortcut, State};

    const SIDEBAR: u32 = 5;
    const SLOT: u32 = 6;

    fn spec() -> Vec<Menu> {
        let command = |title: &str, shortcut, tag| Entry::Item {
            title: title.to_string(),
            shortcut: Some(shortcut),
            action: Action::Command(tag),
        };
        vec![
            Menu {
                title: "rmdv".into(),
                role: Role::Plain,
                entries: vec![Entry::Submenu(Menu {
                    title: "Services".into(),
                    role: Role::Services,
                    entries: Vec::new(),
                })],
            },
            Menu {
                title: "View".into(),
                role: Role::Plain,
                entries: vec![
                    command("Toggle Sidebar", Shortcut::cmd('b'), SIDEBAR),
                    Entry::Submenu(Menu {
                        title: "Quick Slots".into(),
                        role: Role::Plain,
                        entries: vec![command("Slot 1", Shortcut::cmd('1'), SLOT)],
                    }),
                    Entry::Item {
                        title: "Enter Full Screen".into(),
                        shortcut: Some(Shortcut::cmd_control('f')),
                        action: Action::Native(Native::ToggleFullScreen),
                    },
                ],
            },
            Menu {
                title: "File".into(),
                role: Role::Plain,
                entries: vec![Entry::Submenu(Menu {
                    title: "Open Recent".into(),
                    role: Role::Recent,
                    entries: Vec::new(),
                })],
            },
        ]
    }

    fn key(chars: &str, flags: NSEventModifierFlags, code: u16) -> Retained<NSEvent> {
        let chars = NSString::from_str(chars);
        let null: *const AnyObject = std::ptr::null();
        let event: Option<Retained<NSEvent>> = unsafe {
            msg_send![
                NSEvent::class(),
                keyEventWithType: NSEventType::KeyDown,
                location: NSPoint::new(0.0, 0.0),
                modifierFlags: flags,
                timestamp: 0.0f64,
                windowNumber: 0isize,
                context: null,
                characters: &*chars,
                charactersIgnoringModifiers: &*chars,
                isARepeat: false,
                keyCode: code
            ]
        };
        event.expect("key event")
    }

    fn submenu(main: &NSMenu, index: isize) -> Retained<NSMenu> {
        main.itemAtIndex(index)
            .and_then(|item| item.submenu())
            .expect("submenu")
    }

    fn open(menu: &NSMenu) {
        let delegate = menu.delegate().expect("delegate");
        delegate.menuWillOpen(menu);
    }

    fn close(menu: &NSMenu) {
        let delegate = menu.delegate().expect("delegate");
        delegate.menuDidClose(menu);
    }

    /// App items show their keys but never take them: the key goes on to
    /// the window, where the app's key handler maps it as before.
    fn app_items_leave_their_keys_to_the_window(mtm: MainThreadMarker) {
        let main = native_menu::build(mtm, &spec());
        let _ = native_menu::drain_for_tests();
        let cmd = NSEventModifierFlags::Command;
        assert!(
            !main.performKeyEquivalent(&key("b", cmd, 0x0B)),
            "⌘B must reach the window, not the menu"
        );
        assert!(
            !main.performKeyEquivalent(&key("1", cmd, 0x12)),
            "⌘1 in a nested submenu must reach the window too"
        );
        assert!(native_menu::drain_for_tests().is_empty(), "no item ran");

        // An open menu shows the keys; a closed one gives them up.
        let view = submenu(&main, 1);
        let sidebar = view.itemAtIndex(0).expect("sidebar item");
        assert_eq!(sidebar.keyEquivalent().to_string(), "");
        open(&view);
        assert_eq!(sidebar.keyEquivalent().to_string(), "b", "the key shows");
        assert_eq!(sidebar.keyEquivalentModifierMask(), cmd);
        let slots = view
            .itemAtIndex(1)
            .and_then(|i| i.submenu())
            .expect("slots");
        let slot = slots.itemAtIndex(0).expect("slot item");
        assert_eq!(
            slot.keyEquivalent().to_string(),
            "",
            "a submenu opens on its own"
        );
        open(&slots);
        assert_eq!(slot.keyEquivalent().to_string(), "1");
        close(&slots);
        close(&view);
        assert_eq!(sidebar.keyEquivalent().to_string(), "");
        assert_eq!(slot.keyEquivalent().to_string(), "");
        assert!(!main.performKeyEquivalent(&key("b", cmd, 0x0B)));
        assert!(native_menu::drain_for_tests().is_empty());
    }

    /// A native item that owns its key still takes it. The app's Enter
    /// Full Screen has no key (⌃⌘F is Find in Document); this spec gives it
    /// one because, with no window, the key has no effect.
    fn native_items_keep_their_keys(mtm: MainThreadMarker) {
        let main = native_menu::build(mtm, &spec());
        let flags = NSEventModifierFlags::Command | NSEventModifierFlags::Control;
        // No window: `toggleFullScreen:` finds no responder and does nothing.
        assert!(main.performKeyEquivalent(&key("f", flags, 0x03)));
        assert!(!main.performKeyEquivalent(&key("f", NSEventModifierFlags::Command, 0x03)));
        assert!(native_menu::drain_for_tests().is_empty());
    }

    /// A click sends the item's tag to the app.
    fn a_click_sends_the_tag(mtm: MainThreadMarker) {
        let main = native_menu::build(mtm, &spec());
        let _ = native_menu::drain_for_tests();
        let view = submenu(&main, 1);
        view.performActionForItemAtIndex(0);
        assert_eq!(native_menu::drain_for_tests(), vec![SIDEBAR]);
    }

    /// Items take their state and titles from what the app pushed when
    /// AppKit validates them, as it does before it shows a menu.
    fn items_show_the_pushed_state(mtm: MainThreadMarker) {
        let main = native_menu::build(mtm, &spec());
        let view = submenu(&main, 1);
        let sidebar = view.itemAtIndex(0).expect("sidebar item");
        let slots = view
            .itemAtIndex(1)
            .and_then(|i| i.submenu())
            .expect("slots");
        let slot = slots.itemAtIndex(0).expect("slot item");
        native_menu::set_state(State {
            enabled: State::bit(SIDEBAR),
            checked: State::bit(SIDEBAR),
        });
        native_menu::set_titles(vec![(SLOT, "Slot 1: a.md".into())]);
        view.update();
        slots.update();
        assert!(sidebar.isEnabled());
        assert_eq!(sidebar.state(), NSControlStateValueOn);
        assert!(!slot.isEnabled());
        assert_eq!(slot.state(), NSControlStateValueOff);
        assert_eq!(slot.title().to_string(), "Slot 1: a.md");

        native_menu::set_state(State {
            enabled: State::bit(SLOT),
            checked: 0,
        });
        native_menu::set_titles(Vec::new());
        view.update();
        slots.update();
        assert!(!sidebar.isEnabled());
        assert_eq!(sidebar.state(), NSControlStateValueOff);
        assert!(slot.isEnabled());
        assert_eq!(slot.title().to_string(), "Slot 1: a.md", "a title stays");
        // A native item validates through the responder chain, not here.
        let full = view.itemAtIndex(2).expect("full screen item");
        assert_eq!(full.title().to_string(), "Enter Full Screen");
    }

    /// Open Recent is rebuilt when the list changed, and only then.
    fn open_recent_rebuilds_only_on_change(mtm: MainThreadMarker) {
        let main = native_menu::build(mtm, &spec());
        let recent = submenu(&submenu(&main, 2), 0);
        native_menu::set_recent(vec!["a.md — ~".into(), "b.md — ~/x".into()]);
        native_menu::update_recent_for_tests();
        assert_eq!(recent.numberOfItems(), 2);
        let first = recent.itemAtIndex(0).expect("first");
        assert_eq!(first.title().to_string(), "a.md — ~");
        assert_eq!(first.tag(), native_menu::RECENT_TAG as isize);
        native_menu::update_recent_for_tests();
        let again = recent.itemAtIndex(0).expect("first");
        assert!(std::ptr::eq(&*first, &*again), "an unchanged list is kept");

        native_menu::set_state(State {
            enabled: State::bit(0),
            checked: 0,
        });
        recent.update();
        assert!(again.isEnabled(), "bit 0 enables the recent items");
        let _ = native_menu::drain_for_tests();
        recent.performActionForItemAtIndex(1);
        assert_eq!(
            native_menu::drain_for_tests(),
            vec![native_menu::RECENT_TAG + 1]
        );

        native_menu::set_recent(Vec::new());
        native_menu::update_recent_for_tests();
        assert_eq!(recent.numberOfItems(), 1);
        let empty = recent.itemAtIndex(0).expect("placeholder");
        assert_eq!(empty.title().to_string(), "No Recent Items");
        recent.update();
        assert!(!empty.isEnabled());
    }

    pub fn run() {
        let mtm = MainThreadMarker::new().expect("main thread");
        let _ = NSApplication::sharedApplication(mtm);
        let checks: [(&str, fn(MainThreadMarker)); 5] = [
            (
                "app_items_leave_their_keys_to_the_window",
                app_items_leave_their_keys_to_the_window,
            ),
            ("native_items_keep_their_keys", native_items_keep_their_keys),
            ("a_click_sends_the_tag", a_click_sends_the_tag),
            ("items_show_the_pushed_state", items_show_the_pushed_state),
            (
                "open_recent_rebuilds_only_on_change",
                open_recent_rebuilds_only_on_change,
            ),
        ];
        for (name, check) in checks {
            check(mtm);
            println!("test {name} ... ok");
        }
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    appkit::run();
}
