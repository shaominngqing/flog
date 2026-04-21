//! Logs view — main log list with toolbar and status bar.

pub mod detail;
pub mod highlight;
pub mod jump;
pub mod stats;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, AppMode};
use crate::domain::LogLevel;
use highlight::auto_highlight;

// Import shared palette from parent
use super::{
    safe_pad, wrap_text, BASE, BLUE, GREEN, LAVENDER, MANTLE, MAUVE, OVERLAY0, PEACH, PINK, RED,
    SAPPHIRE, SUBTEXT0, SURFACE0, SURFACE1, TEXT, YELLOW,
};

// Logs-specific colors
const ERROR_ROW_BG: Color = Color::Rgb(50, 30, 35); // subtle dark red
const WARNING_ROW_BG: Color = Color::Rgb(50, 45, 30); // subtle dark yellow

const TAG_COLORS: [Color; 5] = [BLUE, GREEN, PEACH, MAUVE, SAPPHIRE];

fn tag_color(tag: &str) -> Color {
    let hash: usize = tag.bytes().fold(0usize, |acc, b| {
        acc.wrapping_mul(31).wrapping_add(b as usize)
    });
    TAG_COLORS[hash % TAG_COLORS.len()]
}

const TAG_WIDTH: usize = 14;
const TIME_WIDTH: usize = 12;
const LEVEL_WIDTH: usize = 9; // " VERBOSE " is the longest
/// Max visual lines per log entry (header line + wrapped continuation lines).
const MAX_WRAP_LINES: usize = 3;
/// Max collapsed stack trace preview lines shown in the log list.
const MAX_STACK_PREVIEW_LINES: usize = 5;

fn level_color(level: LogLevel) -> Color {
    match level {
        LogLevel::Verbose => OVERLAY0,
        LogLevel::Debug => SUBTEXT0,
        LogLevel::Info => BLUE,
        LogLevel::Warning => YELLOW,
        LogLevel::Error => RED,
        LogLevel::System => OVERLAY0,
    }
}

fn message_color(level: LogLevel) -> Color {
    match level {
        LogLevel::Error => RED,
        LogLevel::Warning => YELLOW,
        LogLevel::Info => TEXT,
        LogLevel::Debug => SUBTEXT0,
        LogLevel::Verbose => OVERLAY0,
        LogLevel::System => OVERLAY0,
    }
}

/// Returns (fg, bg, bold) for a level pill badge.
fn level_badge(level: LogLevel) -> (Color, Color, bool) {
    match level {
        LogLevel::Verbose => (OVERLAY0, SURFACE0, false),
        LogLevel::Debug => (SUBTEXT0, SURFACE0, false),
        LogLevel::Info => (MANTLE, BLUE, true),
        LogLevel::Warning => (MANTLE, YELLOW, true),
        LogLevel::Error => (MANTLE, RED, true),
        LogLevel::System => (OVERLAY0, SURFACE0, false),
    }
}

/// Render level as a styled pill with fixed LEVEL_WIDTH-char width.
fn level_pill(level: LogLevel, _row_bg: Color) -> Span<'static> {
    let (fg, bg, bold) = level_badge(level);
    let label = level.as_str();
    // Center the label within LEVEL_WIDTH
    let total_pad = LEVEL_WIDTH.saturating_sub(label.len());
    let left_pad = total_pad / 2;
    let right_pad = total_pad - left_pad;
    let text = format!("{}{}{}", " ".repeat(left_pad), label, " ".repeat(right_pad));
    let mut style = Style::default().fg(fg).bg(bg);
    if bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    Span::styled(text, style)
}

// ── Search sparkline ──

fn search_sparkline(matches: &[usize], total: usize, width: usize) -> String {
    if total == 0 || width == 0 || matches.is_empty() {
        return String::new();
    }
    let bars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let bs = (total as f64 / width as f64).ceil().max(1.0) as usize;
    let mut buckets = vec![0u32; width];
    for &m in matches {
        buckets[(m / bs).min(width - 1)] += 1;
    }
    let mx = *buckets.iter().max().unwrap_or(&1).max(&1);
    buckets
        .iter()
        .map(|&c| {
            if c == 0 {
                ' '
            } else {
                bars[((c as f64 / mx as f64) * 7.0) as usize]
            }
        })
        .collect()
}

