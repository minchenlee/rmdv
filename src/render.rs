use crate::app::{ImageCache, ImageState, Message, ReaderTarget};
use crate::ast::{Block, BlockId, Inline, ListItem};
use crate::diagram::{DiagramCache, DiagramState};
use crate::keyed_body::{KeyedBody, RowKey};
use crate::select_scope::TargetMark;
use crate::theme::{Palette, Typography};
use iced::advanced::widget::{tree, Operation, Tree, Widget};
use iced::advanced::{layout, mouse, overlay, renderer, Clipboard, Layout, Shell};
use iced::widget::{
    container, image as image_widget, mouse_area, row, scrollable, span, stack, svg as svg_widget,
    text, tooltip, Column, Space,
};
use iced::{Background, Element, Event, Length, Padding, Rectangle, Size, Vector};
use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

pub fn block_anchor_id(id: BlockId) -> iced::widget::Id {
    iced::widget::Id::from(format!("block-anchor-{}", id.0))
}

#[derive(Clone, Default)]
pub struct Highlight {
    pub query: String,
    pub current_block: Option<usize>,
    pub current_in_block: usize,
}

/// How wide `render` lets its content column grow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadingWidth {
    /// Main reading area: capped at the reading column (`READING_MAX_PX`).
    Capped,
    /// Side panels: fill whatever width the panel gives.
    Fill,
}

const READING_MAX: f32 = crate::theme::READING_MAX_PX;

fn reading_max_width(width: ReadingWidth) -> f32 {
    match width {
        ReadingWidth::Capped => READING_MAX,
        ReadingWidth::Fill => f32::INFINITY,
    }
}

pub fn render<'a>(
    blocks: &'a [(BlockId, Block)],
    pal: &Palette,
    typ: &Typography,
    hl: &Highlight,
    virt: Option<&crate::virt::VirtWindow>,
    image_cache: &'a ImageCache,
    current_file: Option<&'a Path>,
    folded: &std::collections::HashSet<BlockId>,
    hovered_heading: Option<BlockId>,
    diagram_cache: &'a DiagramCache,
    diagram_theme_id: u32,
    keyed_widget_reuse: bool,
    keyed_widget_generation: (u64, u64),
    recently_scrolled: bool,
    reading_width: ReadingWidth,
    copied: Option<usize>,
) -> Element<'a, Message> {
    let img_ctx = ImgCtx {
        cache: image_cache,
        current_file,
        diagram_cache,
        diagram_theme_id,
        recently_scrolled,
        copied,
    };
    // Which blocks to materialize: the precomputed fold-aware window (body
    // view; built in `App::update`, never here), or the full fold-aware list
    // (mindmap panel, small docs). Spacers stand in for off-window blocks so
    // scrollbar geometry matches `VirtWindow`'s prefix sums.
    let local_display: Vec<usize>;
    let (display, (start, end)) = match virt {
        Some(w) => (w.display.as_slice(), w.range),
        None => {
            local_display = crate::virt::display_list(blocks, folded);
            let n = local_display.len();
            (local_display.as_slice(), (0, n))
        }
    };
    // Rows are always wrapped in `KeyedBody`. Normal document callers retain
    // state by BlockId; preview callers use fresh-diff mode, which clears the
    // inner child trees only when this materialized range/document namespace
    // changes. That keeps the tag-checked boundary while avoiding state reuse
    // across rapid preview transitions.
    let mut keys: Vec<RowKey> = Vec::with_capacity(end - start + 2);
    let mut col = Column::with_capacity(end - start + 2).spacing(crate::virt::BLOCK_GAP_PX);
    if let Some(h) = virt.and_then(|w| w.top_spacer()) {
        keys.push(RowKey::TopSpacer);
        col = col.push(Space::new().height(h));
    }
    let first = start.min(display.len());
    for (dpos, &idx) in (first..).zip(&display[first..end.min(display.len())]) {
        let (id, b) = &blocks[idx];
        let local = if hl.current_block == Some(idx) {
            Some(hl.current_in_block)
        } else {
            None
        };
        let el: Element<'a, Message> = if let Block::Heading { level, .. } = b {
            let is_folded = folded.contains(id);
            let show_chev = is_folded || hovered_heading == Some(*id);
            // Like Notion, the document's first block gets no space above.
            let space_above = if dpos == 0 {
                0.0
            } else {
                typ.heading_space_above(*level)
            };
            let id = *id;
            TargetMark::new(
                move || ReaderTarget::Heading(id),
                render_heading_with_chevron(
                    id,
                    b,
                    pal,
                    typ,
                    &hl.query,
                    local,
                    is_folded,
                    show_chev,
                    space_above,
                ),
            )
            .into()
        } else {
            container(render_block(b, pal, typ, &hl.query, local, &img_ctx))
                .id(block_anchor_id(*id))
                .into()
        };
        keys.push(RowKey::Block(id.0));
        col = col.push(crate::select_scope::BlockMark::new(idx, el));
    }
    if let Some(h) = virt.and_then(|w| w.bottom_spacer()) {
        keys.push(RowKey::BottomSpacer);
        col = col.push(Space::new().height(h));
    }

    // Reading column cap: READING_MAX_PX (theme.rs).
    let _ = typ.measure_ch;
    let body: Element<'a, Message> = if keyed_widget_reuse {
        KeyedBody::new(keys, col).into()
    } else {
        KeyedBody::new_fresh(keys, col, keyed_widget_generation).into()
    };
    container(crate::select_scope::SelectScope::new(blocks, folded, body))
        .max_width(reading_max_width(reading_width))
        .into()
}

/// Side of the square that a heading's fold chevron answers clicks in.
const FOLD_CHEVRON_HIT: f32 = 24.0;

pub(crate) fn render_heading_with_chevron<'a>(
    id: BlockId,
    b: &'a Block,
    pal: &Palette,
    typ: &Typography,
    query: &str,
    current_in_block: Option<usize>,
    folded: bool,
    visible: bool,
    space_above: f32,
) -> Element<'a, Message> {
    let cache = EMPTY_IMG_CACHE.get_or_init(ImageCache::default);
    let dcache = EMPTY_DIAGRAM_CACHE.get_or_init(|| DiagramCache::new(1));
    let img = ImgCtx {
        cache,
        current_file: None,
        diagram_cache: dcache,
        diagram_theme_id: 0,
        recently_scrolled: false,
        copied: None,
    };
    let head = render_block(b, pal, typ, query, current_in_block, &img);
    let glyph = if folded {
        crate::icon::ic::CHEVRON_RIGHT
    } else {
        crate::icon::ic::CHEVRON_DOWN
    };
    let color = if visible {
        pal.muted
    } else {
        iced::Color::TRANSPARENT
    };
    // Only the chevron folds; the rest of the row is text to select.
    let chev = mouse_area(
        container(crate::icon::glyph(glyph, 14.0, color))
            .center_x(Length::Fixed(FOLD_CHEVRON_HIT))
            .center_y(Length::Fixed(FOLD_CHEVRON_HIT)),
    )
    .interaction(iced::mouse::Interaction::Pointer)
    .on_press(Message::ToggleFold(id));
    let action = if folded {
        "Expand section"
    } else {
        "Collapse section"
    };
    let chev = crate::app::reader_tip(chev, action, tooltip::Position::Left, *pal);
    let chev_layer = container(chev)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Right)
        .align_y(iced::alignment::Vertical::Center);
    let stacked = stack![container(head).width(Length::Fill), chev_layer];
    container(
        mouse_area(stacked)
            .on_enter(Message::HeadingHoverEnter(id))
            .on_exit(Message::HeadingHoverExit(id)),
    )
    // Inside the anchored container, so measured heights include the space.
    .padding(Padding {
        top: space_above,
        ..Padding::ZERO
    })
    .id(block_anchor_id(id))
    .into()
}

static EMPTY_IMG_CACHE: std::sync::OnceLock<ImageCache> = std::sync::OnceLock::new();
static EMPTY_DIAGRAM_CACHE: std::sync::OnceLock<DiagramCache> = std::sync::OnceLock::new();

pub fn data_view<'a>(
    code: &'a str,
    _spans: &'a [crate::ast::HlSpan],
    pal: &Palette,
    typ: &Typography,
) -> Element<'a, Message> {
    let lang = detect_data_lang(code);
    let colored = colorize_data(lang, code, pal);
    let mut out: Vec<RtSpan<'a>> = Vec::new();
    for (range, color) in colored {
        let slice = &code[range];
        out.push(
            span(slice)
                .color(color)
                .font(iced::Font::with_name("JetBrains Mono"))
                .size(typ.code_size)
                .line_height(iced::widget::text::LineHeight::Relative(1.55)),
        );
    }
    container(rich_text_links(out, TextFills::of(pal)))
        .padding(Padding::from([28, 32]))
        .width(Length::Fill)
        .into()
}

/// Owned-string variant of `data_view` for callers that compute the code on the
/// fly (mindmap data-doc leaf panel) and need a `'static` element. Spans own
/// their text so the returned element borrows nothing.
pub fn data_view_owned(code: String, pal: &Palette, typ: &Typography) -> Element<'static, Message> {
    let lang = detect_data_lang(&code);
    let colored = colorize_data(lang, &code, pal);
    let mut out: Vec<RtSpan<'static>> = Vec::new();
    for (range, color) in colored {
        let slice = code[range].to_string();
        out.push(
            span(slice)
                .color(color)
                .font(iced::Font::with_name("JetBrains Mono"))
                .size(typ.code_size)
                .line_height(iced::widget::text::LineHeight::Relative(1.55)),
        );
    }
    container(rich_text_links(out, TextFills::of(pal)))
        .padding(Padding::from([28, 32]))
        .width(Length::Fill)
        .into()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DataLang {
    Json,
    Yaml,
    Toml,
}

fn detect_data_lang(code: &str) -> DataLang {
    let t = code.trim_start();
    if t.starts_with('{') || t.starts_with('[') {
        return DataLang::Json;
    }
    if code.lines().any(|l| {
        let lt = l.trim_start();
        lt.starts_with('[') && lt.contains(']')
    }) && !code.contains(':')
    {
        return DataLang::Toml;
    }
    if code.contains(" = ") || code.lines().any(|l| l.trim_start().starts_with('[')) {
        return DataLang::Toml;
    }
    DataLang::Yaml
}

fn depth_palette(pal: &Palette) -> [iced::Color; 6] {
    let s = pal.syntax;
    [
        s.function, s.keyword, s.type_, s.constant, s.variable, s.operator,
    ]
}

fn colorize_data(
    lang: DataLang,
    code: &str,
    pal: &Palette,
) -> Vec<(std::ops::Range<usize>, iced::Color)> {
    match lang {
        DataLang::Json => colorize_json(code, pal),
        DataLang::Yaml => colorize_yaml(code, pal),
        DataLang::Toml => colorize_toml(code, pal),
    }
}

