//! The full-page workspace search results view (⌘⇧F).

use super::*;
// Explicit import: the `column!` macro is otherwise ambiguous between the glob
// import and iced's exported macro.
use iced::widget::column;

/// Vault-wide search results page (Zed-style). Fills the reader area. Query bar
/// plus match count on top, results grouped under collapsible file headers.
/// Each match shows surrounding context lines with line numbers and the matched
/// span highlighted. Arrow keys move the cursor over visible matches,
/// Enter/click open the file at the line, Esc exits.
#[allow(clippy::too_many_arguments)] // cohesive view fn; splitting args adds noise
pub(in crate::app) fn vault_search_page<'a>(
    query: &'a str,
    searched_query: Option<&str>,
    hits: &'a [crate::vault_search::VaultHit],
    // Distinct files in `hits`; computed once in VaultSearchDone.
    file_count: usize,
    cursor: usize,
    truncated: bool,
    collapsed: &HashSet<PathBuf>,
    workspace: Option<&std::path::Path>,
    viewport: Option<&iced::widget::scrollable::Viewport>,
    pal: Palette,
) -> Element<'a, Message> {
    // The displayed results reflect `searched_query`; if the live `query` has
    // since been edited, prompt for Enter rather than showing a stale count.
    let edited = searched_query != Some(query);
    let count_text = if query.is_empty() {
        String::new()
    } else if edited {
        "press Enter to search".to_string()
    } else if truncated {
        format!("{}+ matches (refine query)", crate::vault_search::MAX_HITS)
    } else {
        format!("{} matches in {} files", hits.len(), file_count)
    };
    let bar = container(
        irow![
            text_input("Search all files… (press Enter)", query)
                .id(App::vault_input_id())
                .on_input(Message::VaultQueryChanged)
                .on_submit(Message::VaultEnter)
                .padding(Padding::from([8, 12]))
                .size(14)
                .style(move |_, _| iced::widget::text_input::Style {
                    background: Color::TRANSPARENT.into(),
                    border: Border::default(),
                    icon: pal.muted,
                    placeholder: pal.subtle,
                    value: pal.fg,
                    selection: pal.selection,
                }),
            text(count_text).size(12).color(pal.subtle),
        ]
        .spacing(12)
        .align_y(iced::Alignment::Center),
    )
    .padding(Padding::from([6, 14]))
    .width(Length::Fill);

    let mut list = Column::new().spacing(0).padding(Padding::from([6, 10]));
    if query.is_empty() {
        list = list.push(
            container(
                text("Type a query and press Enter to search every file")
                    .color(pal.subtle)
                    .size(13),
            )
            .padding(14),
        );
    } else if edited {
        // Query typed but not yet searched (search runs on Enter, not per key).
        list = list
            .push(container(text("Press Enter to search").color(pal.subtle).size(13)).padding(14));
    } else if hits.is_empty() {
        list = list.push(container(text("No matches").color(pal.subtle).size(13)).padding(14));
    } else {
        // Virtualized results list. Walk all hits once to build a flat row model
        // (one Header per file run, one Hit per visible match) with estimated
        // heights, then render only the rows intersecting the viewport plus an
        // overscan — and always the cursor row, so the cursor-follow scroll
        // Operation can measure its real bounds by anchor id. Skipped rows above
        // and below collapse into `Space` of their summed estimated heights so
        // the scrollbar extent and positions stay correct.
        enum Row {
            Header {
                path: PathBuf,
                run_count: usize,
                folded: bool,
            },
            Hit {
                hi: usize,
                vis_idx: usize,
            },
        }

        // Exact per-row heights. Context lines are fixed single-line rows
        // (SIZE * LINE_H, no wrapping), so these estimates match the real layout
        // — which keeps the virtualization spacers from drifting against the
        // measured scroll offset.
        const LINE_PX: f32 = 12.5 * 1.4; // context_line_row fixed height
        const ROW_GAP: f32 = 1.0; // Column::spacing(1) between context lines
        const ROW_PAD_H: f32 = 12.0; // hit button padding (6 top + 6 bottom)
        const HEADER_H: f32 = 12.0 + 13.0 * 1.3 + 2.0; // header button: pad + 13px line
        let hit_height = |hi: usize| -> f32 {
            let n = hits[hi].context.len() as f32;
            ROW_PAD_H + n * LINE_PX + (n - 1.0).max(0.0) * ROW_GAP
        };

        // Build the row model in file-walk order.
        let mut rows: Vec<Row> = Vec::new();
        let mut vis = 0usize;
        let mut idx = 0usize;
        while idx < hits.len() {
            let path = hits[idx].path.clone();
            let folded = collapsed.contains(&path);
            let run_start = idx;
            while idx < hits.len() && hits[idx].path == path {
                idx += 1;
            }
            let run_count = idx - run_start;
            rows.push(Row::Header {
                path,
                run_count,
                folded,
            });
            if folded {
                continue;
            }
            for hi in run_start..run_start + run_count {
                rows.push(Row::Hit { hi, vis_idx: vis });
                vis += 1;
            }
        }

        // Cumulative tops + total height from the estimates.
        let mut tops: Vec<f32> = Vec::with_capacity(rows.len());
        let mut y = 0.0f32;
        for r in &rows {
            tops.push(y);
            y += match r {
                Row::Header { .. } => HEADER_H,
                Row::Hit { hi, .. } => hit_height(*hi),
            };
        }
        let total_h = y;

        // Viewport window in content coordinates (fall back to "render all" until
        // the first scroll event lands a viewport).
        let virtualize = std::env::var("RMDV_NO_VIRT").is_err();
        let (win_top, win_bot) = match (virtualize, viewport) {
            (true, Some(vp)) => {
                let off = vp.absolute_offset().y;
                let vh = vp.bounds().height;
                const OVERSCAN: f32 = 600.0;
                (off - OVERSCAN, off + vh + OVERSCAN)
            }
            _ => (0.0, total_h),
        };

        let row_h = |i: usize| -> f32 {
            match &rows[i] {
                Row::Header { .. } => HEADER_H,
                Row::Hit { hi, .. } => hit_height(*hi),
            }
        };
        let in_window = |i: usize| -> bool {
            let top = tops[i];
            let bot = top + row_h(i);
            bot >= win_top && top <= win_bot
        };

        // Render with a leading spacer for skipped rows, the windowed rows, and a
        // trailing spacer. The cursor's hit row is force-rendered even if off the
        // window so its anchor id exists for measurement.
        let mut skipped_above = 0.0f32;
        let mut pending_below = 0.0f32;
        let mut started = false;
        for (i, r) in rows.iter().enumerate() {
            let is_cursor_row = matches!(r, Row::Hit { vis_idx, .. } if *vis_idx == cursor);
            let render = in_window(i) || is_cursor_row;
            if !render {
                if started {
                    pending_below += row_h(i);
                } else {
                    skipped_above += row_h(i);
                }
                continue;
            }
            if !started {
                if skipped_above > 0.0 {
                    list = list.push(Space::new().height(skipped_above));
                }
                started = true;
            } else if pending_below > 0.0 {
                // Reclaim a gap created by jumping to the cursor row out of window.
                list = list.push(Space::new().height(pending_below));
                pending_below = 0.0;
            }

            match r {
                Row::Header {
                    path,
                    run_count,
                    folded,
                } => {
                    let rel = workspace
                        .and_then(|ws| path.strip_prefix(ws).ok())
                        .unwrap_or(path)
                        .to_string_lossy()
                        .into_owned();
                    let chevron = if *folded {
                        icon::ic::CHEVRON_RIGHT
                    } else {
                        icon::ic::CHEVRON_DOWN
                    };
                    let header_label = if *folded {
                        format!("{rel}  ({run_count})")
                    } else {
                        rel
                    };
                    let header_row = irow![
                        icon::glyph(chevron, 13.0, pal.accent),
                        text(header_label).size(13).color(pal.accent),
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center);
                    let path = path.clone();
                    let header = button(header_row)
                        .padding(Padding::from([6, 8]))
                        .width(Length::Fill)
                        .style(move |_, status| button::Style {
                            background: match status {
                                button::Status::Hovered => Some(Background::Color(pal.surface_alt)),
                                _ => None,
                            },
                            text_color: pal.accent,
                            border: Border {
                                radius: 5.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        })
                        .on_press(Message::VaultToggleFile(path));
                    list = list.push(header);
                }
                Row::Hit { hi, vis_idx } => {
                    let hi = *hi;
                    let is_cursor = *vis_idx == cursor;
                    let hit = &hits[hi];
                    let mut block = Column::new().spacing(1);
                    for cl in &hit.context {
                        block = block.push(context_line_row(
                            cl,
                            hit.col_start,
                            hit.col_end,
                            is_cursor,
                            pal,
                        ));
                    }
                    let row = button(block)
                        .padding(Padding::from([6, 8]))
                        .width(Length::Fill)
                        .style(move |_, status| button::Style {
                            background: match (is_cursor, status) {
                                (true, _) => Some(Background::Color(pal.surface_alt)),
                                (_, button::Status::Hovered) => {
                                    Some(Background::Color(pal.code_bg))
                                }
                                _ => None,
                            },
                            text_color: pal.fg,
                            border: Border {
                                radius: 5.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        })
                        .on_press(Message::VaultOpenHit(hi));
                    // Stable id so cursor-follow can scroll by measured bounds.
                    let row = container(row)
                        .id(App::vault_match_anchor_id(*vis_idx))
                        .width(Length::Fill);
                    list = list.push(row);
                }
            }
        }
        // Trailing spacer for everything skipped after the last rendered row.
        if pending_below > 0.0 {
            list = list.push(Space::new().height(pending_below));
        }
    }

    // Constrain the results column to a comfortable reading width so long lines
    // wrap instead of sprawling edge-to-edge; centre it in the viewport.
    let list = container(container(list).max_width(1100.0).width(Length::Fill))
        .width(Length::Fill)
        .align_x(iced::Alignment::Center);

    let body = scrollable(list)
        .id(App::vault_scroll_id())
        .on_scroll(Message::VaultScrolled)
        .height(Length::Fill)
        .width(Length::Fill)
        .direction(slim_scroll_direction())
        .style(move |_, status| sleek_scrollable_style(status, pal, true));

    let divider = container(Space::new().height(1.0))
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(pal.rule.into()),
            ..Default::default()
        });

    let footer = container(
        text("↑↓ move · ⏎ open · esc exit")
            .size(11)
            .color(pal.subtle),
    )
    .padding(Padding::from([6, 14]))
    .width(Length::Fill);

    container(column![bar, divider, body, footer])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(pal.bg.into()),
            ..Default::default()
        })
        .into()
}

