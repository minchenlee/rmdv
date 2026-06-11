# mdv performance log

Every performance optimization shipped, newest first, with measured numbers and
commit refs. Point-in-time baselines live in [benchmarks.md](benchmarks.md).
Hardware for all numbers: MacBook Pro (Apple M2), macOS 26.x, `--release` build.

Status note: commits from 2026-06-10 onward are on `perf/render-path`, not yet
merged to `main`.

## Memory

### Arrow-spam footprint spike — `ab1c5de` (2026-06-11)
Rapid mindmap arrow-navigation spiked physical footprint 170MB → 500–650MB,
draining only after ~60s idle. Instrumentation (malloc_history at high-water
mark) showed live heap peaked at just 142MB while cumulative alloc/free traffic
hit 10.9GB in 3 minutes: the gap was freed multi-MB render allocations parked
in libmalloc's large-entry death-row cache as empty MALLOC_LARGE regions —
reclaim lag, not a leak. Fix: launch the GUI with `MallocLargeCache=0` (set via
a one-time self re-exec in `launch_instance`, since libmalloc reads it before
`main`). A churn testcase retaining 66.5MB of empty pages drops to 2.7MB with
the knob. Startup unchanged (2.303s vs 2.306s A/B on a 920KB doc).

Two approaches were tested and rejected on evidence: an iced_wgpu fork adding
`device.poll` after submit (wgpu-core 27 already maintains the device inside
every `queue.submit` — zero measured delta), and
`malloc_zone_pressure_relief()` (no-op on macOS 26's xzone allocator).

### Mindmap preview-panel debounce — `d1ee5a5` (2026-06-11)
During arrow-key bursts the right preview panel rebuilt its rendered markdown
on every keypress. Panel rebuild now lags selection by 75ms (generation
counter; clicks and panel-entry stay immediate), coalescing rebuilds during
key-repeat (30ms repeat rate) into one.

### Mindmap viewport culling — `ad6246f` (2026-06-10)
Node rects/labels were viewport-culled but edge béziers and collapsed-node
dots were not: all ~900 edges of a big doc tessellated every frame, and
iced_wgpu vertex buffers never shrink, so the high-water mark stuck. Edges now
cull via endpoint-AABB (conservative-exact: control points lie inside the
box), dots share the node `visible()` test. Big-doc mindmap settled
177.9MB → 126.8MB, peak 206MB → 142.8MB; GPU mesh buffers 23.8MB → 2.4MB.
Pixel-identical output (proved by geometry argument + adversarial review).

### Virtual scrolling, redesigned — `33cd247` (2026-06-10)
10k-line doc: **452.9MB → 69.8MB**. Fold-aware display list with height prefix
sums, rendered range + hysteresis band rebuilt only in `update()`; docs ≤256
display blocks stay full-render. Required a custom `KeyedBody` widget because
`iced::widget::keyed::Column` cannot diff a sliding key window (panics; see
commit + `docs/benchmarks.md` history). The v0.2 virtual scroll (`463f4f7`,
2026-05-10) had been disabled for smoothness issues (`f8be880`); this is the
re-enable with measured heights and stable widget state.

### Byte-budgeted caches + delta undo — `325bc45` (2026-06-10)
Diagram and image caches were entry-count LRUs, so a handful of huge rasters
could pin hundreds of MB; both now evict by byte budget. Editor undo stack
switched from full-document snapshots to deltas — undo memory on large docs
drops from O(doc × edits) to O(edits).

## Mindmap interaction

### Layout cache — `5a2eb9a` (2026-06-10)
`build_layout` (full AST walk + label fitting + recursive layout) ran on every
redraw, every arrow key, and every focus change. Now cached in `App` as
`RefCell<Option<(Arc<Vec<MNode>>, Size)>>`, invalidated only when AST, file,
or collapsed-set changes; `view()` pays a refcount bump instead of an O(n)
rebuild.

### Draw batching + projection (initial mindmap ship, `9ea0fc7`, 2026-05-16)
Two render-killers avoided by design: `frame.scale(z)` + `fill_text` makes
iced_wgpu re-rasterize all glyphs every zoom step — replaced with manual
screen-space projection and constant font size; and per-node `Path` allocation
— replaced with one batched Path per style category (edges, fills, borders,
dots). These are invariants: do not reintroduce.

## Hot paths (zero behavior change) — `92d2ccb` (2026-06-10)
- mindmap `draw()`: five full node-list passes merged into one
- render: search query lowercased once per block, not per span
- search: needle lowered once per query, no discarded offset Vec
- sidebar tree: one flatten per keystroke instead of two
- overlays: reuse the just-computed filtered-list length
- vault page: distinct-file count stored at search-done, not per frame

## Parsing & highlighting

### AST reuse for outline — `dd1eb3b` (2026-06-10)
File open/save/reparse ran the parser and byte-to-line table twice (once for
AST, once for outline sections). Sections now derive from the already-parsed
AST.

### Vault search (⌘⇧F) — `32255a4` (2026-06-02)
Compiled tree-sitter `Query` cached as `Arc<Query>` per language (~30µs/call,
helps all highlighting). Only the match line is highlighted, not context lines
(~6× fewer spans/frame). Files read + scanned concurrently (buffered 16) with
a global result cap.

### Highlight cache — `f0dd9e4`, `1e5dc48`, `a795831` (2026-05-10)
The v0.2 headline: parsing 10k lines took **~10.5s** because the parser
compiled a tree-sitter query per code block. Highlighting moved to a shared
LRU (`HlCache`, capped 64 entries) keyed so hot-reload reparses reuse spans
for unchanged blocks; `Language` cached per name. Parse of the same doc:
**8.1ms** (~99.9% drop).

## Diagrams (Mermaid + DOT) — `3e2447b`, `943a78b`, `75ddd37` (2026-05-16)
Rendered SVGs rasterized off-thread with a render concurrency cap; LRU cache
(now byte-budgeted, see `325bc45`); cache-key collision fix and hot-path clone
removal; short-circuit when priming an already-cached diagram.

## Startup — `55e59b4`, `dc9880f` (2026-05-10)
System font scan (~7.7ms but on the critical path) deferred to first view —
window paints with bundled fonts, fallback table builds lazily after first
frame. `--benchmark-startup` flag records checkpoint timings (`pre_run`
~16µs, `first_view` ~150ms on a small doc). Criterion bench: `cargo bench
--bench cold_start`.

## Measuring

```
cargo bench --bench cold_start          # parse + font-load microbenches
./target/release/mdv --benchmark-startup
vmmap --summary <pid> | grep "Physical footprint"
```

Footprint measurements use isolated instances (`TMPDIR=<dir>` separates the
IPC socket) so a running personal instance isn't disturbed. Caveat learned the
hard way: footprint ≠ live heap — freed pages linger in malloc caches (see
`ab1c5de`), so always cross-check a "leak" with `malloc_history`/`heap`
before attributing.