fn color_for_value(text: &str, pal: &Palette) -> iced::Color {
    let s = pal.syntax;
    let t = text.trim();
    if t == "true" || t == "false" || t == "null" || t == "~" {
        s.constant
    } else if t.parse::<f64>().is_ok() {
        s.number
    } else if (t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\''))
    {
        s.string
    } else if t.is_empty() {
        pal.fg
    } else {
        s.string
    }
}

fn colorize_json(code: &str, pal: &Palette) -> Vec<(std::ops::Range<usize>, iced::Color)> {
    let depths = depth_palette(pal);
    let s = pal.syntax;
    let mut out: Vec<(std::ops::Range<usize>, iced::Color)> = Vec::new();
    let bytes = code.as_bytes();
    let mut i = 0usize;
    let mut depth: i32 = -1;
    let mut expect_key = true;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b'{' | b'[' => {
                depth += 1;
                out.push((i..i + 1, s.punctuation));
                expect_key = c == b'{';
                i += 1;
            }
            b'}' | b']' => {
                out.push((i..i + 1, s.punctuation));
                depth -= 1;
                expect_key = false;
                i += 1;
            }
            b',' => {
                out.push((i..i + 1, s.punctuation));
                expect_key = true;
                i += 1;
            }
            b':' => {
                out.push((i..i + 1, s.punctuation));
                expect_key = false;
                i += 1;
            }
            b'"' => {
                let start = i;
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' && i + 1 < bytes.len() {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == b'"' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                let color = if expect_key {
                    let d = depth.max(0) as usize;
                    depths[d % depths.len()]
                } else {
                    s.string
                };
                out.push((start..i, color));
            }
            b' ' | b'\t' | b'\n' | b'\r' => {
                let start = i;
                while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r') {
                    i += 1;
                }
                out.push((start..i, pal.fg));
            }
            _ => {
                let start = i;
                while i < bytes.len()
                    && !matches!(
                        bytes[i],
                        b'{' | b'}'
                            | b'['
                            | b']'
                            | b','
                            | b':'
                            | b' '
                            | b'\t'
                            | b'\n'
                            | b'\r'
                            | b'"'
                    )
                {
                    i += 1;
                }
                let tok = &code[start..i];
                let col = if tok == "true" || tok == "false" || tok == "null" {
                    s.constant
                } else if tok.parse::<f64>().is_ok() {
                    s.number
                } else {
                    pal.fg
                };
                out.push((start..i, col));
            }
        }
    }
    out
}

fn colorize_yaml(code: &str, pal: &Palette) -> Vec<(std::ops::Range<usize>, iced::Color)> {
    let depths = depth_palette(pal);
    let s = pal.syntax;
    let mut out: Vec<(std::ops::Range<usize>, iced::Color)> = Vec::new();
    let mut offset = 0usize;
    for line in code.split_inclusive('\n') {
        let line_start = offset;
        let line_len = line.len();
        offset += line_len;

        let lead_ws: usize = line.chars().take_while(|c| *c == ' ').count();
        let after_ws_byte = line_start + lead_ws;
        if lead_ws > 0 {
            out.push((line_start..after_ws_byte, pal.fg));
        }
        let body = &line[lead_ws..];
        let body_trim = body.trim_end_matches(['\n', '\r']);
        let trailing_nl = body.len() - body_trim.len();

        if body_trim.is_empty() {
            if trailing_nl > 0 {
                out.push((after_ws_byte..line_start + line_len, pal.fg));
            }
            continue;
        }
        if body_trim.starts_with('#') {
            out.push((after_ws_byte..after_ws_byte + body_trim.len(), s.comment));
            if trailing_nl > 0 {
                out.push((
                    after_ws_byte + body_trim.len()..line_start + line_len,
                    pal.fg,
                ));
            }
            continue;
        }

        let depth = lead_ws / 2;
        let key_color = depths[depth % depths.len()];

        if let Some(rest) = body_trim.strip_prefix("- ") {
            out.push((after_ws_byte..after_ws_byte + 1, s.punctuation));
            out.push((after_ws_byte + 1..after_ws_byte + 2, pal.fg));
            let rest_start = after_ws_byte + 2;
            push_yaml_kv_or_value(rest, rest_start, key_color, pal, &mut out);
        } else {
            push_yaml_kv_or_value(body_trim, after_ws_byte, key_color, pal, &mut out);
        }
        if trailing_nl > 0 {
            let end = line_start + line_len;
            out.push((end - trailing_nl..end, pal.fg));
        }
    }
    out
}

fn push_yaml_kv_or_value(
    body: &str,
    body_start: usize,
    key_color: iced::Color,
    pal: &Palette,
    out: &mut Vec<(std::ops::Range<usize>, iced::Color)>,
) {
    let s = pal.syntax;
    if let Some(colon_idx) = body.find(':') {
        let after = &body[colon_idx + 1..];
        if after.is_empty() || after.starts_with(' ') {
            let key_end = body_start + colon_idx;
            out.push((body_start..key_end, key_color));
            out.push((key_end..key_end + 1, s.punctuation));
            let val = &body[colon_idx + 1..];
            let val_start = key_end + 1;
            let val_trim_start = val.len() - val.trim_start().len();
            if val_trim_start > 0 {
                out.push((val_start..val_start + val_trim_start, pal.fg));
            }
            let val_body = val.trim_start();
            if !val_body.is_empty() {
                let vs = val_start + val_trim_start;
                out.push((vs..vs + val_body.len(), color_for_value(val_body, pal)));
            }
            return;
        }
    }
    out.push((
        body_start..body_start + body.len(),
        color_for_value(body, pal),
    ));
}

fn colorize_toml(code: &str, pal: &Palette) -> Vec<(std::ops::Range<usize>, iced::Color)> {
    let depths = depth_palette(pal);
    let s = pal.syntax;
    let mut out: Vec<(std::ops::Range<usize>, iced::Color)> = Vec::new();
    let mut offset = 0usize;
    for line in code.split_inclusive('\n') {
        let line_start = offset;
        let line_len = line.len();
        offset += line_len;

        let body = line.trim_end_matches(['\n', '\r']);
        let trailing_nl = line.len() - body.len();
        let trimmed_start = body.len() - body.trim_start().len();
        if trimmed_start > 0 {
            out.push((line_start..line_start + trimmed_start, pal.fg));
        }
        let content = body.trim_start();
        let content_start = line_start + trimmed_start;

        if content.is_empty() {
            if trailing_nl > 0 {
                out.push((content_start..line_start + line_len, pal.fg));
            }
            continue;
        }
        if content.starts_with('#') {
            out.push((content_start..content_start + content.len(), s.comment));
            if trailing_nl > 0 {
                out.push((content_start + content.len()..line_start + line_len, pal.fg));
            }
            continue;
        }
        if content.starts_with('[') {
            let depth = content.bytes().take_while(|b| *b == b'[').count();
            let color = depths[(depth - 1) % depths.len()];
            out.push((content_start..content_start + content.len(), color));
            if trailing_nl > 0 {
                out.push((content_start + content.len()..line_start + line_len, pal.fg));
            }
            continue;
        }
        if let Some(eq_idx) = content.find('=') {
            let key = &content[..eq_idx].trim_end();
            let key_end = content_start + key.len();
            out.push((content_start..key_end, depths[0]));
            let between_end = content_start + eq_idx + 1;
            if between_end > key_end {
                out.push((key_end..between_end, s.punctuation));
            }
            let val = &content[eq_idx + 1..];
            let val_start = content_start + eq_idx + 1;
            let val_trim_start = val.len() - val.trim_start().len();
            if val_trim_start > 0 {
                out.push((val_start..val_start + val_trim_start, pal.fg));
            }
            let val_body = val.trim_start();
            if !val_body.is_empty() {
                let vs = val_start + val_trim_start;
                out.push((vs..vs + val_body.len(), color_for_value(val_body, pal)));
            }
        } else {
            out.push((content_start..content_start + content.len(), pal.fg));
        }
        if trailing_nl > 0 {
            let end = line_start + line_len;
            out.push((end - trailing_nl..end, pal.fg));
        }
    }
    out
}

#[derive(Clone, Copy)]
struct ImgCtx<'a> {
    cache: &'a ImageCache,
    current_file: Option<&'a Path>,
    diagram_cache: &'a DiagramCache,
    diagram_theme_id: u32,
    recently_scrolled: bool,
    /// The key ([`copy_key`]) of the block whose copy button copied last,
    /// while its check shows.
    copied: Option<usize>,
}

fn render_block<'a>(
    b: &'a Block,
    pal: &Palette,
    typ: &Typography,
    query: &str,
    current_in_block: Option<usize>,
    img: &ImgCtx<'a>,
) -> Element<'a, Message> {
    let mut ctx = HlCtx {
        query,
        lower_query: query.to_lowercase(),
        counter: 0,
        current_in_block,
        pal: *pal,
        recently_scrolled: img.recently_scrolled,
    };
    match b {
        Block::Heading { level, inlines, .. } => {
            let spans = inline_spans(inlines, pal, typ.heading_size(*level), &mut ctx);
            rich_text_spaced(spans, crate::theme::HEADING_LINE_HEIGHT, TextFills::of(pal))
        }
        Block::Paragraph(inlines) => {
            let spans = inline_spans(inlines, pal, typ.body_size, &mut ctx);
            rich_text_spaced(
                spans,
                paragraph_line_height(inlines, typ.line_height),
                TextFills::of(pal),
            )
        }
        Block::CodeBlock { code, spans, .. } => {
            let pal_c = *pal;
            let mut out: Vec<RtSpan<'a>> = Vec::new();
            let mut cursor = 0usize;
            for s in spans {
                if s.range.start < cursor || s.range.end > code.len() {
                    continue;
                }
                if s.range.start > cursor {
                    let slice = &code[cursor..s.range.start];
                    push_code_with_hl(slice, pal_c.fg, pal, typ.code_size, &mut out, &mut ctx);
                }
                let color = style_color(s.style, pal);
                let slice = &code[s.range.start..s.range.end];
                push_code_with_hl(slice, color, pal, typ.code_size, &mut out, &mut ctx);
                cursor = s.range.end;
            }
            if cursor < code.len() {
                push_code_with_hl(
                    &code[cursor..],
                    pal_c.fg,
                    pal,
                    typ.code_size,
                    &mut out,
                    &mut ctx,
                );
            }
            // The right padding keeps every line clear of the copy button.
            let body = container(rich_text_links(out, TextFills::of(&pal_c)))
                .padding(Padding {
                    top: 10.0,
                    right: COPY_BUTTON_CLEARANCE,
                    bottom: 10.0,
                    left: 12.0,
                })
                .style(move |_| container::Style {
                    background: Some(pal_c.code_bg.into()),
                    border: iced::Border {
                        color: pal_c.code_border,
                        width: 1.0,
                        radius: crate::theme::radius::MD.into(),
                    },
                    ..Default::default()
                })
                .width(Length::Fill);
            let copied = img.copied == Some(copy_key(code));
            let copy_btn = copy_button(code, "Copy Code", copied, &pal_c);
            TargetMark::new(
                move || ReaderTarget::Code(code.clone()),
                HoverReveal::new(body, copy_btn).pinned(copied),
            )
            .into()
        }
        Block::Blockquote(blocks) => {
            let inner = blocks.iter().fold(Column::new().spacing(8), |c, b| {
                c.push(render_block(b, pal, typ, query, current_in_block, img))
            });
            let pal_q = *pal;
            // The left rule is a 1.5 px layer over the quote's left edge.
            // A shadow behind a transparent container looks the same on
            // wgpu, but tiny-skia fills the whole shadow rectangle.
            let rule = container(Space::new().width(1.5).height(Length::Fill)).style(move |_| {
                container::Style {
                    background: Some(pal_q.accent.into()),
                    ..Default::default()
                }
            });
            stack![
                container(inner)
                    .padding(Padding {
                        top: 2.0,
                        right: 14.0,
                        bottom: 2.0,
                        left: 17.0,
                    })
                    .width(Length::Fill),
                rule,
            ]
            .into()
        }
        Block::Diagram { source, hash, kind } => {
            if matches!(kind, crate::ast::DiagramKind::Math) {
                TargetMark::new(
                    move || ReaderTarget::Math(source.clone()),
                    render_math_block(*hash, source, pal, typ, img),
                )
                .into()
            } else {
                let hash = *hash;
                TargetMark::new(
                    move || ReaderTarget::Diagram(hash),
                    render_diagram(hash, source, pal, typ, img),
                )
                .into()
            }
        }
        Block::List { ordered, items } => render_list(*ordered, items, pal, typ, &mut ctx, img),
        Block::Table { headers, rows } => render_table(headers, rows, pal, typ, &mut ctx),
        Block::Image { url, alt, link } => {
            let current_file = img.current_file;
            TargetMark::new(
                move || ReaderTarget::Image {
                    key: image_key(url, current_file),
                    link: link.clone(),
                },
                render_image(url, alt, link.as_deref(), pal, img),
            )
            .into()
        }
        Block::Rule => {
            let pal_r = *pal;
            container(Space::new().height(1.0))
                .width(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(pal_r.rule.into()),
                    ..Default::default()
                })
                .into()
        }
    }
}

