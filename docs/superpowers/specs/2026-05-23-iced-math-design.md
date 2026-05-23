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
pub fn inline<'a, Message>(src: &str) -> Element<'a, Message>;
pub fn block<'a, Message>(src: &str)  -> Element<'a, Message>;

/// One-time font load. Returns Task to chain in app startup.
pub fn init() -> iced::Task<Result<(), iced::font::Error>>;
```

`inline()` — text-style layout for in-line math.
`block()` — display-style layout (larger ops, limits above/below big operators), centered with vertical padding.

Errors (parse failure, unknown command) render as raw source in red monospace inline. Never panic.

## 4. Architecture

```
LaTeX src ──▶ pulldown-latex ──▶ MathML events ──▶ IR builder ──▶ Boxer ──▶ Box tree ──▶ Iced Widget draw
                                                                  (uses MATH table)        (Renderer fill_text + fill_quad)
```

**Module layout**

```
src/
├── lib.rs        public API (inline, block, init)
├── parse.rs      pulldown-latex driver, MathML events → IR
├── ir.rs         IR node enum (Atom, Frac, Subsup, Radical, Accent, Row, Fenced, Mtable, Op, Space)
├── font.rs       Latin Modern Math bytes + OpenType MATH table reader (ttf-parser)
├── boxer.rs      IR → positioned Box tree (TeX spacing rules + MATH constants)
├── render.rs     Box tree → Renderer primitives
├── widget.rs     iced::advanced::Widget implementation
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
    Glyph { ch: char, font_size: f32 },
    HBox(Vec<(Point, Box)>),
    VBox(Vec<(Point, Box)>),
    Rule { thickness: f32 },
}
```

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

Custom `iced::advanced::Widget` (not Canvas Program — no interaction in v0.1).

```rust
impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for MathWidget
where Renderer: iced::advanced::text::Renderer,
{
    fn size(&self) -> Size<Length> { Size { width: Shrink, height: Shrink } }
    fn layout(&self, _, _, _) -> Node {
        Node::new(Size::new(self.root.width, self.root.height + self.root.depth))
    }
    fn draw(&self, _, renderer, theme, _, layout, _, _) {
        let color = theme.palette().text;
        draw_box(renderer, &self.root, layout.bounds().position(), color);
    }
}
```

`draw_box` walks Box tree:
- `Glyph` → `Renderer::fill_text` at `origin + (0, b.height)` (baseline offset).
- `Rule` → `Renderer::fill_quad`.
- `HBox` / `VBox` → recurse with offset accumulation.

**Font registration** — `iced_math::init()` loads bundled Latin Modern Math via `iced::font::load`. One-shot, stored in `static OnceLock<Font>`. Caller chains this in app startup `Task`.

**Block layout** — `block()` wraps widget in `container` with `center_x` + vertical padding.

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
iced = { version = "0.14", default-features = false, features = ["advanced"] }
pulldown-latex = "0.7"
ttf-parser = "0.25"
```

No async runtime. No JS. No image crate (no rasterization).

## 10. Testing

1. **Unit** — parse.rs (LaTeX → expected IR), boxer.rs (IR → Box dimensions within ε of KaTeX references), font.rs (MATH constants), spacing table sanity.
2. **Golden image** (`insta`) — `tests/corpus/*.tex` rendered via tiny-skia to PNG, compared against committed `tests/snapshots/*.png`. ~50 equations at v0.1, ~200 at v1.0.
3. **Visual demo** (`examples/viewer.rs`) — standalone Iced app, side panel corpus list. Pre-release manual eyeball pass.
4. **Reference comparison** (opt-in, offline) — `scripts/render-katex.sh` produces KaTeX SVGs for corpus, `--features reference-compare` overlays. Not in CI.

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
├── src/                  (lib.rs, parse.rs, ir.rs, font.rs, boxer.rs, render.rs, widget.rs, error.rs)
├── examples/viewer.rs
├── tests/                (parse.rs, boxer.rs, golden.rs, corpus/*.tex, snapshots/*.png)
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

- `E=mc^2` inline parse+layout ≤ 200µs (M1).
- Display matrix (3×3) parse+layout ≤ 2ms (M1).
- Criterion bench gate in CI; regression > 20% fails PR.

## 14. Open Questions Resolved

- ✅ Coverage: Tier 2 (KaTeX-equivalent).
- ✅ Render primitive: native Iced Canvas via `advanced::Widget` (not image, not Iced widget tree, not SVG).
- ✅ Font: bundle Latin Modern Math (fallback STIX Two Math if license blocks).
- ✅ API: two free functions `inline` / `block`.
- ✅ Errors: raw source in red monospace.
- ✅ Architecture: MathML → IR → Boxer → Canvas.
- ✅ Repo: separate from mdv.

## 15. Out of Scope (Future)

- Custom macros / `\newcommand`.
- mhchem (chemistry).
- `\href` / interactive elements.
- Color overrides at call site (deferred to v0.5).
- WASM target.
- Right-to-left math.
- Accessibility tree export (MathML SSML).
