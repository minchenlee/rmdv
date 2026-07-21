# rmdv — project status

Last verified: 2026-07-19 14:59 CST (Asia/Taipei)
Stale after: 7 days
Canonical repository: `/Users/liminchen/Documents/GitHub/mdv`
Expected branch: `main`; always resolve its live HEAD before mutation.
Last verified main base: `origin/main@7a0514dd9a2bb9079449ebf7780ef317b184ac42`.
Authority: This is a routing snapshot. Verify Git, GitHub, runtime identity, and manual evidence before mutation.

## Current outcome

Review the preferred Impeccable landing-site variant with its restored ASCII
terminal footer and native-matched static shortcut reference, then decide whether
it should become a commit. Publication remains a separate decision.

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
- Site deployment retry `29646699009` confirmed Wrangler 4.112.0 reads the
  static-assets config, then stopped because `CLOUDFLARE_API_TOKEN` and
  `CLOUDFLARE_ACCOUNT_ID` are absent from GitHub Actions.

## Live workstreams

| ID | State | Owner | Outcome | Acceptance | Plan |
| --- | --- | --- | --- | --- | --- |
| MDV-001 | ready | unassigned | Prove the Windows IPC lifetime fix on an actual Windows CI runner. | Windows build/package succeeds and the run proves non-empty app/setup executables with hashes plus a downloadable artifact. | [`docs/plans/active/MDV-001-windows-build-verification.md`](docs/plans/active/MDV-001-windows-build-verification.md) |
| MDV-002 | ready | unassigned | Bound search result and highlight-cache memory without changing visible search behavior. | Explicit budgets, truncation behavior, focused regressions, and measured memory evidence. | [`docs/plans/active/MDV-002-search-highlight-memory-bounds.md`](docs/plans/active/MDV-002-search-highlight-memory-bounds.md) |
| MDV-009 | ready | unassigned | Retarget and review Zoom Controls against the merged Full Mindmap implementation. | Clean isolated candidate, focused interaction tests, full relevant suites, and native zoom acceptance. | [`docs/plans/active/MDV-009-mindmap-zoom-controls-integration.md`](docs/plans/active/MDV-009-mindmap-zoom-controls-integration.md) |

The complete portfolio, including P2 and deferred work, is in
[`docs/BACKLOG.md`](docs/BACKLOG.md).

## Human decisions / blockers

- Do not push, open or merge a PR, tag, release, publish artifacts, or deploy
  without an explicit owner request.
- The first redesign is preserved in named stash commit `5c78ae9`; the current
  working tree is the uncommitted, owner-preferred Impeccable variant with the
  ASCII terminal footer and static shortcut reference. Do not describe either
  as committed or live.
- The current candidate is isolated on `codex/site-impeccable-redesign` in
  `.codex/worktrees/site-impeccable-redesign`; the original
  `codex/fix-cjk-rendering` checkout retains its unrelated Rust, demo, and
  MDV-010 work.
- Scoped transfer stash `fe4a0af` remains as a recoverable backup until the
  isolated candidate is intentionally committed.
- Local `main` and `origin/main` both resolved to `7a0514d`; MDV-010 is deferred
  unless historical reconciliation is requested or divergence recurs.
- The Windows release job remains best-effort (`continue-on-error: true`).

## Next safe actions

1. Review the terminal footer inside the current Impeccable variant and
   decide whether this candidate is ready to commit.
2. If committing, stage only the isolated site and control-plane paths shown by
   this worktree's status.
3. If live publication is requested later, add the Cloudflare credentials,
   dispatch the manual workflow, and verify the live domain separately.

## Verification state

### Verified now

- Before MDV-012 mutation, local `main` and `origin/main` both resolved to
  `7a0514dd9a2bb9079449ebf7780ef317b184ac42`.
- The first redesign was backed up with tracked and untracked files, then the
  checkout returned to the clean original base before the second design began.
- The Impeccable variant replaces the centered sales page with a literal product
  workspace, using an outline rail, document path, real product captures,
  row-based capabilities, a matching 404, and structural mobile collapse.
- Browser checks passed at 1440×900 and 390×844 in both themes with no horizontal
  overflow. All lazy captures loaded, and theme, palette, focus restoration,
  lightbox, and keyboard scrolling were exercised.
- JavaScript syntax, JSON-LD, duplicate IDs, local assets, exact CSP hashes,
  contrast roles, the Impeccable detector, and `git diff --check` passed.
- The animated ASCII wordmark now closes the page inside a fixed dark terminal
  footer with an `[EOF]` bar and project metadata. Removing it from the hero puts
  the real product capture immediately after the introduction. The footer fits
  1440×900 and 390×844 layouts in both page themes, animates only in view,
  pauses offscreen, and retains its one-frame reduced-motion path.
- The landing page now presents seven reader shortcut rows using the current
  native names and bindings. Full Mindmap is explicit, the fold chord is shown
  as `⌘K` then `0–6`, and the website shortcut sheet is labeled separately. The
  saved `node site/check-shortcuts.mjs` contract verifies the HTML rows,
  structured feature keys, cross-platform modifier handling, and Rust
  handlers/native labels together. It also fails if the removed preview
  interaction returns.
- The rejected shortcut-preview layer and its `Try UI` controls were removed.
  The seven app-table chords are not captured; website `⌘/`, `j/k/g/G`, `t`,
  Space, and `p` interactions remain. Browser checks at 1440×900 and 390×844 found
  all seven static rows, no preview controls or dialog, no horizontal overflow,
  and no console warnings or errors; the shortcut sheet uses the same `⌘/`
  binding as the native app. The website palette uses plain `p` because browsers
  can reserve `⌘⇧P`; it opened from body and focused-button contexts, then
  accepted `p` as query text once its input owned focus.

### Recorded only

- Release commit `b67161c` remains published as tag `v0.5.0`; workflow
  `29645918474` recorded successful platform artifacts, checksums, and
  `latest.json`.
- Historical Rust, native UI, Full Mindmap, Zoom, Zen, CJK, packaging, and
  screenshot evidence remains in `docs/status-history/`. It was not rerun for
  this static-site-only outcome.

### Not verified

- Neither redesign is committed, pushed, published, or checked on the live
  domain. The public site remains unchanged.
- No Rust, native GUI, packaging, signing, or release check was rerun.
- Windows compilation/package behavior has not been proven on a Windows runner.
- Zoom Controls has not been retargeted to or manually accepted on current
  `origin/main`.

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
