//! The context menu (MDV-048 P6): one menu at a time, drawn inside rmdv as
//! a positioned overlay at the pointer (a native `NSMenu` would block the
//! Iced event loop while it tracks).
//!
//! A surface opens a menu on a right press with [`App::open_context_menu`].
//! The root [`super::view::PressWatch`] reports every right press in window
//! coordinates first ([`Message::PointerRightPress`]), so the menu appears
//! where the press was. Each item carries the message it runs; the menu
//! closes before that message runs.

use super::*;

/// Width of every context menu.
pub(super) const MENU_WIDTH: f32 = 232.0;
/// Height of one menu item.
pub(super) const MENU_ITEM_H: f32 = 24.0;
/// Height of a separator, with its space above and below.
pub(super) const MENU_SEPARATOR_H: f32 = 9.0;
/// Space between the menu's edge and its items.
pub(super) const MENU_PAD: f32 = 5.0;
/// The menu keeps this far from the window edges.
const MENU_EDGE_MARGIN: f32 = 6.0;
/// The menu's top-left corner sits this far right of and below the press,
/// so the pointer starts outside the first item, as in a native menu.
const MENU_POINTER_GAP: f32 = 1.0;

/// One line of a context menu.
#[derive(Debug, Clone)]
pub(super) enum MenuEntry {
    /// An item; a disabled item has no message.
    Item {
        label: String,
        shortcut: Option<&'static str>,
        message: Option<Message>,
    },
    Separator,
}

impl MenuEntry {
    pub(super) fn item(label: impl Into<String>, message: Message) -> Self {
        MenuEntry::Item {
            label: label.into(),
            shortcut: None,
            message: Some(message),
        }
    }

    /// An item shown greyed out: it does nothing.
    pub(super) fn disabled(label: impl Into<String>) -> Self {
        MenuEntry::Item {
            label: label.into(),
            shortcut: None,
            message: None,
        }
    }

    /// The same item, with the shortcut that does the same thing.
    pub(super) fn shortcut(mut self, hint: Option<&'static str>) -> Self {
        if let MenuEntry::Item { shortcut, .. } = &mut self {
            *shortcut = hint;
        }
        self
    }

    pub(super) fn enabled(&self) -> bool {
        matches!(
            self,
            MenuEntry::Item {
                message: Some(_),
                ..
            }
        )
    }

    pub(super) fn height(&self) -> f32 {
        match self {
            MenuEntry::Item { .. } => MENU_ITEM_H,
            MenuEntry::Separator => MENU_SEPARATOR_H,
        }
    }
}

/// The open context menu.
#[derive(Debug, Clone)]
pub(super) struct ContextMenu {
    /// The right press that opened it, in window coordinates.
    pub(super) at: iced::Point,
    pub(super) entries: Vec<MenuEntry>,
    /// The highlighted item: by the arrow keys, or by the pointer after it
    /// moves.
    pub(super) highlighted: Option<usize>,
    /// Pointer motion over the menu's items, so a still pointer never
    /// highlights.
    pub(super) pointer: ListPointer,
    /// The file tree row the menu acts on; the row shows a ring while the
    /// menu is open. `None` for the workspace root and other surfaces.
    pub(super) tree_target: Option<PathBuf>,
}

impl ContextMenu {
    /// The menu's size as drawn: fixed item and separator heights.
    pub(super) fn size(&self) -> iced::Size {
        let rows: f32 = self.entries.iter().map(MenuEntry::height).sum();
        iced::Size::new(MENU_WIDTH, rows + 2.0 * MENU_PAD)
    }

    /// The enabled item `delta` steps from the highlighted one, wrapping
    /// around; separators and disabled items are skipped. With nothing
    /// highlighted, Down goes to the first item and Up to the last.
    fn step(&self, delta: isize) -> Option<usize> {
        let enabled: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.enabled())
            .map(|(i, _)| i)
            .collect();
        if enabled.is_empty() {
            return None;
        }
        let len = enabled.len() as isize;
        let next = match self
            .highlighted
            .and_then(|current| enabled.iter().position(|&i| i == current))
        {
            Some(at) => (at as isize + delta).rem_euclid(len),
            None if delta < 0 => len - 1,
            None => 0,
        };
        Some(enabled[next as usize])
    }
}

/// `sections` joined by separators; empty sections are left out.
pub(super) fn menu_sections(sections: Vec<Vec<MenuEntry>>) -> Vec<MenuEntry> {
    let mut entries = Vec::new();
    for section in sections.into_iter().filter(|s| !s.is_empty()) {
        if !entries.is_empty() {
            entries.push(MenuEntry::Separator);
        }
        entries.extend(section);
    }
    entries
}

/// `Some(message)` as an item, `None` as the same item greyed out.
pub(super) fn menu_item(label: &str, message: Option<Message>) -> MenuEntry {
    match message {
        Some(message) => MenuEntry::item(label, message),
        None => MenuEntry::disabled(label),
    }
}

