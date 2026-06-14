//! One-time migration of user config from the legacy `mdv` directory to `rmdv`.
//!
//! The project was renamed mdv → rmdv, which moved the config dir from
//! `<config>/mdv` to `<config>/rmdv` (prefs.json, recent.json, themes/*.toml).
//! Without migration, upgrading users would silently lose all of it. On first
//! run we copy the legacy tree into the new location if — and only if — the new
//! location does not exist yet, so we never clobber newer state.

use std::path::PathBuf;
use std::sync::Once;

static MIGRATE_ONCE: Once = Once::new();

fn legacy_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("mdv"))
}

fn new_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("rmdv"))
}

/// Run the legacy → new copy exactly once per process. Idempotent and safe to
/// call before every config read; the `Once` guard makes repeat calls free.
pub fn run() {
    MIGRATE_ONCE.call_once(|| {
        let (Some(old), Some(new)) = (legacy_dir(), new_dir()) else {
            return;
        };
        // Only migrate when the legacy dir exists and the new one does not.
        if !old.is_dir() || new.exists() {
            return;
        }
        // Copy into a sibling staging dir first, then atomically rename it into
        // place. A crash/error mid-copy leaves only the staging dir (which we
        // remove), so `new` never appears half-populated — the next launch
        // retries from scratch instead of treating a partial copy as migrated.
        let staging = new.with_file_name(format!("rmdv.migrating-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&staging); // clear any stale staging
        let result = copy_dir_all(&old, &staging).and_then(|()| {
            // Re-check the race: if another instance finished first, bail.
            if new.exists() {
                Ok(())
            } else {
                std::fs::rename(&staging, &new)
            }
        });
        if let Err(e) = result {
            let _ = std::fs::remove_dir_all(&staging);
            eprintln!(
                "rmdv: could not migrate config {} -> {}: {e}",
                old.display(),
                new.display()
            );
        }
    });
}

/// Recursively copy `src` into `dst`, creating `dst` and any subdirs.
fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::copy_dir_all;

    #[test]
    fn copies_nested_tree() {
        let base =
            std::env::temp_dir().join(format!("rmdv-migrate-test-{}", std::process::id()));
        let src = base.join("mdv");
        let dst = base.join("rmdv");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(src.join("themes")).unwrap();
        std::fs::write(src.join("prefs.json"), b"{}").unwrap();
        std::fs::write(src.join("themes").join("a.toml"), b"name='a'").unwrap();

        copy_dir_all(&src, &dst).unwrap();

        assert!(dst.join("prefs.json").is_file());
        assert!(dst.join("themes").join("a.toml").is_file());
        let _ = std::fs::remove_dir_all(&base);
    }
}
