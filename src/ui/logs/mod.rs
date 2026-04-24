//! Logs view — main log list with toolbar and status bar.

pub mod detail;
mod empty_states;
pub mod highlight;
pub mod jump;
mod list;
pub mod stats;
mod status_bar;
mod toolbar;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::Paragraph,
    Frame,
};

use crate::app::App;
use crate::domain::LogLevel;
use empty_states::{
    draw_jump_to_bottom, draw_no_matching_logs, draw_not_connected, draw_waiting_for_logs,
};
use list::draw_log_list;
use status_bar::{draw_column_header, draw_status_bar};
use toolbar::{draw_toolbar_op1, draw_toolbar_op2};

// Import shared palette from parent
use super::{
    safe_pad, wrap_text, BASE, BLUE, GREEN, MANTLE, MAUVE, OVERLAY0, PEACH, PINK, RED, SAPPHIRE,
    SUBTEXT0, SURFACE0, SURFACE1, TEXT, YELLOW,
};

/// Phase 2.5A — extracted from UI-010.
/// Phase 2.5A — extracted from UI-010.
/// Pure: clamp the viewport start index to `[0, total_filtered]`.
///
/// Note: this is simpler than it looks. Logs use a row-walking render
/// model with variable-height rows (separators are 3 rows, entries can
/// wrap up to MAX_WRAP_LINES), so there is NO fixed-window `(start,end)`
/// slice — the renderer walks entries until `rows_used >= height`.
/// This function only encapsulates the start clamp. Phase 3 (UI-006 /
/// UI-010) decides whether to move logs to fixed-height rows, at which
/// point this can return a full (start, end) tuple.
pub(crate) fn compute_visible_entry_start(total_filtered: usize, offset: usize) -> usize {
    offset.min(total_filtered)
}

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

/// Phase 2.5A — extracted from UI-030.
/// Pure: given a repeat count and max rendered width, return how many
/// '█' characters should be drawn in the bar. Saturates at count 50
/// (magic constant preserved from the original; Phase 3 UI-030 will
/// name it, likely as REPEAT_BAR_MAX_COUNT).
pub(crate) fn repeat_bar_normalized(count: usize, max_w: usize) -> usize {
    let len = (count.min(50) * max_w) / 50;
    len.min(max_w)
}

fn repeat_bar(count: usize, max_w: usize) -> String {
    let len = repeat_bar_normalized(count, max_w);
    format!("x{} {}", count, "█".repeat(len))
}

// ══════════════════════════════════════
//  Main Logs View Draw
// ══════════════════════════════════════

pub fn draw_logs(f: &mut Frame, app: &mut App, area: Rect) {
    // Layout: 8 rows — sep | op1 | gap | op2 | sep | col_header | main | status
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // sep below tab bar
            Constraint::Length(1), // op row 1: inputs
            Constraint::Length(1), // blank spacer between op1 and op2
            Constraint::Length(1), // op row 2: levels + counts
            Constraint::Length(1), // sep below ops
            Constraint::Length(1), // column header
            Constraint::Min(3),    // main
            Constraint::Length(1), // status
        ])
        .split(area);

    app.layout.toolbar_y = rows[1].y; // op row 1 (inputs)
    app.layout.toolbar_op2_y = rows[3].y; // op row 2 (levels + counts)
    app.layout.input_row_y = rows[1].y;
    app.layout.col_header_y = rows[5].y;
    app.layout.bottom_y = rows[7].y;

    crate::ui::draw_separator_rule(f, rows[0]);
    draw_toolbar_op1(f, app, rows[1]);
    // rows[2] is a blank spacer — paint just the MANTLE bg to match toolbar.
    f.render_widget(
        Paragraph::new("").style(Style::default().bg(MANTLE)),
        rows[2],
    );
    draw_toolbar_op2(f, app, rows[3]);
    crate::ui::draw_separator_rule(f, rows[4]);
    draw_column_header(f, rows[5]);

    let list_area = if app.show_detail_panel {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(100 - app.detail_panel_pct),
                Constraint::Percentage(app.detail_panel_pct),
            ])
            .split(rows[6]);

        app.layout.list_y = cols[0].y;
        app.layout.list_height = cols[0].height;

        draw_log_list(f, app, cols[0]);
        detail::draw_side_panel(f, app, cols[1]);
        cols[0]
    } else {
        app.layout.list_y = rows[6].y;
        app.layout.list_height = rows[6].height;

        draw_log_list(f, app, rows[6]);
        rows[6]
    };

    draw_jump_to_bottom(f, app, list_area);

    draw_status_bar(f, app, rows[7]);
}

