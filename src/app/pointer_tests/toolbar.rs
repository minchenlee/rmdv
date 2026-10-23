//! The top toolbar (MDV-048 P2): its setting, hover reveal, controls, and
//! the dirty marks in the toolbar and the window title.

use super::group3::{assert_tips, full_mindmap_app, tips, tree_fixture};
use super::*;
use crate::icon::ic;
use crate::prefs::ToolbarMode;

fn settings_row_index(row: SettingsRow) -> usize {
    SettingsRow::visible()
        .iter()
        .position(|r| *r == row)
        .expect("row is visible on this platform")
}

// --- 1. Toolbar setting ------------------------------------------------------

#[test]
fn toolbar_setting_switches_between_hover_and_always_and_persists() {
    let mut app = App::default();
    let isolated = app.quick_slots_persistence_path.clone().unwrap();
    let saved = |path: &Path| crate::prefs::load_from(path).toolbar;
    // A new user sees the toolbar.
    assert_eq!(app.prefs.toolbar, ToolbarMode::Always);

    let _ = app.update(Message::OpenSettings);
    let _ = app.update(Message::SettingsCursor(settings_row_index(
        SettingsRow::Toolbar,
    )));
    // ← picks On hover, → picks Always, Space flips.
    let _ = app.update(Message::SettingsStep(-1));
    assert_eq!(app.prefs.toolbar, ToolbarMode::OnHover);
    assert_eq!(saved(&isolated), ToolbarMode::OnHover);
    let _ = app.update(Message::SettingsStep(-1));
    assert_eq!(app.prefs.toolbar, ToolbarMode::OnHover);
    let _ = app.update(Message::SettingsStep(1));
    assert_eq!(app.prefs.toolbar, ToolbarMode::Always);
    assert_eq!(saved(&isolated), ToolbarMode::Always);
    let _ = app.update(Message::SettingsActivate);
    assert_eq!(app.prefs.toolbar, ToolbarMode::OnHover);
    let _ = app.update(Message::SettingsActivate);
    assert_eq!(app.prefs.toolbar, ToolbarMode::Always);
    assert_eq!(saved(&isolated), ToolbarMode::Always);
}

#[test]
fn toolbar_setting_row_has_a_clickable_segmented_control() {
    let mut app = App::default();
    let _ = app.update(Message::OpenSettings);
    let mut ui = PointerUi::new();
    // Bring the row into view the way the keyboard does.
    let mut at = ui.text_bounds(&app, "On hover");
    for _ in 0..20 {
        if at.is_some_and(|b| b.y > 40.0 && b.y + b.height < 760.0) {
            break;
        }
        let _ = ui.wheel(&app, Point::new(640.0, 400.0));
        at = ui.text_bounds(&app, "On hover");
    }
    let on_hover = at.expect("the Toolbar row shows an On hover segment");
    assert!(ui.text_bounds(&app, "Always").is_some(), "and Always");
    let messages = ui.click(&app, center(on_hover));
    assert!(
        has(&messages, |m| matches!(
            m,
            Message::SetToolbarMode(ToolbarMode::OnHover)
        )),
        "{messages:?}"
    );
    apply(&mut app, messages);
    assert_eq!(app.prefs.toolbar, ToolbarMode::OnHover);
}

// --- 5. Window title marks unsaved edits -------------------------------------

/// `long_doc_app` in edit mode with one typed character.
fn dirty_app() -> App {
    let mut app = long_doc_app();
    let _ = app.update(Message::ToggleViewMode);
    let _ = app.update(Message::EditorAction(
        iced::widget::text_editor::Action::Edit(iced::widget::text_editor::Edit::Insert('x')),
    ));
    assert!(app.dirty, "typing makes the file dirty");
    app
}

#[test]
fn window_title_shows_a_bullet_while_the_file_is_dirty() {
    let clean = long_doc_app();
    assert_eq!(clean.title(), "rmdv — pointer-long.md");
    assert_eq!(dirty_app().title(), "rmdv — pointer-long.md •");
    assert_eq!(App::default().title(), "rmdv");
}

// --- 2. Toolbar controls (Always mode) ------------------------------------------

fn always(mut app: App) -> App {
    let _ = app.update(Message::SetToolbarMode(ToolbarMode::Always));
    app
}

/// The toolbar shows always by default; hover tests pick On hover.
pub(super) fn on_hover(mut app: App) -> App {
    let _ = app.update(Message::SetToolbarMode(ToolbarMode::OnHover));
    app
}

fn glyph(code: char) -> String {
    code.to_string()
}

fn glyph_at(ui: &mut PointerUi, app: &App, code: char) -> Option<Rectangle> {
    ui.text_bounds(app, &glyph(code))
}

