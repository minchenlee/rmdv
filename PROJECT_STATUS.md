# rmdv — project status

Last verified: 2026-10-26 (Asia/Taipei)
Stale after: 7 days
Canonical repository: `~/Documents/GitHub/mdv`
Expected branch / HEAD / PR: start new work from the live `origin/main`.
Latest published release: `v0.10.1` → `09d6394` (2026-10-19, Release run
39197195676).
Authority: This is a routing snapshot. Verify Git, GitHub, runtime identity, and
manual evidence before mutation.

## Current outcome

- `v0.8.0` (2026-10-09), `v0.9.0` (2026-10-16), `v0.10.0` (2026-10-17), and
  `v0.10.1` (2026-10-19) are published. v0.9.0 added
  rounded tables, Notion-like typography with Line spacing, and Quick Slot
  assign and confirm. Record:
  [`docs/releases/v0.9.0-content-pack.md`](docs/releases/v0.9.0-content-pack.md).
- `v0.10.0` shipped `MDV-040`: selectable and copyable text (drag, double
  and triple click, ⌘A, ⌘C under any input source, and one drag across blocks
  in the visible range) and CJK reading (bundled Noto Sans TC, taller CJK
  lines, squeezed full-width punctuation). The owner accepted it on a trial
  build. It uses a vendored `iced_selection` (see
  `vendor/iced_selection/VENDORED.md`).
- Known limits of cross-block selection: no selection past the window edge
  (no auto-scroll); an endpoint scrolled out of the virtual window copies its
  whole block; after a double or triple click, dragging into another block
  selects by character; Shift-extend works inside one block.
- `v0.10.1` shipped `MDV-044` (PR #69, `a72381f`): Full Mindmap lists a
  folder's direct files and folders up to 10,000 items, and a folder with more
  than 2,000 items opens in pages of 2,000 behind a `More (N left)` node.
  Notes: [`docs/releases/v0.10.1-release-notes.md`](docs/releases/v0.10.1-release-notes.md).
- `MDV-048` pointer support is on `main`, not released: PRs #70–#79 and #81
  merged with merge commits on 2026-10-26 (owner authority after the P8c
  trial); `main` is `8d8b9eb`. It covers overlays, the toolbar and window
  drag, close protection, Back/Forward, link hover, the mind map pointer
  model, gestures and the rmdv image viewer, context menus and file
  operations, reader selection, the sidebar peek, global Quick Slots, the
  native menu bar, and window frame persistence. Plan:
  [`docs/plans/completed/MDV-048-pointer-support.md`](docs/plans/completed/MDV-048-pointer-support.md).

## Live workstreams

- `MDV-049` Mermaid: external PR #82 (Merman, dagre only) conflicts with
  `8d8b9eb` in `src/diagram.rs`; we rebase it for the author.
- `MDV-050` reading width: external PR #84 conflicts in `message.rs`,
  `mod.rs`, `chrome.rs`; we rebase it for the author.
- External PR #83 (preview modal) overlaps the merged MDV-048 image viewer;
  the owner decides after a comparison.

## Human decisions / blockers

- Merges, tags, releases, and deployments need explicit owner authority per
  action. Pushing branches and opening PRs after verification is authorized for
  roadmap work.
- Visual work needs owner acceptance on device; batch it into one verify
  build per round.
- Phase-5 tasks (`MDV-032`–`MDV-035`) start with a design for the owner.
- GitHub Actions: the repo is public and uses standard runners only, which do
  not consume the owner's (exhausted) private-repo minutes. Keep it that way.
- Website deploys run locally with `wrangler deploy` by the owner. The
  `deploy-site` workflow fails because the repo has no `CLOUDFLARE_API_TOKEN`
  secret (run 39041702967); v0.10.0's site went live this way (Worker version
  `6e4b9588`, 2026-10-17). The v0.10.1 version strings are on `main`; the
  site deploy waits for the owner.
- Release dispatches default to `dry_run: true`; a real release needs
  `dry_run: false` and owner authority.

## Next safe actions

1. Rebase PRs #82 and #84 onto `main` and get their CI green.
2. Prepare `v0.11.0` (MDV-048, and MDV-049 if it lands) when the owner
   authorizes a release.
3. Re-run the cold-start and parse perf check on a quiet machine.

## Verification state

### Verified now

- `v0.10.1` was tagged at `09d6394` by the owner (2026-10-19). Release run
  39197195676 passed every job. The seven installers match `SHA256SUMS`, and
  `latest.json` reports `0.10.1` for darwin-aarch64, darwin-x86_64, and
  linux-x86_64. The `aarch64` app and dmg pass codesign, stapler, and `spctl`
  (Notarized Developer ID), version `0.10.1`. The release body is the
  committed release notes file.
- A dry run before the tag (run 39193575518) failed only at "Check out release
  notes", because the tag did not exist yet; tracked on the board as
  `rmdv-release-dryrun-notes`.
- `v0.10.0` was tagged at `9e876c0` (owner authority, 2026-10-17); the Release
  workflow passed every job and the seven installers match `SHA256SUMS`. The
  `aarch64` dmg and app were also checked by hand: codesign valid, stapled,
  `spctl` accepted as Notarized Developer ID, version `0.10.0`.
- Before the tag, the cross-block tree (`d53f0a3`) passed `cargo fmt --check`, `cargo check
  --all-targets` with no warnings, 536 library tests, and all integration
  suites in release mode.
- v0.9.0 release evidence is in the v0.9.0 content pack; v0.10.0 notes are in
  [`docs/releases/v0.10.0-release-notes.md`](docs/releases/v0.10.0-release-notes.md).

### Not verified / follow-up

- Cross-block selection was checked with headless screenshots, unit tests, and
  an owner trial build; there is no automated native-window test.
- The x86_64, Linux, and Windows v0.10.0 and v0.10.1 installers were checked
  by SHA-256 only, not opened.
- `MDV-044`: the >10,000-item behavior is inferred from source, not tested on
  a real folder. Cold start and frame time were not measured.
- `MDV-038`: a table row clipped at the viewport top can render without text
  after `goto` (pre-existing). `MDV-042`: `goto` stops about one block high.
- `HlCache` is bounded by entry count only (`MDV-002`).

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
