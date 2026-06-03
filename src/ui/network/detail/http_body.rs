//! HTTP section renderers — Query Params, Request/Response Headers,
//! Request/Response Body.
//!
//! Phase 3 Step 3.8 (UI-037): extracted from `detail/mod.rs`.

use std::collections::{HashMap, HashSet};

use ratatui::text::Line;

use crate::domain::network::NetworkEntry;
use crate::ui::json_viewer::{self, JsonHotRegion, JsonViewerState};

use super::shared::{
    parse_query_params, push_kv_wrapped, push_section_header, render_json_section,
    render_json_section_with_depth,
};

/// Render the "Query Parameters" section if the URL has any.
pub(super) fn render_query_params(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
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
    push_section_header(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        sec,
        is_collapsed,
    );
    if is_collapsed {
        return;
    }
    for (key, value) in &query_params {
        push_kv_wrapped(
            lines,
            section_map,
            json_click_map,
            json_section_keys,
            key,
            value,
            inner_w,
        );
    }
    lines.push(Line::raw(""));
    section_map.push(None);
    json_click_map.push(Vec::new());
    json_section_keys.push(None);
}

/// Render a "Request Headers" / "Response Headers" section. Headers are
/// rendered with expand_depth=0 (root only expanded) to save space.
/// Returns the parsed Tree for the section (if rendered), keyed by `section_key`.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_headers(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    headers: Option<&String>,
    section_title: &str,
    section_key: &str,
    collapsed_sections: &HashSet<String>,
    viewer_states: &mut HashMap<String, JsonViewerState>,
    inner_w: usize,
    copied_ids: &std::collections::HashSet<u32>,
) -> Option<(String, json_viewer::Tree)> {
    let headers = headers?;
    let is_collapsed = collapsed_sections.contains(section_title);
    push_section_header(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        section_title,
        is_collapsed,
    );
    if is_collapsed {
        return None;
    }
    let tree_entry = render_json_section_with_depth(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        headers,
        section_key,
        viewer_states,
        inner_w,
        0,
        copied_ids,
    );
    lines.push(Line::raw(""));
    section_map.push(None);
    json_click_map.push(Vec::new());
    json_section_keys.push(None);
    tree_entry
}

/// Render a "Request Body" / "Response Body" section. Bodies use the
/// default expand depth (root + direct children expanded).
/// Returns the parsed Tree for the section (if rendered), keyed by `section_key`.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_body(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    body: Option<&String>,
    section_title: &str,
    section_key: &str,
    collapsed_sections: &HashSet<String>,
    viewer_states: &mut HashMap<String, JsonViewerState>,
    inner_w: usize,
    copied_ids: &std::collections::HashSet<u32>,
) -> Option<(String, json_viewer::Tree)> {
    let body_ref = match body {
        Some(b) if !b.is_empty() => b,
        _ => return None,
    };
    let is_collapsed = collapsed_sections.contains(section_title);
    push_section_header(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        section_title,
        is_collapsed,
    );
    if is_collapsed {
        return None;
    }
    let tree_entry = render_json_section(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        body_ref,
        section_key,
        viewer_states,
        inner_w,
        copied_ids,
    );
    lines.push(Line::raw(""));
    section_map.push(None);
    json_click_map.push(Vec::new());
    json_section_keys.push(None);
    tree_entry
}
