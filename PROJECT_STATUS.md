# rmdv — project status

Last verified: 2026-10-05 (Asia/Taipei)
Stale after: 7 days
Canonical repository: `/Users/liminchen/Documents/GitHub/mdv`
Expected branch / HEAD / PR: start new work from the live `origin/main`
(`0ed7305` when verified). Latest release: `v0.7.0` → `9dd7217`; `main` is
unreleased since then.
Authority: This is a routing snapshot. Verify Git, GitHub, runtime identity, and
manual evidence before mutation.

## Current outcome

The refactor roadmap through phase 2 has landed:
- PRs #28–#33 (`MDV-022`): safety fixes, the `src/app/` split with design
  tokens, search and finder performance, and theme persistence.
- `MDV-023`: phase-1 leftovers (PR #35).
- `MDV-029`: CI coverage and release gates (PR #36).
- `MDV-024`–`MDV-028`: the phase-2 performance round (PRs #37–#41).
- Two acceptance fixes: ⌘Z undo (PR #42) and lazy scan of folders the
  workspace budget skipped (PR #43).

The owner accepted everything on device. Next is phase 3: `MDV-031`, then
`MDV-019`.

## Live workstreams

| ID | State | Owner | Outcome | Acceptance | Plan |
| --- | --- | --- | --- | --- | --- |
| MDV-031 | ready | Claude (next) | Zeron-method visual refinement on the design tokens. | Owner native acceptance per theme; contrast unit tests. | Plan to be written before work starts. |
| MDV-036 | ready | Claude | Bare-directory launch stops showing a "Couldn't open file — Is a directory" card. | Regression test; file/`open-folder` launches unchanged. | — |

The roadmap order and every other task are in [`docs/BACKLOG.md`](docs/BACKLOG.md).

## Human decisions / blockers

- Merges, tags, releases, and deployments need explicit owner authority per
  action. Pushing branches and opening PRs after verification is authorized for
  roadmap work.
- Visual work `MDV-031`/`MDV-019` needs owner acceptance on device; batch it
  into one verify build per round.
- Phase-5 tasks (`MDV-032`–`MDV-035`) start with a design for the owner.
- GitHub Actions: the repo is public and uses standard runners only, which do
  not consume the owner's (exhausted) private-repo minutes. Keep it that way.
- `MDV-021` (fail-closed release artifact gates) must land before the next
  release candidate.

## Next safe actions

1. Write the `MDV-031` plan (tokens, contrast tests, per-theme acceptance) and
   start it on a fresh branch from `origin/main`.
2. Fix `MDV-036` with a regression test; open its PR and stop before merge.
3. Record the `MDV-008` clippy baseline (report-only).

## Verification state

### Verified now

- `origin/main` is `0ed7305`. PRs #34–#43 were squash-merged on 2026-10-05
  with owner authority (`48732fd`, `1c7a10b`, `8ab3ca9`, `f60da48`,
  `b971c35`, `27fcd52`, `f280f82`, `f4ad819`, `1df1b13`, `0ed7305`).
- Each PR's final head passed CI: Linux and macOS tests, the Windows check, and
  the Linux package. Main push CI passed on each merge commit (`f280f82`'s run
  was cancelled by the next push).
- `main@0ed7305` source equals the owner-accepted verify build
  `codex/verify-phase2@d408864`, except the order of tests in
  `src/app/tests.rs` and the docs/CI files from #34 and #36. That build passed
  `cargo test --release -- --test-threads=1` (452 library tests plus all
  integration suites), the lean check, and a release build.
- Owner native acceptance of #37–#43 on the verify app (2026-10-05); details in
  [`MDV-024–028`](docs/plans/completed/MDV-024-028-phase-2-performance.md).
- Local `main` is `1244e4c`, an ancestor of `origin/main`; old local-only
  commits are preserved on `archive/main-before-sync-20261004` (`MDV-010`
  closed by archival).

### Not verified / follow-up

- A local release test run on the exact `0ed7305` tree. It is pending a free
  machine slot; the source-equal verify build passed.
- One unreproduced "Couldn't open file" card after relaunching the verify app
  (see `MDV-022`).
- `HlCache` is bounded by entry count only (`MDV-002`).
- v0.7.0 release evidence is in the archived snapshot
  [`docs/status-history/2026-08-14-v0.7.0-project-status.md`](docs/status-history/2026-08-14-v0.7.0-project-status.md).

## Routes

- Product contract: [`PRODUCT.md`](PRODUCT.md)
- User-facing overview: [`README.md`](README.md)
- Backlog: [`docs/BACKLOG.md`](docs/BACKLOG.md)
- Active plans: [`docs/plans/active/`](docs/plans/active/)
- Completed plans: [`docs/plans/completed/`](docs/plans/completed/)
- Release records: [`docs/releases/`](docs/releases/)
- Status history: [`docs/status-history/`](docs/status-history/)
- Module map: [`src/AGENTS.md`](src/AGENTS.md)
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
