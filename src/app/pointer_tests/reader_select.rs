//! Reader selection with the pointer (MDV-048 P7): a selection starts in
//! the margins and between blocks, a drag past the viewport edge scrolls,
//! double-click selects Chinese and Japanese words, and word, line, and
//! Shift+click selections reach across blocks. The selected text is read
//! back through the reader's right-press hit, which changes nothing.

use super::reader_menu::{block_bounds, drag, fixture, hit, paragraph};
use super::tree_menu::right_press;
use super::*;
use crate::ast::Block;

/// The text the reader reports as selected, read with a right press at
/// `probe` (a spot that does not change the selection).
fn selected(app: &mut App, ui: &mut PointerUi, probe: Point) -> Option<String> {
    let messages = right_press(app, ui, probe);
    let _ = app.update(Message::ContextMenuClose);
    hit(&messages).selection
}

fn heading(ui: &mut PointerUi, app: &App) -> Rectangle {
    block_bounds(ui, app, |b| matches!(b, Block::Heading { .. }))
}

#[test]
fn a_press_in_the_margin_starts_a_selection_at_the_nearest_text() {
    let (mut app, dir) = fixture("margin");
    let mut ui = PointerUi::new();
    let text = paragraph(&mut ui, &app);
    let y = text.y + 8.0;
    // The left margin beside the reading column, level with the paragraph.
    let margin = Point::new(24.0, y);
    assert!(margin.x < text.x - 40.0, "{margin:?} is left of {text:?}");
    drag(&mut app, &mut ui, margin, Point::new(text.x + 80.0, y));

    let got = selected(&mut app, &mut ui, Point::new(text.x + 30.0, y));
    let got = got.expect("the drag from the margin selected text");
    assert!(got.starts_with("Some"), "{got:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_press_between_blocks_starts_a_selection() {
    let (mut app, dir) = fixture("between");
    let mut ui = PointerUi::new();
    let head = heading(&mut ui, &app);
    let text = paragraph(&mut ui, &app);
    assert!(
        text.y > head.y + head.height,
        "a gap separates {head:?} and {text:?}"
    );
    let gap = Point::new(text.x + 40.0, (head.y + head.height + text.y) / 2.0);
    drag(
        &mut app,
        &mut ui,
        gap,
        Point::new(text.x + 80.0, text.y + 8.0),
    );

    let got = selected(&mut app, &mut ui, Point::new(text.x + 30.0, text.y + 8.0));
    let got = got.expect("the drag from between the blocks selected text");
    assert!(got.contains("Some"), "{got:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_margin_and_the_gaps_between_blocks_show_the_text_cursor() {
    let (app, dir) = fixture("ibeam");
    let mut ui = PointerUi::new();
    let head = heading(&mut ui, &app);
    let text = paragraph(&mut ui, &app);
    for at in [
        Point::new(24.0, text.y + 8.0),
        Point::new(text.x + 40.0, (head.y + head.height + text.y) / 2.0),
    ] {
        let (_, interaction) = ui.move_to(&app, at);
        assert_eq!(interaction, mouse::Interaction::Text, "{at:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_press_on_an_image_keeps_its_own_action_and_selects_nothing() {
    let (mut app, dir) = fixture("image-press");
    let mut ui = PointerUi::new();
    let image = block_bounds(&mut ui, &app, |b| matches!(b, Block::Image { .. }));
    let text = paragraph(&mut ui, &app);
    let at = center(image);
    let _ = ui.move_to(&app, at);
    let pressed = ui
        .send(
            &app,
            at,
            vec![Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Left,
            ))],
        )
        .0;
    assert!(
        has(&pressed, |m| matches!(m, Message::OpenImageZoom(_))),
        "{pressed:?}"
    );
    let mut messages = ui.move_to(&app, Point::new(text.x + 80.0, text.y + 8.0)).0;
    messages.extend(
        ui.send(
            &app,
            Point::new(text.x + 80.0, text.y + 8.0),
            vec![Event::Mouse(mouse::Event::ButtonReleased(
                mouse::Button::Left,
            ))],
        )
        .0,
    );
    // The zoom is not applied: the reader stays in front for the probe.
    apply(&mut app, messages);
    assert_eq!(
        selected(&mut app, &mut ui, Point::new(text.x + 30.0, text.y + 8.0)),
        None
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// --- Drag-select auto-scroll ---------------------------------------------------

/// The bounds of "Paragraph `n`" in `long_doc_app`, when it is mounted.
fn long_paragraph(ui: &mut PointerUi, app: &App, n: usize) -> Option<Rectangle> {
    let prefix = format!("Paragraph {n} ");
    let id = app
        .ast
        .iter()
        .find(|(_, b)| crate::plain_text::block_text(b).is_some_and(|t| t.starts_with(&prefix)))
        .map(|(id, _)| *id)?;
    ui.find(
        app,
        Find {
            text: None,
            contains: None,
            id: Some(crate::render::block_anchor_id(id)),
            found: None,
            found_text: None,
        },
    )
    .map(|(bounds, _)| bounds)
}

fn press(ui: &mut PointerUi, app: &App, at: Point) -> Vec<Message> {
    let mut out = ui.move_to(app, at).0;
    out.extend(
        ui.send(
            app,
            at,
            vec![Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Left,
            ))],
        )
        .0,
    );
    out
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

fn autoscroll(messages: &[Message]) -> Vec<Option<f32>> {
    messages
        .iter()
        .filter_map(|m| match m {
            Message::ReaderAutoScroll(speed) => Some(*speed),
            _ => None,
        })
        .collect()
}

/// The number of the last whole "Paragraph N with" in `text`; the end of
/// a selection can cut a number short.
fn last_paragraph(text: &str) -> usize {
    text.match_indices("Paragraph ")
        .filter_map(|(i, _)| {
            let rest = &text[i + "Paragraph ".len()..];
            let (n, after) = rest.split_once(' ')?;
            after.starts_with("with").then(|| n.parse().ok()).flatten()
        })
        .last()
        .unwrap_or_else(|| panic!("no paragraph in {text:?}"))
}

#[test]
fn a_drag_past_the_viewport_edge_starts_the_scroll_and_coming_back_stops_it() {
    let app = long_doc_app();
    let mut ui = PointerUi::new();
    let text = long_paragraph(&mut ui, &app, 2).expect("paragraph 2 is on screen");
    let start = Point::new(text.x + 1.0, text.y + 8.0);
    let inside = Point::new(text.x + 80.0, 400.0);
    let below = Point::new(text.x + 80.0, WINDOW.height + 60.0);
    let above = Point::new(text.x + 80.0, -60.0);

    let _ = press(&mut ui, &app, start);
    let past = autoscroll(&ui.move_to(&app, below).0);
    assert!(
        matches!(past.as_slice(), [Some(dy)] if *dy > 0.0),
        "below the bottom edge the reader scrolls down: {past:?}"
    );
    // Farther past the edge scrolls faster.
    let farther = autoscroll(&ui.move_to(&app, Point::new(below.x, below.y + 120.0)).0);
    assert!(
        matches!((past[0], farther.as_slice()), (Some(a), [Some(b)]) if *b > a),
        "{past:?} then {farther:?}"
    );
    let back = autoscroll(&ui.move_to(&app, inside).0);
    assert_eq!(back, [None], "back inside, the scroll stops");
    let up = autoscroll(&ui.move_to(&app, above).0);
    assert!(
        matches!(up.as_slice(), [Some(dy)] if *dy < 0.0),
        "above the top edge the reader scrolls up: {up:?}"
    );
    assert_eq!(
        autoscroll(&release(&mut ui, &app, above)),
        [None],
        "releasing the button stops the scroll"
    );
    // A move without the button held starts nothing.
    assert!(autoscroll(&ui.move_to(&app, below).0).is_empty());
}

#[test]
fn the_scroll_tick_runs_only_while_a_drag_is_past_the_edge() {
    fn recipes(app: &App) -> usize {
        iced::advanced::subscription::into_recipes(app.subscription()).len()
    }
    let mut app = long_doc_app();
    let idle = recipes(&app);
    let _ = app.update(Message::ReaderAutoScroll(Some(12.0)));
    assert_eq!(app.reader_autoscroll, Some(12.0));
    assert!(
        recipes(&app) > idle,
        "the tick runs while the drag is past the edge"
    );
    let _ = app.update(Message::ReaderAutoScroll(None));
    assert_eq!(app.reader_autoscroll, None);
    assert_eq!(recipes(&app), idle, "no tick once the drag ends");

    // Only the reader scrolls this way: a mind map panel's text does not
    // move the document behind it.
    app.view_mode = ViewMode::Mindmap;
    let _ = app.update(Message::ReaderAutoScroll(Some(12.0)));
    assert_eq!(app.reader_autoscroll, None);
}

/// Like `long_doc_app`, with enough blocks that the reader mounts only a
/// window of them.
fn virtual_doc_app() -> App {
    let mut source = String::new();
    for i in 0..(crate::virt::VIRT_MIN_BLOCKS * 2) {
        source.push_str(&format!(
            "Paragraph {i} with enough words to fill a reader line here.\n\n"
        ));
    }
    let mut app = App::default();
    let _ = app.update(Message::FileLoaded(Ok((
        std::path::PathBuf::from("/tmp/pointer-virtual.md"),
        source,
    ))));
    app
}

#[test]
fn the_selection_follows_the_auto_scroll_across_window_rebuilds() {
    let mut app = virtual_doc_app();
    let mut ui = PointerUi::new();
    let text = long_paragraph(&mut ui, &app, 2).expect("paragraph 2 is on screen");
    // The last paragraph that starts on screen before the scroll.
    let on_screen = (2..60)
        .take_while(|&n| long_paragraph(&mut ui, &app, n).is_some_and(|b| b.y < WINDOW.height))
        .last()
        .expect("paragraphs fill the screen");
    let start = Point::new(text.x + 1.0, text.y + 8.0);
    let below = Point::new(text.x + 80.0, WINDOW.height + 200.0);

    let mut messages = press(&mut ui, &app, start);
    messages.extend(ui.move_to(&app, below).0);
    group2::apply_ui(&mut app, &mut ui, messages);
    assert!(app.reader_autoscroll.is_some_and(|dy| dy > 0.0));

    for _ in 0..40 {
        group2::apply_ui(&mut app, &mut ui, vec![Message::ReaderAutoScrollTick]);
        let redraw = Event::Window(iced::window::Event::RedrawRequested(
            std::time::Instant::now(),
        ));
        let messages = ui.send(&app, below, vec![redraw]).0;
        group2::apply_ui(&mut app, &mut ui, messages);
    }
    assert!(
        long_paragraph(&mut ui, &app, 2).is_none(),
        "the scroll moved paragraph 2 out of the mounted window"
    );
    let messages = release(&mut ui, &app, below);
    group2::apply_ui(&mut app, &mut ui, messages);
    assert_eq!(app.reader_autoscroll, None);

    let got = selected(&mut app, &mut ui, Point::new(text.x + 30.0, 400.0))
        .expect("the drag selected text");
    assert!(
        got.starts_with("Paragraph 2 "),
        "{:?}",
        &got[..80.min(got.len())]
    );
    let reached = last_paragraph(&got);
    assert!(
        reached > on_screen + 10,
        "the selection end followed the scroll: {reached} vs {on_screen} on screen before"
    );
}

// --- Chinese and Japanese words ------------------------------------------------

const MIXED: &str = "中文分詞測試 with English";

/// A document whose second block is `MIXED`.
fn cjk_app() -> App {
    let mut app = App::default();
    let _ = app.update(Message::FileLoaded(Ok((
        std::path::PathBuf::from("/tmp/pointer-cjk.md"),
        format!("# Words\n\n{MIXED}\n\nAfter the words.\n"),
    ))));
    app
}

fn mixed_paragraph(ui: &mut PointerUi, app: &App) -> Rectangle {
    block_bounds(ui, app, |b| {
        crate::plain_text::block_text(b).is_some_and(|t| t.starts_with("中文"))
    })
}

/// Double-clicks the `n`th character of `MIXED` (each Han character is one
/// em wide) and returns the word the reader selected.
fn double_click_char(app: &mut App, ui: &mut PointerUi, n: usize) -> Option<String> {
    let text = mixed_paragraph(ui, app);
    let em = app.typography.body_size;
    let at = Point::new(text.x + (n as f32 + 0.3) * em, text.y + em * 0.6);
    let messages = ui.double_click(app, at);
    apply(app, messages);
    let after = block_bounds(ui, app, |b| {
        crate::plain_text::block_text(b).is_some_and(|t| t.starts_with("After"))
    });
    selected(app, ui, Point::new(after.x + 4.0, after.y + 8.0))
}

#[test]
fn double_click_on_chinese_selects_a_dictionary_word() {
    let mut app = cjk_app();
    let mut ui = PointerUi::new();
    if !cfg!(target_os = "macos") {
        // UAX #29 elsewhere: one ideograph per word. A CI machine may have no
        // CJK font, so a Han glyph is not one em wide: scan the line instead.
        let text = mixed_paragraph(&mut ui, &app);
        let em = app.typography.body_size;
        let mut han = Vec::new();
        for step in 0..30 {
            let at = Point::new(text.x + 1.0 + step as f32 * 4.0, text.y + em * 0.6);
            let messages = ui.double_click(&app, at);
            apply(&mut app, messages);
            let after = Point::new(text.x + 4.0, text.y + text.height + 30.0);
            if let Some(word) = selected(&mut app, &mut ui, after) {
                if word.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)) {
                    han.push(word);
                }
            }
        }
        assert!(han.iter().any(|word| word == "中"), "{han:?}");
        assert!(
            han.iter().all(|word| word.chars().count() == 1),
            "one ideograph per word: {han:?}"
        );
        return;
    }
    let first = double_click_char(&mut app, &mut ui, 0);
    let fifth = double_click_char(&mut app, &mut ui, 4);
    // The right half of a character is still that character's word.
    let text = mixed_paragraph(&mut ui, &app);
    let em = app.typography.body_size;
    let at = Point::new(text.x + 1.8 * em, text.y + em * 0.6);
    let messages = ui.double_click(&app, at);
    apply(&mut app, messages);
    let after = Point::new(text.x + 4.0, text.y + text.height + 30.0);
    let second_right_half = selected(&mut app, &mut ui, after);
    assert_eq!(first.as_deref(), Some("中文"));
    assert_eq!(fifth.as_deref(), Some("測試"));
    assert_eq!(second_right_half.as_deref(), Some("中文"));
}

#[test]
fn double_click_on_english_beside_chinese_selects_the_english_word() {
    let mut app = cjk_app();
    let mut ui = PointerUi::new();
    let text = mixed_paragraph(&mut ui, &app);
    let em = app.typography.body_size;
    // Past "中文分詞測試 with " (six ems and a few Latin letters).
    let mut found = None;
    for step in 0..25 {
        let at = Point::new(
            text.x + 6.0 * em + 2.0 + step as f32 * 8.0,
            text.y + em * 0.6,
        );
        let messages = ui.double_click(&app, at);
        apply(&mut app, messages);
        let after = Point::new(text.x + 4.0, text.y + text.height + 30.0);
        if let Some(word) = selected(&mut app, &mut ui, after) {
            if word.chars().any(|c| c.is_ascii_uppercase()) {
                found = Some(word);
                break;
            }
            assert!(
                ["with", " "].contains(&word.as_str()),
                "only Latin words beside the Chinese: {word:?}"
            );
        }
    }
    assert_eq!(found.as_deref(), Some("English"));
}

#[test]
fn word_bounds_split_chinese_by_dictionary_and_latin_by_uax29() {
    use iced_selection::word::{next_end_of_word, previous_start_of_word, word_around};
    let word = |g: usize| {
        let (start, end) = word_around(MIXED, g);
        MIXED
            .chars()
            .skip(start)
            .take(end - start)
            .collect::<String>()
    };
    // "中文分詞測試 with English": graphemes 0-5 are Han, 7-10 "with".
    assert_eq!(word(8), "with");
    assert_eq!(word(6), " ");
    assert_eq!(word(MIXED.chars().count()), "English", "past the end");
    // Katakana stays one word on every platform.
    assert_eq!(word_around("カタカナ語", 1).0, 0);
    if cfg!(target_os = "macos") {
        // The system dictionary decides: it joins 中文 and 測試 (it may
        // keep 分 and 詞 apart).
        assert_eq!(
            (word(0), word(1), word(5)),
            ("中文".into(), "中文".into(), "測試".into())
        );
        assert_eq!(next_end_of_word(MIXED, 0), 2);
        assert_eq!(next_end_of_word(MIXED, 4), 6);
        assert_eq!(previous_start_of_word(MIXED, 7), 4, "spaces are skipped");
        assert_eq!(previous_start_of_word(MIXED, 5), 4);
    } else {
        assert_eq!((word(0), word(1)), ("中".into(), "文".into()));
        assert_eq!(next_end_of_word(MIXED, 0), 1);
    }
    assert_eq!(next_end_of_word(MIXED, 6), 11, "the next word is \"with\"");
    assert_eq!(previous_start_of_word("", 0), 0);
    assert_eq!(next_end_of_word("", 0), 0);
}

// --- Word, line, and Shift+click selections across blocks ---------------------

/// Three one-line paragraphs of same-width words.
fn words_app() -> App {
    let mut app = App::default();
    let _ = app.update(Message::FileLoaded(Ok((
        std::path::PathBuf::from("/tmp/pointer-words.md"),
        "# Words\n\naaaa bbbb cccc\n\ndddd eeee ffff\n\ngggg hhhh iiii\n\nThe end.\n".into(),
    ))));
    app
}

/// The paragraph that starts with `first` and the middle of its `n`th
/// four-letter word (0-based; words are evenly wide).
fn word_point(ui: &mut PointerUi, app: &App, first: &str, n: usize) -> Point {
    let text = block_bounds(ui, app, |b| {
        crate::plain_text::block_text(b).is_some_and(|t| t.starts_with(first))
    });
    // 14 characters: "xxxx xxxx xxxx"; word n spans 5n..5n+4.
    let x = text.x + text.width * (5.0 * n as f32 + 2.0) / 14.0;
    Point::new(x, text.y + text.height / 2.0)
}

fn probe(ui: &mut PointerUi, app: &App) -> Point {
    let end = block_bounds(ui, app, |b| {
        crate::plain_text::block_text(b).is_some_and(|t| t.starts_with("The end"))
    });
    Point::new(end.x + 4.0, end.y + 8.0)
}

/// `clicks` presses at `from` (the last one held), a drag to `to`, and a
/// release, all at once so the clicks count as one double or triple click.
fn multi_click_drag(app: &mut App, ui: &mut PointerUi, clicks: usize, from: Point, to: Point) {
    let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
    let release = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
    let mut events = Vec::new();
    for i in 0..clicks {
        if i > 0 {
            events.push(release.clone());
        }
        events.push(press.clone());
    }
    let mut messages = ui.move_to(app, from).0;
    messages.extend(ui.send(app, from, events).0);
    messages.extend(ui.move_to(app, to).0);
    messages.extend(ui.send(app, to, vec![release]).0);
    apply(app, messages);
}

fn shift(ui: &mut PointerUi, app: &App, held: bool) {
    let modifiers = if held {
        iced::keyboard::Modifiers::SHIFT
    } else {
        iced::keyboard::Modifiers::empty()
    };
    let _ = ui.send(
        app,
        Point::new(1.0, 1.0),
        vec![Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
            modifiers,
        ))],
    );
}

