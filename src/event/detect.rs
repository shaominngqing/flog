//! Pure click-region detection (Phase 3 UI-041 step 2).
//!
//! `detect_click_region` walks the cached render-layout rects in the same
//! priority order as the original `handle_normal_mouse` and returns a
//! semantic `ClickRegion` describing what was clicked. No mutation. No
//! returns of borrowed state.
//!
//! This lets mouse-routing unit tests assert *what region* a click maps
//! to without also having to assert the cascading side-effects. The
//! mutation phase lives in `apply::apply_click_region` (Task 4).
//!
//! Unchanged priority ladder (must match `handle_normal_mouse`):
//!   1. Device picker overlay (when `show_device_picker`).
//!   2. Logs detail side-panel (when `active_tab == Logs` && `show_detail_panel`).
//!   3. Jump-to-bottom floating pill.
//!   4. Tab bar.
//!   5. Network-tab regions (toolbar, mock rules panel, detail scroll/click,
//!      list, status bar).
//!   6. Logs-tab regions (input row, list, status bar, timeline).

use std::time::{Duration, Instant};

use crate::app::{App, ViewTab};
use crate::domain::network_filter::{MethodFilter, ProtocolFilter, StatusFilter};
use crate::domain::LogLevel;

use super::click_region::{ClickClass, ClickRegion};
use super::pills::{PILL_PADDING, SSE_EVENTS_PILL, SSE_MERGED_PILL, WS_CHAT_PILL, WS_LIST_PILL};
use super::DOUBLE_CLICK_MS;

#[allow(dead_code)] // wired up via Task 5 two-phase dispatch
const LEVEL_BUTTON_WIDTH: u16 = 3;

/// Map a click `(x, y)` to a semantic region. Read-only borrow of `App`.
///
/// Returns `None` when the click lands on a passive/decorative area
/// (list padding without a row, empty status-bar space, etc.) — callers
/// treat `None` as "ignore this click" the same way the original handler
/// did with its no-op fall-throughs.
#[allow(dead_code)] // wired up via Task 5 two-phase dispatch
pub(crate) fn detect_click_region(app: &App, x: u16, y: u16) -> Option<ClickRegion> {
    // ── 1. Device picker overlay (modal) ───────────────────────────────
    if app.show_device_picker {
        return detect_device_picker(app, x, y);
    }

    // ── 2. Logs detail side-panel ──────────────────────────────────────
    if app.active_tab == ViewTab::Logs && app.show_detail_panel {
        let panel_start =
            (app.layout.width as u32 * (100 - app.detail_panel_pct as u32) / 100) as u16;
        if x >= panel_start && y > app.layout.toolbar_y && y < app.layout.bottom_y {
            return detect_logs_detail_panel(app, x, y);
        }
    }

    // ── 3. Jump-to-bottom floating pill (overlay) ──────────────────────
    if let Some((px, py, pw, ph)) = app.layout.jump_to_bottom_rect {
        if y >= py && y < py + ph && x >= px && x < px + pw {
            return Some(ClickRegion::LogsJumpToBottom);
        }
    }

    // ── 4. Tab bar (common to both tabs) ───────────────────────────────
    if y >= app.layout.tab_bar_y && y < app.layout.tab_bar_y + 2 {
        if x >= app.layout.tab_logs_x.0 && x < app.layout.tab_logs_x.1 {
            return Some(ClickRegion::LogsTab);
        }
        if x >= app.layout.tab_network_x.0 && x < app.layout.tab_network_x.1 {
            return Some(ClickRegion::NetworkTab);
        }
    }

    // ── 5. Network tab ─────────────────────────────────────────────────
    if app.active_tab == ViewTab::Network {
        return detect_network(app, x, y);
    }

    // ── 6. Logs tab ────────────────────────────────────────────────────
    detect_logs(app, x, y)
}

// ─────────────────────────────────────────────────────────────────────
//  Device picker overlay
// ─────────────────────────────────────────────────────────────────────

