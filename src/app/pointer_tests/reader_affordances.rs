//! Reader affordances (MDV-048 P7): copy buttons on code, math, and
//! diagram blocks show on hover and turn into a check after a copy; math
//! copies only from its button; diagrams say a click enlarges them; task
//! items draw read-only checkboxes; the reader scrollbar fades, widens on
//! hover, and has a 12 px hit area; wide tables shade the side that
//! overflows.

use super::gestures::{hscroll_rows, wide_table_app};
use super::group3::{assert_tips, tips};
use super::reader_menu::block_bounds;
use super::*;
use crate::ast::{Block, DiagramKind};
use crate::render::draw_log;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn doc_app(tag: &str, source: &str) -> App {
    let mut app = App::default();
    let _ = app.update(Message::FileLoaded(Ok((
        PathBuf::from(format!("/tmp/pointer-affordance-{tag}.md")),
        source.to_string(),
    ))));
    app
}

/// Seeds a ready raster for the document's first diagram of `kind`.
fn seed_diagram(app: &mut App, kind: DiagramKind) -> (u64, String) {
    let (hash, source) = app
        .ast
        .iter()
        .find_map(|(_, block)| match block {
            Block::Diagram {
                hash,
                source,
                kind: k,
            } if *k == kind => Some((*hash, source.clone())),
            _ => None,
        })
        .expect("the diagram block");
    let (w, h) = (240u32, 60u32);
    app.diagram_cache.put(
        (hash, app.diagram_theme_id),
        crate::diagram::DiagramState::Ready {
            inline: iced::widget::image::Handle::from_rgba(w, h, vec![90; (w * h * 4) as usize]),
            source_bytes: Arc::new(Vec::new()),
            device_w: w,
        },
    );
    (hash, source)
}

fn glyph(code: char) -> String {
    code.to_string()
}

/// Moves the pointer to `at` and draws the view there.
fn draw_at(ui: &mut PointerUi, app: &App, at: Point) {
    let _ = ui.move_to(app, at);
    let cache = ui.cache.take().unwrap_or_default();
    let mut user_interface = UserInterface::build(app.view(), ui.size, cache, &mut ui.renderer);
    user_interface.draw(
        &mut ui.renderer,
        &iced::Theme::Light,
        &iced::advanced::renderer::Style {
            text_color: iced::Color::BLACK,
        },
        mouse::Cursor::Available(at),
    );
    ui.cache = Some(user_interface.into_cache());
}

/// The redraw the event loop schedules after the pointer moves to `to`.
fn redraw_after_move(ui: &mut PointerUi, app: &App, to: Point) -> iced::window::RedrawRequest {
    let cache = ui.cache.take().unwrap_or_default();
    let mut user_interface = UserInterface::build(app.view(), ui.size, cache, &mut ui.renderer);
    let mut messages = Vec::new();
    let (state, _) = user_interface.update(
        &[Event::Mouse(mouse::Event::CursorMoved { position: to })],
        mouse::Cursor::Available(to),
        &mut ui.renderer,
        &mut iced::advanced::clipboard::Null,
        &mut messages,
    );
    ui.cache = Some(user_interface.into_cache());
    match state {
        user_interface::State::Updated { redraw_request, .. } => redraw_request,
        user_interface::State::Outdated => iced::window::RedrawRequest::NextFrame,
    }
}

/// Bounds of every widget that reports custom state (the reader's rich
/// text reports its selection state this way, not its text).
fn inner_widgets(ui: &mut PointerUi, app: &App) -> Vec<Rectangle> {
    struct Customs(Vec<Rectangle>);
    impl Operation for Customs {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
        fn custom(&mut self, _id: Option<&Id>, bounds: Rectangle, _state: &mut dyn std::any::Any) {
            self.0.push(bounds);
        }
    }
    let cache = ui.cache.take().unwrap_or_default();
    let mut user_interface = UserInterface::build(app.view(), ui.size, cache, &mut ui.renderer);
    let mut customs = Customs(Vec::new());
    user_interface.operate(&ui.renderer, &mut customs);
    ui.cache = Some(user_interface.into_cache());
    customs.0
}

