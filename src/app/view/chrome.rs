//! Floating chrome: update banner, footer, toasts, progress, image zoom, welcome screen, search bar, and the Quick Slot rail.

use super::*;
// Explicit import: the `column!` macro is otherwise ambiguous between the glob
// import and iced's exported macro.
use iced::widget::column;

/// Bottom-center banner inviting the user to install a downloaded update.
pub(in crate::app) fn update_banner<'a>(version: &str, pal: Palette) -> Element<'a, Message> {
    use iced::widget::{button, container, row, text as text_w};
    // A small accent dot signals "something new", matching the warm accent the
    // rest of the UI uses for its single highlight color.
    let dot = container(Space::new().width(7).height(7)).style(move |_| container::Style {
        background: Some(pal.accent.into()),
        border: Border {
            radius: theme::radius::PILL.into(),
            ..Default::default()
        },
        ..Default::default()
    });
    let label = text_w(format!("rmdv {version} ready to install"))
        .size(13.0)
        .color(pal.fg);
    // Primary action uses the shared accent-pill button; "Later" is a quiet
    // ghost so the two read as primary/secondary, not two competing buttons.
    let install = primary_button("Install & Restart", pal).on_press(Message::InstallUpdate);
    let later = button(text_w("Later").size(13.0).color(pal.muted))
        .padding(Padding::from([8, 14]))
        .style(move |_, status| button::Style {
            background: match status {
                button::Status::Hovered | button::Status::Pressed => {
                    Some(Background::Color(pal.surface_alt))
                }
                _ => None,
            },
            text_color: pal.muted,
            border: Border {
                color: pal.rule,
                width: 1.0,
                radius: theme::radius::PILL.into(),
            },
            ..Default::default()
        })
        .on_press(Message::DismissUpdate);
    let bar = container(
        row![dot, label, Space::new().width(8), install, later]
            .spacing(10)
            .align_y(iced::alignment::Vertical::Center),
    )
    .padding(Padding::from([8, 10]))
    .style(move |_| container::Style {
        background: Some(pal.surface.into()),
        border: iced::Border {
            color: pal.rule,
            width: 1.0,
            radius: theme::radius::XXL.into(),
        },
        text_color: Some(pal.fg),
        ..Default::default()
    });
    container(bar)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(iced::Padding {
            bottom: 24.0,
            ..iced::Padding::ZERO
        })
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Bottom)
        .into()
}

/// Bottom status bar: word count + estimated reading time (~200 wpm).
pub(in crate::app) fn status_footer<'a>(words: usize, pal: Palette) -> Element<'a, Message> {
    use iced::widget::{container, text as text_w};
    let minutes = ((words as f32) / 200.0).ceil().max(1.0) as usize;
    let label = format!(
        "{} word{} · {} min read",
        words,
        if words == 1 { "" } else { "s" },
        minutes
    );
    // Translucent pill so document content remains visible scrolling behind it.
    let mut pill_bg = pal.bg;
    pill_bg.a = 0.82;
    let pill = container(text_w(label).size(12.0).color(pal.muted))
        .padding([4, 12])
        .style(move |_| container::Style {
            background: Some(pill_bg.into()),
            border: iced::Border {
                color: pal.rule,
                width: 1.0,
                radius: theme::radius::LG.into(),
            },
            ..Default::default()
        });
    // Float bottom-right over the reader; content scrolls underneath.
    container(pill)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([10, 14])
        .align_x(iced::alignment::Horizontal::Right)
        .align_y(iced::alignment::Vertical::Bottom)
        .into()
}

