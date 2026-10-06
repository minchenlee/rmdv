//! Settings page state: the row model, keyboard cursor, row actions, and the
//! relaunch used when a launch-time setting changes.

use super::*;
use std::ffi::OsString;

/// One row of the Settings page, top to bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsRow {
    Theme,
    SoftSyntax,
    Glass,
    GlassOpacity,
    FontSize,
    Footer,
    HiddenFiles,
    MindmapAutocenter,
    AutoFocus,
    Cli,
    ThemesFolder,
    PrefsFile,
}

impl SettingsRow {
    /// The rows this platform shows, in page order. Glass rows are macOS only;
    /// the CLI row needs a platform with a managed CLI path.
    pub fn visible() -> Vec<SettingsRow> {
        use SettingsRow::*;
        let mut rows = vec![Theme, SoftSyntax];
        if cfg!(target_os = "macos") {
            rows.extend([Glass, GlassOpacity]);
        }
        rows.extend([FontSize, Footer, HiddenFiles, MindmapAutocenter, AutoFocus]);
        if crate::cli_install::install_path().is_some() {
            rows.push(Cli);
        }
        rows.extend([ThemesFolder, PrefsFile]);
        rows
    }
}

/// Glass opacity slider step (5 %).
pub const GLASS_OPACITY_STEP: f32 = 0.05;

const GLASS_MODES: [Glass; 3] = [Glass::Off, Glass::Sidebar, Glass::Window];

impl App {
    pub(super) fn settings_scroll_id() -> iced::widget::Id {
        iced::widget::Id::new("settings")
    }

    pub(super) fn settings_row_id(index: usize) -> iced::widget::Id {
        iced::widget::Id::from(format!("settings-row-{index}"))
    }

    /// The row under the keyboard cursor.
    pub(super) fn settings_row(&self) -> SettingsRow {
        let rows = SettingsRow::visible();
        rows[self.settings_cursor.min(rows.len() - 1)]
    }

    /// Every theme the Theme row offers: presets, then bundled and custom.
    pub(super) fn theme_entries(&self) -> Vec<ThemeEntry> {
        ThemePreset::ALL
            .into_iter()
            .map(ThemeEntry::Preset)
            .chain(
                self.custom_themes
                    .iter()
                    .map(|t| ThemeEntry::Custom(t.slug.clone(), t.name.clone(), t.palette)),
            )
            .collect()
    }

    /// True when the saved glass mode needs a transparent window that this
    /// launch did not create.
    pub(super) fn glass_needs_restart(&self) -> bool {
        self.prefs.glass != Glass::Off && !self.glass_capable
    }

    pub(super) fn open_settings(&mut self) -> Task<Message> {
        self.overlay = Overlay::None;
        self.picker = None;
        self.settings_open = true;
        self.settings_cursor = 0;
        iced::widget::operation::snap_to(
            Self::settings_scroll_id(),
            iced::widget::scrollable::RelativeOffset::START,
        )
    }

    pub(super) fn settings_move(&mut self, delta: i32) -> Task<Message> {
        let last = SettingsRow::visible().len() as i64 - 1;
        self.settings_cursor =
            (self.settings_cursor as i64 + i64::from(delta)).clamp(0, last) as usize;
        scroll_anchor_into_view(
            Self::settings_scroll_id(),
            Self::settings_row_id(self.settings_cursor),
            Message::SettingsScrollTo,
        )
    }

    /// Space / Enter: toggle a switch or press the row's button.
    pub(super) fn settings_activate(&mut self) -> Task<Message> {
        let message = match self.settings_row() {
            SettingsRow::Theme | SettingsRow::GlassOpacity => return Task::none(),
            SettingsRow::SoftSyntax => Message::ToggleSoftSyntax,
            SettingsRow::Glass => Message::SetGlass(self.prefs.glass.next()),
            SettingsRow::FontSize => Message::FontSizeReset,
            SettingsRow::Footer => Message::ToggleFooter,
            SettingsRow::HiddenFiles => Message::ToggleHidden,
            SettingsRow::MindmapAutocenter => Message::ToggleMindmapAutocenter,
            SettingsRow::AutoFocus => Message::ToggleAutoFocusOnNav,
            SettingsRow::Cli if crate::cli_install::should_offer() => Message::InstallCli,
            SettingsRow::Cli => return Task::none(),
            SettingsRow::ThemesFolder => Message::OpenThemesDir,
            SettingsRow::PrefsFile => Message::RevealPrefsFile,
        };
        self.update(message)
    }

