//! The band at the left window edge that reveals the hidden sidebar as a
//! peek (MDV-048 polish).
//!
//! [`EdgePeek`] fills the window as one layer of the view's stack and holds
//! the peek (or nothing) as its content. It watches a thin band at its left
//! edge: the pointer entering the band with no mouse button held publishes
//! `on_enter` once. A press anywhere sets the held state, so a drag-select
//! that reaches the edge does not reveal the sidebar. On macOS a pointer
//! move also asks the system whether a button is still down, because a
//! window drag can swallow the release. The band takes no input and shows
//! no cursor of its own, so the content under it keeps its hover, cursor,
//! and clicks. It publishes on entering only, never on plain moves.

use crate::app::Message;
use iced::advanced::widget::{tree, Operation, Tree, Widget};
use iced::advanced::{layout, mouse, overlay, renderer, Clipboard, Layout, Shell};
use iced::{Element, Event, Length, Rectangle, Size, Vector};

/// Width of the band at the left window edge.
pub(in crate::app) const EDGE_BAND_W: f32 = 5.0;

/// Whether a mouse button is down, from the system rather than from
/// events: a window drag can swallow the release (winit's `drag_window` on
/// macOS "may prevent the button release event to be triggered").
#[cfg(all(not(test), target_os = "macos"))]
fn buttons_down() -> Option<bool> {
    Some(objc2_app_kit::NSEvent::pressedMouseButtons() != 0)
}

/// Elsewhere the events alone tell.
#[cfg(all(not(test), not(target_os = "macos")))]
fn buttons_down() -> Option<bool> {
    None
}

#[cfg(test)]
thread_local! {
    /// What [`buttons_down`] reports in tests; `None` as on platforms
    /// without the query.
    pub(in crate::app) static TEST_BUTTONS_DOWN: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn buttons_down() -> Option<bool> {
    TEST_BUTTONS_DOWN.with(std::cell::Cell::get)
}

pub(in crate::app) struct EdgePeek<'a> {
    content: Element<'a, Message>,
    on_enter: Option<Message>,
    top: f32,
}

impl<'a> EdgePeek<'a> {
    pub(in crate::app) fn new(content: impl Into<Element<'a, Message>>) -> Self {
        Self {
            content: content.into(),
            on_enter: None,
            top: 0.0,
        }
    }

    /// Published when the pointer enters the band; `None` turns the band off.
    pub(in crate::app) fn on_enter(mut self, message: Option<Message>) -> Self {
        self.on_enter = message;
        self
    }

    /// The band starts this far below the top, clear of the toolbar row.
    pub(in crate::app) fn top(mut self, top: f32) -> Self {
        self.top = top;
        self
    }
}

#[derive(Default)]
struct State {
    /// The pointer is in the band and its entry was handled.
    in_band: bool,
    /// A mouse button is down.
    held: bool,
}

impl<'a> Widget<Message, iced::Theme, iced::Renderer> for EdgePeek<'a> {
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
        Size::new(Length::Fill, Length::Fill)
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
        layout::Node::with_children(limits.max(), vec![child])
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
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(_)) => state.held = true,
            Event::Mouse(mouse::Event::ButtonReleased(_)) => state.held = false,
            // A held state whose release never came: ask the system.
            Event::Mouse(mouse::Event::CursorMoved { .. }) if state.held => {
                if let Some(down) = buttons_down() {
                    state.held = down;
                }
            }
            _ => {}
        }
        let bounds = layout.bounds();
        let band = Rectangle {
            x: bounds.x,
            y: bounds.y + self.top,
            width: EDGE_BAND_W,
            height: (bounds.height - self.top).max(0.0),
        };
        // A layer above (the peek itself, an overlay) levitates the cursor,
        // which is then over nothing here.
        let over = self.on_enter.is_some() && cursor.is_over(band);
        if !over {
            state.in_band = false;
        } else if !state.in_band && !state.held {
            state.in_band = true;
            if let Some(message) = &self.on_enter {
                shell.publish(message.clone());
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
    layout.children().next().expect("EdgePeek has one child")
}

impl<'a> From<EdgePeek<'a>> for Element<'a, Message> {
    fn from(peek: EdgePeek<'a>) -> Self {
        Element::new(peek)
    }
}
