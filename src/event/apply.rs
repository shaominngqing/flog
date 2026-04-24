//! Mutation phase of the two-phase mouse dispatch (Phase 3 UI-041 step 3).
//!
//! `apply_click_region(app, region, class)` takes the semantic output of
//! `detect::detect_click_region` and performs all side-effects that the
//! original `handle_normal_mouse` executed inline.
//!
//! Each match arm mirrors one branch of the original handler so the net
//! behavior is preserved (verified by the 108 mouse characterization
//! tests in Task 5 once the dispatcher is rewired).

use crate::app::{App, InputField, ViewTab};

use super::click_region::{ClickClass, ClickRegion};

/// Perform all mutations for a detected click region.
pub(crate) fn apply_click_region(
    app: &mut App,
    region: ClickRegion,
    class: ClickClass,
    x: u16,
    y: u16,
) {
    match region {
        // ── Device picker overlay ─────────────────────────────────────
        ClickRegion::DevicePickerOutside => {
            app.show_device_picker = false;
        }
        ClickRegion::DevicePickerItem { index } => {
            if let Some(app_id) = app.layout.device_picker_item_ids.get(index) {
                let id = app_id.clone();
                if let Some(ref tx) = app.connect_device_tx {
                    let _ = tx.send(id);
                }
                app.show_device_picker = false;
            }
        }
        ClickRegion::DevicePickerScroll { direction } => {
            use super::click_region::ScrollDir;
            match direction {
                ScrollDir::Up => {
                    app.device_picker_scroll = app.device_picker_scroll.saturating_sub(1);
                }
                ScrollDir::Down => {
                    let max = app.layout.device_picker_total_lines.saturating_sub(1);
                    if app.device_picker_scroll < max {
                        app.device_picker_scroll += 1;
                    }
                }
            }
        }

        // ── Tab bar ────────────────────────────────────────────────────
        ClickRegion::LogsTab => app.switch_tab(ViewTab::Logs),
        ClickRegion::NetworkTab => app.switch_tab(ViewTab::Network),

        // ── Logs toolbar ───────────────────────────────────────────────
        ClickRegion::LogsToolbarLevel(level) => app.set_level(level),
        ClickRegion::LogsToolbarSearch => app.enter_input_field(InputField::LogSearch),
        ClickRegion::LogsToolbarTag => app.enter_input_field(InputField::LogTag),
        ClickRegion::LogsToolbarExclude => app.enter_input_field(InputField::LogExclude),

        // ── Logs list / bottom ─────────────────────────────────────────
        ClickRegion::LogsListRow { row } => apply_logs_list_row(app, row, class),
        ClickRegion::LogsJumpToBottom => app.go_bottom(),

        // ── Logs detail panel ──────────────────────────────────────────
        ClickRegion::LogsDetailPanel { line_idx, .. } => {
            // JSON fold toggle in detail viewer. line_idx is the content
            // row (header already subtracted by detect).
            if let Some(Some(node_id)) = app.detail.viewer_click_map.get(line_idx).copied() {
                app.toggle_detail_fold(node_id);
            }
        }
        ClickRegion::LogsDetailClose => {
            // detect collapses copy-pill onto this variant. Copy it.
            if let Some(idx) = app.selected_store_index() {
                if let Some(entry) = app.store.get(idx) {
                    let text = entry.full_message();
                    let msg = super::actions::copy_to_clipboard(&text);
                    app.show_status(msg);
                }
            }
        }

        // ── Network toolbar ────────────────────────────────────────────
        ClickRegion::NetworkToolbarSearch => app.enter_input_field(InputField::NetSearch),
        ClickRegion::NetworkToolbarExclude => app.enter_input_field(InputField::NetExclude),
        ClickRegion::NetworkProtocolPill(protocol) => {
            app.network.filter.protocol = protocol;
            app.network.invalidate_filter();
        }
        ClickRegion::NetworkMethodPill(method) => {
            app.network.filter.method = method;
            app.network.invalidate_filter();
        }
        ClickRegion::NetworkStatusPill(status) => {
            app.network.filter.status = status;
            app.network.invalidate_filter();
        }
        ClickRegion::NetworkMockRulesBtn => app.enter_mock_rules(),

        // ── Network list / detail ──────────────────────────────────────
        ClickRegion::NetworkListRow { row } => apply_network_list_row(app, row),
        ClickRegion::NetworkDetailPanel { line_idx, .. } => {
            // Generic: treat as a section toggle fallthrough.
            if let Some(Some(section_key)) = app.network.detail_section_map.get(line_idx) {
                let key = section_key.clone();
                if app.network.collapsed_sections.contains(&key) {
                    app.network.collapsed_sections.remove(&key);
                } else {
                    app.network.collapsed_sections.insert(key);
                }
            }
        }
        ClickRegion::NetworkDetailSseEventsPill => {
            app.network.sse_merged_mode = false;
        }
        ClickRegion::NetworkDetailSseMergedPill => apply_enter_sse_merged_mode(app),
        ClickRegion::NetworkDetailSseFieldPill { idx } => apply_sse_field_selection(app, idx),
        ClickRegion::NetworkDetailWsChatPill => {
            app.network.ws_chat_mode = true;
        }
        ClickRegion::NetworkDetailSectionToggle { section_key } => {
            apply_network_detail_section_toggle(app, &section_key);
        }
        ClickRegion::NetworkDetailMockBtn => super::actions::mock_from_selected(app),
        ClickRegion::NetworkDetailReplayBtn => super::actions::replay_selected(app),
        ClickRegion::NetworkDetailClose => {
            app.network.show_detail = false;
        }

        // ── Mock rules panel ───────────────────────────────────────────
        ClickRegion::MockRuleRow { index } => apply_mock_rule_row(app, index, class),
        ClickRegion::MockRuleEditBtn { index } => {
            if let Some(rule) = app.mock_rules.rules().get(index) {
                let id = rule.id;
                app.enter_mock_edit(id);
            }
        }
        ClickRegion::MockRuleToggle { index } => {
            if let Some(rule) = app.mock_rules.rules().get(index) {
                let id = rule.id;
                app.mock_rules.toggle(id);
                super::actions::trigger_mock_sync(app);
            }
        }
        ClickRegion::MockRuleDelete { index } => {
            if let Some(rule) = app.mock_rules.rules().get(index) {
                let id = rule.id;
                app.mock_rules.remove(id);
                if app.mock_rule_selected >= app.mock_rules.len() && app.mock_rule_selected > 0 {
                    app.mock_rule_selected -= 1;
                }
                super::actions::trigger_mock_sync(app);
            }
        }
        ClickRegion::MockRuleAdd => {}
        ClickRegion::MockRuleClose => {
            app.network.show_mock_rules_panel = false;
        }

        // ── Status bar / misc ──────────────────────────────────────────
        ClickRegion::StatusBar => apply_status_bar(app, x, y),
        ClickRegion::Scrollbar { .. } => {}
    }
}

