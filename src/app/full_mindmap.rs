//! Full Mindmap Mode: workspace loading, verification waves, navigation, and the side-panel preview.

use super::*;

impl App {
    pub(super) fn full_mindmap_selected_file(&self) -> Option<PathBuf> {
        self.full_mindmap.as_ref().and_then(|full| {
            full.selected.as_ref().and_then(|selected| match selected {
                WorkspaceNodeId::File(path) => Some(path.clone()),
                _ => None,
            })
        })
    }

    pub(super) fn new_full_mindmap_state() -> FullMindmapState {
        static NEXT_PREVIEW_NAMESPACE: AtomicU64 = AtomicU64::new(1);
        let preview_namespace = NEXT_PREVIEW_NAMESPACE.fetch_add(1, Ordering::Relaxed);
        let (preview_settle_tx, _) =
            tokio::sync::watch::channel::<Option<PendingFullMindmapPreviewSettle>>(None);
        FullMindmapState {
            selected: None,
            expanded: HashSet::new(),
            panel_open: false,
            panel_width: MIND_PANEL_DEFAULT,
            panel_step: 0,
            panel_drag: None,
            pending_open: None,
            pending_preview_settle: None,
            pending_preview: None,
            pending_workspace_load: None,
            pending_folder_loads: HashMap::new(),
            verification_wave: None,
            verification_followup_pending: false,
            verification_hidden: HashSet::new(),
            expansion_generation: 0,
            materialized_folders: HashMap::new(),
            deferred_file_selection: None,
            focus_request: None,
            preview: FullMindmapPreview::None,
            preview_identity: None,
            preview_namespace,
            preview_shape_ready: false,
            preview_window: crate::virt::VirtWindow::default(),
            preview_viewport: None,
            preview_height_cache: crate::virt::HeightCache::default(),
            preview_measurement_pending: false,
            preview_measurement_range: None,
            preview_measurement_generation: None,
            preview_generation: 0,
            preview_work_epoch: Arc::new(AtomicU64::new(0)),
            preview_parse_gate: Arc::new(tokio::sync::Semaphore::new(3)),
            preview_settle_tx,
            preview_settle_worker_started: false,
            preview_loading_images: HashMap::new(),
            preview_pending_diagrams: HashMap::new(),
            preview_asset_images: HashSet::new(),
            preview_failed_images: HashSet::new(),
            preview_asset_diagrams: HashSet::new(),
            preview_asset_cursor: 0,
            preview_asset_cursor_tag: None,
            preview_asset_wave_id: 0,
            load_error: None,
            layout_cache: std::cell::RefCell::new(None),
            layout_generation: 0,
        }
    }

    pub(super) fn full_mindmap_start_folder(&self) -> Option<PathBuf> {
        self.workspace.clone().or_else(|| {
            self.file
                .as_ref()
                .and_then(|file| file.parent().map(PathBuf::from))
        })
    }

    pub(super) fn full_mindmap_graph(&self) -> Option<std::sync::Arc<WorkspaceGraph>> {
        let full = self.full_mindmap.as_ref()?;
        if let Some(graph) = full.layout_cache.borrow().as_ref() {
            return Some(std::sync::Arc::clone(graph));
        }
        let graph = std::sync::Arc::new(workspace_mindmap::from_tree_with_hidden(
            self.workspace_tree.as_ref()?,
            &full.expanded,
            &full.materialized_folders,
            &full.pending_folder_loads.keys().cloned().collect(),
            &full.verification_hidden,
            self.workspace_truncated,
        ));
        *full.layout_cache.borrow_mut() = Some(std::sync::Arc::clone(&graph));
        Some(graph)
    }

    pub(super) fn invalidate_full_mindmap_layout(&mut self) {
        if let Some(full) = self.full_mindmap.as_mut() {
            // Invalidate the cache and publish a distinct focus-generation
            // signal. A selected folder can survive a graph rebuild, so the
            // canvas cannot infer this intent from selection identity alone.
            full.layout_generation = full.layout_generation.wrapping_add(1);
            *full.layout_cache.borrow_mut() = None;
        }
    }

    /// Cancel all delayed-reveal ownership. The spawned filesystem futures
    /// cannot be force-stopped by Iced, so clearing the wave and hidden set is
    /// the cancellation boundary; completion handlers then fail the strict
    /// identity check and cannot reveal stale nodes.
    pub(super) fn cancel_full_mindmap_verification(&mut self) {
        let had_hidden = self.full_mindmap.as_ref().is_some_and(|full| {
            full.verification_wave.is_some() || !full.verification_hidden.is_empty()
        });
        if let Some(full) = self.full_mindmap.as_mut() {
            full.verification_wave = None;
            full.verification_followup_pending = false;
            full.verification_hidden.clear();
        }
        self.full_mindmap_progress = None;
        if had_hidden {
            self.invalidate_full_mindmap_layout();
        }
    }

    pub(super) fn bump_full_mindmap_expansion_generation(&mut self) {
        if let Some(full) = self.full_mindmap.as_mut() {
            full.expansion_generation = full.expansion_generation.wrapping_add(1);
        }
    }

    pub(super) fn full_mindmap_folder_count(
        &self,
        path: &std::path::Path,
    ) -> Option<tree::RecursiveFileCount> {
        let full = self.full_mindmap.as_ref()?;
        if let Some(materialized) = full.materialized_folders.get(path) {
            return Some(match materialized {
                workspace_mindmap::MaterializedFolder::Verified {
                    recursive_supported_file_count,
                    ..
                } => *recursive_supported_file_count,
                workspace_mindmap::MaterializedFolder::Loaded {
                    recursive_supported_file_count,
                    ..
                } => *recursive_supported_file_count,
                workspace_mindmap::MaterializedFolder::Error(_) => {
                    tree::RecursiveFileCount::Unavailable
                }
            });
        }
        // A lazily expanded parent owns shallow child skeletons that are no
        // longer reachable through the retained workspace tree. Resolve the
        // child's count from that in-memory branch before giving up.
        for materialized in full.materialized_folders.values() {
            if let workspace_mindmap::MaterializedFolder::Loaded { folders, .. } = materialized {
                if let Some(node) = folders.iter().find(|node| node.path == path) {
                    return node.recursive_supported_file_count;
                }
            }
        }
        self.workspace_tree
            .as_ref()
            .and_then(|root| tree::find_folder(root, path))
            .and_then(|node| node.recursive_supported_file_count)
    }

    /// Snapshot unresolved LowerBound(0) folders that are actually visible
    /// beneath an expanded parent. The graph is authoritative: collapsed
    /// descendants and folders hidden by an older wave never enter the queue.
    pub(super) fn full_mindmap_verification_candidates(
        &self,
    ) -> (Vec<PathBuf>, HashMap<PathBuf, PathBuf>) {
        let Some(full) = self.full_mindmap.as_ref() else {
            return (Vec::new(), HashMap::new());
        };
        let Some(graph) = self.full_mindmap_graph() else {
            return (Vec::new(), HashMap::new());
        };
        let expanded = full.expanded.clone();
        let mut candidates = Vec::new();
        let mut parent_by_candidate = HashMap::new();
        for node in graph.nodes.iter() {
            let Some(WorkspaceNodeId::Folder(path)) = node.id.as_ref() else {
                continue;
            };
            // The acted-on folder itself stays visible while its explicit
            // expansion owns a branch-local load. Delayed verification is for
            // unresolved children on the newly expanded frontier; hiding the
            // parent would remove the user's selection and suppress its
            // Loading files status.
            if expanded.contains(path) {
                continue;
            }
            // Count/status facts already accepted for this path (including a
            // count-only verification result) are authoritative until the
            // branch is explicitly expanded; do not rescan them on every
            // sibling materialization.
            if full.materialized_folders.contains_key(path) {
                continue;
            }
            if !matches!(
                self.full_mindmap_folder_count(path),
                Some(tree::RecursiveFileCount::LowerBound(0))
            ) {
                continue;
            }
            let Some(parent) = graph.parent(&WorkspaceNodeId::Folder(path.clone())) else {
                continue;
            };
            if !matches!(
                parent,
                WorkspaceNodeId::Root(_) | WorkspaceNodeId::Folder(_)
            ) || !expanded.contains(parent.path())
            {
                continue;
            }
            if parent_by_candidate
                .insert(path.clone(), parent.path().to_path_buf())
                .is_none()
            {
                candidates.push(path.clone());
            }
        }
        // Graph order is stable and follows the user's visible tree. Keep the
        // first fixed prefix; excess candidates remain visible with the
        // retained scan-limit label rather than being silently dropped.
        candidates.truncate(FULL_MINDMAP_VERIFICATION_MAX_CANDIDATES);
        parent_by_candidate.retain(|path, _| candidates.contains(path));
        (candidates, parent_by_candidate)
    }

