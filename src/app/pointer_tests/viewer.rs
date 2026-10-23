//! The image and diagram viewer (MDV-048 P5): zoom and pan, close by a
//! click beside the image, double-click, the zoom island, keys, and the
//! native pinch. The zoom math is tested in `crate::zoom_viewer`; these
//! tests drive the viewer through `App::view` in the headless UI.

use super::super::zoom_raster;
use super::group3::{assert_tips, tips};
use super::*;
use crate::mindmap::ZoomCommand;
use crate::zoom_viewer;
use iced::Vector;
use std::sync::Arc;

const HASH: u64 = 0x5eed;
/// The diagram's raster size; it fits the 1280×800 window at
/// min((1280 − 48) / 200, (800 − 48) / 100) = 6.16, so it is drawn
/// 1232×616 at x 24…1256, y 92…708.
const W: u32 = 200;
const H: u32 = 100;
const FIT: f32 = 6.16;

/// An app showing a ready diagram in the viewer.
fn diagram_viewer_app() -> App {
    let mut app = App::default();
    let key = (HASH, app.diagram_theme_id);
    app.diagram_cache.put(
        key,
        crate::diagram::DiagramState::Ready {
            inline: iced::widget::image::Handle::from_rgba(W, H, vec![200; (W * H * 4) as usize]),
            source_bytes: Arc::new(
                br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50"/>"#.to_vec(),
            ),
            device_w: W,
        },
    );
    let _ = app.update(Message::DiagramZoom(HASH));
    assert_eq!(app.overlay, Overlay::ImageZoom);
    app
}

struct Probe {
    found: Option<(Rectangle, zoom_viewer::State)>,
}

impl Operation for Probe {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn custom(&mut self, _id: Option<&Id>, bounds: Rectangle, state: &mut dyn std::any::Any) {
        if let Some(state) = state.downcast_ref::<zoom_viewer::State>() {
            self.found = Some((bounds, state.clone()));
        }
    }
}

/// The viewer's bounds and its scale and pan now.
fn viewer(ui: &mut PointerUi, app: &App) -> (Rectangle, f32, Vector) {
    let cache = ui.cache.take().unwrap_or_default();
    let mut user_interface = UserInterface::build(app.view(), ui.size, cache, &mut ui.renderer);
    let mut probe = Probe { found: None };
    user_interface.operate(&ui.renderer, &mut probe);
    ui.cache = Some(user_interface.into_cache());
    let (bounds, state) = probe.found.expect("the viewer is on screen");
    let (scale, pan) = state.resolved(Size::new(W as f32, H as f32), bounds.size());
    (bounds, scale, pan)
}

fn redraw(ui: &mut PointerUi, app: &App, at: Point) -> Vec<Message> {
    ui.send(
        app,
        at,
        vec![Event::Window(iced::window::Event::RedrawRequested(
            std::time::Instant::now(),
        ))],
    )
    .0
}

fn press(ui: &mut PointerUi, app: &App, at: Point) -> Vec<Message> {
    ui.send(
        app,
        at,
        vec![Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Left,
        ))],
    )
    .0
}

fn release(ui: &mut PointerUi, app: &App, at: Point) -> Vec<Message> {
    ui.send(
        app,
        at,
        vec![Event::Mouse(mouse::Event::ButtonReleased(
            mouse::Button::Left,
        ))],
    )
    .0
}

fn scroll(ui: &mut PointerUi, app: &App, at: Point, delta: mouse::ScrollDelta) -> Vec<Message> {
    ui.send(
        app,
        at,
        vec![Event::Mouse(mouse::Event::WheelScrolled { delta })],
    )
    .0
}

