//! Floating chrome: update banner, footer, toasts, progress, image zoom, welcome screen, search bar, and the Quick Slot rail.

use super::*;
// Explicit import: the `column!` macro is otherwise ambiguous between the glob
// import and iced's exported macro.
use iced::widget::column;
use iced::widget::tooltip::Position;

/// Bottom-center banner inviting the user to install a downloaded update.
pub(in crate::app) fn update_banner<'a>(version: &str, pal: Palette) -> Element<'a, Message> {
    use iced::widget::{container, row, text as text_w};
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
    let later = secondary_button("Later", pal).on_press(Message::DismissUpdate);
    let bar = container(
        row![dot, label, Space::new().width(8), install, later]
            .spacing(10)
            .align_y(iced::alignment::Vertical::Center),
    )
    .padding(Padding::from([8, 10]))
    .style(move |_| container::Style {
        background: Some(pal.popover().into()),
        border: iced::Border {
            color: pal.border(),
            width: 1.0,
            radius: theme::radius::MD.into(),
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

/// The document counts the footer pill can show.
#[derive(Debug, Clone, Copy)]
pub(in crate::app) struct FooterCounts {
    pub(in crate::app) words: usize,
    pub(in crate::app) characters: usize,
}

/// The footer pill's text for `stat`. Reading time assumes ~200 wpm.
pub(in crate::app) fn footer_label(counts: FooterCounts, stat: crate::prefs::FooterStat) -> String {
    use crate::prefs::FooterStat;
    let plural = |n: usize, one: &str| format!("{n} {one}{}", if n == 1 { "" } else { "s" });
    let words = plural(counts.words, "word");
    let minutes = ((counts.words as f32) / 200.0).ceil().max(1.0) as usize;
    let time = format!("{minutes} min read");
    match stat {
        FooterStat::WordsAndTime => format!("{words} · {time}"),
        FooterStat::Words => words,
        FooterStat::ReadingTime => time,
        FooterStat::Characters => plural(counts.characters, "character"),
    }
}

/// The footer pill's id, for tests.
pub(in crate::app) const STATUS_FOOTER_ID: &str = "status-footer";

/// Bottom status pill: one document statistic. A click shows the next one
/// (saved in prefs).
pub(in crate::app) fn status_footer<'a>(
    counts: FooterCounts,
    stat: crate::prefs::FooterStat,
    pal: Palette,
) -> Element<'a, Message> {
    use iced::widget::{button, container, text as text_w};
    // Translucent pill so document content remains visible scrolling behind it.
    let mut pill_bg = pal.bg;
    pill_bg.a = 0.82;
    let pill = button(text_w(footer_label(counts, stat)).size(12.0))
        .padding([4, 12])
        .on_press(Message::FooterCycleStat)
        .style(move |_, status| {
            let (background, text_color) = match status {
                button::Status::Hovered => (pal.hover(), pal.fg),
                button::Status::Pressed => (pal.active(), pal.fg),
                _ => (pill_bg, pal.muted),
            };
            button::Style {
                background: Some(background.into()),
                text_color,
                border: iced::Border {
                    color: pal.border(),
                    width: 1.0,
                    radius: theme::radius::MD.into(),
                },
                ..Default::default()
            }
        });
    let pill = container(action_tip(
        pill,
        "Show Next Statistic",
        None,
        iced::widget::tooltip::Position::Top,
        pal,
    ))
    .id(STATUS_FOOTER_ID);
    // Float bottom-right over the reader; content scrolls underneath.
    container(pill)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([10, 14])
        .align_x(iced::alignment::Horizontal::Right)
        .align_y(iced::alignment::Vertical::Bottom)
        .into()
}

/// The destination of the link under the pointer, bottom left of the
/// reader, as a browser shows it. It takes no pointer events, so the link
/// under it stays hovered.
pub(in crate::app) fn link_pill<'a>(label: &str, pal: Palette, left: f32) -> Element<'a, Message> {
    use iced::widget::{container, text as text_w};
    let pill = container(
        text_w(label.to_string())
            .size(12.0)
            .color(pal.fg)
            .wrapping(iced::widget::text::Wrapping::None),
    )
    .padding([4, 10])
    .clip(true)
    .style(move |_| container::Style {
        background: Some(pal.popover().into()),
        border: iced::Border {
            color: pal.border(),
            width: 1.0,
            radius: theme::radius::PILL.into(),
        },
        shadow: theme::shadow::HINT,
        ..Default::default()
    });
    container(pill)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            top: 0.0,
            right: LINK_PILL_RIGHT_ROOM,
            bottom: 10.0,
            left: left + 12.0,
        })
        .align_x(iced::alignment::Horizontal::Left)
        .align_y(iced::alignment::Vertical::Bottom)
        .into()
}

