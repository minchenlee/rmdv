# rmdv — project status

Last verified: 2026-08-10 13:07 CST (Asia/Taipei)
Stale after: 7 days
Canonical repository: `/Users/liminchen/Documents/GitHub/mdv`
Expected branch / HEAD / PR: `codex/release-v0.7.0`, based on
`origin/main@34ef5841b58880a825e4238080691cfbfc6f57f3`; no PR exists yet.
Last verified main base: `origin/main@34ef5841b58880a825e4238080691cfbfc6f57f3`.
Authority: This is a routing snapshot. Verify Git, GitHub, runtime identity, and manual evidence before mutation.

## Current outcome

Prepare a reviewable v0.7.0 candidate from the current proven remote `main`.
The release includes only already-merged work; no dirty feature candidate is
being imported into the release branch.

## v0.7.0 release preparation

- Preparation branch: `codex/release-v0.7.0` from `origin/main@34ef584`.
- Included user-facing scope: reliable macOS Finder opening, responsive
  Markdown tables, Mindmap panel sizing/shortcut corrections, `⌘R` file/folder
  refresh, Finder reveal and path-copy actions, and Document Mindmap depth
  folding for Markdown, JSON, YAML, and TOML.
- Included site/tooling scope: static AI disclosure sticker and documented Node
  runtime for site deployment.
- Quick Slots is reserved as `MDV-017`; Theme Settings/Studio is `MDV-019`.
  Both are explicitly excluded from v0.7.0 and remain isolated candidates.
- Version/release/site metadata is prepared locally. Tagging, publishing,
  signed artifact verification, and site deployment have not occurred.

## Published baseline

- v0.6.0 remains the latest published GitHub Release and live-site version.
- Its signed/notarized macOS artifacts, Linux AppImage, best-effort Windows
  artifacts, checksums, updater manifest, and authenticated site deployment
  were verified in the v0.6.0 release record under `docs/releases/`.

## Live workstreams

| ID | State | Owner | Outcome | Acceptance | Plan |
| --- | --- | --- | --- | --- | --- |
| MDV-020 | submitted | Codex | Publish and live-verify v0.7.0 from current `origin/main`. | Local/native gates, exact-head review/CI, published artifacts, checksums/manifest, and the deployed site are verified. | [`docs/plans/active/MDV-020-release-v0.7.0.md`](docs/plans/active/MDV-020-release-v0.7.0.md) |
| MDV-001 | ready | unassigned | Prove the Windows IPC lifetime fix on an actual Windows CI runner. | Windows build/package succeeds and the run proves non-empty app/setup executables with hashes plus a downloadable artifact. | [`docs/plans/active/MDV-001-windows-build-verification.md`](docs/plans/active/MDV-001-windows-build-verification.md) |
| MDV-002 | ready | unassigned | Bound search result and highlight-cache memory without changing visible search behavior. | Explicit budgets, truncation behavior, focused regressions, and measured memory evidence. | [`docs/plans/active/MDV-002-search-highlight-memory-bounds.md`](docs/plans/active/MDV-002-search-highlight-memory-bounds.md) |
| MDV-004 | ready | unassigned | Make merged Full Mindmap discoverable in public and in-app guidance. | README features/shortcuts and in-app shortcut overlay match real keys and behavior; documentation/static checks pass. | [`docs/plans/active/MDV-004-full-mindmap-discoverability.md`](docs/plans/active/MDV-004-full-mindmap-discoverability.md) |

The complete portfolio, including P2 and deferred work, is in
[`docs/BACKLOG.md`](docs/BACKLOG.md).

## Human decisions / blockers

- Quick Slots (`MDV-017`) and Theme Settings/Studio (`MDV-019`) must not enter
  v0.7.0.
- The owner authorized the complete v0.7.0 commit/PR/review/merge/tag/release/
  artifact-verification/site-deployment flow on 2026-08-10. Do not cross a
  stage gate before its exact GUI, review, CI, artifact, or live-runtime
  evidence passes.
