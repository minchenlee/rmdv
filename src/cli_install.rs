//! Install the `rmdv` shell entry point from inside a packaged app.
//!
//! The application bundle/AppImage already contains the CLI-capable `rmdv`
//! executable. This module only manages the user-facing symlink, following
//! the same model as Zed's in-app CLI installation command.

use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
const CLI_PATH: &str = "/usr/local/bin/rmdv";

/// Whether this process is running from a supported packaged app where an
/// in-app CLI installation is meaningful.
pub fn should_offer() -> bool {
    cli_target().is_some() && install_path().is_some()
}

/// The path that the installer manages for the current platform.
pub fn install_path() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        Some(PathBuf::from(CLI_PATH))
    }
    #[cfg(target_os = "linux")]
    {
        dirs::home_dir().map(|home| linux_install_path(&home))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Check whether the shell entry point resolves to this exact packaged app.
pub fn is_installed() -> bool {
    let Some(target) = cli_target() else {
        return false;
    };
    let Some(path) = install_path() else {
        return false;
    };
    std::fs::canonicalize(path)
        .map(|installed| installed == target)
        .unwrap_or(false)
}

/// Install the CLI symlink asynchronously.
pub async fn install() -> Result<(), String> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        tokio::task::spawn_blocking(install_blocking)
            .await
            .map_err(|e| format!("CLI installer stopped: {e}"))?
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Err("CLI installation is currently available on macOS and Linux only".to_string())
    }
}

#[cfg(target_os = "macos")]
fn cli_target() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let executable = std::fs::canonicalize(&executable).unwrap_or(executable);
    let path = executable.to_string_lossy();
    path.contains(".app/Contents/MacOS/").then_some(executable)
}

#[cfg(target_os = "linux")]
fn cli_target() -> Option<PathBuf> {
    // AppImage sets APPIMAGE to the stable outer image path. current_exe()
    // points into the temporary mounted image and would leave a broken link
    // after the AppImage exits.
    let app_image = std::env::var_os("APPIMAGE").map(PathBuf::from)?;
    if !app_image.is_file() {
        return None;
    }
    std::fs::canonicalize(app_image).ok()
}

#[cfg(target_os = "linux")]
fn linux_install_path(home: &Path) -> PathBuf {
    home.join(".local").join("bin").join("rmdv")
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn cli_target() -> Option<PathBuf> {
    None
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn install_blocking() -> Result<(), String> {
    let target = cli_target().ok_or_else(|| {
        "rmdv must be running from a packaged macOS app or Linux AppImage".to_string()
    })?;
    let path = install_path().ok_or_else(|| "no CLI install path is available".to_string())?;

    let direct_error = match install_link(&target, &path) {
        Ok(()) => return Ok(()),
        Err(error) => error,
    };

    #[cfg(target_os = "macos")]
    {
        // `/usr/local/bin` is commonly owned by root. Match Zed's behavior and
        // let macOS show its standard administrator authorization dialog rather
        // than asking the user to copy a shell command manually.
        let command = format!(
            "mkdir -p /usr/local/bin && ln -sfn {} {}",
            shell_quote(&target),
            shell_quote(&path),
        );
        let script = format!(
            "do shell script \"{}\" with administrator privileges",
            applescript_quote(&command)
        );
        let output = std::process::Command::new("osascript")
            .args(["-e", &script])
            .output()
            .map_err(|e| format!("could not start macOS authorization: {e}"))?;
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if detail.contains("User canceled") || detail.contains("-128") {
                return Err("installation cancelled".to_string());
            }
            return Err(if detail.is_empty() {
                format!("could not install rmdv CLI: {direct_error}")
            } else {
                detail
            });
        }
    }

    #[cfg(target_os = "linux")]
    {
        return Err(format!(
            "could not install rmdv CLI at {}: {direct_error}",
            path.display()
        ));
    }

    if is_installed() {
        Ok(())
    } else {
        Err(format!(
            "installation finished, but {} was not installed",
            path.display()
        ))
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn install_link(target: &Path, cli_path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::symlink;

    let parent = cli_path
        .parent()
        .expect("the CLI install path has a parent directory");
    std::fs::create_dir_all(parent)?;

    // Stage beside the destination, then rename atomically so a failed install
    // never leaves a half-written CLI path.
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let temporary = parent.join(format!(".rmdv-cli-{}-{nonce}.tmp", std::process::id()));
    let _ = std::fs::remove_file(&temporary);
    symlink(target, &temporary)?;
    if let Err(error) = std::fs::rename(&temporary, cli_path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

#[cfg(target_os = "macos")]
fn applescript_quote(command: &str) -> String {
    command.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    #[test]
    fn shell_quote_handles_apostrophes() {
        let quoted = super::shell_quote(std::path::Path::new("/Applications/Ada's.app"));
        assert_eq!(quoted, "'/Applications/Ada'\\''s.app'");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn applescript_quote_escapes_script_literals() {
        assert_eq!(
            super::applescript_quote("echo \\\"ok\\\""),
            "echo \\\\\\\"ok\\\\\\\""
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_install_path_is_user_scoped() {
        let home = std::path::Path::new("/home/example");
        assert_eq!(
            super::linux_install_path(home),
            home.join(".local").join("bin").join("rmdv")
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_install_link_points_to_the_appimage() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("rmdv-cli-install-test-{nonce}"));
        std::fs::create_dir_all(&root).expect("create test directory");
        let target = root.join("rmdv.AppImage");
        std::fs::write(&target, b"test AppImage").expect("create test AppImage");
        let cli_path = root.join(".local/bin/rmdv");

        super::install_link(&target, &cli_path).expect("install CLI symlink");

        assert_eq!(
            std::fs::read_link(&cli_path).expect("read CLI symlink"),
            target
        );
        assert_eq!(
            std::fs::canonicalize(&cli_path).expect("resolve CLI symlink"),
            std::fs::canonicalize(&target).expect("resolve AppImage")
        );
        std::fs::remove_dir_all(root).expect("remove test directory");
    }
}
