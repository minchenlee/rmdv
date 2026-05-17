# mdv CLI / Agent Control Surface — Design

**Date:** 2026-05-17
**Status:** Approved for implementation planning
**Branch context:** `gpui-port` (work targets Iced `main`; see memory `mdv_stack.md`)

## Goal

Expose a command-line / IPC control surface so coding agents (Claude Code, Codex, scripts) can drive a running mdv window:

- Open a file or folder.
- Switch view mode (view / edit / mindmap).
- Scroll to a specific source line or section heading path.
- Reveal a file in the sidebar tree.
- Query current state (active file, line, mode, folder) as JSON.
- Enumerate headings of any markdown file (stateless) as JSON.

Primary use case: an agent locates a relevant passage in a markdown file the user owns and pulls mdv directly to that passage — no manual navigation by the user.

## Non-goals (v1)

- Editing / saving / writing files via the CLI (agents already edit files directly; mdv's file watcher reloads).
- Multi-instance or per-folder windows (current app is single-window).
- Fuzzy section matching, nth-match selection.
- Auth tokens (same-user trust boundary).
- Streaming / subscribe-style responses.
- Windows polish beyond what `interprocess` gives for free.

## Architecture

### Single-instance daemon model

The first `mdv` invocation spawns the window and an IPC listener. Subsequent invocations attempt to connect to the listener:

- **Connect succeeds** → process runs in **client mode**: serialize the parsed args as one JSON request, write to socket, read one JSON response, print to stdout, exit. No new window.
- **Connect fails (refused / socket missing)** → unlink any stale socket file, process becomes the new instance, applies the parsed args as an initial command on the first frame.

This is the "C-smart" hybrid: one-shot `mdv foo.md --line 42` works whether mdv is running or not, and there is never a duplicate window.

### Data flow

```
agent → `mdv goto --line 42`
            │
            ▼
        client.rs → socket → server.rs (Iced subscription task)
                                │
                                ▼
                       Message::Ipc(Request, oneshot::Sender<Response>)
                                │
                                ▼
                      App::update — mutates state, dispatches scroll Task
                                │
                                ▼
                       Response (JSON) ← oneshot ← server.rs → socket → client → stdout
```

### Components

| File | Purpose |
|---|---|
| `src/main.rs` | Entry. Parse args via clap. Stateless subcommands (`theme`, `list-sections`) run and exit. Otherwise try-connect; client mode or instance mode. |
| `src/cli.rs` | Clap derive structs, arg → `Request` mapping, JSON output helpers (`--pretty` flag). |
| `src/ipc/mod.rs` | `Request`, `Response` types (serde). Re-exports. |
| `src/ipc/server.rs` | `interprocess` listener inside `iced::Subscription`. One client at a time. |
| `src/ipc/client.rs` | Connect, write one line, read one line, exit. |
| `src/ipc/socket.rs` | Platform path: `$TMPDIR/mdv-$UID.sock` (macOS/Linux), `\\.\pipe\mdv-$user` (Windows). Stale-socket recovery via try-connect. |
| `src/ipc/sections.rs` | Stateless `list-sections` impl. Reused by IPC server (running instance) and standalone CLI (no instance). |
| `src/parser.rs` | Emit byte offset for each block (from `pulldown-cmark` `OffsetIter`). |
| `src/app.rs` | New: `Message::Ipc`, `block_lines: Vec<u32>` field, section-path resolver, IPC subscription wiring. |

## Command surface

```
mdv [FILE|DIR] [--line N] [--section "path"] [--mode <view|edit|mindmap>]
mdv open <file> [--line N] [--section "Install/Setup"]
mdv open-folder <dir>
mdv goto (--line N | --section "path")
mdv mode <view|edit|mindmap>
mdv reveal <file>
mdv focus
mdv close
mdv current
mdv list-sections <file>
mdv theme ...                          # existing, unchanged
mdv --help | --version                 # existing
```

### Bare-form precedence

`mdv foo.md --line 42 --section "Install/Setup" --mode view` is sugar for an `open` request with the same options. Preserves current positional UX. If the path is a directory, treated as `open-folder`.

### Section path semantics

`"Install/Setup"` matches a heading titled "Setup" whose nearest preceding higher-level heading is "Install". Bare `"Setup"` matches the first occurrence. Case-sensitive, exact string match for v1.

Resolution algorithm:

1. Walk blocks in order, maintaining a stack `[(level, title)]`.
2. On each `Heading { level, title }`, pop the stack while top level ≥ this level, push.
3. Current path = `stack.iter().map(|(_,t)| t).join("/")`.
4. First block whose path matches (suffix-match if user-provided path has fewer segments) → resolve to its source line → scroll.

### Output

JSON to stdout, one object per command. `--pretty` emits indented JSON for humans. Errors go to stderr as `{"error":"..."}` with non-zero exit.

Examples:

```json
// mdv current
{"file":"/abs/path/foo.md","line":42,"mode":"view","folder":"/abs/path"}

// mdv list-sections foo.md
[
  {"level":1,"title":"Foo","path":"Foo","line":1},
  {"level":2,"title":"Install","path":"Foo/Install","line":5},
  {"level":3,"title":"Setup","path":"Foo/Install/Setup","line":12}
]

// mdv goto --line 42  (success)
{"ok":true}

// mdv goto --line 99999  (out of range)
{"ok":false,"error":"line 99999 out of range (file has 320 lines)"}
```

## Protocol

Line-delimited JSON over a Unix domain socket (macOS/Linux) or named pipe (Windows), via the `interprocess` crate.

### Request

```json
{"id":1,"cmd":"goto","args":{"line":42}}
{"id":2,"cmd":"open","args":{"file":"/abs/path/foo.md","line":10,"section":"Install/Setup"}}
{"id":3,"cmd":"current"}
{"id":4,"cmd":"mode","args":{"mode":"mindmap"}}
{"id":5,"cmd":"reveal","args":{"file":"/abs/path/bar.md"}}
{"id":6,"cmd":"focus"}
{"id":7,"cmd":"close"}
```

### Response

```json
{"id":1,"ok":true}
{"id":3,"ok":true,"result":{"file":"/abs/path/foo.md","line":42,"mode":"view","folder":"/abs/path"}}
{"id":1,"ok":false,"error":"no file open"}
```

`id` is echoed verbatim for client correlation. v1 client uses one request per connection so `id` is always `1`, but the field is reserved for future multiplexing.

### Connection lifecycle

1. Client connects.
2. Client writes one JSON line (request).
3. Server reads one line, dispatches into Iced update loop, awaits oneshot reply.
4. Server writes one JSON line (response).
5. Both sides close.

Server accepts connections serially — one client at a time. Sufficient because commands are short and the Iced update loop is single-threaded anyway.

### Stale socket recovery (startup)

```
fn acquire() -> Either<Listener, ClientShouldRun> {
    match connect(socket_path) {
        Ok(stream) => Right(stream),       // live instance, become client
        Err(refused_or_missing) => {
            let _ = std::fs::remove_file(socket_path);  // best-effort unlink
            let listener = bind(socket_path)?;
            Left(listener)                  // we are the instance
        }
    }
}
```

## Source-line tracking

`pulldown-cmark::Parser::into_offset_iter()` yields `(Event, Range<usize>)`. The parser already walks events to build the AST; extend it to record the start byte offset of each block.

After parse:

- Compute `byte_to_line: Vec<u32>` once by scanning newlines in the source (cheap).
- For each block in order, look up its line and append to `block_lines: Vec<u32>` on `App`, aligned with the existing `Vec<(BlockId, Block)>`.

Resolver:

```rust
fn block_for_line(line: u32, block_lines: &[u32]) -> Option<usize> {
    if block_lines.is_empty() { return None; }
    // largest index i where block_lines[i] <= line
    match block_lines.binary_search(&line) {
        Ok(i) => Some(i),
        Err(0) => Some(0),
        Err(i) => Some(i - 1),
    }
}
```

Scroll mechanism reuses the existing recipe (`app.rs:541` `scroll_to_current_match`):

1. `block_for_line(N)` → block index.
2. `virt::estimated_block_position(&ast, &height_cache, block_id)` → `(top, h)`.
3. Compute relative offset, dispatch `Message::RestoreBodySnap(rel)`.

`list-sections` is independent of running state: parse the file, walk `Block::Heading`, build the path stack, emit one JSON object per heading. Lives in `src/ipc/sections.rs` so both the running instance and a standalone CLI invocation share the implementation.

## Iced bridge

```rust
// App::subscription
Subscription::run(|| {
    iced::stream::channel(64, |mut out| async move {
        let Some(listener) = ipc::server::acquire().await else { return; };
        loop {
            let Ok(conn) = listener.accept().await else { continue; };
            let Ok(req): Result<Request, _> = read_line_json(&conn).await else { continue; };
            let (reply_tx, reply_rx) = oneshot::channel();
            if out.send(Message::Ipc(req, reply_tx)).await.is_err() { break; }
            let Ok(resp) = reply_rx.await else { continue; };
            let _ = write_line_json(&conn, &resp).await;
        }
    })
})
```

New variant:

```rust
Message::Ipc(Request, oneshot::Sender<Response>)
```

`App::update` matches on `req.cmd`, mutates state, returns the appropriate `Task` (for scroll / mode-change side effects), and sends a `Response` back through the oneshot. Serializes naturally because the update loop is single-threaded.

## Startup flow

```
main():
  args = clap::parse();
  match args.subcommand {
    Theme(_)        => run_theme_cmd(); exit,
    ListSections(p) => run_list_sections(p); exit,
    _ => {}
  }
  let req = build_request_from_args(args);
  match ipc::client::try_send(req) {
    Ok(resp)  => { print_json(resp); exit(0) }       // client mode
    Err(NoListener) => {
      // instance mode
      let initial = req;                              // applied on first frame
      iced::application(App::new_with_initial(initial), ...).run()
    }
  }
```

## Edge cases

| Case | Behaviour |
|---|---|
| `mdv goto --line 42` with no file open | `{"ok":false,"error":"no file open"}` |
| `--line N` where N > line count | `{"ok":false,"error":"line N out of range (file has M lines)"}` |
| `--section "X"` no match | `{"ok":false,"error":"section \"X\" not found"}` |
| `mdv open foo.md` with relative path in client mode | Client resolves to absolute against its own cwd before sending. |
| `mdv list-sections` on non-markdown file | Parse anyway (pulldown-cmark accepts anything); empty array result. |
| Two clients connect simultaneously | Second blocks at `accept()` until first finishes. |
| User has unsaved edits (`self.dirty`), agent sends `open` for a different file | Return `{"ok":false,"error":"unsaved edits in <current file>; save or discard before opening another"}`. Matches existing watcher behaviour (`FileChanged` toast-ignores external changes when dirty, see `app.rs:1525`). Agent can retry after user saves. |
| Stale socket from crashed instance | `acquire()` unlinks on connect-refused. |
| Socket path collision across users | Path includes `$UID` (macOS/Linux) or `$user` (Windows). |
| Client connects but server crashes mid-request | Client sees broken pipe → exit non-zero with `{"error":"ipc disconnect"}`. |

## Dependencies

Add to `Cargo.toml`:

- `clap = { version = "4", features = ["derive"] }` — replace hand-rolled arg parsing in `main.rs`. Existing `theme` subcommands ported.
- `interprocess = { version = "2", features = ["tokio"] }` — cross-OS local socket / named pipe.

Already present: `serde`, `serde_json`, `tokio`, `anyhow`.

## Testing

**Unit tests (`tests/` or `#[cfg(test)]`):**

- `block_for_line` binary-search behaviour on empty, single, duplicate, boundary inputs.
- Section-path resolver: nested headings, suffix match, no match, duplicate titles.
- `Request` / `Response` JSON round-trip.
- Parser source-line tracking: fixture md with known heading lines, assert `block_lines` matches.

**Integration tests:**

- `list-sections` end-to-end (no Iced needed): run binary as subprocess, pipe md fixture, assert JSON output.
- IPC client/server round-trip with a mock listener (no Iced): bind, connect, send `current`-shaped request to a stub handler, assert response.

**Manual smoke:**

- Launch `mdv tests/fixtures/diagrams_stress.md`.
- From another shell: `mdv goto --line 100` → window scrolls.
- `mdv current` → JSON matches.
- Kill mdv, `ls $TMPDIR/mdv-*.sock` → cleanup on next launch.
- `mdv list-sections README.md` without launching mdv → JSON.

## Risk register

| Risk | Mitigation |
|---|---|
| `interprocess` API churn on macOS 26.1 / Tahoe | Pin minor version; smoke-test early. Fallback: hand-rolled `tokio::net::UnixListener` (mac/linux only). |
| Parser changes for byte offsets regress existing tests | Run `cargo test` after each parser change; existing `parser_snapshots__gfm.snap` covers GFM. Update snapshots if intentional. |
| Iced subscription doesn't naturally tear down on shutdown | Drop `listener` on subscription cancel; `interprocess` cleans up. Verify with `lsof`. |
| Scroll-to-line accuracy off for very tall blocks (large diagrams, images) | Existing `virt::estimated_block_position` already used by search; acceptable parity. |
| Client/instance race at startup (two simultaneous launches) | `bind()` on second launcher fails → it falls into client mode automatically. |

## Open questions

None blocking. Future considerations (out of scope):

- Per-folder instances if multi-window lands.
- Streaming events (subscribe to scroll / file open) for richer agent flows.
- Token auth if mdv ever runs as a shared service.

## Out of scope (restated)

- Edit / save / write CLI verbs.
- Multi-instance.
- Fuzzy section match.
- Auth.
- Streaming responses.
