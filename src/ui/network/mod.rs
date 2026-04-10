//! Network Inspector view — request list with filtering and detail panel.

pub mod detail;
pub mod filter;

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

use crate::app::App;
use crate::domain::network::{NetworkStatus, Protocol};

// Import shared palette from parent
use super::{
    BASE, MANTLE, SURFACE0, SURFACE1, OVERLAY0, TEXT, SUBTEXT0,
    BLUE, SAPPHIRE, TEAL, GREEN, YELLOW, PEACH, RED, MAUVE, PINK, LAVENDER,
    safe_pad, safe_truncate,
};

// Column widths
const PROTO_W: usize = 6;
const METHOD_W: usize = 8;
const STATUS_W: usize = 10;
const TIME_W: usize = 8;
const SIZE_W: usize = 8;

// Row background colors for error/warning states
const ERROR_ROW_BG: Color = Color::Rgb(50, 30, 35);
const WARNING_ROW_BG: Color = Color::Rgb(50, 45, 30);

// ══════════════════════════════════════
//  Helper Functions
// ══════════════════════════════════════

pub fn method_color(method: &str) -> Color {
    match method.to_uppercase().as_str() {
        "GET" => BLUE,
        "POST" => GREEN,
        "PUT" => YELLOW,
        "DELETE" => RED,
        "PATCH" => PEACH,
        "HEAD" => OVERLAY0,
        "OPTIONS" => MAUVE,
        _ => SUBTEXT0,
    }
}

pub fn status_color(status: NetworkStatus, http_status: Option<u16>) -> Color {
    match status {
        NetworkStatus::Pending => OVERLAY0,
        NetworkStatus::Active => PEACH,
        NetworkStatus::Failed => RED,
        NetworkStatus::Completed => {
            if let Some(code) = http_status {
                if code >= 500 {
                    RED
                } else if code >= 400 {
                    YELLOW
                } else if code >= 300 {
                    LAVENDER
                } else {
                    GREEN
                }
            } else {
                GREEN
            }
        }
    }
}

fn duration_color(ms: u64) -> Color {
    if ms >= 3000 {
        RED
    } else if ms >= 1000 {
        YELLOW
    } else if ms >= 500 {
        PEACH
    } else {
        SUBTEXT0
    }
}

pub fn format_duration(ms: u64) -> String {
    if ms >= 60000 {
        format!("{:.1}m", ms as f64 / 60000.0)
    } else if ms >= 1000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{}ms", ms)
    }
}

pub fn format_size(bytes: u64) -> String {
    if bytes >= 1_000_000 {
        format!("{:.1}MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1000 {
        format!("{:.1}KB", bytes as f64 / 1000.0)
    } else {
        format!("{}B", bytes)
    }
}

fn protocol_pill(protocol: Protocol) -> Span<'static> {
    let (label, fg, bg) = match protocol {
        Protocol::Http => ("HTTP", MANTLE, BLUE),
        Protocol::Sse => ("SSE", MANTLE, TEAL),
        Protocol::Ws => ("WS", MANTLE, PINK),
    };
    let text = format!(" {} ", label);
    Span::styled(text, Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD))
}

// ══════════════════════════════════════
//  Main Network View Draw
// ══════════════════════════════════════

pub fn draw_network(f: &mut Frame, app: &mut App, area: Rect) {
    // Vertical: toolbar (2 rows) | header | body | status bar
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),  // toolbar (search + filters)
            Constraint::Length(1),  // header
            Constraint::Min(3),     // body (table + optional detail)
            Constraint::Length(1),  // status bar
        ])
        .split(area);

    app.layout.toolbar_y = rows[0].y;
    app.layout.list_y = rows[2].y;
    app.layout.list_height = rows[2].height;
    app.layout.bottom_y = rows[3].y;

    filter::draw_network_toolbar(f, app, rows[0]);
    draw_table_header(f, app, rows[1]);

    // Body: detail panel or table
    if app.network.show_detail {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(60),
                Constraint::Percentage(40),
            ])
            .split(rows[2]);

        app.layout.net_detail_x = cols[1].x;
        draw_table_body(f, app, cols[0]);
        detail::draw_network_detail(f, app, cols[1]);
    } else {
        app.layout.net_detail_x = app.layout.width;
        draw_table_body(f, app, rows[2]);
    }

    draw_network_status_bar(f, app, rows[3]);
}

