# MDV-017 — Workspace Quick Slots

State: verified
Owner: Min-Chen Lee
Accountable lead / integrator: Codex `/root`
Acceptance owner: Min-Chen Lee
Created: 2026-07-27
Updated: 2026-09-02

## Outcome

Give keyboard-first readers nine persistent, workspace-scoped Quick Slots that
return to a file's latest reading context without keeping nine live documents
or competing with the document and Mindmap surfaces.

## Behavioral contract

### Slot identity and persistence

- Each active workspace owns exactly nine ordered slots, addressed as 1–9.
- Slot data lives in the user's rmdv configuration, never inside the workspace,
  and is grouped by the canonical workspace root.
- A file is stored as a path relative to that workspace root. Files outside the
  active workspace cannot be assigned, and assignment is unavailable when no
  workspace is active.
- Reopening the same workspace restores its slots. Changing workspace swaps to
  that workspace's bank. Moving the workspace root creates a different bank in
  v1; no path guessing or implicit migration is allowed.
- A slot stores a mode-appropriate reading context: the file path, Rendered or
  document-Mindmap mode and its restorable reading location, or Full Mindmap
  mode with the selected file preview and its relative preview scroll.
- The active slot checkpoints its latest accepted reading context as the user
  reads and before navigation away. Persistence must be bounded or debounced;
  ordinary scrolling must not perform an unbounded synchronous write stream.
- Raw/Zen is an editing surface, not a persisted reading mode. Dirty edits block
  slot navigation or closure. A clean switch out of Zen restores the target
  slot's saved reading mode.

### Activation, assignment, cycling, and closure

- `Command+1` through `Command+9` activates the corresponding slot on macOS;
  the platform primary modifier provides Control parity elsewhere.
- `Command+N` adds the current viewed file to the first empty slot and activates
  it. If the file already exists in a slot, that slot is activated without
  overwriting its saved reading context. If all nine slots are occupied, the
  bank remains unchanged and concise feedback is shown.
- Outside Zen, `Command+Up` and `Command+Down` cycle backward and forward
  through valid occupied slots, skip empty or missing entries, and wrap at the
  ends. The platform primary modifier provides Control parity elsewhere. In
  Zen, editor-native Command arrows retain cursor motion. Home/End and `g`/`G`
  retain reader top/bottom navigation.
- Clicking an empty slot assigns the current context; clicking an occupied slot
  activates it. Every occupied rail row persistently shows its filename.
- `Command+W` closes and clears the active slot. It activates the nearest valid
  occupied slot to the right, otherwise the nearest one to the left. If it was
  the only valid occupied slot, the file remains visible but becomes unslotted.
  With no active slot, `Command+W` is a no-op with concise feedback.
- `Command+Shift+W` owns Close Window so the Zen-style Close Slot / Close Window
  distinction is deterministic. Closing or clearing a slot never deletes its
  file.
- Clearing offers a bounded Undo action. `Clear All Quick Slots` is available
  from the Command Palette and has no global shortcut.
- Activating the already-current slot is a no-op. Activating an empty slot gives
  concise feedback. A missing-file slot remains visibly broken until cleared or
  overwritten and never redirects to a guessed path.

### Mode behavior

- Rendered activation loads the file and restores the saved relative body
  position after layout.
- Document Mindmap activation loads the file, remains in document Mindmap, and
  restores the saved mode-appropriate selection/location where representable.
- Activating a Rendered or document-Mindmap slot from Full Mindmap exits the
  filesystem navigator and restores that slot's saved file-content mode.
- In Full Mindmap, the currently viewed file is the selected file preview, not
  merely `App.file`. Assignment records that preview; activation stays in Full
  Mindmap, selects the corresponding file, and restores its preview position.
- Slot changes reuse the existing dirty-file and async request-identity guards.
  A stale load or scroll restore must never mutate a newer file or selection.

### Rail and interaction states

- The rail is hidden by default and appears only while the platform primary
  modifier is held. Modifier-only press and release must update it reliably.
