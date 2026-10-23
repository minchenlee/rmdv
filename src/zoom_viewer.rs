//! The image and diagram viewer: zoom and pan one image.
//!
//! It replaces Iced's `image::viewer`, whose wheel zoom took a fixed step
//! per event (a trackpad swipe ran to the limit) and which caught every
//! press over its whole area, so a click beside the image could not close
//! the viewer. Here:
//!
//! - A two-finger scroll (pixel deltas) pans. A mouse wheel (line deltas)
//!   zooms one fixed step per line. ⌘+scroll zooms by `exp(delta * k)`.
//!   Every zoom keeps the point under the pointer still.
//! - A drag pans. A still press and release outside the drawn image rectangle
//!   publishes the close message; a drag never closes.
//! - A double-click toggles Fit and 100% about the pointer.
//! - The app's zoom buttons and keys arrive as a [`ZoomRequest`]; a native
//!   pinch arrives as a running log of magnification, as for the mind map.
//!   Both are applied on the redraw the app's message causes.
//!
//! There is no animation, so the viewer never asks for a redraw on its own.

use crate::mindmap::{ZoomCommand, ZoomRequest};
use iced::advanced::image::Renderer as _;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, tree, Tree, Widget};
use iced::advanced::{Clipboard, Renderer as _, Shell};
use iced::mouse;
use iced::widget::image;
use iced::{keyboard, window, Element, Event, Length, Point, Rectangle, Size, Vector};

/// Room left around the image when it is fitted to the window.
const FIT_MARGIN: f32 = 24.0;
/// The smallest zoom, as a fraction of the fit (or 100%, if smaller).
const MIN_OF_FIT: f32 = 0.25;
/// The largest zoom (100% = the image's own size), unless the fit is larger.
const MAX_SCALE: f32 = 8.0;
/// Zoom per mouse-wheel line.
const LINE_ZOOM_STEP: f32 = 1.2;
/// ⌘+scroll zoom rate per pixel of trackpad delta, as for the mind map.
const PIXEL_ZOOM_RATE: f32 = 0.0025;
/// Zoom per button press or key, as for the mind map.
const STEP_ZOOM: f32 = 1.25;
/// How far a press may move and still count as a click.
const DRAG_SLOP: f32 = 3.0;
/// The largest raster detail tier the viewer asks for.
pub const MAX_DETAIL_TIER: u32 = 16;

/// What the viewer shows: the fitted image, or a zoom and pan the user set.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum View {
    #[default]
    Fit,
    /// `scale` is relative to the image's own size; `pan` is the offset of
    /// the image's center from the viewer's center.
    Manual { scale: f32, pan: Vector },
}

#[derive(Debug, Clone, Copy)]
struct Press {
    origin: Point,
    start_pan: Vector,
    moved: bool,
    /// The press began outside the drawn image.
    outside: bool,
    /// The press was a later click of a double-click (the second or a
    /// third quick one); its release never closes the viewer.
    toggled: bool,
}

/// The viewer's widget state. Tests read it through
/// [`widget::Operation::custom`].
#[derive(Debug, Clone, Default)]
pub struct State {
    pub view: View,
    content: Option<Size>,
    modifiers: keyboard::Modifiers,
    press: Option<Press>,
    last_click: Option<iced::advanced::mouse::Click>,
    request_seen: Option<u64>,
    pinch_seen: Option<f64>,
    detail_sent: Option<u32>,
}

impl State {
    /// The scale and pan drawn now in `bounds`.
    pub fn resolved(&self, content: Size, bounds: Size) -> (f32, Vector) {
        resolve(self.view, content, bounds)
    }
}

/// The scale at which `content` fits `bounds`, leaving a margin.
pub fn fit_scale(content: Size, bounds: Size) -> f32 {
    let room = Size::new(
        (bounds.width - 2.0 * FIT_MARGIN).max(1.0),
        (bounds.height - 2.0 * FIT_MARGIN).max(1.0),
    );
    let fit = (room.width / content.width).min(room.height / content.height);
    if fit.is_finite() && fit > 0.0 {
        fit
    } else {
        1.0
    }
}

/// The zoom range for an image whose fit is `fit`. Both 100% and the fit
/// are always inside it.
pub fn scale_limits(fit: f32) -> (f32, f32) {
    ((fit * MIN_OF_FIT).min(1.0), MAX_SCALE.max(fit))
}

