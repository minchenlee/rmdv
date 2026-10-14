//! Headless PNG capture of the real `App::view` through `iced_test` (tiny-skia).
//! No window opens. Run explicitly:
//!   RMDV_SHOT_FILE=demo/README.md RMDV_SHOT_OUT=/private/tmp/x/readme.png \
//!   HOME=<fake home> ICED_TEST_BACKEND=tiny-skia \
//!   cargo test --test headless_shot -- --ignored --nocapture
use iced_test::Simulator;
use rmdv::app::{App, Message};
use std::borrow::Cow;

fn settings() -> iced::Settings {
    iced::Settings {
        fonts: rmdv::fonts::BUNDLED.iter().map(|b| Cow::Borrowed(*b)).collect(),
        default_font: iced::Font::with_name("Inter"),
        ..Default::default()
    }
}

#[test]
#[ignore = "writes a PNG; needs RMDV_SHOT_FILE and RMDV_SHOT_OUT"]
fn capture_reader_view() {
    let file = std::path::PathBuf::from(std::env::var("RMDV_SHOT_FILE").expect("RMDV_SHOT_FILE"));
    let out = std::path::PathBuf::from(std::env::var("RMDV_SHOT_OUT").expect("RMDV_SHOT_OUT"));
    let file = file.canonicalize().expect("file exists");
    let source = std::fs::read_to_string(&file).expect("read file");

    // The returned Task is dropped, never run: no runtime, no window.
    let (mut app, _task) = App::new(Some(file.clone()));
    let _ = app.update(Message::FileLoaded(Ok((file, source))));

    let theme = app.theme();
    let mut sim = Simulator::with_size(settings(), iced::Size::new(1280.0, 800.0), app.view());
    let snap = sim.snapshot(&theme).expect("snapshot");
    let _ = std::fs::remove_file(&out);
    assert!(snap.matches_image(&out).expect("write png"));
    println!("renderer={} out={}", "see snapshot", out.display());
}
