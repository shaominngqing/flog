//! Network detail panel — shows request/response headers, body, SSE chunks, WS messages.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::app::App;
use crate::domain::network::{NetworkStatus, Protocol, WsDirection};

// Import shared palette from parent
use super::super::{
    MANTLE, SURFACE0, OVERLAY0, TEXT, SUBTEXT0,
    BLUE, GREEN, RED, MAUVE, SAPPHIRE,
};
use super::{format_duration, format_size, method_color, status_color};

const STR_COLOR: Color = GREEN;
const KEY_COLOR: Color = MAUVE;

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

    // Header: method pill + path
    let method_c = method_color(&entry.method);
    let mut header_spans: Vec<Span> = Vec::new();
    if !entry.method.is_empty() {
        header_spans.push(Span::styled(
            format!(" {} ", entry.method),
            Style::default().fg(MANTLE).bg(method_c).add_modifier(Modifier::BOLD),
        ));
        header_spans.push(Span::raw(" "));
    }
    header_spans.push(Span::styled(
        truncate_path(&entry.path, inner_w.saturating_sub(entry.method.len() + 3)),
        Style::default().fg(TEXT),
    ));
    all_lines.push(Line::from(header_spans));
    all_lines.push(Line::from(Span::styled("-".repeat(inner_w), Style::default().fg(SURFACE0))));

    // General section
    all_lines.push(section_header("General"));
    all_lines.push(kv_line("URL", &entry.url, inner_w));
    if !entry.method.is_empty() {
        all_lines.push(kv_line("Method", &entry.method, inner_w));
    }

    let status_str = match entry.status {
        NetworkStatus::Pending => "Pending".to_string(),
        NetworkStatus::Active => "Active".to_string(),
        NetworkStatus::Completed => {
            if let Some(code) = entry.http_status {
                format!("{} {}", code, http_status_text(code))
            } else {
                "Completed".to_string()
            }
        }
        NetworkStatus::Failed => "Failed".to_string(),
    };
    let status_c = status_color(entry.status, entry.http_status);
    all_lines.push(Line::from(vec![
        Span::styled("  Status: ", Style::default().fg(KEY_COLOR)),
        Span::styled(status_str, Style::default().fg(status_c)),
    ]));

    if let Some(dur) = entry.duration {
        all_lines.push(kv_line("Duration", &format_duration(dur), inner_w));
    }
    let size = entry.display_size();
    if size > 0 {
        all_lines.push(kv_line("Size", &format_size(size), inner_w));
    }

    // Request Headers
    if let Some(ref headers) = entry.request_headers {
        all_lines.push(Line::raw(""));
        all_lines.push(section_header("Request Headers"));
        all_lines.extend(render_json_compact(headers, inner_w));
    }

    // Request Body
    if let Some(ref body) = entry.request_body {
        if !body.is_empty() {
            all_lines.push(Line::raw(""));
            all_lines.push(section_header("Request Body"));
            all_lines.extend(render_body_lines(body, inner_w));
        }
    }

    // Response Headers
    if let Some(ref headers) = entry.response_headers {
        all_lines.push(Line::raw(""));
        all_lines.push(section_header("Response Headers"));
        all_lines.extend(render_json_compact(headers, inner_w));
    }

    // Response Body
    if let Some(ref body) = entry.response_body {
        if !body.is_empty() {
            all_lines.push(Line::raw(""));
            all_lines.push(section_header("Response Body"));
            all_lines.extend(render_body_lines(body, inner_w));
        }
    }

    // SSE Stream Events
    if entry.protocol == Protocol::Sse && !entry.sse_chunks.is_empty() {
        all_lines.push(Line::raw(""));
        all_lines.push(section_header(&format!("SSE Events ({})", entry.sse_chunks.len())));

        // Show last 20 chunks
        let start = entry.sse_chunks.len().saturating_sub(20);
        for (i, chunk) in entry.sse_chunks.iter().skip(start).enumerate() {
            let idx = start + i;
            all_lines.push(Line::from(vec![
                Span::styled(format!("  #{} ", idx), Style::default().fg(OVERLAY0)),
                Span::styled(
                    truncate_str(&chunk.data, inner_w.saturating_sub(8)),
                    Style::default().fg(SUBTEXT0),
                ),
            ]));
        }
    }

    // WebSocket Messages
    if entry.protocol == Protocol::Ws && !entry.ws_messages.is_empty() {
        all_lines.push(Line::raw(""));
        all_lines.push(section_header(&format!("WebSocket Messages ({})", entry.ws_messages.len())));

        // Show last 20 messages
        let start = entry.ws_messages.len().saturating_sub(20);
        for msg in entry.ws_messages.iter().skip(start) {
            let (arrow, color) = match msg.direction {
                WsDirection::Send => ("\u{2192}", GREEN),  // ->
                WsDirection::Recv => ("\u{2190}", BLUE),   // <-
            };
            all_lines.push(Line::from(vec![
                Span::styled(format!("  {} ", arrow), Style::default().fg(color)),
                Span::styled(
                    truncate_str(&msg.data, inner_w.saturating_sub(6)),
                    Style::default().fg(SUBTEXT0),
                ),
            ]));
        }
    }

    // Error section
    if let Some(ref error) = entry.error {
        all_lines.push(Line::raw(""));
        all_lines.push(Line::from(Span::styled(
            "  Error",
            Style::default().fg(RED).add_modifier(Modifier::BOLD),
        )));
        for line in error.lines() {
            all_lines.push(Line::from(Span::styled(
                format!("  {}", truncate_str(line, inner_w.saturating_sub(2))),
                Style::default().fg(RED),
            )));
        }
    }

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

    f.render_widget(
        Paragraph::new(visible_lines)
            .block(block)
            .wrap(ratatui::widgets::Wrap { trim: false }),
        area,
    );
}

