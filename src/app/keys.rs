//! Pure keyboard classification: editor bindings, reader and Quick Slot
//! chords, and the surface predicates that decide which shortcuts apply.

use super::Message;

pub(super) fn editor_key_binding(
    kp: iced::widget::text_editor::KeyPress,
) -> Option<iced::widget::text_editor::Binding<Message>> {
    use iced::keyboard::{key::Named, Key};
    use iced::widget::text_editor::{Binding, Motion};

    if kp.modifiers.command() {
        let motion = match kp.key.as_ref() {
            Key::Named(Named::ArrowLeft) => Some(Motion::Home),
            Key::Named(Named::ArrowRight) => Some(Motion::End),
            Key::Named(Named::ArrowUp) => Some(Motion::DocumentStart),
            Key::Named(Named::ArrowDown) => Some(Motion::DocumentEnd),
            _ => None,
        };
        if let Some(motion) = motion {
            return Some(if kp.modifiers.shift() {
                Binding::Select(motion)
            } else {
                Binding::Move(motion)
            });
        }
    }

    // Only the clipboard and select-all chords have iced bindings. iced has
    // no undo, so ⌘Z/⌘⇧Z/⌘Y fall through to `Insert` of the letter (macOS
    // reports the key's text with ⌘ held) and cancel the app's own undo.
    let cmd_or_ctrl = kp.modifiers.command() || kp.modifiers.control();
    if cmd_or_ctrl {
        let keep = matches!(
            kp.key.to_latin(kp.physical_key),
            Some('c' | 'x' | 'v' | 'a')
        );
        if !keep {
            return None;
        }
    }

    iced::widget::text_editor::Binding::from_key_press(kp)
}

/// Resolve the reader typography shortcuts. Document Mindmap panels render
/// through the same typography, so these chords must remain available there.
pub(super) fn reader_font_size_shortcut(
    key: &iced::keyboard::Key,
    modifiers: iced::keyboard::Modifiers,
) -> Option<Message> {
    if !(modifiers.command() || modifiers.control()) {
        return None;
    }
    let reader_action = match key.as_ref() {
        iced::keyboard::Key::Character("=" | "+") => Message::FontSizeUp,
        iced::keyboard::Key::Character("-") => Message::FontSizeDown,
        iced::keyboard::Key::Character("0") => Message::FontSizeReset,
        _ => return None,
    };
    Some(reader_action)
}

/// ⌘⌥[ narrows and ⌘⌥] widens the reading column (Ctrl+Alt off macOS).
/// The physical bracket keys match too: ⌥ changes the character macOS
/// reports for them.
pub(super) fn reading_width_key_message(
    key: &iced::keyboard::Key,
    physical: iced::keyboard::key::Physical,
    modifiers: iced::keyboard::Modifiers,
) -> Option<Message> {
    use iced::keyboard::key::{Code, Physical};
    let primary = modifiers.command() || modifiers.control();
    if !primary || !modifiers.alt() || modifiers.shift() {
        return None;
    }
    let dir = match (key.as_ref(), physical) {
        (iced::keyboard::Key::Character("["), _) | (_, Physical::Code(Code::BracketLeft)) => -1,
        (iced::keyboard::Key::Character("]"), _) | (_, Physical::Code(Code::BracketRight)) => 1,
        _ => return None,
    };
    Some(Message::ReadingWidthStep(dir))
}

/// The image viewer's plain zoom keys: `=` (or `+`) zooms in, `-` out,
/// and `0` shows 100%. With ⌘ or Ctrl they stay the reader's font keys.
pub(super) fn image_zoom_key_message(
    key: &iced::keyboard::Key,
    modifiers: iced::keyboard::Modifiers,
) -> Option<Message> {
    use crate::mindmap::ZoomCommand;
    if modifiers.command() || modifiers.control() || modifiers.alt() {
        return None;
    }
    let iced::keyboard::Key::Character(c) = key else {
        return None;
    };
    let command = match c.as_str() {
        "=" | "+" => ZoomCommand::In,
        "-" => ZoomCommand::Out,
        "0" => ZoomCommand::Reset,
        _ => return None,
    };
    Some(Message::ImageZoomCommand(command))
}

/// ⌘, (Ctrl+, off macOS) opens and closes the Settings page.
pub(super) fn is_settings_key(
    key: &iced::keyboard::Key,
    modifiers: iced::keyboard::Modifiers,
) -> bool {
    (modifiers.command() || modifiers.control())
        && !modifiers.shift()
        && !modifiers.alt()
        && matches!(key, iced::keyboard::Key::Character(c) if c == ",")
}

