# MDV-010 — Reconcile local and remote main

State: deferred
Owner / accountable lead: unassigned
Active writer: none
Created: 2026-07-18
Updated: 2026-07-18

## Outcome

Produce a clean, reviewable candidate from current `origin/main` that preserves
only local-main behavior still missing after PR #8, without duplicating squash-
merged work or losing the local branch as evidence.

## Non-goals

- Do not force-update `main`, delete branches, publish, merge, tag, or release.
- Do not assume ancestry divergence equals behavior divergence.
- Do not combine unrelated product enhancements merely because they share the
  old local-main line.

## Constraints and authority

- Start from a fresh isolated branch at then-current `origin/main`.
- Preserve local `main@67564e5` until the owner explicitly approves a final
  integration or archival route.
- Classify patch equivalence before cherry-picking; PR #8 is a squash and may
  already contain several local-main changes under different commit IDs.
- Request owner authority before any remote mutation.

## Owned and excluded surfaces

- Owned: read-only comparison of `main`, `origin/main`, PR #8, and the eight
  currently local-main-only commits; an isolated candidate and tests for truly
  missing behavior.
- Excluded: `feat/mindmap-zoom-controls` (MDV-009), release/tag state, and broad
  cleanup unrelated to the classified patches.

## Acceptance evidence

- A per-commit table classifies each local-only commit as already represented,
  still required, superseded, or intentionally archived, with diff evidence.
- The candidate is clean, based on current `origin/main`, and contains no
  accidental status-history reintroduction.
- Narrow tests for every retained behavior plus `cargo check`, relevant library
  and integration suites, focused rustfmt, and `git diff --check` pass.
- Any publish/merge step names the exact candidate SHA and has explicit owner
  approval.

## Progress

- [x] Record 2026-07-18 ancestry: local `main` is 5 behind / 8 ahead of
  `origin/main@f75918c`.
- [x] Re-check after the v0.5.0 release line: local `main` and `origin/main`
  both resolve to `7a0514d`, so the original divergence predicate is absent.
- [ ] Build the patch-equivalence classification.
- [ ] Produce and verify the isolated candidate.
- [ ] Present the integration/archival decision to the owner.

## Decision log

| Date | Decision | Evidence / reason |
| --- | --- | --- |
| 2026-07-18 | Compare behavior before applying commits. | PR #8 was squash-merged, so commit ancestry alone overstates missing work. |
| 2026-07-18 | Defer while local and remote `main` are identical. | The patch-equivalence audit was not completed, so the task is not done; however, ordinary reconciliation has no current divergent branch to operate on. |

## Blockers and escalation

- Revisit only if the owner requests an audit of historical local
  `main@67564e5`, or if local and remote `main` diverge again. Remote publication
  and final branch disposition still require explicit owner authority.

## Final evidence

- Pending: classification table, candidate SHA/diff, commands/results, owner
  decision, and remaining branch state.
