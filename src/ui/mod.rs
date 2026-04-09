//! TUI rendering — Catppuccin Macchiato theme.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Paragraph,
        Scrollbar, ScrollbarOrientation, ScrollbarState,
    },
};
use unicode_width::UnicodeWidthStr;

pub mod detail;
pub mod help;
pub mod highlight;
pub mod source_select;
pub mod stats;
pub mod timeline;

use crate::app::{App, AppMode};
use crate::domain::LogLevel;
use highlight::auto_highlight;

// ══════════════════════════════════════
//  Catppuccin Macchiato Palette
// ══════════════════════════════════════

const BASE: Color      = Color::Rgb(36, 39, 58);    // #24273a — main bg
const MANTLE: Color    = Color::Rgb(30, 32, 48);    // #1e2030 — panels/alt bg
const SURFACE0: Color  = Color::Rgb(54, 58, 79);    // #363a4f — subtle borders
const SURFACE1: Color  = Color::Rgb(73, 77, 100);   // #494d64 — active borders
const OVERLAY0: Color  = Color::Rgb(110, 115, 141);  // #6e738d — muted text
const TEXT: Color       = Color::Rgb(202, 211, 245);  // #cad3f5 — main text
const SUBTEXT0: Color  = Color::Rgb(165, 173, 206);  // #a5adce — secondary text

const BLUE: Color      = Color::Rgb(138, 173, 244);  // #8aadf4 — accent
const SAPPHIRE: Color  = Color::Rgb(125, 196, 228);  // #7dc4e4 — links
const TEAL: Color      = Color::Rgb(139, 213, 202);  // #8bd5ca — values
const GREEN: Color     = Color::Rgb(166, 218, 149);  // #a6da95 — success
const YELLOW: Color    = Color::Rgb(238, 212, 159);  // #eed49f — warning
const PEACH: Color     = Color::Rgb(245, 169, 127);  // #f5a97f — info emphasis
const RED: Color       = Color::Rgb(237, 135, 150);  // #ed8796 — error
const MAUVE: Color     = Color::Rgb(198, 160, 246);  // #c6a0f6 — key/label
const PINK: Color      = Color::Rgb(245, 189, 230);  // #f5bde6 — special
const LAVENDER: Color  = Color::Rgb(183, 189, 248);  // #b7bdf8 — subtle hl

const TAG_WIDTH: usize = 14;
const TIME_WIDTH: usize = 12;
const LEVEL_WIDTH: usize = 9; // " VERBOSE " is the longest
/// Max visual lines per log entry (header line + wrapped continuation lines).
const MAX_WRAP_LINES: usize = 3;

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
    if bold { style = style.add_modifier(Modifier::BOLD); }
    Span::styled(text, style)
}

// ── Search sparkline ──

fn search_sparkline(matches: &[usize], total: usize, width: usize) -> String {
    if total == 0 || width == 0 || matches.is_empty() { return String::new(); }
    let bars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let bs = (total as f64 / width as f64).ceil().max(1.0) as usize;
    let mut buckets = vec![0u32; width];
    for &m in matches { buckets[(m / bs).min(width - 1)] += 1; }
    let mx = *buckets.iter().max().unwrap_or(&1).max(&1);
    buckets.iter().map(|&c| if c == 0 { ' ' } else { bars[((c as f64 / mx as f64) * 7.0) as usize] }).collect()
}

fn repeat_bar(count: usize, max_w: usize) -> String {
    let len = (count.min(50) * max_w) / 50;
    format!("x{} {}", count, "█".repeat(len.min(max_w)))
}