pub(super) fn is_shortcuts_key(
    key: &iced::keyboard::Key,
    physical: iced::keyboard::key::Physical,
    modifiers: iced::keyboard::Modifiers,
) -> bool {
    if !(modifiers.command() || modifiers.control()) {
        return false;
    }
    matches!(key, iced::keyboard::Key::Character(c) if c == "/")
        || matches!(
            physical,
            iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Slash)
        )
}

pub(super) fn is_primary_modifier_key(key: &iced::keyboard::Key) -> bool {
    #[cfg(target_os = "macos")]
    {
        return matches!(
            key,
            iced::keyboard::Key::Named(iced::keyboard::key::Named::Super)
        );
    }
    #[cfg(not(target_os = "macos"))]
    matches!(
        key,
        iced::keyboard::Key::Named(iced::keyboard::key::Named::Control)
    )
}

pub(super) fn quick_slots_primary_modifier(modifiers: iced::keyboard::Modifiers) -> bool {
    #[cfg(target_os = "macos")]
    {
        modifiers.command()
    }
    #[cfg(not(target_os = "macos"))]
    {
        modifiers.control()
    }
}

/// Modifier added to the primary key (⌘ on macOS, Ctrl elsewhere) that turns
/// a slot digit from "activate" into "assign the current file". Change the
/// assign chord here and in `QUICK_SLOT_ASSIGN_HINT` only.
pub(super) const QUICK_SLOT_ASSIGN_EXTRA: iced::keyboard::Modifiers =
    iced::keyboard::Modifiers::ALT;

/// Shortcut-sheet label for the assign chord.
pub(super) const QUICK_SLOT_ASSIGN_HINT: &str = "⌥⌘1–9";

/// True when the non-primary modifiers are exactly the assign chord.
pub(super) fn quick_slot_assign_modifier(modifiers: iced::keyboard::Modifiers) -> bool {
    modifiers.shift() == QUICK_SLOT_ASSIGN_EXTRA.shift()
        && modifiers.alt() == QUICK_SLOT_ASSIGN_EXTRA.alt()
}

/// True when no extra modifier is held, so a slot digit activates.
fn quick_slot_activate_modifier(modifiers: iced::keyboard::Modifiers) -> bool {
    !modifiers.shift() && !modifiers.alt()
}

/// The digit-chord action for slot `index`, or `None` for other modifiers.
fn quick_slot_digit_message(index: usize, modifiers: iced::keyboard::Modifiers) -> Option<Message> {
    if quick_slot_activate_modifier(modifiers) {
        Some(Message::QuickSlotActivate(index))
    } else if quick_slot_assign_modifier(modifiers) {
        Some(Message::QuickSlotAssign(index))
    } else {
        None
    }
}

/// Keys while the replace-slot confirmation is open: Esc cancels, Enter
/// replaces, and every other key is swallowed.
pub(super) fn quick_slot_replace_key_message(key: &iced::keyboard::Key) -> Message {
    use iced::keyboard::{key::Named, Key};
    match key {
        Key::Named(Named::Escape) => Message::QuickSlotReplaceCancel,
        Key::Named(Named::Enter) => Message::QuickSlotReplaceConfirm,
        _ => Message::Noop,
    }
}

/// Esc closes the Quick Slots popover while it shows.
pub(super) fn quick_slots_popover_key_message(
    key: &iced::keyboard::Key,
    shown: bool,
) -> Option<Message> {
    use iced::keyboard::{key::Named, Key};
    (shown && matches!(key, Key::Named(Named::Escape))).then_some(Message::QuickSlotsPopover(false))
}

/// Keys while a context menu is open: Up and Down move the highlight,
/// Enter chooses, Esc closes, and every other key is swallowed, as while a
/// native menu tracks.
pub(super) fn context_menu_key_message(key: &iced::keyboard::Key) -> Message {
    use iced::keyboard::{key::Named, Key};
    match key {
        Key::Named(Named::ArrowUp) => Message::ContextMenuMove(-1),
        Key::Named(Named::ArrowDown) => Message::ContextMenuMove(1),
        Key::Named(Named::Enter) => Message::ContextMenuActivate,
        Key::Named(Named::Escape) => Message::ContextMenuClose,
        _ => Message::Noop,
    }
}

/// Keys while a file tree row is renamed in place: the field takes typing
/// and Enter; Esc cancels; every other key is swallowed.
pub(super) fn tree_rename_key_message(key: &iced::keyboard::Key) -> Message {
    use iced::keyboard::{key::Named, Key};
    match key {
        Key::Named(Named::Escape) => Message::TreeRenameCancel,
        _ => Message::Noop,
    }
}