pub(in crate::app) fn toast_overlay<'a>(toast: &Toast, pal: Palette) -> Element<'a, Message> {
    use iced::widget::{button, container, text as text_w};
    let mut content = irow![text_w(toast.text.clone()).size(13.5).color(pal.fg)]
        .spacing(10)
        .align_y(iced::alignment::Vertical::Center);
    if let Some(action) = &toast.action {
        let action_button = button(text_w(action.label.clone()).size(12.5).color(pal.accent_fg))
            .padding(Padding::from([5, 10]))
            .style(move |_, _| button::Style {
                background: Some(Background::Color(pal.accent)),
                text_color: pal.accent_fg,
                border: Border {
                    radius: theme::radius::PILL.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .on_press(action.message.clone());
        content = content.push(action_button);
    }
    let bubble = container(content)
        .padding([8, 14])
        .style(move |_| container::Style {
            background: Some(pal.surface.into()),
            border: iced::Border {
                color: pal.rule,
                width: 1.0,
                radius: theme::radius::LG.into(),
            },
            text_color: Some(pal.fg),
            ..Default::default()
        });
    container(bubble)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([18, 0])
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Top)
        .into()
}

/// Persistent, neutral Full Mindmap verification feedback. It intentionally
/// shares the toast position but sits beneath the ordinary toast layer, so a
/// blocked/error toast remains readable and keeps its own expiry deadline.
pub(in crate::app) fn full_mindmap_progress_overlay<'a>(
    progress: &FullMindmapProgress,
    pal: Palette,
) -> Element<'a, Message> {
    use iced::widget::{container, text as text_w};
    let total = progress.total.max(1);
    let checked = progress.checked.min(progress.total);
    let remaining = progress.total.saturating_sub(checked);
    let ratio = (checked as f32 / total as f32).clamp(0.0, 1.0);
    let filled = ((ratio * 1000.0).round() as u16).max(if ratio > 0.0 { 1 } else { 0 });
    let unfilled = 1000u16.saturating_sub(filled);
    let bar = irow![
        container(Space::new())
            .width(Length::FillPortion(filled))
            .height(Length::Fixed(4.0))
            .style(move |_| container::Style {
                background: Some(pal.muted.into()),
                ..Default::default()
            }),
        container(Space::new())
            .width(Length::FillPortion(unfilled.max(1)))
            .height(Length::Fixed(4.0))
            .style(move |_| container::Style {
                background: Some(pal.rule.into()),
                ..Default::default()
            }),
    ]
    .spacing(0);
    let bubble = container(
        column![
            text_w(format!(
                "Verifying folders · {checked}/{} checked · {remaining} remaining",
                progress.total
            ))
            .size(13.0)
            .color(pal.fg),
            container(bar)
                .width(Length::Fill)
                .clip(true)
                .style(move |_| container::Style {
                    background: Some(pal.rule.into()),
                    border: Border {
                        radius: theme::radius::PILL.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
        ]
        .spacing(6),
    )
    .width(Length::Fixed(320.0))
    .padding([8, 14])
    .style(move |_| container::Style {
        background: Some(pal.surface.into()),
        border: Border {
            color: pal.rule,
            width: 1.0,
            radius: theme::radius::LG.into(),
        },
        text_color: Some(pal.fg),
        ..Default::default()
    });
    container(bubble)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([18, 0])
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Top)
        .into()
}

pub(in crate::app) fn image_zoom_overlay<'a>(
    url: Option<&'a str>,
    diagram: Option<&iced::widget::image::Handle>,
    cache: &ImageCache,
    pal: Palette,
) -> Element<'a, Message> {
    use iced::widget::image::viewer;
    let mk_viewer = |h: iced::widget::image::Handle| -> Element<'a, Message> {
        viewer(h)
            .min_scale(0.25)
            .max_scale(10.0)
            .scale_step(0.18)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    };
    // Diagram overrides image source when set — DiagramZoom clears zoom_url.
    // Reuses image::viewer for scroll-zoom + drag-pan + escape-close parity
    // with normal images.
    let inner: Element<'a, Message> = if let Some(handle) = diagram {
        mk_viewer(handle.clone())
    } else {
        match url {
            Some(u) => match cache.get(u) {
                Some(ImageState::Loaded(h)) => mk_viewer(h.clone()),
                Some(ImageState::LoadedSvg {
                    raster: Some(h), ..
                }) => mk_viewer(h.clone()),
                Some(ImageState::LoadedSvg { raster: None, .. }) | Some(ImageState::Loading) => {
                    text("rendering…").color(pal.muted).into()
                }
                Some(ImageState::Failed) => text("image unavailable").color(pal.muted).into(),
                None => {
                    // Local raster path (cache only stores svg/remote). Use direct viewer.
                    let p = std::path::PathBuf::from(u);
                    mk_viewer(iced::widget::image::Handle::from_path(p))
                }
            },
            None => text("").into(),
        }
    };
    let scrim = container(
        container(inner)
            .padding(8)
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_| container::Style {
        background: Some(Color { a: 0.85, ..pal.bg }.into()),
        ..Default::default()
    });
    // Click background scrim → close. Pointer cursor would mislead since
    // most of the surface is the viewer (which handles its own drags).
    let scrim_click = mouse_area(scrim).on_press(Message::CloseOverlay);
    // Top-right close button. Sits on its own mouse_area so a click on the
    // X always fires CloseOverlay (independent of the scrim mouse_area
    // beneath it in the stack).
    let close_btn_inner = container(crate::icon::glyph(crate::icon::ic::X, 16.0, pal.fg))
        .padding(Padding::from([6, 8]))
        .style(move |_| container::Style {
            background: Some(
                Color {
                    a: 0.75,
                    ..pal.code_bg
                }
                .into(),
            ),
            border: iced::Border {
                color: pal.code_border,
                width: 1.0,
                radius: theme::radius::LG.into(),
            },
            ..Default::default()
        });
    let close_btn = mouse_area(close_btn_inner)
        .interaction(iced::mouse::Interaction::Pointer)
        .on_press(Message::CloseOverlay);
    let close_overlay = container(close_btn)
        .padding(Padding::from([14, 16]))
        .align_x(iced::alignment::Horizontal::Right)
        .align_y(iced::alignment::Vertical::Top)
        .width(Length::Fill)
        .height(Length::Fill);
    stack![scrim_click, close_overlay].into()
}