fn tag_pill_spans(filter: &crate::domain::FilterState) -> Vec<Span<'static>> {
    let colors = [BLUE, GREEN, PEACH, MAUVE, SAPPHIRE];
    let mut spans = Vec::new();
    let mut ci = 0;
    for tag in &filter.tag_include {
        spans.push(Span::styled(format!(" +{} ", tag), Style::default().fg(MANTLE).bg(colors[ci % colors.len()])));
        spans.push(Span::styled(" ", Style::default().bg(MANTLE)));
        ci += 1;
    }
    for tag in &filter.tag_exclude {
        spans.push(Span::styled(format!(" -{} ", tag), Style::default().fg(TEXT).bg(RED)));
        spans.push(Span::styled(" ", Style::default().bg(MANTLE)));
    }
    spans
}

// ══════════════════════════════════════
//  Main Draw
// ══════════════════════════════════════

pub fn draw(f: &mut Frame, app: &mut App) {
    app.tick += 1;

    let full = f.area();
    f.render_widget(Block::default().style(Style::default().bg(BASE)), full);

    // In SourceSelect mode, use full screen for the selection UI
    if app.mode == AppMode::SourceSelect {
        app.layout.list_y = full.y;
        app.layout.list_height = full.height;
        app.layout.bottom_y = full.y + full.height;
        app.layout.width = full.width;
        app.layout.bottom_buttons.clear();
        source_select::draw_source_select(f, app, full);
        return;
    }

    // Vertical: toolbar | main | timeline | status
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // toolbar
            Constraint::Min(3),    // main area (list + optional detail)
            Constraint::Length(3), // timeline
            Constraint::Length(1), // status bar
        ])
        .split(full);

    app.layout.toolbar_y = rows[0].y;
    app.layout.timeline_y = rows[2].y;
    app.layout.bottom_y = rows[3].y;
    app.layout.width = full.width;

    draw_toolbar(f, app, rows[0]);

    // Main area: detail panel or log list
    if app.show_detail_panel {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(100 - app.detail_panel_pct),
                Constraint::Percentage(app.detail_panel_pct),
            ])
            .split(rows[1]);

        app.layout.list_y = cols[0].y;
        app.layout.list_height = cols[0].height;

        draw_log_list(f, app, cols[0]);
        detail::draw_side_panel(f, app, cols[1]);
    } else {
        app.layout.list_y = rows[1].y;
        app.layout.list_height = rows[1].height;

        draw_log_list(f, app, rows[1]);
    }

    timeline::draw_timeline(f, app, rows[2]);
    draw_status_bar(f, app, rows[3]);

    // Source dropdown overlay (rendered last, on top of everything)
    if app.show_source_dropdown {
        source_select::draw_source_dropdown(f, app, rows[3].y);
    } else {
        app.layout.dropdown_rect = None;
        app.layout.dropdown_items.clear();
        app.layout.dropdown_tab_row = None;
    }
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

    // Logo
    spans.push(Span::styled(" flog ", Style::default().fg(MANTLE).bg(BLUE).add_modifier(Modifier::BOLD)));
    x += 6;
    spans.push(Span::styled(" ", Style::default().bg(bg)));
    x += 1;

    // Search
    let si = if search_active { Style::default().fg(MANTLE).bg(YELLOW) } else { Style::default().fg(OVERLAY0).bg(bg) };
    spans.push(Span::styled("/", si));
    x += 1;
    let sw: usize = 20;
    let st = if search_active { format!("{}_", app.search.input) }
        else if app.filter.search_query.is_empty() { "search...".into() }
        else { app.filter.search_query.clone() };
    let ss = if search_active { Style::default().fg(TEXT).bg(SURFACE0) }
        else if !app.filter.search_query.is_empty() { Style::default().fg(YELLOW).bg(bg) }
        else { Style::default().fg(OVERLAY0).bg(bg) };
    app.layout.search_x = (7, 7 + 1 + sw as u16);
    spans.push(Span::styled(safe_pad(&st, sw), ss));
    x += sw as u16;

    // Sparkline
    if !app.search.matches.is_empty() {
        let fc = app.filtered_count();
        let spark = search_sparkline(&app.search.matches, fc, 12);
        spans.push(Span::styled(format!(" {}", spark), Style::default().fg(LAVENDER).bg(bg)));
        x += 1 + spark.width() as u16;
        let info = format!("{}/{}", app.search.match_idx + 1, app.search.matches.len());
        spans.push(Span::styled(format!(" {} ", info), Style::default().fg(YELLOW).bg(bg)));
        x += 2 + info.width() as u16;
        spans.push(Span::styled("<", Style::default().fg(BLUE).bg(bg)));
        spans.push(Span::styled("> ", Style::default().fg(BLUE).bg(bg)));
        x += 3;
    }

    spans.push(Span::styled("  ", Style::default().bg(bg)));
    x += 2;

    // Tag
    let filter_start_x = x;
    if filter_active {
        spans.push(Span::styled("T", Style::default().fg(MANTLE).bg(GREEN)));
        x += 1;
        let fw: usize = 14;
        spans.push(Span::styled(safe_pad(&format!("{}_", app.tag_filter.input), fw), Style::default().fg(TEXT).bg(SURFACE0)));
        x += fw as u16;
    } else if !app.filter.tag_include.is_empty() || !app.filter.tag_exclude.is_empty() {
        let pills = tag_pill_spans(&app.filter);
        for p in &pills { x += p.content.width() as u16; }
        spans.extend(pills);
    } else {
        spans.push(Span::styled("T", Style::default().fg(OVERLAY0).bg(bg)));
        x += 1;
        spans.push(Span::styled(safe_pad("tag...", 6), Style::default().fg(OVERLAY0).bg(bg)));
        x += 6;
    }
    app.layout.filter_x = (filter_start_x, x);

    spans.push(Span::styled("  ", Style::default().bg(bg)));
    x += 2;

    // Level buttons — pill style
    app.layout.levels_x = x;
    for (label, level) in &[("S", LogLevel::System), ("V", LogLevel::Verbose), ("D", LogLevel::Debug), ("I", LogLevel::Info), ("W", LogLevel::Warning), ("E", LogLevel::Error)] {
        let (fg, bg_c, bold) = level_badge(*level);
        let style = if app.filter.min_level == *level {
            // Active: full pill
            let mut s = Style::default().fg(fg).bg(if bg_c == Color::Reset { SURFACE1 } else { bg_c });
            if bold { s = s.add_modifier(Modifier::BOLD); }
            s
        } else if app.filter.min_level > *level {
            // Filtered out: very dim
            Style::default().fg(SURFACE0).bg(bg).add_modifier(Modifier::DIM)
        } else {
            // Available
            Style::default().fg(level_color(*level)).bg(bg)
        };
        spans.push(Span::styled(format!(" {} ", label), style));
        x += 3;
    }

    if !app.bookmarks.is_empty() {
        let bm = format!("  ●{}", app.bookmarks.len());
        x += bm.width() as u16;
        spans.push(Span::styled(bm, Style::default().fg(YELLOW).bg(bg)));
    }

    let rem = area.width.saturating_sub(x);
    if rem > 0 { spans.push(Span::styled(" ".repeat(rem as usize), Style::default().bg(bg))); }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