fn copied(messages: &[Message]) -> Option<String> {
    messages.iter().find_map(|m| match m {
        Message::CopyBlockSource(text, _) => Some(text.clone()),
        _ => None,
    })
}

const CODE_DOC: &str = "Intro text above the block.\n\n```rust\nfn main() {}\n```\n\nOutro text.\n";

fn code_block(ui: &mut PointerUi, app: &App) -> Rectangle {
    block_bounds(ui, app, |b| matches!(b, Block::CodeBlock { .. }))
}

// --- Copy buttons -------------------------------------------------------------

#[test]
fn the_code_copy_button_shows_only_while_the_pointer_is_over_the_block() {
    let app = doc_app("code-hover", CODE_DOC);
    let mut ui = PointerUi::new();
    let code = code_block(&mut ui, &app);
    let outside = Point::new(code.center_x(), code.y - 30.0);
    let inside = Point::new(code.x + 40.0, code.center_y());

    let _ = draw_log::take_reveals();
    draw_at(&mut ui, &app, outside);
    assert_eq!(draw_log::take_reveals(), vec![false], "hidden away from it");
    draw_at(&mut ui, &app, inside);
    assert_eq!(draw_log::take_reveals(), vec![true], "shown over the block");

    // Entering the block redraws, so the button appears without a view
    // rebuild; moving inside it schedules nothing more.
    let _ = ui.move_to(&app, outside);
    assert_eq!(
        redraw_after_move(&mut ui, &app, inside),
        iced::window::RedrawRequest::NextFrame
    );
    let _ = redraw_after_move(&mut ui, &app, Point::new(inside.x + 5.0, inside.y));
    let still = redraw_after_move(&mut ui, &app, Point::new(inside.x + 10.0, inside.y));
    assert_eq!(still, iced::window::RedrawRequest::Wait);
}

#[test]
fn a_copy_turns_the_icon_into_a_check_until_its_timer_ends() {
    let mut app = doc_app("code-check", CODE_DOC);
    let mut ui = PointerUi::new();
    let copy = ui
        .text_bounds(&app, &glyph(crate::icon::ic::COPY))
        .expect("copy icon");
    let messages = ui.click(&app, center(copy));
    let text = copied(&messages).expect("the button copies the block's source");
    assert!(text.contains("fn main() {}"), "{text:?}");

    apply(&mut app, messages);
    assert!(
        ui.text_bounds(&app, &glyph(crate::icon::ic::CHECK))
            .is_some(),
        "a check replaces the copy icon"
    );
    assert!(ui
        .text_bounds(&app, &glyph(crate::icon::ic::COPY))
        .is_none());
    assert!(
        app.toast.is_none(),
        "the check is the feedback, not a toast"
    );

    // An older copy's timer leaves this check alone; its own clears it.
    let seq = app.copy_flash.as_ref().map(|(seq, _)| *seq).expect("flash");
    let _ = app.update(Message::CopyFlashEnd(seq.wrapping_sub(1)));
    assert!(ui
        .text_bounds(&app, &glyph(crate::icon::ic::CHECK))
        .is_some());
    let _ = app.update(Message::CopyFlashEnd(seq));
    assert!(ui
        .text_bounds(&app, &glyph(crate::icon::ic::COPY))
        .is_some());
    assert!(app.copy_flash.is_none());
}

#[test]
fn the_copy_button_leaves_the_first_code_line_clear() {
    let words = "alpha beta gamma delta epsilon zeta eta theta iota kappa ".repeat(6);
    let app = doc_app("code-clear", &format!("```text\n{words}\n```\n"));
    let mut ui = PointerUi::new();
    let copy = ui
        .text_bounds(&app, &glyph(crate::icon::ic::COPY))
        .expect("copy icon");
    let block = code_block(&mut ui, &app);
    let text = inner_widgets(&mut ui, &app)
        .into_iter()
        .filter(|r| r.x >= block.x + 4.0 && block.contains(r.center()))
        .max_by(|a, b| a.width.total_cmp(&b.width))
        .expect("the code text inside the block");
    assert!(
        text.x + text.width <= copy.x - 4.0,
        "the code text {text:?} stays left of the button {copy:?}"
    );
}

