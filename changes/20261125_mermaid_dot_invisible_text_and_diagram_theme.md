# Mermaid/DOT diagram text invisible on Windows, and `[diagram]` theme colors

Date: 2026-11-25
Area: `src/diagram.rs`, `src/theme.rs`, `src/theme_load.rs`, `src/app/mod.rs`,
`src/app/full_mindmap.rs`
Status: uncommitted working-tree change (not merged, not released)

## Symptom

`cargo run tests\fixtures\diagrams.md` showed every Mermaid flowchart as empty
boxes: node and edge shapes drew, but **no label text at all**. Sequence
diagrams, sequence notes, and Graphviz/DOT graphs were affected the same way.
Text was also reported as an arbitrary system color rather than the theme
foreground.

## Root cause

Two independent defects were stacked; fixing either alone leaves visible bugs.

### 1. Font resolution failure — the reason text was invisible

`src/app/mod.rs` passes `font_family = "JetBrains Mono"` to the diagram
renderer so diagrams match the editor font. That face is **bundled** into the
binary and registered with Iced at startup (`src/fonts.rs`), but the resvg/usvg
raster font database in `src/diagram.rs` only called `load_system_fonts()` —
where `JetBrains Mono` does not exist.

`fontdb` treats an unknown family as "no face at all":

1. `Database::query()` returns `None` for the requested family.
2. usvg's default selector then retries its single last-resort family,
   `Family::Serif`.
3. The previous code pointed the generic families at macOS names — `Helvetica`,
   `Times`, `Menlo`. **On Windows none of the three exist either**, so the
   last-resort query also returned `None`.
4. usvg logs `No match for '…' font-family.` and emits **no glyphs**, while the
   shape nodes still paint. Result: boxes with no text.

This is why the bug was Windows-only and why colors looked wrong: the reporter
was seeing the absence of themed glyphs, not a mis-colored glyph.

Measured on Windows against the real app output with the real font: **0**
foreground-colored pixels. Against a resolvable family: hundreds to thousands,
at exactly the palette `fg`.

Root-cause fix for text (macOS unaffected, as it already resolved through its
system faces):

```rust
// src/diagram.rs
static USVG_FONTDB: LazyLock<Arc<resvg::usvg::fontdb::Database>> = LazyLock::new(|| {
    let mut db = resvg::usvg::fontdb::Database::new();
    db.load_system_fonts();

    #[cfg(windows)]
    {
        for bytes in crate::fonts::BUNDLED {
            let source: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(BundledFont(bytes));
            db.load_font_source(resvg::usvg::fontdb::Source::Binary(source));
        }
        db.set_sans_serif_family("Inter");
        db.set_serif_family("Times New Roman");
        db.set_monospace_family("JetBrains Mono");
    }

    #[cfg(not(windows))]
    {
        db.set_sans_serif_family("Helvetica");
        db.set_serif_family("Times");
        db.set_monospace_family("Menlo");
    }
    Arc::new(db)
});
```

`BundledFont` borrows the `'static` bundle bytes, so the ~6 MB font payload is
shared rather than copied into the database. Registering all bundled faces costs
~164 µs, once, lazily on the first diagram render.

### 2. DOT label color — black text on a dark theme

`layout-rs`'s `SVGWriter` reads `color`/`fillcolor` from the DOT attributes but
**ignores `fontcolor`**, and emits `<text>` elements with no `fill` at all. Labels
therefore painted SVG default black, which is invisible on the dark palette. It
also hardcodes `font-family: Times, serif` in every generated class rule.

Fix: post-process the generated SVG (already anticipated by
`docs/superpowers/specs/2026-05-16-diagrams-design.md`) to inject a
`text, textPath { fill: … }` rule. On Windows only, the hardcoded
`Times, serif` is additionally rewritten to the app font — on macOS that serif
already resolves and must be left alone.

### 3. Mermaid defaults leaking into themed diagrams

Mermaid derives several colors from `primaryColor`/`tertiaryColor` instead of
using the theme:

- `clusterBkg`/`clusterBorder` derive from `tertiaryColor`, rendering subgraph
  panels as a saturated hue that ignores the theme.
- `noteBkgColor` is a **hardcoded** `#fff5ad` light yellow, never derived at all,
  so sequence notes stayed yellow on a dark theme.

Fix: `mermaid_init_block` now pins the full variable set (`clusterBkg`,
`clusterBorder`, `edgeLabelBackground`, `noteBkgColor`, `noteBorderColor`,
`noteTextColor`, `actor*`, `signal*`, `label*`, `loopTextColor`,
`activation*`, `titleColor`, `nodeTextColor`, `mainBkg`, `nodeBorder`,
`textColor`).

## New feature: `[diagram]` in custom theme files

Previously diagram colors could only be changed indirectly, by editing palette
fields such as `[ui] bg`, or per document with a leading
`%%{init: {'themeVariables': {…}}}%%` block. There was no way to set diagram
text, block, or line colors in a style file.

`~/.config/rmdv/themes/*.toml` now accepts an optional `[diagram]` table. Every
key is optional; an absent key keeps deriving that role from `[ui]`.

