# iced_math — Native LaTeX Math Widget for Iced

**Status:** Design approved 2026-05-23
**Scope:** New standalone crate (separate repo from mdv)
**Consumer:** mdv (initial), any Iced 0.14 app (future)

## 1. Purpose

mdv needs LaTeX math rendering in markdown (`$..$`, `$$..$$`). No existing Rust crate renders LaTeX to a native Iced widget without a JavaScript runtime. Existing options either require V8 (`mathjax_svg`, `katex` bindings, `charming`) or emit MathML with no widget consumer (`pulldown-latex`, `latex2mathml`).

`iced_math` fills the gap: pure-Rust, zero-JS, native Iced 0.14 widget rendering LaTeX math via TeX-style box layout on Iced Canvas primitives.

## 2. Goals & Non-Goals

**Goals**
- Pure Rust. Zero JS runtime. Zero V8/deno_core.
- Native Iced 0.14 widget. Crisp at any zoom. Theme-aware text color.
- KaTeX-equivalent coverage (~300 LaTeX commands) at v1.0.
- Minimal public API: two free functions.
- Reusable by any Iced app, not coupled to mdv.

**Non-Goals**
- LaTeX prose typesetting (paragraphs, sections, page layout).
- MathJax/full LaTeX parity (custom macros, complex environments beyond KaTeX).
- Interactive editing inside equations.
- WebAssembly target (v0.x — may revisit).

## 3. Public API

```rust
pub fn inline<Message, Theme, Renderer>(src: &str) -> Element<'static, Message, Theme, Renderer>
where
    Theme: iced::widget::svg::Catalog + 'static,
    Renderer: iced::advanced::svg::Renderer + 'static,
    Message: 'static;

pub fn block<Message, Theme, Renderer>(src: &str) -> Element<'static, Message, Theme, Renderer>
where
    Theme: iced::widget::svg::Catalog + 'static,
    Renderer: iced::advanced::svg::Renderer + 'static,
    Message: 'static;
```

Generic over `Theme` and `Renderer` so the crate works with any Iced 0.14 app (custom themes/renderers) provided the theme implements `svg::Catalog`. `iced::Theme` already does.

`inline()` — text-style layout for in-line math.
`block()` — display-style layout (larger ops, limits above/below big operators), centered with vertical padding.

**No `init()` required.** Font bytes are embedded via `include_bytes!` and parsed once into a `static OnceLock<ttf_parser::Face<'static>>`. No Iced font registration needed because glyphs are rendered as SVG paths (see §6), not via Iced's text shaper.

Returned `Element<'static, _>` — widget owns its SVG bytes and fallback strings, no borrow from `src`.

Errors (parse failure, unknown command) render as raw source in red monospace inline. Never panic.

## 4. Architecture

```
LaTeX src ──▶ pulldown-latex ──▶ parser events ──▶ IR builder ──▶ Boxer ──▶ Box tree ──▶ SVG emitter ──▶ iced::widget::svg
                                                                  (uses MATH table)                       (consumes SVG bytes)
```

Key decision: **glyphs are rendered as SVG `<path>` elements**, with glyph outlines extracted from the bundled font via `ttf-parser::OutlineBuilder`. This addresses MATH-table variants and `GlyphAssembly` extensible glyphs that have no Unicode codepoint and cannot be drawn via Iced's text shaper. Iced consumes the final SVG via the first-class `svg` widget — no custom `Widget` impl required, no baseline/anchor issues with `fill_text`.

**Module layout**

```
src/
├── lib.rs        public API (inline, block)
├── parse.rs      pulldown-latex events → IR
├── ir.rs         IR node enum (Atom, Frac, Subsup, Radical, Accent, Row, Fenced, Mtable, Op, Space)
├── font.rs       Latin Modern Math bytes + ttf-parser Face + OpenType MATH table reader
├── boxer.rs      IR → positioned Box tree (TeX spacing rules + MATH constants)
├── svg.rs        Box tree → SVG bytes (glyph outlines as <path>, rules as <rect>)
├── widget.rs     thin wrapper around iced::widget::svg with theme-color styling
└── error.rs      Red-source fallback Element
```

**IR (intermediate representation)**