fn repeat_bar(count: usize, max_w: usize) -> String {
    let len = (count.min(50) * max_w) / 50;
    format!("x{} {}", count, "█".repeat(len.min(max_w)))
}

fn tag_pill_spans(filter: &crate::domain::FilterState) -> Vec<Span<'static>> {
    let colors = [BLUE, GREEN, PEACH, MAUVE, SAPPHIRE];
    let mut spans = Vec::new();
    for (ci, tag) in filter.tag_include.iter().enumerate() {
        spans.push(Span::styled(
            format!(" +{} ", tag),
            Style::default().fg(MANTLE).bg(colors[ci % colors.len()]),
        ));
        spans.push(Span::styled(" ", Style::default().bg(MANTLE)));
    }
    for tag in &filter.tag_exclude {
        spans.push(Span::styled(
            format!(" -{} ", tag),
            Style::default().fg(TEXT).bg(RED),
        ));
        spans.push(Span::styled(" ", Style::default().bg(MANTLE)));
    }
    spans
}

// ══════════════════════════════════════
//  Main Logs View Draw
// ══════════════════════════════════════

pub fn draw_logs(f: &mut Frame, app: &mut App, area: Rect) {
    // Layout: separator | toolbar | main | status
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // separator rule
            Constraint::Length(1), // toolbar
            Constraint::Min(3),    // main area (list + optional detail)
            Constraint::Length(1), // status bar
        ])
        .split(area);

    app.layout.toolbar_y = rows[1].y;
    app.layout.bottom_y = rows[3].y;

    draw_separator_rule(f, rows[0]);
    draw_toolbar(f, app, rows[1]);

    if app.show_detail_panel {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(100 - app.detail_panel_pct),
                Constraint::Percentage(app.detail_panel_pct),
            ])
            .split(rows[2]);

        app.layout.list_y = cols[0].y;
        app.layout.list_height = cols[0].height;

        draw_log_list(f, app, cols[0]);
        detail::draw_side_panel(f, app, cols[1]);
    } else {
        app.layout.list_y = rows[2].y;
        app.layout.list_height = rows[2].height;

        draw_log_list(f, app, rows[2]);
    }

    draw_jump_to_bottom(f, app, rows[2]);

    draw_status_bar(f, app, rows[3]);
}

// ══════════════════════════════════════
//  Toolbar
// ══════════════════════════════════════

