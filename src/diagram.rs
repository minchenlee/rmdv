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

/// Borrows one bundled font out of the static table so it can go into the
/// font database without copying the bytes. Only the Windows branch registers
/// bundled faces; macOS resolves through its system fonts.
#[cfg(windows)]
struct BundledFont(&'static [u8]);

#[cfg(windows)]
impl AsRef<[u8]> for BundledFont {
    fn as_ref(&self) -> &[u8] {
        self.0
    }
}

/// Font database used by `usvg` when rasterizing diagram SVGs.
///
/// Lazy because `load_system_fonts()` takes ~200-300ms on macOS and we don't
/// want to pay it until the first diagram renders. Shared via Arc so each
/// `usvg::Options` clones a cheap handle.
///
/// Diagram SVGs ask for the app's editor font ("JetBrains Mono"), which Iced
/// registers but the OS does not have. How that gets resolved is
/// platform-specific, because `fontdb` treats an unknown family as "no face at
/// all": `query` returns `None`, usvg then retries its single last-resort
/// family (`Family::Serif`), and if *that* also misses, the whole `<text>`
/// element is dropped and the label renders as nothing while its shape still
/// draws.
///
/// macOS ships Helvetica/Times/Menlo, so naming them as the generic families
/// is enough for that fallback chain to land on a real face. Windows has none
/// of the three, so there the chain would die and every label would vanish;
/// Windows instead registers the bundled faces and points the generics at
/// them. Keep the macOS branch as-is — it is the shipping behavior there.
static USVG_FONTDB: LazyLock<Arc<resvg::usvg::fontdb::Database>> = LazyLock::new(|| {
    let mut db = resvg::usvg::fontdb::Database::new();
    db.load_system_fonts();

    #[cfg(windows)]
    {
        // Bundled after the system faces so the app's own type wins ties and
        // diagram text matches the reader even when the host has no
        // "JetBrains Mono"/"Inter" installed.
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
        // macOS generics: all three are system faces there.
        db.set_sans_serif_family("Helvetica");
        db.set_serif_family("Times");
        db.set_monospace_family("Menlo");
    }

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

/// Soft byte budget for the diagram cache. Entry heights are uncapped
/// (MAX_WIDTH only bounds width), so 64 tall retina pixmaps can reach
/// hundreds of MB; this bounds retained RGBA + SVG bytes. Generous enough
/// that a realistic single document never evicts.
pub const DEFAULT_CACHE_BYTE_BUDGET: usize = 256 * 1024 * 1024;

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
    /// Device-pixel size of the raster in `rgba`.
    pub w: u32,
    pub h: u32,
    /// The SVG's own CSS size (its `viewBox`), i.e. the size the Mermaid or
    /// Graphviz layout chose. The raster used to be exactly this at
    /// [`RASTER_SCALE`], but it is capped, so the reading modes need this to
    /// know the figure's natural width even for a very wide diagram.
    pub svg_w: f32,
    pub svg_h: f32,
}

/// State of a diagram in the cache.
#[derive(Debug, Clone)]
pub enum DiagramState {
    /// A render task is in flight.
    Pending,
    /// Render completed successfully.
    ///
    /// - `inline` is a pre-rasterized image handle used for both inline view
    ///   and the zoom modal (`crate::zoom_viewer`, the same viewer as for
    ///   normal images). Bypasses iced_wgpu's per-redraw SVG parse/raster
    ///   step.
    /// - `source_bytes` is the raw SVG payload (copy/export).
    Ready {
        inline: image::Handle,
        source_bytes: Arc<Vec<u8>>,
        /// Device-pixel width of the rasterized image. Math display divides by
        /// [`RASTER_SCALE`] to recover the intended logical width (the inline
        /// raster is 2× for retina crispness; iced would otherwise draw it at
        /// 2× logical size).
        device_w: u32,
        /// The figure's natural size in logical pixels, from the SVG. The
        /// raster can be smaller (it is capped), so a reading mode sizes the
        /// figure from this rather than from the raster.
        figure_w: f32,
        figure_h: f32,
    },
    /// Render failed — held so we don't retry on every redraw.
    Err(String),
}

/// Retained heap cost of one cache entry: decoded RGBA pixels plus the raw
/// SVG payload. `Pending` costs nothing; `Err` only its message.
fn state_cost(s: &DiagramState) -> usize {
    match s {
        DiagramState::Pending => 0,
        DiagramState::Err(msg) => msg.len(),
        DiagramState::Ready {
            inline,
            source_bytes,
            ..
        } => {
            let pixels = match inline {
                image::Handle::Rgba { pixels, .. } => pixels.len(),
                _ => 0,
            };
            pixels + source_bytes.len()
        }
    }
}

/// LRU cache of rendered diagrams, keyed by `(content_hash, theme_id)`.
/// Bounded by entry count AND total byte cost: on insert, least-recently-used
/// entries are dropped until the tracked cost fits the byte budget. The entry
/// just inserted is never evicted, so the diagram being rendered right now
/// always displays.
#[derive(Debug)]
pub struct DiagramCache {
    inner: lru::LruCache<(u64, u32), DiagramState>,
    bytes: usize,
    byte_budget: usize,
}

impl DiagramCache {
    pub fn new(cap: usize) -> Self {
        Self::with_byte_budget(cap, DEFAULT_CACHE_BYTE_BUDGET)
    }

    pub fn with_byte_budget(cap: usize, byte_budget: usize) -> Self {
        let cap = NonZeroUsize::new(cap.max(1)).expect("cap >= 1");
        Self {
            inner: lru::LruCache::new(cap),
            bytes: 0,
            byte_budget,
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
        // LruCache::put drops the LRU entry silently when inserting a new key
        // at capacity; pop it ourselves first so the byte tracking stays exact.
        if !self.inner.contains(&key) && self.inner.len() == self.inner.cap().get() {
            if let Some((_, dropped)) = self.inner.pop_lru() {
                self.bytes = self.bytes.saturating_sub(state_cost(&dropped));
            }
        }
        self.bytes += state_cost(&value);
        if let Some(replaced) = self.inner.put(key, value) {
            self.bytes = self.bytes.saturating_sub(state_cost(&replaced));
        }
        while self.bytes > self.byte_budget && self.inner.len() > 1 {
            // A Pending entry at the LRU end costs 0 bytes — evicting it can't
            // reach the budget and would orphan its in-flight render task
            // (prime_diagram_cache would re-dispatch on the next pass). Stop
            // and accept the overshoot; the budget re-converges once the
            // pending renders complete and re-bump their keys.
            if matches!(self.inner.peek_lru(), Some((_, DiagramState::Pending))) {
                break;
            }
            if let Some((_, dropped)) = self.inner.pop_lru() {
                self.bytes = self.bytes.saturating_sub(state_cost(&dropped));
            } else {
                break;
            }
        }
    }

    pub fn remove(&mut self, key: &(u64, u32)) -> Option<DiagramState> {
        let removed = self.inner.pop(key);
        if let Some(state) = removed.as_ref() {
            self.bytes = self.bytes.saturating_sub(state_cost(state));
        }
        removed
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Tracked retained cost in bytes (RGBA pixels + SVG payloads).
    pub fn cost_bytes(&self) -> usize {
        self.bytes
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
    theme_id_with(palette, &crate::theme::DiagramTheme::NONE)
}

/// [`theme_id`] for a palette plus a custom theme's `[diagram]` overrides. A
/// color override must bust the cache just as a palette change does, otherwise
/// editing `[diagram]` in a theme file would keep showing the old render.
pub fn theme_id_with(palette: &Palette, theme: &crate::theme::DiagramTheme) -> u32 {
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
    theme.hash_into(&mut h);
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

/// Flatten an sRGB overlay onto its opaque backdrop before encoding RGB.
/// Mermaid derives other theme colors from these hex values, so preserving
/// the visible blend is more portable than passing alpha through its themes.
fn color_over(c: Color, backdrop: Color) -> Color {
    let alpha = c.a.clamp(0.0, 1.0);
    Color::from_rgb(
        c.r * alpha + backdrop.r * (1.0 - alpha),
        c.g * alpha + backdrop.g * (1.0 - alpha),
        c.b * alpha + backdrop.b * (1.0 - alpha),
    )
}

/// Concrete diagram colors for one render: every `[diagram]` override applied
/// over the palette-derived default. Keeping this in one place means Mermaid
/// and DOT cannot drift, and "what color is a node" has a single answer.
struct ResolvedDiagram {
    background: Color,
    text: Color,
    node_fill: Color,
    node_border: Color,
    line: Color,
    cluster_fill: Color,
    cluster_border: Color,
    note_fill: Color,
    note_border: Color,
    note_text: Color,
}

impl ResolvedDiagram {
    fn new(palette: &Palette, theme: &crate::theme::DiagramTheme) -> Self {
        // Shapes and edges sit over the document; labels sit over whatever fill
        // is behind them. Composite translucent palette entries onto the right
        // backdrop before encoding, because Mermaid derives further colors from
        // these hex values and layout-rs drops alpha entirely.
        let node_fill = theme
            .node_fill
            .unwrap_or_else(|| color_over(palette.surface, palette.bg));
        let cluster_fill = theme
            .cluster_fill
            .unwrap_or_else(|| color_over(palette.surface_alt, palette.bg));
        let note_fill = theme.note_fill.unwrap_or(cluster_fill);
        let background = theme.background.unwrap_or(palette.bg);

        Self {
            background,
            text: theme
                .text
                .unwrap_or_else(|| color_over(palette.fg, node_fill)),
            node_fill,
            node_border: theme
                .node_border
                .unwrap_or_else(|| color_over(palette.code_border, node_fill)),
            line: theme
                .line
                .unwrap_or_else(|| color_over(palette.muted, palette.bg)),
            cluster_fill,
            cluster_border: theme
                .cluster_border
                .unwrap_or_else(|| color_over(palette.code_border, cluster_fill)),
            note_fill,
            note_border: theme
                .note_border
                .unwrap_or_else(|| color_over(palette.code_border, note_fill)),
            note_text: theme
                .note_text
                .unwrap_or_else(|| color_over(palette.fg, note_fill)),
        }
    }
}

/// Build the mermaid `%%{init}%%` directive that carries our palette.
///
/// The roles come from [`ResolvedDiagram`], so a `[diagram]` override reaches
/// Mermaid without further mapping. Besides the obvious flowchart variables the
/// block pins the ones Mermaid would otherwise derive into colors that ignore
/// the theme: `clusterBkg`/`clusterBorder` default to a hue-shifted
/// `tertiaryColor` (a saturated panel on a dark ground), and `noteBkgColor` is
/// a hardcoded light yellow that is never derived from `primaryColor` at all.
fn mermaid_init_block(
    palette: &Palette,
    theme: &crate::theme::DiagramTheme,
    font_family: &str,
) -> String {
    let pal = ResolvedDiagram::new(palette, theme);
    // Edge labels paint their background behind text, so give them a panel the
    // foreground actually contrasts against rather than the subgraph fill.
    let edge_label_bg = color_over(palette.code_bg, pal.background);
    format!(
        "%%{{init: {{ 'theme': 'base', 'themeVariables': {{ \
         'background': '{bg}', \
         'primaryColor': '{node_fill}', \
         'primaryTextColor': '{text}', \
         'primaryBorderColor': '{node_border}', \
         'lineColor': '{line}', \
         'secondaryColor': '{cluster_fill}', \
         'tertiaryColor': '{cluster_border}', \
         'textColor': '{text}', \
         'titleColor': '{text}', \
         'nodeTextColor': '{text}', \
         'nodeBorder': '{node_border}', \
         'mainBkg': '{node_fill}', \
         'clusterBkg': '{cluster_fill}', \
         'clusterBorder': '{cluster_border}', \
         'edgeLabelBackground': '{edge_label_bg}', \
         'noteBkgColor': '{note_fill}', \
         'noteBorderColor': '{note_border}', \
         'noteTextColor': '{note_text}', \
         'actorBkg': '{node_fill}', \
         'actorBorder': '{node_border}', \
         'actorTextColor': '{text}', \
         'actorLineColor': '{node_border}', \
         'signalColor': '{line}', \
         'signalTextColor': '{text}', \
         'labelBoxBkgColor': '{node_fill}', \
         'labelBoxBorderColor': '{node_border}', \
         'labelTextColor': '{text}', \
         'loopTextColor': '{text}', \
         'activationBkgColor': '{cluster_fill}', \
         'activationBorderColor': '{node_border}', \
         'fontFamily': '{font}' \
         }} }}}}%%\n",
        bg = color_to_hex(pal.background),
        node_fill = color_to_hex(pal.node_fill),
        text = color_to_hex(pal.text),
        node_border = color_to_hex(pal.node_border),
        line = color_to_hex(pal.line),
        cluster_fill = color_to_hex(pal.cluster_fill),
        cluster_border = color_to_hex(pal.cluster_border),
        edge_label_bg = color_to_hex(edge_label_bg),
        note_fill = color_to_hex(pal.note_fill),
        note_border = color_to_hex(pal.note_border),
        note_text = color_to_hex(pal.note_text),
        font = font_family,
    )
}

/// True if `source` already opens with a `%%{init` directive — caller wins.
pub(crate) fn has_user_init(source: &str) -> bool {
    source.trim_start().starts_with("%%{init")
}

/// Build the DOT preamble that injects our palette as default graph/node/edge
/// attributes. Inserted immediately after the user's opening `{`.
///
/// Alpha is flattened onto the backdrop first. `code_border`/`rule` are
/// translucent hairlines; layout-rs only understands opaque hex, and a
/// discarded alpha would turn a faint hairline into a solid black or white
/// ring around every node.
fn dot_preamble(
    palette: &Palette,
    theme: &crate::theme::DiagramTheme,
    font_family: &str,
) -> String {
    let pal = ResolvedDiagram::new(palette, theme);
    format!(
        "  graph [bgcolor=\"transparent\" fontname=\"{font}\"];\n  \
         node  [color=\"{border}\" fillcolor=\"{fill}\" fontname=\"{font}\"];\n  \
         edge  [color=\"{edge}\" fontname=\"{font}\"];\n",
        font = font_family,
        border = color_to_hex(pal.node_border),
        fill = color_to_hex(pal.node_fill),
        edge = color_to_hex(pal.line),
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
    render_blocking_async_with(
        kind,
        source,
        palette,
        crate::theme::DiagramTheme::NONE,
        font_family,
    )
    .await
}

/// [`render_blocking_async`] with a custom theme's `[diagram]` overrides.
pub async fn render_blocking_async_with(
    kind: DiagramKind,
    source: String,
    palette: Palette,
    theme: crate::theme::DiagramTheme,
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
        let svg_string = render_blocking_with(kind, &source, &palette, &theme, &font_family)?;
        let svg_bytes = svg_string.into_bytes();
        let (rgba, w, h, svg_w, svg_h) = rasterize_for_inline(&svg_bytes)?;
        Ok(RenderOutput {
            svg: svg_bytes,
            rgba,
            w,
            h,
            svg_w,
            svg_h,
        })
    })
    .await
    .unwrap_or_else(|_| Err("render task panicked".to_string()))
}

/// The largest side of a sharper raster made for the zoomed viewer.
pub const ZOOM_RASTER_MAX_DIM: u32 = 8192;
/// The most bytes one sharper zoom raster may hold (also limited by what
/// is left of the diagram cache's byte budget).
pub const ZOOM_RASTER_MAX_BYTES: usize = 64 * 1024 * 1024;

/// The size of a sharper raster for a diagram whose inline raster is
/// `base` pixels, when the viewer wants `tier` raster pixels per base
/// pixel. Capped at [`ZOOM_RASTER_MAX_DIM`] a side and `budget_bytes` of
/// RGBA. `None` when it would be no sharper than `base`.
pub fn zoom_raster_size(base: (u32, u32), tier: u32, budget_bytes: usize) -> Option<(u32, u32)> {
    let (bw, bh) = (base.0 as f64, base.1 as f64);
    if bw < 1.0 || bh < 1.0 || tier <= 1 {
        return None;
    }
    let max_dim = f64::from(ZOOM_RASTER_MAX_DIM);
    let by_bytes = (budget_bytes as f64 / (4.0 * bw * bh)).sqrt();
    let factor = f64::from(tier)
        .min(max_dim / bw)
        .min(max_dim / bh)
        .min(by_bytes);
    let (w, h) = ((bw * factor).floor(), (bh * factor).floor());
    // Less than 5% wider is not worth a raster.
    if w <= bw * 1.05 {
        return None;
    }
    Some((w as u32, h as u32))
}

/// Rasterize a diagram's SVG `width` pixels wide for the zoomed viewer,
/// off the UI thread and under the same concurrency cap as diagram renders.
pub async fn rasterize_zoom(svg: Arc<Vec<u8>>, width: u32) -> Result<image::Handle, String> {
    let _permit = RENDER_LIMIT
        .acquire()
        .await
        .map_err(|_| "render limit closed".to_string())?;
    tokio::task::spawn_blocking(move || {
        let (rgba, w, h, _, _) = rasterize_scaled(&svg, |_| width as f32)?;
        Ok(image::Handle::from_rgba(w, h, rgba))
    })
    .await
    .unwrap_or_else(|_| Err("zoom raster task panicked".to_string()))
}

/// Rasterize a diagram SVG to RGBA at a size suitable for the inline view.
///
/// The rasterization target is `min(viewbox_width * 2.0, 1600.0)` px wide,
/// preserving aspect. This gives a crisp display up to ~800px wide UI
/// without ballooning memory on absurdly wide diagrams.
///
/// Returns the raster plus the SVG's own size so the reading modes can keep a
/// figure that was too wide to raster at full retina scale crisp and still know
/// its natural width.
fn rasterize_for_inline(svg_bytes: &[u8]) -> Result<(Vec<u8>, u32, u32, f32, f32), String> {
    const MAX_WIDTH: f32 = 1600.0;
    rasterize_scaled(svg_bytes, |w| (w * RASTER_SCALE).min(MAX_WIDTH))
}

/// Rasterize an SVG `target(intrinsic width)` pixels wide, keeping its
/// aspect. Returns `(pixels, width, height, svg_width, svg_height)`.
fn rasterize_scaled(
    svg_bytes: &[u8],
    target: impl FnOnce(f32) -> f32,
) -> Result<(Vec<u8>, u32, u32, f32, f32), String> {
    use resvg::tiny_skia;
    use resvg::usvg;

    let mut opt = usvg::Options::default();
    opt.fontdb = USVG_FONTDB.clone();
    let tree = usvg::Tree::from_data(svg_bytes, &opt).map_err(|e| e.to_string())?;
    let sz = tree.size();
    let (w, h) = (sz.width(), sz.height());
    if w <= 0.0 || h <= 0.0 {
        return Err("svg has zero size".into());
    }
    let target = target(w);
    let scale = (target / w).max(0.01);
    let pw = (w * scale).round().max(1.0) as u32;
    let ph = (h * scale).round().max(1.0) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(pw, ph).ok_or("pixmap alloc failed")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok((pixmap.take(), pw, ph, w, h))
}

/// Blocking renderer with the palette-derived diagram colors. Thin wrapper over
/// [`render_blocking_with`] for callers that need no `[diagram]` overrides.
pub fn render_blocking(
    kind: DiagramKind,
    source: &str,
    palette: &Palette,
    font_family: &str,
) -> Result<String, String> {
    render_blocking_with(
        kind,
        source,
        palette,
        &crate::theme::DiagramTheme::NONE,
        font_family,
    )
}

/// Blocking renderer. Wraps the inner work in `catch_unwind` so a panic in a
/// third-party crate doesn't take down the UI thread.
///
/// `theme` carries a custom theme's `[diagram]` overrides; pass
/// [`crate::theme::DiagramTheme::NONE`] to derive every diagram color from
/// `palette`.
pub fn render_blocking_with(
    kind: DiagramKind,
    source: &str,
    palette: &Palette,
    theme: &crate::theme::DiagramTheme,
    font_family: &str,
) -> Result<String, String> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err("diagram too large".to_string());
    }

    // Clone inputs we need inside the unwind boundary.
    let source = source.to_string();
    let palette = *palette;
    let theme = *theme;
    let font_family = font_family.to_string();

    let result = std::panic::catch_unwind(move || -> Result<String, String> {
        match kind {
            DiagramKind::Mermaid => render_mermaid(&source, &palette, &theme, &font_family),
            DiagramKind::Dot => render_dot(&source, &palette, &theme, &font_family),
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

fn render_mermaid(
    source: &str,
    palette: &Palette,
    theme: &crate::theme::DiagramTheme,
    font_family: &str,
) -> Result<String, String> {
    let prepared = if has_user_init(source) {
        source.to_string()
    } else {
        let mut s = mermaid_init_block(palette, theme, font_family);
        s.push_str(source);
        s
    };

    // The default output may contain HTML labels, which resvg cannot paint.
    // Ask Merman to lower them to SVG text before using our existing raster path.
    let request = merman::SvgRequest {
        pipeline: Some(merman::svg::SvgPipeline::resvg_safe()),
        ..Default::default()
    };
    let control = merman::OperationControl::new().with_deadline(std::time::Duration::from_secs(10));
    let output = merman::Renderer::new()
        .render(merman::RenderRequest::svg(&prepared, control, request))
        .map_err(|e| e.to_string())?;
    match output {
        merman::RenderOutput::Svg(Some(svg)) => Ok(svg.svg().to_owned()),
        _ => Err("no Mermaid diagram detected".to_string()),
    }
}

fn render_dot(
    source: &str,
    palette: &Palette,
    theme: &crate::theme::DiagramTheme,
    font_family: &str,
) -> Result<String, String> {
    let preamble = dot_preamble(palette, theme, font_family);
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
    Ok(theme_dot_svg(&svg.finalize(), palette, theme, font_family))
}

/// Make layout-rs' SVG honour our palette, and on Windows our font.
///
/// layout-rs' `SVGWriter` reads `color`/`fillcolor` from the DOT attributes but
/// ignores `fontcolor` entirely: every `<text>` it emits has no `fill`, so
/// labels paint with the SVG default (black). That is wrong in every theme —
/// black labels disappear on a dark ground — on every platform, so the fill
/// rule below is unconditional.
///
/// The font family is different. `SVGWriter` hardcodes
/// `font-family: Times, serif`, which macOS resolves through its system Times
/// and renders. Rewriting it to the app's editor font blindly would replace a
/// working family with one the OS lacks, so that half is Windows-only, where
/// the raster font database now carries those bundled faces; it is also where
/// the hardcoded serif would otherwise need a *different* real face anyway.
/// See the "post-process the SVG output" note in
/// `docs/superpowers/specs/2026-05-16-diagrams-design.md`.
fn theme_dot_svg(
    svg: &str,
    palette: &Palette,
    theme: &crate::theme::DiagramTheme,
    font_family: &str,
) -> String {
    // layout-rs writes no label fill at all, so this is the only place the
    // label color is decided.
    let label = color_to_hex(ResolvedDiagram::new(palette, theme).text);

    #[cfg(windows)]
    let mut out = {
        let family = css_font_family(font_family);
        svg.replace(
            "font-family: Times, serif;",
            &format!("font-family: {family};"),
        )
    };
    #[cfg(not(windows))]
    let mut out = {
        let _ = font_family;
        svg.to_owned()
    };

    // A type selector beats a presentation attribute and the `font-color`-less
    // class rule, so this is the one place that decides label colour. A
    // `<textPath>` inherits from its parent, but name it too so an edge label
    // in a `<text>` that sets its own fill cannot win.
    let rule = format!("<style>text,textPath{{fill:{label};}}</style>");
    // layout-rs always closes the document; if it ever did not, skip the patch
    // rather than mangle the SVG.
    if let Some(i) = out.rfind("</svg>") {
        out.insert_str(i, &rule);
    }
    out
}

/// Quote a font family for use in an inline CSS declaration. `JetBrains Mono`
/// is a single family name that needs quotes; a bare name with no spaces works
/// unquoted, but quoting always is simpler and always valid.
#[cfg(windows)]
fn css_font_family(font_family: &str) -> String {
    let name = font_family.trim();
    if name.is_empty() {
        return "sans-serif".to_string();
    }
    format!("\"{}\", sans-serif", name.replace('"', ""))
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
    fn theme_colors_composite_alpha_before_rgb_encoding() {
        let white = Color::WHITE;
        assert_eq!(
            color_to_hex(color_over(Color::from_rgba(0.0, 0.0, 0.0, 0.0), white)),
            "#ffffff"
        );
        assert_eq!(
            color_to_hex(color_over(Color::from_rgba(0.0, 0.0, 0.0, 0.5), white)),
            "#808080"
        );
        assert_eq!(color_to_hex(color_over(Color::BLACK, white)), "#000000");
    }

    #[test]
    fn zoom_raster_size_scales_by_tier_within_the_caps() {
        let big = usize::MAX;
        // Tier 1 is the inline raster itself: nothing sharper to make.
        assert_eq!(zoom_raster_size((800, 400), 1, big), None);
        assert_eq!(zoom_raster_size((800, 400), 0, big), None);
        assert_eq!(zoom_raster_size((800, 400), 2, big), Some((1600, 800)));
        assert_eq!(zoom_raster_size((800, 400), 4, big), Some((3200, 1600)));
        // A side never passes the dimension cap; the aspect holds.
        assert_eq!(zoom_raster_size((1600, 400), 16, big), Some((8192, 2048)));
        // The byte budget caps the pixel count: 4 bytes a pixel.
        let (w, h) = zoom_raster_size((800, 400), 8, 4 * 1600 * 800).unwrap();
        assert!(w <= 1600 && h <= 800 && w > 800, "{w}x{h}");
        assert!((w as usize) * (h as usize) * 4 <= 4 * 1600 * 800);
        // No room left in the budget: keep the inline raster.
        assert_eq!(zoom_raster_size((800, 400), 8, 4 * 800 * 400), None);
        assert_eq!(zoom_raster_size((800, 400), 8, 0), None);
        assert_eq!(zoom_raster_size((0, 400), 8, big), None);
    }

    #[test]
    fn zoom_raster_is_made_at_the_requested_width() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50"><rect width="100" height="50" fill="red"/></svg>"#;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let handle = runtime
            .block_on(rasterize_zoom(Arc::new(svg.as_bytes().to_vec()), 640))
            .expect("raster");
        match handle {
            image::Handle::Rgba { width, height, .. } => assert_eq!((width, height), (640, 320)),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn init_skipped_when_user_provides_it() {
        assert!(has_user_init(
            "%%{init: { 'theme': 'dark' }}%%\ngraph LR\nA-->B"
        ));
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
            let (rgba, w, h, _, _) = rasterize_for_inline(svg.as_bytes())
                .unwrap_or_else(|e| panic!("raster failed for {src:?}: {e}"));
            assert!(
                w > 0 && h > 0 && !rgba.is_empty(),
                "empty raster for {src:?}"
            );
        }
    }

    #[test]
    fn math_glyph_fill_follows_palette_fg() {
        let svg = render_blocking(DiagramKind::Math, "x", &Palette::ONE_LIGHT, "").unwrap();
        let fg = Palette::ONE_LIGHT.fg;
        let hex = color_to_hex(fg);
        assert!(svg.contains(&hex), "expected fill {hex} in svg, got: {svg}");
    }

    /// Count pixels near `target` among the raster's opaque pixels.
    fn count_near(rgba: &[u8], target: Color) -> usize {
        let want = [
            (target.r * 255.0).round() as i32,
            (target.g * 255.0).round() as i32,
            (target.b * 255.0).round() as i32,
        ];
        rgba.chunks_exact(4)
            .filter(|p| p[3] > 200)
            .filter(|p| (0..3).all(|i| (p[i] as i32 - want[i]).abs() <= 24))
            .count()
    }

    /// Labels must actually paint. They used to vanish entirely because the
    /// SVGs ask for "JetBrains Mono" / "Times, serif", neither of which the OS
    /// has, and usvg's last-resort generic family was also unresolvable — so
    /// every `<text>` element was dropped. Assert real text ink exists for
    /// every diagram kind, at the palette foreground.
    #[test]
    fn every_diagram_kind_paints_text_in_the_palette_foreground() {
        for pal in [Palette::ONE_DARK, Palette::ONE_LIGHT] {
            for (kind, src) in [
                (DiagramKind::Mermaid, "graph LR\n A --> B"),
                (DiagramKind::Mermaid, "sequenceDiagram\n Alice->>Bob: Hi"),
                (DiagramKind::Dot, "digraph G { a -> b }"),
            ] {
                let label = format!("{kind:?}");
                let output = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on(render_blocking_async(
                        kind,
                        src.to_string(),
                        pal,
                        "JetBrains Mono".to_string(),
                    ))
                    .unwrap_or_else(|e| panic!("{label} render failed: {e}"));
                let ink = count_near(&output.rgba, pal.fg);
                assert!(
                    ink > 40,
                    "{label} painted no foreground text ({ink} px) for {:?}",
                    pal.bg
                );
            }
        }
    }

    /// The DOT writer ignores `fontcolor`: its `<text>` elements carry no fill,
    /// so labels paint black and vanish on a dark theme. Guard the fill rule on
    /// every platform, and the family rewrite on the platform that needs it.
    #[test]
    fn dot_labels_are_themed() {
        let pal = Palette::ONE_DARK;
        let svg = render_blocking(
            DiagramKind::Dot,
            "digraph G { a -> b }",
            &pal,
            "JetBrains Mono",
        )
        .unwrap();
        let fg = color_to_hex(color_over(pal.fg, color_over(pal.surface, pal.bg)));
        assert!(
            svg.contains(&format!("text,textPath{{fill:{fg};}}")),
            "no themed label fill in: {svg}"
        );
    }

    /// On Windows the hardcoded `font-family: Times, serif` has to become the
    /// app's editor face, which the raster font database carries. On macOS
    /// `Times` already resolves through the system, so it is deliberately left
    /// alone — rewriting it there would swap a working family for a missing one.
    #[cfg(windows)]
    #[test]
    fn dot_labels_use_the_requested_font_on_windows() {
        let svg = render_blocking(
            DiagramKind::Dot,
            "digraph G { a -> b }",
            &Palette::ONE_DARK,
            "JetBrains Mono",
        )
        .unwrap();
        assert!(
            svg.contains("font-family: \"JetBrains Mono\", sans-serif;"),
            "layout-rs font class not rewritten in: {svg}"
        );
        assert!(
            !svg.contains("Times, serif"),
            "layout-rs hardcoded serif survived in: {svg}"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn dot_labels_keep_the_system_serif_off_windows() {
        let svg = render_blocking(
            DiagramKind::Dot,
            "digraph G { a -> b }",
            &Palette::ONE_DARK,
            "JetBrains Mono",
        )
        .unwrap();
        assert!(
            svg.contains("font-family: Times, serif;"),
            "layout-rs serif was rewritten off Windows: {svg}"
        );
    }

    /// A themed hairline border must not be flattened into solid black/white,
    /// or every DOT node gains a hard opaque ring.
    #[test]
    fn dot_node_borders_composite_their_alpha() {
        let pal = Palette::ONE_LIGHT;
        let preamble = dot_preamble(&pal, &crate::theme::DiagramTheme::NONE, "Inter");
        let border = color_to_hex(color_over(pal.code_border, color_over(pal.surface, pal.bg)));
        assert!(
            preamble.contains(&format!("color=\"{border}\"")),
            "{preamble}"
        );
        assert!(!preamble.contains("#000000"), "alpha discarded: {preamble}");
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

    /// Editing `[diagram]` in a theme file must invalidate the cached render,
    /// so the cache key has to cover the overrides, not just the palette.
    #[test]
    fn theme_id_changes_with_diagram_overrides() {
        let base = theme_id_with(&Palette::ONE_DARK, &crate::theme::DiagramTheme::NONE);
        let themed = crate::theme::DiagramTheme {
            line: Some(Color::from_rgb(1.0, 0.0, 0.0)),
            ..crate::theme::DiagramTheme::NONE
        };
        assert_ne!(base, theme_id_with(&Palette::ONE_DARK, &themed));
    }

    /// A `[diagram]` override must reach the rendered output for both formats.
    #[test]
    fn diagram_overrides_reach_mermaid_and_dot() {
        let theme = crate::theme::DiagramTheme {
            node_fill: Some(Color::from_rgb(1.0, 0.0, 1.0)),
            line: Some(Color::from_rgb(0.0, 1.0, 0.0)),
            text: Some(Color::from_rgb(1.0, 1.0, 0.0)),
            ..crate::theme::DiagramTheme::NONE
        };

        let mermaid = render_blocking_with(
            DiagramKind::Mermaid,
            "graph LR\n A --> B",
            &Palette::ONE_DARK,
            &theme,
            "JetBrains Mono",
        )
        .unwrap();
        assert!(
            mermaid.contains(&color_to_hex(theme.node_fill.unwrap())),
            "node_fill override missing from mermaid: {mermaid}"
        );
        assert!(
            mermaid.contains(&color_to_hex(theme.line.unwrap())),
            "line override missing from mermaid: {mermaid}"
        );

        let dot = render_blocking_with(
            DiagramKind::Dot,
            "digraph G { a -> b }",
            &Palette::ONE_DARK,
            &theme,
            "JetBrains Mono",
        )
        .unwrap();
        assert!(
            dot.contains(&color_to_hex(theme.node_fill.unwrap())),
            "node_fill override missing from dot: {dot}"
        );
        assert!(
            dot.contains(&format!("fill:{};", color_to_hex(theme.text.unwrap()))),
            "text override missing from dot label rule: {dot}"
        );
    }

    /// A user `%%{init}%%` block still wins over the injected theme; overrides
    /// only feed the injection.
    #[test]
    fn diagram_overrides_do_not_touch_a_user_init_block() {
        let theme = crate::theme::DiagramTheme {
            node_fill: Some(Color::from_rgb(1.0, 0.0, 1.0)),
            ..crate::theme::DiagramTheme::NONE
        };
        let source = "%%{init: {'theme':'dark'}}%%\ngraph LR\n A --> B";
        assert!(has_user_init(source));
        let svg = render_blocking_with(
            DiagramKind::Mermaid,
            source,
            &Palette::ONE_DARK,
            &theme,
            "JetBrains Mono",
        )
        .unwrap();
        assert!(
            !svg.contains(&color_to_hex(theme.node_fill.unwrap())),
            "override should not be injected alongside a user init block"
        );
    }

    #[test]
    fn cache_basic() {
        let mut cache = DiagramCache::new(2);
        cache.put((1, 0), DiagramState::Pending);
        cache.put((2, 0), DiagramState::Err("boom".into()));
        assert_eq!(cache.len(), 2);
        assert!(matches!(cache.get(&(1, 0)), Some(DiagramState::Pending)));
    }

    fn ready_state(pixel_bytes: usize) -> DiagramState {
        let px = pixel_bytes / 4;
        DiagramState::Ready {
            inline: image::Handle::from_rgba(px as u32, 1, vec![0u8; pixel_bytes]),
            source_bytes: Arc::new(Vec::new()),
            device_w: px as u32,
            figure_w: px as f32 / RASTER_SCALE,
            figure_h: 1.0 / RASTER_SCALE,
        }
    }

    #[test]
    fn cache_tracks_byte_cost() {
        let mut cache = DiagramCache::with_byte_budget(8, 1024);
        cache.put((1, 0), ready_state(400));
        cache.put((2, 0), ready_state(400));
        assert_eq!(cache.cost_bytes(), 800);
        // Replacing a key swaps its cost instead of double-counting.
        cache.put((1, 0), ready_state(200));
        assert_eq!(cache.cost_bytes(), 600);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn cache_remove_tracks_byte_cost() {
        let mut cache = DiagramCache::with_byte_budget(8, usize::MAX);
        cache.put((1, 0), ready_state(400));
        cache.put((2, 0), ready_state(200));
        assert_eq!(cache.cost_bytes(), 600);
        assert!(cache.remove(&(1, 0)).is_some());
        assert_eq!(cache.cost_bytes(), 200);
        assert!(cache.remove(&(1, 0)).is_none());
        assert_eq!(cache.cost_bytes(), 200);
    }

    #[test]
    fn cache_evicts_lru_when_over_byte_budget() {
        let mut cache = DiagramCache::with_byte_budget(8, 1000);
        cache.put((1, 0), ready_state(400));
        cache.put((2, 0), ready_state(400));
        // Third insert exceeds the budget: (1,0) is least recently used, drops.
        cache.put((3, 0), ready_state(400));
        assert!(cache.peek(&(1, 0)).is_none());
        assert!(cache.peek(&(2, 0)).is_some());
        assert!(cache.peek(&(3, 0)).is_some());
        assert_eq!(cache.cost_bytes(), 800);
    }

    #[test]
    fn cache_never_evicts_just_inserted_entry() {
        // Single oversized entry stays even though it busts the budget.
        let mut cache = DiagramCache::with_byte_budget(8, 100);
        cache.put((1, 0), ready_state(4000));
        assert!(cache.peek(&(1, 0)).is_some());
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn cache_budget_eviction_spares_pending_sentinels() {
        let mut cache = DiagramCache::with_byte_budget(8, 1000);
        cache.put((1, 0), DiagramState::Pending);
        // Busts the budget with the Pending entry at the LRU end: the loop
        // must stop rather than orphan the in-flight render.
        cache.put((2, 0), ready_state(2000));
        assert!(cache.peek(&(1, 0)).is_some(), "in-flight Pending survives");
        assert!(cache.peek(&(2, 0)).is_some());
    }

    #[test]
    fn cache_count_cap_eviction_keeps_byte_tracking_exact() {
        let mut cache = DiagramCache::with_byte_budget(2, usize::MAX);
        cache.put((1, 0), ready_state(400));
        cache.put((2, 0), ready_state(400));
        cache.put((3, 0), ready_state(400)); // count cap evicts (1,0)
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.cost_bytes(), 800);
    }

    #[test]
    fn oversized_source_rejected() {
        let big = "x".repeat(MAX_SOURCE_BYTES + 1);
        let err = render_blocking(DiagramKind::Mermaid, &big, &palette(), "system-ui").unwrap_err();
        assert!(err.contains("too large"), "got: {err}");
    }
}
