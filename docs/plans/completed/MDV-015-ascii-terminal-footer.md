# MDV-015 — Move the ASCII wordmark into a terminal footer

State: done
Owner / accountable lead: Codex
Active writer: none
Created: 2026-07-19
Completed: 2026-07-19

## Outcome

Moved the animated ASCII `rmdv` wordmark out of the product introduction and
made it the dominant closing gesture of a dark terminal-style footer.

## Design decision

- The hero now moves directly from product copy to the real workspace capture,
  keeping the product itself above the fold.
- The footer remains One Dark in both page themes, making the end of the page
  feel like a deliberate terminal surface rather than another neutral section.
- A restrained `rmdv: native markdown workspace viewer` and `[EOF]` bar frames
  the art. Identity, license, source, changelog, Iced, and AI-assistance details
  sit in a quieter metadata row below it.
- The wordmark scales from a complete mobile signature to a large desktop
  closing field without changing the existing glyph renderer.

## Final evidence

- At 1440×900, the footer formed a single large closing field with the animated
  wordmark centered above the metadata. The tightened hero placed the workspace
  screenshot immediately after the introduction.
- At 390×844, the terminal bar, complete wordmark, identity, links, and
  disclosure remained inside one unclipped footer frame.
- Dark and light page themes both kept the footer legible; the intentional dark
  terminal surface did not depend on theme-specific text roles.
- Browser sampling proved the wordmark changed while in view and stopped after
  scrolling offscreen. The browser warning/error log remained empty.
- The one-frame reduced-motion branch and visibility/blur gating in `ghost.js`
  were preserved.
- `node --check site/app.js`, `node --check site/ghost.js`, exact inline-script
  CSP checks, JSON-LD parsing, duplicate-ID checks, the Impeccable detector, and
  `git diff --check` passed.

## Authority boundary

The preferred site candidate remains uncommitted and was not pushed, published,
deployed, or verified on the live domain. The unrelated Rust and CJK changes in
the current `codex/fix-cjk-rendering` checkout were not modified.
