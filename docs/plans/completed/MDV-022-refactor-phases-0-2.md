# MDV-022 — Refactor program, phases 0–2 (first wave)

State: done
Owner / accountable lead: owner (minchenlee); implementation by Claude
Active writer: none
Created: 2026-10-04
Updated: 2026-10-05

## Outcome

Land the owner-approved refactor roadmap of 2026-10-04 up to the point where
`src/app.rs` is split and the first performance fixes are in: safety fixes,
the module split, design tokens, three search/finder performance fixes, and
theme persistence (an owner-found gap during acceptance).

The remaining roadmap items are tracked as separate backlog rows (MDV-023 to
MDV-035, plus MDV-002, MDV-008 and MDV-019).

## Non-goals

- No visual change: radius/shadow tokens were introduced value-identical.
- No change to request-identity guards, `BlockId`, or parse threading
  (deferred to design-first tasks).

## Delivered

| PR | Squash on `main` | Scope |
| --- | --- | --- |
| [#28](https://github.com/minchenlee/rmdv/pull/28) | `f8a30fa` | Repo-wide rustfmt with a `cargo fmt --all --check` CI gate; atomic prefs/recent writes (`src/fs_atomic.rs`); IPC read/queue/reply timeouts and a 1 MiB request cap; unused `notify`/`arboard` removed; cached footer word count, lazy `HeightCache` estimate, cached Quick Slot checkpoint path; 64 MiB document cap with zero-copy UTF-8; shared remote-image client, 4 concurrent downloads, 25 MiB cap. |
| [#29](https://github.com/minchenlee/rmdv/pull/29) | `f9ba31f` | `src/app.rs` (21.9k lines) split into `src/app/` modules in move-only steps; `theme::radius` and `theme::shadow` tokens (value-identical); module map in `src/AGENTS.md`. |
| [#30](https://github.com/minchenlee/rmdv/pull/30) | `fe7aa43` | Vault search: ASCII fast path, lazy offset map, 16 MiB per-file cap, `spawn_blocking`. 8 MiB file peak 72 → 8 MiB. |
| [#31](https://github.com/minchenlee/rmdv/pull/31) | `e675b51` | File finder: results memoized by query/root/workspace revision; allocation-free ASCII fuzzy scoring. 10k paths 1.4–1.8 → 1.1–1.3 ms. |
| [#32](https://github.com/minchenlee/rmdv/pull/32) | `5773bc6` | The chosen theme (preset or custom) persists in `prefs.json` and is restored at launch. |
| [#33](https://github.com/minchenlee/rmdv/pull/33) | `a44552e` | In-document search: per-block `search::Matches`, scratch buffers, 80 ms debounce above 1 MiB. 8 MiB document 84–123 → 17–33 ms per keystroke. |

## Acceptance evidence

- Each PR: independent code review (accepted by the owner in place of a Codex
  review), `cargo test --release -- --test-threads=1`, lean
  `cargo check --release --no-default-features --lib --bins --tests`,
  `cargo fmt --all --check`, `git diff --check`, and Linux CI green on the
  exact head.
- Stacked PRs were rebased with `git rebase --onto origin/main <old base>`
  after each squash, re-tested locally on the new base, and re-run in CI
  before merging.
- Owner native acceptance on a separately bundled `rmdv Verify.app`
  (`com.minchenlee.rmdv.verify`, isolated IPC socket): #28–#31 on 2026-10-04
  (one finding: theme not persisted → #32), #32 and #33 on 2026-10-05,
  including the README table rendering.
- Final `main@a44552e` has the same tree as the owner-tested verify build
  (`63714b4`); main CI passed on every merge commit; 439 library tests.

## Not done / follow-up

- Phase-1 leftovers: dead `first_frame_at` field and six
  `expect("checked above")` calls (MDV-023).
- `ink`/`hairline`/`wash` colour helpers were deferred to the visual pass
  (MDV-031) because no call site is pixel-equivalent today.
- Unreproduced once: right after relaunching the verify app, a single
  "Couldn't open file — No such file or directory" card appeared although the
  requested README existed; a second relaunch was correct. Record only.
