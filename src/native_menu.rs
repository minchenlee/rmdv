//! The macOS menu bar (MDV-048 P8): File, Edit, View, Go, Window, and Help
//! beside the app menu, hand-written with `objc2-app-kit` in the style of
//! `native_pinch` and `macos_open`.
//!
//! The app describes the menus with the plain types below
//! ([`Menu`], [`Entry`], [`Shortcut`]); this module builds the `NSMenu`
//! tree from them once, after winit has created `NSApp` and the window.
//! A click on an app item sends the item's tag through a channel into an
//! Iced subscription ([`subscription`]); the app maps the tag back to its
//! command. The app pushes item state ([`set_state`], [`set_titles`],
//! [`set_recent`]) only when it changes, and `validateMenuItem:` reads it
//! when AppKit shows a menu.
//!
//! Key equivalents. Every app item shows its shortcut, but no app item
//! takes a key: an app item has its key equivalent only while its menu is
//! open (`menuWillOpen:` sets it, `menuDidClose:` clears it). A key
//! pressed with the menus closed finds no app item, so it goes on to the
//! window as it did before the menu bar existed. The app's key handler
//! keeps every shortcut and its per-focus behavior (text fields, the Zen
//! editor, reader selections, modal cards), and no shortcut fires twice.
//! A delegate that answers `menuHasKeyEquivalent:forEvent:target:action:`
//! with NO does not do this: AppKit still finds the item, and it takes the
//! key even when the item is disabled. Only the native items winit's menu
//! already had (Hide ⌘H, Hide Others ⌥⌘H, Quit ⌘Q) keep their keys at all
//! times. Enter Full Screen has no key: ⌃⌘F stays Find in Document.

/// A key a menu item shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A character key, lowercase for letters.
    Char(char),
    /// The up arrow.
    Up,
    /// The down arrow.
    Down,
}

/// The modifiers of a [`Shortcut`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mods {
    pub command: bool,
    pub shift: bool,
    pub option: bool,
    pub control: bool,
}

/// A key equivalent: a key and its modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shortcut {
    pub key: Key,
    pub mods: Mods,
}

impl Shortcut {
    /// ⌘ and `c`.
    pub const fn cmd(c: char) -> Self {
        Self::with(
            Key::Char(c),
            Mods {
                command: true,
                shift: false,
                option: false,
                control: false,
            },
        )
    }

    /// ⇧⌘ and `c`.
    pub const fn cmd_shift(c: char) -> Self {
        Self::with(
            Key::Char(c),
            Mods {
                command: true,
                shift: true,
                option: false,
                control: false,
            },
        )
    }

    /// ⌥⌘ and `c`.
    pub const fn cmd_option(c: char) -> Self {
        Self::with(
            Key::Char(c),
            Mods {
                command: true,
                shift: false,
                option: true,
                control: false,
            },
        )
    }

    /// ⌃⌘ and `c`.
    pub const fn cmd_control(c: char) -> Self {
        Self::with(
            Key::Char(c),
            Mods {
                command: true,
                shift: false,
                option: false,
                control: true,
            },
        )
    }

    pub const fn with(key: Key, mods: Mods) -> Self {
        Self { key, mods }
    }

    /// The modifier glyphs and the key, as a set and a string, so that
    /// "⌘⌥R" (the palette's order) and "⌥⌘R" (the menu's) compare equal.
    pub fn glyphs(&self) -> (std::collections::BTreeSet<char>, String) {
        let mut mods = std::collections::BTreeSet::new();
        if self.mods.control {
            mods.insert('⌃');
        }
        if self.mods.option {
            mods.insert('⌥');
        }
        if self.mods.shift {
            mods.insert('⇧');
        }
        if self.mods.command {
            mods.insert('⌘');
        }
        let key = match self.key {
            Key::Char(c) => c.to_uppercase().to_string(),
            Key::Up => "↑".to_string(),
            Key::Down => "↓".to_string(),
        };
        (mods, key)
    }

    /// The `keyEquivalent` string AppKit takes.
    pub fn key_equivalent(&self) -> String {
        match self.key {
            Key::Char(c) => c.to_string(),
            // NSUpArrowFunctionKey and NSDownArrowFunctionKey.
            Key::Up => '\u{F700}'.to_string(),
            Key::Down => '\u{F701}'.to_string(),
        }
    }
}