```toml
[diagram]
text = "#dcdfe4"          # node/actor/note labels, DOT label fill
node_fill = "#21252b"     # flowchart node + actor bodies, DOT fillcolor
node_border = "#3a3f4b"   # node outlines, DOT node color
line = "#969ca7"          # edges, arrows, lifelines, DOT edge color
background = "#282c34"    # canvas behind the diagram
cluster_fill = "#2f343d"  # subgraph panels
cluster_border = "#3a3f4b"
note_fill = "#2f343d"     # sequence/state notes
note_border = "#3a3f4b"
note_text = "#dcdfe4"
```

Design notes:

- `DiagramTheme` (`src/theme.rs`) is a **separate optional struct**, not new
  fields on `Palette`. `Palette` is constructed in nine preset constants plus
  the theme importers; widening it would touch all of them and would wrongly
  model diagram colors as palette state.
- A new `ResolvedDiagram` in `src/diagram.rs` applies the overrides over the
  palette-derived defaults in **one** place, so Mermaid and DOT cannot drift and
  "what color is a node" has a single answer.
- Translucent palette entries are still composited onto the right backdrop
  before encoding (Mermaid derives further colors from these hex values, and
  `layout-rs` discards alpha entirely). `dot_preamble` previously encoded
  `code_border` without compositing, turning a faint hairline into an opaque
  white/black node ring; that is fixed too.
- A user-authored `%%{init}%%` block still wins; overrides only feed the
  injection, exactly as before (`has_user_init`).

### Plumbing

- `render_blocking_with` / `render_blocking_async_with` take a
  `&DiagramTheme`. The old `render_blocking` / `render_blocking_async`
  signatures are kept as thin wrappers passing `DiagramTheme::NONE`, so existing
  callers and tests are unaffected.
- `App` tracks `diagram_theme`, refreshed in `apply_palette` (the single funnel
  every theme change already goes through) via `active_diagram_theme()`.
- `theme_id_with` folds the overrides into the cache key, so editing `[diagram]`
  invalidates cached renders. Without this, a theme-file edit would keep showing
  the previous raster.
- Both render dispatchers (`prime_diagram_cache`, Full Mindmap preview) pass the
  active overrides through.

## Tests added

`src/diagram.rs`:

- `every_diagram_kind_paints_text_in_the_palette_foreground` — rasterizes
  flowchart, sequence, and DOT output in both palettes and asserts real
  foreground-colored glyph pixels exist. **Verified to fail with 0 px when the
  font fix is reverted**, so it genuinely guards the reported bug.
- `dot_labels_are_themed` (all platforms), plus
  `dot_labels_use_the_requested_font_on_windows` /
  `dot_labels_keep_the_system_serif_off_windows` — assert each platform's
  intended DOT font behavior rather than one shared expectation.
- `dot_node_borders_composite_their_alpha`.
- `theme_id_changes_with_diagram_overrides`.
- `diagram_overrides_reach_mermaid_and_dot`.
- `diagram_overrides_do_not_touch_a_user_init_block`.

`src/theme_load.rs`:

- `parses_the_diagram_section` — partial table overrides only the named roles and
  leaves the palette untouched.
- `a_theme_without_a_diagram_table_derives_everything`.
- `a_bad_diagram_color_is_reported`.

## Verification

| Check | Result |
| --- | --- |
| `cargo check --lib` | pass |
| `cargo check --no-default-features` | pass |
| `cargo fmt --all --check` | pass |
| `git diff --check` | pass |
| `cargo test --lib diagram` | 31 passed |
| `cargo test --lib theme_load` | 6 passed |
| `cargo test --test diagram_render` | 8 passed |
| `cargo test --test parser_diagrams` | 10 passed |
| `cargo test --lib` (full) | 1002 passed, 16 failed |

The 16 full-suite failures are **pre-existing and unrelated**: verified to fail
identically with these changes stashed. They are macOS/tmux/environment
assertions (`terminal::*`, `pointer_tests::*`, `fonts::tests::inter_weights_*`,
`prefs::tests::*`, `quick_slots::*`) that do not hold on this Windows host.
`tests/tex_snapshots.rs::parses_academic_paper_subset` similarly fails without
this change (math snapshot drift; `tex.rs` is untouched).

Manually confirmed by rasterizing real renderer output in both palettes:
flowcharts (incl. subgraphs and edge labels), sequence diagrams (incl. notes),
and DOT all paint their labels in the palette foreground, with subgraph and note
panels in theme colors rather than Mermaid's orange/yellow defaults.

## Behavior change summary

- **Windows:** diagram labels now render, in the app's bundled font and the theme
  foreground. DOT labels use the app font.
- **macOS:** unchanged. The original system-font path and the hardcoded
  `Times, serif` DOT family are intentionally preserved.
- **All platforms:** Mermaid subgraph and note colors follow the theme; DOT label
  fills follow the theme instead of painting black; custom themes may set
  `[diagram]` colors.

## Not done / follow-ups

- No `[diagram]` support in the theme importer output (`write_theme_toml`); it
  round-trips only what it already emits. Imported themes inherit derived
  diagram colors.
- Diagram font is still hardcoded to `"JetBrains Mono"` at the two call sites;
  it is not yet theme-configurable.
- macOS could later adopt the bundled-font path for exact cross-platform
  parity, but that is deliberately out of scope here.