```rust
enum Node {
    Atom { class: AtomClass, glyph: char, style: Style },
    Frac { num: Box<Node>, den: Box<Node>, rule: f32 },
    Subsup { base: Box<Node>, sub: Option<Box<Node>>, sup: Option<Box<Node>> },
    Radical { degree: Option<Box<Node>>, body: Box<Node> },
    Accent { kind: AccentKind, body: Box<Node> },
    Row(Vec<Node>),
    Fenced { open: char, close: char, body: Box<Node> },
    Mtable(Vec<Vec<Node>>),
    Op { glyph: char, limits: bool, big: bool },
    Space(SpaceKind),
}

enum AtomClass { Ord, Op, Bin, Rel, Open, Close, Punct, Inner }
```

**Box tree (post-layout)**

```rust
struct Box {
    width: f32,
    height: f32,   // above baseline
    depth: f32,    // below baseline
    kind: BoxKind,
}
enum BoxKind {
    Glyph { glyph_id: u16, font_size: f32 },   // glyph ID, NOT char — required for MATH variants/assembly
    HBox(Vec<(Point, Box)>),
    VBox(Vec<(Point, Box)>),
    Rule { thickness: f32 },
}
```

Glyph identification is by `ttf_parser::GlyphId` (u16). The boxer resolves char → glyph_id via the font's cmap, then optionally swaps to a bigger variant or composes from assembly pieces using MATH-table lookups — all by glyph ID. SVG emitter outlines each glyph by ID via `face.outline_glyph(GlyphId(id), &mut builder)`.

## 5. Layout Engine (Boxer)

Implements TeX math layout per Knuth TeXbook Ch. 17–18 plus OpenType MATH table semantics. Reference implementations: KaTeX (`src/buildCommon.js`, `src/buildHTML.js`), MathJax-3 (`js/output/common/Wrappers/*.ts`), plain TeX (`mlist_to_hlist`).

**Passes per IR subtree**

1. **Class assignment** — atom class from MathML or Unicode block.
2. **Style determination** — Display / Text / Script / ScriptScript × cramped, propagated down.
3. **Glyph variant selection** — pick big/small variant from MathVariants table by style.
4. **Bottom-up box construction**:
   - `Atom` → glyph Box with font metrics.
   - `Frac` → stack num, rule, den using `FractionNumeratorShiftUp` / `FractionDenominatorShiftDown` constants.
   - `Subsup` → position scripts using `SuperscriptShiftUp`, `SubscriptShiftDown`, clamped by `*BottomMin` / `*TopMax`.
   - `Radical` → vinculum + surd, extensible if body tall (GlyphAssembly).
   - `Fenced` → paren variant by body height; GlyphAssembly when no variant tall enough.
   - `Mtable` → measure cells, compute max column widths + row heights, lay out as 2D grid.
   - `Row` → horizontal list with **inter-atom spacing table** (TeX 8×8 class-pair matrix, modulated by style).
   - `Op` → big-op limit positioning (above/below) in display mode, scripts in text mode; `\limits` / `\nolimits` overrides.
5. **Output** — single root Box with width/height/depth and absolute child positions.

**Spacing table** (TeXbook p. 170): `const SPACING: [[Spacing; 8]; 8]` keyed by `(left class, right class)` returning Thin/Med/Thick/None/Invalid, modulated by current style.

**Cramped style** — subscripts and denominators set cramped flag, suppressing further script shift-up.

**Effort estimate**: ~3000–5000 LOC for full Tier 2 boxer.

## 6. Render Layer

**No custom Widget impl.** `svg.rs` emits an SVG document; `widget.rs` wraps `iced::widget::svg::Svg` with theme-color styling.

**SVG emission (svg.rs):**

