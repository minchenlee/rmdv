# rmdv project backlog

Last triaged: 2026-07-18

## State model

`proposed -> ready -> in_progress -> submitted -> verified -> done`

`blocked` and `deferred` are explicit side states. `done` means the acceptance
contract passed; code or prose merely existing is not sufficient.

## Active tasks

| ID | Priority | State | Outcome | Acceptance | Plan | Blocked by |
| --- | --- | --- | --- | --- | --- | --- |
| MDV-010 | P1 | ready | Reconcile local `main` with current remote `main` without duplicating or losing behavior. | Patch-equivalence map; clean isolated candidate; focused and broad checks; explicit publish decision. | [`MDV-010`](plans/active/MDV-010-local-main-reconciliation.md) | Publish/merge authority only after a candidate is proven. |
| MDV-001 | P1 | ready | Verify the Windows IPC lifetime fix on an actual Windows runner. | Build/package succeeds; non-empty app and setup `.exe` files, SHA-256 values, and a downloadable artifact are proven for the exact candidate. | [`MDV-001`](plans/active/MDV-001-windows-build-verification.md) | Requires a pushed CI candidate. |
| MDV-002 | P1 | ready | Bound search-result and highlight-cache memory while preserving visible behavior. | Explicit budgets and truthful truncation; focused regressions; measured memory evidence; relevant suites pass. | [`MDV-002`](plans/active/MDV-002-search-highlight-memory-bounds.md) | — |
| MDV-009 | P2 | ready | Retarget and review Mindmap Zoom Controls on merged Full Mindmap. | Clean candidate; focused/unit/integration checks; anchor-preserving native wheel, pinch, and keyboard acceptance. | [`MDV-009`](plans/active/MDV-009-mindmap-zoom-controls-integration.md) | No direct rebase of the old branch without classifying its commits. |
| MDV-003 | P2 | ready | Explain image-only PDFs instead of rendering an unexplained blank document. | Empty-text extraction reaches a clear OCR-disabled state; text PDFs remain unchanged; tests pass. | [`MDV-003`](plans/active/MDV-003-image-only-pdf-feedback.md) | OCR implementation is explicitly excluded. |
| MDV-004 | P2 | ready | Make merged Full Mindmap discoverable in public and in-app guidance. | README features/shortcuts and in-app shortcut overlay match real keys and behavior; documentation/static checks pass. | [`MDV-004`](plans/active/MDV-004-full-mindmap-discoverability.md) | — |
| MDV-005 | P2 | proposed | Determine whether native screenshot capture can include Zen `text_editor` content reliably. | Reproducible A/B captures classify the limitation and either land a tested fix or record a bounded platform limitation. | [`MDV-005`](plans/active/MDV-005-zen-screenshot-coverage.md) | Native macOS UI harness and active-app identity required. |
| MDV-006 | P2 | ready | Reconcile known stale statements in Zoom, KB hints, and benchmark docs. | Each named stale claim is checked against its owning code/branch and corrected without changing product behavior. | [`MDV-006`](plans/active/MDV-006-stale-documentation-reconciliation.md) | Zoom branch content must be edited in its owning checkout or a deliberate successor. |

## Deferred

| ID | Priority | Reason | Revisit trigger |
| --- | --- | --- | --- |
| MDV-007 | P2 | Initial workspace discovery examines at most 10,000 immediate entries, so an extremely wide directory may omit a later ordinary sibling. Current observed user roots are far below that shape. | A real affected directory, a product requirement for stronger guarantees, or a bounded algorithm proposal with measurements. |
| MDV-008 | P3 | Repository-wide formatting and Clippy have pre-existing debt; broad cleanup would obscure behavior changes. | A dedicated hygiene window with an agreed baseline and no feature diff. |

## Recently completed

| ID | Outcome | Evidence route |
| --- | --- | --- |
| MDV-C001 | Full Mindmap and Zen editing merged through PR #8. | [PR #8](https://github.com/minchenlee/rmdv/pull/8), squash `a8f8348`, and [`docs/status-history/2026-07-18-pre-control-plane-project-status.md`](status-history/2026-07-18-pre-control-plane-project-status.md). |
| MDV-C002 | CJK emphasis regression line merged through PR #7. | Remote `main` history and archived status. |
| MDV-C003 | v0.4.0 PDF release completed. | Tag `v0.4.0` and [`docs/v0.4.0-release-audit.md`](v0.4.0-release-audit.md). |

## Triage contract

- Keep stable IDs when tasks move state or priority.
- One task owns one observable outcome; split work when acceptance or authority
  boundaries differ.
- Add an active plan before a task becomes `in_progress`.
- Move accepted plans to `docs/plans/completed/` and keep only a short routing
  row here.
