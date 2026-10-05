# rmdv project backlog

Last triaged: 2026-10-05

## State model

`proposed -> ready -> in_progress -> submitted -> verified -> done`

`blocked` and `deferred` are explicit side states. `done` means the acceptance
contract passed; code or prose merely existing is not sufficient.

## Active tasks

Roadmap order (owner-approved 2026-10-04, re-confirmed 2026-10-05): phases 0–2,
MDV-023, MDV-029, and MDV-024 to MDV-028 have landed (PRs #28–#43). Next is
phase 3 visuals (MDV-031, then MDV-019), with the remaining phase-4 items
(MDV-008, MDV-030) alongside. Phase 5 tasks (MDV-032 to MDV-035) start with a
design for the owner, not code. Sources:
[`MDV-022`](plans/completed/MDV-022-refactor-phases-0-2.md) and
[`MDV-024–028`](plans/completed/MDV-024-028-phase-2-performance.md).

| ID | Priority | State | Outcome | Acceptance | Plan | Blocked by |
| --- | --- | --- | --- | --- | --- | --- |
| MDV-008 | P2 | ready | Clippy baseline: report-only in CI first, then tighten lint by lint. (The rustfmt half landed in PR #28; include the existing unused `Section` import warning in `tests/ipc_protocol.rs`.) | Recorded warning count; each tightening PR is lint-only with no behavior diff. | — | — |
| MDV-030 | P2 | proposed | Consolidate HTTP/TLS dependencies (reqwest 0.12 → 0.13 alongside liteparse; evaluate dropping `aws-lc-sys`). | Build, tests, update check, and remote-image fetch unchanged; dependency count and build time measured. | — | The resvg 0.45/0.47 split needs iced and mermaid git revs; known cost. |
| MDV-036 | P2 | ready | Launching with a bare directory argument (`rmdv <dir>`) loads the folder but also shows a "Couldn't open file — Is a directory" card. | Bare-directory launch opens the workspace with no error card; file and `open-folder` launches unchanged; regression test. | — | — |
| MDV-002 | P2 | ready | Bound syntax-highlight cache memory by bytes (search results were bounded by PR #30 and #33). | `HlCache` source/total-byte budget with focused tests and measured evidence. | [`MDV-002`](plans/active/MDV-002-search-highlight-memory-bounds.md) | — |
| MDV-031 | P2 | ready | Visual refinement using the zeron method: `ink`/`hairline`/`wash` overlays, a 6/10/16 radius ladder, light-theme role reassignment with contrast tests, code-block recipe, gentle sidebar/overlay fades. | Owner native acceptance per theme; contrast unit tests (fg/bg ≥ 4.5). | — | Visual change; owner time. |
| MDV-032 | P1 | proposed | Move main-document parse and highlighting off the UI thread. | Design approved by the owner before code. | — | Core contract (request-identity guards). |
| MDV-033 | P1 | proposed | Stabilize `BlockId` so inserting a block does not invalidate every later height and widget state. | Design approved by the owner before code. | — | Core contract (`BlockId`). |
| MDV-034 | P2 | proposed | Pre-render diagrams near the viewport only, and fix the diagram LRU that only `peek`s. | Design approved by the owner before code; >64-diagram document renders every diagram. | — | — |
| MDV-035 | P3 | proposed | Generalize the repeated begin/invalidate/is_current request guards into one `RequestSlot<T>`. | Design approved by the owner before code. | — | Core contract (request-identity guards). |
| MDV-009 | P2 | ready | Retarget and review Mindmap Zoom Controls on merged Full Mindmap. | Clean candidate; focused/unit/integration checks; anchor-preserving native wheel, pinch, and keyboard acceptance. | [`MDV-009`](plans/active/MDV-009-mindmap-zoom-controls-integration.md) | No direct rebase of the old branch without classifying its commits. |
| MDV-003 | P2 | ready | Explain image-only PDFs instead of rendering an unexplained blank document. | Empty-text extraction reaches a clear OCR-disabled state; text PDFs remain unchanged; tests pass. | [`MDV-003`](plans/active/MDV-003-image-only-pdf-feedback.md) | OCR implementation is explicitly excluded. |
| MDV-004 | P2 | ready | Make merged Full Mindmap discoverable in public and in-app guidance. | README features/shortcuts and in-app shortcut overlay match real keys and behavior; documentation/static checks pass. | [`MDV-004`](plans/active/MDV-004-full-mindmap-discoverability.md) | — |
| MDV-005 | P2 | proposed | Determine whether native screenshot capture can include Zen `text_editor` content reliably. | Reproducible A/B captures classify the limitation and either land a tested fix or record a bounded platform limitation. | [`MDV-005`](plans/active/MDV-005-zen-screenshot-coverage.md) | Native macOS UI harness and active-app identity required. |
| MDV-006 | P2 | ready | Reconcile known stale statements in Zoom, KB hints, and benchmark docs. | Each named stale claim is checked against its owning code/branch and corrected without changing product behavior. | [`MDV-006`](plans/active/MDV-006-stale-documentation-reconciliation.md) | Zoom branch content must be edited in its owning checkout or a deliberate successor. |

## Deferred

| ID | Priority | Reason | Revisit trigger |
| --- | --- | --- | --- |
| MDV-019 | P1 | Theme Settings and Theme Studio will be rebuilt on the new design tokens rather than ported from its dirty pre-split candidate (owner decision 2026-10-04; the old diff is backed up outside the repo). | MDV-031 tokens have landed. |
| MDV-021 | P1 | The v0.7.0 app payloads are signed, notarized, and Gatekeeper-accepted, but the release workflow does not independently sign the DMG containers, treats DMG stapling as best-effort, or fail closed on non-empty Windows app/setup files and their hashes before upload. | Before the next release candidate, add and exercise explicit artifact-integrity gates without changing Windows best-effort release policy. |
| MDV-007 | P2 | One directory with more than 10,000 immediate entries is still truncated inside that directory. Budget exhaustion across siblings, the case the owner actually hit in `~/Documents`, was fixed in PR #43: a folder the budget skipped is scanned on its own when expanded. | A real directory with >10k immediate entries that hides needed files, or a bounded paging proposal with measurements. |

## Recently completed

| ID | Outcome | Evidence route |
| --- | --- | --- |
| MDV-024–028 | Phase-2 performance round (sidebar rows, editor keystroke, local images, window events, mindmap animation) plus two acceptance fixes (⌘Z undo/scroll, lazy scan of budget-skipped folders); owner-accepted on device. | PRs #37–#43 and [`docs/plans/completed/MDV-024-028-phase-2-performance.md`](plans/completed/MDV-024-028-phase-2-performance.md). |
| MDV-029 | CI runs Linux and macOS tests plus a Windows check on PRs, gates release builds on tests, reports `cargo audit`, and caches PDFium. | PR #36 and [`docs/plans/completed/MDV-029-ci-coverage-and-release-gates.md`](plans/completed/MDV-029-ci-coverage-and-release-gates.md). |
| MDV-023 | Removed dead `first_frame_at` and the six Full Mindmap `expect("checked above")` panics. | PR #35 and [`docs/plans/completed/MDV-023-phase-1-leftovers.md`](plans/completed/MDV-023-phase-1-leftovers.md). |
| MDV-022 | Refactor phases 0–2 first wave: safety fixes, `src/app/` split with design tokens, vault-search, file-finder, and in-document search performance, and theme persistence; owner-accepted on device. | PRs #28–#33 and [`docs/plans/completed/MDV-022-refactor-phases-0-2.md`](plans/completed/MDV-022-refactor-phases-0-2.md). |
| MDV-017 | Workspace Quick Slots merged after a clean port onto `main`. | [PR #26](https://github.com/minchenlee/rmdv/pull/26), squash `1b3b94b`. |
| MDV-010 | Closed by archival: the owner aligned local `main` to `origin/main@1244e4c`; old local-only commits stay on `archive/main-before-sync-20261004`. | [`docs/plans/completed/MDV-010-local-main-reconciliation.md`](plans/completed/MDV-010-local-main-reconciliation.md). |
| MDV-020 | Published and live-verified rmdv v0.7.0 with native smoke, exact-head review/CI, nine verified release assets, signed/notarized macOS apps, and an authenticated production site deployment. | [`docs/plans/completed/MDV-020-release-v0.7.0.md`](plans/completed/MDV-020-release-v0.7.0.md) and [`docs/releases/v0.7.0-content-pack.md`](releases/v0.7.0-content-pack.md). |
| MDV-001 | Proved the Windows MSVC no-default-features build and NSIS package on exact v0.7.0 source; the downloadable app and setup executables are non-empty and match published SHA-256 values. | [`docs/plans/completed/MDV-001-windows-build-verification.md`](plans/completed/MDV-001-windows-build-verification.md). |
| MDV-018 | Added reviewed Document Mindmap depth folding for Markdown, JSON, YAML, and TOML; PR #23 merged and exact-head Linux CI passed. | [`docs/plans/completed/MDV-018-document-mindmap-depth-folding.md`](plans/completed/MDV-018-document-mindmap-depth-folding.md). |
| MDV-016 | Reconciled the static website shortcut reference with current Rust bindings, clarified the two-step fold chord, and added a drift-failing contract check. | [`docs/plans/completed/MDV-016-site-shortcut-contract.md`](plans/completed/MDV-016-site-shortcut-contract.md). |
| MDV-015 | Moved the animated ASCII wordmark from the hero into a dark terminal-style `[EOF]` footer that closes the page without competing with the product introduction. | [`docs/plans/completed/MDV-015-ascii-terminal-footer.md`](plans/completed/MDV-015-ascii-terminal-footer.md). |
| MDV-013 | Restored the animated ASCII `rmdv` wordmark as a responsive README pane inside the preferred Impeccable landing-page variant. | [`docs/plans/completed/MDV-013-ascii-wordmark-restoration.md`](plans/completed/MDV-013-ascii-wordmark-restoration.md). |
| MDV-012 | Preserved the first landing redesign, restored the original, and produced a separately reviewable Impeccable product-workspace variant with static and responsive browser acceptance. | [`docs/plans/completed/MDV-012-impeccable-site-variant.md`](plans/completed/MDV-012-impeccable-site-variant.md) and named Git stash identities. |
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