#[test]
fn always_toolbar_shows_file_name_dirty_dot_and_named_controls() {
    let app = always(dirty_app());
    let mut ui = PointerUi::new();
    let name = ui
        .text_bounds(&app, "pointer-long.md")
        .expect("the toolbar names the file");
    assert!(name.y < TOOLBAR_H, "in the top strip: {name:?}");
    let dot = ui
        .id_bounds(&app, "toolbar-dirty-dot")
        .expect("a dirty dot beside the name");
    assert!(dot.x > name.x + name.width - 1.0, "after the name");
    for id in ["view-read", "view-edit", "view-mindmap"] {
        let bounds = ui
            .id_bounds(&app, id)
            .unwrap_or_else(|| panic!("{id} segment"));
        assert!(bounds.y < TOOLBAR_H, "{id} in the top strip");
    }
    for code in [ic::SAVE, ic::SEARCH, ic::COMMAND, ic::SETTINGS, ic::NETWORK] {
        let bounds = glyph_at(&mut ui, &app, code).expect("toolbar icon");
        assert!(bounds.y < TOOLBAR_H, "{code:?} in the top strip");
    }
    assert_tips(
        &tips(&mut ui, &app),
        &[
            "Read  Esc",
            "Edit  ⌘E",
            "Mind Map  ⌘M",
            "Full Mindmap  ⌘⇧M",
            "Save  ⌘S",
            "Find  ⌘F",
            "Command Palette  ⌘⇧P",
            "Settings  ⌘,",
        ],
    );

    // A clean file shows no dot.
    let clean = always(long_doc_app());
    let mut ui = PointerUi::new();
    assert!(ui.text_bounds(&clean, "pointer-long.md").is_some());
    assert!(ui.id_bounds(&clean, "toolbar-dirty-dot").is_none());
}

#[test]
fn always_toolbar_buttons_send_their_actions() {
    // Save works only while the file is dirty.
    let clean = always(long_doc_app());
    let mut ui = PointerUi::new();
    let save = glyph_at(&mut ui, &clean, ic::SAVE).expect("Save");
    let messages = ui.click(&clean, center(save));
    assert!(
        !has(&messages, |m| matches!(m, Message::SaveFile)),
        "Save is disabled on a clean file: {messages:?}"
    );
    let dirty = always(dirty_app());
    let mut ui = PointerUi::new();
    let save = glyph_at(&mut ui, &dirty, ic::SAVE).expect("Save");
    let messages = ui.click(&dirty, center(save));
    assert!(has(&messages, |m| matches!(m, Message::SaveFile)));

    let mut app = always(long_doc_app());
    let mut ui = PointerUi::new();
    let find = glyph_at(&mut ui, &app, ic::SEARCH).expect("Find");
    let messages = ui.click(&app, center(find));
    apply(&mut app, messages);
    assert!(app.search_open, "Find opens the find bar");

    let mut app = always(long_doc_app());
    let mut ui = PointerUi::new();
    let palette = glyph_at(&mut ui, &app, ic::COMMAND).expect("Command Palette");
    let messages = ui.click(&app, center(palette));
    apply(&mut app, messages);
    assert_eq!(app.overlay, Overlay::Command);

    let mut app = always(long_doc_app());
    let mut ui = PointerUi::new();
    let settings = glyph_at(&mut ui, &app, ic::SETTINGS).expect("Settings");
    let messages = ui.click(&app, center(settings));
    apply(&mut app, messages);
    assert!(app.settings_open);

    let mut ui = PointerUi::new();
    for (label, view) in [
        ("view-read", ToolbarView::Read),
        ("view-edit", ToolbarView::Edit),
        ("view-mindmap", ToolbarView::Mindmap),
    ] {
        let bounds = ui.id_bounds(&app, label).expect("segment");
        let messages = ui.click(&app, center(bounds));
        assert!(
            has(
                &messages,
                |m| matches!(m, Message::ToolbarView(v) if *v == view)
            ),
            "{label}: {messages:?}"
        );
    }
}

