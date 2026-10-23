use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const FILE_NAME: &str = "prefs.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prefs {
    #[serde(default)]
    pub auto_focus_on_nav: bool,
    #[serde(default = "default_true")]
    pub show_footer: bool,
    /// Slug of the last chosen theme (a built-in preset or a custom theme).
    /// `None` follows the system appearance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Soft syntax colors: desaturated hues on top of any theme.
    #[serde(default)]
    pub soft_syntax: bool,
    /// Window glass (macOS): which panels let the desktop blur show through.
    #[serde(default, deserialize_with = "lenient_glass")]
    pub glass: crate::macos_vibrancy::Glass,
    /// Theme tint over the glass, kept within `macos_vibrancy::OPACITY_LEVELS`.
    #[serde(default = "default_glass_opacity")]
    pub glass_opacity: f32,
    /// Reader font-zoom factor (⌘+ / ⌘−), restored on launch.
    #[serde(
        default = "default_font_scale",
        deserialize_with = "lenient_font_scale"
    )]
    pub font_scale: f32,
    /// Body line spacing (Settings → Line spacing), a multiplier on the
    /// theme's line height.
    #[serde(default, deserialize_with = "lenient_line_spacing")]
    pub line_spacing: crate::theme::LineSpacing,
    /// Show dot files and folders in the sidebar (⌘⇧.).
    #[serde(default)]
    pub show_hidden: bool,
    /// Document Mindmap keeps the selected node in view while it moves.
    #[serde(default = "default_true")]
    pub mindmap_autocenter: bool,
    /// Two-finger scroll and the mouse wheel pan the mind map instead of
    /// zooming it (Settings → Scroll pans the mind map).
    #[serde(default, deserialize_with = "lenient_off")]
    pub mindmap_scroll_pans: bool,
    /// Workspace-scoped Quick Slot banks live alongside the existing user
    /// preferences, never inside a workspace tree.
    #[serde(default, deserialize_with = "lenient_quick_slots")]
    pub quick_slots: crate::quick_slots::QuickSlotsStore,
    /// When the top toolbar shows (Settings → Toolbar).
    #[serde(default, deserialize_with = "lenient_toolbar")]
    pub toolbar: ToolbarMode,
}

/// When the top toolbar shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolbarMode {
    /// Hidden until the pointer reaches the top edge of the window.
    OnHover,
    /// Always shown; the content sits below it. The default, so a new user
    /// sees the toolbar.
    #[default]
    Always,
}

impl ToolbarMode {
    /// The other mode.
    pub fn toggled(self) -> Self {
        match self {
            ToolbarMode::OnHover => ToolbarMode::Always,
            ToolbarMode::Always => ToolbarMode::OnHover,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_glass_opacity() -> f32 {
    crate::macos_vibrancy::DEFAULT_OPACITY
}

/// Font-zoom limits, shared with the app's ⌘+ / ⌘− handling.
pub const FONT_SCALE_MIN: f32 = 0.6;
pub const FONT_SCALE_MAX: f32 = 2.2;

fn default_font_scale() -> f32 {
    1.0
}

/// A missing, malformed, or out-of-range font scale falls back into range
/// instead of failing the whole file.
fn lenient_font_scale<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(clamp_font_scale(
        value.as_f64().map_or(default_font_scale(), |v| v as f32),
    ))
}

/// Keeps a font scale finite and inside the supported range.
pub fn clamp_font_scale(scale: f32) -> f32 {
    if scale.is_finite() {
        scale.clamp(FONT_SCALE_MIN, FONT_SCALE_MAX)
    } else {
        default_font_scale()
    }
}

/// An unknown or malformed line spacing falls back to Default instead of
/// failing the whole file.
fn lenient_line_spacing<'de, D>(deserializer: D) -> Result<crate::theme::LineSpacing, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

/// An unknown glass mode (e.g. from a newer build) turns glass off instead of
/// failing the whole file.
fn lenient_glass<'de, D>(deserializer: D) -> Result<crate::macos_vibrancy::Glass, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

/// An unknown toolbar mode falls back to the default (always shown) instead
/// of failing the whole file.
fn lenient_toolbar<'de, D>(deserializer: D) -> Result<ToolbarMode, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

/// A malformed switch that defaults off stays off instead of failing the
/// whole file.
fn lenient_off<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(value.as_bool().unwrap_or(false))
}

/// A malformed or newer-format Quick Slot store (e.g. after a downgrade) falls
/// back to empty slots instead of failing the whole file and resetting every
/// other preference.
fn lenient_quick_slots<'de, D>(
    deserializer: D,
) -> Result<crate::quick_slots::QuickSlotsStore, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            auto_focus_on_nav: false,
            show_footer: true,
            theme: None,
            soft_syntax: false,
            glass: crate::macos_vibrancy::Glass::Window,
            glass_opacity: crate::macos_vibrancy::DEFAULT_OPACITY,
            font_scale: default_font_scale(),
            line_spacing: crate::theme::LineSpacing::Default,
            show_hidden: false,
            mindmap_autocenter: true,
            mindmap_scroll_pans: false,
            quick_slots: crate::quick_slots::QuickSlotsStore::default(),
            toolbar: ToolbarMode::Always,
        }
    }
}

