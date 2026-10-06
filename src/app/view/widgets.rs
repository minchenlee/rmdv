//! Shared widget builders and scrollable styling used across views.

use super::*;

pub(in crate::app) fn primary_button<'a>(
    label: &'a str,
    pal: Palette,
) -> button::Button<'a, Message> {
    button(text(label).size(13))
        .padding(Padding::from([8, 14]))
        .style(move |_, status| {
            let bg = match status {
                button::Status::Hovered => Color {
                    a: 0.92,
                    ..pal.accent
                },
                button::Status::Pressed => Color {
                    a: 0.80,
                    ..pal.accent
                },
                _ => pal.accent,
            };
            button::Style {
                background: Some(Background::Color(bg)),
                text_color: pal.accent_fg,
                border: Border {
                    radius: theme::radius::PILL.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
}

pub(in crate::app) fn ghost_lu<'a>(code: char, pal: Palette) -> button::Button<'a, Message> {
    button(icon::glyph(code, 14.0, pal.muted))
        .padding(Padding::from([4, 8]))
        .style(move |_, status| button::Style {
            background: match status {
                button::Status::Hovered => Some(Background::Color(pal.hover())),
                _ => None,
            },
            text_color: pal.muted,
            border: Border {
                radius: theme::radius::PILL.into(),
                ..Default::default()
            },
            ..Default::default()
        })
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
/// reads as part of the panel next to it instead of a gap. Used under
/// whole-window glass, where the reader's corner would otherwise show the
/// untinted material.
pub(in crate::app) fn corner_fill<'a>(color: Color, radius: f32) -> Element<'a, Message> {
    iced::widget::canvas(CornerFill { color, radius })
        .width(Length::Fixed(radius))
        .height(Length::Fixed(radius))
        .into()
}

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
        use iced::widget::canvas::{Frame, Path};
        use iced::Point;
        let r = self.radius;
        let mut frame = Frame::new(renderer, bounds.size());
        // The square minus the quarter circle the panel's corner keeps: the
        // arc is tangent to both edges, so its center is (r, r).
        let shape = Path::new(|p| {
            p.move_to(Point::new(r, 0.0));
            p.arc_to(Point::ORIGIN, Point::new(0.0, r), r);
            p.line_to(Point::ORIGIN);
            p.close();
        });
        frame.fill(&shape, self.color);
        vec![frame.into_geometry()]
    }
}
