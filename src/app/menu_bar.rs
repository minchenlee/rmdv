//! The native menu bar's model (MDV-048 P8): which commands it lists,
//! their titles and shortcuts, and what a click runs.
//!
//! Titles and shortcuts come from the command palette (`command_items`)
//! where the palette lists the command; a test holds them equal. Every
//! item with a shortcut that the key handler maps runs, on a click, the
//! message the shortcut would run now ([`key_message`] on the current
//! [`KeyContext`]), so a menu click and a key press do the same thing on
//! every surface, and the item is disabled where the shortcut does nothing.
//! The menu never takes those keys itself (see `crate::native_menu`).

use super::*;
use crate::native_menu::{self, Action, Entry, Key, Menu, Mods, Native, Role, Shortcut};

/// An Open Recent title: the name, then the folder it is in.
pub(super) fn recent_title(path: &Path, home: Option<&Path>) -> String {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let Some(parent) = path.parent() else {
        return name;
    };
    let folder = match home.and_then(|home| parent.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Some(rest) => format!("~/{}", rest.display()),
        None => parent.display().to_string(),
    };
    format!("{name} — {folder}")
}

/// The rmdv website, for Help.
pub(super) const WEBSITE: &str = "https://rmdv.mclee.dev/";

/// What the app pushed to the menu bar last; each part is pushed again
/// only when it changes.
#[derive(Debug, Default)]
pub(super) struct MenuPushed {
    state: Option<native_menu::State>,
    titles: Option<Vec<(u32, String)>>,
    recent: Option<Vec<PathBuf>>,
    /// Pushes of the state, the titles, and the recent list, for tests.
    pub(super) pushes: [u32; 3],
    /// Times the menu was computed to compare with the last push, for
    /// tests.
    pub(super) computes: u32,
}

impl MenuPushed {
    /// Whether the menu has had its first push.
    pub(super) fn synced(&self) -> bool {
        self.state.is_some()
    }
}

impl Message {
    /// Pointer hover and scroll reports, which arrive at pointer or frame
    /// rate and change nothing the menu bar shows (its enabled and checked
    /// items, slot titles, and recent list): `update` skips the menu
    /// computation for them.
    pub(super) fn leaves_menu_unchanged(&self) -> bool {
        matches!(
            self,
            Message::ListPointerMoved(_)
                | Message::VaultHover(_)
                | Message::OverlayHover(_)
                | Message::LinkHovered(..)
                | Message::OutlineHovered(_)
                | Message::ReaderScrollbarHover(_)
                | Message::HeadingHoverEnter(_)
                | Message::HeadingHoverExit(_)
                | Message::ScrollerTick
                | Message::ReaderAutoScrollTick
                | Message::TreeScrolled(_)
                | Message::OutlineScrolled(_)
                | Message::OverlayScrolled(_)
                | Message::VaultScrolled(_)
                | Message::BodyScrolled(_)
                | Message::TableScrolled
                | Message::MindmapPanelDragMove(_)
                | Message::FullMindmapPanelDragMove(_)
                | Message::WindowMoved(..)
        )
    }
}

/// A menu bar command; [`MenuCommand::tag`] is its item's tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MenuCommand {
    Settings,
    OpenFolder,
    FindFile,
    Save,
    CloseSlot,
    CloseWindow,
    Refresh,
    RevealInFinder,
    CopyPath,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Find,
    FindInVault,
    View(ToolbarView),
    ToggleSidebar,
    AlwaysShowToolbar,
    ToggleFooter,
    ShowHidden,
    FontUp,
    FontDown,
    FontReset,
    CycleTheme,
    PickTheme,
    CommandPalette,
    Back,
    Forward,
    QuickSlot(u8),
    NextSlot,
    PreviousSlot,
    KeyboardShortcuts,
    Website,
}