type RtSpan<'a> = iced::advanced::text::Span<'a, Message, iced::Font>;

/// `Inter` and the bundled monospace font deliberately do not contain CJK
/// glyphs. cosmic-text can normally find a system fallback, but it matches
/// fallback faces against the requested style/family first. On macOS that
/// means an `Inter Italic` CJK run has no matching PingFang face, and a
/// monospace CJK run has no matching monospace face, so both become tofu.
/// Keep CJK runs on the normal reader family for fallback while preserving
/// the requested style for Latin and other scripts.
fn is_cjk_fallback_char(c: char) -> bool {
    matches!(
        c as u32,
        0x1100..=0x11FF // Hangul Jamo
            | 0x2E80..=0x2FFF // CJK radicals, symbols, and punctuation
            | 0x3000..=0x303F // CJK symbols and punctuation
            | 0x3040..=0x30FF // Hiragana and Katakana
            | 0x3100..=0x312F // Bopomofo
            | 0x3130..=0x318F // Hangul compatibility Jamo
            | 0x31A0..=0x31BF // Bopomofo extensions
            | 0x3400..=0x4DBF // CJK unified ideographs extension A
            | 0x4E00..=0x9FFF // CJK unified ideographs
            | 0xA960..=0xA97F // Hangul Jamo extended-A
            | 0xAC00..=0xD7AF // Hangul syllables
            | 0xD7B0..=0xD7FF // Hangul Jamo extended-B
            | 0xF900..=0xFAFF // CJK compatibility ideographs
            | 0xFF00..=0xFFEF // Fullwidth and halfwidth forms
            | 0x20000..=0x2FA1F // CJK unified ideographs extensions
    )
}

/// Punctuation that Latin text also uses but CJK text sets in the CJK face
/// (centered ellipsis, full-width dashes). It joins a CJK run only when
/// it touches CJK text, so an English "wait… no" keeps Inter's glyph.
fn is_cjk_context_char(c: char) -> bool {
    matches!(c, '\u{2014}' | '\u{2015}' | '\u{2026}')
}

/// Visit contiguous CJK/non-CJK runs without splitting a UTF-8 character.
fn for_text_runs<'a>(text_str: &'a str, mut visit: impl FnMut(&'a str, bool)) {
    let chars: Vec<(usize, char)> = text_str.char_indices().collect();
    let mut cjk: Vec<bool> = chars
        .iter()
        .map(|&(_, c)| is_cjk_fallback_char(c))
        .collect();
    // A context char follows its neighbours: forward pass, then backward pass
    // so a run like "……" before CJK text resolves too.
    for i in 1..chars.len() {
        if is_cjk_context_char(chars[i].1) && cjk[i - 1] {
            cjk[i] = true;
        }
    }
    for i in (0..chars.len().saturating_sub(1)).rev() {
        if is_cjk_context_char(chars[i].1) && cjk[i + 1] {
            cjk[i] = true;
        }
    }

    let mut run_start = 0usize;
    let mut run_is_cjk: Option<bool> = None;
    for (n, &(idx, _)) in chars.iter().enumerate() {
        let is_cjk = cjk[n];
        match run_is_cjk {
            None => run_is_cjk = Some(is_cjk),
            Some(previous) if previous != is_cjk => {
                visit(&text_str[run_start..idx], previous);
                run_start = idx;
                run_is_cjk = Some(is_cjk);
            }
            Some(_) => {}
        }
    }

    if run_start < text_str.len() {
        visit(
            &text_str[run_start..],
            run_is_cjk.expect("a non-empty text run has a classification"),
        );
    }
}

use crate::selectable::{rich_text_links, rich_text_spaced, TextFills};

/// CJK body text reads best at about 1.75 em leading where Latin uses 1.5
/// (Notion default), because ideographs fill the whole em box. The scale is
/// applied to the user's current line height, so Compact/Default/Relaxed keep
/// their proportions.
const CJK_LINE_HEIGHT_SCALE: f32 = 1.75 / 1.5;

fn inlines_have_cjk(inlines: &[Inline]) -> bool {
    inlines.iter().any(|i| match i {
        Inline::Text(t) | Inline::Code(t) => t.chars().any(is_cjk_fallback_char),
        Inline::Emph(c) | Inline::Strong(c) | Inline::Strike(c) => inlines_have_cjk(c),
        Inline::Link { children, .. } => inlines_have_cjk(children),
    })
}

/// Line height for a paragraph: `base` for Latin-only text, scaled up when
/// the paragraph holds any CJK character.
fn paragraph_line_height(inlines: &[Inline], base: f32) -> f32 {
    if inlines_have_cjk(inlines) {
        base * CJK_LINE_HEIGHT_SCALE
    } else {
        base
    }
}

struct HlCtx<'a> {
    query: &'a str,
    /// `query.to_lowercase()` computed once per block, not per span.
    lower_query: String,
    counter: usize,
    current_in_block: Option<usize>,
    pal: Palette,
    recently_scrolled: bool,
}

fn inline_spans<'a>(
    inlines: &'a [Inline],
    pal: &Palette,
    size: f32,
    ctx: &mut HlCtx<'_>,
) -> Vec<RtSpan<'a>> {
    inline_spans_with_style(inlines, pal, size, Style::default(), ctx)
}

fn inline_spans_with_style<'a>(
    inlines: &'a [Inline],
    pal: &Palette,
    size: f32,
    style: Style,
    ctx: &mut HlCtx<'_>,
) -> Vec<RtSpan<'a>> {
    let mut out = Vec::new();
    for i in inlines {
        push_span(i, &mut out, pal, size, style.clone(), ctx);
    }
    out
}

#[derive(Clone, Default)]
struct Style {
    italic: bool,
    bold: bool,
    strike: bool,
    link: Option<String>,
}

fn make_span<'a>(
    text_str: &'a str,
    pal: &Palette,
    size: f32,
    st: &Style,
    code: bool,
    cjk: bool,
    narrow: bool,
    bg: Option<iced::Color>,
) -> RtSpan<'a> {
    let mut font = if narrow {
        iced::Font::with_name(crate::fonts::PUNCT_FAMILY)
    } else if cjk {
        iced::Font::with_name(crate::fonts::CJK_FAMILY)
    } else if code {
        iced::Font::MONOSPACE
    } else {
        iced::Font::with_name("Inter")
    };
    // CJK fonts generally do not ship italic faces. Asking cosmic-text for
    // one prevents it from selecting the normal CJK fallback face, so keep
    // the CJK glyphs upright and let Latin/other scripts carry the italic
    // styling.
    if st.italic && !cjk {
        font.style = iced::font::Style::Italic;
    }
    if st.bold {
        font.weight = iced::font::Weight::Bold;
    }
    let mut s = span(text_str).size(size).font(font);
    if st.strike {
        s = s.strikethrough(true);
    }
    if let Some(c) = bg {
        s = s.background(c);
    } else if code {
        s = s.background(pal.chip()).border(iced::Border {
            radius: crate::theme::radius::XS.into(),
            ..Default::default()
        });
    }
    if let Some(url) = &st.link {
        s = s
            .color(pal.accent)
            .underline(true)
            .link(Message::OpenLink(url.clone()));
    } else {
        s = s.color(pal.fg);
    }
    s
}

fn push_styled_runs<'a>(
    text_str: &'a str,
    pal: &Palette,
    size: f32,
    st: &Style,
    code: bool,
    bg: Option<iced::Color>,
    out: &mut Vec<RtSpan<'a>>,
) {
    for_text_runs(text_str, |run, cjk| {
        if cjk {
            for (part, narrow) in crate::fonts::squeeze_segments(run) {
                out.push(make_span(part, pal, size, st, code, true, narrow, bg));
            }
        } else {
            out.push(make_span(run, pal, size, st, code, false, false, bg));
        }
    });
}

fn push_text_with_hl<'a>(
    text_str: &'a str,
    pal: &Palette,
    size: f32,
    st: &Style,
    monospace: bool,
    out: &mut Vec<RtSpan<'a>>,
    ctx: &mut HlCtx<'_>,
) {
    if ctx.query.is_empty() {
        push_styled_runs(text_str, pal, size, st, monospace, None, out);
        return;
    }
    let lower_text = text_str.to_lowercase();
    let lower_q = ctx.lower_query.as_str();
    let mut cursor = 0usize;
    while let Some(rel) = lower_text[cursor..].find(&lower_q) {
        let abs = cursor + rel;
        if abs > cursor {
            push_styled_runs(&text_str[cursor..abs], pal, size, st, monospace, None, out);
        }
        let end = abs + lower_q.len();
        let is_current = ctx.current_in_block == Some(ctx.counter);
        let bg = if is_current {
            ctx.pal.match_current_bg
        } else {
            ctx.pal.match_bg
        };
        push_styled_runs(&text_str[abs..end], pal, size, st, monospace, Some(bg), out);
        ctx.counter += 1;
        cursor = end;
    }
    if cursor < text_str.len() {
        push_styled_runs(&text_str[cursor..], pal, size, st, monospace, None, out);
    }
}

