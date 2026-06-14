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

// Path from the document root to a node's value: object keys and array
// indices. Used by the leaf panel to re-navigate the parsed value on demand.
pub enum PathSeg { Key(String), Index(usize) }

// Build the flat arena tree the renderer wants, plus a per-node path map so
// the leaf panel can show each node's subtree.
pub fn build_tree(
    root: &DataValue,
    doc_title: &str,
    collapsed: &HashSet<BlockId>,
) -> (Vec<MNode>, HashMap<BlockId, Vec<PathSeg>>);

// tree + layout + path map, mirroring mindmap::build_layout's contract.
pub fn build_layout(
    source: &str,
    lang: &str,                     // "json" | "yaml"
    file: Option<&Path>,
    collapsed: &HashSet<BlockId>,
) -> (Vec<MNode>, Size, HashMap<BlockId, Vec<PathSeg>>);

// Leaf panel: re-parse source, walk `path`, pretty-print that subtree.
// Returns (pretty_string, lang) for a syntax-highlighted code block.
pub fn subtree_pretty(source: &str, lang: &str, path: &[PathSeg]) -> Option<String>;
```

`build_layout` parses `source` with serde, normalizes to `DataValue`, calls
`build_tree`, then reuses `mindmap::layout(...)` **unchanged** for x/y assignment
and canvas `Size`. On parse error it builds the fallback tree (root + ⚠ child)
with an empty path map.

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
  helper markdown uses (`MNode.truncated` flag drives the tooltip).
- While walking, record each node's `Vec<PathSeg>` into the path map keyed by its
  minted `BlockId`.

### Leaf panel — pretty-printed subtree

Clicking a node shows the JSON/YAML **under** that node, pretty-printed and
syntax-highlighted. The markdown panel renders an AST slice keyed by heading
BlockId (`mindmap_panel_range` / `mindmap_panel_view`, `app.rs:1287`/`1317`) —
data docs have no heading AST, so that path returns "Heading not found".

Data-doc panel instead: look up the selected node's `Vec<PathSeg>` in the path
map, call `subtree_pretty(&self.source, lang, path)` which re-parses, navigates
the path, and `serde_json::to_string_pretty` / `serde_yaml::to_string` of that
subtree. Scalars pretty-print to themselves.

The pretty string is rendered through the **existing** `render::data_view(code,
&[], pal, typ)` (`render.rs:150`) — it ignores the `spans` arg and colorizes
internally via `detect_data_lang` + `colorize_data`, so no highlight call is
needed. `data_view` returns `Element<'a>` borrowing `code`, and the pretty string
is computed fresh, so it must outlive the Element. Store it in a new
`RefCell<Option<(BlockId, String)>>` field `mindmap_data_panel: RefCell<...>` on
`App`: `mindmap_panel_view` checks if the cached `(id, _)` matches
`mindmap_panel_shown`; if not, recompute `subtree_pretty` and replace. The
borrowed `&str` then lives in the RefCell for the Element's lifetime. This mirrors
the existing `mindmap_layout: RefCell<...>` cache pattern and the settle-gen lag,
so `subtree_pretty` runs at most once per selection change. Consistent with
Option A (no persisted parsed `DataValue`; re-parse on demand, cache only the
rendered string). Invalidated alongside `invalidate_mindmap_layout` on
file-load/edit.

`mindmap_panel_view` (`app.rs:1317`) gains a top branch: `if self.is_data_doc`,
render the subtree code block from the RefCell; else the existing AST-slice path.

### Stable node identity

Mind map navigation and collapse key on `BlockId`. The converter **mints
sequential `BlockId`s during the DFS walk** (deterministic by position), so
`mindmap_collapsed` and `mindmap_selected` survive a re-layout. Self-contained —
no markdown AST is involved. IDs are stable across rebuilds because the walk order
is fixed by source key order (objects preserve order) and array index.

## Wiring (in `app.rs`)

- **Layout cache field (`app.rs:499`)** — currently
  `RefCell<Option<(Arc<Vec<MNode>>, Size)>>`. Add the path map:
  `RefCell<Option<(Arc<Vec<MNode>>, Size, Arc<HashMap<BlockId, Vec<PathSeg>>>)>>`.
  Markdown path supplies an empty map. This is the one struct-field change.
