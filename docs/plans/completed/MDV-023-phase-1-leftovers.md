# MDV-023 — Phase-1 leftovers

State: done
Owner / accountable lead: owner (minchenlee); implementation by Claude
Active writer: none
Created: 2026-10-05
Updated: 2026-10-05

## Outcome

Finish the two cleanups the refactor roadmap attached to the `src/app/` split
(MDV-022) without changing behavior.

## Non-goals

- No other refactor, rename, or formatting churn.
- No change to Full Mindmap behavior or request guards.

## Owned surfaces

- `src/app/mod.rs`: the `first_frame_at` field, which is written once and never
  read.
- `src/app/full_mindmap.rs`: six `self.full_mindmap.as_mut().expect("checked
  above")` calls (lines ~778, ~1005, ~1157, ~1233–1234, ~1285 at `a44552e`). A
  later edit that moves the earlier check would turn them into GUI panics;
  replace them with `let … else` early returns that keep today's control flow.

## Acceptance evidence

- `git grep first_frame_at` and `git grep 'checked above'` return nothing in
  `src/`.
- Each replacement returns the same value the function already returns when
  Full Mindmap is absent.
- `cargo test --release -- --test-threads=1`, lean
  `cargo check --release --no-default-features --lib --bins --tests`,
  `cargo fmt --all --check`, `git diff --check`, and Linux CI pass.

## Progress

- [x] Remove `first_frame_at`.
- [x] Replace the six `expect` calls.
- [x] Verify and open the PR; merge only with owner authority.

## Final evidence

- [PR #35](https://github.com/minchenlee/rmdv/pull/35), squash-merged as
  `1c7a10b` on 2026-10-05 with owner authority; PR CI and main push CI passed.
- Release tests, lean check, fmt, and `git diff --check` passed locally before
  the PR; the change also shipped in the owner-accepted phase-2 verify build.