/// Keep the image covering the viewer on each axis where it is larger, and
/// centered where it is smaller.
pub fn clamp_pan(pan: Vector, image: Size, bounds: Size) -> Vector {
    let axis = |pan: f32, image: f32, bounds: f32| {
        let room = ((image - bounds) / 2.0).max(0.0);
        if pan.is_finite() {
            pan.clamp(-room, room)
        } else {
            0.0
        }
    };
    Vector::new(
        axis(pan.x, image.width, bounds.width),
        axis(pan.y, image.height, bounds.height),
    )
}

/// The pan that keeps the image point under `anchor` (an offset from the
/// viewer's center) still when the scale goes from `from` to `to`.
pub fn zoom_about(pan: Vector, from: f32, to: f32, anchor: Vector) -> Vector {
    // The image point under the anchor sits at (anchor - pan) / from in image
    // units; after the zoom it must sit under the anchor again.
    anchor - (anchor - pan) * (to / from)
}

/// The zoom factor of one wheel event, or `None` when it pans: a mouse
/// wheel zooms a fixed step per line, and a trackpad scroll zooms in
/// proportion to its delta only while ⌘ (Ctrl elsewhere) is held.
pub fn wheel_zoom_factor(delta: mouse::ScrollDelta, command: bool) -> Option<f32> {
    let factor = match delta {
        mouse::ScrollDelta::Lines { y, .. } => LINE_ZOOM_STEP.powf(y),
        mouse::ScrollDelta::Pixels { y, .. } if command => (y * PIXEL_ZOOM_RATE).exp(),
        mouse::ScrollDelta::Pixels { .. } => return None,
    };
    (factor.is_finite() && factor > 0.0).then_some(factor)
}

/// The raster detail the viewer wants at `scale`: the smallest power of
/// two at least `scale`, from 1 to [`MAX_DETAIL_TIER`].
pub fn detail_tier(scale: f32) -> u32 {
    if !scale.is_finite() || scale <= 1.0 {
        return 1;
    }
    let mut tier = 1;
    while (tier as f32) < scale && tier < MAX_DETAIL_TIER {
        tier *= 2;
    }
    tier
}

fn resolve(view: View, content: Size, bounds: Size) -> (f32, Vector) {
    match view {
        View::Fit => (fit_scale(content, bounds), Vector::ZERO),
        View::Manual { scale, pan } => (scale, clamp_pan(pan, content * scale, bounds)),
    }
}

/// The drawn image rectangle.
pub fn image_rect(bounds: Rectangle, content: Size, scale: f32, pan: Vector) -> Rectangle {
    let size = content * scale;
    let center = bounds.center() + pan;
    Rectangle::new(
        Point::new(center.x - size.width / 2.0, center.y - size.height / 2.0),
        size,
    )
}

/// Zoom by `factor` about `anchor` (an absolute point), within the limits.
fn zoom_by(state: &mut State, content: Size, bounds: Rectangle, factor: f32, anchor: Point) {
    let (scale, _) = resolve(state.view, content, bounds.size());
    zoom_to(state, content, bounds, scale * factor, anchor);
}

fn zoom_to(state: &mut State, content: Size, bounds: Rectangle, to: f32, anchor: Point) {
    let (scale, pan) = resolve(state.view, content, bounds.size());
    let (min, max) = scale_limits(fit_scale(content, bounds.size()));
    let to = to.clamp(min, max);
    if !to.is_finite() || to <= 0.0 {
        return;
    }
    let pan = zoom_about(pan, scale, to, anchor - bounds.center());
    state.view = View::Manual {
        scale: to,
        pan: clamp_pan(pan, content * to, bounds.size()),
    };
}

fn pan_to(state: &mut State, content: Size, bounds: Rectangle, pan: Vector) {
    let (scale, _) = resolve(state.view, content, bounds.size());
    state.view = View::Manual {
        scale,
        pan: clamp_pan(pan, content * scale, bounds.size()),
    };
}

fn is_fit(state: &State, content: Size, bounds: Size) -> bool {
    match state.view {
        View::Fit => true,
        View::Manual { scale, .. } => (scale - fit_scale(content, bounds)).abs() < 1e-3,
    }
}

