//! CLI/IPC request handling: the request handler, goto helpers, and the
//! subscription that feeds requests into the app.

use super::*;

impl App {
    /// Start an IPC file activation only after Full Mindmap has stopped owning
    /// the reader surface. A normal exit clears the navigator synchronously;
    /// when the hidden-file snapshot is stale, `exit_full_mindmap` completes
    /// through `FullMindmapWorkspaceLoaded` and consumes the same pending open
    /// there.
    pub(super) fn begin_ipc_file_open(
        &mut self,
        path: PathBuf,
        nav: Option<PendingNav>,
    ) -> Task<Message> {
        // An IPC/file-finder navigation supersedes any in-flight slot load;
        // its completion must not later reopen the older slot target.
        let path = canonicalize_existing_path(path);
        // Capture the outgoing reading context before IPC navigation changes
        // the visible file (or asks Full Mindmap to relinquish its preview).
        // `checkpoint_active_quick_slot` mutates the in-memory slot
        // synchronously; the returned task only debounces persistence.
        let checkpoint = self.checkpoint_active_quick_slot();
        self.invalidate_pending_quick_slot_restore();
        if self.full_mindmap.is_none() {
            self.pending_nav = nav;
            return Task::batch([checkpoint, self.begin_generic_file_load(path)]);
        }

        // Do not let an older generic load consume the navigation intended for
        // this deferred request while the navigator is still visible.
        self.pending_nav = None;
        self.pending_ipc_file_open = Some(PendingIpcFileOpen { path, nav });
        let exit = self.exit_full_mindmap(false);
        if self.full_mindmap.is_none() {
            Task::batch([checkpoint, self.start_pending_ipc_file_open(exit)])
        } else {
            Task::batch([checkpoint, exit])
        }
    }

    pub(super) fn start_pending_ipc_file_open(&mut self, cleanup: Task<Message>) -> Task<Message> {
        let Some(PendingIpcFileOpen { path, nav }) = self.pending_ipc_file_open.take() else {
            return cleanup;
        };
        self.cancel_refresh_tracking();
        self.pending_nav = nav;
        let load = self.begin_generic_file_load(path);
        Task::batch([cleanup, load])
    }

