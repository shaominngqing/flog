//! "Error" section renderer for the network detail panel.
//!
//! Phase 3 Step 3.8 (UI-037): extracted from `detail/mod.rs`.

use std::collections::HashSet;

use ratatui::{
    style::Style,
    text::{Line, Span},
};

use super::super::super::{wrap_text, RED};
use super::shared::push_section_header;

/// Render the "Error" section, wrapping the error string at the panel
/// width. No-op if `error` is None.
pub(super) fn render_error(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Option<(String, u32)>>,
    error: Option<&String>,
    collapsed_sections: &HashSet<String>,
    inner_w: usize,
) {
    let error = match error {
        Some(e) => e,
        None => return,
    };
    let sec = "Error";
    let is_collapsed = collapsed_sections.contains(sec);
    push_section_header(lines, section_map, json_click_map, sec, is_collapsed);
    if is_collapsed {
        return;
    }
    let wrapped = wrap_text(error, inner_w.saturating_sub(3), 20);
    for wl in &wrapped {
        lines.push(Line::from(Span::styled(
            format!("   {}", wl),
            Style::default().fg(RED),
        )));
        section_map.push(None);
        json_click_map.push(None);
    }
}
