# MDV-048 — Smooth pointer (mouse and trackpad) support

State: in progress
Owner / accountable lead: gh-rmdv-orch
Active writer: gh-rmdv-orch (one phase branch at a time)
Created: 2026-10-22
Updated: 2026-10-24

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
- P7 (2026-10-24): Quick Slots get a pointer entry: a toolbar button that
  toggles the same rail as a click-pinned popover; the ⌘-hold display
  stays.
- P7: no document breadcrumb; the centered toolbar file name covers it.
- P7: drag-out to Finder is deferred; it needs an `NSDraggingSource`
  bridge.
- P7: the Zen editor gets no drag auto-scroll or scrollbar in this phase.
  Iced 0.14's `text_editor` reports a drag only while the pointer is inside
  its bounds (`cursor.position_in(bounds)?`, `text_editor.rs:1311`, and
  only for a single-click drag) and draws no scrollbar, so both need a
  vendored `text_editor` or a wrapper that replays `Action::Scroll` and a
  clamped `Action::Drag`; left for a later phase.
- P7: the reader's drag auto-scroll ticks every 16 ms only while a
  selection drag is past the viewport's top or bottom edge, by a quarter of
  the overshoot per tick (2 to 60 px); a release anywhere stops it.
- P7: CJK words come from the system dictionary through
  `CFStringTokenizer`, so the split follows macOS (for example 中文 and
  測試 are words, 分詞 may stay two). Keyboard word moves keep UAX #29.
- P7: a block's copy button (code, math, diagram) confirms with a check
  for 1.5 s instead of the "Copied" toast; menu copies keep the toast.
  Reader blocks take no keyboard focus, so the button shows on hover
  only.
- P7: the reader scrollbar fades only in the reader; the sidebar, panel,
  and table scrollbars keep the instant show and hide.
