//! The App::update message dispatcher.

use super::*;

impl App {
    pub fn update(&mut self, msg: Message) -> Task<Message> {
        if let Some(rel) = self.queued_snap.take() {
            // Drain any pending IPC-driven scroll BEFORE dispatching the new
            // message so the snap lands before further state mutation.
            // The new message is requeued via a follow-up task.
            let mut tasks = vec![Task::done(Message::RestoreBodySnap(rel))];
            if let Some(id) = self.queued_goto.take() {
                // Precise pass: the estimate snap above lands near the target;
                // this op re-lands it from real laid-out bounds (the block is
                // materialized — apply_goto rebuilt the window around it).
                tasks.push(scroll_block_to_center(id));
            }
            tasks.push(Task::done(msg));
            return Task::batch(tasks);
        }
        match msg {
            Message::Open(p) => self.load_file_unless_dirty(p),
            Message::Refresh => self.refresh_status(),
            Message::RevealFileInFinder => {
                let Some(path) = self.focused_file_path() else {
                    return self.show_toast("No file open".into());
                };
                let label = match Self::reveal_file_in_finder(&path) {
                    Ok(()) => "Revealed in Finder".to_string(),
                    Err(error) => error,
                };
                self.show_toast(label)
            }
            Message::CopyFilePath => {
                let Some(path) = self.focused_file_path() else {
                    return self.show_toast("No file open".into());
                };
                let Some(expected) = path.to_str().map(str::to_owned) else {
                    return self.show_toast("File path cannot be copied as text".into());
                };
                self.pending_clipboard_copy = Some(expected.clone());
                let toast = self.show_toast("Copying file path…".into());
                let verify_expected = expected.clone();
                let verify = Task::perform(
                    async {
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    },
                    move |_| Message::CheckClipboardCopy(verify_expected),
                );
                Task::batch([
                    iced::clipboard::write::<Message>(expected.clone()),
                    verify,
                    toast,
                ])
            }
            Message::CheckClipboardCopy(expected) => {
                if self.pending_clipboard_copy.as_deref() != Some(expected.as_str()) {
                    return Task::none();
                }
                iced::clipboard::read().map(move |actual| Message::ClipboardCopyChecked {
                    expected: expected.clone(),
                    actual,
                })
            }
            Message::ClipboardCopyChecked { expected, actual } => {
                if self.pending_clipboard_copy.as_deref() != Some(expected.as_str()) {
                    return Task::none();
                }
                self.pending_clipboard_copy = None;
                if actual.as_deref() == Some(expected.as_str()) {
                    self.show_toast("File path copied".into())
                } else {
                    self.show_toast("Couldn't copy file path".into())
                }
            }
            Message::OpenFileFinderPath(p) => {
                self.overlay = Overlay::None;
                if self.full_mindmap.is_some() {
                    self.begin_full_mindmap_open(p)
                } else {
                    self.load_file_unless_dirty(p)
                }
            }
            Message::OpenWorkspace(p) => {
                self.cancel_refresh_tracking();
                // A workspace selected through Full Mindmap Mode should not
                // silently alter the hidden sidebar's open/closed preference.
                if self.full_mindmap.is_some() {
                    let exit_after_refresh = self.pending_ipc_file_open.is_some();
                    self.begin_full_mindmap_workspace_load(
                        p,
                        false,
                        None,
                        false,
                        false,
                        exit_after_refresh,
                    )
                } else {
                    self.set_workspace(p, true);
                    Task::none()
                }
            }
            Message::OpenFolderPicker => {
                self.open_overlay(Overlay::FolderPicker);
                Task::none()
            }
            Message::OpenFileFinder => {
                if self.workspace.is_some() {
                    self.open_overlay(Overlay::FileFinder);
                } else {
                    self.open_overlay(Overlay::FolderPicker);
                }
                iced::widget::operation::focus(Self::overlay_input_id())
            }
            Message::OpenCommandPalette => {
                self.open_overlay(Overlay::Command);
                iced::widget::operation::focus(Self::overlay_input_id())
            }
            Message::OpenThemePicker => {
                self.open_overlay(Overlay::ThemePicker);
                iced::widget::operation::focus(Self::overlay_input_id())
            }
            Message::OpenVaultSearch => {
                if self.workspace.is_none() {
                    // No folder open: pick one first.
                    self.open_overlay(Overlay::FolderPicker);
                    return iced::widget::operation::focus(Self::overlay_input_id());
                }
                self.vault_open = true;
                self.vault_query.clear();
                self.vault_searched_query = None;
                self.vault_results.clear();
                self.vault_file_count = 0;
                self.vault_truncated = false;
                self.vault_cursor = 0;
                self.vault_collapsed.clear();
                self.vault_viewport = None;
                // Bump seq so any in-flight `run` from a prior open is dropped
                // by the `VaultSearchDone` seq guard instead of repopulating
                // the freshly-blank page.
                self.vault_seq += 1;
                iced::widget::operation::focus(Self::vault_input_id())
            }
            Message::VaultQueryChanged(q) => {
                // Typing only updates the query text; the search runs on Enter
                // (VaultRunSearch) so we don't re-scan the vault per keystroke.
                self.vault_query = q;
                Task::none()
            }
            Message::VaultEnter => {
                // Enter searches when the query was edited since the last search,
                // otherwise opens the hit the cursor is on.
                if self.vault_searched_query.as_deref() == Some(self.vault_query.as_str()) {
                    Task::done(Message::VaultOpenSelected)
                } else {
                    Task::done(Message::VaultRunSearch)
                }
            }
            Message::VaultRunSearch => {
                self.vault_cursor = 0;
                self.vault_seq += 1;
                self.vault_searched_query = Some(self.vault_query.clone());
                let seq = self.vault_seq;
                let files = self.workspace_files.clone();
                let query = self.vault_query.clone();
                Task::perform(
                    crate::vault_search::run(files, query, seq),
                    Message::VaultSearchDone,
                )
            }
            Message::VaultSearchDone(r) => {
                // Drop stale results whose query was superseded mid-scan.
                if r.seq == self.vault_seq {
                    self.vault_results = r.hits;
                    // Hits arrive grouped by file, so distinct files = number
                    // of adjacent path runs (same walk the view used to do).
                    let mut last: Option<&std::path::Path> = None;
                    let mut n = 0;
                    for h in &self.vault_results {
                        if last != Some(h.path.as_path()) {
                            n += 1;
                            last = Some(h.path.as_path());
                        }
                    }
                    self.vault_file_count = n;
                    self.vault_truncated = r.truncated;
                    self.vault_cursor = 0;
                    // New result set: drop the stale viewport so virtualization
                    // renders from the top, and scroll the list back to 0.
                    self.vault_viewport = None;
                    return iced::widget::operation::scroll_to(
                        Self::vault_scroll_id(),
                        iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: 0.0 },
                    );
                }
                Task::none()
            }
            Message::VaultMove(d) => {
                let visible = self.vault_visible_matches();
                if visible.is_empty() {
                    return Task::none();
                }
                let next = (self.vault_cursor as isize + d).clamp(0, visible.len() as isize - 1);
                self.vault_cursor = next as usize;
                self.scroll_vault_to_cursor()
            }
            Message::VaultToggleFile(path) => {
                // Remember which hit the cursor pointed at so it tracks that
                // match across the visible-list shift (collapsing a group above
                // the cursor otherwise silently re-targets it).
                let anchor = self.vault_visible_matches().get(self.vault_cursor).copied();
                if !self.vault_collapsed.remove(&path) {
                    self.vault_collapsed.insert(path);
                }
                let visible = self.vault_visible_matches();
                self.vault_cursor = anchor
                    .and_then(|hi| visible.iter().position(|&v| v == hi))
                    .unwrap_or_else(|| {
                        // Anchored hit is now hidden: clamp to the last visible.
                        self.vault_cursor.min(visible.len().saturating_sub(1))
                    });
                Task::none()
            }
            Message::VaultOpenSelected => {
                // Resolve the cursor to a hit index and share VaultOpenHit's path.
                match self.vault_visible_matches().get(self.vault_cursor).copied() {
                    Some(hi) => Task::done(Message::VaultOpenHit(hi)),
                    None => Task::none(),
                }
            }
            Message::VaultOpenHit(idx) => {
                if let Some(hit) = self.vault_results.get(idx).cloned() {
                    if let Some(blocked) = self.block_file_open_if_dirty() {
                        return blocked;
                    }
                    self.vault_open = false;
                    self.pending_nav = Some(PendingNav {
                        line: Some(hit.line),
                        ..Default::default()
                    });
                    return Task::done(Message::Open(hit.path));
                }
                Task::none()
            }
            Message::VaultClose => {
                self.vault_open = false;
                Task::none()
            }
            Message::VaultScrollTo(y) => iced::widget::operation::scroll_to(
                Self::vault_scroll_id(),
                iced::widget::scrollable::AbsoluteOffset { x: 0.0, y },
            ),
            Message::ToggleShortcuts => {
                if self.overlay == Overlay::Shortcuts {
                    self.overlay = Overlay::None;
                } else {
                    self.open_overlay(Overlay::Shortcuts);
                }
                Task::none()
            }
            Message::QuickSlotsModifier(held) => {
                if held {
                    if self.quick_slots_modifier_held {
                        return Task::none();
                    }

                    self.quick_slots_modifier_held = true;
                    self.quick_slots_rail_revealed = false;
                    self.quick_slots_modifier_generation =
                        self.quick_slots_modifier_generation.wrapping_add(1);
                    let generation = self.quick_slots_modifier_generation;
                    Task::perform(
                        async move {
                            tokio::time::sleep(std::time::Duration::from_millis(
                                QUICK_SLOTS_RAIL_REVEAL_DELAY_MS,
                            ))
                            .await;
                            generation
                        },
                        Message::QuickSlotsModifierReveal,
                    )
                } else {
                    self.quick_slots_modifier_held = false;
                    self.quick_slots_rail_revealed = false;
                    self.quick_slots_modifier_generation =
                        self.quick_slots_modifier_generation.wrapping_add(1);
                    Task::none()
                }
            }
            Message::QuickSlotsModifierReveal(generation) => {
                if self.quick_slots_modifier_held
                    && generation == self.quick_slots_modifier_generation
                {
                    self.quick_slots_rail_revealed = true;
                }
                Task::none()
            }
            Message::QuickSlotActivate(index) => self.begin_quick_slot_activation(index),
            Message::QuickSlotAssign(index) => self.assign_quick_slot(index),
            Message::QuickSlotClear(index) => self.clear_quick_slot(index),
            Message::QuickSlotClearAll => self.clear_all_quick_slots(),
            Message::QuickSlotUndo => self.undo_quick_slot_clear(),
            Message::QuickSlotNew => self.new_quick_slot(),
            Message::QuickSlotClose => self.close_active_quick_slot(),
            Message::QuickSlotCloseWindow => self.close_quick_slot_window(),
            Message::QuickSlotCycle(delta) => {
                let active = self.quick_slots.active;
                let start = active.unwrap_or(if delta < 0 {
                    crate::quick_slots::SLOT_COUNT - 1
                } else {
                    0
                }) as isize;
                let root = self.quick_slots_workspace_root().map(PathBuf::from);
                let steps: Vec<usize> = if active.is_some() {
                    (1..=crate::quick_slots::SLOT_COUNT).collect()
                } else {
                    (0..crate::quick_slots::SLOT_COUNT).collect()
                };
                let next = steps
                    .into_iter()
                    .map(|step| {
                        (start + isize::from(delta) * step as isize)
                            .rem_euclid(crate::quick_slots::SLOT_COUNT as isize)
                            as usize
                    })
                    .find(|&index| {
                        self.quick_slots.occupied(index).is_some_and(|slot| {
                            root.as_ref()
                                .and_then(|root| {
                                    crate::quick_slots::resolve_path(root, &slot.relative_path)
                                })
                                .is_some_and(|path| path.is_file())
                        })
                    });
                match next {
                    Some(index) => self.begin_quick_slot_activation(index),
                    None => self.show_toast("No other valid Quick Slot".into()),
                }
            }
            Message::QuickSlotsPersist(generation) => {
                if generation < self.quick_slots_persist_generation {
                    self.quick_slots_persist_pending = false;
                    return self.schedule_quick_slots_persist();
                }
                self.persist_quick_slots_now();
                Task::none()
            }
            Message::QuickSlotRestorePending => self.apply_pending_quick_slot_restore(),
            Message::QuickSlotFileLoaded {
                index,
                slot,
                result,
            } => {
                let current = self
                    .pending_quick_slot_restore
                    .as_ref()
                    .is_some_and(|pending| {
                        pending.index == index
                            && pending.slot == slot
                            && self.quick_slot_restore_is_current(pending)
                    });
                if !current {
                    return Task::none();
                }
                match result {
                    Ok((path, source)) => {
                        let expected = self.quick_slots_workspace_root().and_then(|root| {
                            crate::quick_slots::resolve_path(root, &slot.relative_path)
                        });
                        if expected.as_ref() != Some(&path) {
                            self.invalidate_pending_quick_slot_restore();
                            return Task::none();
                        }
                        if self.dirty {
                            self.invalidate_pending_quick_slot_restore();
                            return self.show_toast(self.unsaved_edits_open_message());
                        }
                        self.update(Message::FileLoaded(Ok((path, source))))
                    }
                    Err(error) => {
                        self.invalidate_pending_quick_slot_restore();
                        self.error = Some(error);
                        Task::none()
                    }
                }
            }
            Message::CloseOverlay => {
                let was_zoom = self.overlay == Overlay::ImageZoom;
                self.overlay = Overlay::None;
                self.picker = None;
                self.zoom_url = None;
                self.zoom_diagram = None;
                if was_zoom {
                    self.restore_body_scroll()
                } else {
                    Task::none()
                }
            }
            Message::FullMindmapPreviewImageFetched {
                identity,
                range,
                wave,
                url,
                result,
            } => {
                let asset_identity = FullMindmapPreviewAssetIdentity {
                    preview: identity.clone(),
                    range,
                    wave,
                };
                let valid = self.full_mindmap.as_ref().is_some_and(|full| {
                    full.preview_namespace == identity.namespace
                        && full.preview_identity.as_ref() == Some(&identity.request)
                        && full.preview_window.range == range
                        && full.preview_asset_wave_id == wave
                        && full.selected.as_ref().is_some_and(|selected| {
                            matches!(selected, WorkspaceNodeId::File(path) if path == &identity.request.path)
                        })
                });
                let owns_loading = self
                    .full_mindmap
                    .as_ref()
                    .and_then(|full| full.preview_loading_images.get(&url))
                    .is_some_and(|owner| owner == &asset_identity);
                if owns_loading {
                    if let Some(full) = self.full_mindmap.as_mut() {
                        full.preview_loading_images.remove(&url);
                    }
                }
                if !valid {
                    // Only the operation that inserted this Loading sentinel
                    // may remove it. A stale A completion must not delete B's
                    // same-URL request after B has taken ownership.
                    if owns_loading
                        && matches!(self.image_cache.get(&url), Some(ImageState::Loading))
                    {
                        self.image_cache.remove(&url);
                    }
                    return Task::none();
                }
                // A duplicate/current-identity completion that no longer
                // owns the Loading sentinel cannot claim the shared cache or
                // alter the terminal-failure set.
                if !owns_loading {
                    return Task::none();
                }
                match result {
                    Ok(bytes) => {
                        let state = if is_svg_bytes(&bytes)
                            || url.to_ascii_lowercase().ends_with(".svg")
                        {
                            let arc = std::sync::Arc::new(bytes);
                            let svg = iced::widget::svg::Handle::from_memory(arc.as_ref().clone());
                            ImageState::LoadedSvg {
                                svg,
                                bytes: arc,
                                raster: None,
                            }
                        } else {
                            ImageState::Loaded(iced::widget::image::Handle::from_bytes(bytes))
                        };
                        self.image_cache.insert(url, state);
                        self.trim_image_cache();
                        let assets = self.prime_full_mindmap_preview_assets();
                        Task::batch([
                            assets,
                            self.measure_full_mindmap_preview_heights(),
                            self.measure_window_heights(),
                        ])
                    }
                    Err(_) => {
                        if owns_loading {
                            if let Some(full) = self.full_mindmap.as_mut() {
                                full.preview_failed_images.insert(url.clone());
                            }
                        }
                        self.image_cache.insert(url, ImageState::Failed);
                        self.prime_full_mindmap_preview_assets()
                    }
                }
            }
            Message::FullMindmapPreviewAssetWave {
                identity,
                range,
                wave,
            } => {
                let valid = self.full_mindmap.as_ref().is_some_and(|full| {
                    full.preview_namespace == identity.namespace
                        && full.preview_identity.as_ref() == Some(&identity.request)
                        && matches!(&full.preview, FullMindmapPreview::Document { .. })
                        && full.preview_window.range == range
                        && full.preview_asset_wave_id == wave
                });
                if valid {
                    self.prime_full_mindmap_preview_assets()
                } else {
                    Task::none()
                }
            }
            Message::ImageFetched(url, Ok(bytes)) => {
                // The legacy document fetch message has no request identity.
                // If a Full Mindmap preview owns this URL's pending load,
                // terminal failure, or retained result, let the preview lane
                // remain authoritative; otherwise an old document fetch can
                // overwrite a newer preview operation in the shared cache.
                let preview_pending = self
                    .full_mindmap
                    .as_ref()
                    .is_some_and(|full| full.preview_loading_images.contains_key(&url));
                let preview_failed = self
                    .full_mindmap
                    .as_ref()
                    .is_some_and(|full| full.preview_failed_images.contains(&url));
                let preview_has_result = self.full_mindmap.as_ref().is_some_and(|full| {
                    full.preview_asset_images.contains(&url)
                        && self.image_cache.get(&url).is_some_and(|state| {
                            matches!(state, ImageState::Loaded(_) | ImageState::LoadedSvg { .. })
                        })
                });
                if preview_pending || preview_failed || preview_has_result {
                    return Task::none();
                }
                let state = if is_svg_bytes(&bytes) || url.to_ascii_lowercase().ends_with(".svg") {
                    let arc = std::sync::Arc::new(bytes);
                    let svg = iced::widget::svg::Handle::from_memory(arc.as_ref().clone());
                    ImageState::LoadedSvg {
                        svg,
                        bytes: arc,
                        raster: None,
                    }
                } else {
                    let handle = iced::widget::image::Handle::from_bytes(bytes);
                    ImageState::Loaded(handle)
                };
                self.image_cache.insert(url, state);
                self.trim_image_cache();
                // Loaded image replaces a one-line placeholder — re-measure.
                self.measure_window_heights()
            }
            Message::SvgRasterized(key, Ok(rgba_bytes_w_h)) => {
                let (rgba, w, h) = rgba_bytes_w_h;
                let handle = iced::widget::image::Handle::from_rgba(w, h, rgba);
                if let Some(entry) = self.image_cache.get_mut(&key) {
                    if let ImageState::LoadedSvg { raster, .. } = entry {
                        *raster = Some(handle);
                    }
                    self.image_cache.resync_cost();
                } else {
                    self.image_cache.insert(key, ImageState::Loaded(handle));
                }
                self.trim_image_cache();
                Task::none()
            }
            Message::SvgRasterized(key, Err(_)) => {
                self.image_cache.insert(key, ImageState::Failed);
                Task::none()
            }
            Message::ImageFetched(url, Err(_)) => {
                let preview_pending = self
                    .full_mindmap
                    .as_ref()
                    .is_some_and(|full| full.preview_loading_images.contains_key(&url));
                let preview_failed = self
                    .full_mindmap
                    .as_ref()
                    .is_some_and(|full| full.preview_failed_images.contains(&url));
                let preview_has_result = self.full_mindmap.as_ref().is_some_and(|full| {
                    full.preview_asset_images.contains(&url)
                        && self.image_cache.get(&url).is_some_and(|state| {
                            matches!(state, ImageState::Loaded(_) | ImageState::LoadedSvg { .. })
                        })
                });
                if preview_pending || preview_failed || preview_has_result {
                    return Task::none();
                }
                self.image_cache.insert(url, ImageState::Failed);
                Task::none()
            }
            Message::HintSelection => {
                return self.show_toast("Press ⌘E to edit & select text".into());
            }
            Message::FoldChordStart => {
                self.fold_chord_pending = true;
                return self.show_toast("Fold: press 0-6 …".into());
            }
            Message::FoldChordCancel => {
                self.fold_chord_pending = false;
                Task::none()
            }
            Message::FoldToLevel(n) => {
                self.fold_chord_pending = false;
                // Node-depth folding belongs to the open document, including
                // structured-data Mindmaps. Full Mindmap has independent
                // workspace expansion state and must never mutate the hidden
                // document.
                if self.full_mindmap.is_some() {
                    return Task::none();
                }
                if self.view_mode == ViewMode::Mindmap {
                    let nodes = if self.is_data_doc {
                        let lang = data_lang_for(self.file.as_deref()).unwrap_or("json");
                        let (nodes, size, paths, collapsed) =
                            crate::data_mindmap::build_layout_for_depth(
                                &self.source,
                                lang,
                                self.file.as_deref(),
                                n,
                            );
                        self.mindmap_collapsed = collapsed;
                        self.replace_mindmap_layout(nodes, size, paths)
                    } else {
                        self.mindmap_collapsed = crate::mindmap::collapsed_for_depth(&self.ast, n);
                        self.invalidate_mindmap_layout();
                        self.mindmap_layout().0
                    };
                    let selected_is_visible = self
                        .mindmap_selected
                        .is_some_and(|selected| nodes.iter().any(|node| node.id == Some(selected)));
                    if !selected_is_visible {
                        let next = nodes
                            .first()
                            .and_then(|root| root.children.first())
                            .and_then(|index| nodes.get(*index))
                            .and_then(|node| node.id);
                        self.mindmap_selected = next;
                        self.mindmap_panel_shown =
                            self.mindmap_panel_open.then_some(next).flatten();
                    }
                    return Task::none();
                }
                if self.is_data_doc {
                    return self.show_toast("Fold levels are available in Mindmap mode".into());
                }
                self.folded.clear();
                if n > 0 {
                    for (id, b) in &self.ast {
                        if let Block::Heading { level, .. } = b {
                            if *level as u8 >= n {
                                self.folded.insert(*id);
                            }
                        }
                    }
                }
                self.rebuild_virt_here();
                self.measure_window_heights()
            }
            Message::ToggleFold(id) => {
                if self.folded.contains(&id) {
                    self.folded.remove(&id);
                    self.rebuild_virt_here();
                    return self.measure_window_heights();
                }
                let mut parent_level: Option<u8> = None;
                let mut new_folds: Vec<crate::ast::BlockId> = Vec::new();
                for (bid, b) in &self.ast {
                    if let Block::Heading { level, .. } = b {
                        let lvl = *level as u8;
                        if let Some(pl) = parent_level {
                            if lvl <= pl {
                                break;
                            }
                            new_folds.push(*bid);
                        } else if *bid == id {
                            parent_level = Some(lvl);
                        }
                    }
                }
                if parent_level.is_some() {
                    self.folded.insert(id);
                    for bid in new_folds {
                        self.folded.insert(bid);
                    }
                }
                self.rebuild_virt_here();
                self.measure_window_heights()
            }
            Message::HeadingHoverEnter(id) => {
                self.hovered_heading = Some(id);
                Task::none()
            }
            Message::HeadingHoverExit(id) => {
                if self.hovered_heading == Some(id) {
                    self.hovered_heading = None;
                }
                Task::none()
            }
            Message::FontSizeUp => {
                let size = self.adjust_font_scale(1.1);
                self.height_cache.clear();
                self.rebuild_virt_here();
                let preview_measure = self.refresh_full_mindmap_preview_heights();
                Task::batch([
                    self.measure_window_heights(),
                    preview_measure,
                    self.show_toast(format!("Font {:.0} px", size)),
                ])
            }
            Message::FontSizeDown => {
                let size = self.adjust_font_scale(1.0 / 1.1);
                self.height_cache.clear();
                self.rebuild_virt_here();
                let preview_measure = self.refresh_full_mindmap_preview_heights();
                Task::batch([
                    self.measure_window_heights(),
                    preview_measure,
                    self.show_toast(format!("Font {:.0} px", size)),
                ])
            }
            Message::FontSizeReset => {
                self.font_scale = 1.0;
                self.typography = self.typography_base;
                self.height_cache.clear();
                self.rebuild_virt_here();
                let preview_measure = self.refresh_full_mindmap_preview_heights();
                Task::batch([
                    self.measure_window_heights(),
                    preview_measure,
                    self.show_toast("Font reset".to_string()),
                ])
            }
            Message::MindmapNativePinch(delta) => {
                // The native monitor runs for the whole app. Only forward a
                // finite magnification delta while a graph owns the visible
                // surface; search and overlays retain their normal input.
                if delta.is_finite() && self.overlay == Overlay::None {
                    if self.full_mindmap.is_some() {
                        self.full_mindmap_native_pinch_log += f64::from(delta);
                    } else if self.view_mode == ViewMode::Mindmap && !self.search_open {
                        self.mindmap_native_pinch_log += f64::from(delta);
                    }
                }
                Task::none()
            }
            Message::ToggleFooter => {
                self.show_footer = !self.show_footer;
                self.prefs.show_footer = self.show_footer;
                crate::prefs::save(&self.prefs);
                self.show_toast(
                    if self.show_footer {
                        "Footer shown"
                    } else {
                        "Footer hidden"
                    }
                    .to_string(),
                )
            }
            Message::ToggleViewMode => {
                if self.file.is_none() {
                    return Task::none();
                }
                let checkpoint = self.checkpoint_active_quick_slot();
                match self.view_mode {
                    ViewMode::Raw => Task::batch([checkpoint, self.exit_zen_edit_mode()]),
                    ViewMode::Rendered | ViewMode::Mindmap => {
                        Task::batch([checkpoint, self.enter_zen_edit_mode()])
                    }
                }
            }
            Message::ToggleMindmap => {
                if self.file.is_none() {
                    return Task::none();
                }
                let restore = self.restore_body_scroll();
                match self.view_mode {
                    ViewMode::Mindmap => {
                        self.mindmap_panel_drag = None;
                        self.view_mode = ViewMode::Rendered;
                    }
                    ViewMode::Raw => {
                        self.sync_editor_to_source();
                        self.editor = None;
                        self.edit_history.clear();
                        self.edit_redo.clear();
                        self.restore_zen_chrome();
                        self.view_mode = ViewMode::Mindmap;
                    }
                    ViewMode::Rendered => self.view_mode = ViewMode::Mindmap,
                }
                // On first open (no selection yet), focus root's first child so
                // arrow nav and the preview panel start at the top heading.
                self.mindmap_focus_first_child();
                let checkpoint = self.checkpoint_active_quick_slot();
                Task::batch([checkpoint, restore])
            }
            Message::ToggleFullMindmap => {
                // Manual Full Mindmap navigation supersedes a slot activation
                // that has not completed yet; stale file/preview results must
                // not mutate the newly chosen surface.
                let checkpoint = self.checkpoint_active_quick_slot();
                self.invalidate_pending_quick_slot_restore();
                if self.full_mindmap.is_some() {
                    Task::batch([checkpoint, self.exit_full_mindmap(false)])
                } else {
                    Task::batch([checkpoint, self.enter_full_mindmap()])
                }
            }
            Message::ExitFullMindmap => {
                let checkpoint = self.checkpoint_active_quick_slot();
                self.invalidate_pending_quick_slot_restore();
                Task::batch([checkpoint, self.exit_full_mindmap(false)])
            }
            Message::FullMindmapToggleNode(id) => {
                let checkpoint = self.checkpoint_active_quick_slot();
                self.invalidate_pending_quick_slot_restore();
                let workspace_path = self.full_mindmap_graph().and_then(|graph| {
                    graph.node(&id).and_then(|node| {
                        matches!(
                            node.kind,
                            WorkspaceNodeKind::Root | WorkspaceNodeKind::Folder
                        )
                        .then(|| node.path.clone())
                        .flatten()
                    })
                });
                let mut load = None;
                let mut collapse = None;
                self.reset_full_mindmap_preview_window();
                if let Some(full) = self.full_mindmap.as_mut() {
                    full.deferred_file_selection = None;
                    full.selected = Some(id.clone());
                    full.focus_request = Some(id.clone());
                    full.pending_preview_settle = None;
                    full.pending_preview = None;
                    full.preview = FullMindmapPreview::None;
                    full.load_error = None;
                    if full
                        .pending_workspace_load
                        .as_ref()
                        .is_some_and(|request| !request.preserve_navigation)
                    {
                        full.pending_workspace_load = None;
                    }
                    if let Some(path) = workspace_path.clone() {
                        if full.expanded.contains(&path) {
                            collapse = Some(path);
                        } else {
                            full.expanded.insert(path.clone());
                            load = Some(path);
                        }
                    }
                }
                if !matches!(&id, WorkspaceNodeId::File(_)) {
                    self.clear_active_quick_slot();
                }
                if workspace_path.is_some() {
                    self.cancel_full_mindmap_verification();
                    if load.is_some() {
                        self.bump_full_mindmap_expansion_generation();
                    }
                }
                self.invalidate_full_mindmap_layout();
                if let Some(path) = collapse {
                    self.evict_full_mindmap_folder(&path);
                    // Cancellation clears the old hidden set. Re-snapshot the
                    // remaining expanded frontier now, otherwise unresolved
                    // shells under unrelated expanded parents flash visible
                    // after this branch collapses.
                    Task::batch([checkpoint, self.begin_full_mindmap_verification_wave()])
                } else {
                    let navigation = load.map_or_else(Task::none, |path| {
                        let verification = self.begin_full_mindmap_verification_wave();
                        let branch = self.begin_full_mindmap_folder_load(path);
                        Task::batch([verification, branch])
                    });
                    Task::batch([checkpoint, navigation])
                }
            }
            Message::FullMindmapSelectNode(id) => self.select_full_mindmap_node(id),
            Message::FullMindmapDeselect => {
                let checkpoint = self.checkpoint_active_quick_slot();
                self.invalidate_pending_quick_slot_restore();
                self.reset_full_mindmap_preview_window();
                if let Some(full) = self.full_mindmap.as_mut() {
                    full.selected = None;
                    full.focus_request = None;
                    full.deferred_file_selection = None;
                    full.panel_drag = None;
                    full.pending_preview_settle = None;
                    full.pending_preview = None;
                    full.preview = FullMindmapPreview::None;
                    if full
                        .pending_workspace_load
                        .as_ref()
                        .is_some_and(|request| !request.preserve_navigation)
                    {
                        full.pending_workspace_load = None;
                    }
                }
                self.clear_active_quick_slot();
                self.invalidate_full_mindmap_layout();
                checkpoint
            }
            Message::FullMindmapNavigate(dir) => {
                enum Navigation {
                    Select(WorkspaceNodeId),
                    Dive(WorkspaceNodeId),
                    WorkspaceParent,
                    None,
                }

                let navigation = (|| {
                    let full = self.full_mindmap.as_ref()?;
                    let graph = self.full_mindmap_graph()?;
                    let current = full.selected.clone().unwrap_or_else(|| graph.root_id());
                    let nav = match dir {
                        MindmapDir::Up => graph
                            .sibling(&current, -1)
                            .map(Navigation::Select)
                            .unwrap_or(Navigation::None),
                        MindmapDir::Down => graph
                            .sibling(&current, 1)
                            .map(Navigation::Select)
                            .unwrap_or(Navigation::None),
                        MindmapDir::Left => graph
                            .parent(&current)
                            .map_or_else(|| Navigation::WorkspaceParent, Navigation::Select),
                        MindmapDir::Right => {
                            if graph
                                .node(&current)
                                .is_some_and(|node| node.has_hidden_children)
                            {
                                Navigation::Dive(current)
                            } else {
                                graph
                                    .first_child(&current)
                                    .map(Navigation::Select)
                                    .unwrap_or(Navigation::None)
                            }
                        }
                    };
                    Some(nav)
                })()
                .unwrap_or(Navigation::None);

                match navigation {
                    Navigation::Select(id) => self.update(Message::FullMindmapSelectNode(id)),
                    Navigation::Dive(id) => self.update(Message::FullMindmapDiveWorkspace(id)),
                    Navigation::WorkspaceParent => self.update(Message::FullMindmapWorkspaceParent),
                    Navigation::None => Task::none(),
                }
            }
            Message::FullMindmapDiveWorkspace(id) => {
                let Some(graph) = self.full_mindmap_graph() else {
                    return Task::none();
                };
                let Some(node) = graph.node(&id) else {
                    return Task::none();
                };
                if !matches!(
                    node.kind,
                    WorkspaceNodeKind::Root | WorkspaceNodeKind::Folder
                ) {
                    return Task::none();
                }
                self.invalidate_pending_quick_slot_restore();
                let child = if node.has_hidden_children {
                    if let Some(path) = node.path.clone() {
                        self.cancel_full_mindmap_verification();
                        if let Some(full) = self.full_mindmap.as_mut() {
                            full.expanded.insert(path.clone());
                            full.expansion_generation = full.expansion_generation.wrapping_add(1);
                        }
                        self.invalidate_full_mindmap_layout();
                        let verification = self.begin_full_mindmap_verification_wave();
                        let load = self.begin_full_mindmap_folder_load(path);
                        let select = self
                            .full_mindmap_graph()
                            .and_then(|expanded| expanded.first_child(&id))
                            .map_or_else(Task::none, |child| {
                                self.update(Message::FullMindmapSelectNode(child))
                            });
                        return Task::batch([verification, load, select]);
                    } else {
                        None
                    }
                } else {
                    graph.first_child(&id)
                };
                child.map_or_else(Task::none, |child| {
                    self.update(Message::FullMindmapSelectNode(child))
                })
            }
            Message::FullMindmapActivate => {
                let action = (|| {
                    let full = self.full_mindmap.as_ref()?;
                    let graph = self.full_mindmap_graph()?;
                    let selected = full.selected.clone().unwrap_or_else(|| graph.root_id());
                    if let WorkspaceNodeId::File(path) = &selected {
                        if full.deferred_file_selection.as_ref() == Some(path) {
                            return Some(Message::OpenFileFinderPath(path.clone()));
                        }
                    }
                    let node = graph.node(&selected)?;
                    match node.kind {
                        WorkspaceNodeKind::Root | WorkspaceNodeKind::Folder => {
                            node.path.clone().map(Message::FullMindmapSetRoot)
                        }
                        WorkspaceNodeKind::File => {
                            node.path.clone().map(Message::OpenFileFinderPath)
                        }
                        WorkspaceNodeKind::Empty
                        | WorkspaceNodeKind::Error
                        | WorkspaceNodeKind::Truncated
                        | WorkspaceNodeKind::Loading => None,
                    }
                })();
                action.map_or_else(Task::none, |message| self.update(message))
            }
            Message::FullMindmapToggleSelected => {
                let id = self
                    .full_mindmap
                    .as_ref()
                    .and_then(|full| full.selected.clone());
                let can_toggle = self.full_mindmap_graph().is_some_and(|graph| {
                    id.as_ref().is_some_and(|id| {
                        graph.node(id).is_some_and(|node| {
                            matches!(
                                node.kind,
                                WorkspaceNodeKind::Root | WorkspaceNodeKind::Folder
                            )
                        })
                    })
                });
                if can_toggle {
                    if let Some(id) = id {
                        return self.update(Message::FullMindmapToggleNode(id));
                    }
                }
                Task::none()
            }
            Message::FullMindmapSelectRoot => {
                if let Some(graph) = self.full_mindmap_graph() {
                    return self.update(Message::FullMindmapSelectNode(graph.root_id()));
                }
                Task::none()
            }
            Message::FullMindmapSetRoot(path) => {
                if self.workspace.as_ref() == Some(&path) {
                    return Task::none();
                }
                let exit_after_refresh = self.pending_ipc_file_open.is_some();
                self.begin_full_mindmap_workspace_load(
                    path,
                    true,
                    None,
                    false,
                    false,
                    exit_after_refresh,
                )
            }
            Message::FullMindmapWorkspaceParent => {
                let root = self
                    .workspace
                    .clone()
                    .filter(|_| self.full_mindmap.is_some());
                let Some(root) = root else {
                    return Task::none();
                };
                let Some(parent) = root.parent().map(PathBuf::from) else {
                    return Task::none();
                };
                self.invalidate_pending_quick_slot_restore();
                let exit_after_refresh = self.pending_ipc_file_open.is_some();
                self.begin_full_mindmap_workspace_load(
                    parent,
                    true,
                    None,
                    false,
                    false,
                    exit_after_refresh,
                )
            }
            Message::FullMindmapReturnToFiles => {
                // Files is app-owned navigation away from the active preview;
                // persist its outgoing Full Mindmap context before the exit or
                // hidden-filter refresh takes ownership of the state.
                let checkpoint = self.checkpoint_active_quick_slot();
                self.invalidate_pending_quick_slot_restore();
                Task::batch([checkpoint, self.exit_full_mindmap(true)])
            }
            Message::FullMindmapTogglePanel => {
                if let Some(full) = self.full_mindmap.as_mut() {
                    full.panel_open = !full.panel_open;
                    if !full.panel_open {
                        full.panel_drag = None;
                    }
                }
                Task::none()
            }
            Message::FullMindmapCyclePanelWidth => {
                let window_size = self.window_size;
                if let Some(full) = self.full_mindmap.as_mut() {
                    // This explicit panel-width action reveals the resized result.
                    full.panel_open = true;
                    full.panel_drag = None;
                    full.panel_step = (full.panel_step + 1) % MIND_PANEL_FRACS.len();
                    full.panel_width = mindmap_panel_width_for_step(full.panel_step, window_size);
                }
                self.refresh_full_mindmap_preview_heights()
            }
            Message::FullMindmapPanelDragStart(_) => {
                if let Some(full) = self.full_mindmap.as_mut() {
                    full.panel_drag = Some((full.panel_width, None));
                }
                Task::none()
            }
            Message::FullMindmapPanelDragMove(cursor_x) => {
                if let Some(full) = self.full_mindmap.as_mut() {
                    if let Some((origin, anchor)) = full.panel_drag {
                        match anchor {
                            None => full.panel_drag = Some((origin, Some(cursor_x))),
                            Some(anchor) => {
                                full.panel_width =
                                    mindmap_panel_width_for_drag(origin, anchor, cursor_x);
                            }
                        }
                    }
                }
                self.refresh_full_mindmap_preview_heights()
            }
            Message::FullMindmapPanelDragEnd => {
                if let Some(full) = self.full_mindmap.as_mut() {
                    full.panel_drag = None;
                }
                Task::none()
            }
            Message::FullMindmapFileLoaded { request, result } => {
                let current = self.full_mindmap.as_ref().is_some_and(|full| {
                    full.pending_open
                        .as_ref()
                        .is_some_and(|pending| pending == &request)
                });
                if !current {
                    // A newer request replaced this one, or the user left Full
                    // Mindmap Mode while it was in flight.
                    return Task::none();
                }
                if self.dirty {
                    if let Some(full) = self.full_mindmap.as_mut() {
                        if full.pending_open.as_ref() == Some(&request) {
                            full.pending_open = None;
                        }
                    }
                    self.pending_nav = None;
                    self.invalidate_pending_quick_slot_restore();
                    return self.show_toast(self.unsaved_edits_open_message());
                }
                match result {
                    Err(error) => {
                        if let Some(full) = self.full_mindmap.as_mut() {
                            if full.pending_open.as_ref() == Some(&request) {
                                full.pending_open = None;
                                full.load_error = Some(error);
                            }
                        }
                        Task::none()
                    }
                    Ok((path, source)) if path == request.path => {
                        // Delegate synchronously to the established load path:
                        // the dirty recheck above and this call are one update,
                        // so a late completion cannot discard an intervening
                        // editor change. Clearing the navigator only after the
                        // request was accepted preserves its dirty safety.
                        self.cancel_full_mindmap_verification();
                        self.pending_ipc_file_open = None;
                        self.full_mindmap = None;
                        // Full Mindmap file activation always hands the file
                        // back to the document mindmap, whose normal load path
                        // then focuses the first content child. Let that path
                        // clean up a pre-existing Raw/Zen editor first; its
                        // leave_zen_edit_mode helper restores Rendered, so the
                        // bridge reapplies Mindmap only after delegation.
                        let task = self.update(Message::FileLoaded(Ok((path, source))));
                        self.view_mode = ViewMode::Mindmap;
                        self.mindmap_focus_first_child();
                        task
                    }
                    Ok((path, _)) => {
                        if let Some(full) = self.full_mindmap.as_mut() {
                            if full.pending_open.as_ref() == Some(&request) {
                                full.pending_open = None;
                                full.load_error =
                                    Some(format!("Loaded unexpected file: {}", path.display()));
                            }
                        }
                        Task::none()
                    }
                }
            }
            Message::FullMindmapPreviewSettle { request } => {
                let current = self.full_mindmap.as_ref().is_some_and(|full| {
                    full.pending_preview_settle.as_ref() == Some(&request)
                        && matches!(
                            full.selected.as_ref(),
                            Some(WorkspaceNodeId::File(path)) if path == &request.path
                        )
                });
                if !current {
                    // A newer selection, folder/root/filter change, or mode
                    // exit superseded this timer. No read may begin here.
                    return self.start_full_mindmap_preview_settle_worker();
                }
                if let Some(full) = self.full_mindmap.as_mut() {
                    full.pending_preview_settle = None;
                }
                self.begin_full_mindmap_preview(Some(request.path))
            }
            Message::FullMindmapPreviewLoaded { request, result } => {
                let current = self.full_mindmap.as_ref().is_some_and(|full| {
                    full.pending_preview
                        .as_ref()
                        .is_some_and(|pending| pending == &request)
                });
                if !current {
                    // Selection, folder, workspace, or mode changed while the
                    // read was running. A preview result must never affect the
                    // current document or a newer selection.
                    return Task::none();
                }
                match result {
                    Ok((path, source)) if path == request.path => {
                        // Every Markdown parse/highlight runs away from the
                        // update thread. A short file can still contain
                        // hundreds of blocks; byte size is not a safe proxy
                        // for parser cost or widget-tree size.
                        let Some((epoch, cancel, parse_gate)) =
                            self.full_mindmap.as_ref().map(|full| {
                                (
                                    full.preview_work_epoch.load(Ordering::Acquire),
                                    Arc::clone(&full.preview_work_epoch),
                                    Arc::clone(&full.preview_parse_gate),
                                )
                            })
                        else {
                            return Task::none();
                        };
                        return Task::perform(
                            parse_full_mindmap_preview_guarded(
                                path,
                                Arc::from(source),
                                cancel,
                                epoch,
                                parse_gate,
                            ),
                            move |result| Message::FullMindmapPreviewParsed { request, result },
                        );
                    }
                    Ok((path, _)) => {
                        if let Some(full) = self.full_mindmap.as_mut() {
                            if full.pending_preview.as_ref() == Some(&request) {
                                full.pending_preview = None;
                                full.preview = FullMindmapPreview::Error {
                                    path: request.path,
                                    error: format!(
                                        "Preview loaded unexpected file: {}",
                                        path.display()
                                    ),
                                };
                            }
                        }
                        if self.quick_slot_preview_restore_guard.is_some() {
                            self.invalidate_pending_quick_slot_restore();
                        }
                    }
                    Err(error) => {
                        if error == FULL_MINDMAP_PREVIEW_CANCELLED {
                            // A newer selection already invalidated the
                            // request in normal operation. Keep this guard so
                            // a cooperative worker cancellation can never
                            // replace a current loading state with an error.
                            return Task::none();
                        }
                        if let Some(full) = self.full_mindmap.as_mut() {
                            if full.pending_preview.as_ref() == Some(&request) {
                                full.pending_preview = None;
                                full.preview = FullMindmapPreview::Error {
                                    path: request.path,
                                    error,
                                };
                            }
                        }
                        if self.quick_slot_preview_restore_guard.is_some() {
                            self.invalidate_pending_quick_slot_restore();
                        }
                    }
                }
                Task::none()
            }
            Message::FullMindmapPreviewParsed { request, result } => {
                let current = self
                    .full_mindmap
                    .as_ref()
                    .is_some_and(|full| full.pending_preview.as_ref() == Some(&request));
                if !current {
                    return Task::none();
                }
                match result {
                    Ok(preview) => {
                        if let Some(full) = self.full_mindmap.as_mut() {
                            if full.pending_preview.as_ref() == Some(&request) {
                                full.pending_preview = None;
                                full.preview_shape_ready = false;
                                full.preview = preview;
                            }
                        }
                        // The accepted parse is the first point at which the
                        // full block list exists. Build its own shared
                        // virtual window and measure only that window after
                        // the next layout pass.
                        self.rebuild_full_mindmap_preview_here();
                        let assets = self.prime_full_mindmap_preview_assets();
                        let restore_position = if self
                            .full_mindmap
                            .as_ref()
                            .is_some_and(|full| full.preview_viewport.is_some())
                        {
                            self.take_current_quick_slot_preview_restore()
                        } else {
                            None
                        };
                        let restore = restore_position.map_or_else(Task::none, |position| {
                            self.quick_slot_restore_preview_position(position)
                        });
                        Task::batch([assets, self.measure_full_mindmap_preview_heights(), restore])
                    }
                    Err(error) => {
                        if error == FULL_MINDMAP_PREVIEW_CANCELLED {
                            return Task::none();
                        }
                        if let Some(full) = self.full_mindmap.as_mut() {
                            if full.pending_preview.as_ref() == Some(&request) {
                                full.pending_preview = None;
                                full.preview = FullMindmapPreview::Error {
                                    path: request.path,
                                    error,
                                };
                            }
                        }
                        if self.quick_slot_preview_restore_guard.is_some() {
                            self.invalidate_pending_quick_slot_restore();
                        }
                        Task::none()
                    }
                }
            }
            Message::FullMindmapFolderLoaded { request, result } => {
                let current = self.workspace.as_ref() == Some(&request.workspace_root)
                    && self.show_hidden == request.show_hidden
                    && self.workspace_snapshot_show_hidden == request.show_hidden
                    && self.full_mindmap.as_ref().is_some_and(|full| {
                        full.expanded.contains(&request.folder)
                            && full
                                .pending_folder_loads
                                .get(&request.folder)
                                .is_some_and(|pending| pending == &request)
                    });
                if !current {
                    return Task::none();
                }

                let mut accepted_exact_empty = false;
                let accepted_files = match result {
                    Ok((path, snapshot)) if path == request.folder => {
                        accepted_exact_empty = matches!(
                            snapshot.recursive_supported_file_count,
                            tree::RecursiveFileCount::Exact(0)
                        );
                        let folders = std::sync::Arc::new(snapshot.folders);
                        let files = std::sync::Arc::new(snapshot.files);
                        if let Some(full) = self.full_mindmap.as_mut() {
                            full.pending_folder_loads.remove(&request.folder);
                            full.materialized_folders.insert(
                                request.folder.clone(),
                                workspace_mindmap::MaterializedFolder::Loaded {
                                    folders,
                                    files: std::sync::Arc::clone(&files),
                                    recursive_supported_file_count: snapshot
                                        .recursive_supported_file_count,
                                    truncated: snapshot.truncated,
                                },
                            );
                        }
                        Some(files)
                    }
                    Ok((path, _)) => {
                        if let Some(full) = self.full_mindmap.as_mut() {
                            full.pending_folder_loads.remove(&request.folder);
                            full.materialized_folders.insert(
                                request.folder.clone(),
                                workspace_mindmap::MaterializedFolder::Error(format!(
                                    "Loaded unexpected folder: {}",
                                    path.display()
                                )),
                            );
                        }
                        None
                    }
                    Err(error) => {
                        if let Some(full) = self.full_mindmap.as_mut() {
                            full.pending_folder_loads.remove(&request.folder);
                            full.materialized_folders.insert(
                                request.folder.clone(),
                                workspace_mindmap::MaterializedFolder::Error(error),
                            );
                        }
                        None
                    }
                };
                self.invalidate_full_mindmap_layout();
                if accepted_exact_empty {
                    // A formerly unknown shell is no longer a valid visible
                    // selection once its bounded retry proves it exact-empty.
                    self.normalize_full_mindmap_workspace();
                }
                // The branch snapshot may have introduced a new expanded
                // frontier of LowerBound(0) child shells after the current
                // verification wave froze its denominator. Keep those shells
                // hidden and either queue a follow-up behind the active wave
                // or start a fresh fixed wave immediately when idle.
                let followup = self.schedule_full_mindmap_verification_followup();

                let deferred = self.full_mindmap.as_ref().and_then(|full| {
                    full.deferred_file_selection
                        .as_ref()
                        .filter(|path| path.parent() == Some(request.folder.as_path()))
                        .cloned()
                });
                if let (Some(files), Some(path)) = (accepted_files.as_ref(), deferred) {
                    if files.contains(&path) {
                        if let Some(full) = self.full_mindmap.as_mut() {
                            full.deferred_file_selection = None;
                            let file_id = WorkspaceNodeId::File(path.clone());
                            full.selected = Some(file_id.clone());
                            full.focus_request = Some(file_id);
                        }
                        if self.file.as_ref() == Some(&path) {
                            let source = self.source_snapshot_for_preview();
                            let preview =
                                self.begin_full_mindmap_preview_source(path.clone(), source);
                            return Task::batch([followup, preview]);
                        }
                        return Task::batch([
                            followup,
                            self.begin_full_mindmap_preview(Some(path)),
                        ]);
                    }
                    if let Some(full) = self.full_mindmap.as_mut() {
                        full.deferred_file_selection = None;
                    }
                    self.normalize_full_mindmap_workspace();
                }

                let loading_id = WorkspaceNodeId::Status(
                    request.folder.clone(),
                    workspace_mindmap::WorkspaceStatus::LoadingFiles,
                );
                let selected_loading = self
                    .full_mindmap
                    .as_ref()
                    .is_some_and(|full| full.selected.as_ref() == Some(&loading_id));
                if selected_loading {
                    let parent_id = if self.workspace.as_ref() == Some(&request.folder) {
                        WorkspaceNodeId::Root(request.folder)
                    } else {
                        WorkspaceNodeId::Folder(request.folder)
                    };
                    if let Some(child) = self
                        .full_mindmap_graph()
                        .and_then(|graph| graph.first_child(&parent_id))
                    {
                        return Task::batch([followup, self.select_full_mindmap_node(child)]);
                    }
                    return Task::batch([followup, self.select_full_mindmap_node(parent_id)]);
                }
                followup
            }
            Message::FullMindmapVerificationLoaded { request, result } => {
                self.handle_full_mindmap_verification_loaded(request, result)
            }
            Message::RefreshWorkspaceLoaded { request, result } => {
                self.handle_refresh_workspace_loaded(request, result)
            }
            Message::FullMindmapWorkspaceLoaded { request, result } => {
                let current = self.full_mindmap.as_ref().is_some_and(|full| {
                    full.pending_workspace_load
                        .as_ref()
                        .is_some_and(|pending| pending == &request)
                });
                if !current {
                    // The navigator exited or a newer folder choice superseded
                    // this bounded index while it was running.
                    return Task::none();
                }
                let refresh_id = self
                    .pending_refresh_full_mindmap_workspace
                    .as_ref()
                    .filter(|pending| *pending == &request)
                    .and_then(|_| self.pending_refresh.as_ref().map(|refresh| refresh.id));
                let mut refresh_error = None;
                let open_after = request.open_after.clone();
                let mut followup = Task::none();
                match result {
                    Ok((path, snapshot)) if path == request.path => {
                        if request.preserve_navigation {
                            // Same-workspace refreshes replace only snapshot
                            // data. File preview/open requests and the latest
                            // Full Mindmap navigation remain independently
                            // owned and valid across this completion.
                            self.replace_workspace_snapshot(path.clone(), snapshot);
                            if let Some(full) = self.full_mindmap.as_mut() {
                                full.pending_workspace_load = None;
                            }
                            self.invalidate_full_mindmap_layout();
                            self.normalize_full_mindmap_workspace();
                        } else {
                            self.apply_workspace_snapshot(path.clone(), snapshot, false);
                        }
                        if request.exit_after_refresh {
                            self.cancel_full_mindmap_verification();
                            self.reset_full_mindmap_preview_window();
                            self.full_mindmap = None;
                            if request.return_to_files_after {
                                self.sidebar_open = true;
                                self.sidebar_tab = SidebarTab::Files;
                                self.reveal_current_file();
                            }
                            followup = if self.pending_quick_slot_restore.is_some() {
                                Task::done(Message::QuickSlotRestorePending)
                            } else {
                                self.start_pending_ipc_file_open(self.restore_body_scroll())
                            };
                        } else if !request.preserve_navigation && request.select_root {
                            self.reset_full_mindmap_preview_window();
                            if let Some(full) = self.full_mindmap.as_mut() {
                                let root_id = WorkspaceNodeId::Root(path.clone());
                                full.selected = Some(root_id.clone());
                                full.focus_request = Some(root_id);
                                full.expanded.clear();
                                full.expanded.insert(path);
                                full.pending_open = None;
                                full.pending_preview_settle = None;
                                full.pending_preview = None;
                                full.preview = FullMindmapPreview::None;
                            }
                            self.invalidate_full_mindmap_layout();
                        }
                        if !request.exit_after_refresh {
                            if let Some(file) = open_after.clone() {
                                followup = self.begin_full_mindmap_open(file);
                            }
                        }
                        if !request.exit_after_refresh {
                            let verification = self.begin_full_mindmap_verification_wave();
                            followup = if open_after.is_none() {
                                Task::batch([
                                    followup,
                                    verification,
                                    self.begin_full_mindmap_expanded_folder_loads(),
                                ])
                            } else {
                                Task::batch([followup, verification])
                            };
                        }
                        if !request.exit_after_refresh && self.pending_quick_slot_restore.is_some()
                        {
                            // The accepted snapshot may have replaced the
                            // provisional navigator state. Retry the pending
                            // slot only after that replacement is complete.
                            followup = Task::batch([
                                followup,
                                Task::done(Message::QuickSlotRestorePending),
                            ]);
                        }
                    }
                    Ok((path, _)) => {
                        let message = format!("Indexed unexpected folder: {}", path.display());
                        refresh_error = Some(message.clone());
                        if request.preserve_navigation {
                            self.show_hidden = self.workspace_snapshot_show_hidden;
                        }
                        if request.exit_after_refresh {
                            self.error = Some(message);
                            let cleanup =
                                self.finish_full_mindmap_exit(request.return_to_files_after);
                            followup = self.start_pending_ipc_file_open(cleanup);
                        } else if let Some(full) = self.full_mindmap.as_mut() {
                            full.pending_workspace_load = None;
                            full.load_error = Some(message);
                        }
                        if request.preserve_navigation && !request.exit_after_refresh {
                            followup = self.begin_full_mindmap_expanded_folder_loads();
                        }
                    }
                    Err(error) => {
                        let message = format!("Couldn't index {}: {error}", request.path.display());
                        refresh_error = Some(message.clone());
                        // The existing workspace snapshot is still filtered
                        // under the previous value. Revert the UI preference so
                        // every accepted failure leaves those two facts aligned.
                        if request.preserve_navigation {
                            self.show_hidden = self.workspace_snapshot_show_hidden;
                        }
                        if request.exit_after_refresh {
                            // Exit is terminal user intent. Do not trap the user
                            // in Full Mindmap merely because reconciliation
                            // failed; promote the error before removing its
                            // navigator-local error surface.
                            self.error = Some(message);
                            let cleanup =
                                self.finish_full_mindmap_exit(request.return_to_files_after);
                            followup = self.start_pending_ipc_file_open(cleanup);
                        } else if let Some(full) = self.full_mindmap.as_mut() {
                            full.pending_workspace_load = None;
                            full.load_error = Some(message);
                        }
                        if request.preserve_navigation && !request.exit_after_refresh {
                            followup = self.begin_full_mindmap_expanded_folder_loads();
                        }
                    }
                }
                let refresh_toast = if let Some(refresh_id) = refresh_id {
                    self.pending_refresh_full_mindmap_workspace = None;
                    if let Some(refresh) = self.pending_refresh.as_mut() {
                        if refresh.id == refresh_id {
                            refresh.workspace_done = true;
                            refresh.workspace_error = refresh_error;
                        }
                    }
                    self.finish_refresh()
                } else {
                    Task::none()
                };
                self.invalidate_full_mindmap_layout();
                Task::batch([followup, refresh_toast])
            }
            Message::MindmapToggleNode(id) => {
                self.mindmap_selected = Some(id);
                if self.mindmap_panel_open {
                    self.mindmap_panel_shown = Some(id);
                }
                if self.mindmap_collapsed.contains(&id) {
                    self.mindmap_collapsed.remove(&id);
                } else {
                    self.mindmap_collapsed.insert(id);
                }
                self.invalidate_mindmap_layout();
                Task::none()
            }
            Message::MindmapSelectLeaf(id) => {
                self.mindmap_selected = Some(id);
                self.mindmap_panel_shown = Some(id);
                self.mindmap_panel_open = true;
                Task::none()
            }
            Message::MindmapDeselect => {
                self.mindmap_selected = None;
                self.mindmap_panel_shown = None;
                self.mindmap_panel_open = false;
                self.mindmap_panel_drag = None;
                Task::none()
            }
            Message::MindmapNavigate(dir) => {
                let (nodes, _, _) = self.mindmap_layout();
                // Build parent index.
                let mut parents: Vec<Option<usize>> = vec![None; nodes.len()];
                for (i, n) in nodes.iter().enumerate() {
                    for &c in &n.children {
                        parents[c] = Some(i);
                    }
                }
                // Current index: selected blockid, else first heading.
                let cur = self
                    .mindmap_selected
                    .and_then(|id| nodes.iter().position(|n| n.id == Some(id)))
                    .or_else(|| {
                        // No selection: pick root's first child if any.
                        nodes
                            .first()
                            .and_then(|root| root.children.first().copied())
                    });
                let Some(cur_idx) = cur else {
                    if dir == MindmapDir::Left {
                        return self.enter_full_mindmap_for_current_file();
                    }
                    return Task::none();
                };
                // The document root is represented by an unselectable node
                // (`id == None`). Left at that boundary (including the
                // no-selection/root semantic above) hands navigation to the
                // current file's Full Mindmap workspace.
                if dir == MindmapDir::Left
                    && parents[cur_idx].map_or(true, |parent| nodes[parent].id.is_none())
                {
                    return self.enter_full_mindmap_for_current_file();
                }
                let next_idx: Option<usize> = match dir {
                    MindmapDir::Down | MindmapDir::Up => (|| -> Option<usize> {
                        let parent = parents[cur_idx]?;
                        let kids = &nodes[parent].children;
                        let pos = kids.iter().position(|&i| i == cur_idx)?;
                        match dir {
                            MindmapDir::Down => kids.get(pos + 1).copied(),
                            MindmapDir::Up => {
                                if pos == 0 {
                                    None
                                } else {
                                    Some(kids[pos - 1])
                                }
                            }
                            _ => unreachable!(),
                        }
                    })(),
                    MindmapDir::Left => parents[cur_idx].filter(|&p| nodes[p].id.is_some()),
                    MindmapDir::Right => {
                        let n = &nodes[cur_idx];
                        if !n.children.is_empty() {
                            n.children.first().copied()
                        } else if n.has_hidden_children {
                            // First Right on a collapsed node expands it (and
                            // invalidates the layout cache); we keep using the
                            // pre-expand `nodes` for the rest of this handler
                            // and return None, so the SECOND Right press sees
                            // the rebuilt layout's children and descends.
                            if let Some(id) = n.id {
                                self.mindmap_collapsed.remove(&id);
                                self.invalidate_mindmap_layout();
                            }
                            None
                        } else {
                            None
                        }
                    }
                };
                if let Some(idx) = next_idx {
                    if let Some(id) = nodes[idx].id {
                        self.mindmap_selected = Some(id);
                        self.mindmap_panel_open = true;
                        // Debounce the panel rebuild: the selection ring moves
                        // immediately, but the rendered slice only updates once
                        // navigation pauses, so key-repeat doesn't re-shape the
                        // panel content on every press.
                        self.mindmap_panel_settle_gen =
                            self.mindmap_panel_settle_gen.wrapping_add(1);
                        let settle_gen = self.mindmap_panel_settle_gen;
                        return Task::perform(
                            tokio::time::sleep(std::time::Duration::from_millis(
                                MINDMAP_PANEL_SETTLE_MS,
                            )),
                            move |_| Message::MindmapPanelSettle(settle_gen),
                        );
                    }
                }
                Task::none()
            }
            Message::MindmapPanelSettle(settle_gen) => {
                if settle_gen == self.mindmap_panel_settle_gen {
                    self.mindmap_panel_shown = self.mindmap_selected;
                }
                Task::none()
            }
            Message::ToggleMindmapPanel => {
                self.mindmap_panel_open = !self.mindmap_panel_open;
                if !self.mindmap_panel_open {
                    self.mindmap_panel_drag = None;
                } else {
                    // Re-opening shows the current selection without waiting
                    // for a (possibly never-firing) settle timer.
                    self.mindmap_panel_shown = self.mindmap_selected;
                }
                Task::none()
            }
            Message::WindowResized(id, size) => {
                self.window_size = Some(size);
                // The preview viewport height can change without a scroll
                // event (for example when a window is tiled). Re-window and
                // remeasure its retained layout deterministically.
                let preview_geometry = self.refresh_full_mindmap_preview_heights();
                Task::batch([
                    refresh_window_mode_after_native_transition(id),
                    preview_geometry,
                ])
            }
            Message::WindowUnfocused(id) => {
                self.quick_slots_modifier_held = false;
                self.quick_slots_rail_revealed = false;
                self.quick_slots_modifier_generation =
                    self.quick_slots_modifier_generation.wrapping_add(1);
                refresh_window_mode_after_native_transition(id)
            }
            Message::RefreshWindowMode(id) => {
                // AppKit local monitors must be registered after Iced has
                // created the first NSWindow. Registering during `App::new`
                // leaves the macOS event loop alive without ever creating a
                // visible window on current macOS releases.
                crate::native_pinch::install();
                refresh_window_mode_after_native_transition(id)
            }
            Message::RefreshWindowModeSettled(id) => refresh_window_mode(id),
            Message::WindowModeChanged(mode) => {
                self.window_fullscreen = matches!(mode, iced::window::Mode::Fullscreen);
                Task::none()
            }
            Message::TakeScreenshot => {
                self.overlay = Overlay::None;
                let dir = dirs::desktop_dir()
                    .or_else(dirs::home_dir)
                    .unwrap_or_else(|| std::path::PathBuf::from("."));
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let path = dir.join(format!("rmdv-screenshot-{stamp}.png"));
                self.pending_screenshot = Some((path, None));
                iced::window::latest()
                    .and_then(iced::window::screenshot)
                    .map(Message::ScreenshotCaptured)
            }
            Message::ScreenshotCaptured(shot) => {
                if let Some((path, tx)) = self.pending_screenshot.take() {
                    let saved = image::RgbaImage::from_raw(
                        shot.size.width,
                        shot.size.height,
                        shot.rgba.to_vec(),
                    )
                    .ok_or_else(|| "screenshot buffer size mismatch".to_string())
                    .and_then(|img| {
                        img.save(&path)
                            .map_err(|e| format!("write {}: {e}", path.display()))
                    });
                    match tx {
                        // IPC capture: reply over the socket, no toast.
                        Some(tx) => {
                            let resp = match &saved {
                                Ok(()) => crate::ipc::Response::ok_with(
                                    1,
                                    serde_json::json!({
                                        "path": path.to_string_lossy(),
                                        "width": shot.size.width,
                                        "height": shot.size.height,
                                    }),
                                ),
                                Err(e) => crate::ipc::Response::err(1, e.clone()),
                            };
                            Self::reply(&tx, resp);
                            Task::none()
                        }
                        // Palette capture: surface the result as a toast.
                        None => {
                            let msg = match &saved {
                                Ok(()) => format!(
                                    "Saved screenshot to {}",
                                    path.file_name()
                                        .map(|n| n.to_string_lossy().into_owned())
                                        .unwrap_or_else(|| path.to_string_lossy().into_owned())
                                ),
                                Err(e) => format!("Screenshot failed: {e}"),
                            };
                            self.show_toast(msg)
                        }
                    }
                } else {
                    Task::none()
                }
            }
            Message::MindmapCyclePanelWidth => {
                // Open the panel if it was closed so the size change is visible.
                self.mindmap_panel_open = true;
                self.mindmap_panel_drag = None;
                self.mindmap_panel_step = (self.mindmap_panel_step + 1) % MIND_PANEL_FRACS.len();
                self.mindmap_panel_width =
                    mindmap_panel_width_for_step(self.mindmap_panel_step, self.window_size);
                Task::none()
            }
            Message::MindmapToggleSelected => {
                if let Some(id) = self.mindmap_selected {
                    if self.mindmap_collapsed.contains(&id) {
                        self.mindmap_collapsed.remove(&id);
                    } else {
                        self.mindmap_collapsed.insert(id);
                    }
                    self.invalidate_mindmap_layout();
                }
                Task::none()
            }
            Message::MindmapPanelDragStart(_) => {
                self.mindmap_panel_drag = Some((self.mindmap_panel_width, None));
                Task::none()
            }
            Message::MindmapPanelDragMove(cursor_x) => {
                if let Some((orig_w, anchor)) = self.mindmap_panel_drag {
                    match anchor {
                        None => {
                            self.mindmap_panel_drag = Some((orig_w, Some(cursor_x)));
                        }
                        Some(ax) => {
                            self.mindmap_panel_width =
                                mindmap_panel_width_for_drag(orig_w, ax, cursor_x);
                        }
                    }
                }
                Task::none()
            }
            Message::MindmapPanelDragEnd => {
                self.mindmap_panel_drag = None;
                Task::none()
            }
            Message::ToggleMindmapAutocenter => {
                self.mindmap_autocenter = !self.mindmap_autocenter;
                let label = if self.mindmap_autocenter {
                    "Mindmap auto-center: on"
                } else {
                    "Mindmap auto-center: off"
                };
                self.show_toast(label.into())
            }
            Message::EditorAction(action) => {
                let edits = action.is_edit();
                if edits {
                    self.bump_file_refresh_generation();
                }
                if let Some(ed) = self.editor.as_mut() {
                    if edits {
                        let prev = ed.text();
                        if self.edit_history.push_if_changed(prev) {
                            if self.edit_history.len() > 200 {
                                self.edit_history.drop_oldest();
                            }
                            self.edit_redo.clear();
                        }
                    }
                    ed.perform(action);
                    if edits {
                        self.dirty = ed.text() != self.saved_source;
                    }
                }
                Task::none()
            }
            Message::EditorUndo => {
                let mut changed = false;
                if let Some(ed) = self.editor.as_mut() {
                    if let Some(prev) = self.edit_history.pop() {
                        let current = ed.text();
                        self.edit_redo.push(current);
                        *ed = iced::widget::text_editor::Content::with_text(&prev);
                        self.dirty = prev != self.saved_source;
                        changed = true;
                    }
                }
                if changed {
                    self.bump_file_refresh_generation();
                }
                Task::none()
            }
            Message::EditorRedo => {
                let mut changed = false;
                if let Some(ed) = self.editor.as_mut() {
                    if let Some(next) = self.edit_redo.pop() {
                        let current = ed.text();
                        self.edit_history.push(current);
                        *ed = iced::widget::text_editor::Content::with_text(&next);
                        self.dirty = next != self.saved_source;
                        changed = true;
                    }
                }
                if changed {
                    self.bump_file_refresh_generation();
                }
                Task::none()
            }
            Message::SaveFile => {
                let Some(path) = self.file.clone() else {
                    return Task::none();
                };
                // A write can race a refresh read even when the visible text
                // is unchanged. Advance ownership before dispatching it so a
                // pre-save read cannot be accepted after this save settles.
                self.bump_file_refresh_generation();
                let text = match self.editor.as_ref() {
                    Some(ed) => ed.text(),
                    None => self.source.clone(),
                };
                self.source = text.clone();
                self.reparse_source();
                self.dirty = self.source != self.saved_source;
                let saved_source = text.clone();
                let prime = self.prime_diagram_cache();
                Task::batch([
                    Task::perform(
                        async move {
                            tokio::fs::write(&path, text)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        move |result| Message::FileSaved {
                            result,
                            saved_source,
                        },
                    ),
                    prime,
                ])
            }
            Message::FileSaved {
                result: Ok(()),
                saved_source,
            } => {
                self.bump_file_refresh_generation();
                // An older write may finish after a newer save was queued. Only
                // advance the persisted baseline when this is still the source
                // shown by the app; otherwise the newer write owns the state.
                if self.source == saved_source {
                    self.saved_source = saved_source;
                    let current = self
                        .editor
                        .as_ref()
                        .map(|ed| ed.text())
                        .unwrap_or_else(|| self.source.clone());
                    self.dirty = current != self.saved_source;
                }
                self.show_toast("✓ Saved".into())
            }
            Message::FileSaved {
                result: Err(e),
                saved_source,
            } => {
                self.bump_file_refresh_generation();
                // Keep the guard armed if the failed write is still the active
                // document state. A later save may already have superseded it.
                if self.source == saved_source {
                    self.dirty = true;
                }
                self.error = Some(format!("save failed: {e}"));
                Task::none()
            }
            Message::OpenImageZoom(url) => {
                let raster_task = match self.image_cache.get(&url) {
                    Some(ImageState::LoadedSvg {
                        bytes,
                        raster: None,
                        ..
                    }) => {
                        let key = url.clone();
                        let bytes = bytes.clone();
                        Some(Task::perform(
                            async move { rasterize_svg(&bytes) },
                            move |res| Message::SvgRasterized(key.clone(), res),
                        ))
                    }
                    None if url.to_ascii_lowercase().ends_with(".svg") => {
                        // Local svg path not yet in cache; load+raster.
                        let key = url.clone();
                        let path = std::path::PathBuf::from(&url);
                        self.image_cache.insert(url.clone(), ImageState::Loading);
                        Some(Task::perform(
                            async move {
                                let bytes =
                                    tokio::fs::read(&path).await.map_err(|e| e.to_string())?;
                                rasterize_svg(&bytes)
                            },
                            move |res| Message::SvgRasterized(key.clone(), res),
                        ))
                    }
                    _ => None,
                };
                self.zoom_url = Some(url);
                self.zoom_diagram = None;
                self.overlay = Overlay::ImageZoom;
                let restore = self.restore_body_scroll();
                match raster_task {
                    Some(t) => Task::batch([restore, t]),
                    None => restore,
                }
            }
            Message::PickerNavigate(p) => {
                if let Some(pk) = self.picker.as_mut() {
                    if p.is_dir() {
                        pk.navigate_to(p);
                        self.overlay_selected = 0;
                        // Leaf folder (no subfolders, readable): treat the
                        // navigation as a workspace pick. Saves the user an
                        // extra Space/Enter on dead-end folders.
                        if pk.entries.is_empty() && pk.error.is_none() {
                            let cwd = pk.cwd.clone();
                            self.overlay = Overlay::None;
                            self.picker = None;
                            return Task::done(Message::OpenWorkspace(cwd));
                        }
                    }
                }
                Task::none()
            }
            Message::PickerParent => {
                if let Some(pk) = self.picker.as_mut() {
                    pk.parent();
                    self.overlay_selected = 0;
                }
                Task::none()
            }
            Message::PickerHome => {
                if let Some(home) = Picker::home() {
                    if let Some(pk) = self.picker.as_mut() {
                        pk.navigate_to(home);
                    }
                }
                Task::none()
            }
            Message::PickerSelectFolderHere => {
                if let Some(pk) = &self.picker {
                    let p = pk.cwd.clone();
                    return Task::done(Message::OpenWorkspace(p));
                }
                Task::none()
            }
            Message::PickerOpenFile(path) => {
                self.overlay = Overlay::None;
                self.picker = None;
                self.cancel_refresh_tracking();
                if let Some(blocked) = self.block_file_open_if_dirty() {
                    return blocked;
                }
                self.invalidate_pending_watcher_reload();
                let parent = path.parent().map(|p| p.to_path_buf());
                if self.full_mindmap.is_some() {
                    // Deliberate picker activation supersedes an IPC open that
                    // was waiting for Full Mindmap to exit.
                    self.pending_ipc_file_open = None;
                    // Preserve the picker contract without synchronously
                    // indexing the file's parent on the UI thread. The file
                    // read starts only after that bounded index is ready. The
                    // workspace helper owns the single outgoing Full
                    // Mindmap checkpoint at this transition boundary.
                    if let Some(parent) = parent {
                        return self.begin_full_mindmap_workspace_load(
                            parent,
                            false,
                            Some(path),
                            false,
                            false,
                            false,
                        );
                    }
                    // With no parent directory to index, this wrapper owns
                    // the single checkpoint before its guarded file read.
                    return self.begin_full_mindmap_open(path);
                }
                // The picker owns this file load rather than routing through
                // `load_file_unless_dirty`; checkpoint the outgoing slot
                // before the generic load can replace it.
                let checkpoint = self.checkpoint_active_quick_slot();
                let load = self.begin_generic_file_load(path);
                if let Some(parent) = parent {
                    Task::batch([checkpoint, Task::done(Message::OpenWorkspace(parent)), load])
                } else {
                    Task::batch([checkpoint, load])
                }
            }
            Message::OverlayQueryChanged(q) => {
                self.overlay_query = q;
                self.overlay_selected = 0;
                Task::none()
            }
            Message::OverlayMove(d) => {
                let len = match self.overlay {
                    Overlay::FileFinder => self.filtered_files().len(),
                    Overlay::Command => self.filtered_commands().len(),
                    Overlay::ThemePicker => self.filtered_themes().len(),
                    Overlay::FolderPicker => {
                        self.picker.as_ref().map(|p| p.entries.len()).unwrap_or(0)
                    }
                    Overlay::None | Overlay::ImageZoom | Overlay::Shortcuts => 0,
                };
                if len == 0 {
                    return Task::none();
                }
                let next = (self.overlay_selected as isize + d).clamp(0, len as isize - 1);
                self.overlay_selected = next as usize;
                self.scroll_overlay_to_cursor_with_len(len)
            }
            Message::OverlayConfirm => match self.overlay {
                Overlay::FileFinder => {
                    let files = self.filtered_files();
                    if let Some((p, _, _)) = files.get(self.overlay_selected).cloned() {
                        self.overlay = Overlay::None;
                        return if self.full_mindmap.is_some() {
                            self.begin_full_mindmap_open(p)
                        } else {
                            self.load_file_unless_dirty(p)
                        };
                    }
                    Task::none()
                }
                Overlay::Command => {
                    let cmds = self.filtered_commands();
                    if let Some((_, msg, _)) = cmds.get(self.overlay_selected).cloned() {
                        self.overlay = Overlay::None;
                        return Task::done(msg);
                    }
                    Task::none()
                }
                Overlay::ThemePicker => {
                    let themes = self.filtered_themes();
                    if let Some(t) = themes.get(self.overlay_selected).cloned() {
                        self.overlay = Overlay::None;
                        return Task::done(t.message());
                    }
                    Task::none()
                }
                Overlay::FolderPicker => {
                    if let Some(pk) = self.picker.as_ref() {
                        if let Some(e) = pk.entries.get(self.overlay_selected).cloned() {
                            if e.is_dir {
                                self.overlay = Overlay::None;
                                self.picker = None;
                                return Task::done(Message::OpenWorkspace(e.path));
                            } else if e.is_md {
                                return Task::done(Message::PickerOpenFile(e.path));
                            }
                        }
                    }
                    Task::none()
                }
                Overlay::None | Overlay::ImageZoom | Overlay::Shortcuts => Task::none(),
            },
            Message::OverlayDescend => {
                if self.overlay == Overlay::FolderPicker {
                    if let Some(pk) = self.picker.as_mut() {
                        if let Some(e) = pk.entries.get(self.overlay_selected).cloned() {
                            if e.is_dir {
                                pk.navigate_to(e.path);
                                self.overlay_selected = 0;
                                // Leaf folder: auto-open as workspace.
                                if pk.entries.is_empty() && pk.error.is_none() {
                                    let cwd = pk.cwd.clone();
                                    self.overlay = Overlay::None;
                                    self.picker = None;
                                    return Task::done(Message::OpenWorkspace(cwd));
                                }
                                return self.scroll_overlay_to_cursor();
                            } else if e.is_md {
                                return Task::done(Message::PickerOpenFile(e.path));
                            }
                        } else if pk.entries.is_empty() && pk.error.is_none() {
                            let cwd = pk.cwd.clone();
                            self.overlay = Overlay::None;
                            self.picker = None;
                            return Task::done(Message::OpenWorkspace(cwd));
                        }
                    }
                }
                Task::none()
            }
            Message::FileLoadCompleted { generation, result } => {
                if generation != self.file_refresh_generation {
                    // Refresh, save, edit, or a newer generic load superseded
                    // this read before it reached the update loop.
                    return Task::none();
                }
                self.update(Message::FileLoaded(result))
            }
            Message::RefreshFileLoaded { request, result } => {
                let current = self.pending_refresh_file.as_ref() == Some(&request)
                    && self
                        .pending_refresh
                        .as_ref()
                        .is_some_and(|refresh| refresh.id == request.id)
                    && self.file.as_deref() == Some(request.path.as_path());
                if !current {
                    // A newer navigation or refresh owns the current file.
                    // This completion must not overwrite it.
                    return Task::none();
                }
                self.pending_refresh_file = None;
                let skip_reason = if self.dirty {
                    Some(FileRefreshSkipReason::UnsavedEdits)
                } else if request.generation != self.file_refresh_generation {
                    Some(FileRefreshSkipReason::DocumentChanged)
                } else {
                    None
                };
                if let Some(reason) = skip_reason {
                    if let Some(refresh) = self.pending_refresh.as_mut() {
                        refresh.file_done = true;
                        refresh.file_skip_reason = Some(reason);
                    }
                    return self.finish_refresh();
                }
                match result {
                    Ok((path, source)) if path == request.path => {
                        let load = self.apply_loaded_file(path, source);
                        if let Some(refresh) = self.pending_refresh.as_mut() {
                            refresh.file_done = true;
                        }
                        let toast = self.finish_refresh();
                        Task::batch([load, toast])
                    }
                    Ok((path, _)) => {
                        let error = format!("Loaded unexpected file: {}", path.display());
                        self.error = Some(error.clone());
                        if let Some(refresh) = self.pending_refresh.as_mut() {
                            refresh.file_done = true;
                            refresh.file_error = Some(error);
                        }
                        self.finish_refresh()
                    }
                    Err(error) => {
                        self.error = Some(error.clone());
                        if let Some(refresh) = self.pending_refresh.as_mut() {
                            refresh.file_done = true;
                            refresh.file_error = Some(error);
                        }
                        self.finish_refresh()
                    }
                }
            }
            Message::FileLoaded(Ok((path, src))) => {
                if self.dirty {
                    // An IPC/link/vault open can queue navigation before its
                    // asynchronous read returns. Do not let that stale target
                    // affect the next successful open after this one is blocked.
                    self.pending_nav = None;
                    self.invalidate_pending_quick_slot_restore();
                    return self.show_toast(self.unsaved_edits_open_message());
                }
                // Non-refresh loads supersede a refresh transaction. The
                // refresh-owned path calls `apply_loaded_file` directly after
                // validating its own request and therefore keeps its workspace
                // leg alive.
                self.cancel_refresh_tracking();
                self.apply_loaded_file(path, src)
            }
            Message::FileChanged(p) => {
                self.cancel_refresh_tracking();
                self.bump_file_refresh_generation();
                if self.dirty {
                    return self.show_toast("External change ignored (unsaved edits)".into());
                }
                let p = canonicalize_existing_path(p);
                let Some(request) = self.begin_watcher_reload(p.clone()) else {
                    return Task::none();
                };
                Task::perform(load_file(p), move |result| Message::FileChangedLoaded {
                    request,
                    result,
                })
            }
            Message::FileChangedLoaded { request, result } => {
                if !self.watcher_reload_is_current(&request) {
                    return Task::none();
                }
                self.pending_watcher_reload = None;
                if self.dirty {
                    return self.show_toast("External change ignored (unsaved edits)".into());
                }
                match result {
                    Ok((path, source)) if path == request.path => {
                        self.update(Message::FileLoaded(Ok((path, source))))
                    }
                    Ok(_) => Task::none(),
                    Err(error) => self.update(Message::FileLoaded(Err(error))),
                }
            }
            Message::OpenLink(url) => {
                // Split off a `#fragment` suffix (heading anchor).
                let (target, fragment) = match url.split_once('#') {
                    Some((t, f)) => (t, Some(f)),
                    None => (url.as_str(), None),
                };
                // Bare `#fragment`: navigate within the current document.
                if target.is_empty() {
                    let is_tex = is_tex_path(self.file.as_deref());
                    if let Some(line) =
                        fragment.and_then(|f| line_for_fragment(&self.source, f, is_tex))
                    {
                        return Task::done(goto_line_message(line));
                    }
                    return Task::none();
                }
                // Local markdown file: open it in-app, then navigate to the
                // fragment (if any) once it has loaded.
                if !is_external_link(target) {
                    if let Some(path) = resolve_image_path(target, self.file.as_deref()) {
                        let is_md = path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                            e.eq_ignore_ascii_case("md")
                                || e.eq_ignore_ascii_case("markdown")
                                || e.eq_ignore_ascii_case("tex")
                        });
                        if is_md && path.is_file() {
                            if let Some(blocked) = self.block_file_open_if_dirty() {
                                return blocked;
                            }
                            if let Some(f) = fragment {
                                self.pending_nav = Some(PendingNav {
                                    fragment: Some(f.to_string()),
                                    ..Default::default()
                                });
                            }
                            return Task::done(Message::Open(path));
                        }
                    }
                }
                let _ = open::that_detached(&url);
                Task::none()
            }
            Message::FileLoaded(Err(e)) => {
                self.cancel_refresh_tracking();
                self.pending_nav = None;
                self.error = Some(e);
                Task::none()
            }
            Message::ToggleTheme => {
                let (next_id, label, pal, typo) = self.next_theme();
                self.theme_id = next_id.clone();
                if let theme::ThemeId::Preset(p) = next_id {
                    self.theme_preset = p;
                }
                self.palette = pal;
                if let Some(t) = typo {
                    self.set_typography_base(t);
                }
                let changed = self.refresh_diagram_theme_id();
                let preview_geometry = self.refresh_full_mindmap_preview_heights();
                let toast = self.show_toast(label);
                if changed {
                    let preview_assets = self.refresh_full_mindmap_preview_assets_for_theme();
                    Task::batch([
                        toast,
                        preview_geometry,
                        self.prime_diagram_cache(),
                        preview_assets,
                    ])
                } else {
                    Task::batch([toast, preview_geometry])
                }
            }
            Message::SetTheme(t) => {
                self.theme_preset = t;
                self.palette = theme::palette_for(t);
                self.theme_id = theme::ThemeId::Preset(t);
                let changed = self.refresh_diagram_theme_id();
                let preview_geometry = self.refresh_full_mindmap_preview_heights();
                let toast = self.show_toast(t.label().to_string());
                if changed {
                    let preview_assets = self.refresh_full_mindmap_preview_assets_for_theme();
                    Task::batch([
                        toast,
                        preview_geometry,
                        self.prime_diagram_cache(),
                        preview_assets,
                    ])
                } else {
                    Task::batch([toast, preview_geometry])
                }
            }
            Message::SetCustomTheme(slug) => {
                if let Some(t) = self.custom_themes.iter().find(|t| t.slug == slug) {
                    let (palette, typography, label) = (t.palette, t.typography, t.name.clone());
                    self.palette = palette;
                    self.set_typography_base(typography);
                    self.theme_id = theme::ThemeId::Custom(slug.clone());
                    let changed = self.refresh_diagram_theme_id();
                    let preview_geometry = self.refresh_full_mindmap_preview_heights();
                    let toast = self.show_toast(label);
                    if changed {
                        let preview_assets = self.refresh_full_mindmap_preview_assets_for_theme();
                        Task::batch([
                            toast,
                            preview_geometry,
                            self.prime_diagram_cache(),
                            preview_assets,
                        ])
                    } else {
                        Task::batch([toast, preview_geometry])
                    }
                } else {
                    Task::none()
                }
            }
            Message::ReloadThemes => {
                let mut errs = Vec::new();
                let mut combined = crate::theme_load::bundled().clone();
                combined.extend(crate::theme_load::discover(&mut errs));
                combined.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                self.custom_themes = combined;
                if let theme::ThemeId::Custom(slug) = self.theme_id.clone() {
                    if let Some(t) = self.custom_themes.iter().find(|t| t.slug == slug) {
                        let (palette, typography) = (t.palette, t.typography);
                        self.palette = palette;
                        self.set_typography_base(typography);
                    }
                }
                let n = self.custom_themes.len();
                if !errs.is_empty() {
                    self.error = Some(format!("theme load: {}", errs.join("; ")));
                }
                let changed = self.refresh_diagram_theme_id();
                let preview_geometry = self.refresh_full_mindmap_preview_heights();
                let toast =
                    self.show_toast(format!("{n} custom theme{}", if n == 1 { "" } else { "s" }));
                if changed {
                    let preview_assets = self.refresh_full_mindmap_preview_assets_for_theme();
                    Task::batch([
                        toast,
                        preview_geometry,
                        self.prime_diagram_cache(),
                        preview_assets,
                    ])
                } else {
                    Task::batch([toast, preview_geometry])
                }
            }
            Message::ThemeFilesChanged => {
                let mut errs = Vec::new();
                let before = self.custom_themes.len();
                let mut combined = crate::theme_load::bundled().clone();
                combined.extend(crate::theme_load::discover(&mut errs));
                combined.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                self.custom_themes = combined;
                let after = self.custom_themes.len();
                let active_changed = if let theme::ThemeId::Custom(slug) = self.theme_id.clone() {
                    if let Some(t) = self.custom_themes.iter().find(|t| t.slug == slug) {
                        let (palette, typography) = (t.palette, t.typography);
                        self.palette = palette;
                        self.set_typography_base(typography);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };
                if !errs.is_empty() {
                    self.error = Some(format!("theme load: {}", errs.join("; ")));
                }
                let toast = if active_changed {
                    self.show_toast("theme reloaded".to_string())
                } else if before != after {
                    self.show_toast(format!(
                        "{after} custom theme{}",
                        if after == 1 { "" } else { "s" }
                    ))
                } else {
                    Task::none()
                };
                let preview_geometry = active_changed
                    .then(|| self.refresh_full_mindmap_preview_heights())
                    .unwrap_or_else(Task::none);
                if active_changed && self.refresh_diagram_theme_id() {
                    let preview_assets = self.refresh_full_mindmap_preview_assets_for_theme();
                    Task::batch([
                        toast,
                        preview_geometry,
                        self.prime_diagram_cache(),
                        preview_assets,
                    ])
                } else {
                    Task::batch([toast, preview_geometry])
                }
            }
            Message::OpenThemesDir => match crate::theme_load::ensure_themes_dir() {
                Ok(dir) => match open::that_detached(&dir) {
                    Ok(()) => self.show_toast("opened themes folder".to_string()),
                    Err(e) => {
                        self.error = Some(format!("open themes folder: {e}"));
                        Task::none()
                    }
                },
                Err(e) => {
                    self.error = Some(format!("themes folder: {e}"));
                    Task::none()
                }
            },
            Message::ToastExpire(id) => {
                if let Some(t) = &self.toast {
                    if t.id == id {
                        self.toast = None;
                    }
                }
                Task::none()
            }
            Message::InstallCli => {
                let install =
                    Task::perform(crate::cli_install::install(), Message::CliInstallFinished);
                Task::batch([self.show_toast("Installing rmdv CLI…".to_string()), install])
            }
            Message::CliInstallFinished(result) => match result {
                Ok(()) => {
                    let location = crate::cli_install::install_path()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "the user CLI directory".to_string());
                    self.show_toast(format!("rmdv CLI installed in {location}"))
                }
                Err(error) => self.show_toast_with_action(
                    format!("Could not install rmdv CLI: {error}"),
                    Some(ToastAction {
                        label: "Try Again".to_string(),
                        message: Message::InstallCli,
                    }),
                ),
            },
            Message::UpdateAvailable(ready) => {
                self.pending_update = Some(ready);
                Task::none()
            }
            Message::DismissUpdate => {
                self.pending_update = None;
                Task::none()
            }
            Message::InstallUpdate => {
                if let Some(ready) = &self.pending_update {
                    // apply() relaunches + exits on success; only returns on error.
                    if let Err(e) = crate::update::apply(ready) {
                        self.pending_update = None;
                        return self.show_toast(format!("Update failed: {e}"));
                    }
                }
                Task::none()
            }
            Message::ToggleSidebar => {
                self.sidebar_open = !self.sidebar_open;
                self.restore_body_scroll()
            }
            Message::SetSidebarTab(tab) => {
                self.sidebar_tab = tab;
                Task::none()
            }
            Message::ToggleHidden => {
                let full_active = self.full_mindmap.is_some();
                if !full_active {
                    self.cancel_refresh_tracking();
                }
                self.show_hidden = !self.show_hidden;
                let workspace = self.workspace.clone();
                let pending_workspace = self
                    .full_mindmap
                    .as_ref()
                    .and_then(|full| full.pending_workspace_load.clone());
                // Outside Full Mindmap, preserve the existing sidebar behavior.
                // Full Mindmap refreshes its potentially large workspace only
                // through the stale-safe background loader below.
                if !full_active {
                    if let Some(ws) = workspace.as_ref() {
                        match tree::build_workspace(&ws, self.show_hidden) {
                            Ok(snapshot) => {
                                self.workspace_files = snapshot.files;
                                self.workspace_sidebar_files = snapshot.sidebar_files;
                                self.workspace_tree = Some(snapshot.root);
                                self.workspace_snapshot_show_hidden = self.show_hidden;
                                self.workspace_truncated = snapshot.truncated;
                            }
                            Err(error) => {
                                self.error =
                                    Some(format!("Couldn't refresh {}: {error}", ws.display()));
                            }
                        }
                    }
                }
                // If a picker is open, rebuild its view too.
                if let Some(p) = self.picker.as_mut() {
                    p.show_hidden = self.show_hidden;
                    p.refresh();
                }
                if let Some(full) = self.full_mindmap.as_mut() {
                    full.pending_workspace_load = None;
                }
                self.invalidate_full_mindmap_layout();
                let label = if self.show_hidden {
                    "Hidden files: shown".to_string()
                } else {
                    "Hidden files: hidden".to_string()
                };
                let workspace_refresh = if full_active {
                    if let Some(request) = pending_workspace {
                        self.begin_full_mindmap_workspace_load(
                            request.path,
                            request.select_root,
                            request.open_after,
                            request.preserve_navigation,
                            request.return_to_files_after,
                            request.exit_after_refresh,
                        )
                    } else {
                        workspace.map_or_else(Task::none, |path| {
                            self.begin_full_mindmap_workspace_load(
                                path, false, None, true, false, false,
                            )
                        })
                    }
                } else {
                    Task::none()
                };
                Task::batch([workspace_refresh, self.show_toast(label)])
            }
            Message::TreeToggle(p) => {
                if !self.expanded.remove(&p) {
                    self.expanded.insert(p);
                }
                Task::none()
            }
            Message::TreeMove(d) => {
                let Some(root) = &self.workspace_tree else {
                    return Task::none();
                };
                let len =
                    tree::flatten_with_files(root, &self.workspace_sidebar_files, &self.expanded)
                        .len();
                if len == 0 {
                    return Task::none();
                }
                let len_i = len as isize;
                self.tree_cursor = ((self.tree_cursor as isize + d).rem_euclid(len_i)) as usize;
                self.scroll_tree_to_cursor_with_len(len)
            }
            Message::TreeActivate => {
                let Some(root) = &self.workspace_tree else {
                    return Task::none();
                };
                let rows =
                    tree::flatten_with_files(root, &self.workspace_sidebar_files, &self.expanded);
                let Some(r) = rows.get(self.tree_cursor) else {
                    return Task::none();
                };
                if r.node.is_dir() {
                    let p = r.node.path().to_path_buf();
                    if !self.expanded.remove(&p) {
                        self.expanded.insert(p);
                    }
                    Task::none()
                } else {
                    let p = r.node.path().to_path_buf();
                    self.load_file_unless_dirty(p)
                }
            }
            Message::OutlineMove(d) => {
                let len = self.outline_sections.len();
                if len == 0 {
                    return Task::none();
                }
                let len_i = len as isize;
                self.outline_cursor =
                    ((self.outline_cursor as isize + d).rem_euclid(len_i)) as usize;
                self.scroll_outline_to_cursor()
            }
            Message::OutlineActivate => {
                let Some(s) = self.outline_sections.get(self.outline_cursor) else {
                    return Task::none();
                };
                self.scroll_to_line_top(s.line)
            }
            Message::ScrollToLine(line) => self.scroll_to_line_top(line),
            Message::TreeToggleAtCursor => {
                let Some(root) = &self.workspace_tree else {
                    return Task::none();
                };
                let rows =
                    tree::flatten_with_files(root, &self.workspace_sidebar_files, &self.expanded);
                let Some(r) = rows.get(self.tree_cursor) else {
                    return Task::none();
                };
                if r.node.is_dir() {
                    let p = r.node.path().to_path_buf();
                    if !self.expanded.remove(&p) {
                        self.expanded.insert(p);
                    }
                }
                Task::none()
            }
            Message::CopyTreePath => {
                let Some(root) = &self.workspace_tree else {
                    return Task::none();
                };
                let rows =
                    tree::flatten_with_files(root, &self.workspace_sidebar_files, &self.expanded);
                let Some(r) = rows.get(self.tree_cursor) else {
                    return Task::none();
                };
                let path = r.node.path().display().to_string();
                let toast = self.show_toast("Path copied".into());
                Task::batch([iced::clipboard::write::<Message>(path), toast])
            }
            Message::ScrollBy(dy) => iced::widget::operation::scroll_by(
                Self::scroll_id(),
                iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: dy },
            ),
            Message::ScrollToTop => {
                // Pre-position the window so the jump never lands on a spacer.
                // No measure pass: it would anchor against the pre-jump offset.
                let vh = self.body_viewport_h();
                self.virt_window
                    .rebuild(&self.ast, &self.folded, &self.height_cache, 0.0, vh);
                iced::widget::operation::scroll_to(
                    Self::scroll_id(),
                    iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: 0.0 },
                )
            }
            Message::ScrollToBottom => {
                let vh = self.body_viewport_h();
                self.virt_window
                    .rebuild(&self.ast, &self.folded, &self.height_cache, f32::MAX, vh);
                iced::widget::operation::scroll_to(
                    Self::scroll_id(),
                    iced::widget::scrollable::AbsoluteOffset {
                        x: 0.0,
                        y: f32::MAX,
                    },
                )
            }
            Message::ToggleSearch => {
                self.search_open = !self.search_open;
                if !self.search_open {
                    self.query.clear();
                    self.matches.clear();
                    self.match_idx = 0;
                    self.restore_body_scroll()
                } else {
                    Task::batch([
                        iced::widget::operation::focus(Self::search_input_id()),
                        self.restore_body_scroll(),
                    ])
                }
            }
            Message::QueryChanged(q) => {
                self.query = q;
                self.rebuild_matches();
                self.scroll_to_current_match()
            }
            Message::NextMatch => {
                if !self.matches.is_empty() {
                    self.match_idx = (self.match_idx + 1) % self.matches.len();
                }
                self.scroll_to_current_match()
            }
            Message::PrevMatch => {
                if !self.matches.is_empty() {
                    self.match_idx = (self.match_idx + self.matches.len() - 1) % self.matches.len();
                }
                self.scroll_to_current_match()
            }
            Message::TreeScrolled(v) => {
                self.tree_viewport = Some(v);
                self.last_scroll_at = Some(std::time::Instant::now());
                Task::none()
            }
            Message::OutlineScrolled(v) => {
                self.outline_viewport = Some(v);
                self.last_scroll_at = Some(std::time::Instant::now());
                Task::none()
            }
            Message::OverlayScrolled(v) => {
                self.overlay_viewport = Some(v);
                self.last_scroll_at = Some(std::time::Instant::now());
                Task::none()
            }
            Message::VaultScrolled(v) => {
                self.vault_viewport = Some(v);
                self.last_scroll_at = Some(std::time::Instant::now());
                Task::none()
            }
            Message::FullMindmapPreviewScrolled {
                namespace,
                path,
                identity,
                viewport: v,
            } => {
                let valid_identity = self.full_mindmap.as_ref().is_some_and(|full| {
                    let (expected_namespace, expected_path, expected_id) =
                        full_mindmap_preview_scroll_tag(full);
                    expected_namespace == namespace
                        && expected_id == identity
                        && expected_path == path
                });
                if !valid_identity {
                    // A late scroll event from the previous file/Full Mindmap
                    // instance must not seed this preview's viewport offset.
                    return Task::none();
                }
                let bounds_changed = self.full_mindmap.as_ref().is_some_and(|full| {
                    full.preview_viewport
                        .as_ref()
                        .is_some_and(|previous| previous.bounds().size() != v.bounds().size())
                });
                if bounds_changed {
                    if let Some(full) = self.full_mindmap.as_mut() {
                        full.preview_generation = full.preview_generation.wrapping_add(1);
                        full.preview_height_cache.clear();
                        // A viewport-bounds event is itself a geometry
                        // invalidation; do not let sparse corrections from
                        // the old width/height distort the new prefix.
                        full.preview_window.clear_height_adjustments();
                        full.preview_measurement_pending = false;
                        full.preview_measurement_range = None;
                        full.preview_measurement_generation = None;
                    }
                }
                if let Some(full) = self.full_mindmap.as_mut() {
                    full.preview_viewport = Some(v);
                }
                let preview_restore = self.take_current_quick_slot_preview_restore();
                self.last_scroll_at = Some(std::time::Instant::now());
                let checkpoint = if preview_restore.is_some() {
                    Task::none()
                } else {
                    self.checkpoint_active_quick_slot()
                };
                let needs_rebuild = self.full_mindmap.as_ref().is_some_and(|full| {
                    matches!(full.preview, FullMindmapPreview::Document { .. })
                        && (bounds_changed
                            || full
                                .preview_window
                                .needs_rebuild(Self::full_mindmap_preview_body_offset(full)))
                });
                if !needs_rebuild {
                    let restore = preview_restore.map_or_else(Task::none, |position| {
                        self.quick_slot_restore_preview_position(position)
                    });
                    return Task::batch([checkpoint, restore]);
                }
                self.rebuild_full_mindmap_preview_here();
                let restore = preview_restore.map_or_else(Task::none, |position| {
                    self.quick_slot_restore_preview_position(position)
                });
                Task::batch([
                    checkpoint,
                    self.prime_full_mindmap_preview_assets(),
                    self.measure_full_mindmap_preview_heights(),
                    restore,
                ])
            }
            Message::BodyScrolled(v) => {
                // A width change reflows text, invalidating measured heights;
                // a height change alters the window padding. Either way the
                // window must be rebuilt around the (possibly new) offset.
                let bounds_changed = self
                    .body_viewport
                    .as_ref()
                    .is_some_and(|p| p.bounds().size() != v.bounds().size());
                if bounds_changed {
                    self.height_cache.clear();
                }
                self.body_viewport = Some(v);
                self.last_scroll_at = Some(std::time::Instant::now());
                let checkpoint = if self.quick_slot_body_restore.is_some() {
                    Task::none()
                } else {
                    self.checkpoint_active_quick_slot()
                };
                let restore = self.quick_slot_body_restore.and_then(|position| {
                    let max = self
                        .body_viewport
                        .as_ref()
                        .map(|viewport| {
                            (viewport.content_bounds().height - viewport.bounds().height).max(0.0)
                        })
                        .unwrap_or(0.0);
                    if max > 0.0 {
                        self.quick_slot_body_restore = None;
                        Some(self.quick_slot_restore_position(position))
                    } else {
                        None
                    }
                });
                let offset = self.body_offset();
                let anchor = self.nav_anchor.take();
                if bounds_changed || self.virt_window.needs_rebuild(offset) {
                    // A scroll event during an in-flight nav jump comes from
                    // the estimate snap; keep the target materialized for the
                    // precise scroll op instead of windowing the raw offset.
                    match anchor {
                        Some(idx) => self.rebuild_virt_around_block(idx),
                        None => self.rebuild_virt_here(),
                    }
                    return Task::batch([
                        checkpoint,
                        restore.unwrap_or_else(Task::none),
                        self.measure_window_heights(),
                    ]);
                }
                Task::batch([checkpoint, restore.unwrap_or_else(Task::none)])
            }
            Message::TableScrolled => {
                self.last_scroll_at = Some(std::time::Instant::now());
                Task::none()
            }
            Message::CopyCode(s) => {
                let toast = self.show_toast("Copied".into());
                Task::batch([iced::clipboard::write::<Message>(s), toast])
            }
            Message::DiagramZoom(hash) => {
                // Only zoom Ready diagrams. We have to scan all theme_id keys
                // because cache may hold a stale entry under an old theme_id;
                // we want the one matching the current palette.
                let key = (hash, self.diagram_theme_id);
                let handle = match self.diagram_cache.peek(&key) {
                    Some(crate::diagram::DiagramState::Ready { inline, .. }) => {
                        Some(inline.clone())
                    }
                    _ => None,
                };
                if let Some(h) = handle {
                    self.zoom_diagram = Some(h);
                    self.zoom_url = None;
                    self.overlay = Overlay::ImageZoom;
                    self.restore_body_scroll()
                } else {
                    Task::none()
                }
            }
            Message::CopyDiagramSource(hash) => {
                let src = self.ast.iter().find_map(|(_, b)| match b {
                    Block::Diagram {
                        hash: h, source, ..
                    } if *h == hash => Some(source.clone()),
                    _ => None,
                });
                match src {
                    Some(s) => {
                        let toast = self.show_toast("Copied".into());
                        Task::batch([iced::clipboard::write::<Message>(s), toast])
                    }
                    None => Task::none(),
                }
            }
            Message::DiagramRendered {
                hash,
                theme_id,
                result,
            } => {
                let key = (hash, theme_id);
                // Legacy document diagram completions have no request
                // identity. Keep a Full Mindmap preview's Pending/Ready
                // ownership authoritative for a shared cache key; the
                // preview completion will also refresh its own measurements.
                let preview_pending = self
                    .full_mindmap
                    .as_ref()
                    .is_some_and(|full| full.preview_pending_diagrams.contains_key(&key));
                let preview_has_result = self.full_mindmap.as_ref().is_some_and(|full| {
                    full.preview_asset_diagrams.contains(&key)
                        && self.diagram_cache.peek(&key).is_some_and(|state| {
                            !matches!(state, crate::diagram::DiagramState::Pending)
                        })
                });
                if preview_pending || preview_has_result {
                    return Task::none();
                }
                // Drop stale results — theme changed mid-render, or AST
                // re-parsed away the source block.
                if theme_id != self.diagram_theme_id {
                    return Task::none();
                }
                let still_present = diagram_hash_present(&self.ast, hash);
                if !still_present {
                    return Task::none();
                }
                let state = match result {
                    Ok(out) => {
                        let crate::diagram::RenderOutput { svg, rgba, w, h } = out;
                        let inline = iced::widget::image::Handle::from_rgba(w, h, rgba);
                        crate::diagram::DiagramState::Ready {
                            inline,
                            source_bytes: std::sync::Arc::new(svg),
                            device_w: w,
                        }
                    }
                    Err(msg) => crate::diagram::DiagramState::Err(msg),
                };
                self.diagram_cache.put(key, state);
                // A Ready diagram replaces its faded-source placeholder with
                // an image of a different height — refresh measured heights.
                self.measure_window_heights()
            }
            Message::FullMindmapPreviewDiagramRendered {
                identity,
                range,
                wave,
                hash,
                theme_id,
                result,
            } => {
                let asset_identity = FullMindmapPreviewAssetIdentity {
                    preview: identity.clone(),
                    range,
                    wave,
                };
                let valid = self.full_mindmap.as_ref().is_some_and(|full| {
                    full.preview_namespace == identity.namespace
                        && full.preview_identity.as_ref() == Some(&identity.request)
                        && matches!(
                            &full.preview,
                            FullMindmapPreview::Document { path, .. }
                                if path == &identity.request.path
                        )
                        && full.preview_window.range == range
                        && full.preview_asset_wave_id == wave
                        && self.diagram_theme_id == theme_id
                });
                let key = (hash, theme_id);
                let owns_pending = self
                    .full_mindmap
                    .as_ref()
                    .and_then(|full| full.preview_pending_diagrams.get(&key))
                    .is_some_and(|owner| owner == &asset_identity);
                if owns_pending {
                    if let Some(full) = self.full_mindmap.as_mut() {
                        full.preview_pending_diagrams.remove(&key);
                    }
                }
                if !valid {
                    if owns_pending
                        && matches!(
                            self.diagram_cache.peek(&key),
                            Some(crate::diagram::DiagramState::Pending)
                        )
                    {
                        self.diagram_cache.remove(&key);
                    }
                    return Task::none();
                }
                // A duplicate/current-identity completion that no longer
                // owns the Pending sentinel cannot overwrite a newer Ready
                // or Err result in the shared cache.
                if !owns_pending {
                    return Task::none();
                }
                let state = match result {
                    Ok(out) => {
                        let crate::diagram::RenderOutput { svg, rgba, w, h } = out;
                        crate::diagram::DiagramState::Ready {
                            inline: iced::widget::image::Handle::from_rgba(w, h, rgba),
                            source_bytes: std::sync::Arc::new(svg),
                            device_w: w,
                        }
                    }
                    Err(msg) => crate::diagram::DiagramState::Err(msg),
                };
                self.diagram_cache.put(key, state);
                // A shared cache result can affect both panels. Keep the
                // preview lane's identity check above, then refresh whichever
                // document layout is currently visible as well.
                let assets = self.prime_full_mindmap_preview_assets();
                Task::batch([
                    assets,
                    self.measure_full_mindmap_preview_heights(),
                    self.measure_window_heights(),
                ])
            }
            Message::SidebarDragStart => {
                self.sidebar_drag = Some(self.sidebar_width);
                Task::none()
            }
            Message::SidebarDragMove(x) => {
                if self.sidebar_drag.is_some() {
                    self.sidebar_width = x.clamp(SIDEBAR_MIN, SIDEBAR_MAX);
                }
                Task::none()
            }
            Message::SidebarDragEnd => {
                self.sidebar_drag = None;
                Task::none()
            }
            Message::ScrollerTick => {
                if let Some(t) = self.last_scroll_at {
                    if t.elapsed() >= std::time::Duration::from_millis(SCROLLER_FADE_MS) {
                        self.last_scroll_at = None;
                    }
                }
                Task::none()
            }
            Message::RestoreBodySnap(y) => iced::widget::operation::snap_to(
                Self::scroll_id(),
                iced::widget::scrollable::RelativeOffset { x: 0.0, y },
            ),
            Message::RestoreBodyScroll(y) => iced::widget::operation::scroll_to(
                Self::scroll_id(),
                iced::widget::scrollable::AbsoluteOffset { x: 0.0, y },
            ),
            Message::BlockHeightsMeasured(measured, at_offset) => {
                let body_off = self.body_offset();
                // Compensation is anchored to the offset the measurement was
                // dispatched at; if the viewport moved since (nav jump, user
                // scroll), a scroll_by would fight that movement — skip it.
                let offset_stable = (body_off - at_offset).abs() <= 1.0;
                let (s, e) = self.virt_window.range;
                let mut by_id: HashMap<crate::ast::BlockId, usize> = HashMap::new();
                for (k, &i) in self.virt_window.display
                    [s.min(self.virt_window.display.len())..e.min(self.virt_window.display.len())]
                    .iter()
                    .enumerate()
                {
                    if let Some((bid, _)) = self.ast.get(i) {
                        by_id.insert(*bid, s + k);
                    }
                }
                let mut delta_above = 0.0f32;
                let mut any = false;
                for (bid, h) in measured {
                    let Some(&dpos) = by_id.get(&bid) else {
                        continue;
                    };
                    let old = self.virt_window.block_height(dpos);
                    if (h - old).abs() <= 0.5 {
                        continue;
                    }
                    any = true;
                    // Estimate error in blocks fully above the viewport shifts
                    // everything on screen once corrected; track it so the
                    // offset can be compensated (scroll anchoring).
                    if self.virt_window.block_top(dpos) + old <= body_off {
                        delta_above += h - old;
                    }
                    self.height_cache.set_measured(bid, h);
                }
                if !any {
                    return Task::none();
                }
                self.rebuild_virt_here();
                if offset_stable && delta_above.abs() > 0.5 {
                    return iced::widget::operation::scroll_by(
                        Self::scroll_id(),
                        iced::widget::scrollable::AbsoluteOffset {
                            x: 0.0,
                            y: delta_above,
                        },
                    );
                }
                Task::none()
            }
            Message::FullMindmapPreviewBlockHeightsMeasured {
                path,
                namespace,
                identity,
                generation,
                measured,
                at_offset,
            } => {
                let valid = self.full_mindmap.as_ref().is_some_and(|full| {
                    full.preview_namespace == namespace
                        && full
                            .preview_identity
                            .as_ref()
                            .is_some_and(|request| request.id == identity && request.path == path)
                        && full.preview_generation == generation
                        && matches!(
                            full.selected.as_ref(),
                            Some(WorkspaceNodeId::File(selected)) if selected == &path
                        )
                        && matches!(
                            &full.preview,
                            FullMindmapPreview::Document { path: preview_path, .. }
                                if preview_path == &path
                        )
                });
                if !valid {
                    // Geometry/theme changes invalidate an operation without
                    // invalidating the preview identity. Only the exact
                    // request+generation that owns the coalescing slot may
                    // release it. In particular, an old g1 result cannot
                    // clear a newer g2 operation that has already replaced
                    // the slot.
                    let owns_slot = self.full_mindmap.as_ref().is_some_and(|full| {
                        full.preview_measurement_pending
                            && full.preview_measurement_generation == Some(generation)
                            && full.preview_namespace == namespace
                            && full.preview_identity.as_ref().is_some_and(|request| {
                                request.id == identity && request.path == path
                            })
                    });
                    if owns_slot {
                        if let Some(full) = self.full_mindmap.as_mut() {
                            full.preview_measurement_pending = false;
                            full.preview_measurement_range = None;
                            full.preview_measurement_generation = None;
                        }
                        self.rebuild_full_mindmap_preview_here();
                        return Task::batch([
                            self.prime_full_mindmap_preview_assets(),
                            self.measure_full_mindmap_preview_heights(),
                        ]);
                    }
                    // The panel was reset or a newer file owns the viewport;
                    // this result must not touch either preview state.
                    return Task::none();
                }
                if let Some(full) = self.full_mindmap.as_mut() {
                    full.preview_measurement_pending = false;
                    full.preview_measurement_generation = None;
                }
                let body_off = self
                    .full_mindmap
                    .as_ref()
                    .map(Self::full_mindmap_preview_body_offset)
                    .unwrap_or(0.0);
                let offset_stable = (body_off - at_offset).abs() <= 1.0;
                let mut delta_above = 0.0f32;
                let mut any = false;
                let mut range_changed = false;
                if let Some(full) = self.full_mindmap.as_mut() {
                    range_changed =
                        full.preview_measurement_range.take() != Some(full.preview_window.range);
                    let (start, end) = full.preview_window.range;
                    let display = full.preview_window.display.clone();
                    let blocks = match &full.preview {
                        FullMindmapPreview::Document { blocks, .. } => blocks,
                        _ => return Task::none(),
                    };
                    let mut by_id: HashMap<crate::ast::BlockId, usize> = HashMap::new();
                    for (offset, &idx) in display[start.min(display.len())..end.min(display.len())]
                        .iter()
                        .enumerate()
                    {
                        if let Some((id, _)) = blocks.get(idx) {
                            by_id.insert(*id, start + offset);
                        }
                    }
                    for (id, height) in measured {
                        let Some(&dpos) = by_id.get(&id) else {
                            continue;
                        };
                        if !height.is_finite() || height <= 0.0 {
                            continue;
                        }
                        let old = full.preview_window.block_height(dpos);
                        // Record every finite layout result even when it is
                        // already close to the estimate; otherwise an
                        // accurate estimate is repeatedly redispatched after
                        // each window rebuild.
                        full.preview_height_cache.set_measured(id, height);
                        if (height - old).abs() <= 0.5 {
                            continue;
                        }
                        any = true;
                        if full.preview_window.block_top(dpos) + old <= body_off {
                            delta_above += height - old;
                        }
                        full.preview_window.apply_height_delta(dpos, height - old);
                    }
                }
                if !any {
                    return if range_changed {
                        // The operation completed for an older materialized
                        // range while the viewport moved. Re-measure the
                        // current range once; the pending guard prevents a
                        // completion loop when the widget tree has no IDs.
                        Task::batch([
                            self.prime_full_mindmap_preview_assets(),
                            self.measure_full_mindmap_preview_heights(),
                        ])
                    } else {
                        Task::none()
                    };
                }
                self.rebuild_full_mindmap_preview_here();
                let assets = self.prime_full_mindmap_preview_assets();
                let followup = if range_changed {
                    self.measure_full_mindmap_preview_heights()
                } else {
                    Task::none()
                };
                if offset_stable && delta_above.abs() > 0.5 {
                    let scroll = iced::widget::operation::scroll_by(
                        Self::full_mindmap_preview_scroll_id(),
                        iced::widget::scrollable::AbsoluteOffset {
                            x: 0.0,
                            y: delta_above,
                        },
                    );
                    return Task::batch([assets, scroll, followup]);
                }
                Task::batch([assets, followup])
            }
            Message::Noop => Task::none(),
            Message::ToggleAutoFocusOnNav => {
                self.prefs.auto_focus_on_nav = !self.prefs.auto_focus_on_nav;
                crate::prefs::save(&self.prefs);
                let state = if self.prefs.auto_focus_on_nav {
                    "on"
                } else {
                    "off"
                };
                return self.show_toast(format!("Auto-focus on agent nav: {state}"));
            }
            Message::Ipc(req, tx) => self.handle_ipc(req, tx),
        }
    }
}
