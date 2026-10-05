//! Quick Slots: per-workspace slot persistence, activation, checkpointing, and restore.

use super::*;

impl App {
    pub(super) fn quick_slots_workspace_root(&self) -> Option<&std::path::Path> {
        self.workspace
            .as_deref()
            .or(self.quick_slots_root.as_deref())
    }

    pub(super) fn load_quick_slots_for_workspace(&mut self, root: &std::path::Path) {
        let same = self.quick_slots_root.as_ref().is_some_and(|current| {
            crate::quick_slots::workspace_key(current) == crate::quick_slots::workspace_key(root)
        });
        if !same {
            self.invalidate_pending_quick_slot_restore();
            self.persist_quick_slots_now();
            self.quick_slots = self.prefs.quick_slots.bank(root);
            self.quick_slots_root = Some(root.to_path_buf());
            self.quick_slots_undo = None;
            self.quick_slots_persist_pending = false;
        }
    }

    pub(super) fn persist_quick_slots_now(&mut self) {
        let Some(root) = self.quick_slots_root.clone() else {
            return;
        };
        self.quick_slots_persist_pending = false;
        self.prefs
            .quick_slots
            .put_bank(&root, self.quick_slots.clone());
        self.save_prefs();
    }

    pub(super) fn invalidate_pending_quick_slot_restore(&mut self) {
        self.quick_slot_activation_generation =
            self.quick_slot_activation_generation.wrapping_add(1);
        self.invalidate_pending_watcher_reload();
        self.finish_pending_quick_slot_restore();
        self.quick_slot_body_restore = None;
        self.quick_slot_preview_restore = None;
    }

    pub(super) fn finish_pending_quick_slot_restore(&mut self) {
        self.pending_quick_slot_restore = None;
        self.quick_slot_preview_restore_guard = None;
    }

    pub(super) fn begin_pending_quick_slot_restore(
        &mut self,
        index: usize,
        slot: crate::quick_slots::QuickSlot,
    ) -> Option<PendingQuickSlotRestore> {
        let root_key = crate::quick_slots::workspace_key(self.quick_slots_workspace_root()?);
        self.quick_slot_activation_generation =
            self.quick_slot_activation_generation.wrapping_add(1);
        self.invalidate_pending_watcher_reload();
        self.quick_slot_preview_restore_guard = None;
        self.quick_slot_body_restore = None;
        self.quick_slot_preview_restore = None;
        let pending = PendingQuickSlotRestore {
            generation: self.quick_slot_activation_generation,
            index,
            slot,
            root_key,
        };
        self.pending_quick_slot_restore = Some(pending.clone());
        Some(pending)
    }

    pub(super) fn quick_slot_restore_is_current(&self, pending: &PendingQuickSlotRestore) -> bool {
        self.quick_slot_activation_generation == pending.generation
            && self.quick_slots.active == Some(pending.index)
            && self
                .quick_slots_workspace_root()
                .is_some_and(|root| crate::quick_slots::workspace_key(root) == pending.root_key)
            && self.quick_slots.occupied(pending.index) == Some(&pending.slot)
    }

    pub(super) fn take_current_quick_slot_preview_restore(&mut self) -> Option<f32> {
        let Some(guard) = self.quick_slot_preview_restore_guard.clone() else {
            self.quick_slot_preview_restore = None;
            return None;
        };
        if !self.quick_slot_restore_is_current(&guard) {
            self.invalidate_pending_quick_slot_restore();
            return None;
        }
        self.quick_slot_preview_restore_guard = None;
        self.quick_slot_preview_restore.take()
    }

    /// Debounce context checkpoints so rapid scrolling owns at most one
    /// sleeping task and one follow-up write. The in-memory slot always holds
    /// the latest accepted position even while persistence is pending.
    pub(super) fn schedule_quick_slots_persist(&mut self) -> Task<Message> {
        self.quick_slots_persist_generation = self.quick_slots_persist_generation.wrapping_add(1);
        if self.quick_slots_persist_pending {
            return Task::none();
        }
        self.quick_slots_persist_pending = true;
        let generation = self.quick_slots_persist_generation;
        Task::perform(
            async move {
                tokio::time::sleep(std::time::Duration::from_millis(220)).await;
                generation
            },
            Message::QuickSlotsPersist,
        )
    }