#[test]
fn math_copies_only_from_its_button() {
    let mut app = doc_app("math", "Intro\n\n$$\nx^2 + y^2\n$$\n\nOutro\n");
    let (_, source) = seed_diagram(&mut app, DiagramKind::Math);
    let mut ui = PointerUi::new();
    assert_tips(&tips(&mut ui, &app), &["Copy Math"]);

    let math = block_bounds(&mut ui, &app, |b| matches!(b, Block::Diagram { .. }));
    let messages = ui.click(&app, center(math));
    assert!(
        !has(&messages, |m| matches!(
            m,
            Message::CopyCode(_) | Message::CopyBlockSource(..)
        )),
        "a click on the formula copies nothing: {messages:?}"
    );

    let copy = ui
        .text_bounds(&app, &glyph(crate::icon::ic::COPY))
        .expect("math copy button");
    assert!(math.contains(center(copy)), "{copy:?} sits on {math:?}");
    let messages = ui.click(&app, center(copy));
    assert_eq!(copied(&messages).as_deref(), Some(source.as_str()));
}

#[test]
fn diagrams_say_a_click_enlarges_them() {
    let mut app = doc_app("diagram", "Intro\n\n```mermaid\ngraph TD; A-->B\n```\n");
    let (hash, source) = seed_diagram(&mut app, DiagramKind::Mermaid);
    let mut ui = PointerUi::new();
    assert_tips(&tips(&mut ui, &app), &["Click to enlarge", "Copy Source"]);

    let diagram = block_bounds(&mut ui, &app, |b| matches!(b, Block::Diagram { .. }));
    let messages = ui.click(&app, Point::new(diagram.x + 30.0, diagram.center_y()));
    assert!(has(
        &messages,
        |m| matches!(m, Message::DiagramZoom(h) if *h == hash)
    ));

    let copy = ui
        .text_bounds(&app, &glyph(crate::icon::ic::COPY))
        .expect("diagram copy button");
    let messages = ui.click(&app, center(copy));
    assert_eq!(copied(&messages).as_deref(), Some(source.as_str()));
}

// --- Task checkboxes ---------------------------------------------------------

#[test]
fn task_items_draw_read_only_checkboxes() {
    let app = doc_app("tasks", "- [x] finished item\n- [ ] open item\n");
    let mut ui = PointerUi::new();
    assert!(ui.text_bounds(&app, "✓").is_none(), "no text check mark");
    assert!(ui.text_bounds(&app, "○").is_none(), "no text circle");
    assert!(
        ui.text_bounds(&app, &glyph(crate::icon::ic::CHECK))
            .is_some(),
        "the done item's box holds a check"
    );
    assert_tips(&tips(&mut ui, &app), &["Read-only"]);

    // The box sits at the left of the list's 28 px marker column, on the
    // first item's line.
    let list = block_bounds(&mut ui, &app, |b| matches!(b, Block::List { .. }));
    let checkbox = Point::new(list.x + 7.0, list.y + 12.0);
    let (_, cursor) = ui.move_to(&app, checkbox);
    assert_eq!(cursor, mouse::Interaction::NotAllowed);
    let messages = ui.click(&app, checkbox);
    assert!(
        !has(&messages, |m| matches!(m, Message::SaveFile)),
        "{messages:?}"
    );
}

// --- The reader scrollbar -------------------------------------------------------

