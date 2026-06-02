use crate::app::{ImageState, Message};
use crate::ast::{Block, BlockId, Inline, ListItem};
use crate::diagram::{DiagramCache, DiagramState};
use crate::theme::{Palette, Typography};
use iced::widget::{
    container, image as image_widget, mouse_area, rich_text, row, span, stack, svg as svg_widget,
    text, tooltip, Column, Space,
};
use iced::{Element, Length, Padding};
use std::collections::HashMap;
use std::path::Path;

pub fn block_anchor_id(id: BlockId) -> iced::widget::Id {
    iced::widget::Id::from(format!("block-anchor-{}", id.0))
}

#[derive(Clone, Default)]
pub struct Highlight {
    pub query: String,
    pub current_block: Option<usize>,
    pub current_in_block: usize,
}

pub fn render<'a>(
    blocks: &'a [(BlockId, Block)],
    pal: &Palette,
    typ: &Typography,
    hl: &Highlight,
    viewport: Option<&iced::widget::scrollable::Viewport>,
    cache: &crate::virt::HeightCache,
    image_cache: &'a HashMap<String, ImageState>,
    current_file: Option<&'a Path>,
    folded: &std::collections::HashSet<BlockId>,
    hovered_heading: Option<BlockId>,
    diagram_cache: &'a DiagramCache,
    diagram_theme_id: u32,
) -> Element<'a, Message> {
    // Virt scroll disabled: rebuilding the visible-window Element tree on
    // every scroll event causes per-frame rich_text reflow jank in Iced 0.13.
    // Full render lets Iced's scrollable handle scrolling internally without
    // re-emitting the body tree per delta.
    let _ = (viewport, cache);
    let img_ctx = ImgCtx {
        cache: image_cache,
        current_file,
        diagram_cache,
        diagram_theme_id,
    };
    let mut col = Column::new().spacing(14);
    let mut fold_until: Option<u8> = None;
    for (idx, (id, b)) in blocks.iter().enumerate() {
        if let Block::Heading { level, .. } = b {
            let lvl = *level as u8;
            if let Some(thresh) = fold_until {
                if lvl > thresh {
                    continue;
                }
                fold_until = None;
            }
            let local = if hl.current_block == Some(idx) {
                Some(hl.current_in_block)
            } else {
                None
            };
            let is_folded = folded.contains(id);
            let show_chev = is_folded || hovered_heading == Some(*id);
            col = col.push(render_heading_with_chevron(
                *id, b, pal, typ, &hl.query, local, is_folded, show_chev,
            ));
            if is_folded {
                fold_until = Some(lvl);
            }
            continue;
        }
        if fold_until.is_some() {
            continue;
        }
        let local = if hl.current_block == Some(idx) {
            Some(hl.current_in_block)
        } else {
            None
        };
        col = col.push(
            container(render_block(b, pal, typ, &hl.query, local, &img_ctx))
                .id(block_anchor_id(*id)),
        );
    }

    // Reading column cap: 780px (mdv design system READING_MAX, render.rs).
    let _ = typ.measure_ch;
    container(col).max_width(780.0).into()
}

fn render_heading_with_chevron<'a>(
    id: BlockId,
    b: &'a Block,
    pal: &Palette,
    typ: &Typography,
    query: &str,
    current_in_block: Option<usize>,
    folded: bool,
    visible: bool,
) -> Element<'a, Message> {
    let cache = EMPTY_IMG_CACHE.get_or_init(HashMap::new);
    let dcache = EMPTY_DIAGRAM_CACHE.get_or_init(|| DiagramCache::new(1));
    let img = ImgCtx {
        cache,
        current_file: None,
        diagram_cache: dcache,
        diagram_theme_id: 0,
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
    let chev = mouse_area(
        container(crate::icon::glyph(glyph, 14.0, color)).padding(Padding::from([0, 4])),
    )
    .interaction(iced::mouse::Interaction::Pointer)
    .on_press(Message::ToggleFold(id));
    let chev_layer = container(chev)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Right)
        .align_y(iced::alignment::Vertical::Center);
    let head_clickable = mouse_area(container(head).width(Length::Fill))
        .interaction(iced::mouse::Interaction::Pointer)
        .on_press(Message::ToggleFold(id));
    let stacked = stack![head_clickable, chev_layer];
    container(
        mouse_area(stacked)
            .on_enter(Message::HeadingHoverEnter(id))
            .on_exit(Message::HeadingHoverExit(id)),
    )
    .id(block_anchor_id(id))
    .into()
}