```rust
pub fn emit(root: &Box) -> Vec<u8> {
    let mut out = String::new();
    let w = root.width;
    let h = root.height + root.depth;
    write!(out, r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#).unwrap();
    walk(&mut out, root, Point::new(0.0, root.height));   // baseline at y = root.height
    write!(out, "</svg>").unwrap();
    out.into_bytes()
}

fn walk(out: &mut String, b: &Box, origin: Point) {
    match &b.kind {
        BoxKind::Glyph { glyph_id, font_size } => {
            // ttf-parser outlines are emitted in font design units, y-up.
            // SVG is y-down. Compose a transform that:
            //   1. translates origin (origin.x, origin.y) — origin.y is the glyph's baseline in SVG coords
            //   2. scales by (font_size / units_per_em) in x
            //   3. scales by (-font_size / units_per_em) in y to flip y-up → y-down
            // Final SVG transform string (a c b d e f via matrix(...)):
            //   matrix(s 0 0 -s ox oy)   where s = font_size / units_per_em
            let s = font_size / face::UNITS_PER_EM;
            let path_d = font::outline_in_design_units(*glyph_id);   // raw font-unit path, y-up
            write!(out, r#"<path transform="matrix({s} 0 0 {neg_s} {ox} {oy})" d="{d}"/>"#,
                s = s, neg_s = -s, ox = origin.x, oy = origin.y, d = path_d).unwrap();
        }
        BoxKind::Rule { thickness } => {
            // Rules are produced in SVG (y-down) coordinates directly by the boxer.
            write!(out, r#"<rect x="{}" y="{}" width="{}" height="{}"/>"#, origin.x, origin.y, b.width, thickness).unwrap();
        }
        BoxKind::HBox(children) | BoxKind::VBox(children) => {
            for (offset, child) in children {
                walk(out, child, origin + *offset);
            }
        }
    }
}
```

**Coordinate system contract:**
- Font design units are y-up with origin at glyph baseline.
- Boxer works in SVG-space units (y-down) with sizes in pixels (post-font-size scaling). Box `height`/`depth`/`width` are pixel measurements.
- The only place font→SVG conversion happens is the glyph emit site above. Scale = `font_size / face.units_per_em()`. Negative y-scale performs the y-flip. The translation `(origin.x, origin.y)` lands the glyph baseline at the boxer-determined SVG coordinate.
- `font::outline_in_design_units` returns the SVG path data string built by a `ttf_parser::OutlineBuilder` impl that writes raw `move_to`/`line_to`/`quad_to`/`curve_to`/`close` commands without any unit conversion.

**Theme color:** Iced 0.14's `svg::Style { color: Some(c) }` applies a whole-SVG color filter — every painted region is recolored to `c`, intrinsic colors in the SVG are ignored. Math is single-color, so this is correct. We do not rely on CSS `currentColor` semantics; the SVG itself emits `<path>`/`<rect>` with no `fill` attribute (default black) and Iced's filter recolors all of them.

**Widget wrapper (widget.rs):**

```rust
pub(crate) fn from_svg<Message, Theme, Renderer>(bytes: Vec<u8>) -> Element<'static, Message, Theme, Renderer>
where
    Theme: iced::widget::svg::Catalog + 'static,
    Renderer: iced::advanced::svg::Renderer + 'static,
    Message: 'static,
{
    iced::widget::svg(iced::widget::svg::Handle::from_memory(bytes))
        .width(Length::Shrink)
        .height(Length::Shrink)
        .into()
}
```

Generic over `Theme: svg::Catalog` and `Renderer: svg::Renderer`. Works with `iced::Theme` and any user theme that implements `svg::Catalog`. Color comes from the theme's default `svg::Catalog` style (Iced 0.14 uses palette text color by default). Consumers needing an explicit override can construct the underlying `iced::widget::svg` directly via the lower-level API — exposed in v0.5 via an `inline_with_style` / `block_with_style` extension if demand surfaces.

**Public API generics** (matches widget.rs above):

```rust
pub fn inline<Message, Theme, Renderer>(src: &str) -> Element<'static, Message, Theme, Renderer>
where Theme: svg::Catalog + 'static, Renderer: svg::Renderer + 'static, Message: 'static;
pub fn block<Message, Theme, Renderer>(src: &str)  -> Element<'static, Message, Theme, Renderer>
where Theme: svg::Catalog + 'static, Renderer: svg::Renderer + 'static, Message: 'static;
```

**Block layout** — `block()` wraps the SVG element in `container` with `center_x` + vertical padding.

## 7. Font Strategy

**Bundled**: Latin Modern Math (~900KB, GUST Font License — verify redistribution terms before commit; fallback STIX Two Math under SIL OFL 1.1 is unambiguous and equivalent quality).

**Why bundled**: math requires OpenType MATH table. System probing adds platform variability; many systems lack any MATH font. Bundling guarantees consistent rendering. ~900KB binary growth acceptable.

