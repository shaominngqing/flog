//! SSE Events + Merged-view renderer for the network detail panel.
//!
//! Phase 3 Step 3.8 (UI-037): extracted from `detail/mod.rs`.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::domain::network::NetworkEntry;
use crate::domain::sse_merge;
use crate::ui::json_viewer::JsonHotRegion;

use super::super::super::{wrap_text, MANTLE, OVERLAY0, RED, SAPPHIRE, SUBTEXT0, SURFACE0, TEXT};
use super::shared::{path_without_query, push_section_header, render_json_section};

/// Render the SSE Events section (both Events mode and Merged mode) for an
/// SSE protocol entry. Caller guarantees `entry.sse_chunks` is non-empty.
pub(super) fn render_sse(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    app: &mut App,
    entry: &NetworkEntry,
    inner_w: usize,
) {
    let copied_ids: std::collections::HashSet<u32> = {
        let threshold = std::time::Duration::from_secs(2);
        app.detail
            .copied_node_feedback
            .iter()
            .filter(|(_, t)| t.elapsed() < threshold)
            .map(|(&id, _)| id)
            .collect()
    };
    let rule_key = path_without_query(&entry.path).to_string();
    let has_rule = app.network.sse_merge_rules.contains_key(&rule_key);

    // Check if chunks contain JSON (merged mode is available)
    let has_json_chunks = entry
        .sse_chunks
        .first()
        .map(|c| serde_json::from_str::<serde_json::Value>(&c.data).is_ok())
        .unwrap_or(false);

    if has_rule && app.network.sse_merged_mode {
        render_merged_mode(
            lines,
            section_map,
            json_click_map,
            json_section_keys,
            app,
            entry,
            &rule_key,
            inner_w,
        );
    } else {
        render_events_mode(
            lines,
            section_map,
            json_click_map,
            json_section_keys,
            app,
            entry,
            has_json_chunks,
            inner_w,
            &copied_ids,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn render_merged_mode(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    app: &mut App,
    entry: &NetworkEntry,
    rule_key: &str,
    inner_w: usize,
) {
    let sec_key = "SSE Events";
    let is_collapsed = app.network.collapsed_sections.contains(sec_key);

    // Section header with pill toggle + clear button
    {
        let events_pill = Span::styled(" Events ", Style::default().fg(OVERLAY0).bg(SURFACE0));
        let merged_pill = Span::styled(
            " Merged ",
            Style::default()
                .fg(MANTLE)
                .bg(SAPPHIRE)
                .add_modifier(Modifier::BOLD),
        );
        let clear_pill = Span::styled(" \u{00d7} ", Style::default().fg(RED).bg(SURFACE0));
        let icon = if is_collapsed { "\u{25b6}" } else { "\u{25bc}" };
        let header_text = format!(" {} SSE Events ({})  ", icon, entry.sse_chunks.len());
        lines.push(Line::from(vec![
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
        app.layout.sse_pill_line = Some((lines.len() - 1, header_text.width()));
        section_map.push(Some(sec_key.to_string()));
        json_click_map.push(Vec::new());
        json_section_keys.push(None);
    }

    if !is_collapsed {
        let rule = app.network.sse_merge_rules.get(rule_key).cloned();
        if let Some(rule) = rule {
            let chunks_data: Vec<&str> = entry.sse_chunks.iter().map(|c| c.data.as_str()).collect();

            let candidates = sse_merge::extract_field_paths(&chunks_data);

            let selected_idx = app
                .network
                .sse_merged_field_idx
                .min(candidates.len().saturating_sub(1));
            for (fi, (_, display)) in candidates.iter().enumerate() {
                let is_active = fi == selected_idx;
                let marker = if is_active { "\u{2023} " } else { "  " };
                let style = if is_active {
                    Style::default().fg(SAPPHIRE).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(OVERLAY0)
                };
                lines.push(Line::from(Span::styled(
                    format!("  {}{}", marker, display),
                    style,
                )));
                section_map.push(Some(format!("SSE_FIELD#{}", fi)));
                json_click_map.push(Vec::new());
                json_section_keys.push(None);
            }

            // Divider
            let divider_w = inner_w.saturating_sub(2);
            lines.push(Line::from(Span::styled(
                format!("  {}", "\u{2500}".repeat(divider_w)),
                Style::default().fg(SURFACE0),
            )));
            section_map.push(None);
            json_click_map.push(Vec::new());
            json_section_keys.push(None);

            let merged_text = sse_merge::merge_field(&chunks_data, &rule.field_path);
            if merged_text.is_empty() {
                lines.push(Line::from(Span::styled(
                    "   (no data for this field)",
                    Style::default().fg(OVERLAY0),
                )));
                section_map.push(None);
                json_click_map.push(Vec::new());
                json_section_keys.push(None);
            } else {
                for wl in wrap_text(&merged_text, inner_w.saturating_sub(3), 500) {
                    lines.push(Line::from(Span::styled(
                        format!("   {}", wl),
                        Style::default().fg(TEXT),
                    )));
                    section_map.push(None);
                    json_click_map.push(Vec::new());
                    json_section_keys.push(None);
                }
            }
        }
    }
    lines.push(Line::raw(""));
    section_map.push(None);
    json_click_map.push(Vec::new());
    json_section_keys.push(None);
}

#[allow(clippy::too_many_arguments)]
fn render_events_mode(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    app: &mut App,
    entry: &NetworkEntry,
    has_json_chunks: bool,
    inner_w: usize,
    copied_ids: &std::collections::HashSet<u32>,
) {
    let sec_name = format!("SSE Events ({})", entry.sse_chunks.len());
    let sec_key = "SSE Events";
    let is_collapsed = app.network.collapsed_sections.contains(sec_key);

    if has_json_chunks {
        let events_pill = Span::styled(
            " Events ",
            Style::default()
                .fg(MANTLE)
                .bg(SAPPHIRE)
                .add_modifier(Modifier::BOLD),
        );
        let merged_pill = Span::styled(" Merged ", Style::default().fg(OVERLAY0).bg(SURFACE0));
        let icon = if is_collapsed { "\u{25b6}" } else { "\u{25bc}" };
        let header_text = format!(" {} {}  ", icon, sec_name);
        lines.push(Line::from(vec![
            Span::styled(
                header_text.clone(),
                Style::default().fg(SAPPHIRE).add_modifier(Modifier::BOLD),
            ),
            events_pill,
            Span::raw(" "),
            merged_pill,
        ]));
        app.layout.sse_pill_line = Some((lines.len() - 1, header_text.width()));
        section_map.push(Some(sec_key.to_string()));
        json_click_map.push(Vec::new());
        json_section_keys.push(None);
    } else {
        push_section_header(
            lines,
            section_map,
            json_click_map,
            json_section_keys,
            &sec_name,
            is_collapsed,
        );
        // Override map entry to use fixed key (strip count)
        if let Some(last) = section_map.last_mut() {
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
            lines.push(Line::from(vec![
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
            section_map.push(Some(chunk_key.clone()));
            json_click_map.push(Vec::new());
            json_section_keys.push(None);
            if !chunk_collapsed {
                if let Some((k, t)) = render_json_section(
                    lines,
                    section_map,
                    json_click_map,
                    json_section_keys,
                    &chunk.data,
                    &format!("sse_{}", i),
                    &mut app.network.json_viewer_states,
                    inner_w,
                    &copied_ids,
                ) {
                    app.network.detail_json_trees.insert(k, t);
                }
            }
        }
    }
    lines.push(Line::raw(""));
    section_map.push(None);
    json_click_map.push(Vec::new());
    json_section_keys.push(None);
}
