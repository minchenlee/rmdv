//! One text selection across reader blocks.
//!
//! Each reader text widget (`iced_selection::text::Rich`) selects inside
//! itself. [`SelectScope`] wraps the reading column: when a drag leaves the
//! text widget it started in, the scope takes over. It keeps the selection in
//! document positions (block index, text widget within the block, line,
//! grapheme), not in widget state, because virtual scrolling unmounts blocks.
//! It paints the range into every mounted text widget and copies it on ⌘C.
//!
//! [`BlockMark`] wraps each block row and tells the scope which document
//! block the text widgets under it belong to.
//!
//! On a right press the scope reports what the reader's text holds there
//! ([`Message::ReaderContextMenu`]): the link under the pointer and the
//! selected text. The press changes no selection. [`TargetMark`] wraps a
//! reader element with its own menu items (a heading, a code block, an
//! image, a diagram) and reports it first ([`Message::ReaderTarget`]).

use crate::app::{Message, ReaderHit, ReaderTarget};
use crate::ast::{Block, BlockId};
use iced::advanced::widget::{tree, Operation, Tree, Widget};
use iced::advanced::{layout, mouse, overlay, renderer, Clipboard, Layout, Shell};
use iced::{keyboard, Element, Event, Length, Point, Rectangle, Size, Vector};
use iced_selection::selection::SelectionEnd;
use iced_selection::text::RichState;
use std::any::Any;
use std::collections::HashSet;

type TextState = RichState<Message>;

/// A position in the document: block index, text widget within the block
/// (in layout order), then line and grapheme inside that widget.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Pos {
    block: usize,
    sub: usize,
    line: usize,
    index: usize,
}

impl Pos {
    fn widget(self) -> (usize, usize) {
        (self.block, self.sub)
    }

    fn end(self) -> SelectionEnd {
        SelectionEnd::new(self.line, self.index)
    }
}

/// What [`BlockMark`] hands to [`Operation::custom`].
struct BlockIndex(usize);

/// Wraps a block row; transparent to layout, events, and the widget tree.
pub struct BlockMark<'a> {
    index: usize,
    content: Element<'a, Message>,
}

impl<'a> BlockMark<'a> {
    /// `index` is the block's position in the document's block list.
    pub fn new(index: usize, content: impl Into<Element<'a, Message>>) -> Self {
        Self {
            index,
            content: content.into(),
        }
    }
}

/// Wraps the reading column and owns the cross-block selection.
pub struct SelectScope<'a> {
    blocks: &'a [(BlockId, Block)],
    /// Folded headings: their hidden sections are left out of a copy.
    folded: HashSet<BlockId>,
    content: Element<'a, Message>,
}

impl<'a> SelectScope<'a> {
    pub fn new(
        blocks: &'a [(BlockId, Block)],
        folded: &HashSet<BlockId>,
        content: impl Into<Element<'a, Message>>,
    ) -> Self {
        Self {
            blocks,
            folded: folded.clone(),
            content: content.into(),
        }
    }
}

#[derive(Default)]
struct State {
    /// The document the selection belongs to: block list address and length.
    doc: (usize, usize),
    /// Where the left button went down, while it is held over text.
    press: Option<Pos>,
    /// The drag has left the text widget it started in.
    crossing: bool,
    /// The selection the scope owns: anchor and focus, in either order.
    range: Option<(Pos, Pos)>,
    /// Text widgets may still show a range the scope painted.
    painted: bool,
}

impl State {
    fn ordered(&self) -> Option<(Pos, Pos)> {
        let (a, b) = self.range?;
        (a != b).then(|| (a.min(b), a.max(b)))
    }
}

/// Visits every mounted reader text widget in document order.
struct Visit<F: FnMut(usize, usize, Rectangle, &mut TextState) + Send> {
    block: Option<usize>,
    sub: usize,
    f: F,
}

impl<F: FnMut(usize, usize, Rectangle, &mut TextState) + Send> Operation for Visit<F> {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn custom(&mut self, _id: Option<&iced::widget::Id>, bounds: Rectangle, state: &mut dyn Any) {
        if let Some(BlockIndex(i)) = state.downcast_ref::<BlockIndex>() {
            self.block = Some(*i);
            self.sub = 0;
        } else if let Some(text) = state.downcast_mut::<TextState>() {
            // Text outside any marked block (none in the reader) is skipped.
            if let Some(block) = self.block {
                (self.f)(block, self.sub, bounds, text);
                self.sub += 1;
            }
        }
    }
}

