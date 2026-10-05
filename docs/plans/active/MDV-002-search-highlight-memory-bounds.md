# MDV-002 — Bound search and highlight memory

State: ready
Owner / accountable lead: unassigned
Active writer: none
Created: 2026-07-18
Updated: 2026-10-05

## Outcome

Keep in-document and block search responsive on large inputs by bounding match
retention and syntax-highlight cache memory while preserving ordinary search
and navigation behavior.

## Non-goals

- Do not revive or cherry-pick the archived memory branch wholesale.
- Do not silently drop matches or highlighting without a truthful user-visible
  truncation/degradation contract.
- Do not broaden into general renderer or workspace-memory optimization.

## Constraints and authority

- Budgets must cover bytes as well as entry counts; entry-only caps can retain
  arbitrarily large sources.
- Search ordering, current-match navigation, Unicode offsets, and stale-result
  safety remain behavior contracts.
- Measure before and after on fixed fixtures; passing tests alone does not prove
  the memory outcome.

## Owned and excluded surfaces

- Owned: `src/search.rs` (`find_all`, `find_in_blocks`),
  `src/highlight.rs` (`HlCache`), their callers under `src/app/`, focused tests,
  and a reproducible benchmark artifact.
- Excluded: Full Mindmap workspace indexing, PDF extraction, image/diagram
  caches, and unrelated formatting debt.

## Acceptance evidence

- Named match-count/byte and highlight-source/total-byte budgets are documented
  in code and tests.
- Truncation or cache eviction is deterministic, observable where needed, and
  cannot corrupt match navigation.
- Adversarial large-input tests cover match floods, huge highlighted sources,
  repeated edits, Unicode, and cache replacement.
- A controlled before/after run reports peak memory and visible-behavior
  equivalence; relevant unit/integration checks pass.

## Progress

- [x] Confirm current unbounded vector entrypoints and entry-count-only cache.
- [x] Vault search (`find_all`): ASCII fast path, offset map built only after a
  hit, 16 MiB per-file cap, scan on `spawn_blocking`. 8 MiB file peak 72 → 8
  MiB. PR #30 (`fe7aa43`).
- [x] In-document search (`find_in_blocks`): results stored per matching block
  (`search::Matches`) instead of per hit, so no truncation is needed; scratch
  buffers; 80 ms debounce above 1 MiB. 8 MiB document: 84–123 → 17–33 ms per
  keystroke, ~495k → ≤40 allocations, 24 → 5 MiB extra peak. PR #33
  (`a44552e`).
- [ ] `HlCache` (`src/highlight.rs`) is still bounded only by entry count
  (`CACHE_MAX = 200`); add a source/total-byte budget.
- [ ] Define budgets and the observable truncation/degradation contract.
- [ ] Implement focused tests and the smallest bounded design.
- [ ] Measure and run cross-boundary verification.

## Decision log

| Date | Decision | Evidence / reason |
| --- | --- | --- |
| 2026-10-05 | Compact search results instead of capping them. | Per-block counts keep every match navigable with memory proportional to matching blocks, so no truncation contract or UX change was needed. |
| 2026-07-18 | Require byte budgets and measured evidence. | `find_all`/`find_in_blocks` retain vectors and `HlCache` does not currently express total source-byte ownership. |

## Blockers and escalation

- Escalate to the owner if a truthful search-result cap changes expected UX or
  requires a new persistent setting.

## Final evidence

- Pending: chosen budgets, diff/commit, fixtures, before/after measurements,
  commands/results, and acceptance verdict.
