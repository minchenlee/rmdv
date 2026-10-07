# rmdv — project status

Last verified: 2026-10-07 (Asia/Taipei)
Stale after: 7 days
Canonical repository: `~/Documents/GitHub/mdv`
Expected branch / HEAD / PR: start new work from the live `origin/main`
(`4204211` when verified). Latest release: `v0.7.0` → `9dd7217`; `v0.8.0` is
being prepared (`MDV-039`).
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
- `MDV-036`: bare-directory launch fix (PR #45).
- `MDV-031`: phase-3 visual refinement with the opt-in Soft syntax option
  (PR #47).
- `MDV-037`: optional macOS window glass (PR #51).
- `MDV-019`: the Settings page (PR #54).
- `MDV-021`: fail-closed release gates (PR #55, `57badd8`); dry run green.
- Site copy fixes deployed 2026-10-07 (PR #49; Worker version `b9383cc5`).

The owner approved each merge. Next is the v0.8.0 release (`MDV-039`).

## Live workstreams

| ID | State | Owner | Outcome | Acceptance | Plan |
| --- | --- | --- | --- | --- | --- |
| MDV-039 | in_progress | Claude | Release v0.8.0. | Content-pack checklist. | [`MDV-039`](docs/plans/active/MDV-039-release-v0.8.0.md) |

The roadmap order and every other task are in [`docs/BACKLOG.md`](docs/BACKLOG.md).

## Human decisions / blockers

- Merges, tags, releases, and deployments need explicit owner authority per
  action. Pushing branches and opening PRs after verification is authorized for
  roadmap work.
- Visual work needs owner acceptance on device; batch it into one verify
  build per round.
- Phase-5 tasks (`MDV-032`–`MDV-035`) start with a design for the owner.
- GitHub Actions: the repo is public and uses standard runners only, which do
  not consume the owner's (exhausted) private-repo minutes. Keep it that way.
- Release dispatches default to `dry_run: true`; a real release needs
  `dry_run: false` and owner authority.

## Next safe actions

1. Run the v0.8.0 release checks (CI, plus local checks when disk allows), then
   the native smoke.
2. Record the `MDV-008` clippy baseline (report-only).

## Verification state

### Verified now

- `origin/main` is `20838b1`. PRs #45 (`3909cfa`), #46 (`71a408e`), and #47
  (`20838b1`) were squash-merged on 2026-10-05 with owner authority. Before
  that, PRs #34–#43 were squash-merged on 2026-10-05
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

- A local release test run on the exact `20838b1` tree. #45 and #47 were each
  tested locally (#47: 461 library tests, lean check, release build) but not
  together; main push CI covers the combination.
- `MDV-038`: a table row clipped at the viewport top can render without text
  after `goto` (pre-existing).
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
