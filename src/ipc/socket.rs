use std::path::PathBuf;

/// Explicit socket path override. When set, it is the only endpoint, which
/// keeps isolated instances (benchmarks, tests) away from the personal one.
pub const OVERRIDE_ENV: &str = "RMDV_SOCKET";

fn override_path() -> Option<PathBuf> {
    std::env::var_os(OVERRIDE_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

pub fn has_override() -> bool {
    override_path().is_some()
}

#[cfg(unix)]
pub fn candidate_paths() -> Vec<PathBuf> {
    if let Some(path) = override_path() {
        return vec![path];
    }
    let uid = unsafe { libc::getuid() };
    crate::terminal::TerminalEnvironment::current().socket_paths(uid)
}

#[cfg(unix)]
pub fn default_path() -> PathBuf {
    candidate_paths()
        .into_iter()
        .last()
        .expect("the Unix socket candidate list is never empty")
}

#[cfg(windows)]
pub fn default_path() -> PathBuf {
    let user = std::env::var("USERNAME").unwrap_or_else(|_| "default".to_string());
    PathBuf::from(format!(r"\\.\pipe\rmdv-{user}"))
}

#[cfg(windows)]
pub fn candidate_paths() -> Vec<PathBuf> {
    vec![override_path().unwrap_or_else(default_path)]
}
