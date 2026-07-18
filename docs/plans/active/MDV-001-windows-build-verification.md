# MDV-001 — Verify the Windows build path

State: ready
Owner / accountable lead: unassigned
Active writer: none
Created: 2026-07-18
Updated: 2026-07-18

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
- The configured NSIS packaging step succeeds, then an explicit CI assertion
  proves at least one non-empty application `.exe` and one non-empty
  `*-setup.exe` before upload; record each file's SHA-256.
- The `windows-x86_64` artifact is downloadable from the exact run and contains
  the asserted files matching the recorded hashes. A non-failing upload step is
  insufficient because the current workflow uses `if-no-files-found: warn`.
- IPC protocol/e2e tests and the current-platform `cargo check` remain green.

## Progress

- [x] Confirm the release workflow still marks Windows best-effort and uses the
  MSVC no-default-features build.
- [ ] Identify the smallest current-main candidate containing the fix.
- [ ] Obtain publish authority and run hosted Windows CI.
- [ ] Record the exact run URL, candidate SHA, executable paths and hashes, and
  downloadable artifact result.

## Decision log

| Date | Decision | Evidence / reason |
| --- | --- | --- |
| 2026-07-18 | Keep CI proof separate from release-policy changes. | A passing best-effort job does not itself authorize making Windows release-blocking. |

## Blockers and escalation

- Requires explicit owner approval to push the exact CI candidate.
- If packaging fails after compilation passes, classify the package failure
  separately before changing source or workflow policy.

## Final evidence

- Pending: candidate SHA, GitHub Actions URL, build/package results, executable
  paths and SHA-256 values, downloadable artifact URL/content, and remaining
  release-policy decision.
