//! Full Mindmap Mode canvas and side-panel views.

use super::*;
// Explicit import: the `column!` macro is otherwise ambiguous between the glob
// import and iced's exported macro.
use iced::widget::column;

impl App {
    /// Full-window workspace navigator. It deliberately reads only
    /// `full_mindmap` and `workspace_mindmap` state; document mindmap layout,
    /// collapse, selection, and preview state stay untouched underneath.
    pub(super) fn full_mindmap_view(
        &self,
        pal: Palette,
        recently_scrolled: bool,
    ) -> Element<'_, Message> {
        let Some(full) = self.full_mindmap.as_ref() else {
            return Space::new().into();
        };
        let Some(graph) = self.full_mindmap_graph() else {
            if let Some(load) = &full.pending_workspace_load {
                return container(
                    column![
                        text("Indexing workspace…").size(14).color(pal.fg),
                        text(load.path.display().to_string()).size(12).color(pal.muted),
                        text("Large folders are indexed in the background with a fixed safety limit.")
                            .size(12)
                            .color(pal.muted),
                    ]
                    .spacing(10),
                )
                .padding(24)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into();
            }
            return container(
                text("Workspace navigator unavailable — press Esc to return")
                    .size(14)
                    .color(pal.muted),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into();
        };

        let program = crate::mindmap::MindmapProgram::<WorkspaceNodeId, Message> {
            nodes: graph.nodes.clone(),
            content_size: graph.content_size,
            palette: pal,
            selected: full.selected.clone(),
            // Keep the explicit navigator request distinct from the selected
            // ring. The shared canvas uses `selected` as a compatibility
            // fallback only when no request exists; a present request always
            // targets the accepted node's final layout position.
            focus: full.focus_request.clone(),
            panel_open: full.panel_open,
            panel_width: full.panel_width,
            autocenter: true,
            layout_generation: Some(full.layout_generation),
            keyboard_zoom_enabled: self.overlay == Overlay::None,
            native_pinch_log: self.full_mindmap_native_pinch_log,
            on_toggle: Box::new(Message::FullMindmapToggleNode),
            on_select: Box::new(Message::FullMindmapSelectNode),
            on_deselect: Message::FullMindmapDeselect,
        };
        let canvas: Element<'_, Message> = iced::widget::canvas(program)
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
        let selected_is_file = full
            .selected
            .as_ref()
            .and_then(|id| graph.node(id))
            .is_some_and(|node| node.kind == WorkspaceNodeKind::File)
            && (full.pending_workspace_load.is_none()
                || full
                    .pending_workspace_load
                    .as_ref()
                    .is_some_and(|request| request.preserve_navigation));
        let hint_items: &[(&str, &str)] = match selected_is_file {
            true => &[("←↑→↓", "move"), ("= / −", "zoom"), ("Enter", "open")],
            false => &[
                ("←↑→↓", "move"),
                ("Space", "fold"),
                ("= / −", "zoom"),
                ("Enter", "root"),
            ],
        };
        let canvas: Element<'_, Message> = stack![canvas, floating_mindmap_hint(hint_items, pal)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
        let body: Element<'_, Message> = if full.panel_open {
            irow![
                canvas,
                full_mindmap_panel_resize_handle(pal),
                self.full_mindmap_panel_view(&graph, pal, full.panel_width, recently_scrolled),
            ]
            .into()
        } else {
            canvas
        };
        container(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(pal.bg.into()),
                ..Default::default()
            })
            .into()
    }

    pub(super) fn full_mindmap_panel_view(
        &self,
        graph: &WorkspaceGraph,
        pal: Palette,
        panel_width: f32,
        recently_scrolled: bool,
    ) -> Element<'_, Message> {
        let Some(full) = self.full_mindmap.as_ref() else {
            return Space::new().into();
        };
        let selected = full.selected.as_ref().and_then(|id| graph.node(id));
        let label = full
            .selected
            .as_ref()
            .and_then(|id| graph.index_of(id))
            .and_then(|idx| graph.nodes.get(idx))
            .map(|node| node.full_label.clone())
            .unwrap_or_else(|| "Select a folder or file".to_string());
        let selected_path = selected.and_then(|node| node.path.as_ref());
        let selected_is_file = selected.is_some_and(|node| node.kind == WorkspaceNodeKind::File)
            && (full.pending_workspace_load.is_none()
                || full
                    .pending_workspace_load
                    .as_ref()
                    .is_some_and(|request| request.preserve_navigation));
        let preview_widget_generation = (
            full.preview_identity
                .as_ref()
                .or_else(|| full.pending_preview.as_ref())
                .map(|request| request.id)
                .or_else(|| {
                    full.pending_preview_settle
                        .as_ref()
                        .map(|request| request.id)
                })
                .unwrap_or(0),
            full.preview_generation,
        );
        let content: Element<'_, Message> =
            if selected_is_file {
                let preview: Element<'_, Message> = match &full.preview {
                    FullMindmapPreview::Loading(path) if Some(path) == selected_path => {
                        container(text("Loading preview…").size(13).color(pal.muted))
                            .padding(24)
                            .into()
                    }
                    FullMindmapPreview::Document {
                        path,
                        blocks,
                        truncated,
                        ..
                    } if Some(path) == selected_path => {
                        let rendered = crate::render::render(
                            blocks,
                            &pal,
                            &self.typography,
                            &Highlight::default(),
                            Some(&full.preview_window),
                            &self.image_cache,
                            Some(path.as_path()),
                            &HashSet::new(),
                            None,
                            &self.diagram_cache,
                            self.diagram_theme_id,
                            false,
                            preview_widget_generation,
                            recently_scrolled,
                        )
                        .map(|message| match message {
                            Message::TableScrolled => Message::TableScrolled,
                            _ => Message::Noop,
                        });
                        let mut preview = Column::new().push(rendered);
                        if *truncated {
                            preview = preview.push(
                                text("Preview truncated for performance")
                                    .size(12)
                                    .color(pal.muted),
                            );
                        }
                        preview.into()
                    }
                    FullMindmapPreview::Data {
                        path,
                        source,
                        truncated,
                    } if Some(path) == selected_path => {
                        let mut preview = Column::new().push(
                            crate::render::data_view_owned(source.clone(), &pal, &self.typography)
                                .map(|_| Message::Noop),
                        );
                        if *truncated {
                            preview = preview.push(
                                text("Preview truncated for performance")
                                    .size(12)
                                    .color(pal.muted),
                            );
                        }
                        preview.into()
                    }
                    FullMindmapPreview::Error { path, error } if Some(path) == selected_path => {
                        container(text(error.clone()).size(13).color(pal.accent))
                            .padding(24)
                            .into()
                    }
                    _ => container(text("Loading preview…").size(13).color(pal.muted))
                        .padding(24)
                        .into(),
                };
                // The label/path header and its fixed 20 px top breathing room
                // live outside the scrollable. The preview body therefore starts
                // at exact offset zero, regardless of label/path wrapping.
                let preview_header = container(
                    column![
                        text(label).size(14).color(pal.fg),
                        text(
                            selected_path
                                .map(|path| path.display().to_string())
                                .unwrap_or_default()
                        )
                        .size(12)
                        .color(pal.muted),
                    ]
                    .spacing(10),
                )
                .padding(Padding {
                    top: 20.0,
                    right: 18.0,
                    bottom: 10.0,
                    left: 18.0,
                })
                .width(Length::Fill);
                let scroll_tag = full_mindmap_preview_scroll_tag(full);
                let preview_scroll = scrollable(container(preview).padding(Padding {
                    top: 0.0,
                    right: 18.0,
                    bottom: 20.0,
                    left: 18.0,
                }))
                .id(Self::full_mindmap_preview_scroll_id())
                .on_scroll(move |viewport| Message::FullMindmapPreviewScrolled {
                    namespace: scroll_tag.0,
                    path: scroll_tag.1.clone(),
                    identity: scroll_tag.2,
                    viewport,
                })
                .height(Length::Fill)
                .direction(slim_scroll_direction())
                .style(move |_, status| sleek_scrollable_style(status, pal, recently_scrolled));
                column![preview_header, preview_scroll,].spacing(0).into()
            } else if let Some(load) = &full.pending_workspace_load {
                container(
                    column![
                    text("Indexing project…").size(14).color(pal.fg),
                    text(load.path.display().to_string()).size(12).color(pal.muted),
                    text("Large folders are indexed in the background with a fixed safety limit.")
                        .size(12)
                        .color(pal.muted),
                ]
                    .spacing(10),
                )
                .padding(24)
                .center_y(Length::Fill)
                .into()
            } else {
                let hint = match selected.map(|node| &node.kind) {
                    Some(WorkspaceNodeKind::Empty) => "No supported files are visible here.",
                    Some(WorkspaceNodeKind::Error) => "Choose another readable folder.",
                    Some(WorkspaceNodeKind::Truncated) => {
                        "Some files were omitted to keep navigation responsive."
                    }
                    _ => "Select a file to preview its content.",
                };
                container(text(hint).size(13).color(pal.muted))
                    .padding(24)
                    .center_y(Length::Fill)
                    .into()
            };
        let content: Element<'_, Message> = if let Some(error) = full.load_error.as_ref() {
            column![text(error.clone()).size(12).color(pal.accent), content,]
                .spacing(10)
                .into()
        } else {
            content
        };
        let body: Element<'_, Message> = if selected_is_file {
            content
        } else {
            let scroll_tag = full_mindmap_preview_scroll_tag(full);
            scrollable(container(content).padding(Padding::from([20, 18])))
                .id(Self::full_mindmap_preview_scroll_id())
                .on_scroll(move |viewport| Message::FullMindmapPreviewScrolled {
                    namespace: scroll_tag.0,
                    path: scroll_tag.1.clone(),
                    identity: scroll_tag.2,
                    viewport,
                })
                .height(Length::Fill)
                .direction(slim_scroll_direction())
                // Folder/status content keeps Iced's established default
                // scrollbar catalog; only the file preview below uses the
                // ordinary Markdown transparent-rail style.
                .style(scrollable::default)
                .into()
        };
        container(body)
            .width(Length::Fixed(panel_width))
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(pal.surface.into()),
                border: Border {
                    color: pal.rule,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .into()
    }
}
