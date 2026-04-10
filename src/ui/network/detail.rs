//! Network detail panel — Flipper-style collapsible sections with JSON viewer.

use std::collections::HashMap;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::domain::network::{NetworkStatus, Protocol, WsDirection};
use crate::ui::json_viewer::{self, JsonViewerState};

use super::super::{
    MANTLE, SURFACE0, OVERLAY0, TEXT, SUBTEXT0,
    BLUE, GREEN, RED, MAUVE, SAPPHIRE,
    wrap_text,
};
use super::{format_duration, format_size, method_color, status_color};

const KEY_COLOR: ratatui::style::Color = MAUVE;
const STR_COLOR: ratatui::style::Color = GREEN;

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
                Paragraph::new(Line::from(Span::styled("  Select a request", Style::default().fg(OVERLAY0)))).block(block),
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
    // Track which line indices map to JSON bracket clicks
    let mut json_click_map: Vec<Option<(String, usize)>> = Vec::new();

    // ── Header: method pill + path (wrapped) ──
    let method_c = method_color(&entry.method);
    if !entry.method.is_empty() {
        all_lines.push(Line::from(vec![
            Span::styled(format!(" {} ", entry.method), Style::default().fg(MANTLE).bg(method_c).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {}", entry.path), Style::default().fg(TEXT)),
        ]));
        section_line_map.push(None);
        json_click_map.push(None);
    }
    all_lines.push(Line::from(Span::styled("\u{2500}".repeat(inner_w), Style::default().fg(SURFACE0))));
    section_line_map.push(None);
    json_click_map.push(None);

    // ── General ──
    let sec = "General";
    let is_collapsed = app.network.collapsed_sections.contains(sec);
    push_section_header(&mut all_lines, &mut section_line_map, &mut json_click_map, sec, is_collapsed);
    if !is_collapsed {
        // URL — full, wrapped
        push_kv_wrapped(&mut all_lines, &mut section_line_map, &mut json_click_map, "URL", &entry.url, inner_w);
        if !entry.method.is_empty() {
            push_kv_single(&mut all_lines, &mut section_line_map, &mut json_click_map, "Method", &entry.method);
        }
        let status_str = match entry.status {
            NetworkStatus::Pending => "Pending".to_string(),
            NetworkStatus::Active => match entry.protocol {
                Protocol::Sse => format!("Streaming ({} chunks)", entry.sse_chunks.len()),
                Protocol::Ws => format!("Connected ({} msgs)", entry.ws_messages.len()),
                _ => "Active".to_string(),
            },
            NetworkStatus::Completed => entry.http_status.map_or("OK".to_string(), |s| format!("{} {}", s, http_status_text(s))),
            NetworkStatus::Failed => entry.error.clone().unwrap_or_else(|| "Failed".to_string()),
        };
        let sc = status_color(entry.status, entry.http_status);
        all_lines.push(Line::from(vec![
            Span::styled("   Status: ", Style::default().fg(KEY_COLOR)),
            Span::styled(status_str, Style::default().fg(sc)),
        ]));
        section_line_map.push(None);
        json_click_map.push(None);
        if let Some(dur) = entry.duration {
            push_kv_single(&mut all_lines, &mut section_line_map, &mut json_click_map, "Duration", &format_duration(dur));
        }
        let size = entry.display_size();
        if size > 0 {
            push_kv_single(&mut all_lines, &mut section_line_map, &mut json_click_map, "Size", &format_size(size));
        }
        all_lines.push(Line::raw(""));
        section_line_map.push(None);
        json_click_map.push(None);
    }

    // ── Query Parameters ──
    let query_params = parse_query_params(&entry.url);
    if !query_params.is_empty() {
        let sec = "Query Parameters";
        let is_collapsed = app.network.collapsed_sections.contains(sec);
        push_section_header(&mut all_lines, &mut section_line_map, &mut json_click_map, sec, is_collapsed);
        if !is_collapsed {
            for (key, value) in &query_params {
                push_kv_wrapped(&mut all_lines, &mut section_line_map, &mut json_click_map, key, value, inner_w);
            }
            all_lines.push(Line::raw(""));
            section_line_map.push(None);
            json_click_map.push(None);
        }
    }

    // ── Request Headers ──
    if entry.request_headers.is_some() {
        let sec = "Request Headers";
        let is_collapsed = app.network.collapsed_sections.contains(sec);
        push_section_header(&mut all_lines, &mut section_line_map, &mut json_click_map, sec, is_collapsed);
        if !is_collapsed {
            if let Some(ref headers) = entry.request_headers {
                render_json_section(
                    &mut all_lines,
                    &mut section_line_map,
                    &mut json_click_map,
                    headers,
                    "req_headers",
                    &mut app.network.json_viewer_states,
                    inner_w,
                );
            }
            all_lines.push(Line::raw(""));
            section_line_map.push(None);
            json_click_map.push(None);
        }
    }

    // ── Request Body ──
    if entry.request_body.as_ref().map_or(false, |b| !b.is_empty()) {
        let sec = "Request Body";
        let is_collapsed = app.network.collapsed_sections.contains(sec);
        push_section_header(&mut all_lines, &mut section_line_map, &mut json_click_map, sec, is_collapsed);
        if !is_collapsed {
            if let Some(ref body) = entry.request_body {
                render_json_section(
                    &mut all_lines,
                    &mut section_line_map,
                    &mut json_click_map,
                    body,
                    "req_body",
                    &mut app.network.json_viewer_states,
                    inner_w,
                );
            }
            all_lines.push(Line::raw(""));
            section_line_map.push(None);
            json_click_map.push(None);
        }
    }

    // ── Response Headers ──
    if entry.response_headers.is_some() {
        let sec = "Response Headers";
        let is_collapsed = app.network.collapsed_sections.contains(sec);
        push_section_header(&mut all_lines, &mut section_line_map, &mut json_click_map, sec, is_collapsed);
        if !is_collapsed {
            if let Some(ref headers) = entry.response_headers {
                render_json_section(
                    &mut all_lines,
                    &mut section_line_map,
                    &mut json_click_map,
                    headers,
                    "res_headers",
                    &mut app.network.json_viewer_states,
                    inner_w,
                );
            }
            all_lines.push(Line::raw(""));
            section_line_map.push(None);
            json_click_map.push(None);
        }
    }

    // ── Response Body ──
    if entry.response_body.as_ref().map_or(false, |b| !b.is_empty()) {
        let sec = "Response Body";
        let is_collapsed = app.network.collapsed_sections.contains(sec);
        push_section_header(&mut all_lines, &mut section_line_map, &mut json_click_map, sec, is_collapsed);
        if !is_collapsed {
            if let Some(ref body) = entry.response_body {
                render_json_section(
                    &mut all_lines,
                    &mut section_line_map,
                    &mut json_click_map,
                    body,
                    "res_body",
                    &mut app.network.json_viewer_states,
                    inner_w,
                );
            }
            all_lines.push(Line::raw(""));
            section_line_map.push(None);
            json_click_map.push(None);
        }
    }

    // ── SSE Stream Events ──
    if entry.protocol == Protocol::Sse && !entry.sse_chunks.is_empty() {
        let sec_name = format!("SSE Events ({})", entry.sse_chunks.len());
        let sec_key = "SSE Events";
        let is_collapsed = app.network.collapsed_sections.contains(sec_key);
        push_section_header(&mut all_lines, &mut section_line_map, &mut json_click_map, &sec_name, is_collapsed);
        // Use sec_key for the map entry (strip count)
        if let Some(last) = section_line_map.last_mut() {
            *last = Some(sec_key.to_string());
        }
        if !is_collapsed {
            // Pre-collapse SSE chunks by default on FIRST render only.
            // Use a sentinel key to track whether we've initialized.
            let init_key = "_sse_init";
            if !app.network.collapsed_sections.contains(init_key) {
                app.network.collapsed_sections.insert(init_key.to_string());
                for i in 0..entry.sse_chunks.len() {
                    app.network.collapsed_sections.insert(format!("SSE#{}", i));
                }
            }

            for (i, chunk) in entry.sse_chunks.iter().enumerate() {
                let chunk_key = format!("SSE#{}", i);
                let chunk_collapsed = app.network.collapsed_sections.contains(&chunk_key);
                let prefix = if chunk_collapsed { "  \u{25b6}" } else { "  \u{25bc}" };
                all_lines.push(Line::from(vec![
                    Span::styled(format!("{} #{} ", prefix, i), Style::default().fg(OVERLAY0)),
                    Span::styled(
                        if chunk_collapsed {
                            let preview = if chunk.data.len() > 40 { format!("{}...", &chunk.data[..40]) } else { chunk.data.clone() };
                            preview
                        } else {
                            String::new()
                        },
                        Style::default().fg(SUBTEXT0),
                    ),
                ]));
                section_line_map.push(Some(chunk_key.clone()));
                json_click_map.push(None);
                if !chunk_collapsed {
                    render_json_section(
                        &mut all_lines,
                        &mut section_line_map,
                        &mut json_click_map,
                        &chunk.data,
                        &format!("sse_{}", i),
                        &mut app.network.json_viewer_states,
                        inner_w,
                    );
                }
            }
            all_lines.push(Line::raw(""));
            section_line_map.push(None);
            json_click_map.push(None);
        }
    }

    // ── WebSocket Messages ──
    if entry.protocol == Protocol::Ws && !entry.ws_messages.is_empty() {
        let sent = entry.ws_messages.iter().filter(|m| m.direction == WsDirection::Send).count();
        let recv = entry.ws_messages.iter().filter(|m| m.direction == WsDirection::Recv).count();
        let sec_name = format!("Messages ({} sent / {} recv)", sent, recv);
        let sec_key = "WS Messages";
        let is_collapsed = app.network.collapsed_sections.contains(sec_key);
        push_section_header(&mut all_lines, &mut section_line_map, &mut json_click_map, &sec_name, is_collapsed);
        if let Some(last) = section_line_map.last_mut() {
            *last = Some(sec_key.to_string());
        }
        if !is_collapsed {
            for (i, msg) in entry.ws_messages.iter().enumerate() {
                let (arrow, color) = match msg.direction {
                    WsDirection::Send => ("\u{2192}", GREEN),  // →
                    WsDirection::Recv => ("\u{2190}", BLUE),   // ←
                };
                let msg_key = format!("WS#{}", i);
                let msg_collapsed = app.network.collapsed_sections.contains(&msg_key);
                let prefix = if msg_collapsed { "\u{25b6}" } else { "\u{25bc}" };
                all_lines.push(Line::from(vec![
                    Span::styled(format!("  {} {} ", prefix, arrow), Style::default().fg(color)),
                    Span::styled(
                        if msg_collapsed {
                            let preview = if msg.data.len() > 40 { format!("{}...", &msg.data[..40]) } else { msg.data.clone() };
                            preview
                        } else {
                            format!("({} bytes)", msg.size)
                        },
                        Style::default().fg(SUBTEXT0),
                    ),
                ]));
                section_line_map.push(Some(msg_key.clone()));
                json_click_map.push(None);
                if !msg_collapsed {
                    render_json_section(
                        &mut all_lines,
                        &mut section_line_map,
                        &mut json_click_map,
                        &msg.data,
                        &format!("ws_{}", i),
                        &mut app.network.json_viewer_states,
                        inner_w,
                    );
                }
            }
            all_lines.push(Line::raw(""));
            section_line_map.push(None);
            json_click_map.push(None);
        }
    }

    // ── Error ──
    if let Some(ref error) = entry.error {
        let sec = "Error";
        let is_collapsed = app.network.collapsed_sections.contains(sec);
        push_section_header(&mut all_lines, &mut section_line_map, &mut json_click_map, sec, is_collapsed);
        if !is_collapsed {
            let wrapped = wrap_text(error, inner_w.saturating_sub(3), 20);
            for wl in &wrapped {
                all_lines.push(Line::from(Span::styled(format!("   {}", wl), Style::default().fg(RED))));
                section_line_map.push(None);
                json_click_map.push(None);
            }
        }
    }

    // Store maps for click handling
    app.network.detail_section_map = section_line_map;
    app.network.detail_json_click_map = json_click_map;

    let total_lines = all_lines.len();

    // Apply scroll
    let scroll = app.network.detail_scroll;
    let visible_lines: Vec<Line> = all_lines
        .into_iter()
        .skip(scroll)
        .take(inner_h)
        .collect();

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

    f.render_widget(
        Paragraph::new(visible_lines)
            .block(block),
        area,
    );

    // Scrollbar
    if total_lines > inner_h {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_symbol("\u{2503}")  // ┃
            .track_symbol(Some(" "))
            .thumb_style(Style::default().fg(SAPPHIRE))
            .track_style(Style::default().fg(SURFACE0).bg(MANTLE))
            .begin_symbol(None)
            .end_symbol(None);
        let max_scroll = total_lines.saturating_sub(inner_h);
        let mut state = ScrollbarState::new(max_scroll)
            .position(scroll.min(max_scroll));
        f.render_stateful_widget(scrollbar, area, &mut state);
    }
}