/// Room kept on the right of the link pill for the word-count footer.
const LINK_PILL_RIGHT_ROOM: f32 = 220.0;

/// A toast's action button (Save, Undo): an accent pill.
pub(in crate::app) fn toast_action_style(status: button::Status, pal: Palette) -> button::Style {
    button::Style {
        background: Some(Background::Color(accent_fill(status, pal))),
        text_color: pal.accent_fg,
        border: Border {
            radius: theme::radius::PILL.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub(in crate::app) fn toast_overlay<'a>(toast: &Toast, pal: Palette) -> Element<'a, Message> {
    use iced::widget::{button, container, text as text_w};
    let mut content = irow![text_w(toast.text.clone()).size(13.5).color(pal.fg)]
        .spacing(10)
        .align_y(iced::alignment::Vertical::Center);
    if let Some(action) = &toast.action {
        let action_button = button(text_w(action.label.clone()).size(12.5).color(pal.accent_fg))
            .padding(Padding::from([5, 10]))
            .style(move |_, status| toast_action_style(status, pal))
            .on_press(action.message.clone());
        content = content.push(action_button);
    }
    // Toasts that stay long (an action or an error) can be closed early.
    if toast.action.is_some() || toast.error {
        content = content.push(
            container(ghost_lu(ic::X, pal).on_press(Message::ToastExpire(toast.id)))
                .id(App::toast_close_id()),
        );
    }
    let bubble = container(content)
        .padding([8, 14])
        .style(move |_| container::Style {
            background: Some(pal.popover().into()),
            border: iced::Border {
                color: pal.border(),
                width: 1.0,
                radius: theme::radius::MD.into(),
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
        background: Some(pal.popover().into()),
        border: Border {
            color: pal.border(),
            width: 1.0,
            radius: theme::radius::MD.into(),
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

/// What the image viewer takes from the app besides the image: the latest
/// zoom button or key, and the native pinch stream.
pub(in crate::app) struct ImageZoomInput {
    pub request: crate::mindmap::ZoomRequest,
    pub pinch_log: f64,
    /// A diagram's 100% size (its inline raster's), which a sharper raster
    /// keeps. Set only for diagrams, which also report the detail they want.
    pub content: Option<iced::Size>,
}

pub(in crate::app) fn image_zoom_overlay<'a>(
    url: Option<&'a str>,
    diagram: Option<&iced::widget::image::Handle>,
    cache: &ImageCache,
    input: ImageZoomInput,
    pal: Palette,
) -> Element<'a, Message> {
    let mk_viewer = |h: iced::widget::image::Handle| -> Element<'a, Message> {
        let viewer = crate::zoom_viewer::ZoomViewer::new(h)
            .request(input.request)
            .pinch_log(input.pinch_log)
            .on_close(Message::CloseOverlay);
        match input.content {
            Some(size) => viewer
                .content_size(size)
                .on_detail(ZOOM_RASTER_DENSITY, Message::ImageZoomDetail)
                .into(),
            None => viewer.into(),
        }
    };
    // Diagram overrides image source when set — DiagramZoom clears zoom_url.
    let mut ready = true;
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
                    ready = false;
                    text("rendering…").color(pal.muted).into()
                }
                Some(ImageState::Failed) => {
                    ready = false;
                    text("image unavailable").color(pal.muted).into()
                }
                None => {
                    // Local raster path (cache only stores svg/remote). Use direct viewer.
                    let p = std::path::PathBuf::from(u);
                    mk_viewer(iced::widget::image::Handle::from_path(p))
                }
            },
            None => {
                ready = false;
                text("").into()
            }
        }
    };
    // The viewer fills the scrim and closes on a still click beside the
    // image. While there is no image yet, a click anywhere closes. The idle
    // cursor and the swallowed wheel keep the reader below inert.
    let scrim = container(inner)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Color { a: 0.85, ..pal.bg }.into()),
            ..Default::default()
        });
    let mut scrim_click = mouse_area(scrim)
        .interaction(iced::mouse::Interaction::Idle)
        .on_scroll(|_| Message::Noop);
    if !ready {
        scrim_click = scrim_click.on_press(Message::CloseOverlay);
    }
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
                radius: theme::radius::SM.into(),
            },
            ..Default::default()
        });
    let close_btn = action_tip(
        mouse_area(close_btn_inner)
            .interaction(iced::mouse::Interaction::Pointer)
            .on_press(Message::CloseOverlay),
        "Close",
        Some("Esc"),
        Position::Left,
        pal,
    );
    let close_overlay = container(close_btn)
        .padding(Padding::from([14, 16]))
        .align_x(iced::alignment::Horizontal::Right)
        .align_y(iced::alignment::Vertical::Top)
        .width(Length::Fill)
        .height(Length::Fill);
    let zoom_island: Element<'a, Message> = if ready {
        zoom_controls_island(&[], pal, Message::ImageZoomCommand)
    } else {
        Space::new().into()
    };
    stack![scrim_click, zoom_island, close_overlay].into()
}