// ══════════════════════════════════════
//  Status Bar
// ══════════════════════════════════════

fn draw_status_bar(f: &mut Frame, app: &mut App, area: Rect) {
    let bg = MANTLE;

    if let Some(msg) = app.active_status().map(|s| s.to_string()) {
        let line = Line::from(vec![
            Span::styled(" OK ", Style::default().fg(MANTLE).bg(GREEN).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {} ", msg), Style::default().fg(TEXT).bg(bg)),
        ]);
        f.render_widget(Paragraph::new(line).style(Style::default().bg(bg)), area);
        app.layout.bottom_buttons.clear();
        return;
    }

    let (live_text, live_style) = if app.auto_scroll {
        let dot = match (app.tick / 8) % 4 { 0 => "●", 1 => "◉", 2 => "●", _ => "○" };
        (format!(" {} LIVE ", dot), Style::default().fg(MANTLE).bg(GREEN).add_modifier(Modifier::BOLD))
    } else if app.new_logs_since_pause > 0 {
        (format!(" {} new ", app.new_logs_since_pause), Style::default().fg(MANTLE).bg(YELLOW).add_modifier(Modifier::BOLD))
    } else {
        // Use visible_entry_count (set by renderer) for correct percentage
        let total = app.filtered_count();
        let vis = app.layout.visible_entry_count.max(1);
        let max_off = total.saturating_sub(vis);
        let pct = if max_off > 0 {
            ((app.scroll_offset.min(max_off)) * 100) / max_off
        } else {
            100
        };
        (format!(" {}% ", pct.min(100)), Style::default().fg(TEXT).bg(SURFACE0))
    };

    let total = app.store.len();
    let filtered = app.filtered_count();
    let device = if app.source_name.is_empty() { String::new() } else { format!(" {}", app.source_name) };
    let info = format!(" {}/{}{}", filtered, total, device);

    let buttons: Vec<(&str, &str, Style)> = vec![
        ("separator", " ── ", Style::default().fg(YELLOW).bg(bg)),
        ("clear", " Clear ", Style::default().fg(PEACH).bg(bg)),
        ("export", " Export ", Style::default().fg(SAPPHIRE).bg(bg)),
        ("stats", " Stats ", Style::default().fg(SAPPHIRE).bg(bg)),
        ("help", " ? ", Style::default().fg(SAPPHIRE).bg(bg)),
        ("quit", " x ", Style::default().fg(RED).bg(bg)),
    ];

    let lw = live_text.width() as u16;
    let iw = info.width() as u16;
    let bw: u16 = buttons.iter().map(|(_, l, _)| l.width() as u16 + 1).sum();
    let spacer = area.width.saturating_sub(lw + iw + bw).max(1);

    // Record source info x-range for click handling
    let source_info_start = lw;
    let source_info_end = lw + iw;
    app.layout.source_info_x = (source_info_start, source_info_end);

    let mut spans = vec![
        Span::styled(&live_text, live_style),
        Span::styled(&info, Style::default().fg(SUBTEXT0).bg(bg).add_modifier(Modifier::UNDERLINED)),
        Span::styled(" ".repeat(spacer as usize), Style::default().bg(bg)),
    ];

    let mut xc = lw + iw + spacer;
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

    f.render_widget(Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)), area);
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
        if app.store.is_empty() && !app.connected {
            draw_not_connected(f, app, area);
        } else if app.store.is_empty() && app.connected {
            draw_waiting_for_logs(f, app, area);
        } else {
            draw_no_matching_logs(f, area);
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
            if rows_used + rows > height && rows_used > 0 { break; }
            rows_used += rows;
            start_fi = idx;
            if rows_used >= height { break; }
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
            if rows_used + rows > height && rows_used > 0 { break; }
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
                if rows_back + rows > height && rows_back > 0 { break; }
                rows_back += rows;
                new_start = si;
                if si == 0 || rows_back >= height { break; }
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

            let row_bg = if is_selected { SURFACE0 } else if vi % 2 == 1 { MANTLE } else { BASE };

            let base = Style::default().fg(lc).bg(row_bg);
            let tag_s = Style::default().fg(TEAL).bg(row_bg);
            let dim_s = Style::default().fg(SURFACE1).bg(row_bg);

            let cursor = if is_selected {
                Span::styled("▎", Style::default().fg(BLUE).bg(row_bg))
            } else {
                Span::styled(" ", Style::default().bg(row_bg))
            };

            let bm = if app.is_bookmarked(store_idx) {
                Span::styled("● ", Style::default().fg(YELLOW).bg(row_bg))
            } else {
                Span::styled("  ", Style::default().bg(row_bg))
            };

            let time = safe_pad(if entry.timestamp.is_empty() { "" } else { &entry.timestamp }, TIME_WIDTH);
            let time_style = Style::default().fg(OVERLAY0).bg(row_bg).add_modifier(Modifier::DIM);

            let level_span = level_pill(entry.level, row_bg);
            let tag = safe_pad(&entry.tag, TAG_WIDTH);

            // Separator: 3 rows (blank + divider + blank)
            if entry.tag == "────" {
                if lines.len() < height {
                    lines.push(Line::from(Span::styled(" ".repeat(total_width), Style::default().bg(BASE))));
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
                    lines.push(Line::from(Span::styled(" ".repeat(total_width), Style::default().bg(BASE))));
                    row_map.push(fi);
                }
                if lines.len() >= height { break; }
                continue;
            }

            let header_spans: Vec<Span> = vec![
                cursor, bm,
                Span::styled(time, time_style),
                Span::styled(" ", dim_s),
                level_span,
                Span::styled(" ", dim_s),
                Span::styled(tag, tag_s),
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
                let bar_end = full_msg.find(&entry.message).unwrap_or(0);
                let bar_part: String = first_text.chars().take(bar_end.min(first_text.len())).collect();
                let msg_part: String = first_text.chars().skip(bar_end.min(first_text.len())).collect();
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
                spans.push(Span::styled(" ".repeat(total_width - used), Style::default().bg(row_bg)));
            }
            lines.push(Line::from(spans));
            row_map.push(fi);

            // Helper: build an empty header prefix aligned to columns
            // cursor(1) + bookmark(2) + time(TIME_WIDTH) + sep(1) + level(6) + sep(1) + tag(TAG_WIDTH) + sep(1)
            let empty_prefix = |sel: bool, bg: Color| -> Vec<Span<'static>> {
                let cursor_s = if sel {
                    Span::styled("▎", Style::default().fg(BLUE).bg(bg))
                } else {
                    Span::styled(" ", Style::default().bg(bg))
                };
                let blank = Style::default().bg(bg);
                vec![
                    cursor_s,
                    Span::styled("  ", blank),                          // bookmark
                    Span::styled(" ".repeat(TIME_WIDTH), blank),        // time
                    Span::styled(" ", blank),                           // sep
                    Span::styled(" ".repeat(LEVEL_WIDTH), blank),       // level
                    Span::styled(" ", blank),                           // sep
                    Span::styled(" ".repeat(TAG_WIDTH), blank),         // tag
                    Span::styled(" ", blank),                           // sep
                ]
            };

            // Wrapped continuation lines
            for wrap_line in wrapped.iter().skip(1) {
                if lines.len() >= height { break; }
                let mut ws = empty_prefix(is_selected, row_bg);
                if !app.filter.search_query.is_empty() {
                    ws.extend(highlight_with_filter(wrap_line, &app.filter, base));
                } else {
                    ws.extend(auto_highlight(wrap_line, base));
                }
                let used: usize = ws.iter().map(|s| s.content.width()).sum();
                if used < total_width {
                    ws.push(Span::styled(" ".repeat(total_width - used), Style::default().bg(row_bg)));
                }
                lines.push(Line::from(ws));
                row_map.push(fi);
            }

            // Extra lines (continuation / stacktrace)
            let cont = Style::default().fg(lc).bg(row_bg);
            for extra in &entry.extra_lines {
                if lines.len() >= height { break; }
                let extra_wrap_width = full_width.saturating_sub(header_width + 1);
                let extra_wrapped = wrap_text(extra, extra_wrap_width, MAX_WRAP_LINES);

                for extra_line in &extra_wrapped {
                    if lines.len() >= height { break; }
                    let mut cs = empty_prefix(is_selected, row_bg);
                    if !app.filter.search_query.is_empty() {
                        cs.extend(highlight_with_filter(extra_line, &app.filter, cont));
                    } else {
                        cs.extend(auto_highlight(extra_line, cont));
                    }
                    let used: usize = cs.iter().map(|s| s.content.width()).sum();
                    if used < total_width {
                        cs.push(Span::styled(" ".repeat(total_width - used), Style::default().bg(row_bg)));
                    }
                    lines.push(Line::from(cs));
                    row_map.push(fi);
                }
            }

            if lines.len() >= height { break; }
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
        if prev_fi != Some(fi) { unique_entries += 1; prev_fi = Some(fi); }
    }
    app.layout.visible_entry_count = unique_entries;
    app.layout.row_to_filtered_idx = row_map;

    // Detect if move_down scrolled to the very bottom → re-enable auto_scroll
    if !app.auto_scroll && app.layout.rendered_to_end {
        app.auto_scroll = true;
        app.new_logs_since_pause = 0;
    }

    f.render_widget(Paragraph::new(lines).block(Block::default().borders(Borders::NONE)).style(Style::default().bg(BASE)), area);

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
        let mut state = ScrollbarState::new(max_offset)
            .position(start.min(max_offset));
        f.render_stateful_widget(scrollbar, area, &mut state);
    }
}

