//! The context menu layer (MDV-048 P6): a clear backdrop that closes the
//! menu on any press outside it, and the menu itself at the pointer.

use super::menu_sink::MenuSink;
use super::*;
use iced::Vector;

/// Container id of the open context menu's panel.
pub(in crate::app) const CONTEXT_MENU_ID: &str = "context-menu";

/// The open menu over the whole window. A press outside the menu (left,
/// right, or middle) closes it and goes no further; the wheel, hover, and
/// cursor shape stop at the backdrop too, as while a native menu tracks.
/// Keys stop here as well: the key subscription gives them to the menu,
/// and a field focused underneath must not also take them.
pub(in crate::app) fn context_menu_layer<'a>(
    menu: &'a ContextMenu,
    pal: Palette,
) -> Element<'a, Message> {
    let backdrop = mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
        .interaction(iced::mouse::Interaction::Idle)
        .on_press(Message::ContextMenuClose)
        .on_right_press(Message::ContextMenuClose)
        .on_middle_press(Message::ContextMenuClose);
    // Laid out against the window, so the menu can flip and stay inside it.
    let placed = iced::widget::responsive(move |window| {
        let origin = context_menu_origin(menu.at, menu.size(), window);
        iced::widget::pin(context_menu_panel(menu, pal))
            .x(origin.x)
            .y(origin.y)
            .into()
    });
    MenuSink::new(stack![backdrop, placed]).into()
}

fn context_menu_panel<'a>(menu: &'a ContextMenu, pal: Palette) -> Element<'a, Message> {
    let mut list = Column::new().width(Length::Fill);
    // Each item's top edge in the panel, so its hover reports the pointer in
    // one frame for the whole menu.
    let mut top = context_menu::MENU_PAD;
    for (index, entry) in menu.entries.iter().enumerate() {
        list = list.push(match entry {
            MenuEntry::Item {
                label,
                shortcut,
                message,
            } => menu_item(
                index,
                Vector::new(context_menu::MENU_PAD, top),
                label,
                *shortcut,
                message.is_some(),
                menu.highlighted == Some(index),
                pal,
            ),
            MenuEntry::Separator => menu_separator(pal),
        });
        top += entry.height();
    }
    let panel = container(list)
        .id(iced::widget::Id::new(CONTEXT_MENU_ID))
        .padding(context_menu::MENU_PAD)
        .width(Length::Fixed(context_menu::MENU_WIDTH))
        .style(move |_| container::Style {
            background: Some(pal.popover().into()),
            border: Border {
                color: pal.border(),
                width: 1.0,
                radius: theme::radius::MD.into(),
            },
            shadow: theme::shadow::MENU,
            ..Default::default()
        });
    // Opaque: a press on the menu (a disabled item, the padding) never
    // reaches the backdrop.
    iced::widget::opaque(panel)
}

fn menu_item<'a>(
    index: usize,
    offset: Vector,
    label: &'a str,
    shortcut: Option<&'static str>,
    enabled: bool,
    highlighted: bool,
    pal: Palette,
) -> Element<'a, Message> {
    let (fg, hint) = if highlighted {
        (pal.accent_fg, pal.accent_fg)
    } else if enabled {
        (pal.fg, pal.muted)
    } else {
        (pal.subtle, pal.subtle)
    };
    let mut content = irow![
        text(label)
            .size(13)
            .color(fg)
            .wrapping(text::Wrapping::None),
        Space::new().width(Length::Fill),
    ]
    .align_y(iced::Alignment::Center);
    if let Some(shortcut) = shortcut {
        content = content.push(
            text(shortcut)
                .size(12)
                .color(hint)
                .shaping(text::Shaping::Advanced),
        );
    }
    let item = button(content)
        .padding(Padding::from([3, 10]))
        .width(Length::Fill)
        .height(Length::Fixed(context_menu::MENU_ITEM_H))
        .style(move |_, _| button::Style {
            background: highlighted.then(|| Background::Color(pal.accent)),
            text_color: fg,
            border: Border {
                radius: theme::radius::XS.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .on_press_maybe(enabled.then_some(Message::ContextMenuChoose(index)));
    if !enabled {
        return item.into();
    }
    // Always a MouseArea, so a highlight change keeps the item's state. The
    // highlighted item reports no moves: they would change nothing, and each
    // message rebuilds the whole view.
    let area = mouse_area(item);
    if highlighted {
        area.into()
    } else {
        area.on_move(move |at| Message::ContextMenuHover(index, at + offset))
            .into()
    }
}

fn menu_separator<'a>(pal: Palette) -> Element<'a, Message> {
    let line = container(Space::new().width(Length::Fill).height(Length::Fixed(1.0)))
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(pal.border().into()),
            ..Default::default()
        });
    container(line)
        .padding(Padding::from([4, 10]))
        .height(Length::Fixed(context_menu::MENU_SEPARATOR_H))
        .into()
}