- Nine 24 px square controls float at the far left, vertically
  centered, with 6 px rhythm and no enclosing side panel or decorative card.
- Every slot always shows its number centered in the square. Empty slots show
  only the square. Occupied slots add a stable, 24 px-high themed detail row
  with a more readable filename; active uses the existing accent fill, while
  occupied non-active slots use a persistent border/detail treatment so state
  does not rely on color alone. Missing files use the existing error role.
- Controls remain keyboard operable and retain visible focus treatment. Slot
  removal uses `Command+W`; Clear All remains in the Command Palette.
- Reveal/hide motion is a short 150–200 ms state transition when the widget
  system can express it without layout churn. The no-motion rendering remains
  complete and understandable.

## Hard constraints and non-goals

- Quick Slots are persistent bookmarks with a current context, not nine live
  editor buffers and not a replacement for the workspace tree or file finder.
- Do not keep unsaved source per slot, auto-save, discard edits, delete files,
  infer moved paths, or write metadata into the user's workspace.
- Do not disturb the existing `Command+0` reader-font reset.
- Preserve Full Mindmap request identities, bounded preview parsing, normal
  workspace navigation, and current theme semantics.
- Use the existing Iced, palette, typography, icon, compact-detail-row, toast,
  and overlay vocabulary. The generated palette and mock are direction
  references only;
  no generated raster belongs in the shipped application.
- Do not push, open or merge a PR, tag, release, publish, or deploy without new
  owner authority.

## Ownership and review gates

- One maker owns reconnaissance, implementation, focused tests, debugging, and
  corrections across the feature diff. Delegation depth is zero.
- The accountable lead owns this contract, integration decisions, and final
  acceptance. Product-policy or shared-interface changes must escalate.
- After maker submission, one read-only code reviewer evaluates the exact diff
  for Rust design, persistence boundaries, async identity safety, shortcut
  conflicts, and maintainability.
- After code review is accepted, one independent verifier evaluates observable
  behavior and acceptance evidence. Review and verification do not write
  product files; bounded corrections return to the maker.

## Acceptance evidence

- Serialization and path tests cover separate workspace banks, nine-slot
  bounds, relative-path enforcement, malformed/older config defaults, missing
  files, overwrite, clear, Undo, and no writes to the real test user's config.
- Shortcut tests cover physical digits, physical arrows, Command+N new-slot,
  modifier-only rail visibility, Command/Control parity, `Command+W`,
  `Command+Shift+W`, cycling, overlay/editing guards, and the retained
  `Command+0` binding.
- App-state tests cover assignment and activation, outgoing-context checkpoint,
  Rendered restoration, document Mindmap restoration, Full Mindmap preview
  restoration without mode exit, dirty-edit blocking, stale completions, empty
  slots, missing files, last-slot close, and nearest-slot selection.
- Static inspection confirms every rail state inherits theme roles, keeps
  occupied filename details visible without hover, and does not obscure
  existing overlays or side panels.
- Focused tests, `cargo check`, `cargo check --no-default-features`,
  `cargo test --lib`, `cargo test --tests`, focused rustfmt, and
  `git diff --check` pass on the exact candidate.
- A GUI-capable macOS smoke on the exact release binary confirmed Command
  press/release reveal, stable occupied filename rows, slot activation/cycling,
  Command+N, Zen close semantics, Full Mindmap retention, and rail placement.
  The owner also re-tested the parent-folder switching regression and confirmed
  that a document-Mindmap slot returns to its individual file mindmap mode.

## Stop and escalate when

- Correct persistence requires workspace-local files, unstable path guessing,
  or writes outside the existing rmdv configuration boundary.
- Full Mindmap cannot restore preview context without weakening request identity
  or bounded-work guarantees.
- Iced cannot distinguish the approved modifier or shortcut contracts reliably
  on a supported platform.
- A conflicting product shortcut, dirty-edit policy, or close-window behavior
  requires changing the approved semantics.
- Verification requires push, publication, destructive cleanup, or other
  authority not granted by the owner.

## Progress

