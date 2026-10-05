# MDV-029 — CI coverage and release gates

State: done
Owner / accountable lead: owner (minchenlee); implementation by Claude
Active writer: none
Created: 2026-10-05
Updated: 2026-10-05

## Outcome

CI checks what the release workflow ships, and a release cannot publish from a
commit whose tests fail.

## Scope

1. **De-duplicate triggers.** `ci.yml` runs on `push` to `codex/**` *and* on
   `pull_request`, so every PR commit runs the full job twice. Keep `push` for
   `main` and `pull_request` for PRs.
2. **macOS tests on PRs.** Releases ship macOS builds but only Linux runs tests.
   Add a `macos-14` job running the library and integration tests, PR-only.
3. **Windows compile check.** Releases build Windows with
   `--no-default-features`; add `cargo check --no-default-features` on
   `windows-latest`, PR-only, without the full build.
4. **Release test gate.** `release.yml` builds and publishes without running
   tests; make the build jobs depend on a test job.
5. **`cargo audit`** (report-only first; fail on new advisories once the
   baseline is known).
6. **Build cache.** `ci.yml` runs `cargo clean --target …` because liteparse's
   build-script output (PDFium) is not cached. Experiment with cleaning only
   `liteparse-pdfium-sys` / its build output; keep the full clean if the
   experiment fails.

## Constraints and authority

- `minchenlee/rmdv` is public and every job uses standard GitHub-hosted runners
  (`ubuntu-latest`, `macos-14`, `windows-latest`), which do not consume the
  owner's Actions minutes. The owner's private-repo minutes are exhausted, so do
  not introduce larger runners or anything billed.
- Never trigger `release.yml` with a real tag. Exercise the gate through a
  branch-only dry run (e.g. `workflow_dispatch` with publication disabled) or by
  proving the job graph statically plus a PR run of the shared test job.
- One heavy local job at a time; announce local slots to `tk-main-orch`.

## Acceptance evidence

- A PR run shows Linux, macOS, and Windows jobs with expected results and no
  duplicate push+PR run.
- `release.yml` job graph requires the test job before any build/publish job.
- `cargo audit` output recorded; cache experiment result recorded with timings.

## Progress

- [x] Triggers and new jobs.
- [x] Release gate.
- [x] `cargo audit`.
- [x] Cache experiment.

## Final evidence

- [PR #36](https://github.com/minchenlee/rmdv/pull/36), squash-merged as
  `8ab3ca9` on 2026-10-05 with owner authority.
- `ci.yml` pushes run for `main` only; PRs get one `pull_request` run. Every
  later PR (#37–#43) showed one run per push with `Tests / Linux tests`,
  `Tests / macOS tests` (`macos-14`), `Windows check`, `Linux x86_64`, and
  `cargo audit (report-only)`, all on standard runners.
- Shared `tests.yml` is reused by `release.yml`; `build-macos`, `build-linux`,
  and `build-windows` all declare `needs: tests`, and `release` needs all three
  builds. Proven statically; no tag was pushed.
- `cargo audit` baseline (2026-10-05): 5 vulnerabilities (crossbeam-epoch,
  quick-xml ×2, quinn-proto, rustls) and 9 warnings (unmaintained paste,
  rustybuzz, ttf-parser; unsound anyhow, event-listener, lru, memmap2). The job
  stays report-only until the baseline is addressed.
- Cache: the full `cargo clean` was replaced by caching the PDFium download;
  the Linux package job dropped from 10–11 min to 2m28s on a cache hit.
- Main push CI passed on every merge commit from `48732fd` to `0ed7305`.
