#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use rmdv::app::App;
use rmdv::cli::{parse_from, ParsedCli, Stateless};
use rmdv::ipc;
use std::path::PathBuf;
use std::time::Instant;

// Windows launches rmdv as a console-subsystem process, so Explorer and the
// NSIS shortcut open a console window that stays on screen for the whole GUI
// session. Release builds are marked windows-subsystem (the crate attribute at
// the top of this file), which has no console to inherit. Stateless CLI
// requests started from cmd/PowerShell still have to answer on the caller's
// stdout/stderr, so they re-attach to that console before printing.
//
// The module compiles on every Windows build, not only release ones, so the
// PR-time `cargo check --no-default-features` job type-checks the FFI; in a
// console-subsystem debug build `attach()` sees the inherited handles and
// returns without doing anything.
#[cfg(windows)]
mod win_console {
    use core::ffi::c_void;

    type Handle = *mut c_void;

    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    const STD_INPUT_HANDLE: u32 = -10i32 as u32;
    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    const STD_ERROR_HANDLE: u32 = -12i32 as u32;
    const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;
    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const FILE_SHARE_READ: u32 = 0x1;
    const FILE_SHARE_WRITE: u32 = 0x2;
    const OPEN_EXISTING: u32 = 3;

    #[link(name = "kernel32")]
    extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
        fn GetStdHandle(std_handle: u32) -> Handle;
        fn SetStdHandle(std_handle: u32, handle: Handle) -> i32;
        fn CreateFileW(
            file_name: *const u16,
            desired_access: u32,
            share_mode: u32,
            security_attributes: *const c_void,
            creation_disposition: u32,
            flags_and_attributes: u32,
            template_file: Handle,
        ) -> Handle;
    }

    /// Mirror the standard handles onto `CONIN$`/`CONOUT$`.
    ///
    /// `AttachConsole` alone leaves the std handles null, because a
    /// windows-subsystem process is not given them at launch, so Rust's
    /// `println!`/`eprintln!` would drop their output.
    unsafe fn bind_device(name: &[u16], access: u32) -> Handle {
        CreateFileW(
            name.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            core::ptr::null(),
            OPEN_EXISTING,
            0,
            core::ptr::null_mut(),
        )
    }

    fn to_wide(name: &str) -> Vec<u16> {
        name.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn attach() {
        // Redirected stdout/stderr (a pipe or file) already has a valid
        // handle; keep it instead of replacing it with the console device.
        let stdout_missing = {
            let handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
            handle.is_null() || handle == INVALID_HANDLE_VALUE
        };
        if !stdout_missing {
            return;
        }

        // Fails when Explorer or another console-less parent launched us;
        // there is nothing to attach to and output is simply discarded.
        if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } == 0 {
            return;
        }

        let input_name = to_wide("CONIN$");
        let output_name = to_wide("CONOUT$");
        unsafe {
            let input = bind_device(&input_name, GENERIC_READ | GENERIC_WRITE);
            if !input.is_null() && input != INVALID_HANDLE_VALUE {
                SetStdHandle(STD_INPUT_HANDLE, input);
            }
            let output = bind_device(&output_name, GENERIC_READ | GENERIC_WRITE);
            if !output.is_null() && output != INVALID_HANDLE_VALUE {
                SetStdHandle(STD_OUTPUT_HANDLE, output);
                SetStdHandle(STD_ERROR_HANDLE, output);
            }
        }
    }
}

/// Reborrow the console this process was started from, if any.
///
/// Called only on Windows stateless CLI paths, after argument parsing has
/// established that this process will print and exit rather than open the
/// reader. A windows-subsystem process never gets console handles from its
/// parent, so without this `println!`/`eprintln!` would silently drop output.
#[cfg(windows)]
fn attach_caller_console() {
    win_console::attach();
}

#[cfg(not(windows))]
fn attach_caller_console() {}

