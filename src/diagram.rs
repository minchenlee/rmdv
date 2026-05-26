//! Diagram rendering core — Mermaid + Graphviz/DOT pipelines.
//!
//! Pure-Rust, blocking renderer plus an LRU cache. Higher layers (T3 render path,
//! T4 app integration) wrap the blocking call in `iced::Task::perform` and feed
//! the resulting SVG into `iced::widget::svg::Handle`.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::sync::{Arc, LazyLock};

pub use crate::ast::DiagramKind;
use crate::theme::Palette;

use iced::widget::image;
use iced::Color;
use tokio::sync::Semaphore;

/// System font database used by `usvg` when rasterizing diagram SVGs.
/// Lazy because `load_system_fonts()` takes ~200-300ms on macOS and we
/// don't want to pay it until the first diagram renders. Shared via Arc
/// so each `usvg::Options` clones a cheap handle.
static USVG_FONTDB: LazyLock<Arc<resvg::usvg::fontdb::Database>> = LazyLock::new(|| {
    let mut db = resvg::usvg::fontdb::Database::new();
    db.load_system_fonts();
    // Sensible defaults so mermaid/dot's generic family names resolve
    // even on systems where 'system-ui' / 'sans-serif' aren't registered.
    db.set_sans_serif_family("Helvetica");
    db.set_serif_family("Times");
    db.set_monospace_family("Menlo");
    Arc::new(db)
});

/// Cap on concurrent diagram renders. Each render acquires one permit before
/// dispatching to `spawn_blocking`. Prevents many simultaneous renders from
/// saturating the blocking thread pool while the UI thread tries to redraw.
static RENDER_LIMIT: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(3));

/// Maximum accepted source size (bytes). Anything larger is rejected before
/// the parser/renderer runs.
pub const MAX_SOURCE_BYTES: usize = 64 * 1024;

/// Maximum accepted rendered SVG size (bytes).
pub const MAX_SVG_BYTES: usize = 4 * 1024 * 1024;

/// Default LRU cache capacity.
pub const DEFAULT_CACHE_CAP: usize = 64;

/// Retina supersample factor for inline rasterization. The pixmap is rendered
/// at this multiple of the SVG's intrinsic size for crispness; consumers that
/// want intrinsic logical size (math display) divide pixel dims by this.
pub const RASTER_SCALE: f32 = 2.0;

/// Output of a successful diagram render. Carries the raw SVG bytes plus a
/// pre-rasterized RGBA pixmap so the UI thread never re-parses SVG at draw
/// time.
#[derive(Debug, Clone)]
pub struct RenderOutput {
    pub svg: Vec<u8>,
    pub rgba: Vec<u8>,
    pub w: u32,
    pub h: u32,
}

/// State of a diagram in the cache.
#[derive(Debug, Clone)]
pub enum DiagramState {
    /// A render task is in flight.
    Pending,
    /// Render completed successfully.
    ///
    /// - `inline` is a pre-rasterized image handle used for both inline view
    ///   and the zoom modal (reuses iced's `image::viewer` for scroll-zoom
    ///   + drag-pan + escape-close parity with normal images). Bypasses
    ///   iced_wgpu's per-redraw SVG parse/raster step.
    /// - `source_bytes` is the raw SVG payload (copy/export).
    Ready {
        inline: image::Handle,
        source_bytes: Arc<Vec<u8>>,
        /// Device-pixel width of the rasterized image. Math display divides by
        /// [`RASTER_SCALE`] to recover the intended logical width (the inline
        /// raster is 2× for retina crispness; iced would otherwise draw it at
        /// 2× logical size). Diagrams ignore this — they fill the column.
        device_w: u32,
    },
    /// Render failed — held so we don't retry on every redraw.
    Err(String),
}

/// LRU cache of rendered diagrams, keyed by `(content_hash, theme_id)`.
#[derive(Debug)]
pub struct DiagramCache {
    inner: lru::LruCache<(u64, u32), DiagramState>,
}

