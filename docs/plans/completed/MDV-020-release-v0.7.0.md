# MDV-020 — Release rmdv v0.7.0

State: done
Owner / accountable lead: Codex
Active writer: none
Created: 2026-08-10
Updated: 2026-08-14

## Outcome

Prepare, publish, and live-verify v0.7.0 from the proven pre-release baseline
`origin/main@34ef584`, producing release source/tag `9dd7217` with synchronized
package/site/release metadata, reviewed release evidence, verified platform
artifacts, and an authenticated site deployment.

## Non-goals

- Do not include Quick Slots (`MDV-017`) or Theme Settings and Theme Studio
  (`MDV-019`).
- Do not alter protected signing/deployment credentials or weaken release
  workflow fallbacks and verification gates.
- Do not reconcile or rewrite the divergent protected local `main` branch.

## Constraints and authority

- Prepare on `codex/release-v0.7.0`, created directly from the live remote
  `main@34ef584` after verifying the remote ref.
- The owner authorized commit, push, PR, merge, tag, publication, artifact
  verification, and authenticated site deployment on 2026-08-10. Each stage
  remains gated on its exact review, CI, package, or live-runtime evidence.
- Distinguish local static/test/build evidence from native packaging, signed
  artifacts, GitHub Actions release evidence, and live deployment evidence.

## Owned and excluded surfaces

- Owned: Cargo version metadata, release notes/content pack, site version and
  release metadata, project status/backlog, this release plan, the v0.7.0 tag
  and GitHub Release, published-artifact verification, and site deployment.
- Excluded: product implementation outside the selected merged scope, dirty
  Quick Slots and Theme Studio worktrees, release workflow behavior, and
  signing/deployment secrets.

## Acceptance evidence

- `cargo check`
- `cargo test --lib`
- `cargo test --tests`
- `cargo build --release --bin rmdv`
- `cargo check --no-default-features`
- Site static checks and JavaScript syntax checks.
- `git diff --check`
- Built binary reports `rmdv 0.7.0`.
- Target-specific `.app` packaging embeds the arm64 PDFium runtime, reports the
  expected bundle version and associations, and passes structural codesign
  verification. GUI interaction remains a separate manual gate.
- Exact-head GitHub review and required CI pass before merge.
- Published assets match `SHA256SUMS` and `latest.json`; macOS payloads prove
  their actual signing/notarization state rather than inheriting a workflow
  assumption.
- Authenticated local Wrangler deployment succeeds and the public homepage plus
  `llms.txt` independently report v0.7.0.

## Progress

- [x] Verify remote `main`, open PR state, latest published release, and exact
  `main` CI result.
- [x] Freeze included and excluded v0.7.0 scope.
- [x] Allocate unique feature IDs: Quick Slots `MDV-017`, Document Mindmap
  depth folding `MDV-018`, Theme Settings/Studio `MDV-019`.
- [x] Synchronize version, site metadata, release documents, and control plane.
- [x] Run local validation and freeze the exact candidate diff.
- [x] Build and structurally verify an Apple Silicon `.app` candidate.
- [x] Obtain owner authority for the complete gated publication flow.
- [x] Complete exact packaged-app native GUI smoke.
- [x] Pass exact-head review and CI, then merge the release PR.
- [x] Publish and verify the v0.7.0 tag, release, and platform artifacts.
- [x] Only after the non-draft GitHub Release and artifact state are verified,
      deploy and live-verify the v0.7.0 site.

## Decision log

| Date | Decision | Evidence / reason |
| --- | --- | --- |
| 2026-08-10 | Target v0.7.0 from current merged `main`. | The line contains material user-facing refresh and Document Mindmap capabilities after v0.6.0. |
| 2026-08-10 | Exclude MDV-017 and MDV-019. | Owner explicitly deferred both candidates from this release. |
| 2026-08-10 | Keep MDV-017 for Quick Slots and assign MDV-019 to Theme Settings/Studio. | MDV-018 is already the merged Document Mindmap feature; MDV-019 was the next unused stable ID. |
| 2026-08-10 | Run the complete gated release flow. | Owner explicitly asked to finish commit/PR/review/merge/tag/release/artifact/site-deploy verification. |
| 2026-08-14 | Accept PR #24 and publish v0.7.0. | Exact packaged-app smoke, exact-head review, both PR checks, and merged-main CI passed before the tag was pushed. |
| 2026-08-14 | Accept the published artifact set. | All nine assets downloaded; payload checksums and updater manifest matched; macOS app signatures, notarization, stapling, and Gatekeeper checks passed. |
| 2026-08-14 | Complete the release only after production verification. | Authenticated Wrangler deployment succeeded and live homepage plus `llms.txt` matched the prepared files byte for byte. |

## Blockers and escalation

- None for the completed release. Future fail-closed artifact hardening is
  tracked separately as `MDV-021`; it does not retroactively change the
  verified v0.7.0 app payloads.

