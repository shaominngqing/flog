//! Network Inspector view — request list with filtering and detail panel.

pub mod detail;
pub mod filter;
pub mod mock_rules;
pub mod stats;
mod status_bar;
mod table;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::Paragraph,
    Frame,
};

use crate::app::App;
use crate::domain::network::{NetworkStatus, Protocol};

// Import shared palette from parent
use super::{
    BLUE, GREEN, LAVENDER, MANTLE, MAUVE, OVERLAY0, PEACH, PINK, RED, SUBTEXT0, TEAL, YELLOW,
};

use status_bar::draw_network_status_bar;
use table::draw_table_body;

/// Phase 2.5A — extracted from UI-029.
/// Pure: network-tab viewport slicing. Network entries are all 1-row
/// so this is a real fixed-window `(start, end)`. Kept separate from
/// `ui/logs::compute_visible_entry_start` because logs uses variable-
/// height rows (see UI-006 for the unification question).
#[allow(dead_code)]
pub(crate) fn compute_visible_network_range(
    total_filtered: usize,
    offset: usize,
    height: usize,
) -> (usize, usize) {
    let start = offset.min(total_filtered);
    let end = start.saturating_add(height).min(total_filtered);
    (start, end)
}

// Column widths (shared with table.rs).
pub(super) const PROTO_W: usize = 6;
pub(super) const METHOD_W: usize = 8;
pub(super) const STATUS_W: usize = 10;
pub(super) const TIME_W: usize = 8;
pub(super) const SIZE_W: usize = 8;

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
        // DOM-003: orphan responses are highlighted in yellow to distinguish
        // them from normal completed entries.
        NetworkStatus::Orphan => YELLOW,
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

/// Color for duration based on latency thresholds.
///
/// Thresholds (>1000 ms red, >500 ms yellow, else green) are intentionally
/// magic today — audit UI-032 ack. Extraction to named constants
/// (`DURATION_SLOW_MS` / `DURATION_WARN_MS`) is a cosmetic improvement
/// left to a later sweep over all ui/ thresholds; doing it alone would
/// add one-file churn without settling the palette question.
pub(super) fn duration_color(ms: u64) -> Color {
    if ms > 1000 {
        RED
    } else if ms > 500 {
        YELLOW
    } else {
        GREEN
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

pub(super) fn protocol_pill(protocol: Protocol) -> Span<'static> {
    let (label, fg, bg) = match protocol {
        Protocol::Http => ("HTTP", MANTLE, BLUE),
        Protocol::Sse => ("SSE", MANTLE, TEAL),
        Protocol::Ws => ("WS", MANTLE, PINK),
    };
    let text = format!(" {} ", label);
    Span::styled(
        text,
        Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
    )
}

// ══════════════════════════════════════
//  Main Network View Draw
// ══════════════════════════════════════

pub fn draw_network(f: &mut Frame, app: &mut App, area: Rect) {
    // 8-row chrome: sep | op1 | gap | op2 | sep | col_header | main | status
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // sep
            Constraint::Length(1), // op1: inputs
            Constraint::Length(1), // blank spacer
            Constraint::Length(1), // op2: pills
            Constraint::Length(1), // sep
            Constraint::Length(1), // column header
            Constraint::Min(3),    // list (+ optional detail)
            Constraint::Length(1), // status
        ])
        .split(area);

    app.layout.net_toolbar_y = rows[1].y;
    app.layout.input_row_y = rows[1].y;
    app.layout.net_col_header_y = rows[5].y;
    app.layout.toolbar_y = rows[1].y;
    app.layout.list_y = rows[6].y;
    app.layout.list_height = rows[6].height;
    app.layout.bottom_y = rows[7].y;

    crate::ui::draw_separator_rule(f, rows[0]);
    let count = app.network.filtered_count(&app.network_store);
    let total = app.network_store.len();
    filter::draw_network_op1(f, app, rows[1], count, total);
    // rows[2] is a blank spacer — paint MANTLE bg to match toolbar.
    f.render_widget(
        Paragraph::new("").style(Style::default().bg(MANTLE)),
        rows[2],
    );
    filter::draw_network_op2(f, app, rows[3]);
    crate::ui::draw_separator_rule(f, rows[4]);
    filter::draw_network_column_header(f, rows[5]);

    // Body: detail panel / mock rules panel / full table
    if app.network.show_mock_rules_panel {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(rows[6]);

        app.layout.net_detail_x = cols[1].x;
        draw_table_body(f, app, cols[0]);
        mock_rules::draw_mock_rules_panel(f, app, cols[1]);
    } else if app.network.show_detail {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(rows[6]);

        app.layout.net_detail_x = cols[1].x;
        draw_table_body(f, app, cols[0]);
        detail::draw_network_detail(f, app, cols[1]);
    } else {
        app.layout.net_detail_x = app.layout.width;
        draw_table_body(f, app, rows[6]);
    }

    draw_network_status_bar(f, app, rows[7]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_duration_color() {
        assert_eq!(duration_color(100), GREEN);
        assert_eq!(duration_color(500), GREEN);
        assert_eq!(duration_color(501), YELLOW);
        assert_eq!(duration_color(1000), YELLOW);
        assert_eq!(duration_color(1001), RED);
        assert_eq!(duration_color(5000), RED);
    }

    #[test]
    fn network_visible_range_basic_window() {
        assert_eq!(compute_visible_network_range(100, 10, 20), (10, 30));
    }

    #[test]
    fn network_visible_range_clamps_end() {
        assert_eq!(compute_visible_network_range(15, 10, 20), (10, 15));
    }

    #[test]
    fn network_visible_range_clamps_start() {
        assert_eq!(compute_visible_network_range(5, 100, 20), (5, 5));
    }

    #[test]
    fn network_visible_range_empty() {
        assert_eq!(compute_visible_network_range(0, 0, 10), (0, 0));
        assert_eq!(compute_visible_network_range(0, 50, 10), (0, 0));
    }
}
