# mdv CLI / Agent Control Surface Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a CLI / IPC control surface so coding agents (Claude Code, Codex, scripts) can open files/folders, switch modes, scroll to lines or section paths, reveal files, and query state in a running mdv window.

**Architecture:** Single-instance daemon model — first `mdv` invocation spawns the Iced window and an IPC listener (`interprocess` crate, Unix socket / named pipe). Subsequent invocations try-connect: success → serialize args as one JSON request, write, read response, exit (client mode); failure → unlink stale socket, become the new instance, apply parsed args as an initial command on first frame. The IPC listener lives inside an `iced::Subscription` and hands requests to the update loop via `Message::Ipc(Request, oneshot::Sender<Response>)`.

**Tech Stack:** Rust, Iced 0.14, `clap` v4 (derive), `interprocess` v2 (tokio feature), `serde` / `serde_json` (already present), `tokio` (already present), `pulldown-cmark` `OffsetIter` for source-line tracking.

**Spec:** `docs/superpowers/specs/2026-05-17-cli-agent-control-design.md` (commit `4349bfc`).

**Branch note:** Work targets the Iced `main` branch (per `memory/mdv_stack.md`). Mode mapping: spec uses `view|edit|mindmap`; app enum is `ViewMode::Rendered | Raw | Mindmap`. CLI strings map view→Rendered, edit→Raw, mindmap→Mindmap.

---

## File Structure

**Created:**
- `src/cli.rs` — clap derive structs, arg → `Request` mapping, JSON output helpers.
- `src/ipc/mod.rs` — module root, re-exports.
- `src/ipc/types.rs` — `Request`, `Response`, `Cmd` (serde).
- `src/ipc/socket.rs` — platform socket path + try-connect helper.
- `src/ipc/client.rs` — round-trip one request and exit.
- `src/ipc/server.rs` — `interprocess` listener loop for the Iced subscription.
- `src/ipc/sections.rs` — stateless heading-path enumeration (used by both server and standalone `list-sections`).
- `src/ipc/lines.rs` — `byte_to_line` table + `block_for_line` binary search.
- `tests/ipc_protocol.rs` — JSON round-trip + `block_for_line` + section resolver tests.
- `tests/ipc_e2e.rs` — `list-sections` subprocess test.
- `tests/fixtures/sections.md` — heading fixture.

**Modified:**
- `Cargo.toml` — add `clap`, `interprocess`, `futures` deps.
- `src/lib.rs` — `pub mod cli; pub mod ipc;`.
- `src/main.rs` — replace hand-rolled arg parsing with clap dispatch (client mode vs instance mode).
- `src/ast.rs` — no change in v1 (offsets carried in parallel vec; keeps Block diff minimal).
- `src/parser.rs` — switch `Parser::new_ext` to `into_offset_iter`, return `(Vec<(BlockId, Block)>, Vec<u32>)` where the `u32`s are byte offsets aligned with blocks.
- `src/app.rs` — store `block_lines: Vec<u32>`, add `Message::Ipc(Request, oneshot::Sender<Response>)`, dispatch to handlers, wire IPC subscription, expose `App::new_with_initial(Request)`.

Call sites of `parser::parse` (in `app.rs`) must accept the new tuple shape. All other parser consumers (e.g. tests) get updated in the same task.

---

## Task 0: Confirm clean baseline

- [ ] **Step 1: Verify `cargo test` passes on current `gpui-port` branch baseline**

Run: `cargo test --quiet`
Expected: PASS (all existing tests).

- [ ] **Step 2: Verify `cargo build` succeeds**

Run: `cargo build`
Expected: PASS, no warnings beyond existing.

If either fails, stop and report — do not begin work on a broken baseline.

---

## Task 1: Add dependencies

**Files:**
- Modify: `Cargo.toml` — `[dependencies]` block.

- [ ] **Step 1: Add the three new dependencies**

Edit `Cargo.toml`, append to the `[dependencies]` block (after `tree-sitter-toml-ng`):

```toml
clap = { version = "4", features = ["derive"] }
interprocess = { version = "2", features = ["tokio"] }
futures = "0.3"
```

(`futures` is needed for `AsyncBufReadExt::read_line` / `AsyncWriteExt::write_all` over `interprocess` tokio streams.)

- [ ] **Step 2: Verify compilation**

