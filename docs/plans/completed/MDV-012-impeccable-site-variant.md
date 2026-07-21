# MDV-012 — Impeccable landing-site variant

State: done
Owner / accountable lead: Codex
Active writer: none
Created: 2026-07-18
Completed: 2026-07-18

## Outcome

Produced a second, locally reviewable landing-site redesign using the
Impeccable product register, after preserving the first redesign and restoring
the exact original site as the new variant's starting point.

## Non-goals honored

- No commit, push, publication, deployment, or live-site replacement occurred.
- The first redesign and the older unrelated stash were preserved.
- Rust application behavior and release artifacts were not changed.

## Design decision

The alternate design treats the page as a literal rmdv workspace: a desktop
outline rail, document path, product reading pane, real captures, row-based
capabilities, and structural mobile collapse. It retains the One Dark / One
Light colors, system typography, JetBrains Mono for code, keyboard layer,
command palette, and screenshot lightbox. Decorative grid texture, repeated
marketing scaffolds, wide ghost shadows, and ornamental motion are absent.

## Final evidence

- The first redesign is preserved in Git stash commit
  `5c78ae910b809d12a41fa71c2b6a3712e6c2f3fe`, created from clean base
  `7a0514dd9a2bb9079449ebf7780ef317b184ac42` with untracked files included.
- The checkout was verified clean at that base before this variant began.
- `node --check site/app.js`, `node --check site/ghost.js`, JSON-LD parsing,
  duplicate-ID checks, local-asset checks, exact CSP hash checks,
  `git diff --check`, and the Impeccable detector passed. The detector returned
  no findings after the final copy and lightbox cleanup.
- Text contrast checks reached at least 4.62:1 for the tested body, muted,
  subtle, and accent text roles across both themes.
- Browser acceptance passed at 1440×900 and 390×844 in light and dark themes.
  The landing page and 404 had no horizontal overflow; every lazy product
  capture loaded at its intrinsic width.
- The theme toggle, 15-command palette, focus restoration, screenshot
  lightbox, and keyboard scrolling were exercised. No interaction regression
  was found in the unchanged keyboard layer.
- The first and second variants were rendered from separate local servers at
  matched desktop and phone viewports for visual comparison.

## Remaining authority boundary

The Impeccable variant remains uncommitted in the working tree. Choosing a
variant, committing, pushing, publishing, and verifying the live domain are
separate owner decisions.