/// Double-click: from Fit to 100% about the pointer, otherwise to Fit.
fn toggle_fit(state: &mut State, content: Size, bounds: Rectangle, anchor: Point) {
    if is_fit(state, content, bounds.size()) {
        zoom_to(state, content, bounds, 1.0, anchor);
    } else {
        state.view = View::Fit;
    }
}

fn apply_command(state: &mut State, content: Size, bounds: Rectangle, command: ZoomCommand) {
    let center = bounds.center();
    match command {
        ZoomCommand::Fit => state.view = View::Fit,
        ZoomCommand::In => zoom_by(state, content, bounds, STEP_ZOOM, center),
        ZoomCommand::Out => zoom_by(state, content, bounds, 1.0 / STEP_ZOOM, center),
        ZoomCommand::Reset => zoom_to(state, content, bounds, 1.0, center),
    }
}

/// A zoomable, pannable image. See the module docs.
pub struct ZoomViewer<Message> {
    handle: image::Handle,
    content: Option<Size>,
    request: ZoomRequest,
    pinch_log: f64,
    on_close: Option<Message>,
    on_detail: Option<(f32, fn(u32) -> Message)>,
}

impl<Message> ZoomViewer<Message> {
    pub fn new(handle: image::Handle) -> Self {
        Self {
            handle,
            content: None,
            request: ZoomRequest::default(),
            pinch_log: 0.0,
            on_close: None,
            on_detail: None,
        }
    }

    /// The image's size at 100%, when it differs from the handle's pixel
    /// size (a sharper raster drawn at the base raster's size).
    pub fn content_size(mut self, size: Size) -> Self {
        self.content = Some(size);
        self
    }

    pub fn request(mut self, request: ZoomRequest) -> Self {
        self.request = request;
        self
    }

    /// The app's running native pinch log; the viewer zooms by the change.
    pub fn pinch_log(mut self, log: f64) -> Self {
        self.pinch_log = log;
        self
    }

    pub fn on_close(mut self, message: Message) -> Self {
        self.on_close = Some(message);
        self
    }

    /// Published when the wanted raster detail tier changes: the tier of
    /// the scale times `density` (raster pixels per logical pixel).
    pub fn on_detail(mut self, density: f32, f: fn(u32) -> Message) -> Self {
        self.on_detail = Some((density, f));
        self
    }

    fn content(&self, renderer: &iced::Renderer) -> Option<Size> {
        let size = self.content.or_else(|| {
            renderer
                .measure_image(&self.handle)
                .map(|s| Size::new(s.width as f32, s.height as f32))
        })?;
        (size.width > 0.0 && size.height > 0.0).then_some(size)
    }

    /// Apply a zoom request and a pinch that the viewer has not seen yet.
    fn sync(&self, state: &mut State, content: Size, bounds: Rectangle, cursor: mouse::Cursor) {
        if let Some(seen) = state.request_seen.replace(self.request.generation) {
            if seen != self.request.generation {
                if let Some(command) = self.request.command {
                    apply_command(state, content, bounds, command);
                }
            }
        }
        if let Some(seen) = state.pinch_seen.replace(self.pinch_log) {
            let delta = self.pinch_log - seen;
            if delta.is_finite() && delta.abs() > f64::EPSILON {
                let anchor = cursor
                    .land()
                    .position_over(bounds)
                    .unwrap_or_else(|| bounds.center());
                zoom_by(state, content, bounds, delta.exp() as f32, anchor);
            }
        }
    }
}

