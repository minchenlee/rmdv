# MDV-048 — Smooth pointer (mouse and trackpad) support

State: in progress
Owner / accountable lead: gh-rmdv-orch
Active writer: gh-rmdv-orch (one phase branch at a time)
Created: 2026-10-22
Updated: 2026-10-23

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

## Owner trial feedback (2026-10-23, P3 build)

- The folder picker's "Open This Folder" button goes: it looks heavy and is
  not needed. Double-click on a folder row opens that folder, as Enter does.
- Back to an earlier file lands on the saved position at once; that is fine.
- No alt text under images: accepted.
- The toolbar shows always by default, so a new user sees it; On hover stays
  a Settings option.
- The view switch (Read, Edit, Mind Map, Full Mindmap) uses icons; the
  tooltips keep the names and shortcuts.
- The file name sits at the horizontal center of the window.
- While the sidebar is hidden, the pointer at the left window edge reveals it
  over the content; it hides after the pointer leaves it.
- These go on `feat/pointer-polish` (stacked on P3); P4 is rebased on it.
- 2026-10-23 (P4 decisions):
  - The Full Mindmap node tooltip keeps the full path.
  - A click on the document root shows the document from the top in the
    panel, and its chevron folds every heading. Keep both.
  - A double click on a document mind map heading node opens the Reader at
    that heading (the root: the Reader from the top). The JSON/YAML data map
    and Full Mindmap keep their own double-click behavior.

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
- The file tree's New File, New Folder, Rename, and Move to Trash rescan the
  workspace synchronously, so a new item's row exists for its inline rename
  and no stale row remains. The scan is bounded; at the 10,000-entry budget
  it took 21–26 ms on a local SSD (P6 review, 2026-10-24), once per action.
  A slow network volume would stall longer; an async rescan with a local
  row patch is the fallback if the owner sees a hitch.
- Rename, New File, or New Folder chosen from the sidebar peek pins the
  sidebar open, as Keep Sidebar Open does, because the rename needs the tree.

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
- 2026-10-23: P1 done on `feat/pointer-p1` (PR #70): 17 items, 5 review fixes; lib 591 passed, all integration tests passed. Trial build: Release dry run 39814478737. P2 started on `feat/pointer-p2`.
- 2026-10-23: P2 done on `feat/pointer-p2` (PR #71, stacked on #70): 11 items, 7 distinct review findings fixed red-first; lib 648 passed, integration 726 passed. Trial build: Release dry run 39842358643. P3 started on `feat/pointer-p3`.
- 2026-10-23: P2 CI fix `eff6c56` (opaque toolbar under a native title bar; runner-speed-proof double-click tests); P2 trial build: Release dry run 39845309025, notarized.
- 2026-10-23: P3 done on `feat/pointer-p3` (stacked on P2): history (100 stops, scroll restore, dirty guard), mouse side buttons, `⌘[` `⌘]`, toolbar chevrons, link hover pill, in-app link targets and missing-file toast, linked images; 9 review findings fixed; lib 692 passed. P4 next.
- 2026-10-23: P3 CI fix `c28498c` (history harness ignores toast expiry); P3 trial build: Release dry run 39874226453, notarized.
- 2026-10-23: polish done on `feat/pointer-polish` (stacked on P3): owner feedback from the P3 trial (picker double-click, toolbar Always by default, icon view switch, window-centered file name, left-edge sidebar peek); 3 review findings fixed; lib 718 passed.
- 2026-10-23: P4 done on `feat/pointer-p4` (rebased on polish): mind map pointer model (act on release, click selects, chevron folds, no jump, double-click, hover, zoom buttons, pan clamp, panel close, divider, root node, Full Mindmap preview actions, document double-click to the Reader); 10 review findings fixed; lib 732 passed before the rebase.
- 2026-10-23: P4 rebased on polish (lib 758 passed); P4 trial build: Release dry run 39924838749, notarized.
- 2026-10-23: P5 done on `feat/pointer-p5` (stacked on P4): mind map "Scroll pans" setting (default off) and drag pan outside the canvas; rmdv image and diagram viewer (proportional zoom, pan, pinch, Fit/−/100%/+, double-click, click beside closes, sharper diagram raster); reader pinch and ⌘+scroll font size (coalesced, one prefs write per gesture); editor trackpad scroll at finger speed; macOS two-finger swipe Back/Forward through an AppKit scroll monitor (feel unverified headless); 7 review findings fixed; lib 807 passed, integration all passed.
- 2026-10-23: P5 trial build: Release dry run 39951643772, notarized (PR #75).
- 2026-10-24: P6 done on `feat/pointer-p6` (stacked on P5): shared in-app context menu; file tree menu with New File, New Folder, inline Rename and Move to Trash (macOS `NSFileManager`; other platforms show "not supported"); reader, Zen editor, mind map, outline, vault, finder and Quick Slot menus; tree row and sidebar divider double-click; middle click skipped (no target). 13 review findings: 12 fixed, 1 kept by decision (synchronous rescan). Trash tests clean up through a drop guard. lib 884 passed, integration all passed.

## Decision log

See the two decision sections above.

## Blockers and escalation

- Gesture feel (trackpad, swipe, pinch) needs the owner on device.
- Merges need owner authority per PR.

## Final evidence

Pending.
