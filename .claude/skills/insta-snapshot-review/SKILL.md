---
name: insta-snapshot-review
description: Triage insta snapshot diffs after running tests. Use when user says "snapshot review", "insta review", "accept snapshots", or after `cargo test` reports snapshot mismatches.
---

# insta-snapshot-review

Walks pending insta snapshot diffs and decides accept/reject per file.

## Steps

1. Run `cargo insta pending-snapshots --workspace` — list all `.snap.new` files.
2. For each pending snapshot:
   - Read the `.snap` (old) and `.snap.new` (new) files.
   - Classify diff:
     - **Identity-preserving rename** (struct/field renamed, content equivalent): auto-accept candidate.
     - **Format-only** (whitespace, ordering, trailing newline): auto-accept candidate.
     - **Content change** (different semantic output): flag for human review.
3. Print summary:
   ```
   ACCEPT: <count> snapshots (renames + format)
   REVIEW: <count> snapshots (content changes)
   ```
4. List REVIEW snapshots with file path and 10-line excerpt of the diff.
5. Wait for user decision before running `cargo insta accept`.

## Tooling

- `cargo insta` must be installed: `cargo install cargo-insta` if missing.
- Snapshots live in `tests/snapshots/`.

## Safety

- Never auto-accept content changes. Always show diff and ask.
- For ambiguous cases, default to REVIEW.
