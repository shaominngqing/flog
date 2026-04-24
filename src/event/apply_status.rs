//! Status-bar click handler extracted from `apply.rs` so each module
//! stays under the Phase 3 Step 3.6 line budget.

use crate::app::{App, ViewTab};

use super::actions::{copy_as_curl, copy_response, replay_selected};

pub(super) fn handle(app: &mut App, x: u16, y: u16) {
    if y != app.layout.bottom_y {
        return;
    }
    if toggle_source_info(app, x) {
        return;
    }
    match app.active_tab {
        ViewTab::Network => dispatch_network_button(app, x),
        ViewTab::Logs => dispatch_logs_button(app, x),
    }
}

fn toggle_source_info(app: &mut App, x: u16) -> bool {
    if x < app.layout.source_info_x.0 || x >= app.layout.source_info_x.1 {
        return false;
    }
    app.show_device_picker = !app.show_device_picker;
    if app.show_device_picker {
        if let Some(ref active_id) = app.active_app_id {
            if let Some(pos) = app
                .layout
                .device_picker_item_ids
                .iter()
                .position(|id| id == active_id)
            {
                app.device_picker_selected = pos;
            }
        }
    }
    true
}

fn dispatch_network_button(app: &mut App, x: u16) {
    for (name, x_start, x_end) in app.layout.net_buttons.clone() {
        if x >= x_start && x < x_end {
            match name.as_str() {
                "replay" => replay_selected(app),
                "curl" => copy_as_curl(app),
                "response" => copy_response(app),
                "mock" => app.enter_mock_rules(),
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

fn dispatch_logs_button(app: &mut App, x: u16) {
    for &(name, start, end) in &app.layout.bottom_buttons.clone() {
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