/// Where a menu of `menu` size opened by a press at `at` goes in a window
/// of `window` size: below and right of the press, flipped to the other
/// side of the press when it would cross the window's right or bottom
/// edge, and kept inside the window.
pub(super) fn context_menu_origin(
    at: iced::Point,
    menu: iced::Size,
    window: iced::Size,
) -> iced::Point {
    let axis = |press: f32, extent: f32, room: f32| {
        let mut start = press + MENU_POINTER_GAP;
        if start + extent > room - MENU_EDGE_MARGIN {
            start = press - MENU_POINTER_GAP - extent;
        }
        let max = (room - extent - MENU_EDGE_MARGIN).max(MENU_EDGE_MARGIN);
        start.clamp(MENU_EDGE_MARGIN, max)
    };
    iced::Point::new(
        axis(at.x, menu.width, window.width),
        axis(at.y, menu.height, window.height),
    )
}

/// The parts of the app a context menu belongs to. When one changes (an
/// overlay, a view, a modal, the sidebar, the open document or workspace),
/// the menu no longer matches what is under it and closes. Its items carry
/// targets (a heading, a block, a path) taken from what was under it.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct MenuScope {
    overlay: Overlay,
    view_mode: ViewMode,
    full_mindmap: bool,
    settings_open: bool,
    vault_open: bool,
    search_open: bool,
    sidebar_open: bool,
    sidebar_tab: SidebarTab,
    window_fullscreen: bool,
    modal: (bool, bool),
    file: Option<PathBuf>,
    workspace: Option<PathBuf>,
}

impl App {
    /// Open a context menu with `entries` at the latest right press,
    /// replacing any menu already open.
    pub(super) fn open_context_menu(
        &mut self,
        entries: Vec<MenuEntry>,
        tree_target: Option<PathBuf>,
    ) {
        let at = self.right_press_at.unwrap_or(iced::Point::ORIGIN);
        self.context_menu = Some(ContextMenu {
            at,
            entries,
            highlighted: None,
            pointer: ListPointer::default(),
            tree_target,
        });
    }

    pub(super) fn menu_scope(&self) -> MenuScope {
        MenuScope {
            overlay: self.overlay,
            view_mode: self.view_mode,
            full_mindmap: self.full_mindmap.is_some(),
            settings_open: self.settings_open,
            vault_open: self.vault_open,
            search_open: self.search_open,
            sidebar_open: self.sidebar_open,
            sidebar_tab: self.sidebar_tab,
            window_fullscreen: self.window_fullscreen,
            modal: (
                self.quick_slot_replace.is_some(),
                self.close_confirm.is_some(),
            ),
            file: self.file.clone(),
            workspace: self.workspace.clone(),
        }
    }

    /// Handle a context menu message; `None` for any other message.
    pub(super) fn context_menu_message(&mut self, msg: &Message) -> Option<Task<Message>> {
        let task = match msg {
            Message::PointerRightPress(at) => {
                self.right_press_at = Some(*at);
                // A new press: the elements of the last one are stale.
                self.reader_menu_target = None;
                Task::none()
            }
            Message::ContextMenuHover(index, at) => {
                if let Some(menu) = self.context_menu.as_mut() {
                    // The first report is where the pointer rests: a still
                    // pointer under a freshly opened menu highlights nothing.
                    menu.pointer.moved(*at);
                    let enabled = menu.entries.get(*index).is_some_and(MenuEntry::enabled);
                    if menu.pointer.take_motion() && enabled {
                        menu.highlighted = Some(*index);
                    }
                }
                Task::none()
            }
            Message::ContextMenuMove(delta) => {
                if let Some(menu) = self.context_menu.as_mut() {
                    menu.highlighted = menu.step(*delta).or(menu.highlighted);
                    menu.pointer.settle();
                }
                Task::none()
            }
            Message::ContextMenuActivate => {
                let index = self.context_menu.as_ref().and_then(|menu| menu.highlighted);
                match index {
                    Some(index) => self.choose_context_menu_item(index),
                    // Enter with nothing highlighted closes, as in a native
                    // menu.
                    None => {
                        self.context_menu = None;
                        Task::none()
                    }
                }
            }
            Message::ContextMenuChoose(index) => self.choose_context_menu_item(*index),
            Message::ContextMenuClose => {
                self.context_menu = None;
                Task::none()
            }
            _ => return None,
        };
        Some(task)
    }

    /// Close the menu and run item `index`. A disabled item, a separator, or
    /// a stale index does nothing and leaves the menu open.
    fn choose_context_menu_item(&mut self, index: usize) -> Task<Message> {
        let message = self
            .context_menu
            .as_ref()
            .and_then(|menu| menu.entries.get(index))
            .and_then(|entry| match entry {
                MenuEntry::Item {
                    message: Some(message),
                    ..
                } => Some(message.clone()),
                _ => None,
            });
        let Some(message) = message else {
            return Task::none();
        };
        self.context_menu = None;
        Task::done(message)
    }
}
