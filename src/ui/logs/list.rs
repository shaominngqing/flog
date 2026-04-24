//! Log list rendering — the heart of the Logs view.
//!
//! `draw_log_list` is the variable-height row walker that resolves the
//! viewport start, renders rows (including wrapped messages, extra lines,
//! and collapsed stack traces), and writes back `row_to_filtered_idx` so
//! the event layer can map mouse clicks to log entries.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::domain::LogLevel;

use super::highlight::{auto_highlight, highlight_with_filter};
use super::toolbar::level_pill;
use super::{
    compute_visible_entry_start, draw_no_matching_logs, draw_not_connected, draw_waiting_for_logs,
    entry_row_count, level_color, message_color, repeat_bar, safe_pad, tag_color, wrap_text,
    ERROR_ROW_BG, LEVEL_WIDTH, MAX_STACK_PREVIEW_LINES, MAX_WRAP_LINES, TAG_WIDTH, TIME_WIDTH,
    WARNING_ROW_BG,
};
use super::{BASE, BLUE, OVERLAY0, PINK, RED, SURFACE0, SURFACE1, YELLOW};

pub(super) fn draw_log_list(f: &mut Frame, app: &mut App, area: Rect) {
    let height = area.height as usize;
    let filtered_count = app.filtered_count(); // forces filter rebuild if dirty
    let total_width = area.width as usize;
    let full_width = app.layout.width as usize;

    // ── Empty states ──
    if filtered_count == 0 {
        if app.store.is_empty() && app.active_app_id.is_none() {
            draw_not_connected(f, app, area);
        } else if app.store.is_empty() && app.active_app_id.is_some() {
            draw_waiting_for_logs(f, app, area);
        } else {
            draw_no_matching_logs(f, app, area);
        }
        app.layout.row_to_filtered_idx.clear();
        app.layout.rendered_to_end = true;
        app.layout.visible_entry_count = 0;
        app.scroll_offset = 0;
        app.selected = 0;
        return;
    }

    // Copy filtered indices so we can access &app.store without borrow conflicts.
    let fi_vec: Vec<usize> = app.filtered_indices().to_vec();

    // ════════════════════════════════════════════════════════════════
    //  PHASE 1: Resolve scroll position (the renderer is the authority)
    // ════════════════════════════════════════════════════════════════

    if app.auto_scroll {
        // Walk backwards from the last entry to find where the viewport starts.
        let mut rows_used = 0usize;
        let mut start_fi = filtered_count;
        let mut idx = filtered_count;
        while idx > 0 {
            idx -= 1;
            let rows = entry_row_count_from_store(&app.store, fi_vec[idx], full_width);
            if rows_used + rows > height && rows_used > 0 {
                break;
            }
            rows_used += rows;
            start_fi = idx;
            if rows_used >= height {
                break;
            }
        }
        app.scroll_offset = start_fi;
        // Keep selected within visible range but don't force it to the last entry,
        // so users can click to select while auto_scroll is active.
        if app.selected < start_fi || app.selected >= filtered_count {
            app.selected = filtered_count - 1;
        }
    } else {
        // Clamp
        app.scroll_offset = app.scroll_offset.min(filtered_count.saturating_sub(1));
        app.selected = app.selected.min(filtered_count.saturating_sub(1));

        // If selected is above viewport, scroll up to it
        if app.selected < app.scroll_offset {
            app.scroll_offset = app.selected;
        }

        // Forward scan to find which entries are visible from scroll_offset
        let mut rows_used = 0usize;
        let mut last_visible_fi = app.scroll_offset;
        let mut fi = app.scroll_offset;
        while fi < filtered_count && rows_used < height {
            let rows = entry_row_count_from_store(&app.store, fi_vec[fi], full_width);
            if rows_used + rows > height && rows_used > 0 {
                break;
            }
            rows_used += rows;
            last_visible_fi = fi;
            fi += 1;
        }

        // If selected is below the visible range, scroll down to show it
        if app.selected > last_visible_fi {
            let mut rows_back = 0usize;
            let mut new_start = app.selected;
            let mut si = app.selected;
            loop {
                let rows = entry_row_count_from_store(&app.store, fi_vec[si], full_width);
                if rows_back + rows > height && rows_back > 0 {
                    break;
                }
                rows_back += rows;
                new_start = si;
                if si == 0 || rows_back >= height {
                    break;
                }
                si -= 1;
            }
            app.scroll_offset = new_start;
        }
    }

    // ════════════════════════════════════════════════════════════════
    //  PHASE 2: Render entries from scroll_offset until viewport is full
    // ════════════════════════════════════════════════════════════════

    let start = compute_visible_entry_start(filtered_count, app.scroll_offset);
    let _ = height; // kept in scope; row-walker below uses it directly
    let selected = app.selected;
    let indices: Vec<usize> = fi_vec[start..filtered_count].to_vec();

    let mut row_map: Vec<usize> = Vec::new();
    let mut lines: Vec<Line> = Vec::new();

    for (vi, &store_idx) in indices.iter().enumerate() {
        let fi = start + vi;
        let is_selected = fi == selected;

        if let Some(entry) = app.store.get(store_idx) {
            let lc = level_color(entry.level);

            let row_bg = if is_selected {
                SURFACE1
            } else {
                match entry.level {
                    LogLevel::Error => ERROR_ROW_BG,
                    LogLevel::Warning => WARNING_ROW_BG,
                    _ => BASE,
                }
            };

            let mc = message_color(entry.level);
            let base = Style::default().fg(mc).bg(row_bg);
            let dim_s = Style::default().fg(SURFACE1).bg(row_bg);

            let cursor_color = if entry.level == LogLevel::Error {
                RED
            } else {
                BLUE
            };
            let cursor = if is_selected {
                Span::styled("▎", Style::default().fg(cursor_color).bg(row_bg))
            } else {
                Span::styled(" ", Style::default().bg(row_bg))
            };

            let bm = if app.is_bookmarked(store_idx) {
                Span::styled("● ", Style::default().fg(YELLOW).bg(row_bg))
            } else {
                Span::styled("  ", Style::default().bg(row_bg))
            };

            let time = safe_pad(
                if entry.timestamp.is_empty() {
                    ""
                } else {
                    &entry.timestamp
                },
                TIME_WIDTH,
            );
            let time_style = Style::default()
                .fg(OVERLAY0)
                .bg(row_bg)
                .add_modifier(Modifier::DIM);

            let level_span = level_pill(entry.level, row_bg);

            // Separator: 3 rows (blank + divider + blank)
            if entry.tag == "────" {
                if lines.len() < height {
                    lines.push(Line::from(Span::styled(
                        " ".repeat(total_width),
                        Style::default().bg(BASE),
                    )));
                    row_map.push(fi);
                }
                if lines.len() < height {
                    let line_char = "─".repeat(total_width.saturating_sub(1));
                    lines.push(Line::from(vec![
                        Span::styled(" ", Style::default().bg(BASE)),
                        Span::styled(line_char, Style::default().fg(SURFACE1).bg(BASE)),
                    ]));
                    row_map.push(fi);
                }
                if lines.len() < height {
                    lines.push(Line::from(Span::styled(
                        " ".repeat(total_width),
                        Style::default().bg(BASE),
                    )));
                    row_map.push(fi);
                }
                if lines.len() >= height {
                    break;
                }
                continue;
            }

            let header_spans: Vec<Span> = vec![
                cursor,
                bm,
                Span::styled(time, time_style),
                Span::styled(" ", dim_s),
                level_span,
                Span::styled(" ", dim_s),
                Span::styled(
                    safe_pad(&entry.tag, TAG_WIDTH),
                    Style::default().fg(tag_color(&entry.tag)).bg(row_bg),
                ),
                Span::styled(" ", dim_s),
            ];

            let header_width: usize = header_spans.iter().map(|s| s.content.width()).sum();

            let full_msg = if entry.repeat_count > 1 {
                format!("{} {}", repeat_bar(entry.repeat_count, 8), entry.message)
            } else {
                entry.message.clone()
            };

            let wrap_width = full_width.saturating_sub(header_width + 1);
            let wrapped = wrap_text(&full_msg, wrap_width, MAX_WRAP_LINES);

            // First line
            let first_text = wrapped.first().map(|s| s.as_str()).unwrap_or("");
            let mut spans = header_spans;
            if entry.repeat_count > 1 && !first_text.is_empty() {
                let bar_str = repeat_bar(entry.repeat_count, 8);
                let bar_display_len = bar_str.width();
                // +1 for the space between bar and message
                let split_at = (bar_display_len + 1).min(first_text.width());
                let bar_part: String = first_text.chars().take(split_at).collect();
                let msg_part: String = first_text.chars().skip(split_at).collect();
                if !bar_part.is_empty() {
                    spans.push(Span::styled(bar_part, Style::default().fg(PINK).bg(row_bg)));
                }
                if !app.filter.search_query.is_empty() {
                    spans.extend(highlight_with_filter(&msg_part, &app.filter, base));
                } else {
                    spans.extend(auto_highlight(&msg_part, base));
                }
            } else if !app.filter.search_query.is_empty() {
                spans.extend(highlight_with_filter(first_text, &app.filter, base));
            } else {
                spans.extend(auto_highlight(first_text, base));
            }
            let used: usize = spans.iter().map(|s| s.content.width()).sum();
            if used < total_width {
                spans.push(Span::styled(
                    " ".repeat(total_width - used),
                    Style::default().bg(row_bg),
                ));
            }
            lines.push(Line::from(spans));
            row_map.push(fi);

            // Helper: build an empty header prefix aligned to columns
            // cursor(1) + bookmark(2) + time(TIME_WIDTH) + sep(1) + level(LEVEL_WIDTH) + sep(1) + tag(TAG_WIDTH) + sep(1)
            let empty_prefix = |sel: bool, bg: Color| -> Vec<Span<'static>> {
                let cursor_s = if sel {
                    Span::styled("▎", Style::default().fg(cursor_color).bg(bg))
                } else {
                    Span::styled(" ", Style::default().bg(bg))
                };
                let blank = Style::default().bg(bg);
                vec![
                    cursor_s,
                    Span::styled("  ", blank),                   // bookmark
                    Span::styled(" ".repeat(TIME_WIDTH), blank), // time
                    Span::styled(" ", blank),                    // sep
                    Span::styled(" ".repeat(LEVEL_WIDTH), blank), // level
                    Span::styled(" ", blank),                    // sep
                    Span::styled(" ".repeat(TAG_WIDTH), blank),  // tag
                    Span::styled(" ", blank),                    // sep
                ]
            };

            // Wrapped continuation lines
            for wrap_line in wrapped.iter().skip(1) {
                if lines.len() >= height {
                    break;
                }
                let mut ws = empty_prefix(is_selected, row_bg);
                if !app.filter.search_query.is_empty() {
                    ws.extend(highlight_with_filter(wrap_line, &app.filter, base));
                } else {
                    ws.extend(auto_highlight(wrap_line, base));
                }
                let used: usize = ws.iter().map(|s| s.content.width()).sum();
                if used < total_width {
                    ws.push(Span::styled(
                        " ".repeat(total_width - used),
                        Style::default().bg(row_bg),
                    ));
                }
                lines.push(Line::from(ws));
                row_map.push(fi);
            }

            // Extra lines (continuation / stacktrace)
            let cont = Style::default().fg(lc).bg(row_bg);
            for extra in &entry.extra_lines {
                if lines.len() >= height {
                    break;
                }
                let extra_wrap_width = full_width.saturating_sub(header_width + 1);
                let extra_wrapped = wrap_text(extra, extra_wrap_width, MAX_WRAP_LINES);

                for extra_line in &extra_wrapped {
                    if lines.len() >= height {
                        break;
                    }
                    let mut cs = empty_prefix(is_selected, row_bg);
                    if !app.filter.search_query.is_empty() {
                        cs.extend(highlight_with_filter(extra_line, &app.filter, cont));
                    } else {
                        cs.extend(auto_highlight(extra_line, cont));
                    }
                    let used: usize = cs.iter().map(|s| s.content.width()).sum();
                    if used < total_width {
                        cs.push(Span::styled(
                            " ".repeat(total_width - used),
                            Style::default().bg(row_bg),
                        ));
                    }
                    lines.push(Line::from(cs));
                    row_map.push(fi);
                }
            }

            // Stack trace preview (error + collapsed stacktrace)
            if entry.error.is_some() || entry.stacktrace.is_some() {
                let (preview, remaining) = entry.stack_preview_lines(MAX_STACK_PREVIEW_LINES);
                let err_style = Style::default()
                    .fg(RED)
                    .bg(row_bg)
                    .add_modifier(Modifier::DIM);
                let frame_style = Style::default().fg(OVERLAY0).bg(row_bg);

                for (pi, pline) in preview.iter().enumerate() {
                    if lines.len() >= height {
                        break;
                    }
                    let mut ps = empty_prefix(is_selected, row_bg);
                    let style = if pi == 0 && entry.error.is_some() {
                        err_style // First line is the error summary → RED dimmed
                    } else {
                        frame_style // Stack frames → OVERLAY0
                    };
                    ps.push(Span::styled(safe_pad(pline, wrap_width), style));
                    let used: usize = ps.iter().map(|s| s.content.width()).sum();
                    if used < total_width {
                        ps.push(Span::styled(
                            " ".repeat(total_width - used),
                            Style::default().bg(row_bg),
                        ));
                    }
                    lines.push(Line::from(ps));
                    row_map.push(fi);
                }

                if remaining > 0 && lines.len() < height {
                    let mut ts = empty_prefix(is_selected, row_bg);
                    ts.push(Span::styled(
                        format!("... {} more frames", remaining),
                        Style::default()
                            .fg(OVERLAY0)
                            .bg(row_bg)
                            .add_modifier(Modifier::ITALIC),
                    ));
                    let used: usize = ts.iter().map(|s| s.content.width()).sum();
                    if used < total_width {
                        ts.push(Span::styled(
                            " ".repeat(total_width - used),
                            Style::default().bg(row_bg),
                        ));
                    }
                    lines.push(Line::from(ts));
                    row_map.push(fi);
                }
            }

            // Row separator: removed underline (too noisy), relying on
            // level-based background colors for visual grouping instead.

            if lines.len() >= height {
                break;
            }
        }
    }

    // ════════════════════════════════════════════════════════════════
    //  PHASE 3: Write back layout info for event handlers & status bar
    // ════════════════════════════════════════════════════════════════

    let last_rendered_fi = row_map.last().copied().unwrap_or(0);
    app.layout.rendered_to_end = last_rendered_fi + 1 >= filtered_count;

    let mut unique_entries = 0usize;
    let mut prev_fi: Option<usize> = None;
    for &fi in &row_map {
        if prev_fi != Some(fi) {
            unique_entries += 1;
            prev_fi = Some(fi);
        }
    }
    app.layout.visible_entry_count = unique_entries;
    app.layout.row_to_filtered_idx = row_map;

    // Detect if move_down scrolled to the very bottom → re-enable auto_scroll
    if !app.auto_scroll && app.layout.rendered_to_end {
        app.auto_scroll = true;
        app.new_logs_since_pause = 0;
    }

    f.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::NONE))
            .style(Style::default().bg(BASE)),
        area,
    );

    // Scrollbar
    if filtered_count > unique_entries {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_symbol("┃")
            .track_symbol(Some(" "))
            .thumb_style(Style::default().fg(BLUE))
            .track_style(Style::default().fg(SURFACE0).bg(BASE))
            .begin_symbol(None)
            .end_symbol(None);
        let max_offset = filtered_count.saturating_sub(unique_entries);
        let mut state = ScrollbarState::new(max_offset).position(start.min(max_offset));
        f.render_stateful_widget(scrollbar, area, &mut state);
    }
}

/// Calculate how many terminal rows a single entry occupies.
/// Must match the rendering logic exactly.
pub(super) fn entry_row_count_from_store(
    store: &crate::domain::LogStore,
    store_idx: usize,
    full_width: usize,
) -> usize {
    let entry = match store.get(store_idx) {
        Some(e) => e,
        None => return 1,
    };
    entry_row_count(entry, full_width)
}
