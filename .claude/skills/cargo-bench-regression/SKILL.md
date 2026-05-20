---
name: cargo-bench-regression
description: Run criterion cold_start bench, diff against baseline, surface >5% regressions. Use when user asks "bench", "perf check", "regression check", "cargo bench".
disable-model-invocation: true
---

# cargo-bench-regression

Runs `cargo bench --bench cold_start` against the saved baseline and reports regressions >5%.

## Steps

1. Verify baseline exists at `target/criterion/cold_start/base/`. If missing, run `cargo bench --bench cold_start -- --save-baseline base` first and stop (no diff to make).
2. Run `cargo bench --bench cold_start -- --baseline base`.
3. Parse criterion output for `change:` lines.
4. Report:
   - PASS: all changes within ±5%
   - REGRESS: list any benchmark with `>5%` slower
   - IMPROVE: list any benchmark with `>5%` faster
5. If REGRESS, suggest user investigate before merge.

## Baseline management

- `cargo bench --bench cold_start -- --save-baseline base` — overwrite baseline (use after intentional perf change)
- `cargo bench --bench cold_start -- --save-baseline before-X` — named baseline for A/B
- See `docs/benchmarks.md` for canonical numbers.

## Notes

- Criterion stores results in `target/criterion/`. Not committed.
- Bench harness is custom (`harness = false` in Cargo.toml) — see `benches/cold_start.rs`.
