//! Shared widget builders and scrollable styling used across views.

use super::*;

pub(in crate::app) fn primary_button<'a>(
    label: &'a str,
    pal: Palette,
) -> button::Button<'a, Message> {
    button(text(label).size(13))
        .padding(Padding::from([8, 14]))
        .style(move |_, status| button::Style {
            background: Some(Background::Color(accent_fill(status, pal))),
            text_color: pal.accent_fg,
            border: Border {
                radius: theme::radius::PILL.into(),
                ..Default::default()
            },
            ..Default::default()
        })
}

/// An accent button's fill: lighter on hover, lighter again while pressed.
pub(in crate::app) fn accent_fill(status: button::Status, pal: Palette) -> Color {
    match status {
        button::Status::Hovered => Color {
            a: 0.92,
            ..pal.accent
        },
        button::Status::Pressed => Color {
            a: 0.80,
            ..pal.accent
        },
        _ => pal.accent,
    }
}

/// Quiet outlined pill that sits beside a [`primary_button`] as the
/// secondary choice.
pub(in crate::app) fn secondary_button<'a>(
    label: &'a str,
    pal: Palette,
) -> button::Button<'a, Message> {
    button(text(label).size(13).color(pal.muted))
        .padding(Padding::from([8, 14]))
        .style(move |_, status| button::Style {
            background: match status {
                button::Status::Hovered => Some(Background::Color(pal.hover())),
                button::Status::Pressed => Some(Background::Color(pal.active())),
                _ => None,
            },
            text_color: pal.muted,
            border: Border {
                color: pal.border(),
                width: 1.0,
                radius: theme::radius::PILL.into(),
            },
            ..Default::default()
        })
}

pub(in crate::app) fn ghost_lu<'a>(code: char, pal: Palette) -> button::Button<'a, Message> {
    // 5 px above and below the 14 px glyph's line: a 28 px tall target.
    button(icon::glyph(code, 14.0, pal.muted))
        .padding(Padding::from([5, 8]))
        .style(move |_, status| ghost_style(status, pal))
}

/// A ghost icon button's colors: no fill until hovered, darker while
/// pressed.
pub(in crate::app) fn ghost_style(status: button::Status, pal: Palette) -> button::Style {
    button::Style {
        background: match status {
            button::Status::Hovered => Some(Background::Color(pal.hover())),
            button::Status::Pressed => Some(Background::Color(pal.active())),
            _ => None,
        },
        text_color: pal.muted,
        border: Border {
            radius: theme::radius::PILL.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Tooltip for an icon button: the action's name, then its shortcut when it
/// has one ("Command Palette  ⌘⇧P").
pub(crate) fn action_tip<'a>(
    content: impl Into<Element<'a, Message>>,
    action: &'a str,
    shortcut: Option<&'a str>,
    position: iced::widget::tooltip::Position,
    pal: Palette,
) -> Element<'a, Message> {
    tip(
        content,
        action.into(),
        shortcut,
        position,
        std::time::Duration::ZERO,
        pal,
    )
}

/// [`action_tip`] whose text is built at view time and has no shortcut
/// (a Quick Slot from another workspace: "b.md — notes").
pub(crate) fn label_tip<'a>(
    content: impl Into<Element<'a, Message>>,
    label: String,
    position: iced::widget::tooltip::Position,
    pal: Palette,
) -> Element<'a, Message> {
    tip(
        content,
        label.into(),
        None,
        position,
        std::time::Duration::ZERO,
        pal,
    )
}

/// Wait before a reader tooltip opens. Fold chevrons and Copy Code buttons
/// repeat down the document; a pointer sweeping past them opens none, and so
/// never relayouts the reader.
const READER_TIP_DELAY: std::time::Duration = std::time::Duration::from_millis(400);

/// [`action_tip`] for controls inside the document (fold chevrons, Copy
/// Code): it opens only after the pointer rests on the control.
pub(crate) fn reader_tip<'a>(
    content: impl Into<Element<'a, Message>>,
    action: &'a str,
    position: iced::widget::tooltip::Position,
    pal: Palette,
) -> Element<'a, Message> {
    tip(
        content,
        action.into(),
        None,
        position,
        READER_TIP_DELAY,
        pal,
    )
}