// ══════════════════════════════════════
//  Section helpers
// ══════════════════════════════════════

fn push_section_header(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Option<(String, usize)>>,
    title: &str,
    collapsed: bool,
) {
    let icon = if collapsed { "\u{25b6}" } else { "\u{25bc}" };  // ▶ ▼
    lines.push(Line::from(Span::styled(
        format!(" {} {}", icon, title),
        Style::default().fg(SAPPHIRE).add_modifier(Modifier::BOLD),
    )));
    section_map.push(Some(title.to_string()));
    json_click_map.push(None);
}

fn push_kv_single(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Option<(String, usize)>>,
    key: &str,
    value: &str,
) {
    lines.push(Line::from(vec![
        Span::styled(format!("   {}: ", key), Style::default().fg(KEY_COLOR)),
        Span::styled(value.to_string(), Style::default().fg(STR_COLOR)),
    ]));
    section_map.push(None);
    json_click_map.push(None);
}

fn push_kv_wrapped(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Option<(String, usize)>>,
    key: &str,
    value: &str,
    max_w: usize,
) {
    let prefix = format!("   {}: ", key);
    let first_line_w = max_w.saturating_sub(prefix.len());
    let cont_indent = "   ";
    let _cont_w = max_w.saturating_sub(cont_indent.len());

    if value.width() <= first_line_w {
        lines.push(Line::from(vec![
            Span::styled(prefix, Style::default().fg(KEY_COLOR)),
            Span::styled(value.to_string(), Style::default().fg(STR_COLOR)),
        ]));
        section_map.push(None);
        json_click_map.push(None);
    } else {
        // First line with key
        let wrapped = wrap_text(value, first_line_w, 20);
        if let Some(first) = wrapped.first() {
            lines.push(Line::from(vec![
                Span::styled(prefix, Style::default().fg(KEY_COLOR)),
                Span::styled(first.to_string(), Style::default().fg(STR_COLOR)),
            ]));
            section_map.push(None);
            json_click_map.push(None);
        }
        // Continuation lines
        for wl in wrapped.iter().skip(1) {
            lines.push(Line::from(Span::styled(
                format!("{}{}", cont_indent, wl),
                Style::default().fg(STR_COLOR),
            )));
            section_map.push(None);
            json_click_map.push(None);
        }
    }
}

