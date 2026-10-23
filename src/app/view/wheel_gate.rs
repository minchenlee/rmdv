//! Takes some wheel events before the widget they would scroll
//! (MDV-048 P5).
//!
//! An Iced `scrollable` scrolls on every wheel event over it, and an
//! `event::listen` subscription cannot stop that. [`WheelGate`] wraps the
//! scrollable (or the editor) and sees each wheel event first:
//!
//! - [`Gate::Reader`]: ⌘+wheel changes the reader font size and does not
//!   also scroll the reader. On macOS the gate also tells the swipe bridge
//!   (`crate::native_swipe`) whether the pointer is over the reader and
//!   over nothing that scrolls horizontally ([`HScrollProbe`]).
//! - [`Gate::EditorScroll`]: a trackpad (pixel) scroll moves the editor by
//!   the finger distance. Iced's `text_editor` turns `y` pixels into `y / 4`
//!   lines, about five times the finger distance at a 20 px line. A mouse
//!   wheel (lines) keeps the editor's own speed.

use crate::app::Message;
use iced::advanced::widget::{tree, Operation, Tree, Widget};
use iced::advanced::{layout, mouse, overlay, renderer, Clipboard, Layout, Shell};
use iced::keyboard;
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector};

/// What the gate does with the wheel events it takes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::app) enum Gate {
    /// ⌘+wheel publishes [`Message::ReaderZoomBy`]. With `swipe` (macOS,
    /// a swipe armed), the pointer position tells the swipe bridge whether
    /// a two-finger swipe may start there.
    Reader { swipe: bool },
    /// Pixel scrolls publish the editor's scroll action; `line_height` is
    /// the editor's line height in logical pixels.
    EditorScroll { line_height: f32 },
}

pub(in crate::app) struct WheelGate<'a> {
    content: Element<'a, Message>,
    gate: Gate,
}

impl<'a> WheelGate<'a> {
    pub(in crate::app) fn new(content: impl Into<Element<'a, Message>>, gate: Gate) -> Self {
        Self {
            content: content.into(),
            gate,
        }
    }
}

impl WheelGate<'_> {
    /// `None` lets the content scroll. `Some` takes the wheel event, with
    /// the message to publish if there is one.
    fn take_wheel(&self, state: &mut State, delta: mouse::ScrollDelta) -> Option<Option<Message>> {
        match self.gate {
            Gate::Reader { .. } => {
                if !state.modifiers.command() {
                    return None;
                }
                let steps = crate::app::gestures::wheel_zoom_steps(delta);
                (steps != 0.0).then_some(Some(Message::ReaderZoomBy(steps)))
            }
            Gate::EditorScroll { line_height } => match delta {
                // A mouse wheel keeps the editor's own speed.
                mouse::ScrollDelta::Lines { .. } => None,
                mouse::ScrollDelta::Pixels { y, .. } => {
                    let lines = crate::app::gestures::editor_scroll_lines(
                        &mut state.partial_lines,
                        y,
                        line_height,
                    );
                    // The gate takes every pixel scroll, also one that
                    // adds up to less than a line, so the editor never
                    // scrolls by its own pixel speed.
                    Some((lines != 0).then_some(Message::EditorAction(
                        iced::widget::text_editor::Action::Scroll { lines },
                    )))
                }
            },
        }
    }
}

impl WheelGate<'_> {
    /// Tell the swipe bridge whether a swipe may start at the pointer: over
    /// the reader and over no horizontal scroller. Only with `swipe` (see
    /// [`Gate::Reader`]), so other platforms and other states pay nothing.
    ///
    /// The horizontal scrollers are found once per layout and kept in the
    /// gate's state; a pointer move only tests the pointer against them. A
    /// wheel event moves the content, so it drops them, and the redraw that
    /// follows finds them again. That redraw (and the one after any update,
    /// such as the one that arms a swipe) also refreshes the flag under a
    /// pointer that has not moved.
    fn sync_swipe_pointer(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
    ) {
        match event {
            Event::Mouse(mouse::Event::WheelScrolled { .. }) => {
                tree.state.downcast_mut::<State>().hscrollers = None;
                return;
            }
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft)
            | Event::Window(iced::window::Event::RedrawRequested(_)) => {}
            _ => return,
        }
        let Some(at) = cursor.position_over(layout.bounds()) else {
            crate::native_swipe::set_pointer_ok(false);
            return;
        };
        let (state, children) = (tree.state.downcast_mut::<State>(), &mut tree.children);
        let hscrollers = state.hscrollers.get_or_insert_with(|| {
            #[cfg(test)]
            PROBE_WALKS.set(PROBE_WALKS.get() + 1);
            let mut probe = HScrollProbe::new();
            self.content.as_widget_mut().operate(
                &mut children[0],
                child_layout(layout),
                renderer,
                &mut probe,
            );
            probe.rects
        });
        let ok = !covers(hscrollers, at);
        crate::native_swipe::set_pointer_ok(ok);
    }
}

