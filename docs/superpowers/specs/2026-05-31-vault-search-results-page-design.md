# Vault Search Results Page — Design

**Date:** 2026-05-31
**Status:** Approved (design phase)
**Supersedes the UX of:** the floating overlay shipped in `987dc18` (`Overlay::VaultSearch`). Backend (`src/vault_search.rs`) is largely reused; the presentation changes from a cramped popup to a full reader-area results page modelled on Zed's search view.

## Problem

The shipped `⌘⇧F` vault search is a ~640px floating overlay with one flat row per match (`relpath:line` + 80-char snippet). The user prefers Zed's approach: a dedicated results **page** that fills the content area, groups matches by file, shows surrounding context lines with line numbers, and reports a match count.

## Goal

`⌘⇧F` opens a full-window search **page** (not an overlay) with:
- a query bar showing the input + `N matches in M files`,
- results grouped under collapsible file headers,
- each match shown with ±2 context lines and line numbers, the matched term highlighted,
- `↑↓` to move a match cursor, `Enter`/click to open the file at the line, `Esc` to exit.

## Decisions (from brainstorming)

| Decision | Choice |
|----------|--------|
| Presentation | Full reader-area **page** (replaces the overlay), like Mindmap mode |
| In scope | match count, ±2 context lines + line numbers, collapsible file groups (expanded default) |
| Out of scope (YAGNI) | case/whole-word/regex toggles, replace, glob filter, selection-seeded query, query persistence across exits |
| Context | ±2 source lines around each match |
| Collapse | File groups expanded by default; click `▼`/`▶` on a file header to fold |
| Nav | `↑↓` move match cursor, `Enter`/click open at line, `Esc` exit; click also opens |
| Exit | Opening a result replaces the page with the doc (Rendered view). `Esc` returns to prior view |
| Query state | Blank on every open |

## Architecture

Vault search is **not** a `ViewMode` (those require an open file; vault search is workspace-level and must render with no document open). It is a separate page gated by a new flag, dispatched at the top of `view()` before the file/welcome checks.

### Backend: `src/vault_search.rs` changes

`VaultHit` gains context lines. Replace the single `snippet` with the matched line plus neighbours:

```rust
pub struct VaultHit {
    pub path: PathBuf,
    pub line: u32,                 // 1-based line of the match
    pub col_start: usize,          // char offset of match within its line
    pub col_end: usize,            // char offset end (for highlight span)
    pub context: Vec<ContextLine>, // the match line + ±2 neighbours, in order
}

pub struct ContextLine {
    pub number: u32,               // 1-based source line number
    pub text: String,              // full line text (untrimmed; page handles overflow)
    pub is_match: bool,            // true for the line containing the match
}
```

`scan_text` builds, per match: the match line number/col span + the 2 lines above and below (clamped to file bounds), as `ContextLine`s. Context is computed from the file's line list (split once per file, reused across that file's matches). No trimming — the page scrolls horizontally-free via wrapping or clipping (lines render in a monospace column; long lines clip, consistent with the mockup).

`VaultResults` keeps `hits: Vec<VaultHit>`, `truncated`, `seq`. `MAX_HITS` stays 200. `run` unchanged except it now returns the richer hits.

Grouping by file is derived in the UI layer (hits arrive in file-walk order, already contiguous per file), not stored in the backend — keeps `vault_search.rs` about *finding*, not *display*.

### `src/app.rs` changes

**State (replaces overlay fields):**
```rust
pub vault_open: bool,                    // page visible
pub vault_results: Vec<vault_search::VaultHit>,
pub vault_truncated: bool,
pub vault_query: String,                 // dedicated (no longer reuses overlay_query)
pub vault_seq: u64,
pub vault_cursor: usize,                 // index into the flattened match list
pub vault_collapsed: HashSet<PathBuf>,   // files folded by the user
pub vault_prev_mode: ViewMode,           // restore target on Esc when a file is open
pub vault_viewport: Option<iced::widget::scrollable::Viewport>,
```
Remove `Overlay::VaultSearch` and its overlay-path wiring (the overlay-query/move/confirm branches, `scroll_overlay_to_cursor` arm, `vault_search_overlay` fn, `open_overlay` reset). The `VaultOpenHit` message is repurposed for row clicks on the page.

**Messages:**
```rust
OpenVaultSearch,                 // ⌘⇧F — show page, blank query
VaultQueryChanged(String),       // input on the page's query bar
VaultSearchDone(VaultResults),   // worker result (seq-guarded)
VaultMove(isize),                // ↑↓ cursor over flattened matches
VaultOpenSelected,               // Enter — open cursor's file at line
VaultOpenHit(usize),             // click a match row by flattened index
VaultToggleFile(PathBuf),        // ▼/▶ collapse a file group
VaultClose,                      // Esc — leave page, restore prior view
```

**Handlers:**
- `OpenVaultSearch`: `vault_open = true`, clear `vault_query`/`vault_results`/`vault_cursor`/`vault_collapsed`, record `vault_prev_mode = self.view_mode`, focus the query input. (If no workspace, fall back to `OpenFolderPicker` as today.)
- `VaultQueryChanged(q)`: set `vault_query`, `vault_seq += 1`, `vault_cursor = 0`, `Task::perform(vault_search::run(files, q, seq), VaultSearchDone)` (150ms debounce inside `run`, unchanged).
- `VaultSearchDone(r)`: if `r.seq == vault_seq` → store hits/truncated, `vault_cursor = 0`.
- `VaultMove(d)`: clamp cursor over the flattened **visible** match list (matches inside collapsed files are skipped). Scroll page to keep cursor visible (reuse `edge_scroll` with `vault_viewport`).
- `VaultOpenSelected` / `VaultOpenHit(idx)`: resolve the hit, set `pending_nav = Some(PendingNav { line: Some(hit.line), .. })`, `vault_open = false`, `Task::done(Message::Open(hit.path))`.
- `VaultToggleFile(path)`: toggle membership in `vault_collapsed`. Re-clamp `vault_cursor` to remain on a visible match.
- `VaultClose`: `vault_open = false`. (View returns to whatever `view()` renders for the current file/mode — `vault_prev_mode` is informational; no file was unloaded, so nothing to restore beyond hiding the page.)

