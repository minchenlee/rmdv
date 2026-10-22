//! Recently opened files and folders, newest first, for the welcome page.
//!
//! The store is `recent.json` in the config folder. `paths` holds files, as
//! it always has; `folders` was added later and defaults to empty, so an
//! older file still loads and an older rmdv ignores the new field.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "recent.json";
/// Entries kept per list.
pub const MAX: usize = 5;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recent {
    /// Recent files.
    pub paths: Vec<PathBuf>,
    /// Recent folders opened as the workspace.
    #[serde(default)]
    pub folders: Vec<PathBuf>,
}

impl Recent {
    /// Only the entries that still exist; the welcome page skips the rest.
    pub fn existing(&self) -> Recent {
        Recent {
            paths: self.paths.iter().filter(|p| p.is_file()).cloned().collect(),
            folders: self
                .folders
                .iter()
                .filter(|p| p.is_dir())
                .cloned()
                .collect(),
        }
    }
}

pub fn store_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("rmdv").join(FILE_NAME))
}

pub fn load() -> Recent {
    store_path().map(|p| load_from(&p)).unwrap_or_default()
}

/// The list stored at `store`; empty when it is missing or unreadable.
pub fn load_from(store: &Path) -> Recent {
    std::fs::read_to_string(store)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Record a file opened now in the list at `store`; returns the new list.
pub fn add_file_to(store: &Path, path: &Path) -> Recent {
    let mut recent = load_from(store);
    push_front(&mut recent.paths, path);
    save_to(store, &recent);
    recent
}

/// Record a folder opened now as the workspace in the list at `store`;
/// returns the new list.
pub fn add_folder_to(store: &Path, path: &Path) -> Recent {
    let mut recent = load_from(store);
    push_front(&mut recent.folders, path);
    save_to(store, &recent);
    recent
}

fn push_front(list: &mut Vec<PathBuf>, path: &Path) {
    list.retain(|x| x != path);
    list.insert(0, path.to_path_buf());
    list.truncate(MAX);
}

fn save_to(store: &Path, recent: &Recent) {
    let result = serde_json::to_string_pretty(recent)
        .map_err(std::io::Error::other)
        .and_then(|json| crate::fs_atomic::write_atomic(store, json.as_bytes()));
    if let Err(error) = result {
        eprintln!("rmdv: could not save {}: {error}", store.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(tag: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("rmdv-recent-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = dir.join("recent.json");
        (dir, store)
    }

    #[test]
    fn older_store_without_folders_still_loads() {
        let (dir, store) = fixture("old");
        std::fs::write(&store, r#"{ "paths": ["/a.md", "/b.md"] }"#).unwrap();
        let recent = load_from(&store);
        assert_eq!(
            recent.paths,
            vec![PathBuf::from("/a.md"), PathBuf::from("/b.md")]
        );
        assert!(recent.folders.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn older_reader_ignores_the_folders_field() {
        #[derive(Deserialize)]
        struct OldRecent {
            paths: Vec<PathBuf>,
        }
        let (dir, store) = fixture("compat");
        let _ = add_file_to(&store, Path::new("/a.md"));
        let _ = add_folder_to(&store, Path::new("/notes"));
        let json = std::fs::read_to_string(&store).unwrap();
        let old: OldRecent = serde_json::from_str(&json).expect("old format still parses");
        assert_eq!(old.paths, vec![PathBuf::from("/a.md")]);
        assert!(json.contains("folders"), "{json}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn folders_and_files_are_kept_newest_first_up_to_the_cap() {
        let (dir, store) = fixture("cap");
        for i in 0..7 {
            let _ = add_folder_to(&store, &PathBuf::from(format!("/f{i}")));
        }
        let recent = add_folder_to(&store, Path::new("/f3"));
        assert_eq!(recent.folders.len(), MAX);
        assert_eq!(recent.folders[0], PathBuf::from("/f3"));
        assert_eq!(recent.folders[1], PathBuf::from("/f6"));
        assert!(
            recent.paths.is_empty(),
            "folders do not enter the file list"
        );
        assert_eq!(load_from(&store), recent, "the list is saved");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_entries_are_skipped() {
        let (dir, _) = fixture("missing");
        let file = dir.join("here.md");
        std::fs::write(&file, "# Here\n").unwrap();
        let recent = Recent {
            paths: vec![dir.join("gone.md"), file.clone()],
            folders: vec![dir.join("gone"), dir.clone()],
        };
        let existing = recent.existing();
        assert_eq!(existing.paths, vec![file]);
        assert_eq!(existing.folders, vec![dir.clone()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