fn closes(messages: &[Message]) -> bool {
    has(messages, |m| matches!(m, Message::CloseOverlay))
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

#[test]
fn a_still_click_beside_the_image_closes_the_viewer() {
    let app = diagram_viewer_app();
    let mut ui = PointerUi::new();
    // Above the drawn image (it starts at y 92), far from the close button.
    let messages = ui.click(&app, Point::new(640.0, 50.0));
    assert!(closes(&messages), "{messages:?}");
}

#[test]
fn a_click_on_the_image_or_a_drag_beside_it_keeps_the_viewer_open() {
    let app = diagram_viewer_app();
    let mut ui = PointerUi::new();
    let messages = ui.click(&app, Point::new(640.0, 400.0));
    assert!(!closes(&messages), "a click on the image: {messages:?}");

    let start = Point::new(640.0, 50.0);
    let mut messages = ui.move_to(&app, start).0;
    messages.extend(press(&mut ui, &app, start));
    let end = Point::new(700.0, 60.0);
    messages.extend(ui.move_to(&app, end).0);
    messages.extend(release(&mut ui, &app, end));
    assert!(!closes(&messages), "a drag beside the image: {messages:?}");
}

#[test]
fn a_trackpad_scroll_pans_and_a_command_scroll_zooms_about_the_pointer() {
    let mut app = diagram_viewer_app();
    let mut ui = PointerUi::new();
    let at = Point::new(900.0, 500.0);
    ui.move_to(&app, at);
    let (bounds, scale, pan) = viewer(&mut ui, &app);
    assert!(near(scale, FIT), "{scale}");
    assert_eq!(pan, Vector::ZERO);

    // ⌘+scroll zooms in proportion to the delta, about the pointer.
    let command = Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
        iced::keyboard::Modifiers::COMMAND,
    ));
    ui.send(&app, at, vec![command]);
    scroll(
        &mut ui,
        &app,
        at,
        mouse::ScrollDelta::Pixels { x: 0.0, y: 40.0 },
    );
    let (_, zoomed, zoomed_pan) = viewer(&mut ui, &app);
    assert!(near(zoomed, FIT * 0.1f32.exp()), "{zoomed}");
    let anchor = at - bounds.center();
    let before = (anchor - pan) * (1.0 / scale);
    let after = (anchor - zoomed_pan) * (1.0 / zoomed);
    // The image is now wider than the window but not taller: the point
    // under the pointer stays across, and the image stays centered down.
    assert!(near(before.x, after.x), "{before:?} {after:?}");
    assert_eq!(zoomed_pan.y, 0.0);

    // Without ⌘, a trackpad scroll pans and keeps the zoom.
    let none = Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
        iced::keyboard::Modifiers::empty(),
    ));
    ui.send(&app, at, vec![none]);
    let messages = scroll(
        &mut ui,
        &app,
        at,
        mouse::ScrollDelta::Pixels { x: -30.0, y: -20.0 },
    );
    let (_, scale, pan) = viewer(&mut ui, &app);
    assert!(near(scale, zoomed), "{scale}");
    let expected = zoom_viewer::clamp_pan(
        zoomed_pan + Vector::new(-30.0, -20.0),
        Size::new(W as f32, H as f32) * scale,
        bounds.size(),
    );
    assert_ne!(pan, zoomed_pan, "the scroll pans");
    assert!(near(pan.x, expected.x) && pan.y == 0.0, "{pan:?}");
    // The scroll stays in the viewer: nothing reaches the reader.
    apply(&mut app, messages);
    assert_eq!(app.overlay, Overlay::ImageZoom);
}

#[test]
fn a_double_click_toggles_fit_and_actual_size() {
    let app = diagram_viewer_app();
    let mut ui = PointerUi::new();
    let at = Point::new(640.0, 400.0);
    let messages = ui.double_click(&app, at);
    assert!(!closes(&messages));
    let (_, scale, _) = viewer(&mut ui, &app);
    assert!(near(scale, 1.0), "{scale}");
    // Let the next pair start a fresh double-click.
    std::thread::sleep(std::time::Duration::from_millis(400));
    ui.double_click(&app, at);
    let (_, scale, _) = viewer(&mut ui, &app);
    assert!(near(scale, FIT), "{scale}");
}