    /// Answer one CLI/IPC request and return any follow-up work.
    pub(super) fn handle_ipc(
        &mut self,
        req: crate::ipc::Request,
        tx: std::sync::Arc<
            std::sync::Mutex<Option<futures::channel::oneshot::Sender<crate::ipc::Response>>>,
        >,
    ) -> Task<Message> {
        use crate::ipc::{Cmd, FocusBehavior, Mode, Response};
        let id = req.id;
        let mut follow_up: Task<Message> = Task::none();
        // Tracks whether the handler should chain a focus-raise after
        // the response. `Some(true)` = force raise, `Some(false)` =
        // explicit suppress, `None` = not a nav command.
        let mut nav_focus: Option<FocusBehavior> = None;
        // Screenshot replies only after the file is written, so its
        // handler stashes the sender and suppresses the sync reply.
        let mut defer_reply = false;
        let resp = match req.cmd {
            Cmd::Current => {
                let mode = match self.view_mode {
                    ViewMode::Rendered => "view",
                    ViewMode::Raw => "edit",
                    ViewMode::Mindmap => "mindmap",
                };
                let body = serde_json::json!({
                    "file": self.file.as_ref().map(|p| p.to_string_lossy().into_owned()),
                    "line": current_line_estimate(self),
                    "mode": mode,
                    "folder": self.workspace.as_ref().map(|p| p.to_string_lossy().into_owned()),
                });
                Response::ok_with(id, body)
            }
            Cmd::Focus => {
                follow_up = iced::window::latest().and_then(|wid| iced::window::gain_focus(wid));
                Response::ok(id)
            }
            Cmd::Close => {
                follow_up = iced::window::latest().and_then(|wid| iced::window::close(wid));
                Response::ok(id)
            }
            Cmd::Mode { mode, focus } => {
                let is_pdf = is_pdf_path(self.file.as_deref());
                match mode {
                    Mode::View => {
                        if self.view_mode == ViewMode::Raw {
                            follow_up = self.exit_zen_edit_mode();
                        } else {
                            self.view_mode = ViewMode::Rendered;
                        }
                    }
                    // PDFs are view-only: coerce edit requests to View.
                    Mode::Edit if is_pdf => self.view_mode = ViewMode::Rendered,
                    Mode::Edit => {
                        follow_up = self.enter_zen_edit_mode();
                    }
                    Mode::Mindmap => {
                        if self.view_mode == ViewMode::Raw {
                            self.sync_editor_to_source();
                            self.editor = None;
                            self.editor_text = None;
                            self.edit_history.clear();
                            self.edit_redo.clear();
                            self.restore_zen_chrome();
                        }
                        self.view_mode = ViewMode::Mindmap;
                    }
                }
                nav_focus = Some(focus);
                Response::ok(id)
            }
            Cmd::OpenFolder { dir } => {
                follow_up = Task::done(Message::OpenWorkspace(std::path::PathBuf::from(dir)));
                Response::ok(id)
            }
            Cmd::Reveal { file, focus } => {
                if self.dirty {
                    Response::err(id, self.unsaved_edits_open_message())
                } else {
                    follow_up = self.begin_ipc_file_open(std::path::PathBuf::from(file), None);
                    nav_focus = Some(focus);
                    Response::ok(id)
                }
            }
            Cmd::Open {
                file,
                line,
                section,
                focus,
            } => {
                if self.dirty {
                    Response::err(id, self.unsaved_edits_open_message())
                } else {
                    let path = std::path::PathBuf::from(file);
                    follow_up = self.begin_ipc_file_open(
                        path,
                        Some(PendingNav {
                            line,
                            section,
                            ..Default::default()
                        }),
                    );
                    nav_focus = Some(focus);
                    Response::ok(id)
                }
            }
            Cmd::Goto {
                line,
                section,
                focus,
            } => {
                nav_focus = Some(focus);
                apply_goto(self, id, line, section)
            }
            Cmd::Screenshot { path } => {
                // Capture is async: stash the path + sender, fire the
                // window screenshot, and reply once the PNG is written.
                self.pending_screenshot = Some((
                    std::path::PathBuf::from(path),
                    Some(std::sync::Arc::clone(&tx)),
                ));
                follow_up = iced::window::latest()
                    .and_then(iced::window::screenshot)
                    .map(Message::ScreenshotCaptured);
                defer_reply = true;
                Response::ok(id)
            }
            Cmd::Resize { width, height } => {
                follow_up = iced::window::latest().and_then(move |wid| {
                    iced::window::resize(wid, iced::Size::new(width as f32, height as f32))
                });
                Response::ok(id)
            }
            Cmd::Theme { slug } => match theme::preset_by_slug(&slug) {
                Some(preset) => {
                    follow_up = Task::done(Message::SetTheme(preset));
                    Response::ok(id)
                }
                None => Response::err(id, format!("unknown theme: {slug}")),
            },
            Cmd::DemoBanner { version } => {
                // Fake a ready update purely to render the banner. The
                // artifact path is empty, so "Install" would no-op — this
                // is for demos/screenshots only.
                self.pending_update = Some(crate::update::ReadyUpdate {
                    version,
                    notes_url: None,
                    artifact: std::path::PathBuf::new(),
                    sha256: String::new(),
                });
                Response::ok(id)
            }
        };
        if !defer_reply {
            Self::reply(&tx, resp);
        }
        let should_focus = match nav_focus {
            Some(FocusBehavior::Force) => true,
            Some(FocusBehavior::Suppress) => false,
            Some(FocusBehavior::Default) => self.prefs.auto_focus_on_nav,
            None => false,
        };
        if should_focus {
            let raise = iced::window::latest().and_then(|wid| iced::window::gain_focus(wid));
            follow_up = Task::batch([follow_up, raise]);
        }
        follow_up
    }

    pub(super) fn reply(
        tx: &std::sync::Arc<
            std::sync::Mutex<Option<futures::channel::oneshot::Sender<crate::ipc::Response>>>,
        >,
        resp: crate::ipc::Response,
    ) {
        if let Some(sender) = tx.lock().ok().and_then(|mut g| g.take()) {
            let _ = sender.send(resp);
        }
    }
}

