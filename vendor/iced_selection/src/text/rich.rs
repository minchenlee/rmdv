use iced_widget::graphics::text::Paragraph;
use iced_widget::graphics::text::cosmic_text;

use crate::click;
use crate::core::alignment;
use crate::core::clipboard;
use crate::core::keyboard;
use crate::core::keyboard::key;
use crate::core::layout;
use crate::core::mouse;
use crate::core::renderer;
use crate::core::text::{Paragraph as _, Span};
use crate::core::time::Duration;
use crate::core::touch;
use crate::core::widget::text::{Alignment, LineHeight, Shaping, Wrapping};
use crate::core::widget::tree::{self, Tree};
use crate::core::{
    self, Clipboard, Element, Event, Font, Layout, Length, Pixels, Point,
    Rectangle, Shell, Size, Vector, Widget,
};
use crate::selection::{Selection, SelectionEnd};
use crate::text::{Catalog, Dragging, Style, StyleFn};

/// A bunch of [`Rich`] text.
pub struct Rich<
    'a,
    Link,
    Message,
    Theme = iced_widget::Theme,
    Renderer = iced_widget::Renderer,
> where
    Link: Clone + 'static,
    Theme: Catalog,
    Renderer: core::text::Renderer,
{
    spans: Box<dyn AsRef<[Span<'a, Link, Renderer::Font>]> + 'a>,
    size: Option<Pixels>,
    line_height: LineHeight,
    width: Length,
    height: Length,
    font: Option<Renderer::Font>,
    align_x: Alignment,
    align_y: alignment::Vertical,
    wrapping: Wrapping,
    click_interval: Option<Duration>,
    class: Theme::Class<'a>,
    on_link_click: Option<Box<dyn Fn(Link) -> Message + 'a>>,
    on_link_hover: Option<Box<dyn Fn(Link) -> Message + 'a>>,
    on_hover_lost: Option<Box<dyn Fn() -> Message + 'a>>,
    on_link_enter: Option<Box<dyn Fn(Link, u64) -> Message + 'a>>,
    on_link_leave: Option<Box<dyn Fn(Link, u64) -> Message + 'a>>,
}

impl<'a, Link, Message, Theme, Renderer>
    Rich<'a, Link, Message, Theme, Renderer>
where
    Link: Clone + 'static,
    Theme: Catalog,
    Renderer: core::text::Renderer,
    Renderer::Font: 'a,
{
    /// Creates a new empty [`Rich`] text.
    pub fn new() -> Self {
        Self {
            spans: Box::new([]),
            size: None,
            line_height: LineHeight::default(),
            width: Length::Shrink,
            height: Length::Shrink,
            font: None,
            align_x: Alignment::Default,
            align_y: alignment::Vertical::Top,
            wrapping: Wrapping::default(),
            click_interval: None,
            class: Theme::default(),
            on_link_click: None,
            on_link_hover: None,
            on_hover_lost: None,
            on_link_enter: None,
            on_link_leave: None,
        }
    }

    /// Creates a new [`Rich`] text with the given text spans.
    pub fn with_spans(
        spans: impl AsRef<[Span<'a, Link, Renderer::Font>]> + 'a,
    ) -> Self {
        Self {
            spans: Box::new(spans),
            ..Self::new()
        }
    }

    /// Sets the default size of the [`Rich`] text.
    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.size = Some(size.into());
        self
    }

    /// Sets the default [`LineHeight`] of the [`Rich`] text.
    pub fn line_height(mut self, line_height: impl Into<LineHeight>) -> Self {
        self.line_height = line_height.into();
        self
    }

    /// Sets the default font of the [`Rich`] text.
    pub fn font(mut self, font: impl Into<Renderer::Font>) -> Self {
        self.font = Some(font.into());
        self
    }

    /// Sets the width of the [`Rich`] text boundaries.
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Sets the height of the [`Rich`] text boundaries.
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    /// Centers the [`Rich`] text, both horizontally and vertically.
    pub fn center(self) -> Self {
        self.align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Center)
    }

    /// Sets the [`alignment::Horizontal`] of the [`Rich`] text.
    pub fn align_x(mut self, alignment: impl Into<Alignment>) -> Self {
        self.align_x = alignment.into();
        self
    }

    /// Sets the [`alignment::Vertical`] of the [`Rich`] text.
    pub fn align_y(
        mut self,
        alignment: impl Into<alignment::Vertical>,
    ) -> Self {
        self.align_y = alignment.into();
        self
    }

    /// Sets the [`Wrapping`] strategy of the [`Rich`] text.
    pub fn wrapping(mut self, wrapping: Wrapping) -> Self {
        self.wrapping = wrapping;
        self
    }

    /// The maximum delay required for two consecutive clicks to be interpreted as a double click
    /// (also applies to triple clicks).
    ///
    /// Defaults to 300ms.
    pub fn click_interval(mut self, click_interval: Duration) -> Self {
        self.click_interval = Some(click_interval);
        self
    }

    /// Sets the message that will be produced when a link of the [`Rich`] text
    /// is clicked.
    ///
    /// If the spans of the [`Rich`] text contain no links, you may need to call
    /// this method with `on_link_click(never)` in order for the compiler to infer
    /// the proper `Link` generic type.
    pub fn on_link_click(
        mut self,
        on_link_click: impl Fn(Link) -> Message + 'a,
    ) -> Self {
        self.on_link_click = Some(Box::new(on_link_click));
        self
    }

    /// Sets the message that will be produced when a link of the [`Rich`] text
    /// is hovered.
    pub fn on_link_hover(
        mut self,
        on_link_hovered: impl Fn(Link) -> Message + 'a,
    ) -> Self {
        self.on_link_hover = Some(Box::new(on_link_hovered));
        self
    }

    /// Sets the message that will be produced when a link of the [`Rich`] text
    /// is no longer hovered.
    pub fn on_hover_lost(mut self, on_hover_lost: Message) -> Self
    where
        Message: Clone + 'a,
    {
        self.on_hover_lost = Some(Box::new(move || on_hover_lost.clone()));
        self
    }

    /// Sets the message that will be produced, with the link and the id
    /// of this widget's state, when the pointer enters a link of the
    /// [`Rich`] text. It takes precedence over [`Rich::on_link_hover`].
    /// The id tells apart two widgets that hold the same link.
    pub fn on_link_enter(
        mut self,
        on_link_enter: impl Fn(Link, u64) -> Message + 'a,
    ) -> Self {
        self.on_link_enter = Some(Box::new(on_link_enter));
        self
    }

    /// Sets the message that will be produced, with the link that was
    /// hovered and the id of this widget's state, when the pointer leaves
    /// a link of the [`Rich`] text. It takes precedence over
    /// [`Rich::on_hover_lost`].
    pub fn on_link_leave(
        mut self,
        on_link_leave: impl Fn(Link, u64) -> Message + 'a,
    ) -> Self {
        self.on_link_leave = Some(Box::new(on_link_leave));
        self
    }

    /// Publishes the hover message for a change of the hovered link span:
    /// entering `now`'s link, or else leaving `before`'s.
    fn publish_link_change(
        &self,
        id: u64,
        before: Option<usize>,
        now: Option<usize>,
        shell: &mut Shell<'_, Message>,
    ) {
        let link_of = |index: Option<usize>| {
            self.spans
                .as_ref()
                .as_ref()
                .get(index?)
                .and_then(|span| span.link.clone())
        };
        if let Some(link) = link_of(now) {
            if let Some(on_link_enter) = self.on_link_enter.as_deref() {
                shell.publish(on_link_enter(link, id));
            } else if let Some(on_link_hovered) = self.on_link_hover.as_deref()
            {
                shell.publish(on_link_hovered(link));
            }
        } else if let Some(on_link_leave) = self.on_link_leave.as_deref() {
            if let Some(link) = link_of(before) {
                shell.publish(on_link_leave(link, id));
            }
        } else if let Some(on_hover_lost) = self.on_hover_lost.as_deref() {
            shell.publish(on_hover_lost());
        }
    }

    /// Sets the message that will be produced when a link of the [`Rich`] text
    /// is no longer hovered.
    pub fn on_hover_lost_with(
        mut self,
        on_hover_lost: impl Fn() -> Message + 'a,
    ) -> Self {
        self.on_hover_lost = Some(Box::new(on_hover_lost));
        self
    }

    /// Sets the default style of the [`Rich`] text.
    #[must_use]
    pub fn style(mut self, style: impl Fn(&Theme) -> Style + 'a) -> Self
    where
        Theme::Class<'a>: From<StyleFn<'a, Theme>>,
    {
        self.class = (Box::new(style) as StyleFn<'a, Theme>).into();
        self
    }

    /// Sets the default style class of the [`Rich`] text.
    #[must_use]
    pub fn class(mut self, class: impl Into<Theme::Class<'a>>) -> Self {
        self.class = class.into();
        self
    }
}

impl<'a, Link, Message, Theme, Renderer> Default
    for Rich<'a, Link, Message, Theme, Renderer>
where
    Link: Clone + 'a,
    Theme: Catalog,
    Renderer: core::text::Renderer<Paragraph = Paragraph, Font = Font>,
    Renderer::Font: 'a,
{
    fn default() -> Self {
        Self::new()
    }
}

/// The state of a [`Rich`] text widget.
///
/// [`Rich::operate`](Widget::operate) hands it to
/// [`Operation::custom`](core::widget::Operation::custom), so a parent can
/// drive one selection across several widgets (rmdv).
pub struct State<Link> {
    spans: Vec<Span<'static, Link, Font>>,
    span_pressed: Option<usize>,
    hovered_link: Option<usize>,
    paragraph: Paragraph,
    is_hovered: bool,
    selection: Selection,
    dragging: Option<Dragging>,
    last_click: Option<click::Click>,
    keyboard_modifiers: keyboard::Modifiers,
    visual_lines_bounds: Vec<core::Rectangle>,
    external: Option<Selection>,
    /// Unique per state: the source of this widget's link messages.
    id: u64,
}

impl<Link> State<Link> {
    fn grapheme_line_and_index(
        &self,
        point: Point,
        bounds: core::Rectangle,
    ) -> Option<(usize, usize)> {
        self.hit_grapheme(point, bounds)
            .map(|(line, index, _)| (line, index))
    }

    /// The grapheme under `point`: its line and index. A press on the
    /// right half of a grapheme puts the caret after it; this is still
    /// the grapheme that was pressed.
    fn grapheme_under(
        &self,
        point: Point,
        bounds: core::Rectangle,
    ) -> Option<(usize, usize)> {
        self.hit_grapheme(point, bounds).map(|(line, index, after)| {
            (line, if after { index.saturating_sub(1) } else { index })
        })
    }

    /// The caret position nearest to `point` (line, grapheme index), and
    /// whether it lies after the grapheme the point is over.
    fn hit_grapheme(
        &self,
        point: Point,
        bounds: core::Rectangle,
    ) -> Option<(usize, usize, bool)> {
        let bounded_x = if point.y < bounds.y {
            bounds.x
        } else if point.y > bounds.y + bounds.height {
            bounds.x + bounds.width
        } else {
            point.x.max(bounds.x).min(bounds.x + bounds.width)
        };
        let bounded_y = point.y.max(bounds.y).min(bounds.y + bounds.height);
        let bounded_point = Point::new(bounded_x, bounded_y);
        let mut relative_point =
            bounded_point - core::Vector::new(bounds.x, bounds.y);

        let buffer = self.paragraph.buffer();
        let line_height = buffer.metrics().line_height;
        let visual_line = (relative_point.y / line_height).floor() as usize;
        let visual_line_start_offset = self
            .visual_lines_bounds
            .get(visual_line)
            .map(|r| r.x)
            .unwrap_or_default();
        let visual_line_end = self
            .visual_lines_bounds
            .get(visual_line)
            .map(|r| r.x + r.width)
            .unwrap_or_default();

        if relative_point.x < visual_line_start_offset {
            relative_point.x = visual_line_start_offset;
        }

        if relative_point.x > visual_line_end {
            relative_point.x = visual_line_end;
        }

        let cursor = buffer.hit(relative_point.x, relative_point.y)?;
        let value = buffer.lines[cursor.line].text();

        Some((
            cursor.line,
            unicode_segmentation::UnicodeSegmentation::graphemes(
                &value[..cursor.index.min(value.len())],
                true,
            )
            .count(),
            cursor.affinity == cosmic_text::Affinity::Before,
        ))
    }

    fn selection(&self, selection: Selection) -> Vec<core::Rectangle> {
        let Selection { start, end, .. } = selection;

        let buffer = self.paragraph.buffer();
        let line_height = self.paragraph.buffer().metrics().line_height;
        let selected_lines = end.line - start.line + 1;

        let visual_lines_offset = visual_lines_offset(start.line, buffer);

        buffer
            .lines
            .iter()
            .skip(start.line)
            .take(selected_lines)
            .enumerate()
            .flat_map(|(i, line)| {
                highlight_line(
                    line,
                    if i == 0 { start.index } else { 0 },
                    if i == selected_lines - 1 {
                        end.index
                    } else {
                        line.text().len()
                    },
                )
            })
            .enumerate()
            .filter_map(|(visual_line, (x, width))| {
                if width > 0.0 {
                    Some(core::Rectangle {
                        x,
                        width,
                        y: (visual_line as i32 + visual_lines_offset) as f32
                            * line_height
                            - buffer.scroll().vertical,
                        height: line_height,
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    /// The position nearest to `point`. Both `point` and `bounds` (the
    /// widget's layout bounds) are in the same coordinates.
    pub fn position_at(
        &self,
        point: Point,
        bounds: core::Rectangle,
    ) -> SelectionEnd {
        let (line, index) = self
            .grapheme_line_and_index(point, bounds)
            .unwrap_or((0, 0));

        SelectionEnd { line, index }
    }

    /// The position after the last grapheme.
    pub fn end(&self) -> SelectionEnd {
        let lines = &self.paragraph.buffer().lines;
        let line = lines.len().saturating_sub(1);
        let index = lines.get(line).map_or(0, |l| {
            unicode_segmentation::UnicodeSegmentation::graphemes(l.text(), true)
                .count()
        });

        SelectionEnd { line, index }
    }

    /// The text between two positions, in either order.
    pub fn text_between(&self, a: SelectionEnd, b: SelectionEnd) -> String {
        let mut selection = Selection::default();
        selection.select_range(a, b);
        selection.text(&self.paragraph)
    }

    /// How the widget's current drag selects (by grapheme, word, or
    /// line); `None` when no drag is in progress.
    pub fn dragging(&self) -> Option<Dragging> {
        self.dragging
    }

    /// The widget's own selection as (anchor, focus): the end that stays
    /// and the end that moves. `None` when it is empty.
    pub fn selection_ends(&self) -> Option<(SelectionEnd, SelectionEnd)> {
        if self.selection.is_empty() {
            return None;
        }
        let Selection { start, end, .. } = self.selection;
        let focus = self.selection.active_end();
        Some((if focus == end { start } else { end }, focus))
    }

    /// `at` moved out to the edge of its word or line, as a word or line
    /// drag selects: to the end when `forward`, else to the start.
    pub fn snap(
        &self,
        at: SelectionEnd,
        unit: Dragging,
        forward: bool,
    ) -> SelectionEnd {
        let Some(line) = self.paragraph.buffer().lines.get(at.line) else {
            return at;
        };
        let text = line.text();
        let index = match unit {
            Dragging::Grapheme => at.index,
            Dragging::Word if forward => {
                crate::word::next_end_of_word(text, at.index)
            }
            Dragging::Word => crate::word::previous_start_of_word(text, at.index),
            Dragging::Line if forward => {
                unicode_segmentation::UnicodeSegmentation::graphemes(text, true)
                    .count()
            }
            Dragging::Line => 0,
        };
        SelectionEnd::new(at.line, index)
    }

    /// Whether the widget's own selection is not empty.
    pub fn has_selection(&self) -> bool {
        !self.selection.is_empty()
    }

    /// The text of the widget's own selection; `None` when it is empty.
    pub fn selected_text(&self) -> Option<String> {
        (!self.selection.is_empty())
            .then(|| self.selection.text(&self.paragraph))
    }

    /// Whether `point` lies inside the widget's own selection. Both `point`
    /// and `bounds` (the widget's layout bounds) are in the same
    /// coordinates.
    pub fn selection_contains(
        &self,
        point: Point,
        bounds: core::Rectangle,
    ) -> bool {
        if self.selection.is_empty() || !bounds.contains(point) {
            return false;
        }
        let at = self.position_at(point, bounds);
        let Selection { start, end, .. } = self.selection;
        start.min(end) <= at && at <= start.max(end)
    }

    /// The link of the span under `point`, if any. Both `point` and
    /// `bounds` (the widget's layout bounds) are in the same coordinates.
    pub fn link_at(&self, point: Point, bounds: core::Rectangle) -> Option<Link>
    where
        Link: Clone,
    {
        if !bounds.contains(point) {
            return None;
        }
        let span = self
            .paragraph
            .hit_span(point - core::Vector::new(bounds.x, bounds.y))?;
        self.spans.get(span)?.link.clone()
    }

    /// Clears the widget's own selection and ends its drag.
    pub fn clear_selection(&mut self) {
        self.selection = Selection::default();
        self.dragging = None;
        self.span_pressed = None;
    }

    /// Draws `range` instead of the widget's own selection; `None` draws
    /// the widget's own selection again.
    pub fn set_external_selection(
        &mut self,
        range: Option<(SelectionEnd, SelectionEnd)>,
    ) {
        self.external = range.map(|(a, b)| {
            let mut selection = Selection::default();
            selection.select_range(a, b);
            selection
        });
    }

    fn update_visual_bounds(&mut self) {
        let buffer = self.paragraph.buffer();
        let line_height = buffer.metrics().line_height;
        self.visual_lines_bounds = buffer
            .lines
            .iter()
            .flat_map(|line| highlight_line(line, 0, line.text().len()))
            .enumerate()
            .map(|(visual_line, (x, width))| core::Rectangle {
                x,
                width,
                y: visual_line as f32 * line_height - buffer.scroll().vertical,
                height: line_height,
            })
            .collect();
    }
}

impl<Link, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Rich<'_, Link, Message, Theme, Renderer>
where
    Link: Clone + 'static,
    Theme: Catalog,
    Renderer: core::text::Renderer<Paragraph = Paragraph, Font = Font>,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State<Link>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::<Link> {
            spans: Vec::new(),
            span_pressed: None,
            hovered_link: None,
            paragraph: Paragraph::default(),
            is_hovered: false,
            selection: Selection::default(),
            dragging: None,
            last_click: None,
            keyboard_modifiers: keyboard::Modifiers::default(),
            visual_lines_bounds: Vec::new(),
            external: None,
            id: {
                use std::sync::atomic::{AtomicU64, Ordering};
                static NEXT_ID: AtomicU64 = AtomicU64::new(1);
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            },
        })
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout(
            tree.state.downcast_mut::<State<Link>>(),
            renderer,
            limits,
            self.width,
            self.height,
            self.spans.as_ref().as_ref(),
            self.line_height,
            self.size,
            self.font,
            self.align_x,
            self.align_y,
            self.wrapping,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        defaults: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        if !layout.bounds().intersects(viewport) {
            return;
        }

        let state = tree.state.downcast_ref::<State<Link>>();

        let style = theme.style(&self.class);

        for (index, span) in self.spans.as_ref().as_ref().iter().enumerate() {
            let is_hovered_link = self.on_link_click.is_some()
                && Some(index) == state.hovered_link;
            // With a hover fill, a hovered link is tinted; without one it
            // gets an underline, as upstream draws it.
            let hover_underline =
                is_hovered_link && style.link_hover.is_none();

            if span.highlight.is_some()
                || span.underline
                || span.strikethrough
                || is_hovered_link
            {
                let translation = layout.position() - Point::ORIGIN;
                let regions = state.paragraph.span_bounds(index);

                if let Some(hover) =
                    style.link_hover.filter(|_| is_hovered_link)
                {
                    for bounds in &regions {
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: Rectangle::new(
                                    bounds.position() - Vector::new(2.0, 0.0),
                                    bounds.size() + Size::new(4.0, 0.0),
                                ) + translation,
                                border: core::Border {
                                    radius: hover.radius.into(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            },
                            hover.background,
                        );
                    }
                }

                if let Some(highlight) = span.highlight {
                    for bounds in &regions {
                        let bounds = Rectangle::new(
                            bounds.position()
                                - Vector::new(
                                    span.padding.left,
                                    span.padding.top,
                                ),
                            bounds.size()
                                + Size::new(span.padding.x(), span.padding.y()),
                        );

                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: bounds + translation,
                                border: highlight.border,
                                ..Default::default()
                            },
                            highlight.background,
                        );
                    }
                }

                if span.underline || span.strikethrough || hover_underline {
                    let size = span
                        .size
                        .or(self.size)
                        .unwrap_or(renderer.default_size());

                    let line_height = span
                        .line_height
                        .unwrap_or(self.line_height)
                        .to_absolute(size);

                    let color = span
                        .color
                        .or(style.color)
                        .unwrap_or(defaults.text_color);

                    let baseline = translation
                        + Vector::new(
                            0.0,
                            size.0 + (line_height.0 - size.0) / 2.0,
                        );

                    if span.underline || hover_underline {
                        for bounds in &regions {
                            renderer.fill_quad(
                                renderer::Quad {
                                    bounds: Rectangle::new(
                                        bounds.position() + baseline
                                            - Vector::new(0.0, size.0 * 0.08),
                                        Size::new(bounds.width, 1.0),
                                    ),
                                    ..Default::default()
                                },
                                color,
                            );
                        }
                    }

                    if span.strikethrough {
                        for bounds in &regions {
                            renderer.fill_quad(
                                renderer::Quad {
                                    bounds: Rectangle::new(
                                        bounds.position() + baseline
                                            - Vector::new(0.0, size.0 / 2.0),
                                        Size::new(bounds.width, 1.0),
                                    ),
                                    ..Default::default()
                                },
                                color,
                            );
                        }
                    }
                }
            }
        }

        let shown = state.external.unwrap_or(state.selection);

        if !shown.is_empty() {
            let bounds = layout.bounds();
            let translation = bounds.position() - Point::ORIGIN;
            let ranges = state.selection(shown);

            for range in ranges
                .into_iter()
                .filter_map(|range| bounds.intersection(&(range + translation)))
            {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: range,
                        ..renderer::Quad::default()
                    },
                    style.selection,
                );
            }
        }

        crate::text::draw(
            renderer,
            defaults,
            layout.bounds(),
            &state.paragraph,
            style,
            viewport,
        );
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State<Link>>();

        let bounds = layout.bounds();
        let click_position = cursor.position_in(bounds);

        if viewport.intersection(&bounds).is_none()
            && state.selection.is_empty()
            && state.dragging.is_none()
        {
            // Scrolled out of view under the pointer (a jump moves it
            // in one step): it no longer hovers its link.
            if let Some(index) = state.hovered_link.take() {
                state.is_hovered = false;
                self.publish_link_change(state.id, Some(index), None, shell);
                shell.request_redraw();
            }
            return;
        }

        let was_hovered = state.is_hovered;
        let hovered_link_before = state.hovered_link;
        let selection_before = state.selection;

        state.is_hovered = click_position.is_some();

        if let Some(position) = click_position {
            state.hovered_link =
                state.paragraph.hit_span(position).and_then(|span| {
                    if self.spans.as_ref().as_ref().get(span)?.link.is_some() {
                        Some(span)
                    } else {
                        None
                    }
                });
        } else {
            state.hovered_link = None;
        }

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerPressed { .. }) => {
                if state.hovered_link.is_some() {
                    state.span_pressed = state.hovered_link;
                    shell.capture_event();
                }

                if let Some(position) = cursor.position_over(bounds) {
                    let click = click::Click::new(
                        position,
                        mouse::Button::Left,
                        state.last_click,
                        self.click_interval,
                    );

                    let (line, index) = state
                        .grapheme_line_and_index(position, bounds)
                        .unwrap_or((0, 0));

                    match click.kind() {
                        click::Kind::Single => {
                            let new_end = SelectionEnd { line, index };

                            if state.keyboard_modifiers.shift() {
                                state.selection.change_selection(new_end);
                            } else {
                                state.selection.select_range(new_end, new_end);
                            }

                            state.dragging = Some(Dragging::Grapheme);
                        }
                        click::Kind::Double => {
                            let (line, index) = state
                                .grapheme_under(position, bounds)
                                .unwrap_or((line, index));
                            state.selection.select_word(
                                line,
                                index,
                                &state.paragraph,
                            );
                            state.dragging = Some(Dragging::Word);
                        }
                        click::Kind::Triple => {
                            state.selection.select_line(line, &state.paragraph);
                            state.dragging = Some(Dragging::Line);
                        }
                    }

                    state.last_click = Some(click);

                    shell.capture_event();
                } else {
                    state.selection = Selection::default();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerLifted { .. })
            | Event::Touch(touch::Event::FingerLost { .. }) => {
                state.dragging = None;
                if !matches!(
                    event,
                    Event::Touch(touch::Event::FingerLost { .. })
                ) && state.selection.is_empty()
                {
                    match state.span_pressed {
                        Some(span) if Some(span) == state.hovered_link => {
                            if let Some((link, on_link_clicked)) = self
                                .spans
                                .as_ref()
                                .as_ref()
                                .get(span)
                                .and_then(|span| span.link.clone())
                                .zip(self.on_link_click.as_deref())
                            {
                                shell.publish(on_link_clicked(link));
                            }
                        }
                        _ => {}
                    }

                    state.span_pressed = None;
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. })
            | Event::Touch(touch::Event::FingerMoved { .. }) => {
                if let Some(position) = cursor.land().position()
                    && let Some(dragging) = state.dragging
                {
                    let (line, index) = state
                        .grapheme_line_and_index(position, bounds)
                        .unwrap_or((0, 0));

                    match dragging {
                        Dragging::Grapheme => {
                            let new_end = SelectionEnd { line, index };

                            state.selection.change_selection(new_end);
                        }
                        Dragging::Word => {
                            let new_end = SelectionEnd { line, index };

                            state.selection.change_selection_by_word(
                                new_end,
                                &state.paragraph,
                            );
                        }
                        Dragging::Line => {
                            state.selection.change_selection_by_line(
                                line,
                                &state.paragraph,
                            );
                        }
                    };
                }
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) => {
                match key.as_ref() {
                    keyboard::Key::Character("c")
                        if state.keyboard_modifiers.command()
                            && !state.selection.is_empty() =>
                    {
                        clipboard.write(
                            clipboard::Kind::Standard,
                            state.selection.text(&state.paragraph),
                        );

                        shell.capture_event();
                    }
                    keyboard::Key::Character("a")
                        if state.keyboard_modifiers.command()
                            && !state.selection.is_empty() =>
                    {
                        state.selection.select_all(&state.paragraph);

                        shell.capture_event();
                    }
                    keyboard::Key::Named(key::Named::Home)
                        if state.keyboard_modifiers.shift()
                            && !state.selection.is_empty() =>
                    {
                        if state.keyboard_modifiers.jump() {
                            state.selection.select_beginning();
                        } else {
                            state.selection.select_line_beginning();
                        }

                        shell.capture_event();
                    }
                    keyboard::Key::Named(key::Named::End)
                        if state.keyboard_modifiers.shift()
                            && !state.selection.is_empty() =>
                    {
                        if state.keyboard_modifiers.jump() {
                            state.selection.select_end(&state.paragraph);
                        } else {
                            state.selection.select_line_end(&state.paragraph);
                        }

                        shell.capture_event();
                    }
                    keyboard::Key::Named(key::Named::ArrowLeft)
                        if state.keyboard_modifiers.shift()
                            && !state.selection.is_empty() =>
                    {
                        if state.keyboard_modifiers.macos_command() {
                            state.selection.select_line_beginning();
                        } else if state.keyboard_modifiers.jump() {
                            state
                                .selection
                                .select_left_by_words(&state.paragraph);
                        } else {
                            state.selection.select_left(&state.paragraph);
                        }

                        shell.capture_event();
                    }
                    keyboard::Key::Named(key::Named::ArrowRight)
                        if state.keyboard_modifiers.shift()
                            && !state.selection.is_empty() =>
                    {
                        if state.keyboard_modifiers.macos_command() {
                            state.selection.select_line_end(&state.paragraph);
                        } else if state.keyboard_modifiers.jump() {
                            state
                                .selection
                                .select_right_by_words(&state.paragraph);
                        } else {
                            state.selection.select_right(&state.paragraph);
                        }

                        shell.capture_event();
                    }
                    keyboard::Key::Named(key::Named::ArrowUp)
                        if state.keyboard_modifiers.shift()
                            && !state.selection.is_empty() =>
                    {
                        if state.keyboard_modifiers.macos_command() {
                            state.selection.select_beginning();
                        } else if state.keyboard_modifiers.jump() {
                            state.selection.select_line_beginning();
                        } else {
                            state.selection.select_up(&state.paragraph);
                        }

                        shell.capture_event();
                    }
                    keyboard::Key::Named(key::Named::ArrowDown)
                        if state.keyboard_modifiers.shift()
                            && !state.selection.is_empty() =>
                    {
                        if state.keyboard_modifiers.macos_command() {
                            state.selection.select_end(&state.paragraph);
                        } else if state.keyboard_modifiers.jump() {
                            state.selection.select_line_end(&state.paragraph);
                        } else {
                            state.selection.select_down(&state.paragraph);
                        }

                        shell.capture_event();
                    }
                    keyboard::Key::Named(key::Named::Escape) => {
                        state.dragging = None;
                        state.selection = Selection::default();

                        state.keyboard_modifiers =
                            keyboard::Modifiers::default();

                        if state.selection != selection_before {
                            shell.capture_event();
                        }
                    }
                    _ => {}
                }
            }
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.keyboard_modifiers = *modifiers;
            }
            _ => {}
        }

        // Hover messages go out only when the hovered link changes: once on
        // entering a link and once on leaving it, never per pointer move.
        if state.hovered_link != hovered_link_before {
            self.publish_link_change(
                state.id,
                hovered_link_before,
                state.hovered_link,
                shell,
            );
        }

        if state.is_hovered != was_hovered
            || state.selection != selection_before
            || state.hovered_link != hovered_link_before
        {
            shell.request_redraw();
        }
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn core::widget::Operation,
    ) {
        operation.custom(
            None,
            layout.bounds(),
            tree.state.downcast_mut::<State<Link>>(),
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State<Link>>();

        if state.hovered_link.is_some() {
            mouse::Interaction::Pointer
        } else if cursor.is_over(layout.bounds()) || state.dragging.is_some() {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::None
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn layout<Link, Renderer>(
    state: &mut State<Link>,
    renderer: &Renderer,
    limits: &layout::Limits,
    width: Length,
    height: Length,
    spans: &[Span<'_, Link, Renderer::Font>],
    line_height: LineHeight,
    size: Option<Pixels>,
    font: Option<Renderer::Font>,
    align_x: Alignment,
    align_y: alignment::Vertical,
    wrapping: Wrapping,
) -> layout::Node
where
    Link: Clone,
    Renderer: core::text::Renderer<Paragraph = Paragraph, Font = Font>,
{
    layout::sized(limits, width, height, |limits| {
        let bounds = limits.max();

        let size = size.unwrap_or_else(|| renderer.default_size());
        let font = font.unwrap_or_else(|| renderer.default_font());

        let text_with_spans = || core::Text {
            content: spans,
            bounds,
            size,
            line_height,
            font,
            align_x,
            align_y,
            shaping: Shaping::Advanced,
            wrapping,
        };

        if state.spans != spans {
            state.paragraph =
                Renderer::Paragraph::with_spans(text_with_spans());
            state.spans = spans.iter().cloned().map(Span::to_static).collect();
            state.update_visual_bounds();
        } else {
            match state.paragraph.compare(core::Text {
                content: (),
                bounds,
                size,
                line_height,
                font,
                align_x,
                align_y,
                shaping: Shaping::Advanced,
                wrapping,
            }) {
                core::text::Difference::None => {}
                core::text::Difference::Bounds => {
                    state.paragraph.resize(bounds);
                    state.update_visual_bounds();
                }
                core::text::Difference::Shape => {
                    state.paragraph =
                        Renderer::Paragraph::with_spans(text_with_spans());
                    state.update_visual_bounds();
                }
            }
        }

        state.paragraph.min_bounds()
    })
}

impl<'a, Link, Message, Theme, Renderer>
    FromIterator<Span<'a, Link, Renderer::Font>>
    for Rich<'a, Link, Message, Theme, Renderer>
where
    Link: Clone + 'a,
    Theme: Catalog,
    Renderer: core::text::Renderer<Paragraph = Paragraph, Font = Font>,
    Renderer::Font: 'a,
{
    fn from_iter<T: IntoIterator<Item = Span<'a, Link, Renderer::Font>>>(
        spans: T,
    ) -> Self {
        Self::with_spans(spans.into_iter().collect::<Vec<_>>())
    }
}

impl<'a, Link, Message, Theme, Renderer>
    From<Rich<'a, Link, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Link: Clone + 'a,
    Theme: Catalog + 'a,
    Renderer: core::text::Renderer<Paragraph = Paragraph, Font = Font> + 'a,
{
    fn from(
        text: Rich<'a, Link, Message, Theme, Renderer>,
    ) -> Element<'a, Message, Theme, Renderer> {
        Element::new(text)
    }
}

fn highlight_line(
    line: &cosmic_text::BufferLine,
    from: usize,
    to: usize,
) -> impl Iterator<Item = (f32, f32)> + '_ {
    let layout = line.layout_opt().map(Vec::as_slice).unwrap_or_default();

    // Check for multi codepoint glyphs for each previous visual line
    let mut previous_diff = 0;
    let previous_lines_diff = line
        .layout_opt()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(move |(line_nr, visual_line)| {
            if line_nr == 0 {
                let current_diff = previous_diff
                    + visual_line
                        .glyphs
                        .iter()
                        .fold(0, |d, g| d + g.start.abs_diff(g.end) - 1);
                previous_diff = current_diff;
                0
            } else {
                let current_diff = previous_diff
                    + visual_line
                        .glyphs
                        .iter()
                        .fold(0, |d, g| d + g.start.abs_diff(g.end) - 1);
                let previous_diff_temp = previous_diff;
                previous_diff = current_diff;
                previous_diff_temp
            }
        });

    layout.iter().zip(previous_lines_diff).map(
        move |(visual_line, previous_lines_diff)| {
            let start = visual_line
                .glyphs
                .first()
                .map(|glyph| glyph.start)
                .unwrap_or(0);
            let end = visual_line
                .glyphs
                .last()
                .map(|glyph| glyph.end)
                .unwrap_or(0);

            let to = to + previous_lines_diff;
            let mut range = start.max(from)..end.min(to);

            let x_offset = visual_line
                .glyphs
                .first()
                .map(|glyph| glyph.x)
                .unwrap_or_default();

            if range.is_empty() {
                (x_offset, 0.0)
            } else if range.start == start && range.end == end {
                (x_offset, visual_line.w)
            } else {
                let mut x = 0.0;
                let mut width = 0.0;
                for glyph in &visual_line.glyphs {
                    let glyph_count = glyph.start.abs_diff(glyph.end);

                    // Check for multi codepoint glyphs before or within the range
                    if glyph_count > 1 {
                        if range.start > glyph.start {
                            range.start += glyph_count - 1;
                            range.end += glyph_count - 1;
                        } else if range.end > glyph.start {
                            range.end += glyph_count - 1;
                        }
                    }

                    if range.start > glyph.start {
                        x += glyph.w;
                    }

                    if range.start <= glyph.start && range.end > glyph.start {
                        width += glyph.w;
                    } else if range.end <= glyph.start {
                        break;
                    }
                }

                (x_offset + x, width)
            }
        },
    )
}

fn visual_lines_offset(line: usize, buffer: &cosmic_text::Buffer) -> i32 {
    let scroll = buffer.scroll();

    let start = scroll.line.min(line);
    let end = scroll.line.max(line);

    let visual_lines_offset: usize = buffer.lines[start..]
        .iter()
        .take(end - start)
        .map(|line| line.layout_opt().map(Vec::len).unwrap_or_default())
        .sum();

    visual_lines_offset as i32 * if scroll.line < line { 1 } else { -1 }
}