**Loader**: `ttf-parser` reads OTF, exposes MATH table constants and MathVariants/GlyphAssembly tables. Pure Rust, no native deps. Already in mdv tree transitively via `mermaid-rs-renderer`.

## 8. Error Handling

Single failure mode: parse error from `pulldown-latex` or unsupported command. Builder returns red monospace text widget with raw source:

```rust
text(src).font(Font::MONOSPACE).color(Color::from_rgb8(0xc0, 0x39, 0x2b))
```

Matches KaTeX convention. Self-evident, easy to debug.

## 9. Dependencies

```toml
[dependencies]
iced       = { version = "0.14", default-features = false, features = ["svg"] }
pulldown-latex = "0.7"
ttf-parser = "0.25"

[dev-dependencies]
insta  = { version = "1", features = ["yaml"] }
resvg  = "0.46"     # rasterize SVG for visual regression diffs (dev-only)
tiny-skia = "0.11"  # backing pixmap for resvg
```

No async runtime. No JS. **No runtime rasterization** — SVG goes straight to Iced. `resvg` + `tiny-skia` are dev-only for golden-image visual regression tests.

## 10. Testing

1. **Unit** — parse.rs (LaTeX → expected IR), boxer.rs (IR → Box dimensions within ε of KaTeX references), font.rs (MATH constants), spacing table sanity.
2. **SVG snapshot** (`insta::assert_snapshot!`) — `tests/corpus/*.tex` → emitted SVG string compared against committed `tests/snapshots/*.snap`. Primary regression net. Fast, deterministic, text-diffable in PR review. ~50 equations at v0.1, ~200 at v1.0.
3. **Pixel regression** (`insta` + `resvg`) — same corpus, rasterize emitted SVG via `resvg` to PNG, compare against committed PNG. Catches font/outline drift the SVG diff would miss. Slower; runs in CI on the corpus subset most prone to visual drift.
4. **Visual demo** (`examples/viewer.rs`) — standalone Iced app, side panel corpus list. Pre-release manual eyeball pass.
5. **Reference comparison** (opt-in, offline) — `scripts/render-katex.sh` produces KaTeX SVGs for corpus, `--features reference-compare` overlays. Not in CI.

**CI matrix**: macOS + Linux + Windows × stable + MSRV (Rust 1.75).

## 11. Repo Layout

```
iced_math/
├── Cargo.toml
├── README.md
├── CHANGELOG.md
├── LICENSE-MIT
├── LICENSE-APACHE
├── assets/
│   ├── LatinModernMath.otf
│   └── OFL.txt
├── src/                  (lib.rs, parse.rs, ir.rs, font.rs, boxer.rs, svg.rs, widget.rs, error.rs)
├── examples/viewer.rs
├── tests/                (parse.rs, boxer.rs, golden.rs, corpus/*.tex, snapshots/*.snap, snapshots/*.png)
├── benches/layout.rs     (criterion)
└── .github/workflows/    (ci.yml, release.yml)
```

**Licensing**: code MIT OR Apache-2.0. Font: SIL OFL 1.1 (verify LMM license; fallback STIX Two Math).

## 12. Roadmap

| Version | Scope | ETA |
|---|---|---|
| v0.1.0 | Tier 1 (~50 cmds): atoms, frac, subsup, sqrt, basic operators, parens, greek | ~3 weeks |
| v0.2.0 | + matrices, aligned, cases, accents, big-ops with limits | +3 weeks |
| v0.3.0 | + AMS symbols, color, sizing modes, extensible delimiters | +2 weeks |
| v0.4.0 | KaTeX parity (Tier 2 complete) + polish | +2 weeks |
| v0.5.0 | `Math::parse` cached handle, themed color override | +1 week |
| v1.0.0 | Stable API, full Tier 2, docs done | ~12 weeks total |

mdv integrates at v0.2.0+ (matrices needed). mdv-side integration spec to follow in separate document.

## 13. Performance Targets

Measured on M1, release build. Three pipeline stages benched separately + end-to-end:

| Stage | `E=mc^2` (inline) | 3×3 matrix (display) |
|---|---|---|
| parse + layout | ≤ 200µs | ≤ 2ms |
| parse + layout + SVG emit | ≤ 400µs | ≤ 4ms |
| end-to-end (above + first iced::widget::svg render through resvg-backed renderer) | ≤ 5ms | ≤ 25ms |
| steady-state re-render (handle cached by renderer) | ≤ 200µs | ≤ 500µs |