fn draw_toolbar(f: &mut Frame, app: &mut App, area: Rect) {
    let mut spans: Vec<Span> = Vec::new();
    let mut x: u16 = 0;
    let bg = MANTLE;
    let search_active = app.mode == AppMode::Search;
    let filter_active = app.mode == AppMode::TagFilter;

    // Leading space
    spans.push(Span::styled(" ", Style::default().bg(bg)));
    x += 1;

    // Search
    let si = if search_active {
        Style::default().fg(MANTLE).bg(YELLOW)
    } else {
        Style::default().fg(OVERLAY0).bg(bg)
    };
    spans.push(Span::styled("/", si));
    x += 1;
    let sw: usize = 20;
    let st = if search_active {
        format!("{}_", app.search.input)
    } else if app.filter.search_query.is_empty() {
        "search...".into()
    } else {
        app.filter.search_query.clone()
    };
    let ss = if search_active {
        Style::default().fg(TEXT).bg(SURFACE0)
    } else if !app.filter.search_query.is_empty() {
        Style::default().fg(YELLOW).bg(bg)
    } else {
        Style::default().fg(OVERLAY0).bg(bg)
    };
    app.layout.search_x = (1, 1 + 1 + sw as u16);
    spans.push(Span::styled(safe_pad(&st, sw), ss));
    x += sw as u16;

    // Sparkline
    if !app.search.matches.is_empty() {
        let fc = app.filtered_count();
        let spark = search_sparkline(&app.search.matches, fc, 12);
        spans.push(Span::styled(
            format!(" {}", spark),
            Style::default().fg(LAVENDER).bg(bg),
        ));
        x += 1 + spark.width() as u16;
        let info = format!("{}/{}", app.search.match_idx + 1, app.search.matches.len());
        spans.push(Span::styled(
            format!(" {} ", info),
            Style::default().fg(YELLOW).bg(bg),
        ));
        x += 2 + info.width() as u16;
        spans.push(Span::styled("<", Style::default().fg(BLUE).bg(bg)));
        spans.push(Span::styled("> ", Style::default().fg(BLUE).bg(bg)));
        x += 3;
    }

    spans.push(Span::styled("   ", Style::default().bg(bg)));
    x += 3;

    // Tag filter
    let filter_start_x = x;
    if filter_active {
        spans.push(Span::styled("T", Style::default().fg(MANTLE).bg(GREEN)));
        x += 1;
        let fw: usize = 14;
        spans.push(Span::styled(
            safe_pad(&format!("{}_", app.tag_filter.input), fw),
            Style::default().fg(TEXT).bg(SURFACE0),
        ));
        x += fw as u16;
    } else if !app.filter.tag_include.is_empty() || !app.filter.tag_exclude.is_empty() {
        let pills = tag_pill_spans(&app.filter);
        for p in &pills {
            x += p.content.width() as u16;
        }
        spans.extend(pills);
    } else {
        spans.push(Span::styled("T", Style::default().fg(OVERLAY0).bg(bg)));
        x += 1;
        spans.push(Span::styled(
            safe_pad(" tag...", 7),
            Style::default().fg(OVERLAY0).bg(bg),
        ));
        x += 7;
    }
    app.layout.filter_x = (filter_start_x, x);

    // Separator
    spans.push(Span::styled("  │  ", Style::default().fg(SURFACE1).bg(bg)));
    x += 5;

    // Level buttons — pill style
    app.layout.levels_x = x;
    for (label, level) in &[
        ("S", LogLevel::System),
        ("V", LogLevel::Verbose),
        ("D", LogLevel::Debug),
        ("I", LogLevel::Info),
        ("W", LogLevel::Warning),
        ("E", LogLevel::Error),
    ] {
        let (fg, bg_c, bold) = level_badge(*level);
        let style = if app.filter.min_level == *level {
            let mut s = Style::default()
                .fg(fg)
                .bg(if bg_c == Color::Reset { SURFACE1 } else { bg_c });
            if bold {
                s = s.add_modifier(Modifier::BOLD);
            }
            s
        } else if app.filter.min_level > *level {
            Style::default()
                .fg(SURFACE0)
                .bg(bg)
                .add_modifier(Modifier::DIM)
        } else {
            Style::default().fg(level_color(*level)).bg(bg)
        };
        spans.push(Span::styled(format!(" {} ", label), style));
        x += 3;
    }

    // Separator + counts
    spans.push(Span::styled("  │  ", Style::default().fg(SURFACE1).bg(bg)));
    x += 5;

    let count_text = format!("{}/{}", app.filtered_count(), app.store.len());
    let count_w = count_text.width() as u16;
    spans.push(Span::styled(
        count_text,
        Style::default().fg(SUBTEXT0).bg(bg),
    ));
    x += count_w;

    if !app.bookmarks.is_empty() {
        let bm = format!("  ●{}", app.bookmarks.len());
        x += bm.width() as u16;
        spans.push(Span::styled(bm, Style::default().fg(YELLOW).bg(bg)));
    }

    let rem = area.width.saturating_sub(x);
    if rem > 0 {
        spans.push(Span::styled(
            " ".repeat(rem as usize),
            Style::default().bg(bg),
        ));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

// ══════════════════════════════════════
//  Status Bar
// ══════════════════════════════════════

fn draw_status_bar(f: &mut Frame, app: &mut App, area: Rect) {
    let bg = MANTLE;

    // Left group: toast OR (LIVE pill + counts + app/device/port context)
    let (left_spans, left_width, source_x) =
        if let Some(msg) = app.active_status().map(|s| s.to_string()) {
            let ok_text = " OK ";
            let msg_text = format!(" {} ", msg);
            let w = ok_text.width() + msg_text.width();
            (
                vec![
                    Span::styled(
                        ok_text,
                        Style::default().fg(MANTLE).bg(GREEN).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(msg_text, Style::default().fg(TEXT).bg(bg)),
                ],
                w as u16,
                (0u16, 0u16),
            )
        } else {
            let (live_text, live_style) = if app.auto_scroll {
                let dot = match (app.tick / 8) % 4 {
                    0 => "●",
                    1 => "◉",
                    2 => "●",
                    _ => "○",
                };
                (
                    format!(" {} LIVE ", dot),
                    Style::default().fg(MANTLE).bg(GREEN).add_modifier(Modifier::BOLD),
                )
            } else if app.new_logs_since_pause > 0 {
                (
                    format!(" {} new ", app.new_logs_since_pause),
                    Style::default().fg(MANTLE).bg(YELLOW).add_modifier(Modifier::BOLD),
                )
            } else {
                let total = app.filtered_count();
                let vis = app.layout.visible_entry_count.max(1);
                let max_off = total.saturating_sub(vis);
                let pct = if max_off > 0 {
                    ((app.scroll_offset.min(max_off)) * 100) / max_off
                } else {
                    100
                };
                (
                    format!(" {}% ", pct.min(100)),
                    Style::default().fg(TEXT).bg(SURFACE0),
                )
            };

            let total = app.store.len();
            let filtered = app.filtered_count();
            let counts = format!("  {}/{}  ", filtered, total);

            let ctx = app
                .active_app_id
                .as_ref()
                .and_then(|id| app.connected_apps.iter().find(|a| &a.id == id))
                .map(|ca| {
                    let v = if ca.app_version.is_empty() {
                        String::new()
                    } else {
                        format!(" v{}", ca.app_version)
                    };
                    let dev = if ca.device_name.is_empty() {
                        ca.device_id.clone()
                    } else {
                        ca.device_name.clone()
                    };
                    format!("{}{} · {} · :{}", ca.app_name, v, dev, ca.port)
                })
                .unwrap_or_default();

            let lw = live_text.width() as u16;
            let cw = counts.width() as u16;
            let ctxw = ctx.width() as u16;
            let sx = (lw + cw, lw + cw + ctxw);
            let w = lw + cw + ctxw;
            (
                vec![
                    Span::styled(live_text, live_style),
                    Span::styled(counts, Style::default().fg(SUBTEXT0).bg(bg)),
                    Span::styled(
                        ctx,
                        Style::default()
                            .fg(SUBTEXT0)
                            .bg(bg)
                            .add_modifier(Modifier::UNDERLINED),
                    ),
                ],
                w,
                sx,
            )
        };

    app.layout.source_info_x = source_x;

    // Right group: unified SURFACE0 buttons with SUBTEXT0 label; Quit in RED
    let button_style = Style::default().fg(SUBTEXT0).bg(SURFACE0);
    let quit_style = Style::default().fg(RED).bg(SURFACE0);
    let buttons: Vec<(&str, &str, Style)> = vec![
        ("clear", "  Clear  ", button_style),
        ("export", "  Export  ", button_style),
        ("stats", "  Stats  ", button_style),
        ("help", "  Help  ", button_style),
        ("quit", "  Quit  ", quit_style),
    ];

    let bw: u16 = buttons.iter().map(|(_, l, _)| l.width() as u16 + 1).sum();
    let spacer = area.width.saturating_sub(left_width + bw).max(1);

    let mut spans = left_spans;
    spans.push(Span::styled(
        " ".repeat(spacer as usize),
        Style::default().bg(bg),
    ));

    let mut xc = left_width + spacer;
    app.layout.bottom_buttons.clear();
    for (i, (name, label, style)) in buttons.iter().enumerate() {
        let start = xc;
        spans.push(Span::styled(*label, *style));
        xc += label.width() as u16;
        app.layout.bottom_buttons.push((name, start, xc));
        if i < buttons.len() - 1 {
            spans.push(Span::styled(" ", Style::default().bg(bg)));
            xc += 1;
        }
    }

    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)),
        area,
    );
}

