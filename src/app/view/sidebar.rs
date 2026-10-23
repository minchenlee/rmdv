//! Sidebar: file tree, outline, tab strip, and the drag handles for the sidebar and Mindmap panels.

use super::*;
// Explicit import: the `column!` macro is otherwise ambiguous between the glob
// import and iced's exported macro.
use iced::widget::column;
use iced::widget::tooltip::Position;

pub(in crate::app) fn sidebar_view<'a>(app: &'a App, pal: Palette) -> Element<'a, Message> {
    let recently_scrolled = app
        .last_scroll_at
        .is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(SCROLLER_FADE_MS));
    let ws = app.workspace.as_ref().unwrap();
    let ws_name = ws
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("workspace");
    // Title row: workspace name on the left; the Files/Outline tabs, command-palette
    // button, and sidebar-collapse button pinned to the right beside each other.
    let switch_tip = |inner: Element<'a, Message>| {
        action_tip(inner, "Switch Tab", Some("← →"), Position::Bottom, pal)
    };
    // Only the (variable-length) title may be clipped on a narrow sidebar — the
    // control cluster stays unclipped so the collapse button is never cut off.
    let title_label = container(
        text(ws_name.to_string().to_uppercase())
            .size(11)
            .color(pal.muted)
            // No word-wrap: a long workspace name must truncate (clip below),
            // not wrap onto a second line and grow the header row.
            .wrapping(iced::widget::text::Wrapping::None),
    )
    .width(Length::Fill)
    .clip(true);
    let (hide_glyph, hide_action, hide_id) = if app.sidebar_peek && !app.sidebar_open {
        (
            ic::PANEL_LEFT_OPEN,
            "Keep Sidebar Open",
            iced::widget::Id::new("sidebar-keep-open"),
        )
    } else {
        (
            ic::PANEL_LEFT_CLOSE,
            "Hide Sidebar",
            iced::widget::Id::new("sidebar-hide"),
        )
    };
    let controls = irow![
        switch_tip(sidebar_tab_button(
            "Files",
            app.sidebar_tab == SidebarTab::Files,
            SidebarTab::Files,
            pal
        )),
        switch_tip(sidebar_tab_button(
            "Outline",
            app.sidebar_tab == SidebarTab::Outline,
            SidebarTab::Outline,
            pal
        )),
        action_tip(
            ghost_lu(ic::COMMAND, pal).on_press(Message::OpenCommandPalette),
            "Command Palette",
            Some("⌘⇧P"),
            Position::Bottom,
            pal,
        ),
        action_tip(
            ghost_lu(ic::SETTINGS, pal).on_press(Message::ToggleSettings),
            "Settings",
            Some("⌘,"),
            Position::Bottom,
            pal,
        ),
        // While peeking, the same place pins the sidebar open, as ⌘B does.
        action_tip(
            container(ghost_lu(hide_glyph, pal).on_press(Message::ToggleSidebar)).id(hide_id),
            hide_action,
            Some("⌘B"),
            Position::Bottom,
            pal,
        ),
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center);
    let title_row = irow![title_label, controls]
        .spacing(6)
        .align_y(iced::Alignment::Center);

    // The header sits where a titlebar would: its blank space moves the
    // window.
    let header = container(column![
        Space::new().height(Length::Fixed(sidebar_titlebar_reserve_for_fullscreen(
            app.window_fullscreen,
        ))),
        container(title_row)
            .padding(Padding {
                top: 0.0,
                right: 14.0,
                bottom: 8.0,
                left: 14.0,
            })
            .width(Length::Fill),
    ])
    .width(Length::Fill);
    let header = window_drag_area(header);

    let body: Element<'a, Message> = match app.sidebar_tab {
        SidebarTab::Files => sidebar_files_body(app, pal, recently_scrolled),
        SidebarTab::Outline => sidebar_outline_body(app, pal, recently_scrolled),
    };

    container(column![header, body])
        .width(Length::Fixed(app.sidebar_width))
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(pal.sidebar.into()),
            ..Default::default()
        })
        .into()
}

/// The hidden sidebar shown over the content (the peek): the same sidebar,
/// with a shadow and no divider to drag. It reports the pointer entering
/// and leaving it, and takes every press and hover over it, so the content
/// under it gets none.
pub(in crate::app) fn sidebar_peek_view<'a>(
    app: &'a App,
    mut pal: Palette,
) -> Element<'a, Message> {
    // Over the content, a translucent sidebar would show the text through.
    pal.sidebar.a = 1.0;
    let panel = container(sidebar_view(app, pal))
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(pal.sidebar.into()),
            shadow: theme::shadow::PEEK,
            ..Default::default()
        });
    let panel = iced::widget::opaque(
        mouse_area(panel)
            .on_enter(Message::SidebarPeek(true))
            .on_exit(Message::SidebarPeek(false)),
    );
    container(panel)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Left)
        .into()
}

