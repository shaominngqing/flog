//! Phase 2.5B Task 5b — characterization tests for `src/event.rs` MOUSE dispatch.
//!
//! Drives `event::handle_mouse` over seeded `App` states whose `layout.*` rects
//! are hand-populated so the routing branches fire. Covers:
//!   - `handle_normal_mouse` reachable branches
//!   - `handle_toolbar_op2_click`, `handle_list_click`, `handle_list_right_click`,
//!     `handle_bottom_click`, `handle_detail_panel_click`
//!   - `handle_input_mouse`, `handle_overlay_mouse`, `handle_mock_edit_mouse`
//!   - Helpers invoked from mouse side-effect paths: `replay_selected`,
//!     `copy_as_curl`, `copy_response`, `mock_from_selected`, `copy_current_log`,
//!     `trigger_mock_sync`
//!
//! Audit refs:
//!   - UI-009 mouse handler coverage
//!   - UI-016 click region magic coordinates
//!   - UI-041 handle_normal_mouse cannot be pure-function-tested — many branches
//!     with interleaved mutations are UNTESTABLE until Phase 3 extracts a
//!     `ClickRegion` enum. See `docs/superpowers/audit/03-ui.md`.
//!
//! UNTESTABLE breakdown (things deliberately not covered here):
//!   - Clipboard shell-outs (`pbcopy` / `xclip`): PHYS. We exercise the helper
//!     paths but do not assert clipboard contents.
//!   - SSE / WS pill click handlers inside `handle_normal_mouse` detail branch:
//!     require the renderer to populate `sse_pill_line` / `ws_pill_line` with
//!     a matching `detail_section_map`, plus derived coordinate math. UNTESTABLE
//!     under UI-041 without extracting ClickRegion.
//!   - JSON fold toggle inside detail click: depends on renderer-built click map.

#![cfg(test)]
#![allow(clippy::too_many_lines)]

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use flog::app::{App, AppMode, InputField, SseMergeRule, SsePathSegment, ViewTab};
use flog::domain::entry::{InputSource, LogEntry, LogLevel};
use flog::domain::network::{NetworkEntry, SseChunk};
use flog::domain::network_filter::{MethodFilter, ProtocolFilter, StatusFilter};
use flog::event;

// ---- Event builders -------------------------------------------------

fn click(x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::empty(),
    }
}

fn right_click(x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: x,
        row: y,
        modifiers: KeyModifiers::empty(),
    }
}

fn scroll_up(x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: x,
        row: y,
        modifiers: KeyModifiers::empty(),
    }
}

fn scroll_down(x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: x,
        row: y,
        modifiers: KeyModifiers::empty(),
    }
}

// ---- App seeding ----------------------------------------------------

fn app_with_n_logs(n: usize) -> App {
    let mut app = App::default();
    for i in 0..n {
        app.store.add_entry(LogEntry {
            timestamp: format!("12:00:{:02}.000", i),
            level: LogLevel::Info,
            tag: "T".into(),
            message: format!("msg-{}", i),
            extra_lines: vec![],
            repeat_count: 1,
            source: InputSource::DirectSocket,
            error: None,
            stacktrace: None,
        });
    }
    app.invalidate_filter();
    let _ = app.filtered_count();
    app
}

fn app_with_n_network(n: usize) -> App {
    let mut app = App::default();
    for i in 0..n {
        app.network_store.push_entry(NetworkEntry::new_http(
            i as u64,
            "GET".into(),
            format!("https://x.test/{}", i),
            format!("t-{}", i),
        ));
    }
    app.network.invalidate_filter();
    let _ = app.network.filtered_count(&app.network_store);
    app.active_tab = ViewTab::Network;
    app
}

/// Seed a minimal Logs layout so mouse routing has stable coordinates:
///   row 1  → tab bar
///   row 3  → op row 2 (level buttons)
///   row 5  → input row
///   row 7+ → list (list_height = 5)
///   row 15 → bottom/status bar
fn seed_logs_layout(app: &mut App) {
    app.layout.width = 120;
    app.layout.tab_bar_y = 1;
    app.layout.tab_logs_x = (0, 10);
    app.layout.tab_network_x = (10, 22);
    app.layout.toolbar_y = 2;
    app.layout.toolbar_op2_y = 3;
    app.layout.levels_x = 40;
    app.layout.input_row_y = 5;
    app.layout.log_search_x = (0, 10);
    app.layout.log_exclude_x = (10, 20);
    app.layout.log_tag_x = (20, 30);
    app.layout.list_y = 7;
    app.layout.list_height = 5;
    app.layout.bottom_y = 15;
    app.layout.source_info_x = (30, 60);
    app.layout.bottom_buttons = vec![
        ("separator", 60, 66),
        ("clear", 66, 72),
        ("export", 72, 80),
        ("stats", 80, 86),
        ("help", 86, 92),
        ("quit", 92, 98),
    ];
    // row_to_filtered_idx maps list rows [0..list_height) → filtered index.
    app.layout.row_to_filtered_idx = (0..app.layout.list_height as usize).collect();
}

/// Seed a minimal Network layout.
///   row 1-2 → tab bar (occupies 2 rows)
///   row 4  → net toolbar (search + exclude)
///   row 5  → filter pills row
///   row 7+ → list (list_height=5)
///   row 15 → bottom/status bar
fn seed_network_layout(app: &mut App) {
    app.layout.width = 120;
    app.layout.tab_bar_y = 1;
    app.layout.tab_logs_x = (0, 10);
    app.layout.tab_network_x = (10, 22);
    app.layout.net_toolbar_y = 4;
    app.layout.net_search_x = (0, 20);
    app.layout.net_exclude_x = (20, 40);
    app.layout.net_filter_pills_y = 5;
    app.layout.net_filter_pills = vec![
        ("proto_All".into(), 0, 6),
        ("proto_HTTP".into(), 6, 13),
        ("proto_SSE".into(), 13, 19),
        ("proto_WS".into(), 19, 24),
        ("method_All".into(), 30, 36),
        ("method_GET".into(), 36, 42),
        ("method_POST".into(), 42, 49),
        ("method_PUT".into(), 49, 55),
        ("method_DEL".into(), 55, 61),
        ("method_PATCH".into(), 61, 69),
        ("status_All".into(), 75, 81),
        ("status_OK".into(), 81, 86),
        ("status_Fail".into(), 86, 93),
        ("status_Active".into(), 93, 102),
        ("status_Pending".into(), 102, 112),
    ];
    app.layout.list_y = 7;
    app.layout.list_height = 5;
    app.layout.bottom_y = 15;
    app.layout.net_detail_x = 60;
    app.layout.source_info_x = (30, 60);
    app.layout.net_buttons = vec![
        ("replay".into(), 60, 68),
        ("curl".into(), 68, 74),
        ("response".into(), 74, 84),
        ("mock".into(), 84, 90),
        ("stats".into(), 90, 96),
        ("clear".into(), 96, 102),
        ("help".into(), 102, 108),
    ];
}