fn main() -> iced::Result {
    let t0 = Instant::now();
    rmdv::bench::set_process_start(t0);

    let parsed = match parse_from(std::env::args_os()) {
        Ok(p) => p,
        Err(e) => {
            // clap writes `--help`, `--version`, and usage errors here.
            attach_caller_console();
            e.exit();
        }
    };

    match parsed {
        ParsedCli::Theme(args) => {
            attach_caller_console();
            std::process::exit(run_theme_cmd(&args));
        }
        ParsedCli::Stateless(Stateless::ListSections { file, pretty }) => {
            attach_caller_console();
            std::process::exit(run_list_sections(&file, pretty));
        }
        ParsedCli::Empty => {
            let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
            let already = rt.block_on(async {
                ipc::client::try_send(&ipc::Request {
                    id: 1,
                    cmd: ipc::Cmd::Focus,
                })
                .await
                .ok()
                .flatten()
                .is_some()
            });
            if already {
                std::process::exit(0);
            }
            return launch_instance(None);
        }
        ParsedCli::Request(req) => {
            let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
            let result = rt.block_on(ipc::client::try_send(&req));
            match result {
                Ok(Some(resp)) => {
                    attach_caller_console();
                    print_response(&resp, false);
                    std::process::exit(if resp.ok { 0 } else { 1 });
                }
                Ok(None) => {
                    return launch_instance(Some(req));
                }
                Err(e) => {
                    attach_caller_console();
                    eprintln!("{}", serde_json::json!({ "error": e.to_string() }));
                    std::process::exit(2);
                }
            }
        }
    }
}

fn launch_instance(initial: Option<ipc::Request>) -> iced::Result {
    // Rapid render bursts (mindmap arrow-nav) churn GBs of multi-MB
    // allocations through malloc; libmalloc's large-entry cache holds the
    // freed pages as empty MALLOC_LARGE regions for ~a minute, so footprint
    // reads hundreds of MB above live heap. The cache knob is read at
    // libmalloc init (before main), so re-exec the GUI process once with it
    // disabled. Respects an explicitly set MallocLargeCache.
    #[cfg(target_os = "macos")]
    if std::env::var_os("MallocLargeCache").is_none() {
        use std::os::unix::process::CommandExt;
        if let Ok(exe) = std::env::current_exe() {
            let err = std::process::Command::new(exe)
                .args(std::env::args_os().skip(1))
                .env("MallocLargeCache", "0")
                .exec();
            // exec only returns on failure; run without the knob.
            eprintln!("rmdv: malloc-tuning re-exec failed: {err}");
        }
    }

    let initial_path: Option<PathBuf> = match &initial {
        Some(req) => match &req.cmd {
            ipc::Cmd::Open { file, .. } => Some(PathBuf::from(file)),
            ipc::Cmd::OpenFolder { dir } => Some(PathBuf::from(dir)),
            _ => None,
        },
        None => None,
    };
    let pending_nav = match &initial {
        Some(req) => match &req.cmd {
            ipc::Cmd::Open { line, section, .. } => Some(rmdv::app::PendingNav {
                line: *line,
                section: section.clone(),
                ..Default::default()
            }),
            _ => None,
        },
        None => None,
    };

    // The saved window frame is restored here: on macOS onto the screens'
    // visible areas; elsewhere at its size, centered.
    let prefs = rmdv::prefs::load();
    let window = rmdv::app::launch_window_settings(
        rmdv::macos_vibrancy::launch_transparent(prefs.glass),
        prefs.window,
    );

    let mut app = iced::application(
        move || {
            let (mut app, task) = App::new(initial_path.clone());
            app.pending_nav = pending_nav.clone();
            (app, task)
        },
        App::update,
        App::view,
    )
    .title(App::title)
    .theme(App::theme)
    .style(App::app_style)
    .subscription(App::subscription)
    .window(window)
    .default_font(iced::Font::with_name("Inter"));
    for font in rmdv::fonts::BUNDLED {
        app = app.font(font);
    }
    app.run()
}