    pub(super) fn slot_position(viewport: Option<&iced::widget::scrollable::Viewport>) -> f32 {
        let Some(viewport) = viewport else {
            return 0.0;
        };
        let max = (viewport.content_bounds().height - viewport.bounds().height).max(0.0);
        if max <= 0.0 {
            return 0.0;
        }
        crate::quick_slots::normalize_position(viewport.absolute_offset().y / max)
    }

    /// Capture only a reading surface. Raw/Zen intentionally maps to the last
    /// persisted reader surface (Rendered), never to unsaved editor text.
    pub(super) fn current_quick_slot_context(
        &self,
    ) -> Option<(PathBuf, crate::quick_slots::SlotContext)> {
        let root = self.quick_slots_workspace_root()?;
        if self.full_mindmap.is_some() {
            let path = self.full_mindmap_selected_file()?;
            self.quick_slot_relative_path(root, &path)?;
            return Some((
                path,
                crate::quick_slots::SlotContext {
                    mode: crate::quick_slots::SlotMode::FullMindmap,
                    preview_position: Self::slot_position(
                        self.full_mindmap
                            .as_ref()
                            .and_then(|full| full.preview_viewport.as_ref()),
                    ),
                    ..Default::default()
                },
            ));
        }
        let path = self.file.clone()?;
        self.quick_slot_relative_path(root, &path)?;
        let mode = if self.view_mode == ViewMode::Mindmap {
            crate::quick_slots::SlotMode::DocumentMindmap
        } else {
            crate::quick_slots::SlotMode::Rendered
        };
        Some((
            path,
            crate::quick_slots::SlotContext {
                mode,
                body_position: Self::slot_position(self.body_viewport.as_ref()),
                mindmap_selection: self.mindmap_selected.map(|id| id.0),
                mindmap_panel_open: self.mindmap_panel_open,
                ..Default::default()
            },
        ))
    }

    pub(super) fn quick_slot_relative_path(&self, root: &Path, path: &Path) -> Option<String> {
        let mut memo = self.quick_slot_relative_memo.borrow_mut();
        if let Some((memo_root, memo_path, relative)) = memo.as_ref() {
            if memo_root == root && memo_path == path {
                return relative.clone();
            }
        }
        let relative = crate::quick_slots::relative_path(root, path);
        *memo = Some((root.to_path_buf(), path.to_path_buf(), relative.clone()));
        relative
    }

    pub(super) fn quick_slot_activation_is_current(
        &self,
        index: usize,
        slot: &crate::quick_slots::QuickSlot,
        path: &std::path::Path,
    ) -> bool {
        if self.quick_slots.active != Some(index) || self.editor.is_some() {
            return false;
        }
        let path = canonicalize_existing_path(path.to_path_buf());
        let same_path = |current: Option<PathBuf>| {
            current.is_some_and(|current| canonicalize_existing_path(current) == path)
        };
        match slot.context.mode {
            crate::quick_slots::SlotMode::FullMindmap => {
                self.full_mindmap.is_some() && same_path(self.full_mindmap_selected_file())
            }
            crate::quick_slots::SlotMode::DocumentMindmap => {
                self.full_mindmap.is_none()
                    && self.view_mode == ViewMode::Mindmap
                    && same_path(self.file.clone())
            }
            crate::quick_slots::SlotMode::Rendered => {
                self.full_mindmap.is_none()
                    && self.view_mode == ViewMode::Rendered
                    && same_path(self.file.clone())
            }
        }
    }

    pub(super) fn checkpoint_active_quick_slot(&mut self) -> Task<Message> {
        let Some(index) = self.quick_slots.active else {
            return Task::none();
        };
        // Entering Full Mindmap from a document preserves the active slot's
        // Rendered/DocumentMindmap context. Only a slot that was originally
        // created for Full Mindmap may be checkpointed from the navigator;
        // otherwise selecting a parent folder would silently rewrite a file
        // slot so its next activation stayed in Full Mindmap.
        if self.full_mindmap.is_some()
            && self
                .quick_slots
                .occupied(index)
                .is_some_and(|slot| slot.context.mode != crate::quick_slots::SlotMode::FullMindmap)
        {
            return Task::none();
        }
        let Some((path, context)) = self.current_quick_slot_context() else {
            return Task::none();
        };
        let Some(root) = self.quick_slots_workspace_root() else {
            return Task::none();
        };
        let Some(relative) = self.quick_slot_relative_path(root, &path) else {
            return Task::none();
        };
        let owns = self
            .quick_slots
            .occupied(index)
            .is_some_and(|slot| slot.relative_path == relative);
        if !owns {
            return Task::none();
        }
        if let Some(slot) = self.quick_slots.occupied_mut(index) {
            slot.context = context.normalized();
        }
        self.schedule_quick_slots_persist()
    }