static EMPTY_IMG_CACHE: std::sync::OnceLock<HashMap<String, ImageState>> =
    std::sync::OnceLock::new();
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
    container(rich_text(out))
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
    cache: &'a HashMap<String, ImageState>,
    current_file: Option<&'a Path>,
    diagram_cache: &'a DiagramCache,
    diagram_theme_id: u32,
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
        counter: 0,
        current_in_block,
        pal: *pal,
    };
    match b {
        Block::Heading { level, inlines, .. } => {
            let size = match level {
                1 => typ.h1_size,
                2 => typ.h2_size,
                3 => typ.h3_size,
                4 => typ.h4_size,
                5 => typ.h5_size,
                _ => typ.h6_size,
            };
            let spans = inline_spans(inlines, pal, size, &mut ctx);
            rich_text_links(spans)
        }
        Block::Paragraph(inlines) => {
            let spans = inline_spans(inlines, pal, typ.body_size, &mut ctx);
            rich_text_links(spans)
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
            let body = container(rich_text(out))
                .padding(Padding::from(14))
                .style(move |_| container::Style {
                    background: Some(pal_c.code_bg.into()),
                    border: iced::Border {
                        color: pal_c.code_border,
                        width: 1.0,
                        radius: 8.0.into(),
                    },
                    ..Default::default()
                })
                .width(Length::Fill);
            let code_str = code.to_string();
            let icon_glyph = crate::icon::glyph(crate::icon::ic::COPY, 13.0, pal_c.muted);
            let copy_btn =
                container(
                    mouse_area(container(icon_glyph).padding(Padding::from([4, 6])).style(
                        move |_| container::Style {
                            background: Some(pal_c.code_bg.into()),
                            border: iced::Border {
                                color: pal_c.code_border,
                                width: 1.0,
                                radius: 6.0.into(),
                            },
                            ..Default::default()
                        },
                    ))
                    .interaction(iced::mouse::Interaction::Pointer)
                    .on_press(Message::CopyCode(code_str)),
                )
                .padding(Padding::from([6, 8]))
                .align_x(iced::alignment::Horizontal::Right)
                .width(Length::Fill);
            stack![body, copy_btn].into()
        }
        Block::Blockquote(blocks) => {
            let inner = blocks.iter().fold(Column::new().spacing(8), |c, b| {
                c.push(render_block(b, pal, typ, query, current_in_block, img))
            });
            let pal_q = *pal;
            container(inner)
                .padding(Padding {
                    top: 2.0,
                    right: 14.0,
                    bottom: 2.0,
                    left: 17.0,
                })
                .width(Length::Fill)
                .style(move |_| container::Style {
                    border: iced::Border {
                        color: pal_q.accent,
                        width: 0.0,
                        radius: 0.0.into(),
                    },
                    shadow: iced::Shadow {
                        color: pal_q.accent,
                        offset: iced::Vector::new(-1.5, 0.0),
                        blur_radius: 0.0,
                    },
                    ..Default::default()
                })
                .into()
        }
        Block::Diagram { source, hash, kind } => {
            if matches!(kind, crate::ast::DiagramKind::Math) {
                render_math_block(*hash, source, pal, typ, img)
            } else {
                render_diagram(*hash, source, pal, typ, img)
            }
        }
        Block::List { ordered, items } => render_list(*ordered, items, pal, typ, &mut ctx, img),
        Block::Table { headers, rows } => render_table(headers, rows, pal, typ, &mut ctx),
        Block::Image { url, alt } => render_image(url, alt, pal, img),
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

/// `rich_text` whose link spans dispatch their carried `Message` on click.
/// The span `Link` type is `Message`, so the click handler is the identity.
fn rich_text_links<'a>(spans: Vec<RtSpan<'a>>) -> Element<'a, Message> {
    rich_text(spans).on_link_click(|m| m).into()
}

struct HlCtx<'a> {
    query: &'a str,
    counter: usize,
    current_in_block: Option<usize>,
    pal: Palette,
}