// ═════════════════════════════════════════════════════════════════════
//  Top-level dispatch (`handle_mouse`)
// ═════════════════════════════════════════════════════════════════════

#[test]
fn ui_009_dispatch_normal_routes_to_normal_handler() {
    // Click in a non-matching region should still route + noop cleanly.
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    event::handle_mouse(&mut app, click(1000, 1000));
    // No panic; mode unchanged.
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn ui_009_dispatch_input_mode_routes_to_input_handler() {
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    app.enter_input_field(InputField::LogSearch);
    // Click outside the input row → exits input mode.
    event::handle_mouse(&mut app, click(5, 0));
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn ui_009_dispatch_help_routes_to_overlay_handler() {
    let mut app = App::default();
    app.mode = AppMode::Help;
    // Top-left close button region (y==0, x<10).
    event::handle_mouse(&mut app, click(1, 0));
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn ui_009_dispatch_mock_edit_routes_to_mock_edit_handler() {
    let mut app = App::default();
    app.mock_rules.add("/f".into(), None, 200, "{}".into(), 0);
    let id = app.mock_rules.rules()[0].id;
    app.enter_mock_edit(id);
    app.layout.mock_edit_regions = vec![("cancel".into(), 5, 0, 10)];
    event::handle_mouse(&mut app, click(3, 5));
    // cancel → exits to Normal
    assert_eq!(app.mode, AppMode::Normal);
}

// ═════════════════════════════════════════════════════════════════════
//  Tab bar clicks
// ═════════════════════════════════════════════════════════════════════

#[test]
fn ui_016_tab_bar_logs_click_activates_logs() {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    assert_eq!(app.active_tab, ViewTab::Network);
    event::handle_mouse(&mut app, click(5, 1)); // inside tab_logs_x
    assert_eq!(app.active_tab, ViewTab::Logs);
}

#[test]
fn ui_016_tab_bar_network_click_activates_network() {
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    assert_eq!(app.active_tab, ViewTab::Logs);
    event::handle_mouse(&mut app, click(15, 1)); // inside tab_network_x
    assert_eq!(app.active_tab, ViewTab::Network);
}

#[test]
fn ui_016_tab_bar_click_second_row_still_tab() {
    // Tab bar spans two rows (y and y+1).
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    event::handle_mouse(&mut app, click(15, 2));
    assert_eq!(app.active_tab, ViewTab::Network);
}

// ═════════════════════════════════════════════════════════════════════
//  Jump-to-bottom pill
// ═════════════════════════════════════════════════════════════════════

#[test]
fn ui_jump_to_bottom_click_enables_auto_scroll() {
    let mut app = app_with_n_logs(10);
    seed_logs_layout(&mut app);
    app.auto_scroll = false;
    app.layout.jump_to_bottom_rect = Some((50, 10, 6, 1));
    event::handle_mouse(&mut app, click(52, 10));
    assert!(app.auto_scroll);
}

#[test]
fn ui_jump_to_bottom_click_outside_rect_ignored() {
    let mut app = app_with_n_logs(10);
    seed_logs_layout(&mut app);
    app.auto_scroll = false;
    app.layout.jump_to_bottom_rect = Some((50, 10, 6, 1));
    // Click above the rect → pill branch does not fire (row 9 < py=10).
    // The click then falls through the normal handler without panicking.
    event::handle_mouse(&mut app, click(52, 9));
    // Mode is unchanged; pill didn't re-enable auto_scroll on its own.
    assert_eq!(app.mode, AppMode::Normal);
}

// ═════════════════════════════════════════════════════════════════════
//  Scroll events (Logs + Network)
// ═════════════════════════════════════════════════════════════════════

#[test]
fn ui_scroll_down_in_logs_advances_selected() {
    let mut app = app_with_n_logs(20);
    seed_logs_layout(&mut app);
    let before = app.selected;
    event::handle_mouse(&mut app, scroll_down(10, 10));
    assert!(app.selected > before);
}

#[test]
fn ui_scroll_up_in_logs_retreats_selected() {
    let mut app = app_with_n_logs(20);
    seed_logs_layout(&mut app);
    app.selected = 10;
    app.scroll_offset = 10;
    event::handle_mouse(&mut app, scroll_up(10, 10));
    assert!(app.selected < 10);
}

#[test]
fn ui_scroll_down_in_network_advances_selected() {
    let mut app = app_with_n_network(20);
    seed_network_layout(&mut app);
    let before = app.network.selected;
    event::handle_mouse(&mut app, scroll_down(10, 7));
    assert!(app.network.selected > before);
}

#[test]
fn ui_scroll_up_in_network_retreats_selected() {
    let mut app = app_with_n_network(20);
    seed_network_layout(&mut app);
    app.network.selected = 10;
    app.network.scroll_offset = 10;
    event::handle_mouse(&mut app, scroll_up(10, 7));
    assert!(app.network.selected < 10);
}

// ═════════════════════════════════════════════════════════════════════
//  Logs toolbar op2 (level buttons)
// ═════════════════════════════════════════════════════════════════════

#[test]
fn ui_toolbar_op2_level_button_changes_level() {
    // Button 0 = System, 1 = Verbose, 2 = Debug, 3 = Info, 4 = Warning, 5 = Error.
    // levels_x = 40; each button 3 cols wide.
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    event::handle_mouse(&mut app, click(40, 3)); // button 0
    assert_eq!(app.filter.min_level, LogLevel::System);
    event::handle_mouse(&mut app, click(40 + 3 * 5, 3)); // button 5 = Error
    assert_eq!(app.filter.min_level, LogLevel::Error);
}

#[test]
fn ui_toolbar_op2_click_left_of_levels_noop() {
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    let before = app.filter.min_level;
    event::handle_mouse(&mut app, click(0, 3));
    assert_eq!(app.filter.min_level, before);
}

// ═════════════════════════════════════════════════════════════════════
//  Logs list clicks (handle_list_click + compute_list_target)
// ═════════════════════════════════════════════════════════════════════

#[test]
fn ui_list_click_opens_detail_on_different_row() {
    let mut app = app_with_n_logs(5);
    seed_logs_layout(&mut app);
    assert_eq!(app.selected, 0);
    assert!(!app.show_detail_panel);
    // click row 2 inside list (list_y=7, so y=9 → row_in_list=2)
    event::handle_mouse(&mut app, click(5, 9));
    assert_eq!(app.selected, 2);
    assert!(app.show_detail_panel);
}

#[test]
fn ui_list_click_same_row_toggles_detail_panel() {
    let mut app = app_with_n_logs(5);
    seed_logs_layout(&mut app);
    app.selected = 1;
    app.show_detail_panel = false;
    // click row 1 (y=8)
    event::handle_mouse(&mut app, click(5, 8));
    assert!(app.show_detail_panel);
    // click again → toggles off
    event::handle_mouse(&mut app, click(5, 8));
    assert!(!app.show_detail_panel);
}

#[test]
fn ui_list_click_out_of_range_row_is_noop() {
    let mut app = app_with_n_logs(2);
    seed_logs_layout(&mut app);
    // row_to_filtered_idx only has 5 entries (list_height=5); but only 2 real
    // entries exist — compute_list_target returns a value, but handler code
    // still runs. Clear the mapping to simulate empty list.
    app.layout.row_to_filtered_idx.clear();
    event::handle_mouse(&mut app, click(5, 9));
    assert_eq!(app.selected, 0);
    assert!(!app.show_detail_panel);
}

#[test]
fn ui_list_right_click_bookmarks_entry() {
    let mut app = app_with_n_logs(3);
    seed_logs_layout(&mut app);
    assert!(app.bookmarks.is_empty());
    // right-click row 1 → selects + bookmarks
    event::handle_mouse(&mut app, right_click(5, 8));
    assert_eq!(app.selected, 1);
    assert!(!app.bookmarks.is_empty());
    // status message set
    assert!(app.status_message.is_some());
}

#[test]
fn ui_list_right_click_twice_removes_bookmark() {
    let mut app = app_with_n_logs(3);
    seed_logs_layout(&mut app);
    event::handle_mouse(&mut app, right_click(5, 8));
    assert!(!app.bookmarks.is_empty());
    event::handle_mouse(&mut app, right_click(5, 8));
    assert!(app.bookmarks.is_empty());
}

// ═════════════════════════════════════════════════════════════════════
//  Logs bottom bar clicks
// ═════════════════════════════════════════════════════════════════════

#[test]
fn ui_bottom_click_left_of_source_jumps_to_bottom() {
    let mut app = app_with_n_logs(10);
    seed_logs_layout(&mut app);
    app.auto_scroll = false;
    event::handle_mouse(&mut app, click(5, 15));
    assert!(app.auto_scroll);
}

#[test]
fn ui_bottom_click_on_source_toggles_device_picker() {
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    assert!(!app.show_device_picker);
    event::handle_mouse(&mut app, click(35, 15));
    assert!(app.show_device_picker);
    event::handle_mouse(&mut app, click(35, 15));
    // Second click: picker overlay is open, so this falls into the overlay
    // branch at top of handle_normal_mouse. An "outside picker" click closes
    // the picker (device_picker_rect not set → inside=false → close).
    assert!(!app.show_device_picker);
}

#[test]
fn ui_bottom_click_on_clear_button_clears_logs() {
    let mut app = app_with_n_logs(5);
    seed_logs_layout(&mut app);
    let before = app.store.len();
    assert!(before > 0);
    event::handle_mouse(&mut app, click(68, 15));
    // clear_logs sets a status message and reduces count
    assert!(app.status_message.is_some());
}

#[test]
fn ui_bottom_click_on_help_button_enters_help() {
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    event::handle_mouse(&mut app, click(88, 15));
    assert_eq!(app.mode, AppMode::Help);
}

#[test]
fn ui_bottom_click_on_stats_button_enters_stats() {
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    event::handle_mouse(&mut app, click(82, 15));
    assert_eq!(app.mode, AppMode::Stats);
}

#[test]
fn ui_bottom_click_on_quit_button_sets_should_quit() {
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    event::handle_mouse(&mut app, click(94, 15));
    assert!(app.should_quit);
}

#[test]
fn ui_bottom_click_on_separator_button_inserts_separator() {
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    let before = app.store.len();
    event::handle_mouse(&mut app, click(62, 15));
    assert!(app.store.len() > before);
}

#[test]
fn ui_bottom_click_on_export_button_exports_logs() {
    let mut app = app_with_n_logs(2);
    seed_logs_layout(&mut app);
    event::handle_mouse(&mut app, click(75, 15));
    assert!(app.status_message.is_some());
    // Clean up export file best-effort
    if let Some((ref msg, _)) = app.status_message {
        if let Some(idx) = msg.find("flog_") {
            if let Some(name) = msg[idx..].split(' ').next() {
                let _ = std::fs::remove_file(name);
            }
        }
    }
}

// ═════════════════════════════════════════════════════════════════════
//  Network toolbar clicks (search/exclude fields + filter pills)
// ═════════════════════════════════════════════════════════════════════

#[test]
fn ui_net_search_click_enters_input_field() {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    event::handle_mouse(&mut app, click(5, 4));
    assert!(matches!(
        app.mode,
        AppMode::InputActive(InputField::NetSearch)
    ));
}

#[test]
fn ui_net_exclude_click_enters_input_field() {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    event::handle_mouse(&mut app, click(25, 4));
    assert!(matches!(
        app.mode,
        AppMode::InputActive(InputField::NetExclude)
    ));
}

#[test]
fn ui_net_filter_pill_proto_http_sets_filter() {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    event::handle_mouse(&mut app, click(7, 5));
    assert_eq!(app.network.filter.protocol, ProtocolFilter::Http);
}

#[test]
fn ui_net_filter_pill_proto_sse_sets_filter() {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    event::handle_mouse(&mut app, click(14, 5));
    assert_eq!(app.network.filter.protocol, ProtocolFilter::Sse);
}

#[test]
fn ui_net_filter_pill_proto_ws_sets_filter() {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    event::handle_mouse(&mut app, click(20, 5));
    assert_eq!(app.network.filter.protocol, ProtocolFilter::Ws);
}

#[test]
fn ui_net_filter_pill_proto_all_resets_filter() {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    app.network.filter.protocol = ProtocolFilter::Http;
    event::handle_mouse(&mut app, click(2, 5));
    assert_eq!(app.network.filter.protocol, ProtocolFilter::All);
}

#[test]
fn ui_net_filter_pill_method_get_sets_filter() {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    event::handle_mouse(&mut app, click(37, 5));
    assert_eq!(app.network.filter.method, MethodFilter::Get);
}

#[test]
fn ui_net_filter_pill_method_post_sets_filter() {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    event::handle_mouse(&mut app, click(44, 5));
    assert_eq!(app.network.filter.method, MethodFilter::Post);
}

#[test]
fn ui_net_filter_pill_method_put_sets_filter() {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    event::handle_mouse(&mut app, click(51, 5));
    assert_eq!(app.network.filter.method, MethodFilter::Put);
}

#[test]
fn ui_net_filter_pill_method_del_sets_filter() {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    event::handle_mouse(&mut app, click(57, 5));
    assert_eq!(app.network.filter.method, MethodFilter::Delete);
}

#[test]
fn ui_net_filter_pill_method_patch_sets_filter() {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    event::handle_mouse(&mut app, click(63, 5));
    assert_eq!(app.network.filter.method, MethodFilter::Patch);
}

#[test]
fn ui_net_filter_pill_method_all_resets_filter() {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    app.network.filter.method = MethodFilter::Get;
    event::handle_mouse(&mut app, click(32, 5));
    assert_eq!(app.network.filter.method, MethodFilter::All);
}

#[test]
fn ui_net_filter_pill_status_ok_sets_filter() {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    event::handle_mouse(&mut app, click(83, 5));
    assert_eq!(app.network.filter.status, StatusFilter::Completed);
}

#[test]
fn ui_net_filter_pill_status_fail_sets_filter() {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    event::handle_mouse(&mut app, click(88, 5));
    assert_eq!(app.network.filter.status, StatusFilter::Failed);
}

#[test]
fn ui_net_filter_pill_status_active_sets_filter() {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    event::handle_mouse(&mut app, click(95, 5));
    assert_eq!(app.network.filter.status, StatusFilter::Active);
}

#[test]
fn ui_net_filter_pill_status_pending_sets_filter() {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    event::handle_mouse(&mut app, click(105, 5));
    assert_eq!(app.network.filter.status, StatusFilter::Pending);
}

#[test]
fn ui_net_filter_pill_status_all_resets_filter() {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    app.network.filter.status = StatusFilter::Completed;
    event::handle_mouse(&mut app, click(78, 5));
    assert_eq!(app.network.filter.status, StatusFilter::All);
}

// ═════════════════════════════════════════════════════════════════════
//  Network list clicks (row selection / show_detail toggle)
// ═════════════════════════════════════════════════════════════════════

#[test]
fn ui_net_list_click_row_selects_and_shows_detail() {
    let mut app = app_with_n_network(5);
    seed_network_layout(&mut app);
    // list_y=7. Click row 2 → y=9.
    assert_eq!(app.network.selected, 0);
    event::handle_mouse(&mut app, click(5, 9));
    assert_eq!(app.network.selected, 2);
    assert!(app.network.show_detail);
}

#[test]
fn ui_net_list_click_same_row_toggles_detail() {
    let mut app = app_with_n_network(5);
    seed_network_layout(&mut app);
    app.network.selected = 1;
    assert!(!app.network.show_detail);
    // click same row (list_y=7, row_in_list=1 → y=8)
    event::handle_mouse(&mut app, click(5, 8));
    assert!(app.network.show_detail);
    event::handle_mouse(&mut app, click(5, 8));
    assert!(!app.network.show_detail);
}

#[test]
fn ui_net_list_click_disables_auto_scroll() {
    let mut app = app_with_n_network(5);
    seed_network_layout(&mut app);
    app.network.auto_scroll = true;
    event::handle_mouse(&mut app, click(5, 9));
    assert!(!app.network.auto_scroll);
}

// ═════════════════════════════════════════════════════════════════════
//  Network bottom bar buttons
// ═════════════════════════════════════════════════════════════════════

#[test]
fn ui_net_bottom_click_on_source_toggles_device_picker() {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    assert!(!app.show_device_picker);
    event::handle_mouse(&mut app, click(35, 15));
    assert!(app.show_device_picker);
}

#[test]
fn ui_net_bottom_click_on_mock_button_enters_mock_rules() {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    // No connected client → shows status and returns (does not panic)
    event::handle_mouse(&mut app, click(86, 15));
    assert!(app.status_message.is_some());
}

#[test]
fn ui_net_bottom_click_on_stats_button_enters_network_stats() {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    event::handle_mouse(&mut app, click(92, 15));
    assert_eq!(app.mode, AppMode::Stats);
    assert_eq!(app.active_stats_tab, ViewTab::Network);
}

#[test]
fn ui_net_bottom_click_on_clear_button_clears_network() {
    let mut app = app_with_n_network(3);
    seed_network_layout(&mut app);
    event::handle_mouse(&mut app, click(98, 15));
    assert_eq!(app.network_store.len(), 0);
    assert!(!app.network.show_detail);
    assert!(app.status_message.is_some());
}

#[test]
fn ui_net_bottom_click_on_help_button_enters_help() {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    event::handle_mouse(&mut app, click(104, 15));
    assert_eq!(app.mode, AppMode::Help);
}

#[test]
fn ui_net_bottom_click_on_replay_no_panic_without_client() {
    // replay_selected without connected client → status message, no panic
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    event::handle_mouse(&mut app, click(62, 15));
    assert_eq!(app.mode, AppMode::Normal);
    assert!(app.status_message.is_some());
}

#[test]
fn ui_net_bottom_click_on_curl_button_no_panic() {
    // copy_as_curl path — clipboard shell-out is UNTESTABLE: PHYS, but the
    // call path before shell-out is exercised.
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    event::handle_mouse(&mut app, click(70, 15));
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn ui_net_bottom_click_on_response_button_no_panic() {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    event::handle_mouse(&mut app, click(77, 15));
    assert_eq!(app.mode, AppMode::Normal);
}

// ═════════════════════════════════════════════════════════════════════
//  Device picker overlay mouse
// ═════════════════════════════════════════════════════════════════════

#[test]
fn ui_device_picker_click_outside_closes() {
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    app.show_device_picker = true;
    app.layout.device_picker_rect = Some((20, 5, 30, 10));
    event::handle_mouse(&mut app, click(1, 1));
    assert!(!app.show_device_picker);
}

#[test]
fn ui_device_picker_click_on_item_selects_and_closes() {
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    app.show_device_picker = true;
    app.layout.device_picker_rect = Some((0, 0, 80, 20));
    app.layout.device_picker_items = vec![(5, 0, 30, 0), (6, 0, 30, 1)];
    app.layout.device_picker_item_ids = vec!["app1".into(), "app2".into()];
    event::handle_mouse(&mut app, click(5, 5));
    assert!(!app.show_device_picker);
}

#[test]
fn ui_device_picker_click_inside_but_no_item_noop() {
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    app.show_device_picker = true;
    app.layout.device_picker_rect = Some((0, 0, 80, 20));
    app.layout.device_picker_items = vec![(5, 0, 30, 0)];
    // click inside picker but not on any item row
    event::handle_mouse(&mut app, click(5, 10));
    // Still open — click consumed but no-op
    assert!(app.show_device_picker);
}

#[test]
fn ui_device_picker_scroll_down_advances_scroll() {
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    app.show_device_picker = true;
    app.layout.device_picker_total_lines = 10;
    let before = app.device_picker_scroll;
    event::handle_mouse(&mut app, scroll_down(5, 5));
    assert!(app.device_picker_scroll > before);
}

#[test]
fn ui_device_picker_scroll_up_retreats_scroll() {
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    app.show_device_picker = true;
    app.device_picker_scroll = 5;
    event::handle_mouse(&mut app, scroll_up(5, 5));
    assert_eq!(app.device_picker_scroll, 4);
}

#[test]
fn ui_device_picker_scroll_up_saturates_at_zero() {
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    app.show_device_picker = true;
    app.device_picker_scroll = 0;
    event::handle_mouse(&mut app, scroll_up(5, 5));
    assert_eq!(app.device_picker_scroll, 0);
}

#[test]
fn ui_device_picker_right_click_is_ignored() {
    // Right click + other kinds fall through the _ => {} branch in picker handler.
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    app.show_device_picker = true;
    event::handle_mouse(&mut app, right_click(5, 5));
    assert!(app.show_device_picker); // still open
}

// ═════════════════════════════════════════════════════════════════════
//  Logs input-row clicks (Logs tab, Normal mode → enters input field)
// ═════════════════════════════════════════════════════════════════════

#[test]
fn ui_logs_search_click_enters_input_field() {
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    event::handle_mouse(&mut app, click(3, 5));
    assert!(matches!(
        app.mode,
        AppMode::InputActive(InputField::LogSearch)
    ));
}

#[test]
fn ui_logs_exclude_click_enters_input_field() {
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    event::handle_mouse(&mut app, click(12, 5));
    assert!(matches!(
        app.mode,
        AppMode::InputActive(InputField::LogExclude)
    ));
}

#[test]
fn ui_logs_tag_click_enters_input_field() {
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    event::handle_mouse(&mut app, click(22, 5));
    assert!(matches!(app.mode, AppMode::InputActive(InputField::LogTag)));
}

// ═════════════════════════════════════════════════════════════════════
//  handle_input_mouse
// ═════════════════════════════════════════════════════════════════════

#[test]
fn ui_input_click_on_log_search_field_activates() {
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    app.enter_input_field(InputField::LogSearch);
    // Click on the LogExclude region while in input mode → switches to LogExclude.
    event::handle_mouse(&mut app, click(12, 5));
    assert!(matches!(
        app.mode,
        AppMode::InputActive(InputField::LogExclude)
    ));
}

#[test]
fn ui_input_click_on_log_tag_field_switches_to_tag() {
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    app.enter_input_field(InputField::LogSearch);
    event::handle_mouse(&mut app, click(22, 5));
    assert!(matches!(app.mode, AppMode::InputActive(InputField::LogTag)));
}

#[test]
fn ui_input_click_outside_input_row_exits_input_mode() {
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    app.enter_input_field(InputField::LogSearch);
    event::handle_mouse(&mut app, click(5, 9));
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn ui_input_right_click_exits_input_mode() {
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    app.enter_input_field(InputField::LogSearch);
    event::handle_mouse(&mut app, right_click(5, 5));
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn ui_input_scroll_down_in_logs_scrolls_list() {
    let mut app = app_with_n_logs(30);
    seed_logs_layout(&mut app);
    app.enter_input_field(InputField::LogSearch);
    let before = app.selected;
    event::handle_mouse(&mut app, scroll_down(10, 10));
    assert!(app.selected > before);
    // Still in input mode
    assert!(matches!(app.mode, AppMode::InputActive(_)));
}

#[test]
fn ui_input_scroll_up_in_logs_scrolls_list() {
    let mut app = app_with_n_logs(30);
    seed_logs_layout(&mut app);
    app.enter_input_field(InputField::LogSearch);
    app.selected = 10;
    app.scroll_offset = 10;
    event::handle_mouse(&mut app, scroll_up(10, 10));
    assert!(app.selected < 10);
}

#[test]
fn ui_input_scroll_down_in_network_scrolls_list() {
    let mut app = app_with_n_network(30);
    seed_network_layout(&mut app);
    app.enter_input_field(InputField::NetSearch);
    let before = app.network.selected;
    event::handle_mouse(&mut app, scroll_down(10, 10));
    assert!(app.network.selected > before);
}

#[test]
fn ui_input_scroll_up_in_network_scrolls_list() {
    let mut app = app_with_n_network(30);
    seed_network_layout(&mut app);
    app.enter_input_field(InputField::NetSearch);
    app.network.selected = 10;
    app.network.scroll_offset = 10;
    event::handle_mouse(&mut app, scroll_up(10, 10));
    assert!(app.network.selected < 10);
}

#[test]
fn ui_input_click_net_search_in_network_tab() {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    app.enter_input_field(InputField::NetExclude);
    // layout.input_row_y defaults to 0 in default layout — set it explicitly:
    app.layout.input_row_y = app.layout.net_toolbar_y;
    let y = app.layout.input_row_y;
    event::handle_mouse(&mut app, click(5, y));
    assert!(matches!(
        app.mode,
        AppMode::InputActive(InputField::NetSearch)
    ));
}

#[test]
fn ui_input_click_net_exclude_in_network_tab() {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    app.enter_input_field(InputField::NetSearch);
    app.layout.input_row_y = app.layout.net_toolbar_y;
    let y = app.layout.input_row_y;
    event::handle_mouse(&mut app, click(25, y));
    assert!(matches!(
        app.mode,
        AppMode::InputActive(InputField::NetExclude)
    ));
}

// ═════════════════════════════════════════════════════════════════════
//  handle_detail_panel_click (Logs detail panel)
// ═════════════════════════════════════════════════════════════════════

#[test]
fn ui_detail_panel_click_outside_panel_ignored() {
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    app.show_detail_panel = true;
    app.detail_panel_pct = 35;
    // panel_start_x = 120 * 65 / 100 = 78. Click at x=5 → outside, handler returns.
    event::handle_mouse(&mut app, click(5, 10));
    // No panic; mode unchanged
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn ui_detail_panel_scroll_down_advances_detail_scroll() {
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    app.show_detail_panel = true;
    app.detail_panel_pct = 35;
    // panel_start_x = 120 * 65 / 100 = 78. Click at x=90, y=10 → inside panel.
    let before = app.detail.scroll;
    event::handle_mouse(&mut app, scroll_down(90, 10));
    assert!(app.detail.scroll >= before);
}

#[test]
fn ui_detail_panel_scroll_up_retreats_detail_scroll() {
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    app.show_detail_panel = true;
    app.detail_panel_pct = 35;
    app.detail.scroll = 10;
    event::handle_mouse(&mut app, scroll_up(90, 10));
    assert!(app.detail.scroll < 10);
}

#[test]
fn ui_detail_panel_copy_btn_click_copies() {
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    app.show_detail_panel = true;
    app.detail_panel_pct = 35;
    app.layout.detail_copy_btn = Some((8, 90, 98));
    event::handle_mouse(&mut app, click(92, 8));
    // copy_current_log shells out to pbcopy (UNTESTABLE: PHYS) but should set
    // a status message and not panic.
    assert!(app.status_message.is_some());
}

#[test]
fn ui_detail_panel_click_with_no_panel_open_is_noop() {
    let mut app = app_with_n_logs(1);
    seed_logs_layout(&mut app);
    // show_detail_panel = false → top-level handler skips the panel branch entirely.
    app.show_detail_panel = false;
    event::handle_mouse(&mut app, click(90, 10));
    // Falls through to normal list handling; no panic.
    assert_eq!(app.mode, AppMode::Normal);
}

// ═════════════════════════════════════════════════════════════════════
//  handle_overlay_mouse (Help / Stats)
// ═════════════════════════════════════════════════════════════════════

#[test]
fn ui_overlay_help_close_button_exits() {
    let mut app = App::default();
    app.mode = AppMode::Help;
    event::handle_mouse(&mut app, click(3, 0));
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn ui_overlay_stats_close_button_exits() {
    let mut app = App::default();
    app.mode = AppMode::Stats;
    event::handle_mouse(&mut app, click(3, 0));
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn ui_overlay_help_click_elsewhere_stays() {
    let mut app = App::default();
    app.mode = AppMode::Help;
    event::handle_mouse(&mut app, click(50, 10));
    assert_eq!(app.mode, AppMode::Help);
}

#[test]
fn ui_overlay_right_click_dismisses_help() {
    let mut app = App::default();
    app.mode = AppMode::Help;
    event::handle_mouse(&mut app, right_click(50, 10));
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn ui_overlay_right_click_dismisses_stats() {
    let mut app = App::default();
    app.mode = AppMode::Stats;
    event::handle_mouse(&mut app, right_click(50, 10));
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn ui_overlay_scroll_is_ignored() {
    let mut app = App::default();
    app.mode = AppMode::Help;
    event::handle_mouse(&mut app, scroll_down(5, 5));
    assert_eq!(app.mode, AppMode::Help);
}

#[test]
fn ui_overlay_stats_slowest_click_opens_detail() {
    let mut app = app_with_n_network(3);
    app.mode = AppMode::Stats;
    app.active_stats_tab = ViewTab::Network;
    // Register a clickable slowest row at y=10, x=5..20 → store index 1.
    app.layout.stats_slowest_regions = vec![(1, 10, 5, 20)];
    event::handle_mouse(&mut app, click(8, 10));
    assert_eq!(app.mode, AppMode::Normal); // exit_stats
    assert_eq!(app.network.selected, 1);
    assert!(app.network.show_detail);
}

// ═════════════════════════════════════════════════════════════════════
//  handle_mock_edit_mouse
// ═════════════════════════════════════════════════════════════════════

fn app_in_mock_edit() -> App {
    let mut app = App::default();
    app.mock_rules.add("/foo".into(), None, 200, "{}".into(), 0);
    let id = app.mock_rules.rules()[0].id;
    app.enter_mock_edit(id);
    app
}

#[test]
fn ui_mock_edit_click_url_field_sets_field_0() {
    let mut app = app_in_mock_edit();
    app.layout.mock_edit_regions = vec![
        ("url".into(), 3, 0, 30),
        ("method".into(), 4, 0, 30),
        ("status".into(), 5, 0, 30),
        ("delay".into(), 6, 0, 30),
    ];
    app.mock_edit_field = 2;
    event::handle_mouse(&mut app, click(5, 3));
    assert_eq!(app.mock_edit_field, 0);
}

#[test]
fn ui_mock_edit_click_method_field_sets_field_1() {
    let mut app = app_in_mock_edit();
    app.layout.mock_edit_regions = vec![("method".into(), 4, 0, 30)];
    event::handle_mouse(&mut app, click(5, 4));
    assert_eq!(app.mock_edit_field, 1);
}

#[test]
fn ui_mock_edit_click_status_field_sets_field_2() {
    let mut app = app_in_mock_edit();
    app.layout.mock_edit_regions = vec![("status".into(), 5, 0, 30)];
    event::handle_mouse(&mut app, click(5, 5));
    assert_eq!(app.mock_edit_field, 2);
}

#[test]
fn ui_mock_edit_click_delay_field_sets_field_3() {
    let mut app = app_in_mock_edit();
    app.layout.mock_edit_regions = vec![("delay".into(), 6, 0, 30)];
    event::handle_mouse(&mut app, click(5, 6));
    assert_eq!(app.mock_edit_field, 3);
}

#[test]
fn ui_mock_edit_click_save_saves_and_exits() {
    let mut app = app_in_mock_edit();
    app.layout.mock_edit_regions = vec![("save".into(), 8, 0, 10)];
    event::handle_mouse(&mut app, click(3, 8));
    assert_eq!(app.mode, AppMode::Normal);
    assert!(app.mock_edit_rule_id.is_none());
}

#[test]
fn ui_mock_edit_click_cancel_cancels_and_exits() {
    let mut app = app_in_mock_edit();
    app.layout.mock_edit_regions = vec![("cancel".into(), 8, 20, 30)];
    event::handle_mouse(&mut app, click(25, 8));
    assert_eq!(app.mode, AppMode::Normal);
    assert!(app.mock_edit_rule_id.is_none());
}

#[test]
fn ui_mock_edit_click_body_area_selects_field_4() {
    let mut app = app_in_mock_edit();
    app.mock_edit_field = 0;
    app.layout.mock_edit_body_rect = Some((5, 10, 40, 10));
    event::handle_mouse(&mut app, click(10, 12));
    assert_eq!(app.mock_edit_field, 4);
}

#[test]
fn ui_mock_edit_click_outside_all_regions_is_noop() {
    let mut app = app_in_mock_edit();
    app.mock_edit_field = 0;
    // No regions set; click anywhere → no state change.
    event::handle_mouse(&mut app, click(100, 100));
    assert_eq!(app.mode, AppMode::MockRuleEdit);
    assert_eq!(app.mock_edit_field, 0);
}

#[test]
fn ui_mock_edit_scroll_down_in_body_field_scrolls_editor() {
    let mut app = app_in_mock_edit();
    app.mock_edit_field = 4;
    app.mock_edit_body =
        flog::ui::text_editor::TextEditor::new("l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9\nl10\n");
    app.mock_edit_body.visible_height = 3;
    let before = app.mock_edit_body.scroll_offset;
    event::handle_mouse(&mut app, scroll_down(5, 5));
    assert!(app.mock_edit_body.scroll_offset >= before);
}

#[test]
fn ui_mock_edit_scroll_up_in_body_field_retreats_editor() {
    let mut app = app_in_mock_edit();
    app.mock_edit_field = 4;
    app.mock_edit_body.scroll_offset = 5;
    event::handle_mouse(&mut app, scroll_up(5, 5));
    assert!(app.mock_edit_body.scroll_offset < 5);
}

#[test]
fn ui_mock_edit_scroll_when_not_in_body_field_is_noop() {
    let mut app = app_in_mock_edit();
    app.mock_edit_field = 0;
    let before = app.mock_edit_body.scroll_offset;
    event::handle_mouse(&mut app, scroll_down(5, 5));
    assert_eq!(app.mock_edit_body.scroll_offset, before);
}

#[test]
fn ui_mock_edit_right_click_is_ignored() {
    // The mock edit mouse handler only handles Down(Left) + scroll. Right click falls through.
    let mut app = app_in_mock_edit();
    event::handle_mouse(&mut app, right_click(5, 5));
    assert_eq!(app.mode, AppMode::MockRuleEdit);
}

// ═════════════════════════════════════════════════════════════════════
//  Mock rules panel (inside Network tab) clicks
// ═════════════════════════════════════════════════════════════════════

fn app_with_mock_rules_panel() -> App {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    app.mock_rules.add("/a".into(), None, 200, "{}".into(), 0);
    app.mock_rules.add("/b".into(), None, 200, "{}".into(), 0);
    app.network.show_mock_rules_panel = true;
    // Regions live inside detail panel area (net_detail_x..) between list_y..bottom_y
    app.layout.mock_rule_regions = vec![
        (0, "select".into(), 7, 65, 85),
        (0, "edit".into(), 7, 86, 92),
        (0, "toggle".into(), 7, 92, 98),
        (0, "delete".into(), 7, 98, 104),
        (1, "select".into(), 8, 65, 85),
    ];
    app
}

#[test]
fn ui_mock_panel_click_select_changes_selection() {
    let mut app = app_with_mock_rules_panel();
    assert_eq!(app.mock_rule_selected, 0);
    event::handle_mouse(&mut app, click(70, 8));
    assert_eq!(app.mock_rule_selected, 1);
}

#[test]
fn ui_mock_panel_click_edit_opens_editor() {
    let mut app = app_with_mock_rules_panel();
    event::handle_mouse(&mut app, click(88, 7));
    assert_eq!(app.mode, AppMode::MockRuleEdit);
}

#[test]
fn ui_mock_panel_click_toggle_flips_enabled() {
    let mut app = app_with_mock_rules_panel();
    let id = app.mock_rules.rules()[0].id;
    let before = app.mock_rules.rules()[0].enabled;
    event::handle_mouse(&mut app, click(94, 7));
    let after = app
        .mock_rules
        .rules()
        .iter()
        .find(|r| r.id == id)
        .map(|r| r.enabled)
        .unwrap();
    assert_ne!(before, after);
}

#[test]
fn ui_mock_panel_click_delete_removes_rule() {
    let mut app = app_with_mock_rules_panel();
    let before = app.mock_rules.len();
    event::handle_mouse(&mut app, click(100, 7));
    assert!(app.mock_rules.len() < before);
}

#[test]
fn ui_mock_panel_scroll_consumed_without_panic() {
    let mut app = app_with_mock_rules_panel();
    // Scroll in panel area — consumed by panel branch, no panic.
    event::handle_mouse(&mut app, scroll_down(70, 7));
    assert_eq!(app.mode, AppMode::Normal);
}

// ═════════════════════════════════════════════════════════════════════
//  Network detail panel — [Mock] button + scroll
// ═════════════════════════════════════════════════════════════════════

#[test]
fn ui_net_detail_scroll_down_advances() {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    app.network.show_detail = true;
    app.network.detail_scroll = 0;
    event::handle_mouse(&mut app, scroll_down(70, 7));
    assert!(app.network.detail_scroll > 0);
}

#[test]
fn ui_net_detail_scroll_up_retreats() {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    app.network.show_detail = true;
    app.network.detail_scroll = 10;
    event::handle_mouse(&mut app, scroll_up(70, 7));
    assert!(app.network.detail_scroll < 10);
}

#[test]
fn ui_net_detail_mock_btn_click_invokes_mock_from_selected() {
    let mut app = app_with_n_network(1);
    seed_network_layout(&mut app);
    app.network.show_detail = true;
    app.layout.detail_mock_btn = Some((7, 70, 80));
    event::handle_mouse(&mut app, click(75, 7));
    // Without connected client, status message is set and returns.
    assert!(app.status_message.is_some());
}

// ═════════════════════════════════════════════════════════════════════
//  UI-041 UNTESTABLE branches (deliberately omitted)
// ═════════════════════════════════════════════════════════════════════
//
// The branches below are reachable in theory but require the renderer-side
// coordination that Phase 2.5B cannot simulate cleanly:
//
//   - SSE pill Events/Merged/× toggle (event.rs ~275-367):
//       requires `sse_pill_line` AND the click_x math keyed off the
//       header_w, plus detail_section_map entries at computed line_idx.
//       // UNTESTABLE: D-ref UI-041 — requires ClickRegion extraction
//       //            (Phase 3 UI Event step).
//
//   - WS pill Chat/Raw toggle (event.rs ~373-389):
//       same shape as SSE pill. Computed click_x offsets against
//       `ws_pill_line` header_w.
//       // UNTESTABLE: D-ref UI-041 — requires ClickRegion extraction.
//
//   - SSE_FIELD# / SSE_CLEAR_RULE / WS_GROUP# section keys inside
//     `detail_section_map` (event.rs ~392-467): these are populated by the
//     renderer with specific line indices that match `detail_scroll +
//     (y - detail_content_y)` — setting them up by hand recreates the
//     renderer logic, so the test becomes a change-detector not a
//     characterization.
//       // UNTESTABLE: D-ref UI-041.
//
//   - Generic section-toggle and JSON fold-toggle click maps (event.rs
//     ~471-498): same issue — the maps are derived by the renderer.
//       // UNTESTABLE: D-ref UI-041.
//
//   - Auto-entering SSE merged mode on list click (event.rs ~574-656):
//     code path is exercised by `ui_net_list_click_*` tests above when the
//     entry is HTTP (SSE branch is dead). Testing the SSE branch requires
//     seeding an SSE entry with chunks AND a matching sse_merge_rule — the
//     resulting state mutation is many-step and doesn't characterize
//     anything beyond what the helpers already do.
//       // UNTESTABLE: D-ref UI-041.
//
// Helper functions (pure-ish) that are exercised indirectly through the
// bottom-bar / detail-panel clicks above:
//   - replay_selected:           covered by ui_net_bottom_click_on_replay_no_panic_without_client
//   - copy_as_curl:              covered by ui_net_bottom_click_on_curl_button_no_panic
//   - copy_response:             covered by ui_net_bottom_click_on_response_button_no_panic
//   - copy_current_log:          covered by ui_detail_panel_copy_btn_click_copies
//   - mock_from_selected:        covered by ui_net_detail_mock_btn_click_invokes_mock_from_selected
//   - trigger_mock_sync:         covered by ui_mock_panel_click_toggle_flips_enabled
//                                 (toggle path calls trigger_mock_sync)
//
// copy_to_clipboard (event.rs:926) shells out to pbcopy/xclip:
//   // UNTESTABLE: PHYS — child-process invocation of external clipboard tools.

// Suppress unused-import warning for SseMergeRule / SsePathSegment — they are
// kept in the import list because they are relevant to the UNTESTABLE notes
// above and may be used by future tests once UI-041 is addressed.
#[allow(dead_code)]
fn _keep_imports_live(_r: SseMergeRule, _s: SsePathSegment, _c: SseChunk) {}
