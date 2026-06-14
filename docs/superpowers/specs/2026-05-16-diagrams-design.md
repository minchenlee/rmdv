# Diagram Rendering (Mermaid + Graphviz/DOT) — Design

**Status**: Approved 2026-05-16
**Scope**: Render ` ```mermaid ` and ` ```dot ` / ` ```graphviz ` fenced blocks as inline SVG diagrams in rmdv. Pure-Rust pipeline, no JS runtime, no external CLIs.

## Goals

- Mermaid + Graphviz/DOT render inline matching rmdv's current theme.
- Zero external runtime dependencies (no Node, no Chromium, no Java).
- Lazy rendering — only visible diagrams cost CPU.
- Click-to-zoom and copy-source UX consistent with existing code-block and image affordances.
- Graceful fallback when a diagram fails to parse.

## Non-Goals (v1)

- PlantUML, D2, TikZ, wavedrom, svgbob, ditaa.
- Export diagram as PNG/SVG file.
- Syntax assist or linting inside the editor.
- Animated/live preview during typing inside an unclosed fence.
- Diagram count surfaced in mindmap navigation.

## Crate Choices

- `mermaid-rs-renderer` — pinned to `zed-industries/mermaid-rs-renderer` (Zed's actively maintained fork). MIT/Apache. Pure-Rust mermaid parser → SVG string. Claims 100–1400× faster than mermaid-cli (no Chromium).
- `layout-rs` — pure-Rust DOT layout engine → SVG. Covers core DOT graph features. Advanced GraphViz (HTML labels, complex shapes) intentionally out of scope; such diagrams fall back to the raw code block.
- Existing in-tree: `iced::widget::svg` (uses `resvg`/`usvg`), already used by inline images.

## Architecture

### AST

`src/ast.rs` — add:

```rust
pub enum DiagramKind { Mermaid, Dot }

pub enum Block {
    // …existing variants…
    Diagram {
        kind: DiagramKind,
        source: String,
        hash: u64,        // xxhash of source, used as cache key
    },
}
```

### Parser

`src/parser.rs` — in the existing `Tag::CodeBlock` arm (~line 195), inspect `lang`:

- `"mermaid"` → emit `Block::Diagram { kind: Mermaid, … }`.
- `"dot"` or `"graphviz"` → emit `Block::Diagram { kind: Dot, … }`.
- Everything else → unchanged `Block::CodeBlock`.

Source string is the raw fenced body. Hash computed with the same xxhash currently used at `parser.rs:50`.

### Diagram module

New `src/diagram.rs`:

```rust
pub enum DiagramKind { Mermaid, Dot }

pub enum DiagramState {
    Pending,
    Ready { handle: iced::widget::svg::Handle, source_bytes: bytes::Bytes },
    Err(String),
}

pub struct DiagramCache {
    map: lru::LruCache<(u64, u32), DiagramState>,  // key = (content_hash, theme_id)
}

