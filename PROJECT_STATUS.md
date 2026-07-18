# rmdv — project status

Last verified: 2026-07-18 21:45 CST (Asia/Taipei)
Stale after: 7 days
Canonical repository: `/Users/liminchen/Documents/GitHub/mdv`
Expected branch: `main`; always resolve its live HEAD before mutation.
Last verified main base: `origin/main@33b7d8f806fc5caee617469664a2065f2e3bb9ee`.
Authority: This is a routing snapshot. Verify Git, GitHub, runtime identity, and manual evidence before mutation.

## Current outcome

Release rmdv v0.5.0 from the accepted Full Mindmap and Zoom Controls `main`
line, with public documentation, site metadata, and release assets aligned.

## v0.5.0 release

- Release materials cover the Full Mindmap workspace, native mindmap zoom,
  Zen edit mode, public site metadata, `site/llms.txt`, and the native capture
  at `site/assets/shot-full-mindmap.webp`.
- An isolated RC from `origin/main@33b7d8f` passed 314 library tests, all
  integration targets, default/PDF/no-default checks, the release build, and
  native Full Mindmap smoke with a 50% preview panel.
- Release commit `b67161c` is published as tag `v0.5.0`; GitHub release
  workflow `29645918474` built all platform artifacts, checksums, and
  `latest.json` successfully.
- The first site deployment attempt (`29646511085`) failed because the
  workflow installed Wrangler 3.90.0, which could not consume the repository's
  JSONC static-assets config. The deployment fix pins Wrangler 4.112.0.
- The retry (`29646699009`) confirmed Wrangler 4.112.0 reads the config, then
  stopped because `CLOUDFLARE_API_TOKEN` is not configured in GitHub Actions;
  `CLOUDFLARE_ACCOUNT_ID` is also absent. The site deployment remains pending
  until the owner adds those secrets.

## Live workstreams

| ID | State | Owner | Outcome | Acceptance | Plan |
| --- | --- | --- | --- | --- | --- |
| MDV-010 | ready | unassigned | Reconcile the divergent local `main` without replaying behavior already present in remote `main`. | Patch-equivalence review, clean isolated candidate, focused and cross-boundary tests; no publish without owner approval. | [`docs/plans/active/MDV-010-local-main-reconciliation.md`](docs/plans/active/MDV-010-local-main-reconciliation.md) |
| MDV-001 | ready | unassigned | Prove the Windows IPC lifetime fix on an actual Windows CI runner. | Windows build/package succeeds and the run proves non-empty app/setup executables with hashes plus a downloadable artifact. | [`docs/plans/active/MDV-001-windows-build-verification.md`](docs/plans/active/MDV-001-windows-build-verification.md) |
| MDV-002 | ready | unassigned | Bound search result and highlight-cache memory without changing visible search behavior. | Explicit budgets, truncation behavior, focused regressions, and measured memory evidence. | [`docs/plans/active/MDV-002-search-highlight-memory-bounds.md`](docs/plans/active/MDV-002-search-highlight-memory-bounds.md) |
| MDV-009 | ready | unassigned | Retarget and review Zoom Controls against the merged Full Mindmap implementation. | Clean isolated candidate, focused interaction tests, full relevant suites, and native zoom acceptance. | [`docs/plans/active/MDV-009-mindmap-zoom-controls-integration.md`](docs/plans/active/MDV-009-mindmap-zoom-controls-integration.md) |

The complete portfolio, including P2 and deferred work, is in
[`docs/BACKLOG.md`](docs/BACKLOG.md).

## Human decisions / blockers

- Do not push, open or merge a PR, tag, release, publish artifacts, or deploy
  without an explicit owner request.
- Local `main` and `origin/main` have diverged. Preserve the local commits, but
  determine patch equivalence before choosing cherry-pick, reimplementation, or
  archival; never force-update either line as part of ordinary reconciliation.
- The Windows release job remains best-effort (`continue-on-error: true`).
  Changing that release policy is a product/release decision, not an automatic
  consequence of making the job pass.