/// Where an item's title comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Title {
    /// The palette's name for the same command (before its shortcut).
    Palette(&'static str),
    /// A menu-only title: a macOS standard name, a view name as the
    /// toolbar shows it, or a command the palette does not list.
    Own(&'static str),
}

/// An item: its title, its shortcut, and, for an item whose click runs
/// its shortcut, which messages count as that command.
#[derive(Clone, Copy)]
pub(super) struct CommandSpec {
    pub title: Title,
    pub shortcut: Option<Shortcut>,
    pub keyed: Option<fn(&Message) -> bool>,
}

const SLOT_TITLES: [&str; crate::quick_slots::SLOT_COUNT] = [
    "Slot 1", "Slot 2", "Slot 3", "Slot 4", "Slot 5", "Slot 6", "Slot 7", "Slot 8", "Slot 9",
];

const NO_MODS: Mods = Mods {
    command: false,
    shift: false,
    option: false,
    control: false,
};

const CMD: Mods = Mods {
    command: true,
    ..NO_MODS
};

impl MenuCommand {
    pub(super) const ALL: [MenuCommand; 46] = [
        MenuCommand::Settings,
        MenuCommand::OpenFolder,
        MenuCommand::FindFile,
        MenuCommand::Save,
        MenuCommand::CloseSlot,
        MenuCommand::CloseWindow,
        MenuCommand::Refresh,
        MenuCommand::RevealInFinder,
        MenuCommand::CopyPath,
        MenuCommand::Undo,
        MenuCommand::Redo,
        MenuCommand::Cut,
        MenuCommand::Copy,
        MenuCommand::Paste,
        MenuCommand::SelectAll,
        MenuCommand::Find,
        MenuCommand::FindInVault,
        MenuCommand::View(ToolbarView::Read),
        MenuCommand::View(ToolbarView::Edit),
        MenuCommand::View(ToolbarView::Mindmap),
        MenuCommand::View(ToolbarView::FullMindmap),
        MenuCommand::ToggleSidebar,
        MenuCommand::AlwaysShowToolbar,
        MenuCommand::ToggleFooter,
        MenuCommand::ShowHidden,
        MenuCommand::FontUp,
        MenuCommand::FontDown,
        MenuCommand::FontReset,
        MenuCommand::CycleTheme,
        MenuCommand::PickTheme,
        MenuCommand::CommandPalette,
        MenuCommand::Back,
        MenuCommand::Forward,
        MenuCommand::QuickSlot(0),
        MenuCommand::QuickSlot(1),
        MenuCommand::QuickSlot(2),
        MenuCommand::QuickSlot(3),
        MenuCommand::QuickSlot(4),
        MenuCommand::QuickSlot(5),
        MenuCommand::QuickSlot(6),
        MenuCommand::QuickSlot(7),
        MenuCommand::QuickSlot(8),
        MenuCommand::NextSlot,
        MenuCommand::PreviousSlot,
        MenuCommand::KeyboardShortcuts,
        MenuCommand::Website,
    ];

    /// The item's tag: its place in [`MenuCommand::ALL`], from 1.
    pub(super) fn tag(self) -> u32 {
        Self::ALL
            .iter()
            .position(|command| *command == self)
            .map_or(0, |index| index as u32 + 1)
    }

    pub(super) fn from_tag(tag: u32) -> Option<Self> {
        let index = usize::try_from(tag).ok()?.checked_sub(1)?;
        Self::ALL.get(index).copied()
    }

    pub(super) fn spec(self) -> CommandSpec {
        use MenuCommand as C;
        fn keyed(title: Title, shortcut: Shortcut, accept: fn(&Message) -> bool) -> CommandSpec {
            CommandSpec {
                title,
                shortcut: Some(shortcut),
                keyed: Some(accept),
            }
        }
        fn pointer(title: Title, shortcut: Option<Shortcut>) -> CommandSpec {
            CommandSpec {
                title,
                shortcut,
                keyed: None,
            }
        }
        let p = Title::Palette;
        let own = Title::Own;
        match self {
            C::Settings => keyed(own("Settings…"), Shortcut::cmd(','), |m| {
                matches!(m, Message::ToggleSettings)
            }),
            C::OpenFolder => keyed(p("Open Folder…"), Shortcut::cmd('o'), |m| {
                matches!(m, Message::OpenFolderPicker)
            }),
            C::FindFile => keyed(p("Find File in Workspace…"), Shortcut::cmd('p'), |m| {
                matches!(m, Message::OpenFileFinder)
            }),
            C::Save => keyed(own("Save"), Shortcut::cmd('s'), |m| {
                matches!(m, Message::SaveFile)
            }),
            C::CloseSlot => keyed(own("Close Slot"), Shortcut::cmd('w'), |m| {
                matches!(m, Message::QuickSlotClose)
            }),
            C::CloseWindow => keyed(own("Close Window"), Shortcut::cmd_shift('w'), |m| {
                matches!(m, Message::QuickSlotCloseWindow)
            }),
            C::Refresh => keyed(p("Refresh File / Folder"), Shortcut::cmd('r'), |m| {
                matches!(m, Message::Refresh)
            }),
            C::RevealInFinder => {
                keyed(p("Reveal File in Finder"), Shortcut::cmd_option('r'), |m| {
                    matches!(m, Message::RevealFileInFinder)
                })
            }
            C::CopyPath => keyed(
                p("Copy Focused File Path"),
                Shortcut::cmd_option('c'),
                |m| matches!(m, Message::CopyFilePath),
            ),
            C::Undo => keyed(own("Undo"), Shortcut::cmd('z'), |m| {
                matches!(m, Message::EditorUndo)
            }),
            C::Redo => keyed(own("Redo"), Shortcut::cmd_shift('z'), |m| {
                matches!(m, Message::EditorRedo)
            }),
            C::Cut => pointer(own("Cut"), Some(Shortcut::cmd('x'))),
            C::Copy => pointer(own("Copy"), Some(Shortcut::cmd('c'))),
            C::Paste => pointer(own("Paste"), Some(Shortcut::cmd('v'))),
            C::SelectAll => pointer(own("Select All"), Some(Shortcut::cmd('a'))),
            C::Find => keyed(p("Find in Document"), Shortcut::cmd('f'), |m| {
                matches!(m, Message::ToggleSearch)
            }),
            C::FindInVault => keyed(p("Search All Files…"), Shortcut::cmd_shift('f'), |m| {
                matches!(m, Message::OpenVaultSearch)
            }),
            // The toolbar's view names; the shortcuts are its tooltips'.
            C::View(ToolbarView::Read) => pointer(own("Read"), None),
            C::View(ToolbarView::Edit) => pointer(own("Edit"), Some(Shortcut::cmd('e'))),
            C::View(ToolbarView::Mindmap) => pointer(own("Mind Map"), Some(Shortcut::cmd('m'))),
            C::View(ToolbarView::FullMindmap) => {
                pointer(own("Full Mindmap"), Some(Shortcut::cmd_shift('m')))
            }
            C::ToggleSidebar => keyed(p("Toggle Sidebar"), Shortcut::cmd('b'), |m| {
                matches!(m, Message::ToggleSidebar)
            }),
            C::AlwaysShowToolbar => pointer(own("Always Show Toolbar"), None),
            C::ToggleFooter => pointer(p("Toggle Status Footer"), None),
            C::ShowHidden => keyed(p("Toggle Hidden Files"), Shortcut::cmd_shift('.'), |m| {
                matches!(m, Message::ToggleHidden)
            }),
            C::FontUp => keyed(p("Increase Font Size"), Shortcut::cmd('+'), |m| {
                matches!(m, Message::FontSizeUp)
            }),
            C::FontDown => keyed(p("Decrease Font Size"), Shortcut::cmd('-'), |m| {
                matches!(m, Message::FontSizeDown)
            }),
            C::FontReset => keyed(p("Reset Font Size"), Shortcut::cmd('0'), |m| {
                matches!(m, Message::FontSizeReset)
            }),
            C::CycleTheme => keyed(p("Cycle Theme"), Shortcut::cmd('t'), |m| {
                matches!(m, Message::ToggleTheme)
            }),
            C::PickTheme => pointer(p("Pick Theme…"), None),
            C::CommandPalette => keyed(own("Command Palette…"), Shortcut::cmd_shift('p'), |m| {
                matches!(m, Message::OpenCommandPalette)
            }),
            C::Back => keyed(p("Go Back"), Shortcut::cmd('['), |m| {
                matches!(m, Message::HistoryGo(HistoryDir::Back))
            }),
            C::Forward => keyed(p("Go Forward"), Shortcut::cmd(']'), |m| {
                matches!(m, Message::HistoryGo(HistoryDir::Forward))
            }),
            C::QuickSlot(index) => {
                let index = usize::from(index).min(crate::quick_slots::SLOT_COUNT - 1);
                let digit = char::from_digit(index as u32 + 1, 10).unwrap_or('1');
                CommandSpec {
                    title: own(SLOT_TITLES[index]),
                    shortcut: Some(Shortcut::cmd(digit)),
                    keyed: Some(match index {
                        0 => |m| matches!(m, Message::QuickSlotActivate(0)),
                        1 => |m| matches!(m, Message::QuickSlotActivate(1)),
                        2 => |m| matches!(m, Message::QuickSlotActivate(2)),
                        3 => |m| matches!(m, Message::QuickSlotActivate(3)),
                        4 => |m| matches!(m, Message::QuickSlotActivate(4)),
                        5 => |m| matches!(m, Message::QuickSlotActivate(5)),
                        6 => |m| matches!(m, Message::QuickSlotActivate(6)),
                        7 => |m| matches!(m, Message::QuickSlotActivate(7)),
                        _ => |m| matches!(m, Message::QuickSlotActivate(8)),
                    }),
                }
            }
            C::NextSlot => keyed(own("Next Slot"), Shortcut::with(Key::Down, CMD), |m| {
                matches!(m, Message::QuickSlotCycle(1))
            }),
            C::PreviousSlot => keyed(own("Previous Slot"), Shortcut::with(Key::Up, CMD), |m| {
                matches!(m, Message::QuickSlotCycle(-1))
            }),
            C::KeyboardShortcuts => keyed(p("Show Keyboard Shortcuts"), Shortcut::cmd('/'), |m| {
                matches!(m, Message::ToggleShortcuts)
            }),
            C::Website => pointer(own("rmdv Website"), None),
        }
    }
}

/// The key press a shortcut makes, as the key subscription receives it.
pub(super) fn shortcut_event(shortcut: Shortcut) -> iced::Event {
    use iced::keyboard::{self, key::Named, key::Physical, Modifiers};
    let (key, code) = match shortcut.key {
        Key::Char(c) => (
            iced::keyboard::Key::Character(c.to_string().into()),
            char_code(c),
        ),
        Key::Up => (
            iced::keyboard::Key::Named(Named::ArrowUp),
            Some(keyboard::key::Code::ArrowUp),
        ),
        Key::Down => (
            iced::keyboard::Key::Named(Named::ArrowDown),
            Some(keyboard::key::Code::ArrowDown),
        ),
    };
    let mut modifiers = Modifiers::empty();
    if shortcut.mods.command {
        modifiers |= Modifiers::COMMAND;
    }
    if shortcut.mods.shift {
        modifiers |= Modifiers::SHIFT;
    }
    if shortcut.mods.option {
        modifiers |= Modifiers::ALT;
    }
    if shortcut.mods.control {
        modifiers |= Modifiers::CTRL;
    }
    iced::Event::Keyboard(keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: code.map_or(
            Physical::Unidentified(keyboard::key::NativeCode::Unidentified),
            Physical::Code,
        ),
        location: keyboard::Location::Standard,
        modifiers,
        text: None,
        repeat: false,
    })
}

