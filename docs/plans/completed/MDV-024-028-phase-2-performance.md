# MDV-024–MDV-028 — Phase-2 performance round and acceptance fixes

State: done
Owner / accountable lead: owner (minchenlee); implementation by Claude
Active writer: none
Created: 2026-10-05
Updated: 2026-10-05

## Outcome

Land the five remaining phase-2 performance items from the refactor roadmap
(MDV-022), verified in one owner acceptance round on a bundled verify app. Two
pre-existing bugs found during that round were fixed in the same round.

## Delivered

| Task | PR | Squash | Change | Measured (release build) |
| --- | --- | --- | --- | --- |
| MDV-024 | [#37](https://github.com/minchenlee/rmdv/pull/37) | `f60da48` | Sidebar builds the overscanned band near the viewport plus the first screenful; spacers keep the scroll height. | 4,000 rows: 4.98 → 0.158 ms per `view()` |
| MDV-025 | [#38](https://github.com/minchenlee/rmdv/pull/38) | `b971c35` | The editor's post-edit text is cached and reused as the next undo snapshot; undo snapshots compare in memcmp chunks. | 1 MiB mid-document insert: 4,283 → 1,500 µs, 9.84 → 4.92 MiB, 32,805 → 16,405 allocations per keystroke |
| MDV-026 | [#39](https://github.com/minchenlee/rmdv/pull/39) | `27fcd52` | Local images are read through a Task into the byte-budgeted `ImageCache`; no filesystem calls in `view()`; failed reads retry on reload. | — |
| MDV-027 | [#40](https://github.com/minchenlee/rmdv/pull/40) | `f280f82` | One settle timer per window Moved/Resized burst, sampled 250/600 ms after the last event. | 180-event burst: 180 → 2 tasks |
| MDV-028 | [#41](https://github.com/minchenlee/rmdv/pull/41) | `f4ad819` | Mindmap `sync_anim` runs only when the node `Arc` changes. | N≈2,000: 983 → 0.01 µs per redraw |

Acceptance fixes (both bugs were already on `main` before this round):

| PR | Squash | Fix |
| --- | --- | --- |
| [#42](https://github.com/minchenlee/rmdv/pull/42) | `1df1b13` | ⌘Z/⌘⇧Z/⌘Y were forwarded to iced's editor binding, so macOS inserted the letter and cancelled the app's undo. Undo/redo now restore only the changed span, so the editor keeps its scroll position and the cursor lands on the restored edit. A dirty document can be reopened; opening another file shows a "press ⌘S" toast. |
| [#43](https://github.com/minchenlee/rmdv/pull/43) | `0ed7305` | One wide folder (`~/Documents/Codex`, 12,764 entries) used up the 10,000-entry workspace budget, so later folders such as `GitHub` stayed empty shells. Expanding a folder the budget skipped now scans it on its own; results from a replaced snapshot are dropped, and each folder is scanned at most once per snapshot. |

## Acceptance evidence

- Owner native acceptance on `~/Applications/rmdv Verify.app` (2026-10-05):
  - Window events (#40) and the mindmap (#41) were accepted in round 1.
  - Round 2: ⌘Z in a long document stays in place (#42), `Documents/GitHub`
    expands with its files (#43), and local, remote, and missing images in
    `demo/guide/features/images/gallery.md` behave correctly (#39).
- The final verify build `codex/verify-phase2@d408864` passed
  `cargo test --release -- --test-threads=1` (452 library tests plus all
  integration suites), the lean `--no-default-features` check, and a release
  build.
- A real-tree probe for #43 found `GitHub` with 0 children and 0 files in the
  `~/Documents` scan. A dedicated scan of `GitHub` (607 ms, off the UI thread)
  returned 117 folders and 46 files.
- Every PR ran CI green on its final head (Linux and macOS tests, Windows
  check). Main push CI passed on every merge commit.
- `main@0ed7305` source equals the accepted verify build. The only difference
  is the order of tests in `src/app/tests.rs`, plus the docs and CI files from
  #34 and #36.

## Follow-ups

- Launching with a bare directory argument (`rmdv <dir>`) shows a
  "Couldn't open file — Is a directory" card next to the loaded sidebar. This
  is pre-existing and tracked as MDV-036.
- `tests/ipc_protocol.rs:292` has an unused `Section` import warning, which is
  pre-existing; fold it into the MDV-008 lint baseline.
