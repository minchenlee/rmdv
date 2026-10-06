# MDV-019 — Settings page

State: implementing
Owner / accountable lead: owner (minchenlee); implementation by Claude
Active writer: Claude
Created: 2026-10-06
Updated: 2026-10-06

## Outcome

rmdv has one Settings page that shows every user preference and its current
value, changes it in place, and keeps it across launches. Today each option is a
separate command-palette toggle or cycle: users cannot see what exists or what
is on, and some choices (font size, hidden files, Mindmap auto-center) are lost
on restart.

Owner direction (2026-10-06): add a Settings page; the sketch with Appearance,
Reading, Agent & CLI, and Advanced sections is approved as the starting point.
This replaces the earlier "Theme Settings and Theme Studio" scope of MDV-019.
The Theme Studio (custom-theme editor, import, website composer) moves to a
second phase, `MDV-019b`, built on this page. The pre-split candidate in
`.codex/worktrees/theme-settings-studio` is reference only (owner decision
2026-10-04: rebuild on the MDV-031 tokens, do not port).

## Surface

- **Entry points:** `⌘,` (Ctrl+, on Linux/Windows), the command palette entry
  "Open Settings", and a gear button in the sidebar header next to the `⌘`
  button.
- **Placement:** a full page in the reader area, the same pattern as the `⌘⇧F`
  workspace search page. The sidebar stays. The open document, scroll position,
  view mode, and Quick Slots are kept and come back unchanged on close.
- **Close:** `Esc`, `⌘,` again, or the close button. Closing restores the view
  that was open before.
- **Layout:** one centered column (the reader width), section headers, and one
  row per setting: label and a one-line hint on the left, the control on the
  right. About 14 rows fit without a navigation rail; a rail can come with the
  Theme Studio phase if the page grows.

```
┌ sidebar ─┬───────────────────────────────────────────────────────┐
│          │  Settings                                        ✕   │
│          │                                                       │
│          │  APPEARANCE                                           │
│          │  Theme              [■■ One Dark ▾]                   │
│          │    (inline list of theme cards with swatches)         │
│          │  Soft syntax colors                          [ ●  ]   │
│          │  Window glass       [ Off | Sidebar | Window ]        │
│          │    Turning glass on takes effect after a restart.     │
│          │    [Restart rmdv]                                     │
│          │  Glass opacity      60% ──────●──── 90%     80%       │
│          │  Font size          [ − ]  100%  [ + ]   Reset        │
│          │                                                       │
│          │  READING                                              │
│          │  Status footer                               [ ●  ]   │
│          │  Show hidden files                           [  ○ ]   │
│          │  Mindmap auto-center                         [ ●  ]   │
│          │                                                       │
│          │  AGENT & CLI                                          │
│          │  Focus window on agent navigation            [  ○ ]   │
│          │  Command-line tool  /usr/local/bin/rmdv  [Install CLI]│
│          │                                                       │
│          │  ADVANCED                                             │
│          │  Themes folder      [Open]   [Reload themes]          │
│          │  Preferences file   [Reveal in Finder]                │
└──────────┴───────────────────────────────────────────────────────┘
```

## Settings inventory

| Section | Setting | Control | Stored today | Change |
| --- | --- | --- | --- | --- |
| Appearance | Theme (presets + custom) | theme cards with swatches; selected card marked with a check, not only color | yes (`theme`) | — |
| Appearance | Soft syntax colors | switch | yes (`soft_syntax`) | — |
| Appearance | Window glass (macOS) | segmented: Off / Sidebar / Window | yes (`glass`) | restart note + button when the window was not created transparent |
| Appearance | Glass opacity (macOS) | slider 60–90 %, step 5 % | yes (`glass_opacity`) | disabled while glass is off |
| Appearance | Font size | − / value / + / Reset | **no** | add `font_scale` |
| Reading | Status footer | switch | yes (`show_footer`) | — |
| Reading | Show hidden files | switch | **no** | add `show_hidden` |
| Reading | Mindmap auto-center | switch | **no** | add `mindmap_autocenter` |
| Agent & CLI | Focus window on agent navigation | switch | yes (`auto_focus_on_nav`) | — |
| Agent & CLI | Command-line tool | status + Install CLI button | — | shows installed path or "Not installed" |
| Advanced | Themes folder | Open, Reload | — | existing commands |
| Advanced | Preferences file | Reveal | — | new: reveal `prefs.json` |

Out of scope: per-session layout state (sidebar open, sidebar width, Mindmap
panel width), Quick Slots, update channel, and anything without a working
backend today.

