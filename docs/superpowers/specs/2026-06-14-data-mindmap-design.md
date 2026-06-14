# Mind map view for JSON & YAML files

**Date:** 2026-06-14
**Branch:** `feat/data-mindmap`
**Status:** Design — approved decisions, pending spec review

## Goal

Pressing ⌘M on a `.json`, `.yaml`, or `.yml` file renders a structural mind map of
the data — the key/value hierarchy — reusing the existing mind map canvas, zoom,
pan, arrow-key navigation, collapse, and leaf-detail panel. No new view mode, no
new keyboard shortcut.

Today `.json`/`.yaml` files can technically enter mind map mode but show only the
filename root node, because the mind map tree is built exclusively from
`Block::Heading` AST nodes and data docs are synthesized as a single
`Block::CodeBlock` (`app.rs:synthesize_data_ast`). This feature gives them a real
tree.

## Decisions (locked)

| Question | Decision |
|---|---|
| Leaf labels | `key: value` inline. Scalar leaf → one node `version: "0.2.2"`. Object/array → parent node labeled by key, recurse. |
| Arrays | Indexed children: array → parent; each element → child `[0]`, `[1]`, … Scalar element shown as `[i]: value`; object/array element as `[i]` parent. |
| Toggle | Same ⌘M. Dispatch on `is_data_doc`: data doc → data tree, else heading tree. Same canvas/nav/zoom. |
| Parse failure | Graceful fallback: root node (filename) + one child `⚠ invalid <lang>`. View never refuses. |
| Parsed-data storage | **Option A** — parse fresh from `self.source` when the mind map layout is (re)built; do not persist a parsed value on `App`. Result is cached by the existing `mindmap_layout` cache, so parse happens on first ⌘M only, not per frame. |
| YAML crate | Keep `serde_yaml = "0.9"`. Deprecated upstream but zero CVEs, read-only local parsing. The popular fork `serde_yml` is unsound/unmaintained (RUSTSEC) — avoid. Future swap to `noyalib` (`compat-serde-yaml`) is a separate task, noted in memory. |

## Architecture

The mind map renderer consumes a flat `Vec<MNode>` (arena tree, child-index refs).
Markdown builds that from `Block::Heading` via `mindmap::build_tree`. JSON/YAML get
a **dedicated converter that builds `Vec<MNode>` directly** — not faked through
`Block::Heading`, because that hack loses scalar values and array index semantics.

### New module: `src/data_mindmap.rs`

```rust
// One internal value enum so a single walker serves both JSON and YAML.
enum DataValue {
    Scalar(String),                 // pre-stringified (string/number/bool/null)
    Array(Vec<DataValue>),
    Object(Vec<(String, DataValue)>), // ordered: preserve source key order
}

// Normalize from each parser into DataValue.
fn from_json(v: &serde_json::Value) -> DataValue;
fn from_yaml(v: &serde_yaml::Value) -> DataValue;

// Build the flat arena tree the renderer wants.
pub fn build_tree(
    root: &DataValue,
    doc_title: &str,
    collapsed: &HashSet<BlockId>,
) -> Vec<MNode>;

// tree + layout, mirroring mindmap::build_layout's contract.
pub fn build_layout(
    source: &str,
    lang: &str,                     // "json" | "yaml"
    file: Option<&Path>,
    collapsed: &HashSet<BlockId>,
) -> (Vec<MNode>, Size);
```

`build_layout` parses `source` with serde, normalizes to `DataValue`, calls
`build_tree`, then reuses `mindmap::layout(...)` **unchanged** for x/y assignment
and canvas `Size`. On parse error it builds the fallback tree (root + ⚠ child).

### Tree-build rules

- **Root** → node labeled with the filename (matches markdown doc-title root),
  `level: 0`.
- **Object** → for each `(key, value)`:
  - value is `Scalar` → leaf node, `label = "key: value"`.
  - value is `Object`/`Array` → parent node `label = key`, recurse into children.
- **Array** → parent node (labeled by its key, or `[i]` if nested in an array):
  - element is `Scalar` → child node `label = "[i]: value"`.
  - element is `Object`/`Array` → child node `label = "[i]"`, recurse.
- `full_label` keeps the untruncated `key: value`; `label` truncated via the same
  helper markdown uses (`MNode.truncated` flag drives the tooltip). Leaf-detail
  panel shows `full_label` — free, the panel already keys off the selected node.

### Stable node identity