fn tip<'a>(
    content: impl Into<Element<'a, Message>>,
    action: std::borrow::Cow<'a, str>,
    shortcut: Option<&'a str>,
    position: iced::widget::tooltip::Position,
    delay: std::time::Duration,
    pal: Palette,
) -> Element<'a, Message> {
    #[cfg(test)]
    tip_log::record(&action, shortcut, delay);
    let mut label = irow![text(action).size(11).color(pal.fg)]
        .spacing(8)
        .align_y(iced::Alignment::Center);
    if let Some(shortcut) = shortcut {
        label = label.push(
            text(shortcut)
                .size(11)
                .color(pal.muted)
                .shaping(iced::widget::text::Shaping::Advanced),
        );
    }
    let pill = container(label)
        .padding(Padding::from([4, 8]))
        .style(move |_| container::Style {
            background: Some(pal.chip().into()),
            border: Border {
                color: pal.border(),
                width: 1.0,
                radius: theme::radius::SM.into(),
            },
            ..Default::default()
        });
    iced::widget::tooltip(content, pill, position)
        .delay(delay)
        .into()
}

/// Test-only record of the tooltips built by `view`: a tooltip's body is an
/// overlay that widget operations do not reach.
#[cfg(test)]
pub(crate) mod tip_log {
    use std::cell::RefCell;

    thread_local! {
        static LABELS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
        static DELAYS: RefCell<Vec<(String, std::time::Duration)>> =
            const { RefCell::new(Vec::new()) };
    }

    /// The text an [`action_tip`](super::action_tip) shows, as one string.
    pub(crate) fn label(action: &str, shortcut: Option<&str>) -> String {
        match shortcut {
            Some(shortcut) => format!("{action}  {shortcut}"),
            None => action.to_string(),
        }
    }

    pub(crate) fn record(action: &str, shortcut: Option<&str>, delay: std::time::Duration) {
        LABELS.with(|labels| labels.borrow_mut().push(label(action, shortcut)));
        DELAYS.with(|delays| delays.borrow_mut().push((label(action, shortcut), delay)));
    }

    /// Labels recorded since the last call.
    pub(crate) fn take() -> Vec<String> {
        DELAYS.with(|delays| delays.borrow_mut().clear());
        LABELS.with(|labels| std::mem::take(&mut *labels.borrow_mut()))
    }

    /// Labels with their open delays, recorded since the last call.
    pub(crate) fn take_delays() -> Vec<(String, std::time::Duration)> {
        LABELS.with(|labels| labels.borrow_mut().clear());
        DELAYS.with(|delays| std::mem::take(&mut *delays.borrow_mut()))
    }
}

pub(in crate::app) fn centered_card<'a>(
    content: Element<'a, Message>,
    pal: Palette,
) -> Element<'a, Message> {
    container(
        container(content)
            .padding(Padding::from([40, 56]))
            .style(move |_| container::Style {
                background: Some(pal.popover().into()),
                border: Border {
                    color: pal.border(),
                    width: 1.0,
                    radius: theme::radius::LG.into(),
                },
                shadow: theme::shadow::CARD,
                ..Default::default()
            }),
    )
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .into()
}

pub(in crate::app) fn slim_scroll_direction() -> scrollable::Direction {
    scrollable::Direction::Vertical(
        scrollable::Scrollbar::new()
            .width(6.0)
            .scroller_width(6.0)
            .margin(2.0),
    )
}

pub(in crate::app) fn slim_scroll_direction_horizontal() -> scrollable::Direction {
    scrollable::Direction::Horizontal(
        scrollable::Scrollbar::new()
            .width(6.0)
            .scroller_width(6.0)
            .margin(2.0),
    )
}

/// Width of the reader scrollbar's hit area at the window's right edge:
/// presses, drags, and track clicks land anywhere in it.
pub(in crate::app) const READER_SCROLLBAR_HIT: f32 = 12.0;

