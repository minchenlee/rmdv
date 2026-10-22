//! The top toolbar: sidebar toggle, file name with its dirty dot, the
//! Read / Edit / Mind Map switch, and Save, Find, Command Palette, Settings.
//! Every action here also has a keyboard shortcut; the toolbar is the pointer
//! path to it.

use super::*;
// Explicit import: the `column!` macro is otherwise ambiguous between the glob
// import and iced's exported macro.
use iced::widget::column;
use iced::widget::tooltip::Position;

/// Toolbar height.
pub(in crate::app) const TOOLBAR_H: f32 = 32.0;
/// Height of the top band that reveals the hover toolbar. It is a little
/// taller than the toolbar, so the pointer finds it without aiming.
pub(in crate::app) const TOOLBAR_BAND_H: f32 = 40.0;
/// How long the hover toolbar stays after the pointer leaves its band.
pub(in crate::app) const TOOLBAR_HIDE_DELAY: std::time::Duration =
    std::time::Duration::from_millis(500);

/// Space the toolbar leaves at its left edge. On macOS the traffic lights
/// sit over the top-left of the window while it is not fullscreen.
pub(in crate::app) fn toolbar_left_inset(at_window_edge: bool, fullscreen: bool) -> f32 {
    #[cfg(target_os = "macos")]
    {
        if at_window_edge && !fullscreen {
            return 80.0;
        }
    }
    let _ = (at_window_edge, fullscreen);
    10.0
}

/// How the toolbar sits: its fill, whether its top-left corner follows the
/// reader panel's rounded corner, and whether a hairline closes it off from
/// content that scrolls under it.
pub(in crate::app) struct ToolbarLook {
    pub(in crate::app) fill: Color,
    pub(in crate::app) left_inset: f32,
    pub(in crate::app) corner_radius: f32,
    pub(in crate::app) hairline: bool,
}