// ══════════════════════════════════════
//  Log List
// ══════════════════════════════════════

fn draw_log_list(f: &mut Frame, app: &mut App, area: Rect) {
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

    let start = app.scroll_offset;
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

            let cursor_color = if entry.level == LogLevel::Error { RED } else { BLUE };
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
                let bar_part: String = first_text
                    .chars()
                    .take(split_at)
                    .collect();
                let msg_part: String = first_text
                    .chars()
                    .skip(split_at)
                    .collect();
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
                let err_style = Style::default().fg(RED).bg(row_bg).add_modifier(Modifier::DIM);
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
                    ps.push(Span::styled(
                        safe_pad(pline, wrap_width),
                        style,
                    ));
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
                        Style::default().fg(OVERLAY0).bg(row_bg).add_modifier(Modifier::ITALIC),
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
fn entry_row_count_from_store(
    store: &crate::domain::LogStore,
    store_idx: usize,
    full_width: usize,
) -> usize {
    let entry = match store.get(store_idx) {
        Some(e) => e,
        None => return 1,
    };

    if entry.tag == "────" {
        return 3;
    }

    // Header prefix width (must match render layout)
    // cursor(1) + bookmark(2) + time(TIME_WIDTH) + sep(1) + level(LEVEL_WIDTH) + sep(1) + tag(TAG_WIDTH) + sep(1)
    let header_width = 1 + 2 + LEVEL_WIDTH + 1 + TIME_WIDTH + 1 + TAG_WIDTH + 1;

    let full_msg = if entry.repeat_count > 1 {
        format!("{} {}", repeat_bar(entry.repeat_count, 8), entry.message)
    } else {
        entry.message.clone()
    };

    let wrap_width = full_width.saturating_sub(header_width + 1);
    let wrapped = wrap_text(&full_msg, wrap_width, MAX_WRAP_LINES);

    let mut extra_rows = 0;
    for extra in &entry.extra_lines {
        extra_rows += wrap_text(extra, wrap_width, MAX_WRAP_LINES).len();
    }
    let mut stack_rows = 0;
    if entry.error.is_some() || entry.stacktrace.is_some() {
        let (preview, remaining) = entry.stack_preview_lines(MAX_STACK_PREVIEW_LINES);
        stack_rows = preview.len();
        if remaining > 0 {
            stack_rows += 1; // "... N more frames" line
        }
    }
    wrapped.len() + extra_rows + stack_rows
}

fn draw_separator_rule(f: &mut Frame, area: Rect) {
    let rule: String = "─".repeat(area.width as usize);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            rule,
            Style::default().fg(SURFACE0).bg(MANTLE),
        )))
        .style(Style::default().bg(MANTLE)),
        area,
    );
}

