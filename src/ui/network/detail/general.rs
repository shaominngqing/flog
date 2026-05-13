//! "General" section renderer — URL / Method / Status / Time / Duration / Size.
//!
//! Phase 3 Step 3.8 (UI-037): extracted from `detail/mod.rs`.

use ratatui::{
    style::Style,
    text::{Line, Span},
};

use crate::domain::network::{NetworkEntry, NetworkStatus, Protocol};
use crate::ui::json_viewer::JsonHotRegion;

use super::super::{format_duration, format_size, status_color};
use super::shared::{http_status_text, push_kv_single, push_kv_wrapped, push_section_header};
use super::KEY_COLOR;
use std::collections::HashSet;

/// Render the collapsible "General" section (URL, method, status, time,
/// duration, size). Appends to `lines` / `section_map` / `json_click_map`.
pub(super) fn render_general(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    entry: &NetworkEntry,
    collapsed_sections: &HashSet<String>,
    inner_w: usize,
) {
    let sec = "General";
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

    // URL — full, wrapped
    push_kv_wrapped(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        "URL",
        &entry.url,
        inner_w,
    );
    if !entry.method.is_empty() {
        push_kv_single(
            lines,
            section_map,
            json_click_map,
            json_section_keys,
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
        NetworkStatus::Orphan => entry
            .http_status
            .map_or("Orphan (no matching request)".to_string(), |s| {
                format!("Orphan {} {}", s, http_status_text(s))
            }),
    };
    let sc = status_color(entry.status, entry.http_status);
    lines.push(Line::from(vec![
        Span::styled("   Status: ", Style::default().fg(KEY_COLOR)),
        Span::styled(status_str, Style::default().fg(sc)),
    ]));
    section_map.push(None);
    json_click_map.push(Vec::new());
    json_section_keys.push(None);
    if !entry.timestamp.is_empty() {
        push_kv_single(
            lines,
            section_map,
            json_click_map,
            json_section_keys,
            "Time",
            &entry.timestamp,
        );
    }
    if let Some(dur) = entry.duration {
        push_kv_single(
            lines,
            section_map,
            json_click_map,
            json_section_keys,
            "Duration",
            &format_duration(dur),
        );
    }
    let size = entry.display_size();
    if size > 0 {
        push_kv_single(
            lines,
            section_map,
            json_click_map,
            json_section_keys,
            "Size",
            &format_size(size),
        );
    }
    lines.push(Line::raw(""));
    section_map.push(None);
    json_click_map.push(Vec::new());
    json_section_keys.push(None);
}
