//! Crash-safe replacement of small config files.
//!
//! `std::fs::write` truncates the destination before writing, so a crash or a
//! full disk mid-write leaves a half-written JSON file that the next launch
//! silently replaces with defaults. Writing a sibling temp file and renaming it
//! over the destination keeps either the old or the new contents on disk.

use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// Atomically replace `path` with `contents`, creating the parent directory.
pub fn write_atomic(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    // Unique per process and per call, so concurrent instances and threads
    // never share a temp file.
    let tmp = parent.join(format!(
        ".{name}.tmp-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));

    let result = (|| {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(contents)?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::write_atomic;

    fn scratch_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("rmdv-fs-atomic-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn replaces_existing_contents_and_leaves_no_temp_file() {
        let dir = scratch_dir("replace");
        let path = dir.join("prefs.json");
        write_atomic(&path, b"old").unwrap();
        write_atomic(&path, b"new").unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        let entries: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
        assert_eq!(entries.len(), 1, "temp files must not be left behind");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn failed_write_keeps_previous_contents() {
        let dir = scratch_dir("fail");
        let path = dir.join("prefs.json");
        write_atomic(&path, b"kept").unwrap();
        // Renaming a file over a non-empty directory fails, which stands in
        // for any failure after the temp file was written.
        let blocked = dir.join("blocked");
        std::fs::create_dir_all(blocked.join("child")).unwrap();

        assert!(write_atomic(&blocked, b"lost").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"kept");
        let leftovers = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
            .count();
        assert_eq!(leftovers, 0, "a failed write must clean up its temp file");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