#[test]
fn toolbar_sidebar_toggle_shows_only_while_the_sidebar_is_hidden() {
    let (app, dir) = tree_fixture("toolbar-sidebar");
    let mut app = always(app);
    let alpha = dir.join("alpha.md");
    let _ = app.update(Message::FileLoaded(Ok((alpha, "# Alpha\n".into()))));
    assert!(app.sidebar_open);
    let mut ui = PointerUi::new();
    assert!(
        glyph_at(&mut ui, &app, ic::PANEL_LEFT_OPEN).is_none(),
        "the sidebar header has its own Hide button"
    );
    let _ = app.update(Message::ToggleSidebar);
    let mut ui = PointerUi::new();
    let show = glyph_at(&mut ui, &app, ic::PANEL_LEFT_OPEN).expect("Show Sidebar");
    if cfg!(target_os = "macos") {
        assert!(show.x > 70.0, "clear of the traffic lights: {show:?}");
    }
    assert!(show.y < TOOLBAR_H);
    assert_tips(&tips(&mut ui, &app), &["Show Sidebar  ⌘B"]);
    let messages = ui.click(&app, center(show));
    apply(&mut app, messages);
    assert!(app.sidebar_open, "the toolbar button opens the sidebar");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn always_toolbar_reserves_its_height_without_moving_the_text() {
    let hover = on_hover(long_doc_app());
    let mut ui = PointerUi::new();
    let before = ui
        .id_bounds(&hover, "reader-column")
        .expect("reading column");
    let fixed = always(long_doc_app());
    let mut ui = PointerUi::new();
    let after = ui
        .id_bounds(&fixed, "reader-column")
        .expect("reading column");
    assert_eq!(before.y, after.y, "switching modes does not move the text");
    // The reader scrolls below the toolbar, not behind it.
    let toolbar = ui.id_bounds(&fixed, "toolbar").expect("toolbar");
    assert_eq!(toolbar.y, 0.0);
    assert_eq!(toolbar.height, TOOLBAR_H);
    let hover_toolbar = PointerUi::new().id_bounds(&hover, "toolbar");
    assert!(hover_toolbar.is_none(), "On hover starts hidden");
}

// --- 3. Hover reveal -----------------------------------------------------------

fn hover_count(messages: &[Message], entered: bool) -> usize {
    messages
        .iter()
        .filter(|m| matches!(m, Message::ToolbarHover(e) if *e == entered))
        .count()
}

/// Waits for the messages a task yields, timers included.
pub(super) fn wait_outputs(task: Task<Message>) -> Vec<Message> {
    use iced::futures::StreamExt;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("tokio runtime");
    let Some(stream) = iced_runtime::task::into_stream(task) else {
        return Vec::new();
    };
    runtime.block_on(
        stream
            .filter_map(|action| async move {
                match action {
                    iced_runtime::Action::Output(message) => Some(message),
                    _ => None,
                }
            })
            .collect::<Vec<_>>(),
    )
}

#[test]
fn hover_toolbar_appears_at_the_top_edge_and_hides_after_a_delay() {
    let mut app = on_hover(long_doc_app());
    let mut ui = PointerUi::new();
    let (messages, _) = ui.move_to(&app, Point::new(640.0, 400.0));
    assert_eq!(hover_count(&messages, true), 0);
    assert!(ui.id_bounds(&app, "toolbar").is_none(), "hidden at first");

    // Entering the top band publishes one message and shows the toolbar.
    let (messages, _) = ui.move_to(&app, Point::new(640.0, 30.0));
    assert_eq!(hover_count(&messages, true), 1, "{messages:?}");
    apply(&mut app, messages);
    let bar = ui.id_bounds(&app, "toolbar").expect("revealed");
    assert_eq!(bar.y, 0.0);
    // Moving inside the band and over the toolbar publishes nothing more.
    for x in [600.0, 500.0, 300.0] {
        let (messages, _) = ui.move_to(&app, Point::new(x, 12.0));
        assert!(
            !has(&messages, |m| matches!(m, Message::ToolbarHover(_))),
            "no message on plain moves: {messages:?}"
        );
    }

    // Leaving publishes one message; the toolbar stays for the delay.
    let (messages, _) = ui.move_to(&app, Point::new(640.0, 400.0));
    assert_eq!(hover_count(&messages, false), 1, "{messages:?}");
    let mut hide = Vec::new();
    for message in messages {
        hide.extend(wait_outputs(app.update(message)));
    }
    assert!(
        ui.id_bounds(&app, "toolbar").is_some(),
        "stays during the delay"
    );
    assert!(
        has(&hide, |m| matches!(m, Message::ToolbarHideAfter(_))),
        "the delay ends in a hide message: {hide:?}"
    );
    apply(&mut app, hide);
    assert!(
        ui.id_bounds(&app, "toolbar").is_none(),
        "hidden after the delay"
    );
}

#[test]
fn returning_to_the_band_cancels_the_pending_hide() {
    let mut app = on_hover(long_doc_app());
    let _ = app.update(Message::ToolbarHover(true));
    let _ = app.update(Message::ToolbarHover(false));
    let stale = app.toolbar_hover_generation;
    let _ = app.update(Message::ToolbarHover(true));
    let _ = app.update(Message::ToolbarHideAfter(stale));
    assert!(app.toolbar_revealed, "a stale hide does nothing");
    let _ = app.update(Message::ToolbarHover(false));
    let current = app.toolbar_hover_generation;
    let _ = app.update(Message::ToolbarHideAfter(current));
    assert!(!app.toolbar_revealed);
}

#[test]
fn hidden_band_leaves_the_content_its_pointer() {
    let app = on_hover(long_doc_app());
    let mut ui = PointerUi::new();
    // Scroll text under the band, then probe its height.
    for _ in 0..3 {
        let _ = ui.wheel(&app, Point::new(640.0, 400.0));
    }
    let text_under_band = (1..20).any(|step| {
        let (_, cursor) = ui.move_to(&app, Point::new(640.0, step as f32 * 2.0));
        cursor == mouse::Interaction::Text
    });
    assert!(
        text_under_band,
        "the reader keeps its text cursor under the band"
    );
}

#[test]
fn revealed_toolbar_takes_presses_on_its_blank_space() {
    let mut app = on_hover(long_doc_app());
    let mut ui = PointerUi::new();
    // Text scrolled under the band shows the text cursor there.
    for _ in 0..3 {
        let _ = ui.wheel(&app, Point::new(640.0, 400.0));
    }
    let probes: Vec<Point> = (1..16)
        .map(|step| Point::new(400.0 + step as f32 * 8.0, 16.0))
        .collect();
    let under: Vec<Point> = probes
        .iter()
        .copied()
        .filter(|at| ui.move_to(&app, *at).1 == mouse::Interaction::Text)
        .collect();
    assert!(!under.is_empty(), "text lies under the band");
    let _ = app.update(Message::ToolbarHover(true));
    let bar = ui.id_bounds(&app, "toolbar").expect("revealed");
    let name = ui.text_bounds(&app, "pointer-long.md").expect("name");
    for at in under {
        if at.x > name.x - 4.0 && at.x < name.x + name.width + 4.0 {
            continue;
        }
        assert!(bar.contains(at));
        let (_, cursor) = ui.move_to(&app, at);
        assert_ne!(
            cursor,
            mouse::Interaction::Text,
            "the reader's text cursor does not show through the toolbar at {at:?}"
        );
    }
}

#[test]
fn always_mode_has_no_hover_band() {
    let app = always(long_doc_app());
    let mut ui = PointerUi::new();
    let _ = ui.move_to(&app, Point::new(640.0, 400.0));
    let (messages, _) = ui.move_to(&app, Point::new(640.0, 10.0));
    assert!(!has(&messages, |m| matches!(m, Message::ToolbarHover(_))));
}

// --- 4. View switching, Zen exit and save, mind maps ------------------------------

#[test]
fn view_switch_moves_between_read_edit_and_mind_map() {
    let mut app = long_doc_app();
    let _ = app.update(Message::ToolbarView(ToolbarView::Read));
    assert_eq!(app.view_mode, ViewMode::Rendered, "Read on Read stays");
    let _ = app.update(Message::ToolbarView(ToolbarView::Edit));
    assert_eq!(app.view_mode, ViewMode::Raw);
    assert!(app.editor.is_some());
    let _ = app.update(Message::ToolbarView(ToolbarView::Edit));
    assert_eq!(app.view_mode, ViewMode::Raw, "Edit on Edit stays");
    let _ = app.update(Message::ToolbarView(ToolbarView::Mindmap));
    assert_eq!(app.view_mode, ViewMode::Mindmap);
    let _ = app.update(Message::ToolbarView(ToolbarView::Edit));
    assert_eq!(app.view_mode, ViewMode::Raw);
    let _ = app.update(Message::ToolbarView(ToolbarView::Read));
    assert_eq!(app.view_mode, ViewMode::Rendered);
    assert!(app.editor.is_none());
    let _ = app.update(Message::ToolbarView(ToolbarView::Mindmap));
    let _ = app.update(Message::ToolbarView(ToolbarView::Mindmap));
    assert_eq!(
        app.view_mode,
        ViewMode::Mindmap,
        "Mind Map on Mind Map stays"
    );
    let _ = app.update(Message::ToolbarView(ToolbarView::Read));
    assert_eq!(app.view_mode, ViewMode::Rendered);
}

#[test]
fn view_switch_leaves_settings_and_vault_search_to_show_the_view() {
    let mut app = long_doc_app();
    let _ = app.update(Message::OpenSettings);
    let _ = app.update(Message::ToolbarView(ToolbarView::Read));
    assert!(!app.settings_open, "Read shows the reader");
    app.vault_open = true;
    let _ = app.update(Message::ToolbarView(ToolbarView::Edit));
    assert!(!app.vault_open);
    assert_eq!(app.view_mode, ViewMode::Raw);
}

#[test]
fn hover_toolbar_leaves_edit_mode_and_saves() {
    let mut app = on_hover(dirty_app());
    let _ = app.update(Message::ToolbarHover(true));
    let mut ui = PointerUi::new();
    let save = glyph_at(&mut ui, &app, ic::SAVE).expect("Save in edit mode");
    let messages = ui.click(&app, center(save));
    assert!(has(&messages, |m| matches!(m, Message::SaveFile)));
    let read = ui.id_bounds(&app, "view-read").expect("Read segment");
    let messages = ui.click(&app, center(read));
    apply(&mut app, messages);
    assert_eq!(app.view_mode, ViewMode::Rendered, "Read leaves edit mode");
    assert!(app.dirty, "the edit is kept, unsaved");
}

#[test]
fn document_mind_map_switches_back_and_toggles_its_panel_from_the_toolbar() {
    let mut app = on_hover(long_doc_app());
    let _ = app.update(Message::ToolbarView(ToolbarView::Mindmap));
    let _ = app.update(Message::ToolbarHover(true));
    let open = app.mindmap_panel_open;
    let mut ui = PointerUi::new();
    let (glyph, label) = if open {
        (ic::PANEL_RIGHT_CLOSE, "Hide Panel  ⌘⌥B")
    } else {
        (ic::PANEL_RIGHT_OPEN, "Show Panel  ⌘⌥B")
    };
    assert_tips(&tips(&mut ui, &app), &[label]);
    let toggle = glyph_at(&mut ui, &app, glyph).expect("panel toggle");
    assert!(toggle.y < TOOLBAR_H);
    let messages = ui.click(&app, center(toggle));
    apply(&mut app, messages);
    assert_eq!(app.mindmap_panel_open, !open);
    let read = ui.id_bounds(&app, "view-read").expect("Read segment");
    let messages = ui.click(&app, center(read));
    apply(&mut app, messages);
    assert_eq!(app.view_mode, ViewMode::Rendered);
}

#[test]
fn full_mindmap_segment_enters_and_read_leaves_it() {
    let (app, dir) = full_mindmap_app("toolbar-fmm");
    let mut app = on_hover(app);
    let _ = app.update(Message::ToolbarHover(true));
    let mut ui = PointerUi::new();
    let back = ui.text_bounds(&app, "Back").expect("Back pill stays");
    assert!(
        back.y >= TOOLBAR_BAND_H,
        "the Back island sits below the hover band: {back:?}"
    );
    let bar = ui
        .id_bounds(&app, "toolbar")
        .expect("toolbar over Full Mindmap");
    assert!(
        bar.y + bar.height <= back.y,
        "the toolbar does not cover Back"
    );
    let read = ui.id_bounds(&app, "view-read").expect("Read segment");
    let messages = ui.click(&app, center(read));
    apply(&mut app, messages);
    assert!(app.full_mindmap.is_none(), "Read leaves Full Mindmap");
    assert_eq!(app.view_mode, ViewMode::Rendered);

    let alpha = dir.join("alpha.md");
    let _ = app.update(Message::FileLoaded(Ok((alpha, "# Alpha\n".into()))));
    let _ = app.update(Message::ToolbarView(ToolbarView::FullMindmap));
    assert!(
        app.full_mindmap.is_some(),
        "the Full Mindmap segment enters it"
    );
    let _ = app.update(Message::ToolbarView(ToolbarView::FullMindmap));
    assert!(app.full_mindmap.is_some(), "and pressing it again stays");
    let _ = app.update(Message::ToolbarView(ToolbarView::Mindmap));
    assert!(app.full_mindmap.is_none());
    assert_eq!(
        app.view_mode,
        ViewMode::Mindmap,
        "Mind Map leaves to the document mind map"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// --- 6. Screenshots for owner review --------------------------------------------

/// Renders the toolbar to PNG files for review. Run with
/// `RMDV_TOOLBAR_SHOTS=<dir> cargo test --release --lib toolbar_screenshots -- --ignored`.
/// Headless tiny-skia; no window opens.
#[test]
#[ignore = "writes review screenshots; set RMDV_TOOLBAR_SHOTS"]
fn toolbar_screenshots() {
    use iced::advanced::renderer::Headless;
    let Some(dir) = std::env::var_os("RMDV_TOOLBAR_SHOTS").map(PathBuf::from) else {
        return;
    };
    std::fs::create_dir_all(&dir).expect("shot dir");
    {
        let fonts = iced::advanced::graphics::text::font_system();
        let mut fonts = fonts.write().expect("font system");
        for font in crate::fonts::BUNDLED {
            fonts.load_font(std::borrow::Cow::Borrowed(font));
        }
    }
    let scale = 2.0;
    let shoot = |app: &App, name: &str, height: f32| {
        let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            iced::Font::with_name("Inter"),
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .expect("headless tiny-skia renderer");
        let size = Size::new(WINDOW.width, height);
        let mut ui = UserInterface::build(app.view(), size, Cache::default(), &mut renderer);
        let pal = app.palette;
        ui.draw(
            &mut renderer,
            &app.theme(),
            &iced::advanced::renderer::Style { text_color: pal.fg },
            mouse::Cursor::Unavailable,
        );
        let (w, h) = ((size.width * scale) as u32, (size.height * scale) as u32);
        let rgba = renderer.screenshot(iced::Size::new(w, h), scale, pal.bg);
        let path = dir.join(name);
        image::RgbaImage::from_raw(w, h, rgba)
            .expect("rgba buffer")
            .save(&path)
            .expect("write png");
        eprintln!("wrote {}", path.display());
    };
    for (preset, tag) in [
        (crate::theme::ThemePreset::OneLight, "light"),
        (crate::theme::ThemePreset::OneDark, "dark"),
    ] {
        // Always mode, dirty file, sidebar hidden: Show Sidebar, name, dot,
        // the view switch with Read selected, Save enabled.
        let mut app = always(dirty_app());
        let _ = app.update(Message::ToolbarView(ToolbarView::Read));
        let _ = app.update(Message::SetTheme(preset));
        let _ = app.update(Message::SetGlass(crate::macos_vibrancy::Glass::Off));
        app.toast = None;
        shoot(&app, &format!("toolbar-always-dirty-{tag}.png"), 160.0);
        shoot(
            &app,
            &format!("window-always-dirty-{tag}.png"),
            WINDOW.height,
        );
        // On hover, revealed over edit mode.
        let mut app = on_hover(dirty_app());
        let _ = app.update(Message::SetTheme(preset));
        let _ = app.update(Message::SetGlass(crate::macos_vibrancy::Glass::Off));
        let _ = app.update(Message::ToolbarHover(true));
        app.toast = None;
        shoot(&app, &format!("toolbar-hover-edit-{tag}.png"), 160.0);
    }
    // Document mind map with its panel toggle, sidebar open.
    let (app, ws) = tree_fixture("toolbar-shots");
    let mut app = always(app);
    let alpha = ws.join("alpha.md");
    let _ = app.update(Message::FileLoaded(Ok((
        alpha,
        "# Alpha\n\n## One\n\n## Two\n".into(),
    ))));
    let _ = app.update(Message::SetTheme(crate::theme::ThemePreset::OneLight));
    let _ = app.update(Message::SetGlass(crate::macos_vibrancy::Glass::Off));
    let _ = app.update(Message::ToolbarView(ToolbarView::Mindmap));
    app.toast = None;
    shoot(
        &app,
        "window-always-mindmap-sidebar-light.png",
        WINDOW.height,
    );
    let _ = std::fs::remove_dir_all(&ws);
}

// --- Review fixes: the band must not cover top controls ------------------------

#[test]
fn hover_band_leaves_the_vault_search_bar_its_controls() {
    let (app, dir) = tree_fixture("toolbar-vault");
    let mut app = on_hover(app);
    let _ = app.update(Message::OpenVaultSearch);
    assert!(app.vault_open);
    let mut ui = PointerUi::new();
    let _ = ui.move_to(&app, Point::new(640.0, 400.0));
    let close = ui
        .id_bounds(&app, "vault-close-button")
        .expect("vault close button");
    assert!(close.y < TOOLBAR_BAND_H, "the vault bar sits at the top");
    let (messages, _) = ui.move_to(&app, center(close));
    assert!(
        !has(&messages, |m| matches!(m, Message::ToolbarHover(true))),
        "reaching for the vault's own Close does not reveal the toolbar: {messages:?}"
    );
    let messages = ui.click(&app, center(close));
    assert!(has(&messages, |m| matches!(m, Message::VaultClose)));
    apply(&mut app, messages);
    assert!(!app.vault_open);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn zen_editor_text_starts_below_the_toolbar_in_both_modes() {
    use crate::prefs::ToolbarMode;
    // On hover the band lies over the editor; Always lays the editor out
    // below the toolbar. Either way the first line clears the band.
    assert!(zen_editor_top_inset(ToolbarMode::OnHover) >= TOOLBAR_BAND_H);
    assert_eq!(
        zen_editor_top_inset(ToolbarMode::Always) + TOOLBAR_H,
        zen_editor_top_inset(ToolbarMode::OnHover),
        "switching modes does not move the editor text"
    );
}

// --- Review fixes: focus, padding, and stale hover --------------------------------

#[test]
fn always_toolbar_clicks_keep_the_zen_editor_focused() {
    use super::group2::apply_ui;
    let mut app = always(long_doc_app());
    let mut ui = PointerUi::new();
    let _ = ui.move_to(&app, Point::new(640.0, 400.0));
    apply_ui(&mut app, &mut ui, vec![Message::ToggleViewMode]);
    assert_eq!(ui.focused(&app, "zen-editor"), Some(true));
    let typed = ui.type_char(&app, 'x');
    apply_ui(&mut app, &mut ui, typed);
    assert!(app.dirty, "the typed key reached the editor");

    // Save, the current Edit segment, and the toolbar's blank space (the
    // file name). The messages are not applied: the press alone decides
    // whether the editor keeps its focus.
    let save = glyph_at(&mut ui, &app, ic::SAVE).expect("Save");
    let edit = ui.id_bounds(&app, "view-edit").expect("Edit segment");
    let name = ui.text_bounds(&app, "pointer-long.md").expect("file name");
    for (what, at) in [
        ("Save", center(save)),
        ("Edit", center(edit)),
        ("blank space", center(name)),
    ] {
        let _ = ui.click(&app, at);
        assert_eq!(
            ui.focused(&app, "zen-editor"),
            Some(true),
            "a click on the toolbar's {what} keeps the editor focused"
        );
        let typed = ui.type_char(&app, 'y');
        assert!(
            has(&typed, |m| matches!(m, Message::EditorAction(_))),
            "typing after a click on {what} reaches the editor: {typed:?}"
        );
    }
}

#[test]
fn always_mode_body_offset_matches_the_reader_padding() {
    let mut app = always(long_doc_app());
    let mut ui = PointerUi::new();
    let column = ui.id_bounds(&app, "reader-column").expect("reading column");
    for _ in 0..4 {
        let messages = ui.wheel(&app, Point::new(640.0, 400.0));
        apply(&mut app, messages);
    }
    let viewport = app.body_viewport.as_ref().expect("the reader scrolled");
    let offset = viewport.absolute_offset().y;
    assert!(offset > 100.0, "scrolled past the padding: {offset}");
    // The column's unscrolled top, relative to the scrollable, is the
    // content's real top padding.
    let pad = column.y - viewport.bounds().y;
    assert!(
        (app.body_offset() - (offset - pad)).abs() < 0.5,
        "body offset {} vs scroll offset {offset} minus padding {pad}",
        app.body_offset()
    );
}

/// `tree_fixture` with alpha.md open, the pointer in the hover band, and the
/// toolbar revealed.
fn revealed_in_band(tag: &str) -> (App, PathBuf, PointerUi) {
    let (app, dir) = tree_fixture(tag);
    let mut app = on_hover(app);
    let _ = app.update(Message::FileLoaded(Ok((
        dir.join("alpha.md"),
        "# Alpha\n".into(),
    ))));
    let mut ui = PointerUi::new();
    let _ = ui.move_to(&app, Point::new(800.0, 400.0));
    let (messages, _) = ui.move_to(&app, Point::new(800.0, 20.0));
    assert_eq!(hover_count(&messages, true), 1, "{messages:?}");
    apply(&mut app, messages);
    assert!(app.toolbar_revealed);
    (app, dir, ui)
}

#[test]
fn hover_toolbar_hides_after_vault_search_took_its_band() {
    let (mut app, dir, mut ui) = revealed_in_band("toolbar-stale-vault");
    apply(&mut app, vec![Message::OpenVaultSearch]);
    assert!(app.vault_open);
    // The pointer leaves while the vault page has no band.
    let messages = ui.move_to(&app, Point::new(800.0, 400.0)).0;
    apply(&mut app, messages);
    apply(&mut app, vec![Message::VaultClose]);
    let messages = ui.move_to(&app, Point::new(810.0, 400.0)).0;
    apply(&mut app, messages);
    assert!(
        ui.id_bounds(&app, "toolbar").is_none(),
        "the toolbar is not left open with the pointer elsewhere"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn hover_toolbar_hides_after_a_mode_switch_took_its_band() {
    let (mut app, dir, mut ui) = revealed_in_band("toolbar-stale-mode");
    apply(&mut app, vec![Message::SetToolbarMode(ToolbarMode::Always)]);
    let messages = ui.move_to(&app, Point::new(800.0, 400.0)).0;
    apply(&mut app, messages);
    apply(
        &mut app,
        vec![Message::SetToolbarMode(ToolbarMode::OnHover)],
    );
    let messages = ui.move_to(&app, Point::new(810.0, 400.0)).0;
    apply(&mut app, messages);
    assert!(
        ui.id_bounds(&app, "toolbar").is_none(),
        "the toolbar is not left open with the pointer elsewhere"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn hover_toolbar_follows_its_band_when_the_find_bar_moves_it() {
    // The find bar pushes the band down: a pointer left on the find bar
    // is outside it, and one moved into the band reveals it again.
    let (mut app, dir, mut ui) = revealed_in_band("toolbar-stale-find");
    apply(&mut app, vec![Message::ToggleSearch]);
    assert!(app.search_open);
    let messages = ui.move_to(&app, Point::new(801.0, 20.0)).0;
    apply(&mut app, messages);
    let messages = ui.move_to(&app, Point::new(802.0, 21.0)).0;
    apply(&mut app, messages);
    assert!(
        !app.toolbar_revealed,
        "a pointer on the find bar is not in the band"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// --- Polish (MDV-048): icon view switch, window-centered file name ----------

const VIEW_SEGMENTS: [&str; 4] = [
    "view-read",
    "view-edit",
    "view-mindmap",
    "view-full-mindmap",
];

#[test]
fn view_switch_segments_are_equal_width_icons() {
    let app = always(long_doc_app());
    let mut ui = PointerUi::new();
    for label in ["Read", "Edit", "Mind Map"] {
        assert!(
            ui.text_bounds(&app, label).is_none(),
            "{label} is an icon, not a word"
        );
    }
    let segments: Vec<Rectangle> = VIEW_SEGMENTS
        .iter()
        .map(|id| {
            ui.id_bounds(&app, id)
                .unwrap_or_else(|| panic!("{id} segment"))
        })
        .collect();
    for (id, code) in
        VIEW_SEGMENTS
            .iter()
            .zip([ic::BOOK_OPEN, ic::PENCIL_LINE, ic::GIT_FORK, ic::NETWORK])
    {
        let glyph = glyph_at(&mut ui, &app, code).unwrap_or_else(|| panic!("{id} glyph"));
        let segment = ui.id_bounds(&app, id).unwrap();
        assert!(
            segment.contains(glyph.center()),
            "{id}: glyph {glyph:?} inside {segment:?}"
        );
    }
    let first = segments[0];
    for (id, s) in VIEW_SEGMENTS.iter().zip(&segments) {
        assert!(
            (s.width - first.width).abs() < 0.5 && (s.height - first.height).abs() < 0.5,
            "{id} is as large as Read: {s:?} vs {first:?}"
        );
    }
    // The four glyphs are distinct, so no two modes look alike.
    let codes = [ic::BOOK_OPEN, ic::PENCIL_LINE, ic::GIT_FORK, ic::NETWORK];
    for (i, a) in codes.iter().enumerate() {
        assert!(!codes[i + 1..].contains(a));
    }
}

fn app_with_file(name: &str) -> App {
    let mut app = App::default();
    let _ = app.update(Message::FileLoaded(Ok((
        PathBuf::from(format!("/tmp/{name}")),
        "# Title\n\nBody.\n".into(),
    ))));
    always(app)
}

#[test]
fn file_name_sits_at_the_window_center() {
    let app = app_with_file("centered.md");
    let mut ui = PointerUi::sized(Size::new(1000.0, 700.0));
    let name = ui.text_bounds(&app, "centered.md").expect("file name");
    assert!(
        (name.center_x() - 500.0).abs() <= 1.0,
        "centered on the window: {name:?}"
    );
}

#[test]
fn file_name_follows_the_window_center_beside_the_sidebar() {
    let (app, dir) = tree_fixture("toolbar-center");
    let mut app = always(app);
    // A name the sidebar's tree does not show too.
    let _ = app.update(Message::FileLoaded(Ok((
        PathBuf::from("/tmp/centered-beside.md"),
        "# Beside\n".into(),
    ))));
    app.window_size = Some(Size::new(1000.0, 700.0));
    assert!(app.sidebar_open);
    let mut ui = PointerUi::sized(Size::new(1000.0, 700.0));
    let name = ui
        .text_bounds(&app, "centered-beside.md")
        .expect("file name");
    let bar = ui.id_bounds(&app, "toolbar").expect("toolbar");
    assert!(
        bar.x > 100.0,
        "the toolbar starts right of the sidebar: {bar:?}"
    );
    assert!(
        (name.center_x() - 500.0).abs() <= 1.0,
        "centered on the window, not on the toolbar: {name:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn long_file_name_never_covers_the_toolbar_controls() {
    let long = format!("{}.md", "n".repeat(57));
    assert_eq!(long.chars().count(), 60);
    for dirty in [false, true] {
        let mut app = app_with_file(&long);
        if dirty {
            app.dirty = true;
        }
        let mut ui = PointerUi::sized(Size::new(640.0, 600.0));
        let name = ui.text_bounds(&app, &long).expect("file name");
        let mut controls: Vec<(String, Rectangle)> = VIEW_SEGMENTS
            .iter()
            .map(|id| (id.to_string(), ui.id_bounds(&app, id).expect("segment")))
            .collect();
        for code in [ic::CHEVRON_LEFT, ic::CHEVRON_RIGHT] {
            let b = glyph_at(&mut ui, &app, code).expect("chevron");
            // The chevron's button is wider than its glyph.
            controls.push((format!("{code:?}"), b.expand(8.0)));
        }
        let mut shown = vec![("name".to_string(), name)];
        if dirty {
            shown.push((
                "dot".into(),
                ui.id_bounds(&app, "toolbar-dirty-dot").expect("dirty dot"),
            ));
        }
        for (what, b) in &shown {
            assert!(b.width > 0.0, "{what} shows: {b:?}");
            for (control, c) in &controls {
                assert!(
                    b.intersection(c).is_none(),
                    "dirty={dirty}: {what} {b:?} covers {control} {c:?}"
                );
            }
        }
    }
}

#[test]
fn window_size_is_known_from_the_moment_the_window_opens() {
    // The name is centered on the window before any resize.
    let mut app = App::default();
    assert_eq!(app.window_size, None);
    let _ = app.update(Message::WindowOpened(
        iced::window::Id::unique(),
        Size::new(900.0, 600.0),
        None,
    ));
    assert_eq!(app.window_size, Some(Size::new(900.0, 600.0)));
}