pub(in crate::app) fn sidebar_tab_button<'a>(
    label: &'a str,
    active: bool,
    tab: SidebarTab,
    pal: Palette,
) -> Element<'a, Message> {
    button(
        text(label)
            .size(11)
            .color(if active { pal.fg } else { pal.muted }),
    )
    .padding(Padding::from([3, 9]))
    .style(move |_, status| {
        let bg = if active {
            Some(Background::Color(pal.hover()))
        } else {
            match status {
                button::Status::Hovered => Some(Background::Color(pal.hover())),
                _ => None,
            }
        };
        button::Style {
            background: bg,
            text_color: pal.fg,
            border: Border {
                color: if active {
                    pal.border()
                } else {
                    Color::TRANSPARENT
                },
                width: 1.0,
                radius: theme::radius::SM.into(),
            },
            ..Default::default()
        }
    })
    .on_press(Message::SetSidebarTab(tab))
    .into()
}

pub(in crate::app) fn sidebar_files_body<'a>(
    app: &'a App,
    pal: Palette,
    recently_scrolled: bool,
) -> Element<'a, Message> {
    // Measure longest row so we can pin the Column to a Fixed width. With
    // `Direction::Both`, an unsized Column collapses to its widest *Shrink*
    // child — which would shrink the selection ring to text width. Setting an
    // explicit width lets each row's `Length::Fill` stretch to it, giving a
    // full-width focus ring AND horizontal scroll when names overflow.
    // Approach mirrors Zed's project panel.
    let mut list = Column::new().spacing(0).padding(Padding::from([4, 4]));
    let mut content_w = app.sidebar_width - 12.0; // minus scrollbar gutter
    if let Some(tree_root) = &app.workspace_tree {
        let rows = tree::flatten_with_files(tree_root, &app.workspace_sidebar_files, &app.expanded);
        let current = app.file.as_ref();
        // The cursor row is painted only while the arrow keys drive it.
        let cursor = app.tree_cursor_shown.then_some(app.tree_cursor);
        for r in rows.iter() {
            let w = tree_row_width(r.node, r.depth);
            if w > content_w {
                content_w = w;
            }
        }
        // Build only the rows near the viewport; spacers keep the list's
        // full height so scroll offsets and keyboard edge-scroll still match.
        list = push_sidebar_rows(list, app.tree_viewport.as_ref(), rows.len(), |i| {
            let r = &rows[i];
            tree_row(
                r.node,
                r.depth,
                &app.expanded,
                current,
                cursor == Some(i),
                pal,
            )
        });
    }
    let list = list.width(Length::Fixed(content_w));
    // Nested single-axis scrollables: inner handles vertical, outer handles
    // horizontal. Iced 0.14's `Direction::Both` allows diagonal scrolling,
    // which feels wrong for file trees — Zed and VS Code lock to one axis at
    // a time. Splitting them lets macOS trackpad gestures route naturally:
    // dominant-Y events hit the inner, dominant-X events bubble to the outer.
    let inner = scrollable(list)
        .id(App::tree_scroll_id())
        .width(Length::Fixed(content_w))
        .height(Length::Fill)
        .on_scroll(Message::TreeScrolled)
        .direction(slim_scroll_direction())
        .style(move |_, status| sleek_scrollable_style(status, pal, recently_scrolled));
    scrollable(inner)
        .height(Length::Fill)
        .direction(slim_scroll_direction_horizontal())
        .style(move |_, status| sleek_scrollable_style(status, pal, recently_scrolled))
        .into()
}

pub(in crate::app) fn sidebar_outline_body<'a>(
    app: &'a App,
    pal: Palette,
    recently_scrolled: bool,
) -> Element<'a, Message> {
    let sections = &app.outline_sections;
    let mut list = Column::new().spacing(0).padding(Padding::from([4, 4]));
    if sections.is_empty() {
        list = list.push(
            container(text("No headings").size(12).color(pal.muted))
                .padding(Padding::from([8, 10])),
        );
    } else {
        list = push_sidebar_rows(list, app.outline_viewport.as_ref(), sections.len(), |i| {
            outline_row(&sections[i], i, i == app.outline_cursor, pal)
        });
    }
    scrollable(list.width(Length::Fill))
        .id(App::outline_scroll_id())
        .height(Length::Fill)
        .on_scroll(Message::OutlineScrolled)
        .direction(slim_scroll_direction())
        .style(move |_, status| sleek_scrollable_style(status, pal, recently_scrolled))
        .into()
}

