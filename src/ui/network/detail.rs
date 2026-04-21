//! Network detail panel — Flipper-style collapsible sections with JSON viewer.

use std::collections::HashMap;

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
    },
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::domain::network::{NetworkStatus, Protocol, WsDirection};
use crate::domain::sse_merge;
use crate::domain::ws_chat;
use crate::ui::json_viewer::{self, JsonViewerState};

use super::super::{
    wrap_text, BLUE, GREEN, MANTLE, MAUVE, OVERLAY0, RED, SAPPHIRE, SUBTEXT0, SURFACE0, SURFACE1,
    TEXT,
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
    // Track which line indices map to JSON bracket clicks
    let mut json_click_map: Vec<Option<(String, u32)>> = Vec::new();
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
            json_click_map.push(None);
        }
    }
    // Divider line with [Mock] button (VM Service only)
    {
        let mock_btn = if app.has_connected_client() && entry.protocol == Protocol::Http { " [Mock] " } else { "" };
        let divider_w = inner_w.saturating_sub(mock_btn.len());
        let mut divider_spans = vec![
            Span::styled(
                "\u{2500}".repeat(divider_w),
                Style::default().fg(SURFACE0),
            ),
        ];
        if !mock_btn.is_empty() {
            divider_spans.push(Span::styled(
                mock_btn,
                Style::default().fg(MANTLE).bg(MAUVE).add_modifier(Modifier::BOLD),
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
        json_click_map.push(None);
    }

    // ── General ──
    let sec = "General";
    let is_collapsed = app.network.collapsed_sections.contains(sec);
    push_section_header(
        &mut all_lines,
        &mut section_line_map,
        &mut json_click_map,
        sec,
        is_collapsed,
    );
    if !is_collapsed {
        // URL — full, wrapped
        push_kv_wrapped(
            &mut all_lines,
            &mut section_line_map,
            &mut json_click_map,
            "URL",
            &entry.url,
            inner_w,
        );
        if !entry.method.is_empty() {
            push_kv_single(
                &mut all_lines,
                &mut section_line_map,
                &mut json_click_map,
                "Method",
                &entry.method,
            );
        }
        let status_str = match entry.status {
            NetworkStatus::Pending => "Pending".to_string(),
            NetworkStatus::Active => match entry.protocol {
                Protocol::Sse => format!("Streaming ({} chunks)", entry.sse_chunks.len()),
                Protocol::Ws => format!("Connected ({} msgs)", entry.ws_messages.len()),
                _ => "Active".to_string(),
            },
            NetworkStatus::Completed => entry.http_status.map_or("OK".to_string(), |s| {
                format!("{} {}", s, http_status_text(s))
            }),
            NetworkStatus::Failed => entry.error.clone().unwrap_or_else(|| "Failed".to_string()),
        };
        let sc = status_color(entry.status, entry.http_status);
        all_lines.push(Line::from(vec![
            Span::styled("   Status: ", Style::default().fg(KEY_COLOR)),
            Span::styled(status_str, Style::default().fg(sc)),
        ]));
        section_line_map.push(None);
        json_click_map.push(None);
        if !entry.timestamp.is_empty() {
            push_kv_single(
                &mut all_lines,
                &mut section_line_map,
                &mut json_click_map,
                "Time",
                &entry.timestamp,
            );
        }
        if let Some(dur) = entry.duration {
            push_kv_single(
                &mut all_lines,
                &mut section_line_map,
                &mut json_click_map,
                "Duration",
                &format_duration(dur),
            );
        }
        let size = entry.display_size();
        if size > 0 {
            push_kv_single(
                &mut all_lines,
                &mut section_line_map,
                &mut json_click_map,
                "Size",
                &format_size(size),
            );
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
        push_section_header(
            &mut all_lines,
            &mut section_line_map,
            &mut json_click_map,
            sec,
            is_collapsed,
        );
        if !is_collapsed {
            for (key, value) in &query_params {
                push_kv_wrapped(
                    &mut all_lines,
                    &mut section_line_map,
                    &mut json_click_map,
                    key,
                    value,
                    inner_w,
                );
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
        push_section_header(
            &mut all_lines,
            &mut section_line_map,
            &mut json_click_map,
            sec,
            is_collapsed,
        );
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
    if entry.request_body.as_ref().is_some_and(|b| !b.is_empty()) {
        let sec = "Request Body";
        let is_collapsed = app.network.collapsed_sections.contains(sec);
        push_section_header(
            &mut all_lines,
            &mut section_line_map,
            &mut json_click_map,
            sec,
            is_collapsed,
        );
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
        push_section_header(
            &mut all_lines,
            &mut section_line_map,
            &mut json_click_map,
            sec,
            is_collapsed,
        );
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
    if entry.response_body.as_ref().is_some_and(|b| !b.is_empty()) {
        let sec = "Response Body";
        let is_collapsed = app.network.collapsed_sections.contains(sec);
        push_section_header(
            &mut all_lines,
            &mut section_line_map,
            &mut json_click_map,
            sec,
            is_collapsed,
        );
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
        let rule_key = path_without_query(&entry.path).to_string();
        let has_rule = app.network.sse_merge_rules.contains_key(&rule_key);

        // Check if chunks contain JSON (merged mode is available)
        let has_json_chunks = entry.sse_chunks.first()
            .map(|c| serde_json::from_str::<serde_json::Value>(&c.data).is_ok())
            .unwrap_or(false);

        if has_rule && app.network.sse_merged_mode {
            // ── Merged mode ──
            let sec_key = "SSE Events";
            let is_collapsed = app.network.collapsed_sections.contains(sec_key);

            // Section header with pill toggle + clear button
            {
                let events_pill = Span::styled(
                    " Events ",
                    Style::default().fg(OVERLAY0).bg(SURFACE0),
                );
                let merged_pill = Span::styled(
                    " Merged ",
                    Style::default()
                        .fg(MANTLE)
                        .bg(SAPPHIRE)
                        .add_modifier(Modifier::BOLD),
                );
                let clear_pill = Span::styled(
                    " \u{00d7} ",
                    Style::default().fg(RED).bg(SURFACE0),
                );
                let icon = if is_collapsed { "\u{25b6}" } else { "\u{25bc}" };
                let header_text = format!(
                    " {} SSE Events ({})  ",
                    icon,
                    entry.sse_chunks.len()
                );
                all_lines.push(Line::from(vec![
                    Span::styled(
                        header_text.clone(),
                        Style::default().fg(SAPPHIRE).add_modifier(Modifier::BOLD),
                    ),
                    events_pill,
                    Span::raw(" "),
                    merged_pill,
                    Span::raw(" "),
                    clear_pill,
                ]));
                app.layout.sse_pill_line = Some((all_lines.len() - 1, header_text.width()));
                section_line_map.push(Some(sec_key.to_string()));
                json_click_map.push(None);
            }

            if !is_collapsed {
                let rule = app.network.sse_merge_rules.get(&rule_key).cloned();
                if let Some(rule) = rule {
                    // Collect all chunk data refs
                    let chunks_data: Vec<&str> =
                        entry.sse_chunks.iter().map(|c| c.data.as_str()).collect();

                    // Build candidate field list (scan multiple chunks)
                    let candidates = sse_merge::extract_field_paths(&chunks_data);

                    // Render field selector
                    let selected_idx = app.network.sse_merged_field_idx.min(
                        candidates.len().saturating_sub(1),
                    );
                    for (fi, (_, display)) in candidates.iter().enumerate() {
                        let is_active = fi == selected_idx;
                        let marker = if is_active { "\u{2023} " } else { "  " };
                        let style = if is_active {
                            Style::default().fg(SAPPHIRE).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(OVERLAY0)
                        };
                        all_lines.push(Line::from(Span::styled(
                            format!("  {}{}", marker, display),
                            style,
                        )));
                        section_line_map.push(Some(format!("SSE_FIELD#{}", fi)));
                        json_click_map.push(None);
                    }

                    // Divider
                    let divider_w = inner_w.saturating_sub(2);
                    all_lines.push(Line::from(Span::styled(
                        format!("  {}", "\u{2500}".repeat(divider_w)),
                        Style::default().fg(SURFACE0),
                    )));
                    section_line_map.push(None);
                    json_click_map.push(None);

                    // Merge and render concatenated text
                    let merged_text = sse_merge::merge_field(&chunks_data, &rule.field_path);
                    if merged_text.is_empty() {
                        all_lines.push(Line::from(Span::styled(
                            "   (no data for this field)",
                            Style::default().fg(OVERLAY0),
                        )));
                        section_line_map.push(None);
                        json_click_map.push(None);
                    } else {
                        for wl in wrap_text(&merged_text, inner_w.saturating_sub(3), 500) {
                            all_lines.push(Line::from(Span::styled(
                                format!("   {}", wl),
                                Style::default().fg(TEXT),
                            )));
                            section_line_map.push(None);
                            json_click_map.push(None);
                        }
                    }

                    // Clear rule is now handled by the × pill in the header line
                }
            }
            all_lines.push(Line::raw(""));
            section_line_map.push(None);
            json_click_map.push(None);
        } else {
            // ── Events mode (original + pill toggle if JSON chunks exist) ──
            let sec_name = format!("SSE Events ({})", entry.sse_chunks.len());
            let sec_key = "SSE Events";
            let is_collapsed = app.network.collapsed_sections.contains(sec_key);

            if has_json_chunks {
                // Show pills when chunks are JSON (merged mode available)
                let events_pill = Span::styled(
                    " Events ",
                    Style::default()
                        .fg(MANTLE)
                        .bg(SAPPHIRE)
                        .add_modifier(Modifier::BOLD),
                );
                let merged_pill = Span::styled(
                    " Merged ",
                    Style::default().fg(OVERLAY0).bg(SURFACE0),
                );
                let icon = if is_collapsed { "\u{25b6}" } else { "\u{25bc}" };
                let header_text = format!(" {} {}  ", icon, sec_name);
                all_lines.push(Line::from(vec![
                    Span::styled(
                        header_text.clone(),
                        Style::default().fg(SAPPHIRE).add_modifier(Modifier::BOLD),
                    ),
                    events_pill,
                    Span::raw(" "),
                    merged_pill,
                ]));
                app.layout.sse_pill_line = Some((all_lines.len() - 1, header_text.width()));
                section_line_map.push(Some(sec_key.to_string()));
                json_click_map.push(None);
            } else {
                push_section_header(
                    &mut all_lines,
                    &mut section_line_map,
                    &mut json_click_map,
                    &sec_name,
                    is_collapsed,
                );
                // Override map entry to use fixed key (strip count)
                if let Some(last) = section_line_map.last_mut() {
                    *last = Some(sec_key.to_string());
                }
            }

            if !is_collapsed {
                // Pre-collapse SSE chunks by default on FIRST render only.
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
                    let prefix = if chunk_collapsed {
                        "  \u{25b6}"
                    } else {
                        "  \u{25bc}"
                    };
                    let prefix_text = format!("{} #{} ", prefix, i);
                    let preview_w = inner_w.saturating_sub(prefix_text.len() + 1);
                    all_lines.push(Line::from(vec![
                        Span::styled(prefix_text, Style::default().fg(OVERLAY0)),
                        Span::styled(
                            if chunk_collapsed {
                                if chunk.data.len() > preview_w {
                                    format!("{}...", &chunk.data[..preview_w.saturating_sub(3)])
                                } else {
                                    chunk.data.clone()
                                }
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
        let sec_name = format!("Messages ({}\u{2191} {}\u{2193})", sent, recv);
        let sec_key = "WS Messages";
        let is_collapsed = app.network.collapsed_sections.contains(sec_key);

        // Header with Chat/Raw pills
        {
            let chat_pill = if app.network.ws_chat_mode {
                Span::styled(" Chat ", Style::default().fg(MANTLE).bg(SAPPHIRE).add_modifier(Modifier::BOLD))
            } else {
                Span::styled(" Chat ", Style::default().fg(OVERLAY0).bg(SURFACE0))
            };
            let raw_pill = if !app.network.ws_chat_mode {
                Span::styled(" Raw ", Style::default().fg(MANTLE).bg(SAPPHIRE).add_modifier(Modifier::BOLD))
            } else {
                Span::styled(" Raw ", Style::default().fg(OVERLAY0).bg(SURFACE0))
            };
            let icon = if is_collapsed { "\u{25b6}" } else { "\u{25bc}" };
            let header_text = format!(" {} {}  ", icon, sec_name);
            all_lines.push(Line::from(vec![
                Span::styled(header_text.clone(), Style::default().fg(SAPPHIRE).add_modifier(Modifier::BOLD)),
                chat_pill,
                Span::raw(" "),
                raw_pill,
            ]));
            app.layout.ws_pill_line = Some((all_lines.len() - 1, header_text.width()));
            section_line_map.push(Some(sec_key.to_string()));
            json_click_map.push(None);
        }

        if !is_collapsed {
            if app.network.ws_chat_mode {
                // ── Chat mode: compact timeline with direction pills ──
                let msgs: Vec<(crate::domain::network::WsDirection, &str, u64)> = entry
                    .ws_messages
                    .iter()
                    .map(|m| (m.direction, m.data.as_str(), m.size))
                    .collect();
                let groups = ws_chat::group_messages(&msgs);

                for (gi, group) in groups.iter().enumerate() {
                    let group_key = format!("WS_GROUP#{}", gi);
                    let group_collapsed = !app.network.collapsed_sections.contains(&group_key);
                    // In Chat mode: collapsed = default (not in set), expanded = in set

                    let (pill_text, pill_style, row_bg) = match group.direction {
                        WsDirection::Send => (
                            " \u{2192} SEND ",
                            Style::default().fg(MANTLE).bg(GREEN).add_modifier(Modifier::BOLD),
                            SURFACE0,
                        ),
                        WsDirection::Recv => (
                            " \u{2190} RECV ",
                            Style::default().fg(MANTLE).bg(BLUE).add_modifier(Modifier::BOLD),
                            SURFACE1,
                        ),
                    };
                    let count = group.msg_indices.len();
                    let count_str = if count > 1 { format!(" (\u{00d7}{})", count) } else { String::new() };

                    // Track start of this group's lines so we can apply bg
                    let lines_start = all_lines.len();

                    if group.is_binary {
                        // Binary group: pill + type + binary size, not expandable
                        let total_size = ws_chat::format_binary_size(group.total_size as usize);
                        all_lines.push(Line::from(vec![
                            Span::styled(pill_text, pill_style),
                            Span::styled(
                                format!(" {}{} [binary {}]", group.type_label, count_str, total_size),
                                Style::default().fg(OVERLAY0),
                            ),
                        ]));
                        section_line_map.push(Some(group_key.clone()));
                        json_click_map.push(None);
                    } else if group.merged_delta.is_some() {
                        // Delta group: pill + type + count, merged text below
                        all_lines.push(Line::from(vec![
                            Span::styled(pill_text, pill_style),
                            Span::styled(
                                format!(" {}{}", group.type_label, count_str),
                                Style::default().fg(TEXT),
                            ),
                        ]));
                        section_line_map.push(Some(group_key.clone()));
                        json_click_map.push(None);

                        // Always show merged text for delta groups
                        if let Some(ref merged) = group.merged_delta {
                            if !merged.is_empty() {
                                let wrap_w = inner_w.saturating_sub(3);
                                for wl in wrap_text(merged, wrap_w, 200) {
                                    all_lines.push(Line::from(Span::styled(
                                        format!("   {}", wl),
                                        Style::default().fg(TEXT),
                                    )));
                                    section_line_map.push(None);
                                    json_click_map.push(None);
                                }
                            }
                        }
                    } else {
                        // Regular group: pill + type, click to expand individual messages
                        all_lines.push(Line::from(vec![
                            Span::styled(pill_text, pill_style),
                            Span::styled(
                                format!(" {}{}", group.type_label, count_str),
                                Style::default().fg(TEXT),
                            ),
                        ]));
                        section_line_map.push(Some(group_key.clone()));
                        json_click_map.push(None);

                        // Expanded: show individual messages with JSON
                        if !group_collapsed {
                            for &mi in &group.msg_indices {
                                if let Some(msg) = entry.ws_messages.get(mi) {
                                    let display_data = if ws_chat::has_binary_content(&msg.data) {
                                        ws_chat::preview_message(&msg.data, 0)
                                    } else {
                                        msg.data.clone()
                                    };
                                    render_json_section(
                                        &mut all_lines,
                                        &mut section_line_map,
                                        &mut json_click_map,
                                        &display_data,
                                        &format!("ws_{}", mi),
                                        &mut app.network.json_viewer_states,
                                        inner_w,
                                    );
                                }
                            }
                        }
                    }

                    // Apply background color to ALL lines in this group (including JSON)
                    for line in &mut all_lines[lines_start..] {
                        for span in &mut line.spans {
                            if span.style.bg.is_none() {
                                span.style.bg = Some(row_bg);
                            }
                        }
                    }
                }
            } else {
                // ── Raw mode (original behavior) ──
                for (i, msg) in entry.ws_messages.iter().enumerate() {
                    let (arrow, color) = match msg.direction {
                        WsDirection::Send => ("\u{2192}", GREEN),
                        WsDirection::Recv => ("\u{2190}", BLUE),
                    };
                    let msg_key = format!("WS#{}", i);
                    let msg_collapsed = app.network.collapsed_sections.contains(&msg_key);
                    let prefix = if msg_collapsed { "\u{25b6}" } else { "\u{25bc}" };
                    let ws_prefix_text = format!("  {} {} ", prefix, arrow);
                    let ws_preview_w = inner_w.saturating_sub(ws_prefix_text.len() + 1);
                    all_lines.push(Line::from(vec![
                        Span::styled(ws_prefix_text, Style::default().fg(color)),
                        Span::styled(
                            if msg_collapsed {
                                if msg.data.len() > ws_preview_w {
                                    format!("{}...", &msg.data[..ws_preview_w.saturating_sub(3)])
                                } else {
                                    msg.data.clone()
                                }
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
        push_section_header(
            &mut all_lines,
            &mut section_line_map,
            &mut json_click_map,
            sec,
            is_collapsed,
        );
        if !is_collapsed {
            let wrapped = wrap_text(error, inner_w.saturating_sub(3), 20);
            for wl in &wrapped {
                all_lines.push(Line::from(Span::styled(
                    format!("   {}", wl),
                    Style::default().fg(RED),
                )));
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
    let visible_lines: Vec<Line> = all_lines.into_iter().skip(scroll).take(inner_h).collect();

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

// ══════════════════════════════════════
//  Section helpers
// ══════════════════════════════════════

fn push_section_header(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Option<(String, u32)>>,
    title: &str,
    collapsed: bool,
) {
    let icon = if collapsed { "\u{25b6}" } else { "\u{25bc}" }; // ▶ ▼
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
    json_click_map: &mut Vec<Option<(String, u32)>>,
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
    json_click_map: &mut Vec<Option<(String, u32)>>,
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
    json_click_map: &mut Vec<Option<(String, u32)>>,
    json_text: &str,
    section_key: &str,
    viewer_states: &mut HashMap<String, JsonViewerState>,
    max_w: usize,
) {
    match crate::domain::structured_parser::parse_whole(json_text) {
        Some(value) => {
            let tree = json_viewer::Tree::from_value(&value);
            let state = viewer_states
                .entry(section_key.to_string())
                .or_insert_with(|| json_viewer::init_state(&tree, 1));
            let base = lines.len();
            json_viewer::append_render(
                lines,
                json_click_map,
                &tree,
                state,
                section_key,
                "   ",
                max_w.saturating_sub(3),
            );
            // Keep section_map in sync with lines.
            for _ in base..lines.len() {
                section_map.push(None);
            }
        }
        None => {
            for wl in wrap_text(json_text, max_w.saturating_sub(3), 100) {
                lines.push(Line::from(Span::styled(
                    format!("   {}", wl),
                    Style::default().fg(SUBTEXT0),
                )));
                section_map.push(None);
                json_click_map.push(None);
            }
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
            if pair.is_empty() {
                continue;
            }
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
        408 => "Request Timeout",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "",
    }
}

/// Strip query params from URL path for merge rule matching.
fn path_without_query(path: &str) -> &str {
    path.split('?').next().unwrap_or(path)
}