/// Keys while the close confirmation is open: Esc cancels, Enter saves,
/// ⌘D closes without saving, and every other key is swallowed.
pub(super) fn close_confirm_key_message(
    key: &iced::keyboard::Key,
    modifiers: iced::keyboard::Modifiers,
) -> Message {
    use iced::keyboard::{key::Named, Key};
    match key {
        Key::Named(Named::Escape) => Message::CloseConfirmCancel,
        Key::Named(Named::Enter) => Message::CloseConfirmSave,
        Key::Character(c) if modifiers.command() && c.as_str().eq_ignore_ascii_case("d") => {
            Message::CloseConfirmDiscard
        }
        _ => Message::Noop,
    }
}

pub(super) fn quick_slot_digit_index(physical: iced::keyboard::key::Physical) -> Option<usize> {
    use iced::keyboard::key::{Code, Physical};
    let Physical::Code(code) = physical else {
        return None;
    };
    Some(match code {
        Code::Digit1 => 0,
        Code::Digit2 => 1,
        Code::Digit3 => 2,
        Code::Digit4 => 3,
        Code::Digit5 => 4,
        Code::Digit6 => 5,
        Code::Digit7 => 6,
        Code::Digit8 => 7,
        Code::Digit9 => 8,
        _ => return None,
    })
}

pub(super) fn quick_slot_cycle_delta(physical: iced::keyboard::key::Physical) -> Option<i8> {
    use iced::keyboard::key::{Code, Physical};
    let Physical::Code(code) = physical else {
        return None;
    };
    match code {
        Code::ArrowUp => Some(-1),
        Code::ArrowDown => Some(1),
        _ => None,
    }
}

pub(super) fn is_refresh_key(
    key: &iced::keyboard::Key,
    physical: iced::keyboard::key::Physical,
    modifiers: iced::keyboard::Modifiers,
) -> bool {
    if !(modifiers.command() || modifiers.control()) || modifiers.alt() {
        return false;
    }
    matches!(key, iced::keyboard::Key::Character(c) if c == "r" || c == "R")
        || matches!(
            physical,
            iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::KeyR)
        )
}

pub(super) fn is_command_alt_key(
    key: &iced::keyboard::Key,
    physical: iced::keyboard::key::Physical,
    modifiers: iced::keyboard::Modifiers,
    character: &str,
    physical_code: iced::keyboard::key::Code,
) -> bool {
    if !(modifiers.command() || modifiers.control()) || !modifiers.alt() {
        return false;
    }
    matches!(key, iced::keyboard::Key::Character(c) if c.eq_ignore_ascii_case(character))
        || matches!(
            physical,
            iced::keyboard::key::Physical::Code(code) if code == physical_code
        )
}

pub(super) fn is_reveal_file_key(
    key: &iced::keyboard::Key,
    physical: iced::keyboard::key::Physical,
    modifiers: iced::keyboard::Modifiers,
) -> bool {
    is_command_alt_key(
        key,
        physical,
        modifiers,
        "r",
        iced::keyboard::key::Code::KeyR,
    )
}

pub(super) fn is_copy_file_path_key(
    key: &iced::keyboard::Key,
    physical: iced::keyboard::key::Physical,
    modifiers: iced::keyboard::Modifiers,
) -> bool {
    is_command_alt_key(
        key,
        physical,
        modifiers,
        "c",
        iced::keyboard::key::Code::KeyC,
    )
}

/// In document Mindmap mode, higher-level surfaces and the `⌘K` folding chord
/// own the keyboard. Do not resize unseen panel text behind them.
pub(super) fn reader_font_shortcuts_enabled(
    full_mindmap: bool,
    document_mindmap: bool,
    overlay_open: bool,
    search_open: bool,
    fold_chord: bool,
) -> bool {
    !full_mindmap && !fold_chord && (!document_mindmap || (!overlay_open && !search_open))
}

pub(super) fn quick_slots_shortcuts_enabled(
    overlay_open: bool,
    vault_open: bool,
    search_open: bool,
    editing: bool,
    dirty: bool,
) -> bool {
    !overlay_open && !vault_open && !search_open && (!editing || !dirty)
}

pub(super) fn quick_slots_rail_visible(
    rail_revealed: bool,
    overlay_open: bool,
    vault_open: bool,
    search_open: bool,
    editing: bool,
    dirty: bool,
) -> bool {
    rail_revealed && !overlay_open && !vault_open && !search_open && (!editing || !dirty)
}