fn section_header(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {}", title),
        Style::default().fg(SAPPHIRE).add_modifier(Modifier::BOLD),
    ))
}

fn kv_line(key: &str, value: &str, max_w: usize) -> Line<'static> {
    let key_w = key.len() + 4; // "  key: "
    let val_w = max_w.saturating_sub(key_w);
    Line::from(vec![
        Span::styled(format!("  {}: ", key), Style::default().fg(KEY_COLOR)),
        Span::styled(truncate_str(value, val_w), Style::default().fg(STR_COLOR)),
    ])
}

fn render_json_compact(json_str: &str, max_w: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // Try to parse as JSON object
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(json_str) {
        if let Some(obj) = value.as_object() {
            for (key, val) in obj {
                let val_str = match val {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let key_w = key.len() + 4;
                let val_w = max_w.saturating_sub(key_w);
                lines.push(Line::from(vec![
                    Span::styled(format!("  {}: ", key), Style::default().fg(KEY_COLOR)),
                    Span::styled(truncate_str(&val_str, val_w), Style::default().fg(STR_COLOR)),
                ]));
            }
            return lines;
        }
    }

    // Fallback: show raw text
    for line in json_str.lines().take(20) {
        lines.push(Line::from(Span::styled(
            format!("  {}", truncate_str(line, max_w.saturating_sub(2))),
            Style::default().fg(SUBTEXT0),
        )));
    }
    lines
}

fn render_body_lines(body: &str, max_w: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // Try JSON pretty-print
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        if let Ok(pretty) = serde_json::to_string_pretty(&value) {
            for line in pretty.lines().take(50) {
                lines.push(colorize_json_line(line, max_w));
            }
            if pretty.lines().count() > 50 {
                lines.push(Line::from(Span::styled(
                    "  ... (truncated)",
                    Style::default().fg(OVERLAY0),
                )));
            }
            return lines;
        }
    }

    // Fallback: raw text
    for line in body.lines().take(50) {
        lines.push(Line::from(Span::styled(
            format!("  {}", truncate_str(line, max_w.saturating_sub(2))),
            Style::default().fg(SUBTEXT0),
        )));
    }
    if body.lines().count() > 50 {
        lines.push(Line::from(Span::styled(
            "  ... (truncated)",
            Style::default().fg(OVERLAY0),
        )));
    }
    lines
}

fn colorize_json_line(line: &str, _max_w: usize) -> Line<'static> {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    let mut spans: Vec<Span> = Vec::new();

    // Add indent
    spans.push(Span::raw("  "));
    if indent > 0 {
        spans.push(Span::raw(" ".repeat(indent)));
    }

    // Simple colorization
    if trimmed.starts_with('"') && trimmed.contains(':') {
        // Key-value pair
        if let Some(colon_pos) = trimmed.find(':') {
            let key = &trimmed[..colon_pos];
            let rest = &trimmed[colon_pos..];
            spans.push(Span::styled(key.to_string(), Style::default().fg(KEY_COLOR)));
            spans.push(Span::styled(rest.to_string(), Style::default().fg(STR_COLOR)));
        } else {
            spans.push(Span::styled(trimmed.to_string(), Style::default().fg(SUBTEXT0)));
        }
    } else if trimmed == "{" || trimmed == "}" || trimmed == "[" || trimmed == "]"
        || trimmed == "{," || trimmed == "}," || trimmed == "[," || trimmed == "]," {
        spans.push(Span::styled(trimmed.to_string(), Style::default().fg(OVERLAY0)));
    } else {
        spans.push(Span::styled(trimmed.to_string(), Style::default().fg(STR_COLOR)));
    }

    Line::from(spans)
}

fn truncate_str(s: &str, max_w: usize) -> String {
    if s.len() <= max_w {
        s.to_string()
    } else if max_w > 3 {
        format!("{}...", &s[..max_w - 3])
    } else {
        s[..max_w].to_string()
    }
}

fn truncate_path(path: &str, max_w: usize) -> String {
    if path.len() <= max_w {
        path.to_string()
    } else if max_w > 3 {
        format!("{}...", &path[..max_w - 3])
    } else {
        path[..max_w].to_string()
    }
}

fn http_status_text(code: u16) -> &'static str {
    match code {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "",
    }
}