/// Status-bar click handler. Delegates to the shared
/// `apply::status_bar` module.
fn apply_status_bar(app: &mut App, x: u16, y: u16) {
    super::apply_status::handle(app, x, y);
}

// ─────────────────────────────────────────────────────────────────────
//  Helpers
// ─────────────────────────────────────────────────────────────────────

fn apply_logs_list_row(app: &mut App, row: u16, _class: ClickClass) {
    let y = app.layout.list_y + row;
    super::handle_list_click_public(app, y);
}

fn apply_network_list_row(app: &mut App, row: u16) {
    let row_in_list = row as usize;
    let target = app.network.scroll_offset + row_in_list;
    let count = app.network.filtered_count(&app.network_store);
    if target >= count {
        return;
    }
    app.network.auto_scroll = false;
    if app.network.selected == target && !app.network.show_mock_rules_panel {
        app.network.show_detail = !app.network.show_detail;
        app.network.detail_scroll = 0;
        app.network.collapsed_sections.clear();
        app.network.json_viewer_states.clear();
        sync_sse_merged_mode_for_selection(app, target);
    } else {
        app.network.selected = target;
        app.network.show_detail = true;
        app.network.show_mock_rules_panel = false;
        app.network.detail_scroll = 0;
        app.network.collapsed_sections.clear();
        app.network.json_viewer_states.clear();
        sync_sse_merged_mode_for_selection(app, target);
    }
}

fn sync_sse_merged_mode_for_selection(app: &mut App, target: usize) {
    let indices_vec = app.network.filtered_indices(&app.network_store).to_vec();
    let Some(&new_idx) = indices_vec.get(target) else {
        return;
    };
    let Some(entry) = app.network_store.get(new_idx) else {
        return;
    };
    if entry.protocol != crate::domain::network::Protocol::Sse {
        app.network.sse_merged_mode = false;
        return;
    }
    let rule_key = entry
        .path
        .split('?')
        .next()
        .unwrap_or(&entry.path)
        .to_string();
    app.network.sse_merged_mode = app.network.sse_merge_rules.contains_key(&rule_key);
    if app.network.sse_merged_mode {
        if let Some(rule) = app.network.sse_merge_rules.get(&rule_key) {
            let chunks_data: Vec<&str> = entry.sse_chunks.iter().map(|c| c.data.as_str()).collect();
            let candidates = crate::domain::sse_merge::extract_field_paths(&chunks_data);
            app.network.sse_merged_field_idx = candidates
                .iter()
                .position(|(_, d)| d == &rule.field_display)
                .unwrap_or(0);
        }
    }
}