#[test]
fn a_third_quick_click_after_a_double_click_keeps_the_viewer_open() {
    let app = diagram_viewer_app();
    let mut ui = PointerUi::new();
    // Near the left edge of the image at Fit (x 24…1256). The double-click
    // shrinks it to 200×100, so the third click lands beside it.
    let at = Point::new(60.0, 400.0);
    let mut messages = ui.move_to(&app, at).0;
    let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
    let release = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
    messages.extend(
        ui.send(
            &app,
            at,
            vec![
                press.clone(),
                release.clone(),
                press.clone(),
                release.clone(),
                press,
                release,
            ],
        )
        .0,
    );
    assert!(!closes(&messages), "a triple-click: {messages:?}");
    let (_, scale, _) = viewer(&mut ui, &app);
    assert!(near(scale, 1.0), "only the double-click toggles: {scale}");
}

#[test]
fn zoom_buttons_and_keys_drive_the_viewer() {
    use iced::keyboard::{Key, Modifiers};
    let key = |c: &str| Key::Character(c.into());
    let command = |m: Option<Message>| match m {
        Some(Message::ImageZoomCommand(c)) => Some(c),
        _ => None,
    };
    let none = Modifiers::empty();
    assert_eq!(
        command(image_zoom_key_message(&key("="), none)),
        Some(ZoomCommand::In)
    );
    assert_eq!(
        command(image_zoom_key_message(&key("+"), Modifiers::SHIFT)),
        Some(ZoomCommand::In)
    );
    assert_eq!(
        command(image_zoom_key_message(&key("-"), none)),
        Some(ZoomCommand::Out)
    );
    assert_eq!(
        command(image_zoom_key_message(&key("0"), none)),
        Some(ZoomCommand::Reset)
    );
    // ⌘= stays the reader's font size.
    assert!(image_zoom_key_message(&key("="), Modifiers::COMMAND).is_none());
    assert!(image_zoom_key_message(&key("x"), none).is_none());

    let mut app = diagram_viewer_app();
    let mut ui = PointerUi::new();
    assert_tips(
        &tips(&mut ui, &app),
        &[
            "Zoom to Fit",
            "Zoom Out  −",
            "Actual Size  0",
            "Zoom In  =",
            "Close  Esc",
        ],
    );
    let at = Point::new(640.0, 400.0);
    redraw(&mut ui, &app, at);
    let _ = app.update(Message::ImageZoomCommand(ZoomCommand::In));
    redraw(&mut ui, &app, at);
    let (_, scale, _) = viewer(&mut ui, &app);
    assert!(near(scale, FIT * 1.25), "{scale}");

    let actual = ui.text_bounds(&app, "100%").expect("the 100% button");
    let messages = ui.click(&app, actual.center());
    assert!(
        has(&messages, |m| matches!(
            m,
            Message::ImageZoomCommand(ZoomCommand::Reset)
        )),
        "{messages:?}"
    );
    assert!(
        !closes(&messages),
        "a button press is not a click beside the image"
    );
    apply(&mut app, messages);
    redraw(&mut ui, &app, at);
    let (_, scale, _) = viewer(&mut ui, &app);
    assert!(near(scale, 1.0), "{scale}");
}

#[test]
fn a_click_on_the_zoom_island_beside_its_buttons_keeps_the_viewer_open() {
    let app = diagram_viewer_app();
    let mut ui = PointerUi::new();
    let label = ui.text_bounds(&app, "100%").expect("the 100% button");
    // The 2 px gap left of the 100% button (its padding is 8 px), and the
    // island's bottom padding under it (the island ends 16 px above the
    // window's bottom edge). Both are below the drawn image (y 92…708).
    let gap = Point::new(label.x - 9.0, label.center_y());
    let padding = Point::new(label.center_x(), WINDOW.height - 16.0 - 2.0);
    assert!(padding.y > label.y + label.height + 4.0);
    for at in [gap, padding] {
        let messages = ui.click(&app, at);
        assert!(
            !has(&messages, |m| matches!(m, Message::ImageZoomCommand(_))),
            "{at:?} is beside the buttons: {messages:?}"
        );
        assert!(!closes(&messages), "a near miss at {at:?}: {messages:?}");
    }
}