pub(in crate::app) fn toolbar<'a>(
    app: &'a App,
    pal: Palette,
    look: ToolbarLook,
) -> Element<'a, Message> {
    let full_mindmap = app.full_mindmap.is_some();
    let has_file = app.file.is_some();
    let sidebar_visible = !full_mindmap && app.sidebar_open && app.workspace.is_some();

    // Left: the sidebar toggle while the sidebar is hidden (its header has
    // its own Hide button), then Back and Forward through navigation
    // history, each disabled while it has nowhere to go.
    let mut left = irow![].spacing(2).align_y(iced::Alignment::Center);
    if app.workspace.is_some() && !full_mindmap && !sidebar_visible {
        left = left.push(action_tip(
            tool_button(
                ic::PANEL_LEFT_OPEN,
                Some(Message::ToggleSidebar),
                false,
                pal,
            ),
            "Show Sidebar",
            Some("⌘B"),
            Position::Bottom,
            pal,
        ));
    }
    for (dir, glyph, action, key) in [
        (HistoryDir::Back, ic::CHEVRON_LEFT, "Go Back", "⌘["),
        (HistoryDir::Forward, ic::CHEVRON_RIGHT, "Go Forward", "⌘]"),
    ] {
        left = left.push(action_tip(
            tool_button(
                glyph,
                app.can_go(dir).then_some(Message::HistoryGo(dir)),
                false,
                pal,
            ),
            action,
            Some(key),
            Position::Bottom,
            pal,
        ));
    }

    // Middle: the file name and, while it has unsaved edits, a dot.
    let mut name = irow![].spacing(6).align_y(iced::Alignment::Center);
    if let Some(file) = app.file.as_deref() {
        let label = file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_string();
        name = name.push(
            text(label)
                .size(12.5)
                .color(pal.muted)
                .wrapping(iced::widget::text::Wrapping::None),
        );
        if app.dirty {
            name = name.push(
                container(Space::new().width(6.0).height(6.0))
                    .id(iced::widget::Id::new("toolbar-dirty-dot"))
                    .style(move |_| container::Style {
                        background: Some(pal.accent.into()),
                        border: Border {
                            radius: theme::radius::PILL.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
            );
        }
    }
    let middle = container(name)
        .width(Length::Fill)
        .center_x(Length::Fill)
        .clip(true);

    // Right: the view switch, the document mind map's panel toggle, then
    // Save, Find, Command Palette, and Settings.
    let current = if full_mindmap {
        Some(ToolbarView::FullMindmap)
    } else {
        match app.view_mode {
            ViewMode::Rendered => Some(ToolbarView::Read),
            ViewMode::Raw => Some(ToolbarView::Edit),
            ViewMode::Mindmap => Some(ToolbarView::Mindmap),
        }
    };
    let read_key = match current {
        Some(ToolbarView::Edit) => Some("Esc"),
        Some(ToolbarView::Mindmap) => Some("⌘M"),
        Some(ToolbarView::FullMindmap) => Some("Esc"),
        _ => None,
    };
    let segment = |view: ToolbarView, content: Element<'a, Message>, enabled: bool| {
        view_segment(
            content,
            current == Some(view),
            enabled.then_some(Message::ToolbarView(view)),
            pal,
        )
    };
    let label = |s: &'a str| -> Element<'a, Message> { text(s).size(12).into() };
    let segments = irow![
        action_tip(
            // In Full Mindmap, Read leaves it even with no file open, as Back.
            segment(ToolbarView::Read, label("Read"), has_file || full_mindmap),
            "Read",
            read_key,
            Position::Bottom,
            pal,
        ),
        action_tip(
            segment(ToolbarView::Edit, label("Edit"), has_file),
            "Edit",
            Some("⌘E"),
            Position::Bottom,
            pal,
        ),
        action_tip(
            segment(ToolbarView::Mindmap, label("Mind Map"), has_file),
            "Mind Map",
            Some("⌘M"),
            Position::Bottom,
            pal,
        ),
        action_tip(
            segment(
                ToolbarView::FullMindmap,
                icon::glyph(
                    ic::NETWORK,
                    13.0,
                    if full_mindmap {
                        pal.fg
                    } else if has_file || app.workspace.is_some() {
                        pal.muted
                    } else {
                        pal.subtle
                    },
                )
                .into(),
                has_file || app.workspace.is_some(),
            ),
            "Full Mindmap",
            Some("⌘⇧M"),
            Position::Bottom,
            pal,
        ),
    ]
    .spacing(2)
    .align_y(iced::Alignment::Center);
    let segments = container(segments)
        .padding(2)
        .style(move |_| container::Style {
            background: Some(pal.chip().into()),
            border: Border {
                color: pal.border(),
                width: 1.0,
                radius: (theme::radius::SM + 2.0).into(),
            },
            ..Default::default()
        });

    let mut right = irow![segments].spacing(2).align_y(iced::Alignment::Center);
    if !full_mindmap && has_file && app.view_mode == ViewMode::Mindmap {
        let (glyph, action) = if app.mindmap_panel_open {
            (ic::PANEL_RIGHT_CLOSE, "Hide Panel")
        } else {
            (ic::PANEL_RIGHT_OPEN, "Show Panel")
        };
        right = right.push(action_tip(
            tool_button(glyph, Some(Message::ToggleMindmapPanel), false, pal),
            action,
            Some("⌘⌥B"),
            Position::Bottom,
            pal,
        ));
    }
    let can_find = has_file && !full_mindmap && !app.vault_open;
    right = right
        .push(Space::new().width(6.0))
        .push(action_tip(
            tool_button(
                ic::SAVE,
                (has_file && app.dirty).then_some(Message::SaveFile),
                false,
                pal,
            ),
            "Save",
            Some("⌘S"),
            Position::Bottom,
            pal,
        ))
        .push(action_tip(
            tool_button(
                ic::SEARCH,
                can_find.then_some(Message::ToggleSearch),
                can_find && app.search_open,
                pal,
            ),
            "Find",
            Some("⌘F"),
            Position::Bottom,
            pal,
        ))
        .push(action_tip(
            tool_button(ic::COMMAND, Some(Message::OpenCommandPalette), false, pal),
            "Command Palette",
            Some("⌘⇧P"),
            Position::Bottom,
            pal,
        ))
        .push(action_tip(
            tool_button(
                ic::SETTINGS,
                Some(Message::ToggleSettings),
                app.settings_open,
                pal,
            ),
            "Settings",
            Some("⌘,"),
            Position::Bottom,
            pal,
        ));

    let fill = look.fill;
    let radius = look.corner_radius;
    let bar = container(
        irow![left, middle, right]
            .spacing(8)
            .align_y(iced::Alignment::Center),
    )
    .id(iced::widget::Id::new("toolbar"))
    .width(Length::Fill)
    .height(Length::Fixed(TOOLBAR_H))
    .padding(Padding {
        top: 0.0,
        right: 10.0,
        bottom: 0.0,
        left: look.left_inset,
    })
    .align_y(iced::alignment::Vertical::Center)
    .clip(true)
    .style(move |_| container::Style {
        background: Some(fill.into()),
        border: Border {
            radius: iced::border::top_left(radius),
            ..Default::default()
        },
        ..Default::default()
    });
    // Its blank space moves the window, as the titlebar it replaces does.
    // Opaque: where no drag area takes the press (a native title bar), a
    // press on blank space still must not reach the editor under it.
    let bar = iced::widget::opaque(window_drag_area(bar));
    if look.hairline {
        column![
            bar,
            container(Space::new().height(1.0))
                .width(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(pal.border().into()),
                    ..Default::default()
                }),
        ]
        .into()
    } else {
        bar.into()
    }
}