impl<Message: Clone> Widget<Message, iced::Theme, iced::Renderer> for ZoomViewer<Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        operation.custom(None, layout.bounds(), tree.state.downcast_mut::<State>());
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();
        if let Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) = event {
            state.modifiers = *modifiers;
            return;
        }
        let Some(content) = self.content(renderer) else {
            return;
        };
        if state.content != Some(content) {
            // Another image: start from Fit.
            state.content = Some(content);
            state.view = View::Fit;
            state.press = None;
        }
        match event {
            Event::Window(window::Event::RedrawRequested(_)) => {
                self.sync(state, content, bounds, cursor);
                if let Some((density, on_detail)) = self.on_detail {
                    let (scale, _) = state.resolved(content, bounds.size());
                    let tier = detail_tier(scale * density);
                    if state.detail_sent != Some(tier) {
                        state.detail_sent = Some(tier);
                        shell.publish(on_detail(tier));
                    }
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                // The zoom buttons float above; a scroll over them still
                // moves the image rather than the reader below.
                let Some(at) = cursor.land().position_over(bounds) else {
                    return;
                };
                shell.capture_event();
                match wheel_zoom_factor(*delta, state.modifiers.command()) {
                    Some(factor) => zoom_by(state, content, bounds, factor, at),
                    None => {
                        let mouse::ScrollDelta::Pixels { x, y } = *delta else {
                            return;
                        };
                        let (_, pan) = state.resolved(content, bounds.size());
                        pan_to(state, content, bounds, pan + Vector::new(x, y));
                    }
                }
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                // A levitating cursor is over a button above the viewer.
                let Some(at) = cursor.position_over(bounds) else {
                    return;
                };
                shell.capture_event();
                let click =
                    iced::advanced::mouse::Click::new(at, mouse::Button::Left, state.last_click);
                state.last_click = Some(click);
                let (scale, pan) = state.resolved(content, bounds.size());
                let outside = !image_rect(bounds, content, scale, pan).contains(at);
                // A double-click toggles. A third quick click (`Triple`) is
                // still part of it: the image may have just shrunk away
                // from the pointer, and that click must not close.
                let kind = click.kind();
                let toggled = kind != iced::advanced::mouse::click::Kind::Single;
                if kind == iced::advanced::mouse::click::Kind::Double {
                    toggle_fit(state, content, bounds, at);
                    shell.request_redraw();
                }
                let (_, start_pan) = state.resolved(content, bounds.size());
                state.press = Some(Press {
                    origin: at,
                    start_pan,
                    moved: false,
                    outside,
                    toggled,
                });
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let Some(press) = state.press.as_mut() else {
                    return;
                };
                let delta = *position - press.origin;
                if !press.moved && (delta.x.hypot(delta.y)) <= DRAG_SLOP {
                    return;
                }
                press.moved = true;
                let target = press.start_pan + delta;
                pan_to(state, content, bounds, target);
                shell.capture_event();
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let Some(press) = state.press.take() else {
                    return;
                };
                shell.capture_event();
                if press.moved || press.toggled || !press.outside {
                    return;
                }
                let at = cursor.land().position().unwrap_or(press.origin);
                let (scale, pan) = state.resolved(content, bounds.size());
                if !image_rect(bounds, content, scale, pan).contains(at) {
                    if let Some(message) = self.on_close.clone() {
                        shell.publish(message);
                    }
                }
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        if state.press.is_some_and(|press| press.moved) {
            return mouse::Interaction::Grabbing;
        }
        let bounds = layout.bounds();
        let Some(at) = cursor.position_over(bounds) else {
            return mouse::Interaction::None;
        };
        let Some(content) = self.content(renderer) else {
            return mouse::Interaction::Idle;
        };
        let (scale, pan) = state.resolved(content, bounds.size());
        if image_rect(bounds, content, scale, pan).contains(at) {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::Idle
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &iced::Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let Some(content) = self.content(renderer) else {
            return;
        };
        let (scale, pan) = state.resolved(content, bounds.size());
        let rect = image_rect(bounds, content, scale, pan);
        renderer.with_layer(bounds, |renderer| {
            renderer.draw_image(
                iced::advanced::image::Image {
                    handle: self.handle.clone(),
                    filter_method: image::FilterMethod::Linear,
                    rotation: iced::Radians(0.0),
                    border_radius: Default::default(),
                    opacity: 1.0,
                    snap: false,
                },
                rect,
                bounds,
            );
        });
    }
}

impl<'a, Message: Clone + 'a> From<ZoomViewer<Message>>
    for Element<'a, Message, iced::Theme, iced::Renderer>
{
    fn from(viewer: ZoomViewer<Message>) -> Self {
        Element::new(viewer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTENT: Size = Size::new(400.0, 200.0);
    const BOUNDS: Size = Size::new(1000.0, 800.0);

    #[test]
    fn fit_leaves_a_margin_and_keeps_the_aspect() {
        let fit = fit_scale(CONTENT, BOUNDS);
        // (1000 - 48) / 400 = 2.38; (800 - 48) / 200 = 3.76.
        assert!((fit - 2.38).abs() < 1e-4, "{fit}");
        let fit = fit_scale(Size::new(4000.0, 100.0), BOUNDS);
        assert!((fit - 0.238).abs() < 1e-4, "{fit}");
    }

    #[test]
    fn limits_run_from_a_quarter_of_fit_to_eight_times() {
        assert_eq!(scale_limits(0.5), (0.125, 8.0));
        // A small image still reaches 100%.
        assert_eq!(scale_limits(20.0), (1.0, 20.0));
    }

    #[test]
    fn pan_is_clamped_to_the_image_edges_and_centered_when_small() {
        let image = Size::new(1400.0, 600.0);
        let pan = clamp_pan(Vector::new(500.0, 90.0), image, BOUNDS);
        assert_eq!(pan, Vector::new(200.0, 0.0));
        let pan = clamp_pan(Vector::new(-500.0, -90.0), image, BOUNDS);
        assert_eq!(pan, Vector::new(-200.0, 0.0));
    }

    #[test]
    fn zoom_keeps_the_point_under_the_anchor_still() {
        let pan = Vector::new(30.0, -10.0);
        let anchor = Vector::new(120.0, 80.0);
        let (from, to) = (1.5, 3.0);
        let next = zoom_about(pan, from, to, anchor);
        // The image point under the anchor, in image units, before and after.
        let before = (anchor - pan) * (1.0 / from);
        let after = (anchor - next) * (1.0 / to);
        assert!((before.x - after.x).abs() < 1e-4 && (before.y - after.y).abs() < 1e-4);
    }

    #[test]
    fn trackpad_scroll_pans_and_wheel_or_command_scroll_zooms_in_proportion() {
        let pixels = |y| mouse::ScrollDelta::Pixels { x: 0.0, y };
        assert_eq!(wheel_zoom_factor(pixels(40.0), false), None);
        let small = wheel_zoom_factor(pixels(4.0), true).unwrap();
        let large = wheel_zoom_factor(pixels(40.0), true).unwrap();
        assert!((small - (4.0 * PIXEL_ZOOM_RATE).exp()).abs() < 1e-6);
        assert!((large.ln() / small.ln() - 10.0).abs() < 1e-3);
        assert!(wheel_zoom_factor(pixels(-40.0), true).unwrap() < 1.0);
        let line = wheel_zoom_factor(mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 }, false);
        assert_eq!(line, Some(LINE_ZOOM_STEP));
        let line = wheel_zoom_factor(mouse::ScrollDelta::Lines { x: 0.0, y: -2.0 }, false);
        assert!((line.unwrap() - LINE_ZOOM_STEP.powi(-2)).abs() < 1e-6);
    }

    #[test]
    fn detail_tier_is_the_next_power_of_two() {
        assert_eq!(detail_tier(0.3), 1);
        assert_eq!(detail_tier(1.0), 1);
        assert_eq!(detail_tier(1.01), 2);
        assert_eq!(detail_tier(2.5), 4);
        assert_eq!(detail_tier(8.0), 8);
        assert_eq!(detail_tier(100.0), MAX_DETAIL_TIER);
        assert_eq!(detail_tier(f32::NAN), 1);
    }

    #[test]
    fn zoom_is_clamped_and_a_double_click_toggles_fit_and_actual_size() {
        let bounds = Rectangle::new(Point::ORIGIN, BOUNDS);
        let mut state = State::default();
        zoom_by(&mut state, CONTENT, bounds, 1000.0, bounds.center());
        assert_eq!(state.resolved(CONTENT, BOUNDS).0, MAX_SCALE);
        zoom_by(&mut state, CONTENT, bounds, 1e-6, bounds.center());
        let fit = fit_scale(CONTENT, BOUNDS);
        assert!((state.resolved(CONTENT, BOUNDS).0 - fit * MIN_OF_FIT).abs() < 1e-5);

        state.view = View::Fit;
        toggle_fit(&mut state, CONTENT, bounds, Point::new(600.0, 420.0));
        assert_eq!(state.resolved(CONTENT, BOUNDS).0, 1.0);
        toggle_fit(&mut state, CONTENT, bounds, Point::new(600.0, 420.0));
        assert_eq!(state.view, View::Fit);
    }
}