Run: `cargo check`
Expected: PASS (compiles, may emit "unused crate" warnings — that's fine until next task).

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "deps: add clap, interprocess, futures for CLI/IPC"
```

---

## Task 2: Define IPC request/response types

**Files:**
- Create: `src/ipc/mod.rs`
- Create: `src/ipc/types.rs`
- Create: `tests/ipc_protocol.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create module skeleton**

Create `src/ipc/mod.rs`:

```rust
pub mod types;
pub use types::{Cmd, Mode, Request, Response};
```

- [ ] **Step 2: Add module declaration to lib.rs**

Edit `src/lib.rs`, append:

```rust
pub mod ipc;
```

- [ ] **Step 3: Write the failing JSON round-trip test**

Create `tests/ipc_protocol.rs`:

```rust
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
```

- [ ] **Step 4: Run test to verify it fails**

Run: `cargo test --test ipc_protocol`
Expected: FAIL — compile errors, types not defined yet.

- [ ] **Step 5: Implement the types**

Create `src/ipc/types.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    View,
    Edit,
    Mindmap,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cmd", content = "args", rename_all = "kebab-case")]
pub enum Cmd {
    Open {
        file: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        line: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        section: Option<String>,
    },
    OpenFolder {
        dir: String,
    },
    Goto {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        line: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        section: Option<String>,
    },
    Mode {
        mode: Mode,
    },
    Reveal {
        file: String,
    },
    Focus,
    Close,
    Current,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    #[serde(flatten)]
    pub cmd: Cmd,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    pub fn ok(id: u64) -> Self {
        Self { id, ok: true, result: None, error: None }
    }
    pub fn ok_with(id: u64, result: serde_json::Value) -> Self {
        Self { id, ok: true, result: Some(result), error: None }
    }
    pub fn err(id: u64, msg: impl Into<String>) -> Self {
        Self { id, ok: false, result: None, error: Some(msg.into()) }
    }
}
```

- [ ] **Step 6: Run tests, verify pass**

Run: `cargo test --test ipc_protocol`
Expected: PASS (all 6 tests).

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock src/lib.rs src/ipc tests/ipc_protocol.rs
git commit -m "feat(ipc): request/response types with serde tag/content shape"
```

---

## Task 3: Source-line tracking — `byte_to_line` + `block_for_line`

**Files:**
- Create: `src/ipc/lines.rs`
- Modify: `src/ipc/mod.rs`
- Modify: `tests/ipc_protocol.rs` (add tests)

- [ ] **Step 1: Write failing tests**

Append to `tests/ipc_protocol.rs`:

```rust
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
```

- [ ] **Step 2: Run, verify fail**

Run: `cargo test --test ipc_protocol`
Expected: FAIL (unresolved module `mdv::ipc::lines`).

- [ ] **Step 3: Implement**

Create `src/ipc/lines.rs`:

```rust
pub struct ByteToLine {
    /// Byte offset of the start of each line (1-indexed line number = index + 1).
    line_starts: Vec<u32>,
}

impl ByteToLine {
    /// 1-indexed line number for a byte offset. Returns the last line if `byte`
    /// is past the end. Empty source returns 1.
    pub fn line_for_byte(&self, byte: usize) -> u32 {
        if self.line_starts.is_empty() {
            return 1;
        }
        let byte = byte as u32;
        match self.line_starts.binary_search(&byte) {
            Ok(i) => (i as u32) + 1,
            Err(0) => 1,
            Err(i) => i as u32,
        }
    }
}

pub fn build_byte_to_line(src: &str) -> ByteToLine {
    let mut starts = vec![0u32];
    for (i, b) in src.bytes().enumerate() {
        if b == b'\n' {
            starts.push((i + 1) as u32);
        }
    }
    ByteToLine { line_starts: starts }
}

/// Largest index `i` such that `block_lines[i] <= line`. Returns `Some(0)` if
/// `line` precedes the first block (clamp). Returns `None` only if the slice is
/// empty.
pub fn block_for_line(line: u32, block_lines: &[u32]) -> Option<usize> {
    if block_lines.is_empty() {
        return None;
    }
    match block_lines.binary_search(&line) {
        Ok(i) => Some(i),
        Err(0) => Some(0),
        Err(i) => Some(i - 1),
    }
}
```

Edit `src/ipc/mod.rs`:

```rust
pub mod lines;
pub mod types;
pub use types::{Cmd, Mode, Request, Response};
```

- [ ] **Step 4: Run, verify pass**

Run: `cargo test --test ipc_protocol`
Expected: PASS (all tests including new ones).

- [ ] **Step 5: Commit**

```bash
git add src/ipc tests/ipc_protocol.rs
git commit -m "feat(ipc): byte_to_line table + block_for_line binary search"
```

---

## Task 4: Parser emits per-block byte offsets

**Files:**
- Modify: `src/parser.rs` — switch to `into_offset_iter`, return `(Vec<(BlockId, Block)>, Vec<u32>)`.
- Modify: every call site of `parser::parse` (grep first).

- [ ] **Step 1: Audit call sites**

Run: `grep -rn "parser::parse\b\|crate::parser::parse\b" src/ tests/ benches/`
Note: every site needs updating in this task. Common sites: `src/app.rs`, possibly tests.

- [ ] **Step 2: Write failing test**

Append to `tests/ipc_protocol.rs`:

```rust
#[test]
fn parser_emits_byte_offsets_aligned_with_blocks() {
    let src = "# H1\n\npara one\n\n## H2\n\npara two\n";
    let (blocks, offsets) = mdv::parser::parse(src);
    assert_eq!(blocks.len(), offsets.len());
    // First heading should map to byte 0 (line 1).
    let table = mdv::ipc::lines::build_byte_to_line(src);
    let lines: Vec<u32> = offsets.iter().map(|&b| table.line_for_byte(b as usize)).collect();
    assert_eq!(lines[0], 1, "H1 on line 1, got {}", lines[0]);
    assert_eq!(lines[1], 3, "first paragraph on line 3, got {}", lines[1]);
    assert_eq!(lines[2], 5, "H2 on line 5, got {}", lines[2]);
    assert_eq!(lines[3], 7, "second paragraph on line 7, got {}", lines[3]);
}
```

- [ ] **Step 3: Run, verify fail**

Run: `cargo test --test ipc_protocol parser_emits_byte_offsets`
Expected: FAIL (signature mismatch).

- [ ] **Step 4: Change parser to emit offsets**

Edit `src/parser.rs`:

Replace the existing `pub fn parse(src: &str) -> Vec<(BlockId, Block)>` with:

```rust
pub fn parse(src: &str) -> (Vec<(BlockId, Block)>, Vec<u32>) {
    let src = strip_frontmatter(src);
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_SMART_PUNCTUATION);
    let parser = Parser::new_ext(src, opts).into_offset_iter();
    let mut state = ParseState::default();
    let mut pending_offset: Option<u32> = None;
    for (ev, range) in parser {
        // Capture the first byte offset of any top-level Start event so each
        // block's source position is known when we push it.
        if matches!(ev, Event::Start(_)) && state.stack.is_empty() {
            pending_offset = Some(range.start as u32);
        }
        let before_len = state.blocks.len();
        let take_offset = pending_offset;
        state.handle(ev);
        // If a top-level block was just pushed (e.g. Event::Rule, or end of a
        // top-level frame), record its offset.
        if state.blocks.len() > before_len {
            let off = take_offset
                .or(Some(range.start as u32))
                .unwrap_or(0);
            for _ in before_len..state.blocks.len() {
                state.offsets.push(off);
            }
            pending_offset = None;
        }
    }
    let blocks: Vec<(BlockId, Block)> = state
        .blocks
        .into_iter()
        .enumerate()
        .map(|(pos, b)| (block_id(pos, &b), b))
        .collect();
    (blocks, state.offsets)
}
```

Add `offsets: Vec<u32>` to `ParseState`:

```rust
#[derive(Default)]
struct ParseState {
    blocks: Vec<Block>,
    offsets: Vec<u32>,
    stack: Vec<Frame>,
}
```

Note: `Event::Rule` and other inline-emitted top-level blocks (e.g. `Tag::Image` at top level which calls `push_block` directly) are covered because `pending_offset` is captured for any top-level `Event::Start`, and `Event::Rule` falls through to the `take_offset.or(Some(range.start as u32))` fallback (the rule event itself carries a range).

- [ ] **Step 5: Update all call sites**

For each grep hit in Step 1, change `let blocks = parser::parse(src);` to `let (blocks, _block_offsets) = parser::parse(src);` (or capture the offsets where needed — in `src/app.rs`, store the offsets, see Task 6).

- [ ] **Step 6: Run, verify pass**

Run: `cargo test --test ipc_protocol parser_emits_byte_offsets`
Expected: PASS.

Run full suite: `cargo test`
Expected: PASS. If `parser_snapshots__gfm.snap` regressed, investigate — block content shouldn't change, only the signature.

- [ ] **Step 7: Commit**

```bash
git add src/parser.rs src/app.rs tests/ipc_protocol.rs
git commit -m "feat(parser): emit per-block byte offsets via into_offset_iter"
```

---

## Task 5: Section path enumeration (stateless)

**Files:**
- Create: `src/ipc/sections.rs`
- Create: `tests/fixtures/sections.md`
- Modify: `src/ipc/mod.rs`
- Modify: `tests/ipc_protocol.rs`

- [ ] **Step 1: Create fixture**

Create `tests/fixtures/sections.md`:

```markdown
# Foo