/// The reader's vertical scrollbar: a 6 px thumb that widens to 10 px
/// while the pointer is on its strip, in a hit area of
/// [`READER_SCROLLBAR_HIT`] either way.
pub(in crate::app) fn reader_scroll_direction(hovered: bool) -> scrollable::Direction {
    let (thumb, margin) = if hovered { (10.0, 1.0) } else { (6.0, 2.0) };
    // Hit width = max(rail, thumb) + 2 × margin.
    let rail = READER_SCROLLBAR_HIT - 2.0 * margin;
    scrollable::Direction::Vertical(
        scrollable::Scrollbar::new()
            .width(rail)
            .scroller_width(thumb)
            .margin(margin),
    )
}

/// A layer over the reader that reports when the pointer enters or leaves
/// the scrollbar's strip. It shows no cursor and takes no presses or
/// wheel, so the scrollbar and the text below keep their events.
pub(in crate::app) fn reader_scrollbar_strip<'a>() -> Element<'a, Message> {
    container(
        mouse_area(
            Space::new()
                .width(Length::Fixed(READER_SCROLLBAR_HIT))
                .height(Length::Fill),
        )
        .on_enter(Message::ReaderScrollbarHover(true))
        .on_exit(Message::ReaderScrollbarHover(false)),
    )
    .align_x(iced::alignment::Horizontal::Right)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// [`sleek_scrollable_style`] for the reader: the thumb fades with
/// `alpha` (see `ScrollerFade`) and shows in full while hovered or dragged.
pub(in crate::app) fn reader_scrollable_style(
    status: scrollable::Status,
    pal: Palette,
    alpha: f32,
) -> scrollable::Style {
    let mut style = sleek_scrollable_style(status, pal, true);
    let engaged = matches!(
        status,
        scrollable::Status::Dragged { .. }
            | scrollable::Status::Hovered {
                is_vertical_scrollbar_hovered: true,
                ..
            }
    );
    if !engaged {
        let mut color = pal.scroller_hover;
        color.a *= alpha;
        style.vertical_rail.scroller.background = Background::Color(color);
    }
    style
}

pub(crate) fn sleek_scrollable_style(
    status: scrollable::Status,
    pal: Palette,
    recently_scrolled: bool,
) -> scrollable::Style {
    let scroller_color = match status {
        scrollable::Status::Dragged { .. } => pal.scroller_hover,
        scrollable::Status::Hovered {
            is_vertical_scrollbar_hovered: true,
            ..
        }
        | scrollable::Status::Hovered {
            is_horizontal_scrollbar_hovered: true,
            ..
        } => pal.scroller_hover,
        _ if recently_scrolled => pal.scroller_hover,
        _ => Color::TRANSPARENT,
    };
    let rail = scrollable::Rail {
        background: None,
        border: Border {
            radius: theme::radius::PILL.into(),
            ..Default::default()
        },
        scroller: scrollable::Scroller {
            background: Background::Color(scroller_color),
            border: Border {
                radius: theme::radius::PILL.into(),
                ..Default::default()
            },
        },
    };
    scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
        auto_scroll: scrollable::AutoScroll {
            background: Background::Color(Color::TRANSPARENT),
            border: Border::default(),
            shadow: iced::Shadow::default(),
            icon: Color::TRANSPARENT,
        },
    }
}

/// Fills the pixels outside a panel's rounded top-left corner, so the corner
/// reads as part of the panel next to it instead of a gap. Stack it under the
/// panel. Used under whole-window glass, where the reader's corner would
/// otherwise show the untinted material.
pub(in crate::app) fn corner_fill<'a>(color: Color, radius: f32) -> Element<'a, Message> {
    iced::widget::canvas(CornerFill { color, radius })
        .width(Length::Fixed(radius))
        .height(Length::Fixed(radius))
        .into()
}

/// How far the corner fill reaches under the panel's curve, in points.
const CORNER_OVERLAP: f32 = 0.25;

struct CornerFill {
    color: Color,
    radius: f32,
}