fn push_code_runs<'a>(
    text_str: &'a str,
    color: iced::Color,
    size: f32,
    bg: Option<iced::Color>,
    out: &mut Vec<RtSpan<'a>>,
) {
    for_text_runs(text_str, |run, cjk| {
        let mut s = span(run)
            .font(if cjk {
                // The bundled CJK face; no platform has a universal CJK mono
                // face that can be relied on here.
                iced::Font::with_name(crate::fonts::CJK_FAMILY)
            } else {
                iced::Font::MONOSPACE
            })
            .size(size)
            .color(color);
        if let Some(c) = bg {
            s = s.background(c);
        }
        out.push(s);
    });
}

fn push_code_with_hl<'a>(
    text_str: &'a str,
    color: iced::Color,
    pal: &Palette,
    size: f32,
    out: &mut Vec<RtSpan<'a>>,
    ctx: &mut HlCtx<'_>,
) {
    if ctx.query.is_empty() {
        push_code_runs(text_str, color, size, None, out);
        return;
    }
    let lower_text = text_str.to_lowercase();
    let lower_q = ctx.lower_query.as_str();
    let mut cursor = 0usize;
    while let Some(rel) = lower_text[cursor..].find(&lower_q) {
        let abs = cursor + rel;
        if abs > cursor {
            push_code_runs(&text_str[cursor..abs], color, size, None, out);
        }
        let end = abs + lower_q.len();
        let is_current = ctx.current_in_block == Some(ctx.counter);
        let bg = if is_current {
            ctx.pal.match_current_bg
        } else {
            ctx.pal.match_bg
        };
        push_code_runs(&text_str[abs..end], color, size, Some(bg), out);
        ctx.counter += 1;
        cursor = end;
    }
    if cursor < text_str.len() {
        push_code_runs(&text_str[cursor..], color, size, None, out);
    }
    let _ = pal;
}

fn push_span<'a>(
    i: &'a Inline,
    out: &mut Vec<RtSpan<'a>>,
    pal: &Palette,
    size: f32,
    st: Style,
    ctx: &mut HlCtx<'_>,
) {
    match i {
        Inline::Text(t) => push_text_with_hl(t.as_str(), pal, size, &st, false, out, ctx),
        Inline::Code(t) => push_text_with_hl(t.as_str(), pal, size, &st, true, out, ctx),
        Inline::Emph(c) => {
            for x in c {
                let mut child = st.clone();
                child.italic = true;
                push_span(x, out, pal, size, child, ctx);
            }
        }
        Inline::Strong(c) => {
            for x in c {
                let mut child = st.clone();
                child.bold = true;
                push_span(x, out, pal, size, child, ctx);
            }
        }
        Inline::Strike(c) => {
            for x in c {
                let mut child = st.clone();
                child.strike = true;
                push_span(x, out, pal, size, child, ctx);
            }
        }
        Inline::Link { url, children } => {
            for x in children {
                let mut child = st.clone();
                child.link = Some(url.clone());
                push_span(x, out, pal, size, child, ctx);
            }
        }
    }
}

fn render_image<'a>(
    url: &'a str,
    alt: &'a str,
    link: Option<&'a str>,
    pal: &Palette,
    img: &ImgCtx<'a>,
) -> Element<'a, Message> {
    use crate::app::{is_remote_url, resolve_image_path};
    // A badge's placeholder still follows its link.
    let placeholder = |msg: String| -> Element<'a, Message> {
        let shown: Element<'a, Message> = text(msg).color(pal.muted).into();
        match link {
            Some(link) => linked_image(shown, link, url),
            None => shown,
        }
    };
    let clickable = |image: Element<'a, Message>, key: String| match link {
        Some(link) => linked_image(image, link, url),
        None => zoomable(image, key),
    };
    if is_remote_url(url) {
        match img.cache.get(url) {
            Some(ImageState::Loaded(h)) => {
                clickable(image_widget(h.clone()).into(), url.to_string())
            }
            Some(ImageState::LoadedSvg { svg, .. }) => {
                clickable(svg_widget(svg.clone()).into(), url.to_string())
            }
            Some(ImageState::Failed) => placeholder(format!("[image failed: {alt} ({url})]")),
            _ => placeholder(format!("[loading: {alt} ({url})]")),
        }
    } else {
        // The open document's local images are read into the cache by a
        // Task (keyed by resolved path). Nothing here touches the filesystem.
        let Some(p) = resolve_image_path(url, img.current_file) else {
            return placeholder(format!("[image missing: {alt} ({url})]"));
        };
        let key = p.to_string_lossy().into_owned();
        match img.cache.get(&key) {
            Some(ImageState::Loaded(h)) => clickable(image_widget(h.clone()).into(), key),
            Some(ImageState::LoadedSvg { svg, .. }) => {
                clickable(svg_widget(svg.clone()).into(), key)
            }
            Some(ImageState::Failed) => placeholder(format!("[image missing: {alt} ({url})]")),
            Some(ImageState::Loading) => placeholder(format!("[loading: {alt} ({url})]")),
            // Not read ahead (an image added while editing, or a preview of
            // another file): let the renderer load the path lazily.
            None => {
                let is_svg = p
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("svg"));
                if is_svg {
                    clickable(
                        svg_widget(iced::widget::svg::Handle::from_path(p)).into(),
                        key,
                    )
                } else {
                    clickable(image_widget(p).into(), key)
                }
            }
        }
    }
}

/// What the image viewer opens for an image written as `url`: the URL of
/// a remote image, the resolved path of a local one (the image cache key).
fn image_key(url: &str, current_file: Option<&Path>) -> String {
    if crate::app::is_remote_url(url) {
        return url.to_string();
    }
    crate::app::resolve_image_path(url, current_file)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| url.to_string())
}

/// An image wrapped in a link (a badge): a click follows the link instead
/// of opening the image viewer, and the pointer shows the destination.
fn linked_image<'a>(image: Element<'a, Message>, link: &str, url: &str) -> Element<'a, Message> {
    let source = linked_image_source(link, url);
    mouse_area(image)
        .interaction(iced::mouse::Interaction::Pointer)
        .on_press(Message::OpenLink(link.to_string()))
        .on_enter(Message::LinkHovered(link.to_string(), source))
        .on_exit(Message::LinkLeft(link.to_string(), source))
        .into()
}

/// The hover source of a linked image: its link and picture, so it stays
/// the same across rebuilds and reloads. The top bit keeps it apart from
/// the reader text widgets' ids, which count up from 1.
fn linked_image_source(link: &str, url: &str) -> u64 {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut h = DefaultHasher::new();
    link.hash(&mut h);
    url.hash(&mut h);
    h.finish() | (1 << 63)
}

/// An image that opens the image viewer on click, with the zoom-in cursor
/// that says so.
fn zoomable<'a>(image: impl Into<Element<'a, Message>>, key: String) -> Element<'a, Message> {
    mouse_area(image)
        .interaction(iced::mouse::Interaction::ZoomIn)
        .on_press(Message::OpenImageZoom(key))
        .into()
}

/// Render a `Block::Diagram { kind: Math }`. Unlike mermaid/dot diagrams,
/// display math reads as document content, not a figure — so no card and
/// no border. Just the centered formula on its own line, with a copy
/// button at the right that shows while the pointer is over it.
///
/// - `Ready` → centered image of the rendered SVG; its button copies the LaTeX.
/// - `Pending` / cache miss → faded centered source.
/// - `Err(msg)` → centered source + a small error chip (tooltip carries detail).
fn render_math_block<'a>(
    hash: u64,
    source: &'a str,
    pal: &Palette,
    typ: &Typography,
    img: &ImgCtx<'a>,
) -> Element<'a, Message> {
    let key = (hash, img.diagram_theme_id);
    match img.diagram_cache.peek(&key) {
        Some(DiagramState::Ready {
            inline, device_w, ..
        }) => {
            // The inline raster is RASTER_SCALE× the intrinsic size; draw it at
            // intrinsic logical width so the formula matches its design size
            // instead of rendering 2× too large.
            let logical_w = *device_w as f32 / crate::diagram::RASTER_SCALE;
            let el = image_widget(inline.clone())
                .width(Length::Fixed(logical_w))
                .height(Length::Shrink)
                .content_fit(iced::ContentFit::Contain);
            // Equal side padding keeps the formula centered and clear of
            // the copy button; a click on the formula itself does nothing.
            let formula = container(el)
                .center_x(Length::Fill)
                .padding(Padding::from([8.0, COPY_BUTTON_CLEARANCE]));
            let copied = img.copied == Some(copy_key(source));
            HoverReveal::new(formula, copy_button(source, "Copy Math", copied, pal))
                .pinned(copied)
                .into()
        }
        Some(DiagramState::Err(msg)) => {
            let body = container(
                text(source)
                    .font(iced::Font::MONOSPACE)
                    .size(typ.code_size)
                    .color(pal.fg),
            )
            .center_x(Length::Fill);
            let chip = container(chip(pal, "⚠ math error", pal.fg))
                .padding(Padding::from([4, 0]))
                .align_x(iced::alignment::Horizontal::Center)
                .width(Length::Fill);
            let stacked: Element<'_, Message> =
                Column::new().spacing(4).push(body).push(chip).into();
            tooltip(
                stacked,
                container(text(msg.clone()).color(pal.fg).size(12))
                    .padding(Padding::from([4, 8]))
                    .style({
                        let pal_t = *pal;
                        move |_| container::Style {
                            background: Some(pal_t.surface_alt.into()),
                            border: iced::Border {
                                color: pal_t.rule,
                                width: 1.0,
                                radius: crate::theme::radius::SM.into(),
                            },
                            ..Default::default()
                        }
                    }),
                iced::widget::tooltip::Position::Top,
            )
            .into()
        }
        _ => {
            let mut color = pal.fg;
            color.a *= 0.45;
            container(
                text(source)
                    .font(iced::Font::MONOSPACE)
                    .size(typ.code_size)
                    .color(color),
            )
            .center_x(Length::Fill)
            .padding(Padding::from([4, 0]))
            .into()
        }
    }
}