fn draw_jump_to_bottom(f: &mut Frame, app: &mut App, area: Rect) {
    if !jump::should_show(app.auto_scroll) {
        app.layout.jump_to_bottom_rect = None;
        return;
    }
    if area.height < 4 || area.width < 24 {
        app.layout.jump_to_bottom_rect = None;
        return;
    }

    let label_text = jump::label(app.new_logs_since_pause);
    let pill_w = (label_text.width() as u16 + 2).min(area.width.saturating_sub(4));
    let pill_h: u16 = 3;
    let pill_x = area.x + (area.width.saturating_sub(pill_w)) / 2;
    let pill_y_rel = (area.height as f32 * 0.70) as u16;
    let pill_y = area.y + pill_y_rel.min(area.height.saturating_sub(pill_h));

    let border_style = Style::default().fg(SAPPHIRE).bg(SURFACE0);
    let top = format!("╭{}╮", "─".repeat((pill_w - 2) as usize));
    let bot = format!("╰{}╯", "─".repeat((pill_w - 2) as usize));

    let mid = if app.new_logs_since_pause > 0 {
        let total_inner = (pill_w - 2) as usize;
        let base = "  ↓ Jump to bottom  ";
        let new_text = format!("{} new  ", app.new_logs_since_pause);
        let used = base.width() + new_text.width();
        let pad = total_inner.saturating_sub(used);
        vec![
            Span::styled("│", border_style),
            Span::styled(base.to_string(), Style::default().fg(TEXT).bg(SURFACE0)),
            Span::styled(new_text, Style::default().fg(YELLOW).bg(SURFACE0)),
            Span::styled(" ".repeat(pad), Style::default().bg(SURFACE0)),
            Span::styled("│", border_style),
        ]
    } else {
        let total_inner = (pill_w - 2) as usize;
        let base = "  ↓ Jump to bottom  ";
        let pad = total_inner.saturating_sub(base.width());
        vec![
            Span::styled("│", border_style),
            Span::styled(base.to_string(), Style::default().fg(TEXT).bg(SURFACE0)),
            Span::styled(" ".repeat(pad), Style::default().bg(SURFACE0)),
            Span::styled("│", border_style),
        ]
    };

    let pill_area = Rect::new(pill_x, pill_y, pill_w, pill_h);
    let lines = vec![
        Line::from(Span::styled(top, border_style)),
        Line::from(mid),
        Line::from(Span::styled(bot, border_style)),
    ];
    f.render_widget(
        Paragraph::new(lines).style(Style::default().bg(SURFACE0)),
        pill_area,
    );

    app.layout.jump_to_bottom_rect = Some((pill_x, pill_y, pill_w, pill_h));
}