impl<Message> iced::widget::canvas::Program<Message> for CornerFill {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: iced::Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<iced::widget::canvas::Geometry> {
        use iced::widget::canvas::{path::Arc, Frame, Path};
        use iced::{Point, Radians};
        use std::f32::consts::{FRAC_PI_2, PI};
        let r = self.radius;
        let mut frame = Frame::new(renderer, bounds.size());
        // The square minus the quarter circle the panel's corner keeps. The
        // fill sits under the panel and reaches half a pixel inside its curve,
        // so the panel's anti-aliased edge lands on tint instead of letting
        // the untinted glass through as a light seam.
        let shape = Path::new(|p| {
            p.arc(Arc {
                center: Point::new(r, r),
                radius: r - CORNER_OVERLAP,
                start_angle: Radians(-FRAC_PI_2),
                end_angle: Radians(-PI),
            });
            p.line_to(Point::new(0.0, r));
            p.line_to(Point::ORIGIN);
            p.line_to(Point::new(r, 0.0));
            p.close();
        });
        frame.fill(&shape, self.color);
        vec![frame.into_geometry()]
    }
}

/// Full-window backdrop under a modal panel. A press sends `on_press`; the
/// idle cursor and the swallowed wheel keep hover, cursor shape, and scroll
/// from reaching the content under the overlay.
pub(in crate::app) fn modal_scrim<'a>(on_press: Message) -> Element<'a, Message> {
    mouse_area(
        container(Space::new().width(Length::Fill).height(Length::Fill))
            .style(|_| container::Style {
                background: Some(Background::Color(theme::shadow::SCRIM)),
                ..Default::default()
            })
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .interaction(iced::mouse::Interaction::Idle)
    .on_scroll(|_| Message::Noop)
    .on_press(on_press)
    .into()
}

/// Blank chrome that acts as the window's titlebar: a press moves the
/// window and a double click zooms it. Controls inside take their own
/// presses first, so only the blank space reaches this area. Only where
/// rmdv hides the native title bar (`APP_DRAWS_TITLEBAR`).
pub(in crate::app) fn window_drag_area<'a>(
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    titlebar_drag_area(content, APP_DRAWS_TITLEBAR)
}

/// `window_settings` hides the native title bar on macOS only; elsewhere
/// the native title bar moves and zooms the window.
pub(in crate::app) const APP_DRAWS_TITLEBAR: bool = cfg!(target_os = "macos");

/// `window_drag_area`, or `content` unchanged under a native title bar.
pub(in crate::app) fn titlebar_drag_area<'a>(
    content: impl Into<Element<'a, Message>>,
    titlebar_hidden: bool,
) -> Element<'a, Message> {
    if !titlebar_hidden {
        return content.into();
    }
    mouse_area(content)
        .on_press(Message::WindowDrag)
        .on_double_click(Message::WindowTitlebarDoubleClick)
        .into()
}

/// Blank layer that reports pointer motion over a list surface. Stack it
/// above the list: the stack updates its top layer first, so the report
/// precedes the row hovers of the same event.
pub(in crate::app) fn list_pointer_tracker<'a>() -> Element<'a, Message> {
    mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
        .on_move(Message::ListPointerMoved)
        .into()
}

/// A list row that reports pointer hovers. The selected row reports none:
/// a hover there changes nothing, so moves inside it publish no message.
pub(in crate::app) fn hover_row<'a>(
    row: impl Into<Element<'a, Message>>,
    selected: bool,
    on_hover: Message,
) -> Element<'a, Message> {
    // Always a MouseArea, so selection changes keep the row's widget state.
    let area = mouse_area(row);
    if selected {
        area.into()
    } else {
        area.on_move(move |_| on_hover.clone()).into()
    }
}

/// Background for a list row: only the keyboard selection is filled, so
/// hover and selection show as one highlighted row.
pub(in crate::app) fn list_row_style(
    selected: bool,
    pal: Palette,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, _| button::Style {
        background: selected.then(|| Background::Color(pal.hover())),
        text_color: pal.fg,
        border: Border {
            radius: theme::radius::SM.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}
