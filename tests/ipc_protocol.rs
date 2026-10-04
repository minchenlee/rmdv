use rmdv::ipc::{Cmd, FocusBehavior, Mode, Request, Response};
use serde_json::json;

#[test]
fn request_open_round_trip() {
    let req = Request {
        id: 1,
        cmd: Cmd::Open {
            file: "/abs/foo.md".into(),
            line: Some(42),
            section: Some("Install/Setup".into()),
            focus: FocusBehavior::Default,
        },
    };
    let s = serde_json::to_string(&req).unwrap();
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["cmd"], "open");
    assert_eq!(v["args"]["file"], "/abs/foo.md");
    assert_eq!(v["args"]["line"], 42);
    assert_eq!(v["args"]["section"], "Install/Setup");
    let back: Request = serde_json::from_str(&s).unwrap();
    assert_eq!(back, req);
}

#[test]
fn request_goto_line_round_trip() {
    let req = Request {
        id: 5,
        cmd: Cmd::Goto {
            line: Some(10),
            section: None,
            focus: FocusBehavior::Default,
        },
    };
    let s = serde_json::to_string(&req).unwrap();
    let back: Request = serde_json::from_str(&s).unwrap();
    assert_eq!(back, req);
}

#[test]
fn request_mode_round_trip() {
    for m in [Mode::View, Mode::Edit, Mode::Mindmap] {
        let req = Request {
            id: 9,
            cmd: Cmd::Mode {
                mode: m,
                focus: FocusBehavior::Default,
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&s).unwrap();
        assert_eq!(back, req);
    }
}

#[test]
fn response_ok_no_result_serialises_without_result_field() {
    let r = Response {
        id: 1,
        ok: true,
        result: None,
        error: None,
    };
    let v: serde_json::Value = serde_json::to_value(&r).unwrap();
    assert_eq!(v, json!({"id":1,"ok":true}));
}

#[test]
fn response_error_serialises_with_error_field() {
    let r = Response {
        id: 1,
        ok: false,
        result: None,
        error: Some("no file open".into()),
    };
    let v: serde_json::Value = serde_json::to_value(&r).unwrap();
    assert_eq!(v, json!({"id":1,"ok":false,"error":"no file open"}));
}

#[test]
fn response_current_result_serialises() {
    let r = Response {
        id: 3,
        ok: true,
        result: Some(json!({
            "file": "/abs/foo.md",
            "line": 42,
            "mode": "view",
            "folder": "/abs"
        })),
        error: None,
    };
    let s = serde_json::to_string(&r).unwrap();
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["result"]["mode"], "view");
}

use rmdv::ipc::lines::{block_for_line, build_byte_to_line};

#[test]
fn byte_to_line_empty_source() {
    let table = build_byte_to_line("");
    assert_eq!(table.line_for_byte(0), 1);
}

#[test]
fn byte_to_line_three_lines() {
    let src = "a\nbb\nccc";
    let table = build_byte_to_line(src);
    assert_eq!(table.line_for_byte(0), 1); // 'a'
    assert_eq!(table.line_for_byte(1), 1); // '\n' belongs to line 1
    assert_eq!(table.line_for_byte(2), 2); // 'b'
    assert_eq!(table.line_for_byte(5), 3); // 'c'
    assert_eq!(table.line_for_byte(99), 3); // out of range clamps to last
}

#[test]
fn block_for_line_empty_returns_none() {
    assert_eq!(block_for_line(10, &[]), None);
}

#[test]
fn block_for_line_exact_match() {
    let lines = [1u32, 5, 12, 20];
    assert_eq!(block_for_line(5, &lines), Some(1));
    assert_eq!(block_for_line(12, &lines), Some(2));
}

#[test]
fn block_for_line_before_first_clamps_to_first() {
    let lines = [3u32, 7, 11];
    assert_eq!(block_for_line(1, &lines), Some(0));
}

#[test]
fn block_for_line_between_blocks_picks_preceding() {
    let lines = [1u32, 5, 12, 20];
    assert_eq!(block_for_line(8, &lines), Some(1));
    assert_eq!(block_for_line(19, &lines), Some(2));
}

#[test]
fn block_for_line_after_last_picks_last() {
    let lines = [1u32, 5, 12];
    assert_eq!(block_for_line(9999, &lines), Some(2));
}

#[test]
fn block_for_line_duplicate_line_values_picks_first_match() {
    let lines = [1u32, 5, 5, 10];
    let idx = block_for_line(5, &lines).unwrap();
    assert!(idx == 1 || idx == 2, "got {idx}");
}