/// Render a `Block::Diagram`. Looks up `(hash, theme_id)` in the cache.
///
/// - `Ready` → `iced::svg` with a hover overlay (zoom + copy icons).
/// - `Pending` / cache miss → faded source code block + "rendering" chip.
/// - `Err(msg)` → source code block + error chip wrapped in a tooltip.
///
/// T4 owns task dispatch; this function never kicks off a render.
fn render_diagram<'a>(
    hash: u64,
    source: &'a str,
    pal: &Palette,
    typ: &Typography,
    img: &ImgCtx<'a>,
) -> Element<'a, Message> {
    let key = (hash, img.diagram_theme_id);
    let state = img.diagram_cache.peek(&key);

    match state {
        Some(DiagramState::Ready { inline, .. }) => {
            let pal_c = *pal;
            // Use the pre-rasterized RGBA handle for inline display. This
            // bypasses iced_wgpu's per-redraw SVG parse step that the old
            // `svg::Handle::from_memory` path triggered.
            let inline_el = image_widget(inline.clone())
                .width(Length::Fill)
                .height(Length::Shrink)
                .content_fit(iced::ContentFit::Contain);
            let body = container(inline_el)
                .padding(Padding::from(10))
                .max_height(600.0)
                .width(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(pal_c.code_bg.into()),
                    border: iced::Border {
                        color: pal_c.code_border,
                        width: 1.0,
                        radius: crate::theme::radius::MD.into(),
                    },
                    ..Default::default()
                });
            // Whole-area click → zoom (mirrors image click-to-zoom UX).
            let clickable = crate::app::reader_tip(
                mouse_area(body)
                    .interaction(iced::mouse::Interaction::Pointer)
                    .on_press(Message::DiagramZoom(hash)),
                "Click to enlarge",
                tooltip::Position::FollowCursor,
                pal_c,
            );
            let copied = img.copied == Some(copy_key(source));
            HoverReveal::new(
                clickable,
                copy_button(source, "Copy Source", copied, &pal_c),
            )
            .pinned(copied)
            .into()
        }
        Some(DiagramState::Err(msg)) => {
            let body = source_code_block(source, pal, typ, 1.0);
            let chip = chip(pal, "⚠ error", pal.fg);
            let overlay = container(chip)
                .padding(Padding::from([6, 8]))
                .align_x(iced::alignment::Horizontal::Right)
                .align_y(iced::alignment::Vertical::Bottom)
                .width(Length::Fill)
                .height(Length::Fill);
            let stacked: Element<'_, Message> = stack![body, overlay].into();
            tooltip(
                stacked,
                container(text(msg.clone()).color(pal.fg).size(12))
                    .padding(Padding::from([4, 8]))
                    .style({
                        let pal_t = *pal;
                        move |_| container::Style {
                            background: Some(pal_t.surface_alt.into()),
                            border: iced::Border {
                                color: pal_t.rule,
                                width: 1.0,
                                radius: crate::theme::radius::SM.into(),
                            },
                            ..Default::default()
                        }
                    }),
                iced::widget::tooltip::Position::Top,
            )
            .into()
        }
        // Pending or cache miss.
        _ => {
            let body = source_code_block(source, pal, typ, 0.45);
            let chip = chip(pal, "rendering…", pal.muted);
            let overlay = container(chip)
                .padding(Padding::from([6, 8]))
                .align_x(iced::alignment::Horizontal::Right)
                .align_y(iced::alignment::Vertical::Bottom)
                .width(Length::Fill)
                .height(Length::Fill);
            stack![body, overlay].into()
        }
    }
}

/// Render a diagram's raw source as a monospace block — mirrors the
/// `Block::CodeBlock` container styling. `opacity` is currently used only as
/// a hint for color fade (Iced 0.14 has no widget-level opacity); we fold it
/// into the text color alpha so Pending sources read as visually faded.
fn source_code_block<'a>(
    source: &'a str,
    pal: &Palette,
    typ: &Typography,
    opacity: f32,
) -> Element<'a, Message> {
    let pal_c = *pal;
    let mut color = pal_c.fg;
    color.a = (color.a * opacity).clamp(0.0, 1.0);
    container(
        text(source)
            .font(iced::Font::MONOSPACE)
            .size(typ.code_size)
            .color(color),
    )
    .padding(Padding::from([10, 12]))
    .style(move |_| container::Style {
        background: Some(pal_c.code_bg.into()),
        border: iced::Border {
            color: pal_c.code_border,
            width: 1.0,
            radius: crate::theme::radius::MD.into(),
        },
        ..Default::default()
    })
    .width(Length::Fill)
    .into()
}

/// Small pill used by Pending / Err overlays.
fn chip<'a>(pal: &Palette, label: &'a str, fg: iced::Color) -> Element<'a, Message> {
    let pal_c = *pal;
    container(text(label).size(11).color(fg))
        .padding(Padding::from([2, 8]))
        .style(move |_| container::Style {
            background: Some(pal_c.chip().into()),
            border: iced::Border {
                color: pal_c.border(),
                width: 1.0,
                radius: crate::theme::radius::XS.into(),
            },
            ..Default::default()
        })
        .into()
}

fn render_list<'a>(
    ordered: bool,
    items: &'a [ListItem],
    pal: &Palette,
    typ: &Typography,
    ctx: &mut HlCtx<'_>,
    img: &ImgCtx<'a>,
) -> Element<'a, Message> {
    let mut col = Column::new().spacing(8);
    for (idx, it) in items.iter().enumerate() {
        let line = typ.body_size * typ.line_height;
        let marker: Element<'a, Message> = match (ordered, it.task) {
            (_, Some(checked)) => task_checkbox(checked, line, pal),
            (ordered, None) => {
                let bullet = if ordered {
                    format!("{}.", idx + 1)
                } else {
                    "•".to_string()
                };
                text(bullet)
                    .color(pal.accent)
                    .size(typ.body_size)
                    .line_height(iced::widget::text::LineHeight::Relative(typ.line_height))
                    .into()
            }
        };
        let inner = it.blocks.iter().fold(Column::new().spacing(6), |c, b| {
            c.push(render_block_inner(b, pal, typ, ctx, img))
        });
        col = col.push(row![container(marker).width(Length::Fixed(28.0)), inner].spacing(6));
    }
    col.into()
}

/// Side of a task item's checkbox.
const TASK_BOX: f32 = 14.0;

/// A task item's checkbox, centered on the item's first line. The reader
/// does not write the file, so it is read-only: a not-allowed cursor and a
/// tooltip that says so.
fn task_checkbox<'a>(checked: bool, line: f32, pal: &Palette) -> Element<'a, Message> {
    let pal_c = *pal;
    let mark: Element<'a, Message> = if checked {
        crate::icon::glyph(crate::icon::ic::CHECK, 11.0, pal_c.accent_fg).into()
    } else {
        Space::new().into()
    };
    let face = container(mark)
        .center_x(Length::Fixed(TASK_BOX))
        .center_y(Length::Fixed(TASK_BOX))
        .style(move |_| container::Style {
            background: checked.then(|| pal_c.accent.into()),
            border: iced::Border {
                color: if checked { pal_c.accent } else { pal_c.muted },
                width: 1.5,
                radius: crate::theme::radius::XS.into(),
            },
            ..Default::default()
        });
    let cell = container(face).center_y(Length::Fixed(line));
    crate::app::reader_tip(
        mouse_area(cell).interaction(iced::mouse::Interaction::NotAllowed),
        "Read-only",
        tooltip::Position::Top,
        pal_c,
    )
}

fn render_block_inner<'a>(
    b: &'a Block,
    pal: &Palette,
    typ: &Typography,
    ctx: &mut HlCtx<'_>,
    img: &ImgCtx<'a>,
) -> Element<'a, Message> {
    match b {
        Block::Paragraph(inlines) => {
            let spans = inline_spans(inlines, pal, typ.body_size, ctx);
            rich_text_spaced(
                spans,
                paragraph_line_height(inlines, typ.line_height),
                TextFills::of(pal),
            )
        }
        _ => render_block(b, pal, typ, ctx.query, ctx.current_in_block, img),
    }
}

fn render_table<'a>(
    headers: &'a [Vec<Inline>],
    rows: &'a [Vec<Vec<Inline>>],
    pal: &Palette,
    typ: &Typography,
    ctx: &mut HlCtx<'_>,
) -> Element<'a, Message> {
    let cols = headers
        .len()
        .max(rows.iter().map(|r| r.len()).max().unwrap_or(0))
        .max(1);
    let pal_t = *pal;

    let make_cell = |content: Element<'a, Message>| -> Element<'a, Message> {
        AdaptiveTableCell::new(content).into()
    };

    let mut cells = Vec::with_capacity((rows.len() + 1) * cols);
    for column_index in 0..cols {
        let content: Element<'a, Message> = if let Some(cell) = headers.get(column_index) {
            let spans = inline_spans_with_style(
                cell,
                pal,
                typ.body_size,
                Style {
                    bold: true,
                    ..Style::default()
                },
                ctx,
            );
            rich_text_links(spans, TextFills::of(pal))
        } else {
            text("").into()
        };
        cells.push(make_cell(content));
    }

    for row_cells in rows {
        for column_index in 0..cols {
            let content: Element<'a, Message> = if let Some(cell) = row_cells.get(column_index) {
                let spans = inline_spans(cell, pal, typ.body_size, ctx);
                rich_text_links(spans, TextFills::of(pal))
            } else {
                text("").into()
            };
            cells.push(make_cell(content));
        }
    }

    let viewport = Rc::new(Cell::new(0.0_f32));
    let table = AdaptiveTable::new(cols, cells, pal_t, viewport.clone()).width(Length::Shrink);
    let recently_scrolled = ctx.recently_scrolled;

    let scroller = scrollable(table)
        .width(Length::Fill)
        .height(Length::Shrink)
        .on_scroll(|_| Message::TableScrolled)
        .style(move |_, status| {
            crate::app::sleek_scrollable_style(status, pal_t, recently_scrolled)
        })
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new()
                .width(6.0)
                .scroller_width(6.0)
                .margin(2.0),
        ));

    TableViewport {
        content: scroller.into(),
        viewport,
        edges: (false, false),
        shade: crate::theme::shadow::OVERFLOW_EDGE,
    }
    .into()
}

/// Wraps the table's horizontal scrollable and publishes the width it is
/// offered, because `scrollable` hides that width from its child. It also
/// shades each side the table overflows on, read from the scrollable's
/// offset after every layout and event.
struct TableViewport<'a, Message, Theme, Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    viewport: Rc<Cell<f32>>,
    /// Whether more of the table lies past the left and right edges.
    edges: (bool, bool),
    shade: iced::Color,
}

/// Width of the shade at an edge a wide table overflows.
const TABLE_EDGE_SHADE: f32 = 16.0;

/// Whether a table scrolled `offset` px into `content` px, shown in
/// `viewport` px, has more to see on the left and on the right.
fn table_overflow(offset: f32, viewport: f32, content: f32) -> (bool, bool) {
    // Half a pixel of slack: layout rounding never shows a shade.
    (offset > 0.5, offset + viewport < content - 0.5)
}

/// Reads a scrollable's offset without visiting its content.
struct OverflowProbe(Option<(bool, bool)>);

impl Operation for OverflowProbe {
    fn traverse(&mut self, _operate: &mut dyn FnMut(&mut dyn Operation)) {}

