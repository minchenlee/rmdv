use super::image_cache::image_state_cost;
use super::*;
use crate::ast::{Block, DiagramKind, ListItem};

#[test]
fn quick_slot_physical_digits_and_arrows_are_layout_safe() {
    use iced::keyboard::key::{Code, Physical};

    assert_eq!(
        quick_slot_digit_index(Physical::Code(Code::Digit1)),
        Some(0)
    );
    assert_eq!(
        quick_slot_digit_index(Physical::Code(Code::Digit9)),
        Some(8)
    );
    assert_eq!(quick_slot_digit_index(Physical::Code(Code::Digit0)), None);
    assert_eq!(
        quick_slot_cycle_delta(Physical::Code(Code::ArrowUp)),
        Some(-1)
    );
    assert_eq!(
        quick_slot_cycle_delta(Physical::Code(Code::ArrowDown)),
        Some(1)
    );
    assert_eq!(
        quick_slot_cycle_delta(Physical::Code(Code::BracketLeft)),
        None
    );
    assert_eq!(
        quick_slot_cycle_delta(Physical::Code(Code::BracketRight)),
        None
    );
    assert_eq!(quick_slot_cycle_delta(Physical::Code(Code::Slash)), None);
}

#[test]
fn quick_slot_primary_modifier_matches_platform_command() {
    use iced::keyboard::Modifiers;

    assert!(quick_slots_primary_modifier(Modifiers::COMMAND));
    #[cfg(target_os = "macos")]
    assert!(!quick_slots_primary_modifier(Modifiers::CTRL));
    #[cfg(not(target_os = "macos"))]
    assert!(!quick_slots_primary_modifier(Modifiers::LOGO));
}

#[test]
fn clean_zen_quick_slot_shortcuts_and_rail_yield_to_editor() {
    assert!(quick_slots_shortcuts_enabled(
        false, false, false, true, false
    ));
    assert!(!quick_slots_shortcuts_enabled(
        false, false, false, true, true
    ));
    assert!(!quick_slots_shortcuts_enabled(
        true, false, false, false, false
    ));
    assert!(!quick_slots_shortcuts_enabled(
        false, true, false, false, false
    ));
    assert!(!quick_slots_shortcuts_enabled(
        false, false, true, false, false
    ));
    assert!(quick_slots_shortcuts_enabled(
        false, false, false, false, false
    ));

    assert!(quick_slots_rail_visible(
        true, false, false, false, true, false
    ));
    assert!(!quick_slots_rail_visible(
        true, false, false, false, true, true
    ));
    assert!(!quick_slots_rail_visible(
        true, true, false, false, false, false
    ));
    assert!(quick_slots_rail_visible(
        true, false, false, false, false, false
    ));
}

#[test]
fn clean_zen_quick_slot_arrows_yield_to_editor_native_motion() {
    use iced::keyboard::{key::Code, key::Physical, Modifiers};

    let command = Modifiers::COMMAND;
    // Clean Zen keeps the rail and explicit slot digits available while
    // editor-native arrows continue to own document/line motion.
    let quick_slots_allowed = true;
    assert!(quick_slot_physical_message(
        Physical::Code(Code::ArrowUp),
        command,
        quick_slots_allowed,
        true,
        true,
    )
    .is_none());
    assert!(quick_slot_physical_message(
        Physical::Code(Code::ArrowDown),
        command,
        quick_slots_allowed,
        true,
        true,
    )
    .is_none());
    assert!(quick_slot_physical_message(
        Physical::Code(Code::ArrowLeft),
        command,
        quick_slots_allowed,
        true,
        true,
    )
    .is_none());
    assert!(quick_slot_physical_message(
        Physical::Code(Code::Digit1),
        command,
        quick_slots_allowed,
        true,
        true,
    )
    .is_some_and(|message| matches!(message, Message::QuickSlotActivate(0))));
    assert!(quick_slot_physical_message(
        Physical::Code(Code::Digit1),
        command | Modifiers::ALT,
        quick_slots_allowed,
        true,
        true,
    )
    .is_none());
    assert!(quick_slot_physical_message(
        Physical::Code(Code::KeyN),
        command,
        quick_slots_allowed,
        true,
        true,
    )
    .is_some_and(|message| matches!(message, Message::QuickSlotNew)));
    assert!(matches!(
        quick_slot_physical_message(
            Physical::Code(Code::KeyW),
            command,
            quick_slots_allowed,
            true,
            true,
        ),
        Some(Message::QuickSlotClose)
    ));
    assert!(matches!(
        quick_slot_physical_message(
            Physical::Code(Code::KeyW),
            command | Modifiers::SHIFT,
            quick_slots_allowed,
            true,
            true,
        ),
        Some(Message::QuickSlotCloseWindow)
    ));

    // A dirty Zen buffer keeps editor-native digit chords intact too;
    // slot actions must not steal them or emit a toast.
    let dirty_quick_slots_allowed = false;
    assert!(quick_slot_physical_message(
        Physical::Code(Code::Digit1),
        command,
        dirty_quick_slots_allowed,
        true,
        true,
    )
    .is_none());
    assert!(quick_slot_physical_message(
        Physical::Code(Code::Digit1),
        command | Modifiers::ALT,
        dirty_quick_slots_allowed,
        true,
        true,
    )
    .is_none());
    assert!(quick_slot_physical_message(
        Physical::Code(Code::KeyN),
        command,
        dirty_quick_slots_allowed,
        true,
        true,
    )
    .is_none());
}

#[test]
fn quick_slot_navigation_and_new_tab_remain_available_outside_editor() {
    use iced::keyboard::{key::Code, key::Physical, Modifiers};

    let command = Modifiers::COMMAND;
    for _surface in ["rendered", "document-mindmap", "full-mindmap"] {
        assert!(matches!(
            quick_slot_physical_message(Physical::Code(Code::ArrowUp), command, true, false, true,),
            Some(Message::QuickSlotCycle(-1))
        ));
        assert!(matches!(
            quick_slot_physical_message(
                Physical::Code(Code::ArrowDown),
                command,
                true,
                false,
                true,
            ),
            Some(Message::QuickSlotCycle(1))
        ));
        assert!(matches!(
            quick_slot_physical_message(Physical::Code(Code::KeyN), command, true, false, true,),
            Some(Message::QuickSlotNew)
        ));
        assert!(quick_slot_physical_message(
            Physical::Code(Code::ArrowLeft),
            command,
            true,
            false,
            true,
        )
        .is_none());
    }
}

#[test]
fn quick_slots_rail_offsets_around_sidebar_and_full_mindmap() {
    assert_eq!(
        quick_slots_rail_left_offset(true, SIDEBAR_WIDTH, false),
        SIDEBAR_WIDTH + QUICK_SLOTS_RAIL_GAP
    );
    assert_eq!(
        quick_slots_rail_left_offset(true, SIDEBAR_WIDTH + 95.0, false),
        SIDEBAR_WIDTH + 95.0 + QUICK_SLOTS_RAIL_GAP
    );
    assert_eq!(
        quick_slots_rail_left_offset(false, SIDEBAR_WIDTH, false),
        QUICK_SLOTS_RAIL_GAP
    );
    assert_eq!(
        quick_slots_rail_left_offset(true, SIDEBAR_WIDTH, true),
        QUICK_SLOTS_RAIL_GAP
    );
}

#[test]
fn window_unfocused_clears_quick_slots_modifier() {
    let mut app = App::default();
    app.quick_slots_modifier_held = true;
    app.quick_slots_rail_revealed = true;
    let generation = app.quick_slots_modifier_generation;
    let _ = app.update(Message::WindowUnfocused(iced::window::Id::unique()));
    assert!(!app.quick_slots_modifier_held);
    assert!(!app.quick_slots_rail_revealed);
    assert_ne!(app.quick_slots_modifier_generation, generation);
}

#[test]
fn quick_slots_modifier_rail_reveal_is_delayed_and_cancelable() {
    let mut app = App::default();
    let initial_generation = app.quick_slots_modifier_generation;

    let _ = app.update(Message::QuickSlotsModifier(true));
    assert!(app.quick_slots_modifier_held);
    assert!(!app.quick_slots_rail_revealed);
    let generation = app.quick_slots_modifier_generation;
    assert_ne!(generation, initial_generation);

    // Duplicate platform events must not restart the reveal timer.
    let _ = app.update(Message::QuickSlotsModifier(true));
    assert_eq!(app.quick_slots_modifier_generation, generation);

    let _ = app.update(Message::QuickSlotsModifierReveal(generation));
    assert!(app.quick_slots_rail_revealed);

    let _ = app.update(Message::QuickSlotsModifier(false));
    assert!(!app.quick_slots_modifier_held);
    assert!(!app.quick_slots_rail_revealed);
    assert_ne!(app.quick_slots_modifier_generation, generation);

    // A stale timer from the prior press cannot bring the rail back.
    let _ = app.update(Message::QuickSlotsModifierReveal(generation));
    assert!(!app.quick_slots_rail_revealed);
}

#[test]
fn app_tests_use_isolated_quick_slot_persistence() {
    let app = App::default();
    let isolated = app
        .quick_slots_persistence_path
        .as_ref()
        .expect("tests must inject an isolated persistence path");
    assert!(isolated.starts_with(std::env::temp_dir()));
    assert_ne!(
        Some(isolated.clone()),
        crate::prefs::production_store_path_for_tests()
    );
}

fn quick_slot_restore_test_app(label: &str) -> (App, PathBuf, PathBuf) {
    let root = full_mindmap_test_dir(label);
    let old_file = root.join("old.md");
    let new_file = root.join("new.md");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(&old_file, "# Old\n").unwrap();
    std::fs::write(&new_file, "# New\n").unwrap();
    let mut app = App::default();
    app.set_workspace(root.clone(), false);
    (app, old_file, new_file)
}

