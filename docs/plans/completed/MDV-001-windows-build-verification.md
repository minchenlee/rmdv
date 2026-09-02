# MDV-001 — Verify the Windows build path

State: done
Owner / accountable lead: Codex
Active writer: none
Created: 2026-07-18
Updated: 2026-08-14

## Outcome

Prove that the owned IPC socket-name lifetime fix builds and packages on the
actual `x86_64-pc-windows-msvc` release path before the next release decision.

## Non-goals

- Do not enable PDF support or the in-app updater on Windows.
- Do not make the Windows job release-blocking without a separate owner
  decision.
- Do not publish a release merely to obtain CI evidence.

## Constraints and authority

- The relevant release job currently uses `--no-default-features` and
  `continue-on-error: true`.
- Local macOS static review is recorded evidence, not Windows verification.
- A pushed branch/PR is required for hosted Windows CI and needs explicit
  publish authority.

## Owned and excluded surfaces

- Owned: `src/ipc/client.rs`, `src/ipc/server.rs`, Windows-specific `cfg`
  paths, and the existing Windows job in `.github/workflows/release.yml`.
- Excluded: PDF packaging, updater support, other platform release jobs, and
  release-policy changes.

## Acceptance evidence

- The exact candidate contains the owned-name lifetime behavior associated
  with `6fa6450`, whether through ancestry or patch equivalence.
- Hosted Windows CI passes
  `cargo build --release --no-default-features --target x86_64-pc-windows-msvc`.
- The configured NSIS packaging step succeeds. Prefer an explicit CI assertion
  proving at least one non-empty application `.exe` and one non-empty
  `*-setup.exe` before upload; an end-to-end release download is acceptable
  equivalent evidence only when both files are non-empty and each SHA-256
  matches the workflow-generated checksum record.
- The `windows-x86_64` artifact is downloadable from the exact run and contains
  the asserted files matching the recorded hashes. A non-failing upload step is
  insufficient because the current workflow uses `if-no-files-found: warn`.
- IPC protocol/e2e tests and the current-platform `cargo check` remain green.

## Progress

- [x] Confirm the release workflow still marks Windows best-effort and uses the
  MSVC no-default-features build.
- [x] Identify the smallest current-main candidate containing the fix.
- [x] Obtain publish authority and run hosted Windows CI.
- [x] Record the exact run URL, candidate SHA, executable paths and hashes, and
      downloadable artifact result.

## Decision log

| Date | Decision | Evidence / reason |
| --- | --- | --- |
| 2026-07-18 | Keep CI proof separate from release-policy changes. | A passing best-effort job does not itself authorize making Windows release-blocking. |
| 2026-08-14 | Accept the v0.7.0 release run as the exact hosted Windows proof. | Release source `9dd7217` contains `6fa6450`; its Windows job built and packaged successfully, uploaded two files, and the published files were downloaded and hash-verified. |
| 2026-08-14 | Route durable fail-closed checks separately. | End-to-end downloadable artifacts prove this task's observable outcome; adding a pre-upload non-empty/hash assertion remains release-workflow hardening under `MDV-021`. |

## Blockers and escalation

- None for the completed build/package proof. Windows remains best-effort and a
  native Windows launch was outside this plan.

## Final evidence

- Exact candidate: release source/tag
  `9dd7217b5c6c1328630451c1452a21eb1830041d`; `git merge-base --is-ancestor
  6fa6450 9dd7217` exited 0, proving the owned lifetime fix is in the candidate.
- Hosted workflow [31807308166](https://github.com/minchenlee/rmdv/actions/runs/31807308166),
  Windows job [94789227324](https://github.com/minchenlee/rmdv/actions/runs/31807308166/job/94789227324),
  completed successfully. The MSVC no-default-features release build finished,
  and cargo-packager produced the NSIS setup executable.
- The Actions upload reported exactly two files, 24,117,505 compressed bytes,
  artifact ID `9222178800`, and artifact-ZIP SHA-256
  `c155407a8441240014450a6a857b593aa1cee0196441fffa22f82027971b1cf6`.
- The public v0.7.0 release assets were independently downloaded:
  `rmdv.exe` is 38,494,208 bytes with SHA-256
  `e676563821429ba6bf8924f477b1ca6cd2fd20bcbfab9fb9010ea5d42d7dfcaa`;
  `rmdv_0.7.0_x64-setup.exe` is 10,670,275 bytes with SHA-256
  `fcc836d81fb38b47ec469aa14933a7ba02ea83842ac6a9f37e586dbbebe042a1`.
  Both values match the published `SHA256SUMS` and GitHub asset digests.
- The release candidate's IPC/integration suites and current-platform Cargo
  check passed before PR #24; merged-main Linux CI run `31807215705` passed on
  exact release source.
- Acceptance verdict: done for hosted Windows build/package/download proof.
  The workflow still lacks the plan's preferred pre-upload fail-closed assertion;
  `MDV-021` owns that reusable guard rather than keeping this proven outcome
  artificially open.
