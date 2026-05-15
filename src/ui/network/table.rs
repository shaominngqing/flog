//! Network request-list table body — the middle row of the Network view.
//!
//! Phase 3 Step 3.8 (UI-010 mirror): extracted from `ui/network/mod.rs`.

use ratatui::{
    layout::{Constraint, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table,
        TableState,
    },
    Frame,
};

use crate::app::App;
use crate::domain::network::{EntrySource, NetworkStatus};

use super::super::{safe_truncate, BASE, BLUE, MANTLE, MAUVE, OVERLAY0, SAPPHIRE, SUBTEXT0,
    SURFACE0, SURFACE1, TEXT};
use super::status_bar::draw_empty_network;
use super::{
    duration_color, format_duration, format_size, method_color, protocol_pill, status_color,
    METHOD_W, PROTO_W, SIZE_W, STATUS_W, TIME_W,
};

// Row background colors for error/warning states
const ERROR_ROW_BG: Color = Color::Rgb(50, 30, 35);
const WARNING_ROW_BG: Color = Color::Rgb(50, 45, 30);
const REPLAY_ROW_BG: Color = Color::Rgb(35, 45, 65);
const MOCKED_ROW_BG: Color = Color::Rgb(50, 35, 65);

pub(super) fn draw_table_body(f: &mut Frame, app: &mut App, area: Rect) {
    let height = area.height as usize;

    let filtered_indices: Vec<usize> = app.network.filtered_indices(&app.network_store).to_vec();
    let filtered_count = filtered_indices.len();

    if filtered_count == 0 {
        draw_empty_network(f, app, area);
        return;
    }

    // Scroll / selection logic (unchanged from original)
    app.network.selected = app.network.selected.min(filtered_count.saturating_sub(1));

    if app.network.auto_scroll {
        app.network.selected = filtered_count.saturating_sub(1);
        app.network.scroll_offset = filtered_count.saturating_sub(height);
    } else {
        let max_offset = filtered_count.saturating_sub(height);
        app.network.scroll_offset = app.network.scroll_offset.min(max_offset);

        if app.network.selected < app.network.scroll_offset {
            app.network.scroll_offset = app.network.selected;
        }
        if height > 0 && app.network.selected >= app.network.scroll_offset + height {
            app.network.scroll_offset = app.network.selected.saturating_sub(height - 1);
        }

        if app.network.selected + 1 >= filtered_count {
            app.network.auto_scroll = true;
        }
    }

    let start = app.network.scroll_offset;

    // Build rows
    let rows: Vec<Row> = filtered_indices
        .iter()
        .enumerate()
        .filter_map(|(fi, &store_idx)| {
            let entry = app.network_store.get(store_idx)?;
            let is_selected = fi == app.network.selected;

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

            // Col 0: cursor (width 1)
            let cursor_cell = Cell::from(if is_selected {
                Span::styled("\u{258e}", Style::default().fg(BLUE))
            } else {
                Span::raw(" ")
            });

            // Col 1: protocol pill (width PROTO_W)
            let pill = protocol_pill(entry.protocol);
            // Pad pill to PROTO_W so the column is always the same width
            let pill_w = pill.content.len(); // ASCII labels only, safe
            let pill_pad = PROTO_W.saturating_sub(pill_w);
            let proto_cell = Cell::from(Line::from(vec![
                pill,
                Span::styled(" ".repeat(pill_pad), Style::default()),
            ]));

            // Col 2: method (width METHOD_W)
            let method_c = method_color(&entry.method);
            let method_text = match entry.source {
                EntrySource::Replay => {
                    if entry.method.is_empty() { "\u{21bb}-".to_string() }
                    else { format!("\u{21bb}{}", entry.method) }
                }
                EntrySource::Mocked => {
                    if entry.method.is_empty() { "\u{25c6}-".to_string() }
                    else { format!("\u{25c6}{}", entry.method) }
                }
                EntrySource::App => {
                    if entry.method.is_empty() { "-".to_string() }
                    else { entry.method.clone() }
                }
            };
            let method_cell = Cell::from(Span::styled(method_text, Style::default().fg(method_c)));

            // Col 3: URL (flex — Constraint::Fill)
            let path_only = entry.path.split('?').next().unwrap_or(&entry.path);
            // Estimate available width for truncation (area.width minus fixed cols + separators)
            // Fixed: 1(cursor) + PROTO_W + 1 + METHOD_W + 1 + STATUS_W + 1 + TIME_W + 1 + SIZE_W
            let fixed = 1 + PROTO_W + 1 + METHOD_W + 1 + STATUS_W + 1 + TIME_W + 1 + SIZE_W;
            let url_max = (area.width as usize).saturating_sub(fixed);
            let url_color = match entry.source {
                EntrySource::Mocked => MAUVE,
                EntrySource::Replay => BLUE,
                EntrySource::App => TEXT,
            };
            // Prepend MOCK/REPLAY tag inside the URL cell
            let url_cell = match entry.source {
                EntrySource::Mocked => {
                    let tag = Span::styled(
                        "MOCK ",
                        Style::default()
                            .fg(MANTLE)
                            .bg(MAUVE)
                            .add_modifier(ratatui::style::Modifier::BOLD),
                    );
                    let tag_w = 5; // "MOCK " = 5 chars
                    let path = safe_truncate(path_only, url_max.saturating_sub(tag_w));
                    Cell::from(Line::from(vec![
                        tag,
                        Span::styled(path, Style::default().fg(url_color)),
                    ]))
                }
                EntrySource::Replay => {
                    let tag = Span::styled(
                        "REPLAY ",
                        Style::default()
                            .fg(MANTLE)
                            .bg(BLUE)
                            .add_modifier(ratatui::style::Modifier::BOLD),
                    );
                    let tag_w = 7; // "REPLAY " = 7 chars
                    let path = safe_truncate(path_only, url_max.saturating_sub(tag_w));
                    Cell::from(Line::from(vec![
                        tag,
                        Span::styled(path, Style::default().fg(url_color)),
                    ]))
                }
                EntrySource::App => {
                    let path = safe_truncate(path_only, url_max);
                    Cell::from(Span::styled(path, Style::default().fg(url_color)))
                }
            };

            // Col 4: status (width STATUS_W)
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
            let status_cell = Cell::from(Span::styled(status_text, Style::default().fg(status_c)));

            // Col 5: duration (width TIME_W)
            let time_text = entry.duration.map(format_duration).unwrap_or_else(|| "-".to_string());
            let time_c = entry.duration.map(duration_color).unwrap_or(OVERLAY0);
            let time_cell = Cell::from(Span::styled(time_text, Style::default().fg(time_c)));

            // Col 6: size (width SIZE_W)
            let size = entry.display_size();
            let size_text = if size > 0 { format_size(size) } else { "-".to_string() };
            let size_cell = Cell::from(Span::styled(size_text, Style::default().fg(SUBTEXT0)));

            Some(
                Row::new([
                    cursor_cell,
                    proto_cell,
                    method_cell,
                    url_cell,
                    status_cell,
                    time_cell,
                    size_cell,
                ])
                .style(Style::default().bg(row_bg)),
            )
        })
        .collect();

    let widths = [
        Constraint::Length(1),                   // cursor
        Constraint::Length(PROTO_W as u16),       // proto pill
        Constraint::Length(METHOD_W as u16),      // method
        Constraint::Fill(1),                      // URL (takes remaining)
        Constraint::Length(STATUS_W as u16),      // status
        Constraint::Length(TIME_W as u16),        // duration
        Constraint::Length(SIZE_W as u16),        // size
    ];

    let mut table_state = TableState::default().with_offset(start);

    f.render_stateful_widget(
        Table::new(rows, widths)
            .block(Block::default().borders(Borders::NONE))
            .style(Style::default().bg(BASE))
            .column_spacing(1),
        area,
        &mut table_state,
    );

    // Scrollbar
    if filtered_count > height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_symbol("\u{2503}")
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
