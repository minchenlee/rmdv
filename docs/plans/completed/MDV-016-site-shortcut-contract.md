# MDV-016 — Match website shortcuts to native rmdv

State: done
Owner / accountable lead: Codex
Active writer: none
Created: 2026-07-19
Completed: 2026-07-19

## Outcome

Reconciled every shortcut presented in the website's static reader table with
the current Rust event bindings and native UI terminology, then added a
deterministic contract check to prevent silent drift or interaction creep.

## Audit findings

- No website key combination was factually wrong.
- The fold row rendered `⌘K 0–6` like a simultaneous chord even though the app
  implements `⌘K`, then a separate level key.
- Five action labels were marketing paraphrases rather than the names used by
  the native command surfaces.
- Full Mindmap's `⌘⇧M` appeared in the workspace capability list but not in the
  main shortcut table.
- Website help was already separate, but its relationship to the native `⌘/`
  shortcut could be more explicit.

## Implementation

- Uses the native action names: `Find File in Workspace`, `Search All Files`,
  `Toggle Mindmap`, `Toggle Full Mindmap Mode`, `Toggle Zen Edit`, and
  `Fold to Level`.
- Shows the folding interaction as `⌘K` **then** `0–6`.
- Adds Full Mindmap to the main table and calls the plain-key row `Scroll
  Document`, matching the `j/k/g/G` handlers.
- Labels the overlay `Keyboard shortcuts for this website` and uses the native
  app's `⌘/` (`Ctrl+/` on Windows and Linux) binding to toggle it.
- Uses plain `p` for the website command palette because browsers can reserve
  the native app's `⌘⇧P` / `Ctrl⇧P` chord before the page receives it.
- Keeps the app rows as documentation only. The site does not capture native
  rmdv chords or launch visual replicas of app surfaces.
- Adds `site/check-shortcuts.mjs`, which fails when the website rows, structured
  feature keys, cross-platform modifiers, native handlers, or native labels stop
  agreeing, and when the removed preview interaction is reintroduced.

## Final evidence

- `node site/check-shortcuts.mjs` passed: seven website rows match native
  bindings.
- At 1440×900 and 390×844, the seven-row table remained legible and unclipped;
  the `then 0–6` sequence stayed readable at the mobile breakpoint.
- Browser checks confirmed that the static table has seven rows with no preview
  controls or preview dialog. `⌘/` opened and closed the website sheet, plain `?`
  no longer opened it, and plain `p` opened the palette from both body and
  focused-button contexts without stealing `p` typed into the palette input.
  Browser warnings/errors remained empty.
- `node --check site/app.js`, `node --check site/ghost.js`, JSON-LD parsing,
  duplicate-ID checks, exact inline-script CSP checks, the Impeccable detector,
  and `git diff --check` passed.

## Verification boundary

The contract proves current source-level equivalence between the public static
table and Rust implementation. No native binary was rebuilt or manually
exercised because the Rust behavior itself was not changed.

## Authority boundary

The website candidate remains uncommitted and was not pushed, published,
deployed, or verified on the live domain. The unrelated Rust and CJK changes in
the current `codex/fix-cjk-rendering` checkout were not modified.