fn run_list_sections(file: &std::path::Path, pretty: bool) -> i32 {
    let src = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{{\"error\":\"read {}: {}\"}}", file.display(), e);
            return 1;
        }
    };
    let sections = ipc::sections::list_sections(&src);
    let out = if pretty {
        serde_json::to_string_pretty(&sections)
    } else {
        serde_json::to_string(&sections)
    };
    match out {
        Ok(s) => {
            println!("{s}");
            0
        }
        Err(e) => {
            eprintln!("{{\"error\":\"json: {e}\"}}");
            1
        }
    }
}

fn print_response(resp: &ipc::Response, pretty: bool) {
    let out = if pretty {
        serde_json::to_string_pretty(resp)
    } else {
        serde_json::to_string(resp)
    };
    match out {
        Ok(s) => println!("{s}"),
        Err(e) => eprintln!("{{\"error\":\"json: {e}\"}}"),
    }
}

fn run_theme_cmd(args: &[String]) -> i32 {
    // The theme CLI reads the config dir directly, so migrate before it does.
    rmdv::config_migrate::run();
    let sub = match args.first().map(String::as_str) {
        Some(s) => s,
        None => {
            eprintln!("usage: rmdv theme <list|dir|import>");
            return 2;
        }
    };
    match sub {
        "list" => {
            for p in rmdv::theme::ThemePreset::ALL {
                println!(
                    "{:24} {:6} builtin",
                    rmdv::theme::preset_slug(p),
                    if p.is_dark() { "dark" } else { "light" }
                );
            }
            for t in rmdv::theme_load::bundled() {
                println!(
                    "{:24} {:6} bundled",
                    t.slug,
                    if t.dark { "dark" } else { "light" }
                );
            }
            let mut errs = Vec::new();
            for t in rmdv::theme_load::discover(&mut errs) {
                println!(
                    "{:24} {:6} custom ({})",
                    t.slug,
                    if t.dark { "dark" } else { "light" },
                    t.path.display()
                );
            }
            for e in errs {
                eprintln!("warning: {e}");
            }
            0
        }
        "dir" => match rmdv::theme_load::themes_dir() {
            Some(d) => {
                println!("{}", d.display());
                0
            }
            None => {
                eprintln!("no config dir on this platform");
                1
            }
        },
        "import" => run_theme_import(&args[1..]),
        other => {
            eprintln!("unknown theme subcommand: {other}");
            2
        }
    }
}

fn run_theme_import(args: &[String]) -> i32 {
    if args.is_empty() {
        eprintln!("usage: rmdv theme import [--base16|--vscode] <path>");
        return 2;
    }
    let (kind, path_str) = match args[0].as_str() {
        "--base16" => ("base16", args.get(1).map(String::as_str)),
        "--vscode" => ("vscode", args.get(1).map(String::as_str)),
        other => ("auto", Some(other)),
    };
    let Some(p) = path_str else {
        eprintln!("missing path");
        return 2;
    };
    let path = PathBuf::from(p);
    let imp = match kind {
        "base16" => rmdv::theme_import::import_base16(&path),
        "vscode" => rmdv::theme_import::import_vscode(&path),
        _ => rmdv::theme_import::import_auto(&path),
    };
    let imp = match imp {
        Ok(v) => v,
        Err(e) => {
            eprintln!("import failed: {e}");
            return 1;
        }
    };
    let dir = match rmdv::theme_load::ensure_themes_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("could not create themes dir: {e}");
            return 1;
        }
    };
    let out = dir.join(format!("{}.toml", imp.slug));
    if let Err(e) = std::fs::write(&out, &imp.toml) {
        eprintln!("write failed: {e}");
        return 1;
    }
    println!("imported \"{}\" -> {}", imp.name, out.display());
    0
}