// ══════════════════════════════════════
//  JSON rendering using json_viewer
// ══════════════════════════════════════

fn render_json_section(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Option<(String, usize)>>,
    json_text: &str,
    section_key: &str,
    viewer_states: &mut HashMap<String, JsonViewerState>,
    max_w: usize,
) {
    if serde_json::from_str::<serde_json::Value>(json_text).is_ok() {
        let fmt_lines = json_viewer::bracket_format(json_text);
        let state = viewer_states.entry(section_key.to_string())
            .or_insert_with(|| json_viewer::init_state(&fmt_lines, 1));
        let rendered = json_viewer::render_json(&fmt_lines, state, 0, usize::MAX);
        let _base = lines.len();
        for (i, line) in rendered.into_iter().enumerate() {
            // Add indent prefix
            let mut spans = vec![Span::raw("   ")];
            spans.extend(line.spans);
            lines.push(Line::from(spans));
            section_map.push(None);
            // Map to json click target
            let source = state.row_to_source.get(i).copied();
            if let Some(sl) = source {
                json_click_map.push(Some((section_key.to_string(), sl)));
            } else {
                json_click_map.push(None);
            }
        }
    } else {
        // Fallback: wrapped text
        for wl in wrap_text(json_text, max_w.saturating_sub(3), 100) {
            lines.push(Line::from(Span::styled(format!("   {}", wl), Style::default().fg(SUBTEXT0))));
            section_map.push(None);
            json_click_map.push(None);
        }
    }
}