/// Resolve the physical Quick Slot chord after the surrounding surfaces have
/// been classified. Zen keeps Command+W/Command+Shift+W close semantics and
/// clean-buffer slot activation/assignment, while directional slot chords
/// yield to editor-native arrows/line motion.
pub(super) fn quick_slot_physical_message(
    physical: iced::keyboard::key::Physical,
    modifiers: iced::keyboard::Modifiers,
    quick_slots_allowed: bool,
    editing: bool,
    surface_allowed: bool,
) -> Option<Message> {
    use iced::keyboard::key::{Code, Physical};

    if !surface_allowed || !quick_slots_primary_modifier(modifiers) {
        return None;
    }
    if let Some(index) = quick_slot_digit_index(physical) {
        if !quick_slots_allowed {
            return None;
        }
        return quick_slot_digit_message(index, modifiers);
    }
    if editing {
        return match physical {
            Physical::Code(Code::KeyW) if !modifiers.alt() => Some(if modifiers.shift() {
                Message::QuickSlotCloseWindow
            } else {
                Message::QuickSlotClose
            }),
            _ => None,
        };
    }
    if !quick_slots_allowed {
        return None;
    }
    if !modifiers.shift() && !modifiers.alt() {
        if let Some(delta) = quick_slot_cycle_delta(physical) {
            return Some(Message::QuickSlotCycle(delta));
        }
    }
    if let Physical::Code(Code::KeyW) = physical {
        if modifiers.shift() {
            return Some(Message::QuickSlotCloseWindow);
        }
        if !modifiers.alt() {
            return Some(Message::QuickSlotClose);
        }
    }
    None
}

pub(super) fn fold_level_shortcut(key: &iced::keyboard::Key) -> Option<Message> {
    let iced::keyboard::Key::Character(value) = key else {
        return None;
    };
    let depth = value.chars().next()?.to_digit(10)?;
    (depth <= 6).then_some(Message::FoldToLevel(depth as u8))
}

/// Map a physical key to the ASCII character a US layout would produce.
fn physical_latin(code: iced::keyboard::key::Code) -> Option<char> {
    use iced::keyboard::key::Code;
    Some(match code {
        Code::Comma => ',',
        Code::Period => '.',
        Code::Slash => '/',
        Code::Minus => '-',
        Code::Equal => '=',
        _ => return None,
    })
}

/// Under a non-Latin input source (Zhuyin, Pinyin, Japanese, Cyrillic…) the
/// logical key is a non-Latin character such as "ㄖ", so shortcut matching on
/// `Key::Character("b")` never fires. Replace such a key with the Latin
/// character of the physical key. Latin keys pass through unchanged, so
/// Dvorak/AZERTY layouts keep their own letters.
pub(crate) fn latin_shortcut_key(
    key: iced::keyboard::Key,
    physical: iced::keyboard::key::Physical,
    modifiers: iced::keyboard::Modifiers,
) -> iced::keyboard::Key {
    let iced::keyboard::Key::Character(value) = &key else {
        return key;
    };
    if !value.chars().next().is_some_and(|c| c >= '\u{370}') {
        return key;
    }
    let iced::keyboard::key::Physical::Code(code) = physical else {
        return key;
    };
    let latin = key
        .to_latin(physical)
        .filter(|c| c.is_ascii())
        .or_else(|| physical_latin(code));
    match latin {
        Some(c) if modifiers.shift() && c.is_ascii_alphabetic() => {
            iced::keyboard::Key::Character(c.to_ascii_uppercase().to_string().into())
        }
        Some(c) => iced::keyboard::Key::Character(c.to_string().into()),
        None => key,
    }
}

/// Whole-document selection keys in the reader: ⌘A selects all, ⌘C copies
/// while all is selected, and Esc cancels. `reader_has_keyboard` is false
/// while the editor, a search field, or an overlay owns the keyboard; those
/// keep ⌘A and ⌘C for their own text.
pub(super) fn reader_select_all_key(
    key: &iced::keyboard::Key,
    modifiers: iced::keyboard::Modifiers,
    reader_has_keyboard: bool,
    select_all: bool,
) -> Option<Message> {
    use iced::keyboard::{key::Named, Key};
    if !reader_has_keyboard {
        return None;
    }
    let cmd =
        (modifiers.command() || modifiers.control()) && !modifiers.shift() && !modifiers.alt();
    match key.as_ref() {
        Key::Character("a") if cmd => Some(Message::SelectAllDocument),
        Key::Character("c") if cmd && select_all => Some(Message::CopyDocument),
        Key::Named(Named::Escape) if select_all => Some(Message::ClearSelectAll),
        _ => None,
    }
}

/// While the sidebar divider is dragged: pointer moves set the width and
/// the left release ends the drag, wherever the pointer is. A release the
/// window never saw (the next press, or the window losing focus) ends it
/// too, so the divider never stays stuck to the pointer.
pub(super) fn sidebar_drag_message(event: &iced::Event) -> Option<Message> {
    use iced::mouse::{Button, Event};
    match event {
        iced::Event::Mouse(Event::CursorMoved { position }) => {
            Some(Message::SidebarDragMove(position.x))
        }
        iced::Event::Mouse(Event::ButtonReleased(Button::Left) | Event::ButtonPressed(_))
        | iced::Event::Window(iced::window::Event::Unfocused) => Some(Message::SidebarDragEnd),
        _ => None,
    }
}