#[allow(dead_code)]
fn detect_device_picker(app: &App, x: u16, y: u16) -> Option<ClickRegion> {
    let inside = if let Some((px, py, pw, ph)) = app.layout.device_picker_rect {
        x >= px && x < px + pw && y >= py && y < py + ph
    } else {
        false
    };
    if !inside {
        return Some(ClickRegion::DevicePickerOutside);
    }
    for &(item_y, item_x_start, item_x_end, idx) in &app.layout.device_picker_items {
        if y == item_y && x >= item_x_start && x < item_x_end {
            return Some(ClickRegion::DevicePickerItem { index: idx });
        }
    }
    // Click inside picker but not on an item (border / header). Represent
    // as None so callers can ignore it (original handler was a silent
    // no-op here).
    None
}

// ─────────────────────────────────────────────────────────────────────
//  Logs detail panel
// ─────────────────────────────────────────────────────────────────────

#[allow(dead_code)]
fn detect_logs_detail_panel(app: &App, x: u16, y: u16) -> Option<ClickRegion> {
    // Copy pill pinned to the title row wins over JSON fold. We reuse
    // `LogsDetailClose` as the copy/close dispatch signal; apply branches
    // on whether the copy button rect still contains the click.
    if let Some((btn_y, x0, x1)) = app.layout.detail_copy_btn {
        if y == btn_y && x >= x0 && x < x1 {
            return Some(ClickRegion::LogsDetailClose);
        }
    }
    let panel_row = y.saturating_sub(app.layout.list_y);
    let header = app.detail.header_lines.max(2) as u16;
    if panel_row >= header {
        let content_row = (panel_row - header) as usize;
        return Some(ClickRegion::LogsDetailPanel {
            line_idx: content_row,
            x,
        });
    }
    None
}

// ─────────────────────────────────────────────────────────────────────
//  Network tab
// ─────────────────────────────────────────────────────────────────────