## Next safe actions

1. Execute MDV-010 in a fresh isolated branch from current `origin/main`,
   classifying each local-main-only patch before applying anything.
2. Prepare MDV-001 as a bounded Windows CI candidate; request publish authority
   only when the exact diff and acceptance command are ready.
3. Define MDV-002's explicit byte budgets and observable truncation contract
   before implementing search/highlight memory bounds.

## Verification state

### Verified now

- The control-plane candidate contained exactly 20 documentation/instruction
  paths and no product source or workflow changes. Independent re-review closed
  all actionable findings with `VERDICT: PASS`.
- Strict four-layer and root/`src`/`src/ipc`/`tests`/`site`/`demo` AGENTS audits,
  link checks, whitespace checks, and `git diff --check` passed on the candidate.
- GitHub PR #8 is merged into `main` as
  `a8f8348619829199b53ad761d07293b1f419bba3`; its final head was
  `19715ae1ce840fcedfa72705011e4abc0f40b892`.
- GitHub reported remote `main@f75918c` and zero open pull requests immediately
  before publication at 2026-07-18 17:32 CST. Resolve the live post-merge HEAD.
- After fetching `origin/main`, local `main` is 5 commits behind and 8 commits
  ahead; `feat/mindmap-zoom-controls` is 6 behind and 10 ahead. These counts
  describe Git ancestry, not missing behavior.
- GitHub's latest published release is `v0.4.0` (published 2026-06-23); no
  release action was taken in this refactor.

### Recorded only

- Pre-publication snapshot: the isolated
  `/Users/liminchen/Documents/GitHub/mdv/.codex/worktrees/agent-project-system`
  checkout started clean at `f75918c`, then carried the reviewed 20-path
  control-plane diff on `codex/agent-project-system`.
- The archived pre-control-plane status records PR #8's final automated gates:
  298/298 library tests, all integration targets, default/no-default/PDF checks,
  touched-file rustfmt, `git diff --check`, and an optimized release build.
- The same history records native acceptance of the root-Left c9watch
  reproduction after verifying the active executable with `lsof`.
- Local Zen, CJK, Windows IPC, screenshot, site, and Zoom branch evidence is
  retained in the archived status. It was not rerun for this docs-only refactor.

### Not verified

- No Rust, site, native GUI, packaging, signing, release, or deployment check
  was rerun for this documentation-only refactor.
- Windows compilation/package behavior has not been proven on a Windows runner.
- Zoom Controls has not been retargeted to or manually accepted on current
  `origin/main`.
- The local-main-only commits have not yet been classified as already present,
  still needed, or obsolete relative to the PR #8 squash result.

## Routes

- Product contract: [`PRODUCT.md`](PRODUCT.md)
- User-facing overview: [`README.md`](README.md)
- Backlog: [`docs/BACKLOG.md`](docs/BACKLOG.md)
- Active plans: [`docs/plans/active/`](docs/plans/active/)
- Completed plans: [`docs/plans/completed/`](docs/plans/completed/)
- Status history: [`docs/status-history/`](docs/status-history/)
- Full Mindmap design: [`docs/superpowers/specs/2026-07-10-full-mindmap-mode-design.md`](docs/superpowers/specs/2026-07-10-full-mindmap-mode-design.md)
- CLI and IPC design: [`docs/superpowers/specs/2026-05-17-cli-agent-control-design.md`](docs/superpowers/specs/2026-05-17-cli-agent-control-design.md)

## Update contract

- Start: read the effective `AGENTS.md` chain and this file, then verify Git,
  GitHub, runtime identity, and the chosen task's active plan.
- During work: keep one accountable lead and one active writer per mutable
  artifact; preserve unrelated dirty work and authority boundaries.
- End: update only facts and evidence changed by the session, advance task state
  only to the level proven, and keep no more than three next safe actions here.
- Move completed plans to `docs/plans/completed/` and chronological narrative to
  `docs/status-history/`; do not grow this file back into a work diary.
