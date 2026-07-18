# Repository guidance

## First pass

1. Read [`PROJECT_STATUS.md`](PROJECT_STATUS.md) and verify its Git/GitHub and
   runtime claims before changing the repository.
2. If control-plane files are already modified, review that diff and reconcile
   status, backlog, and plans before selecting new product work. Present the
   candidate for owner approval; do not land it implicitly.
3. Select one ready outcome from [`docs/BACKLOG.md`](docs/BACKLOG.md), then read
   its execution contract under [`docs/plans/active/`](docs/plans/active/).
4. Identify the module that owns the behavior and read that module's `AGENTS.md`
   before broad exploration or edits.
5. Start from the module's listed entrypoints and run its smallest relevant
   checks.

## Module map

| Path | Responsibility | Read next |
| --- | --- | --- |
| `src/` | Rust library, Iced application state, parsing, rendering, and workspace navigation | [`src/AGENTS.md`](src/AGENTS.md) |
| `src/ipc/` | Cross-platform CLI/IPC request types, local-socket transport, and section lookup | [`src/ipc/AGENTS.md`](src/ipc/AGENTS.md) |
| `tests/` | Integration tests, parser/renderer fixtures, and snapshots | [`tests/AGENTS.md`](tests/AGENTS.md) |
| `site/` | Static landing site and Cloudflare asset payload | [`site/AGENTS.md`](site/AGENTS.md) |
| `demo/` | Sample vault content and the manual recording tour | [`demo/AGENTS.md`](demo/AGENTS.md) |

## Project control plane

- `PROJECT_STATUS.md` is the compact current snapshot, not a chronological log.
- `docs/BACKLOG.md` owns stable task IDs and portfolio state.
- `docs/plans/active/` owns multi-session execution and acceptance contracts;
  accepted plans move to `docs/plans/completed/`.
- Update current facts and acceptance evidence before ending material work.
- Route completed and superseded narrative to `docs/status-history/`.

## Global invariants

- Preserve unrelated work in a dirty checkout or sibling worktree.
- The actual repository is `/Users/liminchen/Documents/GitHub/mdv`;
  `/Users/liminchen/Documents/GitHub/mdv-main` is a legacy non-repo path.
- This is one Cargo package: keep the library/binary split in `src/lib.rs` and
  `src/main.rs` coherent.
- The `pdf` feature is enabled by default; Windows builds deliberately use
  `--no-default-features`.
- Keep durable design and volatile status in their linked documents instead of
  copying them into every module guide.
- Do not describe an uncommitted change as merged, released, deployed, or
  manually accepted.
- Do not push, merge, tag, release, publish, or deploy without explicit owner
  authority.

## Repository checks

- `cargo check`
- `cargo test --lib`
- `cargo test --tests`
- `cargo build --release --bin rmdv`
- `git diff --check`

## Longer references

- [Current status](PROJECT_STATUS.md)
- [Project backlog](docs/BACKLOG.md)
- [Product contract](PRODUCT.md)
- [Full Mindmap design](docs/superpowers/specs/2026-07-10-full-mindmap-mode-design.md)
- [CLI and IPC design](docs/superpowers/specs/2026-05-17-cli-agent-control-design.md)
- [Zen edit-mode design](docs/superpowers/specs/2026-07-06-zen-edit-mode-design.md)