/// The US-layout physical key of a shortcut character.
fn char_code(c: char) -> Option<iced::keyboard::key::Code> {
    use iced::keyboard::key::Code;
    Some(match c.to_ascii_lowercase() {
        'a' => Code::KeyA,
        'b' => Code::KeyB,
        'c' => Code::KeyC,
        'd' => Code::KeyD,
        'e' => Code::KeyE,
        'f' => Code::KeyF,
        'g' => Code::KeyG,
        'h' => Code::KeyH,
        'i' => Code::KeyI,
        'j' => Code::KeyJ,
        'k' => Code::KeyK,
        'l' => Code::KeyL,
        'm' => Code::KeyM,
        'n' => Code::KeyN,
        'o' => Code::KeyO,
        'p' => Code::KeyP,
        'q' => Code::KeyQ,
        'r' => Code::KeyR,
        's' => Code::KeyS,
        't' => Code::KeyT,
        'u' => Code::KeyU,
        'v' => Code::KeyV,
        'w' => Code::KeyW,
        'x' => Code::KeyX,
        'y' => Code::KeyY,
        'z' => Code::KeyZ,
        '0' => Code::Digit0,
        '1' => Code::Digit1,
        '2' => Code::Digit2,
        '3' => Code::Digit3,
        '4' => Code::Digit4,
        '5' => Code::Digit5,
        '6' => Code::Digit6,
        '7' => Code::Digit7,
        '8' => Code::Digit8,
        '9' => Code::Digit9,
        ',' => Code::Comma,
        '.' => Code::Period,
        '/' => Code::Slash,
        '[' => Code::BracketLeft,
        ']' => Code::BracketRight,
        '-' => Code::Minus,
        '=' | '+' => Code::Equal,
        _ => return None,
    })
}

/// Whether Open Recent may open a path now: where ⌘P could open a file.
fn recent_allowed(context: KeyContext) -> bool {
    matches!(
        key_message(context, shortcut_event(Shortcut::cmd('p')), false),
        Message::OpenFileFinder
    )
}

fn native(title: &str, shortcut: Option<Shortcut>, action: Native) -> Entry {
    Entry::Item {
        title: title.to_string(),
        shortcut,
        action: Action::Native(action),
    }
}

impl App {
    /// The menu bar: the app menu, File, Edit, View, Go, Window, Help.
    pub(super) fn menu_bar(&self) -> Vec<Menu> {
        use MenuCommand as C;
        let item = |command| self.menu_item(command);
        let menu = |title: &str, role, entries| Menu {
            title: title.to_string(),
            role,
            entries,
        };
        let app = menu(
            "rmdv",
            Role::Plain,
            vec![
                native("About rmdv", None, Native::About),
                Entry::Separator,
                item(C::Settings),
                Entry::Separator,
                Entry::Submenu(menu("Services", Role::Services, Vec::new())),
                Entry::Separator,
                native("Hide rmdv", Some(Shortcut::cmd('h')), Native::Hide),
                native(
                    "Hide Others",
                    Some(Shortcut::cmd_option('h')),
                    Native::HideOthers,
                ),
                native("Show All", None, Native::ShowAll),
                Entry::Separator,
                // `terminate:` reaches `macos_quit`, which asks about
                // unsaved edits first.
                native("Quit rmdv", Some(Shortcut::cmd('q')), Native::Quit),
            ],
        );
        let file = menu(
            "File",
            Role::Plain,
            vec![
                item(C::OpenFolder),
                Entry::Submenu(menu("Open Recent", Role::Recent, Vec::new())),
                item(C::FindFile),
                Entry::Separator,
                item(C::Save),
                Entry::Separator,
                item(C::CloseSlot),
                item(C::CloseWindow),
                Entry::Separator,
                item(C::Refresh),
                item(C::RevealInFinder),
                item(C::CopyPath),
            ],
        );
        let edit = menu(
            "Edit",
            Role::Plain,
            vec![
                item(C::Undo),
                item(C::Redo),
                Entry::Separator,
                item(C::Cut),
                item(C::Copy),
                item(C::Paste),
                item(C::SelectAll),
                Entry::Separator,
                item(C::Find),
                item(C::FindInVault),
            ],
        );
        let view = menu(
            "View",
            Role::Plain,
            vec![
                item(C::View(ToolbarView::Read)),
                item(C::View(ToolbarView::Edit)),
                item(C::View(ToolbarView::Mindmap)),
                item(C::View(ToolbarView::FullMindmap)),
                Entry::Separator,
                item(C::ToggleSidebar),
                item(C::AlwaysShowToolbar),
                item(C::ToggleFooter),
                item(C::ShowHidden),
                Entry::Separator,
                item(C::FontUp),
                item(C::FontDown),
                item(C::FontReset),
                Entry::Separator,
                item(C::CycleTheme),
                item(C::PickTheme),
                item(C::CommandPalette),
                Entry::Separator,
                // No ⌃⌘F: a native key is permanent, and ⌃⌘F is Find in
                // Document (Control reads as Command).
                native("Enter Full Screen", None, Native::ToggleFullScreen),
            ],
        );
        let slots = (0..crate::quick_slots::SLOT_COUNT as u8)
            .map(|index| item(C::QuickSlot(index)))
            .collect();
        let go = menu(
            "Go",
            Role::Plain,
            vec![
                item(C::Back),
                item(C::Forward),
                Entry::Separator,
                Entry::Submenu(menu("Quick Slots", Role::Plain, slots)),
                item(C::NextSlot),
                item(C::PreviousSlot),
            ],
        );
        // Minimize has no ⌘M: ⌘M is Mind Map.
        let window = menu(
            "Window",
            Role::Windows,
            vec![
                native("Minimize", None, Native::Minimize),
                native("Zoom", None, Native::Zoom),
                Entry::Separator,
                native("Bring All to Front", None, Native::BringAllToFront),
            ],
        );
        let help = menu(
            "Help",
            Role::Help,
            vec![item(C::KeyboardShortcuts), item(C::Website)],
        );
        vec![app, file, edit, view, go, window, help]
    }

    /// An item's title: the palette's name when the palette lists it.
    fn menu_title(&self, title: Title) -> String {
        match title {
            Title::Own(own) => own.to_string(),
            Title::Palette(name) => self
                .command_items()
                .into_iter()
                .map(|(label, _)| split_command_label(label).0)
                .find(|palette| *palette == name)
                .unwrap_or(name)
                .to_string(),
        }
    }