/// Phase 2.5A — extracted from UI-010.
/// Pure: calculate how many terminal rows a single entry occupies given
/// the full terminal width. Mirrors the inline rendering logic in
/// `entry_row_count_from_store` exactly (copied verbatim; only the
/// store lookup lives in the caller).
pub(crate) fn entry_row_count(entry: &crate::domain::entry::LogEntry, full_width: usize) -> usize {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_start_within_bounds() {
        assert_eq!(compute_visible_entry_start(100, 10), 10);
    }

    #[test]
    fn visible_start_equals_offset_when_offset_lt_total() {
        assert_eq!(compute_visible_entry_start(15, 10), 10);
    }

    #[test]
    fn visible_start_clamps_to_total_when_offset_too_large() {
        assert_eq!(compute_visible_entry_start(5, 100), 5);
    }

    #[test]
    fn visible_start_zero_total() {
        assert_eq!(compute_visible_entry_start(0, 0), 0);
        assert_eq!(compute_visible_entry_start(0, 50), 0);
    }

    /// Build a minimal LogEntry for entry_row_count tests. LogEntry has
    /// no Default impl (by design — Phase 3 decision), so every field
    /// is listed explicitly.
    fn make_test_entry(ts: &str, tag: &str, msg: &str) -> crate::domain::entry::LogEntry {
        crate::domain::entry::LogEntry {
            timestamp: ts.to_string(),
            level: crate::domain::entry::LogLevel::Info,
            tag: tag.to_string(),
            message: msg.to_string(),
            extra_lines: Vec::new(),
            repeat_count: 1,
            source: crate::domain::entry::InputSource::DirectSocket,
            error: None,
            stacktrace: None,
        }
    }

    #[test]
    fn entry_row_count_separator_is_three() {
        let sep = make_test_entry("", "────", "");
        assert_eq!(entry_row_count(&sep, 80), 3);
    }

    #[test]
    fn entry_row_count_short_message_one_row() {
        let e = make_test_entry("t", "TAG", "short msg");
        // 1 row for message; no extra_lines; no stack
        assert_eq!(entry_row_count(&e, 80), 1);
    }

    #[test]
    fn entry_row_count_caps_at_max_wrap_lines() {
        let very_long = "x".repeat(5000);
        let e = make_test_entry("t", "TAG", &very_long);
        // Header width is 1+2+LEVEL_WIDTH+1+TIME_WIDTH+1+TAG_WIDTH+1 = 41;
        // full_width must exceed header_width+1 for wrap_width > 0 and
        // therefore for wrap_text to produce > 1 line. At full_width=80
        // wrap_width=38, and 5000 xs cap at MAX_WRAP_LINES = 3.
        assert_eq!(entry_row_count(&e, 80), MAX_WRAP_LINES);
    }

    #[test]
    fn repeat_bar_normalized_zero_count() {
        assert_eq!(repeat_bar_normalized(0, 20), 0);
    }

    #[test]
    fn repeat_bar_normalized_saturates_at_50() {
        // at count=50 the bar fills max_w
        assert_eq!(repeat_bar_normalized(50, 20), 20);
        // beyond 50, still saturated
        assert_eq!(repeat_bar_normalized(100, 20), 20);
        assert_eq!(repeat_bar_normalized(1_000_000, 20), 20);
    }

    #[test]
    fn repeat_bar_normalized_proportional() {
        // at count=25 (half of 50), bar is half of max_w
        assert_eq!(repeat_bar_normalized(25, 20), 10);
        // at count=10 (1/5), bar is 1/5 of max_w
        assert_eq!(repeat_bar_normalized(10, 20), 4);
    }

    #[test]
    fn repeat_bar_normalized_zero_width() {
        assert_eq!(repeat_bar_normalized(42, 0), 0);
    }
}