- **`App::mindmap_layout()` (`app.rs:1059`)** — branch at the top, return the map
  too:
  ```rust
  let (nodes, size, paths) = if self.is_data_doc {
      let lang = data_lang_for(self.file.as_deref()).unwrap_or("json");
      data_mindmap::build_layout(&self.source, lang, self.file.as_deref(), &self.mindmap_collapsed)
  } else {
      let (n, s) = mindmap::build_layout(&self.ast, self.file.as_deref(), &self.mindmap_collapsed);
      (n, s, HashMap::new())
  };
  ```
  Raw file text lives in `App.source: String` (`app.rs:394`), retained for the
  lifetime of the open doc — verified. Callers of `mindmap_layout()` that only
  want `(nodes, size)` get a 2-tuple accessor; a separate `mindmap_paths()`
  borrows the map (or `mindmap_layout` returns the triple and the two existing
  callers — `mindmap_focus_first_child` `app.rs:1084`, `view()` `app.rs:3472` —
  ignore the third element).
- **`mindmap_panel_view()` (`app.rs:1317`)** — top branch: `if self.is_data_doc`,
  look up `mindmap_panel_shown` in the path map, call
  `data_mindmap::subtree_pretty`, render via `render::data_view`; else the existing
  AST-slice path.
- **`is_data_doc` / `data_lang_for()`** — already set on file load. No change.
- **Cache invalidation** (`invalidate_mindmap_layout`) — reused unchanged; toggling
  files or editing already clears it.
- **Rendered (non-mindmap) view** for data docs — untouched; mindmap branch is
  checked first in `view()`.

## Reused untouched

Canvas `draw`, zoom/pan, ←↑→↓ nav, collapse toggle, `MNode` / `MindmapState` /
`MindmapProgram`, `mindmap::layout()`, and `render::data_view` + `hl_cache` for the
panel code block.

## Components & boundaries

| Unit | Does | Depends on |
|---|---|---|
| `data_mindmap::from_json` / `from_yaml` | parser `Value` → `DataValue` | serde_json, serde_yaml |
| `data_mindmap::build_tree` | `DataValue` → `Vec<MNode>` + minted BlockIds + path map | mindmap::MNode, BlockId |
| `data_mindmap::build_layout` | parse + tree + layout + fallback + map | mindmap::layout |
| `data_mindmap::subtree_pretty` | source + path → pretty subtree string | serde_json/yaml |
| `App::mindmap_layout` (edit) | dispatch md vs data, carry map | data_mindmap, is_data_doc |
| `App::mindmap_panel_view` (edit) | data branch renders subtree code block | subtree_pretty, render::data_view |

Each unit testable in isolation: `build_tree` is pure (`DataValue` in, tree+map
out); `build_layout`/`subtree_pretty` are pure (`&str` in, value out).

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
9. Path map → each leaf's `Vec<PathSeg>` round-trips: `subtree_pretty(source, lang,
   path)` returns that node's value (scalar → scalar text; object key → its object).
10. `subtree_pretty` on a parent key returns the pretty-printed object/array;
    on a scalar returns the scalar; on a stale/invalid path returns `None`.
11. `lang = "toml"` (or any non-json/yaml) → fallback tree, no panic.

Then per the project verify-UI rule (`.claude/rules/ui-verification.md`):
release build, open a real `.json` and a real `.yaml` over the rmdv IPC/CLI,
⌘M, screenshot, and LOOK at it — node labels, no cropping, arrow-nav follows
focus, and clicking a parent node shows its pretty-printed subtree in the panel.

## Scope cuts (YAGNI)

- No TOML mind map (request was JSON + YAML). `.toml` is `is_data_doc`, so it
  enters the data branch, but `build_layout` only parses `lang ∈ {json, yaml}`;
  any other lang yields the fallback tree (root + `⚠ <lang> mindmap not supported`).
  No crash, explicit message. Rendered (non-mindmap) TOML view is unchanged.
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
