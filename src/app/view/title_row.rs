//! The toolbar's row (MDV-048 polish): controls at both ends and the file
//! name centered on the window, as a native title is.

use crate::app::Message;
use iced::advanced::widget::{Operation, Tree, Widget};
use iced::advanced::{layout, mouse, overlay, renderer, Clipboard, Layout, Shell};
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector};

/// `left` at the start, `right` at the end, and `title` centered on the
/// window's horizontal center, with `mark` (the dirty dot) just after it.
/// Neither covers a group: where the center would put the title under one,
/// it moves aside, and where it is wider than the space between them, it is
/// cut to that space (the title clips); the mark always shows.
pub(in crate::app) struct TitleRow<'a> {
    /// Left group, title, mark, right group.
    children: [Element<'a, Message>; 4],
    /// Space kept between the title and each group.
    gap: f32,
    /// Toolbar padding outside this row, at its left and right.
    pad: (f32, f32),
    /// The window's width. The toolbar ends at the window's right edge, so
    /// this places the row in the window. `None`: the toolbar spans it.
    window_width: Option<f32>,
}

impl<'a> TitleRow<'a> {
    pub(in crate::app) fn new(
        left: impl Into<Element<'a, Message>>,
        title: impl Into<Element<'a, Message>>,
        mark: impl Into<Element<'a, Message>>,
        right: impl Into<Element<'a, Message>>,
    ) -> Self {
        Self {
            children: [left.into(), title.into(), mark.into(), right.into()],
            gap: 8.0,
            pad: (0.0, 0.0),
            window_width: None,
        }
    }

    pub(in crate::app) fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    /// The toolbar's left and right padding around this row.
    pub(in crate::app) fn padding(mut self, left: f32, right: f32) -> Self {
        self.pad = (left, right);
        self
    }

    pub(in crate::app) fn window_width(mut self, width: Option<f32>) -> Self {
        self.window_width = width;
        self
    }
}

/// Space between the title and its mark.
const MARK_GAP: f32 = 6.0;

/// Where a title of width `title` starts: centered on `center`, but with
/// the whole group (title and mark, `group` wide) inside `[lo, hi]`, which
/// it fits (`group <= hi - lo`).
pub(in crate::app) fn title_x(center: f32, title: f32, group: f32, lo: f32, hi: f32) -> f32 {
    (center - title / 2.0).min(hi - group).max(lo)
}

impl<'a> Widget<Message, iced::Theme, iced::Renderer> for TitleRow<'a> {
    fn children(&self) -> Vec<Tree> {
        self.children.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.children);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let limits = limits.width(Length::Fill).height(Length::Shrink);
        let max = limits.max();
        let loose = layout::Limits::new(Size::ZERO, max);
        let [left, title, mark, right] = &mut self.children;
        let left = left
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, &loose);
        let right = right
            .as_widget_mut()
            .layout(&mut tree.children[3], renderer, &loose);
        let width = max.width;
        let lo = left.size().width + self.gap;
        let hi = (width - right.size().width - self.gap).max(lo);
        let mark = mark
            .as_widget_mut()
            .layout(&mut tree.children[2], renderer, &loose);
        let mark_w = mark.size().width;
        let mark_gap = if mark_w > 0.0 { MARK_GAP } else { 0.0 };
        let title_room = (hi - lo - mark_gap - mark_w).max(0.0);
        let title = title.as_widget_mut().layout(
            &mut tree.children[1],
            renderer,
            &layout::Limits::new(Size::ZERO, Size::new(title_room, max.height)),
        );
        let title_w = title.size().width.min(title_room);
        let group_w = title_w + mark_gap + mark_w;
        // The row's left edge in the window: the toolbar ends at the
        // window's right edge.
        let (pad_left, pad_right) = self.pad;
        let toolbar = width + pad_left + pad_right;
        let window = self.window_width.unwrap_or(toolbar);
        let row_left = window - toolbar + pad_left;
        let center = window / 2.0 - row_left;
        let x = title_x(center, title_w, group_w, lo, hi);

        let height = left
            .size()
            .height
            .max(right.size().height)
            .max(title.size().height)
            .max(mark.size().height);
        let middle = |node: layout::Node, x: f32| {
            let y = (height - node.size().height) / 2.0;
            node.move_to(Point::new(x, y))
        };
        let right_x = width - right.size().width;
        layout::Node::with_children(
            Size::new(width, height),
            vec![
                middle(left, 0.0),
                middle(title, x),
                middle(mark, x + title_w + mark_gap),
                middle(right, right_x),
            ],
        )
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            for ((child, state), layout) in self
                .children
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
            {
                child
                    .as_widget_mut()
                    .operate(state, layout, renderer, operation);
            }
        });
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        for ((child, state), layout) in self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            child.as_widget_mut().update(
                state, event, layout, cursor, renderer, clipboard, shell, viewport,
            );
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((child, state), layout)| {
                child
                    .as_widget()
                    .mouse_interaction(state, layout, cursor, viewport, renderer)
            })
            .max()
            .unwrap_or_default()
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        for ((child, state), layout) in self
            .children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
        {
            child
                .as_widget()
                .draw(state, renderer, theme, style, layout, cursor, viewport);
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        overlay::from_children(
            &mut self.children,
            tree,
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a> From<TitleRow<'a>> for Element<'a, Message> {
    fn from(row: TitleRow<'a>) -> Self {
        Element::new(row)
    }
}
