//! The Zen editor's context menu (MDV-048 P6): Cut, Copy, Paste, and Select
//! All. The right press reaches the area around the editor (the editor
//! ignores right presses), so the editor keeps its focus and selection.
//! Cut and Paste run as editor edits, so the manual undo stack covers them.

use super::*;
use iced::widget::text_editor::{Action, Edit};

impl App {
    pub(super) fn open_editor_context_menu(&mut self) -> Task<Message> {
        let Some(editor) = self.editor.as_ref() else {
            return Task::none();
        };
        if self.view_mode != ViewMode::Raw {
            return Task::none();
        }
        let selected = editor.selection().is_some_and(|text| !text.is_empty());
        let needs_selection = |label: &str, message: Message| {
            if selected {
                MenuEntry::item(label, message)
            } else {
                MenuEntry::disabled(label)
            }
        };
        let entries = vec![
            needs_selection("Cut", Message::EditorMenuCut).shortcut(Some("⌘X")),
            needs_selection("Copy", Message::EditorMenuCopy).shortcut(Some("⌘C")),
            MenuEntry::item("Paste", Message::EditorMenuPaste).shortcut(Some("⌘V")),
            MenuEntry::Separator,
            MenuEntry::item("Select All", Message::EditorAction(Action::SelectAll))
                .shortcut(Some("⌘A")),
        ];
        self.open_context_menu(entries, None);
        Task::none()
    }

    /// The editor's selected text, if any.
    fn editor_selection(&self) -> Option<String> {
        self.editor
            .as_ref()
            .and_then(|editor| editor.selection())
            .filter(|text| !text.is_empty())
    }

    /// Cut, as ⌘X does: copy the selection, then delete it as an edit.
    pub(super) fn editor_menu_cut(&mut self) -> Task<Message> {
        let Some(text) = self.editor_selection() else {
            return Task::none();
        };
        let delete = self.update(Message::EditorAction(Action::Edit(Edit::Delete)));
        Task::batch([iced::clipboard::write::<Message>(text), delete])
    }

    pub(super) fn editor_menu_copy(&self) -> Task<Message> {
        match self.editor_selection() {
            Some(text) => iced::clipboard::write::<Message>(text),
            None => Task::none(),
        }
    }

    /// Paste, as ⌘V does, once the clipboard is read.
    pub(super) fn editor_pasted(&mut self, text: Option<String>) -> Task<Message> {
        match text.filter(|text| !text.is_empty()) {
            Some(text) => self.update(Message::EditorAction(Action::Edit(Edit::Paste(
                std::sync::Arc::new(text),
            )))),
            None => Task::none(),
        }
    }
}