    fn scrollable(
        &mut self,
        _id: Option<&iced::widget::Id>,
        bounds: Rectangle,
        content_bounds: Rectangle,
        translation: Vector,
        _state: &mut dyn iced::advanced::widget::operation::Scrollable,
    ) {
        self.0 = Some(table_overflow(
            translation.x,
            bounds.width,
            content_bounds.width,
        ));
    }
}

impl<Message, Theme, Renderer> TableViewport<'_, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
{
    fn probe_edges(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer) {
        let mut probe = OverflowProbe(None);
        self.content
            .as_widget_mut()
            .operate(tree, layout, renderer, &mut probe);
        self.edges = probe.0.unwrap_or((false, false));
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for TableViewport<'_, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
{
    fn tag(&self) -> tree::Tag {
        self.content.as_widget().tag()
    }

    fn state(&self) -> tree::State {
        self.content.as_widget().state()
    }

    fn children(&self) -> Vec<Tree> {
        self.content.as_widget().children()
    }

    fn diff(&self, tree: &mut Tree) {
        self.content.as_widget().diff(tree);
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.viewport.set(limits.max().width);
        let node = self.content.as_widget_mut().layout(tree, renderer, limits);
        self.probe_edges(tree, Layout::new(&node), renderer);
        node
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content
            .as_widget()
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
        let (left, right) = self.edges;
        #[cfg(test)]
        draw_log::edges(left, right);
        let bounds = layout.bounds();
        let clear = iced::Color {
            a: 0.0,
            ..self.shade
        };
        // A nested layer does not inherit its parent's clip: clip to what
        // the reader shows.
        let Some(clip) = bounds.intersection(viewport).filter(|_| left || right) else {
            return;
        };
        let width = TABLE_EDGE_SHADE.min(bounds.width / 2.0);
        // An overflowing table draws in the scrollable's own layer, which
        // composites after this one; the shade needs a layer above it.
        renderer.with_layer(clip, |renderer| {
            // Each shade runs from the edge (dark) inward (clear).
            for (shown, x, angle) in [
                (left, bounds.x, std::f32::consts::FRAC_PI_2),
                (
                    right,
                    bounds.x + bounds.width - width,
                    3.0 * std::f32::consts::FRAC_PI_2,
                ),
            ] {
                if !shown {
                    continue;
                }
                let gradient = iced::gradient::Linear::new(iced::Radians(angle))
                    .add_stop(0.0, self.shade)
                    .add_stop(1.0, clear);
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle {
                            x,
                            y: bounds.y,
                            width,
                            height: bounds.height,
                        },
                        ..renderer::Quad::default()
                    },
                    Background::Gradient(gradient.into()),
                );
            }
        });
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            tree, event, layout, cursor, renderer, clipboard, shell, viewport,
        );
        // A scroll moves the offset without a new layout.
        if let Event::Mouse(_) | Event::Touch(_) = event {
            self.probe_edges(tree, layout, renderer);
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content
            .as_widget()
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(tree, layout, renderer, operation);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content
            .as_widget_mut()
            .overlay(tree, layout, renderer, viewport, translation)
    }
}

impl<'a, Message, Theme, Renderer> From<TableViewport<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: 'a + renderer::Renderer,
{
    fn from(w: TableViewport<'a, Message, Theme, Renderer>) -> Self {
        Element::new(w)
    }
}

/// A block with a control layered over it (a copy button) that is drawn
/// only while the pointer is over the block, or while `pinned` (the check
/// after a copy). The control is laid out and updated like a `Stack`
/// layer either way, so it keeps its tooltip and widget state; a hover
/// change asks for one redraw instead of a view rebuild.
struct HoverReveal<'a> {
    children: [Element<'a, Message>; 2],
    pinned: bool,
}

#[derive(Default)]
struct HoverRevealState {
    hovered: bool,
}

impl<'a> HoverReveal<'a> {
    fn new(
        base: impl Into<Element<'a, Message>>,
        control: impl Into<Element<'a, Message>>,
    ) -> Self {
        Self {
            children: [base.into(), control.into()],
            pinned: false,
        }
    }

    fn pinned(mut self, pinned: bool) -> Self {
        self.pinned = pinned;
        self
    }
}

impl Widget<Message, iced::Theme, iced::Renderer> for HoverReveal<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<HoverRevealState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(HoverRevealState::default())
    }

    fn children(&self) -> Vec<Tree> {
        self.children.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.children);
    }

    fn size(&self) -> Size<Length> {
        self.children[0].as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let [base, control] = &mut self.children;
        let base = base
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        let size = base.size();
        let control = control.as_widget_mut().layout(
            &mut tree.children[1],
            renderer,
            &layout::Limits::new(Size::ZERO, size),
        );
        layout::Node::with_children(size, vec![base, control])
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if let Event::Mouse(_) | Event::Window(_) = event {
            let hovered = cursor.is_over(layout.bounds());
            let state = tree.state.downcast_mut::<HoverRevealState>();
            if state.hovered != hovered {
                state.hovered = hovered;
                shell.request_redraw();
            }
        }
        let mut layouts = layout.children();
        let (Some(base_layout), Some(control_layout)) = (layouts.next(), layouts.next()) else {
            return;
        };
        let [base, control] = &mut self.children;
        control.as_widget_mut().update(
            &mut tree.children[1],
            event,
            control_layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        if shell.is_event_captured() {
            return;
        }
        // Over the control the block below sees a levitating cursor, as
        // in a `Stack`: no text hover or selection under the button.
        let over_control = control.as_widget().mouse_interaction(
            &tree.children[1],
            control_layout,
            cursor,
            viewport,
            renderer,
        ) != mouse::Interaction::None;
        let cursor = if over_control && !cursor.is_levitating() {
            cursor.levitate()
        } else {
            cursor
        };
        base.as_widget_mut().update(
            &mut tree.children[0],
            event,
            base_layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .rev()
            .map(|((child, tree), layout)| {
                child
                    .as_widget()
                    .mouse_interaction(tree, layout, cursor, viewport, renderer)
            })
            .find(|interaction| *interaction != mouse::Interaction::None)
            .unwrap_or_default()
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let mut layouts = layout.children();
        let (Some(base_layout), Some(control_layout)) = (layouts.next(), layouts.next()) else {
            return;
        };
        self.children[0].as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            base_layout,
            cursor,
            viewport,
        );
        let revealed = self.pinned || cursor.is_over(layout.bounds());
        #[cfg(test)]
        draw_log::reveal(revealed);
        if revealed {
            use iced::advanced::Renderer as _;
            renderer.with_layer(*viewport, |renderer| {
                self.children[1].as_widget().draw(
                    &tree.children[1],
                    renderer,
                    theme,
                    style,
                    control_layout,
                    cursor,
                    viewport,
                );
            });
        }
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.traverse(&mut |operation| {
            self.children
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
                .for_each(|((child, tree), layout)| {
                    child
                        .as_widget_mut()
                        .operate(tree, layout, renderer, operation);
                });
        });
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        overlay::from_children(
            &mut self.children,
            tree,
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a> From<HoverReveal<'a>> for Element<'a, Message> {
    fn from(w: HoverReveal<'a>) -> Self {
        Element::new(w)
    }
}

/// Room the copy button keeps free at the right of a code block's text,
/// so no line runs under it: the button (25 px) and its 8 px inset, plus
/// a gap.
const COPY_BUTTON_CLEARANCE: f32 = 40.0;

/// Which block a copy button belongs to: the address of its source text
/// in the document. Blocks with the same text stay apart; a reparse moves
/// the text, which only ends a check early.
pub(crate) fn copy_key(source: &str) -> usize {
    source.as_ptr() as usize
}

/// The hover-revealed copy button at a block's top-right corner. It copies
/// `source` and, while `copied`, shows a check instead of the copy icon.
fn copy_button<'a>(
    source: &str,
    action: &'a str,
    copied: bool,
    pal: &Palette,
) -> Element<'a, Message> {
    let pal_c = *pal;
    let (code, color) = if copied {
        (crate::icon::ic::CHECK, pal_c.accent)
    } else {
        (crate::icon::ic::COPY, pal_c.muted)
    };
    let face = container(crate::icon::glyph(code, 13.0, color))
        .padding(Padding::from([4, 6]))
        .style(move |_| container::Style {
            background: Some(pal_c.code_bg.into()),
            border: iced::Border {
                color: pal_c.code_border,
                width: 1.0,
                radius: crate::theme::radius::SM.into(),
            },
            ..Default::default()
        });
    container(crate::app::reader_tip(
        mouse_area(face)
            .interaction(iced::mouse::Interaction::Pointer)
            .on_press(Message::CopyBlockSource(
                source.to_string(),
                copy_key(source),
            )),
        action,
        tooltip::Position::Left,
        pal_c,
    ))
    .padding(Padding::from([6, 8]))
    .align_x(iced::alignment::Horizontal::Right)
    .width(Length::Fill)
    .into()
}

/// A table cell that switches between intrinsic measurement and the shared
/// fixed-column layout supplied by [`AdaptiveTable`].
///
/// The table's first pass marks its limits as shrink-wrapped. In that pass the
/// cell uses `Length::Shrink`, so an unbounded horizontal scroll limit cannot
/// make every non-empty column report the same maximum width. The final pass
/// uses fixed limits, and the cell switches to `Length::Fill` so the content
/// wraps against the column width and all rows stay aligned.
struct AdaptiveTableCell<'a, Message, Theme, Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    padding: Padding,
    clip: bool,
}

impl<'a, Message, Theme, Renderer> AdaptiveTableCell<'a, Message, Theme, Renderer> {
    fn new(content: Element<'a, Message, Theme, Renderer>) -> Self {
        Self {
            content,
            padding: Padding::from([11, 16]),
            clip: true,
        }
    }
}

