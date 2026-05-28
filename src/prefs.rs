use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const FILE_NAME: &str = "prefs.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prefs {
    #[serde(default)]
    pub auto_focus_on_nav: bool,
    #[serde(default = "default_true")]
    pub show_footer: bool,
}

fn default_true() -> bool {
    true
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            auto_focus_on_nav: false,
            show_footer: true,
        }
    }
}

fn store_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("mdv").join(FILE_NAME))
}

pub fn load() -> Prefs {
    let Some(p) = store_path() else {
        return Prefs::default();
    };
    std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(prefs: &Prefs) {
    let Some(p) = store_path() else {
        return;
    };
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(prefs) {
        let _ = std::fs::write(&p, json);
    }
}