impl DiagramCache {
    pub fn new(cap: usize) -> Self {
        let cap = NonZeroUsize::new(cap.max(1)).expect("cap >= 1");
        Self {
            inner: lru::LruCache::new(cap),
        }
    }

    pub fn get(&mut self, key: &(u64, u32)) -> Option<&DiagramState> {
        self.inner.get(key)
    }

    /// Non-mutating lookup that does not bump LRU recency. Used by the render
    /// path, which only has a `&self` borrow of the cache.
    pub fn peek(&self, key: &(u64, u32)) -> Option<&DiagramState> {
        self.inner.peek(key)
    }

    pub fn put(&mut self, key: (u64, u32), value: DiagramState) {
        self.inner.put(key, value);
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl Default for DiagramCache {
    fn default() -> Self {
        Self::new(DEFAULT_CACHE_CAP)
    }
}

/// Stable `u32` digest of a palette. Used as a cache-bust key so theme changes
/// invalidate cached SVGs without explicit pruning.
pub fn theme_id(palette: &Palette) -> u32 {
    let mut h = DefaultHasher::new();
    // Hash every color-bearing field. We only need monotonic determinism — the
    // exact mixing isn't load-bearing.
    for c in [
        palette.bg,
        palette.surface,
        palette.surface_alt,
        palette.sidebar,
        palette.fg,
        palette.muted,
        palette.subtle,
        palette.accent,
        palette.accent_fg,
        palette.code_bg,
        palette.code_border,
        palette.rule,
        palette.selection,
        palette.match_bg,
        palette.match_current_bg,
        palette.scroller,
        palette.scroller_hover,
        palette.indent_guide,
        palette.tree_selected_bg,
        palette.tree_selected_border,
    ] {
        hash_color(&c, &mut h);
    }
    let full = h.finish();
    (full ^ (full >> 32)) as u32
}

fn hash_color<H: Hasher>(c: &Color, h: &mut H) {
    // Color isn't Hash; fold bytes manually.
    c.r.to_bits().hash(h);
    c.g.to_bits().hash(h);
    c.b.to_bits().hash(h);
    c.a.to_bits().hash(h);
}

fn color_to_hex(c: Color) -> String {
    let r = (c.r.clamp(0.0, 1.0) * 255.0).round() as u8;
    let g = (c.g.clamp(0.0, 1.0) * 255.0).round() as u8;
    let b = (c.b.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

/// Build the mermaid `%%{init}%%` directive that carries our palette.
///
/// Spec maps:
///   background         <- palette.bg
///   primaryColor       <- palette.surface
///   primaryTextColor   <- palette.fg          (Palette has `fg`, not `text`)
///   primaryBorderColor <- palette.code_border (closest analogue to `border`)
///   lineColor          <- palette.muted
///   secondaryColor     <- palette.surface_alt
///   tertiaryColor      <- palette.accent
///   fontFamily         <- caller-supplied font family
fn mermaid_init_block(palette: &Palette, font_family: &str) -> String {
    format!(
        "%%{{init: {{ 'theme': 'base', 'themeVariables': {{ \
         'background': '{bg}', \
         'primaryColor': '{primary}', \
         'primaryTextColor': '{text}', \
         'primaryBorderColor': '{border}', \
         'lineColor': '{line}', \
         'secondaryColor': '{secondary}', \
         'tertiaryColor': '{tertiary}', \
         'fontFamily': '{font}' \
         }} }}}}%%\n",
        bg = color_to_hex(palette.bg),
        primary = color_to_hex(palette.surface),
        text = color_to_hex(palette.fg),
        border = color_to_hex(palette.code_border),
        line = color_to_hex(palette.muted),
        secondary = color_to_hex(palette.surface_alt),
        tertiary = color_to_hex(palette.accent),
        font = font_family,
    )
}

/// True if `source` already opens with a `%%{init` directive — caller wins.
pub(crate) fn has_user_init(source: &str) -> bool {
    source.trim_start().starts_with("%%{init")
}

/// Build the DOT preamble that injects our palette as default graph/node/edge
/// attributes. Inserted immediately after the user's opening `{`.
fn dot_preamble(palette: &Palette, font_family: &str) -> String {
    format!(
        "  graph [bgcolor=\"transparent\" fontname=\"{font}\"];\n  \
         node  [fontcolor=\"{text}\" color=\"{border}\" fillcolor=\"{fill}\" fontname=\"{font}\"];\n  \
         edge  [color=\"{edge}\" fontname=\"{font}\"];\n",
        font = font_family,
        text = color_to_hex(palette.fg),
        border = color_to_hex(palette.code_border),
        fill = color_to_hex(palette.surface),
        edge = color_to_hex(palette.muted),
    )
}

/// Insert `preamble` immediately after the first `{` in `source`. If no `{`
/// is found, returns `source` unchanged (the parser will error out anyway,
/// and we don't want to mangle malformed input).
fn inject_dot_preamble(source: &str, preamble: &str) -> String {
    if let Some(idx) = source.find('{') {
        let (head, tail) = source.split_at(idx + 1);
        let mut out = String::with_capacity(source.len() + preamble.len() + 1);
        out.push_str(head);
        out.push('\n');
        out.push_str(preamble);
        out.push_str(tail);
        out
    } else {
        source.to_string()
    }
}

/// Async wrapper around [`render_blocking`] for `iced::Task::perform`. Owns
/// its inputs and offloads to `tokio::task::spawn_blocking`, so the blocking
/// renderer never stalls the runtime executor.
///
/// After producing the SVG string, this also rasterizes it to RGBA on the
/// same blocking thread so the UI never re-parses SVG at draw time. A global
/// [`RENDER_LIMIT`] semaphore caps how many of these run concurrently.
pub async fn render_blocking_async(
    kind: DiagramKind,
    source: String,
    palette: Palette,
    font_family: String,
) -> Result<RenderOutput, String> {
    // Acquire a permit before spawning the blocking job. The permit drops at
    // the end of this future, which is also after the blocking task has been
    // awaited — so the cap really gates blocking-pool occupancy.
    let _permit = RENDER_LIMIT
        .acquire()
        .await
        .map_err(|_| "render limit closed".to_string())?;
    tokio::task::spawn_blocking(move || -> Result<RenderOutput, String> {
        let svg_string = render_blocking(kind, &source, &palette, &font_family)?;
        let svg_bytes = svg_string.into_bytes();
        let (rgba, w, h) = rasterize_for_inline(&svg_bytes)?;
        Ok(RenderOutput {
            svg: svg_bytes,
            rgba,
            w,
            h,
        })
    })
    .await
    .unwrap_or_else(|_| Err("render task panicked".to_string()))
}

/// Rasterize a diagram SVG to RGBA at a size suitable for the inline view.
///
/// The rasterization target is `min(viewbox_width * 2.0, 1600.0)` px wide,
/// preserving aspect. This gives a crisp display up to ~800px wide UI
/// without ballooning memory on absurdly wide diagrams.
fn rasterize_for_inline(svg_bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
    use resvg::tiny_skia;
    use resvg::usvg;
    const MAX_WIDTH: f32 = 1600.0;

    let mut opt = usvg::Options::default();
    opt.fontdb = USVG_FONTDB.clone();
    let tree = usvg::Tree::from_data(svg_bytes, &opt).map_err(|e| e.to_string())?;
    let sz = tree.size();
    let (w, h) = (sz.width(), sz.height());
    if w <= 0.0 || h <= 0.0 {
        return Err("svg has zero size".into());
    }
    let target = (w * RASTER_SCALE).min(MAX_WIDTH);
    let scale = (target / w).max(0.01);
    let pw = (w * scale).round().max(1.0) as u32;
    let ph = (h * scale).round().max(1.0) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(pw, ph).ok_or("pixmap alloc failed")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok((pixmap.take(), pw, ph))
}

/// Blocking renderer. Wraps the inner work in `catch_unwind` so a panic in a
/// third-party crate doesn't take down the UI thread.
pub fn render_blocking(
    kind: DiagramKind,
    source: &str,
    palette: &Palette,
    font_family: &str,
) -> Result<String, String> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err("diagram too large".to_string());
    }

    // Clone inputs we need inside the unwind boundary.
    let source = source.to_string();
    let palette = *palette;
    let font_family = font_family.to_string();

    let result = std::panic::catch_unwind(move || -> Result<String, String> {
        match kind {
            DiagramKind::Mermaid => render_mermaid(&source, &palette, &font_family),
            DiagramKind::Dot => render_dot(&source, &palette, &font_family),
            DiagramKind::Math => render_math(&source, &palette),
        }
    });

    let svg = match result {
        Ok(Ok(svg)) => svg,
        Ok(Err(msg)) => return Err(msg),
        Err(_) => return Err("internal renderer error".to_string()),
    };

    if svg.len() > MAX_SVG_BYTES {
        return Err("rendered diagram too large".to_string());
    }

    Ok(svg)
}

fn render_mermaid(source: &str, palette: &Palette, font_family: &str) -> Result<String, String> {
    let prepared = if has_user_init(source) {
        source.to_string()
    } else {
        let mut s = mermaid_init_block(palette, font_family);
        s.push_str(source);
        s
    };

    let svg = mermaid_rs_renderer::render_with_options(
        &prepared,
        mermaid_rs_renderer::RenderOptions::default(),
    )
    .map_err(|e| e.to_string())?;

    // The renderer hardcodes a `<rect ... fill="#FFFFFF"/>` as the first
    // child of <svg> for the SVG background. Strip it so the diagram is
    // transparent and the app's container (or zoom-modal scrim) shows
    // through. Cheap pattern match — the rect is always emitted at the
    // start with the diagram's exact width/height.
    Ok(strip_mermaid_white_bg(&svg))
}

fn strip_mermaid_white_bg(svg: &str) -> String {
    // First `<rect ... fill="#FFFFFF"/>` immediately after the opening
    // <svg ...> tag. We scan only the head to avoid touching real content.
    let head_end = svg.find('>').map(|i| i + 1).unwrap_or(0);
    let scan_end = (head_end + 600).min(svg.len());
    let needle_start = "<rect ";
    let needle_white = "fill=\"#FFFFFF\"";
    if let Some(rel) = svg[head_end..scan_end].find(needle_start) {
        let abs = head_end + rel;
        if let Some(end_rel) = svg[abs..scan_end].find("/>") {
            let abs_end = abs + end_rel + 2;
            if svg[abs..abs_end].contains(needle_white) {
                let mut out = String::with_capacity(svg.len() - (abs_end - abs));
                out.push_str(&svg[..abs]);
                out.push_str(&svg[abs_end..]);
                return out;
            }
        }
    }
    svg.to_string()
}

fn render_dot(source: &str, palette: &Palette, font_family: &str) -> Result<String, String> {
    let preamble = dot_preamble(palette, font_family);
    let prepared = inject_dot_preamble(source, &preamble);

    use layout::backends::svg::SVGWriter;
    use layout::gv::{DotParser, GraphBuilder};

    let mut parser = DotParser::new(&prepared);
    let graph = parser.process().map_err(|e| e.to_string())?;

    let mut builder = GraphBuilder::new();
    builder.visit_graph(&graph);
    let mut vg = builder.get();

    let mut svg = SVGWriter::new();
    vg.do_it(false, false, false, &mut svg);
    Ok(svg.finalize())
}

/// Render `$$…$$` display math to SVG via `iced_math`. Glyphs are filled with
/// the theme foreground so block math matches body-text color; the SVG is then
/// rasterized through the same resvg path as other diagrams (iced_math emits
/// pure `<path>`/`<rect>` — no fonts needed at raster time).
/// Display-math glyph size in px. Tuned against the 15.5px body text so the
/// fraction body reads at roughly body weight rather than dominating the
/// column (see the 15.5/16/17/18 comparison — 16 matched best).
const MATH_DISPLAY_PX: f32 = 18.0;

fn render_math(source: &str, palette: &Palette) -> Result<String, String> {
    let fill = iced_math::Color::rgb(
        (palette.fg.r.clamp(0.0, 1.0) * 255.0).round() as u8,
        (palette.fg.g.clamp(0.0, 1.0) * 255.0).round() as u8,
        (palette.fg.b.clamp(0.0, 1.0) * 255.0).round() as u8,
    );
    let bytes = iced_math::MathRenderer::new()
        .font_size(MATH_DISPLAY_PX)
        .display_style(true)
        .color(fill)
        .to_svg(source)
        .map_err(|e| e.to_string())?;
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Palette;

    fn palette() -> Palette {
        Palette::ONE_DARK
    }

    #[test]
    fn init_skipped_when_user_provides_it() {
        assert!(has_user_init("%%{init: { 'theme': 'dark' }}%%\ngraph LR\nA-->B"));
        assert!(!has_user_init("graph LR\nA-->B"));
    }

    #[test]
    fn math_renders_all_constructs_to_rasterizable_svg() {
        // Each goes through the full render_blocking + rasterize path the app
        // uses, proving fractions/matrices/\mathbb/\binom/sums all produce a
        // non-empty pixmap rather than an error or zero-size SVG.
        for src in [
            r"\frac{-b \pm \sqrt{b^2 - 4ac}}{2a}",
            r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}",
            r"\mathbb{E}[X] \in \mathbb{R}^n \quad \binom{n}{k}",
            r"\sum_{i=1}^{n} i = \frac{n(n+1)}{2}",
        ] {
            let svg = render_blocking(DiagramKind::Math, src, &palette(), "")
                .unwrap_or_else(|e| panic!("render failed for {src:?}: {e}"));
            assert!(svg.contains("<path"), "no glyph paths for {src:?}");
            let (rgba, w, h) = rasterize_for_inline(svg.as_bytes())
                .unwrap_or_else(|e| panic!("raster failed for {src:?}: {e}"));
            assert!(w > 0 && h > 0 && !rgba.is_empty(), "empty raster for {src:?}");
        }
    }

    #[test]
    fn math_glyph_fill_follows_palette_fg() {
        let svg = render_blocking(DiagramKind::Math, "x", &Palette::ONE_LIGHT, "").unwrap();
        let fg = Palette::ONE_LIGHT.fg;
        let hex = color_to_hex(fg);
        assert!(svg.contains(&hex), "expected fill {hex} in svg, got: {svg}");
    }

    #[test]
    fn dot_preamble_inserted_after_brace() {
        let injected = inject_dot_preamble("digraph G { a -> b }", "PREAMBLE\n");
        assert!(injected.starts_with("digraph G {"));
        assert!(injected.contains("PREAMBLE"));
        // Preamble appears before the edge.
        let pre = injected.find("PREAMBLE").unwrap();
        let edge = injected.find("a -> b").unwrap();
        assert!(pre < edge);
    }

    #[test]
    fn theme_id_changes_with_palette() {
        let a = theme_id(&Palette::ONE_DARK);
        let b = theme_id(&Palette::ONE_LIGHT);
        assert_ne!(a, b);
    }

    #[test]
    fn cache_basic() {
        let mut cache = DiagramCache::new(2);
        cache.put((1, 0), DiagramState::Pending);
        cache.put((2, 0), DiagramState::Err("boom".into()));
        assert_eq!(cache.len(), 2);
        assert!(matches!(cache.get(&(1, 0)), Some(DiagramState::Pending)));
    }

    #[test]
    fn oversized_source_rejected() {
        let big = "x".repeat(MAX_SOURCE_BYTES + 1);
        let err =
            render_blocking(DiagramKind::Mermaid, &big, &palette(), "system-ui").unwrap_err();
        assert!(err.contains("too large"), "got: {err}");
    }
}