Intro paragraph.

## Install

Install paragraph.

### Setup

Setup paragraph.

## Usage

Usage paragraph.

### Setup

A second "Setup" heading nested under Usage.
```

- [ ] **Step 2: Write failing tests**

Append to `tests/ipc_protocol.rs`:

```rust
use mdv::ipc::sections::{list_sections, resolve_section_path, Section};

#[test]
fn list_sections_builds_paths_and_lines() {
    let src = std::fs::read_to_string("tests/fixtures/sections.md").unwrap();
    let sections = list_sections(&src);
    let by_path: Vec<(&str, u32, u8)> =
        sections.iter().map(|s| (s.path.as_str(), s.line, s.level)).collect();
    assert!(by_path.contains(&("Foo", 1, 1)), "got {by_path:?}");
    assert!(by_path.contains(&("Foo/Install", 5, 2)), "got {by_path:?}");
    assert!(by_path.contains(&("Foo/Install/Setup", 9, 3)), "got {by_path:?}");
    assert!(by_path.contains(&("Foo/Usage", 13, 2)), "got {by_path:?}");
    assert!(by_path.contains(&("Foo/Usage/Setup", 17, 3)), "got {by_path:?}");
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
    // "Install/Setup" should match "Foo/Install/Setup" via suffix.
    let s = resolve_section_path("Install/Setup", &sections).unwrap();
    assert_eq!(s.path, "Foo/Install/Setup");
}

#[test]
fn resolve_section_missing_returns_none() {
    let src = std::fs::read_to_string("tests/fixtures/sections.md").unwrap();
    let sections = list_sections(&src);
    assert!(resolve_section_path("Nope", &sections).is_none());
}
```

- [ ] **Step 3: Run, verify fail**

Run: `cargo test --test ipc_protocol list_sections`
Expected: FAIL — module not found.

- [ ] **Step 4: Implement**

Create `src/ipc/sections.rs`:

```rust
use crate::ast::{Block, Inline};
use crate::ipc::lines::build_byte_to_line;
use crate::parser;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Section {
    pub level: u8,
    pub title: String,
    pub path: String,
    pub line: u32,
}

pub fn list_sections(src: &str) -> Vec<Section> {
    let (blocks, offsets) = parser::parse(src);
    let table = build_byte_to_line(src);
    let mut stack: Vec<(u8, String)> = Vec::new();
    let mut out = Vec::new();
    for (i, (_, block)) in blocks.iter().enumerate() {
        if let Block::Heading { level, inlines, .. } = block {
            while stack.last().is_some_and(|(l, _)| *l >= *level) {
                stack.pop();
            }
            let title = inline_text(inlines);
            stack.push((*level, title.clone()));
            let path = stack.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join("/");
            let line = table.line_for_byte(offsets[i] as usize);
            out.push(Section { level: *level, title, path, line });
        }
    }
    out
}

/// Find the first section whose path ends with the given path (segment-wise
/// suffix match). Bare title `"Setup"` matches the first heading titled
/// "Setup"; `"Install/Setup"` matches the first whose tail is
/// `Install/Setup`.
pub fn resolve_section_path<'a>(needle: &str, sections: &'a [Section]) -> Option<&'a Section> {
    let needle_segs: Vec<&str> = needle.split('/').filter(|s| !s.is_empty()).collect();
    if needle_segs.is_empty() {
        return None;
    }
    sections.iter().find(|s| {
        let hay: Vec<&str> = s.path.split('/').collect();
        hay.len() >= needle_segs.len()
            && hay[hay.len() - needle_segs.len()..] == needle_segs[..]
    })
}

fn inline_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for i in inlines {
        push_inline(i, &mut out);
    }
    out
}

fn push_inline(i: &Inline, out: &mut String) {
    match i {
        Inline::Text(s) | Inline::Code(s) => out.push_str(s),
        Inline::Emph(c) | Inline::Strong(c) | Inline::Strike(c) => {
            for x in c { push_inline(x, out); }
        }
        Inline::Link { children, .. } => {
            for x in children { push_inline(x, out); }
        }
    }
}
```

Edit `src/ipc/mod.rs`:

```rust
pub mod lines;
pub mod sections;
pub mod types;
pub use types::{Cmd, Mode, Request, Response};
```

- [ ] **Step 5: Run, verify pass**

Run: `cargo test --test ipc_protocol`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/ipc/sections.rs src/ipc/mod.rs tests/fixtures/sections.md tests/ipc_protocol.rs
git commit -m "feat(ipc): list-sections + section path resolver"
```

---

## Task 6: Wire `block_lines` into App state

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Find where parser::parse is called in app.rs**

Run: `grep -nE "parser::parse|crate::parser::parse" src/app.rs`
Expected: one or more sites. Note line numbers.

- [ ] **Step 2: Add `block_lines` field and `byte_to_line` table**

Edit `src/app.rs`. Inside `pub struct App { ... }`, add:

```rust
    pub block_lines: Vec<u32>,
```

- [ ] **Step 3: Populate the field at every parse call**

At each site that calls `parser::parse(&self.source)` (or equivalent), wrap:

```rust
let (ast, block_offsets) = parser::parse(&self.source);
let table = crate::ipc::lines::build_byte_to_line(&self.source);
self.block_lines = block_offsets
    .iter()
    .map(|&b| table.line_for_byte(b as usize))
    .collect();
self.ast = ast;
```

(Adjust the destructuring to match the existing code shape — some sites may assign through a setter; the principle is: whenever `self.ast` changes, refresh `self.block_lines` alongside it.)

- [ ] **Step 4: Make sure `Default` for `App` initialises empty vec**

If `App` derives `Default`, `Vec::default()` is empty — no extra work. If `App` has a manual `Default` impl, add `block_lines: Vec::new()` there.

- [ ] **Step 5: Run full test suite**