Mind map navigation and collapse key on `BlockId`. The converter **mints
sequential `BlockId`s during the DFS walk** (deterministic by position), so
`mindmap_collapsed` and `mindmap_selected` survive a re-layout. Self-contained —
no markdown AST is involved. IDs are stable across rebuilds because the walk order
is fixed by source key order (objects preserve order) and array index.

## Wiring (in `app.rs`)

- **`App::mindmap_layout()` (`app.rs:1059`)** — branch at the top:
  ```rust
  if self.is_data_doc {
      let lang = data_lang_for(self.file.as_deref()).unwrap_or("json");
      data_mindmap::build_layout(&self.source, lang, self.file.as_deref(), &self.mindmap_collapsed)
  } else {
      mindmap::build_layout(&self.ast, self.file.as_deref(), &self.mindmap_collapsed)
  }
  ```
  Raw file text lives in `App.source: String` (`app.rs:394`), retained for the
  lifetime of the open doc — verified.
- **`is_data_doc` / `data_lang_for()`** — already set on file load. No change.
- **Cache invalidation** (`invalidate_mindmap_layout`) — reused unchanged; toggling
  files or editing already clears it.
- **Rendered (non-mindmap) view** for data docs — untouched; mindmap branch is
  checked first in `view()`.

## Reused untouched

Canvas `draw`, zoom/pan, ←↑→↓ nav, collapse toggle, leaf-detail panel,
`MNode` / `MindmapState` / `MindmapProgram`, and `mindmap::layout()`.

## Components & boundaries

| Unit | Does | Depends on |
|---|---|---|
| `data_mindmap::from_json` / `from_yaml` | parser `Value` → `DataValue` | serde_json, serde_yaml |
| `data_mindmap::build_tree` | `DataValue` → `Vec<MNode>` + minted BlockIds | mindmap::MNode, BlockId |
| `data_mindmap::build_layout` | parse + tree + layout + fallback | mindmap::layout |
| `App::mindmap_layout` (edit) | dispatch md vs data | data_mindmap, is_data_doc |

Each unit testable in isolation: `build_tree` is pure (`DataValue` in, `Vec<MNode>`
out); `build_layout` is pure (`&str` in, tree+size out).

## Error handling

- Malformed JSON/YAML → fallback tree (root + `⚠ invalid json` / `⚠ invalid yaml`).
  No panic, no empty canvas.
- Empty document (`{}`, `[]`, empty string) → root node only, no children.
- Non-UTF8 / huge files → bounded by existing file-load limits; converter does not
  add its own size cap (config files are small; deep recursion is the only risk —
  guarded by a depth cap, see below).
- **Depth guard:** cap recursion depth (e.g. 64) to avoid stack blow-up on
  pathological nesting; beyond the cap, emit a single `…` child and stop.

## Testing

Unit tests on `build_tree` / `build_layout` (pure functions, no GUI):

1. Nested object → correct parent/child indices, `key: value` leaf labels.
2. Array of scalars → `[0]: …`, `[1]: …` children under array parent.
3. Array of objects → `[0]`, `[1]` parents, each recursing.
4. Mixed object+array nesting → structure preserved, key order preserved.
5. Empty `{}` / `[]` / `""` → root only.
6. Malformed JSON and malformed YAML → fallback tree with ⚠ child.
7. Deep nesting past the depth cap → `…` truncation child present.
8. BlockId stability → same input twice yields identical id sequence.

Then per the project verify-UI rule (`.claude/rules/ui-verification.md`):
release build, open a real `.json` and a real `.yaml` over the rmdv IPC/CLI,
⌘M, screenshot, and LOOK at it — node labels, no cropping, arrow-nav follows
focus, leaf panel shows full value.

## Scope cuts (YAGNI)

- No TOML mind map (request was JSON + YAML; `data_lang_for` returns `"toml"`
  but the dispatch will treat unknown data langs as JSON-parse-then-fallback, so
  TOML simply shows the fallback — acceptable, not in scope to render).
- No "flatten short scalar arrays" (indexed children chosen).
- No value-type icons / color coding.
- No serde_yaml → noyalib migration (separate task).

## Resolved during design

- **Raw source field** — `App.source: String` (`app.rs:394`) holds the raw file
  text and persists for the open doc. Converter reads it directly. No re-read.
- **BlockId** — `struct BlockId(pub u64)` (`ast.rs:4`), a plain newtype. Minting =
  a sequential `u64` counter advanced during the DFS walk. Data docs carry only a
  synthetic single-`CodeBlock` AST, so the data-mindmap id space is private and
  cannot collide with real heading ids.