// ══════════════════════════════════════
//  Table Header
// ══════════════════════════════════════

fn draw_table_header(f: &mut Frame, _app: &mut App, area: Rect) {
    let bg = SURFACE0;
    let style = Style::default().fg(TEXT).bg(bg).add_modifier(Modifier::BOLD);

    let mut spans: Vec<Span> = vec![
        Span::styled(" ", Style::default().bg(bg)),  // cursor space
        Span::styled(safe_pad("PROTO", PROTO_W), style),
        Span::styled(" ", Style::default().bg(bg)),
        Span::styled(safe_pad("METHOD", METHOD_W), style),
        Span::styled(" ", Style::default().bg(bg)),
    ];

    // URL takes remaining space
    let fixed_width = 1 + PROTO_W + 1 + METHOD_W + 1 + STATUS_W + 1 + TIME_W + 1 + SIZE_W + 1;
    let url_width = (area.width as usize).saturating_sub(fixed_width);
    spans.push(Span::styled(safe_pad("URL", url_width), style));

    spans.extend(vec![
        Span::styled(safe_pad("STATUS", STATUS_W), style),
        Span::styled(" ", Style::default().bg(bg)),
        Span::styled(safe_pad("TIME", TIME_W), style),
        Span::styled(" ", Style::default().bg(bg)),
        Span::styled(safe_pad("SIZE", SIZE_W), style),
    ]);

    // Fill remaining
    let used: usize = spans.iter().map(|s| s.content.width()).sum();
    if used < area.width as usize {
        spans.push(Span::styled(" ".repeat(area.width as usize - used), Style::default().bg(bg)));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

// ══════════════════════════════════════
//  Table Body
// ══════════════════════════════════════

fn draw_table_body(f: &mut Frame, app: &mut App, area: Rect) {
    let height = area.height as usize;
    let total_width = area.width as usize;

    // Get filtered indices
    let filtered_indices: Vec<usize> = app.network.filtered_indices(&app.network_store).to_vec();
    let filtered_count = filtered_indices.len();

    // Empty state
    if filtered_count == 0 {
        draw_empty_network(f, app, area);
        return;
    }

    // Clamp selected first
    app.network.selected = app.network.selected.min(filtered_count.saturating_sub(1));

    if app.network.auto_scroll {
        // Pin to bottom
        app.network.selected = filtered_count.saturating_sub(1);
        app.network.scroll_offset = filtered_count.saturating_sub(height);
    } else {
        // Ensure scroll_offset is valid: never exceed max scrollable position
        let max_offset = filtered_count.saturating_sub(height);
        app.network.scroll_offset = app.network.scroll_offset.min(max_offset);

        // Keep selected within visible viewport
        if app.network.selected < app.network.scroll_offset {
            app.network.scroll_offset = app.network.selected;
        }
        if height > 0 && app.network.selected >= app.network.scroll_offset + height {
            app.network.scroll_offset = app.network.selected.saturating_sub(height - 1);
        }

        // Re-enable auto-scroll when selected is at the very bottom
        if app.network.selected + 1 >= filtered_count {
            app.network.auto_scroll = true;
        }
    }

    // Build lines
    let mut lines: Vec<Line> = Vec::new();
    let start = app.network.scroll_offset;

    for (vi, &store_idx) in filtered_indices.iter().skip(start).take(height).enumerate() {
        let fi = start + vi;
        let is_selected = fi == app.network.selected;

        if let Some(entry) = app.network_store.get(store_idx) {
            let row_bg = if is_selected {
                SURFACE1
            } else if entry.status == NetworkStatus::Failed {
                ERROR_ROW_BG
            } else if entry.http_status.map(|c| c >= 400).unwrap_or(false) {
                WARNING_ROW_BG
            } else {
                BASE
            };

            let cursor = if is_selected {
                Span::styled("\u{258e}", Style::default().fg(BLUE).bg(row_bg))  // ▎
            } else {
                Span::styled(" ", Style::default().bg(row_bg))
            };

            // Protocol pill
            let proto_span = protocol_pill(entry.protocol);

            // Method
            let method_c = method_color(&entry.method);
            let method_text = if entry.method.is_empty() {
                "-".to_string()
            } else {
                entry.method.clone()
            };
            let method_span = Span::styled(
                safe_pad(&method_text, METHOD_W),
                Style::default().fg(method_c).bg(row_bg),
            );

            // URL (takes remaining space) — show path only (strip query) for compact display
            let fixed_width = 1 + PROTO_W + 1 + METHOD_W + 1 + STATUS_W + 1 + TIME_W + 1 + SIZE_W + 1;
            let url_width = total_width.saturating_sub(fixed_width);
            let path_only = entry.path.split('?').next().unwrap_or(&entry.path);
            let url_display = safe_truncate(path_only, url_width);
            let url_span = Span::styled(
                safe_pad(&url_display, url_width),
                Style::default().fg(TEXT).bg(row_bg),
            );

            // Status
            let status_text = match entry.status {
                NetworkStatus::Pending => "...".to_string(),
                NetworkStatus::Active => "active".to_string(),
                NetworkStatus::Failed => "failed".to_string(),
                NetworkStatus::Completed => {
                    entry.http_status.map(|c| c.to_string()).unwrap_or_else(|| "done".to_string())
                }
            };
            let status_c = status_color(entry.status, entry.http_status);
            let status_span = Span::styled(
                safe_pad(&status_text, STATUS_W),
                Style::default().fg(status_c).bg(row_bg),
            );

            // Duration
            let time_text = entry.duration.map(format_duration).unwrap_or_else(|| "-".to_string());
            let time_c = entry.duration.map(duration_color).unwrap_or(OVERLAY0);
            let time_span = Span::styled(
                safe_pad(&time_text, TIME_W),
                Style::default().fg(time_c).bg(row_bg),
            );

            // Size
            let size = entry.display_size();
            let size_text = if size > 0 { format_size(size) } else { "-".to_string() };
            let size_span = Span::styled(
                safe_pad(&size_text, SIZE_W),
                Style::default().fg(SUBTEXT0).bg(row_bg),
            );

            let sep = Span::styled(" ", Style::default().bg(row_bg));

            let mut spans = vec![
                cursor,
                proto_span,
                sep.clone(),
                method_span,
                sep.clone(),
                url_span,
                status_span,
                sep.clone(),
                time_span,
                sep.clone(),
                size_span,
            ];

            // Fill remaining width
            let used: usize = spans.iter().map(|s| s.content.width()).sum();
            if used < total_width {
                spans.push(Span::styled(" ".repeat(total_width - used), Style::default().bg(row_bg)));
            }

            lines.push(Line::from(spans));
        }
    }

    f.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::NONE))
            .style(Style::default().bg(BASE)),
        area,
    );

    // Scrollbar
    if filtered_count > height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_symbol("\u{2503}")  // ┃
            .track_symbol(Some(" "))
            .thumb_style(Style::default().fg(SAPPHIRE))
            .track_style(Style::default().fg(SURFACE0).bg(BASE))
            .begin_symbol(None)
            .end_symbol(None);
        let max_offset = filtered_count.saturating_sub(height);
        let mut state = ScrollbarState::new(max_offset)
            .position(start.min(max_offset));
        f.render_stateful_widget(scrollbar, area, &mut state);
    }
}