#[test]
fn a_double_click_drag_across_blocks_selects_whole_words() {
    let mut app = words_app();
    let mut ui = PointerUi::new();
    let from = word_point(&mut ui, &app, "aaaa", 1);
    let to = word_point(&mut ui, &app, "dddd", 1);
    multi_click_drag(&mut app, &mut ui, 2, from, to);
    let at = probe(&mut ui, &app);
    assert_eq!(
        selected(&mut app, &mut ui, at).as_deref(),
        Some("bbbb cccc\n\ndddd eeee")
    );

    // Backward: from "hhhh" up to "eeee".
    let from = word_point(&mut ui, &app, "gggg", 1);
    let to = word_point(&mut ui, &app, "dddd", 1);
    multi_click_drag(&mut app, &mut ui, 2, from, to);
    assert_eq!(
        selected(&mut app, &mut ui, at).as_deref(),
        Some("eeee ffff\n\ngggg hhhh")
    );
}

#[test]
fn a_triple_click_drag_across_blocks_selects_whole_lines() {
    let mut app = words_app();
    let mut ui = PointerUi::new();
    let from = word_point(&mut ui, &app, "aaaa", 1);
    let to = word_point(&mut ui, &app, "dddd", 1);
    multi_click_drag(&mut app, &mut ui, 3, from, to);
    let at = probe(&mut ui, &app);
    assert_eq!(
        selected(&mut app, &mut ui, at).as_deref(),
        Some("aaaa bbbb cccc\n\ndddd eeee ffff")
    );
}

