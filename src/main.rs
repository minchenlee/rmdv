use mdv::app::App;
use std::path::PathBuf;
use std::time::Instant;

fn main() -> iced::Result {
    let t0 = Instant::now();
    mdv::bench::set_process_start(t0);

    // Subcommand dispatch happens before any Iced setup so `mdv theme list`
    // runs as a normal CLI without spawning a window.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(first) = args.first() {
        if first == "theme" {
            std::process::exit(run_theme_cmd(&args[1..]));
        }
        if first == "--help" || first == "-h" {
            print_help();
            std::process::exit(0);
        }
        if first == "--version" || first == "-V" {
            println!("mdv {}", env!("CARGO_PKG_VERSION"));
            std::process::exit(0);
        }
    }

    let bench = std::env::args().any(|a| a == "--benchmark-startup");
    if bench {
        // Set before any Iced threads spawn — set_var is unsound in multi-threaded contexts.
        std::env::set_var("MDV_BENCH_STARTUP", "1");
    }

    let initial: Option<PathBuf> = std::env::args()
        .skip(1)
        .find(|a| !a.starts_with("--"))
        .map(PathBuf::from);

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

    if bench {
        eprintln!("startup: pre_run={:?}", t0.elapsed());
    }

    iced::application(move || App::new(initial.clone()), App::update, App::view)
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

fn print_help() {
    println!("mdv {}", env!("CARGO_PKG_VERSION"));
    println!("Markdown viewer — open a file or folder.");
    println!();
    println!("USAGE:");
    println!("    mdv [FILE|DIR]");
    println!("    mdv theme <SUBCOMMAND>");
    println!();
    println!("THEME SUBCOMMANDS:");
    println!("    list                  List built-in and custom themes");
    println!("    dir                   Print the custom themes directory");
    println!("    import <PATH>         Import a theme (auto-detects format)");
    println!("    import --base16 <P>   Import a Base16 YAML scheme");
    println!("    import --vscode <P>   Import a VS Code JSON theme");
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
