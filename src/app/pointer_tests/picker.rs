//! The folder picker's pointer paths (MDV-048 polish): a click on a folder
//! row descends, a click on a file row opens the file, and a double-click on
//! a folder row opens that folder as the workspace, as Enter does.

use super::*;

/// `root/` holds `alpha/` (with `inner/` and `a.md`), the empty `empty/`, and
/// `z.md`. The picker lists folders first, so `alpha` is the first row and
/// `inner` takes its place after the picker descends into `alpha`.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("rmdv-pointer-picker-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("alpha").join("inner")).unwrap();
        std::fs::write(root.join("alpha").join("a.md"), "a").unwrap();
        std::fs::create_dir_all(root.join("empty")).unwrap();
        std::fs::write(root.join("z.md"), "z").unwrap();
        Self { root }
    }

    fn app(&self) -> App {
        let mut app = App::default();
        app.overlay = Overlay::FolderPicker;
        app.picker = Some(Picker::new(
            Some(self.root.clone()),
            PickerMode::OpenAny,
            false,
        ));
        app
    }

    fn path(&self, rel: &str) -> PathBuf {
        canonicalize_existing_path(self.root.join(rel))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn workspace(app: &App) -> Option<PathBuf> {
    app.workspace.clone().map(canonicalize_existing_path)
}

/// One press and release in one pass, as one click of a double-click that
/// the event loop delivers after the view rebuilt for the click before it.
fn press_release(ui: &mut PointerUi, app: &App, at: Point) -> Vec<Message> {
    ui.send(
        app,
        at,
        vec![
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ],
    )
    .0
}

#[test]
fn single_click_on_a_folder_row_descends() {
    let fx = Fixture::new("single");
    let mut app = fx.app();
    let mut ui = PointerUi::new();
    let row = ui.text_bounds(&app, "alpha").expect("alpha row");

    let messages = ui.click(&app, center(row));
    apply(&mut app, messages);

    assert_eq!(app.overlay, Overlay::FolderPicker, "the picker stays open");
    let cwd = app
        .picker
        .as_ref()
        .map(|p| canonicalize_existing_path(p.cwd.clone()));
    assert_eq!(cwd, Some(fx.path("alpha")), "the click descended");
    assert_eq!(app.workspace, None, "a single click opens no workspace");
}

#[test]
fn single_click_on_a_file_row_opens_the_file() {
    let fx = Fixture::new("file");
    let mut app = fx.app();
    let mut ui = PointerUi::new();
    let row = ui.text_bounds(&app, "z.md").expect("z.md row");

    let messages = ui.click(&app, center(row));
    assert!(
        has(
            &messages,
            |m| matches!(m, Message::PickerOpenFile(p) if p.ends_with("z.md"))
        ),
        "{messages:?}"
    );
    apply(&mut app, messages);
    assert_eq!(app.overlay, Overlay::None);
    assert!(app.picker.is_none());
}

#[test]
fn double_click_on_a_folder_row_opens_that_folder() {
    let fx = Fixture::new("double");
    let mut app = fx.app();
    let mut ui = PointerUi::new();
    let row = ui.text_bounds(&app, "alpha").expect("alpha row");

    let messages = ui.double_click(&app, center(row));
    apply(&mut app, messages);

    assert_eq!(
        app.overlay,
        Overlay::None,
        "the double-click closes the picker"
    );
    assert!(app.picker.is_none());
    assert_eq!(workspace(&app), Some(fx.path("alpha")));
}

#[test]
fn double_click_across_the_descend_rebuild_opens_the_first_folder() {
    // The first click descends, so the second click lands on `inner`, the
    // first row of the child listing. It must open `alpha`, not `inner`
    // and not the root.
    let fx = Fixture::new("rebuild");
    let mut app = fx.app();
    let mut ui = PointerUi::new();
    let at = center(ui.text_bounds(&app, "alpha").expect("alpha row"));

    let messages = ui.move_to(&app, at).0;
    apply(&mut app, messages);
    let messages = press_release(&mut ui, &app, at);
    apply(&mut app, messages);
    assert!(
        ui.text_bounds(&app, "inner")
            .is_some_and(|b| (b.y - 12.0..b.y + b.height + 12.0).contains(&at.y)),
        "the child listing puts inner under the pointer"
    );
    let messages = press_release(&mut ui, &app, at);
    apply(&mut app, messages);

    assert_eq!(app.overlay, Overlay::None);
    assert!(app.picker.is_none());
    assert_eq!(workspace(&app), Some(fx.path("alpha")));
}

#[test]
fn double_click_on_an_empty_folder_opens_it_and_nothing_else() {
    let fx = Fixture::new("empty");
    // Both clicks in one pass, and the second click after the rebuild.
    for rebuild in [false, true] {
        let mut app = fx.app();
        let mut ui = PointerUi::new();
        let at = center(ui.text_bounds(&app, "empty").expect("empty row"));
        let messages = if rebuild {
            let mut messages = ui.move_to(&app, at).0;
            messages.extend(press_release(&mut ui, &app, at));
            apply(&mut app, messages);
            // The first click already opened the leaf folder and closed the
            // picker; the second press must not reach the view below.
            assert_eq!(app.overlay, Overlay::None);
            press_release(&mut ui, &app, at)
        } else {
            ui.double_click(&app, at)
        };
        let file_before = app.file.clone();
        apply(&mut app, messages);

        assert_eq!(app.overlay, Overlay::None, "rebuild={rebuild}");
        assert!(app.picker.is_none(), "rebuild={rebuild}");
        assert_eq!(workspace(&app), Some(fx.path("empty")), "rebuild={rebuild}");
        assert_eq!(app.file, file_before, "rebuild={rebuild}");
    }
}

#[test]
fn second_press_after_a_leaf_open_does_not_reach_the_view_below() {
    // A document of one long link lies under the picker. The second press
    // of a double-click on a leaf folder lands where the picker was, after
    // the first click opened the folder and closed the picker; the link
    // under it must not open.
    let fx = Fixture::new("leaf-below");
    let words = vec!["word"; 600].join(" ");
    let mut app = App::default();
    let _ = app.update(Message::FileLoaded(Ok((
        PathBuf::from("/tmp/pointer-picker-links.md"),
        format!("[{words}](https://example.com/below)\n"),
    ))));
    app.overlay = Overlay::FolderPicker;
    app.picker = Some(Picker::new(
        Some(fx.root.clone()),
        PickerMode::OpenAny,
        false,
    ));
    let mut ui = PointerUi::new();
    // Right of the name, over the reader column once the sidebar opens.
    let row = ui.text_bounds(&app, "empty").expect("empty row");
    let at = Point::new(760.0, row.center_y());
    let mut messages = ui.move_to(&app, at).0;
    messages.extend(press_release(&mut ui, &app, at));
    apply(&mut app, messages);
    assert_eq!(app.overlay, Overlay::None);
    assert_eq!(workspace(&app), Some(fx.path("empty")));

    let messages = press_release(&mut ui, &app, at);
    assert!(
        !has(&messages, |m| matches!(m, Message::OpenLink(_))),
        "{messages:?}"
    );
    // Without the double-click, a click there does reach the link.
    let mut ui = PointerUi::new();
    let _ = ui.move_to(&app, at);
    let messages = ui.click(&app, at);
    assert!(
        has(&messages, |m| matches!(m, Message::OpenLink(_))),
        "the link lies under the picker row: {messages:?}"
    );
}

#[test]
fn picker_footer_has_no_open_this_folder_button_and_names_double_click() {
    let fx = Fixture::new("footer");
    let app = fx.app();
    let mut ui = PointerUi::new();
    assert!(ui.text_bounds(&app, "Open This Folder").is_none());
    assert!(
        ui.text_bounds(&app, "open / double-click").is_some(),
        "the Enter hint names the pointer path"
    );
}

#[test]
fn a_folder_click_before_esc_does_not_swallow_the_next_click() {
    // A click on a folder row descends; Esc then closes the picker. A click
    // at the same spot right after must reach the document below, not
    // count as the second click of a picker double-click.
    let fx = Fixture::new("esc-below");
    let words = vec!["word"; 600].join(" ");
    let mut app = App::default();
    let _ = app.update(Message::FileLoaded(Ok((
        PathBuf::from("/tmp/pointer-picker-esc-links.md"),
        format!("[{words}](https://example.com/below)\n"),
    ))));
    let _ = app.update(Message::OpenFolderPicker);
    app.picker = Some(Picker::new(
        Some(fx.root.clone()),
        PickerMode::OpenAny,
        false,
    ));
    let mut ui = PointerUi::new();
    let row = ui.text_bounds(&app, "alpha").expect("alpha row");
    let at = Point::new(760.0, row.center_y());
    let mut messages = ui.move_to(&app, at).0;
    messages.extend(press_release(&mut ui, &app, at));
    apply(&mut app, messages);
    assert_eq!(app.overlay, Overlay::FolderPicker, "the click descended");

    let _ = app.update(Message::CloseOverlay);
    assert_eq!(app.overlay, Overlay::None);
    let messages = press_release(&mut ui, &app, at);
    assert!(
        has(&messages, |m| matches!(m, Message::OpenLink(_))),
        "the click reaches the link: {messages:?}"
    );
}