#[test]
fn a_native_pinch_zooms_the_open_viewer() {
    let mut app = diagram_viewer_app();
    let mut ui = PointerUi::new();
    let at = Point::new(640.0, 400.0);
    ui.move_to(&app, at);
    redraw(&mut ui, &app, at);

    let _ = app.update(Message::MindmapNativePinch(0.25));
    assert_eq!(app.image_zoom_pinch_log, 0.25);
    assert_eq!(app.mindmap_native_pinch_log, 0.0);
    assert_eq!(app.full_mindmap_native_pinch_log, 0.0);
    redraw(&mut ui, &app, at);
    let (_, scale, _) = viewer(&mut ui, &app);
    assert!(near(scale, FIT * 0.25f32.exp()), "{scale}");

    // A drag pans the zoomed image and never closes the viewer.
    let (bounds, scale, pan) = viewer(&mut ui, &app);
    let mut messages = press(&mut ui, &app, at);
    let to = Point::new(600.0, 380.0);
    messages.extend(ui.move_to(&app, to).0);
    messages.extend(release(&mut ui, &app, to));
    assert!(!closes(&messages));
    let (_, _, dragged) = viewer(&mut ui, &app);
    let content = Size::new(W as f32, H as f32);
    let expected = zoom_viewer::clamp_pan(
        pan + Vector::new(-40.0, -20.0),
        content * scale,
        bounds.size(),
    );
    assert_ne!(
        dragged, pan,
        "the image is wider than the window, so it pans"
    );
    assert!(
        near(dragged.x, expected.x) && near(dragged.y, expected.y),
        "{dragged:?}"
    );

    // Closing drops the viewer; a later pinch goes nowhere.
    let _ = app.update(Message::CloseOverlay);
    let _ = app.update(Message::MindmapNativePinch(0.5));
    assert_eq!(app.image_zoom_pinch_log, 0.25);
}

// --- 4. A sharper raster for a zoomed diagram ------------------------------

fn tiny_handle(w: u32, h: u32) -> iced::widget::image::Handle {
    iced::widget::image::Handle::from_rgba(w, h, vec![0; (w * h * 4) as usize])
}

fn pending(app: &App) -> Option<zoom_raster::PendingZoomRaster> {
    app.image_zoom_raster.as_ref().and_then(|r| r.pending)
}

#[test]
fn the_viewer_reports_the_detail_it_wants_once_per_change() {
    let app = diagram_viewer_app();
    let mut ui = PointerUi::new();
    let at = Point::new(640.0, 400.0);
    let first = redraw(&mut ui, &app, at);
    // Fit is 6.16 at a density of 2: 12.3 raster pixels per base pixel.
    let tiers: Vec<u32> = first
        .iter()
        .filter_map(|m| match m {
            Message::ImageZoomDetail(t) => Some(*t),
            _ => None,
        })
        .collect();
    assert_eq!(tiers, vec![16], "{first:?}");
    // The next frame wants the same: no message, so no redraw loop.
    let again = redraw(&mut ui, &app, at);
    assert!(!has(&again, |m| matches!(m, Message::ImageZoomDetail(_))));

    // A plain image does not ask for detail.
    let mut app = long_doc_app();
    let url = "/tmp/rmdv-viewer-plain.png".to_string();
    app.image_cache
        .insert(url.clone(), ImageState::Loaded(tiny_handle(40, 30)));
    let _ = app.update(Message::OpenImageZoom(url));
    let messages = redraw(&mut PointerUi::new(), &app, at);
    assert!(!has(&messages, |m| matches!(
        m,
        Message::ImageZoomDetail(_)
    )));
}

