//! Links (MDV-048 P3, group 2): where a link goes, what hovering one shows,
//! and images wrapped in links.

use super::*;
use std::time::Duration;

/// A workspace with `a.md` and files of each kind a link can point at.
fn fixture(tag: &str) -> (App, PathBuf) {
    let dir = std::env::temp_dir().join(format!("rmdv-links-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sub")).expect("fixture dir");
    std::fs::write(dir.join("a.md"), "# A\n\nText.\n").expect("a.md");
    std::fs::write(dir.join("data.json"), "{\"k\": 1}\n").expect("data.json");
    std::fs::write(dir.join("conf.yaml"), "k: 1\n").expect("conf.yaml");
    std::fs::write(dir.join("sub").join("NOTES.MD"), "# Notes\n").expect("NOTES.MD");
    let dir = canonicalize_existing_path(dir);
    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    (app, dir)
}

/// Runs a task the way the runtime does, applying the messages it yields.
fn run(app: &mut App, task: Task<Message>) {
    use iced::futures::StreamExt;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let Some(mut stream) = iced_runtime::task::into_stream(task) else {
        return;
    };
    loop {
        let next = runtime.block_on(async {
            tokio::time::timeout(Duration::from_millis(200), stream.next()).await
        });
        match next {
            Ok(Some(iced_runtime::Action::Output(message))) => {
                let task = app.update(message);
                run(app, task);
            }
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => break,
        }
    }
}

fn send(app: &mut App, message: Message) {
    let task = app.update(message);
    run(app, task);
}

fn toast_text(app: &App) -> String {
    app.toast
        .as_ref()
        .map(|t| t.text.clone())
        .unwrap_or_default()
}

// --- 2. Link targets -----------------------------------------------------------

