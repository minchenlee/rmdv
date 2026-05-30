# Vault-wide Search — Design

**Date:** 2026-05-30
**Status:** Approved (design phase)
**Backlog item:** #3 cross-file full-text search (academic + PKM personas, 9/10)

## Problem

`⌘F` (`ToggleSearch`) searches the **current document only** (`search::find_in_blocks` over `self.ast`). Users with a folder of markdown files (a "vault") cannot find text across files. The walked corpus (`workspace_files: Vec<PathBuf>`) already exists; nothing searches it.

## Goal

A `⌘⇧F` overlay that searches all files in `workspace_files` for a query string and lists every matching line. Selecting a result opens that file and scrolls to the matched line.

## Decisions (from brainstorming)

| Decision | Choice | Why |
|----------|--------|-----|
| Corpus read | **On-demand** (no index) | Zero persistent memory (mdv is memory-conscious — see mem-reduce history), always current, no staleness/invalidation logic. Fine for hundreds of files. |
| Per-file scan | **Raw text grep** (`find_all`) | No parse cost. Byte offset → line number via existing `build_byte_to_line`. Standard ripgrep/PKM model. |
| Result granularity | **Row per match-line** | `path:line` + snippet, lands on exact line. Reuses `PendingNav.line` → `Cmd::Goto`. VSCode/ripgrep UX. |
| Trigger | **`⌘⇧F`** (and capital `F`) | Free slot. Natural pair with `⌘F`. Must be ordered before plain `"f" if cmd` (mirrors existing `p`/`P`). |

## Architecture

### New module: `src/vault_search.rs`

```rust
use std::path::PathBuf;

pub struct VaultHit {
    pub path: PathBuf,
    pub line: u32,        // 1-based, matches Cmd::Goto convention
    pub snippet: String,  // matched line, trimmed to ~80 chars centered on match
}

pub struct VaultResults {
    pub hits: Vec<VaultHit>,
    pub truncated: bool,  // true if cap reached
    pub seq: u64,         // staleness guard, echoed from request
}

pub const MAX_HITS: usize = 200;

pub async fn run(files: Vec<PathBuf>, query: String, seq: u64) -> VaultResults;
```

`run`:
1. Empty query → empty results (with `seq`).
2. For each `path` in `files`: `fs::read_to_string` (skip on error). `search::find_all(&text, &query)` → byte offsets. For each offset: `build_byte_to_line(&text).line_for_byte(offset)` → line; extract that line's text, trim to a window centered on the match.
   - Build the byte→line table once per file (not per match).
3. Push `VaultHit`s until `MAX_HITS`; set `truncated` when exceeded and stop.

Snippet trimming: take the full source line containing the match, collapse leading whitespace, clamp to ~80 chars with a window around the match column so the match is visible.

### `src/app.rs` changes

**Overlay variant:** add `VaultSearch` to `enum Overlay`.

**State (3 fields on `App`):**
```rust
pub vault_results: Vec<vault_search::VaultHit>,
pub vault_truncated: bool,
pub vault_seq: u64,
```
Query text + row cursor reuse existing `overlay_query` / `overlay_selected`.

**Message:** `VaultSearchDone(vault_search::VaultResults)`.

**Keybinding** (in the cmd-key match, before `"f" if cmd`):
```rust
"f" | "F" if cmd && mods.shift() => return Message::OpenVaultSearch,
```
(`OpenVaultSearch` opens overlay + focuses input, like other overlay openers.)

**Handler wiring:**
- `OpenVaultSearch`: `open_overlay(Overlay::VaultSearch)` + focus `overlay_input_id`. Leaves results empty (empty query).
- `OverlayQueryChanged(q)` — when `overlay == VaultSearch`: set `overlay_query = q`, `vault_seq += 1`, return `Task::perform(vault_search::run(self.workspace_files.clone(), q, self.vault_seq), Message::VaultSearchDone)`. Other overlays keep existing fuzzy-filter behavior.
  - **Debounce:** `run` sleeps ~150ms before scanning; result dropped on arrival if `seq != self.vault_seq`. No timer subsystem needed.
