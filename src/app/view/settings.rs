//! The Settings page (⌘,): every user preference with its current value.

use super::*;
// Explicit import: the `column!` macro is otherwise ambiguous between the glob
// import and iced's exported macro.
use iced::widget::column;

/// Theme cards per line on the Theme row.
const THEME_CARDS_PER_LINE: usize = 5;

/// Full reader-area page. One centered column with section headers and one
/// row per setting; the row under the keyboard cursor is highlighted.
pub(in crate::app) fn settings_page<'a>(
    app: &'a App,
    pal: Palette,
    // Page fill; clear when the window glass shows through the reader.
    fill: Color,
) -> Element<'a, Message> {
    let rows = SettingsRow::visible();
    let mut page = Column::new().spacing(0);
    page = page.push(
        irow![
            container(text("Settings").size(22).color(pal.fg)).width(Length::Fill),
            iced::widget::tooltip(
                ghost_lu(ic::X, pal).on_press(Message::CloseSettings),
                text("Esc").size(11).color(pal.muted),
                iced::widget::tooltip::Position::Bottom,
            ),
        ]
        .align_y(iced::Alignment::Center),
    );

    let mut section = None;
    for (index, row) in rows.iter().copied().enumerate() {
        let title = section_title(row);
        if section != Some(title) {
            section = Some(title);
            page = page.push(
                container(text(title.to_uppercase()).size(11).color(pal.muted)).padding(Padding {
                    top: 26.0,
                    right: 14.0,
                    bottom: 6.0,
                    left: 14.0,
                }),
            );
        } else {
            page = page.push(
                container(
                    container(Space::new().height(1.0))
                        .width(Length::Fill)
                        .style(move |_| container::Style {
                            background: Some(pal.border().into()),
                            ..Default::default()
                        }),
                )
                .padding(Padding::from([0, 14])),
            );
        }
        page = page.push(settings_row(
            index,
            app.settings_cursor == index,
            settings_row_body(app, row, pal),
            pal,
        ));
    }

    page = page.push(
        container(
            text("↑ ↓ move    Space / Enter toggle    ← → adjust    Esc or ⌘, close")
                .size(12)
                .color(pal.subtle),
        )
        .padding(Padding {
            top: 24.0,
            right: 14.0,
            bottom: 0.0,
            left: 14.0,
        }),
    );

    let body = scrollable(
        container(
            container(page)
                .max_width(760.0)
                .padding(Padding::from([40, 32])),
        )
        .width(Length::Fill)
        .center_x(Length::Fill),
    )
    .id(App::settings_scroll_id())
    .height(Length::Fill)
    .width(Length::Fill)
    .direction(slim_scroll_direction())
    .style(move |_, status| sleek_scrollable_style(status, pal, true));

    container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(fill.into()),
            ..Default::default()
        })
        .into()
}

fn section_title(row: SettingsRow) -> &'static str {
    match row {
        SettingsRow::Theme
        | SettingsRow::SoftSyntax
        | SettingsRow::Glass
        | SettingsRow::GlassOpacity
        | SettingsRow::FontSize => "Appearance",
        SettingsRow::Footer | SettingsRow::HiddenFiles | SettingsRow::MindmapAutocenter => {
            "Reading"
        }
        SettingsRow::AutoFocus | SettingsRow::Cli => "Agent & CLI",
        SettingsRow::ThemesFolder | SettingsRow::PrefsFile => "Advanced",
    }
}

/// One row: hover moves the keyboard cursor here; the cursor row is filled.
fn settings_row<'a>(
    index: usize,
    cursor: bool,
    body: Element<'a, Message>,
    pal: Palette,
) -> Element<'a, Message> {
    mouse_area(
        container(body)
            .id(App::settings_row_id(index))
            .width(Length::Fill)
            .padding(Padding::from([12, 14]))
            .style(move |_| container::Style {
                background: cursor.then(|| pal.active().into()),
                border: Border {
                    radius: theme::radius::SM.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
    )
    .on_enter(Message::SettingsCursor(index))
    .into()
}

/// Label and one-line hint on the left, the control on the right.
fn labeled<'a>(
    label: &'a str,
    hint: String,
    control: Element<'a, Message>,
    pal: Palette,
) -> Element<'a, Message> {
    irow![
        column![
            text(label).size(14).color(pal.fg),
            text(hint).size(12.5).color(pal.muted),
        ]
        .spacing(3)
        .width(Length::Fill),
        control,
    ]
    .spacing(16)
    .align_y(iced::Alignment::Center)
    .into()
}