#[test]
fn links_to_files_the_sidebar_opens_open_inside_rmdv() {
    let (mut app, dir) = fixture("targets");
    let a = dir.join("a.md");
    send(&mut app, Message::Open(a.clone()));
    assert_eq!(app.file.as_deref(), Some(a.as_path()));

    send(&mut app, Message::OpenLink("data.json".into()));
    assert_eq!(
        app.file.as_deref(),
        Some(dir.join("data.json").as_path()),
        "a JSON link opens in rmdv"
    );
    send(&mut app, Message::OpenLink("conf.yaml".into()));
    assert_eq!(
        app.file.as_deref(),
        Some(dir.join("conf.yaml").as_path()),
        "a YAML link opens in rmdv"
    );
    send(&mut app, Message::OpenLink("sub/NOTES.MD".into()));
    assert_eq!(
        app.file.as_deref(),
        Some(dir.join("sub").join("NOTES.MD").as_path()),
        "an upper-case .MD link still opens in rmdv"
    );
    assert!(app.external_opens.is_empty(), "{:?}", app.external_opens);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_link_decision_is_the_sidebar_decision() {
    for name in [
        "a.md",
        "a.markdown",
        "a.tex",
        "a.json",
        "a.yaml",
        "a.yml",
        "a.toml",
    ] {
        assert!(link_opens_in_app(Path::new(name)), "{name}");
        assert_eq!(
            link_opens_in_app(Path::new(name)),
            crate::picker::is_markdown_path(Path::new(name))
        );
    }
    assert!(link_opens_in_app(Path::new("A.JSON")));
    assert_eq!(
        link_opens_in_app(Path::new("a.pdf")),
        cfg!(feature = "pdf"),
        "PDFs open in rmdv only with the pdf feature"
    );
    for name in ["a.png", "a.zip", "Makefile"] {
        assert!(!link_opens_in_app(Path::new(name)), "{name}");
    }
}

#[test]
fn a_link_to_a_missing_file_says_file_not_found() {
    let (mut app, dir) = fixture("missing");
    let a = dir.join("a.md");
    send(&mut app, Message::Open(a.clone()));
    send(&mut app, Message::OpenLink("gone.md#part".into()));
    assert_eq!(app.file.as_deref(), Some(a.as_path()), "a.md stays open");
    assert_eq!(toast_text(&app), "File not found: gone.md");
    assert!(
        app.external_opens.is_empty(),
        "nothing is handed to the OS: {:?}",
        app.external_opens
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_external_link_opens_in_the_background_with_command_held() {
    let (mut app, dir) = fixture("external");
    send(&mut app, Message::Open(dir.join("a.md")));
    send(&mut app, Message::OpenLink("https://example.com/x".into()));
    app.quick_slots_modifier_held = true;
    send(&mut app, Message::OpenLink("https://example.com/y".into()));
    assert_eq!(
        app.external_opens,
        vec![
            ("https://example.com/x".to_string(), false),
            // Only macOS has a background open (`open -g`).
            (
                "https://example.com/y".to_string(),
                cfg!(target_os = "macos")
            ),
        ]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_local_file_rmdv_cannot_show_goes_to_the_os_by_its_full_path() {
    let (mut app, dir) = fixture("other");
    std::fs::write(dir.join("pic.png"), b"not really").expect("pic.png");
    send(&mut app, Message::Open(dir.join("a.md")));
    send(&mut app, Message::OpenLink("pic.png".into()));
    assert_eq!(
        app.external_opens,
        vec![(dir.join("pic.png").to_string_lossy().into_owned(), false)]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(target_os = "macos")]
#[test]
fn the_background_open_is_open_dash_g() {
    let command = background_open_command("https://example.com").expect("a command");
    assert_eq!(command.get_program(), "open");
    let args: Vec<_> = command.get_args().collect();
    assert_eq!(args, ["-g", "https://example.com"]);
    assert!(
        background_open_command("-a://evil").is_none(),
        "a target that reads as a flag is not passed to open"
    );
}

// --- 1. Link hover -------------------------------------------------------------

/// A few frames, as after the runtime redraws: the reader reports its
/// viewport and lays out.
fn settle(app: &mut App, ui: &mut PointerUi) {
    for _ in 0..3 {
        let (messages, _) = ui.send(
            app,
            Point::new(5.0, 5.0),
            vec![Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            ))],
        );
        for message in messages {
            send(app, message);
        }
    }
}

/// Sweeps the pointer over the window until a link reports the pointer;
/// returns where.
fn find_link(app: &App, ui: &mut PointerUi) -> Option<(Point, Vec<Message>)> {
    for y in (60..400).step_by(6) {
        for x in (200..1200).step_by(24) {
            let at = Point::new(x as f32, y as f32);
            let (messages, _) = ui.move_to(app, at);
            if messages
                .iter()
                .any(|m| matches!(m, Message::LinkHovered(..)))
            {
                return Some((at, messages));
            }
        }
    }
    None
}

fn link_messages(messages: &[Message]) -> Vec<String> {
    messages
        .iter()
        .filter_map(|m| match m {
            Message::LinkHovered(url, _) => Some(format!("LinkHovered({url:?})")),
            Message::LinkLeft(url, _) => Some(format!("LinkLeft({url:?})")),
            _ => None,
        })
        .collect()
}

#[test]
fn hovering_a_reader_link_sends_one_message_and_shows_its_destination() {
    let (mut app, dir) = fixture("hover");
    let a = dir.join("a.md");
    std::fs::write(
        &a,
        "# A\n\n[a long link to the notes file in sub](sub/NOTES.MD#part)\n",
    )
    .expect("a.md");
    let mut ui = PointerUi::new();
    send(&mut app, Message::Open(a.clone()));
    settle(&mut app, &mut ui);
    assert!(
        ui.text_containing(&app, "sub/NOTES.MD").is_none(),
        "no pill at rest"
    );

    let (at, entered) = find_link(&app, &mut ui).expect("a link under the pointer");
    assert_eq!(
        link_messages(&entered),
        vec![r#"LinkHovered("sub/NOTES.MD#part")"#.to_string()]
    );
    for message in entered {
        send(&mut app, message);
    }
    assert_eq!(
        ui.text_containing(&app, "sub/NOTES.MD").as_deref(),
        Some("sub/NOTES.MD#part"),
        "the pill names the workspace-relative destination"
    );
    for dx in [3.0, 6.0, 9.0] {
        let (messages, _) = ui.move_to(&app, Point::new(at.x + dx, at.y));
        assert!(
            link_messages(&messages).is_empty(),
            "moves inside the link send nothing: {messages:?}"
        );
    }
    let (left, _) = ui.move_to(&app, Point::new(at.x, at.y + 200.0));
    assert_eq!(
        link_messages(&left),
        vec![r#"LinkLeft("sub/NOTES.MD#part")"#.to_string()]
    );
    for message in left {
        send(&mut app, message);
    }
    assert!(
        ui.text_containing(&app, "sub/NOTES.MD").is_none(),
        "the pill hides"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_link_pill_shows_paths_relative_to_the_workspace() {
    let (mut app, dir) = fixture("labels");
    let notes = dir.join("sub").join("NOTES.MD");
    let mut ui = PointerUi::new();
    send(&mut app, Message::Open(notes));
    for (url, label) in [
        ("../a.md#top", "a.md#top"),
        ("./other.md", "sub/other.md"),
        ("#part", "#part"),
        ("https://example.com/page", "https://example.com/page"),
    ] {
        send(&mut app, Message::LinkHovered(url.into(), 1));
        assert_eq!(
            ui.text_containing(&app, label).as_deref(),
            Some(label),
            "{url}"
        );
        send(&mut app, Message::LinkLeft(url.into(), 1));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn leaving_one_link_after_entering_the_next_keeps_the_next() {
    let (mut app, dir) = fixture("order");
    let mut ui = PointerUi::new();
    send(&mut app, Message::Open(dir.join("a.md")));
    // Two links in different blocks: the lower one reports the enter
    // before the upper one reports the leave.
    send(&mut app, Message::LinkHovered("sub/x.md".into(), 1));
    send(&mut app, Message::LinkHovered("sub/y.md".into(), 2));
    send(&mut app, Message::LinkLeft("sub/x.md".into(), 1));
    assert_eq!(
        ui.text_containing(&app, "sub/y.md").as_deref(),
        Some("sub/y.md")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A long document whose first block is a link that wraps over many lines.
fn long_link_doc(dir: &Path) -> PathBuf {
    let a = dir.join("a.md");
    let mut long = String::from("# A\n\n[");
    long.push_str(&"many linked words ".repeat(60));
    long.push_str("](sub/NOTES.MD)\n\n");
    for i in 0..200 {
        long.push_str(&format!(
            "Paragraph {i} with enough words to fill a line.\n\n"
        ));
    }
    std::fs::write(&a, &long).expect("a.md");
    a
}

/// Frames with the pointer resting at `at`: the reader reports its
/// viewport, and links check what is under the pointer now.
fn redraw_at(app: &mut App, ui: &mut PointerUi, at: Point) {
    for _ in 0..3 {
        let (messages, _) = ui.send(
            app,
            at,
            vec![Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            ))],
        );
        super::group2::apply_ui(app, ui, messages);
    }
}

fn reader_offset(app: &App) -> f32 {
    app.body_viewport
        .as_ref()
        .map_or(0.0, |viewport| viewport.absolute_offset().y)
}

#[test]
fn the_pill_stays_while_scrolling_keeps_the_link_under_the_pointer() {
    let (mut app, dir) = fixture("scroll");
    let a = long_link_doc(&dir);
    let mut ui = PointerUi::new();
    send(&mut app, Message::Open(a));
    settle(&mut app, &mut ui);
    let (at, entered) = find_link(&app, &mut ui).expect("a link under the pointer");
    super::group2::apply_ui(&mut app, &mut ui, entered);
    // Into the link's second line, so a small scroll keeps it under.
    let at = Point::new(at.x, at.y + 40.0);
    let (moved, _) = ui.move_to(&app, at);
    assert!(
        link_messages(&moved).is_empty(),
        "still on the link: {moved:?}"
    );
    assert!(ui.text_containing(&app, "sub/NOTES.MD").is_some());

    // A trackpad scroll of a few pixels.
    let (messages, _) = ui.send(
        &app,
        at,
        vec![Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Pixels { x: 0.0, y: -8.0 },
        })],
    );
    super::group2::apply_ui(&mut app, &mut ui, messages);
    redraw_at(&mut app, &mut ui, at);
    assert!(reader_offset(&app) > 0.0, "the reader scrolled");
    assert!(
        ui.text_containing(&app, "sub/NOTES.MD").is_some(),
        "the link is still under the pointer, so its pill stays"
    );

    // A jump far away (⌘↓): the link leaves the pointer and the view.
    super::group2::apply_ui(&mut app, &mut ui, vec![Message::ScrollToBottom]);
    redraw_at(&mut app, &mut ui, at);
    assert!(reader_offset(&app) > 2000.0, "at the bottom");
    assert!(
        ui.text_containing(&app, "sub/NOTES.MD").is_none(),
        "the link scrolled away, so its pill hides"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wheeling_the_link_away_from_the_pointer_hides_the_pill() {
    let (mut app, dir) = fixture("wheel-away");
    let a = long_link_doc(&dir);
    let mut ui = PointerUi::new();
    send(&mut app, Message::Open(a));
    settle(&mut app, &mut ui);
    let (at, entered) = find_link(&app, &mut ui).expect("a link under the pointer");
    super::group2::apply_ui(&mut app, &mut ui, entered);
    assert!(ui.text_containing(&app, "sub/NOTES.MD").is_some());
    for _ in 0..4 {
        let messages = ui.wheel(&app, at);
        super::group2::apply_ui(&mut app, &mut ui, messages);
        redraw_at(&mut app, &mut ui, at);
    }
    assert!(reader_offset(&app) > 400.0, "the reader scrolled");
    assert!(
        ui.text_containing(&app, "sub/NOTES.MD").is_none(),
        "the link moved off the pointer, so its pill hides"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn navigating_hides_the_link_pill() {
    let (mut app, dir) = fixture("hide");
    let a = long_link_doc(&dir);
    let mut ui = PointerUi::new();
    send(&mut app, Message::Open(a.clone()));
    settle(&mut app, &mut ui);

    send(&mut app, Message::LinkHovered("sub/NOTES.MD".into(), 1));
    send(&mut app, Message::Open(dir.join("conf.yaml")));
    assert!(
        ui.text_containing(&app, "sub/NOTES.MD").is_none(),
        "opening another file hides the pill"
    );

    send(&mut app, Message::Open(a));
    send(&mut app, Message::LinkHovered("sub/NOTES.MD".into(), 1));
    send(&mut app, Message::ToggleMindmap);
    assert!(
        ui.text_containing(&app, "sub/NOTES.MD").is_none(),
        "the mind map hides the pill"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn moving_up_between_two_links_to_one_url_keeps_the_pill() {
    let (mut app, dir) = fixture("same-url");
    let a = dir.join("a.md");
    std::fs::write(
        &a,
        "# A\n\n[the notes](sub/NOTES.MD)\n\nPlain words between.\n\n[the notes](sub/NOTES.MD)\n",
    )
    .expect("a.md");
    let mut ui = PointerUi::new();
    send(&mut app, Message::Open(a));
    settle(&mut app, &mut ui);
    let (upper, _) = find_link(&app, &mut ui).expect("the upper link");
    let _ = ui.move_to(&app, Point::new(5.0, 5.0));
    // The lower link: the same row of x, further down.
    let lower = (1..60)
        .map(|step| Point::new(upper.x, upper.y + 30.0 + step as f32 * 4.0))
        .find(|at| {
            let (messages, _) = ui.move_to(&app, *at);
            let hit = has(&messages, |m| matches!(m, Message::LinkHovered(..)));
            let _ = ui.move_to(&app, Point::new(5.0, 5.0));
            hit
        })
        .expect("the lower link");
    let (entered, _) = ui.move_to(&app, lower);
    super::group2::apply_ui(&mut app, &mut ui, entered);
    assert!(ui.text_containing(&app, "sub/NOTES.MD").is_some());

    // One move from the lower link straight onto the upper one: the upper
    // widget reports its enter before the lower one reports its leave.
    let (swap, _) = ui.move_to(&app, upper);
    assert_eq!(link_messages(&swap).len(), 2, "{swap:?}");
    super::group2::apply_ui(&mut app, &mut ui, swap);
    assert!(
        ui.text_containing(&app, "sub/NOTES.MD").is_some(),
        "the pointer is on the upper link, so the pill stays"
    );
    let (left, _) = ui.move_to(&app, Point::new(5.0, 5.0));
    super::group2::apply_ui(&mut app, &mut ui, left);
    assert!(
        ui.text_containing(&app, "sub/NOTES.MD").is_none(),
        "leaving the upper link hides it"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn pages_over_the_reader_hide_the_link_pill() {
    let (mut app, dir) = fixture("pages");
    let mut ui = PointerUi::new();
    send(&mut app, Message::Open(dir.join("a.md")));
    let pill = |ui: &mut PointerUi, app: &App| ui.text_containing(app, "sub/NOTES.MD");

    send(&mut app, Message::LinkHovered("sub/NOTES.MD".into(), 1));
    send(&mut app, Message::OpenSettings);
    assert!(pill(&mut ui, &app).is_none(), "Settings hides the pill");
    send(&mut app, Message::CloseSettings);
    assert!(pill(&mut ui, &app).is_none(), "and it stays hidden");

    send(&mut app, Message::LinkHovered("sub/NOTES.MD".into(), 1));
    send(&mut app, Message::OpenVaultSearch);
    assert!(app.vault_open);
    assert!(pill(&mut ui, &app).is_none(), "vault search hides the pill");
    send(&mut app, Message::VaultClose);

    send(&mut app, Message::LinkHovered("sub/NOTES.MD".into(), 1));
    send(&mut app, Message::FileLoaded(Err("boom".into())));
    assert!(app.error.is_some());
    assert!(
        pill(&mut ui, &app).is_none(),
        "the error card hides the pill"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// --- 3. Images wrapped in links ------------------------------------------------

fn image_block_bounds(ui: &mut PointerUi, app: &App, linked: bool) -> Rectangle {
    let id = app
        .ast
        .iter()
        .find(|(_, block)| {
            matches!(block, crate::ast::Block::Image { link, .. } if link.is_some() == linked)
        })
        .map(|(id, _)| *id)
        .expect("image block");
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
    .expect("image block anchor")
    .0
}

#[test]
fn a_linked_image_follows_its_link_and_an_unlinked_one_zooms() {
    let badge = "https://example.com/badge.png";
    let plain = "https://example.com/plain.png";
    let mut app = App::default();
    let _ = app.update(Message::FileLoaded(Ok((
        PathBuf::from("/tmp/pointer-badge.md"),
        format!(
            "Intro\n\n[![build]({badge})](https://ci.example.com)\n\n![plain]({plain})\n\nOutro\n"
        ),
    ))));
    for url in [badge, plain] {
        let handle = iced::widget::image::Handle::from_rgba(200, 100, vec![200u8; 200 * 100 * 4]);
        app.image_cache
            .insert(url.to_string(), ImageState::Loaded(handle));
    }
    let mut ui = PointerUi::new();

    let bounds = image_block_bounds(&mut ui, &app, true);
    let over = Point::new(bounds.x + 20.0, bounds.center_y());
    let (entered, cursor) = ui.move_to(&app, over);
    assert_eq!(cursor, mouse::Interaction::Pointer, "a linked image points");
    assert!(
        has(
            &entered,
            |m| matches!(m, Message::LinkHovered(u, _) if u == "https://ci.example.com")
        ),
        "its destination shows: {entered:?}"
    );
    let messages = ui.click(&app, over);
    assert!(
        has(
            &messages,
            |m| matches!(m, Message::OpenLink(u) if u == "https://ci.example.com")
        ),
        "a click follows the link: {messages:?}"
    );
    assert!(
        !has(&messages, |m| matches!(m, Message::OpenImageZoom(_))),
        "{messages:?}"
    );

    let bounds = image_block_bounds(&mut ui, &app, false);
    let over = Point::new(bounds.x + 20.0, bounds.center_y());
    let (_, cursor) = ui.move_to(&app, over);
    assert_eq!(
        cursor,
        mouse::Interaction::ZoomIn,
        "an unlinked image zooms"
    );
    let messages = ui.click(&app, over);
    assert!(has(&messages, |m| matches!(
        m,
        Message::OpenImageZoom(u) if u == plain
    )));
}