pub(in crate::app) fn welcome_view<'a>(pal: Palette) -> Element<'a, Message> {
    let kbd = |label: &'static str, key: &'static str| {
        irow![
            container(
                text(key)
                    .size(12)
                    .color(pal.fg)
                    .shaping(iced::widget::text::Shaping::Advanced)
            )
            .padding(Padding::from([2, 7]))
            .style(move |_| container::Style {
                background: Some(pal.surface_alt.into()),
                border: Border {
                    color: pal.rule,
                    width: 1.0,
                    radius: theme::radius::SM.into(),
                },
                ..Default::default()
            }),
            text(label).size(13).color(pal.muted).font(iced::Font {
                family: iced::font::Family::Name("JetBrains Mono"),
                ..iced::Font::DEFAULT
            }),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center)
    };
    centered_card(
        column![
            text("rmdv").size(40).color(pal.fg),
            text("Lightweight, beautiful, native markdown viewer")
                .size(14)
                .color(pal.muted),
            Space::new().height(22),
            primary_button("Browse a Project as Mindmap", pal).on_press(Message::ToggleFullMindmap),
            Space::new().height(8),
            kbd("Open Folder", "⌘O"),
            kbd("Find File in Workspace", "⌘P"),
            kbd("Command Palette", "⌘⇧P"),
            kbd("Toggle Sidebar", "⌘B"),
            kbd("Find in Document", "⌘F"),
            kbd("Cycle Theme", "⌘T"),
            kbd("Edit / Select Text", "⌘E"),
            kbd("Fold to Level (then 0–6)", "⌘K"),
            kbd("Full Mindmap Mode", "⌘⇧M"),
        ]
        .spacing(8)
        .align_x(iced::Alignment::Start)
        .into(),
        pal,
    )
}