- `VaultSearchDone(r)`: if `r.seq == self.vault_seq` → `self.vault_results = r.hits; self.vault_truncated = r.truncated; self.overlay_selected = 0`. Else drop.
- `OverlayMove(d)` — for `VaultSearch`: clamp cursor over `vault_results.len()`.
- `OverlayConfirm` — for `VaultSearch`: `let hit = &self.vault_results[self.overlay_selected]`; `self.pending_nav = Some(PendingNav { line: Some(hit.line), ..Default::default() })`; close overlay; `Task::done(Message::Open(hit.path.clone()))`.
- `scroll_overlay_to_cursor`: add arm `Overlay::VaultSearch => (self.vault_results.len().min(MAX_HITS), 32.0)`.

**Render `vault_search_overlay(query, hits, selected, truncated, pal)`:**
- Clone of `file_finder_overlay` layout: text_input (id `overlay_input_id`, `on_input = OverlayQueryChanged`) + scrollable rows (scroll id `overlay_scroll_id`).
- Row: dim `relpath:line` + snippet. Selected row highlighted. Fixed 32px height.
- Footer: empty query → "Type to search all files"; else "{n} results" or "200+ results (refine query)" when truncated; "No matches" when zero.
- Wired in `view()` overlay match + `open_overlay`.

**Command palette:** add entry `("Search All Files  ⌘⇧F", Message::OpenVaultSearch)`.

## Data flow

```
⌘⇧F → OpenVaultSearch → Overlay::VaultSearch (input focused)
  type → OverlayQueryChanged(q) → vault_seq++ → Task::perform(run(files, q, seq))
    run: sleep 150ms → per file: read → find_all → line_for_byte → snippet → VaultResults
  → VaultSearchDone(r) → if r.seq==vault_seq: store hits, cursor=0
  ↑/↓ → OverlayMove → clamp over vault_results
  ⏎ → OverlayConfirm → pending_nav.line = hit.line; Open(hit.path)
    → FileLoaded → existing nav_task consumes pending_nav.line → Cmd::Goto{line} → scroll
  Esc → CloseOverlay
```

## Reuse (no new infra)

- `search::find_all` — byte-offset substring scan (case-insensitive).
- `ipc::lines::build_byte_to_line` / `line_for_byte` — byte→1-based-line, same convention as Goto/sections.
- `PendingNav.line` → `FileLoaded` nav_task → `Cmd::Goto` — exact-line scroll already wired for `.md` link fragments.
- Overlay plumbing: `OverlayMove`, `OverlayConfirm`, `OverlayScrolled`, `CloseOverlay`, `overlay_input_id`, `overlay_selected`, file-finder visual style.

## Error handling

- Unreadable file (permissions, non-UTF8): skipped silently in `run` (per-file `read_to_string` error ignored).
- Empty query: empty results, no scan.
- Stale result (query superseded mid-scan): dropped via `seq` mismatch.
- Goto line beyond loaded file (file changed on disk since walk): existing `apply_goto` range guard handles it (clamps / errors gracefully).

## Testing

Unit tests in `src/vault_search.rs` (tempdir fixtures):
1. Match across multiple files → hits from each, correct paths.
2. Line number correctness — match on line N reports line N (1-based).
3. Multiple matches in one file → one hit per occurrence with distinct lines.
4. Snippet contains the matched substring, trimmed to window.
5. `MAX_HITS` cap → `truncated == true`, `hits.len() == MAX_HITS`.
6. Empty query → empty hits.
7. Unreadable/binary file in set → skipped, other files still searched.

`seq` is plumbed through `run` unchanged (test asserts echo).

Manual: `⌘⇧F` in a folder, type, arrow, enter → lands on line in opened file. Esc closes.

## Out of scope (YAGNI)

- Persistent index / incremental update.
- Regex / fuzzy / whole-word / case toggle (raw case-insensitive substring only).
- Replace-across-files.
- Highlighting all matches in the opened doc (single-doc `⌘F` already does in-doc highlight; vault search just lands on the line).
- Searching inside rendered diagram/math output beyond raw source text.