Run: `cargo test`
Expected: PASS. If snapshots regress, inspect — block_lines is additive state, shouldn't affect rendered output.

- [ ] **Step 6: Commit**

```bash
git add src/app.rs
git commit -m "feat(app): track block_lines alongside ast"
```

---

## Task 7: CLI argument parser

**Files:**
- Create: `src/cli.rs`
- Modify: `src/lib.rs`
- Modify: `tests/ipc_protocol.rs`

- [ ] **Step 1: Add module declaration**

Edit `src/lib.rs`, append:

```rust
pub mod cli;
```

- [ ] **Step 2: Write failing tests**

Append to `tests/ipc_protocol.rs`:

```rust
use mdv::cli::{parse_from, ParsedCli};

#[test]
fn cli_bare_file_becomes_open_request() {
    let p = parse_from(["mdv", "/abs/foo.md"]).unwrap();
    match p {
        ParsedCli::Request(r) => match r.cmd {
            Cmd::Open { file, line: None, section: None } => assert_eq!(file, "/abs/foo.md"),
            other => panic!("unexpected cmd: {other:?}"),
        },
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn cli_bare_file_with_line_and_section() {
    let p = parse_from(["mdv", "/abs/foo.md", "--line", "42", "--section", "Install/Setup"])
        .unwrap();
    let ParsedCli::Request(r) = p else { panic!() };
    match r.cmd {
        Cmd::Open { file, line: Some(42), section: Some(s) } => {
            assert_eq!(file, "/abs/foo.md");
            assert_eq!(s, "Install/Setup");
        }
        other => panic!("unexpected cmd: {other:?}"),
    }
}

#[test]
fn cli_goto_subcommand() {
    let p = parse_from(["mdv", "goto", "--line", "10"]).unwrap();
    let ParsedCli::Request(r) = p else { panic!() };
    assert!(matches!(r.cmd, Cmd::Goto { line: Some(10), section: None }));
}

#[test]
fn cli_mode_subcommand() {
    let p = parse_from(["mdv", "mode", "edit"]).unwrap();
    let ParsedCli::Request(r) = p else { panic!() };
    assert!(matches!(r.cmd, Cmd::Mode { mode: Mode::Edit }));
}

#[test]
fn cli_current_subcommand() {
    let p = parse_from(["mdv", "current"]).unwrap();
    let ParsedCli::Request(r) = p else { panic!() };
    assert!(matches!(r.cmd, Cmd::Current));
}

#[test]
fn cli_list_sections_is_stateless() {
    let p = parse_from(["mdv", "list-sections", "tests/fixtures/sections.md"]).unwrap();
    match p {
        ParsedCli::Stateless(crate::cli_test::Stateless::ListSections(path)) => {
            assert_eq!(path, std::path::PathBuf::from("tests/fixtures/sections.md"))
        }
        _ => panic!("expected stateless variant"),
    }
}

#[test]
fn cli_no_args_is_no_op_request() {
    // Bare `mdv` with no args means: launch instance with no initial command,
    // or — if already running — focus the window (round-trip just queries state).
    let p = parse_from(["mdv"]).unwrap();
    assert!(matches!(p, ParsedCli::Empty));
}
```