/// Calculate how many terminal rows a single entry occupies.
/// Must match the rendering logic exactly.
fn entry_row_count_from_store(store: &crate::domain::LogStore, store_idx: usize, full_width: usize) -> usize {
    let entry = match store.get(store_idx) {
        Some(e) => e,
        None => return 1,
    };

    if entry.tag == "────" {
        return 3;
    }

    // Header prefix width (must match render layout)
    // cursor(1) + bookmark(2) + time(TIME_WIDTH) + sep(1) + level(LEVEL_WIDTH) + sep(1) + tag(TAG_WIDTH) + sep(1)
    let header_width = 1 + 2 + TIME_WIDTH + 1 + LEVEL_WIDTH + 1 + TAG_WIDTH + 1;

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
    wrapped.len() + extra_rows
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
    let logo_h = LOGO.len() as u16 + 4; // logo + spacing + text
    let start_y = area.height.saturating_sub(logo_h) / 2;

    let mut lines: Vec<Line> = Vec::new();
    for _ in 0..start_y { lines.push(Line::raw("")); }

    for logo_line in logo_lines() {
        lines.push(logo_line);
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "   Flutter Log Viewer",
        Style::default().fg(OVERLAY0),
    )));
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "   Select a source to begin",
        Style::default().fg(SURFACE1),
    )));

    f.render_widget(Paragraph::new(lines).style(Style::default().bg(BASE)), area);
}