/// One context line: a fixed-width line-number gutter + the source line as
/// markdown-highlighted `rich_text`. On the match line the matched character
/// span gets a highlight background; context lines are dimmed. Long lines wrap
/// within the filled text column (no horizontal blow-out).
pub(in crate::app) fn context_line_row<'a>(
    cl: &'a crate::vault_search::ContextLine,
    col_start: usize,
    col_end: usize,
    is_cursor: bool,
    pal: Palette,
) -> Element<'a, Message> {
    use iced::widget::{rich_text, span};

    const SIZE: f32 = 12.5;
    const LINE_H: f32 = 1.4;

    // Plain (shrink-height) gutter text; a fixed-width box would let the row's
    // height be driven by container sizing rather than the text line.
    let gutter = text(format!("{:>5} ", cl.number))
        .size(SIZE)
        .line_height(LINE_H)
        .font(iced::Font::MONOSPACE)
        .color(pal.subtle);

    // Byte range of the match within this line, for the highlight background.
    let (mb_start, mb_end) = if cl.is_match {
        let s = byte_index_for_char(&cl.text, col_start);
        let e = byte_index_for_char(&cl.text, col_end);
        (s, e)
    } else {
        (0, 0)
    };
    let match_bg = if is_cursor {
        pal.match_current_bg
    } else {
        pal.match_bg
    };

    // Build (byte-range, color) segments from the highlight spans: highlighted
    // ranges get their style colour, gaps get the base colour. Then overlay the
    // match window by splitting any segment that straddles it.
    let line = &cl.text;
    let base_color = if cl.is_match { pal.fg } else { pal.muted };
    let mut segs: Vec<(usize, usize, iced::Color)> = Vec::new();
    let mut cursor = 0usize;
    for sp in &cl.spans {
        let r = sp.range.clone();
        // Spans may overlap (highlight() emits nested captures); drop any that
        // starts inside a range already claimed, like the code-block renderer.
        if r.start < cursor || r.start >= line.len() {
            continue;
        }
        let end = r.end.min(line.len());
        if r.start > cursor {
            segs.push((cursor, r.start, base_color));
        }
        if end > r.start {
            segs.push((r.start, end, crate::render::style_color(sp.style, &pal)));
        }
        cursor = end;
    }
    if cursor < line.len() {
        segs.push((cursor, line.len(), base_color));
    }

    let mut rt: Vec<iced::advanced::text::Span<'a, Message, iced::Font>> = Vec::new();
    for (lo, hi, color) in segs {
        // Split this segment on the match window so the overlap carries match_bg.
        let parts: [(usize, usize, Option<iced::Color>); 3] =
            if cl.is_match && mb_end > lo && mb_start < hi {
                [
                    (lo, mb_start.max(lo), None),
                    (mb_start.max(lo), mb_end.min(hi), Some(match_bg)),
                    (mb_end.min(hi), hi, None),
                ]
            } else {
                [(lo, hi, None), (hi, hi, None), (hi, hi, None)]
            };
        for (a, b, bg) in parts {
            if a >= b {
                continue;
            }
            let mut s = span(&line[a..b])
                .font(iced::Font::MONOSPACE)
                .size(SIZE)
                .line_height(LINE_H)
                .color(color);
            if let Some(c) = bg {
                s = s.background(c);
            }
            rt.push(s);
        }
    }
    if rt.is_empty() {
        rt.push(
            span(" ")
                .font(iced::Font::MONOSPACE)
                .size(SIZE)
                .line_height(LINE_H)
                .color(base_color),
        );
    }

    // Single visual line per source line (Zed-style): no wrapping, so every row
    // is exactly one line tall. This keeps long / CJK / table lines from blowing
    // the row height up vertically AND makes the virtualization height estimate
    // exact. Overflow past the column width is clipped by the parent.
    let body = rich_text(rt)
        .size(SIZE)
        .line_height(LINE_H)
        .wrapping(iced::widget::text::Wrapping::None)
        .width(Length::Fill);

    irow![gutter, body]
        .width(Length::Fill)
        .height(Length::Fixed(SIZE * LINE_H))
        .spacing(4)
        .align_y(iced::Alignment::Center)
        .clip(true)
        .into()
}

/// Byte offset of the `n`-th char in `s` (clamped to `s.len()`).
pub(in crate::app) fn byte_index_for_char(s: &str, n: usize) -> usize {
    s.char_indices().nth(n).map(|(b, _)| b).unwrap_or(s.len())
}
