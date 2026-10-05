# rmdv — project status

Last verified: 2026-08-14 22:27 CST (Asia/Taipei)
Stale after: 7 days
Canonical repository: `/Users/liminchen/Documents/GitHub/mdv`
Expected branch / HEAD / PR: start new work from the live `origin/main`;
`v0.7.0` points to release source `9dd7217b5c6c1328630451c1452a21eb1830041d`.
No next product PR is selected.
Authority: This is a routing snapshot. Verify Git, GitHub, runtime identity, and
manual evidence before mutation.

## Current outcome

rmdv v0.7.0 is published and the matching public site is live. The release
outcome is complete; select a new backlog outcome explicitly before changing
product code, and preserve the two deferred dirty feature worktrees.

## Published v0.7.0 baseline

- PR [#24](https://github.com/minchenlee/rmdv/pull/24) was squash-merged on
  2026-08-14 as `9dd7217`; annotated tag `v0.7.0` points to that commit.
- The non-draft, non-prerelease [GitHub Release](https://github.com/minchenlee/rmdv/releases/tag/v0.7.0)
  was published 2026-08-14. Release workflow
  [31807308166](https://github.com/minchenlee/rmdv/actions/runs/31807308166)
  passed Linux, Windows, macOS arm64, macOS x86_64, and publication jobs.
- Included user-facing scope: reliable Finder opening, responsive Markdown
  tables, Mindmap panel corrections, file/folder refresh, Finder reveal and
  path copy, and Document Mindmap depth folding for Markdown, JSON, YAML, and
  TOML.
- Quick Slots (`MDV-017`) and Theme Settings/Studio (`MDV-019`) were explicitly
  excluded and remain isolated candidates.
- Published checksums, updater manifest, signed/notarized macOS app payloads,
  Linux AppImage, and both Windows executables were downloaded and verified.
- Authenticated local Wrangler deployed Worker version
  `74e58243-55e3-4955-96a0-a4587b756e72`; the live homepage, `llms.txt`, and
  sitemap returned HTTP 200 and matched the release files byte for byte.
- Detailed evidence is in
  [`docs/releases/v0.7.0-release-notes.md`](docs/releases/v0.7.0-release-notes.md),
  [`docs/releases/v0.7.0-content-pack.md`](docs/releases/v0.7.0-content-pack.md),
  and the completed [`MDV-020`](docs/plans/completed/MDV-020-release-v0.7.0.md)
  plan.

## Live workstreams

| ID | State | Owner | Outcome | Acceptance | Plan |
| --- | --- | --- | --- | --- | --- |
| MDV-002 | ready | unassigned | Bound search-result and highlight-cache memory without changing visible search behavior. | Explicit budgets, truthful truncation, focused regressions, and measured memory evidence. | [`docs/plans/active/MDV-002-search-highlight-memory-bounds.md`](docs/plans/active/MDV-002-search-highlight-memory-bounds.md) |
| MDV-004 | ready | unassigned | Make merged Full Mindmap discoverable in public and in-app guidance. | README features/shortcuts and in-app shortcut overlay match real behavior; documentation/static checks pass. | [`docs/plans/active/MDV-004-full-mindmap-discoverability.md`](docs/plans/active/MDV-004-full-mindmap-discoverability.md) |
| MDV-009 | ready | unassigned | Retarget and review Mindmap Zoom Controls on merged Full Mindmap. | Clean candidate, focused checks, and anchor-preserving native acceptance. | [`docs/plans/active/MDV-009-mindmap-zoom-controls-integration.md`](docs/plans/active/MDV-009-mindmap-zoom-controls-integration.md) |

The complete portfolio is in [`docs/BACKLOG.md`](docs/BACKLOG.md).

## Human decisions / blockers

- Do not import Quick Slots (`MDV-017`) or Theme Settings/Studio (`MDV-019`)
  implicitly. Their worktrees are dirty and require deliberate current-main
  ports plus their own native acceptance.
- The protected local `main@a9a0291` is six commits ahead of and three behind
  `origin/main@9dd7217`; its unrelated commits remain untouched (`MDV-010`).
- Windows remains best-effort in the release workflow. v0.7.0 proved the
  build/package outcome tracked by `MDV-001`, but fail-closed artifact guards
  and independent DMG-container signing remain follow-up hardening (`MDV-021`).
- No release, deployment, feature integration, or local-main reconciliation is
  authorized merely by selecting a backlog row.

## Next safe actions

1. Have the owner select one ready backlog outcome before opening a product
   branch.
2. If resuming `MDV-017` or `MDV-019`, preserve the dirty source worktree and
   port only reviewed changes onto a new branch from current `origin/main`.
3. Complete `MDV-021` before the next release candidate is tagged.

## Verification state

### Verified now

- `origin/main` and tag `v0.7.0` resolve to `9dd7217`; PR #24 is merged, its
  exact-head Linux CI passed, and main CI run
  [31807215705](https://github.com/minchenlee/rmdv/actions/runs/31807215705)
  passed on the merge commit.
- Native smoke on the exact packaged candidate exercised `⌘R`, path copy,
  Finder reveal, Document Mindmap, `⌘K` then `1`, and `⌘K` then `0`; the
  expected toasts, selected path, collapsed root, and restored branches were
  observed.
- All nine release assets downloaded. `shasum -a 256 -c SHA256SUMS` passed for
  all seven payloads, and `latest.json` version, URLs, and three updater hashes
  match the published assets.
- Both macOS app payloads pass deep strict codesign verification and Gatekeeper
  assessment as `Notarized Developer ID`; both app and DMG stapling tickets
  validate. The DMG images mount and pass `hdiutil verify`.
- The Windows release job built and packaged exact commit `9dd7217`; downloaded
  `rmdv.exe` and `rmdv_0.7.0_x64-setup.exe` are non-empty and match published
  SHA-256 values, completing `MDV-001`'s observable build/package outcome.
- Local Wrangler 4.123.0 under Node 22.21.1 deployed the site. Live homepage
  SHA-256 `86907fd846d515415483cb269da884934e02be0bf55c3f3a39a703a7db287084`
  and `llms.txt` SHA-256
  `1c8ba18c724261af4a7c1b18a76659f681a00bbf87a6e16d8c0c4353a0d0e97a`
  matched their local release files; sitemap SHA-256
  `b9e132b3642fe460dfbf9e77924150498c129d05cf28bc19445f3b847735decd`
  matched as well.
- Quick Slots and Theme Settings/Studio remain dirty in their isolated
  worktrees; the canonical checkout and release worktree were not used to
  absorb those changes.

### Recorded only

- On the release candidate, default/no-default Cargo checks, 361 library tests,
  integration targets, the optimized arm64 build, package structure checks,
  site shortcut contract, `git diff --check`, and strict project-system audit
  passed before PR #24 merged. See the completed `MDV-020` plan for commands and
  artifacts.
- Feature-level review and acceptance for the work included since v0.6.0 is
  retained in its merged PRs and completed plans; it was not repeated during
  this status-only reconciliation.

### Not verified / follow-up

- The v0.7.0 DMG outer containers carry valid stapled notarization tickets but
  are not independently Developer ID signed; Gatekeeper accepts the contained
  apps. `MDV-021` owns making the future workflow fail closed on this and on
  Windows artifact assertions.
- Quick Slots and Theme Settings/Studio have not received current-main review,
  exact-build native acceptance, or merge authority.
- Native Windows launch behavior was outside `MDV-001`; its hosted build and
  NSIS package path are verified.

## Routes

- Product contract: [`PRODUCT.md`](PRODUCT.md)
- User-facing overview: [`README.md`](README.md)
- Backlog: [`docs/BACKLOG.md`](docs/BACKLOG.md)
- Active plans: [`docs/plans/active/`](docs/plans/active/)
- Completed plans: [`docs/plans/completed/`](docs/plans/completed/)
- Release records: [`docs/releases/`](docs/releases/)
- Status history: [`docs/status-history/`](docs/status-history/)
- Full Mindmap design: [`docs/superpowers/specs/2026-07-10-full-mindmap-mode-design.md`](docs/superpowers/specs/2026-07-10-full-mindmap-mode-design.md)
- CLI and IPC design: [`docs/superpowers/specs/2026-05-17-cli-agent-control-design.md`](docs/superpowers/specs/2026-05-17-cli-agent-control-design.md)

## Update contract

- Start: read the effective `AGENTS.md` chain and this file, then verify Git,
  GitHub, runtime identity, and the selected task's active plan.
- During work: keep one accountable lead and one active writer per mutable
  artifact; preserve unrelated dirty work and authority boundaries.
- End: update only facts and evidence changed by the session, advance task state
  only to the level proven, and keep no more than three next safe actions here.
- Move completed plans to `docs/plans/completed/` and chronological narrative to
  `docs/status-history/`; do not grow this file back into a work diary.
