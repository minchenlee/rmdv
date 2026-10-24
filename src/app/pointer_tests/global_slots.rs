//! Global Quick Slots (MDV-048 P8b): one bank of nine for every workspace.
//! A slot keeps the workspace root it was assigned in; pressing a slot from
//! another workspace opens that workspace, then the file with its context.

use super::*;

/// Two workspaces, `a` (with `a.md` and `same.md`) and `b` (with `b.md` and
/// `same.md`), both canonical. The app shows `a` with nothing open.
fn two_workspaces(tag: &str) -> (App, PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!("rmdv-gslots-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let a = base.join("notes-a");
    let b = base.join("notes-b");
    for (dir, own) in [(&a, "a.md"), (&b, "b.md")] {
        std::fs::create_dir_all(dir).expect("fixture dir");
        std::fs::write(dir.join(own), format!("# {own}\n\nText.\n")).expect("own file");
        std::fs::write(dir.join("same.md"), "# Same\n").expect("same.md");
    }
    let a = canonicalize_existing_path(a);
    let b = canonicalize_existing_path(b);
    let mut app = App::default();
    app.set_workspace(a.clone(), true);
    (app, a, b)
}

fn cleanup(app: &App, a: &Path) {
    let _ = std::fs::remove_file(app.quick_slots_persistence_path.clone().unwrap());
    let _ = std::fs::remove_dir_all(a.parent().unwrap());
}

fn open(app: &mut App, path: &Path) {
    let source = std::fs::read_to_string(path).unwrap();
    let _ = app.update(Message::FileLoaded(Ok((path.to_path_buf(), source))));
    assert_eq!(app.file.as_deref(), Some(path));
}

fn slot_path(app: &App, index: usize) -> Option<&str> {
    app.quick_slots
        .occupied(index)
        .map(|slot| slot.relative_path.as_str())
}

fn toast(app: &App) -> Option<&str> {
    app.toast.as_ref().map(|toast| toast.text.as_str())
}

fn title(app: &App, index: u8) -> Option<String> {
    app.menu_titles()
        .into_iter()
        .find(|(tag, _)| *tag == super::super::menu_bar::MenuCommand::QuickSlot(index).tag())
        .map(|(_, title)| title)
}

fn folder_name(dir: &Path) -> String {
    dir.file_name().unwrap().to_string_lossy().into_owned()
}

/// Assign `file` (in the open workspace) to slot `index` from the reader.
fn assign(app: &mut App, file: &Path, index: usize) {
    open(app, file);
    let _ = app.update(Message::QuickSlotAssign(index));
    assert!(
        app.quick_slots.occupied(index).is_some(),
        "{:?}",
        toast(app)
    );
}

#[test]
fn a_slot_stays_when_another_workspace_opens() {
    let (mut app, a, b) = two_workspaces("stays");
    assign(&mut app, &a.join("a.md"), 0);

    let _ = app.update(Message::OpenWorkspace(b.clone()));

    assert_eq!(app.workspace.as_deref(), Some(b.as_path()));
    assert_eq!(
        slot_path(&app, 0),
        Some("a.md"),
        "one bank for every workspace"
    );
    // A slot assigned in `b` joins the same bank.
    assign(&mut app, &b.join("b.md"), 1);
    let _ = app.update(Message::OpenWorkspace(a.clone()));
    assert_eq!(slot_path(&app, 0), Some("a.md"));
    assert_eq!(slot_path(&app, 1), Some("b.md"));
    cleanup(&app, &a);
}

#[test]
fn a_slot_from_another_workspace_opens_that_workspace_then_its_file() {
    let (mut app, a, b) = two_workspaces("cross-open");
    let a_md = a.join("a.md");
    open(&mut app, &a_md);
    // Document Mindmap with its panel open is the saved context.
    app.view_mode = ViewMode::Mindmap;
    app.mindmap_panel_open = true;
    let _ = app.update(Message::QuickSlotAssign(0));
    assert_eq!(slot_path(&app, 0), Some("a.md"));
    let _ = app.update(Message::OpenWorkspace(b.clone()));
    app.view_mode = ViewMode::Rendered;
    app.mindmap_panel_open = false;
    open(&mut app, &b.join("b.md"));

    let _ = app.update(Message::QuickSlotActivate(0));

    assert_eq!(
        app.workspace.as_deref(),
        Some(a.as_path()),
        "the slot's workspace opens first ({:?})",
        toast(&app)
    );
    assert!(app.sidebar_open, "as Open Folder does");
    let pending = app
        .pending_quick_slot_restore
        .clone()
        .expect("then the slot's file loads");
    assert_eq!(pending.index, 0);
    let _ = app.update(Message::QuickSlotFileLoaded {
        index: 0,
        slot: pending.slot.clone(),
        result: Ok((a_md.clone(), std::fs::read_to_string(&a_md).unwrap())),
    });
    assert_eq!(app.file.as_deref(), Some(a_md.as_path()));
    assert_eq!(app.view_mode, ViewMode::Mindmap, "with its saved context");
    assert!(app.mindmap_panel_open);
    assert_eq!(app.quick_slots.active, Some(0));
    cleanup(&app, &a);
}

#[test]
fn a_slot_from_this_workspace_opens_as_before() {
    let (mut app, a, _b) = two_workspaces("same-open");
    assign(&mut app, &a.join("a.md"), 0);
    open(&mut app, &a.join("same.md"));
    let epoch = app.workspace_epoch;

    let _ = app.update(Message::QuickSlotActivate(0));

    assert_eq!(app.workspace.as_deref(), Some(a.as_path()));
    assert_eq!(app.workspace_epoch, epoch, "the workspace is not reopened");
    assert_eq!(
        app.pending_quick_slot_restore
            .as_ref()
            .map(|pending| pending.index),
        Some(0)
    );
    assert_eq!(app.quick_slots.active, Some(0));
    cleanup(&app, &a);
}

#[test]
fn a_slot_whose_file_or_workspace_is_gone_shows_missing() {
    let (mut app, a, b) = two_workspaces("missing");
    assign(&mut app, &a.join("a.md"), 0);
    assign(&mut app, &a.join("same.md"), 1);
    let _ = app.update(Message::OpenWorkspace(b.clone()));
    open(&mut app, &b.join("b.md"));

    // The file is gone; its workspace is still there.
    std::fs::remove_file(a.join("a.md")).unwrap();
    let _ = app.update(Message::QuickSlotActivate(0));
    assert_eq!(app.workspace.as_deref(), Some(b.as_path()), "nothing opens");
    assert_eq!(toast(&app), Some("Quick Slot 1 is missing: a.md"));
    assert_eq!(app.quick_slots.active, Some(0));
    assert_eq!(app.file.as_deref(), Some(b.join("b.md").as_path()));

    // The whole workspace is gone.
    std::fs::remove_dir_all(&a).unwrap();
    let _ = app.update(Message::QuickSlotActivate(1));
    assert_eq!(app.workspace.as_deref(), Some(b.as_path()));
    assert_eq!(toast(&app), Some("Quick Slot 2 is missing: same.md"));
    assert_eq!(app.quick_slots.active, Some(1));
    assert_eq!(slot_path(&app, 1), Some("same.md"), "the slot stays");
    cleanup(&app, &a);
}

#[test]
fn renaming_a_file_leaves_another_workspaces_slot_alone() {
    let (mut app, a, b) = two_workspaces("rename");
    assign(&mut app, &a.join("same.md"), 0);
    let _ = app.update(Message::OpenWorkspace(b.clone()));
    assign(&mut app, &b.join("same.md"), 1);
    open(&mut app, &b.join("b.md"));

    let _ = app.update(Message::TreeRenameStart(b.join("same.md")));
    let _ = app.update(Message::TreeRenameInput("moved.md".into()));
    let _ = app.update(Message::TreeRenameCommit);

    assert!(b.join("moved.md").is_file(), "{:?}", toast(&app));
    assert_eq!(
        slot_path(&app, 0),
        Some("same.md"),
        "a's slot is not b's file"
    );
    assert_eq!(slot_path(&app, 1), Some("moved.md"), "b's slot follows it");
    cleanup(&app, &a);
}

#[test]
fn menu_titles_name_the_workspace_of_a_slot_from_another_workspace() {
    let (mut app, a, b) = two_workspaces("titles");
    assign(&mut app, &a.join("a.md"), 0);
    let _ = app.update(Message::OpenWorkspace(b.clone()));
    assign(&mut app, &b.join("b.md"), 1);

    assert_eq!(
        title(&app, 0),
        Some(format!("Slot 1: a.md — {}", folder_name(&a)))
    );
    assert_eq!(title(&app, 1).as_deref(), Some("Slot 2: b.md"));
    let state = app.menu_state();
    let tag = super::super::menu_bar::MenuCommand::QuickSlot(0).tag();
    assert!(state.is_enabled(tag), "a slot from another workspace opens");
    cleanup(&app, &a);
}

#[test]
fn the_global_bank_seeds_once_from_the_most_recent_workspace() {
    let base = std::env::temp_dir().join(format!("rmdv-gslots-seed-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let (a, b) = (base.join("notes-a"), base.join("notes-b"));
    for dir in [&a, &b] {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("a.md"), "# A\n").unwrap();
        std::fs::write(dir.join("b.md"), "# B\n").unwrap();
    }
    let (a, b) = (canonicalize_existing_path(a), canonicalize_existing_path(b));
    let legacy = |relative: &str, index: usize| {
        let mut bank = crate::quick_slots::WorkspaceSlots::default();
        let slot: crate::quick_slots::QuickSlot =
            serde_json::from_value(serde_json::json!({ "relative_path": relative })).unwrap();
        bank.set(index, slot);
        bank
    };
    let mut app = App::default();
    app.prefs.quick_slots.put_bank(&a, legacy("a.md", 0));
    app.prefs.quick_slots.put_bank(&b, legacy("b.md", 2));
    let banks = app.prefs.quick_slots.clone();
    // `b` was opened last; launch seeds the bank, then `a` opens.
    app.recent.folders = vec![b.clone(), a.clone()];
    app.seed_global_quick_slots();
    app.set_workspace(a.clone(), true);

    assert_eq!(slot_path(&app, 2), Some("b.md"), "the most recent bank");
    assert_eq!(slot_path(&app, 0), None, "banks are never merged");
    // The slot is b's: from a it opens b.
    let _ = app.update(Message::QuickSlotActivate(2));
    assert_eq!(app.workspace.as_deref(), Some(b.as_path()));

    // Seeding happens once: another workspace does not reseed.
    let _ = app.update(Message::OpenWorkspace(a.clone()));
    assert_eq!(slot_path(&app, 2), Some("b.md"));
    assert_eq!(slot_path(&app, 0), None);
    // Assigning writes the global bank, not the old per-workspace map.
    open(&mut app, &a.join("b.md"));
    let _ = app.update(Message::QuickSlotAssign(5));
    assert_eq!(slot_path(&app, 5), Some("b.md"));
    assert_eq!(
        app.prefs.quick_slots, banks,
        "the old banks stay as they were"
    );
    let _ = std::fs::remove_file(app.quick_slots_persistence_path.clone().unwrap());
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn without_a_recent_bank_the_first_workspace_seeds_the_global_bank() {
    let base = std::env::temp_dir().join(format!("rmdv-gslots-launch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let (first, second) = (base.join("first"), base.join("second"));
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    std::fs::write(first.join("a.md"), "# A\n").unwrap();
    std::fs::write(second.join("b.md"), "# B\n").unwrap();
    let (first, second) = (
        canonicalize_existing_path(first),
        canonicalize_existing_path(second),
    );
    let mut app = App::default();
    let mut bank = crate::quick_slots::WorkspaceSlots::default();
    bank.set(
        0,
        serde_json::from_value(serde_json::json!({ "relative_path": "a.md" })).unwrap(),
    );
    app.prefs.quick_slots.put_bank(&first, bank);

    app.set_workspace(first.clone(), false);
    assert_eq!(
        slot_path(&app, 0),
        Some("a.md"),
        "the launch workspace's bank"
    );
    app.set_workspace(second.clone(), false);
    assert_eq!(
        slot_path(&app, 0),
        Some("a.md"),
        "still there in another workspace"
    );
    let _ = app.update(Message::QuickSlotActivate(0));
    assert_eq!(
        app.workspace.as_deref(),
        Some(first.as_path()),
        "it opens first"
    );
    let _ = std::fs::remove_file(app.quick_slots_persistence_path.clone().unwrap());
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn in_full_mindmap_a_slot_from_another_workspace_waits_for_its_index() {
    let (mut app, a, b) = two_workspaces("full-mindmap");
    let a_md = a.join("a.md");
    assign(&mut app, &a_md, 0);
    let _ = app.update(Message::OpenWorkspace(b.clone()));
    open(&mut app, &b.join("b.md"));
    let _ = app.update(Message::ToggleFullMindmap);
    assert!(app.full_mindmap.is_some());

    let _ = app.update(Message::QuickSlotActivate(0));

    // Full Mindmap opens the slot's workspace through its own bounded index.
    let request = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_workspace_load.clone())
        .expect("the slot's workspace is being indexed");
    assert_eq!(request.path, a);
    assert!(app.pending_quick_slot_switch.is_some(), "the slot waits");
    assert_eq!(app.workspace.as_deref(), Some(b.as_path()));
    let snapshot = tree::build_workspace(&request.path, app.show_hidden).unwrap();
    let _ = app.update(Message::FullMindmapWorkspaceLoaded {
        request,
        result: Ok((a.clone(), snapshot)),
    });

    assert_eq!(app.workspace.as_deref(), Some(a.as_path()));
    assert!(app.pending_quick_slot_switch.is_none());
    assert_eq!(app.quick_slots.active, Some(0), "then the slot activates");
    let pending = app
        .pending_quick_slot_restore
        .clone()
        .expect("its file loads");
    assert_eq!(pending.slot.resolve().as_deref(), Some(a_md.as_path()));
    cleanup(&app, &a);
}

#[test]
fn opening_another_workspace_meanwhile_drops_the_waiting_slot() {
    let (mut app, a, b) = two_workspaces("superseded");
    assign(&mut app, &a.join("a.md"), 0);
    let _ = app.update(Message::OpenWorkspace(b.clone()));
    let _ = app.update(Message::ToggleFullMindmap);
    // Assigning marked slot 1 active. Start with no active slot, so an
    // activation would show in the marker.
    app.quick_slots.active = None;
    let _ = app.update(Message::QuickSlotActivate(0));
    assert!(app.pending_quick_slot_switch.is_some());
    assert_eq!(
        app.quick_slots.active, None,
        "the slot waits, not active yet"
    );

    let other = a.parent().unwrap().join("notes-c");
    std::fs::create_dir_all(&other).unwrap();
    let other = canonicalize_existing_path(other);
    let _ = app.update(Message::OpenWorkspace(other.clone()));
    if let Some(request) = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_workspace_load.clone())
    {
        let snapshot = tree::build_workspace(&request.path, app.show_hidden).unwrap();
        let _ = app.update(Message::FullMindmapWorkspaceLoaded {
            request,
            result: Ok((other.clone(), snapshot)),
        });
    }

    assert_eq!(app.workspace.as_deref(), Some(other.as_path()));
    assert!(app.pending_quick_slot_switch.is_none());
    assert_eq!(app.quick_slots.active, None, "the slot did not activate");
    assert!(
        app.pending_quick_slot_restore.is_none(),
        "and its file does not load"
    );
    cleanup(&app, &a);
}

// --- P8b review fixes ---------------------------------------------------------

/// `b`'s `b.md` as a Full Mindmap slot: pressing it keeps Full Mindmap open.
fn full_mindmap_slot(b: &Path) -> crate::quick_slots::QuickSlot {
    crate::quick_slots::QuickSlot {
        root: b.to_string_lossy().into_owned(),
        relative_path: "b.md".into(),
        context: crate::quick_slots::SlotContext {
            mode: crate::quick_slots::SlotMode::FullMindmap,
            ..Default::default()
        },
    }
}

/// In Full Mindmap in `b`, slot 1 (from `a`) waits for `a`'s index.
fn waiting_for_a(tag: &str) -> (App, PathBuf, PathBuf) {
    let (mut app, a, b) = two_workspaces(tag);
    assign(&mut app, &a.join("a.md"), 0);
    let _ = app.update(Message::OpenWorkspace(b.clone()));
    assert!(app.quick_slots.set(1, full_mindmap_slot(&b)));
    let _ = app.update(Message::ToggleFullMindmap);
    app.quick_slots.active = None;
    let _ = app.update(Message::QuickSlotActivate(0));
    assert!(app.pending_quick_slot_switch.is_some(), "slot 1 waits");
    (app, a, b)
}

/// `a`'s Full Mindmap index lands.
fn land_a(app: &mut App, a: &Path) {
    let request = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_workspace_load.clone())
        .expect("a is being indexed");
    assert_eq!(request.path, a);
    let snapshot = tree::build_workspace(&request.path, app.show_hidden).unwrap();
    let _ = app.update(Message::FullMindmapWorkspaceLoaded {
        request,
        result: Ok((a.to_path_buf(), snapshot)),
    });
}

#[test]
fn a_newer_slot_press_cancels_the_waiting_slot() {
    let (mut app, a, _b) = waiting_for_a("newer-press");

    // The user presses slot 2, a Full Mindmap slot of this workspace.
    let _ = app.update(Message::QuickSlotActivate(1));
    assert!(app.full_mindmap.is_some());
    assert!(
        app.pending_quick_slot_switch.is_none(),
        "the newest press wins"
    );
    land_a(&mut app, &a);

    assert_ne!(app.quick_slots.active, Some(0), "slot 1 does not activate");
    assert_ne!(
        app.pending_quick_slot_restore
            .as_ref()
            .map(|pending| pending.index),
        Some(0)
    );
    cleanup(&app, &a);
}

#[test]
fn clearing_or_assigning_a_slot_cancels_the_waiting_slot() {
    let (mut app, a, _b) = waiting_for_a("newer-clear");
    let _ = app.update(Message::QuickSlotClear(1));
    assert!(
        app.pending_quick_slot_switch.is_none(),
        "a clear cancels it"
    );
    land_a(&mut app, &a);
    assert_eq!(app.quick_slots.active, None, "slot 1 does not activate");
    cleanup(&app, &a);

    let (mut app, a, b) = waiting_for_a("newer-assign");
    // The file tree's Add to Quick Slot takes the first free slot, 3.
    let _ = app.update(Message::QuickSlotAssignPath(b.join("same.md")));
    assert!(app.quick_slots.occupied(2).is_some(), "{:?}", toast(&app));
    assert!(
        app.pending_quick_slot_switch.is_none(),
        "an assignment cancels it"
    );
    land_a(&mut app, &a);
    assert_ne!(app.quick_slots.active, Some(0), "slot 1 does not activate");
    cleanup(&app, &a);
}

#[test]
fn with_no_workspace_open_the_toolbar_popover_opens_a_slot() {
    let (mut app, a, _b) = two_workspaces("welcome");
    assign(&mut app, &a.join("a.md"), 2);
    let mut welcome = App::default();
    welcome.quick_slots = app.quick_slots.clone();
    welcome.quick_slots.active = None;
    assert!(welcome.workspace.is_none() && welcome.file.is_none());

    let mut ui = PointerUi::new();
    let button = ui
        .id_bounds(&welcome, "toolbar-quick-slots")
        .expect("the toolbar button");
    let messages = ui.click(&welcome, center(button));
    apply(&mut welcome, messages);
    assert!(
        welcome.quick_slots_pinned,
        "the popover opens with no workspace"
    );
    let popover = ui
        .id_bounds(&welcome, "quick-slots-popover")
        .expect("the popover shows");
    let chip = ui.text_bounds(&welcome, "3").expect("slot 3's chip");
    assert!(popover.contains(center(chip)));
    let messages = ui.click(&welcome, center(chip));
    apply(&mut welcome, messages);

    assert_eq!(
        welcome.workspace.as_deref(),
        Some(a.as_path()),
        "the slot's workspace opens ({:?})",
        toast(&welcome)
    );
    let _ = std::fs::remove_file(welcome.quick_slots_persistence_path.clone().unwrap());
    cleanup(&app, &a);
}

#[test]
fn the_replace_confirmation_names_the_workspace_of_the_old_slot() {
    let (mut app, a, b) = two_workspaces("replace-names");
    assign(&mut app, &a.join("same.md"), 2);
    let _ = app.update(Message::OpenWorkspace(b.clone()));
    open(&mut app, &b.join("same.md"));

    let _ = app.update(Message::QuickSlotAssign(2));

    assert!(app.quick_slot_replace.is_some(), "another workspace's file");
    let mut ui = PointerUi::new();
    let body = format!(
        "Slot 3 holds same.md — {}. Replace it with same.md?",
        folder_name(&a)
    );
    assert!(ui.text_bounds(&app, &body).is_some(), "{body:?}");
    let _ = app.update(Message::QuickSlotReplaceCancel);

    // A slot of this workspace keeps the plain text.
    assign(&mut app, &b.join("b.md"), 3);
    open(&mut app, &b.join("same.md"));
    let _ = app.update(Message::QuickSlotAssign(3));
    let body = "Slot 4 holds b.md. Replace it with same.md?";
    assert!(ui.text_bounds(&app, body).is_some(), "{body:?}");
    cleanup(&app, &a);
}