#[test]
fn quick_slot_activation_rejects_older_generic_load_and_refresh() {
    let (mut app, old_file, _new_file) = quick_slot_restore_test_app("slot-supersedes");
    app.file = Some(old_file.clone());
    app.source = "# Old\n".into();
    app.saved_source = app.source.clone();
    app.quick_slots.set(
        0,
        crate::quick_slots::QuickSlot {
            relative_path: "new.md".into(),
            context: crate::quick_slots::SlotContext::default(),
        },
    );
    let other = old_file.with_file_name("other.md");
    let _ = app.load_file_unless_dirty(other.clone());
    let generic_generation = app.file_refresh_generation;
    let _ = app.update(Message::Refresh);
    let refresh_request = app
        .pending_refresh_file
        .clone()
        .expect("refresh should own the current file");

    let _ = app.update(Message::QuickSlotActivate(0));
    assert_ne!(app.file_refresh_generation, refresh_request.generation);

    let _ = app.update(Message::FileLoadCompleted {
        generation: generic_generation,
        result: Ok((other, "stale generic load".into())),
    });
    let _ = app.update(Message::RefreshFileLoaded {
        request: refresh_request,
        result: Ok((old_file.clone(), "stale refresh".into())),
    });
    assert_eq!(app.file.as_deref(), Some(old_file.as_path()));
    assert_eq!(app.source, "# Old\n");

    let isolated = app.quick_slots_persistence_path.clone().unwrap();
    let root = app.workspace.take().unwrap();
    let _ = std::fs::remove_file(isolated);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn new_quick_slot_activates_same_file_without_overwriting_context() {
    let (mut app, old_file, _new_file) = quick_slot_restore_test_app("new-slot-existing");
    app.file = Some(old_file.clone());
    app.source = "# Old\n".into();
    app.saved_source = app.source.clone();
    app.view_mode = ViewMode::Mindmap;
    // Simulate a legacy persisted bank that retained a harmless CurDir
    // segment; New Slot must still identify the current file.
    app.quick_slots.slots[0] = Some(crate::quick_slots::QuickSlot {
        relative_path: "./old.md".into(),
        context: crate::quick_slots::SlotContext::default(),
    });

    let _ = app.update(Message::QuickSlotNew);

    let slot = app.quick_slots.occupied(0).expect("existing file slot");
    assert_eq!(slot.relative_path, "./old.md");
    assert_eq!(slot.context.mode, crate::quick_slots::SlotMode::Rendered);
    assert_eq!(app.quick_slots.active, Some(0));
    let isolated = app.quick_slots_persistence_path.clone().unwrap();
    let root = app.workspace.take().unwrap();
    let _ = std::fs::remove_file(isolated);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn new_quick_slot_uses_first_empty_slot() {
    let (mut app, old_file, new_file) = quick_slot_restore_test_app("new-slot-empty");
    app.file = Some(old_file.clone());
    app.source = "# Old\n".into();
    app.saved_source = app.source.clone();
    app.quick_slots.set(
        0,
        crate::quick_slots::QuickSlot {
            relative_path: "new.md".into(),
            context: crate::quick_slots::SlotContext::default(),
        },
    );

    let _ = app.update(Message::QuickSlotNew);

    assert_eq!(app.quick_slots.active, Some(1));
    assert_eq!(
        app.quick_slots
            .occupied(0)
            .map(|slot| slot.relative_path.as_str()),
        Some("new.md")
    );
    assert_eq!(
        app.quick_slots
            .occupied(1)
            .map(|slot| slot.relative_path.as_str()),
        Some("old.md")
    );
    assert!(new_file.is_file());
    let isolated = app.quick_slots_persistence_path.clone().unwrap();
    let root = app.workspace.take().unwrap();
    let _ = std::fs::remove_file(isolated);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn new_quick_slot_does_not_overwrite_full_bank() {
    let (mut app, old_file, _new_file) = quick_slot_restore_test_app("new-slot-full");
    app.file = Some(old_file.clone());
    app.source = "# Old\n".into();
    app.saved_source = app.source.clone();
    for index in 0..crate::quick_slots::SLOT_COUNT {
        assert!(app.quick_slots.set(
            index,
            crate::quick_slots::QuickSlot {
                relative_path: format!("slot-{index}.md"),
                context: crate::quick_slots::SlotContext::default(),
            },
        ));
    }

    let _ = app.update(Message::QuickSlotNew);

    assert_eq!(app.quick_slots.active, None);
    assert!(app
        .toast
        .as_ref()
        .is_some_and(|toast| toast.text == "Quick Slots are full"));
    for index in 0..crate::quick_slots::SLOT_COUNT {
        let expected = format!("slot-{index}.md");
        assert_eq!(
            app.quick_slots
                .occupied(index)
                .map(|slot| slot.relative_path.as_str()),
            Some(expected.as_str())
        );
    }
    let isolated = app.quick_slots_persistence_path.clone().unwrap();
    let root = app.workspace.take().unwrap();
    let _ = std::fs::remove_file(isolated);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn new_quick_slot_activates_existing_full_mindmap_file_without_overwrite() {
    let (mut app, old_file, _new_file) = quick_slot_restore_test_app("new-slot-full-mode");
    let mut full = App::new_full_mindmap_state();
    full.selected = Some(WorkspaceNodeId::File(old_file.clone()));
    app.full_mindmap = Some(full);
    app.quick_slots.set(
        0,
        crate::quick_slots::QuickSlot {
            relative_path: "old.md".into(),
            context: crate::quick_slots::SlotContext::default(),
        },
    );

    let _ = app.update(Message::QuickSlotNew);

    assert_eq!(app.quick_slots.active, Some(0));
    assert_eq!(
        app.quick_slots.occupied(0).map(|slot| slot.context.mode),
        Some(crate::quick_slots::SlotMode::Rendered)
    );
    let isolated = app.quick_slots_persistence_path.clone().unwrap();
    let root = app.workspace.take().unwrap();
    let _ = std::fs::remove_file(isolated);
    let _ = std::fs::remove_dir_all(root);
}

fn deferred_full_preview_restore_for_workspace(
    opened_root: PathBuf,
    canonical_root: &std::path::Path,
) -> (App, PathBuf, PendingQuickSlotRestore) {
    let folder = canonical_root.join("notes");
    let file = folder.join("note.md");
    let canonical_root = std::fs::canonicalize(canonical_root).unwrap();
    let folder = std::fs::canonicalize(folder).unwrap();
    let file = std::fs::canonicalize(file).unwrap();

    let mut app = App::default();
    app.set_workspace(opened_root, false);
    app.file = Some(file.clone());
    app.source = "# Note\n\nPreview body\n".into();
    app.saved_source = app.source.clone();
    let mut full = App::new_full_mindmap_state();
    full.expanded.insert(folder.clone());
    app.full_mindmap = Some(full);

    let _folder_task = app.begin_full_mindmap_folder_load(folder.clone());
    let folder_request = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_folder_loads.get(&folder).cloned())
        .expect("deferred folder should own a materialization request");
    let slot = crate::quick_slots::QuickSlot {
        relative_path: "notes/note.md".into(),
        context: crate::quick_slots::SlotContext {
            mode: crate::quick_slots::SlotMode::FullMindmap,
            preview_position: 0.75,
            ..Default::default()
        },
    };
    app.quick_slots.set(0, slot);
    let _ = app.begin_quick_slot_activation(0);
    let _ = app.update(Message::QuickSlotRestorePending);
    let snapshot = tree::load_expanded_folder(&folder, false).unwrap();
    let _ = app.update(Message::FullMindmapFolderLoaded {
        request: folder_request,
        result: Ok((folder, snapshot)),
    });
    let guard = app
        .quick_slot_preview_restore_guard
        .clone()
        .expect("deferred Full Mindmap restore should remain guarded");
    assert_eq!(app.workspace.as_deref(), Some(canonical_root.as_path()));
    assert_eq!(app.quick_slot_preview_restore, Some(0.75));
    (app, file, guard)
}

#[test]
fn dotdot_workspace_open_uses_canonical_full_preview_identity() {
    let root = full_mindmap_test_dir("workspace-dotdot-alias");
    let folder = root.join("notes");
    let file = folder.join("note.md");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(&file, "# Note\n\nPreview body\n").unwrap();
    let opened = root
        .join("..")
        .join(root.file_name().expect("workspace name"));
    let (mut app, file, _guard) = deferred_full_preview_restore_for_workspace(opened, &root);
    let request = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_preview.clone())
        .expect("canonical preview request");
    let parsed = parse_full_mindmap_preview_blocking(file.clone(), app.source.clone());
    let _ = app.update(Message::FullMindmapPreviewParsed {
        request,
        result: Ok(parsed),
    });
    assert_eq!(app.take_current_quick_slot_preview_restore(), Some(0.75));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn directory_open_event_opens_workspace_without_error_card() {
    let root = full_mindmap_test_dir("dir-open-event");
    let docs = root.join("docs");
    std::fs::create_dir_all(&docs).unwrap();
    std::fs::write(docs.join("note.md"), "# Note\n").unwrap();
    let mut app = App::default();
    let _ = app.update(Message::OpenFileFinderPath(root.clone()));
    assert_eq!(app.workspace, Some(root.clone()));
    assert!(app.error.is_none());

    // AppKit repeats the launch argument after `App::new` opened it; the
    // second event must not rescan and collapse the tree.
    app.expanded.insert(docs.clone());
    let task = app.update(Message::OpenFileFinderPath(root.clone()));
    assert_eq!(task.units(), 0);
    assert!(app.expanded.contains(&docs));
    assert!(app.error.is_none());
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn symlink_workspace_open_uses_canonical_full_preview_identity() {
    use std::os::unix::fs::symlink;

    let root = full_mindmap_test_dir("workspace-symlink-real");
    let alias = full_mindmap_test_dir("workspace-symlink-alias");
    let folder = root.join("notes");
    let file = folder.join("note.md");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(&file, "# Note\n\nPreview body\n").unwrap();
    symlink(&root, &alias).unwrap();

    let (mut app, file, _guard) = deferred_full_preview_restore_for_workspace(alias.clone(), &root);
    let request = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_preview.clone())
        .expect("canonical preview request");
    let parsed = parse_full_mindmap_preview_blocking(file.clone(), app.source.clone());
    let _ = app.update(Message::FullMindmapPreviewParsed {
        request,
        result: Ok(parsed),
    });
    assert_eq!(app.take_current_quick_slot_preview_restore(), Some(0.75));
    let _ = std::fs::remove_file(alias);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn clearing_during_inflight_slot_restore_drops_stale_completion() {
    let (mut app, _old_file, _new_file) = quick_slot_restore_test_app("stale-clear");
    let old_slot = crate::quick_slots::QuickSlot {
        relative_path: "old.md".into(),
        context: crate::quick_slots::SlotContext::default(),
    };
    app.quick_slots.set(0, old_slot.clone());
    let _load = app.begin_quick_slot_activation(0);
    let _ = app.clear_quick_slot(0);
    let old_path =
        crate::quick_slots::resolve_path(app.workspace.as_deref().unwrap(), "old.md").unwrap();
    let _ = app.update(Message::QuickSlotFileLoaded {
        index: 0,
        slot: old_slot,
        result: Ok((old_path, "# stale\n".into())),
    });
    assert_eq!(app.file, None);
    assert!(app.quick_slots.occupied(0).is_none());
    let _ = std::fs::remove_dir_all(app.workspace.take().unwrap());
}

#[test]
fn overwriting_during_inflight_slot_restore_drops_stale_completion() {
    let (mut app, _old_file, new_file) = quick_slot_restore_test_app("stale-overwrite");
    let old_slot = crate::quick_slots::QuickSlot {
        relative_path: "old.md".into(),
        context: crate::quick_slots::SlotContext::default(),
    };
    app.quick_slots.set(0, old_slot.clone());
    let _load = app.begin_quick_slot_activation(0);
    app.file = Some(new_file.clone());
    app.source = "# New\n".into();
    app.saved_source = app.source.clone();
    let _ = app.assign_quick_slot(0);
    let old_path =
        crate::quick_slots::resolve_path(app.workspace.as_deref().unwrap(), "old.md").unwrap();
    let _ = app.update(Message::QuickSlotFileLoaded {
        index: 0,
        slot: old_slot,
        result: Ok((old_path, "# stale\n".into())),
    });
    assert_eq!(app.file, Some(new_file));
    assert_eq!(
        app.quick_slots
            .occupied(0)
            .map(|slot| slot.relative_path.as_str()),
        Some("new.md")
    );
    let _ = std::fs::remove_dir_all(app.workspace.take().unwrap());
}

#[test]
fn stale_watcher_completion_cannot_supersede_pending_slot_restore() {
    let (mut app, old_file, _new_file) = quick_slot_restore_test_app("watcher-slot-race");
    app.file = Some(old_file.clone());
    app.source = "# Old\n".into();
    app.saved_source = app.source.clone();
    let _ = app.update(Message::FileChanged(old_file.clone()));
    let watcher = app
        .pending_watcher_reload
        .clone()
        .expect("watcher should own the old reload");

    let slot = crate::quick_slots::QuickSlot {
        relative_path: "new.md".into(),
        context: crate::quick_slots::SlotContext::default(),
    };
    app.quick_slots.set(0, slot);
    let _ = app.begin_quick_slot_activation(0);
    assert!(app.pending_quick_slot_restore.is_some());
    let _ = app.update(Message::FileChangedLoaded {
        request: watcher,
        result: Ok((old_file.clone(), "# Stale watcher\n".into())),
    });

    assert_eq!(app.file, Some(old_file));
    assert_eq!(app.source, "# Old\n");
    assert_eq!(app.quick_slots.active, Some(0));
    assert!(app.pending_quick_slot_restore.is_some());
    let _ = std::fs::remove_dir_all(app.workspace.take().unwrap());
}

#[test]
fn slot_completion_wins_over_pending_watcher_reload() {
    let (mut app, old_file, new_file) = quick_slot_restore_test_app("watcher-slot-order");
    app.file = Some(old_file.clone());
    app.source = "# Old\n".into();
    app.saved_source = app.source.clone();
    let _ = app.update(Message::FileChanged(old_file.clone()));
    let watcher = app
        .pending_watcher_reload
        .clone()
        .expect("watcher should own the old reload");

    let slot = crate::quick_slots::QuickSlot {
        relative_path: "new.md".into(),
        context: crate::quick_slots::SlotContext::default(),
    };
    app.quick_slots.set(0, slot.clone());
    let _ = app.begin_quick_slot_activation(0);
    let _ = app.update(Message::QuickSlotFileLoaded {
        index: 0,
        slot,
        result: Ok((new_file.clone(), "# New\n".into())),
    });
    let _ = app.update(Message::FileChangedLoaded {
        request: watcher,
        result: Ok((old_file, "# Stale watcher\n".into())),
    });

    assert_eq!(app.file, Some(new_file));
    assert_eq!(app.source, "# New\n");
    assert_eq!(app.quick_slots.active, Some(0));
    let _ = std::fs::remove_dir_all(app.workspace.take().unwrap());
}

#[test]
fn same_current_watcher_reload_applies_without_changing_slot_identity() {
    let (mut app, old_file, _new_file) = quick_slot_restore_test_app("watcher-same-file");
    app.file = Some(old_file.clone());
    app.source = "# Old\n".into();
    app.saved_source = app.source.clone();
    app.quick_slots.set(
        0,
        crate::quick_slots::QuickSlot {
            relative_path: "old.md".into(),
            context: crate::quick_slots::SlotContext::default(),
        },
    );
    app.quick_slots.active = Some(0);
    let _ = app.update(Message::FileChanged(old_file.clone()));
    let watcher = app
        .pending_watcher_reload
        .clone()
        .expect("watcher should own same-file reload");
    let _ = app.update(Message::FileChangedLoaded {
        request: watcher,
        result: Ok((old_file.clone(), "# Reloaded\n".into())),
    });

    assert_eq!(app.file, Some(old_file));
    assert_eq!(app.source, "# Reloaded\n");
    assert_eq!(app.quick_slots.active, Some(0));
    assert!(app.pending_watcher_reload.is_none());
    let _ = std::fs::remove_dir_all(app.workspace.take().unwrap());
}

#[test]
fn close_window_blocks_dirty_reader_and_full_mindmap() {
    let mut app = App::default();
    app.dirty = true;
    let _ = app.update(Message::QuickSlotCloseWindow);
    assert!(app
        .toast
        .as_ref()
        .is_some_and(|toast| toast.text.contains("unsaved")));

    let mut full = App::default();
    full.full_mindmap = Some(App::new_full_mindmap_state());
    full.dirty = true;
    let _ = full.update(Message::QuickSlotCloseWindow);
    assert!(full
        .toast
        .as_ref()
        .is_some_and(|toast| toast.text.contains("unsaved")));
}

#[test]
fn clean_close_window_persists_active_slot_before_close_task() {
    let (mut app, _old_file, _new_file) = quick_slot_restore_test_app("close-persist");
    let root = app.workspace.clone().unwrap();
    app.quick_slots.set(
        0,
        crate::quick_slots::QuickSlot {
            relative_path: "old.md".into(),
            context: crate::quick_slots::SlotContext::default(),
        },
    );
    app.quick_slots.active = Some(0);
    let _ = app.close_quick_slot_window();
    let isolated = app.quick_slots_persistence_path.clone().unwrap();
    let stored = crate::prefs::load_from(&isolated);
    assert_eq!(stored.quick_slots.bank(&root).active, Some(0));
    let _ = std::fs::remove_file(isolated);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn full_preview_restore_guard_rejects_overwrite() {
    let (mut app, _old_file, _new_file) = quick_slot_restore_test_app("stale-preview");
    let root = app.workspace.clone().unwrap();
    let old_slot = crate::quick_slots::QuickSlot {
        relative_path: "old.md".into(),
        context: crate::quick_slots::SlotContext {
            mode: crate::quick_slots::SlotMode::FullMindmap,
            preview_position: 0.75,
            ..Default::default()
        },
    };
    app.quick_slots.set(0, old_slot.clone());
    app.quick_slots.active = Some(0);
    app.quick_slot_activation_generation = 1;
    app.quick_slot_preview_restore_guard = Some(PendingQuickSlotRestore {
        generation: 1,
        index: 0,
        slot: old_slot,
        root_key: crate::quick_slots::workspace_key(&root),
    });
    app.quick_slot_preview_restore = Some(0.75);
    app.quick_slots.set(
        0,
        crate::quick_slots::QuickSlot {
            relative_path: "new.md".into(),
            context: crate::quick_slots::SlotContext::default(),
        },
    );
    assert_eq!(app.take_current_quick_slot_preview_restore(), None);
    assert_eq!(app.quick_slot_preview_restore_guard, None);
    assert_eq!(app.quick_slot_preview_restore, None);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn deferred_full_preview_source_reparse_preserves_current_slot_restore() {
    let root = full_mindmap_test_dir("deferred-preview-restore");
    let folder = root.join("notes");
    let file = folder.join("note.md");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(&file, "# Note\n\nPreview body\n").unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let folder = std::fs::canonicalize(folder).unwrap();
    let file = std::fs::canonicalize(file).unwrap();

    let mut app = App::default();
    app.set_workspace(root.clone(), false);
    app.file = Some(file.clone());
    app.source = "# Note\n\nPreview body\n".into();
    app.saved_source = app.source.clone();
    let mut full = App::new_full_mindmap_state();
    full.expanded.insert(folder.clone());
    app.full_mindmap = Some(full);

    let _folder_task = app.begin_full_mindmap_folder_load(folder.clone());
    let folder_request = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_folder_loads.get(&folder).cloned())
        .expect("deferred folder should own a materialization request");

    let slot = crate::quick_slots::QuickSlot {
        relative_path: "notes/note.md".into(),
        context: crate::quick_slots::SlotContext {
            mode: crate::quick_slots::SlotMode::FullMindmap,
            preview_position: 0.75,
            ..Default::default()
        },
    };
    app.quick_slots.set(0, slot.clone());
    let _ = app.begin_quick_slot_activation(0);
    let _ = app.update(Message::QuickSlotRestorePending);
    let guarded = app
        .quick_slot_preview_restore_guard
        .clone()
        .expect("Quick Slot restore should be guarded before materialization");
    assert_eq!(app.quick_slot_preview_restore, Some(0.75));

    let snapshot = tree::load_expanded_folder(&folder, false).unwrap();
    let _ = app.update(Message::FullMindmapFolderLoaded {
        request: folder_request,
        result: Ok((folder.clone(), snapshot)),
    });
    // The deferred folder completion reparses the already-open source. The
    // current slot's position and identity guard must survive that handoff.
    assert_eq!(app.quick_slot_preview_restore, Some(0.75));
    assert_eq!(app.quick_slot_preview_restore_guard, Some(guarded.clone()));

    let preview_request = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_preview.clone())
        .expect("source reparse should own a preview request");
    let parsed = parse_full_mindmap_preview_blocking(file.clone(), app.source.clone());
    let _ = app.update(Message::FullMindmapPreviewParsed {
        request: preview_request,
        result: Ok(parsed),
    });
    assert_eq!(
        app.take_current_quick_slot_preview_restore(),
        Some(0.75),
        "current identity may consume the preserved restore value"
    );

    // A changed slot identity must not consume the same saved value.
    app.quick_slot_preview_restore_guard = Some(guarded);
    app.quick_slot_preview_restore = Some(0.75);
    app.quick_slots.set(
        0,
        crate::quick_slots::QuickSlot {
            relative_path: "notes/other.md".into(),
            context: crate::quick_slots::SlotContext::default(),
        },
    );
    assert_eq!(app.take_current_quick_slot_preview_restore(), None);
    assert_eq!(app.quick_slot_preview_restore, None);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn quick_slot_context_never_persists_raw_zen_mode() {
    let root = full_mindmap_test_dir("quick-slot-context");
    let file = root.join("notes.md");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(&file, "# Notes\n").unwrap();

    let mut app = App::default();
    app.set_workspace(root.clone(), false);
    app.file = Some(file.clone());
    app.source = "# Notes\n".into();
    app.saved_source = app.source.clone();
    app.view_mode = ViewMode::Raw;
    let (_, context) = app.current_quick_slot_context().expect("context");
    assert_eq!(context.mode, crate::quick_slots::SlotMode::Rendered);
    assert_eq!(context.body_position, 0.0);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn clean_zen_activation_leaves_editor_and_restores_active_reader_slot() {
    let (mut app, old_file, _new_file) = quick_slot_restore_test_app("clean-zen-activate");
    let old_file = std::fs::canonicalize(old_file).unwrap();
    let slot = crate::quick_slots::QuickSlot {
        relative_path: "old.md".into(),
        context: crate::quick_slots::SlotContext::default(),
    };
    app.file = Some(old_file.clone());
    app.source = "# Old\n".into();
    app.saved_source = app.source.clone();
    app.quick_slots.set(0, slot.clone());
    app.quick_slots.active = Some(0);
    app.view_mode = ViewMode::Raw;
    app.editor = Some(iced::widget::text_editor::Content::with_text(
        app.source.as_str(),
    ));
    app.dirty = false;

    let _ = app.begin_quick_slot_activation(0);
    let pending = app
        .pending_quick_slot_restore
        .clone()
        .expect("clean Zen activation should own a restore");
    assert!(app.quick_slot_restore_is_current(&pending));
    assert_eq!(
        crate::quick_slots::resolve_path(app.workspace.as_deref().unwrap(), "old.md"),
        Some(old_file.clone())
    );
    let _ = app.update(Message::QuickSlotFileLoaded {
        index: 0,
        slot,
        result: Ok((old_file, "# Old\n".into())),
    });

    assert_eq!(app.view_mode, ViewMode::Rendered);
    assert!(app.editor.is_none());
    assert!(!app.dirty);
    assert_eq!(app.quick_slots.active, Some(0));
    let _ = std::fs::remove_dir_all(app.workspace.take().unwrap());
}

#[test]
fn clean_zen_full_mindmap_activation_does_not_revive_editor_on_exit() {
    let (mut app, old_file, _new_file) = quick_slot_restore_test_app("clean-zen-full-mindmap");
    let root = app.workspace.clone().unwrap();
    let slot = crate::quick_slots::QuickSlot {
        relative_path: "old.md".into(),
        context: crate::quick_slots::SlotContext {
            mode: crate::quick_slots::SlotMode::FullMindmap,
            preview_position: 0.5,
            ..Default::default()
        },
    };
    app.file = Some(old_file.clone());
    app.source = "# Old\n".into();
    app.saved_source = app.source.clone();
    app.quick_slots.set(0, slot.clone());
    app.quick_slots.active = Some(0);
    app.view_mode = ViewMode::Raw;
    app.editor = Some(iced::widget::text_editor::Content::with_text(
        app.source.as_str(),
    ));
    app.dirty = false;

    let _ = app.begin_quick_slot_activation(0);
    assert!(app.full_mindmap.is_some());
    assert_eq!(app.view_mode, ViewMode::Rendered);
    assert!(app.editor.is_none());
    assert!(app.zen_restore.is_none());
    assert_eq!(app.quick_slots.occupied(0), Some(&slot));
    assert_eq!(app.prefs.quick_slots.bank(&root).occupied(0), Some(&slot));
    let isolated = app.quick_slots_persistence_path.clone().unwrap();
    let stored = crate::prefs::load_from(&isolated);
    assert_eq!(stored.quick_slots.bank(&root).occupied(0), Some(&slot));

    let _ = app.update(Message::QuickSlotRestorePending);
    let _ = app.exit_full_mindmap(false);
    assert!(app.full_mindmap.is_none());
    assert_eq!(app.view_mode, ViewMode::Rendered);
    assert!(app.editor.is_none());
    assert!(app.zen_restore.is_none());
    let _ = std::fs::remove_file(isolated);
    let _ = std::fs::remove_dir_all(app.workspace.take().unwrap());
}

#[test]
fn clean_zen_document_mindmap_activation_preserves_slot_context_in_bank_and_prefs() {
    let (mut app, old_file, _new_file) = quick_slot_restore_test_app("clean-zen-document-mindmap");
    let root = app.workspace.clone().unwrap();
    let slot = crate::quick_slots::QuickSlot {
        relative_path: "old.md".into(),
        context: crate::quick_slots::SlotContext {
            mode: crate::quick_slots::SlotMode::DocumentMindmap,
            body_position: 0.75,
            mindmap_panel_open: true,
            ..Default::default()
        },
    };
    app.file = Some(old_file.clone());
    app.source = "# Old\n".into();
    app.saved_source = app.source.clone();
    app.quick_slots.set(0, slot.clone());
    app.quick_slots.active = Some(0);
    app.view_mode = ViewMode::Raw;
    app.editor = Some(iced::widget::text_editor::Content::with_text(
        app.source.as_str(),
    ));
    app.dirty = false;

    let _ = app.begin_quick_slot_activation(0);
    assert_eq!(app.quick_slots.occupied(0), Some(&slot));
    assert_eq!(app.prefs.quick_slots.bank(&root).occupied(0), Some(&slot));
    let isolated = app.quick_slots_persistence_path.clone().unwrap();
    let stored = crate::prefs::load_from(&isolated);
    assert_eq!(stored.quick_slots.bank(&root).occupied(0), Some(&slot));

    let _ = app.update(Message::QuickSlotFileLoaded {
        index: 0,
        slot: slot.clone(),
        result: Ok((old_file, "# Old\n".into())),
    });
    assert_eq!(app.view_mode, ViewMode::Mindmap);
    assert!(app.editor.is_none());
    assert_eq!(app.quick_slots.occupied(0), Some(&slot));
    let _ = std::fs::remove_file(isolated);
    let _ = std::fs::remove_dir_all(app.workspace.take().unwrap());
}

#[test]
fn document_mindmap_slot_activation_from_full_mindmap_exits_navigator() {
    let (mut app, old_file, new_file) =
        quick_slot_restore_test_app("document-mindmap-from-full-mindmap");
    let root = app.workspace.clone().unwrap();
    let parent = root.join("parent");
    std::fs::create_dir_all(&parent).unwrap();
    std::fs::write(parent.join("child.md"), "# Child\n").unwrap();
    let old_file = std::fs::canonicalize(old_file).unwrap();
    let new_file = std::fs::canonicalize(new_file).unwrap();
    app.file = Some(new_file);
    app.source = "# New\n".into();
    app.saved_source = app.source.clone();
    app.view_mode = ViewMode::Mindmap;
    let mut full = full_workspace_state(&root);
    full.selected = Some(WorkspaceNodeId::Folder(parent));
    app.full_mindmap = Some(full);
    app.quick_slots.set(
        0,
        crate::quick_slots::QuickSlot {
            relative_path: "old.md".into(),
            context: crate::quick_slots::SlotContext {
                mode: crate::quick_slots::SlotMode::DocumentMindmap,
                mindmap_panel_open: true,
                ..Default::default()
            },
        },
    );
    // Selecting the parent folder clears the current active marker while
    // keeping both bookmarks available in the bank.
    app.quick_slots.active = None;

    let _ = app.begin_quick_slot_activation(0);
    assert!(
        app.full_mindmap.is_none(),
        "switching to a document slot must leave Full Mindmap"
    );
    let slot = app.quick_slots.occupied(0).unwrap().clone();
    let _ = app.update(Message::QuickSlotFileLoaded {
        index: 0,
        slot,
        result: Ok((old_file.clone(), "# Old\n".into())),
    });

    assert_eq!(app.file, Some(old_file));
    assert_eq!(app.view_mode, ViewMode::Mindmap);
    assert!(app.full_mindmap.is_none());
    assert!(app.editor.is_none());
    let isolated = app.quick_slots_persistence_path.clone().unwrap();
    let _ = std::fs::remove_file(isolated);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn selecting_full_mindmap_parent_does_not_rewrite_document_slot_mode() {
    let (mut app, old_file, _new_file) =
        quick_slot_restore_test_app("full-mindmap-parent-keeps-document-slot");
    let root = app.workspace.clone().unwrap();
    let parent = root.join("parent");
    std::fs::create_dir_all(&parent).unwrap();
    std::fs::write(parent.join("child.md"), "# Child\n").unwrap();
    let old_file = std::fs::canonicalize(old_file).unwrap();

    app.file = Some(old_file.clone());
    app.source = "# Old\n".into();
    app.saved_source = app.source.clone();
    app.view_mode = ViewMode::Mindmap;
    app.load_ast_from_source();
    let slot = crate::quick_slots::QuickSlot {
        relative_path: "old.md".into(),
        context: crate::quick_slots::SlotContext {
            mode: crate::quick_slots::SlotMode::DocumentMindmap,
            mindmap_panel_open: true,
            ..Default::default()
        },
    };
    app.quick_slots.set(0, slot.clone());
    app.quick_slots.active = Some(0);
    let mut full = full_workspace_state(&root);
    full.selected = Some(WorkspaceNodeId::File(old_file.clone()));
    app.full_mindmap = Some(full);

    let _ = app.update(Message::FullMindmapSelectNode(WorkspaceNodeId::Folder(
        parent,
    )));

    assert_eq!(app.quick_slots.occupied(0), Some(&slot));
    assert_eq!(app.quick_slots.active, None);
    let _ = app.begin_quick_slot_activation(0);
    assert!(app.full_mindmap.is_none());
    let _ = app.update(Message::QuickSlotFileLoaded {
        index: 0,
        slot,
        result: Ok((old_file.clone(), "# Old\n".into())),
    });
    assert_eq!(app.file, Some(old_file));
    assert_eq!(app.view_mode, ViewMode::Mindmap);
    assert!(app.full_mindmap.is_none());
    let isolated = app.quick_slots_persistence_path.clone().unwrap();
    let _ = std::fs::remove_file(isolated);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn switching_quick_slots_checkpoints_outgoing_context_before_restore() {
    let (mut app, old_file, _new_file) = quick_slot_restore_test_app("switch-checkpoints-outgoing");
    let root = app.workspace.clone().unwrap();
    let outgoing = crate::quick_slots::QuickSlot {
        relative_path: "old.md".into(),
        context: crate::quick_slots::SlotContext::default(),
    };
    let incoming = crate::quick_slots::QuickSlot {
        relative_path: "new.md".into(),
        context: crate::quick_slots::SlotContext::default(),
    };
    app.file = Some(old_file);
    app.source = "# Old\n".into();
    app.saved_source = app.source.clone();
    app.view_mode = ViewMode::Mindmap;
    app.mindmap_selected = Some(crate::ast::BlockId(7));
    app.mindmap_panel_open = true;
    app.quick_slots.set(0, outgoing);
    app.quick_slots.set(1, incoming);
    app.quick_slots.active = Some(0);
    app.dirty = false;

    let _ = app.begin_quick_slot_activation(1);
    let checkpointed = app.quick_slots.occupied(0).expect("outgoing slot");
    assert_eq!(
        checkpointed.context.mode,
        crate::quick_slots::SlotMode::DocumentMindmap
    );
    assert_eq!(checkpointed.context.mindmap_selection, Some(7));
    assert!(checkpointed.context.mindmap_panel_open);
    let isolated = app.quick_slots_persistence_path.clone().unwrap();
    let stored = crate::prefs::load_from(&isolated);
    let stored_bank = stored.quick_slots.bank(&root);
    let persisted = stored_bank.occupied(0).expect("persisted outgoing slot");
    assert_eq!(
        persisted.context.mode,
        crate::quick_slots::SlotMode::DocumentMindmap
    );
    assert_eq!(persisted.context.mindmap_selection, Some(7));
    assert!(persisted.context.mindmap_panel_open);
    let _ = std::fs::remove_file(isolated);
    let _ = std::fs::remove_dir_all(app.workspace.take().unwrap());
}

#[test]
fn picker_file_navigation_checkpoints_active_slot_before_load() {
    let (mut app, old_file, new_file) = quick_slot_restore_test_app("picker-checkpoints-outgoing");
    app.file = Some(old_file);
    app.source = "# Old\n".into();
    app.saved_source = app.source.clone();
    app.view_mode = ViewMode::Mindmap;
    app.mindmap_selected = Some(crate::ast::BlockId(17));
    app.mindmap_panel_open = true;
    app.quick_slots.set(
        0,
        crate::quick_slots::QuickSlot {
            relative_path: "old.md".into(),
            context: crate::quick_slots::SlotContext::default(),
        },
    );
    app.quick_slots.active = Some(0);
    app.dirty = false;

    let _ = app.update(Message::PickerOpenFile(new_file.clone()));

    let checkpointed = app.quick_slots.occupied(0).expect("active slot");
    assert_eq!(
        checkpointed.context.mode,
        crate::quick_slots::SlotMode::DocumentMindmap
    );
    assert_eq!(checkpointed.context.mindmap_selection, Some(17));
    assert!(checkpointed.context.mindmap_panel_open);
    // Complete the asynchronous picker read: manual navigation must clear
    // the active marker in memory and in the persisted workspace bank.
    let _ = app.update(Message::FileLoaded(Ok((new_file, "# New\n".into()))));
    assert_eq!(app.quick_slots.active, None);
    let isolated = app.quick_slots_persistence_path.clone().unwrap();
    let stored = crate::prefs::load_from(&isolated);
    let root = app.workspace.as_ref().expect("workspace root");
    let persisted = stored.quick_slots.bank(root);
    assert_eq!(persisted.active, None);
    assert_eq!(
        persisted.occupied(0).map(|slot| slot.context.mode),
        Some(crate::quick_slots::SlotMode::DocumentMindmap)
    );
    assert_eq!(
        persisted
            .occupied(0)
            .and_then(|slot| slot.context.mindmap_selection),
        Some(17)
    );
    let _ = std::fs::remove_file(isolated);
    let _ = std::fs::remove_dir_all(app.workspace.take().unwrap());
}

#[test]
fn ipc_file_navigation_checkpoints_active_slot_before_load() {
    let (mut app, old_file, new_file) = quick_slot_restore_test_app("ipc-checkpoints-outgoing");
    app.file = Some(old_file);
    app.source = "# Old\n".into();
    app.saved_source = app.source.clone();
    app.view_mode = ViewMode::Mindmap;
    app.mindmap_selected = Some(crate::ast::BlockId(23));
    app.mindmap_panel_open = true;
    app.quick_slots.set(
        0,
        crate::quick_slots::QuickSlot {
            relative_path: "old.md".into(),
            context: crate::quick_slots::SlotContext::default(),
        },
    );
    app.quick_slots.active = Some(0);
    app.dirty = false;

    let _ = app.update(Message::Ipc(
        crate::ipc::Request {
            id: 17,
            cmd: crate::ipc::Cmd::Open {
                file: new_file.to_string_lossy().into_owned(),
                line: Some(8),
                section: Some("Target".into()),
                focus: crate::ipc::FocusBehavior::Suppress,
            },
        },
        std::sync::Arc::new(std::sync::Mutex::new(None)),
    ));

    let checkpointed = app.quick_slots.occupied(0).expect("active slot");
    assert_eq!(
        checkpointed.context.mode,
        crate::quick_slots::SlotMode::DocumentMindmap
    );
    assert_eq!(checkpointed.context.mindmap_selection, Some(23));
    assert!(checkpointed.context.mindmap_panel_open);
    assert_eq!(app.pending_nav.as_ref().and_then(|nav| nav.line), Some(8));
    let isolated = app.quick_slots_persistence_path.clone().unwrap();
    let _ = std::fs::remove_file(isolated);
    let _ = std::fs::remove_dir_all(app.workspace.take().unwrap());
}

#[test]
fn active_full_mindmap_slot_reactivates_after_manual_navigation() {
    let (mut app, _old_file, new_file) =
        quick_slot_restore_test_app("active-full-mindmap-manual-nav");
    let slot = crate::quick_slots::QuickSlot {
        relative_path: "old.md".into(),
        context: crate::quick_slots::SlotContext {
            mode: crate::quick_slots::SlotMode::FullMindmap,
            preview_position: 0.6,
            ..Default::default()
        },
    };
    app.quick_slots.set(0, slot.clone());
    app.quick_slots.active = Some(0);
    app.file = Some(new_file);
    app.source = "# New\n".into();
    app.saved_source = app.source.clone();
    app.view_mode = ViewMode::Rendered;
    app.dirty = false;

    let _ = app.begin_quick_slot_activation(0);
    let pending = app
        .pending_quick_slot_restore
        .clone()
        .expect("manual navigation must not suppress Full Mindmap restore");
    assert_eq!(pending.slot, slot);
    assert!(app.full_mindmap.is_some());
    let _ = std::fs::remove_file(app.quick_slots_persistence_path.clone().unwrap());
    let _ = std::fs::remove_dir_all(app.workspace.take().unwrap());
}

#[test]
fn active_rendered_slot_reactivates_after_manual_navigation() {
    let (mut app, _old_file, new_file) = quick_slot_restore_test_app("active-rendered-manual-nav");
    let slot = crate::quick_slots::QuickSlot {
        relative_path: "old.md".into(),
        context: crate::quick_slots::SlotContext::default(),
    };
    app.quick_slots.set(0, slot.clone());
    app.quick_slots.active = Some(0);
    app.file = Some(new_file);
    app.source = "# New\n".into();
    app.saved_source = app.source.clone();
    app.view_mode = ViewMode::Rendered;
    app.dirty = false;

    let _ = app.begin_quick_slot_activation(0);
    let pending = app
        .pending_quick_slot_restore
        .clone()
        .expect("manual navigation must not suppress Rendered restore");
    assert_eq!(pending.slot, slot);
    assert_eq!(app.quick_slots.active, Some(0));
    let _ = std::fs::remove_file(app.quick_slots_persistence_path.clone().unwrap());
    let _ = std::fs::remove_dir_all(app.workspace.take().unwrap());
}

#[test]
fn active_document_mindmap_slot_reactivates_when_mode_differs() {
    let (mut app, old_file, _new_file) =
        quick_slot_restore_test_app("active-document-mindmap-mode-mismatch");
    let slot = crate::quick_slots::QuickSlot {
        relative_path: "old.md".into(),
        context: crate::quick_slots::SlotContext {
            mode: crate::quick_slots::SlotMode::DocumentMindmap,
            ..Default::default()
        },
    };
    app.quick_slots.set(0, slot.clone());
    app.quick_slots.active = Some(0);
    app.file = Some(old_file);
    app.source = "# Old\n".into();
    app.saved_source = app.source.clone();
    app.view_mode = ViewMode::Rendered;
    app.dirty = false;

    let _ = app.begin_quick_slot_activation(0);
    let pending = app
        .pending_quick_slot_restore
        .clone()
        .expect("mode mismatch must restore Document Mindmap");
    assert_eq!(pending.slot, slot);
    let _ = std::fs::remove_file(app.quick_slots_persistence_path.clone().unwrap());
    let _ = std::fs::remove_dir_all(app.workspace.take().unwrap());
}

#[test]
fn full_mindmap_activation_of_content_slots_exits_to_saved_content_mode() {
    for (label, mode) in [
        ("rendered", crate::quick_slots::SlotMode::Rendered),
        (
            "document-mindmap",
            crate::quick_slots::SlotMode::DocumentMindmap,
        ),
    ] {
        let (mut app, old_file, new_file) =
            quick_slot_restore_test_app(&format!("full-to-content-{label}"));
        let slot = crate::quick_slots::QuickSlot {
            relative_path: "new.md".into(),
            context: crate::quick_slots::SlotContext {
                mode,
                ..Default::default()
            },
        };
        let mut full = App::new_full_mindmap_state();
        full.selected = Some(WorkspaceNodeId::File(old_file));
        app.full_mindmap = Some(full);
        app.quick_slots.set(0, slot.clone());

        let _ = app.begin_quick_slot_activation(0);

        assert!(
            app.full_mindmap.is_none(),
            "content slots must leave Full Mindmap before loading"
        );
        assert_eq!(app.quick_slots.active, Some(0));
        assert_eq!(
            app.pending_quick_slot_restore
                .as_ref()
                .map(|pending| pending.slot.context.mode),
            Some(mode)
        );
        assert!(new_file.is_file());
        let _ = std::fs::remove_file(app.quick_slots_persistence_path.clone().unwrap());
        let _ = std::fs::remove_dir_all(app.workspace.take().unwrap());
    }
}

#[test]
fn quick_slot_workspace_banks_restore_by_canonical_root() {
    let first = full_mindmap_test_dir("quick-slot-bank-a");
    let second = full_mindmap_test_dir("quick-slot-bank-b");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    std::fs::write(first.join("a.md"), "# A\n").unwrap();
    std::fs::write(second.join("b.md"), "# B\n").unwrap();

    let mut app = App::default();
    let mut bank = crate::quick_slots::WorkspaceSlots::default();
    bank.set(
        0,
        crate::quick_slots::QuickSlot {
            relative_path: "a.md".into(),
            context: crate::quick_slots::SlotContext::default(),
        },
    );
    app.prefs.quick_slots.put_bank(&first, bank);
    app.set_workspace(first.clone(), false);
    assert!(app.quick_slots.occupied(0).is_some());
    app.set_workspace(second.clone(), false);
    assert!(app.quick_slots.occupied(0).is_none());
    app.set_workspace(first.clone(), false);
    assert_eq!(
        app.quick_slots
            .occupied(0)
            .map(|slot| slot.relative_path.as_str()),
        Some("a.md")
    );
    let _ = std::fs::remove_dir_all(first);
    let _ = std::fs::remove_dir_all(second);
}

#[test]
fn diagram_hash_present_finds_nested_list_math() {
    let blocks = vec![(
        crate::ast::BlockId(1),
        Block::List {
            ordered: true,
            items: vec![ListItem {
                task: None,
                blocks: vec![Block::Diagram {
                    kind: DiagramKind::Math,
                    source: "x".into(),
                    hash: 42,
                }],
            }],
        },
    )];

    assert!(diagram_hash_present(&blocks, 42));
}

#[test]
fn sidebar_titlebar_reserve_collapses_only_for_fullscreen() {
    #[cfg(target_os = "macos")]
    {
        // Fullscreen keeps a small top margin but less than the windowed
        // traffic-light reserve.
        assert_eq!(sidebar_titlebar_reserve_for_fullscreen(true), 10.0);
        assert_eq!(sidebar_titlebar_reserve_for_fullscreen(false), 22.0);
        assert!(
            sidebar_titlebar_reserve_for_fullscreen(true)
                < sidebar_titlebar_reserve_for_fullscreen(false)
        );
    }

    #[cfg(not(target_os = "macos"))]
    {
        assert_eq!(sidebar_titlebar_reserve_for_fullscreen(true), 0.0);
        assert_eq!(sidebar_titlebar_reserve_for_fullscreen(false), 0.0);
    }
}

#[test]
fn full_mindmap_preview_scrollbar_reuses_transparent_rail_style() {
    let pal = Palette::ONE_DARK;
    let idle = sleek_scrollable_style(
        scrollable::Status::Active {
            is_horizontal_scrollbar_disabled: false,
            is_vertical_scrollbar_disabled: false,
        },
        pal,
        false,
    );
    assert!(idle.vertical_rail.background.is_none());
    assert!(idle.horizontal_rail.background.is_none());
    assert_eq!(
        idle.vertical_rail.scroller.background,
        Background::Color(Color::TRANSPARENT)
    );

    // The shared ordinary Markdown style keeps the affordance on hover;
    // only the rail/idle track remains transparent.
    let hovered = sleek_scrollable_style(
        scrollable::Status::Hovered {
            is_horizontal_scrollbar_hovered: false,
            is_vertical_scrollbar_hovered: true,
            is_horizontal_scrollbar_disabled: false,
            is_vertical_scrollbar_disabled: false,
        },
        pal,
        false,
    );
    assert_eq!(
        hovered.vertical_rail.scroller.background,
        Background::Color(pal.scroller_hover)
    );
}

#[test]
fn full_mindmap_non_file_panel_keeps_default_scrollbar_style() {
    let default = scrollable::default(
        &Theme::Dark,
        scrollable::Status::Active {
            is_horizontal_scrollbar_disabled: false,
            is_vertical_scrollbar_disabled: false,
        },
    );
    // The folder/status branch explicitly uses the catalog default, so
    // its rails remain the established themed tracks rather than inheriting
    // the transparent file-preview treatment.
    assert!(default.vertical_rail.background.is_some());
    assert!(default.horizontal_rail.background.is_some());
}

#[test]
fn folder_only_snapshot_sidebar_activates_indexed_file_through_dirty_guard() {
    let dir = full_mindmap_test_dir("sidebar-indexed-activate");
    let target = dir.join("target.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&target, "# Target\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    assert!(app
        .workspace_tree
        .as_ref()
        .unwrap()
        .children
        .iter()
        .all(|node| node.is_dir));
    let rows = tree::flatten_with_files(
        app.workspace_tree.as_ref().unwrap(),
        &app.workspace_sidebar_files,
        &app.expanded,
    );
    app.tree_cursor = rows
        .iter()
        .position(|row| row.node.path() == target)
        .expect("indexed root file should remain a standard sidebar row");
    app.file = Some(dir.join("current.md"));
    app.source = "unsaved current".into();
    app.saved_source = "saved current".into();
    app.dirty = true;

    let _ = app.update(Message::TreeActivate);

    assert_eq!(app.source, "unsaved current");
    assert_eq!(app.file.as_deref(), Some(dir.join("current.md").as_path()));
    assert!(app.dirty);
    assert!(app
        .toast
        .as_ref()
        .is_some_and(|toast| toast.text.contains("unsaved")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn standard_sidebar_rows_follow_hidden_snapshot_refresh() {
    let dir = full_mindmap_test_dir("sidebar-hidden-refresh");
    let visible = dir.join("visible.md");
    let hidden = dir.join(".hidden.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&visible, "# Visible\n").unwrap();
    std::fs::write(&hidden, "# Hidden\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    let before = tree::flatten_with_files(
        app.workspace_tree.as_ref().unwrap(),
        &app.workspace_sidebar_files,
        &app.expanded,
    );
    assert!(before.iter().any(|row| row.node.path() == visible));
    assert!(!before.iter().any(|row| row.node.path() == hidden));

    let _ = app.update(Message::ToggleHidden);
    let after = tree::flatten_with_files(
        app.workspace_tree.as_ref().unwrap(),
        &app.workspace_sidebar_files,
        &app.expanded,
    );
    assert!(after.iter().any(|row| row.node.path() == visible));
    assert!(after.iter().any(|row| row.node.path() == hidden));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn manual_refresh_rebuilds_workspace_in_background_and_preserves_expansion() {
    let dir = full_mindmap_test_dir("manual-refresh-workspace");
    let docs = dir.join("docs");
    let added = docs.join("added.md");
    std::fs::create_dir_all(&docs).unwrap();
    std::fs::write(docs.join("existing.md"), "# Existing\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.expanded.insert(docs.clone());
    std::fs::write(&added, "# Added\n").unwrap();

    let _ = app.update(Message::Refresh);
    let request = app
        .pending_refresh_workspace
        .clone()
        .expect("workspace refresh should be pending");
    assert!(!app.workspace_files.contains(&added));

    let snapshot = tree::build_workspace(&request.path, request.show_hidden).unwrap();
    let _ = app.update(Message::RefreshWorkspaceLoaded {
        request,
        result: Ok((dir.clone(), snapshot)),
    });

    assert!(app.workspace_files.contains(&added));
    assert!(app.expanded.contains(&dir));
    assert!(app.expanded.contains(&docs));
    assert!(app.error.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn stale_manual_workspace_refresh_completion_is_ignored() {
    let dir = full_mindmap_test_dir("stale-manual-refresh-workspace");
    let docs = dir.join("docs");
    let added = docs.join("added.md");
    std::fs::create_dir_all(&docs).unwrap();
    std::fs::write(docs.join("existing.md"), "# Existing\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);

    let _ = app.update(Message::Refresh);
    let stale_request = app
        .pending_refresh_workspace
        .clone()
        .expect("first workspace refresh should be pending");
    std::fs::write(&added, "# Added\n").unwrap();

    let _ = app.update(Message::Refresh);
    let current_request = app
        .pending_refresh_workspace
        .clone()
        .expect("second workspace refresh should be pending");
    let snapshot =
        tree::build_workspace(&current_request.path, current_request.show_hidden).unwrap();

    let _ = app.update(Message::RefreshWorkspaceLoaded {
        request: stale_request,
        result: Ok((dir.clone(), snapshot.clone())),
    });
    assert!(!app.workspace_files.contains(&added));

    let _ = app.update(Message::RefreshWorkspaceLoaded {
        request: current_request,
        result: Ok((dir.clone(), snapshot)),
    });
    assert!(app.workspace_files.contains(&added));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn manual_workspace_refresh_is_cancelled_across_full_mindmap_entry_and_exit() {
    let dir = full_mindmap_test_dir("manual-refresh-full-mindmap-transition");
    let added = dir.join("added.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("existing.md"), "# Existing\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    std::fs::write(&added, "# Added\n").unwrap();

    let _ = app.update(Message::Refresh);
    let request = app
        .pending_refresh_workspace
        .clone()
        .expect("ordinary workspace refresh should be pending");
    let snapshot = tree::build_workspace(&request.path, request.show_hidden).unwrap();

    let _ = app.update(Message::ToggleFullMindmap);
    assert!(app.full_mindmap.is_some());
    assert!(app.pending_refresh.is_none());
    assert!(app.pending_refresh_workspace.is_none());
    assert!(app.pending_refresh_file.is_none());
    assert!(app.pending_refresh_full_mindmap_workspace.is_none());

    let _ = app.update(Message::ExitFullMindmap);
    assert!(app.full_mindmap.is_none());
    let _ = app.update(Message::RefreshWorkspaceLoaded {
        request,
        result: Ok((dir.clone(), snapshot)),
    });

    assert!(!app.workspace_files.contains(&added));
    assert!(app.pending_refresh.is_none());
    assert!(app.pending_refresh_workspace.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_refresh_is_cancelled_by_normal_exit() {
    let dir = full_mindmap_test_dir("full-mindmap-refresh-exit");
    let added = dir.join("added.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("existing.md"), "# Existing\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.full_mindmap = Some(full_workspace_state(&dir));
    std::fs::write(&added, "# Added\n").unwrap();

    let _ = app.update(Message::Refresh);
    let request = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_workspace_load.clone())
        .expect("Full Mindmap workspace refresh should be pending");
    assert_eq!(
        app.pending_refresh_full_mindmap_workspace.as_ref(),
        Some(&request)
    );
    let snapshot = tree::build_workspace(&request.path, app.show_hidden).unwrap();

    let _ = app.update(Message::ExitFullMindmap);
    assert!(app.full_mindmap.is_none());
    assert!(app.pending_refresh.is_none());
    assert!(app.pending_refresh_full_mindmap_workspace.is_none());
    assert!(app.pending_refresh_file.is_none());

    let _ = app.update(Message::FullMindmapWorkspaceLoaded {
        request,
        result: Ok((dir.clone(), snapshot)),
    });
    assert!(!app.workspace_files.contains(&added));
    assert!(app.toast.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn manual_refresh_does_not_discard_unsaved_file_edits() {
    let file = PathBuf::from("/tmp/rmdv-manual-refresh.md");
    let mut app = App::default();
    app.file = Some(file);
    app.source = "unsaved".into();
    app.saved_source = "saved".into();
    app.dirty = true;

    let _ = app.update(Message::Refresh);

    assert_eq!(app.source, "unsaved");
    assert!(app
        .toast
        .as_ref()
        .is_some_and(|toast| { toast.text.contains("unsaved edits") }));
}

#[test]
fn manual_refresh_shows_file_refreshed_toast() {
    let dir = full_mindmap_test_dir("manual-refresh-file-toast");
    let file = dir.join("manual-refresh.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&file, "# Original\n").unwrap();
    let mut app = App::default();
    app.file = Some(file.clone());

    let _ = app.update(Message::Refresh);
    let request = app
        .pending_refresh_file
        .clone()
        .expect("file refresh should be pending");
    assert!(app.toast.is_none());

    let _ = app.update(Message::RefreshFileLoaded {
        request,
        result: Ok((file, "# Refreshed\n".into())),
    });

    assert_eq!(
        app.toast.as_ref().map(|toast| toast.text.as_str()),
        Some("File refreshed")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn manual_refresh_reports_file_failure_after_load() {
    let file = PathBuf::from("/tmp/rmdv-manual-refresh-failure.md");
    let mut app = App::default();
    app.file = Some(file);

    let _ = app.update(Message::Refresh);
    let request = app
        .pending_refresh_file
        .clone()
        .expect("file refresh should be pending");
    let _ = app.update(Message::RefreshFileLoaded {
        request,
        result: Err("read failed".into()),
    });

    assert_eq!(
        app.toast.as_ref().map(|toast| toast.text.as_str()),
        Some("File refresh failed: read failed")
    );
}

#[test]
fn manual_refresh_preserves_file_failure_after_workspace_success() {
    let dir = full_mindmap_test_dir("refresh-file-failure-workspace-success");
    let file = dir.join("current.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&file, "# Current\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.file = Some(file.clone());
    app.source = "# Current\n".into();
    app.saved_source = app.source.clone();

    let _ = app.update(Message::Refresh);
    let file_request = app
        .pending_refresh_file
        .clone()
        .expect("file refresh should be pending");
    let workspace_request = app
        .pending_refresh_workspace
        .clone()
        .expect("workspace refresh should be pending");

    let _ = app.update(Message::RefreshFileLoaded {
        request: file_request,
        result: Err("read failed".into()),
    });
    assert!(app.toast.is_none());

    let snapshot =
        tree::build_workspace(&workspace_request.path, workspace_request.show_hidden).unwrap();
    let _ = app.update(Message::RefreshWorkspaceLoaded {
        request: workspace_request,
        result: Ok((dir.clone(), snapshot)),
    });

    let expected = "File refresh failed: read failed";
    assert_eq!(app.error.as_deref(), Some(expected));
    assert_eq!(
        app.toast.as_ref().map(|toast| toast.text.as_str()),
        Some(expected)
    );
    assert!(app.pending_refresh.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn manual_refresh_preserves_workspace_failure_after_file_success() {
    let dir = full_mindmap_test_dir("refresh-workspace-failure-file-success");
    let file = dir.join("current.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&file, "# Current\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.file = Some(file.clone());
    app.source = "# Current\n".into();
    app.saved_source = app.source.clone();

    let _ = app.update(Message::Refresh);
    let file_request = app
        .pending_refresh_file
        .clone()
        .expect("file refresh should be pending");
    let workspace_request = app
        .pending_refresh_workspace
        .clone()
        .expect("workspace refresh should be pending");

    let _ = app.update(Message::RefreshWorkspaceLoaded {
        request: workspace_request,
        result: Err("scan failed".into()),
    });
    assert!(app.toast.is_none());

    let _ = app.update(Message::RefreshFileLoaded {
        request: file_request,
        result: Ok((file, "# Refreshed\n".into())),
    });

    let expected = format!(
        "Folder refresh failed: Couldn't refresh {}: scan failed",
        dir.display()
    );
    assert_eq!(app.source, "# Refreshed\n");
    assert_eq!(app.error.as_deref(), Some(expected.as_str()));
    assert_eq!(
        app.toast.as_ref().map(|toast| toast.text.as_str()),
        Some(expected.as_str())
    );
    assert!(app.pending_refresh.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn manual_refresh_rejects_file_read_started_before_edit_and_save() {
    let dir = full_mindmap_test_dir("refresh-stale-after-save");
    let file = dir.join("current.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&file, "# Original\n").unwrap();

    let mut app = App::default();
    app.file = Some(file.clone());
    app.source = "# Original\n".into();
    app.saved_source = app.source.clone();

    let _ = app.update(Message::Refresh);
    let request = app
        .pending_refresh_file
        .clone()
        .expect("file refresh should be pending");

    app.editor = Some(iced::widget::text_editor::Content::with_text(
        "# Saved edit\n",
    ));
    let _ = app.update(Message::SaveFile);
    let _ = app.update(Message::FileSaved {
        result: Ok(()),
        saved_source: "# Saved edit\n".into(),
    });
    assert!(!app.dirty);

    let _ = app.update(Message::RefreshFileLoaded {
        request,
        result: Ok((file, "# Stale pre-save read\n".into())),
    });

    assert_eq!(app.source, "# Saved edit\n");
    assert_eq!(app.saved_source, "# Saved edit\n");
    assert!(!app.dirty);
    assert_eq!(
        app.toast.as_ref().map(|toast| toast.text.as_str()),
        Some("File refresh skipped (document changed)")
    );
    assert!(app.pending_refresh.is_none());
    assert!(app.pending_refresh_file.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn successful_full_mindmap_refresh_clears_previous_refresh_error() {
    let dir = full_mindmap_test_dir("full-mindmap-refresh-retry");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("existing.md"), "# Existing\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.full_mindmap = Some(full_workspace_state(&dir));

    let _ = app.update(Message::Refresh);
    let failed_request = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_workspace_load.clone())
        .expect("first Full Mindmap refresh should be pending");
    let _ = app.update(Message::FullMindmapWorkspaceLoaded {
        request: failed_request,
        result: Err("scan failed".into()),
    });
    assert!(app.error.is_some());
    assert!(app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.load_error.as_ref())
        .is_some());

    let _ = app.update(Message::Refresh);
    let retry = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_workspace_load.clone())
        .expect("retry should replace the failed refresh");
    let snapshot = tree::build_workspace(&retry.path, app.show_hidden).unwrap();
    let _ = app.update(Message::FullMindmapWorkspaceLoaded {
        request: retry,
        result: Ok((dir.clone(), snapshot)),
    });

    assert!(app.error.is_none());
    assert!(app
        .full_mindmap
        .as_ref()
        .is_some_and(|full| full.load_error.is_none()));
    assert_eq!(
        app.toast.as_ref().map(|toast| toast.text.as_str()),
        Some("Folder refreshed")
    );
    assert!(app.pending_refresh.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn newer_refresh_rejects_an_older_generic_file_load_completion() {
    let old = PathBuf::from("/tmp/rmdv-refresh-current.md");
    let new = PathBuf::from("/tmp/rmdv-generic-load-target.md");
    let mut app = App::default();
    app.file = Some(old.clone());
    app.source = "current".into();
    app.saved_source = app.source.clone();

    let _ = app.load_file_unless_dirty(new.clone());
    let generic_generation = app.file_refresh_generation;
    let _ = app.update(Message::Refresh);
    let refresh_request = app
        .pending_refresh_file
        .clone()
        .expect("newer refresh should own the current file");
    assert_ne!(generic_generation, refresh_request.generation);

    let _ = app.update(Message::FileLoadCompleted {
        generation: generic_generation,
        result: Ok((new, "stale generic load".into())),
    });
    assert_eq!(app.file.as_deref(), Some(old.as_path()));
    assert_eq!(app.source, "current");
    assert!(app.pending_refresh.is_some());

    let _ = app.update(Message::RefreshFileLoaded {
        request: refresh_request,
        result: Ok((old.clone(), "refreshed current".into())),
    });
    assert_eq!(app.file.as_deref(), Some(old.as_path()));
    assert_eq!(app.source, "refreshed current");
    assert!(app.pending_refresh.is_none());
}

#[test]
fn newer_generic_file_load_cancels_refresh_without_stranding_tracker() {
    let old = PathBuf::from("/tmp/rmdv-refresh-before-navigation.md");
    let new = PathBuf::from("/tmp/rmdv-navigation-after-refresh.md");
    let mut app = App::default();
    app.file = Some(old.clone());
    app.source = "current".into();
    app.saved_source = app.source.clone();

    let _ = app.update(Message::Refresh);
    let stale_refresh = app
        .pending_refresh_file
        .clone()
        .expect("refresh should start first");
    let _ = app.load_file_unless_dirty(new.clone());
    let generic_generation = app.file_refresh_generation;
    assert!(app.pending_refresh.is_none());

    let _ = app.update(Message::RefreshFileLoaded {
        request: stale_refresh,
        result: Ok((old, "stale refresh".into())),
    });
    assert_eq!(app.source, "current");
    assert!(app.pending_refresh.is_none());

    let _ = app.update(Message::FileLoadCompleted {
        generation: generic_generation,
        result: Ok((new.clone(), "new navigation".into())),
    });
    assert_eq!(app.file.as_deref(), Some(new.as_path()));
    assert_eq!(app.source, "new navigation");
    assert!(app.pending_refresh.is_none());
}

#[test]
fn stale_manual_refresh_completion_is_ignored_after_navigation() {
    let old = PathBuf::from("/tmp/rmdv-refresh-old.md");
    let new = PathBuf::from("/tmp/rmdv-refresh-new.md");
    let mut app = App::default();
    app.file = Some(old.clone());
    app.source = "old".into();
    app.saved_source = "old".into();

    let _ = app.update(Message::Refresh);
    let request = app
        .pending_refresh_file
        .clone()
        .expect("file refresh should be pending");
    let _ = app.update(Message::Open(new));
    let _ = app.update(Message::RefreshFileLoaded {
        request,
        result: Ok((old, "stale refresh".into())),
    });

    assert_eq!(app.source, "old");
    assert!(app.pending_refresh.is_none());
}

#[test]
fn command_alt_file_shortcuts_accept_logical_and_physical_keys() {
    use iced::keyboard::key::{Code, Physical};
    use iced::keyboard::{Key, Modifiers};

    let modifiers = Modifiers::COMMAND | Modifiers::ALT;
    assert!(is_reveal_file_key(
        &Key::Character("r".into()),
        Physical::Code(Code::KeyR),
        modifiers,
    ));
    assert!(is_copy_file_path_key(
        &Key::Character("x".into()),
        Physical::Code(Code::KeyC),
        modifiers,
    ));
    assert!(!is_refresh_key(
        &Key::Character("r".into()),
        Physical::Code(Code::KeyR),
        modifiers,
    ));
}

#[test]
fn copy_file_path_shows_feedback_for_current_file() {
    let mut app = App::default();
    let current = PathBuf::from("/tmp/rmdv-copy-path.md");
    app.file = Some(current.clone());

    assert_eq!(app.focused_file_path(), Some(current));

    let _ = app.update(Message::CopyFilePath);

    assert_eq!(
        app.toast.as_ref().map(|toast| toast.text.as_str()),
        Some("Copying file path…")
    );
}

#[cfg(unix)]
#[test]
fn copy_file_path_rejects_non_unicode_path() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let path = PathBuf::from(OsString::from_vec(b"/tmp/rmdv-\xff.md".to_vec()));
    let mut app = App::default();
    app.file = Some(path);

    let _ = app.update(Message::CopyFilePath);

    assert_eq!(
        app.toast.as_ref().map(|toast| toast.text.as_str()),
        Some("File path cannot be copied as text")
    );
    assert!(app.pending_clipboard_copy.is_none());
}

#[test]
fn clipboard_copy_verification_shows_success_or_failure_feedback() {
    let expected = "/tmp/rmdv-copy-path.md".to_string();
    let mut success = App::default();
    success.pending_clipboard_copy = Some(expected.clone());
    let _ = success.update(Message::ClipboardCopyChecked {
        expected: expected.clone(),
        actual: Some(expected.clone()),
    });
    assert_eq!(
        success.toast.as_ref().map(|toast| toast.text.as_str()),
        Some("File path copied")
    );

    let mut failure = App::default();
    failure.pending_clipboard_copy = Some(expected.clone());
    let _ = failure.update(Message::ClipboardCopyChecked {
        expected,
        actual: None,
    });
    assert_eq!(
        failure.toast.as_ref().map(|toast| toast.text.as_str()),
        Some("Couldn't copy file path")
    );
}

#[test]
fn copy_file_path_uses_full_mindmap_file_focus() {
    let current = PathBuf::from("/tmp/rmdv-current.md");
    let focused = PathBuf::from("/tmp/rmdv-focused-in-mindmap.md");
    let mut app = App::default();
    app.file = Some(current);
    app.full_mindmap = Some(App::new_full_mindmap_state());
    app.full_mindmap.as_mut().unwrap().selected = Some(WorkspaceNodeId::File(focused.clone()));

    assert_eq!(app.focused_file_path(), Some(focused));
}

#[test]
fn copy_file_path_uses_focused_file_sidebar_row() {
    let dir = full_mindmap_test_dir("copy-focused-sidebar");
    let focused = dir.join("focused.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&focused, "# Focused\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.sidebar_open = true;
    app.sidebar_tab = SidebarTab::Files;
    app.file = Some(dir.join("current.md"));
    let rows = tree::flatten_with_files(
        app.workspace_tree.as_ref().unwrap(),
        &app.workspace_sidebar_files,
        &app.expanded,
    );
    app.tree_cursor = rows
        .iter()
        .position(|row| row.node.path() == focused)
        .expect("focused sidebar file should be visible");

    assert_eq!(app.focused_file_path(), Some(focused));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reveal_file_without_open_document_shows_feedback() {
    let mut app = App::default();

    let _ = app.update(Message::RevealFileInFinder);

    assert_eq!(
        app.toast.as_ref().map(|toast| toast.text.as_str()),
        Some("No file open")
    );
}

fn editor_key_press(
    key: iced::keyboard::Key,
    physical_key: iced::keyboard::key::Physical,
    modifiers: iced::keyboard::Modifiers,
) -> iced::widget::text_editor::KeyPress {
    iced::widget::text_editor::KeyPress {
        key: key.clone(),
        modified_key: key,
        physical_key,
        modifiers,
        text: None,
        status: iced::widget::text_editor::Status::Focused { is_hovered: false },
    }
}

#[test]
fn editor_key_binding_maps_command_arrows_to_cursor_motion() {
    use iced::keyboard::key::{Code, Named, Physical};
    use iced::keyboard::{Key, Modifiers};
    use iced::widget::text_editor::{Binding, Motion};

    assert!(matches!(
        editor_key_binding(editor_key_press(
            Key::Named(Named::ArrowLeft),
            Physical::Code(Code::ArrowLeft),
            Modifiers::COMMAND,
        )),
        Some(Binding::Move(Motion::Home))
    ));
    assert!(matches!(
        editor_key_binding(editor_key_press(
            Key::Named(Named::ArrowRight),
            Physical::Code(Code::ArrowRight),
            Modifiers::COMMAND,
        )),
        Some(Binding::Move(Motion::End))
    ));
    assert!(matches!(
        editor_key_binding(editor_key_press(
            Key::Named(Named::ArrowUp),
            Physical::Code(Code::ArrowUp),
            Modifiers::COMMAND,
        )),
        Some(Binding::Move(Motion::DocumentStart))
    ));
    assert!(matches!(
        editor_key_binding(editor_key_press(
            Key::Named(Named::ArrowDown),
            Physical::Code(Code::ArrowDown),
            Modifiers::COMMAND,
        )),
        Some(Binding::Move(Motion::DocumentEnd))
    ));
}

#[test]
fn editor_key_binding_maps_shift_command_arrows_to_selection_motion() {
    use iced::keyboard::key::{Code, Named, Physical};
    use iced::keyboard::{Key, Modifiers};
    use iced::widget::text_editor::{Binding, Motion};

    assert!(matches!(
        editor_key_binding(editor_key_press(
            Key::Named(Named::ArrowLeft),
            Physical::Code(Code::ArrowLeft),
            Modifiers::COMMAND | Modifiers::SHIFT,
        )),
        Some(Binding::Select(Motion::Home))
    ));
    assert!(matches!(
        editor_key_binding(editor_key_press(
            Key::Named(Named::ArrowDown),
            Physical::Code(Code::ArrowDown),
            Modifiers::COMMAND | Modifiers::SHIFT,
        )),
        Some(Binding::Select(Motion::DocumentEnd))
    ));
}

#[test]
fn editor_key_binding_leaves_undo_chords_to_the_app() {
    use iced::keyboard::key::{Code, Physical};
    use iced::keyboard::{Key, Modifiers};
    use iced::widget::text_editor::Binding;

    // macOS reports the letter as the key's text even with ⌘ held, so a
    // forwarded ⌘Z would insert "z" and cancel the app's EditorUndo.
    let press = |c: &str, code, modifiers| {
        let mut kp = editor_key_press(Key::Character(c.into()), Physical::Code(code), modifiers);
        kp.text = Some(c.into());
        kp
    };
    for (c, code, modifiers) in [
        ("z", Code::KeyZ, Modifiers::COMMAND),
        ("z", Code::KeyZ, Modifiers::COMMAND | Modifiers::SHIFT),
        ("y", Code::KeyY, Modifiers::COMMAND),
    ] {
        assert!(editor_key_binding(press(c, code, modifiers)).is_none());
    }
    // Clipboard chords still reach the editor, and plain letters still type.
    assert!(matches!(
        editor_key_binding(press("v", Code::KeyV, Modifiers::COMMAND)),
        Some(Binding::Paste)
    ));
    assert!(matches!(
        editor_key_binding(press("z", Code::KeyZ, Modifiers::empty())),
        Some(Binding::Insert('z'))
    ));
}

fn heading(id: u64, level: u8, label: &str) -> (BlockId, Block) {
    (
        BlockId(id),
        Block::Heading {
            level,
            id: label.to_lowercase(),
            inlines: vec![crate::ast::Inline::Text(label.to_string())],
        },
    )
}

#[test]
fn document_mindmap_fold_levels_limit_visible_heading_depth_and_zero_resets() {
    let mut app = App::default();
    app.view_mode = ViewMode::Mindmap;
    app.ast = vec![
        heading(1, 1, "Root child"),
        heading(2, 2, "Grandchild"),
        heading(3, 3, "Great-grandchild"),
        heading(4, 1, "Root sibling"),
    ];

    let (nodes, _, _) = app.mindmap_layout();
    assert_eq!(nodes.iter().map(|node| node.level).max(), Some(3));

    app.mindmap_selected = Some(BlockId(3));
    app.mindmap_panel_shown = Some(BlockId(3));
    app.mindmap_panel_open = true;
    let initial_generation = app.mindmap_layout_generation.get();
    let _ = app.update(Message::FoldToLevel(1));
    let (nodes, _, _) = app.mindmap_layout();
    assert_eq!(nodes.iter().map(|node| node.level).max(), Some(1));
    assert_eq!(
        nodes.iter().filter_map(|node| node.id).collect::<Vec<_>>(),
        vec![BlockId(1), BlockId(4)]
    );
    assert!(app.folded.is_empty());
    assert_eq!(
        app.mindmap_collapsed,
        HashSet::from([BlockId(1), BlockId(2)])
    );
    assert_eq!(app.mindmap_selected, Some(BlockId(1)));
    assert_eq!(app.mindmap_panel_shown, Some(BlockId(1)));
    assert!(app.mindmap_panel_open);
    assert_eq!(
        app.mindmap_layout_generation.get(),
        initial_generation.wrapping_add(1)
    );

    let _ = app.update(Message::FoldToLevel(2));
    let (nodes, _, _) = app.mindmap_layout();
    assert_eq!(nodes.iter().map(|node| node.level).max(), Some(2));
    assert!(nodes.iter().any(|node| node.id == Some(BlockId(2))));
    assert!(nodes.iter().all(|node| node.id != Some(BlockId(3))));

    let _ = app.update(Message::FoldToLevel(0));
    let (nodes, _, _) = app.mindmap_layout();
    assert!(app.mindmap_collapsed.is_empty());
    assert_eq!(nodes.iter().map(|node| node.level).max(), Some(3));
    assert!(nodes.iter().any(|node| node.id == Some(BlockId(3))));

    app.mindmap_collapsed.insert(BlockId(1));
    app.invalidate_mindmap_layout();
    app.full_mindmap = Some(App::new_full_mindmap_state());
    let _ = app.update(Message::FoldToLevel(0));
    assert_eq!(app.mindmap_collapsed, HashSet::from([BlockId(1)]));
}

#[test]
fn document_mindmap_fold_levels_use_structural_depth_when_headings_skip_ranks() {
    let mut app = App::default();
    app.view_mode = ViewMode::Mindmap;
    app.ast = vec![
        heading(1, 3, "Direct child"),
        heading(2, 5, "Grandchild"),
        heading(3, 6, "Great-grandchild"),
        heading(4, 2, "Direct sibling"),
    ];

    let _ = app.update(Message::FoldToLevel(1));
    let (nodes, _, _) = app.mindmap_layout();
    assert_eq!(
        nodes.iter().filter_map(|node| node.id).collect::<Vec<_>>(),
        vec![BlockId(1), BlockId(4)]
    );

    let _ = app.update(Message::FoldToLevel(2));
    let (nodes, _, _) = app.mindmap_layout();
    assert_eq!(
        nodes.iter().filter_map(|node| node.id).collect::<Vec<_>>(),
        vec![BlockId(1), BlockId(2), BlockId(4)]
    );

    let _ = app.update(Message::FoldToLevel(0));
    let (nodes, _, _) = app.mindmap_layout();
    assert_eq!(
        nodes.iter().filter_map(|node| node.id).collect::<Vec<_>>(),
        vec![BlockId(1), BlockId(2), BlockId(3), BlockId(4)]
    );
}

#[test]
fn document_mindmap_node_toggle_selects_focus_anchor_and_advances_layout_generation() {
    let mut app = App::default();
    app.view_mode = ViewMode::Mindmap;
    app.ast = vec![heading(1, 1, "Top"), heading(2, 2, "Child")];
    app.mindmap_panel_open = true;
    let generation = app.mindmap_layout_generation.get();

    let _ = app.update(Message::MindmapToggleNode(BlockId(1)));

    assert_eq!(app.mindmap_selected, Some(BlockId(1)));
    assert_eq!(app.mindmap_panel_shown, Some(BlockId(1)));
    assert!(app.mindmap_collapsed.contains(&BlockId(1)));
    assert_eq!(
        app.mindmap_layout_generation.get(),
        generation.wrapping_add(1)
    );
}

#[test]
fn data_mindmap_depth_folding_supports_json_yaml_and_toml() {
    for (source, extension) in [
        (r#"{"a":{"b":{"c":1}}}"#, "json"),
        ("a:\n  b:\n    c: 1\n", "yaml"),
        ("[a.b]\nc = 1\n", "toml"),
    ] {
        let mut app = App::default();
        app.file = Some(PathBuf::from(format!("nested.{extension}")));
        app.source = source.into();
        app.is_data_doc = true;
        app.view_mode = ViewMode::Mindmap;
        app.invalidate_mindmap_layout();

        let (nodes, _, _) = app.mindmap_layout();
        assert_eq!(
            nodes.iter().map(|node| node.level).max(),
            Some(3),
            "{extension} full graph"
        );

        let _ = app.update(Message::FoldToLevel(1));
        assert!(
            app.mindmap_layout.borrow().is_some(),
            "{extension} depth update should cache its single-pass layout"
        );
        let (nodes, _, _) = app.mindmap_layout();
        assert_eq!(
            nodes.iter().map(|node| node.level).max(),
            Some(1),
            "{extension} depth 1"
        );

        let _ = app.update(Message::FoldToLevel(0));
        let (nodes, _, _) = app.mindmap_layout();
        assert!(app.mindmap_collapsed.is_empty());
        assert_eq!(
            nodes.iter().map(|node| node.level).max(),
            Some(3),
            "{extension} reset"
        );

        app.mindmap_selected = Some(BlockId(0));
        let _ = app.update(Message::MindmapToggleSelected);
        let (nodes, _, paths) = app.mindmap_layout();
        assert_eq!(nodes.len(), 1, "{extension} collapsed root");
        assert!(nodes[0].has_hidden_children, "{extension} collapsed root");
        assert_eq!(paths.len(), 1, "{extension} root-only preview paths");

        let _ = app.update(Message::MindmapToggleSelected);
        let (nodes, _, _) = app.mindmap_layout();
        assert_eq!(
            nodes.iter().map(|node| node.level).max(),
            Some(3),
            "{extension} expanded root"
        );
    }
}

#[test]
fn fold_level_shortcut_accepts_document_depths_zero_through_six() {
    use iced::keyboard::Key;

    for depth in 0..=6 {
        assert!(matches!(
            fold_level_shortcut(&Key::Character(depth.to_string().into())),
            Some(Message::FoldToLevel(actual)) if actual == depth
        ));
    }
    assert!(fold_level_shortcut(&Key::Character("7".into())).is_none());
}

#[test]
fn command_palette_exposes_node_depths_only_in_document_mindmap() {
    let depth_commands = |app: &App| {
        app.command_items()
            .into_iter()
            .filter(|(label, _)| label.starts_with("Mindmap: Show"))
            .collect::<Vec<_>>()
    };

    let mut app = App::default();
    assert!(depth_commands(&app).is_empty());

    app.view_mode = ViewMode::Mindmap;
    let commands = depth_commands(&app);
    assert_eq!(
        commands.iter().map(|(label, _)| *label).collect::<Vec<_>>(),
        vec![
            "Mindmap: Show All Node Levels  ⌘K 0",
            "Mindmap: Show 1 Node Level  ⌘K 1",
            "Mindmap: Show 2 Node Levels  ⌘K 2",
            "Mindmap: Show 3 Node Levels  ⌘K 3",
            "Mindmap: Show 4 Node Levels  ⌘K 4",
            "Mindmap: Show 5 Node Levels  ⌘K 5",
            "Mindmap: Show 6 Node Levels  ⌘K 6",
        ]
    );
    for (depth, (_, message)) in commands.iter().enumerate() {
        assert!(matches!(
            message,
            Message::FoldToLevel(actual) if *actual == depth as u8
        ));
    }

    app.is_data_doc = true;
    assert_eq!(depth_commands(&app).len(), 7);

    app.full_mindmap = Some(App::new_full_mindmap_state());
    assert!(depth_commands(&app).is_empty());
}

#[test]
fn editor_key_binding_still_blocks_non_editor_command_chords() {
    use iced::keyboard::key::{Code, Physical};
    use iced::keyboard::{Key, Modifiers};

    assert!(editor_key_binding(editor_key_press(
        Key::Character("b".into()),
        Physical::Code(Code::KeyB),
        Modifiers::COMMAND,
    ))
    .is_none());
}

#[test]
fn mindmap_preserves_reader_font_chords_and_guards_obscured_surfaces() {
    use iced::keyboard::{Key, Modifiers};

    assert!(reader_font_shortcuts_enabled(
        false, true, false, false, false
    ));
    assert!(!reader_font_shortcuts_enabled(
        false, true, true, false, false
    ));
    assert!(!reader_font_shortcuts_enabled(
        false, true, false, true, false
    ));
    assert!(!reader_font_shortcuts_enabled(
        false, true, false, false, true
    ));
    assert!(!reader_font_shortcuts_enabled(
        true, false, false, false, false
    ));

    assert!(matches!(
        reader_font_size_shortcut(&Key::Character("+".into()), Modifiers::COMMAND,),
        Some(Message::FontSizeUp)
    ));
    assert!(matches!(
        reader_font_size_shortcut(&Key::Character("-".into()), Modifiers::CTRL,),
        Some(Message::FontSizeDown)
    ));
    assert!(matches!(
        reader_font_size_shortcut(&Key::Character("0".into()), Modifiers::COMMAND,),
        Some(Message::FontSizeReset)
    ));
    assert!(reader_font_size_shortcut(&Key::Character("+".into()), Modifiers::NONE,).is_none());
}

#[test]
fn quick_slot_shortcut_hints_stay_within_four_characters() {
    for (keys, _) in QUICK_SLOT_SHORTCUT_HINTS {
        assert!(
            keys.chars().count() <= 4,
            "Quick Slot shortcut hint `{keys}` exceeds the four-character limit"
        );
    }
}

#[test]
fn quick_slot_shortcut_hints_include_close_active_slot() {
    assert!(QUICK_SLOT_SHORTCUT_HINTS.contains(&("⌘W", "Close active slot")));
}

#[test]
fn shortcuts_key_accepts_logical_and_physical_slash() {
    use iced::keyboard::key::{Code, Physical};
    use iced::keyboard::{Key, Modifiers};

    assert!(is_shortcuts_key(
        &Key::Character("/".into()),
        Physical::Code(Code::Slash),
        Modifiers::COMMAND,
    ));
    assert!(is_shortcuts_key(
        &Key::Character("?".into()),
        Physical::Code(Code::Slash),
        Modifiers::COMMAND,
    ));
    assert!(!is_shortcuts_key(
        &Key::Character("/".into()),
        Physical::Code(Code::Slash),
        Modifiers::NONE,
    ));
}

#[test]
fn refresh_key_accepts_logical_and_physical_key_r() {
    use iced::keyboard::key::{Code, Physical};
    use iced::keyboard::{Key, Modifiers};

    assert!(is_refresh_key(
        &Key::Character("r".into()),
        Physical::Code(Code::KeyR),
        Modifiers::COMMAND,
    ));
    assert!(is_refresh_key(
        &Key::Character("R".into()),
        Physical::Code(Code::KeyR),
        Modifiers::CTRL,
    ));
    assert!(!is_refresh_key(
        &Key::Character("r".into()),
        Physical::Code(Code::KeyR),
        Modifiers::NONE,
    ));
}

#[test]
fn zen_editor_bottom_inset_clears_floating_shortcut_layers() {
    assert_eq!(zen_editor_bottom_inset(false), 40.0);
    assert_eq!(zen_editor_bottom_inset(true), 72.0);
    assert_eq!(
        zen_editor_bottom_inset(true),
        KEYBOARD_BUTTON_FOOTER_BOTTOM_PAD + KEYBOARD_BUTTON_HEIGHT + ZEN_EDITOR_OVERLAY_CLEARANCE
    );
}

#[test]
fn zen_entry_hides_sidebar_search_and_preserves_footer() {
    let mut app = App::default();
    app.file = Some(std::path::PathBuf::from("note.md"));
    app.source = "# Title\n\nBody".into();
    app.sidebar_open = true;
    app.show_footer = true;
    app.search_open = true;
    app.overlay = Overlay::Command;

    let _ = app.enter_zen_edit_mode();

    assert_eq!(app.view_mode, ViewMode::Raw);
    assert!(app.editor.is_some());
    assert!(!app.sidebar_open);
    assert!(app.show_footer);
    assert!(!app.search_open);
    assert_eq!(app.overlay, Overlay::None);
    assert!(app.zen_restore.is_some());
}

#[test]
fn zen_exit_restores_saved_chrome_state() {
    let mut app = App::default();
    app.file = Some(std::path::PathBuf::from("note.md"));
    app.source = "before".into();
    app.sidebar_open = true;
    app.show_footer = false;
    app.search_open = true;

    let _ = app.enter_zen_edit_mode();
    assert!(!app.show_footer);
    let _ = app.exit_zen_edit_mode();

    assert_eq!(app.view_mode, ViewMode::Rendered);
    assert!(app.sidebar_open);
    assert!(!app.show_footer);
    assert!(app.search_open);
    assert!(app.zen_restore.is_none());
}

#[test]
fn footer_visibility_persists_across_zen_view_cycles() {
    let mut app = App::default();
    app.file = Some(std::path::PathBuf::from("note.md"));
    app.source = "before".into();
    app.show_footer = true;

    let _ = app.enter_zen_edit_mode();
    // Simulate toggling the footer off while editing.
    app.show_footer = false;
    let _ = app.exit_zen_edit_mode();

    assert!(!app.show_footer);

    let _ = app.enter_zen_edit_mode();
    assert!(!app.show_footer);
    let _ = app.exit_zen_edit_mode();
    assert!(!app.show_footer);
}

#[test]
fn clean_file_loaded_while_in_zen_clears_editor_and_restores_chrome() {
    let mut app = App::default();
    app.file = Some(std::path::PathBuf::from("old.md"));
    app.source = "old file".into();
    app.sidebar_open = true;
    app.show_footer = false;
    app.search_open = true;

    let _ = app.enter_zen_edit_mode();
    app.editor = Some(iced::widget::text_editor::Content::with_text("old file"));
    app.dirty = false;

    let new_path = std::path::PathBuf::from("new.md");
    let _ = app.update(Message::FileLoaded(Ok((
        new_path.clone(),
        "new file".into(),
    ))));

    assert_eq!(app.view_mode, ViewMode::Rendered);
    assert_eq!(app.file.as_deref(), Some(new_path.as_path()));
    assert_eq!(app.source, "new file");
    assert!(app.editor.is_none());
    assert!(!app.dirty);
    assert!(app.sidebar_open);
    assert!(!app.show_footer);
    assert!(app.search_open);
    assert!(app.zen_restore.is_none());
}

#[test]
fn dirty_file_loaded_while_in_zen_keeps_editor_and_current_file() {
    let mut app = App::default();
    let old_path = std::path::PathBuf::from("old.md");
    app.file = Some(old_path.clone());
    app.source = "old file".into();
    app.sidebar_open = true;
    app.show_footer = false;
    app.search_open = true;

    let _ = app.enter_zen_edit_mode();
    app.editor = Some(iced::widget::text_editor::Content::with_text(
        "old edited text",
    ));
    app.dirty = true;
    app.pending_nav = Some(PendingNav {
        line: Some(12),
        ..Default::default()
    });

    let _ = app.update(Message::FileLoaded(Ok((
        std::path::PathBuf::from("new.md"),
        "new file".into(),
    ))));

    assert_eq!(app.view_mode, ViewMode::Raw);
    assert_eq!(app.file.as_deref(), Some(old_path.as_path()));
    assert_eq!(app.source, "old file");
    assert_eq!(
        app.editor.as_ref().map(|ed| ed.text()),
        Some("old edited text".to_string())
    );
    assert!(app.dirty);
    assert!(app.pending_nav.is_none());
    assert!(!app.sidebar_open);
    assert!(!app.show_footer);
    assert!(!app.search_open);
    assert!(app.zen_restore.is_some());
    assert!(app
        .toast
        .as_ref()
        .is_some_and(|toast| toast.text.contains("unsaved edits")));
}

#[test]
fn zen_exit_and_reentry_preserve_unsaved_document_state() {
    let mut app = App::default();
    let old_path = std::path::PathBuf::from("old.md");
    app.file = Some(old_path.clone());
    app.source = "saved text".into();
    app.saved_source = app.source.clone();

    let _ = app.enter_zen_edit_mode();
    app.editor = Some(iced::widget::text_editor::Content::with_text(
        "unsaved text",
    ));
    app.dirty = true;
    let _ = app.exit_zen_edit_mode();

    assert_eq!(app.source, "unsaved text");
    assert!(app.dirty);

    let _ = app.enter_zen_edit_mode();
    assert!(app.dirty);
    let _ = app.update(Message::FileLoaded(Ok((
        std::path::PathBuf::from("new.md"),
        "new file".into(),
    ))));

    assert_eq!(app.file.as_deref(), Some(old_path.as_path()));
    assert_eq!(app.source, "unsaved text");
    assert!(app.dirty);
}

#[test]
fn failed_save_keeps_file_switch_guard_armed() {
    let mut app = App::default();
    app.file = Some(std::path::PathBuf::from("note.md"));
    app.source = "unsaved text".into();
    app.saved_source = "saved text".into();
    app.dirty = false; // Mimic the old optimistic-save state.

    let _ = app.update(Message::FileSaved {
        result: Err("disk full".into()),
        saved_source: "unsaved text".into(),
    });

    assert!(app.dirty);
    assert!(app
        .error
        .as_ref()
        .is_some_and(|error| error.contains("disk full")));
}

#[test]
fn completed_save_updates_the_persisted_baseline() {
    let mut app = App::default();
    app.file = Some(std::path::PathBuf::from("note.md"));
    app.source = "saved text".into();
    app.saved_source = "old text".into();
    app.dirty = true;

    let _ = app.update(Message::FileSaved {
        result: Ok(()),
        saved_source: "saved text".into(),
    });

    assert_eq!(app.saved_source, "saved text");
    assert!(!app.dirty);
}

#[test]
fn file_finder_open_while_dirty_blocks_load_and_returns_to_editor() {
    let mut app = App::default();
    let old_path = std::path::PathBuf::from("old.md");
    let new_path = std::path::PathBuf::from("new.md");
    app.file = Some(old_path.clone());
    app.source = "old file".into();
    app.workspace = Some(std::path::PathBuf::from("."));
    app.workspace_files = vec![new_path];
    app.sidebar_open = true;
    app.show_footer = false;
    app.search_open = true;

    let _ = app.enter_zen_edit_mode();
    app.overlay = Overlay::FileFinder;
    app.overlay_selected = 0;
    app.editor = Some(iced::widget::text_editor::Content::with_text(
        "old edited text",
    ));
    app.dirty = true;

    let _ = app.update(Message::OverlayConfirm);

    assert_eq!(app.view_mode, ViewMode::Raw);
    assert_eq!(app.file.as_deref(), Some(old_path.as_path()));
    assert_eq!(
        app.editor.as_ref().map(|ed| ed.text()),
        Some("old edited text".to_string())
    );
    assert!(app.dirty);
    assert_eq!(app.overlay, Overlay::None);
    assert!(app
        .toast
        .as_ref()
        .is_some_and(|toast| toast.text.contains("unsaved edits")));
}

#[test]
fn dirty_vault_hit_does_not_close_search_or_queue_navigation() {
    let mut app = App::default();
    app.file = Some(std::path::PathBuf::from("old.md"));
    app.source = "old file".into();
    app.dirty = true;
    app.vault_open = true;
    app.vault_results.push(crate::vault_search::VaultHit {
        path: std::path::PathBuf::from("new.md"),
        line: 7,
        col_start: 0,
        col_end: 4,
        context: Vec::new(),
    });

    let _ = app.update(Message::VaultOpenHit(0));

    assert!(app.vault_open);
    assert!(app.pending_nav.is_none());
    assert!(app
        .toast
        .as_ref()
        .is_some_and(|toast| toast.text.contains("unsaved edits")));
}

#[test]
fn dirty_local_link_does_not_queue_navigation() {
    let dir = std::env::temp_dir().join(format!("rmdv-dirty-link-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("target.md"), "# Target\n").unwrap();

    let mut app = App::default();
    let current = dir.join("current.md");
    app.file = Some(current.clone());
    app.source = "old file".into();
    app.dirty = true;

    let _ = app.update(Message::OpenLink("target.md#target".into()));

    assert_eq!(app.file.as_deref(), Some(current.as_path()));
    assert!(app.dirty);
    assert!(app.pending_nav.is_none());
    assert!(app
        .toast
        .as_ref()
        .is_some_and(|toast| toast.text.contains("unsaved edits")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn scroll_to_line_while_in_zen_syncs_editor_and_restores_chrome() {
    let mut app = App::default();
    app.file = Some(std::path::PathBuf::from("note.md"));
    app.source = "# One\n\nBody\n\n# Two\n".into();
    app.load_ast_from_source();
    app.sidebar_open = true;
    app.show_footer = false;
    app.search_open = true;

    let target_line = line_for_fragment(&app.source, "two", false).unwrap();
    let _ = app.enter_zen_edit_mode();
    app.editor = Some(iced::widget::text_editor::Content::with_text(
        "# One\n\nEdited body\n\n# Two\n",
    ));

    let _ = app.update(Message::ScrollToLine(target_line));

    assert_eq!(app.view_mode, ViewMode::Rendered);
    assert_eq!(app.source, "# One\n\nEdited body\n\n# Two\n");
    assert!(app.editor.is_none());
    assert!(app.sidebar_open);
    assert!(!app.show_footer);
    assert!(app.search_open);
    assert!(app.zen_restore.is_none());
}

fn loaded_image(bytes: usize) -> ImageState {
    ImageState::Loaded(iced::widget::image::Handle::from_bytes(vec![0u8; bytes]))
}

#[test]
fn image_cache_trim_evicts_oldest_unreferenced_first() {
    let mut cache = ImageCache::default();
    cache.insert("a".into(), loaded_image(400));
    cache.insert("b".into(), loaded_image(400));
    cache.insert("c".into(), loaded_image(400));
    assert_eq!(cache.cost_bytes(), 1200);
    cache.trim(800, |_| false);
    assert!(!cache.contains_key("a"), "oldest entry should evict first");
    assert!(cache.contains_key("b"));
    assert!(cache.contains_key("c"));
}

#[test]
fn image_cache_trim_never_evicts_kept_keys() {
    let mut cache = ImageCache::default();
    cache.insert("current-doc".into(), loaded_image(400));
    cache.insert("old-doc".into(), loaded_image(400));
    cache.trim(0, |k| k == "current-doc");
    assert!(cache.contains_key("current-doc"));
    assert!(!cache.contains_key("old-doc"));
}

#[test]
fn image_cache_trim_noop_under_budget() {
    let mut cache = ImageCache::default();
    cache.insert("a".into(), loaded_image(100));
    cache.insert("b".into(), loaded_image(100));
    cache.trim(1024, |_| false);
    assert_eq!(cache.len(), 2);
}

#[test]
fn image_cache_reinsert_does_not_duplicate_order() {
    let mut cache = ImageCache::default();
    cache.insert("a".into(), ImageState::Loading);
    cache.insert("a".into(), loaded_image(400));
    cache.insert("b".into(), loaded_image(400));
    cache.trim(500, |_| false);
    // "a" (oldest) evicted exactly once; "b" stays.
    assert!(!cache.contains_key("a"));
    assert!(cache.contains_key("b"));
    assert_eq!(cache.len(), 1);
}

#[test]
fn image_cache_svg_cost_counts_payload_twice_plus_raster() {
    let state = ImageState::LoadedSvg {
        svg: iced::widget::svg::Handle::from_memory(vec![0u8; 100]),
        bytes: std::sync::Arc::new(vec![0u8; 100]),
        raster: Some(iced::widget::image::Handle::from_rgba(5, 5, vec![0u8; 100])),
    };
    assert_eq!(image_state_cost(&state), 300);
}

fn full_workspace_state(root: &std::path::Path) -> FullMindmapState {
    fn materialize(path: &std::path::Path, state: &mut FullMindmapState) {
        if let Ok(snapshot) = tree::load_expanded_folder(path, true) {
            state.materialized_folders.insert(
                path.to_path_buf(),
                workspace_mindmap::MaterializedFolder::Loaded {
                    folders: std::sync::Arc::new(snapshot.folders),
                    files: std::sync::Arc::new(snapshot.files),
                    recursive_supported_file_count: snapshot.recursive_supported_file_count,
                    truncated: snapshot.truncated,
                },
            );
        }
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    materialize(&entry.path(), state);
                }
            }
        }
    }

    let mut state = App::new_full_mindmap_state();
    state.expanded.insert(root.to_path_buf());
    state.selected = Some(WorkspaceNodeId::Root(root.to_path_buf()));
    materialize(root, &mut state);
    state
}

fn full_mindmap_test_dir(label: &str) -> std::path::PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root =
        std::fs::canonicalize(std::env::temp_dir()).unwrap_or_else(|_| std::env::temp_dir());
    temp_root.join(format!(
        "rmdv-full-mindmap-{label}-{}-{stamp}",
        std::process::id()
    ))
}

fn verification_test_app(
    candidate_count: usize,
) -> (App, std::path::PathBuf, Vec<std::path::PathBuf>) {
    let root = std::path::PathBuf::from("/verification-root");
    let candidates = (0..candidate_count)
        .map(|index| root.join(format!("candidate-{index}")))
        .collect::<Vec<_>>();
    let children = candidates
        .iter()
        .map(|path| Node {
            path: path.clone(),
            name: path.file_name().unwrap().to_string_lossy().into_owned(),
            is_dir: true,
            children: Vec::new(),
            recursive_supported_file_count: Some(tree::RecursiveFileCount::LowerBound(0)),
        })
        .collect();
    let root_node = Node {
        path: root.clone(),
        name: "verification-root".into(),
        is_dir: true,
        children,
        recursive_supported_file_count: Some(tree::RecursiveFileCount::LowerBound(0)),
    };
    let mut app = App::default();
    app.workspace = Some(root.clone());
    app.workspace_tree = Some(root_node);
    app.workspace_snapshot_show_hidden = false;
    let mut full = App::new_full_mindmap_state();
    full.expanded.insert(root.clone());
    full.selected = Some(WorkspaceNodeId::Root(root.clone()));
    app.full_mindmap = Some(full);
    (app, root, candidates)
}

fn verification_request_for(app: &App, folder: &std::path::Path) -> PendingFullMindmapVerification {
    let full = app.full_mindmap.as_ref().unwrap();
    let wave = full.verification_wave.as_ref().unwrap();
    PendingFullMindmapVerification {
        id: *wave.request_ids.get(folder).unwrap(),
        wave_id: wave.id,
        workspace_root: wave.workspace_root.clone(),
        parent: wave
            .parent_by_candidate
            .get(folder)
            .cloned()
            .unwrap_or_else(|| wave.workspace_root.clone()),
        parent_expansion_generation: wave.parent_expansion_generation,
        folder: folder.to_path_buf(),
        show_hidden: wave.show_hidden,
    }
}

#[test]
fn full_mindmap_verification_wave_hides_unknowns_and_reveals_positive_counts() {
    let (mut app, root, candidates) = verification_test_app(1);
    let _ = app.begin_full_mindmap_verification_wave();
    assert_eq!(
        app.full_mindmap_progress,
        Some(FullMindmapProgress {
            wave_id: app
                .full_mindmap
                .as_ref()
                .unwrap()
                .verification_wave
                .as_ref()
                .unwrap()
                .id,
            checked: 0,
            total: 1,
        })
    );
    assert!(app
        .full_mindmap_graph()
        .unwrap()
        .node(&WorkspaceNodeId::Folder(candidates[0].clone()))
        .is_none());

    let request = verification_request_for(&app, &candidates[0]);
    let _ = app.update(Message::FullMindmapVerificationLoaded {
        request,
        result: Ok((
            candidates[0].clone(),
            tree::ExpandedFolderSnapshot {
                folders: Vec::new(),
                files: vec![candidates[0].join("readme.md"); 3],
                recursive_supported_file_count: tree::RecursiveFileCount::Exact(3),
                truncated: false,
            },
        )),
    });
    let graph = app.full_mindmap_graph().unwrap();
    assert_eq!(
        graph
            .nodes
            .iter()
            .find(|node| node.id.as_ref() == Some(&WorkspaceNodeId::Folder(candidates[0].clone())))
            .map(|node| node.full_label.as_str()),
        Some("candidate-0 · 3 files")
    );
    assert!(matches!(
        app.full_mindmap
            .as_ref()
            .unwrap()
            .materialized_folders
            .get(&candidates[0]),
        Some(workspace_mindmap::MaterializedFolder::Verified {
            recursive_supported_file_count: tree::RecursiveFileCount::Exact(3),
            ..
        })
    ));
    // Verification reveals only the recursive count. Expanding the
    // folder removes that count-only fact and owns the real branch load.
    let _ = app.update(Message::FullMindmapToggleNode(WorkspaceNodeId::Folder(
        candidates[0].clone(),
    )));
    let full = app.full_mindmap.as_ref().unwrap();
    assert!(full.pending_folder_loads.contains_key(&candidates[0]));
    assert!(!full.materialized_folders.contains_key(&candidates[0]));
    assert_eq!(app.full_mindmap_progress, None);
    assert_eq!(app.workspace.as_deref(), Some(root.as_path()));
}

#[test]
fn full_mindmap_deselect_survives_exact_empty_verification_normalization() {
    let (mut app, _root, candidates) = verification_test_app(1);
    let _ = app.begin_full_mindmap_verification_wave();
    let request = verification_request_for(&app, &candidates[0]);

    let _ = app.update(Message::FullMindmapDeselect);
    let full = app.full_mindmap.as_ref().unwrap();
    assert_eq!(full.selected, None);
    assert_eq!(full.focus_request, None);
    assert!(!full.panel_open);
    assert!(full.verification_wave.is_some());

    let _ = app.update(Message::FullMindmapVerificationLoaded {
        request,
        result: Ok((
            candidates[0].clone(),
            tree::ExpandedFolderSnapshot {
                folders: Vec::new(),
                files: Vec::new(),
                recursive_supported_file_count: tree::RecursiveFileCount::Exact(0),
                truncated: false,
            },
        )),
    });
    let full = app.full_mindmap.as_ref().unwrap();
    assert_eq!(full.selected, None);
    assert_eq!(full.focus_request, None);
    assert!(!full.panel_open);
}

#[test]
fn full_mindmap_verification_wave_is_fixed_and_bounded() {
    let (mut app, _root, candidates) =
        verification_test_app(FULL_MINDMAP_VERIFICATION_CONCURRENCY + 1);
    let _ = app.begin_full_mindmap_verification_wave();
    let wave = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .verification_wave
        .as_ref()
        .unwrap();
    assert_eq!(
        wave.candidates.len(),
        FULL_MINDMAP_VERIFICATION_CONCURRENCY + 1
    );
    assert_eq!(wave.in_flight.len(), FULL_MINDMAP_VERIFICATION_CONCURRENCY);
    assert_eq!(wave.next_index, FULL_MINDMAP_VERIFICATION_CONCURRENCY);
    assert_eq!(app.full_mindmap_progress.unwrap().total, candidates.len());

    let request = verification_request_for(&app, &candidates[0]);
    let _ = app.update(Message::FullMindmapVerificationLoaded {
        request,
        result: Err("permission denied".into()),
    });
    let wave = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .verification_wave
        .as_ref()
        .unwrap();
    assert_eq!(wave.checked, 1);
    assert_eq!(wave.next_index, candidates.len());
    assert_eq!(app.full_mindmap_progress.unwrap().checked, 1);
    let folder = WorkspaceNodeId::Folder(candidates[0].clone());
    let graph = app.full_mindmap_graph().unwrap();
    let index = graph
        .index_of(&folder)
        .expect("unavailable folder remains visible");
    assert_eq!(
        graph.nodes[index].full_label,
        "candidate-0 · count unavailable"
    );
}

#[test]
fn full_mindmap_verification_overflow_leaves_exact_unqueued_excess_visible() {
    let excess = 3;
    let total = FULL_MINDMAP_VERIFICATION_MAX_CANDIDATES + excess;
    let (mut app, _root, candidates) = verification_test_app(total);
    let _ = app.begin_full_mindmap_verification_wave();
    let wave = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .verification_wave
        .as_ref()
        .unwrap();
    assert_eq!(
        wave.candidates.len(),
        FULL_MINDMAP_VERIFICATION_MAX_CANDIDATES
    );
    assert_eq!(
        app.full_mindmap_progress.unwrap().total,
        FULL_MINDMAP_VERIFICATION_MAX_CANDIDATES
    );

    let graph = app.full_mindmap_graph().unwrap();
    let excess_visible = candidates[FULL_MINDMAP_VERIFICATION_MAX_CANDIDATES..]
        .iter()
        .filter_map(|path| {
            graph
                .index_of(&WorkspaceNodeId::Folder(path.clone()))
                .map(|index| &graph.nodes[index].full_label)
        })
        .collect::<Vec<_>>();
    assert_eq!(excess_visible.len(), excess);
    assert!(excess_visible
        .iter()
        .all(|label| label.ends_with(" · scan limit reached")));
}

fn followup_verification_test_app() -> (
    App,
    PathBuf,
    PathBuf,
    PathBuf,
    PathBuf,
    PendingFullMindmapFolderLoad,
) {
    let root = PathBuf::from("/followup-root");
    let branch = root.join("branch");
    let child = branch.join("new-child");
    let other = root.join("other");
    let root_node = Node {
        path: root.clone(),
        name: "followup-root".into(),
        is_dir: true,
        children: vec![
            Node {
                path: branch.clone(),
                name: "branch".into(),
                is_dir: true,
                children: Vec::new(),
                recursive_supported_file_count: Some(tree::RecursiveFileCount::LowerBound(1)),
            },
            Node {
                path: other.clone(),
                name: "other".into(),
                is_dir: true,
                children: Vec::new(),
                recursive_supported_file_count: Some(tree::RecursiveFileCount::LowerBound(0)),
            },
        ],
        recursive_supported_file_count: Some(tree::RecursiveFileCount::LowerBound(1)),
    };
    let mut app = App::default();
    app.workspace = Some(root.clone());
    app.workspace_tree = Some(root_node);
    app.workspace_snapshot_show_hidden = false;
    let mut full = App::new_full_mindmap_state();
    full.expanded.extend([root.clone(), branch.clone()]);
    full.selected = Some(WorkspaceNodeId::Folder(branch.clone()));
    let pending = PendingFullMindmapFolderLoad {
        id: 900,
        workspace_root: root.clone(),
        folder: branch.clone(),
        show_hidden: false,
    };
    full.pending_folder_loads
        .insert(branch.clone(), pending.clone());
    app.full_mindmap = Some(full);
    (app, root, branch, child, other, pending)
}

#[test]
fn full_mindmap_materialized_child_stays_hidden_until_followup_exact_zero() {
    let (mut app, root, branch, child, other, pending) = followup_verification_test_app();
    let _ = app.begin_full_mindmap_verification_wave();
    let old_wave_id = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .verification_wave
        .as_ref()
        .unwrap()
        .id;
    let branch_snapshot = tree::ExpandedFolderSnapshot {
        folders: vec![Node {
            path: child.clone(),
            name: "new-child".into(),
            is_dir: true,
            children: Vec::new(),
            recursive_supported_file_count: Some(tree::RecursiveFileCount::LowerBound(0)),
        }],
        files: Vec::new(),
        recursive_supported_file_count: tree::RecursiveFileCount::LowerBound(1),
        truncated: false,
    };
    let _ = app.update(Message::FullMindmapFolderLoaded {
        request: pending,
        result: Ok((branch.clone(), branch_snapshot)),
    });
    assert!(app
        .full_mindmap_graph()
        .unwrap()
        .node(&WorkspaceNodeId::Folder(child.clone()))
        .is_none());
    assert!(
        app.full_mindmap
            .as_ref()
            .unwrap()
            .verification_followup_pending
    );

    let old_request = verification_request_for(&app, &other);
    assert_eq!(old_request.wave_id, old_wave_id);
    let _ = app.update(Message::FullMindmapVerificationLoaded {
        request: old_request,
        result: Ok((
            other.clone(),
            tree::ExpandedFolderSnapshot {
                folders: Vec::new(),
                files: Vec::new(),
                recursive_supported_file_count: tree::RecursiveFileCount::Exact(2),
                truncated: false,
            },
        )),
    });
    let followup_request = verification_request_for(&app, &child);
    assert_ne!(followup_request.wave_id, old_wave_id);
    assert!(app
        .full_mindmap_graph()
        .unwrap()
        .node(&WorkspaceNodeId::Folder(child.clone()))
        .is_none());

    let _ = app.update(Message::FullMindmapVerificationLoaded {
        request: followup_request,
        result: Ok((
            child.clone(),
            tree::ExpandedFolderSnapshot {
                folders: Vec::new(),
                files: Vec::new(),
                recursive_supported_file_count: tree::RecursiveFileCount::Exact(0),
                truncated: false,
            },
        )),
    });
    assert!(app
        .full_mindmap_graph()
        .unwrap()
        .node(&WorkspaceNodeId::Folder(child))
        .is_none());
    assert_eq!(app.workspace.as_deref(), Some(root.as_path()));
}

#[test]
fn full_mindmap_collapse_restarts_wave_for_other_expanded_parents() {
    let root = PathBuf::from("/collapse-restart-root");
    let left = root.join("left");
    let right = root.join("right");
    let left_child = left.join("left-child");
    let right_child = right.join("right-child");
    let child = |path: PathBuf| Node {
        name: path.file_name().unwrap().to_string_lossy().into_owned(),
        path,
        is_dir: true,
        children: Vec::new(),
        recursive_supported_file_count: Some(tree::RecursiveFileCount::LowerBound(0)),
    };
    let root_node = Node {
        path: root.clone(),
        name: "collapse-restart-root".into(),
        is_dir: true,
        children: vec![
            Node {
                path: left.clone(),
                name: "left".into(),
                is_dir: true,
                children: vec![child(left_child.clone())],
                recursive_supported_file_count: Some(tree::RecursiveFileCount::LowerBound(1)),
            },
            Node {
                path: right.clone(),
                name: "right".into(),
                is_dir: true,
                children: vec![child(right_child.clone())],
                recursive_supported_file_count: Some(tree::RecursiveFileCount::LowerBound(1)),
            },
        ],
        recursive_supported_file_count: Some(tree::RecursiveFileCount::LowerBound(2)),
    };
    let mut app = App::default();
    app.workspace = Some(root.clone());
    app.workspace_tree = Some(root_node);
    app.workspace_snapshot_show_hidden = false;
    let mut full = App::new_full_mindmap_state();
    full.expanded
        .extend([root.clone(), left.clone(), right.clone()]);
    full.selected = Some(WorkspaceNodeId::Folder(left.clone()));
    app.full_mindmap = Some(full);
    let _ = app.begin_full_mindmap_verification_wave();
    assert!(app
        .full_mindmap_graph()
        .unwrap()
        .node(&WorkspaceNodeId::Folder(right_child.clone()))
        .is_none());

    let _ = app.update(Message::FullMindmapToggleNode(WorkspaceNodeId::Folder(
        left.clone(),
    )));
    assert!(!app.full_mindmap.as_ref().unwrap().expanded.contains(&left));
    assert!(app
        .full_mindmap_graph()
        .unwrap()
        .node(&WorkspaceNodeId::Folder(right_child))
        .is_none());
}

#[test]
fn full_mindmap_stale_prior_wave_completion_cannot_reveal_after_restart() {
    let (mut app, _root, candidates) = verification_test_app(1);
    let _ = app.begin_full_mindmap_verification_wave();
    let stale = verification_request_for(&app, &candidates[0]);
    let old_wave_id = stale.wave_id;
    let _ = app.begin_full_mindmap_verification_wave();
    let current = verification_request_for(&app, &candidates[0]);
    assert_ne!(current.wave_id, old_wave_id);
    let _ = app.update(Message::FullMindmapVerificationLoaded {
        request: stale,
        result: Ok((
            candidates[0].clone(),
            tree::ExpandedFolderSnapshot {
                folders: Vec::new(),
                files: Vec::new(),
                recursive_supported_file_count: tree::RecursiveFileCount::Exact(9),
                truncated: false,
            },
        )),
    });
    assert_eq!(
        app.full_mindmap
            .as_ref()
            .unwrap()
            .verification_wave
            .as_ref()
            .unwrap()
            .checked,
        0
    );
    assert!(app
        .full_mindmap_graph()
        .unwrap()
        .node(&WorkspaceNodeId::Folder(candidates[0].clone()))
        .is_none());
}

#[test]
fn full_mindmap_verification_stale_result_cannot_reveal_after_cancel() {
    let (mut app, _root, candidates) = verification_test_app(1);
    let _ = app.begin_full_mindmap_verification_wave();
    let request = verification_request_for(&app, &candidates[0]);
    app.cancel_full_mindmap_verification();
    assert!(app
        .full_mindmap_graph()
        .unwrap()
        .node(&WorkspaceNodeId::Folder(candidates[0].clone()))
        .is_some());
    let _ = app.update(Message::FullMindmapVerificationLoaded {
        request,
        result: Ok((
            candidates[0].clone(),
            tree::ExpandedFolderSnapshot {
                folders: Vec::new(),
                files: Vec::new(),
                recursive_supported_file_count: tree::RecursiveFileCount::Exact(4),
                truncated: false,
            },
        )),
    });
    assert!(app.full_mindmap_progress.is_none());
    assert!(app
        .full_mindmap_graph()
        .unwrap()
        .node(&WorkspaceNodeId::Folder(candidates[0].clone()))
        .is_some());
}

#[test]
fn full_mindmap_verification_rejects_same_wave_wrong_request_id() {
    let (mut app, _root, candidates) = verification_test_app(1);
    let _ = app.begin_full_mindmap_verification_wave();
    let mut request = verification_request_for(&app, &candidates[0]);
    request.id = request.id.wrapping_add(1);
    let _ = app.update(Message::FullMindmapVerificationLoaded {
        request,
        result: Ok((
            candidates[0].clone(),
            tree::ExpandedFolderSnapshot {
                folders: Vec::new(),
                files: Vec::new(),
                recursive_supported_file_count: tree::RecursiveFileCount::Exact(1),
                truncated: false,
            },
        )),
    });
    assert_eq!(app.full_mindmap_progress.unwrap().checked, 0);
    assert!(app
        .full_mindmap_graph()
        .unwrap()
        .node(&WorkspaceNodeId::Folder(candidates[0].clone()))
        .is_none());
}

fn complete_full_mindmap_workspace_load(app: &mut App) -> PendingFullMindmapWorkspaceLoad {
    let request = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_workspace_load.clone())
        .expect("expected a pending Full Mindmap workspace load");
    let snapshot = tree::build_workspace(&request.path, app.show_hidden).unwrap();
    let _ = app.update(Message::FullMindmapWorkspaceLoaded {
        request: request.clone(),
        result: Ok((request.path.clone(), snapshot)),
    });
    complete_full_mindmap_folder_loads(app);
    request
}

fn complete_full_mindmap_folder_loads(app: &mut App) {
    loop {
        let requests = app
            .full_mindmap
            .as_ref()
            .map(|full| {
                full.pending_folder_loads
                    .values()
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if requests.is_empty() {
            break;
        }
        for request in requests {
            let snapshot =
                tree::load_expanded_folder(&request.folder, request.show_hidden).unwrap();
            let _ = app.update(Message::FullMindmapFolderLoaded {
                request: request.clone(),
                result: Ok((request.folder.clone(), snapshot)),
            });
        }
    }
}

fn settle_full_mindmap_preview(app: &mut App) -> PendingFullMindmapPreview {
    let request = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_preview_settle.clone())
        .expect("expected a pending Full Mindmap preview settle");
    let _ = app.update(Message::FullMindmapPreviewSettle { request });
    app.full_mindmap
        .as_ref()
        .and_then(|full| full.pending_preview.clone())
        .expect("settled preview should own a read request")
}

fn accept_full_mindmap_preview(
    app: &mut App,
    request: &PendingFullMindmapPreview,
    path: PathBuf,
    source: String,
) {
    let parsed = parse_full_mindmap_preview_blocking(path.clone(), source.clone());
    let _ = app.update(Message::FullMindmapPreviewLoaded {
        request: request.clone(),
        result: Ok((path.clone(), source)),
    });
    let _ = app.update(Message::FullMindmapPreviewParsed {
        request: request.clone(),
        result: Ok(parsed),
    });
}

#[test]
fn full_mindmap_without_workspace_adopts_home_in_background() {
    let mut app = App::default();
    let _ = app.enter_full_mindmap();

    let full = app.full_mindmap.as_ref().unwrap();
    assert_eq!(
        full.pending_workspace_load.as_ref().map(|load| &load.path),
        Picker::home().as_ref()
    );
    assert!(app.workspace.is_none(), "entry indexing must stay async");
    assert_eq!(app.view_mode, ViewMode::Rendered);
}

#[test]
fn full_mindmap_without_project_adopts_current_file_parent_and_preview() {
    let dir = full_mindmap_test_dir("current-parent");
    let file = dir.join("readme.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&file, "# Project\n").unwrap();

    let mut app = App::default();
    app.file = Some(file.clone());
    app.source = "# Project\n".into();
    let _ = app.enter_full_mindmap();

    assert_eq!(
        app.full_mindmap
            .as_ref()
            .and_then(|full| full.pending_workspace_load.as_ref())
            .map(|load| load.path.as_path()),
        Some(dir.as_path())
    );
    complete_full_mindmap_workspace_load(&mut app);
    let preview_request = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_preview.clone())
        .expect("current-file preview should own a parse request");
    let parsed = parse_full_mindmap_preview_blocking(file.clone(), app.source.clone());
    let _ = app.update(Message::FullMindmapPreviewParsed {
        request: preview_request,
        result: Ok(parsed),
    });

    assert_eq!(app.workspace.as_deref(), Some(dir.as_path()));
    assert!(!app.sidebar_open);
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().selected,
        Some(WorkspaceNodeId::File(file.clone()))
    );
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().focus_request,
        Some(WorkspaceNodeId::File(file.clone()))
    );
    assert!(matches!(
        app.full_mindmap.as_ref().unwrap().preview,
        FullMindmapPreview::Document { ref path, .. } if path == &file
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_standard_picker_file_waits_for_parent_index_before_opening() {
    let dir = full_mindmap_test_dir("picker-file-index");
    let file = dir.join("readme.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&file, "# Project\n").unwrap();

    let mut app = App::default();
    app.full_mindmap = Some(App::new_full_mindmap_state());

    let _ = app.update(Message::PickerOpenFile(file.clone()));
    let pending = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_workspace_load
        .as_ref()
        .unwrap();
    assert_eq!(pending.path, dir);
    assert_eq!(pending.open_after.as_ref(), Some(&file));
    assert!(app.full_mindmap.as_ref().unwrap().pending_open.is_none());

    complete_full_mindmap_workspace_load(&mut app);
    assert_eq!(app.workspace.as_deref(), Some(dir.as_path()));
    assert_eq!(
        app.full_mindmap
            .as_ref()
            .and_then(|full| full.pending_open.as_ref())
            .map(|pending| pending.path.as_path()),
        Some(file.as_path())
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_picker_file_checkpoints_active_preview_before_index() {
    let (mut app, old_file, new_file) =
        quick_slot_restore_test_app("picker-checkpoints-full-mindmap");
    let root = app.workspace.clone().expect("workspace root");
    let mut full = App::new_full_mindmap_state();
    full.selected = Some(WorkspaceNodeId::File(old_file.clone()));
    app.full_mindmap = Some(full);
    app.quick_slots.set(
        0,
        crate::quick_slots::QuickSlot {
            relative_path: "old.md".into(),
            context: crate::quick_slots::SlotContext {
                mode: crate::quick_slots::SlotMode::FullMindmap,
                preview_position: 0.75,
                ..Default::default()
            },
        },
    );
    app.quick_slots.active = Some(0);
    app.dirty = false;

    let _ = app.update(Message::PickerOpenFile(new_file.clone()));

    let checkpointed = app.quick_slots.occupied(0).expect("active slot");
    assert_eq!(checkpointed.relative_path, "old.md");
    assert_eq!(
        checkpointed.context.mode,
        crate::quick_slots::SlotMode::FullMindmap
    );
    assert_eq!(
        checkpointed.context.preview_position, 0.0,
        "a viewport-less preview checkpoints its normalized relative position"
    );
    assert_eq!(
        app.full_mindmap
            .as_ref()
            .and_then(|full| full.selected.clone()),
        Some(WorkspaceNodeId::File(old_file.clone()))
    );
    let pending = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_workspace_load.as_ref())
        .expect("picker should wait for parent indexing");
    assert_eq!(pending.path, root);
    assert_eq!(pending.open_after.as_ref(), Some(&new_file));

    let isolated = app.quick_slots_persistence_path.clone().unwrap();
    let _ = std::fs::remove_file(isolated);
    let _ = std::fs::remove_dir_all(app.workspace.take().unwrap());
}

#[test]
fn full_mindmap_workspace_navigation_checkpoints_active_preview_before_index() {
    let (mut app, old_file, _new_file) =
        quick_slot_restore_test_app("workspace-checkpoints-full-mindmap");
    let root = app.workspace.clone().expect("workspace root");
    let next_root = root.with_file_name(format!(
        "{}-next",
        root.file_name().expect("workspace name").to_string_lossy()
    ));
    std::fs::create_dir_all(&next_root).unwrap();
    std::fs::write(next_root.join("next.md"), "# Next\n").unwrap();

    let mut full = App::new_full_mindmap_state();
    full.selected = Some(WorkspaceNodeId::File(old_file.clone()));
    app.full_mindmap = Some(full);
    app.quick_slots.set(
        0,
        crate::quick_slots::QuickSlot {
            relative_path: "old.md".into(),
            context: crate::quick_slots::SlotContext {
                mode: crate::quick_slots::SlotMode::FullMindmap,
                preview_position: 0.5,
                ..Default::default()
            },
        },
    );
    app.quick_slots.active = Some(0);
    app.dirty = false;

    let _ = app.update(Message::OpenWorkspace(next_root.clone()));

    let checkpointed = app.quick_slots.occupied(0).expect("active slot");
    assert_eq!(checkpointed.relative_path, "old.md");
    assert_eq!(
        checkpointed.context.mode,
        crate::quick_slots::SlotMode::FullMindmap
    );
    assert_eq!(checkpointed.context.preview_position, 0.0);
    assert_eq!(
        app.full_mindmap
            .as_ref()
            .and_then(|full| full.selected.clone()),
        Some(WorkspaceNodeId::File(old_file))
    );
    assert_eq!(
        app.full_mindmap
            .as_ref()
            .and_then(|full| full.pending_workspace_load.as_ref())
            .map(|pending| pending.path.as_path()),
        Some(next_root.as_path())
    );

    let isolated = app.quick_slots_persistence_path.clone().unwrap();
    let _ = std::fs::remove_file(isolated);
    let _ = std::fs::remove_dir_all(app.workspace.take().unwrap());
    let _ = std::fs::remove_dir_all(next_root);
}

#[test]
fn full_mindmap_toggle_folder_checkpoints_and_clears_active_slot() {
    let (mut app, old_file, _new_file) = quick_slot_restore_test_app("toggle-folder-checkpoint");
    let root = app.workspace.clone().expect("workspace root");
    let folder = root.join("notes");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("note.md"), "# Note\n").unwrap();
    let mut full = full_workspace_state(&root);
    full.selected = Some(WorkspaceNodeId::File(old_file));
    app.full_mindmap = Some(full);
    app.quick_slots.set(
        0,
        crate::quick_slots::QuickSlot {
            relative_path: "old.md".into(),
            context: crate::quick_slots::SlotContext {
                mode: crate::quick_slots::SlotMode::FullMindmap,
                preview_position: 0.75,
                ..Default::default()
            },
        },
    );
    app.quick_slots.active = Some(0);
    app.dirty = false;

    let _ = app.update(Message::FullMindmapToggleNode(WorkspaceNodeId::Folder(
        folder,
    )));

    let checkpointed = app.quick_slots.occupied(0).expect("slot remains stored");
    assert_eq!(
        checkpointed.context.mode,
        crate::quick_slots::SlotMode::FullMindmap
    );
    assert_eq!(checkpointed.context.preview_position, 0.0);
    assert_eq!(app.quick_slots.active, None);
    let isolated = app.quick_slots_persistence_path.clone().unwrap();
    let stored = crate::prefs::load_from(&isolated);
    let persisted = stored.quick_slots.bank(&root);
    assert_eq!(persisted.active, None);
    assert_eq!(persisted.occupied(0), Some(checkpointed));

    let _ = std::fs::remove_file(isolated);
    let _ = std::fs::remove_dir_all(app.workspace.take().unwrap());
}

#[test]
fn full_mindmap_deselect_checkpoints_and_clears_active_slot() {
    let (mut app, old_file, _new_file) = quick_slot_restore_test_app("deselect-checkpoint");
    let root = app.workspace.clone().expect("workspace root");
    let mut full = App::new_full_mindmap_state();
    full.selected = Some(WorkspaceNodeId::File(old_file));
    app.full_mindmap = Some(full);
    app.quick_slots.set(
        0,
        crate::quick_slots::QuickSlot {
            relative_path: "old.md".into(),
            context: crate::quick_slots::SlotContext {
                mode: crate::quick_slots::SlotMode::FullMindmap,
                preview_position: 0.5,
                ..Default::default()
            },
        },
    );
    app.quick_slots.active = Some(0);
    app.dirty = false;

    let _ = app.update(Message::FullMindmapDeselect);

    let checkpointed = app.quick_slots.occupied(0).expect("slot remains stored");
    assert_eq!(
        checkpointed.context.mode,
        crate::quick_slots::SlotMode::FullMindmap
    );
    assert_eq!(checkpointed.context.preview_position, 0.0);
    assert_eq!(app.quick_slots.active, None);
    assert_eq!(app.full_mindmap.as_ref().unwrap().selected, None);
    let isolated = app.quick_slots_persistence_path.clone().unwrap();
    let stored = crate::prefs::load_from(&isolated);
    assert_eq!(stored.quick_slots.bank(&root).active, None);

    let _ = std::fs::remove_file(isolated);
    let _ = std::fs::remove_dir_all(app.workspace.take().unwrap());
}

#[test]
fn full_mindmap_enter_makes_selected_folder_the_new_root() {
    let dir = full_mindmap_test_dir("keyboard-root");
    let parent = dir.join("parent");
    let project = parent.join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("readme.md"), "# Project\n").unwrap();

    let mut app = App::default();
    app.set_workspace(parent.clone(), false);
    let mut full = full_workspace_state(&parent);
    full.selected = Some(WorkspaceNodeId::Folder(project.clone()));
    app.full_mindmap = Some(full);

    let _ = app.update(Message::FullMindmapActivate);
    assert_eq!(
        app.full_mindmap
            .as_ref()
            .and_then(|full| full.pending_workspace_load.as_ref())
            .map(|load| load.path.as_path()),
        Some(project.as_path())
    );
    complete_full_mindmap_workspace_load(&mut app);
    assert_eq!(app.workspace.as_deref(), Some(project.as_path()));
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().selected,
        Some(WorkspaceNodeId::Root(project.clone()))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_space_toggles_workspace_folder_and_keeps_selection() {
    let dir = full_mindmap_test_dir("keyboard-workspace");
    let folder = dir.join("notes");
    let file = folder.join("guide.md");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(&file, "# Guide\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.full_mindmap = Some(full_workspace_state(&dir));
    app.full_mindmap.as_mut().unwrap().selected = Some(WorkspaceNodeId::Folder(folder.clone()));

    assert!(matches!(
        full_mindmap_space_message(),
        Message::FullMindmapToggleSelected
    ));
    let _ = app.update(full_mindmap_space_message());

    let full = app.full_mindmap.as_ref().unwrap();
    assert!(full.expanded.contains(&folder));
    assert_eq!(full.selected, Some(WorkspaceNodeId::Folder(folder.clone())));
    assert!(app
        .full_mindmap_graph()
        .unwrap()
        .node(&WorkspaceNodeId::File(file.clone()))
        .is_some());

    let _ = app.update(full_mindmap_space_message());
    let full = app.full_mindmap.as_ref().unwrap();
    assert!(!full.expanded.contains(&folder));
    assert_eq!(full.selected, Some(WorkspaceNodeId::Folder(folder.clone())));
    assert!(app
        .full_mindmap_graph()
        .unwrap()
        .node(&WorkspaceNodeId::File(file))
        .is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_expands_interrupted_shell_without_rerooting() {
    let dir = full_mindmap_test_dir("lazy-interrupted-shell");
    let documents = dir.join("Documents");
    let notes = documents.join("Notes");
    let guide = notes.join("guide.md");
    let overview = documents.join("overview.md");
    std::fs::create_dir_all(&notes).unwrap();
    std::fs::write(&guide, "# Guide\n").unwrap();
    std::fs::write(&overview, "# Overview\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    let tree = app.workspace_tree.as_mut().unwrap();
    let documents_node = tree
        .children
        .iter_mut()
        .find(|node| node.path == documents)
        .unwrap();
    documents_node.children.clear();
    documents_node.recursive_supported_file_count = Some(tree::RecursiveFileCount::LowerBound(0));
    tree.recursive_supported_file_count = Some(tree::RecursiveFileCount::LowerBound(0));
    app.workspace_truncated = true;
    app.full_mindmap = Some(App::new_full_mindmap_state());
    {
        let full = app.full_mindmap.as_mut().unwrap();
        full.expanded.insert(dir.clone());
        full.selected = Some(WorkspaceNodeId::Folder(documents.clone()));
    }

    let _ = app.update(Message::FullMindmapToggleSelected);
    assert_eq!(app.workspace.as_deref(), Some(dir.as_path()));
    assert!(app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_folder_loads
        .contains_key(&documents));
    assert!(app
        .full_mindmap_graph()
        .unwrap()
        .node(&WorkspaceNodeId::Status(
            documents.clone(),
            workspace_mindmap::WorkspaceStatus::LoadingFiles,
        ))
        .is_some());

    complete_full_mindmap_folder_loads(&mut app);
    let graph = app.full_mindmap_graph().unwrap();
    assert!(graph
        .node(&WorkspaceNodeId::Folder(notes.clone()))
        .is_some());
    assert!(graph
        .node(&WorkspaceNodeId::File(overview.clone()))
        .is_some());
    assert_eq!(app.workspace.as_deref(), Some(dir.as_path()));

    let _ = app.update(Message::FullMindmapNavigate(MindmapDir::Right));
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().selected,
        Some(WorkspaceNodeId::Folder(notes))
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_lazy_exact_empty_shell_is_pruned_and_selection_normalized() {
    let dir = full_mindmap_test_dir("lazy-empty-shell");
    let empty = dir.join("empty");
    std::fs::create_dir_all(&empty).unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    let tree = app.workspace_tree.as_mut().unwrap();
    tree.children.push(Node {
        path: empty.clone(),
        name: "empty".into(),
        is_dir: true,
        children: Vec::new(),
        recursive_supported_file_count: Some(tree::RecursiveFileCount::LowerBound(0)),
    });
    tree.recursive_supported_file_count = Some(tree::RecursiveFileCount::LowerBound(0));
    app.workspace_truncated = true;
    app.full_mindmap = Some(App::new_full_mindmap_state());
    {
        let full = app.full_mindmap.as_mut().unwrap();
        full.expanded.insert(dir.clone());
        full.selected = Some(WorkspaceNodeId::Folder(empty.clone()));
    }

    let _ = app.update(Message::FullMindmapToggleSelected);
    complete_full_mindmap_folder_loads(&mut app);

    assert!(app
        .full_mindmap_graph()
        .unwrap()
        .node(&WorkspaceNodeId::Folder(empty))
        .is_none());
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().selected,
        Some(WorkspaceNodeId::Root(dir.clone()))
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_lazy_exact_empty_nested_shell_selects_nearest_ancestor() {
    let dir = full_mindmap_test_dir("lazy-empty-nested-shell");
    let documents = dir.join("Documents");
    let shell = documents.join("Archive");
    std::fs::create_dir_all(&shell).unwrap();
    std::fs::write(documents.join("guide.md"), "# Guide\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    let tree = app.workspace_tree.as_mut().unwrap();
    let documents_node = tree
        .children
        .iter_mut()
        .find(|node| node.path == documents)
        .unwrap();
    documents_node.children.push(Node {
        path: shell.clone(),
        name: "Archive".into(),
        is_dir: true,
        children: Vec::new(),
        recursive_supported_file_count: Some(tree::RecursiveFileCount::LowerBound(0)),
    });
    documents_node.recursive_supported_file_count = Some(tree::RecursiveFileCount::LowerBound(0));
    tree.recursive_supported_file_count = Some(tree::RecursiveFileCount::LowerBound(0));
    app.workspace_truncated = true;
    app.full_mindmap = Some(App::new_full_mindmap_state());
    {
        let full = app.full_mindmap.as_mut().unwrap();
        full.expanded.insert(dir.clone());
        full.expanded.insert(documents.clone());
        full.selected = Some(WorkspaceNodeId::Folder(shell.clone()));
    }

    let _ = app.update(Message::FullMindmapToggleSelected);
    complete_full_mindmap_folder_loads(&mut app);

    let graph = app.full_mindmap_graph().unwrap();
    assert!(graph.node(&WorkspaceNodeId::Folder(shell)).is_none());
    assert!(graph
        .node(&WorkspaceNodeId::Folder(documents.clone()))
        .is_some());
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().selected,
        Some(WorkspaceNodeId::Folder(documents))
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_collapse_evicts_branch_and_rejects_pre_reexpand_result() {
    let dir = full_mindmap_test_dir("lazy-collapse-stale");
    let folder = dir.join("notes");
    let file = folder.join("a.md");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(&file, "# A\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    let _ = app.enter_full_mindmap();
    complete_full_mindmap_folder_loads(&mut app);
    let folder_id = WorkspaceNodeId::Folder(folder.clone());

    let _ = app.update(Message::FullMindmapToggleNode(folder_id.clone()));
    let first = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_folder_loads
        .get(&folder)
        .cloned()
        .unwrap();
    assert!(app
        .full_mindmap_graph()
        .unwrap()
        .node(&WorkspaceNodeId::Status(
            folder.clone(),
            workspace_mindmap::WorkspaceStatus::LoadingFiles,
        ))
        .is_some());

    let _ = app.update(Message::FullMindmapToggleNode(folder_id.clone()));
    let full = app.full_mindmap.as_ref().unwrap();
    assert!(!full.pending_folder_loads.contains_key(&folder));
    assert!(!full.materialized_folders.contains_key(&folder));

    let _ = app.update(Message::FullMindmapToggleNode(folder_id));
    let second = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_folder_loads
        .get(&folder)
        .cloned()
        .unwrap();
    assert_ne!(first, second);
    let snapshot = tree::load_expanded_folder(&folder, false).unwrap();
    let _ = app.update(Message::FullMindmapFolderLoaded {
        request: first,
        result: Ok((folder.clone(), snapshot.clone())),
    });
    assert_eq!(
        app.full_mindmap
            .as_ref()
            .unwrap()
            .pending_folder_loads
            .get(&folder),
        Some(&second)
    );
    assert!(!app
        .full_mindmap
        .as_ref()
        .unwrap()
        .materialized_folders
        .contains_key(&folder));

    let _ = app.update(Message::FullMindmapFolderLoaded {
        request: second,
        result: Ok((folder, snapshot)),
    });
    assert!(app
        .full_mindmap_graph()
        .unwrap()
        .node(&WorkspaceNodeId::File(file))
        .is_some());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_folder_results_are_stale_across_filter_root_and_reentry() {
    let dir = full_mindmap_test_dir("lazy-generation-stale");
    let other = full_mindmap_test_dir("lazy-generation-other");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(dir.join("a.md"), "# A\n").unwrap();
    let snapshot = tree::load_expanded_folder(&dir, false).unwrap();

    let pending_root = |app: &App| {
        app.full_mindmap
            .as_ref()
            .unwrap()
            .pending_folder_loads
            .get(&dir)
            .cloned()
            .unwrap()
    };

    let mut hidden_app = App::default();
    hidden_app.set_workspace(dir.clone(), false);
    let _ = hidden_app.enter_full_mindmap();
    let hidden_stale = pending_root(&hidden_app);
    let _ = hidden_app.update(Message::ToggleHidden);
    let _ = hidden_app.update(Message::FullMindmapFolderLoaded {
        request: hidden_stale,
        result: Ok((dir.clone(), snapshot.clone())),
    });
    assert!(!hidden_app
        .full_mindmap
        .as_ref()
        .unwrap()
        .materialized_folders
        .contains_key(&dir));

    let mut root_app = App::default();
    root_app.set_workspace(dir.clone(), false);
    let _ = root_app.enter_full_mindmap();
    let root_stale = pending_root(&root_app);
    let _ = root_app.update(Message::FullMindmapSetRoot(other.clone()));
    let workspace_request = root_app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_workspace_load
        .clone();
    let _ = root_app.update(Message::FullMindmapFolderLoaded {
        request: root_stale,
        result: Ok((dir.clone(), snapshot.clone())),
    });
    assert_eq!(
        root_app
            .full_mindmap
            .as_ref()
            .unwrap()
            .pending_workspace_load,
        workspace_request
    );
    assert!(!root_app
        .full_mindmap
        .as_ref()
        .unwrap()
        .materialized_folders
        .contains_key(&dir));

    let mut reentry_app = App::default();
    reentry_app.set_workspace(dir.clone(), false);
    let _ = reentry_app.enter_full_mindmap();
    let reentry_stale = pending_root(&reentry_app);
    let _ = reentry_app.update(Message::ExitFullMindmap);
    let _ = reentry_app.enter_full_mindmap();
    let replacement = pending_root(&reentry_app);
    assert_ne!(reentry_stale, replacement);
    let _ = reentry_app.update(Message::FullMindmapFolderLoaded {
        request: reentry_stale,
        result: Ok((dir.clone(), snapshot)),
    });
    assert_eq!(
        reentry_app
            .full_mindmap
            .as_ref()
            .unwrap()
            .pending_folder_loads
            .get(&dir),
        Some(&replacement)
    );

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&other);
}

#[test]
fn full_mindmap_right_expands_and_selects_first_child_in_one_step() {
    let dir = full_mindmap_test_dir("keyboard-right");
    let folder = dir.join("notes");
    let first = folder.join("a.md");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(&first, "# A\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.full_mindmap = Some(full_workspace_state(&dir));
    app.full_mindmap.as_mut().unwrap().selected = Some(WorkspaceNodeId::Folder(folder.clone()));

    let _ = app.update(Message::FullMindmapNavigate(MindmapDir::Right));

    let full = app.full_mindmap.as_ref().unwrap();
    assert!(full.expanded.contains(&folder));
    assert_eq!(full.selected, Some(WorkspaceNodeId::File(first)));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_left_at_workspace_root_moves_to_parent_workspace() {
    let dir = full_mindmap_test_dir("workspace-parent");
    let parent = dir.join("parent");
    let project = parent.join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("readme.md"), "# Project\n").unwrap();

    let mut app = App::default();
    app.set_workspace(project.clone(), false);
    let mut full = full_workspace_state(&project);
    full.pending_open = Some(PendingFullMindmapOpen {
        id: 1,
        path: project.join("readme.md"),
    });
    full.pending_preview = Some(PendingFullMindmapPreview {
        id: 2,
        path: project.join("readme.md"),
    });
    full.preview = FullMindmapPreview::Loading(project.join("readme.md"));
    app.full_mindmap = Some(full);

    let _ = app.update(Message::FullMindmapNavigate(MindmapDir::Left));
    assert!(app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_workspace_load
        .is_some());
    complete_full_mindmap_workspace_load(&mut app);

    assert_eq!(app.workspace.as_deref(), Some(parent.as_path()));
    let full = app.full_mindmap.as_ref().unwrap();
    assert_eq!(full.selected, Some(WorkspaceNodeId::Root(parent.clone())));
    assert_eq!(full.expanded, HashSet::from([parent.clone()]));
    assert!(app
        .full_mindmap_graph()
        .unwrap()
        .node(&WorkspaceNodeId::Folder(project))
        .is_some());
    assert!(full.pending_open.is_none());
    assert!(full.pending_preview.is_none());
    assert!(matches!(full.preview, FullMindmapPreview::None));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_root_parent_navigation_preserves_dirty_document() {
    let dir = full_mindmap_test_dir("workspace-parent-dirty");
    let parent = dir.join("parent");
    let project = parent.join("project");
    let current = project.join("current.md");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(&current, "# Current\n").unwrap();

    let mut app = App::default();
    app.set_workspace(project.clone(), false);
    app.file = Some(current.clone());
    app.source = "# Draft\n".into();
    app.saved_source = "# Saved\n".into();
    app.editor = Some(iced::widget::text_editor::Content::with_text("# Draft\n"));
    app.dirty = true;
    app.full_mindmap = Some(full_workspace_state(&project));

    let _ = app.update(Message::FullMindmapNavigate(MindmapDir::Left));
    complete_full_mindmap_workspace_load(&mut app);

    assert_eq!(app.workspace.as_deref(), Some(parent.as_path()));
    assert_eq!(app.file.as_deref(), Some(current.as_path()));
    assert_eq!(app.source, "# Draft\n");
    assert_eq!(app.saved_source, "# Saved\n");
    assert_eq!(
        app.editor.as_ref().map(|editor| editor.text()),
        Some("# Draft\n".into())
    );
    assert!(app.dirty);
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().selected,
        Some(WorkspaceNodeId::Root(parent))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_root_parent_navigation_ignores_late_file_open() {
    let dir = full_mindmap_test_dir("workspace-parent-late-open");
    let parent = dir.join("parent");
    let project = parent.join("project");
    let next = project.join("next.md");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(&next, "# Next\n").unwrap();

    let mut app = App::default();
    app.set_workspace(project.clone(), false);
    app.full_mindmap = Some(full_workspace_state(&project));
    let _ = app.begin_full_mindmap_open(next.clone());
    let request = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_open
        .clone()
        .unwrap();

    let _ = app.update(Message::FullMindmapNavigate(MindmapDir::Left));
    let _ = app.update(Message::FullMindmapFileLoaded {
        request,
        result: Ok((next, "# Stale\n".into())),
    });
    complete_full_mindmap_workspace_load(&mut app);

    assert_eq!(app.workspace.as_deref(), Some(parent.as_path()));
    assert!(app.full_mindmap.is_some());
    assert_eq!(app.file, None);
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().selected,
        Some(WorkspaceNodeId::Root(parent))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_file_enter_opens_document_mindmap_and_focuses_first_child() {
    let dir = full_mindmap_test_dir("file-enter-document-mindmap");
    let file = dir.join("guide.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&file, "# Guide\n## Details\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    let mut full = full_workspace_state(&dir);
    full.selected = Some(WorkspaceNodeId::File(file.clone()));
    app.full_mindmap = Some(full);
    assert_eq!(app.view_mode, ViewMode::Rendered);

    let _ = app.update(Message::FullMindmapActivate);
    let request = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_open.clone())
        .expect("Enter should own a background file request");
    let _ = app.update(Message::FullMindmapFileLoaded {
        request,
        result: Ok((file.clone(), "# Guide\n## Details\n".into())),
    });

    assert!(app.full_mindmap.is_none());
    assert_eq!(app.view_mode, ViewMode::Mindmap);
    assert_eq!(app.file.as_deref(), Some(file.as_path()));
    let first_heading = app
        .ast
        .iter()
        .find_map(|(id, block)| matches!(block, Block::Heading { .. }).then_some(*id));
    assert_eq!(app.mindmap_selected, first_heading);
    assert!(app.mindmap_panel_open);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_file_enter_from_clean_zen_ends_in_document_mindmap() {
    let dir = full_mindmap_test_dir("file-enter-clean-zen");
    let old_file = dir.join("old.md");
    let file = dir.join("guide.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&old_file, "# Old\n").unwrap();
    std::fs::write(&file, "# Guide\n## Details\n").unwrap();

    let mut app = App::default();
    app.file = Some(old_file);
    app.source = "# Old\n".into();
    app.saved_source = app.source.clone();
    app.sidebar_open = true;
    app.show_footer = false;
    app.search_open = true;
    let _ = app.enter_zen_edit_mode();
    assert_eq!(app.view_mode, ViewMode::Raw);
    assert!(app.editor.is_some());
    app.set_workspace(dir.clone(), false);

    let mut full = full_workspace_state(&dir);
    full.selected = Some(WorkspaceNodeId::File(file.clone()));
    app.full_mindmap = Some(full);
    app.dirty = false;

    let _ = app.update(Message::FullMindmapActivate);
    let request = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_open.clone())
        .expect("Enter should own a background file request");
    let _ = app.update(Message::FullMindmapFileLoaded {
        request,
        result: Ok((file.clone(), "# Guide\n## Details\n".into())),
    });

    assert!(app.full_mindmap.is_none());
    assert_eq!(app.view_mode, ViewMode::Mindmap);
    assert!(app.editor.is_none());
    assert!(app.zen_restore.is_none());
    assert_eq!(app.file.as_deref(), Some(file.as_path()));
    let first_heading = app
        .ast
        .iter()
        .find_map(|(id, block)| matches!(block, Block::Heading { .. }).then_some(*id));
    assert_eq!(app.mindmap_selected, first_heading);
    assert!(app.sidebar_open);
    assert!(!app.show_footer);
    assert!(app.search_open);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_dirty_file_enter_still_blocks_open() {
    let dir = full_mindmap_test_dir("file-enter-dirty");
    let file = dir.join("guide.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&file, "# Guide\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    let mut full = full_workspace_state(&dir);
    full.selected = Some(WorkspaceNodeId::File(file));
    app.full_mindmap = Some(full);
    app.source = "unsaved current".into();
    app.saved_source = "saved current".into();
    app.dirty = true;

    let _ = app.update(Message::FullMindmapActivate);
    assert!(app.full_mindmap.as_ref().unwrap().pending_open.is_none());
    assert!(app.full_mindmap.is_some());
    assert!(app
        .toast
        .as_ref()
        .is_some_and(|toast| toast.text.contains("unsaved edits")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn document_mindmap_root_left_returns_and_focuses_current_file() {
    let dir = full_mindmap_test_dir("document-root-left");
    let file = dir.join("guide.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&file, "# Guide\n## Details\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.file = Some(file.clone());
    app.source = "# Guide\n## Details\n".into();
    app.saved_source = app.source.clone();
    app.view_mode = ViewMode::Mindmap;
    app.load_ast_from_source();
    app.mindmap_focus_first_child();
    let top_heading = app
        .mindmap_selected
        .expect("first heading should be selected");

    let _ = app.update(Message::MindmapNavigate(MindmapDir::Left));
    assert!(app.full_mindmap.is_some());
    assert_eq!(app.view_mode, ViewMode::Mindmap);
    assert_eq!(app.workspace.as_deref(), Some(dir.as_path()));
    // The bridge records the file as the viewport target immediately;
    // selection remains on the visible root until its branch listing is
    // accepted, so the first rendered graph cannot consume root focus.
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().focus_request,
        Some(WorkspaceNodeId::File(file.clone()))
    );
    // Document and Full Mindmap selections are intentionally distinct; the
    // underlying document heading remains selected while the workspace
    // bridge takes ownership of the visible navigator.
    assert_eq!(app.mindmap_selected, Some(top_heading));

    complete_full_mindmap_folder_loads(&mut app);
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().selected,
        Some(WorkspaceNodeId::File(file.clone()))
    );
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().focus_request,
        Some(WorkspaceNodeId::File(file.clone()))
    );
    assert!(app
        .full_mindmap_graph()
        .unwrap()
        .node(&WorkspaceNodeId::File(file))
        .is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn document_mindmap_root_left_checkpoints_active_slot_before_full_mindmap() {
    let (mut app, old_file, _new_file) =
        quick_slot_restore_test_app("document-root-left-checkpoint");
    app.file = Some(old_file.clone());
    app.source = "# Old\n".into();
    app.saved_source = app.source.clone();
    app.view_mode = ViewMode::Mindmap;
    app.load_ast_from_source();
    app.mindmap_selected = Some(crate::ast::BlockId(31));
    app.mindmap_panel_open = true;
    app.quick_slots.set(
        0,
        crate::quick_slots::QuickSlot {
            relative_path: "old.md".into(),
            context: crate::quick_slots::SlotContext::default(),
        },
    );
    app.quick_slots.active = Some(0);
    app.dirty = false;

    let _ = app.update(Message::MindmapNavigate(MindmapDir::Left));

    let checkpointed = app.quick_slots.occupied(0).expect("active slot");
    assert_eq!(
        checkpointed.context.mode,
        crate::quick_slots::SlotMode::DocumentMindmap
    );
    assert_eq!(checkpointed.context.mindmap_selection, Some(31));
    assert!(checkpointed.context.mindmap_panel_open);
    assert!(app.full_mindmap.is_some());

    let isolated = app.quick_slots_persistence_path.clone().unwrap();
    let _ = std::fs::remove_file(isolated);
    let _ = std::fs::remove_dir_all(app.workspace.take().unwrap());
}

#[test]
fn document_mindmap_root_left_deselect_cancels_deferred_file_selection() {
    let dir = full_mindmap_test_dir("document-root-left-deselect");
    let file = dir.join("guide.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&file, "# Guide\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.file = Some(file.clone());
    app.source = "# Guide\n".into();
    app.saved_source = app.source.clone();
    app.view_mode = ViewMode::Mindmap;
    app.load_ast_from_source();
    app.mindmap_focus_first_child();

    let _ = app.update(Message::MindmapNavigate(MindmapDir::Left));
    let full = app.full_mindmap.as_ref().unwrap();
    assert_eq!(full.deferred_file_selection, Some(file.clone()));
    assert!(full.pending_folder_loads.values().next().is_some());

    let _ = app.update(Message::FullMindmapDeselect);
    let full = app.full_mindmap.as_ref().unwrap();
    assert_eq!(full.selected, None);
    assert_eq!(full.focus_request, None);
    assert_eq!(full.deferred_file_selection, None);
    assert!(!full.panel_open);

    // A late accepted folder result must not resurrect the bridge's stale
    // file selection or schedule a read-only preview after cancellation.
    complete_full_mindmap_folder_loads(&mut app);
    let full = app.full_mindmap.as_ref().unwrap();
    assert_eq!(full.selected, None);
    assert_eq!(full.focus_request, None);
    assert_eq!(full.deferred_file_selection, None);
    assert!(!full.panel_open);
    assert!(matches!(full.preview, FullMindmapPreview::None));
    assert!(full.pending_preview.is_none());
    assert!(full.pending_preview_settle.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn document_mindmap_root_left_toggle_cancels_deferred_file_selection() {
    let dir = full_mindmap_test_dir("document-root-left-toggle");
    let file = dir.join("guide.md");
    let other = dir.join("other");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(&file, "# Guide\n").unwrap();
    std::fs::write(other.join("note.md"), "# Other\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.file = Some(file.clone());
    app.source = "# Guide\n".into();
    app.saved_source = app.source.clone();
    app.view_mode = ViewMode::Mindmap;
    app.load_ast_from_source();
    app.mindmap_focus_first_child();

    let _ = app.update(Message::MindmapNavigate(MindmapDir::Left));
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().deferred_file_selection,
        Some(file)
    );

    let other_id = WorkspaceNodeId::Folder(other.clone());
    let _ = app.update(Message::FullMindmapToggleNode(other_id.clone()));
    let full = app.full_mindmap.as_ref().unwrap();
    assert_eq!(full.deferred_file_selection, None);
    assert_eq!(full.selected, Some(other_id.clone()));
    assert_eq!(full.focus_request, Some(other_id.clone()));

    complete_full_mindmap_folder_loads(&mut app);
    let full = app.full_mindmap.as_ref().unwrap();
    assert_eq!(full.selected, Some(other_id.clone()));
    assert_eq!(full.focus_request, Some(other_id));
    assert!(!full.panel_open);
    assert!(full.pending_preview.is_none());
    assert!(full.pending_preview_settle.is_none());
    assert!(matches!(full.preview, FullMindmapPreview::None));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_toggle_cancels_non_preserve_workspace_loads() {
    for select_root in [false, true] {
        let label = if select_root {
            "toggle-cancels-root-load"
        } else {
            "toggle-cancels-parent-load"
        };
        let dir = full_mindmap_test_dir(label);
        let old_root = dir.join("old");
        let next_root = dir.join("next");
        std::fs::create_dir_all(&old_root).unwrap();
        std::fs::create_dir_all(&next_root).unwrap();
        std::fs::write(old_root.join("old.md"), "# Old\n").unwrap();
        std::fs::write(next_root.join("next.md"), "# Next\n").unwrap();

        let mut app = App::default();
        app.set_workspace(old_root.clone(), false);
        app.full_mindmap = Some(full_workspace_state(&old_root));
        let old_id = WorkspaceNodeId::Root(old_root.clone());
        app.full_mindmap.as_mut().unwrap().focus_request = Some(old_id.clone());

        let _ = app.begin_full_mindmap_workspace_load(
            next_root.clone(),
            select_root,
            None,
            false,
            false,
            false,
        );
        let request = app
            .full_mindmap
            .as_ref()
            .unwrap()
            .pending_workspace_load
            .clone()
            .unwrap();

        let _ = app.update(Message::FullMindmapToggleNode(old_id.clone()));
        let full = app.full_mindmap.as_ref().unwrap();
        assert!(full.pending_workspace_load.is_none());
        assert_eq!(full.deferred_file_selection, None);
        assert_eq!(full.selected, Some(old_id.clone()));
        assert_eq!(full.focus_request, Some(old_id.clone()));

        let snapshot = tree::build_workspace(&next_root, false).unwrap();
        let _ = app.update(Message::FullMindmapWorkspaceLoaded {
            request,
            result: Ok((next_root, snapshot)),
        });
        let full = app.full_mindmap.as_ref().unwrap();
        assert_eq!(app.workspace.as_deref(), Some(old_root.as_path()));
        assert_eq!(full.selected, Some(old_id.clone()));
        assert_eq!(full.focus_request, Some(old_id));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn full_mindmap_deselect_cancels_non_preserve_workspace_loads() {
    for select_root in [false, true] {
        let label = if select_root {
            "deselect-cancels-root-load"
        } else {
            "deselect-cancels-parent-load"
        };
        let dir = full_mindmap_test_dir(label);
        let old_root = dir.join("old");
        let next_root = dir.join("next");
        std::fs::create_dir_all(&old_root).unwrap();
        std::fs::create_dir_all(&next_root).unwrap();
        std::fs::write(old_root.join("old.md"), "# Old\n").unwrap();
        std::fs::write(next_root.join("next.md"), "# Next\n").unwrap();

        let mut app = App::default();
        app.set_workspace(old_root.clone(), false);
        app.full_mindmap = Some(full_workspace_state(&old_root));
        let _ = app.begin_full_mindmap_workspace_load(
            next_root.clone(),
            select_root,
            None,
            false,
            false,
            false,
        );
        let request = app
            .full_mindmap
            .as_ref()
            .unwrap()
            .pending_workspace_load
            .clone()
            .unwrap();

        let _ = app.update(Message::FullMindmapDeselect);
        let full = app.full_mindmap.as_ref().unwrap();
        assert!(full.pending_workspace_load.is_none());
        assert_eq!(full.selected, None);
        assert_eq!(full.focus_request, None);
        assert!(!full.panel_open);

        let snapshot = tree::build_workspace(&next_root, false).unwrap();
        let _ = app.update(Message::FullMindmapWorkspaceLoaded {
            request,
            result: Ok((next_root, snapshot)),
        });
        let full = app.full_mindmap.as_ref().unwrap();
        assert_eq!(app.workspace.as_deref(), Some(old_root.as_path()));
        assert_eq!(full.selected, None);
        assert_eq!(full.focus_request, None);
        assert!(!full.panel_open);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn document_mindmap_root_left_adopts_current_file_parent_outside_workspace() {
    let dir = full_mindmap_test_dir("document-root-left-adopt");
    let old_root = dir.join("old-root");
    let current_root = dir.join("current-root");
    let file = current_root.join("guide.md");
    std::fs::create_dir_all(&old_root).unwrap();
    std::fs::create_dir_all(&current_root).unwrap();
    std::fs::write(&file, "# Guide\n").unwrap();

    let mut app = App::default();
    app.set_workspace(old_root.clone(), false);
    app.file = Some(file.clone());
    app.source = "# Guide\n".into();
    app.saved_source = app.source.clone();
    app.view_mode = ViewMode::Mindmap;
    app.load_ast_from_source();
    app.mindmap_focus_first_child();

    let _ = app.update(Message::MindmapNavigate(MindmapDir::Left));
    let request = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_workspace_load.as_ref())
        .expect("outside-workspace return should adopt the current parent");
    assert_eq!(request.path, current_root);

    complete_full_mindmap_workspace_load(&mut app);
    assert_eq!(app.workspace.as_deref(), Some(current_root.as_path()));
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().selected,
        Some(WorkspaceNodeId::File(file.clone()))
    );
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().focus_request,
        Some(WorkspaceNodeId::File(file.clone()))
    );
    assert!(app
        .full_mindmap_graph()
        .unwrap()
        .node(&WorkspaceNodeId::File(file))
        .is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn document_mindmap_root_left_adopts_parent_from_broad_unindexed_workspace() {
    let dir = full_mindmap_test_dir("document-root-left-broad-workspace");
    let broad_root = dir.join("broad-root");
    let file_parent = broad_root.join("deep/project/docs/plans");
    let file = file_parent.join("design.md");
    std::fs::create_dir_all(&file_parent).unwrap();
    std::fs::write(&file, "# Design\n").unwrap();

    let mut app = App::default();
    app.set_workspace(broad_root.clone(), false);
    // Model a bounded workspace such as `/`: the current file is
    // lexically beneath the root, but it is not in the accepted file
    // index and therefore cannot be guaranteed to materialize in the
    // rendered WorkspaceGraph.
    app.workspace_files.retain(|path| path != &file);
    app.file = Some(file.clone());
    app.source = "# Design\n".into();
    app.saved_source = app.source.clone();
    app.view_mode = ViewMode::Mindmap;
    app.load_ast_from_source();
    app.mindmap_focus_first_child();

    let _ = app.update(Message::MindmapNavigate(MindmapDir::Left));
    let request = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_workspace_load.as_ref())
        .expect("an unindexed file must adopt its directly renderable parent");
    assert_eq!(request.path, file_parent);
    assert_ne!(request.path, broad_root);

    complete_full_mindmap_workspace_load(&mut app);
    assert_eq!(app.workspace.as_deref(), file.parent());
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().selected,
        Some(WorkspaceNodeId::File(file.clone()))
    );
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().focus_request,
        Some(WorkspaceNodeId::File(file))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn document_mindmap_nested_left_remains_document_navigation() {
    let file = PathBuf::from("/nested-left.md");
    let mut app = App::default();
    app.file = Some(file);
    app.source = "# Top\n## Nested\n".into();
    app.saved_source = app.source.clone();
    app.view_mode = ViewMode::Mindmap;
    app.load_ast_from_source();
    let headings = app
        .ast
        .iter()
        .filter_map(|(id, block)| matches!(block, Block::Heading { .. }).then_some(*id))
        .collect::<Vec<_>>();
    assert_eq!(headings.len(), 2);
    app.mindmap_selected = Some(headings[1]);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let _ = app.update(Message::MindmapNavigate(MindmapDir::Left));
    });
    assert!(app.full_mindmap.is_none());
    assert_eq!(app.view_mode, ViewMode::Mindmap);
    assert_eq!(app.mindmap_selected, Some(headings[0]));
}

#[test]
fn document_root_left_has_consistent_no_selection_boundary() {
    let dir = full_mindmap_test_dir("document-root-left-no-selection");
    let file = dir.join("guide.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&file, "# Guide\n").unwrap();

    let mut app = App::default();
    app.file = Some(file);
    app.source = "# Guide\n".into();
    app.saved_source = app.source.clone();
    app.view_mode = ViewMode::Mindmap;
    app.load_ast_from_source();
    app.mindmap_selected = None;

    let _ = app.update(Message::MindmapNavigate(MindmapDir::Left));
    assert!(app.full_mindmap.is_some());
    assert_eq!(app.view_mode, ViewMode::Mindmap);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn stale_full_mindmap_file_and_workspace_loads_cannot_switch_modes() {
    let dir = full_mindmap_test_dir("document-bridge-stale");
    let old_root = dir.join("old-root");
    let current_root = dir.join("current-root");
    let next_root = dir.join("next-root");
    let first = old_root.join("first.md");
    let second = old_root.join("second.md");
    let current = current_root.join("current.md");
    std::fs::create_dir_all(&old_root).unwrap();
    std::fs::create_dir_all(&current_root).unwrap();
    std::fs::create_dir_all(&next_root).unwrap();
    std::fs::write(&first, "# First\n").unwrap();
    std::fs::write(&second, "# Second\n").unwrap();
    std::fs::write(&current, "# Current\n").unwrap();

    let mut app = App::default();
    app.set_workspace(old_root.clone(), false);
    app.full_mindmap = Some(full_workspace_state(&old_root));
    app.full_mindmap.as_mut().unwrap().selected = Some(WorkspaceNodeId::File(first.clone()));
    let _ = app.update(Message::FullMindmapActivate);
    let stale_file = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_open
        .clone()
        .unwrap();
    let _ = app.begin_full_mindmap_open(second.clone());
    let current_file_request = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_open
        .clone()
        .unwrap();
    let _ = app.update(Message::FullMindmapFileLoaded {
        request: stale_file,
        result: Ok((first, "# Stale\n".into())),
    });
    assert!(app.full_mindmap.is_some());
    assert_eq!(app.view_mode, ViewMode::Rendered);
    assert_eq!(app.file, None);
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().pending_open,
        Some(current_file_request.clone())
    );
    let _ = app.update(Message::FullMindmapFileLoaded {
        request: current_file_request,
        result: Ok((second.clone(), "# Second\n".into())),
    });
    assert!(app.full_mindmap.is_none());
    assert_eq!(app.view_mode, ViewMode::Mindmap);
    assert_eq!(app.file.as_deref(), Some(second.as_path()));

    // A separate stale workspace completion must not strand the bridge or
    // alter the document mode while a newer root request owns Full Mindmap.
    let mut bridge = App::default();
    bridge.set_workspace(old_root.clone(), false);
    bridge.file = Some(current.clone());
    bridge.source = "# Current\n".into();
    bridge.saved_source = bridge.source.clone();
    bridge.view_mode = ViewMode::Mindmap;
    bridge.load_ast_from_source();
    bridge.mindmap_focus_first_child();
    let _ = bridge.update(Message::MindmapNavigate(MindmapDir::Left));
    let stale_workspace = bridge
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_workspace_load.clone())
        .unwrap();
    let _ = bridge.update(Message::FullMindmapSetRoot(next_root.clone()));
    let replacement = bridge
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_workspace_load.clone())
        .unwrap();
    let stale_snapshot = tree::build_workspace(&stale_workspace.path, false).unwrap();
    let _ = bridge.update(Message::FullMindmapWorkspaceLoaded {
        request: stale_workspace,
        result: Ok((replacement.path.clone(), stale_snapshot)),
    });
    assert_eq!(
        bridge
            .full_mindmap
            .as_ref()
            .and_then(|full| full.pending_workspace_load.clone()),
        Some(replacement)
    );
    assert!(bridge.full_mindmap.is_some());
    assert_eq!(bridge.view_mode, ViewMode::Mindmap);
    assert_eq!(bridge.workspace.as_deref(), Some(old_root.as_path()));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_return_to_files_refreshes_stale_hidden_sidebar_off_thread() {
    let dir = full_mindmap_test_dir("return-hidden-refresh");
    let visible = dir.join("visible.md");
    let hidden = dir.join(".hidden.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&visible, "# Visible\n").unwrap();
    std::fs::write(&hidden, "# Hidden\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.full_mindmap = Some(full_workspace_state(&dir));
    let _ = app.update(Message::ToggleHidden);
    assert!(app.show_hidden);
    assert!(!app.workspace_files.contains(&hidden));
    assert!(!app.workspace_snapshot_show_hidden);

    let _ = app.update(Message::FullMindmapReturnToFiles);
    let request = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_workspace_load.clone())
        .expect("return should wait for a background sidebar refresh");
    assert!(request.return_to_files_after);
    assert!(app.full_mindmap.is_some());
    assert!(!app.workspace_files.contains(&hidden));

    complete_full_mindmap_workspace_load(&mut app);
    assert!(app.full_mindmap.is_none());
    assert!(app.sidebar_open);
    assert_eq!(app.sidebar_tab, SidebarTab::Files);
    assert!(app.workspace_files.contains(&visible));
    assert!(app.workspace_files.contains(&hidden));
    assert!(app.workspace_snapshot_show_hidden);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_return_to_files_checkpoints_active_preview_before_exit() {
    let (mut app, old_file, _new_file) = quick_slot_restore_test_app("return-files-checkpoint");
    let mut full = App::new_full_mindmap_state();
    full.selected = Some(WorkspaceNodeId::File(old_file));
    app.full_mindmap = Some(full);
    app.quick_slots.set(
        0,
        crate::quick_slots::QuickSlot {
            relative_path: "old.md".into(),
            context: crate::quick_slots::SlotContext {
                mode: crate::quick_slots::SlotMode::FullMindmap,
                preview_position: 0.75,
                ..Default::default()
            },
        },
    );
    app.quick_slots.active = Some(0);
    app.dirty = false;

    let _ = app.update(Message::FullMindmapReturnToFiles);

    let checkpointed = app.quick_slots.occupied(0).expect("active slot");
    assert_eq!(
        checkpointed.context.mode,
        crate::quick_slots::SlotMode::FullMindmap
    );
    assert_eq!(checkpointed.context.preview_position, 0.0);
    assert!(app.full_mindmap.is_none());
    assert!(app.sidebar_open);
    assert_eq!(app.sidebar_tab, SidebarTab::Files);

    let isolated = app.quick_slots_persistence_path.clone().unwrap();
    let _ = std::fs::remove_file(isolated);
    let _ = std::fs::remove_dir_all(app.workspace.take().unwrap());
}

#[test]
fn full_mindmap_normal_exit_messages_reconcile_hidden_snapshot_before_exit() {
    for (label, exit_message) in [
        ("escape", Message::ExitFullMindmap),
        ("toggle-shortcut", Message::ToggleFullMindmap),
    ] {
        let dir = full_mindmap_test_dir(label);
        let hidden = dir.join(".hidden.md");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("visible.md"), "# Visible\n").unwrap();
        std::fs::write(&hidden, "# Hidden\n").unwrap();

        let mut app = App::default();
        app.set_workspace(dir.clone(), false);
        app.dirty = true;
        app.source = "unsaved".into();
        app.full_mindmap = Some(full_workspace_state(&dir));
        let _ = app.update(Message::ToggleHidden);

        let _ = app.update(exit_message);
        let request = app
            .full_mindmap
            .as_ref()
            .and_then(|full| full.pending_workspace_load.clone())
            .expect("exit should wait for the reconciled snapshot");
        assert!(request.exit_after_refresh);
        assert!(!request.return_to_files_after);
        assert!(!app.workspace_files.contains(&hidden));

        complete_full_mindmap_workspace_load(&mut app);
        assert!(app.full_mindmap.is_none());
        assert!(app.workspace_files.contains(&hidden));
        assert!(!app.sidebar_open);
        assert!(app.dirty);
        assert_eq!(app.source, "unsaved");

        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn full_mindmap_hidden_workspace_refresh_is_background_additive_and_stale_safe() {
    let dir = full_mindmap_test_dir("workspace-hidden-refresh");
    let visible = dir.join("visible.md");
    let hidden_dir = dir.join(".hidden");
    let hidden = hidden_dir.join("secret.md");
    std::fs::create_dir_all(&hidden_dir).unwrap();
    std::fs::write(&visible, "# Visible\n").unwrap();
    std::fs::write(&hidden, "# Hidden\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    let mut full = full_workspace_state(&dir);
    full.selected = Some(WorkspaceNodeId::File(visible.clone()));
    app.full_mindmap = Some(full);

    let _ = app.update(Message::ToggleHidden);
    let shown_request = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_workspace_load
        .clone()
        .expect("hidden toggle must schedule a background workspace refresh");
    assert!(shown_request.preserve_navigation);
    assert!(!app.workspace_files.contains(&hidden));

    // A second toggle supersedes the first request. Its late completion
    // must not reintroduce hidden files under the newer hidden-off intent.
    let shown_snapshot = tree::build_workspace(&dir, true).unwrap();
    let _ = app.update(Message::ToggleHidden);
    let hidden_request = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_workspace_load
        .clone()
        .unwrap();
    assert_ne!(shown_request, hidden_request);
    let _ = app.update(Message::FullMindmapWorkspaceLoaded {
        request: shown_request,
        result: Ok((dir.clone(), shown_snapshot)),
    });
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().pending_workspace_load,
        Some(hidden_request)
    );
    assert!(!app.workspace_files.contains(&hidden));

    complete_full_mindmap_workspace_load(&mut app);
    assert!(app.workspace_files.contains(&visible));
    assert!(!app.workspace_files.contains(&hidden));
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().selected,
        Some(WorkspaceNodeId::File(visible.clone()))
    );

    let _ = app.update(Message::ToggleHidden);
    complete_full_mindmap_workspace_load(&mut app);
    let graph = app.full_mindmap_graph().unwrap();
    assert!(graph
        .node(&WorkspaceNodeId::File(visible.clone()))
        .is_some());
    assert!(app.workspace_files.contains(&hidden));
    assert!(graph.node(&WorkspaceNodeId::Folder(hidden_dir)).is_some());
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().selected,
        Some(WorkspaceNodeId::File(visible))
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_deselect_survives_preserve_navigation_hidden_refresh() {
    let dir = full_mindmap_test_dir("workspace-hidden-deselect-race");
    let visible = dir.join("visible.md");
    let hidden = dir.join(".hidden.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&visible, "# Visible\n").unwrap();
    std::fs::write(&hidden, "# Hidden\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    let mut full = full_workspace_state(&dir);
    let visible_id = WorkspaceNodeId::File(visible.clone());
    full.selected = Some(visible_id.clone());
    full.focus_request = Some(visible_id);
    app.full_mindmap = Some(full);

    let _ = app.update(Message::ToggleHidden);
    let request = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_workspace_load
        .clone()
        .unwrap();
    assert!(request.preserve_navigation);

    let _ = app.update(Message::FullMindmapDeselect);
    let full = app.full_mindmap.as_ref().unwrap();
    assert_eq!(full.pending_workspace_load, Some(request));
    assert_eq!(full.deferred_file_selection, None);
    assert_eq!(full.selected, None);
    assert_eq!(full.focus_request, None);
    assert!(!full.panel_open);

    let accepted = complete_full_mindmap_workspace_load(&mut app);
    assert!(accepted.preserve_navigation);
    assert!(app.workspace_files.contains(&visible));
    assert!(app.workspace_files.contains(&hidden));
    assert!(app.workspace_snapshot_show_hidden);
    let full = app.full_mindmap.as_ref().unwrap();
    assert!(full.pending_workspace_load.is_none());
    assert_eq!(full.selected, None);
    assert_eq!(full.focus_request, None);
    assert!(!full.panel_open);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_hidden_refresh_survives_selection_and_keeps_latest_valid_node() {
    let dir = full_mindmap_test_dir("workspace-hidden-selection-race");
    let first = dir.join("first.md");
    let second = dir.join("second.md");
    let hidden = dir.join(".secret.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&first, "# First\n").unwrap();
    std::fs::write(&second, "# Second\n").unwrap();
    std::fs::write(&hidden, "# Hidden\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    let mut full = full_workspace_state(&dir);
    full.selected = Some(WorkspaceNodeId::File(first.clone()));
    app.full_mindmap = Some(full);

    let _ = app.update(Message::ToggleHidden);
    let request = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_workspace_load
        .clone()
        .unwrap();
    let _ = app.update(Message::FullMindmapSelectNode(WorkspaceNodeId::File(
        second.clone(),
    )));
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().pending_workspace_load,
        Some(request.clone())
    );

    let snapshot = tree::build_workspace(&dir, true).unwrap();
    let _ = app.update(Message::FullMindmapWorkspaceLoaded {
        request,
        result: Ok((dir.clone(), snapshot)),
    });
    complete_full_mindmap_folder_loads(&mut app);

    let graph = app.full_mindmap_graph().unwrap();
    assert!(graph.node(&WorkspaceNodeId::File(first)).is_some());
    assert!(graph.node(&WorkspaceNodeId::File(second.clone())).is_some());
    assert!(graph.node(&WorkspaceNodeId::File(hidden)).is_some());
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().selected,
        Some(WorkspaceNodeId::File(second))
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_hidden_refresh_preserves_pending_preview_and_completion() {
    let dir = full_mindmap_test_dir("workspace-hidden-preview-race");
    let file = dir.join("preview.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&file, "# Preview\n").unwrap();
    std::fs::write(dir.join(".hidden.md"), "# Hidden\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.full_mindmap = Some(full_workspace_state(&dir));
    let _ = app.update(Message::ToggleHidden);
    let workspace_request = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_workspace_load
        .clone()
        .unwrap();
    let _ = app.update(Message::FullMindmapSelectNode(WorkspaceNodeId::File(
        file.clone(),
    )));
    let preview_request = settle_full_mindmap_preview(&mut app);

    let snapshot = tree::build_workspace(&dir, true).unwrap();
    let _ = app.update(Message::FullMindmapWorkspaceLoaded {
        request: workspace_request,
        result: Ok((dir.clone(), snapshot)),
    });
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().pending_preview,
        Some(preview_request.clone())
    );

    accept_full_mindmap_preview(
        &mut app,
        &preview_request,
        file.clone(),
        "# Preview\n".into(),
    );
    assert!(app.full_mindmap.as_ref().unwrap().pending_preview.is_none());
    assert!(matches!(
        &app.full_mindmap.as_ref().unwrap().preview,
        FullMindmapPreview::Document { path, .. } if path == &file
    ));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_hidden_refresh_serializes_open_in_both_completion_orders() {
    for stale_first in [true, false] {
        let label = if stale_first {
            "workspace-hidden-open-stale-first"
        } else {
            "workspace-hidden-open-current-first"
        };
        let dir = full_mindmap_test_dir(label);
        let file = dir.join("open.md");
        let hidden = dir.join(".hidden.md");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&file, "# Open\n").unwrap();
        std::fs::write(&hidden, "# Hidden\n").unwrap();

        let mut app = App::default();
        app.set_workspace(dir.clone(), false);
        let mut full = full_workspace_state(&dir);
        full.selected = Some(WorkspaceNodeId::File(file.clone()));
        app.full_mindmap = Some(full);
        let _ = app.update(Message::ToggleHidden);
        let stale_refresh = app
            .full_mindmap
            .as_ref()
            .unwrap()
            .pending_workspace_load
            .clone()
            .unwrap();

        let _ = app.update(Message::FullMindmapActivate);
        let accepted_refresh = app
            .full_mindmap
            .as_ref()
            .unwrap()
            .pending_workspace_load
            .clone()
            .expect("activation should supersede the filter refresh");
        assert_ne!(stale_refresh, accepted_refresh);
        assert_eq!(accepted_refresh.open_after.as_ref(), Some(&file));
        assert!(app.full_mindmap.as_ref().unwrap().pending_open.is_none());

        let hidden_snapshot = tree::build_workspace(&dir, true).unwrap();
        if stale_first {
            let _ = app.update(Message::FullMindmapWorkspaceLoaded {
                request: stale_refresh.clone(),
                result: Ok((dir.clone(), hidden_snapshot.clone())),
            });
            assert_eq!(
                app.full_mindmap.as_ref().unwrap().pending_workspace_load,
                Some(accepted_refresh.clone())
            );
            assert!(app.full_mindmap.as_ref().unwrap().pending_open.is_none());
        }

        let _ = app.update(Message::FullMindmapWorkspaceLoaded {
            request: accepted_refresh,
            result: Ok((dir.clone(), hidden_snapshot.clone())),
        });
        let open_request = app
            .full_mindmap
            .as_ref()
            .unwrap()
            .pending_open
            .clone()
            .expect("file read starts only after the snapshot is accepted");
        assert!(app.workspace_snapshot_show_hidden);
        assert!(app.workspace_files.contains(&hidden));

        if !stale_first {
            let _ = app.update(Message::FullMindmapWorkspaceLoaded {
                request: stale_refresh,
                result: Ok((dir.clone(), hidden_snapshot)),
            });
            assert_eq!(
                app.full_mindmap.as_ref().unwrap().pending_open,
                Some(open_request.clone())
            );
        }

        let _ = app.update(Message::FullMindmapFileLoaded {
            request: open_request,
            result: Ok((file.clone(), "# Open\n".into())),
        });
        assert!(app.full_mindmap.is_none());
        assert_eq!(app.file.as_deref(), Some(file.as_path()));
        assert_eq!(app.source, "# Open\n");
        assert!(app.workspace_snapshot_show_hidden);
        assert!(app.workspace_files.contains(&hidden));

        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn full_mindmap_refresh_open_respects_dirty_guard_and_pending_exit() {
    let dir = full_mindmap_test_dir("workspace-hidden-open-guards");
    let file = dir.join("open.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&file, "# Open\n").unwrap();
    std::fs::write(dir.join(".hidden.md"), "# Hidden\n").unwrap();

    let mut dirty_app = App::default();
    dirty_app.set_workspace(dir.clone(), false);
    let mut dirty_full = full_workspace_state(&dir);
    dirty_full.selected = Some(WorkspaceNodeId::File(file.clone()));
    dirty_app.full_mindmap = Some(dirty_full);
    dirty_app.dirty = true;
    let _ = dirty_app.update(Message::ToggleHidden);
    let refresh = dirty_app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_workspace_load
        .clone()
        .unwrap();
    let _ = dirty_app.update(Message::FullMindmapActivate);
    let dirty_full = dirty_app.full_mindmap.as_ref().unwrap();
    assert_eq!(dirty_full.pending_workspace_load, Some(refresh));
    assert!(dirty_full.pending_open.is_none());
    assert!(dirty_app.toast.is_some());

    let mut exiting_app = App::default();
    exiting_app.set_workspace(dir.clone(), false);
    let mut exiting_full = full_workspace_state(&dir);
    exiting_full.selected = Some(WorkspaceNodeId::File(file));
    exiting_app.full_mindmap = Some(exiting_full);
    let _ = exiting_app.update(Message::ToggleHidden);
    let _ = exiting_app.update(Message::ExitFullMindmap);
    let exit_refresh = exiting_app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_workspace_load
        .clone()
        .unwrap();
    assert!(exit_refresh.exit_after_refresh);
    let _ = exiting_app.update(Message::FullMindmapActivate);
    let exiting_full = exiting_app.full_mindmap.as_ref().unwrap();
    assert_eq!(
        exiting_full.pending_workspace_load,
        Some(exit_refresh.clone())
    );
    assert!(exiting_full.pending_open.is_none());
    assert!(exit_refresh.open_after.is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_hidden_refresh_failure_reverts_filter_and_remains_visible() {
    let dir = full_mindmap_test_dir("workspace-hidden-refresh-failure");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("visible.md"), "# Visible\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.full_mindmap = Some(full_workspace_state(&dir));
    let _ = app.update(Message::ToggleHidden);
    let request = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_workspace_load
        .clone()
        .unwrap();

    let _ = app.update(Message::FullMindmapWorkspaceLoaded {
        request,
        result: Err("permission denied".into()),
    });

    let full = app
        .full_mindmap
        .as_ref()
        .expect("ordinary failure stays open");
    assert!(full.pending_workspace_load.is_none());
    assert!(full
        .load_error
        .as_deref()
        .is_some_and(|error| error.contains("permission denied")));
    assert_eq!(app.show_hidden, app.workspace_snapshot_show_hidden);
    assert!(!app.show_hidden);
    assert!(app.error.is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_exit_refresh_failure_never_traps_and_surfaces_error() {
    for (label, exit_message, expect_files) in [
        ("exit-error", Message::ExitFullMindmap, false),
        ("toggle-exit-error", Message::ToggleFullMindmap, false),
        ("files-exit-error", Message::FullMindmapReturnToFiles, true),
    ] {
        let dir = full_mindmap_test_dir(label);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("visible.md"), "# Visible\n").unwrap();

        let mut app = App::default();
        app.set_workspace(dir.clone(), false);
        app.full_mindmap = Some(full_workspace_state(&dir));
        let _ = app.update(Message::ToggleHidden);
        let _ = app.update(exit_message);
        let request = app
            .full_mindmap
            .as_ref()
            .unwrap()
            .pending_workspace_load
            .clone()
            .expect("exit should wait for reconciliation");
        assert!(request.exit_after_refresh);

        let _ = app.update(Message::FullMindmapWorkspaceLoaded {
            request,
            result: Err("workspace disappeared".into()),
        });

        assert!(app.full_mindmap.is_none());
        assert_eq!(app.show_hidden, app.workspace_snapshot_show_hidden);
        assert!(!app.show_hidden);
        assert!(app
            .error
            .as_deref()
            .is_some_and(|error| error.contains("workspace disappeared")));
        assert_eq!(app.sidebar_open, expect_files);
        if expect_files {
            assert_eq!(app.sidebar_tab, SidebarTab::Files);
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn full_mindmap_cycles_its_own_panel_width() {
    let dir = full_mindmap_test_dir("panel-width");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("readme.md"), "# Project\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    let mut full = full_workspace_state(&dir);
    full.panel_open = false;
    full.panel_drag = Some((MIND_PANEL_DEFAULT, Some(400.0)));
    app.full_mindmap = Some(full);
    app.window_size = Some(iced::Size::new(1200.0, 800.0));
    app.mindmap_panel_width = 333.0;

    let _ = app.update(Message::FullMindmapCyclePanelWidth);
    let full = app.full_mindmap.as_ref().unwrap();
    assert!(full.panel_open);
    assert!(full.panel_drag.is_none());
    assert_eq!(full.panel_step, 1);
    assert_eq!(full.panel_width, 600.0);

    let _ = app.update(Message::FullMindmapCyclePanelWidth);
    assert_eq!(app.full_mindmap.as_ref().unwrap().panel_width, 800.0);
    let _ = app.update(Message::FullMindmapCyclePanelWidth);
    assert_eq!(app.full_mindmap.as_ref().unwrap().panel_width, 400.0);
    assert_eq!(app.mindmap_panel_width, 333.0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn mindmap_panel_width_preserves_fraction_on_large_windows() {
    let window_size = Some(iced::Size::new(2400.0, 1600.0));

    assert_eq!(mindmap_panel_width_for_step(0, window_size), 800.0);
    assert_eq!(mindmap_panel_width_for_step(1, window_size), 1200.0);
    assert_eq!(mindmap_panel_width_for_step(2, window_size), 1600.0);
}

#[test]
fn mindmap_panel_width_keeps_minimum_on_narrow_windows() {
    let window_size = Some(iced::Size::new(400.0, 800.0));

    assert_eq!(mindmap_panel_width_for_step(0, window_size), MIND_PANEL_MIN);
    assert_eq!(mindmap_panel_width_for_step(1, window_size), MIND_PANEL_MIN);
}

#[test]
fn mindmap_panel_drag_has_no_upper_cap() {
    assert_eq!(mindmap_panel_width_for_drag(800.0, 400.0, 0.0), 1200.0);
    assert_eq!(mindmap_panel_width_for_drag(400.0, 400.0, 600.0), 240.0);
}

#[test]
fn full_mindmap_focus_preserves_user_panel_visibility() {
    let dir = full_mindmap_test_dir("panel-visibility");
    let folder = dir.join("notes");
    let file = folder.join("guide.md");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(&file, "# Guide\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.full_mindmap = Some(full_workspace_state(&dir));
    assert!(!app.full_mindmap.as_ref().unwrap().panel_open);

    let _ = app.update(Message::FullMindmapSelectNode(WorkspaceNodeId::File(
        file.clone(),
    )));
    assert!(!app.full_mindmap.as_ref().unwrap().panel_open);

    let _ = app.update(Message::FullMindmapToggleNode(WorkspaceNodeId::Folder(
        folder.clone(),
    )));
    assert!(!app.full_mindmap.as_ref().unwrap().panel_open);

    let _ = app.update(Message::FullMindmapTogglePanel);
    assert!(app.full_mindmap.as_ref().unwrap().panel_open);
    let _ = app.update(Message::FullMindmapDeselect);
    assert!(app.full_mindmap.as_ref().unwrap().panel_open);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_panel_width_uses_default_before_first_resize() {
    let mut app = App::default();
    app.full_mindmap = Some(App::new_full_mindmap_state());
    let _ = app.update(Message::FullMindmapCyclePanelWidth);
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().panel_width,
        MIND_PANEL_DEFAULT
    );
}

#[test]
fn full_mindmap_file_selection_previews_without_mutating_dirty_document() {
    let dir = full_mindmap_test_dir("preview");
    std::fs::create_dir_all(&dir).unwrap();
    let preview_file = dir.join("preview.md");
    std::fs::write(&preview_file, "# Preview\n\nContent\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.file = Some(std::path::PathBuf::from("current.md"));
    app.source = "unsaved current document".into();
    app.saved_source = "saved current document".into();
    app.dirty = true;
    app.full_mindmap = Some(full_workspace_state(&dir));

    let _ = app.update(Message::FullMindmapSelectNode(WorkspaceNodeId::File(
        preview_file.clone(),
    )));
    assert!(app.full_mindmap.as_ref().unwrap().pending_preview.is_none());
    assert!(app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_preview_settle
        .is_some());
    let request = settle_full_mindmap_preview(&mut app);
    assert!(matches!(
        app.full_mindmap.as_ref().unwrap().preview,
        FullMindmapPreview::Loading(_)
    ));

    accept_full_mindmap_preview(
        &mut app,
        &request,
        preview_file.clone(),
        "# Preview\n\nContent\n".into(),
    );

    assert!(matches!(
        app.full_mindmap.as_ref().unwrap().preview,
        FullMindmapPreview::Document { ref path, .. } if path == &preview_file
    ));
    assert_eq!(
        app.file.as_deref(),
        Some(std::path::Path::new("current.md"))
    );
    assert_eq!(app.source, "unsaved current document");
    assert!(app.dirty);
    let _preview_view = app.view();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_preview_selection_waits_for_settle_before_read() {
    let dir = full_mindmap_test_dir("preview-settle");
    let file = dir.join("preview.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&file, "# Preview\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.full_mindmap = Some(full_workspace_state(&dir));

    let _ = app.update(Message::FullMindmapSelectNode(WorkspaceNodeId::File(
        file.clone(),
    )));
    let settle = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_preview_settle
        .clone()
        .expect("file selection should own a settle timer");
    assert!(app.full_mindmap.as_ref().unwrap().pending_preview.is_none());
    assert!(matches!(
        app.full_mindmap.as_ref().unwrap().preview,
        FullMindmapPreview::Loading(ref path) if path == &file
    ));

    let _ = app.update(Message::FullMindmapPreviewSettle { request: settle });
    let preview_request = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_preview
        .clone()
        .expect("settle should start the bounded read");
    assert!(app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_preview_settle
        .is_none());
    accept_full_mindmap_preview(
        &mut app,
        &preview_request,
        file.clone(),
        "# Preview\n".into(),
    );
    assert!(matches!(
        app.full_mindmap.as_ref().unwrap().preview,
        FullMindmapPreview::Document { ref path, .. } if path == &file
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_preview_settle_uses_300ms_without_changing_document_panel() {
    assert_eq!(MINDMAP_PANEL_SETTLE_MS, 75);
    assert_eq!(FULL_MINDMAP_PREVIEW_SETTLE_MS, 300);
    assert_ne!(FULL_MINDMAP_PREVIEW_SETTLE_MS, MINDMAP_PANEL_SETTLE_MS);
}

#[test]
fn full_mindmap_preview_keeps_blocks_beyond_legacy_caps() {
    let dir = full_mindmap_test_dir("preview-complete-blocks");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("large.md");
    let mut source = String::new();
    for index in 0..(crate::virt::VIRT_MIN_BLOCKS + 12) {
        source.push_str(&format!("# Heading {index}\n\n{}\n\n", "x".repeat(1200)));
    }
    std::fs::write(&file, &source).unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.full_mindmap = Some(full_workspace_state(&dir));
    let _ = app.update(Message::FullMindmapSelectNode(WorkspaceNodeId::File(
        file.clone(),
    )));
    let request = settle_full_mindmap_preview(&mut app);
    let _ = app.update(Message::FullMindmapPreviewLoaded {
        request: request.clone(),
        result: Ok((file.clone(), source.clone())),
    });
    // Large complete previews parse off-thread in production. Feed the
    // accepted worker result directly here so this regression remains
    // deterministic without waiting on the runtime task scheduler.
    let parsed = parse_full_mindmap_preview_blocking(file.clone(), source);
    let _ = app.update(Message::FullMindmapPreviewParsed {
        request,
        result: Ok(parsed),
    });

    let full = app.full_mindmap.as_ref().unwrap();
    match &full.preview {
        FullMindmapPreview::Document {
            blocks, truncated, ..
        } => {
            assert!(blocks.len() > MIND_PANEL_MAX_BLOCKS);
            assert!(!truncated);
        }
        other => panic!("expected complete document preview, got {other:?}"),
    }
    assert!(full.preview_window.active);
    assert!(full.preview_window.display.len() > MIND_PANEL_MAX_BLOCKS);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_preview_virtual_window_is_owned_and_ranges_large_documents() {
    let file = PathBuf::from("preview.md");
    let blocks = (0..320)
        .map(|index| {
            (
                BlockId(index),
                Block::Paragraph(vec![Inline::Text("x".repeat(80))]),
            )
        })
        .collect::<Vec<_>>();
    let mut app = App::default();
    app.virt_window.range = (7, 9);
    app.virt_window.active = true;
    app.full_mindmap = Some(App::new_full_mindmap_state());
    {
        let full = app.full_mindmap.as_mut().unwrap();
        full.selected = Some(WorkspaceNodeId::File(file.clone()));
        full.preview = FullMindmapPreview::Document {
            path: file,
            blocks: blocks.clone(),
            truncated: false,
            shape: None,
            assets: None,
        };
    }
    app.rebuild_full_mindmap_preview_here();

    let full = app.full_mindmap.as_ref().unwrap();
    assert!(full.preview_window.active);
    assert!(full.preview_window.range.1 - full.preview_window.range.0 < blocks.len());
    assert!(full.preview_window.bottom_spacer().is_some());
    // Building the panel must not mutate the normal document window.
    assert_eq!(app.virt_window.range, (7, 9));
    assert!(app.virt_window.active);
}

#[test]
fn full_mindmap_preview_geometry_refresh_discards_sparse_height_adjustments() {
    let file = PathBuf::from("geometry.md");
    let blocks = (0..320)
        .map(|index| {
            (
                BlockId(index),
                Block::Paragraph(vec![Inline::Text("x".into())]),
            )
        })
        .collect::<Vec<_>>();
    let mut app = App::default();
    let mut full = App::new_full_mindmap_state();
    full.selected = Some(WorkspaceNodeId::File(file.clone()));
    full.preview_identity = Some(PendingFullMindmapPreview {
        id: 1,
        path: file.clone(),
    });
    full.preview = FullMindmapPreview::Document {
        path: file,
        blocks,
        truncated: false,
        shape: None,
        assets: None,
    };
    app.full_mindmap = Some(full);
    app.rebuild_full_mindmap_preview_here();
    let dpos = 10;
    let base = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .preview_window
        .prefix_at(dpos + 1);
    app.full_mindmap
        .as_mut()
        .unwrap()
        .preview_window
        .apply_height_delta(dpos, 120.0);
    assert_eq!(
        app.full_mindmap
            .as_ref()
            .unwrap()
            .preview_window
            .prefix_at(dpos + 1),
        base + 120.0
    );

    // Theme/font/panel-width/window-bound changes all route through this
    // helper. They invalidate real measured heights and sparse deltas
    // before rebuilding the retained worker shape.
    let _ = app.refresh_full_mindmap_preview_heights();
    assert_eq!(
        app.full_mindmap
            .as_ref()
            .unwrap()
            .preview_window
            .prefix_at(dpos + 1),
        base
    );
}

#[test]
fn full_mindmap_preview_asset_prime_is_visible_range_bounded() {
    let file = PathBuf::from("asset-heavy.md");
    let blocks = (0..320u64)
        .map(|index| {
            (
                BlockId(index),
                Block::Paragraph(vec![Inline::Text("visible".into())]),
            )
        })
        .collect::<Vec<_>>();
    let mut by_block = HashMap::new();
    for (id, _) in &blocks {
        by_block.insert(
            *id,
            vec![FullMindmapPreviewAsset::Image(format!(
                "https://example.test/{}.png",
                id.0
            ))],
        );
    }
    let mut full = App::new_full_mindmap_state();
    full.selected = Some(WorkspaceNodeId::File(file.clone()));
    full.preview_identity = Some(PendingFullMindmapPreview {
        id: 1,
        path: file.clone(),
    });
    full.preview = FullMindmapPreview::Document {
        path: file,
        blocks,
        truncated: false,
        shape: None,
        assets: Some(Arc::new(FullMindmapPreviewAssetIndex { by_block })),
    };
    let mut app = App::default();
    app.full_mindmap = Some(full);
    app.rebuild_full_mindmap_preview_here();
    let _ = app.prime_full_mindmap_preview_assets();
    let full = app.full_mindmap.as_ref().unwrap();
    assert!(full.preview_loading_images.len() <= FULL_MINDMAP_PREVIEW_ASSET_BATCH);
    assert!(full.preview_asset_cursor <= FULL_MINDMAP_PREVIEW_ASSET_BATCH);
}

#[test]
fn full_mindmap_preview_failed_image_is_terminal_for_current_wave() {
    let file = PathBuf::from("asset-failure.md");
    let url = "https://example.test/permanent.png".to_string();
    let blocks = vec![(
        BlockId(0),
        Block::Image {
            url: url.clone(),
            alt: "permanent failure".into(),
        },
    )];
    let mut by_block = HashMap::new();
    by_block.insert(
        BlockId(0),
        vec![FullMindmapPreviewAsset::Image(url.clone())],
    );
    let mut full = App::new_full_mindmap_state();
    full.selected = Some(WorkspaceNodeId::File(file.clone()));
    full.preview_identity = Some(PendingFullMindmapPreview {
        id: 1,
        path: file.clone(),
    });
    full.preview = FullMindmapPreview::Document {
        path: file,
        blocks,
        truncated: false,
        shape: None,
        assets: Some(Arc::new(FullMindmapPreviewAssetIndex { by_block })),
    };
    let mut app = App::default();
    app.full_mindmap = Some(full);
    app.rebuild_full_mindmap_preview_here();
    let _ = app.prime_full_mindmap_preview_assets();
    let identity = FullMindmapPreviewIdentity {
        namespace: app.full_mindmap.as_ref().unwrap().preview_namespace,
        request: app
            .full_mindmap
            .as_ref()
            .unwrap()
            .preview_identity
            .clone()
            .unwrap(),
    };
    assert!(app
        .full_mindmap
        .as_ref()
        .unwrap()
        .preview_loading_images
        .contains_key(&url));
    let range = app.full_mindmap.as_ref().unwrap().preview_window.range;
    let wave = app.full_mindmap.as_ref().unwrap().preview_asset_wave_id;

    // The accepted failure is terminal for this identity/range. The
    // handler still primes the next descriptor, but never re-dispatches
    // the same permanently failing URL.
    let _ = app.update(Message::FullMindmapPreviewImageFetched {
        identity,
        range,
        wave,
        url: url.clone(),
        result: Err("permanent failure".into()),
    });
    let full = app.full_mindmap.as_ref().unwrap();
    assert!(full.preview_failed_images.contains(&url));
    assert!(full.preview_loading_images.is_empty());
    assert!(matches!(
        app.image_cache.get(&url),
        Some(ImageState::Failed)
    ));

    // Identity-less legacy document completion cannot replace a
    // preview-owned terminal failure with a late successful fetch.
    let _ = app.update(Message::ImageFetched(url.clone(), Ok(vec![1, 2, 3])));
    assert!(app
        .full_mindmap
        .as_ref()
        .unwrap()
        .preview_failed_images
        .contains(&url));
    assert!(matches!(
        app.image_cache.get(&url),
        Some(ImageState::Failed)
    ));

    // A same-range prime (including a cursor replay caused by another
    // layout callback) keeps the failed sentinel terminal rather than
    // deleting it and starting an unbounded retry loop.
    app.full_mindmap.as_mut().unwrap().preview_asset_cursor = 0;
    let _ = app.prime_full_mindmap_preview_assets();
    let full = app.full_mindmap.as_ref().unwrap();
    assert!(full.preview_loading_images.is_empty());
    assert!(full.preview_failed_images.contains(&url));
    assert!(matches!(
        app.image_cache.get(&url),
        Some(ImageState::Failed)
    ));
}

#[test]
fn full_mindmap_preview_old_wave_failure_cannot_mutate_reentered_range() {
    let file = PathBuf::from("asset-wave-reentry.md");
    let url = "https://example.test/retry-on-range.png".to_string();
    let blocks = vec![
        (
            BlockId(0),
            Block::Image {
                url: url.clone(),
                alt: "range A".into(),
            },
        ),
        (
            BlockId(1),
            Block::Image {
                url: url.clone(),
                alt: "range B".into(),
            },
        ),
    ];
    let mut by_block = HashMap::new();
    by_block.insert(
        BlockId(0),
        vec![FullMindmapPreviewAsset::Image(url.clone())],
    );
    by_block.insert(
        BlockId(1),
        vec![FullMindmapPreviewAsset::Image(url.clone())],
    );
    let mut full = App::new_full_mindmap_state();
    full.selected = Some(WorkspaceNodeId::File(file.clone()));
    full.preview_identity = Some(PendingFullMindmapPreview {
        id: 1,
        path: file.clone(),
    });
    full.preview = FullMindmapPreview::Document {
        path: file,
        blocks,
        truncated: false,
        shape: None,
        assets: Some(Arc::new(FullMindmapPreviewAssetIndex { by_block })),
    };
    let mut app = App::default();
    app.full_mindmap = Some(full);
    app.rebuild_full_mindmap_preview_here();
    app.full_mindmap.as_mut().unwrap().preview_window.range = (0, 1);
    let _ = app.prime_full_mindmap_preview_assets();
    let identity = FullMindmapPreviewIdentity {
        namespace: app.full_mindmap.as_ref().unwrap().preview_namespace,
        request: app
            .full_mindmap
            .as_ref()
            .unwrap()
            .preview_identity
            .clone()
            .unwrap(),
    };
    let wave_a = app.full_mindmap.as_ref().unwrap().preview_asset_wave_id;
    assert_eq!(
        app.full_mindmap
            .as_ref()
            .unwrap()
            .preview_loading_images
            .get(&url)
            .unwrap()
            .range,
        (0, 1)
    );

    // Move off-screen. The old Loading sentinel is drained before the
    // new wave, so a hung A request cannot consume the 64-item cap.
    app.full_mindmap.as_mut().unwrap().preview_window.range = (1, 2);
    let _ = app.prime_full_mindmap_preview_assets();
    let wave_b = app.full_mindmap.as_ref().unwrap().preview_asset_wave_id;
    assert_ne!(wave_a, wave_b);
    assert_eq!(
        app.full_mindmap
            .as_ref()
            .unwrap()
            .preview_loading_images
            .get(&url)
            .unwrap()
            .range,
        (1, 2)
    );

    // Re-enter A before its original request completes. The monotonic
    // wave id distinguishes this A from the old A even though the range
    // and URL are identical.
    app.full_mindmap.as_mut().unwrap().preview_window.range = (0, 1);
    let _ = app.prime_full_mindmap_preview_assets();
    let wave_a2 = app.full_mindmap.as_ref().unwrap().preview_asset_wave_id;
    assert_ne!(wave_a, wave_a2);
    assert_ne!(wave_b, wave_a2);
    assert_eq!(
        app.full_mindmap
            .as_ref()
            .unwrap()
            .preview_loading_images
            .get(&url)
            .unwrap()
            .wave,
        wave_a2
    );

    // The old A failure is stale: it must not mark the re-entered A wave
    // terminal or remove its current Loading sentinel.
    let _ = app.update(Message::FullMindmapPreviewImageFetched {
        identity: identity.clone(),
        range: (0, 1),
        wave: wave_a,
        url: url.clone(),
        result: Err("old range failure".into()),
    });
    let full = app.full_mindmap.as_ref().unwrap();
    assert!(!full.preview_failed_images.contains(&url));
    assert_eq!(full.preview_loading_images.get(&url).unwrap().wave, wave_a2);
    assert!(matches!(
        app.image_cache.get(&url),
        Some(ImageState::Loading)
    ));

    // The current wave's failure is accepted once and remains terminal
    // across same-range cursor re-primes.
    let _ = app.update(Message::FullMindmapPreviewImageFetched {
        identity,
        range: (0, 1),
        wave: wave_a2,
        url: url.clone(),
        result: Err("current range failure".into()),
    });
    let full = app.full_mindmap.as_ref().unwrap();
    assert!(full.preview_failed_images.contains(&url));
    assert!(full.preview_loading_images.is_empty());
    assert!(matches!(
        app.image_cache.get(&url),
        Some(ImageState::Failed)
    ));
    app.full_mindmap.as_mut().unwrap().preview_asset_cursor = 0;
    let _ = app.prime_full_mindmap_preview_assets();
    assert!(app
        .full_mindmap
        .as_ref()
        .unwrap()
        .preview_loading_images
        .is_empty());
}

#[test]
fn full_mindmap_preview_duplicate_diagram_completion_cannot_overwrite_ready() {
    let file = PathBuf::from("diagram-wave.md");
    let mut full = App::new_full_mindmap_state();
    full.selected = Some(WorkspaceNodeId::File(file.clone()));
    full.preview_identity = Some(PendingFullMindmapPreview {
        id: 1,
        path: file.clone(),
    });
    full.preview = FullMindmapPreview::Document {
        path: file,
        blocks: Vec::new(),
        truncated: false,
        shape: None,
        assets: None,
    };
    full.preview_window.range = (0, 1);
    let mut app = App::default();
    let theme_id = app.diagram_theme_id;
    let key = (77, theme_id);
    app.full_mindmap = Some(full);
    let identity = FullMindmapPreviewIdentity {
        namespace: app.full_mindmap.as_ref().unwrap().preview_namespace,
        request: app
            .full_mindmap
            .as_ref()
            .unwrap()
            .preview_identity
            .clone()
            .unwrap(),
    };
    let range = (0, 1);
    let wave = app.full_mindmap.as_ref().unwrap().preview_asset_wave_id;
    let owner = FullMindmapPreviewAssetIdentity {
        preview: identity.clone(),
        range,
        wave,
    };
    app.diagram_cache
        .put(key, crate::diagram::DiagramState::Pending);
    app.full_mindmap
        .as_mut()
        .unwrap()
        .preview_pending_diagrams
        .insert(key, owner);
    let rendered = crate::diagram::RenderOutput {
        svg: vec![1, 2, 3],
        rgba: vec![255, 0, 0, 255],
        w: 1,
        h: 1,
    };
    let _ = app.update(Message::FullMindmapPreviewDiagramRendered {
        identity: identity.clone(),
        range,
        wave,
        hash: key.0,
        theme_id,
        result: Ok(rendered),
    });
    assert!(app
        .full_mindmap
        .as_ref()
        .unwrap()
        .preview_pending_diagrams
        .is_empty());
    assert!(matches!(
        app.diagram_cache.peek(&key),
        Some(crate::diagram::DiagramState::Ready { .. })
    ));

    // The current identity is still valid, but its Pending owner is gone.
    // A duplicate late Err must not replace the accepted Ready result.
    let _ = app.update(Message::FullMindmapPreviewDiagramRendered {
        identity,
        range,
        wave,
        hash: key.0,
        theme_id,
        result: Err("late duplicate".into()),
    });
    assert!(matches!(
        app.diagram_cache.peek(&key),
        Some(crate::diagram::DiagramState::Ready { .. })
    ));
}

#[test]
fn full_mindmap_preview_selection_resets_window_and_stale_measurement() {
    let dir = full_mindmap_test_dir("preview-window-stale");
    std::fs::create_dir_all(&dir).unwrap();
    let first = dir.join("first.md");
    let second = dir.join("second.md");
    std::fs::write(&first, "# First\n").unwrap();
    std::fs::write(&second, "# Second\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.full_mindmap = Some(full_workspace_state(&dir));
    let _ = app.update(Message::FullMindmapSelectNode(WorkspaceNodeId::File(
        first.clone(),
    )));
    let first_request = settle_full_mindmap_preview(&mut app);
    accept_full_mindmap_preview(&mut app, &first_request, first.clone(), "# First\n".into());
    app.full_mindmap
        .as_mut()
        .unwrap()
        .preview_height_cache
        .set_measured(BlockId(0), 777.0);
    let old_generation = app.full_mindmap.as_ref().unwrap().preview_generation;
    let _ = app.update(Message::FullMindmapSelectNode(WorkspaceNodeId::File(
        second.clone(),
    )));
    let full = app.full_mindmap.as_ref().unwrap();
    assert!(full.preview_generation > old_generation);
    assert!(full.preview_window.display.is_empty());
    assert_ne!(
        full.preview_height_cache.get(BlockId(0), &Block::Rule),
        777.0
    );

    let current = settle_full_mindmap_preview(&mut app);
    accept_full_mindmap_preview(&mut app, &current, second.clone(), "# Second\n".into());
    let full = app.full_mindmap.as_ref().unwrap();
    let generation = full.preview_generation;
    let namespace = full.preview_namespace;
    let identity = full.preview_identity.as_ref().unwrap().id;
    let _ = app.update(Message::FullMindmapPreviewBlockHeightsMeasured {
        path: first,
        namespace,
        identity,
        generation: generation.saturating_sub(1),
        measured: vec![(BlockId(0), 9999.0)],
        at_offset: 0.0,
    });
    assert!(matches!(
        &app.full_mindmap.as_ref().unwrap().preview,
        FullMindmapPreview::Document { path, .. } if path == &second
    ));
    assert_ne!(
        app.full_mindmap
            .as_ref()
            .unwrap()
            .preview_height_cache
            .get(BlockId(0), &Block::Rule),
        9999.0
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_stale_preview_settle_after_new_selection_is_ignored() {
    let dir = full_mindmap_test_dir("preview-settle-stale");
    let first = dir.join("first.md");
    let second = dir.join("second.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&first, "# First\n").unwrap();
    std::fs::write(&second, "# Second\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.full_mindmap = Some(full_workspace_state(&dir));
    let _ = app.update(Message::FullMindmapSelectNode(WorkspaceNodeId::File(first)));
    let stale = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_preview_settle
        .clone()
        .unwrap();
    let _ = app.update(Message::FullMindmapSelectNode(WorkspaceNodeId::File(
        second.clone(),
    )));
    let current = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_preview_settle
        .clone()
        .unwrap();
    assert_ne!(stale, current);

    let _ = app.update(Message::FullMindmapPreviewSettle { request: stale });
    assert_eq!(
        app.full_mindmap
            .as_ref()
            .and_then(|full| full.pending_preview_settle.clone()),
        Some(current.clone())
    );
    assert!(app.full_mindmap.as_ref().unwrap().pending_preview.is_none());

    let _ = app.update(Message::FullMindmapPreviewSettle { request: current });
    assert_eq!(
        app.full_mindmap
            .as_ref()
            .and_then(|full| full.pending_preview.as_ref().map(|request| &request.path)),
        Some(&second)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_preview_reset_keeps_settle_worker_alive() {
    let dir = full_mindmap_test_dir("preview-settle-worker-rearm");
    let file = dir.join("preview.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&file, "# Preview\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.full_mindmap = Some(full_workspace_state(&dir));
    let _ = app.update(Message::FullMindmapSelectNode(WorkspaceNodeId::File(
        file.clone(),
    )));
    let full = app.full_mindmap.as_ref().unwrap();
    assert!(full.pending_preview_settle.is_some());
    assert!(full.preview_settle_worker_started);
    assert!(matches!(&full.preview, FullMindmapPreview::Loading(path) if path == &file));
    // Resetting while the timer/read is in flight clears only the watch
    // value. The persistent stream remains the sole worker and can own a
    // later selection without leaving a dead receiver behind.
    app.reset_full_mindmap_preview_window();
    assert!(
        app.full_mindmap
            .as_ref()
            .unwrap()
            .preview_settle_worker_started
    );
    let _ = app.update(Message::FullMindmapSelectNode(WorkspaceNodeId::File(
        file.clone(),
    )));
    assert!(app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_preview_settle
        .is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_first_preview_selection_delivers_settle_to_worker() {
    let dir = full_mindmap_test_dir("preview-settle-worker-first-selection");
    let file = dir.join("preview.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&file, "# Preview\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.full_mindmap = Some(full_workspace_state(&dir));
    assert!(
        !app.full_mindmap
            .as_ref()
            .unwrap()
            .preview_settle_worker_started
    );

    // This is the production call order: the first selection starts the
    // persistent worker and publishes its request in one update. Observe
    // the actual settle stream from a receiver created after scheduling;
    // no test-only send after subscription can hide a missed first wake.
    let _worker_task = app.schedule_full_mindmap_preview(Some(file.clone()));
    let request = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_preview_settle.clone())
        .expect("first selection should own a pending settle");
    assert!(
        app.full_mindmap
            .as_ref()
            .unwrap()
            .preview_settle_worker_started
    );
    let receiver = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .preview_settle_tx
        .subscribe();

    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        use futures::StreamExt;
        let mut stream = Box::pin(full_mindmap_preview_settle_stream(receiver));

        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(2), stream.next(),)
                .await
                .expect("first settle should not remain pending"),
            Some(request),
        );
    });

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_preview_settle_stream_reuses_one_latest_only_worker() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        use futures::StreamExt;

        let (sender, receiver) = tokio::sync::watch::channel(None);
        let mut stream = Box::pin(full_mindmap_preview_settle_stream(receiver));
        let first = PendingFullMindmapPreviewSettle {
            id: 1,
            path: PathBuf::from("/preview/first.md"),
        };
        sender.send(Some(first.clone())).unwrap();
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(2), stream.next())
                .await
                .unwrap(),
            Some(first)
        );

        let second = PendingFullMindmapPreviewSettle {
            id: 2,
            path: PathBuf::from("/preview/second.md"),
        };
        // Replacing the watch value wakes the same stream; no second
        // settle worker is needed for a later selection.
        sender.send(Some(second.clone())).unwrap();
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(2), stream.next())
                .await
                .unwrap(),
            Some(second)
        );
    });
}

#[test]
fn full_mindmap_stale_preview_parse_after_new_selection_is_ignored() {
    let dir = full_mindmap_test_dir("preview-parse-stale");
    let first = dir.join("first.md");
    let second = dir.join("second.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&first, "# First\n").unwrap();
    std::fs::write(&second, "# Second\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.full_mindmap = Some(full_workspace_state(&dir));
    let _ = app.update(Message::FullMindmapSelectNode(WorkspaceNodeId::File(
        first.clone(),
    )));
    let first_request = settle_full_mindmap_preview(&mut app);
    let large_source = format!(
        "# First\n\n{}",
        "x".repeat(FULL_MINDMAP_PREVIEW_STALE_SOURCE_BYTES)
    );
    let _ = app.update(Message::FullMindmapPreviewLoaded {
        request: first_request.clone(),
        result: Ok((first.clone(), large_source)),
    });
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().pending_preview.as_ref(),
        Some(&first_request)
    );

    let _ = app.update(Message::FullMindmapSelectNode(WorkspaceNodeId::File(
        second.clone(),
    )));
    let _second_request = settle_full_mindmap_preview(&mut app);
    let stale_preview = parse_full_mindmap_preview_blocking(first.clone(), "# First\n".into());
    let _ = app.update(Message::FullMindmapPreviewParsed {
        request: first_request,
        result: Ok(stale_preview),
    });
    assert_eq!(
        app.full_mindmap
            .as_ref()
            .unwrap()
            .pending_preview
            .as_ref()
            .map(|pending| &pending.path),
        Some(&second)
    );
    assert!(matches!(
        &app.full_mindmap.as_ref().unwrap().preview,
        FullMindmapPreview::Loading(path) if path == &second
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_preview_loaded_defers_parse_for_short_source() {
    let dir = full_mindmap_test_dir("preview-parse-deferred");
    let file = dir.join("preview.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&file, "# Preview\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.full_mindmap = Some(full_workspace_state(&dir));
    let _ = app.update(Message::FullMindmapSelectNode(WorkspaceNodeId::File(
        file.clone(),
    )));
    let request = settle_full_mindmap_preview(&mut app);
    let source = "# Preview\n".to_string();

    // Loaded only hands source ownership to the worker. It must not parse
    // or materialize blocks synchronously on the update thread.
    let _ = app.update(Message::FullMindmapPreviewLoaded {
        request: request.clone(),
        result: Ok((file.clone(), source.clone())),
    });
    assert_eq!(
        app.full_mindmap
            .as_ref()
            .and_then(|full| full.pending_preview.as_ref()),
        Some(&request)
    );
    assert!(matches!(
        app.full_mindmap.as_ref().unwrap().preview,
        FullMindmapPreview::Loading(ref path) if path == &file
    ));

    let parsed = parse_full_mindmap_preview_blocking(file.clone(), source);
    let _ = app.update(Message::FullMindmapPreviewParsed {
        request,
        result: Ok(parsed),
    });
    assert!(matches!(
        app.full_mindmap.as_ref().unwrap().preview,
        FullMindmapPreview::Document { ref path, .. } if path == &file
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_preview_measurement_coalesces_and_releases_empty_result() {
    let root = PathBuf::from("/preview-measurement-coalesced");
    let file = root.join("preview.md");
    let blocks = (0..320u64)
        .map(|index| {
            (
                BlockId(index),
                Block::Paragraph(vec![crate::ast::Inline::Text("x".into())]),
            )
        })
        .collect();
    let mut full = App::new_full_mindmap_state();
    full.selected = Some(WorkspaceNodeId::File(file.clone()));
    full.preview = FullMindmapPreview::Document {
        path: file.clone(),
        blocks,
        truncated: false,
        shape: None,
        assets: None,
    };
    full.preview_identity = Some(PendingFullMindmapPreview {
        id: 1,
        path: file.clone(),
    });
    let mut app = App::default();
    app.full_mindmap = Some(full);
    app.rebuild_full_mindmap_preview_here();
    assert!(app.full_mindmap.as_ref().unwrap().preview_window.active);

    let _first = app.measure_full_mindmap_preview_heights();
    assert!(
        app.full_mindmap
            .as_ref()
            .unwrap()
            .preview_measurement_pending
    );
    let _second = app.measure_full_mindmap_preview_heights();
    assert!(
        app.full_mindmap
            .as_ref()
            .unwrap()
            .preview_measurement_pending
    );

    let full = app.full_mindmap.as_ref().unwrap();
    let generation = full.preview_generation;
    let namespace = full.preview_namespace;
    let identity = full.preview_identity.as_ref().unwrap().id;
    let _ = app.update(Message::FullMindmapPreviewBlockHeightsMeasured {
        path: file.clone(),
        namespace,
        identity,
        generation,
        measured: vec![(BlockId(0), 24.0)],
        at_offset: 0.0,
    });
    assert!(app
        .full_mindmap
        .as_ref()
        .unwrap()
        .preview_height_cache
        .is_measured(BlockId(0)));
    assert!(
        !app.full_mindmap
            .as_ref()
            .unwrap()
            .preview_measurement_pending
    );

    // A later operation may find no new IDs (for example while the
    // widget tree is being replaced); its empty completion still clears
    // the guard without creating a dispatch loop.
    let _third = app.measure_full_mindmap_preview_heights();
    assert!(
        app.full_mindmap
            .as_ref()
            .unwrap()
            .preview_measurement_pending
    );
    let full = app.full_mindmap.as_ref().unwrap();
    let generation = full.preview_generation;
    let namespace = full.preview_namespace;
    let identity = full.preview_identity.as_ref().unwrap().id;
    let file = app
        .full_mindmap
        .as_ref()
        .and_then(|full| match &full.preview {
            FullMindmapPreview::Document { path, .. } => Some(path.clone()),
            _ => None,
        })
        .unwrap();
    let range = app.full_mindmap.as_ref().unwrap().preview_window.range;
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().preview_measurement_range,
        Some(range)
    );
    let _ = app.update(Message::FullMindmapPreviewBlockHeightsMeasured {
        path: file,
        namespace,
        identity,
        generation,
        measured: Vec::new(),
        at_offset: 0.0,
    });
    assert!(
        !app.full_mindmap
            .as_ref()
            .unwrap()
            .preview_measurement_pending
    );
}

#[test]
fn full_mindmap_stale_preview_worker_exits_before_parsing() {
    let cancel = Arc::new(AtomicU64::new(7));
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let result = runtime.block_on(parse_full_mindmap_preview_guarded(
        PathBuf::from("/does-not-exist.md"),
        Arc::from("# stale"),
        cancel,
        6,
        Arc::new(tokio::sync::Semaphore::new(3)),
    ));
    assert!(matches!(
        result,
        Err(error) if error == FULL_MINDMAP_PREVIEW_CANCELLED
    ));
}

#[test]
fn full_mindmap_preview_settle_cancels_on_folder_selection_and_exit() {
    let dir = full_mindmap_test_dir("preview-settle-cancel");
    let file = dir.join("preview.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&file, "# Preview\n").unwrap();

    let mut folder_app = App::default();
    folder_app.set_workspace(dir.clone(), false);
    folder_app.full_mindmap = Some(full_workspace_state(&dir));
    let _ = folder_app.update(Message::FullMindmapSelectNode(WorkspaceNodeId::File(
        file.clone(),
    )));
    let stale_folder = folder_app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_preview_settle
        .clone()
        .unwrap();
    let _ = folder_app.update(Message::FullMindmapSelectNode(WorkspaceNodeId::Root(
        dir.clone(),
    )));
    assert!(folder_app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_preview_settle
        .is_none());
    let _ = folder_app.update(Message::FullMindmapPreviewSettle {
        request: stale_folder,
    });
    assert!(folder_app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_preview
        .is_none());

    let mut exit_app = App::default();
    exit_app.set_workspace(dir.clone(), false);
    exit_app.full_mindmap = Some(full_workspace_state(&dir));
    let _ = exit_app.update(Message::FullMindmapSelectNode(WorkspaceNodeId::File(
        file.clone(),
    )));
    let stale_exit = exit_app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_preview_settle
        .clone()
        .unwrap();
    let _ = exit_app.update(Message::ExitFullMindmap);
    assert!(exit_app.full_mindmap.is_none());
    let _ = exit_app.update(Message::FullMindmapPreviewSettle {
        request: stale_exit,
    });
    assert!(exit_app.full_mindmap.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_enter_bypasses_preview_settle_and_opens_immediately() {
    let dir = full_mindmap_test_dir("preview-settle-enter");
    let file = dir.join("open.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&file, "# Open\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    let mut full = full_workspace_state(&dir);
    full.selected = Some(WorkspaceNodeId::File(file.clone()));
    app.full_mindmap = Some(full);
    let _ = app.update(Message::FullMindmapSelectNode(WorkspaceNodeId::File(
        file.clone(),
    )));
    let stale_settle = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_preview_settle
        .clone()
        .unwrap();

    let _ = app.update(Message::FullMindmapActivate);
    let open_request = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_open.clone())
        .expect("Enter should start file activation immediately");
    assert!(app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_preview_settle
        .is_none());
    assert!(app.full_mindmap.as_ref().unwrap().pending_preview.is_none());
    let _ = app.update(Message::FullMindmapPreviewSettle {
        request: stale_settle,
    });
    assert_eq!(
        app.full_mindmap
            .as_ref()
            .and_then(|full| full.pending_open.clone()),
        Some(open_request.clone())
    );
    let _ = app.update(Message::FullMindmapFileLoaded {
        request: open_request,
        result: Ok((file.clone(), "# Open\n".into())),
    });
    assert!(app.full_mindmap.is_none());
    assert_eq!(app.view_mode, ViewMode::Mindmap);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_ready_preview_selection_does_not_duplicate_request() {
    let dir = full_mindmap_test_dir("preview-ready");
    let file = dir.join("ready.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&file, "# Ready\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    let mut full = full_workspace_state(&dir);
    full.selected = Some(WorkspaceNodeId::File(file.clone()));
    full.preview = FullMindmapPreview::Document {
        path: file.clone(),
        blocks: Vec::new(),
        truncated: false,
        shape: None,
        assets: None,
    };
    app.full_mindmap = Some(full);
    app.full_mindmap_request_seq = 41;

    let _ = app.update(Message::FullMindmapSelectNode(WorkspaceNodeId::File(
        file.clone(),
    )));
    let full = app.full_mindmap.as_ref().unwrap();
    assert_eq!(app.full_mindmap_request_seq, 41);
    assert!(full.pending_preview_settle.is_none());
    assert!(full.pending_preview.is_none());
    assert!(matches!(
        &full.preview,
        FullMindmapPreview::Document { path, .. } if path == &file
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_entry_previews_the_already_open_workspace_file() {
    let dir = full_mindmap_test_dir("initial-preview");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("current.md");
    std::fs::write(&file, "# Current\n\nPreview me\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.file = Some(file.clone());
    app.source = "# Current\n\nPreview me\n".into();
    app.saved_source = app.source.clone();

    let _ = app.enter_full_mindmap();
    complete_full_mindmap_folder_loads(&mut app);
    let preview_request = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_preview.clone())
        .expect("entry preview should own a parse request");
    let parsed = parse_full_mindmap_preview_blocking(file.clone(), app.source.clone());
    let _ = app.update(Message::FullMindmapPreviewParsed {
        request: preview_request,
        result: Ok(parsed),
    });

    let full = app.full_mindmap.as_ref().unwrap();
    assert_eq!(full.selected, Some(WorkspaceNodeId::File(file.clone())));
    assert!(matches!(
        &full.preview,
        FullMindmapPreview::Document { path, .. } if path == &file
    ));
    assert!(full.pending_preview.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_ignores_stale_preview_completion() {
    let dir = full_mindmap_test_dir("stale-preview");
    std::fs::create_dir_all(&dir).unwrap();
    let first = dir.join("first.md");
    let second = dir.join("second.md");
    std::fs::write(&first, "# First\n").unwrap();
    std::fs::write(&second, "# Second\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.full_mindmap = Some(full_workspace_state(&dir));
    let _ = app.update(Message::FullMindmapSelectNode(WorkspaceNodeId::File(
        first.clone(),
    )));
    let stale = settle_full_mindmap_preview(&mut app);
    let _ = app.update(Message::FullMindmapSelectNode(WorkspaceNodeId::File(
        second.clone(),
    )));
    let current = settle_full_mindmap_preview(&mut app);

    let _ = app.update(Message::FullMindmapPreviewLoaded {
        request: stale,
        result: Ok((first, "# Stale\n".into())),
    });

    assert_eq!(
        app.full_mindmap
            .as_ref()
            .and_then(|full| full.pending_preview.clone()),
        Some(current)
    );
    assert!(matches!(
        app.full_mindmap.as_ref().unwrap().preview,
        FullMindmapPreview::Loading(ref path) if path == &second
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_preview_reader_keeps_complete_source_before_parsing() {
    let dir = full_mindmap_test_dir("preview-complete-source");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("large.md");
    std::fs::write(&file, "x".repeat(MIND_PANEL_MAX_TEXT_BYTES * 2)).unwrap();

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (_, source) = runtime.block_on(load_full_mindmap_preview(file)).unwrap();
    assert_eq!(source.len(), MIND_PANEL_MAX_TEXT_BYTES * 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_view_builds_while_loading_and_after_workspace_acceptance() {
    let mut app = App::default();
    let _ = app.enter_full_mindmap();
    {
        let _loading_view = app.view();
    }

    let dir = full_mindmap_test_dir("view");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("readme.md"), "# Project\n").unwrap();
    app.set_workspace(dir.clone(), false);
    {
        let _workspace_view = app.view();
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_expansion_is_independent_from_document_and_sidebar_state() {
    let root = std::path::PathBuf::from("/workspace");
    let folder = root.join("src");
    let mut app = App::default();
    app.workspace = Some(root.clone());
    app.workspace_tree = Some(Node {
        path: root.clone(),
        name: "workspace".into(),
        is_dir: true,
        children: vec![Node {
            path: folder.clone(),
            name: "src".into(),
            is_dir: true,
            children: vec![Node {
                path: folder.join("app.rs"),
                name: "app.rs".into(),
                is_dir: false,
                children: Vec::new(),
                recursive_supported_file_count: None,
            }],
            recursive_supported_file_count: Some(tree::RecursiveFileCount::Exact(1)),
        }],
        recursive_supported_file_count: Some(tree::RecursiveFileCount::Exact(1)),
    });
    app.full_mindmap = Some(full_workspace_state(&root));
    app.expanded.insert(root.clone());
    app.mindmap_collapsed.insert(BlockId(99));

    let _ = app.update(Message::FullMindmapToggleNode(WorkspaceNodeId::Folder(
        folder.clone(),
    )));

    assert!(app
        .full_mindmap
        .as_ref()
        .unwrap()
        .expanded
        .contains(&folder));
    assert!(!app.expanded.contains(&folder));
    assert!(app.mindmap_collapsed.contains(&BlockId(99)));
}

#[test]
fn full_mindmap_file_open_successfully_loads_and_exits() {
    let dir = full_mindmap_test_dir("open");
    std::fs::create_dir_all(&dir).unwrap();
    let old = dir.join("old.md");
    let new = dir.join("new.md");
    std::fs::write(&old, "# Old\n").unwrap();
    std::fs::write(&new, "# New\n").unwrap();

    let mut app = App::default();
    app.workspace = Some(dir.clone());
    app.workspace_tree = Some(tree::build(&dir, false));
    app.file = Some(old.clone());
    app.source = "# Old\n".into();
    app.saved_source = app.source.clone();
    app.full_mindmap = Some(full_workspace_state(&dir));

    let _ = app.begin_full_mindmap_open(new.clone());
    let request = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_open
        .clone()
        .unwrap();
    let _ = app.update(Message::FullMindmapFileLoaded {
        request,
        result: Ok((new.clone(), "# New\n".into())),
    });

    assert!(app.full_mindmap.is_none());
    assert_eq!(app.file.as_deref(), Some(new.as_path()));
    assert_eq!(app.source, "# New\n");
    assert!(!app.dirty);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ipc_open_exits_full_mindmap_before_loading_document() {
    let old = std::path::PathBuf::from("/workspace/old.md");
    let new = std::path::PathBuf::from("/workspace/new.md");
    let mut app = App::default();
    app.file = Some(old);
    app.source = "# Old\n".into();
    app.saved_source = app.source.clone();
    app.full_mindmap = Some(full_workspace_state(std::path::Path::new("/workspace")));

    let _ = app.update(Message::Ipc(
        crate::ipc::Request {
            id: 1,
            cmd: crate::ipc::Cmd::Open {
                file: new.to_string_lossy().into_owned(),
                line: Some(4),
                section: Some("Target".into()),
                focus: crate::ipc::FocusBehavior::Suppress,
            },
        },
        std::sync::Arc::new(std::sync::Mutex::new(None)),
    ));

    assert!(app.full_mindmap.is_none());
    assert!(app.pending_ipc_file_open.is_none());
    assert_eq!(app.pending_nav.as_ref().and_then(|nav| nav.line), Some(4));
    assert_eq!(
        app.pending_nav
            .as_ref()
            .and_then(|nav| nav.section.as_deref()),
        Some("Target")
    );

    let _ = app.update(Message::FileLoaded(Ok((
        new.clone(),
        "# New\n\n# Target\n".into(),
    ))));

    assert!(app.full_mindmap.is_none());
    assert_eq!(app.file.as_deref(), Some(new.as_path()));
    assert!(app.pending_nav.is_none());
}

#[test]
fn ipc_reveal_exits_full_mindmap_before_loading_document() {
    let old = std::path::PathBuf::from("/workspace/old.md");
    let new = std::path::PathBuf::from("/workspace/new.md");
    let mut app = App::default();
    app.file = Some(old);
    app.source = "# Old\n".into();
    app.saved_source = app.source.clone();
    app.full_mindmap = Some(full_workspace_state(std::path::Path::new("/workspace")));

    let _ = app.update(Message::Ipc(
        crate::ipc::Request {
            id: 1,
            cmd: crate::ipc::Cmd::Reveal {
                file: new.to_string_lossy().into_owned(),
                focus: crate::ipc::FocusBehavior::Suppress,
            },
        },
        std::sync::Arc::new(std::sync::Mutex::new(None)),
    ));

    assert!(app.full_mindmap.is_none());
    assert!(app.pending_ipc_file_open.is_none());
    assert!(app.pending_nav.is_none());

    let _ = app.update(Message::FileLoaded(Ok((new.clone(), "# New\n".into()))));

    assert!(app.full_mindmap.is_none());
    assert_eq!(app.file.as_deref(), Some(new.as_path()));
}

#[test]
fn ipc_open_waits_for_stale_full_mindmap_snapshot_before_loading() {
    let dir = full_mindmap_test_dir("ipc-open-stale-snapshot");
    let old = dir.join("old.md");
    let new = dir.join("new.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&old, "# Old\n").unwrap();
    std::fs::write(&new, "# New\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.file = Some(old);
    app.source = "# Old\n".into();
    app.saved_source = app.source.clone();
    app.full_mindmap = Some(full_workspace_state(&dir));
    app.show_hidden = true;

    let _ = app.update(Message::Ipc(
        crate::ipc::Request {
            id: 1,
            cmd: crate::ipc::Cmd::Open {
                file: new.to_string_lossy().into_owned(),
                line: Some(3),
                section: Some("New".into()),
                focus: crate::ipc::FocusBehavior::Suppress,
            },
        },
        std::sync::Arc::new(std::sync::Mutex::new(None)),
    ));

    assert!(app.full_mindmap.is_some());
    assert!(app.pending_nav.is_none());
    assert!(app.pending_ipc_file_open.is_some());
    let workspace_request = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_workspace_load.clone())
        .expect("stale snapshot should delay the exit");
    assert!(workspace_request.exit_after_refresh);

    let snapshot = tree::build_workspace(&dir, true).unwrap();
    let _ = app.update(Message::FullMindmapWorkspaceLoaded {
        request: workspace_request,
        result: Ok((dir.clone(), snapshot)),
    });

    assert!(app.full_mindmap.is_none());
    assert!(app.pending_ipc_file_open.is_none());
    assert_eq!(app.pending_nav.as_ref().and_then(|nav| nav.line), Some(3));

    let _ = app.update(Message::FileLoaded(Ok((new.clone(), "# New\n".into()))));

    assert!(app.full_mindmap.is_none());
    assert_eq!(app.file.as_deref(), Some(new.as_path()));
    assert!(app.pending_nav.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ipc_open_survives_manual_refresh_during_full_mindmap_reconciliation() {
    let dir = full_mindmap_test_dir("ipc-open-manual-refresh");
    let old = dir.join("old.md");
    let new = dir.join("new.md");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&old, "# Old\n").unwrap();
    std::fs::write(&new, "# New\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.file = Some(old.clone());
    app.source = "# Old\n".into();
    app.saved_source = app.source.clone();
    app.full_mindmap = Some(full_workspace_state(&dir));
    app.show_hidden = true;

    let _ = app.update(Message::Ipc(
        crate::ipc::Request {
            id: 1,
            cmd: crate::ipc::Cmd::Open {
                file: new.to_string_lossy().into_owned(),
                line: Some(3),
                section: Some("New".into()),
                focus: crate::ipc::FocusBehavior::Suppress,
            },
        },
        std::sync::Arc::new(std::sync::Mutex::new(None)),
    ));
    let stale_reconciliation = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_workspace_load.clone())
        .expect("IPC open should wait for hidden-snapshot reconciliation");
    assert!(stale_reconciliation.exit_after_refresh);
    assert!(app.pending_ipc_file_open.is_some());

    let _ = app.update(Message::Refresh);
    let active_reconciliation = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_workspace_load.clone())
        .expect("manual refresh should preserve the reconciliation request");
    assert_eq!(active_reconciliation, stale_reconciliation);
    assert!(active_reconciliation.exit_after_refresh);
    let stale_file_refresh = app
        .pending_refresh_file
        .clone()
        .expect("the old visible file refresh may still be in flight");

    let snapshot = tree::build_workspace(&dir, true).unwrap();
    let _ = app.update(Message::FullMindmapWorkspaceLoaded {
        request: active_reconciliation,
        result: Ok((dir.clone(), snapshot)),
    });
    assert!(app.full_mindmap.is_none());
    assert!(app.pending_ipc_file_open.is_none());
    assert_eq!(app.pending_nav.as_ref().and_then(|nav| nav.line), Some(3));
    assert!(app.pending_refresh.is_none());

    let _ = app.update(Message::RefreshFileLoaded {
        request: stale_file_refresh,
        result: Ok((old, "# Stale refresh\n".into())),
    });
    assert_eq!(app.source, "# Old\n");

    let ipc_generation = app.file_refresh_generation;
    let _ = app.update(Message::FileLoadCompleted {
        generation: ipc_generation,
        result: Ok((new.clone(), "# New\n".into())),
    });
    assert_eq!(app.file.as_deref(), Some(new.as_path()));
    assert_eq!(app.source, "# New\n");
    assert!(app.pending_nav.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ipc_open_survives_full_mindmap_root_change_during_reconciliation() {
    let dir = full_mindmap_test_dir("ipc-open-root-change");
    let old_root = dir.join("old-root");
    let new_root = dir.join("new-root");
    let old = old_root.join("old.md");
    let new = new_root.join("new.md");
    std::fs::create_dir_all(&old_root).unwrap();
    std::fs::create_dir_all(&new_root).unwrap();
    std::fs::write(&old, "# Old\n").unwrap();
    std::fs::write(&new, "# New\n").unwrap();

    let mut app = App::default();
    app.set_workspace(old_root.clone(), false);
    app.file = Some(old);
    app.source = "# Old\n".into();
    app.saved_source = app.source.clone();
    app.full_mindmap = Some(full_workspace_state(&old_root));
    app.show_hidden = true;

    let _ = app.update(Message::Ipc(
        crate::ipc::Request {
            id: 1,
            cmd: crate::ipc::Cmd::Open {
                file: new.to_string_lossy().into_owned(),
                line: Some(3),
                section: Some("New".into()),
                focus: crate::ipc::FocusBehavior::Suppress,
            },
        },
        std::sync::Arc::new(std::sync::Mutex::new(None)),
    ));

    let stale_request = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_workspace_load.clone())
        .expect("stale snapshot should delay the exit");
    assert!(stale_request.exit_after_refresh);
    assert!(app.pending_ipc_file_open.is_some());

    let _ = app.update(Message::FullMindmapSetRoot(new_root.clone()));
    let root_request = app
        .full_mindmap
        .as_ref()
        .and_then(|full| full.pending_workspace_load.clone())
        .expect("root change should replace the stale refresh");
    assert_eq!(root_request.path, new_root);
    assert!(root_request.exit_after_refresh);
    assert!(app.pending_ipc_file_open.is_some());

    let stale_snapshot = tree::build_workspace(&old_root, true).unwrap();
    let _ = app.update(Message::FullMindmapWorkspaceLoaded {
        request: stale_request,
        result: Ok((old_root.clone(), stale_snapshot)),
    });
    assert_eq!(
        app.full_mindmap
            .as_ref()
            .and_then(|full| full.pending_workspace_load.clone()),
        Some(root_request.clone())
    );

    let root_snapshot = tree::build_workspace(&new_root, true).unwrap();
    let _ = app.update(Message::FullMindmapWorkspaceLoaded {
        request: root_request,
        result: Ok((new_root.clone(), root_snapshot)),
    });

    assert!(app.full_mindmap.is_none());
    assert!(app.pending_ipc_file_open.is_none());
    assert_eq!(app.pending_nav.as_ref().and_then(|nav| nav.line), Some(3));

    let _ = app.update(Message::FileLoaded(Ok((new.clone(), "# New\n".into()))));
    assert_eq!(app.file.as_deref(), Some(new.as_path()));
    assert!(app.pending_nav.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_dirty_open_and_late_completion_preserve_current_editor() {
    let root = std::path::PathBuf::from("/workspace");
    let old = root.join("old.md");
    let new = root.join("new.md");
    let mut app = App::default();
    app.file = Some(old.clone());
    app.source = "saved".into();
    app.saved_source = "saved".into();
    app.full_mindmap = Some(full_workspace_state(&root));
    app.dirty = true;

    let _ = app.begin_full_mindmap_open(new.clone());
    assert!(app.full_mindmap.as_ref().unwrap().pending_open.is_none());
    assert_eq!(app.file.as_deref(), Some(old.as_path()));
    assert!(app.full_mindmap.is_some());

    app.dirty = false;
    let _ = app.begin_full_mindmap_open(new.clone());
    let request = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_open
        .clone()
        .unwrap();
    app.editor = Some(iced::widget::text_editor::Content::with_text(
        "unsaved editor",
    ));
    app.dirty = true;
    let _ = app.update(Message::FullMindmapFileLoaded {
        request,
        result: Ok((new, "new file".into())),
    });

    assert!(app.full_mindmap.is_some());
    assert!(app.full_mindmap.as_ref().unwrap().pending_open.is_none());
    assert_eq!(app.file.as_deref(), Some(old.as_path()));
    assert_eq!(
        app.editor.as_ref().map(|editor| editor.text()),
        Some("unsaved editor".into())
    );
    assert!(app.dirty);
}

#[test]
fn full_mindmap_ignores_stale_async_file_completion() {
    let root = std::path::PathBuf::from("/workspace");
    let mut app = App::default();
    app.full_mindmap = Some(full_workspace_state(&root));
    let first = root.join("first.md");
    let second = root.join("second.md");

    let _ = app.begin_full_mindmap_open(first.clone());
    let stale = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_open
        .clone()
        .unwrap();
    let _ = app.begin_full_mindmap_open(second.clone());
    let current = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_open
        .clone()
        .unwrap();

    let _ = app.update(Message::FullMindmapFileLoaded {
        request: stale,
        result: Err("stale failure".into()),
    });

    assert_eq!(
        app.full_mindmap
            .as_ref()
            .and_then(|full| full.pending_open.clone()),
        Some(current)
    );
    assert!(app.full_mindmap.as_ref().unwrap().load_error.is_none());
}

#[test]
fn full_mindmap_workspace_switch_cancels_inflight_file_open() {
    let dir = full_mindmap_test_dir("workspace-switch");
    let first_workspace = dir.join("first");
    let second_workspace = dir.join("second");
    let first_file = first_workspace.join("first.md");
    std::fs::create_dir_all(&first_workspace).unwrap();
    std::fs::create_dir_all(&second_workspace).unwrap();
    std::fs::write(&first_file, "# First\n").unwrap();

    let mut app = App::default();
    app.set_workspace(first_workspace.clone(), false);
    app.full_mindmap = Some(full_workspace_state(&first_workspace));
    let _ = app.begin_full_mindmap_open(first_file.clone());
    let request = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_open
        .clone()
        .unwrap();

    // This is the normal \"Open Folder…\" fallback while Full Mindmap is
    // active. It must supersede the old pending file read.
    let _ = app.update(Message::OpenWorkspace(second_workspace.clone()));
    assert_eq!(app.workspace.as_deref(), Some(first_workspace.as_path()));
    assert_eq!(
        app.full_mindmap
            .as_ref()
            .and_then(|full| full.pending_workspace_load.as_ref())
            .map(|pending| pending.path.as_path()),
        Some(second_workspace.as_path())
    );
    complete_full_mindmap_workspace_load(&mut app);
    assert_eq!(app.workspace.as_deref(), Some(second_workspace.as_path()));
    assert!(app.full_mindmap.as_ref().unwrap().pending_open.is_none());

    let _ = app.update(Message::FullMindmapFileLoaded {
        request,
        result: Ok((first_file, "# Stale\n".into())),
    });
    assert!(app.full_mindmap.is_some());
    assert!(app.file.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_ignores_stale_workspace_index_completion() {
    let dir = full_mindmap_test_dir("workspace-index-stale");
    let first = dir.join("first");
    let second = dir.join("second");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();

    let mut app = App::default();
    app.full_mindmap = Some(App::new_full_mindmap_state());
    let _ = app.begin_full_mindmap_workspace_load(first.clone(), false, None, false, false, false);
    let stale = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_workspace_load
        .clone()
        .unwrap();
    let _ = app.begin_full_mindmap_workspace_load(second.clone(), false, None, false, false, false);
    let current = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_workspace_load
        .clone()
        .unwrap();

    let _ = app.update(Message::FullMindmapWorkspaceLoaded {
        request: stale,
        result: Ok((first.clone(), tree::build_workspace(&first, false).unwrap())),
    });

    assert!(app.workspace.is_none());
    assert_eq!(
        app.full_mindmap.as_ref().unwrap().pending_workspace_load,
        Some(current)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_layout_cache_reuses_graph_and_node_allocations() {
    let dir = full_mindmap_test_dir("graph-cache-arc");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("readme.md"), "# Project\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    app.full_mindmap = Some(full_workspace_state(&dir));

    let first = app.full_mindmap_graph().unwrap();
    let second = app.full_mindmap_graph().unwrap();
    assert!(std::sync::Arc::ptr_eq(&first, &second));
    assert!(std::sync::Arc::ptr_eq(&first.nodes, &second.nodes));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn full_mindmap_request_ids_survive_exit_and_reentry() {
    let root = std::path::PathBuf::from("/workspace");
    let path = root.join("same.md");
    let mut app = App::default();
    app.full_mindmap = Some(full_workspace_state(&root));
    let _ = app.begin_full_mindmap_open(path.clone());
    let old_request = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_open
        .clone()
        .unwrap();

    let _ = app.update(Message::ExitFullMindmap);
    app.full_mindmap = Some(full_workspace_state(&root));
    let _ = app.begin_full_mindmap_open(path.clone());
    let current_request = app
        .full_mindmap
        .as_ref()
        .unwrap()
        .pending_open
        .clone()
        .unwrap();
    assert_ne!(old_request.id, current_request.id);

    let _ = app.update(Message::FullMindmapFileLoaded {
        request: old_request.clone(),
        result: Err("stale failure".into()),
    });
    assert_eq!(
        app.full_mindmap
            .as_ref()
            .and_then(|full| full.pending_open.clone()),
        Some(current_request.clone())
    );
    assert!(app.full_mindmap.as_ref().unwrap().load_error.is_none());

    let _ = app.update(Message::FullMindmapFileLoaded {
        request: old_request,
        result: Ok((path, "# Stale\n".into())),
    });
    assert!(app.full_mindmap.is_some());
    assert_eq!(
        app.full_mindmap
            .as_ref()
            .and_then(|full| full.pending_open.clone()),
        Some(current_request)
    );
    assert!(app.file.is_none());
}

#[test]
fn full_mindmap_exit_restores_underlying_navigation_state_unchanged() {
    let mut app = App::default();
    app.file = Some(std::path::PathBuf::from("note.md"));
    app.source = "draft".into();
    app.saved_source = "saved".into();
    app.view_mode = ViewMode::Raw;
    app.editor = Some(iced::widget::text_editor::Content::with_text("draft"));
    app.sidebar_open = true;
    app.sidebar_tab = SidebarTab::Outline;
    app.search_open = true;
    app.show_footer = false;
    app.full_mindmap = Some(full_workspace_state(std::path::Path::new("/workspace")));

    let _ = app.update(Message::ExitFullMindmap);

    assert!(app.full_mindmap.is_none());
    assert_eq!(app.view_mode, ViewMode::Raw);
    assert_eq!(
        app.editor.as_ref().map(|editor| editor.text()),
        Some("draft".into())
    );
    assert!(app.sidebar_open);
    assert_eq!(app.sidebar_tab, SidebarTab::Outline);
    assert!(app.search_open);
    assert!(!app.show_footer);
}

#[test]
fn native_pinch_is_scoped_to_the_visible_mindmap_surface() {
    let mut app = App::default();

    let _ = app.update(Message::MindmapNativePinch(0.25));
    assert_eq!(app.mindmap_native_pinch_log, 0.0);

    app.view_mode = ViewMode::Mindmap;
    let _ = app.update(Message::MindmapNativePinch(0.25));
    assert_eq!(app.mindmap_native_pinch_log, 0.25);

    app.search_open = true;
    let _ = app.update(Message::MindmapNativePinch(0.5));
    assert_eq!(app.mindmap_native_pinch_log, 0.25);

    app.search_open = false;
    app.overlay = Overlay::Shortcuts;
    let _ = app.update(Message::MindmapNativePinch(0.5));
    assert_eq!(app.mindmap_native_pinch_log, 0.25);

    app.overlay = Overlay::None;
    app.full_mindmap = Some(App::new_full_mindmap_state());
    let _ = app.update(Message::MindmapNativePinch(0.75));
    assert_eq!(app.full_mindmap_native_pinch_log, 0.75);
    assert_eq!(app.mindmap_native_pinch_log, 0.25);
}

#[test]
fn document_text_keeps_valid_utf8_and_repairs_invalid_bytes() {
    assert_eq!(document_text(b"# Title".to_vec()), "# Title");
    assert_eq!(document_text(vec![b'a', 0xff, b'b']), "a\u{fffd}b");
}

#[test]
fn oversized_documents_are_refused_before_reading() {
    let path = std::env::temp_dir().join(format!("rmdv-oversized-{}.md", std::process::id()));
    // A sparse file reports the size without writing the bytes.
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(MAX_DOCUMENT_BYTES + 1).unwrap();
    drop(file);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let error = runtime
        .block_on(read_document_bytes(&path))
        .expect_err("a file past the cap must not be read");
    assert!(error.contains("too large"), "{error}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn file_finder_results_follow_query_and_workspace_reindex() {
    let dir = std::env::temp_dir().join(format!("rmdv-file-finder-memo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("alpha.md"), "# a\n").unwrap();
    std::fs::write(dir.join("beta.md"), "# b\n").unwrap();

    let mut app = App::default();
    app.set_workspace(dir.clone(), false);
    let names = |app: &App| -> Vec<String> {
        app.filtered_files()
            .into_iter()
            .map(|(_, rel, _)| rel)
            .collect()
    };

    app.overlay_query = "bet".into();
    assert_eq!(names(&app), vec!["beta.md"]);
    assert_eq!(
        names(&app),
        vec!["beta.md"],
        "a repeated call reuses the answer"
    );
    app.overlay_query = "alp".into();
    assert_eq!(names(&app), vec!["alpha.md"]);

    // Same root, same query, same file count: only the reindex differs.
    app.overlay_query = "bet".into();
    assert_eq!(names(&app), vec!["beta.md"]);
    std::fs::rename(dir.join("beta.md"), dir.join("gamma.md")).unwrap();
    app.set_workspace(dir.clone(), false);
    assert!(
        names(&app).is_empty(),
        "a reindexed workspace must not reuse stale results"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn chosen_theme_is_saved_and_restored_on_next_launch() {
    let mut app = App::default();
    let isolated = app.quick_slots_persistence_path.clone().unwrap();
    let _ = app.update(Message::SetTheme(ThemePreset::Nord));
    let saved = crate::prefs::load_from(&isolated);
    assert_eq!(
        saved.theme.as_deref(),
        Some(theme::preset_slug(ThemePreset::Nord))
    );

    let mut next = App::default();
    next.prefs = saved;
    next.restore_saved_theme();
    assert_eq!(next.theme_id, theme::ThemeId::Preset(ThemePreset::Nord));
    assert_eq!(next.theme_preset, ThemePreset::Nord);
    assert_eq!(next.palette, theme::palette_for(ThemePreset::Nord));
    let _ = std::fs::remove_file(isolated);
}

#[test]
fn chosen_custom_theme_is_saved_and_restored_on_next_launch() {
    let custom = crate::theme_load::bundled()
        .first()
        .expect("rmdv bundles at least one custom theme")
        .clone();
    let mut app = App::default();
    app.custom_themes = crate::theme_load::bundled().clone();
    let isolated = app.quick_slots_persistence_path.clone().unwrap();
    let _ = app.update(Message::SetCustomTheme(custom.slug.clone()));
    let saved = crate::prefs::load_from(&isolated);
    assert_eq!(saved.theme.as_deref(), Some(custom.slug.as_str()));

    let mut next = App::default();
    next.custom_themes = crate::theme_load::bundled().clone();
    next.prefs = saved;
    next.restore_saved_theme();
    assert_eq!(next.theme_id, theme::ThemeId::Custom(custom.slug.clone()));
    assert_eq!(next.palette, custom.palette);
    let _ = std::fs::remove_file(isolated);
}

#[test]
fn cycled_theme_is_saved() {
    let mut app = App::default();
    let isolated = app.quick_slots_persistence_path.clone().unwrap();
    let _ = app.update(Message::ToggleTheme);
    let saved = crate::prefs::load_from(&isolated);
    assert_eq!(saved.theme, Some(app.theme_id.slug()));
    let _ = std::fs::remove_file(isolated);
}

#[test]
fn unknown_saved_theme_keeps_the_system_default() {
    let mut app = App::default();
    let before = (app.theme_id.clone(), app.palette);
    app.prefs.theme = Some("deleted-custom-theme".into());
    app.restore_saved_theme();
    assert_eq!((app.theme_id.clone(), app.palette), before);

    // Older prefs files have no `theme` key and must still load.
    let legacy: crate::prefs::Prefs = serde_json::from_str(r#"{"show_footer":false}"#).unwrap();
    assert_eq!(legacy.theme, None);
    assert!(!legacy.show_footer);
}

fn search_test_app(source: String) -> App {
    let mut app = App::default();
    app.ast = crate::parser::parse(&source).0;
    app.source = source;
    app.search_open = true;
    app
}

#[test]
fn small_document_search_updates_on_every_keystroke() {
    let mut app = search_test_app("alpha beta\n\nalpha\n".into());
    let _ = app.update(Message::QueryChanged("alpha".into()));
    assert!(!app.search_pending);
    assert_eq!(app.matches.len(), 2);
}

#[test]
fn large_document_search_waits_for_the_latest_keystroke() {
    let mut source = String::new();
    let mut blocks = 0;
    while source.len() <= SEARCH_DEBOUNCE_MIN_BYTES {
        source.push_str("alpha beta gamma gamma\n\n");
        blocks += 1;
    }
    let mut app = search_test_app(source);
    let _ = app.update(Message::QueryChanged("al".into()));
    let stale = app.search_generation;
    let _ = app.update(Message::QueryChanged("beta".into()));
    assert!(app.search_pending, "large documents debounce the search");
    assert!(app.matches.is_empty(), "no search ran yet");

    let _ = app.update(Message::SearchDebounced(stale));
    assert!(app.matches.is_empty(), "a superseded timer must not search");

    let _ = app.update(Message::SearchDebounced(app.search_generation));
    assert!(!app.search_pending);
    assert_eq!(app.matches.len(), blocks);
    assert_eq!(
        app.matches.get(1).map(|m| (m.block, m.in_block)),
        Some((1, 0))
    );

    // Enter before the timer fires navigates the new query's results.
    let _ = app.update(Message::QueryChanged("gamma".into()));
    assert!(app.search_pending);
    assert_eq!(app.matches.len(), blocks, "still the previous query's hits");
    let _ = app.update(Message::NextMatch);
    assert!(!app.search_pending);
    assert_eq!(app.matches.len(), 2 * blocks);
    assert_eq!(app.match_idx, 1);
    assert_eq!(
        app.matches.get(1).map(|m| (m.block, m.in_block)),
        Some((0, 1))
    );

    // Clearing the query needs no search, so it applies at once.
    let _ = app.update(Message::QueryChanged(String::new()));
    assert!(!app.search_pending);
    assert!(app.matches.is_empty());

    // Closing search drops a pending run.
    let _ = app.update(Message::QueryChanged("alpha".into()));
    let _ = app.update(Message::ToggleSearch);
    assert!(!app.search_pending);
}

#[test]
fn debounced_search_with_find_bar_hidden_updates_results_only() {
    let mut source = String::new();
    while source.len() <= SEARCH_DEBOUNCE_MIN_BYTES {
        source.push_str("alpha beta\n\n");
    }
    let mut app = search_test_app(source);
    let _ = app.update(Message::QueryChanged("beta".into()));
    // Zen edit mode hides the find bar but keeps the query.
    app.search_open = false;
    let _ = app.update(Message::SearchDebounced(app.search_generation));
    assert!(!app.search_pending);
    assert!(!app.matches.is_empty());
}

#[test]
fn sidebar_rows_near_builds_only_the_overscanned_viewport() {
    // 5000 rows × 26 px, 800 px tall viewport, 600 px overscan each side.
    let at_top = sidebar_rows_near(0.0, 800.0, 5000);
    assert_eq!(at_top.start, 0);
    assert!(at_top.end < 60, "{at_top:?}");

    let middle = sidebar_rows_near(26.0 * 2000.0 + 4.0, 800.0, 5000);
    assert!(
        middle.start <= 2000 - 23 && middle.start >= 2000 - 24,
        "{middle:?}"
    );
    assert!(
        middle.end >= 2000 + 30 + 23 && middle.end <= 2000 + 30 + 25,
        "{middle:?}"
    );

    let bottom = sidebar_rows_near(26.0 * 5000.0 - 800.0 + 8.0, 800.0, 5000);
    assert_eq!(bottom.end, 5000);

    // A stale offset from a longer list still shows the end of a short one.
    let stale = sidebar_rows_near(26.0 * 4000.0, 800.0, 100);
    assert_eq!(stale.end, 100);
    assert!(stale.start <= 100 - 31, "{stale:?}");
    let stale_long = sidebar_rows_near(26.0 * 9000.0, 800.0, 5000);
    assert_eq!(stale_long.end, 5000);
    assert!(stale_long.len() >= 30);

    // Short lists build every row.
    assert_eq!(sidebar_rows_near(0.0, 800.0, 10), 0..10);
    assert_eq!(sidebar_row_window(None, 5000), [0..5000, 5000..5000]);

    // The first screenful is always built next to the viewport band, so a
    // scrollable re-created at the top never shows blank space.
    let [head, near] = sidebar_row_bands(26.0 * 2000.0, 800.0, 5000);
    assert_eq!(head, sidebar_rows_near(0.0, 800.0, 5000));
    assert_eq!(near, sidebar_rows_near(26.0 * 2000.0, 800.0, 5000));
    let [merged, empty] = sidebar_row_bands(26.0 * 20.0, 800.0, 5000);
    assert_eq!(merged.start, 0);
    assert_eq!(merged.end, sidebar_rows_near(26.0 * 20.0, 800.0, 5000).end);
    assert!(empty.is_empty());
}

#[test]
fn typing_reuses_the_post_edit_text_as_the_next_undo_snapshot() {
    use iced::widget::text_editor::{Action, Content, Edit, Motion};
    let text = |app: &App| app.editor.as_ref().unwrap().text();
    let mut app = App::default();
    app.saved_source = "ab".into();
    app.editor = Some(Content::with_text("ab"));

    let _ = app.update(Message::EditorAction(Action::Move(Motion::DocumentEnd)));
    let _ = app.update(Message::EditorAction(Action::Edit(Edit::Insert('c'))));
    assert!(app.dirty);
    assert_eq!(app.editor_text.as_deref(), Some(text(&app).as_str()));

    // Cursor moves between edits do not change the text, so the cache stays.
    let _ = app.update(Message::EditorAction(Action::Move(Motion::Left)));
    let _ = app.update(Message::EditorAction(Action::Move(Motion::Right)));
    assert!(app.editor_text.is_some());
    let _ = app.update(Message::EditorAction(Action::Edit(Edit::Insert('d'))));
    assert_eq!(text(&app), "abcd");

    let _ = app.update(Message::EditorUndo);
    assert_eq!(text(&app), "abc");
    assert!(app.editor_text.is_none());
    let _ = app.update(Message::EditorUndo);
    assert_eq!(text(&app), "ab");
    assert!(!app.dirty);
    let _ = app.update(Message::EditorRedo);
    assert_eq!(text(&app), "abc");
    assert!(app.dirty);

    // The first edit after a redo reads the replaced editor, not a stale cache.
    let _ = app.update(Message::EditorAction(Action::Move(Motion::DocumentEnd)));
    let _ = app.update(Message::EditorAction(Action::Edit(Edit::Insert('x'))));
    assert_eq!(text(&app), "abcx");
    let _ = app.update(Message::EditorUndo);
    assert_eq!(text(&app), "abc");
}

#[test]
fn local_images_load_through_a_task_into_the_budgeted_cache() {
    let dir = std::env::temp_dir().join(format!("rmdv-local-image-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let pic = dir.join("pic.png");
    let key = pic.to_string_lossy().into_owned();
    std::fs::write(&pic, [7u8; 512]).unwrap();

    let mut app = App::default();
    app.file = Some(dir.join("doc.md"));
    app.source = "# T\n\n![pic](pic.png)\n".into();
    app.reparse_source();
    let tasks = app.prime_document_images();
    assert_eq!(tasks.len(), 1);
    assert!(matches!(
        app.image_cache.get(&key),
        Some(ImageState::Loading)
    ));
    // Already loading: no second read.
    assert!(app.prime_document_images().is_empty());

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (read_key, result) = runtime.block_on(read_local_image(key.clone()));
    assert_eq!(read_key, key);
    let bytes = result.unwrap();
    assert_eq!(bytes.len(), 512);
    let before = app.image_cache.cost_bytes();
    let _ = app.update(Message::ImageFetched(key.clone(), Ok(bytes)));
    assert!(matches!(
        app.image_cache.get(&key),
        Some(ImageState::Loaded(_))
    ));
    assert_eq!(app.image_cache.cost_bytes(), before + 512);
    assert!(app.prime_document_images().is_empty());

    // A missing file fails, then is retried when the document loads again.
    let (_, missing) = runtime.block_on(read_local_image(
        dir.join("absent.png").to_string_lossy().into_owned(),
    ));
    assert!(missing.is_err());
    let _ = app.update(Message::ImageFetched(key.clone(), Err("missing".into())));
    assert!(matches!(
        app.image_cache.get(&key),
        Some(ImageState::Failed)
    ));
    assert_eq!(app.prime_document_images().len(), 1);
    assert!(matches!(
        app.image_cache.get(&key),
        Some(ImageState::Loading)
    ));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn window_event_burst_shares_one_mode_refresh() {
    let mut app = App::default();
    // An opaque launch: no blur view to sync.
    app.glass_capable = false;
    let id = iced::window::Id::unique();
    // Before: every Moved/Resized/Focused event queued an immediate mode query
    // plus two settle timers (3 tasks). Now one burst queues the immediate
    // query and a single timer.
    let units: usize = (0..60)
        .map(|_| app.update(Message::RefreshWindowMode(id)).units())
        .sum();
    assert_eq!(units, 2);
    assert!(app.window_mode_settle_armed);

    // The timer fires while events are still arriving: re-arm, no sample.
    assert_eq!(app.update(Message::RefreshWindowModeSettled(id)).units(), 1);
    assert!(app.window_mode_settle_armed);

    // After the final settle delay the timer samples once and disarms.
    app.window_mode_last_trigger =
        std::time::Instant::now().checked_sub(std::time::Duration::from_millis(700));
    assert_eq!(app.update(Message::RefreshWindowModeSettled(id)).units(), 1);
    assert!(!app.window_mode_settle_armed);

    // The next event starts a new burst with its own immediate sample.
    assert_eq!(app.update(Message::RefreshWindowMode(id)).units(), 2);
}

#[test]
fn window_mode_settle_samples_at_250_and_600_ms_after_the_last_event() {
    use std::time::Duration;
    let ms = Duration::from_millis;
    assert_eq!(window_mode_settle_step(ms(0)), (false, Some(ms(250))));
    assert_eq!(window_mode_settle_step(ms(100)), (false, Some(ms(150))));
    assert_eq!(window_mode_settle_step(ms(250)), (true, Some(ms(350))));
    assert_eq!(window_mode_settle_step(ms(400)), (true, Some(ms(200))));
    assert_eq!(window_mode_settle_step(ms(600)), (true, None));
    assert_eq!(window_mode_settle_step(ms(5000)), (true, None));
}

#[test]
fn dirty_document_can_be_reopened_but_blocks_other_files() {
    let dir = std::env::temp_dir().join(format!("rmdv-dirty-reopen-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let a = dir.join("a.md");
    let b = dir.join("b.md");
    std::fs::write(&a, "a").unwrap();
    std::fs::write(&b, "b").unwrap();

    let mut app = App::default();
    app.file = Some(a.clone());
    app.source = "a edited".into();
    app.saved_source = "a".into();
    app.dirty = true;
    app.vault_open = true;

    // Returning to the document being edited keeps the edits and leaves the
    // vault page instead of warning about them.
    let _ = app.update(Message::Open(a.clone()));
    assert_eq!(app.file.as_deref(), Some(a.as_path()));
    assert_eq!(app.source, "a edited");
    assert!(app.dirty);
    assert!(!app.vault_open);
    assert!(app.toast.is_none());

    // Another file stays blocked, and the toast names the way out.
    let _ = app.update(Message::Open(b));
    assert_eq!(app.file.as_deref(), Some(a.as_path()));
    let toast = app
        .toast
        .as_ref()
        .map(|t| t.text.clone())
        .unwrap_or_default();
    assert!(toast.contains("unsaved edits in a.md"), "{toast}");
    assert!(toast.contains("\u{2318}S"), "{toast}");
}

#[test]
fn undo_and_redo_keep_the_cursor_at_the_restored_edit() {
    use iced::widget::text_editor::{Action, Content, Cursor, Edit, Position};
    let doc: String = (0..200).map(|i| format!("line {i}\n")).collect();
    let mut app = App::default();
    app.saved_source = doc.clone();
    app.editor = Some(Content::with_text(&doc));
    let ed = |app: &App| app.editor.as_ref().unwrap().text();
    let line = |app: &App| app.editor.as_ref().unwrap().cursor().position.line;

    app.editor.as_mut().unwrap().move_to(Cursor {
        position: Position {
            line: 150,
            column: 4,
        },
        selection: None,
    });
    for c in "中文!".chars() {
        let _ = app.update(Message::EditorAction(Action::Edit(Edit::Insert(c))));
    }
    assert!(ed(&app).contains("line中文! 150"));

    let _ = app.update(Message::EditorUndo);
    assert!(ed(&app).contains("line中文 150"));
    assert_eq!(line(&app), 150);
    let _ = app.update(Message::EditorUndo);
    let _ = app.update(Message::EditorUndo);
    assert_eq!(ed(&app), doc);
    assert_eq!(line(&app), 150);
    assert!(!app.dirty);

    let _ = app.update(Message::EditorRedo);
    assert!(ed(&app).contains("line中 150"));
    assert_eq!(line(&app), 150);
    assert!(app.dirty);
}

#[test]
fn restore_editor_text_splices_inserts_deletes_and_replacements() {
    use iced::widget::text_editor::Content;
    for (from, to) in [
        ("abc\ndef\n", "abc\nXdef\n"),
        ("abc\nXdef\n", "abc\ndef\n"),
        ("héllo wörld", "héllo wörld!"),
        ("一二三四", "一二X三四"),
        ("一二X三四", "一二三四"),
        ("aaa\nbbb\nccc", "aaa\nccc"),
        ("same", "same"),
        ("", "new\ntext"),
        ("old\ntext", ""),
        ("crlf\r\nline", "crlf\r\nline2"),
    ] {
        let mut ed = Content::with_text(from);
        restore_editor_text(&mut ed, from, to);
        assert_eq!(ed.text(), to, "{from:?} -> {to:?}");
    }
    assert_eq!(changed_span("一二三", "一X二三"), (3, 3, 4));
    assert_eq!(changed_span("aa", "aaa"), (2, 2, 3));
}

#[test]
fn expanding_a_budget_stopped_folder_scans_it_once_and_fills_the_sidebar() {
    let dir = full_mindmap_test_dir("lazy-folder-scan");
    let early = dir.join("a");
    let late = dir.join("b");
    let inner = late.join("inner");
    std::fs::create_dir_all(&early).unwrap();
    std::fs::create_dir_all(&inner).unwrap();
    for name in ["1.md", "2.md", "3.md"] {
        std::fs::write(early.join(name), "# Early\n").unwrap();
    }
    let note = late.join("note.md");
    std::fs::write(&note, "# Note\n").unwrap();
    std::fs::write(inner.join("deep.md"), "# Deep\n").unwrap();

    let mut app = App::default();
    let snapshot = tree::build_workspace_with_limits(&dir, false, 12, 8, 100, 5).unwrap();
    app.apply_workspace_snapshot(dir.clone(), snapshot, true);
    let epoch = app.workspace_epoch;
    assert!(app.workspace_sidebar_files.files_for(&late).is_empty());

    // Expanding a fully scanned folder requests nothing.
    let task = app.update(Message::TreeToggle(early.clone()));
    assert_eq!(task.units(), 0);
    assert!(app.sidebar_folder_scans.is_empty());

    let task = app.update(Message::TreeToggle(late.clone()));
    assert_eq!(task.units(), 1);
    assert!(app.sidebar_folder_scans.contains(&late));

    // A result from a replaced snapshot is dropped.
    let scanned = tree::build_workspace(&late, false).unwrap();
    let _ = app.update(Message::SidebarFolderScanned {
        epoch: epoch.wrapping_sub(1),
        folder: late.clone(),
        result: Ok((late.clone(), scanned.clone())),
    });
    assert!(app.workspace_sidebar_files.files_for(&late).is_empty());

    let task = app.update(Message::SidebarFolderScanned {
        epoch,
        folder: late.clone(),
        result: Ok((late.clone(), scanned)),
    });
    assert_eq!(task.units(), 0);
    assert_eq!(app.workspace_sidebar_files.files_for(&late), [note.clone()]);
    assert!(app.workspace_files.contains(&note));
    let tree = app.workspace_tree.as_ref().unwrap();
    assert!(tree::find_folder(tree, &inner).is_some());

    // Re-expanding the same folder in this epoch does not rescan it.
    let _ = app.update(Message::TreeToggle(late.clone()));
    let task = app.update(Message::TreeToggle(late.clone()));
    assert_eq!(task.units(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn soft_syntax_option_follows_theme_changes_and_turns_off_cleanly() {
    let mut app = App::default();
    let isolated = app.quick_slots_persistence_path.clone().unwrap();
    let _ = app.update(Message::SetTheme(ThemePreset::Nord));
    let nord = theme::palette_for(ThemePreset::Nord);
    assert_eq!(app.palette, nord);

    let _ = app.update(Message::ToggleSoftSyntax);
    assert!(crate::prefs::load_from(&isolated).soft_syntax);
    assert_eq!(app.palette.syntax.variable, nord.fg);
    assert_ne!(app.palette.syntax.keyword, nord.syntax.keyword);

    let _ = app.update(Message::SetTheme(ThemePreset::OneLight));
    let one_light = theme::palette_for(ThemePreset::OneLight);
    assert_eq!(app.palette, one_light.with_soft_syntax(true));

    let _ = app.update(Message::ToggleSoftSyntax);
    assert_eq!(app.palette, one_light);
}

#[test]
fn window_glass_modes_paint_one_tint_per_panel_and_sync_the_blur_view() {
    let mut app = App::default();
    let isolated = app.quick_slots_persistence_path.clone().unwrap();
    let _ = app.update(Message::SetTheme(ThemePreset::OneDark));
    let pal = theme::palette_for(ThemePreset::OneDark);
    // Start from an opaque launch with glass off.
    app.glass_capable = false;
    let _ = app.update(Message::SetGlass(Glass::Off));
    let _ = app.update(Message::SetGlassOpacity(0.8));

    // Off: everything opaque, nothing to sync.
    assert_eq!(app.glass(), Glass::Off);
    assert_eq!(app.chrome_ground(), pal.sidebar);
    assert_eq!(app.reader_ground(), pal.bg);
    assert_eq!(app.reader_fill(), pal.bg);

    // A saved mode stays off when the window was not created transparent.
    let _ = app.update(Message::CycleGlass);
    assert_eq!(crate::prefs::load_from(&isolated).glass, Glass::Sidebar);
    assert_eq!(app.glass(), Glass::Off);

    app.glass_capable = true;
    app.glass_window = Some(iced::window::Id::unique());

    // Sidebar: the ground carries the tint, the sidebar paints none, the
    // reader stays opaque.
    let _ = app.update(Message::Noop);
    assert_eq!(app.glass_applied, Some((true, true)));
    assert_eq!(app.chrome_palette().sidebar, Color::TRANSPARENT);
    assert_eq!(app.chrome_ground().a, 0.8);
    assert_eq!(app.reader_ground(), pal.bg);
    assert_eq!(app.view_palette(), pal);

    // Whole window: each panel paints its own tint over a clear ground, and
    // nested reader views paint nothing.
    let _ = app.update(Message::CycleGlass);
    assert_eq!(app.glass(), Glass::Window);
    assert_eq!(app.chrome_ground(), Color::TRANSPARENT);
    assert_eq!(app.chrome_palette().sidebar.a, 0.8);
    assert_eq!(app.reader_ground().a, 0.8);
    assert_eq!(app.reader_fill(), Color::TRANSPARENT);
    // Dim text moves toward the body color as the tint thins.
    let _ = app.update(Message::CycleGlassOpacity);
    let _ = app.update(Message::CycleGlassOpacity);
    assert_eq!(crate::prefs::load_from(&isolated).glass_opacity, 0.6);
    let view = app.view_palette();
    assert!(theme::contrast_ratio(view.muted, pal.bg) > theme::contrast_ratio(pal.muted, pal.bg));

    // A light theme switches the blur view to the light material.
    let _ = app.update(Message::SetTheme(ThemePreset::OneLight));
    assert_eq!(app.glass_applied, Some((true, false)));

    // Off hides the blur view and restores opaque fills.
    let _ = app.update(Message::CycleGlass);
    assert_eq!(app.glass(), Glass::Off);
    assert_eq!(app.glass_applied, Some((false, false)));
    assert_eq!(
        app.reader_fill(),
        theme::palette_for(ThemePreset::OneLight).bg
    );
}

fn settings_row_index(row: SettingsRow) -> usize {
    SettingsRow::visible()
        .iter()
        .position(|r| *r == row)
        .expect("row is visible on this platform")
}

#[test]
fn settings_page_draws_over_the_current_view_and_closes_back_to_it() {
    let mut app = App::default();
    app.view_mode = ViewMode::Mindmap;
    app.vault_open = true;
    app.overlay = Overlay::Command;

    let _ = app.update(Message::ToggleSettings);
    assert!(app.settings_open);
    assert_eq!(app.overlay, Overlay::None);
    assert_eq!(app.settings_cursor, 0);
    let _ = app.view();
    // The view under the page is untouched.
    assert_eq!(app.view_mode, ViewMode::Mindmap);
    assert!(app.vault_open);

    let _ = app.update(Message::ToggleSettings);
    assert!(!app.settings_open);
    let _ = app.update(Message::OpenSettings);
    let _ = app.update(Message::CloseSettings);
    assert!(!app.settings_open);

    // Opening a file or another view closes the page so the result shows.
    let _ = app.update(Message::OpenSettings);
    assert!(Message::FileLoaded(Ok((PathBuf::from("/tmp/a.md"), String::new()))).leaves_settings());
    let _ = app.update(Message::OpenVaultSearch);
    assert!(!app.settings_open);
    assert!(!Message::FileLoaded(Err("x".into())).leaves_settings());
    assert!(!Message::ToggleFooter.leaves_settings());
}

#[test]
fn settings_cursor_moves_within_the_visible_rows() {
    let mut app = App::default();
    let _ = app.update(Message::OpenSettings);
    let last = SettingsRow::visible().len() - 1;
    let _ = app.update(Message::SettingsMove(-1));
    assert_eq!(app.settings_cursor, 0);
    let _ = app.update(Message::SettingsMove(1));
    assert_eq!(app.settings_cursor, 1);
    for _ in 0..40 {
        let _ = app.update(Message::SettingsMove(1));
    }
    assert_eq!(app.settings_cursor, last);
    let _ = app.update(Message::SettingsCursor(999));
    assert_eq!(app.settings_cursor, last);
    assert_eq!(app.settings_row(), SettingsRow::PrefsFile);
    assert_eq!(
        SettingsRow::visible().contains(&SettingsRow::Glass),
        cfg!(target_os = "macos")
    );
}

#[test]
fn settings_rows_change_the_same_state_as_their_commands_and_persist() {
    let mut app = App::default();
    let isolated = app.quick_slots_persistence_path.clone().unwrap();
    let _ = app.update(Message::SetTheme(ThemePreset::OneDark));
    let _ = app.update(Message::OpenSettings);
    let saved = |path: &Path| crate::prefs::load_from(path);

    let at = |app: &mut App, row: SettingsRow, msg: Message| {
        let _ = app.update(Message::SettingsCursor(settings_row_index(row)));
        let _ = app.update(msg);
    };

    // Switches: Space toggles; → sets on, ← sets off, and a repeat is a no-op.
    at(&mut app, SettingsRow::SoftSyntax, Message::SettingsActivate);
    assert!(app.prefs.soft_syntax && saved(&isolated).soft_syntax);
    at(&mut app, SettingsRow::SoftSyntax, Message::SettingsStep(1));
    assert!(app.prefs.soft_syntax);
    at(&mut app, SettingsRow::SoftSyntax, Message::SettingsStep(-1));
    assert!(!app.prefs.soft_syntax && !saved(&isolated).soft_syntax);

    at(&mut app, SettingsRow::Footer, Message::SettingsActivate);
    assert!(!app.show_footer && !saved(&isolated).show_footer);
    at(&mut app, SettingsRow::HiddenFiles, Message::SettingsStep(1));
    assert!(app.show_hidden && saved(&isolated).show_hidden);
    at(
        &mut app,
        SettingsRow::MindmapAutocenter,
        Message::SettingsStep(-1),
    );
    assert!(!app.mindmap_autocenter && !saved(&isolated).mindmap_autocenter);
    at(&mut app, SettingsRow::AutoFocus, Message::SettingsActivate);
    assert!(app.prefs.auto_focus_on_nav && saved(&isolated).auto_focus_on_nav);

    // Font size: → zooms in like ⌘+, Space resets like ⌘0.
    at(&mut app, SettingsRow::FontSize, Message::SettingsStep(1));
    assert!((app.font_scale - 1.1).abs() < 1e-6);
    assert!((saved(&isolated).font_scale - 1.1).abs() < 1e-6);
    at(&mut app, SettingsRow::FontSize, Message::SettingsActivate);
    assert_eq!(app.font_scale, 1.0);
    assert_eq!(saved(&isolated).font_scale, 1.0);

    // Theme: → switches to the next card and saves it; ← stops at the first.
    let start = app
        .theme_entries()
        .iter()
        .position(|e| e.matches_current(&app.theme_id))
        .unwrap();
    at(&mut app, SettingsRow::Theme, Message::SettingsStep(1));
    assert!(app.theme_entries()[start + 1].matches_current(&app.theme_id));
    assert_eq!(saved(&isolated).theme, Some(app.theme_id.slug()));
    for _ in 0..start + 3 {
        at(&mut app, SettingsRow::Theme, Message::SettingsStep(-1));
    }
    assert!(app.theme_entries()[0].matches_current(&app.theme_id));

    // Glass (macOS rows): ← / → walk the modes without wrapping; opacity
    // steps by 5 % and only while glass is on.
    app.glass_capable = false;
    let _ = app.update(Message::SetGlass(Glass::Off));
    let _ = app.update(Message::SetGlassOpacity(0.8));
    if cfg!(target_os = "macos") {
        at(
            &mut app,
            SettingsRow::GlassOpacity,
            Message::SettingsStep(-1),
        );
        assert_eq!(app.prefs.glass_opacity, 0.8);
        at(&mut app, SettingsRow::Glass, Message::SettingsStep(1));
        assert_eq!(saved(&isolated).glass, Glass::Sidebar);
        at(&mut app, SettingsRow::Glass, Message::SettingsStep(1));
        at(&mut app, SettingsRow::Glass, Message::SettingsStep(1));
        assert_eq!(app.prefs.glass, Glass::Window);
        // This launch was not transparent, so the page offers a restart.
        assert!(app.glass_needs_restart());
        at(
            &mut app,
            SettingsRow::GlassOpacity,
            Message::SettingsStep(-1),
        );
        assert!((saved(&isolated).glass_opacity - 0.75).abs() < 1e-6);
        at(&mut app, SettingsRow::Glass, Message::SettingsActivate);
        assert_eq!(app.prefs.glass, Glass::Off);
        assert!(!app.glass_needs_restart());
    }
    let _ = app.update(Message::SetGlassOpacity(0.2));
    assert_eq!(app.prefs.glass_opacity, 0.6);
    let _ = app.view();
}

#[test]
fn settings_key_is_command_comma_only() {
    use iced::keyboard::{Key, Modifiers};
    let comma = Key::Character(",".into());
    assert!(is_settings_key(&comma, Modifiers::COMMAND));
    assert!(is_settings_key(&comma, Modifiers::CTRL));
    assert!(!is_settings_key(&comma, Modifiers::empty()));
    assert!(!is_settings_key(
        &comma,
        Modifiers::COMMAND | Modifiers::SHIFT
    ));
    assert!(!is_settings_key(
        &Key::Character(".".into()),
        Modifiers::COMMAND
    ));
}

#[test]
fn restart_relaunches_the_bundle_or_the_executable_with_the_open_path() {
    use std::ffi::OsString;
    let file = Path::new("/notes/a.md");
    let bundled = Path::new("/Applications/rmdv.app/Contents/MacOS/rmdv");
    let argv = relaunch_argv(bundled, Some(file));
    if cfg!(target_os = "macos") {
        assert_eq!(
            argv,
            ["open", "/Applications/rmdv.app", "--args", "/notes/a.md"]
                .map(OsString::from)
                .to_vec()
        );
        assert_eq!(
            relaunch_argv(bundled, None),
            ["open", "/Applications/rmdv.app"]
                .map(OsString::from)
                .to_vec()
        );
    }
    let bare = Path::new("/usr/local/bin/rmdv");
    assert_eq!(
        relaunch_argv(bare, Some(file)),
        ["/usr/local/bin/rmdv", "/notes/a.md"]
            .map(OsString::from)
            .to_vec()
    );
    assert_eq!(
        relaunch_argv(bare, None),
        vec![OsString::from("/usr/local/bin/rmdv")]
    );
}

#[test]
fn non_latin_input_source_shortcuts_resolve_to_physical_latin_key() {
    use iced::keyboard::key::{Code, Physical};
    use iced::keyboard::{Key, Modifiers};

    let cases: [(&str, Code, Modifiers, &str); 7] = [
        ("ㄖ", Code::KeyB, Modifiers::COMMAND, "b"),
        ("ㄎ", Code::KeyF, Modifiers::COMMAND | Modifiers::SHIFT, "F"),
        ("ㄩ", Code::KeyM, Modifiers::COMMAND, "m"),
        ("，", Code::Comma, Modifiers::COMMAND, ","),
        (
            "。",
            Code::Period,
            Modifiers::COMMAND | Modifiers::SHIFT,
            ".",
        ),
        ("ㄨ", Code::KeyJ, Modifiers::NONE, "j"),
        ("ж", Code::KeyG, Modifiers::NONE, "g"),
    ];
    for (logical, code, mods, expected) in cases {
        let key = latin_shortcut_key(Key::Character(logical.into()), Physical::Code(code), mods);
        assert_eq!(key, Key::Character(expected.into()), "{logical} {code:?}");
    }
    assert!(is_settings_key(
        &latin_shortcut_key(
            Key::Character("，".into()),
            Physical::Code(Code::Comma),
            Modifiers::COMMAND
        ),
        Modifiers::COMMAND
    ));
    // Latin keys (Dvorak, AZERTY accents) are untouched.
    let e = Key::Character("é".into());
    assert_eq!(
        latin_shortcut_key(e.clone(), Physical::Code(Code::Digit2), Modifiers::COMMAND),
        e
    );
    let d = Key::Character("j".into());
    assert_eq!(
        latin_shortcut_key(d.clone(), Physical::Code(Code::KeyC), Modifiers::NONE),
        d
    );
}
