use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use crate::app::{App, AppMode, ViewTab};
use crate::domain::network_filter::{MethodFilter, ProtocolFilter, StatusFilter};
use crate::domain::LogLevel;

const SCROLL_LINES: usize = 3;
const LEVEL_BUTTON_WIDTH: u16 = 3;

const DOUBLE_CLICK_MS: u128 = 400;

pub fn handle_key(app: &mut App, key: KeyEvent) {
    match app.mode {
        AppMode::Normal => handle_normal_key(app, key),
        AppMode::Search => handle_search_key(app, key),
        AppMode::TagFilter => handle_filter_key(app, key),
        AppMode::Help | AppMode::Stats => handle_overlay_key(app, key),
        AppMode::MockRuleEdit => handle_mock_edit_key(app, key),
    }
}

pub fn handle_mouse(app: &mut App, mouse: MouseEvent) {
    match app.mode {
        AppMode::Normal => handle_normal_mouse(app, mouse),
        AppMode::Search | AppMode::TagFilter => handle_input_mouse(app, mouse),
        AppMode::Help | AppMode::Stats => handle_overlay_mouse(app, mouse),
        AppMode::MockRuleEdit => handle_mock_edit_mouse(app, mouse),
    }
}

// ══════════════════════════════════════
//  Normal mode — Mouse
// ══════════════════════════════════════

