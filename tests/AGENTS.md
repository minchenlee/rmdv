# Integration-test guidance

## Scope

Applies to `tests/**`, including fixtures and snapshots.

## Purpose

Owns executable integration contracts for the public rmdv library and binary.
The directory exercises parsing, rendering, virtual scrolling, search, TeX,
CLI/IPC behavior, and hot-reload behavior; it does not contain product logic.

## Start here

- [`ipc_protocol.rs`](ipc_protocol.rs) and [`ipc_e2e.rs`](ipc_e2e.rs) — public CLI/IPC contracts
- [`parser_snapshots.rs`](parser_snapshots.rs) and [`snapshots/`](snapshots/) — AST snapshot behavior
- [`fixtures/`](fixtures/) — stable Markdown, diagram, TeX, and section inputs
- [`diagram_render.rs`](diagram_render.rs) and [`virtual_scroll.rs`](virtual_scroll.rs) — renderer and geometry contracts
- [`hot_reload_smoke.sh`](hot_reload_smoke.sh) — manual release-binary smoke path
- [`../Cargo.toml`](../Cargo.toml) — test dependencies and package feature matrix

## Architecture and boundaries

- Rust files here compile as Cargo integration targets against the public
  `rmdv` library; `ipc_e2e.rs` invokes the built binary through
  `CARGO_BIN_EXE_rmdv`.
- Fixtures are inputs, not implementation copies. Snapshot files under
  `snapshots/` are expected outputs and must change only when the behavior is
  intentionally changed and reviewed.
- Keep unit-only invariants close to their implementation in `src/`; use this
  directory for public behavior, cross-module behavior, subprocess behavior,
  or stable rendering/fixture contracts.
- Test paths are repository-relative. Do not make tests depend on a developer's
  home directory, running GUI instance, or network unless the test is clearly
  the documented manual smoke path.

## Commands

- `cargo test --tests` — run all integration targets.
- `cargo test --test ipc_protocol` — run protocol and CLI mapping coverage.
- `cargo test --test ipc_e2e` — run the stateless binary subprocess coverage.
- `cargo test --test parser_snapshots` — run AST snapshot coverage.
- `cargo test --test virtual_scroll` — run virtual-window geometry coverage.
- `./tests/hot_reload_smoke.sh` — manual smoke test; requires a prior `cargo build --release` and a GUI-capable environment.

## Editing constraints

- Preserve fixture filenames and relative paths unless the test and every
  documented caller are updated together.
- Never delete or mass-rewrite a snapshot merely to make a test green; inspect
  the diff and confirm the parser/rendering change is intentional.
- Keep tests deterministic and bounded. Avoid sleeps, network calls, or
  machine-specific assumptions in automated integration tests.
- Add coverage for both the accepted result and stale/error behavior when a
  change crosses an async request or public protocol boundary.

## Verification

1. Run the affected integration target first.
2. Run `cargo test --tests` after changing shared fixtures, snapshots, or public behavior.
3. Run `git diff --check` and inspect any snapshot diff before handoff.

## Status and handoff

- [Current status](../PROJECT_STATUS.md)
- [CLI and IPC design](../docs/superpowers/specs/2026-05-17-cli-agent-control-design.md)
- [Full Mindmap design](../docs/superpowers/specs/2026-07-10-full-mindmap-mode-design.md)
