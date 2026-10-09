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
    /// Show dot files and folders in the sidebar (⌘⇧.).
    #[serde(default)]
    pub show_hidden: bool,
    /// Document Mindmap keeps the selected node in view while it moves.
    #[serde(default = "default_true")]
    pub mindmap_autocenter: bool,
    /// Workspace-scoped Quick Slot banks live alongside the existing user
    /// preferences, never inside a workspace tree.
    #[serde(default, deserialize_with = "lenient_quick_slots")]
    pub quick_slots: crate::quick_slots::QuickSlotsStore,
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

/// An unknown glass mode (e.g. from a newer build) turns glass off instead of
/// failing the whole file.
fn lenient_glass<'de, D>(deserializer: D) -> Result<crate::macos_vibrancy::Glass, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
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
            show_hidden: false,
            mindmap_autocenter: true,
            quick_slots: crate::quick_slots::QuickSlotsStore::default(),
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
