# MDV-013 — Restore the ASCII wordmark

State: done
Owner / accountable lead: Codex
Active writer: none
Created: 2026-07-19
Completed: 2026-07-19

## Outcome

Restored the animated ASCII `rmdv` wordmark to the locally preferred
Impeccable landing-page variant without reverting to the first redesign's
campaign-style composition.

## Implementation

- Re-enabled the existing deferred `ghost.js` on the landing page.
- Placed its output in a dedicated README-style pane between the hero copy and
  real product screenshot.
- Reused One Dark / One Light theme roles for muted, accent, and bright glyphs.
- Sized the 96-column art responsively while preserving a fixed reserved height
  for zero layout shift.
- Retained the original one-frame reduced-motion path, 30 fps throttle,
  visibility pause, blur pause, and offscreen IntersectionObserver pause.

## Final evidence

- At 1440×900, the art rendered at 469×264 within a 1058×309 pane. The complete
  product workspace remained within the 1280px shell with no horizontal overflow.
- At 390×844, the art rendered at 308×174 within a 368×219 pane. The headline,
  stacked calls to action, wordmark, and screenshot remained unclipped with no
  horizontal overflow.
- Dark and light themes both rendered the wordmark using their existing color
  roles. The browser reported no warnings or errors.
- Browser sampling proved frames changed while the wordmark was visible and
  stopped changing once it was scrolled offscreen.
- The reduced-motion branch was verified in source: it renders one frame and
  never starts the ticker.
- `node --check site/ghost.js`, `node --check site/app.js`, the Impeccable
  detector, local asset checks, CSP checks, and `git diff --check` passed.

## Authority boundary

The preferred site variant remains uncommitted and was not pushed, published,
deployed, or verified on the live domain. Unrelated Rust and demo work present
in the checkout was not changed.