    fn menu_item(&self, command: MenuCommand) -> Entry {
        let spec = command.spec();
        Entry::Item {
            title: self.menu_title(spec.title),
            shortcut: spec.shortcut,
            action: Action::Command(command.tag()),
        }
    }

    /// What a click on `command` runs now; `None`: the item is disabled.
    /// An item with a shortcut runs what its key runs on the surface that
    /// has the keys now, so a click never reaches past a text field, the
    /// editor, a modal card, or an open menu.
    pub(super) fn menu_message(&self, command: MenuCommand) -> Option<Message> {
        self.menu_message_in(self.key_context(), command)
    }

    fn menu_message_in(&self, context: KeyContext, command: MenuCommand) -> Option<Message> {
        use MenuCommand as C;
        let spec = command.spec();
        let key = |shortcut: Shortcut| key_message(context, shortcut_event(shortcut), false);
        if let (Some(accept), Some(shortcut)) = (spec.keyed, spec.shortcut) {
            let message = key(shortcut);
            if !accept(&message) {
                return None;
            }
            let enabled = match command {
                C::Save => self.dirty,
                C::Back => self.can_go(HistoryDir::Back),
                C::Forward => self.can_go(HistoryDir::Forward),
                C::QuickSlot(index) => self.quick_slots.occupied(usize::from(index)).is_some(),
                C::NextSlot | C::PreviousSlot => (0..crate::quick_slots::SLOT_COUNT)
                    .any(|index| self.quick_slots.occupied(index).is_some()),
                _ => true,
            };
            return enabled.then(|| match command {
                // Settings… opens the page; its key toggles it.
                C::Settings => Message::OpenSettings,
                _ => message,
            });
        }
        // Items without a key of their own wait, as keys do, while a menu
        // or a modal card is open.
        if context.menu_open
            || context.slot_replace_open
            || context.close_confirm_open
            || context.renaming
        {
            return None;
        }
        let palette_allowed =
            || matches!(key(Shortcut::cmd_shift('p')), Message::OpenCommandPalette);
        // The editor's text, unless a search field has the keys or a page
        // (the vault search, Settings) covers the editor.
        let editor_keys = context.editing
            && !context.focused
            && !context.overlay_open
            && !context.vault_open
            && !context.settings_open;
        // The cursor, not the selected text: a whole-file selection would
        // be copied on every update.
        let editor_selected = editor_keys
            && self.editor.as_ref().is_some_and(|editor| {
                let cursor = editor.cursor();
                cursor.selection.is_some_and(|end| end != cursor.position)
            });
        let has_file = self.file.is_some();
        let full_mindmap = self.full_mindmap.is_some();
        match command {
            C::Cut => editor_selected.then_some(Message::EditorMenuCut),
            C::Copy if editor_keys => editor_selected.then_some(Message::EditorMenuCopy),
            // The reader copies a whole-document selection, as ⌘C does; a
            // partial selection is its text widget's own.
            C::Copy => matches!(key(Shortcut::cmd('c')), Message::CopyDocument)
                .then_some(Message::CopyDocument),
            C::Paste => editor_keys.then_some(Message::EditorMenuPaste),
            C::SelectAll if editor_keys => Some(Message::EditorAction(
                iced::widget::text_editor::Action::SelectAll,
            )),
            C::SelectAll => (matches!(key(Shortcut::cmd('a')), Message::SelectAllDocument)
                && self.reader_shows_document()
                && !self.ast.is_empty())
            .then_some(Message::SelectAllDocument),
            // As the toolbar's view switch enables its segments.
            C::View(view) => {
                let enabled = match view {
                    ToolbarView::Read => has_file || full_mindmap,
                    ToolbarView::Edit | ToolbarView::Mindmap => has_file,
                    ToolbarView::FullMindmap => has_file || self.workspace.is_some(),
                };
                (enabled && !context.overlay_open).then_some(Message::ToolbarView(view))
            }
            C::AlwaysShowToolbar => (!context.overlay_open)
                .then_some(Message::SetToolbarMode(self.prefs.toolbar.toggled())),
            // Palette commands: where the palette could open.
            C::ToggleFooter => palette_allowed().then_some(Message::ToggleFooter),
            C::PickTheme => palette_allowed().then_some(Message::OpenThemePicker),
            C::Website => Some(Message::OpenLink(WEBSITE.to_string())),
            _ => None,
        }
    }

    /// The Open Recent paths: folders, then files, as the welcome page
    /// lists them.
    pub(super) fn menu_recent(&self) -> Vec<PathBuf> {
        self.recent
            .folders
            .iter()
            .chain(&self.recent.paths)
            .cloned()
            .collect()
    }

    /// What a click on Open Recent item `index` runs: it opens the path,
    /// where ⌘P could open a file.
    fn recent_message(&self, index: usize) -> Option<Message> {
        let path = self.menu_recent().into_iter().nth(index)?;
        recent_allowed(self.key_context()).then_some(Message::OpenFileFinderPath(path))
    }

    /// A click on a menu bar item: run what the item stands for now.
    pub(super) fn native_menu_click(&mut self, tag: u32) -> Task<Message> {
        let message = match tag.checked_sub(native_menu::RECENT_TAG) {
            Some(index) => self.recent_message(index as usize),
            None => MenuCommand::from_tag(tag).and_then(|command| self.menu_message(command)),
        };
        match message {
            Some(message) => self.update(message),
            None => Task::none(),
        }
    }

    /// Which items are enabled and checked now.
    pub(super) fn menu_state(&self) -> native_menu::State {
        let context = self.key_context();
        let mut state = native_menu::State::default();
        for command in MenuCommand::ALL {
            let bit = native_menu::State::bit(command.tag());
            if self.menu_message_in(context, command).is_some() {
                state.enabled |= bit;
            }
            if self.menu_checked(command) {
                state.checked |= bit;
            }
        }
        if recent_allowed(context) {
            state.enabled |= native_menu::State::bit(0);
        }
        state
    }

    /// Whether `command`'s item shows a check: the view on screen and the
    /// settings that are on.
    fn menu_checked(&self, command: MenuCommand) -> bool {
        use MenuCommand as C;
        let full_mindmap = self.full_mindmap.is_some();
        match command {
            C::View(view) => {
                let current = match self.view_mode {
                    _ if full_mindmap => ToolbarView::FullMindmap,
                    ViewMode::Rendered => ToolbarView::Read,
                    ViewMode::Raw => ToolbarView::Edit,
                    ViewMode::Mindmap => ToolbarView::Mindmap,
                };
                view == current
            }
            C::ToggleSidebar => self.sidebar_open && !full_mindmap,
            C::AlwaysShowToolbar => self.prefs.toolbar == crate::prefs::ToolbarMode::Always,
            C::ToggleFooter => self.show_footer,
            C::ShowHidden => self.show_hidden,
            C::QuickSlot(index) => self.quick_slots.active == Some(usize::from(index)),
            _ => false,
        }
    }