#[test]
fn the_scrollbar_fades_in_holds_and_fades_out_then_stops_ticking() {
    let t0 = Instant::now();
    let ms = Duration::from_millis;
    let hold = ms(SCROLLER_FADE_MS);
    let mut fade = ScrollerFade::default();
    assert_eq!(fade.alpha(t0), 0.0);
    assert_eq!(fade.next_tick(t0), None, "a hidden bar needs no tick");

    fade.poke(t0);
    assert!(fade.alpha(t0) < 0.05);
    assert!((fade.alpha(t0 + ms(75)) - 0.5).abs() < 0.05);
    assert_eq!(fade.alpha(t0 + SCROLLER_FADE), 1.0);
    assert_eq!(fade.next_tick(t0 + ms(10)), Some(SCROLLER_FRAME));

    // Scrolling on keeps it shown; then it waits, then fades out.
    let last = t0 + ms(500);
    fade.poke(last);
    assert_eq!(fade.alpha(last), 1.0);
    assert_eq!(fade.next_tick(last + ms(200)), Some(SCROLLER_HOLD_TICK));
    assert_eq!(fade.alpha(last + hold), 1.0);
    assert!((fade.alpha(last + hold + ms(75)) - 0.5).abs() < 0.05);
    assert_eq!(fade.next_tick(last + hold + ms(75)), Some(SCROLLER_FRAME));
    let gone = last + hold + SCROLLER_FADE;
    assert_eq!(fade.alpha(gone), 0.0);
    fade.tick(gone);
    assert_eq!(fade.next_tick(gone), None, "done: no more ticks");

    // A scroll during the fade-out continues from the alpha it had.
    let mut fade = ScrollerFade::default();
    fade.poke(t0);
    let mid = t0 + hold + ms(75);
    let before = fade.alpha(mid);
    fade.poke(mid);
    assert!((fade.alpha(mid) - before).abs() < 0.05, "no jump");

    // While the pointer rests on the strip the bar stays without ticks.
    let mut fade = ScrollerFade::default();
    fade.set_hovered(true, t0);
    let later = t0 + ms(5000);
    assert_eq!(fade.alpha(later), 1.0);
    assert_eq!(fade.next_tick(later), None);
    fade.set_hovered(false, later);
    assert_eq!(fade.alpha(later + hold), 1.0);
    assert_eq!(fade.alpha(later + hold + SCROLLER_FADE), 0.0);
}

#[test]
fn an_idle_reader_runs_no_scroller_tick() {
    let mut app = long_doc_app();
    app.last_scroll_at = None;
    assert_eq!(app.scroller_tick_interval(Instant::now()), None);
    let now = Instant::now();
    app.reader_scroller.poke(now);
    assert_eq!(app.scroller_tick_interval(now), Some(SCROLLER_FRAME));
}

#[test]
fn the_reader_scrollbar_takes_presses_12px_from_the_edge() {
    let app = long_doc_app();
    let mut ui = PointerUi::new();
    let track = Point::new(WINDOW.width - 11.5, WINDOW.height - 120.0);
    let messages = ui.click(&app, track);
    assert!(
        has(&messages, |m| matches!(m, Message::BodyScrolled(_))),
        "a track press 11 px from the edge scrolls: {messages:?}"
    );
}

#[test]
fn the_reader_scrollbar_strip_reports_hover() {
    let mut app = long_doc_app();
    let mut ui = PointerUi::new();
    let _ = ui.move_to(&app, Point::new(640.0, 400.0));
    let messages = ui.move_to(&app, Point::new(WINDOW.width - 4.0, 400.0)).0;
    assert!(
        has(&messages, |m| matches!(
            m,
            Message::ReaderScrollbarHover(true)
        )),
        "{messages:?}"
    );
    apply(&mut app, messages);
    assert!(app.reader_scroller.hovered);
    let messages = ui.move_to(&app, Point::new(640.0, 400.0)).0;
    assert!(
        has(&messages, |m| matches!(
            m,
            Message::ReaderScrollbarHover(false)
        )),
        "{messages:?}"
    );
    apply(&mut app, messages);
    assert!(!app.reader_scroller.hovered);
}

// --- Wide tables ------------------------------------------------------------------