#[test]
fn a_sharper_raster_is_debounced_identified_and_dropped_when_stale() {
    let mut app = diagram_viewer_app();
    // Tier 1 is the inline raster: nothing to make.
    let _ = app.update(Message::ImageZoomDetail(1));
    assert_eq!(pending(&app), None);

    let _ = app.update(Message::ImageZoomDetail(4));
    let first = pending(&app).expect("a request after tier 4");
    assert_eq!(first.size, (800, 400));
    assert!(!first.dispatched);
    // The same tier again keeps the running debounce.
    let _ = app.update(Message::ImageZoomDetail(4));
    assert_eq!(pending(&app), Some(first));

    let _ = app.update(Message::ImageZoomDetail(8));
    let second = pending(&app).expect("a request after tier 8");
    assert_ne!(second.id, first.id);
    assert_eq!(second.size, (1600, 800));

    // The first debounce ends late: it makes nothing.
    let _ = app.update(Message::ImageZoomRasterDue(first.id));
    assert!(!pending(&app).unwrap().dispatched);
    // The second ends: its raster is being made, once.
    let _ = app.update(Message::ImageZoomRasterDue(second.id));
    assert!(pending(&app).unwrap().dispatched);

    // A stale result is dropped; the latest one is shown.
    let _ = app.update(Message::ImageZoomRasterReady(
        first.id,
        Ok(tiny_handle(800, 400)),
    ));
    assert!(app.image_zoom_raster.as_ref().unwrap().sharp.is_none());
    let _ = app.update(Message::ImageZoomRasterReady(
        second.id,
        Ok(tiny_handle(1600, 800)),
    ));
    let raster = app.image_zoom_raster.as_ref().unwrap();
    assert_eq!(raster.sharp.as_ref().map(|(w, _)| *w), Some(1600));
    assert_eq!(raster.pending, None);

    // The sharper raster draws at the inline raster's size: the zoom keeps.
    let mut ui = PointerUi::new();
    let (_, scale, _) = viewer(&mut ui, &app);
    assert!(near(scale, FIT), "{scale}");

    // A lower tier never asks for a smaller raster.
    let _ = app.update(Message::ImageZoomDetail(4));
    assert_eq!(pending(&app), None);

    // Closing drops the raster; a late result for a new request is ignored.
    let _ = app.update(Message::ImageZoomDetail(16));
    let late = pending(&app).unwrap();
    let _ = app.update(Message::CloseOverlay);
    assert!(app.image_zoom_raster.is_none());
    let _ = app.update(Message::ImageZoomRasterDue(late.id));
    let _ = app.update(Message::ImageZoomRasterReady(
        late.id,
        Ok(tiny_handle(8, 8)),
    ));
    assert!(app.image_zoom_raster.is_none());
    assert!(app.zoom_diagram.is_none());
}

#[test]
fn leaving_the_viewer_without_close_drops_its_rasters() {
    let mut app = diagram_viewer_app();
    let _ = app.update(Message::ImageZoomDetail(16));
    let request = pending(&app).unwrap();
    let _ = app.update(Message::ImageZoomRasterDue(request.id));
    let _ = app.update(Message::ImageZoomRasterReady(
        request.id,
        Ok(tiny_handle(1600, 800)),
    ));
    assert!(app.image_zoom_raster.as_ref().unwrap().sharp.is_some());

    // Finder "open with" (or the CLI) opens a file over the viewer.
    let path = std::env::temp_dir().join(format!("rmdv-viewer-leave-{}.md", std::process::id()));
    std::fs::write(&path, "# Other\n").expect("fixture");
    let _ = app.update(Message::OpenFileFinderPath(path));
    assert_eq!(app.overlay, Overlay::None);
    assert!(app.image_zoom_raster.is_none(), "the sharp raster is freed");
    assert!(app.zoom_diagram.is_none());
    assert!(app.zoom_url.is_none());
}

#[test]
fn a_failed_raster_clears_the_request_and_keeps_the_inline_one() {
    let mut app = diagram_viewer_app();
    let _ = app.update(Message::ImageZoomDetail(2));
    let request = pending(&app).unwrap();
    let _ = app.update(Message::ImageZoomRasterDue(request.id));
    let _ = app.update(Message::ImageZoomRasterReady(
        request.id,
        Err("boom".into()),
    ));
    let raster = app.image_zoom_raster.as_ref().unwrap();
    assert_eq!(raster.pending, None);
    assert!(raster.sharp.is_none());
}