    /// Manual navigation that leaves the active slot's file has no current
    /// target to keep active. Invalidate older debounced writes and persist the
    /// cleared marker immediately so a late completion cannot resurrect it.
    pub(super) fn clear_active_quick_slot(&mut self) {
        if self.quick_slots.active.is_none() {
            return;
        }
        self.quick_slots.active = None;
        self.quick_slots_persist_generation = self.quick_slots_persist_generation.wrapping_add(1);
        self.persist_quick_slots_now();
    }

    pub(super) fn quick_slot_path(
        &self,
        index: usize,
    ) -> Option<(crate::quick_slots::QuickSlot, PathBuf)> {
        let slot = self.quick_slots.occupied(index)?.clone();
        let root = self.quick_slots_workspace_root()?;
        let path = crate::quick_slots::resolve_path(root, &slot.relative_path)?;
        Some((slot, path))
    }

    pub(super) fn quick_slot_restore_position(&self, position: f32) -> Task<Message> {
        let Some(viewport) = self.body_viewport.as_ref() else {
            return Task::none();
        };
        let max = (viewport.content_bounds().height - viewport.bounds().height).max(0.0);
        if max <= 0.0 {
            return Task::none();
        }
        iced::widget::operation::scroll_to(
            Self::scroll_id(),
            iced::widget::scrollable::AbsoluteOffset {
                x: 0.0,
                y: (crate::quick_slots::normalize_position(position) * max).max(0.0),
            },
        )
    }

    pub(super) fn quick_slot_restore_preview_position(&self, position: f32) -> Task<Message> {
        let Some(viewport) = self
            .full_mindmap
            .as_ref()
            .and_then(|full| full.preview_viewport.as_ref())
        else {
            return Task::none();
        };
        let max = (viewport.content_bounds().height - viewport.bounds().height).max(0.0);
        iced::widget::operation::scroll_to(
            Self::full_mindmap_preview_scroll_id(),
            iced::widget::scrollable::AbsoluteOffset {
                x: 0.0,
                y: crate::quick_slots::normalize_position(position) * max,
            },
        )
    }

    pub(super) fn apply_quick_slot_restore_after_file(
        &mut self,
        path: &std::path::Path,
        slot: crate::quick_slots::QuickSlot,
    ) -> Task<Message> {
        self.view_mode = match slot.context.mode {
            crate::quick_slots::SlotMode::DocumentMindmap => ViewMode::Mindmap,
            crate::quick_slots::SlotMode::Rendered | crate::quick_slots::SlotMode::FullMindmap => {
                ViewMode::Rendered
            }
        };
        self.editor = None;
        self.editor_text = None;
        self.zen_restore = None;
        self.mindmap_selected = slot
            .context
            .mindmap_selection
            .map(crate::ast::BlockId)
            .filter(|id| self.ast.iter().any(|(block_id, _)| block_id == id));
        self.mindmap_panel_shown = self.mindmap_selected;
        self.mindmap_panel_open = slot.context.mindmap_panel_open;
        self.finish_pending_quick_slot_restore();
        self.quick_slot_body_restore = Some(slot.context.body_position);
        if path == self.file.as_deref().unwrap_or(path) {
            let max = self
                .body_viewport
                .as_ref()
                .map(|viewport| {
                    (viewport.content_bounds().height - viewport.bounds().height).max(0.0)
                })
                .unwrap_or(0.0);
            if max > 0.0 {
                self.quick_slot_body_restore = None;
                return self.quick_slot_restore_position(slot.context.body_position);
            }
        }
        Task::none()
    }