#[test]
fn parser_emits_byte_offsets_aligned_with_blocks() {
    let src = "# H1\n\npara one\n\n## H2\n\npara two\n";
    let (blocks, offsets) = rmdv::parser::parse(src);
    assert_eq!(blocks.len(), offsets.len());
    let table = rmdv::ipc::lines::build_byte_to_line(src);
    let lines: Vec<u32> = offsets
        .iter()
        .map(|&b| table.line_for_byte(b as usize))
        .collect();
    assert_eq!(lines[0], 1, "H1 on line 1, got {}", lines[0]);
    assert_eq!(lines[1], 3, "first paragraph on line 3, got {}", lines[1]);
    assert_eq!(lines[2], 5, "H2 on line 5, got {}", lines[2]);
    assert_eq!(lines[3], 7, "second paragraph on line 7, got {}", lines[3]);
}

use rmdv::cli::{parse_from, ParsedCli};

#[test]
fn cli_bare_file_becomes_open_request() {
    let p = parse_from(["rmdv", "/abs/foo.md"]).unwrap();
    match p {
        ParsedCli::Request(r) => match r.cmd {
            Cmd::Open {
                file,
                line: None,
                section: None,
                ..
            } => assert_eq!(file, "/abs/foo.md"),
            other => panic!("unexpected cmd: {other:?}"),
        },
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn cli_bare_file_with_line_and_section() {
    let p = parse_from([
        "rmdv",
        "/abs/foo.md",
        "--line",
        "42",
        "--section",
        "Install/Setup",
    ])
    .unwrap();
    let ParsedCli::Request(r) = p else { panic!() };
    match r.cmd {
        Cmd::Open {
            file,
            line: Some(42),
            section: Some(s),
            ..
        } => {
            assert_eq!(file, "/abs/foo.md");
            assert_eq!(s, "Install/Setup");
        }
        other => panic!("unexpected cmd: {other:?}"),
    }
}

#[test]
fn cli_goto_subcommand() {
    let p = parse_from(["rmdv", "goto", "--line", "10"]).unwrap();
    let ParsedCli::Request(r) = p else { panic!() };
    assert!(matches!(
        r.cmd,
        Cmd::Goto {
            line: Some(10),
            section: None,
            ..
        }
    ));
}

#[test]
fn cli_mode_subcommand() {
    let p = parse_from(["rmdv", "mode", "edit"]).unwrap();
    let ParsedCli::Request(r) = p else { panic!() };
    assert!(matches!(
        r.cmd,
        Cmd::Mode {
            mode: Mode::Edit,
            ..
        }
    ));
}

#[test]
fn cli_goto_with_focus_flag_emits_force() {
    let p = parse_from(["rmdv", "goto", "--line", "10", "--focus"]).unwrap();
    let ParsedCli::Request(r) = p else { panic!() };
    match r.cmd {
        Cmd::Goto { focus, .. } => assert_eq!(focus, FocusBehavior::Force),
        other => panic!("{other:?}"),
    }
}

#[test]
fn cli_open_with_no_focus_flag_emits_suppress() {
    let p = parse_from(["rmdv", "open", "/abs/foo.md", "--no-focus"]).unwrap();
    let ParsedCli::Request(r) = p else { panic!() };
    match r.cmd {
        Cmd::Open { focus, .. } => assert_eq!(focus, FocusBehavior::Suppress),
        other => panic!("{other:?}"),
    }
}

#[test]
fn cli_current_subcommand() {
    let p = parse_from(["rmdv", "current"]).unwrap();
    let ParsedCli::Request(r) = p else { panic!() };
    assert!(matches!(r.cmd, Cmd::Current));
}

#[test]
fn cli_list_sections_is_stateless() {
    use rmdv::cli::Stateless;
    let p = parse_from(["rmdv", "list-sections", "tests/fixtures/sections.md"]).unwrap();
    match p {
        ParsedCli::Stateless(Stateless::ListSections {
            file,
            pretty: false,
        }) => {
            assert_eq!(file, std::path::PathBuf::from("tests/fixtures/sections.md"));
        }
        other => panic!("expected stateless ListSections, got {other:?}"),
    }
}

#[test]
fn cli_no_args_is_empty() {
    let p = parse_from(["rmdv"]).unwrap();
    assert!(matches!(p, ParsedCli::Empty));
}

use rmdv::ipc::sections::{list_sections, resolve_section_path, Section};

#[test]
fn list_sections_builds_paths_and_lines() {
    let src = std::fs::read_to_string("tests/fixtures/sections.md").unwrap();
    let sections = list_sections(&src);
    let by_path: Vec<(&str, u32, u8)> = sections
        .iter()
        .map(|s| (s.path.as_str(), s.line, s.level))
        .collect();
    assert!(by_path.contains(&("Foo", 1, 1)), "got {by_path:?}");
    assert!(by_path.contains(&("Foo/Install", 5, 2)), "got {by_path:?}");
    assert!(
        by_path.contains(&("Foo/Install/Setup", 9, 3)),
        "got {by_path:?}"
    );
    assert!(by_path.contains(&("Foo/Usage", 13, 2)), "got {by_path:?}");
    assert!(
        by_path.contains(&("Foo/Usage/Setup", 17, 3)),
        "got {by_path:?}"
    );
}

#[test]
fn resolve_section_bare_title_first_match() {
    let src = std::fs::read_to_string("tests/fixtures/sections.md").unwrap();
    let sections = list_sections(&src);
    let s = resolve_section_path("Setup", &sections).unwrap();
    assert_eq!(s.path, "Foo/Install/Setup", "first match should win");
}

#[test]
fn resolve_section_full_path_disambiguates() {
    let src = std::fs::read_to_string("tests/fixtures/sections.md").unwrap();
    let sections = list_sections(&src);
    let s = resolve_section_path("Usage/Setup", &sections).unwrap();
    assert_eq!(s.path, "Foo/Usage/Setup");
}

#[test]
fn resolve_section_suffix_path() {
    let src = std::fs::read_to_string("tests/fixtures/sections.md").unwrap();
    let sections = list_sections(&src);
    let s = resolve_section_path("Install/Setup", &sections).unwrap();
    assert_eq!(s.path, "Foo/Install/Setup");
}

#[test]
fn resolve_section_missing_returns_none() {
    let src = std::fs::read_to_string("tests/fixtures/sections.md").unwrap();
    let sections = list_sections(&src);
    assert!(resolve_section_path("Nope", &sections).is_none());
}

use rmdv::app::{is_external_link, line_for_fragment, slugify};
use rmdv::terminal::{TerminalEnvironment, TerminalSignals};

#[test]
fn slugify_matches_github_style() {
    assert_eq!(slugify("Hello World"), "hello-world");
    assert_eq!(slugify("Getting Started"), "getting-started");
    assert_eq!(slugify("API_Reference"), "api-reference");
    // Runs of separators (here ` & ` with the `&` dropped) collapse to one `-`
    // so headings match hand-written anchors with incidental extra spacing.
    assert_eq!(slugify("Install & Setup!"), "install-setup");
    assert_eq!(slugify("Results  Discussion"), "results-discussion");
    assert_eq!(slugify("  Trim Me  "), "trim-me");
}

#[test]
fn external_link_detection() {
    assert!(is_external_link("https://example.com"));
    assert!(is_external_link("http://example.com"));
    assert!(is_external_link("mailto:a@b.com"));
    assert!(is_external_link("ftp://host/file"));
    assert!(!is_external_link("other.md"));
    assert!(!is_external_link("../docs/guide.md#setup"));
    assert!(!is_external_link("#section"));
}

#[test]
fn fragment_resolves_to_heading_line() {
    let src = std::fs::read_to_string("tests/fixtures/sections.md").unwrap();
    assert_eq!(line_for_fragment(&src, "install", false), Some(5));
    assert_eq!(line_for_fragment(&src, "usage", false), Some(13));
    // First matching heading wins (two "Setup" headings).
    assert_eq!(line_for_fragment(&src, "setup", false), Some(9));
    assert_eq!(line_for_fragment(&src, "nope", false), None);
}

#[test]
fn socket_path_is_user_scoped() {
    let p = rmdv::ipc::socket::default_path();
    let s = p.to_string_lossy();
    #[cfg(unix)]
    assert!(
        s.contains(&format!("rmdv-{}", unsafe { libc::getuid() })),
        "got {s}"
    );
    #[cfg(windows)]
    assert!(s.to_lowercase().contains("rmdv"), "got {s}");
}

#[test]
fn terminal_environment_detection_handles_shell_tmux_and_pipes() {
    let normal = TerminalEnvironment::from_signals(TerminalSignals {
        term: Some("xterm-256color"),
        stdin_is_tty: true,
        stdout_is_tty: true,
        stderr_is_tty: true,
        ..Default::default()
    });
    assert!(!normal.is_tmux());
    assert!(normal.is_interactive());

    let nested_tmux = TerminalEnvironment::from_signals(TerminalSignals {
        tmux: Some("/tmp/tmux-501/default,1234,2"),
        term: Some("screen-256color"),
        stdin_is_tty: true,
        stdout_is_tty: true,
        stderr_is_tty: true,
        ..Default::default()
    });
    assert!(nested_tmux.is_tmux());

    let malformed = TerminalEnvironment::from_signals(TerminalSignals {
        tmux: Some("malformed"),
        term: Some("xterm-256color"),
        ..Default::default()
    });
    assert!(!malformed.is_tmux());
    assert!(!malformed.is_interactive());
}

#[cfg(unix)]
fn ipc_test_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("rmdv-ipc-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A failed compatibility alias must not make the stable endpoint this process
/// just bound look like another running instance.
#[cfg(unix)]
#[tokio::test]
async fn acquire_keeps_stable_endpoint_when_alias_bind_fails() {
    let dir = ipc_test_dir("alias");
    let stable = dir.join("stable.sock");
    let unusable_alias = dir.join("missing").join("alias.sock");
    let listeners = rmdv::ipc::server::acquire_paths(&[stable.clone(), unusable_alias], false)
        .expect("stable endpoint should be acquired");
    assert_eq!(listeners.len(), 1);
    assert!(std::os::unix::net::UnixStream::connect(&stable).is_ok());
    drop(listeners);
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[tokio::test]
async fn acquire_reclaims_a_stale_socket_file() {
    let dir = ipc_test_dir("stale");
    let path = dir.join("stale.sock");
    let _ = std::fs::remove_file(&path);
    drop(std::os::unix::net::UnixListener::bind(&path).unwrap());
    assert!(path.exists(), "std listener leaves its socket file behind");
    let listeners = rmdv::ipc::server::acquire_paths(&[path.clone()], false)
        .expect("stale socket should be reclaimed");
    assert_eq!(listeners.len(), 1);
    drop(listeners);
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[tokio::test]
async fn acquire_never_displaces_a_live_listener() {
    let dir = ipc_test_dir("live");
    let path = dir.join("live.sock");
    let _ = std::fs::remove_file(&path);
    let live = std::os::unix::net::UnixListener::bind(&path).unwrap();
    assert!(rmdv::ipc::server::acquire_paths(&[path.clone()], false).is_err());
    assert!(std::os::unix::net::UnixStream::connect(&path).is_ok());
    drop(live);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn read_request_parses_one_line() {
    let line = b"{\"id\":7,\"cmd\":\"focus\"}\n";
    let req = rmdv::ipc::server::read_request(&line[..]).await.unwrap();
    assert_eq!(req.id, 7);
}

#[tokio::test]
async fn read_request_rejects_an_oversized_line() {
    let huge = vec![b'x'; rmdv::ipc::server::MAX_REQUEST_BYTES as usize + 16];
    let error = rmdv::ipc::server::read_request(&huge[..])
        .await
        .expect_err("a line past the cap must be refused");
    assert!(error.to_string().contains("exceeds"), "{error}");
}

/// A client that connects and never sends a line must not stall later clients.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn silent_connection_does_not_block_other_clients() {
    use futures::StreamExt;
    use std::io::{BufRead, BufReader, Write};

    let dir = ipc_test_dir("silent");
    let path = dir.join("silent.sock");
    let _ = std::fs::remove_file(&path);
    let listeners = rmdv::ipc::server::acquire_paths(&[path.clone()], false).unwrap();
    let (tx, mut rx) = futures::channel::mpsc::channel(8);
    tokio::spawn(rmdv::ipc::server::run(listeners, tx));
    tokio::spawn(async move {
        while let Some((req, reply)) = rx.next().await {
            let req: Request = req;
            let _ = reply.send(Response::ok(req.id));
        }
    });

    let silent = std::os::unix::net::UnixStream::connect(&path).unwrap();
    let client_path = path.clone();
    let reply = tokio::task::spawn_blocking(move || {
        let mut stream = std::os::unix::net::UnixStream::connect(&client_path).unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .unwrap();
        stream.write_all(b"{\"id\":9,\"cmd\":\"focus\"}\n").unwrap();
        stream.shutdown(std::net::Shutdown::Write).unwrap();
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line).map(|_| line)
    })
    .await
    .unwrap()
    .expect("second client must be answered while the first stays silent");
    let resp: Response = serde_json::from_str(reply.trim_end()).unwrap();
    assert_eq!(resp.id, 9);
    assert!(resp.ok);
    drop(silent);
    let _ = std::fs::remove_dir_all(&dir);
}