// ══════════════════════════════════════
//  Empty State
// ══════════════════════════════════════

fn draw_empty_network(f: &mut Frame, app: &mut App, area: Rect) {
    let mid_y = area.height / 2;
    let mut lines: Vec<Line> = Vec::new();

    for _ in 0..mid_y.saturating_sub(3) {
        lines.push(Line::raw(""));
    }

    if app.network_store.is_empty() {
        lines.push(Line::from(Span::styled(
            "    Network Inspector",
            Style::default().fg(SAPPHIRE).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "    Add FlogHttpInterceptor to your Dio instance",
            Style::default().fg(OVERLAY0),
        )));
        lines.push(Line::from(Span::styled(
            "    to see network requests here.",
            Style::default().fg(SURFACE1),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "          \u{2205}",  // empty set symbol
            Style::default().fg(SURFACE1),
        )));
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "    No matching requests",
            Style::default().fg(OVERLAY0),
        )));
        lines.push(Line::from(Span::styled(
            "    Try adjusting your filters",
            Style::default().fg(SURFACE1),
        )));
    }

    f.render_widget(Paragraph::new(lines).style(Style::default().bg(BASE)), area);
}

// ══════════════════════════════════════
//  Status Bar
// ══════════════════════════════════════

fn draw_network_status_bar(f: &mut Frame, app: &mut App, area: Rect) {
    let bg = MANTLE;

    // Show toast message if active
    if let Some(msg) = app.active_status().map(|s| s.to_string()) {
        let line = Line::from(vec![
            Span::styled(" OK ", Style::default().fg(MANTLE).bg(GREEN).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {} ", msg), Style::default().fg(TEXT).bg(bg)),
        ]);
        f.render_widget(Paragraph::new(line).style(Style::default().bg(bg)), area);
        app.layout.net_buttons.clear();
        return;
    }

    // Count stats
    let total = app.network_store.len();
    let filtered = app.network.filtered_indices(&app.network_store).len();
    let failed_count = app.network_store.iter()
        .filter(|e| e.status == NetworkStatus::Failed || e.http_status.map(|c| c >= 400).unwrap_or(false))
        .count();

    // LIVE / scroll indicator (matching Logs view)
    let (live_text, live_style) = if app.network.auto_scroll {
        let dot = match (app.tick / 8) % 4 { 0 => "\u{25cf}", 1 => "\u{25c9}", 2 => "\u{25cf}", _ => "\u{25cb}" };
        (format!(" {} LIVE ", dot), Style::default().fg(MANTLE).bg(GREEN).add_modifier(Modifier::BOLD))
    } else {
        let pct = if filtered > 0 {
            ((app.network.selected + 1) * 100) / filtered
        } else {
            100
        };
        (format!(" {}% ", pct.min(100)), Style::default().fg(TEXT).bg(SURFACE0))
    };

    let info = format!(" {}/{} requests", filtered, total);
    let failed_info = if failed_count > 0 {
        format!("  {} failed", failed_count)
    } else {
        String::new()
    };

    let buttons: Vec<(&str, &str, Style)> = vec![
        ("curl", " Copy as cURL ", Style::default().fg(MANTLE).bg(GREEN).add_modifier(Modifier::BOLD)),
        ("response", " Copy Response ", Style::default().fg(MANTLE).bg(SAPPHIRE).add_modifier(Modifier::BOLD)),
        ("clear", " Clear ", Style::default().fg(MANTLE).bg(PEACH).add_modifier(Modifier::BOLD)),
        ("help", " ? ", Style::default().fg(MANTLE).bg(OVERLAY0).add_modifier(Modifier::BOLD)),
    ];

    let lw = live_text.width() as u16;
    let info_w = info.width() as u16;
    let failed_w = failed_info.width() as u16;
    let bw: u16 = buttons.iter().map(|(_, l, _)| l.width() as u16 + 1).sum();
    let spacer = area.width.saturating_sub(lw + info_w + failed_w + bw).max(1);

    let mut spans = vec![
        Span::styled(&live_text, live_style),
        Span::styled(&info, Style::default().fg(SUBTEXT0).bg(bg)),
    ];

    if !failed_info.is_empty() {
        spans.push(Span::styled(&failed_info, Style::default().fg(RED).bg(bg)));
    }

    spans.push(Span::styled(" ".repeat(spacer as usize), Style::default().bg(bg)));

    // Store button click regions
    let mut xc = info_w + failed_w + spacer as u16;
    app.layout.net_buttons.clear();
    for (i, (name, label, style)) in buttons.iter().enumerate() {
        let start = xc;
        spans.push(Span::styled(*label, *style));
        xc += label.width() as u16;
        app.layout.net_buttons.push((name.to_string(), start, xc));
        if i < buttons.len() - 1 {
            spans.push(Span::styled(" ", Style::default().bg(bg)));
            xc += 1;
        }
    }

    f.render_widget(Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)), area);
}
