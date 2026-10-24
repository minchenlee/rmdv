# MDV-049 — Upstream mermaid-rs-renderer trial

## Outcome and scope

Owner requested a second branch using
`https://github.com/1jehuang/mermaid-rs-renderer` instead of Merman.
`codex/mermaid-rs-renderer` starts from clean `main@09d6394`; the separate
Merman implementation remains at `codex/merman-renderer@190d76a`.

The base already used Zed's fork of this library. This trial switches to the
original author's upstream commit `3726ccbffe0e8032361eb9668694b24f77858060`,
with default CLI/PNG features disabled. Reuse the existing SVG/RGBA worker,
cache, DOT, and LaTeX paths. Carry over the Mermaid alpha-compositing fix and
equivalent renderer tests from the Merman trial for a useful comparison.

## Acceptance

- CJK flowchart, sequence, class, state, and mindmap render through SVG/RGBA.
- Light/dark palettes, subtle alpha-composited borders, and user init overrides
  survive; malformed/oversized source returns errors.
- Run focused and full library/integration tests, cargo check, formatting,
  diff checks, sample PNG export, and a release build.
- Supply `tests/fixtures/renderer_trial.md` and a runnable local executable.
  Owner visual acceptance is pending until explicitly given.

## Identity and authority

The checkout is `/Users/harry_chung/Developer/Personal/rmdv`, using the owner's
fork as origin and minchenlee/rmdv as upstream. Latest upstream release was
checked as v0.10.1. Local rustc is 1.98.1 on macOS arm64; no installed `rmdv`
command was found. The previous Merman binary was copied to
`/tmp/rmdv-merman/rmdv` before rebuilding the shared Cargo output directory.

Owner requested a second local trial branch. The trial is retained on
`codex/mermaid-rs-renderer`. No push, merge, release, installation, or deployment
is authorized.

## Verification

- `cargo test --test diagram_render`: 8 passed.
- `cargo test --locked --lib --tests`: 627 passed, 2 ignored, no failures.
- `cargo check --locked`, `cargo fmt --all --check`, and `git diff --check`:
  passed.
- `cargo run --locked --example renderer_smoke -- /tmp/rmdv-mermaid-rs`:
  exported 1147 × 364 light/dark PNGs and SVGs through the actual app worker
  and raster path. Visually checked CJK labels, arrows, borders, and geometry.
- The upstream renderer honors theme backgrounds itself; removed the old
  hardcoded-white-background stripping workaround.
- The declared Rust floor is 1.88 because upstream uses edition-2024 let
  chains. Only the local 1.98.1 toolchain was tested, not the minimum toolchain.
- Existing warnings remain in the untouched app import, IPC test import, and
  dependency `block`; no unrelated cleanup was performed.
- `cargo build --locked --release --bin rmdv`: passed; the local executable
  reports `rmdv 0.10.1`. Native-window acceptance, Linux, and Windows are not
  verified. Do not treat the Merman branch's results as this branch's evidence.

## Reproduction

`cargo build --locked --release --bin rmdv`, then
`./target/release/rmdv tests/fixtures/renderer_trial.md`.
Close a running rmdv instance before comparing binaries (the app is
single-instance). `cargo run --locked --example renderer_smoke --
/tmp/rmdv-mermaid-rs` exports sample light/dark SVG and PNG.