fn inline_spans<'a>(
    inlines: &'a [Inline],
    pal: &Palette,
    size: f32,
    ctx: &mut HlCtx<'_>,
) -> Vec<RtSpan<'a>> {
    let mut out = Vec::new();
    for i in inlines {
        push_span(i, &mut out, pal, size, Style::default(), ctx);
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
    monospace: bool,
    bg: Option<iced::Color>,
) -> RtSpan<'a> {
    let mut font = if monospace {
        iced::Font::MONOSPACE
    } else {
        iced::Font::with_name("Inter")
    };
    if st.italic {
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
    } else if monospace {
        s = s.background(pal.code_bg);
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
        out.push(make_span(text_str, pal, size, st, monospace, None));
        return;
    }
    let lower_text = text_str.to_lowercase();
    let lower_q = ctx.query.to_lowercase();
    let mut cursor = 0usize;
    while let Some(rel) = lower_text[cursor..].find(&lower_q) {
        let abs = cursor + rel;
        if abs > cursor {
            out.push(make_span(
                &text_str[cursor..abs],
                pal,
                size,
                st,
                monospace,
                None,
            ));
        }
        let end = abs + lower_q.len();
        let is_current = ctx.current_in_block == Some(ctx.counter);
        let bg = if is_current {
            ctx.pal.match_current_bg
        } else {
            ctx.pal.match_bg
        };
        out.push(make_span(
            &text_str[abs..end],
            pal,
            size,
            st,
            monospace,
            Some(bg),
        ));
        ctx.counter += 1;
        cursor = end;
    }
    if cursor < text_str.len() {
        out.push(make_span(
            &text_str[cursor..],
            pal,
            size,
            st,
            monospace,
            None,
        ));
    }
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
        out.push(
            span(text_str)
                .font(iced::Font::MONOSPACE)
                .size(size)
                .color(color),
        );
        return;
    }
    let lower_text = text_str.to_lowercase();
    let lower_q = ctx.query.to_lowercase();
    let mut cursor = 0usize;
    while let Some(rel) = lower_text[cursor..].find(&lower_q) {
        let abs = cursor + rel;
        if abs > cursor {
            out.push(
                span(&text_str[cursor..abs])
                    .font(iced::Font::MONOSPACE)
                    .size(size)
                    .color(color),
            );
        }
        let end = abs + lower_q.len();
        let is_current = ctx.current_in_block == Some(ctx.counter);
        let bg = if is_current {
            ctx.pal.match_current_bg
        } else {
            ctx.pal.match_bg
        };
        out.push(
            span(&text_str[abs..end])
                .font(iced::Font::MONOSPACE)
                .size(size)
                .color(color)
                .background(bg),
        );
        ctx.counter += 1;
        cursor = end;
    }
    if cursor < text_str.len() {
        out.push(
            span(&text_str[cursor..])
                .font(iced::Font::MONOSPACE)
                .size(size)
                .color(color),
        );
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
    pal: &Palette,
    img: &ImgCtx<'a>,
) -> Element<'a, Message> {
    use crate::app::{is_remote_url, resolve_image_path};
    let placeholder = |msg: String| -> Element<'a, Message> { text(msg).color(pal.muted).into() };
    if is_remote_url(url) {
        match img.cache.get(url) {
            Some(ImageState::Loaded(h)) => mouse_area(image_widget(h.clone()))
                .on_press(Message::OpenImageZoom(url.to_string()))
                .into(),
            Some(ImageState::LoadedSvg { svg, .. }) => mouse_area(svg_widget(svg.clone()))
                .on_press(Message::OpenImageZoom(url.to_string()))
                .into(),
            Some(ImageState::Failed) => placeholder(format!("[image failed: {alt} ({url})]")),
            _ => placeholder(format!("[loading: {alt} ({url})]")),
        }
    } else {
        match resolve_image_path(url, img.current_file) {
            Some(p) if p.exists() => {
                let is_svg = p
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("svg"))
                    .unwrap_or(false);
                let url_for_zoom = p.to_string_lossy().to_string();
                if is_svg {
                    mouse_area(svg_widget(iced::widget::svg::Handle::from_path(p.clone())))
                        .on_press(Message::OpenImageZoom(url_for_zoom))
                        .into()
                } else {
                    mouse_area(image_widget(p.clone()))
                        .on_press(Message::OpenImageZoom(url_for_zoom))
                        .into()
                }
            }
            _ => placeholder(format!("[image missing: {alt} ({url})]")),
        }
    }
}

