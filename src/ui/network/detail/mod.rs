//! Network detail panel — Flipper-style collapsible sections with JSON viewer.

mod error;
mod general;
mod http_body;
mod shared;
mod sse;
mod ws;

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
    },
    Frame,
};

use crate::app::App;
use crate::domain::network::Protocol;

use super::super::{wrap_text, GREEN, MANTLE, MAUVE, OVERLAY0, SAPPHIRE, SURFACE0, TEXT};
use super::method_color;

pub(super) const KEY_COLOR: ratatui::style::Color = MAUVE;
pub(super) const STR_COLOR: ratatui::style::Color = GREEN;

pub fn draw_network_detail(f: &mut Frame, app: &mut App, area: Rect) {
    let indices = app.network.filtered_indices(&app.network_store).to_vec();

    let entry = if let Some(&idx) = indices.get(app.network.selected) {
        app.network_store.get(idx).cloned()
    } else {
        None
    };

    let entry = match entry {
        Some(e) => e,
        None => {
            let block = Block::default()
                .title(" Details ")
                .borders(Borders::LEFT)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(SURFACE0))
                .style(Style::default().bg(MANTLE));
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "  Select a request",
                    Style::default().fg(OVERLAY0),
                )))
                .block(block),
                area,
            );
            return;
        }
    };

    let inner_h = area.height.saturating_sub(2) as usize;
    let inner_w = area.width.saturating_sub(3) as usize;

    let mut all_lines: Vec<Line> = Vec::new();
    // Track which line indices are section headers (for click toggling)
    let mut section_line_map: Vec<Option<String>> = Vec::new();
    // Track which line indices map to JSON hot regions (typed)
    let mut json_click_map: Vec<Vec<crate::ui::json_viewer::JsonHotRegion>> = Vec::new();
    // Parallel: section key for each line (for ToggleFold routing in apply)
    let mut json_section_keys: Vec<Option<String>> = Vec::new();
    app.layout.sse_pill_line = None;
    app.layout.ws_pill_line = None;

    // ── Header: method pill + path (wrapped) ──
    let method_c = method_color(&entry.method);
    if !entry.method.is_empty() {
        let method_pill = format!(" {} ", entry.method);
        let path_w = inner_w.saturating_sub(method_pill.len() + 1);
        let path_lines = wrap_text(&entry.path, path_w, 5);
        for (pi, pl) in path_lines.iter().enumerate() {
            if pi == 0 {
                all_lines.push(Line::from(vec![
                    Span::styled(
                        method_pill.clone(),
                        Style::default()
                            .fg(MANTLE)
                            .bg(method_c)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!(" {}", pl), Style::default().fg(TEXT)),
                ]));
            } else {
                let indent = " ".repeat(method_pill.len() + 1);
                all_lines.push(Line::from(Span::styled(
                    format!("{}{}", indent, pl),
                    Style::default().fg(TEXT),
                )));
            }
            section_line_map.push(None);
            json_click_map.push(Vec::new());
            json_section_keys.push(None);
        }
    }
    // Divider line with [Mock] button (VM Service only)
    {
        let mock_btn = if app.has_connected_client() && entry.protocol == Protocol::Http {
            " [Mock] "
        } else {
            ""
        };
        let divider_w = inner_w.saturating_sub(mock_btn.len());
        let mut divider_spans = vec![Span::styled(
            "\u{2500}".repeat(divider_w),
            Style::default().fg(SURFACE0),
        )];
        if !mock_btn.is_empty() {
            divider_spans.push(Span::styled(
                mock_btn,
                Style::default()
                    .fg(MANTLE)
                    .bg(MAUVE)
                    .add_modifier(Modifier::BOLD),
            ));
            // Register click region — will be offset by area.x+1 and the content_y + line_idx
            // We store the raw line index; the click handler translates using detail_content_y + scroll
            let line_idx = all_lines.len();
            let btn_x_start = area.x + 1 + divider_w as u16;
            let btn_x_end = btn_x_start + mock_btn.len() as u16;
            let btn_y = area.y + 1 + line_idx as u16; // approximate — before scroll
            app.layout.detail_mock_btn = Some((btn_y, btn_x_start, btn_x_end));
        } else {
            app.layout.detail_mock_btn = None;
        }
        all_lines.push(Line::from(divider_spans));
        section_line_map.push(None);
        json_click_map.push(Vec::new());
        json_section_keys.push(None);
    }

    // ── General ──
    general::render_general(
        &mut all_lines,
        &mut section_line_map,
        &mut json_click_map,
        &mut json_section_keys,
        &entry,
        &app.network.collapsed_sections,
        inner_w,
    );

    // ── Query Parameters ──
    http_body::render_query_params(
        &mut all_lines,
        &mut section_line_map,
        &mut json_click_map,
        &mut json_section_keys,
        &entry,
        &app.network.collapsed_sections,
        inner_w,
    );

    // ── Request Headers ──
    http_body::render_headers(
        &mut all_lines,
        &mut section_line_map,
        &mut json_click_map,
        &mut json_section_keys,
        entry.request_headers.as_ref(),
        "Request Headers",
        "req_headers",
        &app.network.collapsed_sections,
        &mut app.network.json_viewer_states,
        inner_w,
    );

    // ── Request Body ──
    http_body::render_body(
        &mut all_lines,
        &mut section_line_map,
        &mut json_click_map,
        &mut json_section_keys,
        entry.request_body.as_ref(),
        "Request Body",
        "req_body",
        &app.network.collapsed_sections,
        &mut app.network.json_viewer_states,
        inner_w,
    );

    // ── Response Headers ──
    http_body::render_headers(
        &mut all_lines,
        &mut section_line_map,
        &mut json_click_map,
        &mut json_section_keys,
        entry.response_headers.as_ref(),
        "Response Headers",
        "res_headers",
        &app.network.collapsed_sections,
        &mut app.network.json_viewer_states,
        inner_w,
    );

    // ── Response Body ──
    http_body::render_body(
        &mut all_lines,
        &mut section_line_map,
        &mut json_click_map,
        &mut json_section_keys,
        entry.response_body.as_ref(),
        "Response Body",
        "res_body",
        &app.network.collapsed_sections,
        &mut app.network.json_viewer_states,
        inner_w,
    );

    // ── SSE Stream Events ──
    if entry.protocol == Protocol::Sse && !entry.sse_chunks.is_empty() {
        sse::render_sse(
            &mut all_lines,
            &mut section_line_map,
            &mut json_click_map,
            &mut json_section_keys,
            app,
            &entry,
            inner_w,
        );
    }

    // ── WebSocket Messages ──
    if entry.protocol == Protocol::Ws && !entry.ws_messages.is_empty() {
        ws::render_ws(
            &mut all_lines,
            &mut section_line_map,
            &mut json_click_map,
            &mut json_section_keys,
            app,
            &entry,
            inner_w,
        );
    }

    // ── Error ──
    error::render_error(
        &mut all_lines,
        &mut section_line_map,
        &mut json_click_map,
        &mut json_section_keys,
        entry.error.as_ref(),
        &app.network.collapsed_sections,
        inner_w,
    );

    // Store maps for click handling
    app.network.detail_section_map = section_line_map;
    app.network.detail_json_click_map = json_click_map;
    app.network.detail_json_section_keys = json_section_keys;

    let total_lines = all_lines.len();

    // Apply scroll
    let scroll = app.network.detail_scroll;
    let mut visible_lines: Vec<Line> = all_lines.into_iter().skip(scroll).take(inner_h).collect();

    // Populate app.detail.viewer_click_map from the visible slice so that
    // detail_cursor_down/up clamp correctly and the Enter/o handlers in
    // keys.rs can read a single authoritative map for both tabs.
    app.detail.viewer_click_map = app
        .network
        .detail_json_click_map
        .iter()
        .skip(scroll)
        .take(inner_h)
        .cloned()
        .collect();

    // Highlight the cursor row with SURFACE0 background.
    // viewer_cursor is relative to the visible window (same indexing as viewer_click_map).
    if let Some(cursor) = app.detail.viewer_cursor {
        if let Some(line) = visible_lines.get_mut(cursor) {
            let highlighted_spans: Vec<ratatui::text::Span<'static>> = line
                .spans
                .iter()
                .map(|s| {
                    let style = s.style.patch(Style::default().bg(SURFACE0));
                    ratatui::text::Span::styled(s.content.clone(), style)
                })
                .collect();
            *line = Line::from(highlighted_spans);
        }
    }

    let block = Block::default()
        .title(" Details ")
        .title_style(Style::default().fg(SAPPHIRE).add_modifier(Modifier::BOLD))
        .borders(Borders::LEFT)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(SURFACE0))
        .style(Style::default().bg(MANTLE));

    // Record the inner area Y for click handling
    let inner = block.inner(area);
    app.layout.net_detail_content_y = inner.y;

    f.render_widget(Paragraph::new(visible_lines).block(block), area);

    // Scrollbar
    if total_lines > inner_h {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_symbol("\u{2503}") // ┃
            .track_symbol(Some(" "))
            .thumb_style(Style::default().fg(SAPPHIRE))
            .track_style(Style::default().fg(SURFACE0).bg(MANTLE))
            .begin_symbol(None)
            .end_symbol(None);
        let max_scroll = total_lines.saturating_sub(inner_h);
        let mut state = ScrollbarState::new(max_scroll).position(scroll.min(max_scroll));
        f.render_stateful_widget(scrollbar, area, &mut state);
    }
}
