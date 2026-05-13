//! Shared helpers for the network detail panel — section headers,
//! key/value lines, JSON section rendering, query-param parsing, URL
//! decoding, and HTTP status text.
//!
//! Phase 3 Step 3.8 (UI-037): extracted from `detail/mod.rs` to keep the
//! coordinator lean and each section renderer free of boilerplate.

use std::collections::HashMap;

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

use crate::ui::json_viewer::{self, JsonViewerState};

use super::super::super::{sanitize_for_cell, wrap_text, SAPPHIRE, SUBTEXT0};
use super::{KEY_COLOR, STR_COLOR};

// ══════════════════════════════════════
//  Section helpers
// ══════════════════════════════════════

pub(super) fn push_section_header(
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

pub(super) fn push_kv_single(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Option<(String, u32)>>,
    key: &str,
    value: &str,
) {
    // UI-046: key/value both come from user data (query params, URL
    // pieces, general metadata) — sanitize before they reach a Span.
    let key = sanitize_for_cell(key);
    let value = sanitize_for_cell(value);
    lines.push(Line::from(vec![
        Span::styled(format!("   {}: ", key), Style::default().fg(KEY_COLOR)),
        Span::styled(value.into_owned(), Style::default().fg(STR_COLOR)),
    ]));
    section_map.push(None);
    json_click_map.push(None);
}

pub(super) fn push_kv_wrapped(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Option<(String, u32)>>,
    key: &str,
    value: &str,
    max_w: usize,
) {
    // UI-046: user-data in, sanitize before width math / Span body.
    let key = sanitize_for_cell(key);
    let value = sanitize_for_cell(value);
    let prefix = format!("   {}: ", key);
    let first_line_w = max_w.saturating_sub(prefix.len());
    let cont_indent = "   ";
    let _cont_w = max_w.saturating_sub(cont_indent.len());

    if value.width() <= first_line_w {
        lines.push(Line::from(vec![
            Span::styled(prefix, Style::default().fg(KEY_COLOR)),
            Span::styled(value.into_owned(), Style::default().fg(STR_COLOR)),
        ]));
        section_map.push(None);
        json_click_map.push(None);
    } else {
        let wrapped = wrap_text(&value, first_line_w, 20);
        if let Some(first) = wrapped.first() {
            lines.push(Line::from(vec![
                Span::styled(prefix, Style::default().fg(KEY_COLOR)),
                Span::styled(first.to_string(), Style::default().fg(STR_COLOR)),
            ]));
            section_map.push(None);
            json_click_map.push(None);
        }
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

/// Default initial expansion depth for JSON sections (DevTools style:
/// root + its direct children expanded, grandchildren folded).
pub(super) const DEFAULT_JSON_EXPAND_DEPTH: u32 = 1;

/// Render a JSON/Dart-Map section with the default expansion depth.
/// Callers that need a different depth use [`render_json_section_with_depth`].
pub(super) fn render_json_section(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Option<(String, u32)>>,
    json_text: &str,
    section_key: &str,
    viewer_states: &mut HashMap<String, JsonViewerState>,
    max_w: usize,
) {
    render_json_section_with_depth(
        lines,
        section_map,
        json_click_map,
        json_text,
        section_key,
        viewer_states,
        max_w,
        DEFAULT_JSON_EXPAND_DEPTH,
    );
}

/// Like [`render_json_section`] but overrides the initial expansion depth.
/// Use `0` to show only the root expanded (e.g. for headers where metadata
/// is usually collapsed by default).
// Phase 3 redesign — see Audit UI-037: extract parameter struct.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_json_section_with_depth(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Option<(String, u32)>>,
    json_text: &str,
    section_key: &str,
    viewer_states: &mut HashMap<String, JsonViewerState>,
    max_w: usize,
    expand_depth: u32,
) {
    match crate::domain::structured_parser::parse_whole(json_text) {
        Some(value) => {
            let tree = json_viewer::Tree::from_value(&value);
            let state = viewer_states
                .entry(section_key.to_string())
                .or_insert_with(|| json_viewer::init_state(&tree, expand_depth));
            let base = lines.len();
            // append_render now produces Vec<Vec<JsonHotRegion>>. Translate back
            // to the network-section fold-map format (Option<(section_key, node_id)>)
            // so callers of this function don't need to change.
            let mut new_click_map: Vec<Vec<crate::ui::json_viewer::JsonHotRegion>> = Vec::new();
            json_viewer::append_render(
                lines,
                &mut new_click_map,
                &tree,
                state,
                section_key,
                "   ",
                max_w.saturating_sub(3),
            );
            for regions in new_click_map {
                let slot = regions.into_iter().find_map(|r| {
                    if let crate::ui::json_viewer::JsonAction::ToggleFold(id) = r.action {
                        Some((section_key.to_string(), id))
                    } else {
                        None
                    }
                });
                json_click_map.push(slot);
            }
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

pub(super) fn parse_query_params(url: &str) -> Vec<(String, String)> {
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

pub(super) fn url_decode(s: &str) -> String {
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

pub(super) fn http_status_text(code: u16) -> &'static str {
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
pub(super) fn path_without_query(path: &str) -> &str {
    path.split('?').next().unwrap_or(path)
}
