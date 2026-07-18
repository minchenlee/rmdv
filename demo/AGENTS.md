# Demo-vault guidance

## Scope

Applies to `demo/**`.

## Purpose

Owns the sample vault used to exercise rmdv's Markdown, diagrams, math,
mindmaps, PDF, search, theme, and edit-mode behavior. It is fixture/content
work, not application implementation.

## Start here

- [`README.md`](README.md) — vault map and feature tour
- [`RECORDING-SCRIPT.md`](RECORDING-SCRIPT.md) — human recording order and expected visuals
- [`tour.sh`](tour.sh) — optional real-keystroke macOS tour
- [`guide/`](guide/) — Markdown feature and installation pages
- [`reference/`](reference/) — nested API, JSON, and YAML examples
- [`papers/`](papers/) — TeX and PDF rendering fixtures

## Architecture and boundaries

- The directory is opened as a folder by the application; its nested paths,
  file extensions, and content are deliberately chosen to exercise the real
  workspace tree, parser, renderer, and search behavior.
- `README.md` is the landing page for the vault. `RECORDING-SCRIPT.md` and
  `tour.sh` are coupled to the fixture paths and shortcuts; renames or moved
  examples require updating both.
- Keep product behavior in `src/` and automated assertions in `tests/`. Demo
  content may expose a regression, but should not work around one with app code.

## Commands

- `cargo build --release && ./target/release/rmdv demo/` — build and launch the vault for manual inspection.
- `./demo/tour.sh --dry` — print the scripted tour without sending keystrokes; the script still requires the release binary and `cliclick`.
- `./demo/tour.sh` — run the real-keystroke macOS tour when Accessibility permission and a recording setup are available.

## Editing constraints

- Preserve the fixture names and relative paths used by `README.md`,
  `RECORDING-SCRIPT.md`, and `tour.sh`.
- Keep intentionally broken diagrams, deep directories, non-string YAML/JSON
  keys, TeX, and the PDF fixture when they are part of a feature demonstration.
- Avoid adding network-only or machine-specific requirements to ordinary demo
  pages; the manual tour's macOS `cliclick` dependency is explicitly scoped to
  `tour.sh`.

## Verification

1. Run `cargo build --release` when the demo or renderer contract changes.
2. Launch `./target/release/rmdv demo/` and inspect the affected fixture manually.
3. Run `./demo/tour.sh --dry` when fixture paths or tour order change.
4. Run `git diff --check` before handoff.

## Status and handoff

- [Current status](../PROJECT_STATUS.md)
- [Demo README](README.md)
- [Recording script](RECORDING-SCRIPT.md)