fn adaptive_table_cell_width(limits: &layout::Limits) -> Length {
    if limits.compression().width {
        Length::Shrink
    } else {
        Length::Fill
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for AdaptiveTableCell<'_, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
{
    fn tag(&self) -> tree::Tag {
        self.content.as_widget().tag()
    }

    fn state(&self) -> tree::State {
        self.content.as_widget().state()
    }

    fn children(&self) -> Vec<Tree> {
        self.content.as_widget().children()
    }

    fn diff(&self, tree: &mut Tree) {
        self.content.as_widget().diff(tree);
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fill,
            height: Length::Shrink,
        }
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        // `AdaptiveTable` passes compressed limits during its intrinsic
        // measurement pass and fixed limits during final layout.
        let width = adaptive_table_cell_width(limits);

        container::layout(
            limits,
            width,
            Length::Shrink,
            f32::INFINITY,
            f32::INFINITY,
            self.padding,
            iced::alignment::Horizontal::Left,
            iced::alignment::Vertical::Top,
            |limits| self.content.as_widget_mut().layout(tree, renderer, limits),
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        if let Some(clipped_viewport) = bounds.intersection(viewport) {
            self.content.as_widget().draw(
                tree,
                renderer,
                theme,
                style,
                layout.children().next().unwrap(),
                cursor,
                if self.clip {
                    &clipped_viewport
                } else {
                    viewport
                },
            );
        }
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            tree,
            event,
            layout.children().next().unwrap(),
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            tree,
            layout.children().next().unwrap(),
            cursor,
            viewport,
            renderer,
        )
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content.as_widget_mut().operate(
            tree,
            layout.children().next().unwrap(),
            renderer,
            operation,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            tree,
            layout.children().next().unwrap(),
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message, Theme, Renderer> From<AdaptiveTableCell<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: 'a + renderer::Renderer,
{
    fn from(cell: AdaptiveTableCell<'a, Message, Theme, Renderer>) -> Self {
        Element::new(cell)
    }
}

/// Final column widths for a table whose columns measured `natural` wide.
///
/// `viewport` is the text-column width the table may occupy. Short tables grow
/// to fill it, wide tables shrink (widest columns first, never below
/// `min_column`), and a table whose columns cannot fit at `min_column` keeps
/// its natural widths so the horizontal scroll takes over.
fn fit_table_columns(natural: &[f32], viewport: f32, separators: f32, min_column: f32) -> Vec<f32> {
    let columns = natural.len();
    let available = viewport - separators;
    if columns == 0
        || !available.is_finite()
        || available <= 0.0
        || columns as f32 * min_column > available
    {
        return natural.to_vec();
    }

    let sum = natural.iter().sum::<f32>();
    if sum <= available {
        let extra = (available - sum) / columns as f32;
        return natural.iter().map(|w| w + extra).collect();
    }

    // Water-fill: find the cap `c` where sum(min(w, c)) == available, so the
    // widest columns give up width first.
    let mut sorted = natural.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let mut remaining = available;
    let mut cap = available / columns as f32;
    for (i, w) in sorted.iter().enumerate() {
        let left = (columns - i) as f32;
        let share = remaining / left;
        if *w >= share {
            cap = share;
            break;
        }
        remaining -= w;
    }
    natural.iter().map(|w| w.min(cap).max(min_column)).collect()
}

/// A content-driven table layout with a responsive minimum width.
///
/// Iced's stock table makes its first shrink column fluid. That is useful for
/// ordinary tables, but it prevents a table inside a horizontal scrollable
/// from exposing its intrinsic width. This layout measures every column,
/// fills the available document width when it can, and grows beyond it only
/// when the content actually needs more room.
struct AdaptiveTable<'a, Message, Theme, Renderer> {
    columns: usize,
    cells: Vec<Element<'a, Message, Theme, Renderer>>,
    width: Length,
    height: Length,
    style: TableStyle,
    viewport: Rc<Cell<f32>>,
}

/// Colors and metrics of a table's rounded frame, taken from the theme so
/// presets and custom themes share one look.
#[derive(Debug, Clone, Copy)]
struct TableStyle {
    radius: f32,
    border_width: f32,
    border: iced::Color,
    header_bg: iced::Color,
    body_bg: iced::Color,
    /// Width of the rules between columns. Zero: columns are separated by
    /// cell padding only.
    column_rule: f32,
    /// Width of the rules between rows.
    row_rule: f32,
}

impl TableStyle {
    fn from_palette(pal: &Palette) -> Self {
        Self {
            radius: crate::theme::radius::TABLE,
            border_width: 1.0,
            border: pal.rule,
            header_bg: pal.surface_alt,
            body_bg: pal.surface,
            column_rule: 0.0,
            row_rule: 1.0,
        }
    }

    /// Corners of the header fill. It follows the frame's top corners, and
    /// also its bottom corners when the header is the only row, so no fill
    /// shows outside the rounded frame.
    fn header_radius(&self, rows: usize) -> iced::border::Radius {
        let bottom = if rows <= 1 { self.radius } else { 0.0 };
        iced::border::Radius {
            top_left: self.radius,
            top_right: self.radius,
            bottom_right: bottom,
            bottom_left: bottom,
        }
    }
}

struct AdaptiveTableMetrics {
    columns: Vec<f32>,
    rows: Vec<f32>,
}

impl<'a, Message, Theme, Renderer> AdaptiveTable<'a, Message, Theme, Renderer> {
    const MIN_COLUMN_WIDTH: f32 = 128.0;
    const MAX_COLUMN_WIDTH: f32 = 320.0;

    fn new(
        columns: usize,
        cells: Vec<Element<'a, Message, Theme, Renderer>>,
        palette: Palette,
        viewport: Rc<Cell<f32>>,
    ) -> Self {
        Self {
            columns: columns.max(1),
            cells,
            width: Length::Shrink,
            height: Length::Shrink,
            style: TableStyle::from_palette(&palette),
            viewport,
        }
    }

    fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for AdaptiveTable<'_, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
{
    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<AdaptiveTableMetrics>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(AdaptiveTableMetrics {
            columns: Vec::new(),
            rows: Vec::new(),
        })
    }

    fn children(&self) -> Vec<Tree> {
        self.cells
            .iter()
            .map(|cell| Tree::new(cell.as_widget()))
            .collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.cells);
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let metrics = tree.state.downcast_mut::<AdaptiveTableMetrics>();
        let columns = self.columns;
        let row_count = (self.cells.len() + columns - 1) / columns;
        let table_limits = limits.width(self.width).height(self.height);
        let measure_max_width = table_limits.max().width;

        // First pass: measure the widest cell in every column without a
        // wrapping width. The cap keeps a single URL or token from turning
        // the entire document into an effectively unusable canvas.
        let measure_limits =
            layout::Limits::new(Size::ZERO, Size::new(measure_max_width, f32::INFINITY))
                .width(Length::Shrink);
        let mut column_widths = vec![0.0_f32; columns];

        for (index, (cell, state)) in self.cells.iter_mut().zip(&mut tree.children).enumerate() {
            let column = index % columns;
            let node = cell
                .as_widget_mut()
                .layout(state, renderer, &measure_limits);
            column_widths[column] =
                column_widths[column].max(node.size().width.min(Self::MAX_COLUMN_WIDTH));
        }

        for width in &mut column_widths {
            *width = width
                .max(Self::MIN_COLUMN_WIDTH)
                .min(Self::MAX_COLUMN_WIDTH);
        }

        let separators_width = self.style.column_rule * columns.saturating_sub(1) as f32;
        let viewport_width = self.viewport.get();

        // Horizontal scrollables hide the real viewport from their child, so
        // `TableViewport` publishes it. Tables fill the text column, shrink
        // by wrapping wide columns, and scroll only when columns cannot fit
        // at `MIN_COLUMN_WIDTH`.
        column_widths = fit_table_columns(
            &column_widths,
            viewport_width,
            separators_width,
            Self::MIN_COLUMN_WIDTH,
        );

        // Second pass: lay out every cell against its shared column width.
        // This is what keeps all rows aligned and lets rich text wrap inside
        // a cell instead of painting past its boundary.
        let mut cells = Vec::with_capacity(self.cells.len());
        cells.resize(self.cells.len(), layout::Node::default());
        let mut row_heights = vec![0.0_f32; row_count];

        for (index, (cell, state)) in self.cells.iter_mut().zip(&mut tree.children).enumerate() {
            let row = index / columns;
            let column = index % columns;
            let width = column_widths[column];
            let cell_limits = layout::Limits::new(Size::ZERO, Size::new(width, f32::INFINITY))
                .width(Length::Fixed(width))
                .height(Length::Shrink);
            let node = cell.as_widget_mut().layout(state, renderer, &cell_limits);
            row_heights[row] = row_heights[row].max(node.size().height);
            cells[index] = node;
        }

        metrics.columns = column_widths.clone();
        metrics.rows = row_heights.clone();

        let table_width = column_widths.iter().sum::<f32>() + separators_width;
        let table_height = row_heights.iter().sum::<f32>()
            + self.style.row_rule * row_count.saturating_sub(1) as f32;

        let mut y = 0.0;
        for row in 0..row_count {
            let mut x = 0.0;
            for column in 0..columns {
                let index = row * columns + column;
                if let Some(cell) = cells.get_mut(index) {
                    cell.move_to_mut((x, y));
                }
                x += column_widths[column] + self.style.column_rule;
            }
            y += row_heights[row] + self.style.row_rule;
        }

        let size = table_limits.resolve(
            self.width,
            self.height,
            Size::new(table_width, table_height),
        );

        layout::Node::with_children(size, cells)
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        for ((cell, state), layout) in self
            .cells
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            cell.as_widget_mut().update(
                state, event, layout, cursor, renderer, clipboard, shell, viewport,
            );
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let metrics = tree.state.downcast_ref::<AdaptiveTableMetrics>();
        let table = &self.style;
        let row_count = metrics.rows.len();

        // The body fill carries the frame's rounded corners. Row surfaces are
        // drawn independently from cell content, so the header fill stays
        // continuous even when a row's tallest cell is taller than its
        // neighbors.
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: iced::Border {
                    radius: table.radius.into(),
                    ..Default::default()
                },
                snap: true,
                ..Default::default()
            },
            Background::Color(table.body_bg),
        );
        if let Some(header) = metrics.rows.first().copied() {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        height: header,
                        ..bounds
                    },
                    border: iced::Border {
                        radius: table.header_radius(row_count),
                        ..Default::default()
                    },
                    snap: true,
                    ..Default::default()
                },
                Background::Color(table.header_bg),
            );
        }

        for ((cell, state), layout) in self.cells.iter().zip(&tree.children).zip(layout.children())
        {
            cell.as_widget()
                .draw(state, renderer, theme, style, layout, cursor, viewport);
        }

        // Row rules stop inside the frame stroke. They sit at least a header
        // height away from the top and bottom edges, so the rounded corners
        // never cut them.
        let mut y = bounds.y;
        for height in metrics
            .rows
            .iter()
            .copied()
            .take(row_count.saturating_sub(1))
        {
            y += height;
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: bounds.x + table.border_width,
                        y,
                        width: (bounds.width - 2.0 * table.border_width).max(0.0),
                        height: table.row_rule,
                    },
                    snap: true,
                    ..Default::default()
                },
                table.border,
            );
            y += table.row_rule;
        }

        // The frame stroke goes last so it sits above the fills and rules.
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: iced::Border {
                    color: table.border,
                    width: table.border_width,
                    radius: table.radius.into(),
                },
                snap: true,
                ..Default::default()
            },
            Background::Color(iced::Color::TRANSPARENT),
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.cells
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((cell, state), layout)| {
                cell.as_widget()
                    .mouse_interaction(state, layout, cursor, viewport, renderer)
            })
            .max()
            .unwrap_or_default()
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        for ((cell, state), layout) in self
            .cells
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            cell.as_widget_mut()
                .operate(state, layout, renderer, operation);
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        overlay::from_children(
            &mut self.cells,
            tree,
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message, Theme, Renderer> From<AdaptiveTable<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: 'a + renderer::Renderer,
{
    fn from(table: AdaptiveTable<'a, Message, Theme, Renderer>) -> Self {
        Element::new(table)
    }
}

pub fn style_color(s: crate::ast::HlStyle, pal: &Palette) -> iced::Color {
    use crate::ast::HlStyle::*;
    let sx = &pal.syntax;
    match s {
        Keyword => sx.keyword,
        Type => sx.type_,
        Function => sx.function,
        String => sx.string,
        Number => sx.number,
        Comment => sx.comment,
        Operator => sx.operator,
        Constant => sx.constant,
        Variable => sx.variable,
        Punctuation => sx.punctuation,
        Plain => pal.fg,
    }
}

/// Test-only record of what the reader drew: whether each hover-revealed
/// control (copy buttons) was drawn, and which overflow edges each wide
/// table shaded. Drawing leaves no other trace a headless test can read.
#[cfg(test)]
pub(crate) mod draw_log {
    use std::cell::RefCell;

    thread_local! {
        static REVEALS: RefCell<Vec<bool>> = const { RefCell::new(Vec::new()) };
        static EDGES: RefCell<Vec<(bool, bool)>> = const { RefCell::new(Vec::new()) };
    }

    pub(crate) fn reveal(drawn: bool) {
        REVEALS.with(|log| log.borrow_mut().push(drawn));
    }

    /// One entry per hover-revealed control drawn since the last call:
    /// whether its control was drawn.
    pub(crate) fn take_reveals() -> Vec<bool> {
        REVEALS.with(|log| std::mem::take(&mut *log.borrow_mut()))
    }

    pub(crate) fn edges(left: bool, right: bool) {
        EDGES.with(|log| log.borrow_mut().push((left, right)));
    }

    /// One entry per table drawn since the last call: whether its left and
    /// right edges were shaded.
    pub(crate) fn take_edges() -> Vec<(bool, bool)> {
        EDGES.with(|log| std::mem::take(&mut *log.borrow_mut()))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        adaptive_table_cell_width, fit_table_columns, for_text_runs, is_cjk_fallback_char,
        paragraph_line_height, push_styled_runs, reading_max_width, ReadingWidth, Style,
        TableStyle,
    };
    use crate::theme::{radius, Palette};
    use iced::advanced::layout;
    use iced::{Length, Size};

    #[test]
    fn table_frame_uses_the_theme_radius_and_palette() {
        for pal in [Palette::ONE_DARK, Palette::GITHUB_LIGHT] {
            let style = TableStyle::from_palette(&pal);
            assert_eq!(style.radius, radius::TABLE);
            assert_eq!(style.border_width, 1.0);
            assert_eq!(style.border, pal.rule);
            assert_eq!(style.header_bg, pal.surface_alt);
            assert_eq!(style.body_bg, pal.surface);
        }
    }

    #[test]
    fn table_header_rounds_only_the_top_corners() {
        let style = TableStyle::from_palette(&Palette::ONE_DARK);
        let r = style.header_radius(4);
        assert_eq!((r.top_left, r.top_right), (radius::TABLE, radius::TABLE));
        assert_eq!((r.bottom_left, r.bottom_right), (0.0, 0.0));

        // A header-only table is the whole frame, so all four corners round.
        let r = style.header_radius(1);
        assert_eq!(
            (r.bottom_left, r.bottom_right),
            (radius::TABLE, radius::TABLE)
        );
    }

    #[test]
    fn table_has_row_rules_but_no_column_rules() {
        let style = TableStyle::from_palette(&Palette::ONE_DARK);
        assert_eq!(style.column_rule, 0.0);
        assert_eq!(style.row_rule, 1.0);
    }

    #[test]
    fn table_cell_intrinsic_measurement_does_not_fill_unbounded_limits() {
        let measure_limits = layout::Limits::new(Size::ZERO, Size::INFINITE).width(Length::Shrink);
        assert_eq!(adaptive_table_cell_width(&measure_limits), Length::Shrink);

        let fixed_limits = layout::Limits::new(Size::ZERO, Size::new(256.0, f32::INFINITY))
            .width(Length::Fixed(256.0));
        assert_eq!(adaptive_table_cell_width(&fixed_limits), Length::Fill);
    }

    #[test]
    fn table_columns_grow_to_fill_a_wide_viewport() {
        let widths = fit_table_columns(&[128.0, 200.0, 128.0], 800.0, 2.0, 128.0);
        assert_eq!(widths.len(), 3);
        assert!((widths.iter().sum::<f32>() + 2.0 - 800.0).abs() < 0.01);
        assert!(widths
            .iter()
            .zip([128.0, 200.0, 128.0])
            .all(|(w, n)| *w >= n));
    }

    #[test]
    fn table_columns_shrink_widest_first_to_fit_a_narrow_viewport() {
        let widths = fit_table_columns(&[128.0, 320.0, 320.0], 600.0, 2.0, 128.0);
        assert!((widths.iter().sum::<f32>() + 2.0 - 600.0).abs() < 0.01);
        assert_eq!(widths[0], 128.0);
        assert!((widths[1] - widths[2]).abs() < 0.01);
        assert!(widths[1] < 320.0 && widths[1] >= 128.0);
    }

    #[test]
    fn table_columns_keep_natural_width_when_minimums_do_not_fit() {
        let natural = [200.0, 300.0, 250.0, 128.0];
        let widths = fit_table_columns(&natural, 400.0, 3.0, 128.0);
        assert_eq!(widths, natural.to_vec());
    }

    #[test]
    fn table_columns_ignore_unbounded_or_zero_viewport() {
        let natural = [150.0, 220.0];
        assert_eq!(
            fit_table_columns(&natural, f32::INFINITY, 1.0, 128.0),
            natural.to_vec()
        );
        assert_eq!(
            fit_table_columns(&natural, 0.0, 1.0, 128.0),
            natural.to_vec()
        );
    }

    #[test]
    fn reading_width_caps_main_area_and_fills_panels() {
        assert_eq!(reading_max_width(ReadingWidth::Capped), 720.0);
        assert!(reading_max_width(ReadingWidth::Fill) > 10_000.0);
    }

    #[test]
    fn hangul_jamo_boundaries_use_cjk_fallback() {
        for codepoint in [
            0x1100, 0x11FF, // Hangul Jamo
            0x3130, 0x318F, // Hangul compatibility Jamo
            0xA960, 0xA97F, // Hangul Jamo extended-A
            0xD7B0, 0xD7FF, // Hangul Jamo extended-B
        ] {
            assert!(
                is_cjk_fallback_char(char::from_u32(codepoint).unwrap()),
                "U+{codepoint:04X} should use CJK fallback"
            );
        }

        for codepoint in [0x10FF, 0x1200, 0x3190, 0xA95F, 0xA980, 0xE000] {
            assert!(
                !is_cjk_fallback_char(char::from_u32(codepoint).unwrap()),
                "U+{codepoint:04X} should not use CJK fallback"
            );
        }
    }

    #[test]
    fn decomposed_hangul_jamo_stays_in_one_cjk_run() {
        let mut runs = Vec::new();
        for_text_runs("A한B", |run, cjk| {
            runs.push((run.to_string(), cjk));
        });

        assert_eq!(
            runs,
            vec![
                ("A".to_string(), false),
                ("한".to_string(), true),
                ("B".to_string(), false),
            ]
        );
    }

    #[test]
    fn cjk_runs_are_split_without_breaking_utf8() {
        let mut runs = Vec::new();
        for_text_runs("A中文。B日本語C한국어D", |run, cjk| {
            runs.push((run.to_string(), cjk));
        });

        assert_eq!(
            runs,
            vec![
                ("A".to_string(), false),
                ("中文。".to_string(), true),
                ("B".to_string(), false),
                ("日本語".to_string(), true),
                ("C".to_string(), false),
                ("한국어".to_string(), true),
                ("D".to_string(), false),
            ]
        );
    }

    #[test]
    fn cjk_paragraphs_get_taller_lines_and_latin_paragraphs_do_not() {
        use crate::ast::Inline;
        let latin = [Inline::Text("plain english".into())];
        let cjk = [Inline::Text("這是中文".into())];
        let nested = [Inline::Strong(vec![Inline::Emph(vec![Inline::Text(
            "中".into(),
        )])])];
        let code = [Inline::Text("a ".into()), Inline::Code("終端".into())];
        assert_eq!(paragraph_line_height(&latin, 1.5), 1.5);
        assert!((paragraph_line_height(&cjk, 1.5) - 1.75).abs() < 1e-5);
        assert!((paragraph_line_height(&nested, 1.5) - 1.75).abs() < 1e-5);
        assert!((paragraph_line_height(&code, 1.5) - 1.75).abs() < 1e-5);
        // Compact 1.35 / Relaxed 1.725 keep their ratio to Default.
        assert!((paragraph_line_height(&cjk, 1.35) - 1.575).abs() < 1e-5);
        assert!((paragraph_line_height(&cjk, 1.725) - 2.0125).abs() < 1e-4);
    }

    #[test]
    fn ellipsis_and_dashes_join_cjk_runs_only_next_to_cjk_text() {
        let runs = |t: &str| {
            let mut v = Vec::new();
            for_text_runs(t, |r, c| v.push((r.to_string(), c)));
            v
        };
        assert_eq!(
            runs("等待……與——好"),
            vec![("等待……與——好".to_string(), true)]
        );
        assert_eq!(runs("wait… no"), vec![("wait… no".to_string(), false)]);
        assert_eq!(
            runs("a…中"),
            vec![("a".to_string(), false), ("…中".to_string(), true)]
        );
    }
    #[test]
    fn squeezed_punctuation_keeps_the_original_text_in_the_spans() {
        for text in [
            "他說（好",
            "是）。",
            "「他說」「好」，（對）。「……」",
            "一二三",
        ] {
            let mut out = Vec::new();
            push_styled_runs(
                text,
                &Palette::GITHUB_LIGHT,
                16.0,
                &Style::default(),
                false,
                None,
                &mut out,
            );
            let joined: String = out.iter().map(|s| s.text.as_ref()).collect();
            assert_eq!(joined, text);
            assert!(
                !joined
                    .chars()
                    .any(|c| ('\u{E000}'..='\u{F8FF}').contains(&c)),
                "private-use char in {joined:?}"
            );
        }
        // The squeezed marks sit in their own spans with the narrow face.
        let mut out = Vec::new();
        push_styled_runs(
            "「他說」「好」",
            &Palette::GITHUB_LIGHT,
            16.0,
            &Style::default(),
            false,
            None,
            &mut out,
        );
        let narrow: Vec<&str> = out
            .iter()
            .filter(|s| {
                s.font.map(|f| f.family)
                    == Some(iced::font::Family::Name(crate::fonts::PUNCT_FAMILY))
            })
            .map(|s| s.text.as_ref())
            .collect();
        assert_eq!(narrow, vec!["」"]);
    }
}
