//! HTTP section renderers — Query Params, Request/Response Headers,
//! Request/Response Body.
//!
//! Phase 3 Step 3.8 (UI-037): extracted from `detail/mod.rs`.

use std::collections::{HashMap, HashSet};

use ratatui::text::Line;

use crate::domain::network::NetworkEntry;
use crate::ui::json_viewer::JsonViewerState;

use super::shared::{
    parse_query_params, push_kv_wrapped, push_section_header, render_json_section,
    render_json_section_with_depth,
};

/// Render the "Query Parameters" section if the URL has any.
pub(super) fn render_query_params(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Option<(String, u32)>>,
    entry: &NetworkEntry,
    collapsed_sections: &HashSet<String>,
    inner_w: usize,
) {
    let query_params = parse_query_params(&entry.url);
    if query_params.is_empty() {
        return;
    }
    let sec = "Query Parameters";
    let is_collapsed = collapsed_sections.contains(sec);
    push_section_header(lines, section_map, json_click_map, sec, is_collapsed);
    if is_collapsed {
        return;
    }
    for (key, value) in &query_params {
        push_kv_wrapped(lines, section_map, json_click_map, key, value, inner_w);
    }
    lines.push(Line::raw(""));
    section_map.push(None);
    json_click_map.push(None);
}

/// Render a "Request Headers" / "Response Headers" section. Headers are
/// rendered with expand_depth=0 (root only expanded) to save space.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_headers(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Option<(String, u32)>>,
    headers: Option<&String>,
    section_title: &str,
    section_key: &str,
    collapsed_sections: &HashSet<String>,
    viewer_states: &mut HashMap<String, JsonViewerState>,
    inner_w: usize,
) {
    if headers.is_none() {
        return;
    }
    let is_collapsed = collapsed_sections.contains(section_title);
    push_section_header(
        lines,
        section_map,
        json_click_map,
        section_title,
        is_collapsed,
    );
    if is_collapsed {
        return;
    }
    if let Some(hdrs) = headers {
        render_json_section_with_depth(
            lines,
            section_map,
            json_click_map,
            hdrs,
            section_key,
            viewer_states,
            inner_w,
            0,
        );
    }
    lines.push(Line::raw(""));
    section_map.push(None);
    json_click_map.push(None);
}

/// Render a "Request Body" / "Response Body" section. Bodies use the
/// default expand depth (root + direct children expanded).
#[allow(clippy::too_many_arguments)]
pub(super) fn render_body(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Option<(String, u32)>>,
    body: Option<&String>,
    section_title: &str,
    section_key: &str,
    collapsed_sections: &HashSet<String>,
    viewer_states: &mut HashMap<String, JsonViewerState>,
    inner_w: usize,
) {
    let body_ref = match body {
        Some(b) if !b.is_empty() => b,
        _ => return,
    };
    let is_collapsed = collapsed_sections.contains(section_title);
    push_section_header(
        lines,
        section_map,
        json_click_map,
        section_title,
        is_collapsed,
    );
    if is_collapsed {
        return;
    }
    render_json_section(
        lines,
        section_map,
        json_click_map,
        body_ref,
        section_key,
        viewer_states,
        inner_w,
    );
    lines.push(Line::raw(""));
    section_map.push(None);
    json_click_map.push(None);
}
