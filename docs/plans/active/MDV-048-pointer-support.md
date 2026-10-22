# MDV-048 — Smooth pointer (mouse and trackpad) support

State: in progress
Owner / accountable lead: gh-rmdv-orch
Active writer: gh-rmdv-orch (one phase branch at a time)
Created: 2026-10-22
Updated: 2026-10-22

## Outcome

rmdv stays keyboard-first, but every flow can also be done with the pointer:
no dead ends, a context menu where a Mac user expects one, consistent gestures,
and visible controls for actions that today exist only as shortcuts.

Audit (read-only, 2026-10-22, `main@75b637c`, every item with file:line):
`~/Documents/alfred-status/reports/rmdv-pointer-audit/README.md`.

## Non-goals

- Do not change or remove an existing keyboard shortcut. Pointer paths are
  additive. `⌘M` stays Mind Map and `⌘W` stays Close Slot.
- Do not open a visible rmdv window during automated work; verify with unit
  tests and the headless harness. Gesture feel is verified by the owner.
- Do not push to `main`, merge, tag, or release without owner authority.

## Owner decisions (2026-10-22)

- Scope: all nine audit directions (A–I).
- Toolbar: hidden until the pointer hovers the top edge; a Settings option
  shows it always.
- Mind map branch click: select the node and open the panel. A chevron on the
  node collapses and expands.
- File tree context menu includes Rename, New File, New Folder, and Move to
  Trash.
- Lists (palette, finder, theme, picker, vault results): delegated to the
  orchestrator. Decision: hover moves the selection only after real pointer
  motion (`mouse_area::on_move`), the same as Settings, so a still pointer
  never steals the selection while the arrow keys scroll the list.

## Orchestrator decisions (reversible; listed for owner review)

- Mind map two-finger scroll keeps zooming by default; "scroll pans" is a
  Settings option.
- Context menus are drawn inside rmdv (positioned overlay), not a native
  `NSMenu`, because a native pop-up menu blocks the Iced event loop.
- Task checkboxes stay read-only (no file write); they get a checkbox look.
- Math blocks get an explicit copy button, as code blocks do; click-anywhere
  copy is removed.
- CJK word selection uses `CFStringTokenizer` on macOS and keeps UAX #29
  elsewhere.
- Buttons keep Iced's pointing-hand cursor (an arrow cursor needs a custom
  wrapper that conflicts with `Stack` levitation).
- The native menu bar is hand-written with `objc2-app-kit`, macOS only, the
  same bridge style as `src/native_pinch.rs` and `src/macos_open.rs`.
- Close protection covers the red close button and `⌘Q`.

## Phases (one branch and PR each, stacked)

| Phase | Branch | Scope |
| --- | --- | --- |
| P1 | `feat/pointer-p1` | A: overlay correctness; quick wins: edit-mode focus, welcome buttons, tree/outline cursor sync, folder drop, heading blank-area fold, image cursor, tooltips with action names, Full Mindmap exit pill |
| P2 | `feat/pointer-p2` | B: hover toolbar (setting), mode switch, Save and dirty state, sidebar toggle, Zen exit, window drag, drop highlight, recent items; C (part): close protection |
| P3 | `feat/pointer-p3` | E: navigation history, mouse back/forward, `⌘[` `⌘]`, link hover and targets |
| P4 | `feat/pointer-p4` | G: mind map pointer model (act on release, select vs collapse, no jump, double-click, fit and zoom buttons, hover, panel close) |
| P5 | `feat/pointer-p5` | F: gestures (scroll-pans option, image viewer, reader pinch font size, editor scroll speed, swipe) |
| P6 | `feat/pointer-p6` | D: context menus, double-click, file operations |
| P7 | `feat/pointer-p7` | H and I: selection from margins, auto-scroll, CJK words, outline scroll-spy, Quick Slot pointer entry, hit targets, divider persistence |
| P8 | `feat/pointer-p8` | C: native menu bar |

## Acceptance evidence

Per phase:

- A failing test first for each behavior change (unit test on `App::update`
  or the widget), then green.
- `cargo fmt --all --check`, `cargo check`, `cargo check --no-default-features`,
  `cargo test --release --lib`, `cargo test --release --tests`, `git diff --check`.
- Independent review (correctness, Iced 0.14 compliance, render-path cost) with
  each finding verified before it is fixed.
- Owner trial build for pointer feel before merge.

## Progress

- 2026-10-22: audit done; owner decisions recorded; P1 started.

## Decision log

See the two decision sections above.

## Blockers and escalation

- Gesture feel (trackpad, swipe, pinch) needs the owner on device.
- Merges need owner authority per PR.

## Final evidence

Pending.
