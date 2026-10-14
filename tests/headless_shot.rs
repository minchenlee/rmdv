//! Headless PNG capture of the real `App::view` through `iced_test` (tiny-skia).
//! No window opens. Run explicitly:
//!   RMDV_SHOT_FILE=demo/README.md RMDV_SHOT_OUT=/private/tmp/x/readme.png \
//!   HOME=<fake home> ICED_TEST_BACKEND=tiny-skia \
//!   RMDV_SHOT_SCENE=reader|search|mermaid|fullmindmap (default reader)
//!   RMDV_SHOT_QUERY=<text>  for the search scene
//!   cargo test --test headless_shot -- --ignored --nocapture
use iced_test::Simulator;
use rmdv::app::{App, Message};
use futures::StreamExt;
use iced_runtime::Action;
use std::borrow::Cow;
use std::time::{Duration, Instant};

/// Run a Task without a window: feed every `Output` message back into
/// `App::update`; ignore window, widget, clipboard and system actions.
fn drain(app: &mut App, rt: &tokio::runtime::Runtime, task: iced::Task<Message>, budget: Duration) {
    let deadline = Instant::now() + budget;
    let mut queue = vec![task];
    rt.block_on(async {
        while let Some(t) = queue.pop() {
            let Some(mut stream) = iced_runtime::task::into_stream(t) else {
                continue;
            };
            loop {
                let left = deadline.saturating_duration_since(Instant::now());
                match tokio::time::timeout(left, stream.next()).await {
                    Ok(Some(Action::Output(m))) => queue.push(app.update(m)),
                    Ok(Some(_)) => {}
                    Ok(None) | Err(_) => break,
                }
            }
        }
    });
}


fn settings() -> iced::Settings {
    iced::Settings {
        fonts: rmdv::fonts::BUNDLED.iter().map(|b| Cow::Borrowed(*b)).collect(),
        default_font: iced::Font::with_name("Inter"),
        ..Default::default()
    }
}

#[test]
#[ignore = "writes a PNG; needs RMDV_SHOT_FILE and RMDV_SHOT_OUT"]
fn capture() {
    let file = std::path::PathBuf::from(std::env::var("RMDV_SHOT_FILE").expect("RMDV_SHOT_FILE"));
    let out = std::path::PathBuf::from(std::env::var("RMDV_SHOT_OUT").expect("RMDV_SHOT_OUT"));
    let scene = std::env::var("RMDV_SHOT_SCENE").unwrap_or_else(|_| "reader".into());
    let file = file.canonicalize().expect("file exists");
    let doc = file.clone();
    let source = std::fs::read_to_string(&file).expect("read file");
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let budget = Duration::from_secs(5);

    let (mut app, task) = App::new(Some(file.clone()));
    drain(&mut app, &rt, task, budget);
    let root = std::env::var("RMDV_SHOT_ROOT")
        .map(std::path::PathBuf::from)
        .ok()
        .or_else(|| file.parent().map(|p| p.to_path_buf()));
    if let Some(dir) = root {
        let t = app.update(Message::OpenWorkspace(dir));
        drain(&mut app, &rt, t, budget);
    }
    let t = app.update(Message::FileLoaded(Ok((file, source))));
    drain(&mut app, &rt, t, budget);

    // RMDV_SHOT_SCENE is a comma list of steps: search, fullmindmap, panel,
    // goto (needs RMDV_SHOT_SECTION), reader/mermaid (no-ops).
    for step in scene.split(',') {
        let msgs: Vec<Message> = match step {
            "search" => vec![
                Message::OpenVaultSearch,
                Message::VaultQueryChanged(
                    std::env::var("RMDV_SHOT_QUERY").unwrap_or_else(|_| "open".into()),
                ),
                Message::VaultRunSearch,
            ],
            "fullmindmap" => vec![Message::ToggleFullMindmap],
            "panel" => vec![Message::FullMindmapTogglePanel],
            "mindmap" => vec![Message::ToggleMindmap],
            "nosidebar" => vec![Message::ToggleSidebar],
            "goto" => {
                let want = std::env::var("RMDV_SHOT_SECTION").expect("RMDV_SHOT_SECTION");
                let src = std::fs::read_to_string(&doc).expect("read doc");
                let line = rmdv::ipc::sections::list_sections(&src)
                    .into_iter()
                    .find(|h| h.path == want)
                    .expect("section exists")
                    .line;
                vec![Message::ScrollToLine(line as u32)]
            }
            _ => vec![],
        };
        for m in msgs {
            let t = app.update(m);
            drain(&mut app, &rt, t, budget);
        }
    }

    // Let widgets publish layout-driven messages (viewport size, fit) and
    // feed them back for a few passes before the final frame.
    let theme = app.theme();
    let mut snap = None;
    for pass in 0..4 {
        let (s, msgs) = {
            let mut sim =
                Simulator::with_size(settings(), iced::Size::new(1280.0, 800.0), app.view());
            if let Ok(n) = std::env::var("RMDV_SHOT_WHEEL") {
                // Scroll the reader under the cursor by N logical pixels (negative = down).
                let n: f32 = n.parse().expect("RMDV_SHOT_WHEEL number");
                sim.point_at(iced::Point::new(700.0, 400.0));
                let wheel = iced::Event::Mouse(iced::mouse::Event::WheelScrolled {
                    delta: iced::mouse::ScrollDelta::Pixels { x: 0.0, y: n },
                });
                sim.simulate(std::iter::once(wheel));
            }
            let s = sim.snapshot(&theme).expect("snapshot");
            (s, sim.into_messages().collect::<Vec<_>>())
        };
        if pass == 3 || msgs.is_empty() {
            snap = Some(s);
            break;
        }
        for m in msgs {
            let t = app.update(m);
            drain(&mut app, &rt, t, budget);
        }
    }
    let snap = snap.expect("snapshot");
    let _ = std::fs::remove_file(&out);
    assert!(snap.matches_image(&out).expect("write png"));
    println!("scene={scene} out={}", out.display());
}
