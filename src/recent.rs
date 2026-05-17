use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "recent.json";
const MAX: usize = 5;

#[derive(Default, Serialize, Deserialize)]
pub struct Recent {
    pub paths: Vec<PathBuf>,
}

fn store_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("mdv").join(FILE_NAME))
}

pub fn load() -> Recent {
    let Some(p) = store_path() else {
        return Recent::default();
    };
    std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn add(path: &Path) {
    let Some(p) = store_path() else {
        return;
    };
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut r = load();
    r.paths.retain(|x| x != path);
    r.paths.insert(0, path.to_path_buf());
    r.paths.truncate(MAX);
    if let Ok(json) = serde_json::to_string_pretty(&r) {
        let _ = std::fs::write(&p, json);
    }
}