/// Fixed height of every file-tree and outline row.
pub(in crate::app) const SIDEBAR_ROW_H: f32 = 26.0;
/// Top padding of the sidebar lists, before their first row.
const SIDEBAR_LIST_TOP_PAD: f32 = 4.0;
/// Rows built beyond each edge of the viewport, so a scroll that lands before
/// the next `view()` does not reveal blank space.
const SIDEBAR_OVERSCAN: f32 = 600.0;

/// Rows of a sidebar list worth building for `viewport`: the band around the
/// viewport, plus the first screenful. A list re-created after the sidebar was
/// hidden starts at the top while `viewport` still holds the old offset, so the
/// top rows must exist until the new scrollable reports its viewport. Until
/// the first viewport arrives every row is built.
pub(in crate::app) fn sidebar_row_window(
    viewport: Option<&iced::widget::scrollable::Viewport>,
    total: usize,
) -> [std::ops::Range<usize>; 2] {
    match viewport {
        Some(viewport) => sidebar_row_bands(
            viewport.absolute_offset().y,
            viewport.bounds().height,
            total,
        ),
        None => [0..total, total..total],
    }
}

/// The first screenful and the band near `offset_y`, merged when they touch.
pub(in crate::app) fn sidebar_row_bands(
    offset_y: f32,
    height: f32,
    total: usize,
) -> [std::ops::Range<usize>; 2] {
    let head = sidebar_rows_near(0.0, height, total);
    let near = sidebar_rows_near(offset_y, height, total);
    if near.start <= head.end {
        [0..near.end.max(head.end), total..total]
    } else {
        [head, near]
    }
}

/// Push the rows `sidebar_row_window` selects, with spacers standing in for
/// the rest so the column keeps its full height.
fn push_sidebar_rows<'a>(
    mut list: Column<'a, Message>,
    viewport: Option<&iced::widget::scrollable::Viewport>,
    total: usize,
    row: impl Fn(usize) -> Element<'a, Message>,
) -> Column<'a, Message> {
    let mut next = 0;
    for range in sidebar_row_window(viewport, total) {
        if range.is_empty() {
            continue;
        }
        if range.start > next {
            list = list.push(Space::new().height((range.start - next) as f32 * SIDEBAR_ROW_H));
        }
        for i in range.clone() {
            list = list.push(row(i));
        }
        next = range.end;
    }
    if total > next {
        list = list.push(Space::new().height((total - next) as f32 * SIDEBAR_ROW_H));
    }
    list
}

/// Rows within the overscan band around a viewport scrolled to `offset_y` px
/// with `height` px visible. A stale offset past the end (the list shrank)
/// still yields the list's last screenful.
pub(in crate::app) fn sidebar_rows_near(
    offset_y: f32,
    height: f32,
    total: usize,
) -> std::ops::Range<usize> {
    let offset = offset_y - SIDEBAR_LIST_TOP_PAD;
    let top = offset - SIDEBAR_OVERSCAN;
    let bottom = offset + height + SIDEBAR_OVERSCAN;
    let span = ((bottom - top) / SIDEBAR_ROW_H).ceil() as usize;
    let first = ((top / SIDEBAR_ROW_H).floor().max(0.0) as usize).min(total);
    let last = ((bottom / SIDEBAR_ROW_H).ceil().max(0.0) as usize).min(total);
    if first >= last {
        return total.saturating_sub(span)..total;
    }
    first..last
}