    /// Keep newly materialized frontier shells hidden without mutating the
    /// active wave's fixed candidate list/denominator. When that wave drains,
    /// the pending shells are included by a fresh fixed snapshot. If no wave
    /// is active, start that snapshot immediately in this same update so the
    /// shells cannot flash for one rendered frame.
    pub(super) fn schedule_full_mindmap_verification_followup(&mut self) -> Task<Message> {
        let active = self
            .full_mindmap
            .as_ref()
            .is_some_and(|full| full.verification_wave.is_some());
        if !active {
            return self.begin_full_mindmap_verification_wave();
        }

        let (candidates, _) = self.full_mindmap_verification_candidates();
        let active_candidates = self
            .full_mindmap
            .as_ref()
            .and_then(|full| full.verification_wave.as_ref())
            .map(|wave| wave.candidates.iter().collect::<HashSet<_>>())
            .unwrap_or_default();
        let new_candidates = candidates
            .into_iter()
            .filter(|path| {
                // A verification result already owns only count/status for
                // this path; it must not be scheduled repeatedly on every
                // sibling materialization.
                !self
                    .full_mindmap
                    .as_ref()
                    .is_some_and(|full| full.materialized_folders.contains_key(path))
                    && !active_candidates.contains(path)
            })
            .collect::<Vec<_>>();
        if new_candidates.is_empty() {
            return Task::none();
        }
        if let Some(full) = self.full_mindmap.as_mut() {
            full.verification_hidden.extend(new_candidates);
            full.verification_followup_pending = true;
        }
        self.invalidate_full_mindmap_layout();
        Task::none()
    }

    /// Start one fixed wave. The denominator is frozen before any worker is
    /// launched and therefore cannot change when a result exposes new shells.
    pub(super) fn begin_full_mindmap_verification_wave(&mut self) -> Task<Message> {
        let Some(workspace_root) = self.workspace.clone() else {
            self.cancel_full_mindmap_verification();
            return Task::none();
        };
        if self.workspace_snapshot_show_hidden != self.show_hidden {
            self.cancel_full_mindmap_verification();
            return Task::none();
        }
        // Rebuild the candidate snapshot from the currently visible frontier,
        // never from a graph whose old wave still hides shells.
        self.cancel_full_mindmap_verification();
        let (candidates, parent_by_candidate) = self.full_mindmap_verification_candidates();
        if candidates.is_empty() {
            return Task::none();
        }
        self.full_mindmap_request_seq = self.full_mindmap_request_seq.wrapping_add(1);
        let wave_id = self.full_mindmap_request_seq;
        let show_hidden = self.show_hidden;
        let parent_expansion_generation = self
            .full_mindmap
            .as_ref()
            .map(|full| full.expansion_generation)
            .unwrap_or_default();
        let total = candidates.len();
        if let Some(full) = self.full_mindmap.as_mut() {
            full.verification_hidden = candidates.iter().cloned().collect();
            full.verification_wave = Some(FullMindmapVerificationWave {
                id: wave_id,
                workspace_root,
                show_hidden,
                parent_expansion_generation,
                candidates,
                parent_by_candidate,
                next_index: 0,
                in_flight: HashSet::new(),
                request_ids: HashMap::new(),
                checked: 0,
            });
        }
        self.full_mindmap_progress = Some(FullMindmapProgress {
            wave_id,
            checked: 0,
            total,
        });
        self.invalidate_full_mindmap_layout();
        self.launch_full_mindmap_verification_tasks()
    }

    /// Fill the fixed worker window from the wave's immutable candidate queue.
    pub(super) fn launch_full_mindmap_verification_tasks(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        loop {
            let next = {
                let Some(full) = self.full_mindmap.as_mut() else {
                    break;
                };
                let Some(wave) = full.verification_wave.as_mut() else {
                    break;
                };
                if wave.in_flight.len() >= FULL_MINDMAP_VERIFICATION_CONCURRENCY
                    || wave.next_index >= wave.candidates.len()
                {
                    break;
                }
                let folder = wave.candidates[wave.next_index].clone();
                wave.next_index += 1;
                wave.in_flight.insert(folder.clone());
                Some((
                    wave.id,
                    wave.workspace_root.clone(),
                    wave.show_hidden,
                    wave.parent_expansion_generation,
                    wave.parent_by_candidate
                        .get(&folder)
                        .cloned()
                        .unwrap_or_else(|| wave.workspace_root.clone()),
                    folder,
                ))
            };
            let Some((
                wave_id,
                workspace_root,
                show_hidden,
                parent_expansion_generation,
                parent,
                folder,
            )) = next
            else {
                break;
            };
            self.full_mindmap_request_seq = self.full_mindmap_request_seq.wrapping_add(1);
            let request = PendingFullMindmapVerification {
                id: self.full_mindmap_request_seq,
                wave_id,
                workspace_root,
                parent,
                parent_expansion_generation,
                folder: folder.clone(),
                show_hidden,
            };
            if let Some(full) = self.full_mindmap.as_mut() {
                if let Some(wave) = full.verification_wave.as_mut() {
                    wave.request_ids.insert(folder.clone(), request.id);
                }
            }
            tasks.push(Task::perform(
                load_full_mindmap_folder(folder, show_hidden),
                move |result| Message::FullMindmapVerificationLoaded { request, result },
            ));
        }
        Task::batch(tasks)
    }

    pub(super) fn handle_full_mindmap_verification_loaded(
        &mut self,
        request: PendingFullMindmapVerification,
        result: Result<(PathBuf, tree::ExpandedFolderSnapshot), String>,
    ) -> Task<Message> {
        let current = self.workspace.as_ref() == Some(&request.workspace_root)
            && self.show_hidden == request.show_hidden
            && self.workspace_snapshot_show_hidden == request.show_hidden
            && self.full_mindmap.as_ref().is_some_and(|full| {
                full.expansion_generation == request.parent_expansion_generation
                    && full.expanded.contains(&request.parent)
                    && full.verification_wave.as_ref().is_some_and(|wave| {
                        wave.id == request.wave_id
                            && wave.workspace_root == request.workspace_root
                            && wave.show_hidden == request.show_hidden
                            && wave.parent_expansion_generation
                                == request.parent_expansion_generation
                            && wave.candidates.contains(&request.folder)
                            && wave.in_flight.contains(&request.folder)
                            && wave.request_ids.get(&request.folder) == Some(&request.id)
                    })
            });
        if !current {
            return Task::none();
        }

        let mut accepted_exact_empty = false;
        let materialized = match result {
            Ok((path, snapshot)) if path == request.folder => {
                accepted_exact_empty = matches!(
                    snapshot.recursive_supported_file_count,
                    tree::RecursiveFileCount::Exact(0)
                );
                workspace_mindmap::MaterializedFolder::Verified {
                    recursive_supported_file_count: snapshot.recursive_supported_file_count,
                    truncated: snapshot.truncated,
                }
            }
            Ok((path, _)) => workspace_mindmap::MaterializedFolder::Error(format!(
                "Verified unexpected folder: {}",
                path.display()
            )),
            Err(error) => workspace_mindmap::MaterializedFolder::Error(error),
        };

        let (done, followup_pending) = if let Some(full) = self.full_mindmap.as_mut() {
            full.verification_hidden.remove(&request.folder);
            full.materialized_folders
                .insert(request.folder.clone(), materialized);
            let wave = full
                .verification_wave
                .as_mut()
                .expect("verified request owns a wave");
            wave.in_flight.remove(&request.folder);
            wave.request_ids.remove(&request.folder);
            // Each accepted result advances exactly once. This monotonic
            // counter drives both the determinate bar and remaining label.
            wave.checked = wave.checked.saturating_add(1);
            let done = wave.checked >= wave.candidates.len() && wave.in_flight.is_empty();
            let followup_pending = done && full.verification_followup_pending;
            if done {
                full.verification_wave = None;
                full.verification_followup_pending = false;
                if !followup_pending {
                    full.verification_hidden.clear();
                    self.full_mindmap_progress = None;
                }
            } else {
                self.full_mindmap_progress = Some(FullMindmapProgress {
                    wave_id: wave.id,
                    checked: wave.checked,
                    total: wave.candidates.len(),
                });
            }
            (done, followup_pending)
        } else {
            return Task::none();
        };
        self.invalidate_full_mindmap_layout();
        if accepted_exact_empty {
            self.normalize_full_mindmap_workspace();
        }
        if done {
            if followup_pending {
                self.schedule_full_mindmap_verification_followup()
            } else {
                Task::none()
            }
        } else {
            self.launch_full_mindmap_verification_tasks()
        }
    }

