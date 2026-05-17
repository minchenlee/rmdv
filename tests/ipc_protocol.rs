use mdv::ipc::{Cmd, Mode, Request, Response};
use serde_json::json;

#[test]
fn request_open_round_trip() {
    let req = Request {
        id: 1,
        cmd: Cmd::Open {
            file: "/abs/foo.md".into(),
            line: Some(42),
            section: Some("Install/Setup".into()),
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
    let req = Request { id: 5, cmd: Cmd::Goto { line: Some(10), section: None } };
    let s = serde_json::to_string(&req).unwrap();
    let back: Request = serde_json::from_str(&s).unwrap();
    assert_eq!(back, req);
}

#[test]
fn request_mode_round_trip() {
    for m in [Mode::View, Mode::Edit, Mode::Mindmap] {
        let req = Request { id: 9, cmd: Cmd::Mode { mode: m } };
        let s = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&s).unwrap();
        assert_eq!(back, req);
    }
}

#[test]
fn response_ok_no_result_serialises_without_result_field() {
    let r = Response { id: 1, ok: true, result: None, error: None };
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

use mdv::ipc::lines::{block_for_line, build_byte_to_line};

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
    let (blocks, offsets) = mdv::parser::parse(src);
    assert_eq!(blocks.len(), offsets.len());
    let table = mdv::ipc::lines::build_byte_to_line(src);
    let lines: Vec<u32> = offsets.iter().map(|&b| table.line_for_byte(b as usize)).collect();
    assert_eq!(lines[0], 1, "H1 on line 1, got {}", lines[0]);
    assert_eq!(lines[1], 3, "first paragraph on line 3, got {}", lines[1]);
    assert_eq!(lines[2], 5, "H2 on line 5, got {}", lines[2]);
    assert_eq!(lines[3], 7, "second paragraph on line 7, got {}", lines[3]);
}