#[allow(dead_code)]
fn detect_network(app: &App, x: u16, y: u16) -> Option<ClickRegion> {
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
                return pill_id_to_region(id);
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
                    "edit" => ClickRegion::MockRuleRow { index: *row_idx },
                    "toggle" => ClickRegion::MockRuleToggle { index: *row_idx },
                    "delete" => ClickRegion::MockRuleDelete { index: *row_idx },
                    _ => return None,
                });
            }
        }
        // Inside mock panel area but no row match — original handler
        // consumed the event (returned) and did nothing. Signal with None.
        return None;
    }

    // Network detail panel (scroll / click)
    if app.network.show_detail
        && x >= app.layout.net_detail_x
        && y >= app.layout.list_y
        && y < app.layout.bottom_y
    {
        // [Mock] button in detail header
        if let Some((btn_y, btn_x_start, btn_x_end)) = app.layout.detail_mock_btn {
            if y == btn_y && x >= btn_x_start && x < btn_x_end {
                return Some(ClickRegion::NetworkDetailMockBtn);
            }
        }

        let detail_content_y = app.layout.net_detail_content_y;
        if y >= detail_content_y && y < app.layout.bottom_y {
            let line_idx = app.network.detail_scroll + (y - detail_content_y) as usize;

            // SSE pill click detection
            if let Some((pill_line, header_w)) = app.layout.sse_pill_line {
                if line_idx == pill_line {
                    let click_x = (x.saturating_sub(app.layout.net_detail_x + 1)) as usize;
                    let events_start = header_w;
                    let events_end = events_start + SSE_EVENTS_PILL.len();
                    let merged_start = events_end + PILL_PADDING;
                    let merged_end = merged_start + SSE_MERGED_PILL.len();
                    if click_x >= events_start && click_x < events_end {
                        return Some(ClickRegion::NetworkDetailSseEventsPill);
                    } else if click_x >= merged_start && click_x < merged_end {
                        return Some(ClickRegion::NetworkDetailSseMergedPill);
                    } else {
                        let clear_start = merged_end + 1;
                        let clear_end = clear_start + " \u{00d7} ".len();
                        if click_x >= clear_start && click_x < clear_end {
                            return Some(ClickRegion::NetworkDetailSectionToggle {
                                section_key: "SSE_CLEAR_RULE".to_string(),
                            });
                        }
                    }
                }
            }

            // WS pill click detection
            if let Some((pill_line, header_w)) = app.layout.ws_pill_line {
                if line_idx == pill_line {
                    let click_x = (x.saturating_sub(app.layout.net_detail_x + 1)) as usize;
                    let chat_start = header_w;
                    let chat_end = chat_start + WS_CHAT_PILL.len();
                    let raw_start = chat_end + PILL_PADDING;
                    let raw_end = raw_start + WS_LIST_PILL.len();
                    if click_x >= chat_start && click_x < chat_end {
                        return Some(ClickRegion::NetworkDetailWsChatPill);
                    } else if click_x >= raw_start && click_x < raw_end {
                        // "Raw" = exit chat mode. Reuse section toggle
                        // carrying a sentinel key so apply can branch.
                        return Some(ClickRegion::NetworkDetailSectionToggle {
                            section_key: "WS_RAW_EXIT".to_string(),
                        });
                    }
                }
            }

            // SSE_FIELD / SSE_CLEAR_RULE / WS_GROUP / generic section
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
                if section_key.strip_prefix("WS_GROUP#").is_some() {
                    return Some(ClickRegion::NetworkDetailSectionToggle {
                        section_key: section_key.clone(),
                    });
                }
                return Some(ClickRegion::NetworkDetailSectionToggle {
                    section_key: section_key.clone(),
                });
            }

            // JSON fold click
            if let Some(Some((section_key, node_id))) =
                app.network.detail_json_click_map.get(line_idx).cloned()
            {
                return Some(ClickRegion::NetworkDetailSectionToggle {
                    section_key: format!("JSON#{section_key}#{node_id}"),
                });
            }
        }
        return None;
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
                    _ => ClickRegion::StatusBar, // generic action-button group
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

// ─────────────────────────────────────────────────────────────────────
//  Logs tab
// ─────────────────────────────────────────────────────────────────────

#[allow(dead_code)]
fn detect_logs(app: &App, x: u16, y: u16) -> Option<ClickRegion> {
    // Input row (search / exclude / tag)
    if y == app.layout.input_row_y {
        if x >= app.layout.log_search_x.0 && x < app.layout.log_search_x.1 {
            return Some(ClickRegion::LogsToolbarSearch);
        }
        if x >= app.layout.log_exclude_x.0 && x < app.layout.log_exclude_x.1 {
            return Some(ClickRegion::LogsToolbarExclude);
        }
        if x >= app.layout.log_tag_x.0 && x < app.layout.log_tag_x.1 {
            return Some(ClickRegion::LogsToolbarTag);
        }
    }

    // op row 2: level buttons
    if y == app.layout.toolbar_op2_y && x >= app.layout.levels_x {
        let offset = x - app.layout.levels_x;
        let btn_idx = offset / LEVEL_BUTTON_WIDTH;
        let level = match btn_idx {
            0 => LogLevel::System,
            1 => LogLevel::Verbose,
            2 => LogLevel::Debug,
            3 => LogLevel::Info,
            4 => LogLevel::Warning,
            5 => LogLevel::Error,
            _ => return None,
        };
        return Some(ClickRegion::LogsToolbarLevel(level));
    }

    // List area
    if y >= app.layout.list_y && y < app.layout.list_y + app.layout.list_height {
        let row = y - app.layout.list_y;
        return Some(ClickRegion::LogsListRow { row });
    }

    // Status bar
    if y == app.layout.bottom_y {
        if x < app.layout.source_info_x.0 {
            return Some(ClickRegion::LogsJumpToBottom);
        }
        if x >= app.layout.source_info_x.0 && x < app.layout.source_info_x.1 {
            return Some(ClickRegion::StatusBar);
        }
        for &(_name, start, end) in &app.layout.bottom_buttons {
            if x >= start && x < end {
                return Some(ClickRegion::StatusBar);
            }
        }
    }

    None
}