    pub(super) fn enter_full_mindmap(&mut self) -> Task<Message> {
        self.enter_full_mindmap_at(None)
    }

    /// Enter Full Mindmap, optionally forcing a fresh workspace root. The
    /// document-Mindmap boundary uses the override when the current file is
    /// outside the existing workspace; ordinary Full Mindmap entry preserves
    /// its already-indexed workspace exactly as before.
    pub(super) fn enter_full_mindmap_at(&mut self, forced_start: Option<PathBuf>) -> Task<Message> {
        self.cancel_refresh_tracking();
        self.overlay = Overlay::None;
        if self.full_mindmap.is_some() {
            self.cancel_full_mindmap_verification();
            // Re-entry replaces the navigator state. Bump the preview epoch
            // before dropping the old state so any active read/parse exits at
            // its next cooperative checkpoint instead of blocking new work.
            self.reset_full_mindmap_preview_window();
        }
        self.full_mindmap = Some(Self::new_full_mindmap_state());
        if forced_start.is_none() && self.workspace_tree.is_some() {
            self.reset_full_mindmap_workspace();
            let verification = self.begin_full_mindmap_verification_wave();
            let expanded = self.begin_full_mindmap_expanded_folder_loads();
            Task::batch([verification, expanded])
        } else {
            let start = forced_start
                .or_else(|| self.full_mindmap_start_folder())
                .or_else(Picker::home);
            start.map_or_else(Task::none, |path| {
                self.begin_full_mindmap_workspace_load(path, false, None, false, false, false)
            })
        }
    }

    /// Return to the workspace navigator from document-level Mindmap. Keep an
    /// existing workspace only when its accepted bounded index contains the
    /// current file; otherwise adopt the file's parent (or Home) through the
    /// same background workspace-load path used by every other Full Mindmap
    /// root change.
    pub(super) fn enter_full_mindmap_for_current_file(&mut self) -> Task<Message> {
        let Some(file) = self.file.clone() else {
            return Task::none();
        };
        let in_workspace = self
            .workspace
            .as_ref()
            .is_some_and(|root| file.starts_with(root))
            && self.workspace_files.contains(&file);
        let forced_start = (!in_workspace)
            .then(|| file.parent().map(PathBuf::from).or_else(Picker::home))
            .flatten();
        // The document-Mindmap root-left gesture replaces the reader surface
        // with Full Mindmap before the workspace helper can observe it. Capture
        // the outgoing document context at this boundary; the helper's later
        // Full Mindmap checkpoint is a no-op because the new navigator has no
        // selected preview yet.
        let checkpoint = self.checkpoint_active_quick_slot();
        Task::batch([checkpoint, self.enter_full_mindmap_at(forced_start)])
    }

    /// Exit Full Mindmap without exposing a workspace snapshot built under a
    /// different hidden-file filter. Normal exits restore the prior surface;
    /// the explicit Files action additionally opens the sidebar.
    pub(super) fn exit_full_mindmap(&mut self, return_to_files: bool) -> Task<Message> {
        let stale_workspace =
            self.full_mindmap.is_some() && self.workspace_snapshot_show_hidden != self.show_hidden;
        if stale_workspace {
            if let Some(path) = self.workspace.clone() {
                return self.begin_full_mindmap_workspace_load(
                    path,
                    false,
                    None,
                    true,
                    return_to_files,
                    true,
                );
            }
        }
        self.finish_full_mindmap_exit(return_to_files)
    }

    pub(super) fn finish_full_mindmap_exit(&mut self, return_to_files: bool) -> Task<Message> {
        self.cancel_refresh_tracking();
        self.cancel_full_mindmap_verification();
        self.reset_full_mindmap_preview_window();
        self.quick_slot_preview_restore = None;
        self.full_mindmap = None;
        if return_to_files {
            self.sidebar_open = true;
            self.sidebar_tab = SidebarTab::Files;
            self.reveal_current_file();
        }
        self.restore_body_scroll()
    }

    /// Start (or refresh) the workspace phase without touching sidebar state.
    pub(super) fn reset_full_mindmap_workspace(&mut self) {
        let Some(tree) = self.workspace_tree.as_ref() else {
            return;
        };
        let root = tree.path.clone();
        let current_file = self
            .file
            .as_ref()
            .filter(|file| file.starts_with(&root))
            .cloned();
        let ancestors = current_file
            .as_ref()
            .map(|file| tree::ancestors_of(&root, file))
            .unwrap_or_default();
        self.reset_full_mindmap_preview_window();
        {
            let Some(full) = self.full_mindmap.as_mut() else {
                return;
            };
            full.expanded.clear();
            full.expanded.insert(root.clone());
            for ancestor in ancestors {
                full.expanded.insert(ancestor);
            }
            let root_id = WorkspaceNodeId::Root(root);
            full.selected = Some(root_id.clone());
            // The document-Mindmap root-Left bridge already knows which file
            // must own the first Full Mindmap viewport. Keep that request
            // alive before the branch listing exposes the file node; the
            // canvas will deliberately defer consuming it until the rendered
            // WorkspaceGraph contains the exact file. Without this early
            // identity, the first root frame becomes the only focus event and
            // a later materialization can leave the viewport at the root.
            full.focus_request = Some(
                current_file
                    .as_ref()
                    .map(|path| WorkspaceNodeId::File(path.clone()))
                    .unwrap_or(root_id),
            );
            full.panel_drag = None;
            // Rebuilding the workspace means navigation has changed intent. An
            // in-flight file read from the prior workspace must not later exit
            // the navigator into that old document.
            full.pending_open = None;
            full.pending_preview_settle = None;
            full.pending_preview = None;
            full.pending_workspace_load = None;
            full.pending_folder_loads.clear();
            full.verification_wave = None;
            full.verification_followup_pending = false;
            full.verification_hidden.clear();
            full.expansion_generation = full.expansion_generation.wrapping_add(1);
            full.materialized_folders.clear();
            full.deferred_file_selection = current_file;
            full.preview = FullMindmapPreview::None;
            full.load_error = None;
            full.layout_generation = full.layout_generation.wrapping_add(1);
            *full.layout_cache.borrow_mut() = None;
        }
        self.full_mindmap_progress = None;
        self.invalidate_full_mindmap_layout();
    }

    /// Keep a Full Mindmap selection valid after rebuilding the source tree
    /// (for example after toggling hidden files) without sharing sidebar state.
    pub(super) fn normalize_full_mindmap_workspace(&mut self) {
        let selected = match self.full_mindmap.as_ref() {
            Some(FullMindmapState { selected, .. }) => selected.clone(),
            _ => return,
        };
        if selected.is_none() {
            // Explicit canvas deselection is a valid latest navigation state.
            // A preserve-navigation snapshot refresh may replace graph data,
            // but it must not manufacture a root selection/focus afterward.
            if let Some(full) = self.full_mindmap.as_mut() {
                full.focus_request = None;
            }
            return;
        }
        let Some(graph) = self.full_mindmap_graph() else {
            return;
        };
        let deferred = self
            .full_mindmap
            .as_ref()
            .and_then(|full| full.deferred_file_selection.as_ref());
        let deferred_file = selected
            .as_ref()
            .is_some_and(|id| matches!(id, WorkspaceNodeId::File(path) if deferred == Some(path)));
        let next = selected
            .clone()
            .filter(|id| graph.node(id).is_some() || deferred_file)
            .or_else(|| {
                selected
                    .as_ref()
                    .and_then(|id| graph.nearest_visible_ancestor(id.path()))
            })
            .unwrap_or_else(|| graph.root_id());
        let selection_changed = self
            .full_mindmap
            .as_ref()
            .is_some_and(|full| full.selected.as_ref() != Some(&next));
        // A bridge entry records a file focus before its parent listing is
        // accepted. Normalization may run for an intermediate root/sibling
        // snapshot, but it must not replace that still-pending file request
        // with the currently visible root. Explicit user navigation clears
        // `deferred_file_selection` first, so this preservation cannot block
        // a newer selection.
        let preserve_deferred_file_focus = self.full_mindmap.as_ref().is_some_and(|full| {
            matches!(
                    (&full.focus_request, &full.deferred_file_selection),
                    (
                        Some(WorkspaceNodeId::File(focus)),
                        Some(deferred),
                    ) if focus == deferred
            )
        });
        if selection_changed {
            self.reset_full_mindmap_preview_window();
        }
        if let Some(full) = self.full_mindmap.as_mut() {
            if selection_changed {
                full.pending_preview_settle = None;
                full.pending_preview = None;
                full.preview = FullMindmapPreview::None;
            }
            if !preserve_deferred_file_focus && full.focus_request.as_ref() != Some(&next) {
                full.focus_request = Some(next.clone());
            }
            full.selected = Some(next);
        }
    }