/// The mouse side buttons go Back and Forward in navigation history.
/// Presses only: the release would act a second time. `App::history_go`
/// ignores them while a modal is open.
pub(super) fn mouse_history_message(event: &iced::Event) -> Option<Message> {
    use iced::mouse::{Button, Event};
    match event {
        iced::Event::Mouse(Event::ButtonPressed(Button::Back)) => {
            Some(Message::HistoryGo(super::HistoryDir::Back))
        }
        iced::Event::Mouse(Event::ButtonPressed(Button::Forward)) => {
            Some(Message::HistoryGo(super::HistoryDir::Forward))
        }
        _ => None,
    }
}

/// ⌘[ goes Back and ⌘] goes Forward (Ctrl off macOS), as in Safari and
/// Finder. The physical bracket keys match too, for layouts that put
/// another character there.
pub(super) fn history_key_message(
    key: &iced::keyboard::Key,
    physical: iced::keyboard::key::Physical,
    modifiers: iced::keyboard::Modifiers,
) -> Option<Message> {
    use iced::keyboard::key::{Code, Physical};
    let primary = modifiers.command() || modifiers.control();
    if !primary || modifiers.shift() || modifiers.alt() {
        return None;
    }
    let dir = match (key.as_ref(), physical) {
        (iced::keyboard::Key::Character("["), _) | (_, Physical::Code(Code::BracketLeft)) => {
            super::HistoryDir::Back
        }
        (iced::keyboard::Key::Character("]"), _) | (_, Physical::Code(Code::BracketRight)) => {
            super::HistoryDir::Forward
        }
        _ => return None,
    };
    Some(Message::HistoryGo(dir))
}

/// Which surface owns the keyboard, from [`super::App::key_context`]. The
/// key subscription hashes it, and the native menu bar asks the same
/// questions, so a menu item does what its shortcut does.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(super) struct KeyContext {
    pub focused: bool,
    pub overlay_open: bool,
    pub tree_active: bool,
    pub outline_active: bool,
    pub sidebar_open: bool,
    pub editing: bool,
    pub fold_chord: bool,
    pub mindmap: bool,
    pub vault_open: bool,
    pub full_mindmap: bool,
    pub quick_slots_allowed: bool,
    pub settings_open: bool,
    pub slot_replace_open: bool,
    pub select_all: bool,
    pub close_confirm_open: bool,
    pub image_zoom_open: bool,
    pub menu_open: bool,
    pub renaming: bool,
    pub slots_popover: bool,
}

