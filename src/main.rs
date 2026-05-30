use mdv::app::App;
use mdv::cli::{parse_from, ParsedCli, Stateless};
use mdv::ipc;
use std::path::PathBuf;
use std::time::Instant;

fn main() -> iced::Result {
    let t0 = Instant::now();
    mdv::bench::set_process_start(t0);

    let parsed = match parse_from(std::env::args_os()) {
        Ok(p) => p,
        Err(e) => {
            e.exit();
        }
    };

    match parsed {
        ParsedCli::Theme(args) => std::process::exit(run_theme_cmd(&args)),
        ParsedCli::Stateless(Stateless::ListSections { file, pretty }) => {
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
                    print_response(&resp, false);
                    std::process::exit(if resp.ok { 0 } else { 1 });
                }
                Ok(None) => {
                    return launch_instance(Some(req));
                }
                Err(e) => {
                    eprintln!("{{\"error\":\"{}\"}}", e.to_string().replace('"', "'"));
                    std::process::exit(2);
                }
            }
        }
    }
}

fn launch_instance(initial: Option<ipc::Request>) -> iced::Result {
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
            ipc::Cmd::Open { line, section, .. } => Some(mdv::app::PendingNav {
                line: *line,
                section: section.clone(),
                ..Default::default()
            }),
            _ => None,
        },
        None => None,
    };

    #[cfg(target_os = "macos")]
    let platform_specific = iced::window::settings::PlatformSpecific {
        title_hidden: true,
        titlebar_transparent: true,
        fullsize_content_view: true,
    };
    #[cfg(not(target_os = "macos"))]
    let platform_specific = iced::window::settings::PlatformSpecific::default();
    let window = iced::window::Settings {
        platform_specific,
        ..Default::default()
    };

    iced::application(
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
    .subscription(App::subscription)
    .window(window)
    .font(include_bytes!("assets/fonts/Inter-Variable.ttf").as_slice())
    .font(include_bytes!("assets/fonts/JetBrainsMono-Regular.otf").as_slice())
    .font(include_bytes!("assets/fonts/lucide.ttf").as_slice())
    .default_font(iced::Font::with_name("Inter"))
    .run()
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
    let sub = match args.first().map(String::as_str) {
        Some(s) => s,
        None => {
            eprintln!("usage: mdv theme <list|dir|import>");
            return 2;
        }
    };
    match sub {
        "list" => {
            for p in mdv::theme::ThemePreset::ALL {
                println!(
                    "{:24} {:6} builtin",
                    mdv::theme::preset_slug(p),
                    if p.is_dark() { "dark" } else { "light" }
                );
            }
            for t in mdv::theme_load::bundled() {
                println!(
                    "{:24} {:6} bundled",
                    t.slug,
                    if t.dark { "dark" } else { "light" }
                );
            }
            let mut errs = Vec::new();
            for t in mdv::theme_load::discover(&mut errs) {
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
        "dir" => match mdv::theme_load::themes_dir() {
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
        eprintln!("usage: mdv theme import [--base16|--vscode] <path>");
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
        "base16" => mdv::theme_import::import_base16(&path),
        "vscode" => mdv::theme_import::import_vscode(&path),
        _ => mdv::theme_import::import_auto(&path),
    };
    let imp = match imp {
        Ok(v) => v,
        Err(e) => {
            eprintln!("import failed: {e}");
            return 1;
        }
    };
    let dir = match mdv::theme_load::ensure_themes_dir() {
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
