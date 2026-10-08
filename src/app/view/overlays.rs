//! Modal overlays: folder picker, file finder, command palette, themes, and the shortcuts sheet.

use super::*;
// Explicit import: the `column!` macro is otherwise ambiguous between the glob
// import and iced's exported macro.
use iced::widget::column;

pub(in crate::app) fn folder_picker_overlay<'a>(
    pk: Option<&'a Picker>,
    selected: usize,
    pal: Palette,
) -> Element<'a, Message> {
    let panel: Element<'a, Message> = if let Some(pk) = pk {
        let crumbs = pk.breadcrumbs();
        let mut crumb_row = iced::widget::Row::new()
            .spacing(2)
            .align_y(iced::Alignment::Center);
        crumb_row = crumb_row.push(ghost_lu(ic::HOME, pal).on_press(Message::PickerHome));
        crumb_row = crumb_row.push(ghost_lu(ic::ARROW_UP, pal).on_press(Message::PickerParent));
        crumb_row = crumb_row.push(Space::new().width(8));
        for (label, path) in crumbs.iter() {
            crumb_row = crumb_row.push(text("/").color(pal.subtle).size(12));
            let label = label.clone();
            let path = path.clone();
            crumb_row = crumb_row.push(
                button(text(label).size(12).color(pal.fg))
                    .padding(Padding::from([3, 6]))
                    .style(move |_, status| button::Style {
                        background: match status {
                            button::Status::Hovered => Some(Background::Color(pal.hover())),
                            _ => None,
                        },
                        text_color: pal.fg,
                        border: Border {
                            radius: theme::radius::SM.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .on_press(Message::PickerNavigate(path)),
            );
        }
        let header = container(crumb_row)
            .padding(Padding::from([10, 14]))
            .width(Length::Fill);

        let mut list = Column::new().spacing(1).padding(Padding::from([6, 8]));
        if let Some(err) = &pk.error {
            list = list.push(text(err.clone()).color(pal.muted).size(13));
        } else if pk.entries.is_empty() {
            list =
                list.push(container(text("Empty folder").color(pal.subtle).size(13)).padding(14));
        } else {
            for (i, e) in pk.entries.iter().enumerate() {
                let is_sel = i == selected;
                let path_clone = e.path.clone();
                let name = e.name.clone();
                let glyph = if e.is_dir { ic::FOLDER } else { ic::FILE_TEXT };
                let on_press = if e.is_dir {
                    Message::PickerNavigate(path_clone)
                } else {
                    Message::PickerOpenFile(path_clone)
                };
                let row = button(
                    irow![
                        icon::glyph(glyph, 13.0, pal.subtle),
                        text(name).size(13).color(pal.fg),
                    ]
                    .spacing(10)
                    .align_y(iced::Alignment::Center),
                )
                .padding(Padding::from([7, 12]))
                .width(Length::Fill)
                .height(Length::Fixed(32.0))
                .style(move |_, status| button::Style {
                    background: match (is_sel, status) {
                        (true, _) => Some(Background::Color(pal.hover())),
                        (_, button::Status::Hovered) => Some(Background::Color(pal.hover())),
                        _ => None,
                    },
                    text_color: pal.fg,
                    border: Border {
                        radius: theme::radius::SM.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .on_press(on_press);
                list = list.push(row);
            }
        }
        let body = scrollable(list)
            .id(App::overlay_scroll_id())
            .height(Length::Fill)
            .on_scroll(Message::OverlayScrolled)
            .direction(slim_scroll_direction())
            .style(move |_, status| sleek_scrollable_style(status, pal, true));

        let footer = picker_hint_footer(pal);
        column![header, body, footer].into()
    } else {
        text("No picker").into()
    };

    overlay_frame(panel, pal, 640.0, 560.0)
}

pub(in crate::app) fn file_finder_overlay<'a>(
    query: &'a str,
    files: Vec<(PathBuf, String, i32)>,
    selected: usize,
    pal: Palette,
) -> Element<'a, Message> {
    let input = container(
        text_input("Find file… (fuzzy)", query)
            .id(App::overlay_input_id())
            .on_input(Message::OverlayQueryChanged)
            .on_submit(Message::OverlayConfirm)
            .padding(Padding::from([10, 14]))
            .size(14)
            .style(move |_, _| iced::widget::text_input::Style {
                background: Color::TRANSPARENT.into(),
                border: Border::default(),
                icon: pal.muted,
                placeholder: pal.subtle,
                value: pal.fg,
                selection: pal.selection,
            }),
    );

    let mut list = Column::new().spacing(0).padding(Padding::from([6, 8]));
    if files.is_empty() {
        list = list.push(container(text("No matches").color(pal.subtle).size(13)).padding(14));
    } else {
        for (i, (p, rel, _)) in files.into_iter().enumerate().take(80) {
            let is_sel = i == selected;
            let path_clone = p.clone();
            let parent = std::path::Path::new(&rel)
                .parent()
                .map(|x| x.to_string_lossy().into_owned())
                .unwrap_or_default();
            let name = std::path::Path::new(&rel)
                .file_name()
                .map(|x| x.to_string_lossy().into_owned())
                .unwrap_or_else(|| rel.clone());
            let inner = irow![
                text(name).size(13).color(pal.fg),
                Space::new().width(8),
                text(parent).size(12).color(pal.subtle),
            ]
            .align_y(iced::Alignment::Center);
            let row = button(inner)
                .padding(Padding::from([7, 12]))
                .width(Length::Fill)
                .height(Length::Fixed(32.0))
                .style(move |_, status| button::Style {
                    background: match (is_sel, status) {
                        (true, _) => Some(Background::Color(pal.hover())),
                        (_, button::Status::Hovered) => Some(Background::Color(pal.hover())),
                        _ => None,
                    },
                    text_color: pal.fg,
                    border: Border {
                        radius: theme::radius::SM.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .on_press(Message::OpenFileFinderPath(path_clone));
            list = list.push(row);
        }
    }
    let body = scrollable(list)
        .id(App::overlay_scroll_id())
        .on_scroll(Message::OverlayScrolled)
        .height(Length::Fill)
        .direction(slim_scroll_direction())
        .style(move |_, status| sleek_scrollable_style(status, pal, true));

    let divider = container(Space::new().height(1.0))
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(pal.rule.into()),
            ..Default::default()
        });

    overlay_frame(column![input, divider, body].into(), pal, 600.0, 460.0)
}

pub(in crate::app) const QUICK_SLOT_SHORTCUT_HINTS: &[(&str, &str)] = &[
    ("⌘1–9", "Activate slots 1 to 9"),
    ("⌘N", "Add current file to next empty slot"),
    ("⌘↑", "Previous slot (outside Zen)"),
    ("⌘↓", "Next slot (outside Zen)"),
    ("⌘W", "Close active slot"),
    ("⌘⇧W", "Close window"),
];

/// Static, read-only keyboard cheatsheet. Grouped by category, no search, no
/// cursor. Esc or backdrop click dismisses (handled by `overlay_frame`).
pub(in crate::app) fn shortcuts_overlay<'a>(pal: Palette) -> Element<'a, Message> {
    // (group title, [(keys, action)]). Hand-authored so we can group by category
    // and include non-command bindings (arrows, Space) the palette omits.
    let groups: [(&str, &[(&str, &str)]); 7] = [
        (
            "File",
            &[
                ("⌘O", "Open Folder"),
                ("⌘R", "Refresh File / Folder"),
                ("⌘⌥R", "Reveal File in Finder"),
                ("⌘⌥C", "Copy Focused File Path"),
                ("⌘P", "Find File in Workspace"),
                ("⌘S", "Save"),
            ],
        ),
        (
            "Navigation",
            &[
                ("⌘F", "Find in Document"),
                ("⌘⇧F", "Search All Files"),
                ("Home / g", "Reader top (outside Zen)"),
                ("End / G", "Reader bottom (outside Zen)"),
                ("↑ ↓", "Move outline / tree selection"),
                ("Enter", "Jump to selection"),
            ],
        ),
        (
            "View",
            &[
                ("⌘B", "Toggle Sidebar"),
                ("⌘E", "Toggle Zen Edit"),
                ("Esc", "Exit Zen Edit"),
                ("⌘T", "Cycle Theme"),
                ("⌘⇧.", "Toggle Hidden Files"),
                ("⌘+ ⌘-", "Reader Font Size Up / Down"),
                ("⌘0", "Reset Reader Font Size"),
                ("⌘⇧P", "Command Palette"),
            ],
        ),
        (
            "Edit",
            &[
                ("⌘← ⌘→", "Zen line start / end"),
                ("⌘↑ ⌘↓", "Zen document start / end"),
                ("⌘S", "Save"),
            ],
        ),
        (
            "Mindmap",
            &[
                ("⌘M", "Toggle Mindmap"),
                ("⌘K 0–6", "Show Through Node Level"),
                ("⌘⌥B", "Toggle Panel"),
                ("⌘⌥W", "Cycle Panel Width"),
                ("= −", "Zoom Graph In / Out"),
                ("0", "Reset Graph Zoom (100%)"),
                ("← ↑ → ↓", "Navigate nodes"),
                ("Space", "Fold / unfold node"),
            ],
        ),
        ("Quick Slots", QUICK_SLOT_SHORTCUT_HINTS),
        ("Help", &[("⌘/", "Show Shortcuts")]),
    ];

    // Three balanced columns so the sheet is compact and nothing clips:
    // File + Quick Slots | Navigation + Mindmap | View + Edit + Help.
    let columns = [
        vec![groups[0], groups[5]],
        vec![groups[1], groups[4]],
        vec![groups[2], groups[3], groups[6]],
    ];

    let mut cols = irow![].spacing(24);
    for col_groups in columns {
        let mut col = Column::new().spacing(2).width(Length::Fixed(300.0));
        for (gi, (title, rows)) in col_groups.iter().enumerate() {
            let top = if gi == 0 { 0.0 } else { 18.0 };
            let mut header_font = iced::Font::with_name("Inter");
            header_font.weight = iced::font::Weight::Semibold;
            col = col.push(
                container(text(*title).size(11).color(pal.muted).font(header_font)).padding(
                    Padding {
                        top,
                        bottom: 5.0,
                        left: 2.0,
                        right: 0.0,
                    },
                ),
            );
            for (keys, action) in rows.iter() {
                let row = irow![
                    container(key_caps(keys, pal)).width(Length::Fixed(118.0)),
                    text(*action).size(13).color(pal.fg),
                ]
                .spacing(12)
                .align_y(iced::Alignment::Center);
                col = col.push(container(row).padding(Padding::from([4, 2])));
            }
        }
        cols = cols.push(col);
    }

    let card = container(cols).padding(Padding::from([34, 40]));

    // Dedicated frame: vertically centered (equal top/bottom margin). Scrim
    // darkness matches the command palette (`overlay_frame`).
    let panel = container(card)
        .max_width(1060.0)
        .max_height(520.0)
        .style(move |_| container::Style {
            background: Some(pal.popover().into()),
            border: Border {
                color: pal.border(),
                width: 1.0,
                radius: theme::radius::LG.into(),
            },
            shadow: theme::shadow::SHEET,
            ..Default::default()
        });

    let scrim = mouse_area(
        container(Space::new().width(Length::Fill).height(Length::Fill))
            .style(|_| container::Style {
                background: Some(Background::Color(theme::shadow::SCRIM)),
                ..Default::default()
            })
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .on_press(Message::CloseOverlay);

    let centered = container(panel)
        .padding(Padding::from([60, 40]))
        .center_x(Length::Fill)
        .center_y(Length::Fill);

    stack![scrim, centered].into()
}

/// Render a shortcut string as a row of square key caps. Space-separated
/// combos; within a combo each character gets its own square box, except
/// multi-letter words (e.g. `Enter`, `Space`) which stay as one wider cap.
pub(in crate::app) fn key_caps<'a>(keys: &str, pal: Palette) -> Element<'a, Message> {
    let mut row = irow![].spacing(4).align_y(iced::Alignment::Center);
    for combo in keys.split(' ').filter(|s| !s.is_empty()) {
        let is_word = combo.chars().count() > 1 && combo.chars().all(|c| c.is_ascii_alphabetic());
        let caps: Vec<String> = if is_word {
            vec![combo.to_string()]
        } else {
            combo.chars().map(|c| c.to_string()).collect()
        };
        for cap in caps {
            let multi = cap.chars().count() > 1;
            let cap_text = text(cap).size(12).color(pal.fg).font(editor_font());
            // Square (24x24) for single glyphs; wider but same height for words.
            let w = if multi {
                Length::Shrink
            } else {
                Length::Fixed(24.0)
            };
            row = row.push(
                container(cap_text)
                    .width(w)
                    .height(Length::Fixed(24.0))
                    .padding(if multi {
                        Padding::from([0, 8])
                    } else {
                        Padding::ZERO
                    })
                    .align_x(iced::alignment::Horizontal::Center)
                    .align_y(iced::alignment::Vertical::Center)
                    .style(move |_| container::Style {
                        background: Some(pal.chip().into()),
                        border: Border {
                            color: pal.border(),
                            width: 1.0,
                            radius: theme::radius::SM.into(),
                        },
                        ..Default::default()
                    }),
            );
        }
    }
    row.into()
}

pub(in crate::app) fn command_overlay<'a>(
    query: &'a str,
    cmds: Vec<(&'static str, Message, i32)>,
    selected: usize,
    pal: Palette,
) -> Element<'a, Message> {
    let input = container(
        text_input("Run a command…", query)
            .id(App::overlay_input_id())
            .on_input(Message::OverlayQueryChanged)
            .on_submit(Message::OverlayConfirm)
            .padding(Padding::from([10, 14]))
            .size(14)
            .style(move |_, _| iced::widget::text_input::Style {
                background: Color::TRANSPARENT.into(),
                border: Border::default(),
                icon: pal.muted,
                placeholder: pal.subtle,
                value: pal.fg,
                selection: pal.selection,
            }),
    );

    let mut list = Column::new().spacing(0).padding(Padding::from([6, 8]));
    if cmds.is_empty() {
        list = list.push(container(text("No commands").color(pal.subtle).size(13)).padding(14));
    } else {
        for (i, (label, msg, _)) in cmds.into_iter().enumerate() {
            let is_sel = i == selected;
            let row = button(text(label).size(13).color(pal.fg))
                .padding(Padding::from([7, 12]))
                .width(Length::Fill)
                .height(Length::Fixed(32.0))
                .style(move |_, status| button::Style {
                    background: match (is_sel, status) {
                        (true, _) => Some(Background::Color(pal.hover())),
                        (_, button::Status::Hovered) => Some(Background::Color(pal.hover())),
                        _ => None,
                    },
                    text_color: pal.fg,
                    border: Border {
                        radius: theme::radius::SM.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .on_press(msg);
            list = list.push(row);
        }
    }

    let body = scrollable(list)
        .id(App::overlay_scroll_id())
        .on_scroll(Message::OverlayScrolled)
        .height(Length::Fill)
        .direction(slim_scroll_direction())
        .style(move |_, status| sleek_scrollable_style(status, pal, true));

    let divider = container(Space::new().height(1.0))
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(pal.rule.into()),
            ..Default::default()
        });

    overlay_frame(column![input, divider, body].into(), pal, 560.0, 420.0)
}

pub(in crate::app) fn theme_overlay<'a>(
    query: &'a str,
    themes: Vec<ThemeEntry>,
    selected: usize,
    current: theme::ThemeId,
    pal: Palette,
) -> Element<'a, Message> {
    let input = container(
        text_input("Pick theme…", query)
            .id(App::overlay_input_id())
            .on_input(Message::OverlayQueryChanged)
            .on_submit(Message::OverlayConfirm)
            .padding(Padding::from([10, 14]))
            .size(14)
            .style(move |_, _| iced::widget::text_input::Style {
                background: Color::TRANSPARENT.into(),
                border: Border::default(),
                icon: pal.muted,
                placeholder: pal.subtle,
                value: pal.fg,
                selection: pal.selection,
            }),
    );

    let mut list = Column::new().spacing(0).padding(Padding::from([6, 8]));
    for (i, t) in themes.into_iter().enumerate() {
        let is_sel = i == selected;
        let is_current = t.matches_current(&current);
        let swatch_pal = t.palette();
        let swatch = container(
            Space::new()
                .width(Length::Fixed(14.0))
                .height(Length::Fixed(14.0)),
        )
        .style(move |_| container::Style {
            background: Some(swatch_pal.accent.into()),
            border: Border {
                color: swatch_pal.border(),
                width: 1.0,
                radius: theme::radius::XS.into(),
            },
            ..Default::default()
        });
        let bg_swatch = container(
            Space::new()
                .width(Length::Fixed(14.0))
                .height(Length::Fixed(14.0)),
        )
        .style(move |_| container::Style {
            background: Some(swatch_pal.bg.into()),
            border: Border {
                color: swatch_pal.border(),
                width: 1.0,
                radius: theme::radius::XS.into(),
            },
            ..Default::default()
        });
        let label = t.label().to_string();
        let msg = t.message();
        let marker: Element<'a, Message> = if is_current {
            icon::glyph(ic::CHECK, 12.0, pal.accent).into()
        } else {
            Space::new().width(12.0).into()
        };
        let row = button(
            irow![
                marker,
                Space::new().width(4),
                bg_swatch,
                Space::new().width(2),
                swatch,
                Space::new().width(8),
                text(label).size(13).color(pal.fg),
            ]
            .align_y(iced::Alignment::Center),
        )
        .padding(Padding::from([7, 12]))
        .width(Length::Fill)
        .style(move |_, status| button::Style {
            background: match (is_sel, status) {
                (true, _) => Some(Background::Color(pal.hover())),
                (_, button::Status::Hovered) => Some(Background::Color(pal.hover())),
                _ => None,
            },
            text_color: pal.fg,
            border: Border {
                radius: theme::radius::SM.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .on_press(msg);
        list = list.push(row);
    }

    let body = scrollable(list)
        .id(App::overlay_scroll_id())
        .on_scroll(Message::OverlayScrolled)
        .height(Length::Fill)
        .direction(slim_scroll_direction())
        .style(move |_, status| sleek_scrollable_style(status, pal, true));

    let divider = container(Space::new().height(1.0))
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(pal.rule.into()),
            ..Default::default()
        });

    overlay_frame(column![input, divider, body].into(), pal, 480.0, 420.0)
}

pub(in crate::app) fn picker_hint_footer<'a>(pal: Palette) -> Element<'a, Message> {
    let hint = |k: &'static str, label: &'static str| -> Element<'a, Message> {
        irow![
            container(text(k).size(11).color(pal.fg))
                .padding(Padding::from([2, 6]))
                .style(move |_| container::Style {
                    background: Some(pal.chip().into()),
                    border: Border {
                        color: pal.border(),
                        width: 1.0,
                        radius: theme::radius::XS.into(),
                    },
                    ..Default::default()
                }),
            Space::new().width(6),
            text(label).size(11).color(pal.subtle),
        ]
        .align_y(iced::Alignment::Center)
        .into()
    };
    let divider = container(Space::new().height(1.0))
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(pal.rule.into()),
            ..Default::default()
        });
    let row = irow![
        hint("↑↓", "navigate"),
        Space::new().width(14),
        hint("←", "up"),
        Space::new().width(14),
        hint("→", "descend"),
        Space::new().width(14),
        hint("␣", "descend / open"),
        Space::new().width(14),
        hint("↵", "open"),
        Space::new().width(Length::Fill),
        hint("⎋", "close"),
    ]
    .align_y(iced::Alignment::Center);
    column![
        divider,
        container(row)
            .padding(Padding::from([8, 14]))
            .width(Length::Fill),
    ]
    .into()
}