    /// Full Mindmap workspace changes are intentionally background-only. A
    /// project root can contain thousands of unrelated entries; indexing it on
    /// the Iced update thread would freeze navigation and could exhaust memory.
    pub(super) fn begin_full_mindmap_workspace_load(
        &mut self,
        path: PathBuf,
        select_root: bool,
        open_after: Option<PathBuf>,
        preserve_navigation: bool,
        return_to_files_after: bool,
        exit_after_refresh: bool,
    ) -> Task<Message> {
        let path = canonicalize_existing_path(path);
        self.cancel_refresh_tracking();
        // Full Mindmap workspace changes replace the selected preview/root;
        // preserve the outgoing slot context before any request state moves.
        // The non-Full-Mindmap path is covered by `set_workspace`'s snapshot
        // replacement, which checkpoints before swapping the active root.
        let checkpoint = if self.full_mindmap.is_some() {
            self.checkpoint_active_quick_slot()
        } else {
            Task::none()
        };
        if (self.pending_quick_slot_restore.is_some()
            || self.quick_slot_preview_restore_guard.is_some())
            && self.quick_slots_workspace_root().is_some_and(|current| {
                crate::quick_slots::workspace_key(current)
                    != crate::quick_slots::workspace_key(&path)
            })
        {
            self.invalidate_pending_quick_slot_restore();
        }
        if self.full_mindmap.is_none() {
            self.set_workspace(path, true);
            return Task::none();
        }
        let already_pending = self.full_mindmap.as_ref().is_some_and(|full| {
            full.pending_workspace_load.as_ref().is_some_and(|pending| {
                pending.path == path
                    && pending.select_root == select_root
                    && pending.open_after == open_after
                    && pending.preserve_navigation == preserve_navigation
                    && pending.return_to_files_after == return_to_files_after
                    && pending.exit_after_refresh == exit_after_refresh
            })
        });
        if already_pending {
            return checkpoint;
        }
        // Root/filter changes supersede every delayed-reveal worker. The
        // request identity check also guards futures that cannot be aborted.
        self.cancel_full_mindmap_verification();
        self.bump_full_mindmap_expansion_generation();
        if !preserve_navigation {
            self.reset_full_mindmap_preview_window();
        }
        self.full_mindmap_request_seq = self.full_mindmap_request_seq.wrapping_add(1);
        let request = PendingFullMindmapWorkspaceLoad {
            id: self.full_mindmap_request_seq,
            path: path.clone(),
            select_root,
            open_after,
            preserve_navigation,
            return_to_files_after,
            exit_after_refresh,
        };
        let Some(full) = self.full_mindmap.as_mut() else {
            return checkpoint;
        };
        full.pending_workspace_load = Some(request.clone());
        // A new refresh owns the status line. Do not leave an error from a
        // prior failed snapshot visible after a later refresh succeeds.
        full.load_error = None;
        if preserve_navigation {
            if let Some(WorkspaceNodeId::File(path)) = full.selected.as_ref() {
                full.deferred_file_selection = Some(path.clone());
            }
        } else {
            full.deferred_file_selection = None;
        }
        full.pending_folder_loads.clear();
        full.materialized_folders.clear();
        if !preserve_navigation {
            full.pending_open = None;
        }
        if !preserve_navigation {
            full.pending_preview_settle = None;
            full.pending_preview = None;
            full.load_error = None;
            full.preview = FullMindmapPreview::None;
            full.preview_shape_ready = false;
        }
        self.invalidate_full_mindmap_layout();
        let show_hidden = self.show_hidden;
        Task::batch([
            checkpoint,
            Task::perform(load_workspace_snapshot(path, show_hidden), move |result| {
                Message::FullMindmapWorkspaceLoaded { request, result }
            }),
        ])
    }

