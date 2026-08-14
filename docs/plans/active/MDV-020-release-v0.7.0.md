# MDV-020 — Release rmdv v0.7.0

State: submitted
Owner / accountable lead: Codex
Active writer: none
Created: 2026-08-10
Updated: 2026-08-10

## Outcome

Prepare, publish, and live-verify v0.7.0 from the proven
`origin/main@34ef584`, with synchronized package/site/release metadata,
reviewed release evidence, verified platform artifacts, and an authenticated
site deployment.

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
- [ ] Complete exact packaged-app native GUI smoke.
- [ ] Pass exact-head review and CI, then merge the release PR.
- [ ] Publish and verify the v0.7.0 tag, release, and platform artifacts.
- [ ] Only after the non-draft GitHub Release and artifact state are verified,
      deploy and live-verify the v0.7.0 site.

## Decision log

| Date | Decision | Evidence / reason |
| --- | --- | --- |
| 2026-08-10 | Target v0.7.0 from current merged `main`. | The line contains material user-facing refresh and Document Mindmap capabilities after v0.6.0. |
| 2026-08-10 | Exclude MDV-017 and MDV-019. | Owner explicitly deferred both candidates from this release. |
| 2026-08-10 | Keep MDV-017 for Quick Slots and assign MDV-019 to Theme Settings/Studio. | MDV-018 is already the merged Document Mindmap feature; MDV-019 was the next unused stable ID. |
| 2026-08-10 | Run the complete gated release flow. | Owner explicitly asked to finish commit/PR/review/merge/tag/release/artifact/site-deploy verification. |

## Blockers and escalation

- The owner authority gate is satisfied. Native GUI smoke currently requires an
  unlocked Mac; merge/tag remain blocked until that smoke and exact-head
  review/CI pass.

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
- Remaining gates: GUI native smoke, exact-head review/CI, merge/tag/release
  artifact verification, and site deployment.