- P8: menu bar app items show their shortcuts only while their menu is
  open (`menuWillOpen:` sets the key equivalent, `menuDidClose:` clears
  it), so every key still reaches the window and keeps its per-focus
  behavior. An `NSMenuDelegate` that answers
  `menuHasKeyEquivalent:forEvent:target:action:` with NO did not stop
  AppKit: it still found the item and took the key, even for a disabled
  item (AppKit test, 2026-10-24). Only Hide ⌘H, Hide Others ⌥⌘H and
  Quit ⌘Q (the keys winit's menu already took) take their keys from the
  menu.
- P8: Enter Full Screen has no key equivalent: a native key is permanent,
  and ⌃⌘F stays Find in Document through the Control-as-Command alias
  (no existing shortcut changes). Full screen stays on the green button
  and the menu click. Making ⌃⌘F the macOS full-screen key needs an owner
  decision. ⌘M stays Mind Map (no Minimize key) and ⌘W stays Close Slot.
- P8: a menu click runs what the item's shortcut runs on the surface that
  has the keys now, and the item is disabled where the key does
  something else. Edit > Copy copies the editor's selection or a
  whole-document reader selection; a partial reader selection still
  copies with ⌘C only.
- P8: the main window's frame (size, position, zoomed) is saved in prefs
  once it has been still for 500 ms after a move or resize, and on close;
  full screen is never saved or restored (a zoomed window keeps its normal
  frame under the zoom; with no normal frame saved yet it saves the launch
  size, centered, so the zoomed size never becomes the normal one). On launch, macOS reads the screens' visible areas
  from AppKit before the window opens (it answers before `NSApplication`
  exists) and puts the window fully on the screen it overlaps most, or the
  nearest one, at least 640×420. Other platforms have no screen list
  before the window opens, so they restore the size and center the window;
  once it is open, a window larger than its monitor (less 16×80 for the
  frame, title bar and taskbar) shrinks to fit and stays centered.
  A close cannot ask the window, so a zoom or unzoom in the last 500 ms
  before a close is saved with the previous zoom state.
- P8b (2026-10-24, owner feedback from the P8 trial): Full Mindmap has no
  floating nav island. Its actions are toolbar buttons while it shows:
  Back is the Read segment, and Open Parent Folder (disabled at the
  filesystem root) and the panel toggle (⌘⌥B) sit after the view switch.
  Read's tooltip names Esc only when Full Mindmap was entered from Read
  (Esc returns to the view it was entered from). Both buttons hide while
  Settings covers Full Mindmap. The bottom zoom and hint island stays. In
  On hover mode the toolbar reveals over Full Mindmap as it does over the
  reader.
- P8b: Quick Slots are one global bank of nine, saved as
  `global_quick_slots` in `prefs.json`. A slot keeps the workspace root it
  was assigned in, its path relative to that root, and its context, so a
  rename or move inside that root still updates it and a slot from another
  workspace with the same relative path does not. A slot of the open
  workspace opens as before; a slot of another workspace opens that
  workspace the way Open Folder does (in Full Mindmap, through its bounded
  index), then the file with its context. A gone root or file shows the
  missing-slot toast and opens nothing. The bank is seeded once: at launch
  from the most recently opened folder whose old bank has a slot, else from
  the first workspace that opens; banks are never merged. The old
  per-workspace `quick_slots.banks` stay unchanged and unused, so a
  downgrade still finds them. The rail tooltip and the Go menu title add
  the workspace folder name for a slot of another workspace ("b.md —
  notes"); the title stays the file name, and the replace-slot
  confirmation names it too. A later slot press, assignment, clear, or
  close cancels a slot still waiting for its workspace. The toolbar Quick
  Slots popover opens with no workspace open; assigning still needs one.

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
| P8b | `feat/pointer-p8b` | P8 trial feedback: Full Mindmap actions in the toolbar, global Quick Slots |
| P8c | `feat/pointer-p8c` | P8b trial feedback: sidebar peek hides at once and matches the docked sidebar; a Quick Slot switch keeps the sidebar |

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

- 2026-10-24: P7 reader selection on `feat/pointer-p7` (stacked on P6): a press in the margins, between blocks, or below the end starts a selection at the nearest text (I-beam there); drag past the viewport edge auto-scrolls and the selection follows across virtual window rebuilds; macOS double-click selects Chinese and Japanese dictionary words (`CFStringTokenizer`, vendored `iced_selection::word`); double/triple-click drags keep word/line granularity across blocks; Shift+click extends within and across blocks.
- 2026-10-24: P7 reader affordances on `feat/pointer-p7`: hover-revealed copy buttons on code, math, and diagrams with a check after a copy (math click-anywhere copy removed), "Click to enlarge" on diagrams, read-only task checkboxes (not-allowed cursor, "Read-only"); reader scrollbar 150 ms fade (ticks only while fading), 6→10 px thumb on hover, 12 px hit area; wide-table edge shades. lib 909 passed, integration all passed.
- 2026-10-24: P7 sidebar and chrome on `feat/pointer-p7`: the outline marks the section at the reader's top line (recomputed only when the top block changes) and follows it while the pointer is off the outline; sidebar divider hover line, width saved in prefs (lenient), drag below the minimum collapses and a re-open restores the width, a lost release ends the drag; a toolbar Quick Slots button pins the rail as a popover (outside press, Esc, or a choice closes it; unassignable empty slots look disabled); ghost buttons, sidebar tabs, and slot chips 28 px with pressed states, toast buttons answer hover and press, whole-row Settings switches; the footer pill cycles its statistic (saved), the shortcut sheet has an X, clickable rows, and the missing ⌘, ⌘⇧M ⌘Z j/k Space ←→ rows (checked against the palette), palette shortcuts are right-aligned key caps that typing does not match, and the palette, finder, and vault fields have clear buttons. lib 939 passed, integration all passed.
- 2026-10-24: P7 review on `feat/pointer-p7`: 13 findings (9 distinct after duplicates), all fixed red-first: losing window focus stops the drag auto-scroll and drops the held press; a Shift press beside the text lets a copy button, image, or chevron take it first (a Shift press on text, links included, still extends); the outline scrolls to its mark when shown again and an outline or scrollbar strip removed under the pointer drops its hover (one check after every update); the pinned Quick Slots popover unpins whenever the rail may not show, and its padding takes presses (`opaque`); the wide-table shade draws in its own layer above the table (pixel test); the copy check is keyed by block, not by text; word drags keep the last four lines' segments and one `CFStringTokenizer` per thread (200 moves over a 1000-character Chinese line: 11.7 s before, under 0.1 s after). lib 949 passed, integration all passed.
- 2026-10-24: P6 trial build: Release dry run 39991024040, notarized (PR #76). P7 verified by the orchestrator: lib 949 passed, integration all passed, release build, Linux title bar simulation 338 passed with the 1 expected macOS-only failure.
- 2026-10-24: P8 done on `feat/pointer-p8` (stacked on P7): native macOS menu bar (rmdv, File with Open Recent, Edit, View, Go with Quick Slots, Window, Help) built once with `objc2-app-kit` beside `native_pinch::install`; one target sends item tags through a subscription (`Message::NativeMenu`); a click runs the shortcut's `key_message` on the current surface; item state, slot titles and the recent list are pushed only on change and applied in `validateMenuItem:` / `menuNeedsUpdate:`. AppKit test target `native_menu_appkit` (no window). lib 966 passed, integration all passed.
- 2026-10-24: P8 window frame persistence on `feat/pointer-p8`: size, position and zoom saved once per settled move or resize (500 ms) and on close, lenient load, restore clamped onto a visible screen (macOS) or centered (elsewhere); full screen not restored. AppKit test target `window_frame_appkit` (no window); a minimized (0×0) window keeps the saved frame. lib 986 passed, integration all passed.
- 2026-10-24: P8 review on `feat/pointer-p8`: 8 findings, all fixed red-first: Edit items stay disabled while the vault page or Settings covers the Zen editor; Enter Full Screen has no key, so ⌃⌘F stays Find in Document; hover, scroll and tick messages skip the menu computation, and `WindowMoved` runs the update epilogue once; a first zoomed save keeps the launch size as the normal frame; off macOS a window larger than its monitor shrinks to fit once it opens. lib 992 passed, integration all passed.
- 2026-10-24: P7 trial build: Release dry run 40024937189, macOS aarch64 notarized (PR #77). The first run failed on the Linux CJK double-click test (no CJK font sets glyph widths there), fixed in the P7 test and carried onto P8. macOS x86_64 failed twice on `hdiutil detach` "Resource busy" after notarization, a release-script issue outside MDV-048. P8 verified by the orchestrator: lib 992 passed, integration all passed, release build, Linux title bar simulation 348 passed with the 1 expected macOS-only failure.

- 2026-10-24: P8b on `feat/pointer-p8b` (stacked on P8): the Full Mindmap nav island is gone; Open Parent Folder and the panel toggle are toolbar buttons while Full Mindmap shows, and Read leaves it, in both toolbar modes. Quick Slots are one global bank: a slot of another workspace opens that workspace, then its file with its context; seeded once from the most recent workspace's old bank, which stays unchanged.
- 2026-10-24: P8b review: 7 findings, all fixed red-first: the superseded-switch test starts with no active slot (the stale marker came from the assignment); a newer slot action cancels a waiting cross-workspace slot; the Quick Slots popover opens with no workspace; the replace confirmation names the old slot's workspace; Full Mindmap's toolbar buttons hide under Settings; Read names Esc only when Esc returns to Read; `QuickSlot::in_workspace` compares keys without canonicalizing. lib 1013 passed.
- 2026-10-24: P8 trial build: Release dry run 40040570298, all four platforms built, macOS aarch64 notarized (PR #78). P8b verified by the orchestrator: lib 1013 passed, integration all passed, release build, Linux title bar simulation 365 passed with the 1 expected macOS-only failure; the real prefs file was not written by the tests.
- 2026-10-25: P8c on `feat/pointer-p8c` (stacked on P8b), from the P8b trial: leaving the sidebar peek hides it at once (the delay stays only after a band entry the peek never confirms); the peek uses the docked sidebar's palette, no heavy shadow, and under glass an opaque tint of the sidebar over the reader color; a Quick Slot of another workspace opens it without showing the sidebar. Verified: lib 1014 passed, integration all passed, `--no-default-features` check, release build, Linux title bar simulation 366 passed with the 1 expected macOS-only failure; the real prefs file was not written.

## Decision log

See the two decision sections above.

## Blockers and escalation

- Gesture feel (trackpad, swipe, pinch) needs the owner on device.
- Merges need owner authority per PR.

## Final evidence

Pending.