fn handle_normal_mouse(app: &mut App, mouse: MouseEvent) {
    // Handle device picker overlay
    if app.show_device_picker {
        match mouse.kind {
            crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                let x = mouse.column;
                let y = mouse.row;

                // First check if click is inside the picker rect
                let inside = if let Some((px, py, pw, ph)) = app.layout.device_picker_rect {
                    x >= px && x < px + pw && y >= py && y < py + ph
                } else {
                    false
                };

                if !inside {
                    // Click outside → close
                    app.show_device_picker = false;
                    return;
                }

                // Check click on device items
                for &(item_y, item_x_start, item_x_end, idx) in &app.layout.device_picker_items {
                    if y == item_y && x >= item_x_start && x < item_x_end {
                        if let Some(device_id) = app.layout.device_picker_item_ids.get(idx) {
                            let id = device_id.clone();
                            if let Some(ref tx) = app.connect_device_tx {
                                let _ = tx.send(id);
                            }
                            app.show_device_picker = false;
                        }
                        return;
                    }
                }
                // Click inside picker but not on item (e.g. border) — do nothing
            }
            crossterm::event::MouseEventKind::ScrollUp => {
                app.device_picker_scroll = app.device_picker_scroll.saturating_sub(1);
            }
            crossterm::event::MouseEventKind::ScrollDown => {
                app.device_picker_scroll += 1;
            }
            _ => {}
        }
        return;
    }

    // Check if click is in the detail side panel (Logs view only)
    if app.active_tab == ViewTab::Logs && app.show_detail_panel {
        let panel_start =
            (app.layout.width as u32 * (100 - app.detail_panel_pct as u32) / 100) as u16;
        if mouse.column >= panel_start
            && mouse.row > app.layout.toolbar_y
            && mouse.row < app.layout.timeline_y
        {
            handle_detail_panel_click(app, &mouse);
            return;
        }
    }

    // Tab bar click detection (common to both tabs)
    if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
        let y = mouse.row;
        let x = mouse.column;
        if y >= app.layout.tab_bar_y && y < app.layout.tab_bar_y + 2 {
            if x >= app.layout.tab_logs_x.0 && x < app.layout.tab_logs_x.1 {
                app.switch_tab(ViewTab::Logs);
                return;
            }
            if x >= app.layout.tab_network_x.0 && x < app.layout.tab_network_x.1 {
                app.switch_tab(ViewTab::Network);
                return;
            }
        }
    }

    // Network tab mouse handling
    if app.active_tab == ViewTab::Network {
        // Network toolbar click handling
        if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
            let x = mouse.column;
            let y = mouse.row;
            // Line 1: search
            if y == app.layout.net_toolbar_y
                && x >= app.layout.net_search_x.0
                && x < app.layout.net_search_x.1
            {
                app.network.search_active = true;
                app.network.search_input = app.network.filter.search.clone();
                return;
            }
            // Line 2: filter pills
            if y == app.layout.net_filter_pills_y {
                for (id, x_start, x_end) in &app.layout.net_filter_pills {
                    if x >= *x_start && x < *x_end {
                        match id.as_str() {
                            "proto_All" => app.network.filter.protocol = ProtocolFilter::All,
                            "proto_HTTP" => app.network.filter.protocol = ProtocolFilter::Http,
                            "proto_SSE" => app.network.filter.protocol = ProtocolFilter::Sse,
                            "proto_WS" => app.network.filter.protocol = ProtocolFilter::Ws,
                            "method_All" => app.network.filter.method = MethodFilter::All,
                            "method_GET" => app.network.filter.method = MethodFilter::Get,
                            "method_POST" => app.network.filter.method = MethodFilter::Post,
                            "method_PUT" => app.network.filter.method = MethodFilter::Put,
                            "method_DEL" => app.network.filter.method = MethodFilter::Delete,
                            "method_PATCH" => app.network.filter.method = MethodFilter::Patch,
                            "status_All" => app.network.filter.status = StatusFilter::All,
                            "status_OK" => app.network.filter.status = StatusFilter::Completed,
                            "status_Fail" => app.network.filter.status = StatusFilter::Failed,
                            "status_Active" => app.network.filter.status = StatusFilter::Active,
                            "status_Pending" => app.network.filter.status = StatusFilter::Pending,
                            _ => {}
                        }
                        app.network.invalidate_filter();
                        return;
                    }
                }
            }
        }

        // Mock rules panel click handling (when shown in right panel)
        if app.network.show_mock_rules_panel
            && mouse.column >= app.layout.net_detail_x
            && mouse.row >= app.layout.list_y
            && mouse.row < app.layout.bottom_y
        {
            if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
                let x = mouse.column;
                let y = mouse.row;
                for (row_idx, action, ry, x_start, x_end) in app.layout.mock_rule_regions.clone() {
                    if y == ry && x >= x_start && x < x_end {
                        match action.as_str() {
                            "select" => {
                                app.mock_rule_selected = row_idx;
                                // Double-click to edit
                                let now = Instant::now();
                                let is_double = if let Some((pt, px, py)) = app.layout.last_click {
                                    now.duration_since(pt).as_millis() < DOUBLE_CLICK_MS && px == x && py == y
                                } else { false };
                                app.layout.last_click = Some((now, x, y));
                                if is_double {
                                    if let Some(rule) = app.mock_rules.rules().get(row_idx) {
                                        let id = rule.id;
                                        app.enter_mock_edit(id);
                                    }
                                }
                            }
                            "edit" => {
                                if let Some(rule) = app.mock_rules.rules().get(row_idx) {
                                    let id = rule.id;
                                    app.enter_mock_edit(id);
                                }
                            }
                            "toggle" => {
                                if let Some(rule) = app.mock_rules.rules().get(row_idx) {
                                    let id = rule.id;
                                    app.mock_rules.toggle(id);
                                    trigger_mock_sync(app);
                                }
                            }
                            "delete" => {
                                if let Some(rule) = app.mock_rules.rules().get(row_idx) {
                                    let id = rule.id;
                                    app.mock_rules.remove(id);
                                    if app.mock_rule_selected >= app.mock_rules.len() && app.mock_rule_selected > 0 {
                                        app.mock_rule_selected -= 1;
                                    }
                                    trigger_mock_sync(app);
                                }
                            }
                            _ => {}
                        }
                        return;
                    }
                }
            }
            return; // consume all events in mock rules panel area
        }

        // Network detail scroll handling (must be checked before list scroll)
        if app.network.show_detail
            && mouse.column >= app.layout.net_detail_x
            && mouse.row >= app.layout.list_y
            && mouse.row < app.layout.bottom_y
        {
            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    app.network.detail_scroll =
                        app.network.detail_scroll.saturating_sub(SCROLL_LINES);
                    return;
                }
                MouseEventKind::ScrollDown => {
                    app.network.detail_scroll += SCROLL_LINES;
                    return;
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    let x = mouse.column;
                    let y = mouse.row;

                    // Check [Mock] button in detail header
                    if let Some((btn_y, btn_x_start, btn_x_end)) = app.layout.detail_mock_btn {
                        if y == btn_y && x >= btn_x_start && x < btn_x_end {
                            mock_from_selected(app);
                            return;
                        }
                    }

                    // Use the precise Y set by the detail renderer
                    let detail_content_y = app.layout.net_detail_content_y;
                    if y >= detail_content_y && y < app.layout.bottom_y {
                        let line_idx = app.network.detail_scroll + (y - detail_content_y) as usize;

                        // Check SSE pill clicks (Events/Merged toggle)
                        if let Some((pill_line, header_w)) = app.layout.sse_pill_line {
                            if line_idx == pill_line {
                                let click_x = (x.saturating_sub(app.layout.net_detail_x + 1)) as usize;
                                let events_start = header_w;
                                let events_end = events_start + " Events ".len();
                                let merged_start = events_end + 1;
                                let merged_end = merged_start + " Merged ".len();
                                if click_x >= events_start && click_x < events_end {
                                    app.network.sse_merged_mode = false;
                                    return;
                                } else if click_x >= merged_start && click_x < merged_end {
                                    // Create rule if not exists, then enter merged mode
                                    let sel = app.network.selected;
                                    let indices = app.network.filtered_indices(&app.network_store).to_vec();
                                    if let Some(&idx) = indices.get(sel) {
                                        if let Some(entry) = app.network_store.get(idx) {
                                            if entry.protocol == crate::domain::network::Protocol::Sse {
                                                let rule_key = entry.path.split('?').next().unwrap_or(&entry.path).to_string();
                                                let chunks_data: Vec<&str> = entry.sse_chunks.iter().map(|c| c.data.as_str()).collect();
                                                if !app.network.sse_merge_rules.contains_key(&rule_key) {
                                                    if let Some((path, display)) = crate::domain::sse_merge::auto_detect_field(&chunks_data) {
                                                        app.network.sse_merge_rules.insert(rule_key.clone(), crate::app::SseMergeRule {
                                                            field_path: path,
                                                            field_display: display,
                                                        });
                                                    }
                                                }
                                                app.network.sse_merged_mode = true;
                                                // Sync field_idx with the rule's field
                                                if let Some(rule) = app.network.sse_merge_rules.get(&rule_key) {
                                                    let candidates = crate::domain::sse_merge::extract_field_paths(&chunks_data);
                                                    app.network.sse_merged_field_idx = candidates.iter()
                                                        .position(|(_, d)| d == &rule.field_display)
                                                        .unwrap_or(0);
                                                } else {
                                                    app.network.sse_merged_field_idx = 0;
                                                }
                                            }
                                        }
                                    }
                                    return;
                                } else {
                                    // Check × (clear rule) pill — right after Merged pill
                                    let clear_start = merged_end + 1;
                                    let clear_end = clear_start + " \u{00d7} ".len();
                                    if click_x >= clear_start && click_x < clear_end {
                                        let sel = app.network.selected;
                                        let indices = app.network.filtered_indices(&app.network_store).to_vec();
                                        if let Some(&store_idx) = indices.get(sel) {
                                            if let Some(entry) = app.network_store.get(store_idx) {
                                                let rule_key = entry.path.split('?').next().unwrap_or(&entry.path).to_string();
                                                app.network.sse_merge_rules.remove(&rule_key);
                                                app.network.sse_merged_mode = false;
                                            }
                                        }
                                        return;
                                    }
                                }
                                // If click is not on pills, fall through to section toggle below
                            }
                        }

                        // Check WS pill clicks (Chat/Raw toggle)
                        if let Some((pill_line, header_w)) = app.layout.ws_pill_line {
                            if line_idx == pill_line {
                                let click_x = (x.saturating_sub(app.layout.net_detail_x + 1)) as usize;
                                let chat_start = header_w;
                                let chat_end = chat_start + " Chat ".len();
                                let raw_start = chat_end + 1;
                                let raw_end = raw_start + " Raw ".len();
                                if click_x >= chat_start && click_x < chat_end {
                                    app.network.ws_chat_mode = true;
                                    return;
                                } else if click_x >= raw_start && click_x < raw_end {
                                    app.network.ws_chat_mode = false;
                                    return;
                                }
                            }
                        }

                        // Check SSE-specific section keys before generic toggle
                        if let Some(Some(section_key)) = app.network.detail_section_map.get(line_idx) {
                            // Field selection in Merged mode
                            if let Some(idx_str) = section_key.strip_prefix("SSE_FIELD#") {
                                if let Ok(fi) = idx_str.parse::<usize>() {
                                    app.network.sse_merged_field_idx = fi;
                                    let sel = app.network.selected;
                                    let indices = app.network.filtered_indices(&app.network_store).to_vec();
                                    if let Some(&store_idx) = indices.get(sel) {
                                        if let Some(entry) = app.network_store.get(store_idx) {
                                            let rule_key = entry.path.split('?').next().unwrap_or(&entry.path).to_string();
                                            let chunks_data: Vec<&str> = entry.sse_chunks.iter().map(|c| c.data.as_str()).collect();
                                            {
                                                let candidates = crate::domain::sse_merge::extract_field_paths(&chunks_data);
                                                if let Some((path, display)) = candidates.into_iter().nth(fi) {
                                                    app.network.sse_merge_rules.insert(rule_key, crate::app::SseMergeRule {
                                                        field_path: path,
                                                        field_display: display,
                                                    });
                                                }
                                            }
                                        }
                                    }
                                    return;
                                }
                            }
                            // Clear Rule button
                            if section_key == "SSE_CLEAR_RULE" {
                                let sel = app.network.selected;
                                let indices = app.network.filtered_indices(&app.network_store).to_vec();
                                if let Some(&store_idx) = indices.get(sel) {
                                    if let Some(entry) = app.network_store.get(store_idx) {
                                        let rule_key = entry.path.split('?').next().unwrap_or(&entry.path).to_string();
                                        app.network.sse_merge_rules.remove(&rule_key);
                                        app.network.sse_merged_mode = false;
                                    }
                                }
                                return;
                            }
                            // WS group expand/collapse in Chat mode
                            if let Some(idx_str) = section_key.strip_prefix("WS_GROUP#") {
                                if idx_str.parse::<usize>().is_ok() {
                                    let key = section_key.clone();
                                    if app.network.collapsed_sections.contains(&key) {
                                        app.network.collapsed_sections.remove(&key);
                                    } else {
                                        app.network.collapsed_sections.insert(key);
                                    }
                                    return;
                                }
                            }
                        }

                        // First check section_line_map for section toggle
                        if let Some(Some(section_key)) =
                            app.network.detail_section_map.get(line_idx)
                        {
                            let key = section_key.clone();
                            if app.network.collapsed_sections.contains(&key) {
                                app.network.collapsed_sections.remove(&key);
                            } else {
                                app.network.collapsed_sections.insert(key);
                            }
                            return;
                        }
                        // Then check json_click_map for bracket toggle
                        if let Some(Some((section_key, source_line))) =
                            app.network.detail_json_click_map.get(line_idx)
                        {
                            if let Some(state) = app.network.json_viewer_states.get_mut(section_key)
                            {
                                crate::ui::json_viewer::toggle_fold(state, *source_line);
                            }
                            return;
                        }
                    }
                    return;
                }
                _ => {}
            }
        }

        match mouse.kind {
            MouseEventKind::ScrollUp => {
                app.network.move_up(SCROLL_LINES);
            }
            MouseEventKind::ScrollDown => {
                let count = app.network.filtered_count(&app.network_store);
                app.network.move_down(SCROLL_LINES, count);
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let y = mouse.row;
                let x = mouse.column;

                // Click on status bar buttons
                if y == app.layout.bottom_y {
                    for (name, x_start, x_end) in &app.layout.net_buttons {
                        if x >= *x_start && x < *x_end {
                            match name.as_str() {
                                "replay" => replay_selected(app),
                                "curl" => copy_as_curl(app),
                                "response" => copy_response(app),
                                "mock" => app.enter_mock_rules(),
                                // mockrules removed — rules show in side panel via Mock button
                                "stats" => app.enter_network_stats(),
                                "clear" => {
                                    app.network_store.clear();
                                    app.network.invalidate_filter();
                                    app.network.show_detail = false;
                                    app.show_status("Cleared".to_string());
                                }
                                "help" => app.enter_help(),
                                _ => {}
                            }
                            return;
                        }
                    }
                }

                // Click in list area (left half or full width)
                if y >= app.layout.list_y && y < app.layout.list_y + app.layout.list_height {
                    let row_in_list = (y - app.layout.list_y) as usize;
                    let target = app.network.scroll_offset + row_in_list;
                    let count = app.network.filtered_count(&app.network_store);
                    if target < count {
                        // Disable auto_scroll so renderer doesn't override selection
                        app.network.auto_scroll = false;
                        if app.network.selected == target && !app.network.show_mock_rules_panel {
                            app.network.show_detail = !app.network.show_detail;
                            app.network.detail_scroll = 0;
                            app.network.collapsed_sections.clear();
                            app.network.json_viewer_states.clear();
                            // Auto-enter merged mode if SSE entry has a rule
                            {
                                let indices_vec = app.network.filtered_indices(&app.network_store).to_vec();
                                if let Some(&new_idx) = indices_vec.get(target) {
                                    if let Some(entry) = app.network_store.get(new_idx) {
                                        if entry.protocol == crate::domain::network::Protocol::Sse {
                                            let rule_key = entry.path.split('?').next().unwrap_or(&entry.path).to_string();
                                            app.network.sse_merged_mode = app.network.sse_merge_rules.contains_key(&rule_key);
                                            if app.network.sse_merged_mode {
                                                if let Some(rule) = app.network.sse_merge_rules.get(&rule_key) {
                                                    let chunks_data: Vec<&str> = entry.sse_chunks.iter().map(|c| c.data.as_str()).collect();
                                                    let candidates = crate::domain::sse_merge::extract_field_paths(&chunks_data);
                                                    app.network.sse_merged_field_idx = candidates.iter()
                                                        .position(|(_, d)| d == &rule.field_display)
                                                        .unwrap_or(0);
                                                }
                                            }
                                        } else {
                                            app.network.sse_merged_mode = false;
                                        }
                                    }
                                }
                            }
                        } else {
                            app.network.selected = target;
                            app.network.show_detail = true;
                            app.network.show_mock_rules_panel = false;
                            app.network.detail_scroll = 0;
                            app.network.collapsed_sections.clear();
                            app.network.json_viewer_states.clear();
                            // Auto-enter merged mode if SSE entry has a rule
                            {
                                let indices_vec = app.network.filtered_indices(&app.network_store).to_vec();
                                if let Some(&new_idx) = indices_vec.get(target) {
                                    if let Some(entry) = app.network_store.get(new_idx) {
                                        if entry.protocol == crate::domain::network::Protocol::Sse {
                                            let rule_key = entry.path.split('?').next().unwrap_or(&entry.path).to_string();
                                            app.network.sse_merged_mode = app.network.sse_merge_rules.contains_key(&rule_key);
                                            if app.network.sse_merged_mode {
                                                if let Some(rule) = app.network.sse_merge_rules.get(&rule_key) {
                                                    let chunks_data: Vec<&str> = entry.sse_chunks.iter().map(|c| c.data.as_str()).collect();
                                                    let candidates = crate::domain::sse_merge::extract_field_paths(&chunks_data);
                                                    app.network.sse_merged_field_idx = candidates.iter()
                                                        .position(|(_, d)| d == &rule.field_display)
                                                        .unwrap_or(0);
                                                }
                                            }
                                        } else {
                                            app.network.sse_merged_mode = false;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        return;
    }

    // Logs tab mouse handling
    match mouse.kind {
        MouseEventKind::ScrollUp => {
            app.move_up(SCROLL_LINES);
        }
        MouseEventKind::ScrollDown => {
            app.move_down(SCROLL_LINES);
        }

        MouseEventKind::Down(MouseButton::Left) => {
            let y = mouse.row;
            let x = mouse.column;
            let now = Instant::now();
            app.status_message = None;

            let is_double = if let Some((prev_time, prev_x, prev_y)) = app.layout.last_click {
                now.duration_since(prev_time).as_millis() < DOUBLE_CLICK_MS
                    && prev_y == y
                    && (prev_x as i16 - x as i16).unsigned_abs() < 3
            } else {
                false
            };
            app.layout.last_click = Some((now, x, y));

            if y == app.layout.toolbar_y {
                handle_toolbar_click(app, x);
            } else if y >= app.layout.list_y && y < app.layout.list_y + app.layout.list_height {
                handle_list_click(app, y, is_double);
            } else if y >= app.layout.timeline_y && y < app.layout.bottom_y {
                // Timeline click → jump to position
                let fc = app.filtered_count();
                let offset = crate::ui::logs::timeline::click_to_offset(x, app.layout.width, fc);
                app.scroll_offset = offset;
                app.selected = offset;
                app.auto_scroll = false;
            } else if y == app.layout.bottom_y {
                handle_bottom_click(app, x);
            }
        }

        MouseEventKind::Down(MouseButton::Right) => {
            let y = mouse.row;
            if y >= app.layout.list_y && y < app.layout.list_y + app.layout.list_height {
                handle_list_right_click(app, y);
            }
        }

        _ => {}
    }
}

fn handle_toolbar_click(app: &mut App, x: u16) {
    if x >= app.layout.search_x.0 && x < app.layout.search_x.1 {
        app.enter_search();
    } else if x >= app.layout.filter_x.0 && x < app.layout.filter_x.1 {
        app.enter_tag_filter();
    } else if x >= app.layout.levels_x {
        let offset = x - app.layout.levels_x;
        let btn_idx = offset / LEVEL_BUTTON_WIDTH;
        match btn_idx {
            0 => app.set_level(LogLevel::System),
            1 => app.set_level(LogLevel::Verbose),
            2 => app.set_level(LogLevel::Debug),
            3 => app.set_level(LogLevel::Info),
            4 => app.set_level(LogLevel::Warning),
            5 => app.set_level(LogLevel::Error),
            _ => {
                // 搜索导航按钮
                if !app.search.matches.is_empty() {
                    let nav_offset = offset.saturating_sub(6 * LEVEL_BUTTON_WIDTH + 4);
                    // ◀ and ▶ are each ~2 chars wide
                    if nav_offset < 2 {
                        app.prev_match();
                    } else {
                        app.next_match();
                    }
                }
            }
        }
    }
}

fn compute_list_target(app: &App, y: u16) -> Option<usize> {
    let row_in_list = (y - app.layout.list_y) as usize;
    // Use the row→entry mapping built by the renderer (handles variable-height entries).
    app.layout.row_to_filtered_idx.get(row_in_list).copied()
}

fn handle_list_click(app: &mut App, y: u16, _is_double: bool) {
    if let Some(target) = compute_list_target(app, y) {
        if app.selected == target {
            // Click same row → toggle panel open/close
            app.show_detail_panel = !app.show_detail_panel;
        } else {
            // Click different row → open panel with new selection
            app.selected = target;
            app.show_detail_panel = true;
            app.reset_detail_for_selection();
        }
    }
}

fn handle_list_right_click(app: &mut App, y: u16) {
    if let Some(target) = compute_list_target(app, y) {
        app.selected = target;
        app.toggle_bookmark();
        // Feedback
        if let Some(idx) = app.selected_store_index() {
            if app.is_bookmarked(idx) {
                app.show_status("Bookmarked".to_string());
            } else {
                app.show_status("Bookmark removed".to_string());
            }
        }
    }
}

fn handle_bottom_click(app: &mut App, x: u16) {
    // Click Live/Paused indicator → jump to bottom
    if x < app.layout.source_info_x.0 {
        app.go_bottom();
        return;
    }

    // Click source info area → toggle device picker
    if x >= app.layout.source_info_x.0 && x < app.layout.source_info_x.1 {
        app.show_device_picker = !app.show_device_picker;
        if app.show_device_picker {
            // Set picker selection to current active app
            if let Some(ref active_id) = app.active_app_id {
                // Find the index in the merged list (connected_apps first, then discovered)
                if let Some(pos) = app.connected_apps.iter().position(|a| a.id == *active_id) {
                    app.device_picker_selected = pos;
                }
            }
        }
        return;
    }

    // Check right-side buttons
    for &(name, start, end) in &app.layout.bottom_buttons {
        if x >= start && x < end {
            match name {
                "separator" => app.insert_separator(),
                "clear" => app.clear_logs(),
                "export" => app.export_logs(),
                "stats" => app.enter_stats(),
                "help" => app.enter_help(),
                "quit" => app.should_quit = true,
                _ => {}
            }
            return;
        }
    }
}

// ══════════════════════════════════════
//  Input mode — Mouse
// ══════════════════════════════════════

fn handle_input_mouse(app: &mut App, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let y = mouse.row;
            if y >= app.layout.list_y && y < app.layout.list_y + app.layout.list_height {
                match app.mode {
                    AppMode::Search => app.apply_search(),
                    AppMode::TagFilter => app.apply_tag_filter(),
                    _ => {}
                }
            } else if y == app.layout.toolbar_y {
                match app.mode {
                    AppMode::Search => app.cancel_search(),
                    AppMode::TagFilter => app.cancel_tag_filter(),
                    _ => {}
                }
            }
        }
        MouseEventKind::Down(MouseButton::Right) => match app.mode {
            AppMode::Search => app.cancel_search(),
            AppMode::TagFilter => app.cancel_tag_filter(),
            _ => {}
        },
        MouseEventKind::ScrollUp => app.move_up(SCROLL_LINES),
        MouseEventKind::ScrollDown => app.move_down(SCROLL_LINES),
        _ => {}
    }
}

/// Handle clicks in the detail side panel area.
fn handle_detail_panel_click(app: &mut App, mouse: &MouseEvent) {
    if !app.show_detail_panel {
        return;
    }

    // Detail panel starts at: width * (100 - detail_panel_pct) / 100
    let panel_start_x =
        (app.layout.width as u32 * (100 - app.detail_panel_pct as u32) / 100) as u16;
    if mouse.column < panel_start_x {
        return;
    }

    // Only handle clicks within the list area (not toolbar/timeline/status)
    if mouse.row <= app.layout.toolbar_y || mouse.row >= app.layout.timeline_y {
        return;
    }

    match mouse.kind {
        MouseEventKind::ScrollUp => app.detail_scroll_up(SCROLL_LINES),
        MouseEventKind::ScrollDown => app.detail_scroll_down(SCROLL_LINES),
        MouseEventKind::Down(MouseButton::Left) => {
            let panel_row = mouse.row.saturating_sub(app.layout.list_y);
            let header = app.detail.header_lines.max(2) as u16;
            if panel_row >= header {
                let content_row = (panel_row - header) as usize;
                if let Some(&source_line) = app.detail.viewer_state.row_to_source.get(content_row) {
                    app.toggle_detail_fold(source_line);
                }
            }
        }
        _ => {}
    }
}

/// Copy text to system clipboard (pbcopy on macOS, xclip on Linux).
fn copy_to_clipboard(text: &str) -> String {
    let result = std::process::Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                stdin.write_all(text.as_bytes())?;
            }
            child.wait()
        });

    match result {
        Ok(_) => "Copied to clipboard".to_string(),
        Err(_) => {
            let r2 = std::process::Command::new("xclip")
                .args(["-selection", "clipboard"])
                .stdin(std::process::Stdio::piped())
                .spawn()
                .and_then(|mut child| {
                    use std::io::Write;
                    if let Some(ref mut stdin) = child.stdin {
                        stdin.write_all(text.as_bytes())?;
                    }
                    child.wait()
                });
            match r2 {
                Ok(_) => "Copied to clipboard".to_string(),
                Err(_) => "Copy failed (no pbcopy/xclip)".to_string(),
            }
        }
    }
}

/// Replay the currently selected HTTP request.
fn replay_selected(app: &mut App) {
    if !app.has_connected_client() {
        app.show_status("Replay unavailable — no client connected".to_string());
        return;
    }

    let indices = app.network.filtered_indices(&app.network_store).to_vec();
    if let Some(&idx) = indices.get(app.network.selected) {
        if let Some(entry) = app.network_store.get(idx).cloned() {
            if entry.protocol == crate::domain::network::Protocol::Http {
                if let Some(handle) = app.get_active_handle() {
                    handle.send_replay(
                        entry.method.clone(),
                        entry.url.clone(),
                        entry.request_headers.clone(),
                        entry.request_body.clone(),
                    );
                    app.show_status("Replaying request...".to_string());
                }
            } else {
                app.show_status("Replay is only available for HTTP requests".to_string());
            }
        }
    }
}

/// Copy selected network request as cURL command.
fn copy_as_curl(app: &mut App) {
    let indices = app.network.filtered_indices(&app.network_store).to_vec();
    let entry = if let Some(&idx) = indices.get(app.network.selected) {
        app.network_store.get(idx).cloned()
    } else {
        None
    };

    let entry = match entry {
        Some(e) => e,
        None => {
            app.show_status("No request selected".to_string());
            return;
        }
    };

    if entry.protocol != crate::domain::network::Protocol::Http {
        app.show_status("Copy as cURL is only available for HTTP requests".to_string());
        return;
    }

    let mut cmd = format!("curl -X {} '{}'", entry.method, entry.url);

    // Add headers
    if let Some(ref headers_json) = entry.request_headers {
        if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(headers_json) {
            for (key, val) in &map {
                let val_str = match val {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Array(arr) => {
                        // Dio stores headers as arrays
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                    other => other.to_string(),
                };
                cmd.push_str(&format!(" \\\n  -H '{}: {}'", key, val_str));
            }
        }
    }

    // Add body
    if let Some(ref body) = entry.request_body {
        if !body.is_empty() {
            let escaped = body.replace('\'', "'\\''");
            cmd.push_str(&format!(" \\\n  -d '{}'", escaped));
        }
    }

    let msg = copy_to_clipboard(&cmd);
    app.show_status(format!("cURL {}", msg));
}

/// Copy selected network request's response body to clipboard.
fn copy_response(app: &mut App) {
    let indices = app.network.filtered_indices(&app.network_store).to_vec();
    let entry = if let Some(&idx) = indices.get(app.network.selected) {
        app.network_store.get(idx).cloned()
    } else {
        None
    };

    let entry = match entry {
        Some(e) => e,
        None => {
            app.show_status("No request selected".to_string());
            return;
        }
    };

    // SSE: copy merged text (if in merged mode) or all chunk data
    if entry.protocol == crate::domain::network::Protocol::Sse && !entry.sse_chunks.is_empty() {
        let chunks_data: Vec<&str> = entry.sse_chunks.iter().map(|c| c.data.as_str()).collect();
        let text = if app.network.sse_merged_mode {
            let rule_key = entry.path.split('?').next().unwrap_or(&entry.path).to_string();
            if let Some(rule) = app.network.sse_merge_rules.get(&rule_key) {
                crate::domain::sse_merge::merge_field(&chunks_data, &rule.field_path)
            } else {
                chunks_data.join("\n")
            }
        } else {
            chunks_data.join("\n")
        };
        if text.is_empty() {
            app.show_status("No SSE data".to_string());
            return;
        }
        let msg = copy_to_clipboard(&text);
        app.show_status(format!("Response {}", msg));
        return;
    }

    // WS: copy chat summary (if in chat mode) or all message data
    if entry.protocol == crate::domain::network::Protocol::Ws && !entry.ws_messages.is_empty() {
        let text = if app.network.ws_chat_mode {
            let msgs: Vec<(crate::domain::network::WsDirection, &str, u64)> = entry
                .ws_messages
                .iter()
                .map(|m| (m.direction, m.data.as_str(), m.size))
                .collect();
            let groups = crate::domain::ws_chat::group_messages(&msgs);
            let mut lines = Vec::new();
            for group in &groups {
                let arrow = match group.direction {
                    crate::domain::network::WsDirection::Send => "\u{2192}",
                    crate::domain::network::WsDirection::Recv => "\u{2190}",
                };
                if group.is_binary {
                    let total_kb = group.total_size as f64 / 1024.0;
                    lines.push(format!("{} {} [binary {:.1}KB]", arrow, group.type_label, total_kb));
                } else if let Some(ref merged) = group.merged_delta {
                    lines.push(format!("{} {} (\u{00d7}{})", arrow, group.type_label, group.msg_indices.len()));
                    if !merged.is_empty() {
                        lines.push(merged.clone());
                    }
                } else {
                    for &mi in &group.msg_indices {
                        if let Some(msg) = entry.ws_messages.get(mi) {
                            let preview = crate::domain::ws_chat::preview_message(&msg.data, 200);
                            lines.push(format!("{} {}", arrow, preview));
                        }
                    }
                }
            }
            lines.join("\n")
        } else {
            entry.ws_messages.iter().map(|m| m.data.as_str()).collect::<Vec<_>>().join("\n")
        };
        if text.is_empty() {
            app.show_status("No WS data".to_string());
            return;
        }
        let msg = copy_to_clipboard(&text);
        app.show_status(format!("Response {}", msg));
        return;
    }

    let body = entry.response_body.as_deref().unwrap_or("");
    if body.is_empty() {
        app.show_status("No response body".to_string());
        return;
    }

    // Try pretty-print JSON
    let text = if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        serde_json::to_string_pretty(&value).unwrap_or_else(|_| body.to_string())
    } else {
        body.to_string()
    };

    let msg = copy_to_clipboard(&text);
    app.show_status(format!("Response {}", msg));
}

/// Trigger mock rule sync to connected clients.
fn trigger_mock_sync(app: &App) {
    if let Some(handle) = app.get_active_handle() {
        let json = app.mock_rules.to_json_string();
        handle.send_mock_sync(json);
    }
}

/// Create a mock rule from the currently selected network request and open editor.
fn mock_from_selected(app: &mut App) {
    if !app.has_connected_client() {
        app.show_status("Mock unavailable — no client connected".to_string());
        return;
    }

    let indices = app.network.filtered_indices(&app.network_store).to_vec();
    let entry = if let Some(&idx) = indices.get(app.network.selected) {
        app.network_store.get(idx).cloned()
    } else {
        None
    };

    let entry = match entry {
        Some(e) => e,
        None => {
            app.show_status("No request selected".to_string());
            return;
        }
    };

    if entry.protocol != crate::domain::network::Protocol::Http {
        app.show_status("Mock is only available for HTTP requests".to_string());
        return;
    }

    let url_pattern = entry
        .path
        .split('?')
        .next()
        .unwrap_or(&entry.path)
        .to_string();
    let method = if entry.method.is_empty() {
        None
    } else {
        Some(entry.method.clone())
    };

    // Dedup: check if a rule with same URL pattern + method already exists
    let already_exists = app.mock_rules.rules().iter().any(|r| {
        r.url_pattern == url_pattern && r.method == method
    });
    if already_exists {
        app.show_status(format!("Rule already exists: {}", url_pattern));
        app.network.show_mock_rules_panel = true;
        app.network.show_detail = false;
        return;
    }

    let status_code = entry.http_status.unwrap_or(200);
    let response_body = entry
        .response_body
        .clone()
        .unwrap_or_else(|| "{}".to_string());

    app.mock_rules
        .add(url_pattern.clone(), method, status_code, response_body, 0);
    trigger_mock_sync(app);

    // Show rules panel in right side and give feedback
    app.network.show_mock_rules_panel = true;
    app.network.show_detail = false;
    app.mock_rule_selected = app.mock_rules.len().saturating_sub(1);
    app.show_status(format!("Mock rule added: {}", url_pattern));
}

fn copy_current_log(app: &mut App) {
    if let Some(idx) = app.selected_store_index() {
        if let Some(entry) = app.store.get(idx) {
            let text = entry.full_message();
            let msg = copy_to_clipboard(&text);
            app.show_status(msg);
        }
    }
}

// ══════════════════════════════════════
//  Overlay (Help / Stats) — Mouse
// ══════════════════════════════════════

fn handle_overlay_mouse(app: &mut App, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let x = mouse.column;
            let y = mouse.row;

            if y == 0 && x < 10 {
                match app.mode {
                    AppMode::Help => app.exit_help(),
                    AppMode::Stats => app.exit_stats(),
                    _ => {}
                }
                return;
            }

            // Stats: clickable slowest requests
            if app.mode == AppMode::Stats && app.active_stats_tab == ViewTab::Network {
                for &(store_idx, ry, x_start, x_end) in &app.layout.stats_slowest_regions {
                    if y == ry && x >= x_start && x < x_end {
                        app.exit_stats();
                        let filtered = app.network.filtered_indices(&app.network_store).to_vec();
                        if let Some(fi) = filtered.iter().position(|&idx| idx == store_idx) {
                            app.network.selected = fi;
                            app.network.auto_scroll = false;
                            app.network.show_detail = true;
                        }
                        return;
                    }
                }
            }
        }
        MouseEventKind::Down(MouseButton::Right) => match app.mode {
            AppMode::Help => app.exit_help(),
            AppMode::Stats => app.exit_stats(),
            _ => {}
        },
        _ => {}
    }
}

