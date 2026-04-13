use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use crate::app::{App, AppMode, SourceSelectPhase, ViewTab};
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
        AppMode::SourceSelect => handle_source_select_key(app, key),
        AppMode::MockRules => handle_mock_rules_key(app, key),
        AppMode::MockRuleEdit => handle_mock_edit_key(app, key),
    }
}

pub fn handle_mouse(app: &mut App, mouse: MouseEvent) {
    match app.mode {
        AppMode::Normal => handle_normal_mouse(app, mouse),
        AppMode::Search | AppMode::TagFilter => handle_input_mouse(app, mouse),
        AppMode::Help | AppMode::Stats => handle_overlay_mouse(app, mouse),
        AppMode::SourceSelect => handle_source_select_mouse(app, mouse),
        AppMode::MockRules => handle_mock_rules_mouse(app, mouse),
        AppMode::MockRuleEdit => handle_mock_edit_mouse(app, mouse),
    }
}

// ══════════════════════════════════════
//  Normal mode — Mouse
// ══════════════════════════════════════

fn handle_normal_mouse(app: &mut App, mouse: MouseEvent) {
    // When source dropdown is open, intercept all mouse events
    if app.show_source_dropdown {
        handle_dropdown_mouse(app, &mouse);
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
                                }
                            }
                            "delete" => {
                                if let Some(rule) = app.mock_rules.rules().get(row_idx) {
                                    let id = rule.id;
                                    app.mock_rules.remove(id);
                                    if app.mock_rule_selected >= app.mock_rules.len() && app.mock_rule_selected > 0 {
                                        app.mock_rule_selected -= 1;
                                    }
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
                    let y = mouse.row;
                    // Use the precise Y set by the detail renderer
                    let detail_content_y = app.layout.net_detail_content_y;
                    if y >= detail_content_y && y < app.layout.bottom_y {
                        let line_idx = app.network.detail_scroll + (y - detail_content_y) as usize;
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
                                "mock" => mock_from_selected(app),
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
                        } else {
                            app.network.selected = target;
                            app.network.show_detail = true;
                            app.network.show_mock_rules_panel = false;
                            app.network.detail_scroll = 0;
                            app.network.collapsed_sections.clear();
                            app.network.json_viewer_states.clear();
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

/// How many selectable (non-current) items in the active dropdown tab.
fn dropdown_selectable_count(app: &App) -> usize {
    if app.dropdown.tab == 0 {
        app.dropdown
            .discovered_vm
            .iter()
            .filter(|s| !(app.connected && app.source_name.contains(&extract_vm_port(&s.ws_url))))
            .count()
    } else {
        app.dropdown
            .discovered_adb
            .iter()
            .filter(|d| !(app.connected && app.source_name.contains(&d.model)))
            .count()
    }
}

/// Confirm the currently selected dropdown item — connect to it.
fn dropdown_confirm(app: &mut App) {
    let sel = app.dropdown.selected;
    if app.dropdown.tab == 0 {
        let selectable: Vec<&crate::input::discover::DiscoveredService> = app
            .dropdown
            .discovered_vm
            .iter()
            .filter(|s| !(app.connected && app.source_name.contains(&extract_vm_port(&s.ws_url))))
            .collect();
        if let Some(svc) = selectable.get(sel) {
            let url = svc.ws_url.clone();
            app.show_source_dropdown = false;
            app.send_source_command(crate::app::SourceCommand::ConnectVm(url));
        }
    } else {
        let selectable: Vec<&crate::input::adb::AdbDevice> = app
            .dropdown
            .discovered_adb
            .iter()
            .filter(|d| !(app.connected && app.source_name.contains(&d.model)))
            .collect();
        if let Some(dev) = selectable.get(sel) {
            let serial = dev.serial.clone();
            app.show_source_dropdown = false;
            app.send_source_command(crate::app::SourceCommand::ConnectAdb(Some(serial)));
        }
    }
}

fn extract_vm_port(ws_url: &str) -> String {
    ws_url
        .split(':')
        .nth(2)
        .and_then(|s| s.split('/').next())
        .unwrap_or("?")
        .to_string()
}

/// Handle mouse events when the source dropdown overlay is open.
fn handle_dropdown_mouse(app: &mut App, mouse: &MouseEvent) {
    let x = mouse.column;
    let y = mouse.row;

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let inside = if let Some((px, py, pw, ph)) = app.layout.dropdown_rect {
                x >= px && x < px + pw && y >= py && y < py + ph
            } else {
                false
            };

            if !inside {
                app.show_source_dropdown = false;
                return;
            }

            // Click on tab row
            if let Some((tab_y, vm_end, adb_start, adb_end)) = app.layout.dropdown_tab_row {
                if y == tab_y {
                    if x < vm_end {
                        app.dropdown.tab = 0;
                        app.dropdown.selected = 0;
                        app.dropdown.scroll_offset = 0;
                    } else if x >= adb_start && x < adb_end {
                        app.dropdown.tab = 1;
                        app.dropdown.selected = 0;
                        app.dropdown.scroll_offset = 0;
                    }
                    return;
                }
            }

            // Click on a device item — select and connect
            for &(item_y, x_start, x_end, selectable_idx) in &app.layout.dropdown_items {
                if y == item_y && x >= x_start && x < x_end {
                    app.dropdown.selected = selectable_idx;
                    dropdown_confirm(app);
                    return;
                }
            }

            // Click inside panel but not on an item — absorb
        }

        MouseEventKind::Down(MouseButton::Right) => {
            app.show_source_dropdown = false;
        }

        MouseEventKind::ScrollUp => {
            app.dropdown.selected = app.dropdown.selected.saturating_sub(1);
        }
        MouseEventKind::ScrollDown => {
            let max = dropdown_selectable_count(app);
            if app.dropdown.selected + 1 < max {
                app.dropdown.selected += 1;
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
    // Click source info area → toggle source dropdown
    if x >= app.layout.source_info_x.0 && x < app.layout.source_info_x.1 {
        app.toggle_source_dropdown();
        // Start background scanning for dropdown (doesn't affect current source)
        if app.show_source_dropdown {
            app.dropdown_scan_requested = true;
        }
        return;
    }

    // Click Live/Paused indicator → jump to bottom
    if x < app.layout.source_info_x.0 {
        app.go_bottom();
        return;
    }

    // 检查右侧按钮
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
    let indices = app.network.filtered_indices(&app.network_store).to_vec();
    if let Some(&idx) = indices.get(app.network.selected) {
        if let Some(entry) = app.network_store.get(idx).cloned() {
            if entry.protocol == crate::domain::network::Protocol::Http {
                if let Some(tx) = &app.replay_tx {
                    let _ = tx.send(entry);
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

/// Create a mock rule from the currently selected network request and open editor.
fn mock_from_selected(app: &mut App) {
    if !app.is_vm_service_connected() {
        app.show_status("Mock requires VM Service connection".to_string());
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

    // Exit select mode on any key press
    if app.select_mode {
        app.select_mode = false;
        app.show_status("Select mode off".to_string());
        return;
    }

    // Handle source dropdown if open
    if app.show_source_dropdown {
        match key.code {
            KeyCode::Esc => {
                app.show_source_dropdown = false;
            }
            KeyCode::Tab | KeyCode::BackTab => {
                app.dropdown.tab = 1 - app.dropdown.tab;
                app.dropdown.selected = 0;
                app.dropdown.scroll_offset = 0;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.dropdown.selected = app.dropdown.selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = dropdown_selectable_count(app);
                if app.dropdown.selected + 1 < max {
                    app.dropdown.selected += 1;
                }
            }
            KeyCode::Enter => {
                dropdown_confirm(app);
            }
            _ => {}
        }
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
                app.network.filter.reset();
                app.network.invalidate_filter();
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

// ══════════════════════════════════════
//  Mock Rules overlay
// ══════════════════════════════════════

fn handle_mock_rules_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => app.mode = AppMode::Normal,
        KeyCode::Char('j') | KeyCode::Down => {
            let len = app.mock_rules.len();
            if len > 0 {
                app.mock_rule_selected = (app.mock_rule_selected + 1).min(len - 1);
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if app.mock_rule_selected > 0 {
                app.mock_rule_selected -= 1;
            }
        }
        KeyCode::Enter | KeyCode::Char('e') => {
            if let Some(rule) = app.mock_rules.rules().get(app.mock_rule_selected) {
                let id = rule.id;
                app.enter_mock_edit(id);
            }
        }
        KeyCode::Char(' ') => {
            // Toggle enabled/disabled
            if let Some(rule) = app.mock_rules.rules().get(app.mock_rule_selected) {
                let id = rule.id;
                app.mock_rules.toggle(id);
            }
        }
        KeyCode::Char('d') | KeyCode::Delete => {
            // Delete rule
            if let Some(rule) = app.mock_rules.rules().get(app.mock_rule_selected) {
                let id = rule.id;
                app.mock_rules.remove(id);
                if app.mock_rule_selected > 0
                    && app.mock_rule_selected >= app.mock_rules.len()
                {
                    app.mock_rule_selected = app.mock_rules.len().saturating_sub(1);
                }
            }
        }
        _ => {}
    }
}

fn handle_mock_edit_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.cancel_mock_edit(),
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::CONTROL) => app.save_mock_edit(),
        KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.save_mock_edit()
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

fn handle_mock_rules_mouse(app: &mut App, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let x = mouse.column;
            let y = mouse.row;
            let now = Instant::now();

            // Double-click detection
            let is_double = if let Some((prev_time, prev_x, prev_y)) = app.layout.last_click {
                now.duration_since(prev_time).as_millis() < DOUBLE_CLICK_MS
                    && prev_x == x
                    && prev_y == y
            } else {
                false
            };
            app.layout.last_click = Some((now, x, y));

            // Back button (top-left)
            if y == 0 && x < 10 {
                app.mode = AppMode::Normal;
                return;
            }

            // Check rule regions
            for (row_idx, action, ry, x_start, x_end) in app.layout.mock_rule_regions.clone() {
                if y == ry && x >= x_start && x < x_end {
                    match action.as_str() {
                        "select" => {
                            app.mock_rule_selected = row_idx;
                            // Double-click on row → enter edit
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
                            }
                        }
                        "delete" => {
                            if let Some(rule) = app.mock_rules.rules().get(row_idx) {
                                let id = rule.id;
                                app.mock_rules.remove(id);
                                if app.mock_rule_selected >= app.mock_rules.len()
                                    && app.mock_rule_selected > 0
                                {
                                    app.mock_rule_selected -= 1;
                                }
                            }
                        }
                        _ => {}
                    }
                    return;
                }
            }
        }
        MouseEventKind::Down(MouseButton::Right) => app.mode = AppMode::Normal,
        _ => {}
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

// ══════════════════════════════════════
//  Source Select mode
// ══════════════════════════════════════

fn handle_source_select_key(app: &mut App, key: KeyEvent) {
    use crate::app::{SourceCommand, SourceSelectPhase};
    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Esc => {
            match &app.source_select.phase {
                Some(SourceSelectPhase::ChooseType) => {
                    // Exit selection, start auto-discover as fallback
                    app.exit_source_select();
                    app.send_source_command(SourceCommand::AutoDiscover);
                }
                Some(SourceSelectPhase::ScanningVm)
                | Some(SourceSelectPhase::ScanningAdb)
                | Some(SourceSelectPhase::PickVmService(_))
                | Some(SourceSelectPhase::PickAdbDevice(_)) => {
                    // Back to choose type
                    app.source_select.phase = Some(SourceSelectPhase::ChooseType);
                    app.source_select.selected_idx = 0;
                    app.source_select.items_count = 2;
                }
                None => {}
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.source_select.selected_idx = app.source_select.selected_idx.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.source_select.selected_idx + 1 < app.source_select.items_count {
                app.source_select.selected_idx += 1;
            }
        }
        KeyCode::Enter => {
            confirm_source_select(app);
        }
        _ => {}
    }
}

fn handle_source_select_mouse(app: &mut App, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let y = mouse.row;
            let x = mouse.column;

            // Check against the source_select_items regions recorded by the renderer
            for &(item_y, x_start, x_end, idx) in &app.layout.source_select_items {
                if y == item_y && x >= x_start && x < x_end {
                    app.source_select.selected_idx = idx;
                    confirm_source_select(app);
                    return;
                }
            }

            // During scanning phases, clicking anywhere acts as "back" (Esc)
            match &app.source_select.phase {
                Some(SourceSelectPhase::ScanningVm) | Some(SourceSelectPhase::ScanningAdb) => {
                    // Right-click or click in lower area = go back
                    // Only trigger if click is in the lower hint area
                    let area_h = app.layout.list_height;
                    if y > app.layout.list_y + area_h.saturating_sub(3) {
                        app.source_select.phase = Some(SourceSelectPhase::ChooseType);
                        app.source_select.selected_idx = 0;
                        app.source_select.items_count = 2;
                    }
                }
                _ => {}
            }
        }
        MouseEventKind::Down(MouseButton::Right) => {
            // Right-click during scanning = go back
            match &app.source_select.phase {
                Some(SourceSelectPhase::ScanningVm) | Some(SourceSelectPhase::ScanningAdb) => {
                    app.source_select.phase = Some(SourceSelectPhase::ChooseType);
                    app.source_select.selected_idx = 0;
                    app.source_select.items_count = 2;
                }
                _ => {}
            }
        }
        _ => {}
    }
}

fn confirm_source_select(app: &mut App) {
    use crate::app::{SourceCommand, SourceSelectPhase};
    let idx = app.source_select.selected_idx;

    // First check the phase by reference, only consume (take) when needed
    let action = match &app.source_select.phase {
        Some(SourceSelectPhase::ChooseType) => match idx {
            0 => Some("vm"),
            1 => Some("adb"),
            _ => None,
        },
        Some(SourceSelectPhase::PickVmService(services)) => {
            if services.get(idx).is_some() {
                Some("pick_vm")
            } else {
                None
            }
        }
        Some(SourceSelectPhase::PickAdbDevice(devices)) => {
            if devices.get(idx).is_some() {
                Some("pick_adb")
            } else {
                None
            }
        }
        _ => None,
    };

    match action {
        Some("vm") => {
            app.source_select.phase = Some(SourceSelectPhase::ScanningVm);
        }
        Some("adb") => {
            app.source_select.phase = Some(SourceSelectPhase::ScanningAdb);
        }
        Some("pick_vm") => {
            if let Some(SourceSelectPhase::PickVmService(services)) = app.source_select.phase.take()
            {
                if let Some(svc) = services.get(idx) {
                    let url = svc.ws_url.clone();
                    app.exit_source_select();
                    app.send_source_command(SourceCommand::ConnectVm(url));
                }
            }
        }
        Some("pick_adb") => {
            if let Some(SourceSelectPhase::PickAdbDevice(devices)) = app.source_select.phase.take()
            {
                if let Some(dev) = devices.get(idx) {
                    let serial = dev.serial.clone();
                    app.exit_source_select();
                    app.send_source_command(SourceCommand::ConnectAdb(Some(serial)));
                }
            }
        }
        _ => {}
    }
}