fn visit(
    content: &mut Element<'_, Message>,
    tree: &mut Tree,
    layout: Layout<'_>,
    renderer: &iced::Renderer,
    f: impl FnMut(usize, usize, Rectangle, &mut TextState) + Send,
) {
    let mut op = Visit {
        block: None,
        sub: 0,
        f,
    };
    content
        .as_widget_mut()
        .operate(tree, layout, renderer, &mut op);
}

/// Distance from `p` to `r`: vertical first, so the row under the pointer
/// wins over a nearer column beside it.
fn distance(p: Point, r: Rectangle) -> (f32, f32) {
    let gap = |v: f32, lo: f32, hi: f32| {
        if v < lo {
            lo - v
        } else if v > hi {
            v - hi
        } else {
            0.0
        }
    };
    (gap(p.y, r.y, r.y + r.height), gap(p.x, r.x, r.x + r.width))
}

/// The text position nearest to `p`, and whether `p` is inside that widget.
fn locate(
    content: &mut Element<'_, Message>,
    tree: &mut Tree,
    layout: Layout<'_>,
    renderer: &iced::Renderer,
    p: Point,
) -> Option<(Pos, bool)> {
    let mut best: Option<((f32, f32), Pos)> = None;
    visit(
        content,
        tree,
        layout,
        renderer,
        |block, sub, bounds, text| {
            let d = distance(p, bounds);
            if best.map_or(true, |(bd, _)| d < bd) {
                let at = text.position_at(p, bounds);
                best = Some((
                    d,
                    Pos {
                        block,
                        sub,
                        line: at.line,
                        index: at.index,
                    },
                ));
            }
        },
    );
    best.map(|(d, pos)| (pos, d == (0.0, 0.0)))
}

/// Shows `range` in every mounted text widget, or clears it.
fn paint(
    content: &mut Element<'_, Message>,
    tree: &mut Tree,
    layout: Layout<'_>,
    renderer: &iced::Renderer,
    range: Option<(Pos, Pos)>,
) {
    visit(content, tree, layout, renderer, |block, sub, _, text| {
        let shown = range.and_then(|(start, end)| {
            let here = (block, sub);
            if here < start.widget() || here > end.widget() {
                return None;
            }
            let lo = if here == start.widget() {
                start.end()
            } else {
                SelectionEnd::new(0, 0)
            };
            let hi = if here == end.widget() {
                end.end()
            } else {
                text.end()
            };
            Some((lo, hi))
        });
        text.set_external_selection(shown);
    });
}

/// The selected text. The two end blocks come from their mounted widgets;
/// shown blocks in between come from the document, so it does not matter
/// whether they are mounted. Sections under folded headings are left out.
/// Blocks are separated by a blank line, text widgets inside one block by a
/// line break.
fn copy_text(
    content: &mut Element<'_, Message>,
    tree: &mut Tree,
    layout: Layout<'_>,
    renderer: &iced::Renderer,
    blocks: &[(BlockId, Block)],
    folded: &HashSet<BlockId>,
    (start, end): (Pos, Pos),
) -> String {
    let mut head: Vec<String> = Vec::new();
    let mut tail: Vec<String> = Vec::new();
    let mut seen = (false, false);
    visit(content, tree, layout, renderer, |block, sub, _, text| {
        let here = (block, sub);
        if here < start.widget() || here > end.widget() {
            return;
        }
        if block != start.block && block != end.block {
            return;
        }
        let lo = if here == start.widget() {
            start.end()
        } else {
            SelectionEnd::new(0, 0)
        };
        let hi = if here == end.widget() {
            end.end()
        } else {
            text.end()
        };
        let part = text.text_between(lo, hi);
        if block == start.block {
            seen.0 = true;
            head.push(part);
        } else {
            seen.1 = true;
            tail.push(part);
        }
    });
    let whole = |i: usize| {
        blocks
            .get(i)
            .and_then(|(_, b)| crate::plain_text::block_text(b))
    };
    let mut pieces: Vec<String> = Vec::new();
    // An end block that scrolled out of the virtual window falls back to
    // its whole text.
    pieces.push(if seen.0 {
        head.join("\n")
    } else {
        whole(start.block).unwrap_or_default()
    });
    pieces.extend(
        crate::virt::display_list(blocks, folded)
            .into_iter()
            .filter(|&i| i > start.block && i < end.block)
            .filter_map(whole),
    );
    if end.block != start.block {
        pieces.push(if seen.1 {
            tail.join("\n")
        } else {
            whole(end.block).unwrap_or_default()
        });
    }
    pieces.retain(|p| !p.is_empty());
    pieces.join("\n\n")
}