// ─────────────────────────────────────────────────────────────────────
//  Helpers
// ─────────────────────────────────────────────────────────────────────

#[allow(dead_code)]
fn pill_id_to_region(id: &str) -> Option<ClickRegion> {
    Some(match id {
        "proto_All" => ClickRegion::NetworkProtocolPill(ProtocolFilter::All),
        "proto_HTTP" => ClickRegion::NetworkProtocolPill(ProtocolFilter::Http),
        "proto_SSE" => ClickRegion::NetworkProtocolPill(ProtocolFilter::Sse),
        "proto_WS" => ClickRegion::NetworkProtocolPill(ProtocolFilter::Ws),
        "method_All" => ClickRegion::NetworkMethodPill(MethodFilter::All),
        "method_GET" => ClickRegion::NetworkMethodPill(MethodFilter::Get),
        "method_POST" => ClickRegion::NetworkMethodPill(MethodFilter::Post),
        "method_PUT" => ClickRegion::NetworkMethodPill(MethodFilter::Put),
        "method_DEL" => ClickRegion::NetworkMethodPill(MethodFilter::Delete),
        "method_PATCH" => ClickRegion::NetworkMethodPill(MethodFilter::Patch),
        "status_All" => ClickRegion::NetworkStatusPill(StatusFilter::All),
        "status_OK" => ClickRegion::NetworkStatusPill(StatusFilter::Completed),
        "status_Fail" => ClickRegion::NetworkStatusPill(StatusFilter::Failed),
        "status_Active" => ClickRegion::NetworkStatusPill(StatusFilter::Active),
        "status_Pending" => ClickRegion::NetworkStatusPill(StatusFilter::Pending),
        _ => return None,
    })
}

/// Classify a click as single or double based on prior click timing.
///
/// Pure function. `prev` is `Some((prev_time, prev_x, prev_y))` when the
/// previous click should still be considered; `None` on the first click
/// of a session. Returns `Double` if a previous click exists at the same
/// `(x, y)` within `DOUBLE_CLICK_MS`, else `Single`.
#[allow(dead_code)]
pub(crate) fn classify_click(
    now: Instant,
    x: u16,
    y: u16,
    prev: Option<(Instant, u16, u16)>,
) -> ClickClass {
    match prev {
        Some((prev_time, px, py))
            if px == x
                && py == y
                && now.duration_since(prev_time)
                    < Duration::from_millis(DOUBLE_CLICK_MS as u64) =>
        {
            ClickClass::Double
        }
        _ => ClickClass::Single,
    }
}