/// The message for one keyboard event on the surface `context` describes.
/// `reader_copied` is true when selectable reader text copied on this ⌘C.
pub(super) fn key_message(context: KeyContext, ev: iced::Event, reader_copied: bool) -> Message {
    use super::{full_mindmap_space_message, MindmapDir, SidebarTab};
    let KeyContext {
        focused,
        overlay_open,
        tree_active,
        outline_active,
        sidebar_open,
        editing,
        fold_chord,
        mindmap,
        vault_open,
        full_mindmap,
        quick_slots_allowed,
        settings_open,
        slot_replace_open,
        select_all,
        close_confirm_open,
        image_zoom_open,
        menu_open,
        renaming,
        slots_popover,
    } = context;
    use iced::keyboard::{key::Named, Event as KEv, Key};
    if reader_copied && !select_all {
        return Message::Noop;
    }
    if let iced::Event::Keyboard(KEv::ModifiersChanged(modifiers)) = ev {
        return Message::QuickSlotsModifier(quick_slots_primary_modifier(modifiers));
    }
    let (key, modified_key, physical, mods, released) = match ev {
        iced::Event::Keyboard(KEv::KeyPressed {
            key,
            modified_key,
            physical_key,
            modifiers,
            ..
        }) => (key, modified_key, physical_key, modifiers, false),
        iced::Event::Keyboard(KEv::KeyReleased {
            key,
            modified_key,
            physical_key,
            modifiers,
            ..
        }) => (key, modified_key, physical_key, modifiers, true),
        _ => return Message::Noop,
    };
    if is_primary_modifier_key(&key) {
        return Message::QuickSlotsModifier(!released);
    }
    // An open context menu owns the keyboard, as a native menu
    // does while it tracks.
    if menu_open {
        return if released {
            Message::Noop
        } else {
            context_menu_key_message(&key)
        };
    }
    // The replace-slot confirmation owns the keyboard: nothing
    // reaches the surfaces underneath it.
    if slot_replace_open {
        return if released {
            Message::Noop
        } else {
            quick_slot_replace_key_message(&key)
        };
    }
    // A non-Latin input source reports e.g. "ㄖ" for the B key;
    // match shortcuts on the physical key's Latin character.
    let key = latin_shortcut_key(key, physical, mods);
    let modified_key = latin_shortcut_key(modified_key, physical, mods);
    // The close confirmation owns the keyboard the same way.
    if close_confirm_open {
        return if released {
            Message::Noop
        } else {
            close_confirm_key_message(&key, mods)
        };
    }
    // The inline rename field takes typing, editing keys, and
    // Enter itself; no shortcut acts on the tree or the reader
    // underneath it. Both confirmations above end a rename.
    if renaming {
        return if released {
            Message::Noop
        } else {
            tree_rename_key_message(&key)
        };
    }
    let cmd = mods.command() || mods.control();
    // Quick Slot chords are physical-key based so alternate
    // layouts cannot turn a digit/arrow into a different action.
    // Never steal overlay/editor/vault input.
    if !released {
        // A pending ⌘K fold chord owns the next key, so ⌘K then
        // ⌘1 still folds instead of activating slot 1.
        let surface_allowed =
            !overlay_open && !vault_open && !settings_open && !focused && !fold_chord;
        if let Some(message) = quick_slot_physical_message(
            physical,
            mods,
            quick_slots_allowed,
            editing,
            surface_allowed,
        ) {
            return message;
        }
    }
    if released {
        return Message::Noop;
    }
    if let Some(message) = quick_slots_popover_key_message(&key, slots_popover) {
        return message;
    }
    if is_settings_key(&key, mods) && !overlay_open {
        return Message::ToggleSettings;
    }
    // The Settings page owns the keyboard like the vault page. A
    // few global commands still pass; they change state the page
    // shows, so it stays current.
    if settings_open && !overlay_open {
        if let Key::Character(c) = &key {
            return match c.as_str() {
                "p" if cmd && mods.shift() => Message::OpenCommandPalette,
                "P" if cmd => Message::OpenCommandPalette,
                "b" if cmd => Message::ToggleSidebar,
                "t" if cmd => Message::ToggleTheme,
                "." if cmd && mods.shift() => Message::ToggleHidden,
                ">" if cmd => Message::ToggleHidden,
                _ => reader_font_size_shortcut(&modified_key, mods).unwrap_or(Message::Noop),
            };
        }
        return match key {
            Key::Named(Named::Escape) => Message::CloseSettings,
            Key::Named(Named::ArrowDown) => Message::SettingsMove(1),
            Key::Named(Named::ArrowUp) => Message::SettingsMove(-1),
            Key::Named(Named::ArrowLeft) => Message::SettingsStep(-1),
            Key::Named(Named::ArrowRight) => Message::SettingsStep(1),
            Key::Named(Named::Space) | Key::Named(Named::Enter) => Message::SettingsActivate,
            _ => Message::Noop,
        };
    }
    if reader_font_shortcuts_enabled(full_mindmap, mindmap, overlay_open, focused, fold_chord) {
        if let Some(message) = reader_font_size_shortcut(&modified_key, mods) {
            return message;
        }
        if let Some(message) = reading_width_key_message(&key, physical, mods) {
            return message;
        }
    }
    // Keep the full navigator's activation separate from document
    // ⌘M. Physical matching handles layouts that emit a shifted
    // character differently on macOS.
    if cmd && mods.shift() {
        use iced::keyboard::key::{Code, Physical};
        if let Physical::Code(Code::KeyM) = physical {
            return Message::ToggleFullMindmap;
        }
    }
    if is_shortcuts_key(&key, physical, mods) {
        return Message::ToggleShortcuts;
    }
    if is_refresh_key(&key, physical, mods) {
        return Message::Refresh;
    }
    if is_reveal_file_key(&key, physical, mods) {
        return Message::RevealFileInFinder;
    }
    if is_copy_file_path_key(&key, physical, mods) {
        return Message::CopyFilePath;
    }
    // ⌘⌥B: alt+letter on macOS swaps the logical char, so match the
    // physical KeyB code instead of the produced character.
    if cmd && mods.alt() {
        use iced::keyboard::key::{Code, Physical};
        if let Physical::Code(Code::KeyB) = physical {
            return if full_mindmap {
                Message::FullMindmapTogglePanel
            } else {
                Message::ToggleMindmapPanel
            };
        }
        if let Physical::Code(Code::KeyW) = physical {
            if full_mindmap {
                return Message::FullMindmapCyclePanelWidth;
            }
            if mindmap {
                return Message::MindmapCyclePanelWidth;
            }
        }
    }
    if fold_chord {
        if let Some(message) = fold_level_shortcut(&key) {
            return message;
        }
        return Message::FoldChordCancel;
    }
    let reader_has_keyboard = !editing
        && !focused
        && !overlay_open
        && !mindmap
        && !full_mindmap
        && !vault_open
        && !settings_open;
    if let Some(message) = reader_select_all_key(&key, mods, reader_has_keyboard, select_all) {
        return message;
    }
    if let Key::Character(c) = &key {
        // Full Mindmap owns the window but retains the useful
        // global/fallback commands. Other document-only chords do
        // nothing while its surface is visible.
        if full_mindmap {
            if let Some(message) = history_key_message(&key, physical, mods) {
                return message;
            }
            return match c.as_str() {
                "p" if cmd && mods.shift() => Message::OpenCommandPalette,
                "P" if cmd => Message::OpenCommandPalette,
                "p" if cmd => Message::OpenFileFinder,
                "o" if cmd => Message::OpenFolderPicker,
                "." if cmd && mods.shift() => Message::ToggleHidden,
                ">" if cmd => Message::ToggleHidden,
                "t" if cmd => Message::ToggleTheme,
                "s" if cmd => Message::SaveFile,
                _ => Message::Noop,
            };
        }
        // Vault search page owns the screen: only ⌘⇧F (re-open,
        // idempotent) passes; every other ⌘-shortcut would mutate
        // state under the page, so swallow it.
        if vault_open {
            if (c.as_str() == "f" || c.as_str() == "F") && cmd && mods.shift() {
                return Message::OpenVaultSearch;
            }
            return Message::Noop;
        }
        if let Some(message) = history_key_message(&key, physical, mods) {
            return message;
        }
        match c.as_str() {
            "p" if cmd && mods.shift() => return Message::OpenCommandPalette,
            "P" if cmd => return Message::OpenCommandPalette,
            "p" if cmd => return Message::OpenFileFinder,
            "k" if cmd && !editing => return Message::FoldChordStart,
            "o" if cmd => return Message::OpenFolderPicker,
            "b" if cmd => return Message::ToggleSidebar,
            // ⌘⇧. — toggle hidden files. Match both '.' and '>'
            // since shift+. produces '>' on many layouts.
            "." if cmd && mods.shift() => return Message::ToggleHidden,
            ">" if cmd => return Message::ToggleHidden,
            // ⌘⇧F — vault-wide search. Match both 'f'+shift and the
            // capital 'F' some layouts emit; ordered before ⌘F.
            "f" | "F" if cmd && mods.shift() => return Message::OpenVaultSearch,
            "f" if cmd => return Message::ToggleSearch,
            "t" if cmd => return Message::ToggleTheme,
            "e" if cmd => return Message::ToggleViewMode,
            "m" if cmd => return Message::ToggleMindmap,
            "c" if cmd && !editing && !overlay_open => return Message::HintSelection,
            "s" if cmd => return Message::SaveFile,
            "z" if cmd && editing && mods.shift() => return Message::EditorRedo,
            "z" if cmd && editing => return Message::EditorUndo,
            "y" if cmd && editing => return Message::EditorRedo,
            _ => {}
        }
    }
    // Vault search page owns Esc/arrows/Enter while open. The query
    // text_input keeps focus but doesn't consume these, so they're
    // handled here at the app key layer (like the overlay did).
    if vault_open {
        return match key {
            Key::Named(Named::Escape) => Message::VaultClose,
            Key::Named(Named::ArrowDown) => Message::VaultMove(1),
            Key::Named(Named::ArrowUp) => Message::VaultMove(-1),
            Key::Named(Named::Enter) => Message::VaultEnter,
            _ => Message::Noop,
        };
    }
    if matches!(&key, Key::Named(Named::Escape)) {
        if overlay_open {
            return Message::CloseOverlay;
        }
        if focused {
            return Message::ToggleSearch;
        }
        if editing {
            return Message::ToggleViewMode;
        }
    }
    if image_zoom_open {
        if let Some(message) = image_zoom_key_message(&key, mods) {
            return message;
        }
    }
    if overlay_open {
        return match key {
            Key::Named(Named::ArrowDown) => Message::OverlayMove(1),
            Key::Named(Named::ArrowUp) => Message::OverlayMove(-1),
            Key::Named(Named::Enter) => Message::OverlayConfirm,
            Key::Named(Named::Space) => Message::OverlayDescend,
            Key::Named(Named::ArrowRight) => Message::OverlayDescend,
            Key::Named(Named::ArrowLeft) => Message::PickerParent,
            _ => Message::Noop,
        };
    }
    if full_mindmap {
        return match key {
            Key::Named(Named::Escape) => Message::ExitFullMindmap,
            Key::Named(Named::ArrowDown) => Message::FullMindmapNavigate(MindmapDir::Down),
            Key::Named(Named::ArrowUp) => Message::FullMindmapNavigate(MindmapDir::Up),
            Key::Named(Named::ArrowLeft) => Message::FullMindmapNavigate(MindmapDir::Left),
            Key::Named(Named::ArrowRight) => Message::FullMindmapNavigate(MindmapDir::Right),
            Key::Named(Named::Space) => full_mindmap_space_message(),
            Key::Named(Named::Enter) if cmd => Message::FullMindmapActivate,
            Key::Named(Named::Enter) => Message::FullMindmapActivate,
            Key::Named(Named::Home) if cmd => Message::FullMindmapSelectRoot,
            Key::Named(Named::Home) => Message::FullMindmapSelectRoot,
            _ => Message::Noop,
        };
    }
    if focused {
        if matches!(&key, Key::Named(Named::Enter)) {
            return if mods.shift() {
                Message::PrevMatch
            } else {
                Message::NextMatch
            };
        }
        return Message::Noop;
    }
    if editing {
        return Message::Noop;
    }
    let m: Option<Message> = match key {
        // Sidebar wins arrow keys when open: keyboard file nav
        // takes priority over mindmap node nav (handled below).
        Key::Named(Named::ArrowDown)
            if mindmap && !overlay_open && !tree_active && !outline_active =>
        {
            Some(Message::MindmapNavigate(MindmapDir::Down))
        }
        Key::Named(Named::ArrowUp)
            if mindmap && !overlay_open && !tree_active && !outline_active =>
        {
            Some(Message::MindmapNavigate(MindmapDir::Up))
        }
        Key::Named(Named::ArrowLeft)
            if mindmap && !overlay_open && !tree_active && !outline_active =>
        {
            Some(Message::MindmapNavigate(MindmapDir::Left))
        }
        Key::Named(Named::ArrowRight)
            if mindmap && !overlay_open && !tree_active && !outline_active =>
        {
            Some(Message::MindmapNavigate(MindmapDir::Right))
        }
        Key::Named(Named::Space) if mindmap && !overlay_open && !tree_active && !outline_active => {
            Some(Message::MindmapToggleSelected)
        }
        Key::Named(Named::ArrowDown) if tree_active => Some(Message::TreeMove(1)),
        Key::Named(Named::ArrowUp) if tree_active => Some(Message::TreeMove(-1)),
        Key::Named(Named::ArrowDown) if outline_active => Some(Message::OutlineMove(1)),
        Key::Named(Named::ArrowUp) if outline_active => Some(Message::OutlineMove(-1)),
        Key::Named(Named::ArrowLeft) if sidebar_open => {
            Some(Message::SetSidebarTab(SidebarTab::Files))
        }
        Key::Named(Named::ArrowRight) if sidebar_open => {
            Some(Message::SetSidebarTab(SidebarTab::Outline))
        }
        Key::Named(Named::Enter) if tree_active => Some(Message::TreeActivate),
        Key::Named(Named::Space) if tree_active => Some(Message::TreeActivate),
        Key::Named(Named::Enter) if outline_active => Some(Message::OutlineActivate),
        Key::Named(Named::Space) if outline_active => Some(Message::OutlineActivate),
        Key::Named(Named::ArrowDown) if mods.command() => Some(Message::ScrollToBottom),
        Key::Named(Named::ArrowUp) if mods.command() => Some(Message::ScrollToTop),
        Key::Named(Named::ArrowDown) => Some(Message::ScrollBy(40.0)),
        Key::Named(Named::ArrowUp) => Some(Message::ScrollBy(-40.0)),
        Key::Named(Named::Space) if mods.shift() => Some(Message::ScrollBy(-400.0)),
        Key::Named(Named::Space) => Some(Message::ScrollBy(400.0)),
        Key::Named(Named::PageDown) => Some(Message::ScrollBy(400.0)),
        Key::Named(Named::PageUp) => Some(Message::ScrollBy(-400.0)),
        Key::Named(Named::Home) => Some(Message::ScrollToTop),
        Key::Named(Named::End) => Some(Message::ScrollToBottom),
        Key::Character(c) => match c.as_str() {
            "j" => Some(Message::ScrollBy(40.0)),
            "k" => Some(Message::ScrollBy(-40.0)),
            "g" => Some(Message::ScrollToTop),
            "G" => Some(Message::ScrollToBottom),
            _ => None,
        },
        _ => None,
    };
    m.unwrap_or(Message::Noop)
}