- [x] Product behavior, persistence scope, palette, rail placement, and close
  semantics approved by the owner.
- [x] Isolated `codex/quick-slots` worktree created from clean local
  `main@67706d3` after refreshing `origin`.
- [x] Maker implementation and self-check submitted; the current checkpoint
  and navigation corrections are validated on the exact candidate.
- [x] Fresh read-only code review accepted the exact current diff, including
  checkpoint boundaries and manual-navigation active-marker persistence.
- [x] Fresh independent automated/static verification accepted the core
  candidate; the direct owner-requested rail/mode and shortcut-hint follow-up passes 375 tests.
- [x] Native macOS acceptance passed on the exact release binary, including the
  Full Mindmap parent-folder/document-slot regression.

## Decision log

| Date | Decision | Evidence / reason |
| --- | --- | --- |
| 2026-07-27 | Use nine slots so direct `Command+1`–`9` never conflicts with existing `Command+0`. | Owner decision. |
| 2026-07-27 | Persist one bank per canonical workspace root and store relative file paths in user config. | Keeps repository trees clean and prevents cross-workspace guessing. |
| 2026-07-27 | Preserve Full Mindmap and treat its selected file preview as the viewed file. | Owner-approved screenshot workflow. |
| 2026-07-27 | Use active-theme roles, persistent 24 px filename detail rows without inline Clear buttons, and a left-anchored transient rail. | Owner-approved direct polish; active-slot removal remains `Command+W`. |
| 2026-07-27 | Follow the Zen-style `Command+W` close-slot and `Command+Shift+W` close-window split. | Owner-approved shortcut model. |
| 2026-07-27 | Use one maker, then separate code review and behavioral verification. | Owner-approved ai-team-pm execution shape. |

## Final evidence

- Candidate: committed as `ca7a2f8` on `codex/quick-slots` and locally merged
  into `main`; no push, PR, release, or deploy was performed.
- Implementation: `src/quick_slots.rs`, `src/prefs.rs`, `src/app.rs`, and
  `src/lib.rs` provide nine canonical-workspace banks, relative-path safety,
  mode-aware restores, async request identity, direct assignment/activation,
  ArrowUp/ArrowDown cycling, Command+N tab-like creation, close/clear/Undo, the
  theme-aware 24 px rail with persistent occupied filename details,
  sidebar-safe positioning, and focus-loss cleanup. Rendered and document-
  Mindmap slots explicitly leave Full Mindmap and restore their saved content
  mode.
- Earlier read-only code review accepted bounded corrections for symlink
  containment, stale async completions, clean-Zen mode restoration and
  persistence, window-close checkpointing, rail/sidebar overlap, and stuck
  modifier state. A native-smoke finding that an active index alone could
  suppress a real bookmark jump was corrected so no-op identity now requires
  canonical file and visible reading-surface equality. Fresh code review and
  automated/static verification accepted the current checkpoint/navigation
  candidate; native GUI acceptance passed on the exact release binary.
- Direct validation for the current owner-requested follow-up passed targeted
  Quick Slot regressions, full serial release tests (414), and
  `cargo test --tests` (with one pre-existing unused-`Section` warning). The
  default and no-default `cargo check` gates both passed. The release build
  passed; its exact fresh arm64 binary is
  `/private/tmp/mdv-quick-slots-final-release/release/rmdv` with SHA-256
  `059e56605d7d500f32d6f4649ad083da1a5f77841789c7b65f3b4027eb73c721`.
  Focused rustfmt and `git diff --check` passed. Native acceptance was confirmed
  by the owner on the exact rebuilt release app.
- The earlier native macOS smoke used a temporary, uninstalled `.app` whose
  executable SHA-256 matched that prior candidate (`fdbf527c36d47d26…`). It
  covered the superseded bracket-cycle and hover-only clear behavior, so it is
  retained as historical evidence only and does not establish acceptance for
  the current ArrowUp/ArrowDown, Command+N, or persistent-detail contract.
- Native interaction acceptance for the current candidate was confirmed by the
  owner after the release rebuild, including the parent-folder edge case.