/// A compact inline row of `key — label` hint pills, matching the picker footer
/// style (chip cap, hairline border, subtle label). Reused for floating mind
/// map hints and the sidebar tab-row hint.
pub(in crate::app) fn hint_pills<'a>(
    items: &[(&'a str, &'a str)],
    pal: Palette,
) -> Element<'a, Message> {
    let mut row = irow![].align_y(iced::Alignment::Center);
    for (i, (k, label)) in items.iter().enumerate() {
        if i > 0 {
            row = row.push(Space::new().width(12));
        }
        let pill = irow![
            container(text(k.to_string()).size(11).color(pal.fg))
                .padding(Padding::from([2, 6]))
                .style(move |_| container::Style {
                    background: Some(pal.chip().into()),
                    border: Border {
                        color: pal.border(),
                        width: 1.0,
                        radius: theme::radius::XS.into(),
                    },
                    ..Default::default()
                }),
            Space::new().width(6),
            text(label.to_string()).size(11).color(pal.subtle),
        ]
        .align_y(iced::Alignment::Center);
        row = row.push(pill);
    }
    row.into()
}

pub(in crate::app) fn overlay_frame<'a>(
    content: Element<'a, Message>,
    pal: Palette,
    max_w: f32,
    max_h: f32,
) -> Element<'a, Message> {
    let panel = container(content)
        .max_width(max_w)
        .max_height(max_h)
        .width(Length::Fill)
        .height(Length::Fill)
        .clip(true)
        .style(move |_| container::Style {
            background: Some(pal.popover().into()),
            border: Border {
                color: pal.border(),
                width: 1.0,
                radius: theme::radius::LG.into(),
            },
            shadow: theme::shadow::DIALOG,
            ..Default::default()
        });

    let scrim = mouse_area(
        container(Space::new().width(Length::Fill).height(Length::Fill))
            .style(|_| container::Style {
                background: Some(Background::Color(theme::shadow::SCRIM)),
                ..Default::default()
            })
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .on_press(Message::CloseOverlay);

    // The panel is `Fill` tall up to `max_h`, so its box has a fixed height
    // and a fixed top while the result list filters. Centering that box (not
    // the content) keeps the input row still.
    let centered = container(panel)
        .padding(Padding::from([40, 40]))
        .center_x(Length::Fill)
        .center_y(Length::Fill);

    iced::widget::stack![scrim, centered].into()
}
