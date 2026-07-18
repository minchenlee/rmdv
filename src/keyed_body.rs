//! A keyed wrapper around the body column that makes iced's positional diff
//! survive a sliding render window.
//!
//! Why not `iced::widget::keyed::Column`: its diff only repairs ONE contiguous
//! changed region (designed for append/prepend edits). A virtual-scroll window
//! shift changes keys at BOTH ends, so its trailing pairwise diff pairs trees
//! and widgets of different types without a tag check — stateful children
//! (mouse_area headings) later downcast a stateless tree and panic, and the
//! shaped-paragraph state is lost for every overlapping block anyway.
//!
//! `KeyedBody` instead stores the key list in its own state and, when keys
//! change, REORDERS the inner column's child trees to match the new key order
//! before delegating a normal tag-checked positional diff. Blocks that stay in
//! the window keep their widget state (shaped `rich_text` paragraphs) no
//! matter how far the window slid; vacated slots become empty trees that the
//! tag check rebuilds safely. Preview callers can select the fresh-diff mode,
//! which keeps this wrapper boundary but clears all child trees on a range or
//! document namespace change.

use iced::advanced::widget::{tree, Operation, Tree, Widget};
use iced::advanced::{layout, mouse, overlay, renderer, Clipboard, Layout, Shell};
use iced::{Element, Event, Length, Rectangle, Size, Vector};
use std::collections::HashMap;

/// Row identity inside the body column. Block rows carry their content-stable
/// `BlockId` hash; the spacers have fixed identities.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RowKey {
    TopSpacer,
    Block(u64),
    BottomSpacer,
}

pub struct KeyedBody<'a, Message, Theme, Renderer> {
    keys: Vec<RowKey>,
    content: Element<'a, Message, Theme, Renderer>,
    rebuild_on_key_change: bool,
    generation: (u64, u64),
}

impl<'a, Message, Theme, Renderer> KeyedBody<'a, Message, Theme, Renderer> {
    /// `keys` must parallel the children of `content` (one key per child).
    pub fn new(
        keys: Vec<RowKey>,
        content: impl Into<Element<'a, Message, Theme, Renderer>>,
    ) -> Self {
        Self::with_mode(keys, content, false, (0, 0))
    }

    /// Construct a preview body that keeps the keyed wrapper's safe tree
    /// boundary but drops all child trees when the materialized range or
    /// namespace changes. This avoids positional downcasts without paying a
    /// full rebuild on every steady-state frame.
    pub fn new_fresh(
        keys: Vec<RowKey>,
        content: impl Into<Element<'a, Message, Theme, Renderer>>,
        generation: (u64, u64),
    ) -> Self {
        Self::with_mode(keys, content, true, generation)
    }

    fn with_mode(
        keys: Vec<RowKey>,
        content: impl Into<Element<'a, Message, Theme, Renderer>>,
        rebuild_on_key_change: bool,
        generation: (u64, u64),
    ) -> Self {
        Self {
            keys,
            content: content.into(),
            rebuild_on_key_change,
            generation,
        }
    }
}

struct State {
    keys: Vec<RowKey>,
    generation: (u64, u64),
    rebuild_on_key_change: bool,
}

/// Rearrange `trees` (parallel to `old` keys) into `new` key order. Keys
/// missing from `old` get a fresh empty tree, which the subsequent positional
/// diff's tag check rebuilds from the new widget.
fn reorder_by_keys(trees: &mut Vec<Tree>, old: &[RowKey], new: &[RowKey]) {
    if trees.len() != old.len() {
        // The inner widget was replaced wholesale at some point; fall back to
        // the plain positional diff.
        return;
    }
    let mut by_key: HashMap<RowKey, Tree> = old.iter().copied().zip(trees.drain(..)).collect();
    *trees = new
        .iter()
        .map(|k| by_key.remove(k).unwrap_or_else(Tree::empty))
        .collect();
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for KeyedBody<'_, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            keys: self.keys.clone(),
            generation: self.generation,
            rebuild_on_key_change: self.rebuild_on_key_change,
        })
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        {
            let state = tree.state.downcast_mut::<State>();
            if state.keys != self.keys
                || state.rebuild_on_key_change != self.rebuild_on_key_change
                || (self.rebuild_on_key_change && state.generation != self.generation)
            {
                if let Some(inner) = tree.children.get_mut(0) {
                    if self.rebuild_on_key_change || state.rebuild_on_key_change {
                        // Keep the outer wrapper and its tag-checked child
                        // boundary, but force the inner Column to construct
                        // fresh children for a new preview range/document.
                        inner.children.clear();
                    } else {
                        reorder_by_keys(&mut inner.children, &state.keys, &self.keys);
                    }
                }
                state.keys.clone_from(&self.keys);
                state.generation = self.generation;
                state.rebuild_on_key_change = self.rebuild_on_key_change;
            }
        }
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
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
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
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
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
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message, Theme, Renderer> From<KeyedBody<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: 'a + renderer::Renderer,
{
    fn from(body: KeyedBody<'a, Message, Theme, Renderer>) -> Self {
        Element::new(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::advanced::widget::Widget;
    use iced::widget::{button, text, Column};

    #[test]
    fn reorder_moves_overlap_and_fills_holes() {
        let old = vec![RowKey::Block(1), RowKey::Block(2), RowKey::Block(3)];
        let mut trees: Vec<Tree> = old.iter().map(|_| Tree::empty()).collect();
        // Mark tree #2 (key Block(2)) so we can track where it lands.
        trees[1].children.push(Tree::empty());
        let new = vec![RowKey::Block(2), RowKey::Block(4)];
        reorder_by_keys(&mut trees, &old, &new);
        assert_eq!(trees.len(), 2);
        assert_eq!(
            trees[0].children.len(),
            1,
            "Block(2)'s tree must follow its key"
        );
        assert_eq!(trees[1].children.len(), 0, "Block(4) starts fresh");
    }

    #[test]
    fn reorder_bails_on_length_mismatch() {
        let old = vec![RowKey::Block(1)];
        let mut trees: Vec<Tree> = vec![];
        reorder_by_keys(&mut trees, &old, &[RowKey::Block(2)]);
        assert!(trees.is_empty(), "mismatched inputs must be left untouched");
    }

    #[test]
    fn fresh_diff_rebuilds_mixed_rows_without_positional_downcast() {
        type TestBody = KeyedBody<'static, (), iced::Theme, iced::Renderer>;

        let first_column = Column::new()
            .push(button("stateful").on_press(()))
            .push(text("stateless"));
        let first = TestBody::new_fresh(
            vec![RowKey::Block(1), RowKey::Block(2)],
            first_column,
            (0, 10),
        );
        let first_element: iced::Element<'static, (), iced::Theme, iced::Renderer> = first.into();
        let mut tree = Tree::new(&first_element);

        // The range now contains the opposite widget shapes under unrelated
        // keys. Fresh mode must clear the old child trees before Column's
        // positional diff, so this exercises the historical downcast panic
        // boundary directly.
        let second_column = Column::new()
            .push(text("stateless first"))
            .push(button("stateful second").on_press(()));
        let second = TestBody::new_fresh(
            vec![RowKey::Block(3), RowKey::Block(4)],
            second_column,
            (0, 11),
        );
        second.diff(&mut tree);

        assert_eq!(tree.children.len(), 1);
        assert_eq!(tree.children[0].children.len(), 2);
        let state = tree.state.downcast_ref::<State>();
        assert_eq!(state.generation, (0, 11));
        assert_eq!(state.keys, vec![RowKey::Block(3), RowKey::Block(4)]);
    }
}