#[test]
fn a_wide_table_shades_the_side_that_overflows() {
    let app = wide_table_app();
    let mut ui = PointerUi::new();
    let column = ui.id_bounds(&app, "reader-column").expect("reader column");
    let rows = hscroll_rows(&mut ui, &app, column.center_x());
    let over = Point::new(column.center_x(), rows[rows.len() / 2]);

    let _ = draw_log::take_edges();
    draw_at(&mut ui, &app, over);
    assert_eq!(
        draw_log::take_edges(),
        vec![(false, true)],
        "more on the right"
    );

    let sideways = |x: f32| {
        vec![Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Pixels { x, y: 0.0 },
        })]
    };
    let _ = ui.send(&app, over, sideways(-200.0));
    draw_at(&mut ui, &app, over);
    assert_eq!(draw_log::take_edges(), vec![(true, true)], "both sides");

    let _ = ui.send(&app, over, sideways(-20_000.0));
    draw_at(&mut ui, &app, over);
    assert_eq!(draw_log::take_edges(), vec![(true, false)], "at the end");
}

#[test]
fn the_overflow_shade_shows_over_the_table() {
    // The shade was drawn under the scrolled table's own layer, so its
    // opaque fill hid it. Read the pixels: beside the right edge each one
    // is darker than the same row's unshaded left edge.
    use iced::advanced::renderer::Headless;
    let app = wide_table_app();
    let mut ui = PointerUi::new();
    let mut probe = HScrollProbe::new();
    ui.operate(&app, &mut probe);
    let table = *probe.rects().first().expect("the wide table");
    let _ = draw_log::take_edges();
    draw_at(&mut ui, &app, Point::new(table.center_x(), table.y - 30.0));
    assert_eq!(draw_log::take_edges(), vec![(false, true)]);
    let (w, h) = (ui.size.width as u32, ui.size.height as u32);
    let pixels = ui
        .renderer
        .screenshot(Size::new(w, h), 1.0, iced::Color::WHITE);
    let at = |x: f32, y: f32| {
        let i = ((y as u32 * w + x as u32) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    let (left, right) = (table.x + 3.0, table.x + table.width - 3.0);
    let rows: Vec<f32> = (0..table.height as usize)
        .map(|dy| table.y + dy as f32 + 0.5)
        .filter(|y| *y < h as f32)
        .collect();
    assert!(rows.len() > 40, "{table:?}");
    let same = rows
        .iter()
        .filter(|y| at(left, **y) == at(right, **y))
        .count();
    assert!(
        same * 5 < rows.len(),
        "{same} of {} rows look the same at both edges: no shade shows",
        rows.len()
    );
}

/// How many widgets show exactly `text`.
fn count_text(ui: &mut PointerUi, app: &App, text: &str) -> usize {
    struct Count<'a>(&'a str, usize);
    impl Operation for Count<'_> {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
        fn text(&mut self, _id: Option<&Id>, _bounds: Rectangle, text: &str) {
            if text == self.0 {
                self.1 += 1;
            }
        }
    }
    let mut count = Count(text, 0);
    ui.operate(app, &mut count);
    count.1
}

#[test]
fn a_copy_checks_only_the_block_it_came_from() {
    let mut app = doc_app(
        "code-twins",
        "```sh\nnpm install\n```\n\nBetween the two.\n\n```sh\nnpm install\n```\n",
    );
    let mut ui = PointerUi::new();
    let (copy, check) = (glyph(crate::icon::ic::COPY), glyph(crate::icon::ic::CHECK));
    assert_eq!(count_text(&mut ui, &app, &copy), 2);
    let button = ui.text_bounds(&app, &copy).expect("copy icon");
    let messages = ui.click(&app, center(button));
    assert!(copied(&messages).is_some(), "{messages:?}");
    apply(&mut app, messages);
    assert_eq!(
        (
            count_text(&mut ui, &app, &check),
            count_text(&mut ui, &app, &copy)
        ),
        (1, 1),
        "only the copied block shows the check"
    );
}