    /// The Quick Slot titles now, by tag: a slot names its file.
    pub(super) fn menu_titles(&self) -> Vec<(u32, String)> {
        (0..crate::quick_slots::SLOT_COUNT as u8)
            .map(|index| {
                let base = SLOT_TITLES[usize::from(index)];
                let title = match self.quick_slots.occupied(usize::from(index)) {
                    Some(slot) => {
                        let name = Path::new(&slot.relative_path)
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_else(|| slot.relative_path.clone());
                        format!("{base}: {name}")
                    }
                    None => base.to_string(),
                };
                (MenuCommand::QuickSlot(index).tag(), title)
            })
            .collect()
    }

    /// Push the item state, the slot titles, and the recent list to the
    /// menu bar, each only when it changed. AppKit reads them when it
    /// shows a menu; nothing is rebuilt here.
    pub(super) fn sync_native_menu(&mut self) {
        if !cfg!(test) && !native_menu::installed() {
            return;
        }
        self.menu_pushed.computes += 1;
        let state = self.menu_state();
        if self.menu_pushed.state != Some(state) {
            native_menu::set_state(state);
            self.menu_pushed.state = Some(state);
            self.menu_pushed.pushes[0] += 1;
        }
        let titles = self.menu_titles();
        if self.menu_pushed.titles.as_ref() != Some(&titles) {
            native_menu::set_titles(titles.clone());
            self.menu_pushed.titles = Some(titles);
            self.menu_pushed.pushes[1] += 1;
        }
        let recent = self.menu_recent();
        if self.menu_pushed.recent.as_ref() != Some(&recent) {
            let home = dirs::home_dir();
            native_menu::set_recent(
                recent
                    .iter()
                    .map(|path| recent_title(path, home.as_deref()))
                    .collect(),
            );
            self.menu_pushed.recent = Some(recent);
            self.menu_pushed.pushes[2] += 1;
        }
    }