    pub(super) fn begin_full_mindmap_expanded_folder_loads(&mut self) -> Task<Message> {
        let folders = self
            .full_mindmap
            .as_ref()
            .map(|full| full.expanded.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        Task::batch(
            folders
                .into_iter()
                .map(|folder| self.begin_full_mindmap_folder_load(folder)),
        )
    }

    pub(super) fn begin_full_mindmap_folder_load(&mut self, folder: PathBuf) -> Task<Message> {
        let folder = canonicalize_existing_path(folder);
        let Some(workspace_root) = self.workspace.clone() else {
            return Task::none();
        };
        let eligible = self.full_mindmap.as_ref().is_some_and(|full| {
            full.expanded.contains(&folder)
                && folder.starts_with(&workspace_root)
                && !full
                    .materialized_folders
                    .get(&folder)
                    .is_some_and(|materialized| {
                        matches!(
                            materialized,
                            workspace_mindmap::MaterializedFolder::Loaded { .. }
                                | workspace_mindmap::MaterializedFolder::Error(_)
                        )
                    })
                && !full.pending_folder_loads.contains_key(&folder)
                // A delayed-reveal wave owns unresolved shells, including an
                // expanded ancestor. Avoid a duplicate branch scan whose
                // completion could race the fixed wave's result.
                && !full.verification_hidden.contains(&folder)
        });
        if !eligible || self.workspace_snapshot_show_hidden != self.show_hidden {
            return Task::none();
        }

        // An exact retained node already owns complete folder structure and
        // the standard sidebar's pre-grouped immediate paths. Reuse those
        // bounded in-memory facts; only interrupted/unreadable or lazily
        // discovered shallow nodes need another filesystem pass.
        let retained_snapshot = self
            .workspace_tree
            .as_ref()
            .and_then(|root| tree::find_folder(root, &folder))
            .filter(|node| {
                matches!(
                    node.recursive_supported_file_count,
                    Some(tree::RecursiveFileCount::Exact(_))
                )
            })
            .map(|node| {
                let folders = node
                    .children
                    .iter()
                    .cloned()
                    .map(|mut child| {
                        child.children.clear();
                        child
                    })
                    .collect();
                let files = self.workspace_sidebar_files.files_for(&folder).to_vec();
                let count = node
                    .recursive_supported_file_count
                    .expect("filtered exact retained folder");
                (folders, files, count)
            });

        self.full_mindmap_request_seq = self.full_mindmap_request_seq.wrapping_add(1);
        let request = PendingFullMindmapFolderLoad {
            id: self.full_mindmap_request_seq,
            workspace_root,
            folder: folder.clone(),
            show_hidden: self.show_hidden,
        };
        let full = self.full_mindmap.as_mut().expect("eligible Full Mindmap");
        full.pending_folder_loads
            .insert(folder.clone(), request.clone());
        full.materialized_folders.remove(&folder);
        self.invalidate_full_mindmap_layout();
        if let Some((folders, files, recursive_supported_file_count)) = retained_snapshot {
            let result = Ok((
                folder,
                tree::ExpandedFolderSnapshot {
                    folders,
                    files,
                    recursive_supported_file_count,
                    truncated: false,
                },
            ));
            Task::done(Message::FullMindmapFolderLoaded { request, result })
        } else {
            Task::perform(
                load_full_mindmap_folder(folder, request.show_hidden),
                move |result| Message::FullMindmapFolderLoaded { request, result },
            )
        }
    }

    pub(super) fn evict_full_mindmap_folder(&mut self, folder: &std::path::Path) {
        if let Some(full) = self.full_mindmap.as_mut() {
            full.expansion_generation = full.expansion_generation.wrapping_add(1);
            full.expanded.retain(|path| !path.starts_with(folder));
            full.pending_folder_loads
                .retain(|path, _| !path.starts_with(folder));
            full.materialized_folders
                .retain(|path, _| !path.starts_with(folder));
            if full
                .deferred_file_selection
                .as_ref()
                .is_some_and(|path| path.starts_with(folder))
            {
                full.deferred_file_selection = None;
            }
        }
        self.invalidate_full_mindmap_layout();
    }

    /// Drop logical ownership of any Full Mindmap preview work. The spawned
    /// timer/read cannot always be aborted, so its request identity remains
    /// the final stale-result guard in the message handlers.
    pub(super) fn cancel_full_mindmap_preview(&mut self) {
        self.reset_full_mindmap_preview_window();
        self.quick_slot_preview_restore = None;
        if let Some(full) = self.full_mindmap.as_mut() {
            full.pending_preview_settle = None;
            full.pending_preview = None;
            full.preview = FullMindmapPreview::None;
        }
    }

    /// Full Mindmap-only file load. The wrapper retains request identity so a
    /// late result cannot close the navigator after a newer request supersedes
    /// it, and its error has a navigator-local home instead of `App::error`.
    pub(super) fn begin_full_mindmap_open(&mut self, path: PathBuf) -> Task<Message> {
        self.cancel_refresh_tracking();
        if let Some(blocked) = self.block_file_open_if_dirty() {
            return blocked;
        }
        let path = canonicalize_existing_path(path);
        self.invalidate_pending_quick_slot_restore();
        if self.full_mindmap.is_none() {
            return self.load_file_unless_dirty(path);
        }
        // Deliberate Full Mindmap activation supersedes a deferred IPC open.
        self.pending_ipc_file_open = None;
        let pending_refresh = self
            .full_mindmap
            .as_ref()
            .and_then(|full| full.pending_workspace_load.clone())
            .filter(|request| request.preserve_navigation);
        if let Some(request) = pending_refresh {
            // A hidden-filter refresh and file read must not race: accepting
            // the file first would exit Full Mindmap and make the snapshot
            // completion stale. Supersede the spawned refresh with a new
            // request whose accepted completion starts the file read.
            if request.exit_after_refresh {
                // Esc/toggle/Return to Files already owns the terminal intent.
                // A queued activation must not turn that exit into a file open.
                let checkpoint = self.checkpoint_active_quick_slot();
                self.cancel_full_mindmap_preview();
                return checkpoint;
            }
            // Enter is deliberate activation even while a refresh is queued:
            // save the outgoing slot context, then cancel the preview.
            let checkpoint = self.checkpoint_active_quick_slot();
            self.cancel_full_mindmap_preview();
            let load = self.begin_full_mindmap_workspace_load(
                request.path,
                request.select_root,
                Some(path),
                true,
                request.return_to_files_after,
                false,
            );
            return Task::batch([checkpoint, load]);
        }
        // Enter is deliberate activation: cancel any read-only preview settle
        // or in-flight preview only after its outgoing slot context is saved.
        let checkpoint = self.checkpoint_active_quick_slot();
        self.cancel_full_mindmap_preview();
        // Full Mindmap file activation is app-owned navigation away from the
        // current preview. Preserve its outgoing Quick Slot context before
        // replacing the pending preview/open request.
        self.full_mindmap_request_seq = self.full_mindmap_request_seq.wrapping_add(1);
        let request = PendingFullMindmapOpen {
            id: self.full_mindmap_request_seq,
            path: path.clone(),
        };
        let Some(full) = self.full_mindmap.as_mut() else {
            return checkpoint;
        };
        full.pending_open = Some(request.clone());
        full.load_error = None;
        Task::batch([
            checkpoint,
            Task::perform(load_file(path), move |result| {
                Message::FullMindmapFileLoaded { request, result }
            }),
        ])
    }

    /// Select a workspace node and independently settle a bounded, read-only
    /// preview when it is a file. Preview loads deliberately bypass the dirty
    /// guard because they never alter the current document.
    pub(super) fn select_full_mindmap_node(&mut self, id: WorkspaceNodeId) -> Task<Message> {
        let checkpoint = self.checkpoint_active_quick_slot();
        self.invalidate_pending_quick_slot_restore();
        // A manual canvas selection supersedes any pending slot-specific
        // preview snap. Slot activation sets the snap directly and does not
        // route through this selection handler.
        self.quick_slot_preview_restore = None;
        let node = self
            .full_mindmap_graph()
            .and_then(|graph| graph.node(&id).cloned());
        let preview_path = node
            .as_ref()
            .and_then(|node| {
                matches!(&node.kind, WorkspaceNodeKind::File)
                    .then(|| node.path.clone())
                    .flatten()
            })
            .or_else(|| match &id {
                WorkspaceNodeId::File(path)
                    if self.workspace_files.contains(path)
                        && self
                            .workspace
                            .as_ref()
                            .is_some_and(|root| path.starts_with(root)) =>
                {
                    Some(path.clone())
                }
                _ => None,
            });
        let active_matches = self.quick_slots.active.is_some_and(|index| {
            let Some(path) = preview_path.as_ref() else {
                return false;
            };
            let Some(root) = self.quick_slots_workspace_root() else {
                return false;
            };
            let Some(relative) = crate::quick_slots::relative_path(root, path) else {
                return false;
            };
            self.quick_slots
                .occupied(index)
                .is_some_and(|slot| slot.relative_path == relative)
        });
        if self.quick_slots.active.is_some() && !active_matches {
            // Manual canvas navigation is no longer viewing the active slot's
            // file. Clear only the active marker; the bookmarked slot itself
            // remains intact for later activation.
            self.quick_slots.active = None;
            self.persist_quick_slots_now();
        }
        if let Some(full) = self.full_mindmap.as_mut() {
            full.deferred_file_selection = match &id {
                WorkspaceNodeId::File(path) if node.is_none() => Some(path.clone()),
                _ => None,
            };
            full.selected = Some(id.clone());
            full.focus_request = Some(id);
            // A background hidden-entry refresh describes the same workspace
            // and intentionally captures navigation at completion time. Keep
            // it alive across ordinary selection changes; project switches
            // and parent loads still retain their existing cancellation path.
            if full
                .pending_workspace_load
                .as_ref()
                .is_some_and(|request| !request.preserve_navigation)
            {
                full.pending_workspace_load = None;
            }
            full.load_error = None;
        }
        // `schedule_full_mindmap_preview` owns the preview reset boundary. Do
        // not reset here: same-path reselection must preserve a ready preview
        // and, more importantly, must not cancel its in-flight latest request
        // before the same-path ownership check runs.
        let preview = self.schedule_full_mindmap_preview(preview_path);
        Task::batch([
            checkpoint,
            preview,
            iced::widget::operation::scroll_to(
                Self::full_mindmap_preview_scroll_id(),
                iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: 0.0 },
            ),
        ])
    }

    /// Queue a short settle window for a file selected by keyboard or canvas.
    /// The preview remains in a neutral loading state while the user is still
    /// moving; only the accepted settle message starts the bounded filesystem
    /// read and parser. Every newer selection replaces this identity, so a
    /// timer that cannot be aborted is harmless when it eventually fires.
    pub(super) fn schedule_full_mindmap_preview(&mut self, path: Option<PathBuf>) -> Task<Message> {
        let Some(path) = path else {
            self.cancel_full_mindmap_preview();
            return Task::none();
        };
        let Some(full) = self.full_mindmap.as_ref() else {
            return Task::none();
        };
        let already_owned = full
            .pending_preview_settle
            .as_ref()
            .is_some_and(|pending| pending.path == path)
            || full
                .pending_preview
                .as_ref()
                .is_some_and(|pending| pending.path == path);
        let already_ready = full.pending_preview.is_none()
            && matches!(
                &full.preview,
                FullMindmapPreview::Document { path: current, .. }
                    | FullMindmapPreview::Data { path: current, .. }
                    if current == &path
            );
        if already_owned {
            return Task::none();
        }
        if already_ready {
            // Re-selection must not reread/reparse a retained preview, but a
            // soft cache trim may have evicted one of its remote assets. The
            // preview identity remains stable while this lightweight pass
            // reclaims only missing image/diagram work.
            return self.prime_full_mindmap_preview_assets();
        }

        // Selection changed: reset only preview-owned virtualization and
        // measurement state before the new settle/read request takes over.
        self.reset_full_mindmap_preview_window();
        self.full_mindmap_request_seq = self.full_mindmap_request_seq.wrapping_add(1);
        let request = PendingFullMindmapPreviewSettle {
            id: self.full_mindmap_request_seq,
            path: path.clone(),
        };
        // Subscribe before publishing the request. The persistent worker is
        // normally started lazily on the first file selection; publishing to
        // the watch channel first would let a newly-created receiver start at
        // the already-published version and miss the settle wake-up if the
        // stream waits for `changed()` before inspecting its current value.
        let settle_worker = self.start_full_mindmap_preview_settle_worker();
        let Some(full) = self.full_mindmap.as_mut() else {
            return settle_worker;
        };
        // A different selection supersedes both an older timer and an older
        // read. The old future may still complete, but its request no longer
        // matches `pending_preview` and is ignored.
        full.pending_preview_settle = Some(request.clone());
        full.pending_preview = None;
        full.preview = FullMindmapPreview::Loading(path);
        let _ = full.preview_settle_tx.send(Some(request));
        settle_worker
    }