#[test]
fn shift_click_extends_the_selection_within_and_across_blocks() {
    let mut app = words_app();
    let mut ui = PointerUi::new();
    let at = probe(&mut ui, &app);

    // A caret in "bbbb", then Shift+click in "eeee": from the caret.
    let caret = word_point(&mut ui, &app, "aaaa", 1);
    let _ = ui.click(&app, caret);
    shift(&mut ui, &app, true);
    let target = word_point(&mut ui, &app, "dddd", 1);
    let messages = ui.click(&app, target);
    apply(&mut app, messages);
    shift(&mut ui, &app, false);
    let got = selected(&mut app, &mut ui, at).expect("Shift+click selected");
    assert!(
        got.starts_with("bb") && got.contains("cccc\n\ndddd e") && !got.contains("ffff"),
        "{got:?}"
    );

    // Shift+click again, in the third block: the same start, a new end.
    shift(&mut ui, &app, true);
    let target = word_point(&mut ui, &app, "gggg", 1);
    let messages = ui.click(&app, target);
    apply(&mut app, messages);
    shift(&mut ui, &app, false);
    let longer = selected(&mut app, &mut ui, at).expect("still selected");
    assert!(
        longer.starts_with(&got[..4]) && longer.contains("ffff\n\ngggg h"),
        "{longer:?}"
    );

    // Back in the first block, Shift+click there moves the end back.
    shift(&mut ui, &app, true);
    let target = word_point(&mut ui, &app, "aaaa", 2);
    let messages = ui.click(&app, target);
    apply(&mut app, messages);
    shift(&mut ui, &app, false);
    let inside = selected(&mut app, &mut ui, at).expect("still selected");
    assert!(
        inside.starts_with("bb") && inside.contains(" c") && !inside.contains('\n'),
        "{inside:?}"
    );
}

