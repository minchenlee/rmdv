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