    /// Start the one persistent latest-only debounce worker for this navigator
    /// instance. Every subsequent selection replaces the watch value; stale
    /// sleeps are abandoned by the worker rather than accumulating one task per
    /// key repeat.
    pub(super) fn start_full_mindmap_preview_settle_worker(&mut self) -> Task<Message> {
        let Some(full) = self.full_mindmap.as_mut() else {
            return Task::none();
        };
        if full.preview_settle_worker_started {
            return Task::none();
        }
        full.preview_settle_worker_started = true;
        let receiver = full.preview_settle_tx.subscribe();
        // Keep one stream alive for the navigator lifetime. A one-shot task
        // can resolve its settle message before update() handles it; if a
        // newer selection resets the watch in that gap, the next request
        // would otherwise send into a receiver with no live worker and stay
        // on `Loading preview…` forever.
        Task::run(full_mindmap_preview_settle_stream(receiver), |request| {
            Message::FullMindmapPreviewSettle { request }
        })
    }

    pub(super) fn begin_full_mindmap_preview(&mut self, path: Option<PathBuf>) -> Task<Message> {
        let Some(path) = path else {
            self.reset_full_mindmap_preview_window();
            if let Some(full) = self.full_mindmap.as_mut() {
                full.pending_preview_settle = None;
                full.pending_preview = None;
                full.preview = FullMindmapPreview::None;
            }
            return Task::none();
        };
        let already_ready = self.full_mindmap.as_ref().is_some_and(|full| {
            full.pending_preview
                .as_ref()
                .is_some_and(|pending| pending.path == path)
                || (full.pending_preview.is_none()
                    && matches!(
                    &full.preview,
                    FullMindmapPreview::Document { path: current, .. }
                        | FullMindmapPreview::Data { path: current, .. }
                        if current == &path
                    ))
        });
        if already_ready {
            return Task::none();
        }
        if self.full_mindmap.is_none() {
            return Task::none();
        }
        self.full_mindmap_request_seq = self.full_mindmap_request_seq.wrapping_add(1);
        let request = PendingFullMindmapPreview {
            id: self.full_mindmap_request_seq,
            path: path.clone(),
        };
        let Some(full) = self.full_mindmap.as_mut() else {
            return Task::none();
        };
        let preview_work_epoch = full.preview_work_epoch.load(Ordering::Acquire);
        let preview_work_cancel = Arc::clone(&full.preview_work_epoch);
        // This direct path bypasses the settle timer; clear the watch value
        // as well as the compatibility field so an old worker cannot emit its
        // request again after this accepted read starts.
        let _ = full.preview_settle_tx.send(None);
        full.pending_preview_settle = None;
        full.pending_preview = Some(request.clone());
        full.preview_identity = Some(request.clone());
        full.preview_measurement_generation = None;
        full.preview = FullMindmapPreview::Loading(path.clone());
        Task::perform(
            load_full_mindmap_preview_guarded(path, preview_work_cancel, preview_work_epoch),
            move |result| Message::FullMindmapPreviewLoaded { request, result },
        )
    }

    /// Start parsing an already-open document for the initial Full Mindmap
    /// preview without doing parser/highlighter work on the update thread.
    /// Unlike selection-driven previews this intentionally skips the 300 ms
    /// settle because the document is already the accepted current file.
    pub(super) fn begin_full_mindmap_preview_source(
        &mut self,
        path: PathBuf,
        source: Arc<str>,
    ) -> Task<Message> {
        if self.full_mindmap.is_none() {
            return Task::none();
        }
        let keep_quick_slot_restore =
            self.quick_slot_preview_restore_guard
                .as_ref()
                .is_some_and(|guard| {
                    self.quick_slot_restore_is_current(guard)
                        && self
                            .quick_slots_workspace_root()
                            .and_then(|root| {
                                crate::quick_slots::resolve_path(root, &guard.slot.relative_path)
                            })
                            .is_some_and(|target| target == path)
                });
        if !keep_quick_slot_restore {
            // Source reparses for a different/stale identity must invalidate
            // the position and its guard together. A current slot restore for
            // this same file survives until the accepted parse can consume it.
            self.quick_slot_preview_restore = None;
            self.quick_slot_preview_restore_guard = None;
        }
        // A source-backed reparse is a new accepted preview even when the
        // path is unchanged. Clear old measurement ownership/window shape so
        // pending g1 results cannot poison the new parse's geometry.
        self.reset_full_mindmap_preview_window();
        let Some(full) = self.full_mindmap.as_mut() else {
            return Task::none();
        };
        self.full_mindmap_request_seq = self.full_mindmap_request_seq.wrapping_add(1);
        let request = PendingFullMindmapPreview {
            id: self.full_mindmap_request_seq,
            path: path.clone(),
        };
        let epoch = full.preview_work_epoch.load(Ordering::Acquire);
        let cancel = Arc::clone(&full.preview_work_epoch);
        let parse_gate = Arc::clone(&full.preview_parse_gate);
        // Source-backed previews also bypass debounce. Explicitly cancel any
        // prior watch value before assigning this accepted identity.
        let _ = full.preview_settle_tx.send(None);
        full.pending_preview_settle = None;
        full.pending_preview = Some(request.clone());
        full.preview_identity = Some(request.clone());
        full.preview_measurement_generation = None;
        full.preview = FullMindmapPreview::Loading(path);
        Task::perform(
            parse_full_mindmap_preview_guarded(
                request.path.clone(),
                source,
                cancel,
                epoch,
                parse_gate,
            ),
            move |result| Message::FullMindmapPreviewParsed { request, result },
        )
    }