// ══════════════════════════════════════
//  Keyboard
// ══════════════════════════════════════

fn handle_normal_key(app: &mut App, key: KeyEvent) {
    app.status_message = None;

    // Device picker open — handle keys
    if app.show_device_picker {
        match key.code {
            KeyCode::Esc => {
                app.show_device_picker = false;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.device_picker_selected = app.device_picker_selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = app.discovered_devices.len().saturating_sub(1);
                app.device_picker_selected = (app.device_picker_selected + 1).min(max);
            }
            KeyCode::Enter => {
                let idx = app.device_picker_selected;
                if let Some(device_id) = app.layout.device_picker_item_ids.get(idx) {
                    let id = device_id.clone();
                    if let Some(ref tx) = app.connect_device_tx {
                        let _ = tx.send(id);
                    }
                    app.show_device_picker = false;
                }
            }
            _ => {}
        }
        return;
    }

    // Exit select mode on any key press
    if app.select_mode {
        app.select_mode = false;
        app.show_status("Select mode off".to_string());
        return;
    }

    // Network tab key handling
    if app.active_tab == ViewTab::Network {
        // URL search input mode
        if app.network.search_active {
            match key.code {
                KeyCode::Enter => {
                    app.network.filter.search = app.network.search_input.clone();
                    app.network.search_active = false;
                    app.network.invalidate_filter();
                }
                KeyCode::Esc => {
                    app.network.search_active = false;
                    app.network.search_input.clear();
                }
                KeyCode::Backspace => {
                    app.network.search_input.pop();
                }
                KeyCode::Char(c) => {
                    app.network.search_input.push(c);
                }
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') => app.should_quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.should_quit = true
            }
            // SSE Merged mode: j/k switch fields
            KeyCode::Char('j') | KeyCode::Down if app.network.sse_merged_mode && app.network.show_detail => {
                let sel = app.network.selected;
                let indices = app.network.filtered_indices(&app.network_store).to_vec();
                if let Some(&idx) = indices.get(sel) {
                    if let Some(entry) = app.network_store.get(idx) {
                        if entry.protocol == crate::domain::network::Protocol::Sse {
                            let chunks_data: Vec<&str> = entry.sse_chunks.iter().map(|c| c.data.as_str()).collect();
                            let candidates = crate::domain::sse_merge::extract_field_paths(&chunks_data);
                            let count = candidates.len();
                            if count > 0 {
                                let new_idx = (app.network.sse_merged_field_idx + 1).min(count - 1);
                                app.network.sse_merged_field_idx = new_idx;
                                let rule_key = entry.path.split('?').next().unwrap_or(&entry.path).to_string();
                                if let Some((path, display)) = candidates.into_iter().nth(new_idx) {
                                    app.network.sse_merge_rules.insert(rule_key, crate::app::SseMergeRule {
                                        field_path: path,
                                        field_display: display,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            KeyCode::Char('k') | KeyCode::Up if app.network.sse_merged_mode && app.network.show_detail => {
                app.network.sse_merged_field_idx = app.network.sse_merged_field_idx.saturating_sub(1);
                let sel = app.network.selected;
                let indices = app.network.filtered_indices(&app.network_store).to_vec();
                if let Some(&idx) = indices.get(sel) {
                    if let Some(entry) = app.network_store.get(idx) {
                        if entry.protocol == crate::domain::network::Protocol::Sse {
                            let rule_key = entry.path.split('?').next().unwrap_or(&entry.path).to_string();
                            let chunks_data: Vec<&str> = entry.sse_chunks.iter().map(|c| c.data.as_str()).collect();
                            let candidates = crate::domain::sse_merge::extract_field_paths(&chunks_data);
                            if let Some((path, display)) = candidates.into_iter().nth(app.network.sse_merged_field_idx) {
                                app.network.sse_merge_rules.insert(rule_key, crate::app::SseMergeRule {
                                    field_path: path,
                                    field_display: display,
                                });
                            }
                        }
                    }
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.network.select_up(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let count = app.network.filtered_count(&app.network_store);
                app.network.select_down(1, count);
            }
            KeyCode::PageUp => {
                app.network.move_up(20);
            }
            KeyCode::PageDown => {
                let count = app.network.filtered_count(&app.network_store);
                app.network.move_down(20, count);
            }
            KeyCode::Home => {
                app.network.go_top();
            }
            KeyCode::End => {
                app.network.go_bottom();
            }
            KeyCode::Enter => {
                app.network.show_detail = !app.network.show_detail;
                app.network.detail_scroll = 0;
            }
            KeyCode::Char('/') => {
                app.network.search_active = true;
                app.network.search_input = app.network.filter.search.clone();
            }
            KeyCode::Char('s') => {
                app.select_mode = true;
                app.show_status(
                    "Select mode — use terminal to select text. Press any key to exit.".to_string(),
                );
            }
            KeyCode::Char('r') => replay_selected(app),
            KeyCode::Char('c') => copy_as_curl(app),
            KeyCode::Char('y') => copy_response(app),
            KeyCode::Char('M') => mock_from_selected(app),
            KeyCode::Char('m') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.enter_mock_rules();
            }
            KeyCode::Char('S') => app.enter_network_stats(),
            KeyCode::Char('?') => app.enter_help(),
            KeyCode::Char('1') => app.switch_tab(ViewTab::Logs),
            KeyCode::Char('2') => app.switch_tab(ViewTab::Network),
            KeyCode::Esc => {
                if app.network.sse_merged_mode && app.network.show_detail {
                    app.network.sse_merged_mode = false;
                } else {
                    app.network.filter.reset();
                    app.network.invalidate_filter();
                }
            }
            _ => {}
        }
        return;
    }

    // Logs tab key handling
    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true
        }
        KeyCode::Up | KeyCode::Char('k') => app.select_up(1),
        KeyCode::Down | KeyCode::Char('j') => app.select_down(1),
        KeyCode::PageUp => app.move_up(20),
        KeyCode::PageDown => app.move_down(20),
        KeyCode::Home => app.go_top(),
        KeyCode::End => app.go_bottom(),
        KeyCode::Char('/') => app.enter_search(),
        KeyCode::Char('n') => app.next_match(),
        KeyCode::Char('N') => app.prev_match(),
        KeyCode::Enter => app.toggle_detail_panel(),
        KeyCode::Char('s') => {
            app.select_mode = true;
            app.show_status(
                "Select mode \u{2014} use terminal to select text. Press any key to exit."
                    .to_string(),
            );
        }
        KeyCode::Char('c') => copy_current_log(app),
        KeyCode::Char('e') => app.export_logs(),
        KeyCode::Char('?') => app.enter_help(),
        KeyCode::Char('S') => app.enter_stats(),
        KeyCode::Char('1') => app.switch_tab(ViewTab::Logs),
        KeyCode::Char('2') => app.switch_tab(ViewTab::Network),
        KeyCode::Esc => app.clear_all_filters(),
        _ => {}
    }
}

fn handle_search_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => app.apply_search(),
        KeyCode::Esc => app.cancel_search(),
        KeyCode::Backspace => {
            app.search.input.pop();
        }
        KeyCode::Char(c) => app.search.input.push(c),
        _ => {}
    }
}

fn handle_filter_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => app.apply_tag_filter(),
        KeyCode::Esc => app.cancel_tag_filter(),
        KeyCode::Backspace => {
            app.tag_filter.input.pop();
        }
        KeyCode::Char(c) => app.tag_filter.input.push(c),
        _ => {}
    }
}

fn handle_overlay_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => match app.mode {
            AppMode::Help => app.exit_help(),
            AppMode::Stats => app.exit_stats(),
            _ => {}
        },
        _ => {}
    }
}

fn handle_mock_edit_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.cancel_mock_edit(),
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.save_mock_edit();
            trigger_mock_sync(app);
        }
        KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.save_mock_edit();
            trigger_mock_sync(app);
        }
        KeyCode::Tab => {
            app.mock_edit_field = (app.mock_edit_field + 1) % 5;
        }
        KeyCode::BackTab => {
            app.mock_edit_field = if app.mock_edit_field == 0 {
                4
            } else {
                app.mock_edit_field - 1
            };
        }
        _ => {
            if app.mock_edit_field < 4 {
                // Single-line field
                match key.code {
                    KeyCode::Char(c) => {
                        app.mock_edit_top_values[app.mock_edit_field].push(c);
                    }
                    KeyCode::Backspace => {
                        app.mock_edit_top_values[app.mock_edit_field].pop();
                    }
                    _ => {}
                }
            } else {
                // Body editor
                match key.code {
                    KeyCode::Enter => app.mock_edit_body.insert_char('\n'),
                    KeyCode::Backspace => app.mock_edit_body.backspace(),
                    KeyCode::Delete => app.mock_edit_body.delete(),
                    KeyCode::Left => app.mock_edit_body.move_left(),
                    KeyCode::Right => app.mock_edit_body.move_right(),
                    KeyCode::Up => app.mock_edit_body.move_up(),
                    KeyCode::Down => app.mock_edit_body.move_down(),
                    KeyCode::Home => app.mock_edit_body.move_home(),
                    KeyCode::End => app.mock_edit_body.move_end(),
                    KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if let Ok(output) = std::process::Command::new("pbpaste").output() {
                            if let Ok(text) = String::from_utf8(output.stdout) {
                                app.mock_edit_body.paste(&text);
                            }
                        }
                    }
                    KeyCode::Char(c) => app.mock_edit_body.insert_char(c),
                    _ => {}
                }
            }
        }
    }
}

