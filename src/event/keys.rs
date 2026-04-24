//! Keyboard event handlers, extracted from `event/mod.rs` in Phase 3
//! Step 3.6 Task 5 to keep the dispatcher small.
//!
//! Each top-level handler (Normal / Input / Overlay / MockRuleEdit) is
//! kept intact here; routing is owned by `mod.rs::handle_key`.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use crate::app::{App, AppMode, ViewTab};

use super::actions::{
    copy_as_curl, copy_current_log, copy_response, mock_from_selected, replay_selected,
    trigger_mock_sync,
};

// ══════════════════════════════════════
//  Keyboard
// ══════════════════════════════════════

pub(super) fn handle_normal_key(app: &mut App, key: KeyEvent) {
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
                let max = app.layout.device_picker_item_ids.len().saturating_sub(1);
                app.device_picker_selected = (app.device_picker_selected + 1).min(max);
            }
            KeyCode::Enter => {
                let idx = app.device_picker_selected;
                if let Some(app_id) = app.layout.device_picker_item_ids.get(idx) {
                    let id = app_id.clone();
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
        match key.code {
            KeyCode::Char('q') => app.should_quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.should_quit = true
            }
            // SSE Merged mode: j/k switch fields
            KeyCode::Char('j') | KeyCode::Down
                if app.network.sse_merged_mode && app.network.show_detail =>
            {
                let sel = app.network.selected;
                let indices = app.network.filtered_indices(&app.network_store).to_vec();
                if let Some(&idx) = indices.get(sel) {
                    if let Some(entry) = app.network_store.get(idx) {
                        if entry.protocol == crate::domain::network::Protocol::Sse {
                            let chunks_data: Vec<&str> =
                                entry.sse_chunks.iter().map(|c| c.data.as_str()).collect();
                            let candidates =
                                crate::domain::sse_merge::extract_field_paths(&chunks_data);
                            let count = candidates.len();
                            if count > 0 {
                                let new_idx = handle_sse_field_navigation(
                                    app.network.sse_merged_field_idx,
                                    count,
                                    SseNavDir::Down,
                                );
                                app.network.sse_merged_field_idx = new_idx;
                                let rule_key = entry
                                    .path
                                    .split('?')
                                    .next()
                                    .unwrap_or(&entry.path)
                                    .to_string();
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
                        }
                    }
                }
            }
            KeyCode::Char('k') | KeyCode::Up
                if app.network.sse_merged_mode && app.network.show_detail =>
            {
                let new_idx = handle_sse_field_navigation(
                    app.network.sse_merged_field_idx,
                    usize::MAX,
                    SseNavDir::Up,
                );
                app.network.sse_merged_field_idx = new_idx;
                let sel = app.network.selected;
                let indices = app.network.filtered_indices(&app.network_store).to_vec();
                if let Some(&idx) = indices.get(sel) {
                    if let Some(entry) = app.network_store.get(idx) {
                        if entry.protocol == crate::domain::network::Protocol::Sse {
                            let rule_key = entry
                                .path
                                .split('?')
                                .next()
                                .unwrap_or(&entry.path)
                                .to_string();
                            let chunks_data: Vec<&str> =
                                entry.sse_chunks.iter().map(|c| c.data.as_str()).collect();
                            let candidates =
                                crate::domain::sse_merge::extract_field_paths(&chunks_data);
                            if let Some((path, display)) =
                                candidates.into_iter().nth(app.network.sse_merged_field_idx)
                            {
                                app.network.sse_merge_rules.insert(
                                    rule_key,
                                    crate::app::SseMergeRule {
                                        field_path: path,
                                        field_display: display,
                                    },
                                );
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
            KeyCode::Char('/') => app.enter_input_field(crate::app::InputField::NetSearch),
            KeyCode::Char('\\') => app.enter_input_field(crate::app::InputField::NetExclude),
            KeyCode::Char('s') => {
                app.select_mode = true;
                app.show_status(
                    "Select mode — use terminal to select text. Press any key to exit.".to_string(),
                );
            }
            KeyCode::Char('r') => replay_selected(app),
            KeyCode::Char('c') => copy_as_curl(app),
            KeyCode::Char('y') => copy_response(app),
            KeyCode::Char('E') => {
                // Expand all JSON sections in the network detail panel.
                // Leaves have unused slots; flipping them is harmless —
                // the renderer consults node kind, not the flag, for leaves.
                for state in app.network.json_viewer_states.values_mut() {
                    for slot in state.expanded.iter_mut() {
                        *slot = true;
                    }
                }
            }
            KeyCode::Char('C') => {
                // Collapse all JSON sections (keep root expanded so the
                // panel still shows top-level keys).
                for state in app.network.json_viewer_states.values_mut() {
                    for (i, slot) in state.expanded.iter_mut().enumerate() {
                        *slot = i == 0;
                    }
                }
            }
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
                    app.inputs.net_search.clear();
                    app.inputs.net_search_cursor = 0;
                    app.inputs.net_exclude.clear();
                    app.inputs.net_exclude_cursor = 0;
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
        KeyCode::Char('/') => app.enter_input_field(crate::app::InputField::LogSearch),
        KeyCode::Char('\\') => app.enter_input_field(crate::app::InputField::LogExclude),
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

pub(super) fn handle_input_key(app: &mut App, field: crate::app::InputField, key: KeyEvent) {
    match key.code {
        KeyCode::Enter | KeyCode::Esc => app.exit_input_field(),
        KeyCode::Backspace => {
            let buf = app.inputs.buffer_mut(field);
            if buf.pop().is_some() {
                let len = buf.len();
                let c = app.inputs.cursor_mut(field);
                *c = (*c).min(len);
            }
            app.apply_input_field(field);
        }
        KeyCode::Char(c) => {
            app.inputs.buffer_mut(field).push(c);
            *app.inputs.cursor_mut(field) = app.inputs.buffer(field).len();
            app.apply_input_field(field);
        }
        _ => {}
    }
}

pub(super) fn handle_overlay_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => match app.mode {
            AppMode::Help => app.exit_help(),
            AppMode::Stats => app.exit_stats(),
            _ => {}
        },
        _ => {}
    }
}

pub(super) fn handle_mock_edit_key(app: &mut App, key: KeyEvent) {
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
            app.mock_edit.field = (app.mock_edit.field + 1) % 5;
        }
        KeyCode::BackTab => {
            app.mock_edit.field = if app.mock_edit.field == 0 {
                4
            } else {
                app.mock_edit.field - 1
            };
        }
        _ => {
            if app.mock_edit.field < 4 {
                // Single-line field
                match key.code {
                    KeyCode::Char(c) => {
                        app.mock_edit.top_values[app.mock_edit.field].push(c);
                    }
                    KeyCode::Backspace => {
                        app.mock_edit.top_values[app.mock_edit.field].pop();
                    }
                    _ => {}
                }
            } else {
                // Body editor
                match key.code {
                    KeyCode::Enter => app.mock_edit.body.insert_char('\n'),
                    KeyCode::Backspace => app.mock_edit.body.backspace(),
                    KeyCode::Delete => app.mock_edit.body.delete(),
                    KeyCode::Left => app.mock_edit.body.move_left(),
                    KeyCode::Right => app.mock_edit.body.move_right(),
                    KeyCode::Up => app.mock_edit.body.move_up(),
                    KeyCode::Down => app.mock_edit.body.move_down(),
                    KeyCode::Home => app.mock_edit.body.move_home(),
                    KeyCode::End => app.mock_edit.body.move_end(),
                    KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if let Ok(output) = std::process::Command::new("pbpaste").output() {
                            if let Ok(text) = String::from_utf8(output.stdout) {
                                app.mock_edit.body.paste(&text);
                            }
                        }
                    }
                    KeyCode::Char(c) => app.mock_edit.body.insert_char(c),
                    _ => {}
                }
            }
        }
    }
}

pub(super) fn handle_mock_edit_mouse(app: &mut App, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let x = mouse.column;
            let y = mouse.row;

            // Check editor click regions (fields + buttons)
            for (field, ry, x_start, x_end) in app.layout.mock_edit_regions.clone() {
                if y == ry && x >= x_start && x < x_end {
                    match field.as_str() {
                        "url" => app.mock_edit.field = 0,
                        "method" => app.mock_edit.field = 1,
                        "status" => app.mock_edit.field = 2,
                        "delay" => app.mock_edit.field = 3,
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
                    app.mock_edit.field = 4;
                    let click_row = (y - by) as usize;
                    let click_col = (x - bx) as usize;
                    app.mock_edit
                        .body
                        .click(app.mock_edit.body.scroll_offset + click_row, click_col);
                }
            }
        }
        MouseEventKind::ScrollUp => {
            if app.mock_edit.field == 4 {
                app.mock_edit.body.scroll_offset =
                    app.mock_edit.body.scroll_offset.saturating_sub(3);
            }
        }
        MouseEventKind::ScrollDown => {
            if app.mock_edit.field == 4 {
                let max = app
                    .mock_edit
                    .body
                    .total_lines()
                    .saturating_sub(app.mock_edit.body.visible_height);
                app.mock_edit.body.scroll_offset = (app.mock_edit.body.scroll_offset + 3).min(max);
            }
        }
        _ => {}
    }
}

/// Phase 2.5A — extracted from UI-008.
/// Direction for SSE merged field navigation.
enum SseNavDir {
    Up,
    Down,
}

/// Pure: given current field index and total count, return the new index
/// after one navigation step. Saturates at 0 and count-1. If count is 0,
/// returns current_idx unchanged (caller is responsible for not calling
/// when no fields exist).
fn handle_sse_field_navigation(current_idx: usize, count: usize, dir: SseNavDir) -> usize {
    if count == 0 {
        return current_idx;
    }
    match dir {
        SseNavDir::Up => current_idx.saturating_sub(1),
        SseNavDir::Down => (current_idx + 1).min(count - 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_nav_down_increments_up_to_bound() {
        assert_eq!(handle_sse_field_navigation(0, 3, SseNavDir::Down), 1);
        assert_eq!(handle_sse_field_navigation(1, 3, SseNavDir::Down), 2);
        assert_eq!(handle_sse_field_navigation(2, 3, SseNavDir::Down), 2); // saturate
    }

    #[test]
    fn sse_nav_up_saturates_at_zero() {
        assert_eq!(handle_sse_field_navigation(2, 3, SseNavDir::Up), 1);
        assert_eq!(handle_sse_field_navigation(1, 3, SseNavDir::Up), 0);
        assert_eq!(handle_sse_field_navigation(0, 3, SseNavDir::Up), 0); // saturate
    }

    #[test]
    fn sse_nav_empty_is_noop() {
        assert_eq!(handle_sse_field_navigation(0, 0, SseNavDir::Up), 0);
        assert_eq!(handle_sse_field_navigation(5, 0, SseNavDir::Down), 5);
    }
}