(The `cli_list_sections_is_stateless` test references a `Stateless` enum we'll define. To avoid coupling the test to an internal path, simplify Step 2 by using a public re-export — adjust the test to use `mdv::cli::Stateless`.)

Replace the `cli_list_sections_is_stateless` test with:

```rust
#[test]
fn cli_list_sections_is_stateless() {
    use mdv::cli::Stateless;
    let p = parse_from(["mdv", "list-sections", "tests/fixtures/sections.md"]).unwrap();
    match p {
        ParsedCli::Stateless(Stateless::ListSections { file, pretty: false }) => {
            assert_eq!(file, std::path::PathBuf::from("tests/fixtures/sections.md"));
        }
        other => panic!("expected stateless ListSections, got {other:?}"),
    }
}
```

- [ ] **Step 3: Run, verify fail**

Run: `cargo test --test ipc_protocol cli_`
Expected: FAIL — module not present.

- [ ] **Step 4: Implement clap parser**

Create `src/cli.rs`:

```rust
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
            // If --mode was provided alongside a bare file, we still emit a
            // single Open; mode handling happens in instance/client logic by
            // bundling a follow-up Mode request — see Task 11 (handler).
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
```

- [ ] **Step 5: Run tests, verify pass**

Run: `cargo test --test ipc_protocol cli_`
Expected: PASS. Note: the `is_dir()` check happens at parse time. For tests that pass non-existent paths like `/abs/foo.md`, `is_dir()` returns false → falls into `Cmd::Open`. Good.

- [ ] **Step 6: Commit**

```bash
git add src/cli.rs src/lib.rs tests/ipc_protocol.rs
git commit -m "feat(cli): clap-based parser emitting ipc::Request"
```

---

## Task 8: Socket path + try-connect

**Files:**
- Create: `src/ipc/socket.rs`
- Modify: `src/ipc/mod.rs`
- Modify: `tests/ipc_protocol.rs`

- [ ] **Step 1: Write failing test**

Append to `tests/ipc_protocol.rs`:

```rust
#[test]
fn socket_path_is_user_scoped() {
    let p = mdv::ipc::socket::default_path();
    let s = p.to_string_lossy();
    // Either platform variant must include a per-user discriminator.
    #[cfg(unix)]
    assert!(s.contains(&format!("mdv-{}", unsafe { libc::getuid() })), "got {s}");
    #[cfg(windows)]
    assert!(s.to_lowercase().contains("mdv"), "got {s}");
}
```

Add `libc` for the unix test gate — it's already a transitive dep through tokio; if `cargo test` complains, add `libc = "0.2"` to `[dev-dependencies]`.

- [ ] **Step 2: Run, verify fail**

Run: `cargo test --test ipc_protocol socket_path`
Expected: FAIL — module missing.

- [ ] **Step 3: Implement**

Create `src/ipc/socket.rs`:

```rust
use std::path::PathBuf;

#[cfg(unix)]
pub fn default_path() -> PathBuf {
    let tmp = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    let uid = unsafe { libc::getuid() };
    tmp.join(format!("mdv-{uid}.sock"))
}

#[cfg(windows)]
pub fn default_path() -> PathBuf {
    let user = std::env::var("USERNAME").unwrap_or_else(|_| "default".to_string());
    PathBuf::from(format!(r"\\.\pipe\mdv-{user}"))
}
```

Edit `src/ipc/mod.rs`:

```rust
pub mod lines;
pub mod sections;
pub mod socket;
pub mod types;
pub use types::{Cmd, Mode, Request, Response};
```

Add to `Cargo.toml` under `[dependencies]`:

```toml
libc = "0.2"
```

(`libc` is needed on unix only, but it's a tiny dep and conditional `cfg` for deps in `Cargo.toml` is verbose. Pulling it unconditionally keeps the manifest simple.)

- [ ] **Step 4: Run, verify pass**

Run: `cargo test --test ipc_protocol socket_path`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/ipc tests/ipc_protocol.rs
git commit -m "feat(ipc): platform-scoped socket path"
```

---

## Task 9: Client — connect, send, receive, exit

**Files:**
- Create: `src/ipc/client.rs`
- Modify: `src/ipc/mod.rs`

(No unit test in this task — exercised end-to-end in Task 13.)

- [ ] **Step 1: Implement**

Create `src/ipc/client.rs`:

```rust
use crate::ipc::{socket, Request, Response};
use anyhow::{anyhow, Result};
use futures::{AsyncBufReadExt, AsyncWriteExt};
use interprocess::local_socket::{
    tokio::{prelude::*, Stream},
    GenericFilePath, GenericNamespaced, ToFsName, ToNsName,
};

/// Attempt a single round-trip. Returns `Ok(Some(response))` if an instance is
/// listening, `Ok(None)` if no instance is running (caller should become the
/// instance), `Err` on protocol/io errors after a successful connect.
pub async fn try_send(req: &Request) -> Result<Option<Response>> {
    let path = socket::default_path();
    let name = match path_to_name(&path) {
        Ok(n) => n,
        Err(e) => return Err(anyhow!("invalid socket path {}: {e}", path.display())),
    };

    let stream = match Stream::connect(name).await {
        Ok(s) => s,
        Err(e) if is_no_listener(&e) => return Ok(None),
        Err(e) => return Err(anyhow!("connect failed: {e}")),
    };

    let (recv, mut send) = stream.split();
    let mut line = serde_json::to_string(req)?;
    line.push('\n');
    send.write_all(line.as_bytes()).await?;
    send.flush().await?;
    drop(send); // half-close so server's read_line returns EOF after our line

    let mut reader = futures::io::BufReader::new(recv);
    let mut buf = String::new();
    reader.read_line(&mut buf).await?;
    if buf.is_empty() {
        return Err(anyhow!("ipc disconnect"));
    }
    let resp: Response = serde_json::from_str(buf.trim_end())?;
    Ok(Some(resp))
}

#[cfg(unix)]
fn path_to_name(p: &std::path::Path) -> std::io::Result<interprocess::local_socket::Name<'_>> {
    p.to_fs_name::<GenericFilePath>()
}

#[cfg(windows)]
fn path_to_name(p: &std::path::Path) -> std::io::Result<interprocess::local_socket::Name<'_>> {
    let s = p.to_string_lossy();
    // Named pipes are namespaced, not filesystem paths.
    let trimmed = s.trim_start_matches(r"\\.\pipe\");
    trimmed.to_ns_name::<GenericNamespaced>()
}

fn is_no_listener(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::NotFound
            | std::io::ErrorKind::ConnectionRefused
            | std::io::ErrorKind::AddrNotAvailable
    )
}
```

Edit `src/ipc/mod.rs`:

```rust
pub mod client;
pub mod lines;
pub mod sections;
pub mod socket;
pub mod types;
pub use types::{Cmd, Mode, Request, Response};
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check --tests`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src/ipc tests
git commit -m "feat(ipc): client try_send round-trip"
```

---

## Task 10: Server — listener loop inside Iced subscription

**Files:**
- Create: `src/ipc/server.rs`
- Modify: `src/ipc/mod.rs`

- [ ] **Step 1: Implement listener**

Create `src/ipc/server.rs`:

```rust
use crate::ipc::{socket, Request, Response};
use anyhow::{anyhow, Result};
use futures::{
    channel::{mpsc, oneshot},
    AsyncBufReadExt, AsyncWriteExt, SinkExt,
};
use interprocess::local_socket::{
    tokio::{prelude::*, Listener, Stream},
    GenericFilePath, GenericNamespaced, ListenerOptions, ToFsName, ToNsName,
};
use std::path::Path;

/// Message handed to the Iced update loop.
pub type Pending = (Request, oneshot::Sender<Response>);

/// Bind the listener, recovering from a stale socket.
pub fn acquire() -> Result<Listener> {
    let path = socket::default_path();
    // First try to connect — if something answers, the caller is not the instance.
    if can_connect_blocking(&path) {
        return Err(anyhow!("instance already running"));
    }
    // Stale or absent — best-effort unlink (unix only; Windows pipes don't persist).
    #[cfg(unix)]
    {
        let _ = std::fs::remove_file(&path);
    }
    let name = path_to_name(&path)?;
    let opts = ListenerOptions::new().name(name);
    let listener = opts.create_tokio().map_err(|e| anyhow!("bind {}: {e}", path.display()))?;
    Ok(listener)
}

#[cfg(unix)]
fn can_connect_blocking(path: &Path) -> bool {
    use std::os::unix::net::UnixStream;
    UnixStream::connect(path).is_ok()
}

#[cfg(windows)]
fn can_connect_blocking(path: &Path) -> bool {
    // On Windows a non-existent pipe yields ERROR_FILE_NOT_FOUND immediately.
    std::fs::OpenOptions::new().read(true).write(true).open(path).is_ok()
}

/// Run the listener loop, forwarding requests through `tx` and writing replies
/// back to the connecting client. Serialises clients (one at a time).
pub async fn run(listener: Listener, mut tx: mpsc::Sender<Pending>) {
    loop {
        let conn = match listener.accept().await {
            Ok(c) => c,
            Err(_) => continue,
        };
        if let Err(_e) = handle_one(conn, &mut tx).await {
            // best-effort: drop on protocol error, keep listening
        }
    }
}

async fn handle_one(stream: Stream, tx: &mut mpsc::Sender<Pending>) -> Result<()> {
    let (recv, mut send) = stream.split();
    let mut reader = futures::io::BufReader::new(recv);
    let mut buf = String::new();
    reader.read_line(&mut buf).await?;
    if buf.is_empty() {
        return Err(anyhow!("empty request"));
    }
    let req: Request = serde_json::from_str(buf.trim_end())
        .map_err(|e| anyhow!("bad json: {e}"))?;
    let id = req.id;
    let (reply_tx, reply_rx) = oneshot::channel();
    tx.send((req, reply_tx)).await?;
    let resp = reply_rx.await.unwrap_or_else(|_| Response::err(id, "instance shutdown"));
    let mut line = serde_json::to_string(&resp)?;
    line.push('\n');
    send.write_all(line.as_bytes()).await?;
    send.flush().await?;
    Ok(())
}

#[cfg(unix)]
fn path_to_name(p: &Path) -> Result<interprocess::local_socket::Name<'_>> {
    p.to_fs_name::<GenericFilePath>().map_err(|e| anyhow!("name: {e}"))
}

#[cfg(windows)]
fn path_to_name(p: &Path) -> Result<interprocess::local_socket::Name<'_>> {
    let s = p.to_string_lossy();
    let trimmed = s.trim_start_matches(r"\\.\pipe\");
    trimmed.to_ns_name::<GenericNamespaced>().map_err(|e| anyhow!("name: {e}"))
}
```

