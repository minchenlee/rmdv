//! Launch-environment detection used by terminal-facing process boundaries.
//!
//! `rmdv` is a GUI application, so terminal state must not decide whether the
//! application can start. This module records the available signals in one
//! place and provides a conservative, stable socket namespace for the IPC
//! layer. In particular, terminal multiplexers must not create a separate
//! rmdv instance merely because they changed `TMPDIR` or TTY state. It does
//! not invoke `tmux`, `ps`, or another process-ancestry helper: those probes
//! are optional and platform-specific, while the IPC namespace remains safe
//! and stable when all such hints are absent.

use std::env;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

/// A terminal multiplexer that can be identified from the launch context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Multiplexer {
    Tmux,
}

/// Raw, testable inputs used to build a [`TerminalEnvironment`] snapshot.
#[derive(Debug, Clone, Copy, Default)]
pub struct TerminalSignals<'a> {
    /// The value of `TMUX`, if it is valid Unicode.
    pub tmux: Option<&'a str>,
    /// The value of `TERM`, if it is valid Unicode.
    pub term: Option<&'a str>,
    /// The value of `TMPDIR`, if it is present.
    pub temp_dir: Option<&'a Path>,
    pub stdin_is_tty: bool,
    pub stdout_is_tty: bool,
    pub stderr_is_tty: bool,
}

/// A best-effort snapshot of the process launch environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalEnvironment {
    stdin_is_tty: bool,
    stdout_is_tty: bool,
    stderr_is_tty: bool,
    multiplexer: Option<Multiplexer>,
    temp_dir: Option<PathBuf>,
}

impl TerminalEnvironment {
    /// Read the environment and the three standard stream descriptors.
    pub fn current() -> Self {
        let tmux = env::var("TMUX").ok();
        let term = env::var("TERM").ok();
        let temp_dir = env::var_os("TMPDIR").map(PathBuf::from);

        Self::from_signals(TerminalSignals {
            tmux: tmux.as_deref(),
            term: term.as_deref(),
            temp_dir: temp_dir.as_deref(),
            stdin_is_tty: std::io::stdin().is_terminal(),
            stdout_is_tty: std::io::stdout().is_terminal(),
            stderr_is_tty: std::io::stderr().is_terminal(),
        })
    }

    /// Build a snapshot from explicit signals. This keeps environment parsing
    /// deterministic and avoids mutating process-global variables in tests.
    pub fn from_signals(signals: TerminalSignals<'_>) -> Self {
        let multiplexer = (signals.tmux.is_some_and(valid_tmux_value)
            || signals.term.is_some_and(is_tmux_term))
        .then_some(Multiplexer::Tmux);

        Self {
            stdin_is_tty: signals.stdin_is_tty,
            stdout_is_tty: signals.stdout_is_tty,
            stderr_is_tty: signals.stderr_is_tty,
            multiplexer,
            temp_dir: signals
                .temp_dir
                .filter(|path| valid_temp_dir(path))
                .map(Path::to_path_buf),
        }
    }

    /// Whether a valid tmux marker or tmux-specific terminal type was found.
    pub fn is_tmux(&self) -> bool {
        matches!(self.multiplexer, Some(Multiplexer::Tmux))
    }

    /// Whether stdin and stdout both look like an interactive terminal.
    ///
    /// This is informational only. Callers must not use it to reject a GUI
    /// launch or a stateless CLI invocation whose output is being piped.
    pub fn is_interactive(&self) -> bool {
        self.stdin_is_tty && self.stdout_is_tty
    }

    pub fn stdin_is_tty(&self) -> bool {
        self.stdin_is_tty
    }

    pub fn stdout_is_tty(&self) -> bool {
        self.stdout_is_tty
    }

    pub fn stderr_is_tty(&self) -> bool {
        self.stderr_is_tty
    }

    /// The valid `TMPDIR` supplied by the launcher, if there was one.
    pub fn configured_temp_dir(&self) -> Option<&Path> {
        self.temp_dir.as_deref()
    }

    /// Return the stable IPC endpoint followed by the legacy environment path.
    ///
    /// The stable endpoint is deliberately independent of `TMUX`, `TERM`, and
    /// TTY state: rmdv is one instance per user, not one instance per shell.
    /// The configured path remains as a compatibility endpoint for instances
    /// started by older builds and for environments where `/tmp` is not usable.
    #[cfg(unix)]
    pub fn socket_paths(&self, uid: u32) -> Vec<PathBuf> {
        let socket_name = format!("rmdv-{uid}.sock");
        let stable = stable_socket_root(uid).join(&socket_name);
        let legacy_root = self.temp_dir.as_deref().unwrap_or(Path::new("/tmp"));
        let legacy = legacy_root.join(socket_name);

        if stable == legacy {
            vec![stable]
        } else {
            vec![stable, legacy]
        }
    }
}

