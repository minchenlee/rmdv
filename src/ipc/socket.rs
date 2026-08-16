use std::path::PathBuf;

#[cfg(unix)]
pub fn candidate_paths() -> Vec<PathBuf> {
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
    vec![default_path()]
}