// How many times a gate walked its content for the swipe probe.
#[cfg(test)]
thread_local! {
    pub(in crate::app) static PROBE_WALKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Finds the scrollables whose content is wider than they are, such as a
/// wide table, where they are in the window: a two-finger swipe over one
/// scrolls it instead of going Back.
///
/// A scrollable reports its bounds in its parent's content coordinates and
/// then lays out its children shifted by its scroll offset (`translation`).
/// The probe keeps the sum of the offsets above the widget it visits, so
/// a table inside the scrolled reader is found where it is on screen.
pub(in crate::app) struct HScrollProbe {
    /// The horizontal scrollers found, in window coordinates.
    rects: Vec<Rectangle>,
    /// The scroll offset of every `traverse` level now open.
    offsets: Vec<Vector>,
    /// The offset the next `traverse` adds: a scrollable's translation, set
    /// right before it traverses its content; zero for any other widget.
    next: Vector,
}

impl HScrollProbe {
    pub(in crate::app) fn new() -> Self {
        Self {
            rects: Vec::new(),
            offsets: Vec::new(),
            next: Vector::ZERO,
        }
    }

    /// A horizontal scroller lies under `at` (window coordinates).
    #[cfg(test)]
    pub(in crate::app) fn covers(&self, at: Point) -> bool {
        covers(&self.rects, at)
    }

    fn offset(&self) -> Vector {
        self.offsets.last().copied().unwrap_or(Vector::ZERO)
    }
}

impl Operation for HScrollProbe {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        let level = self.offset() + std::mem::replace(&mut self.next, Vector::ZERO);
        self.offsets.push(level);
        operate(self);
        self.offsets.pop();
    }

    fn container(&mut self, _id: Option<&iced::advanced::widget::Id>, _bounds: Rectangle) {
        self.next = Vector::ZERO;
    }

    fn scrollable(
        &mut self,
        _id: Option<&iced::advanced::widget::Id>,
        bounds: Rectangle,
        content_bounds: Rectangle,
        translation: Vector,
        _state: &mut dyn iced::advanced::widget::operation::Scrollable,
    ) {
        if content_bounds.width > bounds.width + 0.5 {
            let offset = self.offset();
            self.rects.push(Rectangle {
                x: bounds.x - offset.x,
                y: bounds.y - offset.y,
                ..bounds
            });
        }
        self.next = translation;
    }

    fn focusable(
        &mut self,
        _id: Option<&iced::advanced::widget::Id>,
        _bounds: Rectangle,
        _state: &mut dyn iced::advanced::widget::operation::Focusable,
    ) {
        self.next = Vector::ZERO;
    }

    fn text(&mut self, _id: Option<&iced::advanced::widget::Id>, _bounds: Rectangle, _text: &str) {
        self.next = Vector::ZERO;
    }
}

#[derive(Default)]
struct State {
    /// The keyboard modifiers now; wheel events do not carry them.
    modifiers: keyboard::Modifiers,
    /// The editor lines scrolled but not sent yet (less than one line).
    partial_lines: f32,
    /// The reader's horizontal scrollers in window coordinates (see
    /// [`HScrollProbe`]); `None` until found after a layout or a scroll.
    hscrollers: Option<Vec<Rectangle>>,
}

impl<'a> Widget<Message, iced::Theme, iced::Renderer> for WheelGate<'a> {
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
        // A new layout may move or resize the horizontal scrollers.
        tree.state.downcast_mut::<State>().hscrollers = None;
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
        if self.gate == (Gate::Reader { swipe: true }) {
            self.sync_swipe_pointer(tree, event, layout, cursor, renderer);
        }
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = *modifiers;
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta })
                if cursor.is_over(layout.bounds()) =>
            {
                if let Some(message) = self.take_wheel(state, *delta) {
                    if let Some(message) = message {
                        shell.publish(message);
                    }
                    shell.capture_event();
                    return;
                }
            }
            _ => {}
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

/// One of the horizontal scrollers `rects` (window coordinates) lies
/// under `at`.
fn covers(rects: &[Rectangle], at: Point) -> bool {
    rects.iter().any(|rect| rect.contains(at))
}

fn child_layout(layout: Layout<'_>) -> Layout<'_> {
    layout.children().next().expect("WheelGate has one child")
}

impl<'a> From<WheelGate<'a>> for Element<'a, Message> {
    fn from(gate: WheelGate<'a>) -> Self {
        Element::new(gate)
    }
}