// ══════════════════════════════════════
//  Query parameters parsing
// ══════════════════════════════════════

fn parse_query_params(url: &str) -> Vec<(String, String)> {
    let mut params = Vec::new();
    if let Some(query_start) = url.find('?') {
        let query = &url[query_start + 1..];
        // Strip fragment if present
        let query = query.split('#').next().unwrap_or(query);
        for pair in query.split('&') {
            if pair.is_empty() { continue; }
            if let Some(eq_pos) = pair.find('=') {
                let key = url_decode(&pair[..eq_pos]);
                let value = url_decode(&pair[eq_pos + 1..]);
                params.push((key, value));
            } else {
                params.push((url_decode(pair), String::new()));
            }
        }
    }
    params
}

fn url_decode(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if hex.len() == 2 {
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    result.push(byte as char);
                    continue;
                }
            }
            result.push('%');
            result.push_str(&hex);
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    result
}

fn http_status_text(code: u16) -> &'static str {
    match code {
        200 => "OK", 201 => "Created", 204 => "No Content",
        301 => "Moved Permanently", 302 => "Found", 304 => "Not Modified",
        400 => "Bad Request", 401 => "Unauthorized", 403 => "Forbidden",
        404 => "Not Found", 408 => "Request Timeout", 429 => "Too Many Requests",
        500 => "Internal Server Error", 502 => "Bad Gateway",
        503 => "Service Unavailable", 504 => "Gateway Timeout",
        _ => "",
    }
}