pub(in crate::app) fn outline_row<'a>(
    s: &crate::ipc::sections::Section,
    index: usize,
    is_cursor: bool,
    pal: Palette,
) -> Element<'a, Message> {
    let indent = TREE_INDENT * (s.level.saturating_sub(1)) as f32;
    let weight = if s.level <= 1 {
        iced::font::Weight::Medium
    } else {
        iced::font::Weight::Normal
    };
    let mut font = iced::Font::with_name("Inter");
    font.weight = weight;
    let label = text(s.title.clone())
        .size(13)
        .color(if s.level <= 1 { pal.fg } else { pal.muted })
        .font(font)
        .wrapping(text::Wrapping::None);
    let content =
        irow![Space::new().width(Length::Fixed(indent)), label].align_y(iced::Alignment::Center);
    button(content)
        .padding(Padding::from([4, 8]))
        .width(Length::Fill)
        .height(Length::Fixed(SIDEBAR_ROW_H))
        .style(move |_, status| button::Style {
            background: if is_cursor {
                Some(Background::Color(pal.tree_selected_bg))
            } else {
                match status {
                    button::Status::Hovered => Some(Background::Color(pal.hover())),
                    _ => None,
                }
            },
            text_color: pal.fg,
            border: Border {
                radius: theme::radius::SM.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .on_press(Message::OutlineRowClicked {
            index,
            line: s.line,
        })
        .into()
}

/// A mind map side panel with its close (X) button in the top-right
/// corner. The button hides the panel as ⌘⌥B does; the selection stays.
pub(in crate::app) fn with_panel_close<'a>(
    panel: Element<'a, Message>,
    on_close: Message,
    pal: Palette,
) -> Element<'a, Message> {
    let close = action_tip(
        ghost_lu(ic::X, pal).on_press(on_close),
        "Hide Panel",
        Some("⌘⌥B"),
        Position::Left,
        pal,
    );
    stack![
        panel,
        container(close)
            .width(Length::Fill)
            .padding(6)
            .align_x(iced::alignment::Horizontal::Right),
    ]
    .into()
}

/// The divider between a mind map canvas and its panel: a
/// [`crate::mindmap::PANEL_HANDLE_W`]-wide grab area with the resize cursor
/// that shows a hairline while the pointer is over it (Iced's `hover`
/// redraws only when the pointer enters or leaves). A double click resets
/// the panel to its default width.
fn panel_divider<'a>(
    id: &'static str,
    fill: Color,
    pal: Palette,
    press: Message,
    release: Message,
    reset: Message,
) -> Element<'a, Message> {
    let area = mouse_area(
        container(
            Space::new()
                .width(Length::Fixed(crate::mindmap::PANEL_HANDLE_W))
                .height(Length::Fill),
        )
        .id(iced::widget::Id::new(id))
        .style(move |_| container::Style {
            background: Some(fill.into()),
            ..Default::default()
        })
        .height(Length::Fill),
    )
    .interaction(iced::mouse::Interaction::ResizingHorizontally)
    .on_press(press)
    .on_release(release)
    .on_double_click(reset);
    let hairline = container(
        container(Space::new().width(Length::Fixed(1.0)).height(Length::Fill)).style(move |_| {
            container::Style {
                background: Some(pal.subtle.into()),
                ..Default::default()
            }
        }),
    )
    .width(Length::Fixed(crate::mindmap::PANEL_HANDLE_W))
    .height(Length::Fill)
    .center_x(Length::Fixed(crate::mindmap::PANEL_HANDLE_W));
    iced::widget::hover(area, hairline)
}

pub(in crate::app) fn mindmap_panel_resize_handle<'a>(
    fill: Color,
    pal: Palette,
) -> Element<'a, Message> {
    panel_divider(
        "mindmap-panel-divider",
        fill,
        pal,
        Message::MindmapPanelDragStart(0.0),
        Message::MindmapPanelDragEnd,
        Message::MindmapPanelResetWidth,
    )
}

pub(in crate::app) fn full_mindmap_panel_resize_handle<'a>(
    fill: Color,
    pal: Palette,
) -> Element<'a, Message> {
    panel_divider(
        "full-mindmap-panel-divider",
        fill,
        pal,
        Message::FullMindmapPanelDragStart(0.0),
        Message::FullMindmapPanelDragEnd,
        Message::FullMindmapPanelResetWidth,
    )
}

pub(in crate::app) fn sidebar_resize_handle<'a>(pal: Palette) -> Element<'a, Message> {
    mouse_area(
        container(Space::new().width(Length::Fixed(5.0)).height(Length::Fill))
            .style(move |_| container::Style {
                background: Some(pal.sidebar.into()),
                ..Default::default()
            })
            .height(Length::Fill),
    )
    .interaction(iced::mouse::Interaction::ResizingHorizontally)
    .on_press(Message::SidebarDragStart)
    .on_release(Message::SidebarDragEnd)
    .into()
}