#[test]
fn shift_click_extends_from_the_caret_a_margin_press_left() {
    let mut app = words_app();
    let mut ui = PointerUi::new();
    let at = probe(&mut ui, &app);
    let word = word_point(&mut ui, &app, "dddd", 1);
    let line = block_bounds(&mut ui, &app, |b| {
        crate::plain_text::block_text(b).is_some_and(|t| t.starts_with("dddd"))
    });
    // Right of "dddd eeee ffff": the caret goes to the end of that line.
    let _ = ui.click(&app, Point::new(line.x + line.width + 80.0, word.y));
    shift(&mut ui, &app, true);
    let messages = ui.click(&app, word);
    apply(&mut app, messages);
    shift(&mut ui, &app, false);
    let got = selected(&mut app, &mut ui, at).expect("Shift+click selected");
    assert!(got.ends_with("e ffff") && !got.contains("dddd"), "{got:?}");
}

// --- Review fixes (MDV-048 P7) -------------------------------------------------

#[test]
fn losing_window_focus_stops_the_auto_scroll() {
    // The release can go to another app (⌘Tab mid-drag); the tick must not
    // go on scrolling in the background.
    let mut app = long_doc_app();
    let _ = app.update(Message::ReaderAutoScroll(Some(12.0)));
    let _ = app.update(Message::WindowUnfocused(iced::window::Id::unique()));
    assert_eq!(app.reader_autoscroll, None, "unfocus stops the tick");

    // The reader drops its held press too: the next move does not start
    // the scroll again.
    let mut ui = PointerUi::new();
    let text = long_paragraph(&mut ui, &app, 2).expect("paragraph 2 is on screen");
    let start = Point::new(text.x + 1.0, text.y + 8.0);
    let below = Point::new(text.x + 80.0, WINDOW.height + 60.0);
    let _ = press(&mut ui, &app, start);
    assert!(matches!(
        autoscroll(&ui.move_to(&app, below).0).as_slice(),
        [Some(_)]
    ));
    let unfocused = ui
        .send(
            &app,
            below,
            vec![Event::Window(iced::window::Event::Unfocused)],
        )
        .0;
    assert_eq!(autoscroll(&unfocused), [None], "{unfocused:?}");
    let later = autoscroll(&ui.move_to(&app, Point::new(below.x, below.y + 120.0)).0);
    assert!(later.is_empty(), "no stale drag restarts it: {later:?}");
}