// ══════════════════════════════════════
//  Not Connected (empty state)
// ══════════════════════════════════════

// ── ASCII banner ──

const LOGO: [&str; 6] = [
    r"███████╗██╗      ██████╗  ██████╗ ",
    r"██╔════╝██║     ██╔═══██╗██╔════╝ ",
    r"█████╗  ██║     ██║   ██║██║  ███╗",
    r"██╔══╝  ██║     ██║   ██║██║   ██║",
    r"██║     ███████╗╚██████╔╝╚██████╔╝",
    r"╚═╝     ╚══════╝ ╚═════╝  ╚═════╝ ",
];

/// Gradient colors: Catppuccin Macchiato blue → teal → green
const GRAD: [Color; 6] = [
    Color::Rgb(138, 173, 244), // blue
    Color::Rgb(125, 196, 228), // sapphire
    Color::Rgb(139, 213, 202), // teal
    Color::Rgb(166, 218, 149), // green
    Color::Rgb(139, 213, 202), // teal
    Color::Rgb(125, 196, 228), // sapphire
];

fn gradient_line(text: &str) -> Line<'static> {
    let spans: Vec<Span<'static>> = text
        .chars()
        .enumerate()
        .map(|(i, ch)| {
            let color = GRAD[i % GRAD.len()];
            Span::styled(
                ch.to_string(),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            )
        })
        .collect();
    Line::from(spans)
}

fn logo_lines() -> Vec<Line<'static>> {
    LOGO.iter().map(|l| gradient_line(l)).collect()
}

fn draw_not_connected(f: &mut Frame, _app: &mut App, area: Rect) {
    let logo_h = LOGO.len() as u16 + 13; // logo + spacing + subtitle + spacer + Quick Start card (7)
    let start_y = area.height.saturating_sub(logo_h) / 2;

    let mut lines: Vec<Line> = Vec::new();
    for _ in 0..start_y {
        lines.push(Line::raw(""));
    }

    for logo_line in logo_lines() {
        lines.push(logo_line);
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "   Flutter Log Viewer · Network Inspector",
        Style::default().fg(OVERLAY0),
    )));
    lines.push(Line::raw(""));

    // Quick Start bordered card
    let indent = "    ";
    let card_w = 46usize;
    let top = format!("{}┌{}┐", indent, "─".repeat(card_w - 2));
    let bot = format!("{}└{}┘", indent, "─".repeat(card_w - 2));
    let border_style = Style::default().fg(SURFACE0);
    let content_fg = Style::default().fg(SUBTEXT0);

    let card_row = |text: &str| -> Line<'static> {
        let inner_w = card_w - 2;
        let pad = inner_w.saturating_sub(text.width());
        Line::from(vec![
            Span::styled(indent.to_string(), Style::default()),
            Span::styled("│".to_string(), border_style),
            Span::styled(text.to_string(), content_fg),
            Span::styled(" ".repeat(pad), Style::default()),
            Span::styled("│".to_string(), border_style),
        ])
    };

    lines.push(Line::from(Span::styled(top, border_style)));
    lines.push(card_row("  Quick Start                               "));
    lines.push(card_row("   1. Add flog_dart to your Flutter app     "));
    lines.push(card_row("   2. Run your app in debug mode            "));
    lines.push(card_row("   3. flog will auto-connect                "));
    lines.push(card_row("                                            "));
    lines.push(Line::from(Span::styled(bot, border_style)));

    f.render_widget(Paragraph::new(lines).style(Style::default().bg(BASE)), area);
}