    /// ← / →: step a theme, glass mode, opacity, or font size; set a switch
    /// off (←) or on (→).
    pub(super) fn settings_step(&mut self, dir: i32) -> Task<Message> {
        let forward = dir > 0;
        let switch = |on: bool, toggle: Message| (on != forward).then_some(toggle);
        let message = match self.settings_row() {
            SettingsRow::Theme => {
                let entries = self.theme_entries();
                let current = entries
                    .iter()
                    .position(|e| e.matches_current(&self.theme_id))
                    .unwrap_or(0);
                let next = (current as i64 + i64::from(dir.signum()))
                    .clamp(0, entries.len() as i64 - 1) as usize;
                (next != current).then(|| entries[next].message())
            }
            SettingsRow::Glass => {
                let current = GLASS_MODES
                    .iter()
                    .position(|g| *g == self.prefs.glass)
                    .unwrap_or(0);
                let next = (current as i64 + i64::from(dir.signum())).clamp(0, 2) as usize;
                (next != current).then_some(Message::SetGlass(GLASS_MODES[next]))
            }
            SettingsRow::GlassOpacity if self.prefs.glass != Glass::Off => {
                Some(Message::SetGlassOpacity(
                    self.glass_opacity() + dir.signum() as f32 * GLASS_OPACITY_STEP,
                ))
            }
            SettingsRow::GlassOpacity => None,
            SettingsRow::FontSize if forward => Some(Message::FontSizeUp),
            SettingsRow::FontSize => Some(Message::FontSizeDown),
            SettingsRow::SoftSyntax => switch(self.prefs.soft_syntax, Message::ToggleSoftSyntax),
            SettingsRow::Footer => switch(self.show_footer, Message::ToggleFooter),
            SettingsRow::HiddenFiles => switch(self.show_hidden, Message::ToggleHidden),
            SettingsRow::MindmapAutocenter => {
                switch(self.mindmap_autocenter, Message::ToggleMindmapAutocenter)
            }
            SettingsRow::AutoFocus => {
                switch(self.prefs.auto_focus_on_nav, Message::ToggleAutoFocusOnNav)
            }
            SettingsRow::Cli | SettingsRow::ThemesFolder | SettingsRow::PrefsFile => None,
        };
        message.map_or_else(Task::none, |m| self.update(m))
    }

    pub(super) fn set_glass(&mut self, glass: Glass) -> Task<Message> {
        if self.prefs.glass == glass {
            return Task::none();
        }
        self.prefs.glass = glass;
        self.save_prefs();
        let restart = if self.glass_needs_restart() {
            " — restart rmdv to apply"
        } else {
            ""
        };
        self.show_toast(format!("Window glass: {}{restart}", glass.label()))
    }

    pub(super) fn set_glass_opacity(&mut self, opacity: f32) -> Task<Message> {
        // Snap to the slider step so stored values stay on round percents.
        let snapped = (opacity / GLASS_OPACITY_STEP).round() * GLASS_OPACITY_STEP;
        let opacity = crate::macos_vibrancy::clamp_opacity(snapped);
        if (opacity - self.prefs.glass_opacity).abs() < 0.001 {
            return Task::none();
        }
        self.prefs.glass_opacity = opacity;
        self.save_prefs();
        Task::none()
    }

    /// Relaunch rmdv with the current file (or folder), then close this window.
    pub(super) fn restart_app(&mut self) -> Task<Message> {
        if self.dirty {
            return self.show_toast(self.unsaved_edits_open_message());
        }
        let exe = match std::env::current_exe() {
            Ok(exe) => exe,
            Err(error) => return self.show_toast(format!("Couldn't restart rmdv: {error}")),
        };
        let reopen = self.file.clone().or_else(|| self.workspace.clone());
        let argv = relaunch_argv(&exe, reopen.as_deref());
        // A detached shell waits for this process to exit, so the new instance
        // does not find this one still holding the IPC socket.
        let spawned = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg("while kill -0 \"$0\" 2>/dev/null; do sleep 0.1; done; exec \"$@\"")
            .arg(std::process::id().to_string())
            .args(&argv)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        if let Err(error) = spawned {
            return self.show_toast(format!("Couldn't restart rmdv: {error}"));
        }
        let checkpoint = self.checkpoint_active_quick_slot();
        self.persist_quick_slots_now();
        Task::batch([
            checkpoint,
            iced::window::latest().and_then(iced::window::close),
        ])
    }

    pub(super) fn reveal_prefs_file(&mut self) -> Task<Message> {
        let path = self
            .quick_slots_persistence_path
            .clone()
            .or_else(crate::prefs::store_path);
        let Some(path) = path else {
            return self.show_toast("No preferences file on this system".to_string());
        };
        if !path.exists() {
            self.save_prefs();
        }
        match Self::reveal_file_in_finder(&path) {
            Ok(()) => Task::none(),
            Err(error) => self.show_toast(error),
        }
    }
}

impl Message {
    /// Messages that show another view (a file, folder, or mode). They close
    /// the Settings page first so their result is visible.
    pub(super) fn leaves_settings(&self) -> bool {
        matches!(
            self,
            Message::FileLoaded(Ok(_))
                | Message::OpenWorkspace(_)
                | Message::OpenVaultSearch
                | Message::ToggleFullMindmap
                | Message::ToggleMindmap
                | Message::ToggleViewMode
        )
    }
}

/// The command that starts rmdv again: the app bundle through `open` when this
/// executable lives in one, else the executable itself. `reopen` is the file or
/// folder to show.
pub(super) fn relaunch_argv(exe: &Path, reopen: Option<&Path>) -> Vec<OsString> {
    let bundle = exe
        .ancestors()
        .find(|p| p.extension().is_some_and(|ext| ext == "app"));
    let mut argv: Vec<OsString> = match bundle {
        Some(bundle) if cfg!(target_os = "macos") => {
            let mut argv = vec![OsString::from("open"), bundle.as_os_str().to_owned()];
            if reopen.is_some() {
                argv.push(OsString::from("--args"));
            }
            argv
        }
        _ => vec![exe.as_os_str().to_owned()],
    };
    argv.extend(reopen.map(|p| p.as_os_str().to_owned()));
    argv
}
