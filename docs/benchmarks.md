# rmdv benchmarks

Hardware: MacBook Pro (Apple M2), macOS 26.1
Build: `cargo build --release` at v0.2.0
Date: 2026-05-10

## Cold start

Measured by `rmdv --benchmark-startup`. Median of 5 runs. All deltas are from process entry.

| Checkpoint | Time |
|---|---|
| `pre_run` — argv parsed, window settings built, just before `iced::application().run_with()` | ~16 µs |
| `first_view` — first `App::view()` call (window can paint) | ~150 ms |
| `fonts_loaded` — system font scan complete (runs lazily after first paint) | ~157 ms |

`first_view < fonts_loaded` because v0.2 defers `load_system_fonts` to the first frame, so the window appears with bundled fonts (Inter, JetBrains Mono, Lucide) and the system-font fallback table is built lazily on the render thread.

## Parse + highlight

Criterion: `cargo bench --bench cold_start`.

| Workload | Median |
|---|---|
| `parse_10k_lines` (~1 MB synthetic markdown, parse only — highlight is lazy) | 8.1 ms |
| `font_system_load_system_fonts` (one-time, deferred to first view) | 7.7 ms |

Highlighting moved out of parser in v0.2 (`HlCache` LRU). Re-parses on hot reload reuse cached spans for unchanged code blocks. Before this change, parsing 10k lines took ~10.5 s because the parser invoked `tree-sitter::Query::new` per code block; isolating highlighting to a cached step dropped that ~99.9%.

## Virtual scrolling

The renderer is viewport-aware: only blocks intersecting the visible viewport (plus 5-block padding) are turned into widgets, with cumulative spacers above/below for elided regions. Memory and per-frame work stay roughly constant regardless of document length.

Block heights use a cheap estimator (line-count × line-height with a per-block-type lookup). Measured-height feedback from the rendered widgets is not yet wired in for v0.2 — the estimate is sufficient for paragraphs/code/tables but can drift on documents with large images or unusual wrap behaviour. Real measurement plumbing is a v0.3 follow-up; today's spacer math is estimate-only.

## Reproducing

    cargo bench --bench cold_start
    ./target/release/rmdv --benchmark-startup
