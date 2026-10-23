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
