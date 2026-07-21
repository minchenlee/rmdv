# rmdv — project status

Last verified: 2026-07-21 CST (Asia/Taipei)
Stale after: 7 days
Canonical repository: `/Users/liminchen/Documents/GitHub/mdv`
Expected branch: `main`; always resolve its live HEAD before mutation.
Last verified main base: `origin/main@05770409b7da32574e1ff5bed2f9c5e59c7ebaf6`.
Authority: This is a routing snapshot. Verify Git, GitHub, runtime identity, and manual evidence before mutation.

## Current outcome

v0.6.0 is merged, tagged, and published with platform artifacts and verified
checksums/manifest. The site deployment remains blocked by missing Cloudflare
credentials.

## v0.6.0 release preparation

- Preparation branch: `codex/release-v0.6.0` from `main@4311fbf`.
- Scope: merged PR #11, PR #12, and PR #13 after the published v0.5.0 line.
- Version metadata, release notes, content pack, `site/index.html`, and
  `site/llms.txt` are aligned to v0.6.0 in the preparation branch.
- Owner-reported manual acceptance passed for Finder associations, CLI reset and
  installation, CJK rendering, and native regression smoke.
- Local Apple Silicon release binary and packaged `.app` passed; the packaged
  `Info.plist` contains the four expected file-association groups.
- PR #14 merged to `main` at `0577040`; tag `v0.6.0` and GitHub Release are
  published.
- Release workflow `29830450638` built and uploaded Linux, macOS arm64, macOS
  Intel, and Windows artifacts; the publish job generated `SHA256SUMS` and
  `latest.json`, and both were verified against the published assets.
- macOS artifacts are ad-hoc sealed because `APPLE_CERTIFICATE` was absent;
  they are not Developer ID signed or notarized.
- Site deploy run `29830565893` failed because `CLOUDFLARE_API_TOKEN` was not
  configured; the live site remains on v0.5.0.

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
- `main` and `origin/main` contain the PR #14 merge line; tag `v0.6.0` points
  to merge commit `0577040`.
- The site deploy workflow still requires `CLOUDFLARE_API_TOKEN` and
  `CLOUDFLARE_ACCOUNT_ID`; deployment remains pending until those credentials
  are available.
- The Windows release job remains best-effort (`continue-on-error: true`).

## Next safe actions

1. Add `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID`, rerun the manual site
   deploy, and verify the live homepage plus `llms.txt`.
2. Add Apple signing/notarization secrets before claiming notarized macOS
   artifacts in a future release.

## Verification state

### Verified now

- `main` and `origin/main` contain PR #14; tag `v0.6.0` points to merge commit
  `0577040`, and the release candidate branch was clean before merge.
- Owner-reported manual acceptance passed for the merged release scope: Finder
  associations, CLI reset/installation, CJK rendering, and native regression
  smoke.
- Local `cargo build --release --bin rmdv` passed, and a target-specific
  Apple Silicon build plus `cargo packager` produced an `.app` whose
  `Info.plist` contains Markdown, plain-text, structured-text, and LaTeX
  associations.
- After the v0.6.0 version bump, `cargo check`, both feature checks,
  `cargo test --lib` (321 passed), `cargo test --tests`, `cargo build --release
  --bin rmdv`, and the Apple Silicon package build passed. The binary and app
  bundle report version 0.6.0.
- GitHub release workflow `29830450638` passed all four platform jobs and the
  publish job; the public release contains two macOS DMGs, two macOS updater
  tarballs, one Linux AppImage, two Windows executables, `SHA256SUMS`, and
  `latest.json`.
- Live `https://rmdv.mclee.dev/` and `/llms.txt` were checked after the failed
  deploy and still report v0.5.0.

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

- v0.6.0 macOS artifacts are not Developer ID signed or notarized because the
  release secrets were absent; they are ad-hoc sealed.
- The site has not been deployed or live-verified for v0.6.0 because the
  Cloudflare credentials are missing.

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