/// The top band of the reader in On hover mode. Entering it reveals the
/// toolbar (`toolbar` is `Some` while revealed); leaving starts the hide
/// delay. It publishes on enter and leave only, never on plain moves, and
/// takes no input while the toolbar is hidden, so the content under it keeps
/// its hover, cursor, and clicks.
pub(in crate::app) fn toolbar_band<'a>(
    toolbar: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    let content: Element<'a, Message> = match toolbar {
        // Opaque: a press on the toolbar's blank space must not reach the
        // content under it.
        Some(bar) => iced::widget::opaque(bar),
        None => Space::new().width(Length::Fill).height(Length::Fill).into(),
    };
    mouse_area(
        container(content)
            .width(Length::Fill)
            .height(Length::Fixed(TOOLBAR_BAND_H))
            .align_y(iced::alignment::Vertical::Top),
    )
    .on_enter(Message::ToolbarHover(true))
    .on_exit(Message::ToolbarHover(false))
    .into()
}

/// A toolbar control takes every press on it, even while disabled: a
/// disabled button lets presses through, and the toolbar's blank space
/// under it would start a window drag or zoom.
fn control<'a>(button: button::Button<'a, Message>) -> Element<'a, Message> {
    iced::widget::opaque(button)
}

/// An icon button; `on_press` `None` draws it disabled, `active` filled.
fn tool_button<'a>(
    code: char,
    on_press: Option<Message>,
    active: bool,
    pal: Palette,
) -> Element<'a, Message> {
    let enabled = on_press.is_some();
    let color = if enabled { pal.muted } else { pal.subtle };
    control(
        button(icon::glyph(code, 14.0, color))
            .padding(Padding::from([4, 8]))
            .on_press_maybe(on_press)
            .style(move |_, status| button::Style {
                background: match status {
                    _ if active => Some(Background::Color(pal.active())),
                    button::Status::Hovered => Some(Background::Color(pal.hover())),
                    button::Status::Pressed => Some(Background::Color(pal.active())),
                    _ => None,
                },
                text_color: color,
                border: Border {
                    radius: theme::radius::PILL.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
    )
}

/// One segment of the view switch.
fn view_segment<'a>(
    content: Element<'a, Message>,
    selected: bool,
    on_press: Option<Message>,
    pal: Palette,
) -> Element<'a, Message> {
    let enabled = on_press.is_some();
    control(
        button(content)
            .padding(Padding::from([3, 10]))
            .on_press_maybe(on_press)
            .style(move |_, status| button::Style {
                background: if selected {
                    Some(pal.surface_alt.into())
                } else if enabled && matches!(status, button::Status::Hovered) {
                    Some(pal.hover().into())
                } else {
                    None
                },
                text_color: if selected {
                    pal.fg
                } else if enabled {
                    pal.muted
                } else {
                    pal.subtle
                },
                border: Border {
                    radius: theme::radius::SM.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
    )
}
