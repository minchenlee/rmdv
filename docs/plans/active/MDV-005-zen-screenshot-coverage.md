# MDV-005 — Classify Zen screenshot coverage

State: proposed
Owner / accountable lead: unassigned
Active writer: none
Created: 2026-07-18
Updated: 2026-07-18

## Outcome

Determine reproducibly whether native rmdv screenshots can capture Zen
`text_editor` content, then either land a bounded fix or record the confirmed
Iced/platform limitation with a reliable alternative evidence path.

## Non-goals

- Do not re-open the already-fixed intermittent near-black-frame problem.
- Do not ship a permanent test harness, Accessibility automation, or private
  fixture unless the product genuinely needs it.
- Do not claim native visual acceptance from headless/widget tests alone.

## Constraints and authority

- Use an isolated app identity and verify the active executable; rmdv is
  single-instance and can otherwise route commands to stale code.
- Preserve any existing native screenshot runbook from the local Zen branch as
  evidence, but re-check it against current main before reuse.
- Temporary bundles, fixtures, and captures must have an explicit cleanup plan.

## Owned and excluded surfaces

- Owned: screenshot handling in `src/app.rs`, Zen editor capture reproduction,
  isolated test artifacts, and the native screenshot runbook if ported.
- Excluded: general Zen behavior, toast semantics, unrelated visual polish,
  release packaging, and broad Iced upgrades without a proven need.

## Acceptance evidence

- A/B captures on the exact binary and bundle identity reproduce or disprove the
  missing-editor-content symptom while ordinary rendered content remains valid.
- If fixed, focused tests plus repeated native captures prove the correction
  without regressing black-frame rejection/retry.
- If not fixable in scope, the plan records the platform boundary, alternative
  evidence path, and cleanup proof.

## Progress

- [x] Preserve the historical claim that offscreen capture may omit Zen editor
  content while retaining background/footer.
- [ ] Recover and revalidate the local Zen-branch screenshot runbook.
- [ ] Run controlled current-main A/B captures.
- [ ] Choose and verify a fix or bounded limitation record.

## Decision log

| Date | Decision | Evidence / reason |
| --- | --- | --- |
| 2026-07-18 | Keep editor-content omission separate from black-frame reliability. | The archived status reports distinct symptoms and acceptance contracts. |

## Blockers and escalation

- Requires a GUI-capable macOS session, Accessibility/capture permissions where
  applicable, and confirmation before material temporary cleanup.

## Final evidence

- Pending: exact binary/bundle identity, A/B captures, test results, cleanup
  result, and fix-or-limitation verdict.