fn draw_waiting_for_logs(f: &mut Frame, app: &mut App, area: Rect) {
    let tick = app.tick;
    let logo_h = LOGO.len() as u16 + 5;
    let start_y = area.height.saturating_sub(logo_h) / 2;

    let spinner = match (tick / 5) % 8 {
        0 => "⣾", 1 => "⣽", 2 => "⣻", 3 => "⢿", 4 => "⡿", 5 => "⣟", 6 => "⣯", _ => "⣷",
    };

    let dots = ".".repeat(((tick / 10) % 4) as usize);

    let mut lines: Vec<Line> = Vec::new();
    for _ in 0..start_y { lines.push(Line::raw("")); }

    for logo_line in logo_lines() {
        lines.push(logo_line);
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "   Flutter Log Viewer",
        Style::default().fg(OVERLAY0),
    )));
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(format!("   {} ", spinner), Style::default().fg(BLUE)),
        Span::styled(format!("Waiting for logs{:<3}", dots), Style::default().fg(SUBTEXT0)),
    ]));

    f.render_widget(Paragraph::new(lines).style(Style::default().bg(BASE)), area);
}

fn draw_no_matching_logs(f: &mut Frame, area: Rect) {
    let mid = area.height / 2;
    let mut lines: Vec<Line> = Vec::new();
    for _ in 0..mid.saturating_sub(3) { lines.push(Line::raw("")); }

    // Empty state icon
    lines.push(Line::from(Span::styled(
        "          \u{2205}",  // ∅ symbol
        Style::default().fg(SURFACE1),
    )));
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "    No matching logs",
        Style::default().fg(OVERLAY0),
    )));
    lines.push(Line::from(Span::styled(
        "    Try adjusting your filters or level",
        Style::default().fg(SURFACE1),
    )));

    f.render_widget(Paragraph::new(lines).style(Style::default().bg(BASE)), area);
}

