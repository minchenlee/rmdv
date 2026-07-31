# MDV-018 — Document Mindmap depth folding

State: submitted
Owner / accountable lead: Codex
Active writer: Codex
Created: 2026-07-29
Updated: 2026-07-31

## Outcome

Make the reader's two-step `⌘K`, then `0–6` depth control an explicit, tested
Document Mindmap interaction: `1` leaves the document root and its direct
children, `2` also includes grandchildren, and `0` restores all node depths.
Expose the same neutral node-level actions in the command palette for Markdown,
JSON, YAML, and TOML Mindmaps.

## Non-goals

- Do not add depth folding to Full Mindmap workspace navigation.
- Do not change per-node Space folding, graph zoom, layout, or filesystem
  materialization.
- Do not push, merge, release, or deploy without explicit owner authority.

## Acceptance evidence

- Focused tests prove levels 1, 2, and 0 against Markdown and structured-data
  document graphs.
- The regression exercises the same cached `App::mindmap_layout()` path used by
  the native Document Mindmap rather than calling the layout algorithm with
  reader fold state.
- Full Mindmap cannot mutate the hidden document through the depth-fold message.
- In-app Mindmap shortcut guidance names the shared chord.
- The command palette exposes all seven actions only while Document Mindmap is
  active and routes them through the same `FoldToLevel` messages.
- JSON, YAML, and TOML use stable node identities across collapsed relayouts;
  TOML renders and previews real structure rather than an unsupported warning.
- Markdown depth follows structural parent/child relationships even when source
  headings skip ranks; heading rank continues to control horizontal placement.
- Collapse/expand invalidations advance a Document Mindmap layout generation so
  the existing canvas auto-center logic repositions the focused visible node.
- Focused tests, `cargo check`, rustfmt, and `git diff --check` pass.

## Progress

- [x] Confirm reader and Document Mindmap use separate fold state, then route
  the shared chord to the state owned by the active mode.
- [x] Confirm Full Mindmap blocks the chord at keyboard dispatch.
- [x] Add the explicit mode boundary, regression coverage, and guidance.
- [x] Run acceptance checks and record final evidence.

## Decision log

| Date | Decision | Evidence / reason |
| --- | --- | --- |
| 2026-07-29 | Keep the behavior Document Mindmap-only. | Owner explicitly excluded Full Mindmap. |
| 2026-07-29 | Use neutral node-level wording and support data Mindmaps. | Owner explicitly included JSON, YAML, and TOML after review. |

## Final evidence

- `cargo test --lib data_mindmap --target-dir /Users/liminchen/Documents/GitHub/mdv/target -j 2` — 21 passed, including the app-level JSON/YAML/TOML depth regression.
- `cargo test --lib document_mindmap_fold_levels_use_structural_depth_when_headings_skip_ranks --target-dir /Users/liminchen/Documents/GitHub/mdv/target -j 2` — passed.
- `cargo test --lib mindmap --target-dir /Users/liminchen/Documents/GitHub/mdv/target -j 2` — 150 passed.
- `cargo test --lib --target-dir /Users/liminchen/Documents/GitHub/mdv/target -j 2` — 336 passed.
- `cargo test --lib command_palette_exposes_node_depths_only_in_document_mindmap --target-dir /Users/liminchen/Documents/GitHub/mdv/target -j 2` — passed.
- `cargo check --target-dir /Users/liminchen/Documents/GitHub/mdv/target -j 2` — passed.
- `cargo build --release --bin rmdv --target-dir /Users/liminchen/Documents/GitHub/mdv/target -j 2` — passed; reviewed binary SHA-256 `def032b597f5715983b5649b1b757b0b581323afd33d644d484bf665be09441d`.
- `rustfmt --edition 2021 --check src/app.rs src/mindmap.rs src/data_mindmap.rs` — passed.
- `git diff --check` — passed.
- Native/manual acceptance remains pending; owner-authorized commit and local
  integration are separate delivery steps.