    /// Drop only Full Mindmap preview layout/scroll ownership. The document
    /// reader's `virt_window`, viewport, and height cache are intentionally
    /// untouched. A generation bump makes already-dispatched measurement
    /// operations harmless when selection changes.
    pub(super) fn reset_full_mindmap_preview_window(&mut self) {
        let stale_loading = self
            .full_mindmap
            .as_mut()
            .map(|full| full.preview_loading_images.drain().collect::<Vec<_>>())
            .unwrap_or_default();
        let stale_diagrams = self
            .full_mindmap
            .as_ref()
            .map(|full| {
                full.preview_pending_diagrams
                    .keys()
                    .copied()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for (url, _owner) in stale_loading {
            if matches!(self.image_cache.get(&url), Some(ImageState::Loading)) {
                self.image_cache.remove(&url);
            }
        }
        for key in stale_diagrams {
            if matches!(
                self.diagram_cache.peek(&key),
                Some(crate::diagram::DiagramState::Pending)
            ) {
                self.diagram_cache.remove(&key);
            }
        }
        if let Some(full) = self.full_mindmap.as_mut() {
            let _ = full.preview_settle_tx.send(None);
            full.preview_generation = full.preview_generation.wrapping_add(1);
            full.preview_window = crate::virt::VirtWindow::default();
            full.preview_viewport = None;
            full.preview_height_cache.clear();
            full.preview_measurement_pending = false;
            full.preview_measurement_range = None;
            full.preview_measurement_generation = None;
            full.preview_identity = None;
            full.preview_shape_ready = false;
            full.preview_work_epoch.fetch_add(1, Ordering::AcqRel);
            full.preview_pending_diagrams.clear();
            full.preview_asset_images.clear();
            full.preview_failed_images.clear();
            full.preview_asset_diagrams.clear();
            full.preview_asset_cursor = 0;
            full.preview_asset_cursor_tag = None;
            full.preview_asset_wave_id = full.preview_asset_wave_id.wrapping_add(1);
        }
    }

    pub(super) fn full_mindmap_preview_body_offset(full: &FullMindmapState) -> f32 {
        full.preview_viewport
            .as_ref()
            .map(|viewport| viewport.absolute_offset().y.max(0.0))
            .unwrap_or(0.0)
    }

    /// Rebuild the preview's shared virtual window from its own viewport and
    /// measured-height cache. This mirrors `rebuild_virt_here` but never reads
    /// or mutates document state.
    pub(super) fn rebuild_full_mindmap_preview_here(&mut self) {
        let Some(full) = self.full_mindmap.as_mut() else {
            return;
        };
        let offset = Self::full_mindmap_preview_body_offset(full);
        let viewport_h = full
            .preview_viewport
            .as_ref()
            .map(|viewport| viewport.bounds().height)
            .or(self.window_size.map(|size| size.height))
            .unwrap_or(1000.0);
        if matches!(full.preview, FullMindmapPreview::Document { .. }) {
            if !full.preview_shape_ready {
                // Move the worker-built shape out of the enum so the normal
                // runtime handoff is O(1); the Arc is uniquely owned here.
                let shape = match &mut full.preview {
                    FullMindmapPreview::Document { shape, .. } => shape.take(),
                    _ => None,
                };
                if let Some(shape) = shape {
                    full.preview_window
                        .install_shape_arc(shape, offset, viewport_h);
                } else if let FullMindmapPreview::Document { blocks, .. } = &full.preview {
                    // Small in-memory fixtures and older callers may not carry
                    // worker geometry. Runtime complete previews always do.
                    full.preview_window.rebuild(
                        blocks,
                        &HashSet::new(),
                        &full.preview_height_cache,
                        offset,
                        viewport_h,
                    );
                }
                full.preview_shape_ready = true;
            } else {
                full.preview_window
                    .rebuild_at_current_shape_for_preview(offset, viewport_h);
            }
        } else {
            full.preview_window = crate::virt::VirtWindow::default();
            full.preview_shape_ready = false;
        }
    }

    pub(super) fn refresh_full_mindmap_preview_heights(&mut self) -> Task<Message> {
        if let Some(full) = self.full_mindmap.as_mut() {
            full.preview_generation = full.preview_generation.wrapping_add(1);
            full.preview_height_cache.clear();
            full.preview_window.clear_height_adjustments();
            if !full.preview_measurement_pending {
                full.preview_measurement_range = None;
                full.preview_measurement_generation = None;
            }
        }
        self.rebuild_full_mindmap_preview_here();
        Task::batch([
            self.prime_full_mindmap_preview_assets(),
            self.measure_full_mindmap_preview_heights(),
        ])
    }

    /// Dispatch a widget operation that measures only the materialized
    /// preview blocks. The path and generation travel with the result so a
    /// late operation cannot feed the document cache or a newer selection.
    pub(super) fn measure_full_mindmap_preview_heights(&mut self) -> Task<Message> {
        let Some(full) = self.full_mindmap.as_mut() else {
            return Task::none();
        };
        let FullMindmapPreview::Document { path, blocks, .. } = &full.preview else {
            return Task::none();
        };
        if !full.preview_window.active || full.preview_measurement_pending {
            return Task::none();
        }
        let (start, end) = full.preview_window.range;
        let targets: HashMap<iced::widget::Id, crate::ast::BlockId> = full.preview_window.display
            [start.min(full.preview_window.display.len())
                ..end.min(full.preview_window.display.len())]
            .iter()
            .filter_map(|&idx| blocks.get(idx).map(|(id, _)| *id))
            .filter(|id| !full.preview_height_cache.is_measured(*id))
            .map(|id| (crate::render::block_anchor_id(id), id))
            .collect();
        if targets.is_empty() {
            return Task::none();
        }
        full.preview_measurement_pending = true;
        full.preview_measurement_range = Some(full.preview_window.range);
        full.preview_measurement_generation = Some(full.preview_generation);
        measure_full_mindmap_preview_block_heights(
            path.clone(),
            full.preview_namespace,
            full.preview_identity
                .as_ref()
                .map(|request| request.id)
                .unwrap_or(0),
            full.preview_generation,
            targets,
            Self::full_mindmap_preview_body_offset(full),
        )
    }

    /// Stable identity for the Full Mindmap read-only preview. This is kept
    /// distinct from the document body scrollable so selecting a file never
    /// reuses or mutates document scroll state.
    pub(super) fn full_mindmap_preview_scroll_id() -> iced::widget::Id {
        iced::widget::Id::new("full-mindmap-preview")
    }

    /// Drop Loading/Pending ownership from an older virtual range before a
    /// new visible wave primes. The old futures may still complete, but their
    /// range-tagged messages can no longer remove the new wave's sentinels.
    /// This also prevents hung off-screen image requests from consuming the
    /// current wave's 64-operation cap forever.
    pub(super) fn reconcile_full_mindmap_preview_asset_wave(
        &mut self,
        asset_identity: &FullMindmapPreviewAssetIdentity,
    ) {
        let stale_images = self
            .full_mindmap
            .as_ref()
            .map(|full| {
                full.preview_loading_images
                    .iter()
                    .filter(|(_, owner)| *owner != asset_identity)
                    .map(|(url, _)| url.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let stale_diagrams = self
            .full_mindmap
            .as_ref()
            .map(|full| {
                full.preview_pending_diagrams
                    .iter()
                    .filter(|(_, owner)| *owner != asset_identity)
                    .map(|(key, _)| *key)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for url in stale_images {
            if let Some(full) = self.full_mindmap.as_mut() {
                full.preview_loading_images.remove(&url);
            }
            if matches!(self.image_cache.get(&url), Some(ImageState::Loading)) {
                self.image_cache.remove(&url);
            }
        }
        for key in stale_diagrams {
            if let Some(full) = self.full_mindmap.as_mut() {
                full.preview_pending_diagrams.remove(&key);
            }
            if matches!(
                self.diagram_cache.peek(&key),
                Some(crate::diagram::DiagramState::Pending)
            ) {
                self.diagram_cache.remove(&key);
            }
        }
    }

    /// Prime the same remote-image and diagram caches used by normal document
    /// rendering, but tag every asynchronous result with this preview's
    /// namespace/request identity. Stale preview assets are therefore dropped
    /// without mutating a newer file's lifecycle.
    pub(super) fn prime_full_mindmap_preview_assets(&mut self) -> Task<Message> {
        let Some(full) = self.full_mindmap.as_ref() else {
            return Task::none();
        };
        let Some(request) = full.preview_identity.as_ref() else {
            return Task::none();
        };
        let FullMindmapPreview::Document { blocks, assets, .. } = &full.preview else {
            return Task::none();
        };
        let identity = FullMindmapPreviewIdentity {
            namespace: full.preview_namespace,
            request: request.clone(),
        };
        let range = full.preview_window.range;
        let tag = (identity.namespace, identity.request.id, range);
        let cursor = if full.preview_asset_cursor_tag == Some(tag) {
            full.preview_asset_cursor
        } else {
            0
        };
        let display_len = full.preview_window.display.len();
        let visible_start = range.0.min(display_len);
        let visible_end = range.1.min(display_len);
        // Copy only the currently materialized virtual slice. Cloning the
        // worker-built display vector here would reintroduce an O(all-blocks)
        // UI allocation on every scroll/asset completion.
        let visible_indices = full.preview_window.display[visible_start..visible_end].to_vec();
        let visible_ids = visible_indices
            .iter()
            .filter_map(|&idx| blocks.get(idx).map(|(id, _)| *id))
            .collect::<Vec<_>>();
        let index = assets.clone();
        let visible_blocks = if index.is_none() {
            visible_indices
                .iter()
                .filter_map(|&idx| blocks.get(idx).cloned())
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let mut total = 0usize;
        let mut skipped = 0usize;
        let mut descriptors = Vec::new();
        if let Some(index) = &index {
            total = visible_ids
                .iter()
                .filter_map(|id| index.by_block.get(id))
                .map(Vec::len)
                .sum();
            'indexed: for id in &visible_ids {
                let Some(assets) = index.by_block.get(id) else {
                    continue;
                };
                for asset in assets {
                    if skipped < cursor {
                        skipped += 1;
                        continue;
                    }
                    if descriptors.len() >= FULL_MINDMAP_PREVIEW_ASSET_BATCH {
                        break 'indexed;
                    }
                    skipped += 1;
                    descriptors.push(asset.clone());
                }
            }
        } else {
            for (_, block) in &visible_blocks {
                let mut images = Vec::new();
                let mut diagrams = Vec::new();
                collect_preview_assets(block, &mut images, &mut diagrams);
                let assets = images
                    .into_iter()
                    .map(FullMindmapPreviewAsset::Image)
                    .chain(diagrams.into_iter().map(|(hash, kind, source)| {
                        FullMindmapPreviewAsset::Diagram { hash, kind, source }
                    }))
                    .collect::<Vec<_>>();
                total = total.saturating_add(assets.len());
                for asset in assets {
                    if skipped < cursor {
                        skipped += 1;
                        continue;
                    }
                    if descriptors.len() >= FULL_MINDMAP_PREVIEW_ASSET_BATCH {
                        skipped += 1;
                        continue;
                    }
                    skipped += 1;
                    descriptors.push(asset);
                }
            }
        }

        // A changed visible range starts a fresh lazy wave and drops off-screen
        // keep-set ownership. Subsequent waves for the same tag append only
        // descriptors from this range.
        if let Some(full) = self.full_mindmap.as_mut() {
            if full.preview_asset_cursor_tag != Some(tag) {
                full.preview_asset_wave_id = full.preview_asset_wave_id.wrapping_add(1);
                full.preview_asset_images.clear();
                full.preview_failed_images.clear();
                full.preview_asset_diagrams.clear();
                full.preview_asset_cursor_tag = Some(tag);
            }
        }

        let mut tasks = Vec::new();
        let mut seen_images = HashSet::new();
        let mut seen_diagrams = HashSet::new();
        let mut consumed = 0usize;
        let mut dispatched = 0usize;
        let mut blocked = false;
        let theme_id = self.diagram_theme_id;
        let palette = self.palette;
        let asset_identity = FullMindmapPreviewAssetIdentity {
            preview: identity.clone(),
            range,
            wave: self
                .full_mindmap
                .as_ref()
                .map(|full| full.preview_asset_wave_id)
                .unwrap_or(0),
        };
        self.reconcile_full_mindmap_preview_asset_wave(&asset_identity);
        for asset in descriptors {
            match asset {
                FullMindmapPreviewAsset::Image(url) => {
                    if !is_remote_url(&url) || !seen_images.insert(url.clone()) {
                        consumed += 1;
                        continue;
                    }
                    if let Some(full) = self.full_mindmap.as_mut() {
                        full.preview_asset_images.insert(url.clone());
                    }
                    let preview_failed = self
                        .full_mindmap
                        .as_ref()
                        .is_some_and(|full| full.preview_failed_images.contains(&url));
                    if preview_failed {
                        consumed += 1;
                        continue;
                    }
                    let cache_is_loading =
                        matches!(self.image_cache.get(&url), Some(ImageState::Loading));
                    let cache_is_loaded = matches!(
                        self.image_cache.get(&url),
                        Some(ImageState::Loaded(_) | ImageState::LoadedSvg { .. })
                    );
                    let preview_owns_loading = self.full_mindmap.as_ref().is_some_and(|full| {
                        full.preview_loading_images
                            .get(&url)
                            .is_some_and(|owner| owner == &asset_identity)
                    });
                    if cache_is_loaded || (cache_is_loading && preview_owns_loading) {
                        consumed += 1;
                        continue;
                    }
                    if self.full_mindmap.as_ref().is_some_and(|full| {
                        full.preview_loading_images.len() >= FULL_MINDMAP_PREVIEW_ASSET_BATCH
                    }) {
                        blocked = true;
                        break;
                    }
                    if self.image_cache.contains_key(&url) && !preview_owns_loading {
                        self.image_cache.remove(&url);
                    }
                    self.image_cache.insert(url.clone(), ImageState::Loading);
                    if let Some(full) = self.full_mindmap.as_mut() {
                        full.preview_loading_images
                            .insert(url.clone(), asset_identity.clone());
                    }
                    let task_identity = identity.clone();
                    let task_url = url.clone();
                    tasks.push(Task::perform(
                        fetch_image(task_url.clone()),
                        move |(_, result)| Message::FullMindmapPreviewImageFetched {
                            identity: task_identity.clone(),
                            range,
                            wave: asset_identity.wave,
                            url: task_url.clone(),
                            result,
                        },
                    ));
                    consumed += 1;
                    dispatched += 1;
                }
                FullMindmapPreviewAsset::Diagram { hash, kind, source } => {
                    let key = (hash, theme_id);
                    if !seen_diagrams.insert(hash) {
                        consumed += 1;
                        continue;
                    }
                    if let Some(full) = self.full_mindmap.as_mut() {
                        full.preview_asset_diagrams.insert(key);
                    }
                    let cache_is_pending = matches!(
                        self.diagram_cache.peek(&key),
                        Some(crate::diagram::DiagramState::Pending)
                    );
                    let preview_owns_pending = self.full_mindmap.as_ref().is_some_and(|full| {
                        full.preview_pending_diagrams
                            .get(&key)
                            .is_some_and(|owner| owner == &asset_identity)
                    });
                    if self.diagram_cache.peek(&key).is_some()
                        && (!cache_is_pending || preview_owns_pending)
                    {
                        consumed += 1;
                        continue;
                    }
                    if self.full_mindmap.as_ref().is_some_and(|full| {
                        full.preview_pending_diagrams.len() >= FULL_MINDMAP_PREVIEW_ASSET_BATCH
                    }) {
                        blocked = true;
                        break;
                    }
                    if cache_is_pending && !preview_owns_pending {
                        self.diagram_cache.remove(&key);
                    }
                    self.diagram_cache
                        .put(key, crate::diagram::DiagramState::Pending);
                    if let Some(full) = self.full_mindmap.as_mut() {
                        full.preview_pending_diagrams
                            .insert(key, asset_identity.clone());
                    }
                    let task_identity = identity.clone();
                    tasks.push(Task::perform(
                        crate::diagram::render_blocking_async(
                            kind,
                            source,
                            palette,
                            "JetBrains Mono".into(),
                        ),
                        move |result| Message::FullMindmapPreviewDiagramRendered {
                            identity: task_identity.clone(),
                            range,
                            wave: asset_identity.wave,
                            hash,
                            theme_id,
                            result,
                        },
                    ));
                    consumed += 1;
                    dispatched += 1;
                }
            }
        }
        let next_cursor = cursor.saturating_add(consumed);
        let has_more = next_cursor < total;
        if let Some(full) = self.full_mindmap.as_mut() {
            full.preview_asset_cursor = next_cursor;
        }
        let next_wave = if has_more && dispatched == 0 && !blocked {
            Task::done(Message::FullMindmapPreviewAssetWave {
                identity,
                range,
                wave: asset_identity.wave,
            })
        } else {
            Task::none()
        };
        if tasks.is_empty() {
            next_wave
        } else {
            Task::batch([Task::batch(tasks), next_wave])
        }
    }

    /// Theme ids are part of diagram cache identity. When the palette changes,
    /// discard only preview-owned Pending work from the old theme and rebuild
    /// the retained preview's diagram asset set under the new key immediately.
    /// Loaded image assets remain reusable across theme changes.
    pub(super) fn refresh_full_mindmap_preview_assets_for_theme(&mut self) -> Task<Message> {
        let stale = self
            .full_mindmap
            .as_ref()
            .map(|full| {
                full.preview_pending_diagrams
                    .keys()
                    .filter(|(_, theme_id)| *theme_id != self.diagram_theme_id)
                    .copied()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for key in stale {
            if matches!(
                self.diagram_cache.peek(&key),
                Some(crate::diagram::DiagramState::Pending)
            ) {
                self.diagram_cache.remove(&key);
            }
        }
        let theme_id = self.diagram_theme_id;
        if let Some(full) = self.full_mindmap.as_mut() {
            full.preview_pending_diagrams
                .retain(|(_, id), _| *id == theme_id);
            full.preview_asset_diagrams
                .retain(|(_, id)| *id == theme_id);
            full.preview_failed_images.clear();
            full.preview_asset_cursor = 0;
            full.preview_asset_cursor_tag = None;
        }
        self.prime_full_mindmap_preview_assets()
    }
}