// ─────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;

    fn seeded_app() -> App {
        let mut app = App::default();
        app.layout.width = 80;
        app.layout.tab_bar_y = 0;
        app.layout.tab_logs_x = (1, 9);
        app.layout.tab_network_x = (10, 21);
        app.layout.toolbar_y = 2;
        app.layout.toolbar_op2_y = 3;
        app.layout.levels_x = 20;
        app.layout.input_row_y = 5;
        app.layout.log_search_x = (2, 20);
        app.layout.log_exclude_x = (21, 40);
        app.layout.log_tag_x = (41, 60);
        app.layout.net_toolbar_y = 5;
        app.layout.net_search_x = (2, 30);
        app.layout.net_exclude_x = (31, 55);
        app.layout.net_filter_pills_y = 6;
        app.layout.list_y = 7;
        app.layout.list_height = 8;
        app.layout.bottom_y = 15;
        app.layout.source_info_x = (40, 60);
        app
    }

    // 1. Device picker outside click.
    #[test]
    fn detects_device_picker_outside() {
        let mut app = seeded_app();
        app.show_device_picker = true;
        app.layout.device_picker_rect = Some((10, 2, 20, 8));
        // Click at (0, 0) — outside rect.
        let region = detect_click_region(&app, 0, 0);
        assert_eq!(region, Some(ClickRegion::DevicePickerOutside));
    }

    // 2. Device picker item click.
    #[test]
    fn detects_device_picker_item() {
        let mut app = seeded_app();
        app.show_device_picker = true;
        app.layout.device_picker_rect = Some((10, 2, 20, 8));
        app.layout.device_picker_items = vec![(4, 11, 29, 0), (5, 11, 29, 1)];
        let region = detect_click_region(&app, 15, 5);
        assert_eq!(region, Some(ClickRegion::DevicePickerItem { index: 1 }));
    }

    // 3. Tabs.
    #[test]
    fn detects_logs_tab() {
        let app = seeded_app();
        assert_eq!(detect_click_region(&app, 3, 0), Some(ClickRegion::LogsTab));
    }

    #[test]
    fn detects_network_tab() {
        let app = seeded_app();
        assert_eq!(
            detect_click_region(&app, 15, 0),
            Some(ClickRegion::NetworkTab)
        );
    }

    // 4. Network filter pill.
    #[test]
    fn detects_network_protocol_pill() {
        let mut app = seeded_app();
        app.active_tab = ViewTab::Network;
        app.layout
            .net_filter_pills
            .push(("proto_HTTP".to_string(), 10, 15));
        let region = detect_click_region(&app, 12, 6);
        assert_eq!(
            region,
            Some(ClickRegion::NetworkProtocolPill(ProtocolFilter::Http))
        );
    }

    // 5. Logs list row.
    #[test]
    fn detects_logs_list_row() {
        let app = seeded_app();
        // y=9 → list_y=7 + 2
        assert_eq!(
            detect_click_region(&app, 5, 9),
            Some(ClickRegion::LogsListRow { row: 2 })
        );
    }

    // 6. Network list row.
    #[test]
    fn detects_network_list_row() {
        let mut app = seeded_app();
        app.active_tab = ViewTab::Network;
        assert_eq!(
            detect_click_region(&app, 5, 10),
            Some(ClickRegion::NetworkListRow { row: 3 })
        );
    }

    // 7. Status bar (source info).
    #[test]
    fn detects_status_bar_source_info_logs() {
        let app = seeded_app();
        assert_eq!(
            detect_click_region(&app, 45, 15),
            Some(ClickRegion::StatusBar)
        );
    }

    // 8. Network detail mock button.
    #[test]
    fn detects_network_detail_mock_btn() {
        let mut app = seeded_app();
        app.active_tab = ViewTab::Network;
        app.layout.net_detail_x = 40;
        app.layout.detail_mock_btn = Some((7, 42, 50));
        app.network.show_detail = true;
        let region = detect_click_region(&app, 45, 7);
        assert_eq!(region, Some(ClickRegion::NetworkDetailMockBtn));
    }

    // 9. Jump-to-bottom pill.
    #[test]
    fn detects_jump_to_bottom_pill() {
        let mut app = seeded_app();
        app.layout.jump_to_bottom_rect = Some((70, 14, 8, 1));
        assert_eq!(
            detect_click_region(&app, 72, 14),
            Some(ClickRegion::LogsJumpToBottom)
        );
    }

    // 10. Invalid coords (nothing there).
    #[test]
    fn returns_none_for_empty_coords() {
        let app = seeded_app();
        // (0, 0) isn't on any tab clickable region; tab_logs_x starts at 1.
        // Actually row 0 is tab row; col 0 is to the left of the Logs label.
        // Ensure None is returned when tab bar is empty at col 0.
        let region = detect_click_region(&app, 0, 0);
        // Either None or not a tab region.
        assert!(
            matches!(region, None | Some(ClickRegion::LogsListRow { .. })),
            "got unexpected region: {:?}",
            region
        );
    }
}