// ══════════════════════════════════════
//  Search Highlight
// ══════════════════════════════════════

fn highlight_with_filter(text: &str, filter: &crate::domain::FilterState, base: Style) -> Vec<Span<'static>> {
    let positions = filter.search_positions(text);
    if positions.is_empty() { return vec![Span::styled(text.to_string(), base)]; }

    let hl = Style::default().fg(MANTLE).bg(YELLOW).add_modifier(Modifier::BOLD);
    let mut spans = Vec::new();
    let mut last = 0;
    for r in &positions {
        let s = r.start.min(text.len());
        let e = r.end.min(text.len());
        if s > last { spans.push(Span::styled(text[last..s].to_string(), base)); }
        if s < e { spans.push(Span::styled(text[s..e].to_string(), hl)); }
        last = e;
    }
    if last < text.len() { spans.push(Span::styled(text[last..].to_string(), base)); }
    if spans.is_empty() { spans.push(Span::styled(text.to_string(), base)); }
    spans
}

// ══════════════════════════════════════
//  String Utils
// ══════════════════════════════════════

/// Wrap text into lines of at most `max_w` display-width characters.
/// Returns at most `max_lines` lines. The last line is truncated with "..." if needed.
fn wrap_text(s: &str, max_w: usize, max_lines: usize) -> Vec<String> {
    use unicode_width::UnicodeWidthChar;
    if max_w == 0 || max_lines == 0 { return vec![String::new()]; }

    let mut result: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_w: usize = 0;

    for ch in s.chars() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
        if current_w + cw > max_w {
            result.push(current);
            current = String::new();
            current_w = 0;

            if result.len() >= max_lines {
                // Truncate last line with "..."
                if let Some(last) = result.last_mut() {
                    let trunc_w = max_w.saturating_sub(3);
                    let mut trimmed = String::new();
                    let mut tw = 0;
                    for tc in last.chars() {
                        let tcw = UnicodeWidthChar::width(tc).unwrap_or(0);
                        if tw + tcw > trunc_w { break; }
                        trimmed.push(tc);
                        tw += tcw;
                    }
                    trimmed.push_str("...");
                    *last = trimmed;
                }
                return result;
            }
        }
        current.push(ch);
        current_w += cw;
    }
    if !current.is_empty() || result.is_empty() {
        result.push(current);
    }
    result
}

fn safe_truncate(s: &str, max_w: usize) -> String {
    if max_w == 0 { return String::new(); }
    if s.width() <= max_w { return s.to_string(); }
    let t = max_w.saturating_sub(3);
    let mut r = String::new();
    let mut w = 0;
    for c in s.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if w + cw > t { break; }
        r.push(c); w += cw;
    }
    r.push_str("...");
    r
}

fn safe_pad(s: &str, width: usize) -> String {
    let w = s.width();
    if w >= width {
        let mut r = String::new();
        let mut cw = 0;
        for c in s.chars() {
            let ch_w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
            if cw + ch_w > width { break; }
            r.push(c); cw += ch_w;
        }
        while cw < width { r.push(' '); cw += 1; }
        r
    } else {
        let mut r = s.to_string();
        for _ in 0..(width - w) { r.push(' '); }
        r
    }
}