/// Estimate the pixel width a [`tree_row`] needs at the given depth. Used to
/// size the surrounding Column so the focus ring fills the sidebar width AND
/// horizontal scroll kicks in when names overflow. Approximation uses an
/// average advance for Inter @ 13px; exact metrics aren't worth the cost of a
/// glyph-shaping pass on every render.
pub(in crate::app) fn tree_row_width(node: tree::RowNode<'_>, depth: usize) -> f32 {
    const CHAR_ADVANCE: f32 = 7.0;
    let indent = TREE_INDENT * depth as f32;
    let chevron = 14.0;
    let leaf = 13.0 + 4.0 + 7.0; // icon + gap before + gap after
    let label = node.name().chars().count() as f32 * CHAR_ADVANCE;
    let padding_h = 16.0; // button padding 8 each side
    indent + chevron + leaf + label + padding_h
}

pub(in crate::app) fn tree_row<'a>(
    node: tree::RowNode<'a>,
    depth: usize,
    expanded: &HashSet<PathBuf>,
    current: Option<&'a PathBuf>,
    is_cursor: bool,
    pal: Palette,
) -> Element<'a, Message> {
    let is_dir = node.is_dir();
    let node_path = node.path();
    let is_current = !is_dir && current.map(|c| c.as_path() == node_path).unwrap_or(false);
    let path = node_path.to_path_buf();

    // Indent area with vertical guides per ancestor level.
    let mut indent = iced::widget::Row::new();
    for _ in 0..depth {
        indent = indent.push(indent_guide(pal));
    }

    let chevron: Element<'a, Message> = if is_dir {
        let open = expanded.contains(node_path);
        let g = if open {
            ic::CHEVRON_DOWN
        } else {
            ic::CHEVRON_RIGHT
        };
        icon::glyph(g, 12.0, pal.subtle).into()
    } else {
        Space::new().width(12.0).into()
    };

    let label_color = if is_current {
        pal.fg
    } else if is_dir {
        pal.fg
    } else {
        pal.muted
    };
    let label_weight = if is_dir {
        iced::font::Weight::Medium
    } else {
        iced::font::Weight::Normal
    };
    let mut label_font = iced::Font::with_name("Inter");
    label_font.weight = label_weight;
    let label = text(node.name().into_owned())
        .size(13)
        .color(label_color)
        .font(label_font)
        .wrapping(text::Wrapping::None);

    let leaf_icon: Element<'a, Message> = if is_dir {
        let open = expanded.contains(node_path);
        let g = if open { ic::FOLDER_OPEN } else { ic::FOLDER };
        icon::glyph(g, 13.0, pal.subtle).into()
    } else {
        icon::glyph(ic::FILE_TEXT, 13.0, pal.subtle).into()
    };
    let content = irow![
        indent,
        container(chevron).width(Length::Fixed(14.0)),
        Space::new().width(4.0),
        leaf_icon,
        Space::new().width(7.0),
        label,
    ]
    .align_y(iced::Alignment::Center)
    .spacing(0);

    let on_press = Message::TreeRowClicked { path, is_dir };

    button(content)
        .padding(Padding::from([4, 8]))
        .width(Length::Fill)
        .height(Length::Fixed(SIDEBAR_ROW_H))
        .style(move |_, status| {
            let bg = if is_current {
                Some(Background::Color(pal.tree_selected_bg))
            } else if is_cursor {
                Some(Background::Color(pal.hover()))
            } else {
                match status {
                    button::Status::Hovered => Some(Background::Color(pal.hover())),
                    _ => None,
                }
            };
            // Selection and keyboard cursor: background fill only, no ring.
            button::Style {
                background: bg,
                text_color: pal.fg,
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: theme::radius::SM.into(),
                },
                ..Default::default()
            }
        })
        .on_press(on_press)
        .into()
}

pub(in crate::app) fn indent_guide<'a>(pal: Palette) -> Element<'a, Message> {
    container(
        container(Space::new().height(Length::Fill))
            .width(Length::Fixed(1.0))
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(pal.indent_guide.into()),
                ..Default::default()
            }),
    )
    .width(Length::Fixed(TREE_INDENT))
    .height(Length::Fixed(SIDEBAR_ROW_H))
    .center_x(Length::Fixed(TREE_INDENT))
    .into()
}

/// Sidebar header padding. On macOS we use `fullsize_content_view`, so the
/// traffic-light buttons overlay the top-left of the client area whenever the
/// window is not fullscreen. In fullscreen the buttons are hidden, so the large
/// reserve collapses to a small top margin for breathing room.
pub(in crate::app) fn sidebar_titlebar_reserve_for_fullscreen(fullscreen: bool) -> f32 {
    #[cfg(target_os = "macos")]
    {
        if fullscreen {
            10.0
        } else {
            22.0
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = fullscreen;
        0.0
    }
}
