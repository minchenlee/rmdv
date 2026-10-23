//! Counts left presses over the whole window, for the folder picker's
//! double-click (MDV-048 polish), and reports right presses in window
//! coordinates, so a context menu opens where the press was (MDV-048 P6).
//!
//! A double-click on a picker folder row opens that folder. The first click
//! already descends, so the rows change under the pointer before the second
//! press arrives, and a row cannot tell a double-click apart. [`PressWatch`]
//! wraps the root of the view, where every rebuild keeps its state, and
//! counts the presses itself. Most of the time it only forwards events.

use crate::app::Message;
use iced::advanced::widget::{tree, Operation, Tree, Widget};
use iced::advanced::{layout, mouse, overlay, renderer, Clipboard, Layout, Shell};
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector};
use std::time::{Duration, Instant};

/// Farthest two presses may lie apart and still make one double-click.
const DOUBLE_CLICK_SLOP: f32 = 4.0;

/// The platform's double-click interval.
#[cfg(all(not(test), target_os = "macos"))]
pub(in crate::app) fn double_click_interval() -> Duration {
    let seconds = objc2_app_kit::NSEvent::doubleClickInterval();
    if seconds.is_finite() && seconds > 0.0 {
        Duration::from_secs_f64(seconds.min(5.0))
    } else {
        Duration::from_millis(500)
    }
}

/// The common default (Windows uses 500 ms; most Linux desktops 400 ms).
#[cfg(all(not(test), not(target_os = "macos")))]
pub(in crate::app) fn double_click_interval() -> Duration {
    Duration::from_millis(500)
}

/// Tests rebuild the view between the two clicks; a long interval keeps a
/// slow runner from splitting a double-click into two clicks.
#[cfg(test)]
pub(in crate::app) fn double_click_interval() -> Duration {
    Duration::from_secs(5)
}

/// Wraps `content`, reports left presses, and can keep the second press of a
/// double-click from reaching `content`.
pub(in crate::app) struct PressWatch<'a> {
    content: Element<'a, Message>,
    on_press: Option<fn(bool) -> Message>,
    on_right_press: Option<fn(Point) -> Message>,
    swallow_double: bool,
}

impl<'a> PressWatch<'a> {
    pub(in crate::app) fn new(content: impl Into<Element<'a, Message>>) -> Self {
        Self {
            content: content.into(),
            on_press: None,
            on_right_press: None,
            swallow_double: false,
        }
    }

    /// Publishes `on_press(double)` for each left press; `double` when the
    /// press completes a double-click. `None` publishes nothing.
    pub(in crate::app) fn on_press(mut self, on_press: Option<fn(bool) -> Message>) -> Self {
        self.on_press = on_press;
        self
    }

    /// Publishes `on_right_press(position)` for each right press, before
    /// the content sees it. Widgets inside a scrollable only know positions
    /// relative to their content; this one is in window coordinates.
    pub(in crate::app) fn on_right_press(mut self, on_right_press: fn(Point) -> Message) -> Self {
        self.on_right_press = Some(on_right_press);
        self
    }

    /// The press that completes a double-click stops here.
    pub(in crate::app) fn swallow_double(mut self, swallow: bool) -> Self {
        self.swallow_double = swallow;
        self
    }
}

#[derive(Default)]
struct State {
    /// Where and when the last press that could start a double-click was.
    last: Option<(Point, Instant)>,
}

impl<'a> Widget<Message, iced::Theme, iced::Renderer> for PressWatch<'a> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let child = self
            .content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        layout::Node::with_children(child.size(), vec![child])
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content.as_widget_mut().operate(
            &mut tree.children[0],
            child_layout(layout),
            renderer,
            operation,
        );
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
        if let (
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
            Some(at),
            Some(on_right_press),
        ) = (event, cursor.position(), self.on_right_press)
        {
            shell.publish(on_right_press(at));
        }
        if let (Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)), Some(at)) =
            (event, cursor.position())
        {
            let state = tree.state.downcast_mut::<State>();
            let now = Instant::now();
            let double = state.last.is_some_and(|(before, then)| {
                now.duration_since(then) <= double_click_interval()
                    && before.distance(at) <= DOUBLE_CLICK_SLOP
            });
            // The press after a double-click starts over.
            state.last = (!double).then_some((at, now));
            if let Some(on_press) = self.on_press {
                shell.publish(on_press(double));
            }
            if double && self.swallow_double {
                shell.capture_event();
                return;
            }
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            child_layout(layout),
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            child_layout(layout),
            cursor,
            viewport,
            renderer,
        )
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
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            child_layout(layout),
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            child_layout(layout),
            renderer,
            viewport,
            translation,
        )
    }
}

fn child_layout(layout: Layout<'_>) -> Layout<'_> {
    layout.children().next().expect("PressWatch has one child")
}

impl<'a> From<PressWatch<'a>> for Element<'a, Message> {
    fn from(watch: PressWatch<'a>) -> Self {
        Element::new(watch)
    }
}