fn settings_row_body<'a>(app: &'a App, row: SettingsRow, pal: Palette) -> Element<'a, Message> {
    match row {
        SettingsRow::Theme => theme_row(app, pal),
        SettingsRow::SoftSyntax => labeled(
            "Soft syntax colors",
            "Desaturated code colors on top of any theme.".into(),
            switch(app.prefs.soft_syntax, Message::ToggleSoftSyntax, pal),
            pal,
        ),
        SettingsRow::Glass => glass_row(app, pal),
        SettingsRow::GlassOpacity => {
            let control: Element<'a, Message> = if app.prefs.glass == Glass::Off {
                text("Glass is off").size(13).color(pal.subtle).into()
            } else {
                let opacity = app.glass_opacity();
                irow![
                    text("60%").size(11.5).color(pal.subtle),
                    iced::widget::slider(0.6..=0.9, opacity, Message::SetGlassOpacity)
                        .step(GLASS_OPACITY_STEP)
                        .width(Length::Fixed(160.0))
                        .style(move |_, status| slider_style(status, pal)),
                    text("90%").size(11.5).color(pal.subtle),
                    container(
                        text(format!("{:.0}%", opacity * 100.0))
                            .size(13)
                            .color(pal.fg),
                    )
                    .width(Length::Fixed(40.0))
                    .align_x(iced::alignment::Horizontal::Right),
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center)
                .into()
            };
            labeled(
                "Glass opacity",
                "How strongly the theme tints the glass.".into(),
                control,
                pal,
            )
        }
        SettingsRow::FontSize => labeled(
            "Font size",
            "Reader text size. Same as ⌘+ and ⌘−.".into(),
            irow![
                small_button("−", Message::FontSizeDown, pal),
                container(
                    text(format!("{:.0}%", app.font_scale * 100.0))
                        .size(13)
                        .color(pal.fg),
                )
                .width(Length::Fixed(52.0))
                .align_x(iced::alignment::Horizontal::Center),
                small_button("+", Message::FontSizeUp, pal),
                text_button("Reset", Message::FontSizeReset, pal),
            ]
            .spacing(6)
            .align_y(iced::Alignment::Center)
            .into(),
            pal,
        ),
        SettingsRow::Footer => labeled(
            "Status footer",
            "Word count and reading time below the document.".into(),
            switch(app.show_footer, Message::ToggleFooter, pal),
            pal,
        ),
        SettingsRow::HiddenFiles => labeled(
            "Show hidden files",
            "Dot files and folders in the sidebar. Same as ⌘⇧.".into(),
            switch(app.show_hidden, Message::ToggleHidden, pal),
            pal,
        ),
        SettingsRow::MindmapAutocenter => labeled(
            "Mindmap auto-center",
            "Keep the selected node in view while you move.".into(),
            switch(
                app.mindmap_autocenter,
                Message::ToggleMindmapAutocenter,
                pal,
            ),
            pal,
        ),
        SettingsRow::AutoFocus => labeled(
            "Focus window on agent navigation",
            "Bring rmdv to the front when an agent opens a file over the CLI.".into(),
            switch(
                app.prefs.auto_focus_on_nav,
                Message::ToggleAutoFocusOnNav,
                pal,
            ),
            pal,
        ),
        SettingsRow::Cli => {
            let offer = crate::cli_install::should_offer();
            let installed = offer && crate::cli_install::is_installed();
            let path = crate::cli_install::install_path()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            let hint = if !offer {
                "Available in the packaged app.".to_string()
            } else if installed {
                format!("Installed at {path}")
            } else {
                format!("Not installed. Installs to {path}")
            };
            let label = if installed {
                "Reinstall"
            } else {
                "Install CLI"
            };
            labeled(
                "Command-line tool",
                hint,
                outline_button(label, offer.then_some(Message::InstallCli), pal),
                pal,
            )
        }
        SettingsRow::ThemesFolder => labeled(
            "Themes folder",
            "Add custom themes as TOML files.".into(),
            irow![
                outline_button("Open", Some(Message::OpenThemesDir), pal),
                outline_button("Reload themes", Some(Message::ReloadThemes), pal),
            ]
            .spacing(6)
            .into(),
            pal,
        ),
        SettingsRow::PrefsFile => labeled(
            "Preferences file",
            crate::prefs::store_path()
                .map(|p| tilde_path(&p))
                .unwrap_or_else(|| "Not available on this system".into()),
            outline_button(
                "Reveal in Finder",
                cfg!(target_os = "macos").then_some(Message::RevealPrefsFile),
                pal,
            ),
            pal,
        ),
    }
}

