use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const FILE_NAME: &str = "prefs.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prefs {
    #[serde(default)]
    pub auto_focus_on_nav: bool,
    #[serde(default = "default_true")]
    pub show_footer: bool,
    /// Workspace-scoped Quick Slot banks live alongside the existing user
    /// preferences, never inside a workspace tree.
    #[serde(default)]
    pub quick_slots: crate::quick_slots::QuickSlotsStore,
}

fn default_true() -> bool {
    true
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            auto_focus_on_nav: false,
            show_footer: true,
            quick_slots: crate::quick_slots::QuickSlotsStore::default(),
        }
    }
}

fn store_path() -> Option<PathBuf> {
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
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(&prefs) {
        let _ = std::fs::write(path, json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