fn apply_enter_sse_merged_mode(app: &mut App) {
    let sel = app.network.selected;
    let indices = app.network.filtered_indices(&app.network_store).to_vec();
    let Some(&idx) = indices.get(sel) else {
        return;
    };
    let Some(entry) = app.network_store.get(idx) else {
        return;
    };
    if entry.protocol != crate::domain::network::Protocol::Sse {
        return;
    }
    let rule_key = entry
        .path
        .split('?')
        .next()
        .unwrap_or(&entry.path)
        .to_string();
    let chunks_data: Vec<&str> = entry.sse_chunks.iter().map(|c| c.data.as_str()).collect();
    if !app.network.sse_merge_rules.contains_key(&rule_key) {
        if let Some((path, display)) = crate::domain::sse_merge::auto_detect_field(&chunks_data) {
            app.network.sse_merge_rules.insert(
                rule_key.clone(),
                crate::app::SseMergeRule {
                    field_path: path,
                    field_display: display,
                },
            );
        }
    }
    app.network.sse_merged_mode = true;
    if let Some(rule) = app.network.sse_merge_rules.get(&rule_key) {
        let candidates = crate::domain::sse_merge::extract_field_paths(&chunks_data);
        app.network.sse_merged_field_idx = candidates
            .iter()
            .position(|(_, d)| d == &rule.field_display)
            .unwrap_or(0);
    } else {
        app.network.sse_merged_field_idx = 0;
    }
}

/// Select a specific SSE merged field by candidate index and rebuild
/// the merge rule so the renderer concatenates the new field.
///
/// Shared by mouse pill click (`NetworkDetailSseFieldPill`) and j/k
/// navigation (Task 6).
pub(crate) fn apply_sse_field_selection(app: &mut App, new_idx: usize) {
    app.network.sse_merged_field_idx = new_idx;
    let sel = app.network.selected;
    let indices = app.network.filtered_indices(&app.network_store).to_vec();
    let Some(&store_idx) = indices.get(sel) else {
        return;
    };
    let Some(entry) = app.network_store.get(store_idx) else {
        return;
    };
    let rule_key = entry
        .path
        .split('?')
        .next()
        .unwrap_or(&entry.path)
        .to_string();
    let chunks_data: Vec<&str> = entry.sse_chunks.iter().map(|c| c.data.as_str()).collect();
    let candidates = crate::domain::sse_merge::extract_field_paths(&chunks_data);
    if let Some((path, display)) = candidates.into_iter().nth(new_idx) {
        app.network.sse_merge_rules.insert(
            rule_key,
            crate::app::SseMergeRule {
                field_path: path,
                field_display: display,
            },
        );
    }
}

fn apply_network_detail_section_toggle(app: &mut App, section_key: &str) {
    match section_key {
        "SSE_CLEAR_RULE" => {
            let sel = app.network.selected;
            let indices = app.network.filtered_indices(&app.network_store).to_vec();
            if let Some(&store_idx) = indices.get(sel) {
                if let Some(entry) = app.network_store.get(store_idx) {
                    let rule_key = entry
                        .path
                        .split('?')
                        .next()
                        .unwrap_or(&entry.path)
                        .to_string();
                    app.network.sse_merge_rules.remove(&rule_key);
                    app.network.sse_merged_mode = false;
                }
            }
        }
        "WS_RAW_EXIT" => {
            app.network.ws_chat_mode = false;
        }
        key if key.starts_with("JSON#") => {
            // Synthetic encoding from detect: JSON#<section_key>#<node_id>.
            let rest = &key["JSON#".len()..];
            if let Some((section_key_part, node_id_str)) = rest.rsplit_once('#') {
                if let Ok(node_id) = node_id_str.parse::<u32>() {
                    if let Some(state) = app.network.json_viewer_states.get_mut(section_key_part) {
                        if let Some(slot) = state.expanded.get_mut(node_id as usize) {
                            *slot = !*slot;
                        }
                    }
                }
            }
        }
        _ => {
            let key = section_key.to_string();
            if app.network.collapsed_sections.contains(&key) {
                app.network.collapsed_sections.remove(&key);
            } else {
                app.network.collapsed_sections.insert(key);
            }
        }
    }
}