fn draw_waiting_for_logs(f: &mut Frame, app: &mut App, area: Rect) {
    let tick = app.tick;
    let logo_h = LOGO.len() as u16 + 5;
    let start_y = area.height.saturating_sub(logo_h) / 2;

    let spinner = match (tick / 5) % 8 {
        0 => "⣾",
        1 => "⣽",
        2 => "⣻",
        3 => "⢿",
        4 => "⡿",
        5 => "⣟",
        6 => "⣯",
        _ => "⣷",
    };

    let mut lines: Vec<Line> = Vec::new();
    for _ in 0..start_y {
        lines.push(Line::raw(""));
    }

    for logo_line in logo_lines() {
        lines.push(logo_line);
    }
    lines.push(Line::raw(""));

    let subtitle = app
        .active_app_id
        .as_ref()
        .and_then(|id| app.connected_apps.iter().find(|a| &a.id == id))
        .map(|ca| {
            let version = if ca.app_version.is_empty() {
                String::new()
            } else {
                format!(" v{}", ca.app_version)
            };
            format!("   Connected · {}{} ({})", ca.app_name, version, ca.os)
        })
        .unwrap_or_else(|| "   Flutter Log Viewer".to_string());

    lines.push(Line::from(Span::styled(
        subtitle,
        Style::default().fg(OVERLAY0),
    )));
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(format!("   {}  ", spinner), Style::default().fg(BLUE)),
        Span::styled("Waiting for logs...", Style::default().fg(SUBTEXT0)),
    ]));

    f.render_widget(Paragraph::new(lines).style(Style::default().bg(BASE)), area);
}

fn draw_no_matching_logs(f: &mut Frame, app: &App, area: Rect) {
    let mid = area.height / 2;
    let mut lines: Vec<Line> = Vec::new();
    for _ in 0..mid.saturating_sub(4) {
        lines.push(Line::raw(""));
    }

    lines.push(Line::from(Span::styled(
        "          \u{2205}",
        Style::default().fg(SURFACE1),
    )));
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "    No matching logs",
        Style::default().fg(OVERLAY0),
    )));
    lines.push(Line::from(Span::styled(
        "    Try adjusting filters or level",
        Style::default().fg(SURFACE1),
    )));
    lines.push(Line::raw(""));

    let mut filter_rows: Vec<String> = Vec::new();
    if !app.filter.search_query.is_empty() {
        filter_rows.push(format!("    search: \"{}\"", app.filter.search_query));
    }
    if app.filter.min_level != LogLevel::System {
        filter_rows.push(format!("    level:  {}+", app.filter.min_level.as_str()));
    }
    let tag_includes: Vec<String> = app
        .filter
        .tag_include
        .iter()
        .map(|t| format!("+{}", t))
        .collect();
    let tag_excludes: Vec<String> = app
        .filter
        .tag_exclude
        .iter()
        .map(|t| format!("-{}", t))
        .collect();
    if !tag_includes.is_empty() || !tag_excludes.is_empty() {
        let combined: Vec<String> = tag_includes.into_iter().chain(tag_excludes).collect();
        filter_rows.push(format!("    tags:   {}", combined.join(" ")));
    }

    if !filter_rows.is_empty() {
        lines.push(Line::from(Span::styled(
            "    ┌─ Active filters ─────────────────┐",
            Style::default().fg(SURFACE0),
        )));
        for r in &filter_rows {
            lines.push(Line::from(vec![
                Span::styled("    │", Style::default().fg(SURFACE0)),
                Span::styled(
                    safe_pad(r, 34),
                    Style::default().fg(SUBTEXT0),
                ),
                Span::styled("│", Style::default().fg(SURFACE0)),
            ]));
        }
        lines.push(Line::from(Span::styled(
            "    └──────────────────────────────────┘",
            Style::default().fg(SURFACE0),
        )));
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "    press esc to clear all",
            Style::default().fg(OVERLAY0),
        )));
    }

    f.render_widget(Paragraph::new(lines).style(Style::default().bg(BASE)), area);
}

// ══════════════════════════════════════
//  Search Highlight
// ══════════════════════════════════════

fn highlight_with_filter(
    text: &str,
    filter: &crate::domain::FilterState,
    base: Style,
) -> Vec<Span<'static>> {
    let positions = filter.search_positions(text);
    if positions.is_empty() {
        return vec![Span::styled(text.to_string(), base)];
    }

    let hl = Style::default()
        .fg(MANTLE)
        .bg(YELLOW)
        .add_modifier(Modifier::BOLD);
    let mut spans = Vec::new();
    let mut last = 0;
    for r in &positions {
        let s = r.start.min(text.len());
        let e = r.end.min(text.len());
        if s > last {
            spans.push(Span::styled(text[last..s].to_string(), base));
        }
        if s < e {
            spans.push(Span::styled(text[s..e].to_string(), hl));
        }
        last = e;
    }
    if last < text.len() {
        spans.push(Span::styled(text[last..].to_string(), base));
    }
    if spans.is_empty() {
        spans.push(Span::styled(text.to_string(), base));
    }
    spans
}