    pub(super) fn begin_quick_slot_activation(&mut self, index: usize) -> Task<Message> {
        let Some(slot) = self.quick_slots.occupied(index).cloned() else {
            return self.show_toast(format!("Quick Slot {} is empty", index + 1));
        };
        let Some(root) = self.quick_slots_workspace_root() else {
            return self.show_toast(format!("Quick Slot {} is missing", index + 1));
        };
        let Some(path) = crate::quick_slots::resolve_path(root, &slot.relative_path) else {
            if self.dirty {
                return self.show_toast(self.unsaved_edits_open_message());
            }
            self.invalidate_pending_quick_slot_restore();
            self.quick_slots.active = Some(index);
            self.persist_quick_slots_now();
            return self.show_toast(format!(
                "Quick Slot {} is missing: {}",
                index + 1,
                slot.relative_path
            ));
        };
        // A clean Zen editor still needs the active slot chord/button to leave
        // the editing surface and restore its persisted reading mode. The
        // active marker can also survive manual navigation, so suppress a
        // duplicate only when the current file and reading surface match the
        // saved target (using canonical paths for aliases).
        if path.is_file() && self.quick_slot_activation_is_current(index, &slot, &path) {
            return Task::none();
        }
        if self.dirty {
            return self.show_toast(self.unsaved_edits_open_message());
        }
        if !path.is_file() {
            self.invalidate_pending_quick_slot_restore();
            self.quick_slots.active = Some(index);
            self.persist_quick_slots_now();
            return self.show_toast(format!(
                "Quick Slot {} is missing: {}",
                index + 1,
                slot.relative_path
            ));
        }
        // Do not checkpoint the current UI back into the target when the
        // active slot is being reactivated. In particular, clean Zen maps to
        // `Rendered`, which would overwrite a same-slot Mindmap context
        // before its saved target is restored. Switching to another slot
        // still checkpoints the outgoing slot.
        let checkpoint = if self.quick_slots.active == Some(index) {
            Task::none()
        } else {
            self.checkpoint_active_quick_slot()
        };
        self.quick_slots.active = Some(index);
        // Persist the active marker immediately; ordinary scroll checkpoints
        // remain debounced, but a restart should reopen the selected bank
        // state even if no viewport event arrives before exit.
        self.persist_quick_slots_now();
        match slot.context.mode {
            crate::quick_slots::SlotMode::FullMindmap => {
                if self.view_mode == ViewMode::Raw || self.editor.is_some() {
                    self.leave_zen_edit_mode(false);
                }
                self.begin_pending_quick_slot_restore(index, slot);
                if self.full_mindmap.is_none() {
                    let enter = self.enter_full_mindmap_at(None);
                    Task::batch([
                        checkpoint,
                        enter,
                        Task::done(Message::QuickSlotRestorePending),
                    ])
                } else {
                    Task::batch([checkpoint, Task::done(Message::QuickSlotRestorePending)])
                }
            }
            crate::quick_slots::SlotMode::Rendered
            | crate::quick_slots::SlotMode::DocumentMindmap => {
                let Some(pending) = self.begin_pending_quick_slot_restore(index, slot) else {
                    return Task::none();
                };
                let load = {
                    let slot = pending.slot.clone();
                    Task::perform(load_file(path), move |result| {
                        Message::QuickSlotFileLoaded {
                            index: pending.index,
                            slot,
                            result,
                        }
                    })
                };
                if self.full_mindmap.is_some() {
                    let exit = self.exit_full_mindmap(false);
                    if self.full_mindmap.is_none() {
                        self.supersede_file_loads();
                        Task::batch([checkpoint, exit, load])
                    } else {
                        Task::batch([checkpoint, exit])
                    }
                } else {
                    self.supersede_file_loads();
                    Task::batch([checkpoint, load])
                }
            }
        }
    }

    pub(super) fn assign_quick_slot(&mut self, index: usize) -> Task<Message> {
        if index >= crate::quick_slots::SLOT_COUNT {
            return Task::none();
        }
        if self.full_mindmap.is_none() && self.workspace.is_none() {
            return self.show_toast("Open a workspace before assigning Quick Slots".into());
        }
        if self.dirty {
            return self.show_toast(self.unsaved_edits_open_message());
        }
        let Some((path, context)) = self.current_quick_slot_context() else {
            return self.show_toast("Open a file before assigning a Quick Slot".into());
        };
        let Some(root) = self.quick_slots_workspace_root() else {
            return self.show_toast("Open a workspace before assigning Quick Slots".into());
        };
        let Some(relative_path) = crate::quick_slots::relative_path(root, &path) else {
            return self
                .show_toast("Quick Slots only store files inside the active workspace".into());
        };
        let slot = crate::quick_slots::QuickSlot {
            relative_path,
            context: context.normalized(),
        };
        self.invalidate_pending_quick_slot_restore();
        self.quick_slots.set(index, slot);
        self.quick_slots.active = Some(index);
        self.quick_slots_undo = None;
        self.persist_quick_slots_now();
        self.show_toast(format!("Quick Slot {} assigned", index + 1))
    }