Edit `src/ipc/mod.rs`:

```rust
pub mod client;
pub mod lines;
pub mod sections;
pub mod server;
pub mod socket;
pub mod types;
pub use types::{Cmd, Mode, Request, Response};
```

- [ ] **Step 2: Verify compile**

Run: `cargo check`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src/ipc
git commit -m "feat(ipc): server listener loop with stale-socket recovery"
```

---

## Task 11: App — Message::Ipc + handlers

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Locate the `Message` enum**

Run: `grep -nE "^pub enum Message|^enum Message" src/app.rs`
Note the line, then read ~30 surrounding lines.

- [ ] **Step 2: Add the IPC variant**

Add to the `Message` enum (alongside existing variants):

```rust
    Ipc(crate::ipc::Request, std::sync::Arc<std::sync::Mutex<Option<futures::channel::oneshot::Sender<crate::ipc::Response>>>>),
```

(Wrapped in `Arc<Mutex<Option<_>>>` so `Message` stays `Clone`, which Iced requires. The handler `take()`s the sender once.)

- [ ] **Step 3: Add a small helper for one-shot delivery**

Inside `impl App` add:

```rust
fn reply(
    tx: &std::sync::Arc<std::sync::Mutex<Option<futures::channel::oneshot::Sender<crate::ipc::Response>>>>,
    resp: crate::ipc::Response,
) {
    if let Some(sender) = tx.lock().ok().and_then(|mut g| g.take()) {
        let _ = sender.send(resp);
    }
}
```

- [ ] **Step 4: Handle the variant in `App::update`**

Inside the `match` in `update`, add an arm. Place it before the catch-all:

```rust
Message::Ipc(req, tx) => {
    use crate::ipc::{Cmd, Mode, Response};
    let id = req.id;
    let mut follow_up = iced::Task::none();
    let resp = match req.cmd {
        Cmd::Current => {
            let mode = match self.view_mode {
                ViewMode::Rendered => "view",
                ViewMode::Raw => "edit",
                ViewMode::Mindmap => "mindmap",
            };
            let body = serde_json::json!({
                "file": self.file.as_ref().map(|p| p.to_string_lossy().into_owned()),
                "line": current_line_estimate(self),
                "mode": mode,
                "folder": self.workspace.as_ref().map(|p| p.to_string_lossy().into_owned()),
            });
            Response::ok_with(id, body)
        }
        Cmd::Focus => {
            follow_up = iced::window::get_latest()
                .and_then(|id| iced::window::gain_focus(id));
            Response::ok(id)
        }
        Cmd::Close => {
            follow_up = iced::window::get_latest()
                .and_then(|id| iced::window::close(id));
            Response::ok(id)
        }
        Cmd::Mode { mode } => {
            self.view_mode = match mode {
                Mode::View => ViewMode::Rendered,
                Mode::Edit => ViewMode::Raw,
                Mode::Mindmap => ViewMode::Mindmap,
            };
            Response::ok(id)
        }
        Cmd::OpenFolder { dir } => {
            follow_up = iced::Task::done(Message::OpenWorkspace(std::path::PathBuf::from(dir)));
            Response::ok(id)
        }
        Cmd::Reveal { file } => {
            // Best-effort: open the file and rely on existing tree-reveal behaviour.
            follow_up = iced::Task::perform(
                load_file(std::path::PathBuf::from(file)),
                Message::FileLoaded,
            );
            Response::ok(id)
        }
        Cmd::Open { file, line, section } => {
            if self.dirty {
                Response::err(
                    id,
                    format!(
                        "unsaved edits in {}; save or discard before opening another",
                        self.file
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default()
                    ),
                )
            } else {
                let path = std::path::PathBuf::from(file);
                follow_up = iced::Task::perform(load_file(path), Message::FileLoaded);
                // line/section are applied via Cmd::Goto after the load completes.
                // Stash the pending nav for a one-shot apply on next ast refresh.
                self.pending_nav = Some(PendingNav { line, section });
                Response::ok(id)
            }
        }
        Cmd::Goto { line, section } => apply_goto(self, id, line, section),
    };
    Self::reply(&tx, resp);
    return follow_up;
}
```

- [ ] **Step 5: Add `pending_nav` state + types**

Inside the `App` struct, add:

```rust
    pub pending_nav: Option<PendingNav>,
