# MDV-039 — Release rmdv v0.8.0

State: implementing
Owner / accountable lead: Claude (gh-rmdv-orch)
Active writer: Claude
Created: 2026-10-07
Updated: 2026-10-07

## Outcome

Publish v0.8.0 from current `main` with synchronized package, site, and
release metadata, verified signed artifacts, and a live-verified site.

## Non-goals

- No product changes beyond the merged scope.
- No Theme Studio (`MDV-019b`).

## Constraints and authority

- The owner approved preparing v0.8.0 on 2026-10-07. Merge, tag, release, and
  site deployment each need explicit owner authority.
- `MDV-021` must merge before the tag (PROJECT_STATUS rule).

## Acceptance evidence

See the checklist in
[`docs/releases/v0.8.0-content-pack.md`](../../releases/v0.8.0-content-pack.md).

## Progress

- [x] Scope, version bump, release notes, content pack, site metadata.
- [x] MDV-021 merged with a green dry run (PR #55, `57badd8`; run 37559928672).
- [x] Release checks in CI (local disk too low; see the content pack).
- [x] Owner native smoke on rc2 and merge authority (2026-10-08).
- [ ] Tag, artifact verification, site deployment.

## Decision log

| Date | Decision | Evidence / reason |
| --- | --- | --- |
| 2026-10-07 | Release v0.8.0 from current `main`. | Owner approved; 30 commits since v0.7.0 include the Settings page, glass, and Quick Slots; growth plan F0. |
| 2026-10-07 | Land MDV-021 first. | PROJECT_STATUS requires fail-closed release gates before the next release candidate. |