fn handle_mock_edit_mouse(app: &mut App, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let x = mouse.column;
            let y = mouse.row;

            // Check editor click regions (fields + buttons)
            for (field, ry, x_start, x_end) in app.layout.mock_edit_regions.clone() {
                if y == ry && x >= x_start && x < x_end {
                    match field.as_str() {
                        "url" => app.mock_edit_field = 0,
                        "method" => app.mock_edit_field = 1,
                        "status" => app.mock_edit_field = 2,
                        "delay" => app.mock_edit_field = 3,
                        "save" => {
                            app.save_mock_edit();
                            trigger_mock_sync(app);
                            return;
                        }
                        "cancel" => {
                            app.cancel_mock_edit();
                            return;
                        }
                        _ => {}
                    }
                    return;
                }
            }

            // Check body area click
            if let Some((bx, by, bw, bh)) = app.layout.mock_edit_body_rect {
                if x >= bx && x < bx + bw && y >= by && y < by + bh {
                    app.mock_edit_field = 4;
                    let click_row = (y - by) as usize;
                    let click_col = (x - bx) as usize;
                    app.mock_edit_body.click(
                        app.mock_edit_body.scroll_offset + click_row,
                        click_col,
                    );
                }
            }
        }
        MouseEventKind::ScrollUp => {
            if app.mock_edit_field == 4 {
                app.mock_edit_body.scroll_offset =
                    app.mock_edit_body.scroll_offset.saturating_sub(3);
            }
        }
        MouseEventKind::ScrollDown => {
            if app.mock_edit_field == 4 {
                let max = app
                    .mock_edit_body
                    .total_lines()
                    .saturating_sub(app.mock_edit_body.visible_height);
                app.mock_edit_body.scroll_offset =
                    (app.mock_edit_body.scroll_offset + 3).min(max);
            }
        }
        _ => {}
    }
}