#[test]
fn shift_click_on_a_copy_button_still_copies() {
    let (mut app, dir) = fixture("shift-copy");
    let mut ui = PointerUi::new();
    let text = paragraph(&mut ui, &app);
    // A caret in the paragraph, then Shift+click on the code block's copy
    // button: the button takes the press.
    let _ = ui.click(&app, Point::new(text.x + 30.0, text.y + 8.0));
    shift(&mut ui, &app, true);
    let copy = ui
        .text_bounds(&app, &crate::icon::ic::COPY.to_string())
        .expect("the code block's copy button");
    let messages = ui.click(&app, center(copy));
    shift(&mut ui, &app, false);
    assert!(
        has(&messages, |m| matches!(m, Message::CopyBlockSource(..))),
        "{messages:?}"
    );
    apply(&mut app, messages);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_word_drag_does_not_segment_its_line_again_on_every_move() {
    use iced_selection::word::{next_end_of_word, previous_start_of_word};
    // A long Chinese paragraph with punctuation between short runs: each
    // run used to build its own system tokenizer, on every pointer move.
    let line = "中文分詞測試，日本語の文章、そして漢字。".repeat(50);
    let graphemes = line.chars().count();
    let _ = next_end_of_word(&line, 0);
    let started = std::time::Instant::now();
    for i in 0..200 {
        let at = (i * 7) % graphemes;
        let _ = next_end_of_word(&line, at);
        let _ = previous_start_of_word(&line, at);
    }
    let elapsed = started.elapsed();
    assert!(
        elapsed < std::time::Duration::from_millis(100),
        "200 drag moves over a {graphemes}-character line took {elapsed:?}"
    );
}