pub fn render_blocking(
    kind: DiagramKind,
    source: &str,
    palette: &Palette,
    theme_id: u32,
    font_family: &str,
) -> Result<String, String>;  // returns SVG string
```

`render_blocking` wraps the inner work in `std::panic::catch_unwind` and maps panics to `Err("internal renderer error")`.

### App integration

`src/app.rs`:

- New field `diagram_cache: DiagramCache` (cap 64 entries).
- New `Message::DiagramRendered { hash: u64, theme_id: u32, result: Result<String, String> }`.
- On theme switch (`Message::ThemeCycle`, hot-reload), increment a `theme_id: u32` counter. Stale cache entries are not pruned eagerly — they age out via LRU; visible diagrams trigger fresh renders with the new id.
- On file change via watcher: cache survives (helps re-open). After a re-parse, if a previously-rendered hash no longer appears in the new AST, the entry simply stays in LRU until evicted.

### Render path

`src/render.rs` — add a `Block::Diagram` arm:

1. Look up `(hash, theme_id)` in `diagram_cache`.
2. If absent: insert `DiagramState::Pending`, dispatch `iced::Task::perform(render_blocking_async(…))` → `Message::DiagramRendered`. Render fallback (raw code block + "rendering" chip).
3. If `Pending`: render fallback (raw code block + "rendering" chip).
4. If `Ready { handle, .. }`: render `svg::viewer(handle)` wrapped in a `mouse_area` for click-zoom, with a hover overlay providing `[zoom][copy]` icons.
5. If `Err(msg)`: render the raw code block at normal opacity plus an "⚠ error" chip with a tooltip exposing `msg`.

### Lazy trigger

Visibility check uses the existing `src/virt.rs` viewport gating. Off-screen `Block::Diagram` entries are not added to the render tree, so the cache lookup (and thus the render task) only fires when the diagram scrolls into view.

## Theme Injection

### Mermaid

Built once per render, before invoking `mermaid-rs-renderer`:

```text
%%{init: {
  'theme': 'base',
  'themeVariables': {
    'background':         '<pal.bg>',
    'primaryColor':       '<pal.surface>',
    'primaryTextColor':   '<pal.text>',
    'primaryBorderColor': '<pal.border>',
    'lineColor':          '<pal.muted>',
    'secondaryColor':     '<pal.surface_alt>',
    'tertiaryColor':      '<pal.accent>',
    'fontFamily':         '<editor font name>'
  }
}}%%
<original source>
```

If the source already starts with a `%%{init` directive, injection is skipped — user override wins.

### DOT

Two-stage:

1. Prepend a default-attr block built from the palette:
   ```dot
   graph [bgcolor="transparent" fontname="<font>"];
   node  [fontcolor="<text>" color="<border>" fillcolor="<surface>"];
   edge  [color="<muted>"];
   ```
2. Post-process the SVG output: substitute any hardcoded literal colors `layout-rs` may emit (small fixed map of 3–4 colors) using string replace. Keeps the substitution narrow and predictable.

### Cache key

`(content_hash, theme_id)`. `theme_id: u32` increments on every theme change and on hot-reload from disk.

## UI & Interactions

### In-doc

```
┌─────────────────────────────────────┐
│        [rendered SVG]               │
│                                     │
│                            [⧉][⎘]   │   hover-visible: zoom + copy
└─────────────────────────────────────┘
```

- SVG sized to its `viewBox` aspect, clamped to body content width and to `MAX_HEIGHT = 600` px.
- Top-right hover icons use the existing `mouse_area(container(icon))` pattern from the code-block copy button. `Message::CopyDiagramSource(String)` reuses the existing toast machinery.
- Click body of the diagram → `Message::DiagramZoom(hash)`.

### Zoom modal

Reuses the inline-image viewer (per `rmdv_images` memory). Extend the viewer source enum with a `Diagram(Bytes)` variant. Pan/zoom/⌘0/Esc behave identically. `⌘C` in the modal copies the source.

### Pending fallback

Raw fenced code block, faded opacity (~0.45), small "rendering" chip bottom-right.

### Error fallback

Raw fenced code block, normal opacity, small "⚠ error" chip bottom-right; chip has a tooltip showing the parser/error message.

### Keyboard

No new bindings. Command palette gains nothing in v1 (palette only carries app-wide actions; "copy diagram source" depends on context and is reachable via hover icon).

## Edge Cases & Limits

- Empty / whitespace-only source → render as a plain code block, no render task dispatched.
- Source > 64 KB → reject, code-block fallback + "diagram too large" chip.
- Rendered SVG > 4 MB → reject, code-block fallback + chip.
- Duplicate diagrams (same content) → one cache entry, multiple displays share the handle.
- Diagrams inside list items / blockquotes → existing block walker already nests; no special case.
- User `%%{init}%%` prelude → theme injection skipped.
- DOT with unsupported attrs (HTML labels, complex shapes) → `layout-rs` returns error → chip fallback.
- Watcher reload while a render task is in flight → on `DiagramRendered`, verify hash still exists in current AST before inserting; otherwise drop the result.

## Testing

1. **Parser unit** (`tests/parser_diagrams.rs`): fixture `tests/fixtures/diagrams.md` with mermaid + dot + a non-diagram code block → AST contains correct `Block::Diagram` and `Block::CodeBlock` entries with the right `DiagramKind`.
2. **Renderer unit** (`tests/diagram_render.rs`): canonical mermaid + dot fixtures → SVG output starts with `<svg`; broken inputs → `Err`.
3. **Theme injection unit**: given a fixed palette + source → exact expected prelude string.
4. **Integration**: load a doc with 3 mermaid + 2 dot + 1 broken mermaid → cache populates, broken one yields `Err`, theme cycle invalidates and re-renders.
5. **Manual smoke**: theme cycle mid-render, click-zoom, copy source, edit-mode live update, watcher reload, large diagram clamp.

## Performance Budget

- Mermaid simple flowchart: <100 ms (per upstream claim, validated on fixtures).
- DOT ~30 nodes: <50 ms.
- Cache hit: ~0 ms beyond `svg::viewer` widget cost.
- Off-screen diagrams: 0 ms (lazy via `virt.rs`).

## Files Touched

- `Cargo.toml` — add `mermaid-rs-renderer` (git rev), `layout-rs`, `lru`.
- `src/ast.rs` — add `Block::Diagram`, `DiagramKind`.
- `src/parser.rs` — route mermaid/dot lang fences to `Block::Diagram`.
- `src/diagram.rs` *(new)* — `DiagramCache`, `render_blocking`, theme injection.
- `src/render.rs` — `Block::Diagram` arm with cache lookup + dispatch + fallback.
- `src/app.rs` — `diagram_cache`, `Message::DiagramRendered`, `Message::DiagramZoom`, `Message::CopyDiagramSource`; extend image viewer with `Diagram(Bytes)` source.
- `src/virt.rs` — verify height estimation for `Block::Diagram` (use cached SVG height or a fixed estimate when pending).
- `src/lib.rs` — `pub mod diagram;`.
- `tests/fixtures/diagrams.md` *(new)*.
- `tests/parser_diagrams.rs` *(new)*.
- `tests/diagram_render.rs` *(new)*.

## Open Questions

None at design time. Implementation-time discoveries (e.g. `layout-rs` not exposing the color attrs cleanly) are tracked in the implementation plan.
