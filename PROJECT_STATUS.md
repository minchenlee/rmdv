# rmdv — project status

Last verified: 2026-10-05 (Asia/Taipei)
Stale after: 7 days
Canonical repository: `/Users/liminchen/Documents/GitHub/mdv`
Expected branch / HEAD / PR: start new work from the live `origin/main`
(`a44552e` when verified). Latest release: `v0.7.0` → `9dd7217`; `main` is
unreleased since then.
Authority: This is a routing snapshot. Verify Git, GitHub, runtime identity, and
manual evidence before mutation.

## Current outcome

The first wave of the owner-approved refactor roadmap has landed: safety fixes,
the `src/app/` module split with design tokens, three search/finder
performance fixes, and theme persistence (PRs #28–#33, `MDV-022`). Quick Slots
landed earlier through PR #26. The next roadmap step is `MDV-023` (phase-1
leftovers), then build/CI (`MDV-029`).

## Live workstreams

| ID | State | Owner | Outcome | Acceptance | Plan |
| --- | --- | --- | --- | --- | --- |
| MDV-023 | in_progress | Claude | Remove dead `first_frame_at` and the six `expect("checked above")` panics. | No behavior change; release tests and CI pass. | [`MDV-023`](docs/plans/active/MDV-023-phase-1-leftovers.md) |
| MDV-029 | ready | Claude (next) | CI covers macOS/Windows and gates releases on tests. | Proven on a PR run; release gate exercised without publishing. | [`MDV-029`](docs/plans/active/MDV-029-ci-coverage-and-release-gates.md) |

The roadmap order and every other task are in [`docs/BACKLOG.md`](docs/BACKLOG.md).

## Human decisions / blockers

- Merges, tags, releases, and deployments need explicit owner authority per
  action. Pushing branches and opening PRs after verification is authorized for
  roadmap work.
- Performance items `MDV-024`–`MDV-028` and visual work `MDV-031`/`MDV-019`
  need owner acceptance on device; batch them into one verify build per round.
- Phase-5 tasks (`MDV-032`–`MDV-035`) start with a design for the owner.
- GitHub Actions: the repo is public and uses standard runners only, which do
  not consume the owner's (exhausted) private-repo minutes. Keep it that way.
- `MDV-021` (fail-closed release artifact gates) must land before the next
  release candidate.

## Next safe actions

1. Finish and verify `MDV-023`; open its PR and stop before merge.
2. Start `MDV-029` on a fresh branch from `origin/main`.
3. Plan one owner acceptance round covering `MDV-024`–`MDV-028`.

## Verification state

### Verified now

- `origin/main` is `a44552e`; PRs #28–#33 are squash-merged
  (`f8a30fa`, `f9ba31f`, `fe7aa43`, `e675b51`, `5773bc6`, `a44552e`) and main
  CI passed on each merge commit.
- `cargo test --release -- --test-threads=1` on `a44552e`'s tree: 439 library
  tests plus all integration suites pass (run on the owner-tested verify build,
  identical tree).
- Owner native acceptance of #28–#33 on a separately bundled verify app
  (2026-10-04/05); details in [`MDV-022`](docs/plans/completed/MDV-022-refactor-phases-0-2.md).
- Local `main` is `1244e4c`, an ancestor of `origin/main`; old local-only
  commits are preserved on `archive/main-before-sync-20261004` (`MDV-010`
  closed by archival).

### Not verified / follow-up

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
