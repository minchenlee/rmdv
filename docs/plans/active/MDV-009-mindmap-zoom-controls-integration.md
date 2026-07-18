# MDV-009 — Integrate Mindmap Zoom Controls

State: ready
Owner / accountable lead: unassigned
Active writer: none
Created: 2026-07-18
Updated: 2026-07-18

## Outcome

Retarget the anchor-preserving wheel, pinch, and keyboard zoom behavior from
`feat/mindmap-zoom-controls@46e3a6b` onto the merged Full Mindmap implementation
without regressing document Mindmap or workspace navigation.

## Non-goals

- Do not directly rebase the historical branch and assume conflicts encode the
  right product behavior.
- Do not change Full Mindmap selection, focus, preview, or panel policy except
  where zoom integration demonstrably requires it.
- Do not push or merge without explicit owner authority.

## Constraints and authority

- Preserve both document-level Mindmap and Full Mindmap as distinct modes that
  share canvas mechanics where appropriate.
- Zoom must preserve the pointer or selected-node anchor and respect min/max
  scale bounds across wheel, pinch, and keyboard input.
- Work in an isolated successor branch; keep the historical worktree intact as
  evidence until acceptance.

## Owned and excluded surfaces

- Owned: zoom-related changes represented by `46e3a6b`, shared canvas transform
  logic, focused tests, and native interaction evidence.
- Excluded: unrelated commits on the old Zoom branch, Full Mindmap feature
  redesign, release/publish work, and broad UI polish.

## Acceptance evidence

- A commit/diff classification shows what was ported, adapted, or dropped from
  the historical branch.
- Focused tests cover wheel, pinch, keyboard, scale clamping, both mindmap modes,
  and stale/layout transitions; relevant library/integration checks pass.
- Native A/B acceptance confirms smooth input and anchor preservation for both
  document and Full Mindmap on the exact candidate binary.
- The candidate receives a fresh static review before any merge request.

## Progress

- [x] Confirm historical worktree is clean at `46e3a6b` and diverges 6 behind /
  10 ahead from `origin/main@f75918c` by ancestry.
- [ ] Classify the historical Zoom diff against current canvas APIs.
- [ ] Build and verify an isolated successor candidate.
- [ ] Obtain native acceptance and owner integration decision.

## Decision log

| Date | Decision | Evidence / reason |
| --- | --- | --- |
| 2026-07-18 | Port behavior onto current main instead of blindly rebasing the old branch. | Full Mindmap was squash-merged and its canvas/state APIs changed after the Zoom base. |

## Blockers and escalation

- Native pinch/wheel acceptance needs a GUI-capable macOS session and exact
  executable identity.
- Publishing or merging the successor requires explicit owner approval.

## Final evidence

- Pending: classification, candidate SHA, static review, commands/results,
  native acceptance, and final branch disposition.