- Do not deploy the staged v0.7.0 public-site copy until the non-draft GitHub
  Release exists and its actual signing/checksum/manifest state is verified.
- The GitHub Actions site deploy workflow still requires its own
  `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID`; the live site was
  deployed locally instead.
- The Windows release job remains best-effort (`continue-on-error: true`).

## Next safe actions

1. Run native GUI interaction smoke on the exact packaged v0.7.0 candidate and
   obtain owner acceptance.
2. Commit/push/open the release PR, then merge only after GUI smoke plus
   exact-head review and CI pass; proceed through the already-authorized tag,
   artifact-verification, and site-deployment gates in order.
3. Preserve at least several GiB of free disk before native packaging; the
   release build completed with 3.7 GiB available after scoped Cargo cleanup.

## Verification state

### Verified now

- `git ls-remote origin refs/heads/main` returned
  `34ef5841b58880a825e4238080691cfbfc6f57f3`; the release worktree was created
  directly from that commit and did not import another worktree's changes.
- GitHub has no open PR. Linux CI run `31323797521` completed successfully on
  exact `main@34ef584`.
- PR #15, #16, #17, #18, #20, #21, #22, and #23 are merged; v0.6.0 remains a
  non-draft, non-prerelease GitHub Release published 2026-07-21.
- Canonical feature IDs are now distinct: Quick Slots `MDV-017`, merged
  Document Mindmap depth folding `MDV-018`, and Theme Settings/Studio
  `MDV-019`. The Theme candidate's status, backlog, plan filename, and heading
  were updated together.
- The protected local `main@a9a0291` remains untouched at six commits ahead of
  and two behind `origin/main@34ef584`.
- Default and no-default Cargo checks passed; 361 library tests and all
  integration targets passed. Site JavaScript syntax and shortcut-contract
  checks also passed.
- A clean optimized release build produced a 34 MiB arm64 Mach-O reporting
  `rmdv 0.7.0`, SHA-256
  `cc1109cb8a8850e23857d90a48fad6de53c9c05f361c9f26954021b3f105c474`.
- A target-specific 41 MiB `.app` at
  `/private/tmp/rmdv-v070-package.S2rVPg/rmdv.app` reports version `0.7.0`,
  carries four document-association groups and an arm64 PDFium dylib, and passes
  ad-hoc deep codesign verification. This does not prove Developer ID signing,
  notarization, or GUI behavior.
- `git diff --check` and the strict four-layer project-system audit passed.

### Recorded only

- `origin/main` contains the squash merge of PR #15 at `741f36e`. Its
  corrected `application:openFile:` Objective-C type encoding was verified at
  the merged PR head before merge.
- `origin/main` contains the final merges of PR #16, PR #17, and PR #20.
  The table and Mindmap fixes passed their final Codex reviews; the static
  WebP sticker passed review after its 290×192 transparent footprint was
  restored.
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
- Live `https://rmdv.mclee.dev/` and `/llms.txt` returned HTTP 200 after the
  local deploy and report v0.6.0; the homepage JSON-LD reports
  `softwareVersion` `0.6.0` and `dateModified` `2026-07-21`.

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

#### Earlier release evidence

- Release commit `b67161c` remains published as tag `v0.5.0`; workflow
  `29645918474` recorded successful platform artifacts, checksums, and
  `latest.json`.
- Historical Rust, native UI, Full Mindmap, Zoom, Zen, CJK, packaging, and
  screenshot evidence remains in `docs/status-history/`. It was not rerun for
  this static-site-only outcome.

### Not verified

- The v0.7.0 packaged app has not completed native GUI interaction smoke.
- No v0.7.0 tag, GitHub Release, signed/notarized artifact, checksums/manifest,
  or release-workflow run exists yet.
- The v0.7.0 site metadata has not been deployed or verified on the public site.

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
