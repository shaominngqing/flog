//! WebSocket Messages renderer — Chat and Raw modes.
//!
//! Phase 3 Step 3.8 (UI-037): extracted from `detail/mod.rs`. The
//! UI-042 collapse-key purge lives on `App::toggle_ws_chat_mode`; this
//! module is the consumer of those keys and must stay in sync with the
//! `WS#*` (raw) / `WS_GROUP#*` (chat) key conventions used below.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::domain::network::{NetworkEntry, WsDirection};
use crate::domain::ws_chat;

use super::super::super::{
    wrap_text, BLUE, GREEN, MANTLE, OVERLAY0, SAPPHIRE, SUBTEXT0, SURFACE0, SURFACE1, TEXT,
};
use super::shared::render_json_section;

/// Render the "WebSocket Messages" section. Caller guarantees
/// `entry.ws_messages` is non-empty.
pub(super) fn render_ws(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Option<(String, u32)>>,
    app: &mut App,
    entry: &NetworkEntry,
    inner_w: usize,
) {
    let sent = entry
        .ws_messages
        .iter()
        .filter(|m| m.direction == WsDirection::Send)
        .count();
    let recv = entry
        .ws_messages
        .iter()
        .filter(|m| m.direction == WsDirection::Recv)
        .count();
    let sec_name = format!("Messages ({}\u{2191} {}\u{2193})", sent, recv);
    let sec_key = "WS Messages";
    let is_collapsed = app.network.collapsed_sections.contains(sec_key);

    // Header with Chat/Raw pills
    {
        let chat_pill = if app.network.ws_chat_mode {
            Span::styled(
                " Chat ",
                Style::default()
                    .fg(MANTLE)
                    .bg(SAPPHIRE)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(" Chat ", Style::default().fg(OVERLAY0).bg(SURFACE0))
        };
        let raw_pill = if !app.network.ws_chat_mode {
            Span::styled(
                " Raw ",
                Style::default()
                    .fg(MANTLE)
                    .bg(SAPPHIRE)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(" Raw ", Style::default().fg(OVERLAY0).bg(SURFACE0))
        };
        let icon = if is_collapsed { "\u{25b6}" } else { "\u{25bc}" };
        let header_text = format!(" {} {}  ", icon, sec_name);
        lines.push(Line::from(vec![
            Span::styled(
                header_text.clone(),
                Style::default().fg(SAPPHIRE).add_modifier(Modifier::BOLD),
            ),
            chat_pill,
            Span::raw(" "),
            raw_pill,
        ]));
        app.layout.ws_pill_line = Some((lines.len() - 1, header_text.width()));
        section_map.push(Some(sec_key.to_string()));
        json_click_map.push(None);
    }

    if !is_collapsed {
        if app.network.ws_chat_mode {
            render_chat_mode(lines, section_map, json_click_map, app, entry, inner_w);
        } else {
            render_raw_mode(lines, section_map, json_click_map, app, entry, inner_w);
        }
        lines.push(Line::raw(""));
        section_map.push(None);
        json_click_map.push(None);
    }
}

fn render_chat_mode(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Option<(String, u32)>>,
    app: &mut App,
    entry: &NetworkEntry,
    inner_w: usize,
) {
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
                Style::default()
                    .fg(MANTLE)
                    .bg(GREEN)
                    .add_modifier(Modifier::BOLD),
                SURFACE0,
            ),
            WsDirection::Recv => (
                " \u{2190} RECV ",
                Style::default()
                    .fg(MANTLE)
                    .bg(BLUE)
                    .add_modifier(Modifier::BOLD),
                SURFACE1,
            ),
        };
        let count = group.msg_indices.len();
        let count_str = if count > 1 {
            format!(" (\u{00d7}{})", count)
        } else {
            String::new()
        };

        // Track start of this group's lines so we can apply bg
        let lines_start = lines.len();

        if group.is_binary {
            // Binary group: pill + type + binary size, not expandable
            let total_size = ws_chat::format_binary_size(group.total_size as usize);
            lines.push(Line::from(vec![
                Span::styled(pill_text, pill_style),
                Span::styled(
                    format!(" {}{} [binary {}]", group.type_label, count_str, total_size),
                    Style::default().fg(OVERLAY0),
                ),
            ]));
            section_map.push(Some(group_key.clone()));
            json_click_map.push(None);
        } else if group.merged_delta.is_some() {
            // Delta group: pill + type + count, merged text below
            lines.push(Line::from(vec![
                Span::styled(pill_text, pill_style),
                Span::styled(
                    format!(" {}{}", group.type_label, count_str),
                    Style::default().fg(TEXT),
                ),
            ]));
            section_map.push(Some(group_key.clone()));
            json_click_map.push(None);

            // Always show merged text for delta groups
            if let Some(ref merged) = group.merged_delta {
                if !merged.is_empty() {
                    let wrap_w = inner_w.saturating_sub(3);
                    for wl in wrap_text(merged, wrap_w, 200) {
                        lines.push(Line::from(Span::styled(
                            format!("   {}", wl),
                            Style::default().fg(TEXT),
                        )));
                        section_map.push(None);
                        json_click_map.push(None);
                    }
                }
            }
        } else {
            // Regular group: pill + type, click to expand individual messages
            lines.push(Line::from(vec![
                Span::styled(pill_text, pill_style),
                Span::styled(
                    format!(" {}{}", group.type_label, count_str),
                    Style::default().fg(TEXT),
                ),
            ]));
            section_map.push(Some(group_key.clone()));
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
                            lines,
                            section_map,
                            json_click_map,
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
        for line in &mut lines[lines_start..] {
            for span in &mut line.spans {
                if span.style.bg.is_none() {
                    span.style.bg = Some(row_bg);
                }
            }
        }
    }
}

fn render_raw_mode(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Option<(String, u32)>>,
    app: &mut App,
    entry: &NetworkEntry,
    inner_w: usize,
) {
    // ── Raw mode (original behavior) ──
    for (i, msg) in entry.ws_messages.iter().enumerate() {
        let (arrow, color) = match msg.direction {
            WsDirection::Send => ("\u{2192}", GREEN),
            WsDirection::Recv => ("\u{2190}", BLUE),
        };
        let msg_key = format!("WS#{}", i);
        let msg_collapsed = app.network.collapsed_sections.contains(&msg_key);
        let prefix = if msg_collapsed {
            "\u{25b6}"
        } else {
            "\u{25bc}"
        };
        let ws_prefix_text = format!("  {} {} ", prefix, arrow);
        let ws_preview_w = inner_w.saturating_sub(ws_prefix_text.len() + 1);
        lines.push(Line::from(vec![
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
        section_map.push(Some(msg_key.clone()));
        json_click_map.push(None);
        if !msg_collapsed {
            render_json_section(
                lines,
                section_map,
                json_click_map,
                &msg.data,
                &format!("ws_{}", i),
                &mut app.network.json_viewer_states,
                inner_w,
            );
        }
    }
}