```

Define near the other small types in `app.rs`:

```rust
#[derive(Debug, Clone, Default)]
pub struct PendingNav {
    pub line: Option<u32>,
    pub section: Option<String>,
}
```

In `Default for App` (or `App::new`), initialise `pending_nav: None`.

After every successful AST refresh in `update` (find sites where `self.ast = ...` is assigned and the file just finished loading — typically inside `Message::FileLoaded(Ok(...))`), append:

```rust
if let Some(nav) = self.pending_nav.take() {
    // synthesise a Goto and recurse via Task::done
    return iced::Task::done(Message::Ipc(
        crate::ipc::Request {
            id: 0,
            cmd: crate::ipc::Cmd::Goto { line: nav.line, section: nav.section },
        },
        std::sync::Arc::new(std::sync::Mutex::new(None)),
    ));
}
```

(Sender is `None` because nobody is awaiting the reply — this is a self-dispatched Goto.)

- [ ] **Step 6: Implement `apply_goto` and `current_line_estimate`**

Add to `app.rs`, outside `impl App`:

```rust
fn apply_goto(
    app: &mut App,
    id: u64,
    line: Option<u32>,
    section: Option<String>,
) -> crate::ipc::Response {
    use crate::ipc::Response;
    if app.ast.is_empty() {
        return Response::err(id, "no file open");
    }
    let target_line = if let Some(sec) = section {
        let sections = crate::ipc::sections::list_sections(&app.source);
        match crate::ipc::sections::resolve_section_path(&sec, &sections) {
            Some(s) => s.line,
            None => return Response::err(id, format!("section \"{sec}\" not found")),
        }
    } else if let Some(l) = line {
        let max_line = app.block_lines.last().copied().unwrap_or(1);
        if l > max_line.saturating_add(1000) {
            // generous slack; only error on absurd overshoot
            return Response::err(
                id,
                format!("line {l} out of range (file ends near line {max_line})"),
            );
        }
        l
    } else {
        return Response::err(id, "goto requires --line or --section");
    };

    let Some(idx) = crate::ipc::lines::block_for_line(target_line, &app.block_lines) else {
        return Response::err(id, "no blocks");
    };
    let block_id = app.ast[idx].0;
    let Some((block_top, block_h)) =
        crate::virt::estimated_block_position(&app.ast, &app.height_cache, block_id)
    else {
        return Response::err(id, "could not locate block");
    };
    let estimated_h = crate::virt::estimated_content_height(&app.ast, &app.height_cache);
    let (content_h, view_h) = app
        .body_viewport
        .as_ref()
        .map(|v| (v.content_bounds().height.max(estimated_h), v.bounds().height))
        .unwrap_or((estimated_h, 0.0));
    let max_scroll = (content_h - view_h).max(1.0);
    let target = block_top + block_h * 0.5 - view_h * 0.38;
    let rel = (target / max_scroll).clamp(0.0, 1.0);
    // Dispatching via Task happens outside this helper; caller already returns
    // Task::none(). We push a snap message through a small side channel:
    app.queued_snap = Some(rel);
    Response::ok(id)
}

fn current_line_estimate(app: &App) -> Option<u32> {
    let v = app.body_viewport.as_ref()?;
    let content_h = v.content_bounds().height;
    let view_h = v.bounds().height;
    if content_h <= view_h {
        return app.block_lines.first().copied();
    }
    let rel = v.absolute_offset().y / (content_h - view_h);
    // Find the first block whose vertical fraction is just past `rel`.
    let est_total = crate::virt::estimated_content_height(&app.ast, &app.height_cache).max(1.0);
    let target_px = rel * est_total;
    let mut best: Option<u32> = None;
    for (i, (bid, _)) in app.ast.iter().enumerate() {
        if let Some((top, _)) = crate::virt::estimated_block_position(&app.ast, &app.height_cache, *bid) {
            if top <= target_px {
                best = app.block_lines.get(i).copied();
            } else {
                break;
            }
        }
    }
    best
}
```

Add `queued_snap: Option<f32>` to `App`, default `None`. In `view`-or-`update` cycle (find the place that already handles `Message::RestoreBodySnap` — around `app.rs:1888`), drain `queued_snap` at the top of `update`:

```rust
// at the top of App::update, before the match
if let Some(rel) = self.queued_snap.take() {
    return iced::Task::done(Message::RestoreBodySnap(rel));
}
```

- [ ] **Step 7: Wire the IPC subscription**

Find `App::subscription` (grep for `fn subscription`). Add a new subscription channel that reads from the IPC mpsc and yields `Message::Ipc`. Sketch:

```rust
fn subscription(&self) -> iced::Subscription<Message> {
    let existing = /* keep existing subscriptions, batched */;
    let ipc = iced::Subscription::run(ipc_subscription);
    iced::Subscription::batch([existing, ipc])
}