/// Transient modifier-driven Quick Slot rail. It deliberately has no enclosing
/// panel: the nine fixed controls float at the supplied left offset and inherit
/// the current theme roles used by the rest of the application.
pub(in crate::app) fn quick_slots_rail<'a>(
    app: &'a App,
    pal: Palette,
    left_offset: f32,
) -> Element<'a, Message> {
    let mut controls = Column::new().spacing(6).align_x(iced::Alignment::Start);
    for index in 0..crate::quick_slots::SLOT_COUNT {
        let occupied = app.quick_slots.occupied(index);
        let missing = occupied.is_some_and(|slot| {
            app.quick_slots_workspace_root()
                .and_then(|root| crate::quick_slots::resolve_path(root, &slot.relative_path))
                .map_or(true, |path| !path.is_file())
        });
        let active = app.quick_slots.active == Some(index);
        let assignable = app.current_quick_slot_context().is_some();
        let label = container(
            text((index + 1).to_string())
                .size(12)
                .font(editor_font())
                .color(if active {
                    pal.accent_fg
                } else if missing {
                    pal.accent
                } else {
                    pal.fg
                }),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center);
        let mut slot_button = button(label)
            .width(Length::Fixed(24.0))
            .height(Length::Fixed(24.0))
            .padding(Padding::ZERO)
            .style(move |_, status| {
                let background = if active {
                    Some(Background::Color(pal.accent))
                } else {
                    match status {
                        button::Status::Hovered | button::Status::Pressed => {
                            Some(Background::Color(pal.surface_alt))
                        }
                        _ => Some(Background::Color(pal.surface)),
                    }
                };
                button::Style {
                    background,
                    text_color: if active { pal.accent_fg } else { pal.fg },
                    border: Border {
                        color: if missing {
                            pal.accent
                        } else if occupied.is_some() {
                            pal.accent
                        } else {
                            pal.rule
                        },
                        width: 1.0,
                        radius: theme::radius::SM.into(),
                    },
                    ..Default::default()
                }
            });
        slot_button = if occupied.is_some() {
            slot_button.on_press(Message::QuickSlotActivate(index))
        } else if assignable {
            slot_button.on_press(Message::QuickSlotAssign(index))
        } else {
            slot_button
        };
        let slot_view: Element<'a, Message> = if let Some(slot) = occupied {
            let filename = if missing {
                format!("Missing: {}", slot.relative_path)
            } else {
                std::path::Path::new(&slot.relative_path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_owned)
                    .unwrap_or_else(|| slot.relative_path.clone())
            };
            let details = container(
                text(filename)
                    .size(12)
                    .color(if missing { pal.accent } else { pal.fg })
                    .wrapping(iced::widget::text::Wrapping::None),
            )
            .width(Length::Fixed(220.0))
            .height(Length::Fixed(24.0))
            .padding(Padding::from([0, 6]))
            .align_y(iced::alignment::Vertical::Center)
            .clip(true)
            .style(move |_| container::Style {
                background: Some(pal.surface.into()),
                border: Border {
                    color: pal.rule,
                    width: 1.0,
                    radius: theme::radius::MD.into(),
                },
                ..Default::default()
            });
            irow![slot_button, details]
                .spacing(6)
                .align_y(iced::Alignment::Center)
                .into()
        } else {
            irow![slot_button].into()
        };
        controls = controls.push(slot_view);
    }
    container(controls)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: left_offset,
        })
        .align_x(iced::alignment::Horizontal::Left)
        .align_y(iced::alignment::Vertical::Center)
        .into()
}

pub(in crate::app) fn search_bar_view<'a>(
    query: &'a str,
    matches: &'a [MatchPos],
    idx: usize,
    pal: Palette,
) -> Element<'a, Message> {
    let counter = if matches.is_empty() {
        if query.is_empty() {
            String::new()
        } else {
            "0/0".into()
        }
    } else {
        format!("{}/{}", idx + 1, matches.len())
    };
    container(
        irow![
            text("Find").size(12).color(pal.subtle),
            text_input("type to search…", query)
                .id(App::search_input_id())
                .on_input(Message::QueryChanged)
                .padding(Padding::from([6, 10]))
                .size(13)
                .style(move |_, _| iced::widget::text_input::Style {
                    background: pal.surface_alt.into(),
                    border: Border {
                        color: pal.rule,
                        width: 1.0,
                        radius: theme::radius::PILL.into(),
                    },
                    icon: pal.muted,
                    placeholder: pal.subtle,
                    value: pal.fg,
                    selection: pal.selection,
                })
                .width(Length::Fill),
            text(counter).color(pal.muted).size(12),
            ghost_lu(ic::CHEVRON_LEFT, pal).on_press(Message::PrevMatch),
            ghost_lu(ic::CHEVRON_RIGHT, pal).on_press(Message::NextMatch),
            ghost_lu(ic::X, pal).on_press(Message::ToggleSearch),
        ]
        .padding(Padding::from([8, 14]))
        .spacing(10)
        .align_y(iced::Alignment::Center),
    )
    .style(move |_| container::Style {
        background: Some(pal.surface.into()),
        border: Border {
            color: pal.rule,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    })
    .width(Length::Fill)
    .into()
}

/// Floating mind map keyboard hint. It sits over the canvas so side panels
/// remain dedicated to their selected document or folder preview.
pub(in crate::app) fn floating_mindmap_hint<'a>(
    items: &[(&'a str, &'a str)],
    pal: Palette,
) -> Element<'a, Message> {
    let island = container(hint_pills(items, pal))
        .padding(Padding::from([8, 16]))
        .clip(true)
        .style(move |_| container::Style {
            background: Some(pal.surface.into()),
            border: Border {
                color: pal.rule,
                width: 1.0,
                radius: theme::radius::XL.into(),
            },
            shadow: theme::shadow::HINT,
            ..Default::default()
        });

    container(island)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            top: 0.0,
            right: 0.0,
            bottom: 16.0,
            left: 0.0,
        })
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Bottom)
        .into()
}