/// What the reader's text holds at `p`: the link there, and the selected
/// text (the scope's cross-block `range`, or one widget's own selection)
/// with whether `p` is inside it. Nothing changes.
#[allow(clippy::too_many_arguments)]
fn right_press_hit(
    content: &mut Element<'_, Message>,
    tree: &mut Tree,
    layout: Layout<'_>,
    renderer: &iced::Renderer,
    blocks: &[(BlockId, Block)],
    folded: &HashSet<BlockId>,
    range: Option<(Pos, Pos)>,
    p: Point,
) -> ReaderHit {
    let mut link = None;
    let mut own: Option<(String, bool)> = None;
    visit(content, tree, layout, renderer, |_, _, bounds, text| {
        if link.is_none() {
            link = match text.link_at(p, bounds) {
                Some(Message::OpenLink(url)) => Some(url),
                _ => None,
            };
        }
        if own.is_none() {
            own = text
                .selected_text()
                .map(|selected| (selected, text.selection_contains(p, bounds)));
        }
    });
    let (selection, in_selection) = match range {
        Some((start, end)) => {
            let inside = locate(content, tree, layout, renderer, p)
                .is_some_and(|(at, over)| over && start <= at && at <= end);
            let text = copy_text(
                content,
                tree,
                layout,
                renderer,
                blocks,
                folded,
                (start, end),
            );
            (Some(text), inside)
        }
        None => match own {
            Some((text, inside)) => (Some(text), inside),
            None => (None, false),
        },
    };
    ReaderHit {
        link,
        selection: selection.filter(|text| !text.is_empty()),
        in_selection,
    }
}

fn is_copy(event: &Event) -> bool {
    matches!(
        event,
        Event::Keyboard(keyboard::Event::KeyPressed { key: keyboard::Key::Character(c), modifiers, .. })
            if c.as_str() == "c" && modifiers.command()
    )
}

impl<'a> Widget<Message, iced::Theme, iced::Renderer> for SelectScope<'a> {
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
        let (state_tree, children) = split(tree);
        let state = state_tree.downcast_mut::<State>();
        let child_tree = &mut children[0];
        let child = child_layout(layout);

        let doc = (self.blocks.as_ptr() as usize, self.blocks.len());
        if state.doc != doc {
            *state = State {
                doc,
                painted: state.painted,
                ..State::default()
            };
        }

        let latin = crate::selectable::latin_event(event);
        let event_seen = latin.as_ref().unwrap_or(event);
        let mut forward = true;
        let mut right_press = None;

        match event_seen {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                state.range = None;
                state.crossing = false;
                state.press = cursor
                    .position_over(layout.bounds())
                    .and_then(|p| locate(&mut self.content, child_tree, child, renderer, p))
                    .and_then(|(pos, inside)| inside.then_some(pos));
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let (Some(anchor), Some(p)) = (state.press, cursor.land().position()) {
                    if let Some((focus, _)) =
                        locate(&mut self.content, child_tree, child, renderer, p)
                    {
                        if !state.crossing && focus.widget() != anchor.widget() {
                            state.crossing = true;
                            // The widget the drag started in drops its own
                            // selection and drag; the scope owns it now.
                            visit(
                                &mut self.content,
                                child_tree,
                                child,
                                renderer,
                                |_, _, _, t| {
                                    t.clear_selection();
                                },
                            );
                        }
                        if state.crossing {
                            state.range = Some((anchor, focus));
                            forward = false;
                            shell.request_redraw();
                        }
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.press = None;
                state.crossing = false;
            }
            // Reported after the content, so a `TargetMark` under the
            // pointer reports its element first.
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                right_press = cursor.position_over(layout.bounds());
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Escape),
                ..
            }) => {
                state.range = None;
            }
            // ⌘A selects the whole document (the app's select-all); the
            // cross-block range gives way to it.
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Character(c),
                modifiers,
                ..
            }) if c.as_str() == "a" && modifiers.command() => {
                state.range = None;
            }
            _ if is_copy(event_seen) => {
                if let Some(range) = state.ordered() {
                    let text = copy_text(
                        &mut self.content,
                        child_tree,
                        child,
                        renderer,
                        self.blocks,
                        &self.folded,
                        range,
                    );
                    clipboard.write(iced::advanced::clipboard::Kind::Standard, text);
                    shell.capture_event();
                    return;
                }
            }
            _ => {}
        }

        if forward {
            self.content.as_widget_mut().update(
                child_tree, event, child, cursor, renderer, clipboard, shell, viewport,
            );
        }

        if let Some(p) = right_press {
            let hit = right_press_hit(
                &mut self.content,
                child_tree,
                child,
                renderer,
                self.blocks,
                &self.folded,
                state.ordered(),
                p,
            );
            shell.publish(Message::ReaderContextMenu(hit));
            shell.capture_event();
        }

        // Paint after every event while a range exists, so blocks mounted by
        // scrolling show it too; clear once after it goes away.
        let range = state.ordered();
        if range.is_some() || state.painted {
            paint(&mut self.content, child_tree, child, renderer, range);
            state.painted = range.is_some();
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
    layout.children().next().expect("SelectScope has one child")
}

