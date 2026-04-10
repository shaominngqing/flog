use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use crate::app::{App, AppMode, SourceSelectPhase, ViewTab};
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
    }
}

pub fn handle_mouse(app: &mut App, mouse: MouseEvent) {
    match app.mode {
        AppMode::Normal => handle_normal_mouse(app, mouse),
        AppMode::Search | AppMode::TagFilter => handle_input_mouse(app, mouse),
        AppMode::Help | AppMode::Stats => handle_overlay_mouse(app, mouse),
        AppMode::SourceSelect => handle_source_select_mouse(app, mouse),
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
        let panel_start = (app.layout.width as u32 * (100 - app.detail_panel_pct as u32) / 100) as u16;
        if mouse.column >= panel_start && mouse.row > app.layout.toolbar_y && mouse.row < app.layout.timeline_y {
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
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                app.network.selected = app.network.selected.saturating_sub(SCROLL_LINES);
                if app.network.selected < app.network.scroll_offset {
                    app.network.scroll_offset = app.network.selected;
                }
            }
            MouseEventKind::ScrollDown => {
                let count = app.network.filtered_count(&app.network_store);
                if count > 0 {
                    app.network.selected = (app.network.selected + SCROLL_LINES).min(count - 1);
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let y = mouse.row;
                // Click in list area
                if y >= app.layout.list_y && y < app.layout.list_y + app.layout.list_height {
                    let row_in_list = (y - app.layout.list_y) as usize;
                    let target = app.network.scroll_offset + row_in_list;
                    let count = app.network.filtered_count(&app.network_store);
                    if target < count {
                        if app.network.selected == target {
                            // Click same row -> toggle detail
                            app.network.show_detail = !app.network.show_detail;
                            app.network.detail_scroll = 0;
                        } else {
                            // Click different row -> select and show detail
                            app.network.selected = target;
                            app.network.show_detail = true;
                            app.network.detail_scroll = 0;
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
        app.dropdown.discovered_vm.iter()
            .filter(|s| !(app.connected && app.source_name.contains(&extract_vm_port(&s.ws_url))))
            .count()
    } else {
        app.dropdown.discovered_adb.iter()
            .filter(|d| !(app.connected && app.source_name.contains(&d.model)))
            .count()
    }
}

/// Confirm the currently selected dropdown item — connect to it.
fn dropdown_confirm(app: &mut App) {
    let sel = app.dropdown.selected;
    if app.dropdown.tab == 0 {
        let selectable: Vec<&crate::input::discover::DiscoveredService> = app.dropdown.discovered_vm.iter()
            .filter(|s| !(app.connected && app.source_name.contains(&extract_vm_port(&s.ws_url))))
            .collect();
        if let Some(svc) = selectable.get(sel) {
            let url = svc.ws_url.clone();
            app.show_source_dropdown = false;
            app.send_source_command(crate::app::SourceCommand::ConnectVm(url));
        }
    } else {
        let selectable: Vec<&crate::input::adb::AdbDevice> = app.dropdown.discovered_adb.iter()
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
    ws_url.split(':').nth(2)
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
        MouseEventKind::Down(MouseButton::Right) => {
            match app.mode {
                AppMode::Search => app.cancel_search(),
                AppMode::TagFilter => app.cancel_tag_filter(),
                _ => {}
            }
        }
        MouseEventKind::ScrollUp => app.move_up(SCROLL_LINES),
        MouseEventKind::ScrollDown => app.move_down(SCROLL_LINES),
        _ => {}
    }
}

/// Handle clicks in the detail side panel area.
fn handle_detail_panel_click(app: &mut App, mouse: &MouseEvent) {
    if !app.show_detail_panel { return; }

    // Detail panel starts at: width * (100 - detail_panel_pct) / 100
    let panel_start_x = (app.layout.width as u32 * (100 - app.detail_panel_pct as u32) / 100) as u16;
    if mouse.column < panel_start_x { return; }

    // Only handle clicks within the list area (not toolbar/timeline/status)
    if mouse.row <= app.layout.toolbar_y || mouse.row >= app.layout.timeline_y { return; }

    match mouse.kind {
        MouseEventKind::ScrollUp => app.detail_scroll_up(SCROLL_LINES),
        MouseEventKind::ScrollDown => app.detail_scroll_down(SCROLL_LINES),
        MouseEventKind::Down(MouseButton::Left) => {
            let panel_row = mouse.row.saturating_sub(app.layout.list_y);
            let header = app.detail.header_lines.max(2) as u16;
            if panel_row >= header {
                let content_row = (panel_row - header) as usize;
                if let Some(&source_line) = app.detail.row_to_source.get(content_row) {
                    app.toggle_detail_fold(source_line);
                }
            }
        }
        _ => {}
    }
}

fn copy_current_log(app: &mut App) {
    if let Some(idx) = app.selected_store_index() {
        if let Some(entry) = app.store.get(idx) {
            let text = entry.full_message();
            // Use pbcopy on macOS, xclip on Linux
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

            let msg = match result {
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
            };
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
            if mouse.row == 0 && mouse.column < 10 {
                match app.mode {
                    AppMode::Help => app.exit_help(),
                    AppMode::Stats => app.exit_stats(),
                    _ => {}
                }
            }
        }
        MouseEventKind::Down(MouseButton::Right) => {
            match app.mode {
                AppMode::Help => app.exit_help(),
                AppMode::Stats => app.exit_stats(),
                _ => {}
            }
        }
        _ => {}
    }
}

// ══════════════════════════════════════
//  Keyboard
// ══════════════════════════════════════

fn handle_normal_key(app: &mut App, key: KeyEvent) {
    app.status_message = None;

    // Handle source dropdown if open
    if app.show_source_dropdown {
        match key.code {
            KeyCode::Esc => { app.show_source_dropdown = false; }
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
        match key.code {
            KeyCode::Char('q') => app.should_quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => app.should_quit = true,
            KeyCode::Up | KeyCode::Char('k') => {
                app.network.selected = app.network.selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let count = app.network.filtered_count(&app.network_store);
                if count > 0 && app.network.selected + 1 < count {
                    app.network.selected += 1;
                }
            }
            KeyCode::PageUp => {
                app.network.selected = app.network.selected.saturating_sub(20);
                app.network.scroll_offset = app.network.scroll_offset.saturating_sub(20);
            }
            KeyCode::PageDown => {
                let count = app.network.filtered_count(&app.network_store);
                if count > 0 {
                    app.network.selected = (app.network.selected + 20).min(count - 1);
                    app.network.scroll_offset = (app.network.scroll_offset + 20).min(count.saturating_sub(1));
                }
            }
            KeyCode::Home => {
                app.network.selected = 0;
                app.network.scroll_offset = 0;
            }
            KeyCode::End => {
                let count = app.network.filtered_count(&app.network_store);
                if count > 0 {
                    app.network.selected = count - 1;
                }
            }
            KeyCode::Enter => {
                app.network.show_detail = !app.network.show_detail;
                app.network.detail_scroll = 0;
            }
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
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => app.should_quit = true,
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
        KeyCode::Backspace => { app.search.input.pop(); }
        KeyCode::Char(c) => app.search.input.push(c),
        _ => {}
    }
}

fn handle_filter_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => app.apply_tag_filter(),
        KeyCode::Esc => app.cancel_tag_filter(),
        KeyCode::Backspace => { app.tag_filter.input.pop(); }
        KeyCode::Char(c) => app.tag_filter.input.push(c),
        _ => {}
    }
}

fn handle_overlay_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            match app.mode {
                AppMode::Help => app.exit_help(),
                AppMode::Stats => app.exit_stats(),
                _ => {}
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
                Some(SourceSelectPhase::ScanningVm) |
                Some(SourceSelectPhase::ScanningAdb) |
                Some(SourceSelectPhase::PickVmService(_)) |
                Some(SourceSelectPhase::PickAdbDevice(_)) => {
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
            if services.get(idx).is_some() { Some("pick_vm") } else { None }
        }
        Some(SourceSelectPhase::PickAdbDevice(devices)) => {
            if devices.get(idx).is_some() { Some("pick_adb") } else { None }
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
            if let Some(SourceSelectPhase::PickVmService(services)) = app.source_select.phase.take() {
                if let Some(svc) = services.get(idx) {
                    let url = svc.ws_url.clone();
                    app.exit_source_select();
                    app.send_source_command(SourceCommand::ConnectVm(url));
                }
            }
        }
        Some("pick_adb") => {
            if let Some(SourceSelectPhase::PickAdbDevice(devices)) = app.source_select.phase.take() {
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