    pub(super) fn new_quick_slot(&mut self) -> Task<Message> {
        if self.full_mindmap.is_none() && self.workspace.is_none() {
            return self.show_toast("Open a workspace before assigning Quick Slots".into());
        }
        if self.dirty {
            return self.show_toast(self.unsaved_edits_open_message());
        }
        let Some((path, _context)) = self.current_quick_slot_context() else {
            return self.show_toast("Open a file before assigning a Quick Slot".into());
        };
        let Some(root) = self.quick_slots_workspace_root() else {
            return self.show_toast("Open a workspace before assigning Quick Slots".into());
        };
        let Some(relative_path) = crate::quick_slots::relative_path(root, &path) else {
            return self
                .show_toast("Quick Slots only store files inside the active workspace".into());
        };
        let matching = (0..crate::quick_slots::SLOT_COUNT).find(|&index| {
            self.quick_slots
                .occupied(index)
                .and_then(|slot| crate::quick_slots::normalize_relative_path(&slot.relative_path))
                .is_some_and(|slot_path| slot_path == relative_path)
        });
        if let Some(index) = matching {
            return self.begin_quick_slot_activation(index);
        }
        let Some(index) = (0..crate::quick_slots::SLOT_COUNT)
            .find(|&index| self.quick_slots.occupied(index).is_none())
        else {
            return self.show_toast("Quick Slots are full".into());
        };
        self.assign_quick_slot(index)
    }

    pub(super) fn clear_quick_slot(&mut self, index: usize) -> Task<Message> {
        if self.dirty {
            return self.show_toast(self.unsaved_edits_open_message());
        }
        let Some(slot) = self.quick_slots.clear(index) else {
            return self.show_toast(format!("Quick Slot {} is empty", index + 1));
        };
        self.invalidate_pending_quick_slot_restore();
        self.quick_slots_undo = Some(crate::quick_slots::ClearUndo {
            index,
            slot,
            additional: Vec::new(),
        });
        self.persist_quick_slots_now();
        self.show_toast_with_action(
            format!("Quick Slot {} cleared", index + 1),
            Some(ToastAction {
                label: "Undo".into(),
                message: Message::QuickSlotUndo,
            }),
        )
    }

    pub(super) fn clear_all_quick_slots(&mut self) -> Task<Message> {
        if self.dirty {
            return self.show_toast(self.unsaved_edits_open_message());
        }
        let removed = self.quick_slots.clear_all();
        if removed.is_empty() {
            return self.show_toast("Quick Slots are already empty".into());
        }
        self.invalidate_pending_quick_slot_restore();
        // Clear All is one bounded undo action. Keep the first slot in named
        // fields and retain the remaining eight entries in the same payload.
        let mut removed = removed.into_iter();
        let (index, slot) = removed.next().expect("non-empty clear-all");
        self.quick_slots_undo = Some(crate::quick_slots::ClearUndo {
            index,
            slot,
            additional: removed.collect(),
        });
        self.persist_quick_slots_now();
        self.show_toast_with_action(
            "All Quick Slots cleared".into(),
            Some(ToastAction {
                label: "Undo".into(),
                message: Message::QuickSlotUndo,
            }),
        )
    }

    pub(super) fn undo_quick_slot_clear(&mut self) -> Task<Message> {
        let Some(undo) = self.quick_slots_undo.take() else {
            return self.show_toast("Nothing to undo".into());
        };
        self.invalidate_pending_quick_slot_restore();
        self.quick_slots.set(undo.index, undo.slot);
        for (index, slot) in undo.additional {
            self.quick_slots.set(index, slot);
        }
        self.quick_slots.active = Some(undo.index);
        self.persist_quick_slots_now();
        self.show_toast(format!("Quick Slot {} restored", undo.index + 1))
    }