    /// Install the menu bar once, after NSApp and the window exist.
    pub(super) fn install_native_menu(&self) {
        if cfg!(target_os = "macos") && !native_menu::installed() {
            native_menu::install(&self.menu_bar());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(menus: &[Menu]) -> Vec<(String, Option<Shortcut>, Action)> {
        fn walk(entries: &[Entry], out: &mut Vec<(String, Option<Shortcut>, Action)>) {
            for entry in entries {
                match entry {
                    Entry::Item {
                        title,
                        shortcut,
                        action,
                    } => out.push((title.clone(), *shortcut, *action)),
                    Entry::Separator => {}
                    Entry::Submenu(menu) => walk(&menu.entries, out),
                }
            }
        }
        let mut out = Vec::new();
        for menu in menus {
            walk(&menu.entries, &mut out);
        }
        out
    }

    fn find_menu<'a>(menus: &'a [Menu], title: &str) -> &'a Menu {
        fn walk<'a>(menu: &'a Menu, title: &str) -> Option<&'a Menu> {
            if menu.title == title {
                return Some(menu);
            }
            menu.entries.iter().find_map(|entry| match entry {
                Entry::Submenu(sub) => walk(sub, title),
                _ => None,
            })
        }
        menus
            .iter()
            .find_map(|menu| walk(menu, title))
            .unwrap_or_else(|| panic!("menu {title}"))
    }

    fn item<'a>(menu: &'a Menu, title: &str) -> (&'a Option<Shortcut>, &'a Action) {
        menu.entries
            .iter()
            .find_map(|entry| match entry {
                Entry::Item {
                    title: t,
                    shortcut,
                    action,
                } if t == title => Some((shortcut, action)),
                _ => None,
            })
            .unwrap_or_else(|| panic!("item {title} in {}", menu.title))
    }

    /// A reader on a file inside a workspace: the state where every
    /// palette command and most shortcuts act.
    pub(in crate::app) fn reader_app() -> App {
        let mut app = App::default();
        app.workspace = Some(PathBuf::from("/tmp/menu-bar-ws"));
        let _ = app.update(Message::FileLoaded(Ok((
            PathBuf::from("/tmp/menu-bar-ws/a.md"),
            "# A\n\nText.\n".into(),
        ))));
        app
    }

    /// The same file in the Zen editor.
    pub(in crate::app) fn editing_app() -> App {
        let mut app = reader_app();
        let _ = app.update(Message::ToggleViewMode);
        assert_eq!(app.view_mode, ViewMode::Raw);
        assert!(app.editor.is_some());
        app
    }

    #[test]
    fn every_menu_command_is_listed_once_and_its_tag_round_trips() {
        let app = reader_app();
        let menus = app.menu_bar();
        let tags: Vec<u32> = items(&menus)
            .into_iter()
            .filter_map(|(_, _, action)| match action {
                Action::Command(tag) => Some(tag),
                Action::Native(_) => None,
            })
            .collect();
        for command in MenuCommand::ALL {
            let tag = command.tag();
            assert!((1..=native_menu::MAX_COMMAND_TAG).contains(&tag));
            assert_eq!(MenuCommand::from_tag(tag), Some(command));
            assert_eq!(
                tags.iter().filter(|t| **t == tag).count(),
                1,
                "{command:?} must be in the menu bar once"
            );
        }
        assert_eq!(tags.len(), MenuCommand::ALL.len());
        assert_eq!(MenuCommand::from_tag(0), None);
        assert_eq!(
            MenuCommand::from_tag(MenuCommand::ALL.len() as u32 + 1),
            None
        );
    }

    #[test]
    fn palette_titles_and_shortcuts_match_command_items() {
        let app = reader_app();
        let palette: Vec<(&str, Option<&str>, Message)> = app
            .command_items()
            .into_iter()
            .map(|(label, message)| {
                let (name, keys) = split_command_label(label);
                (name, keys, message)
            })
            .collect();
        let menu_titles: Vec<String> = items(&app.menu_bar())
            .into_iter()
            .map(|(title, _, _)| title)
            .collect();
        let mut checked = 0;
        for command in MenuCommand::ALL {
            let spec = command.spec();
            let Title::Palette(name) = spec.title else {
                continue;
            };
            checked += 1;
            let (_, keys, message) = palette
                .iter()
                .find(|(n, _, _)| *n == name)
                .unwrap_or_else(|| panic!("the palette has no {name:?}"));
            assert!(menu_titles.iter().any(|t| t == name), "{name} title");
            match (keys, spec.shortcut) {
                (None, None) => {}
                (Some(keys), Some(shortcut)) => {
                    let mut mods = std::collections::BTreeSet::new();
                    let mut key = String::new();
                    for c in keys.chars() {
                        if "⌘⌥⇧⌃".contains(c) {
                            mods.insert(c);
                        } else {
                            key.push(c.to_ascii_uppercase());
                        }
                    }
                    assert_eq!(shortcut.glyphs(), (mods, key), "{name} shortcut");
                }
                other => panic!("{name}: palette and menu shortcuts differ: {other:?}"),
            }
            // The click runs the palette's command.
            if let Some(accept) = spec.keyed {
                assert!(accept(message), "{name}: palette runs {message:?}");
            }
        }
        assert!(checked >= 15, "most items name a palette command");
    }

    #[test]
    fn every_shortcut_is_the_key_its_command_has() {
        let reader = reader_app();
        let editing = editing_app();
        for command in MenuCommand::ALL {
            let spec = command.spec();
            let (Some(accept), Some(shortcut)) = (spec.keyed, spec.shortcut) else {
                continue;
            };
            let event = shortcut_event(shortcut);
            let in_reader = key_message(reader.key_context(), event.clone(), false);
            let in_editor = key_message(editing.key_context(), event, false);
            assert!(
                accept(&in_reader) || accept(&in_editor),
                "{command:?}: the key gives {in_reader:?} / {in_editor:?}"
            );
        }
    }

    #[test]
    fn menu_bar_has_the_standard_macos_items() {
        let menus = reader_app().menu_bar();
        let titles: Vec<&str> = menus.iter().map(|menu| menu.title.as_str()).collect();
        assert_eq!(
            titles,
            ["rmdv", "File", "Edit", "View", "Go", "Window", "Help"]
        );
        let app_menu = &menus[0];
        assert_eq!(
            item(app_menu, "About rmdv").1,
            &Action::Native(Native::About)
        );
        assert_eq!(item(app_menu, "Settings…").0, &Some(Shortcut::cmd(',')));
        assert_eq!(
            item(app_menu, "Hide rmdv"),
            (&Some(Shortcut::cmd('h')), &Action::Native(Native::Hide))
        );
        assert_eq!(
            item(app_menu, "Hide Others"),
            (
                &Some(Shortcut::cmd_option('h')),
                &Action::Native(Native::HideOthers)
            )
        );
        assert_eq!(
            item(app_menu, "Show All").1,
            &Action::Native(Native::ShowAll)
        );
        // Quit goes through `terminate:`, which asks about unsaved edits.
        assert_eq!(
            item(app_menu, "Quit rmdv"),
            (&Some(Shortcut::cmd('q')), &Action::Native(Native::Quit))
        );
        assert_eq!(find_menu(&menus, "Services").role, Role::Services);
        assert_eq!(find_menu(&menus, "Open Recent").role, Role::Recent);
        // ⌘M stays Mind Map and ⌘W stays Close Slot.
        let window = find_menu(&menus, "Window");
        assert_eq!(window.role, Role::Windows);
        assert_eq!(
            item(window, "Minimize"),
            (&None, &Action::Native(Native::Minimize))
        );
        assert_eq!(item(window, "Zoom").1, &Action::Native(Native::Zoom));
        assert_eq!(
            item(window, "Bring All to Front").1,
            &Action::Native(Native::BringAllToFront)
        );
        let file = find_menu(&menus, "File");
        assert_eq!(item(file, "Close Slot").0, &Some(Shortcut::cmd('w')));
        let view = find_menu(&menus, "View");
        assert_eq!(item(view, "Mind Map").0, &Some(Shortcut::cmd('m')));
        assert_eq!(
            item(view, "Enter Full Screen"),
            (&None, &Action::Native(Native::ToggleFullScreen))
        );
        assert_eq!(find_menu(&menus, "Help").role, Role::Help);
        assert_eq!(find_menu(&menus, "Quick Slots").entries.len(), 9);
    }

    /// A native item's key equivalent is permanent: AppKit takes the key
    /// before the window sees it. Only the keys winit's menu already took
    /// (Hide, Hide Others, Quit) may stay; no other native item may take a
    /// key the app's key map binds, such as ⌃⌘F (Find in Document through
    /// the Control-as-Command alias).
    #[test]
    fn native_items_take_no_key_the_app_binds() {
        let reader = reader_app();
        let editing = editing_app();
        for (title, shortcut, action) in items(&reader.menu_bar()) {
            let (Some(shortcut), Action::Native(native)) = (shortcut, action) else {
                continue;
            };
            if matches!(native, Native::Hide | Native::HideOthers | Native::Quit) {
                continue;
            }
            for app in [&reader, &editing] {
                let key = key_message(app.key_context(), shortcut_event(shortcut), false);
                assert!(
                    matches!(key, Message::Noop),
                    "{title} takes {:?}, which runs {key:?}",
                    shortcut.glyphs()
                );
            }
        }
        // ⌃⌘F still finds in the document.
        let find = Shortcut::cmd_control('f');
        for app in [&reader, &editing] {
            assert!(matches!(
                key_message(app.key_context(), shortcut_event(find), false),
                Message::ToggleSearch
            ));
        }
    }

    #[test]
    fn no_two_menu_items_show_the_same_shortcut() {
        let all = items(&reader_app().menu_bar());
        let mut seen: Vec<(Shortcut, String)> = Vec::new();
        for (title, shortcut, _) in all {
            let Some(shortcut) = shortcut else {
                continue;
            };
            if let Some((_, other)) = seen.iter().find(|(s, _)| *s == shortcut) {
                panic!("{title} and {other} both show {:?}", shortcut.glyphs());
            }
            seen.push((shortcut, title));
        }
    }

    fn show(message: &Option<Message>) -> String {
        format!("{message:?}")
    }

    /// The states the parity test walks: each surface that owns the keys
    /// differently.
    fn surfaces() -> Vec<(&'static str, App)> {
        let mut out = vec![("reader", reader_app()), ("editing", editing_app())];
        let mut welcome = App::default();
        welcome.workspace = None;
        out.push(("welcome", welcome));
        let mut settings = reader_app();
        let _ = settings.update(Message::OpenSettings);
        assert!(settings.settings_open);
        out.push(("settings", settings));
        let mut vault = reader_app();
        let _ = vault.update(Message::OpenVaultSearch);
        assert!(vault.vault_open);
        out.push(("vault", vault));
        // The pages cover the Zen editor, which keeps its buffer.
        let mut editing_vault = editing_app();
        let _ = editing_vault.update(Message::OpenVaultSearch);
        assert!(editing_vault.vault_open && editing_vault.editor.is_some());
        out.push(("editing vault", editing_vault));
        let mut editing_settings = editing_app();
        let _ = editing_settings.update(Message::OpenSettings);
        assert!(editing_settings.settings_open && editing_settings.editor.is_some());
        out.push(("editing settings", editing_settings));
        let mut palette = reader_app();
        let _ = palette.update(Message::OpenCommandPalette);
        assert!(palette.overlay != Overlay::None);
        out.push(("palette", palette));
        let mut search = reader_app();
        let _ = search.update(Message::ToggleSearch);
        assert!(search.search_open);
        out.push(("search", search));
        let mut mindmap = reader_app();
        let _ = mindmap.update(Message::ToggleMindmap);
        assert_eq!(mindmap.view_mode, ViewMode::Mindmap);
        out.push(("mindmap", mindmap));
        let mut full = reader_app();
        let _ = full.update(Message::ToggleFullMindmap);
        assert!(full.full_mindmap.is_some());
        out.push(("full mindmap", full));
        let mut select_all = reader_app();
        let _ = select_all.update(Message::SelectAllDocument);
        assert!(select_all.reader_select_all);
        out.push(("select all", select_all));
        let mut menu = reader_app();
        menu.open_context_menu(vec![MenuEntry::item("X", Message::Noop)], None);
        out.push(("context menu", menu));
        let mut confirm = editing_app();
        confirm.dirty = true;
        confirm.close_confirm = Some(CloseConfirm { window: None });
        out.push(("close confirm", confirm));
        out
    }

    /// Each item with a shortcut runs, on a click, what its key runs on
    /// the same surface, and is disabled where the key does something
    /// else or nothing: a click never reaches past the surface that owns
    /// the keys.
    #[test]
    fn a_click_runs_what_the_key_runs_on_every_surface() {
        let mut enabled = 0;
        for (surface, app) in surfaces() {
            for command in MenuCommand::ALL {
                let spec = command.spec();
                let (Some(accept), Some(shortcut)) = (spec.keyed, spec.shortcut) else {
                    continue;
                };
                let key = key_message(app.key_context(), shortcut_event(shortcut), false);
                let click = app.menu_message(command);
                if !accept(&key) {
                    assert!(
                        click.is_none(),
                        "{surface}: {command:?} is {} but the key runs {key:?}",
                        show(&click)
                    );
                    continue;
                }
                let Some(click) = click else {
                    // Enabled-only refinements: Save needs edits, Back and
                    // Forward a history, a slot a file.
                    assert!(
                        matches!(
                            command,
                            MenuCommand::Save
                                | MenuCommand::Back
                                | MenuCommand::Forward
                                | MenuCommand::QuickSlot(_)
                                | MenuCommand::NextSlot
                                | MenuCommand::PreviousSlot
                        ),
                        "{surface}: {command:?} is disabled but the key runs {key:?}"
                    );
                    continue;
                };
                enabled += 1;
                let expected = match command {
                    // Settings… opens the page; the key toggles it.
                    MenuCommand::Settings => format!("{:?}", Message::OpenSettings),
                    _ => format!("{key:?}"),
                };
                assert_eq!(format!("{click:?}"), expected, "{surface}: {command:?}");
            }
        }
        assert!(enabled > 60, "most keyed items act somewhere: {enabled}");
    }

    #[test]
    fn modal_cards_and_open_menus_disable_every_item() {
        for (surface, app) in surfaces() {
            if !matches!(surface, "context menu" | "close confirm") {
                continue;
            }
            for command in MenuCommand::ALL {
                assert!(
                    app.menu_message(command).is_none(),
                    "{surface}: {command:?} must wait for the card"
                );
            }
        }
    }

    #[test]
    fn edit_items_follow_the_focus() {
        use MenuCommand as C;
        let reader = reader_app();
        assert!(reader.menu_message(C::Cut).is_none());
        assert!(reader.menu_message(C::Paste).is_none());
        // The reader copies a whole-document selection; a partial one is
        // the text widget's own (its ⌘C).
        assert!(reader.menu_message(C::Copy).is_none());
        assert!(matches!(
            reader.menu_message(C::SelectAll),
            Some(Message::SelectAllDocument)
        ));
        let mut all = reader_app();
        let _ = all.update(Message::SelectAllDocument);
        assert!(matches!(
            all.menu_message(C::Copy),
            Some(Message::CopyDocument)
        ));

        let mut editing = editing_app();
        assert!(editing.menu_message(C::Cut).is_none(), "nothing selected");
        assert!(editing.menu_message(C::Copy).is_none());
        assert!(matches!(
            editing.menu_message(C::Paste),
            Some(Message::EditorMenuPaste)
        ));
        assert!(matches!(
            editing.menu_message(C::SelectAll),
            Some(Message::EditorAction(
                iced::widget::text_editor::Action::SelectAll
            ))
        ));
        let _ = editing.update(Message::EditorAction(
            iced::widget::text_editor::Action::SelectAll,
        ));
        assert!(matches!(
            editing.menu_message(C::Cut),
            Some(Message::EditorMenuCut)
        ));
        assert!(matches!(
            editing.menu_message(C::Copy),
            Some(Message::EditorMenuCopy)
        ));

        // A focused search field keeps its own text keys.
        let mut search = editing_app();
        let _ = search.update(Message::ToggleSearch);
        for command in [C::Cut, C::Copy, C::Paste, C::SelectAll] {
            assert!(search.menu_message(command).is_none(), "{command:?}");
        }
    }

    /// The vault page and Settings cover the Zen editor and own the keys:
    /// the Edit items must not reach the hidden buffer, with or without a
    /// selection in it.
    #[test]
    fn edit_items_never_reach_an_editor_under_a_page() {
        use MenuCommand as C;
        for (surface, mut app) in surfaces() {
            if !matches!(surface, "editing vault" | "editing settings") {
                continue;
            }
            let _ = app.update(Message::EditorAction(
                iced::widget::text_editor::Action::SelectAll,
            ));
            for command in [C::Cut, C::Copy, C::Paste, C::SelectAll] {
                let click = app.menu_message(command);
                assert!(
                    !matches!(
                        click,
                        Some(
                            Message::EditorMenuCut
                                | Message::EditorMenuCopy
                                | Message::EditorMenuPaste
                                | Message::EditorAction(_)
                        )
                    ),
                    "{surface}: {command:?} reaches the hidden editor: {}",
                    show(&click)
                );
            }
            let source = app.source.clone();
            let _ = app.update(Message::NativeMenu(C::Cut.tag()));
            let _ = app.update(Message::NativeMenu(C::Paste.tag()));
            assert_eq!(app.source, source, "{surface}: the buffer is unchanged");
            assert!(!app.dirty, "{surface}: no edit");
        }
    }

    #[test]
    fn pointer_items_run_their_commands() {
        use MenuCommand as C;
        let reader = reader_app();
        assert!(matches!(
            reader.menu_message(C::View(ToolbarView::Edit)),
            Some(Message::ToolbarView(ToolbarView::Edit))
        ));
        assert!(matches!(
            reader.menu_message(C::View(ToolbarView::FullMindmap)),
            Some(Message::ToolbarView(ToolbarView::FullMindmap))
        ));
        let mut welcome = App::default();
        welcome.workspace = None;
        assert!(welcome.menu_message(C::View(ToolbarView::Edit)).is_none());
        assert!(welcome
            .menu_message(C::View(ToolbarView::FullMindmap))
            .is_none());
        assert!(matches!(
            reader.menu_message(C::AlwaysShowToolbar),
            Some(Message::SetToolbarMode(_))
        ));
        assert!(matches!(
            reader.menu_message(C::ToggleFooter),
            Some(Message::ToggleFooter)
        ));
        assert!(matches!(
            reader.menu_message(C::PickTheme),
            Some(Message::OpenThemePicker)
        ));
        assert!(matches!(
            reader.menu_message(C::Website),
            Some(Message::OpenLink(link)) if link == WEBSITE
        ));
    }

    /// A tag from the menu bar does what the item's message does.
    #[test]
    fn a_menu_tag_runs_like_the_key() {
        use MenuCommand as C;
        let mut by_menu = reader_app();
        let mut by_key = reader_app();
        let sidebar = by_menu.sidebar_open;
        let _ = by_menu.update(Message::NativeMenu(C::ToggleSidebar.tag()));
        let _ = by_key.update(Message::ToggleSidebar);
        assert_ne!(by_menu.sidebar_open, sidebar, "the menu toggled it");
        assert_eq!(by_menu.sidebar_open, by_key.sidebar_open);

        let _ = by_menu.update(Message::NativeMenu(C::FontUp.tag()));
        let _ = by_key.update(Message::FontSizeUp);
        assert_eq!(by_menu.font_scale, by_key.font_scale);

        let _ = by_menu.update(Message::NativeMenu(C::View(ToolbarView::Edit).tag()));
        assert_eq!(by_menu.view_mode, ViewMode::Raw);

        // An unknown tag and a disabled item do nothing.
        let mut confirm = editing_app();
        confirm.dirty = true;
        confirm.close_confirm = Some(CloseConfirm { window: None });
        let open = confirm.sidebar_open;
        let _ = confirm.update(Message::NativeMenu(C::ToggleSidebar.tag()));
        let _ = confirm.update(Message::NativeMenu(0));
        assert_eq!(confirm.sidebar_open, open);
        assert!(confirm.close_confirm.is_some());
    }

    #[test]
    fn open_recent_lists_folders_then_files_and_opens_them() {
        let dir = std::env::temp_dir().join(format!("menu-recent-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("note.md");
        std::fs::write(&file, "# Note\n").expect("write");
        let mut app = reader_app();
        app.recent = crate::recent::Recent {
            paths: vec![file.clone()],
            folders: vec![dir.clone()],
        };
        assert_eq!(app.menu_recent(), vec![dir.clone(), file.clone()]);
        // The load is a task; the click runs the welcome page's message.
        assert!(matches!(
            app.recent_message(1),
            Some(Message::OpenFileFinderPath(path)) if path == file
        ));
        assert!(app.recent_message(2).is_none());
        let mut menu = reader_app();
        menu.recent = app.recent.clone();
        menu.open_context_menu(vec![MenuEntry::item("X", Message::Noop)], None);
        assert!(
            menu.recent_message(0).is_none(),
            "an open menu has the keys"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_menu_state_marks_what_is_on_and_what_acts() {
        use MenuCommand as C;
        let app = reader_app();
        let state = app.menu_state();
        for command in MenuCommand::ALL {
            assert_eq!(
                state.is_enabled(command.tag()),
                app.menu_message(command).is_some(),
                "{command:?} enabled"
            );
        }
        let checked = |command: MenuCommand| state.is_checked(command.tag());
        assert!(checked(C::View(ToolbarView::Read)));
        assert!(!checked(C::View(ToolbarView::Edit)));
        assert_eq!(checked(C::ToggleSidebar), app.sidebar_open);
        assert_eq!(checked(C::ShowHidden), app.show_hidden);
        assert_eq!(checked(C::ToggleFooter), app.show_footer);
        assert_eq!(
            checked(C::AlwaysShowToolbar),
            app.prefs.toolbar == crate::prefs::ToolbarMode::Always
        );
        assert!(state.is_enabled(0), "Open Recent acts in the reader");

        let editing = editing_app().menu_state();
        assert!(editing.is_checked(C::View(ToolbarView::Edit).tag()));
        assert!(!editing.is_checked(C::View(ToolbarView::Read).tag()));
        let mut full = reader_app();
        let _ = full.update(Message::ToggleFullMindmap);
        let full = full.menu_state();
        assert!(full.is_checked(C::View(ToolbarView::FullMindmap).tag()));
        assert!(!full.is_checked(C::ToggleSidebar.tag()), "no sidebar shows");

        let mut confirm = editing_app();
        confirm.dirty = true;
        confirm.close_confirm = Some(CloseConfirm { window: None });
        let confirm = confirm.menu_state();
        assert_eq!(confirm.enabled, 0, "a modal card disables every item");
    }

    #[test]
    fn slot_titles_name_their_files() {
        let mut app = reader_app();
        app.quick_slots.slots[1] = Some(crate::quick_slots::QuickSlot {
            relative_path: "notes/b.md".into(),
            context: Default::default(),
        });
        app.quick_slots.active = Some(1);
        let titles = app.menu_titles();
        let title = |index: u8| {
            titles
                .iter()
                .find(|(tag, _)| *tag == MenuCommand::QuickSlot(index).tag())
                .map(|(_, title)| title.as_str())
        };
        assert_eq!(title(0), Some("Slot 1"));
        assert_eq!(title(1), Some("Slot 2: b.md"));
        assert_eq!(titles.len(), crate::quick_slots::SLOT_COUNT);
        let state = app.menu_state();
        assert!(state.is_checked(MenuCommand::QuickSlot(1).tag()));
        assert!(state.is_enabled(MenuCommand::QuickSlot(1).tag()));
        assert!(!state.is_enabled(MenuCommand::QuickSlot(0).tag()));
    }

    #[test]
    fn the_menu_is_pushed_only_when_it_changes() {
        let mut app = reader_app();
        app.sync_native_menu();
        assert_eq!(
            app.menu_pushed.pushes,
            [1, 1, 1],
            "the first sync pushes all"
        );
        for _ in 0..5 {
            let _ = app.update(Message::Noop);
        }
        assert_eq!(app.menu_pushed.pushes, [1, 1, 1], "nothing changed");
        let _ = app.update(Message::ToggleSidebar);
        assert_eq!(app.menu_pushed.pushes, [2, 1, 1], "only the state changed");
        app.quick_slots.slots[0] = Some(crate::quick_slots::QuickSlot {
            relative_path: "a.md".into(),
            context: Default::default(),
        });
        let _ = app.update(Message::Noop);
        assert_eq!(app.menu_pushed.pushes[1], 2, "a slot title changed");
        app.recent
            .paths
            .insert(0, PathBuf::from("/tmp/menu-bar-ws/z.md"));
        let _ = app.update(Message::Noop);
        assert_eq!(app.menu_pushed.pushes[2], 2, "the recent list changed");
        let _ = app.update(Message::Noop);
        assert_eq!(app.menu_pushed.pushes[2], 2);
    }

    /// Pointer hover and scroll reports arrive at pointer or frame rate and
    /// cannot change the menu: they skip its computation, and indeed leave
    /// it as it was.
    #[test]
    fn pointer_and_scroll_messages_skip_the_menu() {
        use iced::Point;
        let window = iced::window::Id::unique();
        let inert = || {
            vec![
                Message::ListPointerMoved(Point::new(3.0, 4.0)),
                Message::VaultHover(0),
                Message::OverlayHover(0),
                Message::LinkHovered("https://example.com/".into(), 0),
                Message::OutlineHovered(true),
                Message::ReaderScrollbarHover(true),
                Message::ScrollerTick,
                Message::ReaderAutoScrollTick,
                Message::TableScrolled,
                Message::MindmapPanelDragMove(300.0),
                Message::FullMindmapPanelDragMove(300.0),
                Message::WindowMoved(window, Point::new(5.0, 6.0)),
            ]
        };
        for (surface, mut app) in surfaces() {
            app.sync_native_menu();
            let computes = app.menu_pushed.computes;
            let before = (app.menu_state(), app.menu_titles(), app.menu_recent());
            for message in inert() {
                let shown = format!("{message:?}");
                let _ = app.update(message);
                assert_eq!(
                    app.menu_pushed.computes, computes,
                    "{surface}: {shown} recomputed the menu"
                );
            }
            let after = (app.menu_state(), app.menu_titles(), app.menu_recent());
            assert_eq!(
                before, after,
                "{surface}: an inert message changed the menu"
            );
            let _ = app.update(Message::Noop);
            assert_eq!(
                app.menu_pushed.computes,
                computes + 1,
                "{surface}: other messages still sync"
            );
        }
    }

    #[test]
    fn recent_titles_show_the_name_and_the_folder() {
        let home = dirs::home_dir().expect("home");
        assert_eq!(
            recent_title(&home.join("notes/a.md"), Some(&home)),
            "a.md — ~/notes"
        );
        assert_eq!(recent_title(&home.join("a.md"), Some(&home)), "a.md — ~");
        assert_eq!(
            recent_title(Path::new("/srv/x/b.md"), Some(&home)),
            "b.md — /srv/x"
        );
    }
}