fn split(tree: &mut Tree) -> (&mut tree::State, &mut Vec<Tree>) {
    (&mut tree.state, &mut tree.children)
}

impl<'a> From<SelectScope<'a>> for Element<'a, Message> {
    fn from(w: SelectScope<'a>) -> Self {
        Element::new(w)
    }
}

impl<'a> Widget<Message, iced::Theme, iced::Renderer> for BlockMark<'a> {
    fn tag(&self) -> tree::Tag {
        self.content.as_widget().tag()
    }

    fn state(&self) -> tree::State {
        self.content.as_widget().state()
    }

    fn children(&self) -> Vec<Tree> {
        self.content.as_widget().children()
    }

    fn diff(&self, tree: &mut Tree) {
        self.content.as_widget().diff(tree);
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
        self.content.as_widget_mut().layout(tree, renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.custom(None, layout.bounds(), &mut BlockIndex(self.index));
        self.content
            .as_widget_mut()
            .operate(tree, layout, renderer, operation);
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
        self.content.as_widget_mut().update(
            tree, event, layout, cursor, renderer, clipboard, shell, viewport,
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
        self.content
            .as_widget()
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
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
        self.content
            .as_widget()
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        self.content
            .as_widget_mut()
            .overlay(tree, layout, renderer, viewport, translation)
    }
}

impl<'a> From<BlockMark<'a>> for Element<'a, Message> {
    fn from(w: BlockMark<'a>) -> Self {
        Element::new(w)
    }
}

/// Wraps a reader element that has its own context menu items; transparent
/// to layout, events, and the widget tree. On a right press over it, it
/// reports its element after its content; the innermost mark reports first.
pub struct TargetMark<'a> {
    target: Box<dyn Fn() -> ReaderTarget + 'a>,
    content: Element<'a, Message>,
}

impl<'a> TargetMark<'a> {
    /// `target` builds the element's description; it runs only on a right
    /// press, so a rebuild of the view copies nothing.
    pub fn new(
        target: impl Fn() -> ReaderTarget + 'a,
        content: impl Into<Element<'a, Message>>,
    ) -> Self {
        Self {
            target: Box::new(target),
            content: content.into(),
        }
    }
}

impl<'a> Widget<Message, iced::Theme, iced::Renderer> for TargetMark<'a> {
    fn tag(&self) -> tree::Tag {
        self.content.as_widget().tag()
    }

    fn state(&self) -> tree::State {
        self.content.as_widget().state()
    }

    fn children(&self) -> Vec<Tree> {
        self.content.as_widget().children()
    }

    fn diff(&self, tree: &mut Tree) {
        self.content.as_widget().diff(tree);
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
        self.content.as_widget_mut().layout(tree, renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(tree, layout, renderer, operation);
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
        self.content.as_widget_mut().update(
            tree, event, layout, cursor, renderer, clipboard, shell, viewport,
        );
        if matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))
        ) && cursor.is_over(layout.bounds())
        {
            shell.publish(Message::ReaderTarget((self.target)()));
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
        self.content
            .as_widget()
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
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
        self.content
            .as_widget()
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        self.content
            .as_widget_mut()
            .overlay(tree, layout, renderer, viewport, translation)
    }
}

impl<'a> From<TargetMark<'a>> for Element<'a, Message> {
    fn from(w: TargetMark<'a>) -> Self {
        Element::new(w)
    }
}
