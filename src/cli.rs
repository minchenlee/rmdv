use crate::ipc::{Cmd, Mode, Request};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "mdv", version, about = "Lightweight beautiful markdown viewer")]
pub struct Cli {
    /// File or directory to open (bare form).
    pub target: Option<PathBuf>,
    /// Source line to scroll to (only meaningful with a file target or `goto`).
    #[arg(long)]
    pub line: Option<u32>,
    /// Section path (e.g. "Install/Setup") to scroll to.
    #[arg(long)]
    pub section: Option<String>,
    /// View mode to switch to.
    #[arg(long, value_enum)]
    pub mode: Option<CliMode>,
    /// Pretty-print JSON output (default: compact, one line).
    #[arg(long, global = true)]
    pub pretty: bool,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CliMode {
    View,
    Edit,
    Mindmap,
}

impl From<CliMode> for Mode {
    fn from(m: CliMode) -> Self {
        match m {
            CliMode::View => Mode::View,
            CliMode::Edit => Mode::Edit,
            CliMode::Mindmap => Mode::Mindmap,
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Open a file (with optional line/section anchor).
    Open(OpenArgs),
    /// Open a folder (sets the sidebar workspace).
    OpenFolder { dir: PathBuf },
    /// Scroll the current file to a line or section.
    Goto(GotoArgs),
    /// Switch view mode.
    Mode { mode: CliMode },
    /// Reveal a file in the sidebar tree.
    Reveal { file: PathBuf },
    /// Raise the mdv window.
    Focus,
    /// Close the mdv window (quit).
    Close,
    /// Print current state as JSON.
    Current,
    /// List headings of a markdown file as JSON. Stateless — does not require a
    /// running mdv instance.
    ListSections { file: PathBuf },
    /// Theme subcommand (existing).
    Theme {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Debug, Args)]
pub struct OpenArgs {
    pub file: PathBuf,
    #[arg(long)]
    pub line: Option<u32>,
    #[arg(long)]
    pub section: Option<String>,
}

#[derive(Debug, Args)]
pub struct GotoArgs {
    #[arg(long)]
    pub line: Option<u32>,
    #[arg(long)]
    pub section: Option<String>,
}

#[derive(Debug)]
pub enum ParsedCli {
    /// Bare `mdv` invocation, no args. Launch instance idle, or focus running one.
    Empty,
    /// Stateless subcommand — runs without an instance and exits.
    Stateless(Stateless),
    /// Theme passthrough — handed to existing `run_theme_cmd`.
    Theme(Vec<String>),
    /// A request to forward to (or apply on startup of) the instance.
    Request(Request),
}

#[derive(Debug)]
pub enum Stateless {
    ListSections { file: PathBuf, pretty: bool },
}

/// Parse argv into a `ParsedCli`. The `id` of any emitted Request is `1`
/// (single-shot client).
pub fn parse_from<I, S>(argv: I) -> Result<ParsedCli, clap::Error>
where
    I: IntoIterator<Item = S>,
    S: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::try_parse_from(argv)?;
    Ok(to_parsed(cli))
}

fn to_parsed(cli: Cli) -> ParsedCli {
    if let Some(cmd) = cli.command {
        return match cmd {
            Command::Open(o) => req(Cmd::Open {
                file: path_to_string(o.file),
                line: o.line,
                section: o.section,
            }),
            Command::OpenFolder { dir } => req(Cmd::OpenFolder { dir: path_to_string(dir) }),
            Command::Goto(g) => req(Cmd::Goto { line: g.line, section: g.section }),
            Command::Mode { mode } => req(Cmd::Mode { mode: mode.into() }),
            Command::Reveal { file } => req(Cmd::Reveal { file: path_to_string(file) }),
            Command::Focus => req(Cmd::Focus),
            Command::Close => req(Cmd::Close),
            Command::Current => req(Cmd::Current),
            Command::ListSections { file } => ParsedCli::Stateless(Stateless::ListSections {
                file,
                pretty: cli.pretty,
            }),
            Command::Theme { args } => ParsedCli::Theme(args),
        };
    }
    match cli.target {
        None => ParsedCli::Empty,
        Some(path) => {
            let cmd = if path.is_dir() {
                Cmd::OpenFolder { dir: path_to_string(path) }
            } else {
                Cmd::Open {
                    file: path_to_string(path),
                    line: cli.line,
                    section: cli.section,
                }
            };
            req(cmd)
        }
    }
}

fn req(cmd: Cmd) -> ParsedCli {
    ParsedCli::Request(Request { id: 1, cmd })
}

fn path_to_string(p: PathBuf) -> String {
    p.to_string_lossy().into_owned()
}
