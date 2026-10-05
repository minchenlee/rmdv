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
/// clean-buffer tab creation/activation, while directional slot chords yield
/// to editor-native arrows/line motion.
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
        return Some(if !modifiers.shift() && !modifiers.alt() {
            Message::QuickSlotActivate(index)
        } else {
            return None;
        });
    }
    if matches!(physical, Physical::Code(Code::KeyN))
        && quick_slots_allowed
        && !modifiers.shift()
        && !modifiers.alt()
    {
        return Some(Message::QuickSlotNew);
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