fn apply_mock_rule_row(app: &mut App, row_idx: usize, class: ClickClass) {
    app.mock_rule_selected = row_idx;
    if matches!(class, ClickClass::Double) {
        if let Some(rule) = app.mock_rules.rules().get(row_idx) {
            let id = rule.id;
            app.enter_mock_edit(id);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::domain::network_filter::ProtocolFilter;
    use std::time::{Duration, Instant};

    use super::super::detect::classify_click;

    // ── classify_click ────────────────────────────────────────────────

    #[test]
    fn classify_click_without_prev_is_single() {
        let now = Instant::now();
        assert_eq!(classify_click(now, 10, 10, None), ClickClass::Single);
    }

    #[test]
    fn classify_click_same_coords_recent_is_double() {
        let now = Instant::now();
        let prev = now.checked_sub(Duration::from_millis(50)).unwrap();
        assert_eq!(
            classify_click(now, 10, 10, Some((prev, 10, 10))),
            ClickClass::Double
        );
    }

    #[test]
    fn classify_click_timeout_is_single() {
        let now = Instant::now();
        let prev = now.checked_sub(Duration::from_millis(2000)).unwrap();
        assert_eq!(
            classify_click(now, 10, 10, Some((prev, 10, 10))),
            ClickClass::Single
        );
    }

    #[test]
    fn classify_click_different_coords_is_single() {
        let now = Instant::now();
        let prev = now.checked_sub(Duration::from_millis(50)).unwrap();
        assert_eq!(
            classify_click(now, 10, 10, Some((prev, 20, 30))),
            ClickClass::Single
        );
    }

    // ── apply_click_region ────────────────────────────────────────────

    #[test]
    fn apply_logs_tab_switches_tab() {
        let mut app = App::default();
        app.active_tab = ViewTab::Network;
        apply_click_region(&mut app, ClickRegion::LogsTab, ClickClass::Single, 0, 0);
        assert_eq!(app.active_tab, ViewTab::Logs);
    }

    #[test]
    fn apply_device_picker_outside_closes_picker() {
        let mut app = App::default();
        app.show_device_picker = true;
        apply_click_region(
            &mut app,
            ClickRegion::DevicePickerOutside,
            ClickClass::Single,
            0,
            0,
        );
        assert!(!app.show_device_picker);
    }

    #[test]
    fn apply_protocol_pill_updates_filter() {
        let mut app = App::default();
        apply_click_region(
            &mut app,
            ClickRegion::NetworkProtocolPill(ProtocolFilter::Http),
            ClickClass::Single,
            0,
            0,
        );
        assert_eq!(app.network.filter.protocol, ProtocolFilter::Http);
    }

    // ── apply_sse_field_selection ─────────────────────────────────────

    fn seed_sse_entry(app: &mut App) {
        use crate::domain::network::{NetworkEntry, Protocol, SseChunk};

        let mut entry =
            NetworkEntry::new_http(1, "GET".into(), "https://x.test/sse".into(), "t".into());
        entry.protocol = Protocol::Sse;
        entry.sse_chunks.push(SseChunk {
            data: "{\"alpha\": \"one\", \"beta\": \"two\"}".into(),
        });
        entry.sse_chunks.push(SseChunk {
            data: "{\"alpha\": \"three\", \"beta\": \"four\"}".into(),
        });
        app.network_store.push_entry(entry);
        app.network.invalidate_filter();
        let _ = app.network.filtered_count(&app.network_store);
        app.network.selected = 0;
    }

    #[test]
    fn apply_sse_field_selection_updates_field_idx() {
        let mut app = App::default();
        seed_sse_entry(&mut app);
        apply_sse_field_selection(&mut app, 1);
        assert_eq!(app.network.sse_merged_field_idx, 1);
    }

    #[test]
    fn apply_sse_field_selection_rebuilds_merge_rule() {
        let mut app = App::default();
        seed_sse_entry(&mut app);
        apply_sse_field_selection(&mut app, 0);
        // Rule should now exist for the "/sse" URL path key.
        assert!(
            app.network
                .sse_merge_rules
                .keys()
                .any(|k| k.ends_with("/sse")),
            "expected merge rule for /sse after selection; got keys: {:?}",
            app.network.sse_merge_rules.keys().collect::<Vec<_>>()
        );
    }
}