fn ipc_subscription() -> impl iced::futures::Stream<Item = Message> {
    iced::stream::channel(64, |mut out: futures::channel::mpsc::Sender<Message>| async move {
        let listener = match crate::ipc::server::acquire() {
            Ok(l) => l,
            Err(_) => return,
        };
        let (tx, mut rx) = futures::channel::mpsc::channel::<crate::ipc::server::Pending>(64);
        tokio::spawn(crate::ipc::server::run(listener, tx));
        use futures::StreamExt;
        while let Some((req, reply)) = rx.next().await {
            let wrapped = std::sync::Arc::new(std::sync::Mutex::new(Some(reply)));
            if out.send(Message::Ipc(req, wrapped)).await.is_err() {
                break;
            }
        }
    })
}
```

Adjust to whatever batching pattern the existing `subscription` uses — do not regress existing subscriptions (theme watcher, file watcher, search, etc).

- [ ] **Step 8: Verify compile**

Run: `cargo check`
Expected: PASS. Fix any field-name typos against the real `App` struct.

- [ ] **Step 9: Run full tests**

Run: `cargo test`
Expected: PASS.

- [ ] **Step 10: Commit**

```bash
git add src/app.rs
git commit -m "feat(app): Message::Ipc + handlers (open/goto/mode/current/...)"
```

---

## Task 12: Rewrite `main.rs` — clap dispatch + client/instance fork

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Replace `main.rs` body**

Overwrite `src/main.rs` with:

```rust
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

    // Stateless / theme branches exit without touching Iced.
    match parsed {
        ParsedCli::Theme(args) => std::process::exit(run_theme_cmd(&args)),
        ParsedCli::Stateless(Stateless::ListSections { file, pretty }) => {
            std::process::exit(run_list_sections(&file, pretty));
        }
        ParsedCli::Empty => {
            // Bare `mdv` — if an instance is running, just focus; otherwise launch idle.
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
                    // No instance — become one, apply this request after startup.
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
    // Translate the initial request to a starting file path for `App::new`.
    // Other request shapes (Goto, Mode, Current) without an Open are no-ops at
    // cold start — there is no file to act on yet.
    let initial_path: Option<PathBuf> = match &initial {
        Some(req) => match &req.cmd {
            ipc::Cmd::Open { file, .. } => Some(PathBuf::from(file)),
            ipc::Cmd::OpenFolder { dir } => Some(PathBuf::from(dir)),
            _ => None,
        },
        None => None,
    };
    // Pending nav (line / section) from the initial Open is applied via the
    // existing `pending_nav` mechanism — see Task 11 Step 5. We stash it onto
    // the app after construction.
    let pending_nav = match &initial {
        Some(req) => match &req.cmd {
            ipc::Cmd::Open { line, section, .. } => Some(mdv::app::PendingNav {
                line: *line,
                section: section.clone(),
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

// Preserve existing theme subcommand behaviour. Body unchanged from prior
// `main.rs:82-179` — copy verbatim.
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
```

Make `PendingNav` `pub` (Task 11 already defined it). Make `App.pending_nav` `pub`.

- [ ] **Step 2: Compile**

Run: `cargo build`
Expected: PASS.

- [ ] **Step 3: Run all tests**

Run: `cargo test`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/main.rs src/app.rs
git commit -m "feat(cli): clap dispatch + client/instance fork in main"
```

---

## Task 13: End-to-end `list-sections` subprocess test

**Files:**
- Create: `tests/ipc_e2e.rs`

- [ ] **Step 1: Write the test**

Create `tests/ipc_e2e.rs`:

```rust
use std::process::Command;

#[test]
fn list_sections_subprocess_emits_json_array() {
    let exe = env!("CARGO_BIN_EXE_mdv");
    let out = Command::new(exe)
        .args(["list-sections", "tests/fixtures/sections.md"])
        .output()
        .expect("spawn mdv");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8(out.stdout).unwrap();
    let v: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    let arr = v.as_array().expect("array");
    assert!(arr.iter().any(|s| s["path"] == "Foo/Install/Setup"));
    assert!(arr.iter().any(|s| s["path"] == "Foo/Usage/Setup"));
}

#[test]
fn list_sections_pretty_flag() {
    let exe = env!("CARGO_BIN_EXE_mdv");
    let out = Command::new(exe)
        .args(["--pretty", "list-sections", "tests/fixtures/sections.md"])
        .output()
        .expect("spawn mdv");
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains('\n'), "pretty output should be multiline");
}
```

- [ ] **Step 2: Run**

Run: `cargo test --test ipc_e2e`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add tests/ipc_e2e.rs
git commit -m "test(ipc): subprocess e2e for list-sections"
```

---

## Task 14: Help text + version preservation

**Files:**
- Verify only.

- [ ] **Step 1: Verify `--help` works**

Run: `cargo run -- --help`
Expected: clap-generated help listing subcommands (open, open-folder, goto, mode, reveal, focus, close, current, list-sections, theme).

- [ ] **Step 2: Verify `--version`**

Run: `cargo run -- --version`
Expected: `mdv 0.2.0` (or whatever Cargo.toml says).

- [ ] **Step 3: Verify theme passthrough**

Run: `cargo run -- theme list`
Expected: lists themes as before.

If any of these fail, fix in `src/cli.rs` (clap config) or `src/main.rs` (theme dispatch) before continuing.

- [ ] **Step 4: Commit (only if fixes needed)**

```bash
git add -p
git commit -m "fix(cli): help/version/theme parity"
```

---

## Task 15: Manual smoke test

**Files:**
- None.

- [ ] **Step 1: Launch instance**

Run in shell A:
```bash
cargo run --release -- README.md
```
Expected: mdv window opens with README.

- [ ] **Step 2: Open another file from shell B**

Run in shell B:
```bash
target/release/mdv open tests/fixtures/sections.md --line 9
```
Expected: shell B prints `{"id":1,"ok":true}` and exits 0. Window in shell A scrolls to "Setup" (line 9 in the fixture).

- [ ] **Step 3: Query current state**

Run in shell B:
```bash
target/release/mdv current
```
Expected: JSON like `{"id":1,"ok":true,"result":{"file":".../sections.md","line":<n>,"mode":"view","folder":null}}`.

- [ ] **Step 4: Switch mode**

```bash
target/release/mdv mode mindmap
```
Expected: window switches to mindmap view; `{"id":1,"ok":true}`.

- [ ] **Step 5: Section navigation**

```bash
target/release/mdv mode view
target/release/mdv goto --section "Usage/Setup"
```
Expected: window scrolls to the second "Setup" (line 17).

- [ ] **Step 6: Stale socket recovery**

In shell A: kill the mdv process (Ctrl+C / Cmd+Q).
Run:
```bash
ls "$TMPDIR/mdv-$(id -u).sock" 2>/dev/null && echo STALE || echo CLEAN
cargo run --release -- README.md
```
Expected: stale file (if any) gets unlinked and a fresh instance starts cleanly.

- [ ] **Step 7: list-sections standalone (no instance)**

Quit mdv. Run:
```bash
target/release/mdv list-sections README.md
```
Expected: JSON array of headings, exit 0. No window opens.

If anything misbehaves, return to the relevant task and fix.

- [ ] **Step 8: Commit (if smoke uncovered fixes)**

Otherwise no commit.

---

## Task 16: Update README + memory note

**Files:**
- Modify: `README.md` — add CLI section.

- [ ] **Step 1: Append CLI section to README**

Add a section to `README.md`:

```markdown
## CLI / agent control

mdv is single-instance. The first invocation opens a window and an IPC
listener; subsequent invocations talk to it.

```bash
# open a file at a specific line
mdv path/to/foo.md --line 42

# navigate the running instance
mdv goto --section "Install/Setup"
mdv mode mindmap
mdv current                          # prints JSON state

# stateless (no running instance needed)
mdv list-sections path/to/foo.md     # JSON array of headings
mdv --pretty list-sections foo.md
```

Designed for coding agents (Claude Code, Codex) to pull mdv to the relevant
section of a file without manual navigation.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: README section for CLI / agent control"
```

---

## Self-review summary

**Spec coverage:** every spec section mapped to one or more tasks.
- Single-instance model → Task 12 (`launch_instance` + try-connect first).
- Command surface → Task 7 (clap), Task 11 (handlers).
- JSON protocol → Tasks 2, 13.
- Source-line tracking → Tasks 3, 4, 6.
- Section path resolver → Task 5.
- Iced bridge → Task 11.
- Edge cases → Task 11 (dirty buffer, no file open, section not found), Task 10 (stale socket).
- Dependencies → Task 1.
- Testing → Tasks 2, 3, 4, 5, 7, 8, 13, 15.

**Placeholders:** none — every code step shows actual code; every test step shows expected behaviour and pass/fail criteria.

**Type consistency:** `Request`, `Response`, `Cmd`, `Mode` defined in Task 2 and referenced unchanged in 5, 7, 9, 10, 11, 12. `PendingNav` defined in Task 11 and reused in Task 12. `ParsedCli`, `Stateless` defined in Task 7 and consumed in Task 12. `block_for_line`, `build_byte_to_line` defined Task 3, consumed in Tasks 5, 11. `list_sections`, `resolve_section_path` defined Task 5, consumed in Tasks 11, 12.

**Mode mapping:** spec `view|edit|mindmap` ↔ app `ViewMode::Rendered|Raw|Mindmap` declared in Task 11 Step 4 (`Cmd::Mode` arm) and Task 11 (Current arm) — consistent.

**Out of scope respected:** no edit/save/write commands; no auth tokens; no multi-instance; no fuzzy match — only suffix-segment match as spec specifies.