## Behavior

- Every change applies at once and is saved at once. There is no Save or Apply
  button. Theme cards behave like `⌘T` and the theme picker do today: choosing
  one switches and persists. (The old candidate's Apply/Cancel step is dropped
  for consistency with the existing theme commands.)
- The command-palette toggles and cycles stay. They and the page change the
  same state, so the page always shows the current value, including changes made
  over IPC or by the palette while the page is open.
- New `prefs.json` fields use serde defaults, so older files load unchanged and
  an unknown value falls back to the default without resetting other fields
  (the pattern used for `glass`).
- **Restart rmdv** relaunches the app the way the updater does (`open -n` on the
  bundle on macOS) after the user clicks it. It is shown only when a saved
  glass mode needs a transparent window that this launch did not create.
- Glass rows appear only on macOS.

## Keyboard

The page follows the vault-search page model, so it is usable without a mouse:

- `↑` / `↓` move a visible row cursor; the page scrolls to keep it in view.
- `Space` / `Enter` toggle a switch or press the row's button.
- `←` / `→` step a segmented control, the opacity slider, or font size; on the
  Theme row they move between theme cards.
- `Esc` closes; `⌘,` toggles.
- Mouse hover and click work on every control as usual.

## Visual rules

- Built only from the MDV-031 tokens: section headers in `muted` small caps,
  rows separated by `pal.border()`, row hover `pal.hover()`, cursor row
  `pal.active()`, controls with `radius::SM`, cards with `radius::MD`.
- Switch, segmented control, and slider are small shared widgets in
  `src/app/view/widgets.rs` so the Theme Studio phase can reuse them.
- Under window glass the page uses the reader fill rules from MDV-037, so it is
  translucent in whole-window mode like the reader.

## Implementation outline

1. `prefs.rs`: add `font_scale`, `show_hidden`, `mindmap_autocenter` with
   defaults; load them at startup; save on change.
2. `App`: `settings_open: bool`, `settings_cursor: usize`; messages
   `OpenSettings`, `CloseSettings`, `SettingsMove(i32)`, `SettingsActivate`,
   `SettingsStep(i32)`, plus direct setters (`SetGlass`, `SetGlassOpacity`,
   `SetFontScale`, …) shared with the palette commands.
3. `src/app/view/settings.rs`: the page; shared controls in `widgets.rs`.
4. `keys.rs`: `⌘,` / Ctrl+,; key routing while the page is open.
5. Sidebar header gear button; palette entry "Open Settings".
6. Restart button reuses the updater's relaunch helper.

## Acceptance

- Unit tests: prefs round-trip and old-file compatibility for the new fields;
  open/close restores the previous view (document, view mode, scroll); every row
  changes the same state as its palette command and persists; keyboard cursor,
  activate, and step on each control type; glass rows absent off macOS.
- `cargo test --release -- --test-threads=1`, the lean
  `--no-default-features` check, `cargo fmt --all --check`,
  `cargo build --release --bin rmdv`, `git diff --check`.
- Screenshots of the page in one dark and one light theme, with glass off and
  whole-window glass.
- Owner native acceptance in the Verify app.

## Phase 2 — MDV-019b Theme Studio (not in this plan)

Custom-theme editing on the Theme row, `Import Theme…`, and the website theme
gallery and composer from the earlier candidate. Planned after this page lands.

## Owner decisions (2026-10-06)

The owner approved the design (PR #52) and the interactive mockup, and said to
start implementation. The three open questions take the recommended answers:

1. Theme cards switch and save on click; there is no Apply/Cancel step.
2. A **Restart rmdv** button appears when a saved glass mode needs a restart.
3. The sidebar header has a gear button next to the `⌘` button.

## Progress

- [x] Owner approved the direction (2026-10-06).
- [x] Owner approved this design and the interactive mockup (2026-10-06).
- [ ] Implementation, tests, screenshots, PR.
- [ ] Owner native acceptance.

## Decision log

| Date | Decision | Why |
| --- | --- | --- |
| 2026-10-06 | MDV-019 becomes a general Settings page; Theme Studio becomes phase 2 (MDV-019b). | Owner: rmdv has no settings page; theme settings belong on it. |
| 2026-10-06 | One column with section headers, no navigation rail in v1. | About 14 rows; a rail adds navigation without saving scrolling. |
| 2026-10-06 | Recommended answers to the three open questions. | Owner approved the design and mockup and asked to start. |
| 2026-10-06 | Restart waits for this process to exit, then reopens the current file or folder. | The new instance must not meet the old one's IPC socket. |