    pub(super) fn close_active_quick_slot(&mut self) -> Task<Message> {
        let Some(active) = self.quick_slots.active else {
            return self.show_toast("No active Quick Slot".into());
        };
        if self.dirty {
            return self.show_toast(self.unsaved_edits_open_message());
        }
        let checkpoint = self.checkpoint_active_quick_slot();
        self.invalidate_pending_quick_slot_restore();
        let _removed = self.quick_slots.clear(active);
        self.quick_slots_undo = None;
        self.persist_quick_slots_now();
        let next = ((active + 1)..crate::quick_slots::SLOT_COUNT)
            .chain((0..active).rev())
            .find(|&index| {
                self.quick_slot_path(index)
                    .is_some_and(|(_, path)| path.is_file())
            });
        let activate = next.map_or_else(Task::none, |index| {
            Task::done(Message::QuickSlotActivate(index))
        });
        Task::batch([
            checkpoint,
            activate,
            self.show_toast("Quick Slot closed".into()),
        ])
    }

    pub(super) fn close_quick_slot_window(&mut self) -> Task<Message> {
        if self.dirty {
            return self.show_toast(self.unsaved_edits_open_message());
        }
        let checkpoint = self.checkpoint_active_quick_slot();
        // The checkpoint mutates the in-memory slot synchronously. Persist it
        // before returning the close task so the debounce cannot lose the
        // latest reading position when the process exits immediately.
        self.persist_quick_slots_now();
        self.invalidate_pending_quick_slot_restore();
        Task::batch([
            checkpoint,
            iced::window::latest().and_then(iced::window::close),
        ])
    }

    pub(super) fn apply_pending_quick_slot_restore(&mut self) -> Task<Message> {
        let Some(pending) = self.pending_quick_slot_restore.clone() else {
            return Task::none();
        };
        if !self.quick_slot_restore_is_current(&pending) {
            self.invalidate_pending_quick_slot_restore();
            return Task::none();
        }
        let index = pending.index;
        let slot = pending.slot.clone();
        let Some(root) = self.quick_slots_workspace_root() else {
            self.invalidate_pending_quick_slot_restore();
            return Task::none();
        };
        let Some(path) = crate::quick_slots::resolve_path(root, &slot.relative_path) else {
            self.invalidate_pending_quick_slot_restore();
            return Task::none();
        };
        if !path.is_file() {
            self.invalidate_pending_quick_slot_restore();
            return self.show_toast(format!("Quick Slot {} is missing", index + 1));
        }
        // A Full Mindmap root may still be materializing. Keep the identity-
        // bearing restore pending until that accepted snapshot is installed;
        // applying it against the provisional state would be overwritten by
        // `reset_full_mindmap_workspace` during snapshot replacement.
        if self
            .full_mindmap
            .as_ref()
            .is_some_and(|full| full.pending_workspace_load.is_some())
        {
            return Task::none();
        }
        if slot.context.mode == crate::quick_slots::SlotMode::FullMindmap {
            let Some(full) = self.full_mindmap.as_mut() else {
                return Task::none();
            };
            full.deferred_file_selection = Some(path.clone());
            full.selected = Some(WorkspaceNodeId::File(path.clone()));
            full.focus_request = Some(WorkspaceNodeId::File(path.clone()));
            self.quick_slot_preview_restore = Some(slot.context.preview_position);
            self.quick_slot_preview_restore_guard = Some(pending.clone());
            self.pending_quick_slot_restore = None;
            let already_ready = full.pending_preview.is_none()
                && matches!(
                    &full.preview,
                    FullMindmapPreview::Document { path: current, .. }
                        | FullMindmapPreview::Data { path: current, .. }
                        if current == &path
                );
            let preview = self.schedule_full_mindmap_preview(Some(path));
            let restore = if already_ready
                && self
                    .full_mindmap
                    .as_ref()
                    .is_some_and(|full| full.preview_viewport.is_some())
            {
                self.take_current_quick_slot_preview_restore()
                    .map_or_else(Task::none, |position| {
                        self.quick_slot_restore_preview_position(position)
                    })
            } else {
                Task::none()
            };
            return Task::batch([
                preview,
                restore,
                self.show_toast(format!("Quick Slot {}", index + 1)),
            ]);
        }
        if self.file.as_ref() == Some(&path) {
            return self.apply_quick_slot_restore_after_file(&path, slot);
        }
        let load = Task::perform(load_file(path), move |result| {
            Message::QuickSlotFileLoaded {
                index,
                slot,
                result,
            }
        });
        load
    }
}