**Keybindings** (in the key-handler):
- `⌘⇧F` (`"f" | "F" if cmd && mods.shift()`) → `OpenVaultSearch` (already present; now opens the page).
- When `vault_open`: `Esc` → `VaultClose`, `ArrowUp/Down` → `VaultMove(∓1)`, `Enter` → `VaultOpenSelected`. These must be checked in a `vault_open` guard branch alongside the existing overlay/editor guards, and must take priority so arrows/Enter don't fall through to body scrolling. The query `text_input` keeps focus; arrows/Enter are handled at the app key layer (text_input doesn't consume them), matching how the overlay did it.

**view() dispatch** — at the top of the `reader` selection, before error/file checks:
```rust
let reader = if self.vault_open {
    vault_search_page(&self.vault_query, &self.vault_results, self.vault_cursor,
                      self.vault_truncated, &self.vault_collapsed,
                      self.workspace.as_deref(), pal)
} else if let Some(err) = &self.error { ... }
  else if self.file.is_none() { welcome_view(pal) }
  else { /* body */ };
```

**Render `vault_search_page`:**
- Column: query bar (input + `N matches in M files`) → divider → scrollable results → footer hint (`↑↓ move · ⏎ open · esc exit`).
- Query input: id `vault_input_id` (new), `on_input = VaultQueryChanged`, `on_submit = VaultOpenSelected`.
- Results: iterate hits, grouping consecutive same-path hits. Per file:
  - header button `▼ relpath` / `▶ relpath (N)` → `VaultToggleFile(path)`.
  - if expanded: per match, render its `context` lines as a block — each line is `{number:>4} │ {text}` in monospace, muted for context, normal for the match line, with the matched span (`col_start..col_end`) highlighted. The match block at `vault_cursor` gets a left-border + surface-alt background (cursor indicator). Whole block is a button → `VaultOpenHit(flattened_index)`.
- Empty query → "Type to search every file in the workspace". No matches → "No matches".
- Footer count: `{n} matches in {m} files`, or `200+ matches (refine query)` when truncated.

**Flattened match list:** the page and `VaultMove`/cursor operate on a flattened index over *visible* matches (those not under a collapsed file). A small helper `visible_matches(&hits, &collapsed) -> Vec<usize>` (indices into `hits`) is computed in the handler for clamping and in the view for cursor mapping. Kept in `app.rs` (display concern).

**Command palette / cheatsheet:** existing `Search All Files… ⌘⇧F` entry and the `⌘⇧F` cheatsheet line stay (behaviour now opens the page).

## Data flow

```
⌘⇧F → OpenVaultSearch → vault_open=true, blank query, focus input
  type → VaultQueryChanged(q) → vault_seq++ → run(files,q,seq) [150ms debounce]
    → VaultSearchDone(r) → if seq matches: store hits (with ±2 context), cursor=0
  ↑↓ → VaultMove → clamp over visible matches, scroll to cursor
  ▼/▶ click → VaultToggleFile → fold/unfold, re-clamp cursor
  ⏎ / click row → resolve hit → pending_nav.line; vault_open=false; Open(path)
    → FileLoaded → existing nav_task → Cmd::Goto{line} → scroll
  Esc → VaultClose → vault_open=false (back to prior view)
```

## Reuse

- `search::find_all` (case-insensitive substring), `ipc::lines::build_byte_to_line` — unchanged.
- `PendingNav.line` → `FileLoaded` → `Cmd::Goto` — exact-line landing, unchanged.
- `edge_scroll` helper for cursor-follow scroll.
- Page styling mirrors existing overlay/sidebar palette usage (`pal.surface_alt`, `pal.rule`, `pal.subtle`, `pal.fg`).

## Error handling

- Unreadable/non-UTF8 file: skipped in `run` (unchanged).
- Empty query: empty results, no scan.
- Stale result: dropped via `seq` mismatch.
- Cursor on a match that becomes hidden (file collapsed): re-clamped to nearest visible match in `VaultToggleFile`.
- Opening a hit whose file changed on disk: existing `apply_goto` range guard handles out-of-range lines.

## Testing

`src/vault_search.rs` unit tests (extend existing 10):
1. Existing line-number / multi-file / case-insensitive / cap / multibyte / empty-query tests adapted to the new `VaultHit` shape.
2. `context` contains the match line flagged `is_match=true` plus up to 2 lines each side, with correct 1-based numbers.
3. Match at file start (line 1) → context has 0 lines above, ≤2 below.
4. Match at file end → ≤2 above, 0 below.
5. `col_start..col_end` spans the matched substring within its line (char offsets, multibyte-safe).
6. Two matches on adjacent lines → each hit carries its own context window (overlap is allowed; not merged).

UI wiring (`app.rs`): build + clippy verified; page render + key flow smoke-tested manually (`⌘⇧F`, type, `↑↓`, `Enter`, `Esc`, click a header to fold) — no IPC surface for the page itself.

## Out of scope (YAGNI)

Case/whole-word/regex toggles, replace-in-files, glob/path filtering, selection-seeded query, persisting query/results across exits, merging overlapping context windows, syntax-highlighting the context lines.