#[cfg(unix)]
fn stable_socket_root(uid: u32) -> PathBuf {
    let Some(home) = user_home_dir(uid) else {
        return PathBuf::from("/tmp");
    };

    #[cfg(target_os = "macos")]
    let root = home.join("Library").join("Caches").join("rmdv");
    #[cfg(not(target_os = "macos"))]
    let root = home.join(".cache").join("rmdv");
    root
}

#[cfg(unix)]
fn user_home_dir(uid: u32) -> Option<PathBuf> {
    let passwd = unsafe { libc::getpwuid(uid as libc::uid_t) };
    if passwd.is_null() {
        return None;
    }

    let home = unsafe { (*passwd).pw_dir };
    if home.is_null() {
        return None;
    }

    unsafe { std::ffi::CStr::from_ptr(home) }
        .to_str()
        .ok()
        .filter(|path| path.starts_with('/'))
        .map(PathBuf::from)
}

fn valid_temp_dir(path: &Path) -> bool {
    path.is_absolute() && !path.as_os_str().is_empty()
}

/// A normal tmux value is `<absolute socket>,<server pid>,<session id>`.
/// Nested sessions use the same shape, so no session-specific partitioning is
/// needed here.
fn valid_tmux_value(value: &str) -> bool {
    let mut fields = value.rsplitn(3, ',');
    let Some(session_id) = fields.next() else {
        return false;
    };
    let Some(server_pid) = fields.next() else {
        return false;
    };
    let Some(socket_path) = fields.next() else {
        return false;
    };

    !socket_path.is_empty()
        && Path::new(socket_path).is_absolute()
        && server_pid.parse::<u32>().is_ok()
        && session_id.parse::<u32>().is_ok()
}

fn is_tmux_term(value: &str) -> bool {
    value == "tmux"
        || value
            .strip_prefix("tmux-")
            .is_some_and(|suffix| !suffix.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signals<'a>(tmux: Option<&'a str>, term: Option<&'a str>) -> TerminalSignals<'a> {
        TerminalSignals {
            tmux,
            term,
            temp_dir: Some(Path::new("/custom/tmp")),
            stdin_is_tty: true,
            stdout_is_tty: true,
            stderr_is_tty: true,
        }
    }

    #[test]
    fn normal_terminal_is_interactive_and_not_tmux() {
        let env = TerminalEnvironment::from_signals(signals(None, Some("xterm-256color")));

        assert!(env.is_interactive());
        assert!(!env.is_tmux());
        assert_eq!(env.configured_temp_dir(), Some(Path::new("/custom/tmp")));
    }

    #[test]
    fn valid_tmux_marker_identifies_nested_sessions() {
        let env = TerminalEnvironment::from_signals(signals(
            Some("/private/tmp/tmux-501/default,1234,7"),
            Some("screen-256color"),
        ));

        assert!(env.is_tmux());
        assert!(env.is_interactive());
    }

    #[test]
    fn tmux_term_is_a_fallback_when_marker_is_unavailable() {
        let env = TerminalEnvironment::from_signals(signals(None, Some("tmux-256color")));

        assert!(env.is_tmux());
    }

    #[test]
    fn malformed_environment_is_ignored_without_becoming_tmux() {
        let env = TerminalEnvironment::from_signals(signals(
            Some("not-a-tmux-value"),
            Some("xterm-256color"),
        ));

        assert!(!env.is_tmux());
    }

    #[test]
    fn malformed_temp_dir_is_discarded() {
        let env = TerminalEnvironment::from_signals(TerminalSignals {
            temp_dir: Some(Path::new("relative/tmp")),
            ..signals(None, None)
        });

        assert_eq!(env.configured_temp_dir(), None);
    }

    #[test]
    fn noninteractive_streams_do_not_block_detection() {
        let env = TerminalEnvironment::from_signals(TerminalSignals {
            stdin_is_tty: false,
            stdout_is_tty: false,
            stderr_is_tty: false,
            ..signals(Some("/tmp/tmux.sock,1234,0"), Some("screen"))
        });

        assert!(!env.is_interactive());
        assert!(env.is_tmux());
    }

    #[cfg(unix)]
    #[test]
    fn socket_paths_are_stable_before_the_legacy_temp_path() {
        let env = TerminalEnvironment::from_signals(signals(None, None));
        let paths = env.socket_paths(42);

        assert_eq!(
            paths[0].file_name(),
            Some(std::ffi::OsStr::new("rmdv-42.sock"))
        );
        assert_ne!(paths[0], Path::new("/custom/tmp/rmdv-42.sock"));
        assert_eq!(paths[1], Path::new("/custom/tmp/rmdv-42.sock"));
    }
}