## Final evidence

- `cargo check --target-dir /Users/liminchen/Documents/GitHub/mdv/target -j 2`
  — passed.
- `cargo check --no-default-features --target-dir /Users/liminchen/Documents/GitHub/mdv/target -j 2`
  — passed.
- `cargo test --lib --target-dir /Users/liminchen/Documents/GitHub/mdv/target -j 2`
  — 361 passed.
- `cargo test --tests --target-dir /Users/liminchen/Documents/GitHub/mdv/target -j 2`
  — all library and integration targets passed; one pre-existing unused-import
  warning remains in `tests/ipc_protocol.rs`.
- `node --check site/app.js`, `node --check site/check-shortcuts.mjs`, and
  `node site/check-shortcuts.mjs` — passed; the contract now follows the
  current `is_shortcuts_key` helper and v0.7.0 structured feature metadata.
- The first optimized build attempt exhausted disk space during stripping and
  produced a 3.3 KiB invalid artifact, which was explicitly rejected. A scoped
  `cargo clean -p rmdv` removed 5.3 GiB of reproducible package artifacts; the
  forced release-profile rebuild then completed without warnings.
- `cargo build --release --bin rmdv --target-dir /Users/liminchen/Documents/GitHub/mdv/target -j 2`
  — passed. The 34 MiB arm64 Mach-O reports `rmdv 0.7.0`; SHA-256
  `cc1109cb8a8850e23857d90a48fad6de53c9c05f361c9f26954021b3f105c474`.
- `git diff --check` and the strict four-layer project-system audit — passed.
- `cargo build --release --bin rmdv --target aarch64-apple-darwin` and
  `cargo packager --release --target aarch64-apple-darwin -f app` — passed.
  The 41 MiB temporary bundle reports version `0.7.0`, contains four document
  association groups, embeds the 6.1 MiB arm64 `libpdfium.dylib`, and passes
  ad-hoc `codesign --verify --deep --strict`. This is structural package
  evidence, not Developer ID signing, notarization, or GUI acceptance.
- Successful package retained at
  `/private/tmp/rmdv-v070-package.S2rVPg/rmdv.app`; bundled executable SHA-256
  after ad-hoc codesigning is
  `556795aedc179a3618b7f5000918eb54fadb3a9a918889a8eb1f95008c3f1e8f`.
  The final Developer ID-signed release payload will have a different hash and
  must be verified from the published artifact.
- Exact packaged-app native smoke passed: refresh and copy-path toasts appeared,
  Finder reveal selected the exact document, Document Mindmap opened, `⌘K`
  then `1` collapsed to the root level, and `⌘K` then `0` restored branches.
- PR #24 exact head `0a6cd53` received a no-major-issues review; its only
  unresolved thread was outdated against an earlier commit. Both exact-head
  Linux CI runs passed. PR #24 squash-merged as `9dd7217`; merged-main Linux CI
  run `31807215705` also passed.
- Annotated tag `v0.7.0` points to `9dd7217`. Release workflow
  `31807308166` passed macOS arm64, macOS x86_64, Linux x86_64, Windows x86_64,
  and publication jobs; the non-draft release was published 2026-08-14.
- All nine release assets downloaded. `shasum -a 256 -c SHA256SUMS` passed for
  all seven payloads; `SHA256SUMS` and `latest.json` matched their GitHub asset
  digests, and the updater manifest's version, URLs, and three payload hashes
  matched the downloaded files.
- Both macOS app payloads contain the correct architecture and PDFium dylib,
  pass `codesign --verify --deep --strict`, carry Developer ID identity
  `CY58UG73K6`, hardened runtime, and stapled notarization tickets, and are
  accepted by Gatekeeper as `Notarized Developer ID`. Both DMGs pass
  `hdiutil verify`, mount successfully, and have valid stapled notarization
  tickets. Their unsigned outer-container hardening is routed to `MDV-021`.
- The Windows job built with `--no-default-features` and packaged NSIS on exact
  release source. Downloaded `rmdv.exe` and
  `rmdv_0.7.0_x64-setup.exe` are non-empty and match SHA-256
  `e676563821429ba6bf8924f477b1ca6cd2fd20bcbfab9fb9010ea5d42d7dfcaa`
  and `fcc836d81fb38b47ec469aa14933a7ba02ea83842ac6a9f37e586dbbebe042a1`.
- Local Wrangler 4.123.0 under Node 22.21.1 authenticated successfully, its
  dry-run read 36 assets, and production deployment created Worker version
  `74e58243-55e3-4955-96a0-a4587b756e72`. Live homepage, `llms.txt`, and
  sitemap returned HTTP 200; their SHA-256 values matched the local release
  files, including the corrected 2026-08-14 publication metadata.
- Acceptance verdict: done. v0.7.0 is merged, tagged, published, artifact-
  verified, and live; Quick Slots and Theme Settings/Studio remain excluded.
