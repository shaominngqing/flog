//! Network request-list table body — the middle row of the Network view.
//!
//! Phase 3 Step 3.8 (UI-010 mirror): extracted from `ui/network/mod.rs`.

use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::domain::network::{EntrySource, NetworkStatus};

use super::super::{
    safe_pad, safe_truncate, BASE, BLUE, MANTLE, MAUVE, OVERLAY0, SAPPHIRE, SUBTEXT0, SURFACE0,
    SURFACE1, TEXT,
};
use super::status_bar::draw_empty_network;
use super::{
    compute_visible_network_range, duration_color, format_duration, format_size, method_color,
    protocol_pill, status_color, METHOD_W, PROTO_W, SIZE_W, STATUS_W, TIME_W,
};

// Row background colors for error/warning states
const ERROR_ROW_BG: Color = Color::Rgb(50, 30, 35);
const WARNING_ROW_BG: Color = Color::Rgb(50, 45, 30);
const REPLAY_ROW_BG: Color = Color::Rgb(35, 45, 65); // subtle blue tint
const MOCKED_ROW_BG: Color = Color::Rgb(50, 35, 65); // subtle purple tint

pub(super) fn draw_table_body(f: &mut Frame, app: &mut App, area: Rect) {
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
    let (start, end) =
        compute_visible_network_range(filtered_indices.len(), app.network.scroll_offset, height);

    for (vi, &store_idx) in filtered_indices[start..end].iter().enumerate() {
        let fi = start + vi;
        let is_selected = fi == app.network.selected;

        if let Some(entry) = app.network_store.get(store_idx) {
            let normal_bg = match entry.source {
                EntrySource::Replay => REPLAY_ROW_BG,
                EntrySource::Mocked => MOCKED_ROW_BG,
                EntrySource::App => BASE,
            };
            let row_bg = if is_selected {
                SURFACE1
            } else if entry.status == NetworkStatus::Failed {
                ERROR_ROW_BG
            } else if entry.http_status.map(|c| c >= 400).unwrap_or(false) {
                WARNING_ROW_BG
            } else {
                normal_bg
            };

            let cursor = if is_selected {
                Span::styled("\u{258e}", Style::default().fg(BLUE).bg(row_bg)) // ▎
            } else {
                Span::styled(" ", Style::default().bg(row_bg))
            };

            // Protocol pill
            let proto_span = protocol_pill(entry.protocol);

            // Method (with replay/mock indicator)
            let method_c = method_color(&entry.method);
            let method_text = match entry.source {
                EntrySource::Replay => {
                    if entry.method.is_empty() {
                        "\u{21bb}-".to_string() // ↻
                    } else {
                        format!("\u{21bb}{}", entry.method)
                    }
                }
                EntrySource::Mocked => {
                    if entry.method.is_empty() {
                        "\u{25c6}-".to_string() // ◆
                    } else {
                        format!("\u{25c6}{}", entry.method)
                    }
                }
                EntrySource::App => {
                    if entry.method.is_empty() {
                        "-".to_string()
                    } else {
                        entry.method.clone()
                    }
                }
            };
            let method_span = Span::styled(
                safe_pad(&method_text, METHOD_W),
                Style::default().fg(method_c).bg(row_bg),
            );

            // Source tag (MOCK / REPLAY pill) — takes space before URL
            let source_tag: Option<Span> = match entry.source {
                EntrySource::Mocked => Some(Span::styled(
                    " MOCK ",
                    Style::default()
                        .fg(MANTLE)
                        .bg(MAUVE)
                        .add_modifier(ratatui::style::Modifier::BOLD),
                )),
                EntrySource::Replay => Some(Span::styled(
                    " REPLAY ",
                    Style::default()
                        .fg(MANTLE)
                        .bg(BLUE)
                        .add_modifier(ratatui::style::Modifier::BOLD),
                )),
                EntrySource::App => None,
            };
            let tag_w = source_tag
                .as_ref()
                .map(|s| s.content.width() + 1)
                .unwrap_or(0);

            // URL (takes remaining space) — show path only (strip query) for compact display
            let fixed_width =
                1 + PROTO_W + 1 + METHOD_W + 1 + tag_w + STATUS_W + 1 + TIME_W + 1 + SIZE_W + 1;
            let url_width = total_width.saturating_sub(fixed_width);
            let path_only = entry.path.split('?').next().unwrap_or(&entry.path);
            let url_display = safe_truncate(path_only, url_width);
            let url_color = match entry.source {
                EntrySource::Mocked => MAUVE,
                EntrySource::Replay => BLUE,
                EntrySource::App => TEXT,
            };
            let url_span = Span::styled(
                safe_pad(&url_display, url_width),
                Style::default().fg(url_color).bg(row_bg),
            );

            // Status
            let status_text = match entry.status {
                NetworkStatus::Pending => "...".to_string(),
                NetworkStatus::Active => "active".to_string(),
                NetworkStatus::Failed => "failed".to_string(),
                NetworkStatus::Orphan => "orphan".to_string(),
                NetworkStatus::Completed => entry
                    .http_status
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "done".to_string()),
            };
            let status_c = status_color(entry.status, entry.http_status);
            let status_span = Span::styled(
                safe_pad(&status_text, STATUS_W),
                Style::default().fg(status_c).bg(row_bg),
            );

            // Duration
            let time_text = entry
                .duration
                .map(format_duration)
                .unwrap_or_else(|| "-".to_string());
            let time_c = entry.duration.map(duration_color).unwrap_or(OVERLAY0);
            let time_span = Span::styled(
                safe_pad(&time_text, TIME_W),
                Style::default().fg(time_c).bg(row_bg),
            );

            // Size
            let size = entry.display_size();
            let size_text = if size > 0 {
                format_size(size)
            } else {
                "-".to_string()
            };
            let size_span = Span::styled(
                safe_pad(&size_text, SIZE_W),
                Style::default().fg(SUBTEXT0).bg(row_bg),
            );

            let sep = Span::styled(" ", Style::default().bg(row_bg));

            let mut spans = vec![cursor, proto_span, sep.clone(), method_span, sep.clone()];
            if let Some(tag) = source_tag {
                spans.push(tag);
                spans.push(sep.clone());
            }
            spans.extend([
                url_span,
                status_span,
                sep.clone(),
                time_span,
                sep.clone(),
                size_span,
            ]);

            // Fill remaining width
            let used: usize = spans.iter().map(|s| s.content.width()).sum();
            if used < total_width {
                spans.push(Span::styled(
                    " ".repeat(total_width - used),
                    Style::default().bg(row_bg),
                ));
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
            .thumb_symbol("\u{2503}") // ┃
            .track_symbol(Some(" "))
            .thumb_style(Style::default().fg(SAPPHIRE))
            .track_style(Style::default().fg(SURFACE0).bg(BASE))
            .begin_symbol(None)
            .end_symbol(None);
        let max_offset = filtered_count.saturating_sub(height);
        let mut state = ScrollbarState::new(max_offset).position(start.min(max_offset));
        f.render_stateful_widget(scrollbar, area, &mut state);
    }
}