pub(in crate::app) fn welcome_view<'a>(
    recent: &'a crate::recent::Recent,
    pal: Palette,
) -> Element<'a, Message> {
    // A narrow reader (a small window beside the sidebar) puts Recent below
    // the shortcuts, and a short one scrolls. The card stays centered while
    // it fits: the scrollable is exactly as tall as the reader, so the card's
    // container is at least that tall.
    iced::widget::responsive(move |size| {
        let content: Element<'a, Message> = match welcome_recent(recent, pal) {
            Some(recent) => irow![welcome_shortcuts(pal), recent]
                .spacing(40)
                .wrap()
                .vertical_spacing(28)
                .into(),
            None => welcome_shortcuts(pal),
        };
        scrollable(centered_card(content, pal))
            .width(Length::Fill)
            .height(Length::Fixed(size.height))
            .direction(slim_scroll_direction())
            .style(move |_, status| sleek_scrollable_style(status, pal, false))
            .into()
    })
    .into()
}

/// The welcome page's title, tagline, and shortcut rows.
fn welcome_shortcuts<'a>(pal: Palette) -> Element<'a, Message> {
    // Each row is a button that sends its shortcut's message; the key cap
    // stays visible as the hint.
    let kbd = |label: &'static str, key: &'static str, message: Message| {
        let row = irow![
            container(
                text(key)
                    .size(12)
                    .color(pal.fg)
                    .shaping(iced::widget::text::Shaping::Advanced)
            )
            .padding(Padding::from([2, 7]))
            .style(move |_| container::Style {
                background: Some(pal.chip().into()),
                border: Border {
                    color: pal.border(),
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
        .align_y(iced::Alignment::Center);
        button(row)
            .padding(Padding::from([3, 6]))
            .style(move |_, status| button::Style {
                background: match status {
                    button::Status::Hovered => Some(Background::Color(pal.hover())),
                    button::Status::Pressed => Some(Background::Color(pal.active())),
                    _ => None,
                },
                text_color: pal.fg,
                border: Border {
                    radius: theme::radius::SM.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .on_press(message)
    };
    let start = column![
        text("rmdv").size(40).color(pal.fg),
        text("Lightweight, beautiful, native markdown viewer")
            .size(14)
            .color(pal.muted),
        Space::new().height(22),
        primary_button("Browse a Project as Mindmap", pal).on_press(Message::ToggleFullMindmap),
        Space::new().height(8),
        kbd("Open Folder", "⌘O", Message::OpenFolderPicker),
        kbd("Find File in Workspace", "⌘P", Message::OpenFileFinder),
        kbd("Command Palette", "⌘⇧P", Message::OpenCommandPalette),
        kbd("Toggle Sidebar", "⌘B", Message::ToggleSidebar),
        kbd("Find in Document", "⌘F", Message::ToggleSearch),
        kbd("Cycle Theme", "⌘T", Message::ToggleTheme),
        // Edit and Fold act on an open document; here they explain that.
        kbd("Edit / Select Text", "⌘E", Message::WelcomeNeedsFile),
        kbd("Fold to Level (then 0–6)", "⌘K", Message::WelcomeNeedsFile),
        kbd("Full Mindmap Mode", "⌘⇧M", Message::ToggleFullMindmap),
    ]
    .spacing(8)
    .align_x(iced::Alignment::Start);
    start.into()
}

/// The welcome page's Recent Folders and Recent Files: one row per entry,
/// which opens that folder as the workspace or that file. `None` when there
/// is nothing to list.
fn welcome_recent<'a>(
    recent: &crate::recent::Recent,
    pal: Palette,
) -> Option<Element<'a, Message>> {
    if recent.paths.is_empty() && recent.folders.is_empty() {
        return None;
    }
    let home = dirs::home_dir();
    let row = |path: &Path, glyph: char| {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_string();
        let parent = path.parent().map(|parent| {
            match home
                .as_deref()
                .and_then(|home| parent.strip_prefix(home).ok())
            {
                Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
                Some(rest) => format!("~/{}", rest.display()),
                None => parent.display().to_string(),
            }
        });
        let mut label = column![text(name)
            .size(13)
            .color(pal.fg)
            .wrapping(iced::widget::text::Wrapping::None)]
        .spacing(1);
        if let Some(parent) = parent {
            label = label.push(
                text(parent)
                    .size(11)
                    .color(pal.subtle)
                    .wrapping(iced::widget::text::Wrapping::None),
            );
        }
        button(
            irow![
                icon::glyph(glyph, 13.0, pal.muted),
                container(label).width(Length::Fill).clip(true),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        )
        .width(Length::Fill)
        .padding(Padding::from([4, 6]))
        .style(move |_, status| button::Style {
            background: match status {
                button::Status::Hovered => Some(Background::Color(pal.hover())),
                button::Status::Pressed => Some(Background::Color(pal.active())),
                _ => None,
            },
            text_color: pal.fg,
            border: Border {
                radius: theme::radius::SM.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .on_press(Message::OpenFileFinderPath(path.to_path_buf()))
    };
    let heading = |label: &'static str| text(label).size(11).color(pal.subtle);
    let mut list = Column::new().spacing(4).width(Length::Fixed(240.0));
    if !recent.folders.is_empty() {
        list = list.push(heading("Recent Folders"));
        for folder in &recent.folders {
            list = list.push(row(folder, ic::FOLDER));
        }
        list = list.push(Space::new().height(10));
    }
    if !recent.paths.is_empty() {
        list = list.push(heading("Recent Files"));
        for file in &recent.paths {
            list = list.push(row(file, ic::FILE_TEXT));
        }
    }
    Some(list.into())
}

/// Container id of the pinned Quick Slots popover.
pub(in crate::app) const QUICK_SLOTS_POPOVER_ID: &str = "quick-slots-popover";

/// Side of a Quick Slot chip: a 28 px pointer target.
const QUICK_SLOT_CHIP: f32 = 28.0;

/// Container ids of the rail's slot rows, slot 1 first.
pub(in crate::app) const QUICK_SLOT_CHIP_IDS: [&str; crate::quick_slots::SLOT_COUNT] = [
    "quick-slot-1",
    "quick-slot-2",
    "quick-slot-3",
    "quick-slot-4",
    "quick-slot-5",
    "quick-slot-6",
    "quick-slot-7",
    "quick-slot-8",
    "quick-slot-9",
];

/// What a Quick Slot chip shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app) enum SlotChipLook {
    /// The slot of the open file.
    Active,
    /// Holds a file.
    Occupied,
    /// Holds a file that no longer exists.
    Missing,
    /// Empty; a click assigns the open file.
    Empty,
    /// Empty, and there is no open file to assign.
    Unassignable,
}

/// A Quick Slot chip's colors.
pub(in crate::app) fn quick_slot_chip_style(
    look: SlotChipLook,
    status: button::Status,
    pal: Palette,
) -> button::Style {
    let active = look == SlotChipLook::Active;
    let background = if active {
        Some(Background::Color(pal.accent))
    } else if look == SlotChipLook::Unassignable {
        // A slot that does nothing: an outline only, no fill or hover.
        None
    } else {
        match status {
            button::Status::Hovered => Some(Background::Color(pal.hover())),
            button::Status::Pressed => Some(Background::Color(pal.active())),
            _ => Some(Background::Color(pal.surface)),
        }
    };
    button::Style {
        background,
        text_color: match look {
            SlotChipLook::Active => pal.accent_fg,
            SlotChipLook::Unassignable => pal.subtle,
            _ => pal.fg,
        },
        border: Border {
            color: match look {
                SlotChipLook::Missing | SlotChipLook::Occupied => pal.accent,
                _ => pal.border(),
            },
            width: 1.0,
            radius: theme::radius::SM.into(),
        },
        ..Default::default()
    }
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
        let missing =
            occupied.is_some_and(|slot| slot.resolve().map_or(true, |path| !path.is_file()));
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
                } else if occupied.is_none() && !assignable {
                    pal.subtle
                } else {
                    pal.fg
                }),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center);
        let look = if active {
            SlotChipLook::Active
        } else if missing {
            SlotChipLook::Missing
        } else if occupied.is_some() {
            SlotChipLook::Occupied
        } else if assignable {
            SlotChipLook::Empty
        } else {
            SlotChipLook::Unassignable
        };
        let slot_button = button(label)
            .width(Length::Fixed(QUICK_SLOT_CHIP))
            .height(Length::Fixed(QUICK_SLOT_CHIP))
            .padding(Padding::ZERO)
            .style(move |_, status| quick_slot_chip_style(look, status, pal));
        let mut slot_button = slot_button;
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
                slot.file_name()
            };
            let details = container(
                text(filename)
                    .size(12)
                    .color(if missing { pal.accent } else { pal.fg })
                    .wrapping(iced::widget::text::Wrapping::None),
            )
            .width(Length::Fixed(220.0))
            .height(Length::Fixed(QUICK_SLOT_CHIP))
            .padding(Padding::from([0, 6]))
            .align_y(iced::alignment::Vertical::Center)
            .clip(true)
            .style(move |_| container::Style {
                background: Some(pal.surface.into()),
                border: Border {
                    color: pal.border(),
                    width: 1.0,
                    radius: theme::radius::SM.into(),
                },
                ..Default::default()
            });
            // A slot from another workspace names it in its tooltip; the
            // title stays the file name.
            let details: Element<'a, Message> = if app.quick_slot_in_workspace(slot) {
                details.into()
            } else {
                label_tip(
                    details,
                    format!("{} — {}", slot.file_name(), slot.workspace_name()),
                    iced::widget::tooltip::Position::Right,
                    pal,
                )
            };
            irow![slot_button, details]
                .spacing(6)
                .align_y(iced::Alignment::Center)
                .into()
        } else {
            irow![slot_button].into()
        };
        let slot_view =
            container(mouse_area(slot_view).on_right_press(Message::QuickSlotContextMenu(index)))
                .id(iced::widget::Id::new(QUICK_SLOT_CHIP_IDS[index]));
        controls = controls.push(slot_view);
    }
    // Pinned from the toolbar, the rail is a popover: a panel that closes
    // on a press outside it and keeps a press on its padding or between its
    // chips from the reader under it. The ⌘ hold keeps the bare rail.
    let rail: Element<'a, Message> = if app.quick_slots_pinned {
        PressOutside::new(
            iced::widget::opaque(
                container(controls)
                    .id(iced::widget::Id::new(QUICK_SLOTS_POPOVER_ID))
                    .padding(6)
                    .style(move |_| container::Style {
                        background: Some(pal.surface.into()),
                        border: Border {
                            color: pal.border(),
                            width: 1.0,
                            radius: theme::radius::MD.into(),
                        },
                        shadow: theme::shadow::MENU,
                        ..Default::default()
                    }),
            ),
            Message::QuickSlotsPopover(false),
        )
        .into()
    } else {
        controls.into()
    };
    container(rail)
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
    matches: &'a Matches,
    idx: usize,
    pending: bool,
    pal: Palette,
) -> Element<'a, Message> {
    let counter = if pending {
        "…".into()
    } else if matches.is_empty() {
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
                    background: pal.field().into(),
                    border: Border {
                        color: pal.border(),
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
            action_tip(
                ghost_lu(ic::CHEVRON_LEFT, pal).on_press(Message::FindStepClicked(false)),
                "Previous Match",
                Some("⇧↩"),
                Position::Bottom,
                pal,
            ),
            action_tip(
                ghost_lu(ic::CHEVRON_RIGHT, pal).on_press(Message::FindStepClicked(true)),
                "Next Match",
                Some("↩"),
                Position::Bottom,
                pal,
            ),
            action_tip(
                ghost_lu(ic::X, pal).on_press(Message::ToggleSearch),
                "Close Find",
                Some("Esc"),
                Position::Bottom,
                pal,
            ),
        ]
        .padding(Padding::from([8, 14]))
        .spacing(10)
        .align_y(iced::Alignment::Center),
    )
    .style(move |_| container::Style {
        background: Some(pal.surface.into()),
        border: Border {
            color: pal.border(),
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
            background: Some(pal.popover().into()),
            border: Border {
                color: pal.border(),
                width: 1.0,
                radius: theme::radius::MD.into(),
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

/// About how wide the mind map island is with its hints: four zoom
/// buttons with the divider and padding, then each hint pill's key and
/// label at 11 px. Generous, so the hints go before the buttons clip.
fn mindmap_controls_width(items: &[(&str, &str)]) -> f32 {
    const BUTTONS: f32 = 190.0;
    const CHAR: f32 = 7.5;
    // Key padding, the gap to the label, and the gap to the next pill.
    const PILL: f32 = 12.0 + 6.0 + 12.0;
    let pills: f32 = items
        .iter()
        .map(|(key, label)| (key.chars().count() + label.chars().count()) as f32 * CHAR + PILL)
        .sum();
    BUTTONS + 20.0 + pills
}

/// The mind map's floating island: its keyboard hints, then Zoom to Fit,
/// Zoom Out, 100%, and Zoom In. Only the buttons take pointer input; the
/// hints and the rest of the layer leave the canvas its events. A canvas
/// too narrow for the hints shows the buttons alone, so they never clip.
pub(in crate::app) fn floating_mindmap_controls<'a>(
    items: &[(&'a str, &'a str)],
    pal: Palette,
) -> Element<'a, Message> {
    let items = items.to_vec();
    let full_width = mindmap_controls_width(&items);
    iced::widget::responsive(move |size| {
        zoom_controls_island(
            if size.width >= full_width {
                &items
            } else {
                &[]
            },
            pal,
            Message::MindmapZoom,
        )
    })
    .into()
}

/// The floating zoom island of the mind map and the image viewer: optional
/// keyboard hints, then Zoom to Fit, Zoom Out, 100%, and Zoom In, each
/// sending `on_zoom`.
pub(in crate::app) fn zoom_controls_island<'a>(
    items: &[(&'a str, &'a str)],
    pal: Palette,
    on_zoom: fn(crate::mindmap::ZoomCommand) -> Message,
) -> Element<'a, Message> {
    use crate::mindmap::ZoomCommand;
    let zoom = |content: Element<'a, Message>, command: ZoomCommand| {
        button(content)
            .padding(Padding::from([4, 8]))
            .style(move |_, status| button::Style {
                background: match status {
                    button::Status::Hovered => Some(Background::Color(pal.hover())),
                    button::Status::Pressed => Some(Background::Color(pal.active())),
                    _ => None,
                },
                text_color: pal.muted,
                border: Border {
                    radius: theme::radius::PILL.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .on_press(on_zoom(command))
    };
    let glyph = |code: char| -> Element<'a, Message> { icon::glyph(code, 14.0, pal.muted).into() };
    let buttons = irow![
        action_tip(
            zoom(glyph(ic::SCAN), ZoomCommand::Fit),
            "Zoom to Fit",
            None,
            Position::Top,
            pal,
        ),
        action_tip(
            zoom(glyph(ic::MINUS), ZoomCommand::Out),
            "Zoom Out",
            Some("−"),
            Position::Top,
            pal,
        ),
        action_tip(
            zoom(
                text("100%").size(11).color(pal.muted).into(),
                ZoomCommand::Reset
            ),
            "Actual Size",
            Some("0"),
            Position::Top,
            pal,
        ),
        action_tip(
            zoom(glyph(ic::PLUS), ZoomCommand::In),
            "Zoom In",
            Some("="),
            Position::Top,
            pal,
        ),
    ]
    .spacing(2)
    .align_y(iced::Alignment::Center);
    let divider = container(Space::new().width(1).height(16)).style(move |_| container::Style {
        background: Some(pal.border().into()),
        ..Default::default()
    });
    let mut content = irow![].spacing(6).align_y(iced::Alignment::Center);
    if !items.is_empty() {
        content = content
            .push(container(hint_pills(items, pal)).padding(Padding::from([0, 10])))
            .push(divider);
    }
    let island = container(content.push(buttons))
        .padding(Padding::from([4, 6]))
        .clip(true)
        .style(move |_| container::Style {
            background: Some(pal.popover().into()),
            border: Border {
                color: pal.border(),
                width: 1.0,
                radius: theme::radius::MD.into(),
            },
            shadow: theme::shadow::HINT,
            ..Default::default()
        });
    // The pill takes a press beside its buttons (the gaps and the padding),
    // so a near miss does not reach the viewer or the canvas below. The
    // full-window container around it lets every other press through.
    let island = mouse_area(island).on_press(Message::Noop);

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

/// The frame shown while files are dragged over the window: a thin accent
/// border around the window and a centered "Open in rmdv" label. It draws
/// only; the pointer passes through it.
pub(in crate::app) fn drop_highlight<'a>(pal: Palette) -> Element<'a, Message> {
    let label = container(text("Open in rmdv").size(13).color(pal.accent_fg))
        .padding(Padding::from([6, 14]))
        .style(move |_| container::Style {
            background: Some(pal.accent.into()),
            border: Border {
                radius: theme::radius::PILL.into(),
                ..Default::default()
            },
            shadow: theme::shadow::HINT,
            ..Default::default()
        });
    container(label)
        .id(iced::widget::Id::new("drop-highlight"))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(move |_| container::Style {
            background: Some(
                Color {
                    a: 0.06,
                    ..pal.accent
                }
                .into(),
            ),
            border: Border {
                color: pal.accent,
                width: 2.0,
                radius: theme::radius::MD.into(),
            },
            ..Default::default()
        })
        .into()
}