/// Build the in-app navigation message used by link anchors and outline clicks.
pub(super) fn goto_line_message(line: u32) -> Message {
    Message::ScrollToLine(line)
}

pub(super) fn apply_goto(
    app: &mut App,
    id: u64,
    line: Option<u32>,
    section: Option<String>,
) -> crate::ipc::Response {
    use crate::ipc::Response;
    if app.ast.is_empty() {
        return Response::err(id, "no file open");
    }
    let target_line = if let Some(sec) = section {
        let sections = &app.outline_sections;
        match crate::ipc::sections::resolve_section_path(&sec, sections) {
            Some(s) => s.line,
            None => return Response::err(id, format!("section \"{sec}\" not found")),
        }
    } else if let Some(l) = line {
        let max_line = app.block_lines.last().copied().unwrap_or(1);
        if l > max_line.saturating_add(1000) {
            return Response::err(
                id,
                format!("line {l} out of range (file ends near line {max_line})"),
            );
        }
        l
    } else {
        return Response::err(id, "goto requires --line or --section");
    };

    let Some(idx) = crate::ipc::lines::block_for_line(target_line, &app.block_lines) else {
        return Response::err(id, "no blocks");
    };
    // Reveal a fold-hidden target (mirrors search nav) and materialize its
    // window so the precise scroll op queued below can find its widget.
    app.unfold_to_reveal(idx);
    app.rebuild_virt_around_block(idx);
    let Some(dpos) = app.virt_window.display_pos(idx) else {
        return Response::err(id, "could not locate block");
    };
    let block_top = app.virt_window.block_top(dpos);
    let block_h = app.virt_window.block_height(dpos);
    // Body estimate + the scrollable's vertical content padding (56 top/bottom).
    let estimated_h = app.virt_window.total_height() + 2.0 * BODY_TOP_PAD;
    let (content_h, view_h) = app
        .body_viewport
        .as_ref()
        .map(|v| {
            (
                v.content_bounds().height.max(estimated_h),
                v.bounds().height,
            )
        })
        .unwrap_or((estimated_h, 0.0));
    let max_scroll = (content_h - view_h).max(1.0);
    let target = BODY_TOP_PAD + block_top + block_h * 0.5 - view_h * 0.38;
    let rel = (target / max_scroll).clamp(0.0, 1.0);
    app.queued_snap = Some(rel);
    app.queued_goto = app.ast.get(idx).map(|(bid, _)| *bid);
    app.nav_anchor = Some(idx);
    crate::ipc::Response::ok(id)
}

pub(super) fn current_line_estimate(app: &App) -> Option<u32> {
    let v = app.body_viewport.as_ref()?;
    let content_h = v.content_bounds().height;
    let view_h = v.bounds().height;
    if content_h <= view_h {
        return app.block_lines.first().copied();
    }
    let body_off = (v.absolute_offset().y - BODY_TOP_PAD).max(0.0);
    let dpos = app.virt_window.display_pos_at(body_off)?;
    let ast_idx = *app.virt_window.display.get(dpos)?;
    app.block_lines.get(ast_idx).copied()
}

pub(super) fn ipc_subscription_stream() -> impl iced::futures::Stream<Item = Message> {
    iced::stream::channel(
        64,
        |mut out: futures::channel::mpsc::Sender<Message>| async move {
            let listener = match crate::ipc::server::acquire() {
                Ok(l) => l,
                Err(_) => return,
            };
            let (tx, mut rx) = futures::channel::mpsc::channel::<crate::ipc::server::Pending>(64);
            tokio::spawn(crate::ipc::server::run(listener, tx));
            use futures::SinkExt;
            use futures::StreamExt;
            while let Some((req, reply)) = rx.next().await {
                let wrapped = std::sync::Arc::new(std::sync::Mutex::new(Some(reply)));
                if out.send(Message::Ipc(req, wrapped)).await.is_err() {
                    break;
                }
            }
        },
    )
}
