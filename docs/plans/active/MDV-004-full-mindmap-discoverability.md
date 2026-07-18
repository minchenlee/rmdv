# MDV-004 — Make Full Mindmap discoverable

State: ready
Owner / accountable lead: unassigned
Active writer: none
Created: 2026-07-18
Updated: 2026-07-18

## Outcome

Document the merged Full Mindmap workspace navigator in the README and in-app
shortcut guidance so users can find it without overstating release state.

## Non-goals

- Do not change Full Mindmap behavior, key bindings, visual design, or release
  claims as part of this documentation task.
- Do not deploy the site or publish a release.

## Constraints and authority

- Use the keys and wording proven by current source, not the historical status.
- Keep document Mindmap (`⌘M`) distinct from Full Mindmap workspace navigation.
- Public claims must describe merged behavior while remaining separate from a
  tagged/released-artifact claim.

## Owned and excluded surfaces

- Owned: `README.md`, the shortcuts-overlay content in `src/app.rs`, and any
  directly coupled static documentation.
- Excluded: interaction implementation, screenshots unless explicitly needed,
  site deployment, packaging, and release metadata.

## Acceptance evidence

- README feature and shortcut entries describe Full Mindmap accurately and
  distinguish it from document Mindmap.
- The in-app overlay exposes the same real key binding and concise purpose.
- Focused shortcut/view tests, static docs checks, rustfmt for touched Rust,
  and `git diff --check` pass.

## Progress

- [x] Confirm Full Mindmap implementation is present on remote `main` while the
  README currently names only document Mindmap.
- [ ] Verify the exact current shortcut and overlay architecture in source.
- [ ] Update the smallest coupled surfaces and tests.
- [ ] Review claims against tagged-release boundaries.

## Decision log

| Date | Decision | Evidence / reason |
| --- | --- | --- |
| 2026-07-18 | Treat discoverability as ready after PR #8 merge. | The historical acceptance/merge blocker is resolved; only accurate guidance remains. |

## Blockers and escalation

- If the owner wants a new shortcut or broader onboarding redesign, split that
  product change from this documentation task before implementation.

## Final evidence

- Pending: changed surfaces, exact key evidence, tests/static checks, diff or
  commit, and release-claim review.
