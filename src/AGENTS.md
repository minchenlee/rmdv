# Rust application guidance

## Scope

Applies to `src/**`.

## Purpose

Owns the rmdv library and the Iced desktop application's state machine. The
library exposes parsers, renderers, workspace models, and the IPC boundary;
`src/main.rs` starts either the stateless CLI path or the single GUI instance.

## Start here

- [`lib.rs`](lib.rs) — crate module graph and public boundaries
- [`main.rs`](main.rs) — process startup, single-instance fallback, and Iced wiring
- [`app.rs`](app.rs) — `App`, `Message`, update/view/subscription, and async ownership
- [`parser.rs`](parser.rs) and [`render.rs`](render.rs) — document AST and widget materialization
- [`tree.rs`](tree.rs) and [`workspace_mindmap.rs`](workspace_mindmap.rs) — bounded filesystem graph and workspace nodes
- [`../docs/superpowers/specs/2026-07-10-full-mindmap-mode-design.md`](../docs/superpowers/specs/2026-07-10-full-mindmap-mode-design.md) — current workspace-navigation contracts

## Architecture and boundaries

- `app.rs` is the UI orchestrator: it owns application state, Iced messages,
  subscriptions, view-mode transitions, and request-identity checks for
  background work.
- `parser.rs`, `ast.rs`, and `tex.rs` produce document structures and source
  offsets. `render.rs`, `diagram.rs`, and `virt.rs` turn those structures into
  bounded, viewport-aware widgets and caches.
- `tree.rs` owns bounded filesystem discovery and the ordinary sidebar index;
  `workspace_mindmap.rs` adapts accepted snapshots into path-based workspace
  graph nodes. Workspace node identities must not reuse document `BlockId`s.
- `cli.rs` and the nested [`ipc/AGENTS.md`](ipc/AGENTS.md) guide the
  process-control boundary. IPC requests are handled by `App`, not by direct
  rendering or filesystem mutations in transport code.
- New parser/model code should remain independent of `App` and Iced. Preserve
  the existing render-to-app cache/message seam, and keep large preview reads,
  parses, and measurements off the Iced update thread.
- Any async completion that can outlive a selection, workspace, preview range,
  or navigator instance must retain the relevant identity guard; stale results
  must be ignored rather than normalized into current state.

## Commands

- `cargo test --lib` — run unit tests embedded in the library modules.
- `cargo check` — check the default feature set, including PDF support.
- `cargo check --no-default-features` — check the Windows/lean feature path.
- `cargo check --features pdf` — check the explicit PDF feature path.
- `cargo build --release --bin rmdv` — build the runnable optimized binary.
- `rustfmt --edition 2021 --check src/app.rs src/mindmap.rs src/virt.rs src/diagram.rs` — focused formatting gate for the current Full Mindmap/UI touch set; use the same form with the actual touched Rust files for other edits.

## Editing constraints

- Read the relevant design spec and `PROJECT_STATUS.md` before changing the
  large `app.rs` state machine; do not copy current status into this file.
- Keep Full Mindmap workspace indexing, preview parsing, asset loading, and
  measurement bounded and request-identified. Do not reintroduce synchronous
  large-file work on the UI thread.
- Keep document navigation semantics separate from workspace navigation, and
  route protocol changes through `src/ipc/` plus its integration tests.
- Put behavior regressions in `src` unit tests or `tests/`; do not edit files
  under `target/` as product source.

## Verification

1. Run the narrowest affected `cargo test --lib` filter or module test.
2. Run `cargo check` and focused rustfmt for touched Rust files.
3. Run `cargo test --tests` when public behavior, CLI/IPC, parsing, or fixture contracts cross the library boundary.
4. Run `git diff --check` before handing off; use the root release build only when the change needs a runnable binary.

## Status and handoff

- [Current status](../PROJECT_STATUS.md)
- [Full Mindmap design](../docs/superpowers/specs/2026-07-10-full-mindmap-mode-design.md)
