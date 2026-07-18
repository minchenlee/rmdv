# MDV-006 — Reconcile stale documentation

State: ready
Owner / accountable lead: unassigned
Active writer: none
Created: 2026-07-18
Updated: 2026-07-18

## Outcome

Correct three known stale claims—Zoom commit state, fullscreen-exit behavior,
and measured-height behavior—against their current owning code and branch.

## Non-goals

- Do not change product behavior merely to make old prose true.
- Do not rewrite design history; mark superseded decisions and preserve why they
  changed.
- Do not edit the historical Zoom worktree from an unrelated checkout.

## Constraints and authority

- Verify each claim from its owning branch/source before editing prose.
- Preserve the difference between historical design intent and current
  implementation.
- Make branch-specific edits in that branch or an explicitly chosen successor.

## Owned and excluded surfaces

- Owned: the Zoom status statement that calls `46e3a6b` uncommitted,
  `docs/superpowers/specs/2026-06-14-kb-hints-custom-themes-design.md`,
  `docs/performance.md`/the actual benchmark owner, and routing notes.
- Excluded: Zoom integration itself (MDV-009), fullscreen implementation,
  virtual-scroll redesign, and broad documentation cleanup.

## Acceptance evidence

- Every edited statement cites or names the branch, commit, code path, or test
  that makes it current.
- Historical text is retained or explicitly marked superseded where it still
  explains a past design decision.
- Link checks or direct path checks and `git diff --check` pass; no product code
  changes are included.

## Progress

- [x] Identify the three stale claim families in the archived status.
- [ ] Verify Zoom status in its owning worktree and choose the successor doc.
- [ ] Verify fullscreen-exit and measured-height behavior from current source.
- [ ] Apply scoped corrections and inspect the docs-only diff.

## Decision log

| Date | Decision | Evidence / reason |
| --- | --- | --- |
| 2026-07-18 | Reconcile by ownership, not by global search-and-replace. | The stale claims span different branches and historical/current document roles. |

## Blockers and escalation

- If the Zoom branch is replaced during MDV-009, update its status in the
  chosen successor rather than creating competing truth in current main.

## Final evidence

- Pending: claim-to-evidence table, changed files, diff check, and any branch-
  ownership decision.