Criterion bench gate in CI; regression > 20% on any tracked metric fails PR. `iced::widget::svg::Handle::from_memory` is content-hashed by Iced — identical SVG bytes produce a cache hit on subsequent renders, which is why steady-state is much cheaper than first render.

Before mdv integrates (v0.2.0), end-to-end + steady-state numbers are validated against a representative mdv corpus (~30 equations from real markdown documents) to confirm scroll/zoom remains smooth.

## 14. Open Questions Resolved

- ✅ Coverage: Tier 2 (KaTeX-equivalent).
- ✅ Render primitive: emit SVG (glyph outlines as `<path>`, rules as `<rect>`) consumed by `iced::widget::svg`. Crisp at any zoom; supports MATH-table variants and `GlyphAssembly` glyphs that lack Unicode codepoints; no custom `Widget` impl, no `fill_text` baseline ambiguity.
- ✅ Font: bundle Latin Modern Math (fallback STIX Two Math if GUST license blocks redistribution). Parsed by `ttf-parser`. No `iced::font::load` needed because glyphs are rendered as SVG paths, not via Iced's text shaper.
- ✅ API: two widget functions `inline` / `block`, returning `Element<'static, _>` (widget owns its SVG bytes). No `init()` function.
- ✅ Errors: raw source in red monospace.
- ✅ Architecture: pulldown-latex events → IR → Boxer → SVG → `iced::widget::svg`.
- ✅ Repo: separate from mdv.

### Codex review issues addressed (2026-05-23 revision 2)

- **Font-outline coordinate conversion (high)**: §6 SVG emission now specifies the exact transform `matrix(s 0 0 -s ox oy)` where `s = font_size / units_per_em`. Documents the y-up font space vs y-down SVG space and locates the single conversion site (glyph emit). Boxer works in SVG-space throughout.
- **`svg::Style { color }` semantics (medium)**: §6 corrected — Iced's color field is a whole-SVG recolor filter, not CSS `currentColor`. Math is single-color so the filter is correct; spec no longer claims `currentColor` behavior.
- **Theme/Renderer generics (medium)**: §3 + §6 widget wrapper now generic over `Theme: svg::Catalog` and `Renderer: svg::Renderer`. Crate works with any Iced 0.14 app, not just `iced::Theme`/default renderer.
- **Perf targets coverage (medium)**: §13 expanded to four metrics (parse+layout, +emit, end-to-end first render, steady-state cached). mdv-corpus validation gate before v0.2 release.
- **Repo layout missing .snap (low)**: §11 lists both `.snap` and `.png` snapshot artifacts.

### Codex review issues addressed (2026-05-23 revision 1)

- **Glyph addressing (high)**: pivoted from `Renderer::fill_text` (Unicode-only, shaped text) to SVG `<path>` outlining via `ttf-parser`. MATH variants and `GlyphAssembly` pieces addressed by glyph ID.
- **Widget::layout `&mut self` and `palette()` (high)**: dropped custom `Widget` impl. Use stock `iced::widget::svg` with a `style` closure that pulls `palette().text` from the concrete `Theme` parameter the closure receives.
- **`fill_text` baseline (high)**: no longer used. Baseline anchoring happens inside the SVG via `<g transform>` placement under our control.
- **Three-function vs two-function API (medium)**: dropped `init()`. Two public functions only.
- **"MathML events" wording (medium)**: corrected to "pulldown-latex events → IR". The pulldown-latex MathML renderer is unused; we consume the parser event stream directly.
- **Lifetime story (medium)**: API returns `Element<'static, _>` — widget owns SVG bytes and any error-fallback `String`, no borrow from `src`.
- **tiny-skia ambiguity (medium)**: declared explicitly as dev-only (`[dev-dependencies]`) for pixel-regression tests. Runtime path is SVG-only; no rasterization in shipped binary.

## 15. Out of Scope (Future)

- Custom macros / `\newcommand`.
- mhchem (chemistry).
- `\href` / interactive elements.
- Color overrides at call site (deferred to v0.5).
- WASM target.
- Right-to-left math.
- Accessibility tree export (MathML SSML).
