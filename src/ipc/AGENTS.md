# IPC and CLI protocol guidance

## Scope

Applies to `src/ipc/**`.

## Purpose

Owns the cross-platform, single-instance control surface: serialized command
types, local-socket transport, request forwarding, and stateless section
lookup. It does not own Iced view state or document rendering.

## Start here

- [`mod.rs`](mod.rs) — IPC module exports
- [`types.rs`](types.rs) — `Cmd`, `Mode`, `Request`, and `Response` wire types
- [`client.rs`](client.rs) and [`server.rs`](server.rs) — one-line socket round trips and request forwarding
- [`sections.rs`](sections.rs) — heading paths and source-line lookup
- [`../cli.rs`](../cli.rs) — Clap command parsing and request construction
- [`../../docs/superpowers/specs/2026-05-17-cli-agent-control-design.md`](../../docs/superpowers/specs/2026-05-17-cli-agent-control-design.md) — protocol and testing contract

## Architecture and boundaries

- `src/main.rs` parses the CLI, tries `client::try_send`, and otherwise starts
  the Iced instance. `server::run` accepts one line-delimited JSON request at a
  time and forwards a `(Request, oneshot sender)` through the Iced subscription.
- `App::update` handles `Message::Ipc`, performs the state transition, and
  sends the response. Transport code must not call rendering code or mutate UI
  state directly.
- `types.rs` is the serialized compatibility boundary. `socket.rs` contains
  platform-specific Unix-socket/named-pipe naming; keep its `cfg` branches in
  sync when changing the transport.
- `sections.rs` is the stateless path: it reuses parser/TeX ASTs and byte-to-line
  mapping, and is covered by protocol and subprocess tests. It may depend on
  parsing modules, but it must not depend on Iced widgets.

## Commands

- `cargo test --test ipc_protocol` — wire types, CLI mapping, section paths, and line resolution.
- `cargo test --test ipc_e2e` — stateless binary/subprocess behavior.
- `cargo test --tests` — integration coverage after a protocol or public CLI change.
- `cargo check` — compile the transport on the current platform.

## Editing constraints

- Preserve one-line JSON request/response semantics, field names, command
  meanings, single-instance fallback, and stale-socket recovery unless the
  design spec and tests change together.
- Keep Windows and Unix implementations compiling; do not silently replace a
  cross-platform `cfg` path with a Unix-only API.
- Route app behavior through `Message::Ipc`; do not add a second UI-control
  path inside `client.rs`, `server.rs`, or `sections.rs`.
- Update [`../../docs/superpowers/specs/2026-05-17-cli-agent-control-design.md`](../../docs/superpowers/specs/2026-05-17-cli-agent-control-design.md) and the relevant tests when the public protocol changes.

## Verification

1. Run `cargo test --test ipc_protocol`.
2. Run `cargo test --test ipc_e2e`.
3. Run `cargo test --tests` and `cargo check` for changes that cross the binary/library boundary.
4. Run `git diff --check` before handoff.

## Status and handoff

- [Current status](../../PROJECT_STATUS.md)
- [CLI and IPC design](../../docs/superpowers/specs/2026-05-17-cli-agent-control-design.md)