pub fn store_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("rmdv").join(FILE_NAME))
}

#[cfg(test)]
pub(crate) fn production_store_path_for_tests() -> Option<PathBuf> {
    store_path()
}

pub fn load() -> Prefs {
    let Some(p) = store_path() else {
        return Prefs::default();
    };
    load_from(&p)
}

pub fn save(prefs: &Prefs) {
    let Some(p) = store_path() else {
        return;
    };
    save_to(&p, prefs);
}

/// Isolated config boundary used by tests and migration tools. Runtime calls
/// continue to use [`load`] and [`save`], while tests can point at a temporary
/// file and prove that no real user config is touched.
pub fn load_from(path: &std::path::Path) -> Prefs {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<Prefs>(&s).ok())
        .map(|mut prefs| {
            prefs.quick_slots = prefs.quick_slots.normalized();
            prefs
        })
        .unwrap_or_default()
}

pub fn save_to(path: &std::path::Path, prefs: &Prefs) {
    let prefs = {
        let mut prefs = prefs.clone();
        prefs.quick_slots = prefs.quick_slots.normalized();
        prefs
    };
    let result = serde_json::to_string_pretty(&prefs)
        .map_err(std::io::Error::other)
        .and_then(|json| crate::fs_atomic::write_atomic(path, json.as_bytes()));
    if let Err(error) = result {
        eprintln!("rmdv: could not save {}: {error}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_quick_slots_keep_other_preferences() {
        let prefs: Prefs = serde_json::from_str(
            r#"{"auto_focus_on_nav":true,"show_footer":false,"quick_slots":42}"#,
        )
        .expect("other preferences must still load");
        assert!(prefs.auto_focus_on_nav);
        assert!(!prefs.show_footer);
    }

    #[test]
    fn glass_preferences_default_to_window_at_60_percent_and_tolerate_unknown_modes() {
        use crate::macos_vibrancy::Glass;
        // A fresh install (or a prefs file from before glass existed) gets the
        // whole-window glass at the lightest tint.
        let prefs: Prefs = serde_json::from_str("{}").unwrap();
        assert_eq!(prefs.glass, Glass::Window);
        assert_eq!(prefs.glass_opacity, 0.6);
        assert_eq!(Prefs::default().glass, Glass::Window);
        assert_eq!(Prefs::default().glass_opacity, 0.6);
        // A saved "off" stays off.
        let prefs: Prefs = serde_json::from_str(r#"{"glass":"off"}"#).unwrap();
        assert_eq!(prefs.glass, Glass::Off);
        let prefs: Prefs =
            serde_json::from_str(r#"{"glass":"window","glass_opacity":0.6}"#).unwrap();
        assert_eq!(prefs.glass, Glass::Window);
        assert_eq!(prefs.glass_opacity, 0.6);
        let prefs: Prefs = serde_json::from_str(r#"{"glass":"frosted","show_footer":false}"#)
            .expect("an unknown glass mode must not reset other preferences");
        assert_eq!(prefs.glass, Glass::Window);
        assert!(!prefs.show_footer);
    }

    #[test]
    fn reading_preferences_default_and_survive_bad_values() {
        let prefs: Prefs = serde_json::from_str(r#"{"show_footer":false}"#).unwrap();
        assert_eq!(prefs.font_scale, 1.0);
        assert!(!prefs.show_hidden);
        assert!(prefs.mindmap_autocenter);
        let prefs: Prefs = serde_json::from_str(
            r#"{"font_scale":1.21,"show_hidden":true,"mindmap_autocenter":false}"#,
        )
        .unwrap();
        assert_eq!(prefs.font_scale, 1.21);
        assert!(prefs.show_hidden);
        assert!(!prefs.mindmap_autocenter);
        let prefs: Prefs =
            serde_json::from_str(r#"{"font_scale":"big","show_footer":false}"#).unwrap();
        assert_eq!(prefs.font_scale, 1.0);
        assert!(!prefs.show_footer);
        let prefs: Prefs = serde_json::from_str(r#"{"font_scale":9.0}"#).unwrap();
        assert_eq!(prefs.font_scale, FONT_SCALE_MAX);
    }

    #[test]
    fn line_spacing_defaults_when_missing_or_unknown() {
        use crate::theme::LineSpacing;
        // A prefs file from before the setting existed.
        let prefs: Prefs = serde_json::from_str(r#"{"show_footer":false}"#).unwrap();
        assert_eq!(prefs.line_spacing, LineSpacing::Default);
        assert!(!prefs.show_footer);
        assert_eq!(Prefs::default().line_spacing, LineSpacing::Default);
        let prefs: Prefs = serde_json::from_str(r#"{"line_spacing":"relaxed"}"#).unwrap();
        assert_eq!(prefs.line_spacing, LineSpacing::Relaxed);
        let prefs: Prefs = serde_json::from_str(r#"{"line_spacing":"compact"}"#).unwrap();
        assert_eq!(prefs.line_spacing, LineSpacing::Compact);
        // An unknown or malformed value must not reset other preferences.
        let prefs: Prefs =
            serde_json::from_str(r#"{"line_spacing":"airy","show_footer":false}"#).unwrap();
        assert_eq!(prefs.line_spacing, LineSpacing::Default);
        assert!(!prefs.show_footer);
        let prefs: Prefs = serde_json::from_str(r#"{"line_spacing":3}"#).unwrap();
        assert_eq!(prefs.line_spacing, LineSpacing::Default);
        // Saved as a lowercase word.
        let json = serde_json::to_string(&Prefs {
            line_spacing: LineSpacing::Compact,
            ..Prefs::default()
        })
        .unwrap();
        assert!(json.contains(r#""line_spacing":"compact""#), "{json}");
    }

    #[test]
    fn toolbar_mode_defaults_to_always_and_survives_bad_values() {
        // A prefs file from before the setting existed: a new user sees the
        // toolbar.
        let prefs: Prefs = serde_json::from_str(r#"{"show_footer":false}"#).unwrap();
        assert_eq!(prefs.toolbar, ToolbarMode::Always);
        assert_eq!(Prefs::default().toolbar, ToolbarMode::Always);
        assert_eq!(ToolbarMode::default(), ToolbarMode::Always);
        // A saved choice still loads as saved.
        let prefs: Prefs = serde_json::from_str(r#"{"toolbar":"on_hover"}"#).unwrap();
        assert_eq!(prefs.toolbar, ToolbarMode::OnHover);
        let prefs: Prefs = serde_json::from_str(r#"{"toolbar":"always"}"#).unwrap();
        assert_eq!(prefs.toolbar, ToolbarMode::Always);
        // An unknown or malformed value must not reset other preferences.
        let prefs: Prefs =
            serde_json::from_str(r#"{"toolbar":"floating","show_footer":false}"#).unwrap();
        assert_eq!(prefs.toolbar, ToolbarMode::Always);
        assert!(!prefs.show_footer);
        let prefs: Prefs = serde_json::from_str(r#"{"toolbar":7,"show_footer":false}"#).unwrap();
        assert_eq!(prefs.toolbar, ToolbarMode::Always);
        assert!(!prefs.show_footer);
        let json = serde_json::to_string(&Prefs {
            toolbar: ToolbarMode::OnHover,
            ..Prefs::default()
        })
        .unwrap();
        assert!(json.contains(r#""toolbar":"on_hover""#), "{json}");
    }

    #[test]
    fn mindmap_scroll_pans_defaults_off_and_survives_bad_values() {
        // A prefs file from before the setting existed keeps today's zoom.
        let prefs: Prefs = serde_json::from_str(r#"{"show_footer":false}"#).unwrap();
        assert!(!prefs.mindmap_scroll_pans);
        assert!(!Prefs::default().mindmap_scroll_pans);
        let prefs: Prefs = serde_json::from_str(r#"{"mindmap_scroll_pans":true}"#).unwrap();
        assert!(prefs.mindmap_scroll_pans);
        // A malformed value must not reset other preferences.
        for bad in [r#""yes""#, "1", "null", "{}"] {
            let json = format!(r#"{{"mindmap_scroll_pans":{bad},"show_footer":false}}"#);
            let prefs: Prefs = serde_json::from_str(&json).unwrap_or_else(|e| panic!("{bad}: {e}"));
            assert!(!prefs.mindmap_scroll_pans, "{bad}");
            assert!(!prefs.show_footer, "{bad}");
        }
        let json = serde_json::to_string(&Prefs {
            mindmap_scroll_pans: true,
            ..Prefs::default()
        })
        .unwrap();
        assert!(json.contains(r#""mindmap_scroll_pans":true"#), "{json}");
    }

    use crate::quick_slots::{QuickSlot, SlotContext, WorkspaceSlots};
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn isolated_config_round_trip_never_uses_real_store() {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let path = std::env::temp_dir().join(format!(
            "rmdv-prefs-test-{}.json",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut prefs = Prefs::default();
        let mut bank = WorkspaceSlots::default();
        bank.set(
            0,
            QuickSlot {
                relative_path: "notes/today.md".into(),
                context: SlotContext::default(),
            },
        );
        prefs
            .quick_slots
            .put_bank(std::path::Path::new("/workspace"), bank);
        save_to(&path, &prefs);
        let loaded = load_from(&path);
        assert!(loaded
            .quick_slots
            .bank(std::path::Path::new("/workspace"))
            .occupied(0)
            .is_some());
        let _ = std::fs::remove_file(path);
    }
}