/// An AppKit action that needs no app code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Native {
    About,
    Hide,
    HideOthers,
    ShowAll,
    Quit,
    Minimize,
    Zoom,
    BringAllToFront,
    ToggleFullScreen,
}

impl Native {
    /// The selector AppKit sends up the responder chain.
    pub fn selector_name(self) -> &'static str {
        match self {
            Native::About => "orderFrontStandardAboutPanel:",
            Native::Hide => "hide:",
            Native::HideOthers => "hideOtherApplications:",
            Native::ShowAll => "unhideAllApplications:",
            Native::Quit => "terminate:",
            Native::Minimize => "performMiniaturize:",
            Native::Zoom => "performZoom:",
            Native::BringAllToFront => "arrangeInFront:",
            Native::ToggleFullScreen => "toggleFullScreen:",
        }
    }
}

/// What a menu item does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// An app command, by tag (1 to [`MAX_COMMAND_TAG`]).
    Command(u32),
    /// An AppKit action.
    Native(Native),
}

/// The largest command tag; tags index the [`State`] bits.
pub const MAX_COMMAND_TAG: u32 = 127;
/// The first tag of the Open Recent items; item `i` has `RECENT_TAG + i`.
pub const RECENT_TAG: u32 = 1000;

/// One row of a menu.
#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Item {
        title: String,
        shortcut: Option<Shortcut>,
        action: Action,
    },
    Separator,
    Submenu(Menu),
}

/// The special role of a menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Plain,
    /// The system Services menu (`NSApp.servicesMenu`).
    Services,
    /// The Window menu (`NSApp.windowsMenu`): AppKit lists the windows.
    Windows,
    /// The Help menu (`NSApp.helpMenu`): AppKit adds its search field.
    Help,
    /// Open Recent: its items come from [`set_recent`].
    Recent,
}

/// A menu: a title, a role, and rows.
#[derive(Debug, Clone, PartialEq)]
pub struct Menu {
    pub title: String,
    pub role: Role,
    pub entries: Vec<Entry>,
}

/// Item state, by command tag: bit `t` of `enabled` and `checked` is tag
/// `t`. Bit 0 enables the Open Recent items.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct State {
    pub enabled: u128,
    pub checked: u128,
}

impl State {
    pub fn bit(tag: u32) -> u128 {
        if tag <= MAX_COMMAND_TAG {
            1u128 << tag
        } else {
            0
        }
    }

    pub fn is_enabled(&self, tag: u32) -> bool {
        self.enabled & Self::bit(tag) != 0
    }

    pub fn is_checked(&self, tag: u32) -> bool {
        self.checked & Self::bit(tag) != 0
    }
}

use std::sync::Mutex;

/// What the app pushed last, read by AppKit on the main thread. Other
/// platforms have no menu bar to read it.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
struct Shared {
    state: State,
    titles: Vec<(u32, String)>,
    recent: Vec<String>,
    recent_generation: u64,
}

static SHARED: Mutex<Shared> = Mutex::new(Shared {
    state: State {
        enabled: 0,
        checked: 0,
    },
    titles: Vec::new(),
    recent: Vec::new(),
    recent_generation: 0,
});

/// Record which items are enabled and checked. The app calls it only when
/// the state changed.
pub fn set_state(state: State) {
    if let Ok(mut shared) = SHARED.lock() {
        shared.state = state;
    }
}

/// Record titles that change at run time (Quick Slot names), by tag.
pub fn set_titles(titles: Vec<(u32, String)>) {
    if let Ok(mut shared) = SHARED.lock() {
        shared.titles = titles;
    }
}