/// Render a `Block::Diagram { kind: Math }`. Unlike mermaid/dot diagrams,
/// display math reads as document content, not a figure — so no card, no
/// border, no copy/zoom chrome. Just the centered formula on its own line.
///
/// - `Ready` → centered image of the rendered SVG; click copies the LaTeX.
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
            // Click the formula → copy its LaTeX source (reuses CopyCode's
            // clipboard write + "Copied" toast). Pointer cursor hints it.
            let clickable = mouse_area(el)
                .interaction(iced::mouse::Interaction::Pointer)
                .on_press(Message::CopyCode(source.to_string()));
            container(clickable)
                .center_x(Length::Fill)
                .padding(Padding::from([4, 0]))
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
                                radius: 5.0.into(),
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
                        radius: 8.0.into(),
                    },
                    ..Default::default()
                });
            // Whole-area click → zoom (mirrors image click-to-zoom UX).
            let clickable = mouse_area(body)
                .interaction(iced::mouse::Interaction::Pointer)
                .on_press(Message::DiagramZoom(hash));

            // Copy-source button only — whole-diagram click already opens
            // the zoom modal. Matches the CodeBlock copy-button styling.
            let copy_icon = container(crate::icon::glyph(crate::icon::ic::COPY, 13.0, pal_c.muted))
                .padding(Padding::from([4, 6]))
                .style(move |_| container::Style {
                    background: Some(pal_c.code_bg.into()),
                    border: iced::Border {
                        color: pal_c.code_border,
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                });
            let copy_btn = mouse_area(copy_icon)
                .interaction(iced::mouse::Interaction::Pointer)
                .on_press(Message::CopyDiagramSource(hash));
            let overlay = container(copy_btn)
                .padding(Padding::from([6, 8]))
                .align_x(iced::alignment::Horizontal::Right)
                .width(Length::Fill);
            stack![clickable, overlay].into()
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
                                radius: 5.0.into(),
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
    .padding(Padding::from(14))
    .style(move |_| container::Style {
        background: Some(pal_c.code_bg.into()),
        border: iced::Border {
            color: pal_c.code_border,
            width: 1.0,
            radius: 8.0.into(),
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
            background: Some(pal_c.surface_alt.into()),
            border: iced::Border {
                color: pal_c.rule,
                width: 1.0,
                radius: 4.0.into(),
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
        let bullet = match (ordered, it.task) {
            (_, Some(true)) => "✓".to_string(),
            (_, Some(false)) => "○".to_string(),
            (true, _) => format!("{}.", idx + 1),
            (false, _) => "•".to_string(),
        };
        let inner = it.blocks.iter().fold(Column::new().spacing(6), |c, b| {
            c.push(render_block_inner(b, pal, typ, ctx, img))
        });
        col = col.push(
            row![
                container(text(bullet).color(pal.accent).size(typ.body_size))
                    .width(Length::Fixed(28.0)),
                inner
            ]
            .spacing(6),
        );
    }
    col.into()
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
            rich_text_links(spans)
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

    let make_cell = |content: Element<'a, Message>, is_header: bool| -> Element<'a, Message> {
        container(content)
            .padding(Padding::from([8, 12]))
            .width(Length::FillPortion(1))
            .style(move |_| container::Style {
                background: if is_header {
                    Some(pal_t.surface_alt.into())
                } else {
                    None
                },
                ..Default::default()
            })
            .into()
    };

    let mut header_row = iced::widget::Row::new().spacing(0);
    for i in 0..cols {
        let content: Element<'a, Message> = if let Some(cell) = headers.get(i) {
            let spans = inline_spans(cell, pal, typ.body_size, ctx);
            rich_text_links(spans)
        } else {
            text("").into()
        };
        header_row = header_row.push(make_cell(content, true));
    }

    let mut grid = Column::new().spacing(0);
    grid = grid.push(
        container(header_row)
            .style(move |_| container::Style {
                border: iced::Border {
                    color: pal_t.code_border,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .width(Length::Fill),
    );

    for row_cells in rows {
        let mut r = iced::widget::Row::new().spacing(0);
        for i in 0..cols {
            let content: Element<'a, Message> = if let Some(cell) = row_cells.get(i) {
                let spans = inline_spans(cell, pal, typ.body_size, ctx);
                rich_text_links(spans)
            } else {
                text("").into()
            };
            r = r.push(make_cell(content, false));
        }
        grid = grid.push(
            container(r)
                .style(move |_| container::Style {
                    border: iced::Border {
                        color: pal_t.code_border,
                        width: 1.0,
                        radius: 0.0.into(),
                    },
                    ..Default::default()
                })
                .width(Length::Fill),
        );
    }

    container(grid)
        .width(Length::Fill)
        .style(move |_| container::Style {
            border: iced::Border {
                color: pal_t.code_border,
                width: 1.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        })
        .into()
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
