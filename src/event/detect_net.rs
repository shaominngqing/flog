//! Network-tab click region detection extracted from `detect.rs` to
//! keep each file under the Phase 3 Step 3.6 line budget.

use crate::app::App;

use super::click_region::ClickRegion;
use super::pills::{PILL_PADDING, SSE_EVENTS_PILL, SSE_MERGED_PILL, WS_CHAT_PILL, WS_LIST_PILL};

pub(super) fn detect(app: &App, x: u16, y: u16) -> Option<ClickRegion> {
    // Toolbar line 1: search + exclude
    if y == app.layout.net_toolbar_y {
        if x >= app.layout.net_search_x.0 && x < app.layout.net_search_x.1 {
            return Some(ClickRegion::NetworkToolbarSearch);
        }
        if x >= app.layout.net_exclude_x.0 && x < app.layout.net_exclude_x.1 {
            return Some(ClickRegion::NetworkToolbarExclude);
        }
    }
    // Toolbar line 2: filter pills
    if y == app.layout.net_filter_pills_y {
        for (id, x_start, x_end) in &app.layout.net_filter_pills {
            if x >= *x_start && x < *x_end {
                return super::detect::pill_id_to_region(id);
            }
        }
    }

    // Mock rules panel
    if app.network.show_mock_rules_panel
        && x >= app.layout.net_detail_x
        && y >= app.layout.list_y
        && y < app.layout.bottom_y
    {
        for (row_idx, action, ry, x_start, x_end) in &app.layout.mock_rule_regions {
            if y == *ry && x >= *x_start && x < *x_end {
                return Some(match action.as_str() {
                    "select" => ClickRegion::MockRuleRow { index: *row_idx },
                    "edit" => ClickRegion::MockRuleEditBtn { index: *row_idx },
                    "toggle" => ClickRegion::MockRuleToggle { index: *row_idx },
                    "delete" => ClickRegion::MockRuleDelete { index: *row_idx },
                    _ => return None,
                });
            }
        }
        return None;
    }

    // Network detail panel
    if app.network.show_detail
        && x >= app.layout.net_detail_x
        && y >= app.layout.list_y
        && y < app.layout.bottom_y
    {
        return detect_network_detail(app, x, y);
    }

    // Status bar
    if y == app.layout.bottom_y {
        if x >= app.layout.source_info_x.0 && x < app.layout.source_info_x.1 {
            return Some(ClickRegion::StatusBar);
        }
        for (name, x_start, x_end) in &app.layout.net_buttons {
            if x >= *x_start && x < *x_end {
                return Some(match name.as_str() {
                    "mock" => ClickRegion::NetworkMockRulesBtn,
                    _ => ClickRegion::StatusBar,
                });
            }
        }
    }

    // List area
    if y >= app.layout.list_y && y < app.layout.list_y + app.layout.list_height {
        let row_in_list = y - app.layout.list_y;
        return Some(ClickRegion::NetworkListRow { row: row_in_list });
    }

    None
}

fn detect_network_detail(app: &App, x: u16, y: u16) -> Option<ClickRegion> {
    // [Mock] button in detail header
    if let Some((btn_y, btn_x_start, btn_x_end)) = app.layout.detail_mock_btn {
        if y == btn_y && x >= btn_x_start && x < btn_x_end {
            return Some(ClickRegion::NetworkDetailMockBtn);
        }
    }

    let detail_content_y = app.layout.net_detail_content_y;
    if y >= detail_content_y && y < app.layout.bottom_y {
        let line_idx = app.network.detail_scroll + (y - detail_content_y) as usize;

        if let Some(pill) = detect_sse_pill(app, x, line_idx) {
            return Some(pill);
        }
        if let Some(pill) = detect_ws_pill(app, x, line_idx) {
            return Some(pill);
        }

        if let Some(Some(section_key)) = app.network.detail_section_map.get(line_idx) {
            if let Some(idx_str) = section_key.strip_prefix("SSE_FIELD#") {
                if let Ok(fi) = idx_str.parse::<usize>() {
                    return Some(ClickRegion::NetworkDetailSseFieldPill { idx: fi });
                }
            }
            if section_key == "SSE_CLEAR_RULE" {
                return Some(ClickRegion::NetworkDetailSectionToggle {
                    section_key: "SSE_CLEAR_RULE".to_string(),
                });
            }
            return Some(ClickRegion::NetworkDetailSectionToggle {
                section_key: section_key.clone(),
            });
        }

        if let Some(Some((section_key, node_id))) =
            app.network.detail_json_click_map.get(line_idx).cloned()
        {
            return Some(ClickRegion::NetworkDetailSectionToggle {
                section_key: format!("JSON#{section_key}#{node_id}"),
            });
        }
    }
    None
}

fn detect_sse_pill(app: &App, x: u16, line_idx: usize) -> Option<ClickRegion> {
    let (pill_line, header_w) = app.layout.sse_pill_line?;
    if line_idx != pill_line {
        return None;
    }
    let click_x = (x.saturating_sub(app.layout.net_detail_x + 1)) as usize;
    let events_start = header_w;
    let events_end = events_start + SSE_EVENTS_PILL.len();
    let merged_start = events_end + PILL_PADDING;
    let merged_end = merged_start + SSE_MERGED_PILL.len();
    if click_x >= events_start && click_x < events_end {
        return Some(ClickRegion::NetworkDetailSseEventsPill);
    }
    if click_x >= merged_start && click_x < merged_end {
        return Some(ClickRegion::NetworkDetailSseMergedPill);
    }
    let clear_start = merged_end + 1;
    let clear_end = clear_start + " \u{00d7} ".len();
    if click_x >= clear_start && click_x < clear_end {
        return Some(ClickRegion::NetworkDetailSectionToggle {
            section_key: "SSE_CLEAR_RULE".to_string(),
        });
    }
    None
}

fn detect_ws_pill(app: &App, x: u16, line_idx: usize) -> Option<ClickRegion> {
    let (pill_line, header_w) = app.layout.ws_pill_line?;
    if line_idx != pill_line {
        return None;
    }
    let click_x = (x.saturating_sub(app.layout.net_detail_x + 1)) as usize;
    let chat_start = header_w;
    let chat_end = chat_start + WS_CHAT_PILL.len();
    let raw_start = chat_end + PILL_PADDING;
    let raw_end = raw_start + WS_LIST_PILL.len();
    if click_x >= chat_start && click_x < chat_end {
        return Some(ClickRegion::NetworkDetailWsChatPill);
    }
    if click_x >= raw_start && click_x < raw_end {
        return Some(ClickRegion::NetworkDetailSectionToggle {
            section_key: "WS_RAW_EXIT".to_string(),
        });
    }
    None
}