/// Record the Open Recent titles; item `i` sends `RECENT_TAG + i`. The
/// submenu is rebuilt the next time it opens.
pub fn set_recent(titles: Vec<String>) {
    if let Ok(mut shared) = SHARED.lock() {
        shared.recent = titles;
        shared.recent_generation = shared.recent_generation.wrapping_add(1);
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{Action, Entry, Menu, Mods, Role, SHARED};
    use futures::stream::{self, BoxStream, StreamExt};
    use objc2::rc::Retained;
    use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject, Sel};
    use objc2::{define_class, msg_send, sel, MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::{
        NSApplication, NSControlStateValueOff, NSControlStateValueOn, NSEventModifierFlags, NSMenu,
        NSMenuDelegate, NSMenuItem, NSMenuItemValidation,
    };
    use objc2_foundation::NSString;
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    use tokio::sync::mpsc::{self, Receiver, Sender};

    struct Channel {
        sender: Sender<u32>,
        receiver: Mutex<Option<Receiver<u32>>>,
    }

    static CHANNEL: OnceLock<Channel> = OnceLock::new();

    thread_local! {
        static TARGET: RefCell<Option<Retained<MenuTarget>>> = const { RefCell::new(None) };
        static RECENT_MENU: RefCell<Option<Retained<NSMenu>>> = const { RefCell::new(None) };
        static RECENT_BUILT: Cell<Option<u64>> = const { Cell::new(None) };
        /// The key equivalent and modifiers of each app item, by tag; set
        /// only while the item's menu is open.
        static KEYS: RefCell<HashMap<isize, (String, NSEventModifierFlags)>> =
            RefCell::new(HashMap::new());
    }

    define_class!(
        // SAFETY: NSObject has no subclassing requirements, and MenuTarget
        // does not implement Drop.
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "RmdvMenuTarget"]
        struct MenuTarget;

        impl MenuTarget {
            /// A click on an app item: its tag goes to the app.
            #[unsafe(method(rmdvMenuItem:))]
            fn menu_item(&self, sender: &NSMenuItem) {
                if let Ok(tag) = u32::try_from(sender.tag()) {
                    send(tag);
                }
            }

        }

        unsafe impl NSObjectProtocol for MenuTarget {}

        unsafe impl NSMenuItemValidation for MenuTarget {
            #[unsafe(method(validateMenuItem:))]
            fn validate_menu_item(&self, item: &NSMenuItem) -> bool {
                validate(item)
            }
        }

        unsafe impl NSMenuDelegate for MenuTarget {
            #[unsafe(method(menuNeedsUpdate:))]
            fn menu_needs_update(&self, menu: &NSMenu) {
                rebuild_recent_if_stale(menu, self);
            }

            /// The menu opens: its app items show their keys.
            #[unsafe(method(menuWillOpen:))]
            fn menu_will_open(&self, menu: &NSMenu) {
                show_keys(menu, true);
            }

            /// The menu closed: its app items give up their keys.
            #[unsafe(method(menuDidClose:))]
            fn menu_did_close(&self, menu: &NSMenu) {
                show_keys(menu, false);
            }
        }
    );

    impl MenuTarget {
        fn new(mtm: MainThreadMarker) -> Retained<Self> {
            let this = Self::alloc(mtm).set_ivars(());
            // SAFETY: NSObject's `init` has this signature.
            unsafe { msg_send![super(this), init] }
        }
    }

    fn send(tag: u32) {
        if let Some(channel) = CHANNEL.get() {
            let _ = channel.sender.try_send(tag);
        }
    }

    /// Give the app items in `menu` their key equivalents, or take them
    /// away. Native items keep theirs; they are not in `KEYS`.
    fn show_keys(menu: &NSMenu, open: bool) {
        KEYS.with(|keys| {
            let keys = keys.borrow();
            for item in menu.itemArray().iter() {
                let Some((key, mask)) = keys.get(&item.tag()) else {
                    continue;
                };
                if open {
                    item.setKeyEquivalent(&NSString::from_str(key));
                    item.setKeyEquivalentModifierMask(*mask);
                } else {
                    item.setKeyEquivalent(&NSString::new());
                }
            }
        });
    }

    fn validate(item: &NSMenuItem) -> bool {
        let Ok(tag) = u32::try_from(item.tag()) else {
            return true;
        };
        let Ok(shared) = SHARED.lock() else {
            return true;
        };
        if tag >= super::RECENT_TAG {
            return shared.state.is_enabled(0);
        }
        if let Some((_, title)) = shared.titles.iter().find(|(t, _)| *t == tag) {
            if item.title().to_string() != *title {
                item.setTitle(&NSString::from_str(title));
            }
        }
        item.setState(if shared.state.is_checked(tag) {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
        shared.state.is_enabled(tag)
    }

    fn rebuild_recent_if_stale(menu: &NSMenu, this: &MenuTarget) {
        let is_recent = RECENT_MENU.with(|recent| {
            recent
                .borrow()
                .as_deref()
                .is_some_and(|recent| std::ptr::eq(recent, menu))
        });
        if !is_recent {
            return;
        }
        let Ok(shared) = SHARED.lock() else {
            return;
        };
        if RECENT_BUILT.get() == Some(shared.recent_generation) {
            return;
        }
        RECENT_BUILT.set(Some(shared.recent_generation));
        fill_recent(menu, &shared.recent, this);
    }

    fn fill_recent(menu: &NSMenu, titles: &[String], this: &MenuTarget) {
        let mtm = this.mtm();
        menu.removeAllItems();
        if titles.is_empty() {
            // No action: AppKit shows it disabled.
            let empty = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(mtm),
                    &NSString::from_str("No Recent Items"),
                    None,
                    &NSString::new(),
                )
            };
            menu.addItem(&empty);
            return;
        }
        for (index, title) in titles.iter().enumerate() {
            let item = command_item(mtm, title, None, super::RECENT_TAG + index as u32, this);
            menu.addItem(&item);
        }
    }

    fn command_item(
        mtm: MainThreadMarker,
        title: &str,
        shortcut: Option<&super::Shortcut>,
        tag: u32,
        this: &MenuTarget,
    ) -> Retained<NSMenuItem> {
        // No key equivalent until the menu opens (see the module docs).
        // SAFETY: `rmdvMenuItem:` is a method of the target set below.
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str(title),
                Some(sel!(rmdvMenuItem:)),
                &NSString::new(),
            )
        };
        item.setTag(tag as isize);
        if let Some(shortcut) = shortcut {
            KEYS.with(|keys| {
                keys.borrow_mut().insert(
                    tag as isize,
                    (shortcut.key_equivalent(), modifier_mask(shortcut.mods)),
                )
            });
        }
        // SAFETY: the target lives for the process (thread-local TARGET).
        unsafe { item.setTarget(Some(this)) };
        item
    }

    fn modifier_mask(mods: Mods) -> NSEventModifierFlags {
        let mut mask = NSEventModifierFlags::empty();
        if mods.command {
            mask |= NSEventModifierFlags::Command;
        }
        if mods.shift {
            mask |= NSEventModifierFlags::Shift;
        }
        if mods.option {
            mask |= NSEventModifierFlags::Option;
        }
        if mods.control {
            mask |= NSEventModifierFlags::Control;
        }
        mask
    }

    fn build_menu(
        mtm: MainThreadMarker,
        spec: &Menu,
        this: &MenuTarget,
        app: &NSApplication,
    ) -> Retained<NSMenu> {
        let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(&spec.title));
        match spec.role {
            // AppKit fills the Services menu; it keeps its own key search.
            Role::Services => {
                app.setServicesMenu(Some(&menu));
                return menu;
            }
            Role::Recent => {
                RECENT_MENU.with(|recent| *recent.borrow_mut() = Some(menu.clone()));
                RECENT_BUILT.set(None);
            }
            Role::Windows => app.setWindowsMenu(Some(&menu)),
            Role::Help => app.setHelpMenu(Some(&menu)),
            Role::Plain => {}
        }
        menu.setDelegate(Some(ProtocolObject::from_ref(this)));
        for entry in &spec.entries {
            match entry {
                Entry::Separator => menu.addItem(&NSMenuItem::separatorItem(mtm)),
                Entry::Submenu(sub) => {
                    let item = unsafe {
                        NSMenuItem::initWithTitle_action_keyEquivalent(
                            NSMenuItem::alloc(mtm),
                            &NSString::from_str(&sub.title),
                            None,
                            &NSString::new(),
                        )
                    };
                    let submenu = build_menu(mtm, sub, this, app);
                    item.setSubmenu(Some(&submenu));
                    menu.addItem(&item);
                }
                Entry::Item {
                    title,
                    shortcut,
                    action: Action::Command(tag),
                } => menu.addItem(&command_item(mtm, title, shortcut.as_ref(), *tag, this)),
                Entry::Item {
                    title,
                    shortcut,
                    action: Action::Native(native),
                } => {
                    let key = shortcut
                        .as_ref()
                        .map(super::Shortcut::key_equivalent)
                        .unwrap_or_default();
                    let selector = Sel::register(
                        &std::ffi::CString::new(native.selector_name()).expect("selector"),
                    );
                    // SAFETY: a nil target sends the standard selector up
                    // the responder chain, where AppKit implements it.
                    let item = unsafe {
                        NSMenuItem::initWithTitle_action_keyEquivalent(
                            NSMenuItem::alloc(mtm),
                            &NSString::from_str(title),
                            Some(selector),
                            &NSString::from_str(&key),
                        )
                    };
                    if let Some(shortcut) = shortcut {
                        item.setKeyEquivalentModifierMask(modifier_mask(shortcut.mods));
                    }
                    menu.addItem(&item);
                }
            }
        }
        menu
    }

    /// Whether the menu bar is installed.
    pub fn installed() -> bool {
        TARGET.with(|target| target.borrow().is_some())
    }

    /// Build the main menu from `menus` (the app menu first) and install
    /// it, once, on the main thread after NSApp and the window exist.
    /// Unit tests construct `App` off the main thread; they skip this.
    pub fn install(menus: &[Menu]) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        if installed() {
            return;
        }
        let main = build(mtm, menus);
        NSApplication::sharedApplication(mtm).setMainMenu(Some(&main));
    }

    /// Build the main menu without installing it.
    pub fn build(mtm: MainThreadMarker, menus: &[Menu]) -> Retained<NSMenu> {
        CHANNEL.get_or_init(|| {
            let (sender, receiver) = mpsc::channel(16);
            Channel {
                sender,
                receiver: Mutex::new(Some(receiver)),
            }
        });
        let target = TARGET.with(|slot| {
            slot.borrow_mut()
                .get_or_insert_with(|| MenuTarget::new(mtm))
                .clone()
        });
        let app = NSApplication::sharedApplication(mtm);
        let main = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(""));
        for spec in menus {
            let item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(mtm),
                    &NSString::from_str(&spec.title),
                    None,
                    &NSString::new(),
                )
            };
            let submenu = build_menu(mtm, spec, &target, &app);
            item.setSubmenu(Some(&submenu));
            main.addItem(&item);
        }
        main
    }

    /// The tags of clicked items, in order.
    pub fn subscription() -> iced::Subscription<u32> {
        if CHANNEL.get().is_none() {
            return iced::Subscription::none();
        }
        iced::Subscription::run(events)
    }

    fn events() -> BoxStream<'static, u32> {
        let receiver = CHANNEL
            .get()
            .and_then(|channel| channel.receiver.lock().ok()?.take());
        match receiver {
            Some(receiver) => stream::unfold(receiver, |mut receiver| async {
                receiver.recv().await.map(|tag| (tag, receiver))
            })
            .boxed(),
            None => stream::empty().boxed(),
        }
    }

    /// The tags sent so far, for the AppKit test; it takes the receiver.
    #[doc(hidden)]
    pub fn drain_for_tests() -> Vec<u32> {
        let mut out = Vec::new();
        if let Some(channel) = CHANNEL.get() {
            if let Ok(mut slot) = channel.receiver.lock() {
                if let Some(receiver) = slot.as_mut() {
                    while let Ok(tag) = receiver.try_recv() {
                        out.push(tag);
                    }
                }
            }
        }
        out
    }

    /// Run `menuNeedsUpdate:` on the Open Recent submenu, as AppKit does
    /// before it shows it; for the AppKit test.
    #[doc(hidden)]
    pub fn update_recent_for_tests() {
        let target = TARGET.with(|t| t.borrow().clone());
        let menu = RECENT_MENU.with(|m| m.borrow().clone());
        if let (Some(target), Some(menu)) = (target, menu) {
            rebuild_recent_if_stale(&menu, &target);
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::Menu;

    pub fn installed() -> bool {
        false
    }

    pub fn install(_menus: &[Menu]) {}

    pub fn subscription() -> iced::Subscription<u32> {
        iced::Subscription::none()
    }
}

#[cfg(target_os = "macos")]
#[doc(hidden)]
pub use imp::{build, drain_for_tests, update_recent_for_tests};
pub use imp::{install, installed, subscription};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_and_menu_glyph_orders_compare_equal() {
        let (mods, key) = Shortcut::cmd_option('r').glyphs();
        assert_eq!(key, "R");
        assert_eq!(mods, ['⌘', '⌥'].into_iter().collect());
    }

    #[test]
    fn state_bits_are_by_tag() {
        let state = State {
            enabled: State::bit(3) | State::bit(0),
            checked: State::bit(5),
        };
        assert!(state.is_enabled(3) && state.is_enabled(0));
        assert!(!state.is_enabled(4));
        assert!(state.is_checked(5));
        assert_eq!(State::bit(RECENT_TAG), 0);
    }
}