fn theme_row<'a>(app: &'a App, pal: Palette) -> Element<'a, Message> {
    let entries = app.theme_entries();
    let current = entries
        .iter()
        .find(|e| e.matches_current(&app.theme_id))
        .map(|e| e.label().to_string())
        .unwrap_or_default();
    let mut grid = Column::new().spacing(10);
    for line in entries.chunks(THEME_CARDS_PER_LINE) {
        let mut cards = irow![].spacing(10);
        for entry in line {
            cards = cards.push(theme_card(entry, entry.matches_current(&app.theme_id), pal));
        }
        // Pad the last line so its cards keep the same width.
        for _ in line.len()..THEME_CARDS_PER_LINE {
            cards = cards.push(Space::new().width(Length::Fill));
        }
        grid = grid.push(cards);
    }
    column![
        irow![
            container(text("Theme").size(14).color(pal.fg)).width(Length::Fill),
            text(current).size(12.5).color(pal.muted),
        ]
        .align_y(iced::Alignment::Center),
        grid,
        text("Click a theme to switch; it is saved at once. Custom themes from the themes folder follow the presets.")
            .size(12)
            .color(pal.muted),
    ]
    .spacing(12)
    .into()
}

/// A theme swatch: its ground, sidebar strip, accent, and text lines.
fn theme_card<'a>(entry: &ThemeEntry, selected: bool, pal: Palette) -> Element<'a, Message> {
    let tp = entry.palette();
    let bar = |color: Color, height: f32, fill: u16, rest: u16| {
        irow![
            container(Space::new().height(height))
                .width(Length::FillPortion(fill))
                .style(move |_| container::Style {
                    background: Some(color.into()),
                    border: Border {
                        radius: (height / 2.0).into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            Space::new().width(Length::FillPortion(rest)),
        ]
    };
    let ink = |a: f32| Color { a, ..tp.fg };
    let preview = container(irow![
        container(Space::new())
            .width(Length::Fixed(16.0))
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(tp.sidebar.into()),
                ..Default::default()
            }),
        column![
            bar(tp.accent, 4.0, 3, 2),
            bar(ink(0.55), 3.0, 5, 1),
            bar(ink(0.35), 3.0, 4, 2),
        ]
        .spacing(5)
        .padding(Padding::from([9, 8])),
    ])
    .height(Length::Fixed(46.0))
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: Some(tp.bg.into()),
        ..Default::default()
    });
    let mut label = irow![container(
        text(entry.label().to_string())
            .size(11.5)
            .color(tp.fg)
            .wrapping(iced::widget::text::Wrapping::None),
    )
    .width(Length::Fill)
    .clip(true)]
    .align_y(iced::Alignment::Center)
    .spacing(4);
    if selected {
        label = label.push(icon::glyph(ic::CHECK, 12.0, tp.accent));
    }
    let label = container(label)
        .padding(Padding::from([5, 8]))
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(tp.sidebar.into()),
            ..Default::default()
        });
    button(column![preview, label])
        .padding(0)
        .width(Length::Fill)
        .on_press(entry.message())
        .style(move |_, status| {
            let ring = if selected {
                pal.accent
            } else if matches!(status, button::Status::Hovered) {
                pal.border_strong()
            } else {
                pal.border()
            };
            button::Style {
                background: Some(tp.bg.into()),
                border: Border {
                    color: ring,
                    width: if selected { 2.0 } else { 1.0 },
                    radius: theme::radius::MD.into(),
                },
                ..Default::default()
            }
        })
        .clip(true)
        .into()
}

fn glass_row<'a>(app: &'a App, pal: Palette) -> Element<'a, Message> {
    let mut modes = irow![].spacing(2);
    for (glass, label) in [
        (Glass::Off, "Off"),
        (Glass::Sidebar, "Sidebar"),
        (Glass::Window, "Window"),
    ] {
        let on = app.prefs.glass == glass;
        modes = modes.push(
            button(text(label).size(12.5))
                .padding(Padding::from([5, 12]))
                .on_press(Message::SetGlass(glass))
                .style(move |_, status| button::Style {
                    background: if on {
                        Some(pal.surface_alt.into())
                    } else if matches!(status, button::Status::Hovered) {
                        Some(pal.hover().into())
                    } else {
                        None
                    },
                    text_color: if on { pal.fg } else { pal.muted },
                    border: Border {
                        radius: theme::radius::SM.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
        );
    }
    let segmented = container(modes)
        .padding(2)
        .style(move |_| container::Style {
            background: Some(pal.chip().into()),
            border: Border {
                color: pal.border(),
                width: 1.0,
                radius: (theme::radius::SM + 2.0).into(),
            },
            ..Default::default()
        });
    let main = labeled(
        "Window glass",
        "Let the blurred desktop show through. macOS only.".into(),
        segmented.into(),
        pal,
    );
    if !app.glass_needs_restart() {
        return main;
    }
    column![
        main,
        container(
            irow![
                container(
                    text("Turning glass on takes effect after a restart.")
                        .size(12.5)
                        .color(pal.muted),
                )
                .width(Length::Fill),
                primary_button("Restart rmdv", pal).on_press(Message::RestartApp),
            ]
            .spacing(12)
            .align_y(iced::Alignment::Center),
        )
        .padding(Padding::from([8, 10]))
        .style(move |_| container::Style {
            background: Some(pal.hover().into()),
            border: Border {
                radius: theme::radius::SM.into(),
                ..Default::default()
            },
            ..Default::default()
        }),
    ]
    .spacing(10)
    .into()
}

/// An on/off switch: a pill track with a sliding knob.
fn switch<'a>(on: bool, toggle: Message, pal: Palette) -> Element<'a, Message> {
    let knob = container(Space::new().width(16.0).height(16.0)).style(move |_| container::Style {
        background: Some(Color::WHITE.into()),
        border: Border {
            radius: theme::radius::PILL.into(),
            ..Default::default()
        },
        shadow: iced::Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.25),
            offset: iced::Vector::new(0.0, 1.0),
            blur_radius: 2.0,
        },
        ..Default::default()
    });
    let track = if on {
        irow![Space::new().width(Length::Fill), knob]
    } else {
        irow![knob, Space::new().width(Length::Fill)]
    };
    button(container(track).width(Length::Fixed(36.0)).padding(2))
        .padding(0)
        .on_press(toggle)
        .style(move |_, status| {
            let off = pal.ink(if matches!(status, button::Status::Hovered) {
                0.24
            } else {
                0.18
            });
            button::Style {
                background: Some(if on { pal.accent } else { off }.into()),
                border: Border {
                    radius: theme::radius::PILL.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .into()
}

fn small_button<'a>(label: &'a str, press: Message, pal: Palette) -> Element<'a, Message> {
    button(
        container(text(label).size(15).color(pal.fg))
            .width(Length::Fixed(26.0))
            .align_x(iced::alignment::Horizontal::Center),
    )
    .padding(Padding::from([2, 0]))
    .on_press(press)
    .style(move |_, status| outline_style(status, true, pal))
    .into()
}

fn outline_button<'a>(
    label: &'a str,
    press: Option<Message>,
    pal: Palette,
) -> Element<'a, Message> {
    let enabled = press.is_some();
    button(text(label).size(12.5))
        .padding(Padding::from([6, 12]))
        .on_press_maybe(press)
        .style(move |_, status| outline_style(status, enabled, pal))
        .into()
}

fn text_button<'a>(label: &'a str, press: Message, pal: Palette) -> Element<'a, Message> {
    button(text(label).size(12.5))
        .padding(Padding::from([6, 8]))
        .on_press(press)
        .style(move |_, status| button::Style {
            background: None,
            text_color: if matches!(status, button::Status::Hovered) {
                pal.fg
            } else {
                pal.muted
            },
            ..Default::default()
        })
        .into()
}

fn outline_style(status: button::Status, enabled: bool, pal: Palette) -> button::Style {
    let background = match status {
        _ if !enabled => None,
        button::Status::Hovered => Some(pal.hover().into()),
        button::Status::Pressed => Some(pal.active().into()),
        _ => None,
    };
    button::Style {
        background,
        text_color: if enabled { pal.fg } else { pal.subtle },
        border: Border {
            color: pal.border_strong(),
            width: 1.0,
            radius: theme::radius::SM.into(),
        },
        ..Default::default()
    }
}

fn slider_style(status: iced::widget::slider::Status, pal: Palette) -> iced::widget::slider::Style {
    use iced::widget::slider;
    let handle = match status {
        slider::Status::Hovered | slider::Status::Dragged => pal.fg,
        slider::Status::Active => Color::WHITE,
    };
    slider::Style {
        rail: slider::Rail {
            backgrounds: (pal.accent.into(), pal.ink(0.18).into()),
            width: 4.0,
            border: Border {
                radius: theme::radius::PILL.into(),
                ..Default::default()
            },
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle { radius: 8.0 },
            background: handle.into(),
            border_width: 1.0,
            border_color: pal.border_strong(),
        },
    }
}

/// `path` with the home directory written as `~`.
fn tilde_path(path: &Path) -> String {
    match dirs::home_dir().and_then(|home| path.strip_prefix(home).ok().map(Path::to_path_buf)) {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}
