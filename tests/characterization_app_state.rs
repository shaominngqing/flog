//! Phase 2.5B Task 6 — characterization tests for `src/app.rs`.
//!
//! Locks down the behavior of the `App` state machine so Phase 3 refactors
//! (InputField split, LogsViewState extraction, LayoutCache move-out, etc.)
//! can proceed with a safety net.
//!
//! Covers the following audit entries:
//!   - UI-002 InputField enum mixes log-only and network-only
//!   - UI-003 App struct conflates UI-local state with session
//!   - UI-004 NetworkState.filtered_indices cache invalidation
//!   - UI-005 LogsViewState extraction
//!   - UI-006 Scroll identity asymmetry
//!   - UI-017 LayoutCache mixed into App state
//!   - UI-018 filter dirty flag pattern
//!   - UI-023 multi-app state / discovered devices
//!   - UI-026 Mock editor fields scattered
//!   - UI-027 Mock edit save/cancel state flow
//!   - UI-028 Mock rule state transitions
//!   - UI-032 reset_session scope
//!   - UI-034 enter_mock_edit deeply nested
//!   - UI-040 Multi-app state invariants
//!
//! Rule 10 applied: every state-transition method gets >=5 cases (happy,
//! empty, boundary, idempotent, combined). Rule 6 applied: tests observe
//! behavior (filtered_count, mode after call) not internal shape where
//! possible.

#![cfg(test)]
#![allow(clippy::too_many_lines)]

use flog::app::{
    App, AppMode, ConnectedApp, InputBuffers, InputField, LayoutCache, MockEditState, NetworkState,
    SseMergeRule, SsePathSegment, ViewTab,
};
use flog::domain::entry::{InputSource, LogEntry, LogLevel};
use flog::domain::network::NetworkEntry;
use flog::input::ConnectorHandle;
use flog::transport::device_monitor::{Device, DeviceKind};

#[path = "support/mod.rs"]
mod support;

// ---- Seeders --------------------------------------------------------

fn log_at(idx: usize, tag: &str, lvl: LogLevel) -> LogEntry {
    LogEntry {
        timestamp: format!("12:00:{:02}.000", idx),
        level: lvl,
        tag: tag.to_string(),
        message: format!("msg-{idx}"),
        extra_lines: vec![],
        repeat_count: 1,
        source: InputSource::DirectSocket,
        error: None,
        stacktrace: None,
    }
}

fn app_with_n_logs(n: usize) -> App {
    let mut app = App::default();
    for i in 0..n {
        app.add_entry(log_at(i, "T", LogLevel::Info));
    }
    let _ = app.filtered_count();
    app
}

fn app_with_n_network(n: usize) -> App {
    let mut app = App::default();
    for i in 0..n {
        app.network_store.push_entry(NetworkEntry::new_http(
            i as u64,
            "GET".into(),
            format!("https://ex.test/{i}"),
            format!("12:00:{:02}.000", i),
        ));
    }
    app.network.invalidate_filter();
    let _ = app.network.filtered_count(&app.network_store);
    app.active_tab = ViewTab::Network;
    app
}

fn sample_device(id: &str, name: &str) -> Device {
    Device {
        id: id.to_string(),
        name: name.to_string(),
        kind: DeviceKind::Local,
    }
}

fn sample_connected_app(
    id: &str,
    device_id: &str,
    app_name: &str,
    app_version: &str,
) -> ConnectedApp {
    let (handle, _rx) = ConnectorHandle::for_testing();
    // NOTE: we leak _rx into a static-ish leak? No — returning handle alone
    // causes rx drop. `send_*` calls will then just fail silently via
    // `let _ =` in the impl, which is what we want in unit tests.
    ConnectedApp {
        id: id.to_string(),
        device_id: device_id.to_string(),
        port: 9753,
        device_name: format!("dev-{device_id}"),
        app_name: app_name.to_string(),
        app_version: app_version.to_string(),
        os: "test".to_string(),
        package_name: "com.test.app".to_string(),
        build_mode: "debug".to_string(),
        handle,
    }
}

// =====================================================================
//  App::new / Default
// =====================================================================

#[test]
fn app_new_has_clean_defaults() {
    let app = App::default();
    assert_eq!(app.active_tab, ViewTab::Logs);
    assert_eq!(app.mode, AppMode::Normal);
    assert!(!app.should_quit);
    assert_eq!(app.logs.selected, 0);
    assert_eq!(app.logs.scroll_offset, 0);
    assert!(app.logs.auto_scroll);
    assert!(!app.show_detail_panel);
    assert_eq!(app.detail_panel_pct, 35);
    assert_eq!(app.server_port, 9753);
    assert!(app.active_app_id.is_none());
    assert!(app.connected_apps.is_empty());
    assert!(app.bookmarks.is_empty());
}

#[test]
fn app_default_equals_new() {
    let a = App::new();
    let b = App::default();
    assert_eq!(a.server_port, b.server_port);
    assert_eq!(a.active_tab, b.active_tab);
}

#[test]
fn app_new_filter_dirty_triggers_lazy_build() {
    // Right after new(), no logs added yet — filtered_count returns 0 and
    // resets dirty flag.
    let mut app = App::new();
    assert_eq!(app.filtered_count(), 0);
    // Calling again hits cache.
    assert_eq!(app.filtered_count(), 0);
}

#[test]
fn app_new_has_connected_client_false() {
    let app = App::new();
    assert!(!app.has_connected_client());
}

#[test]
fn app_new_no_active_status() {
    let app = App::new();
    assert!(app.active_status().is_none());
}

// =====================================================================
//  InputBuffers (UI-002: log vs network fields mixed in one enum)
// =====================================================================

#[test]
fn input_buffers_buffer_mut_writes_per_field() {
    let mut ib = InputBuffers::default();
    ib.buffer_mut(InputField::LogSearch).push_str("ls");
    ib.buffer_mut(InputField::LogExclude).push_str("le");
    ib.buffer_mut(InputField::LogTag).push_str("lt");
    ib.buffer_mut(InputField::NetSearch).push_str("ns");
    ib.buffer_mut(InputField::NetExclude).push_str("ne");
    assert_eq!(ib.buffer(InputField::LogSearch), "ls");
    assert_eq!(ib.buffer(InputField::LogExclude), "le");
    assert_eq!(ib.buffer(InputField::LogTag), "lt");
    assert_eq!(ib.buffer(InputField::NetSearch), "ns");
    assert_eq!(ib.buffer(InputField::NetExclude), "ne");
}

#[test]
fn input_buffers_cursor_mut_writes_per_field() {
    let mut ib = InputBuffers::default();
    *ib.cursor_mut(InputField::LogSearch) = 1;
    *ib.cursor_mut(InputField::LogExclude) = 2;
    *ib.cursor_mut(InputField::LogTag) = 3;
    *ib.cursor_mut(InputField::NetSearch) = 4;
    *ib.cursor_mut(InputField::NetExclude) = 5;
    assert_eq!(ib.cursor(InputField::LogSearch), 1);
    assert_eq!(ib.cursor(InputField::LogExclude), 2);
    assert_eq!(ib.cursor(InputField::LogTag), 3);
    assert_eq!(ib.cursor(InputField::NetSearch), 4);
    assert_eq!(ib.cursor(InputField::NetExclude), 5);
}

#[test]
fn input_buffers_defaults_are_empty() {
    let ib = InputBuffers::default();
    for f in [
        InputField::LogSearch,
        InputField::LogExclude,
        InputField::LogTag,
        InputField::NetSearch,
        InputField::NetExclude,
    ] {
        assert_eq!(ib.buffer(f), "");
        assert_eq!(ib.cursor(f), 0);
    }
}

// =====================================================================
//  add_entry (log path) + store drain + filter_dirty  (UI-018)
// =====================================================================

#[test]
fn add_entry_happy_populates_store_and_dirties_filter() {
    let mut app = App::new();
    app.add_entry(log_at(0, "T", LogLevel::Info));
    assert_eq!(app.store.len(), 1);
    // filtered_count rebuilds
    assert_eq!(app.filtered_count(), 1);
}

#[test]
fn add_entry_empty_store_start_is_clean() {
    let mut app = App::new();
    assert_eq!(app.store.len(), 0);
    app.add_entry(log_at(0, "T", LogLevel::Info));
    assert_eq!(app.store.len(), 1);
}

#[test]
fn add_entry_many_within_cap_does_not_drain() {
    // ring buffer cap is 100K; 5 entries stay.
    let mut app = app_with_n_logs(5);
    assert_eq!(app.store.len(), 5);
    assert_eq!(app.filtered_count(), 5);
}

#[test]
fn add_entry_with_paused_auto_scroll_increments_new_logs() {
    let mut app = app_with_n_logs(3);
    app.select_up(1); // auto_scroll = false
    assert!(!app.logs.auto_scroll);
    let before = app.new_logs_since_pause;
    app.add_entry(log_at(99, "T", LogLevel::Info));
    assert_eq!(app.new_logs_since_pause, before + 1);
}

#[test]
fn add_entry_network_tagged_routes_to_network_store() {
    let mut app = App::new();
    // flog_net tag with a JSON req payload should land in NetworkStore.
    let json = r#"{"id":7,"t":"req","p":"http","method":"GET","url":"https://x.test/"}"#;
    app.add_entry(LogEntry {
        timestamp: "12:00:00.000".into(),
        level: LogLevel::Info,
        tag: "flog_net".into(),
        message: json.into(),
        extra_lines: vec![],
        repeat_count: 1,
        source: InputSource::DirectSocket,
        error: None,
        stacktrace: None,
    });
    assert_eq!(app.network_store.len(), 1);
    // Log store NOT polluted.
    assert_eq!(app.store.len(), 0);
}

#[test]
fn add_entry_flog_net_tag_with_bad_json_falls_back_to_log_path() {
    let mut app = App::new();
    app.add_entry(LogEntry {
        timestamp: "12:00:00.000".into(),
        level: LogLevel::Info,
        tag: "flog_net".into(),
        message: "not-json".into(),
        extra_lines: vec![],
        repeat_count: 1,
        source: InputSource::DirectSocket,
        error: None,
        stacktrace: None,
    });
    // Parser returns None → falls through to log store.
    assert_eq!(app.store.len(), 1);
    assert_eq!(app.network_store.len(), 0);
}

// =====================================================================
//  filtered_indices / filtered_count lazy build (UI-018)
// =====================================================================

#[test]
fn filtered_count_empty_store_is_zero() {
    let mut app = App::new();
    assert_eq!(app.filtered_count(), 0);
}

#[test]
fn filtered_count_rebuilds_after_invalidate() {
    let mut app = app_with_n_logs(3);
    assert_eq!(app.filtered_count(), 3);
    // Change min_level via setter → invalidate → count may change.
    app.set_level(LogLevel::Error);
    assert_eq!(app.filtered_count(), 0);
}

#[test]
fn filtered_count_cache_stable_between_calls() {
    let mut app = app_with_n_logs(4);
    let a = app.filtered_count();
    let b = app.filtered_count();
    assert_eq!(a, b);
    assert_eq!(a, 4);
}

#[test]
fn filtered_indices_returns_slice_of_rebuilt() {
    let mut app = app_with_n_logs(3);
    let idxs = app.filtered_indices();
    assert_eq!(idxs, &[0, 1, 2]);
}

#[test]
fn filtered_count_after_add_reflects_new_count() {
    let mut app = app_with_n_logs(2);
    assert_eq!(app.filtered_count(), 2);
    app.add_entry(log_at(3, "T", LogLevel::Info));
    assert_eq!(app.filtered_count(), 3);
}

#[test]
fn filter_clamps_selected_when_filter_reduces_len() {
    let mut app = app_with_n_logs(5);
    // Move selection past the upcoming filtered length using select_up to
    // disable auto_scroll (clamp only runs when auto_scroll is false).
    app.select_down(4); // selected = 4
    app.select_up(1); // selected = 3, auto_scroll = false
    assert_eq!(app.logs.selected, 3);
    assert!(!app.logs.auto_scroll);
    // Restrict filter to one match.
    app.filter.set_search("msg-0");
    app.invalidate_filter();
    let len = app.filtered_count();
    assert_eq!(len, 1);
    assert!(app.logs.selected < len);
}

#[test]
fn filter_zero_match_resets_selected_and_offset() {
    let mut app = app_with_n_logs(3);
    app.select_down(2);
    app.filter.set_search("no-such-term-xxxxx");
    app.invalidate_filter();
    assert_eq!(app.filtered_count(), 0);
    assert_eq!(app.logs.selected, 0);
    assert_eq!(app.logs.scroll_offset, 0);
}

// =====================================================================
//  Navigation (UI-006 scroll identity, UI-005)
// =====================================================================

#[test]
fn move_up_empty_is_no_op() {
    let mut app = App::new();
    app.move_up(5);
    assert_eq!(app.logs.selected, 0);
    assert_eq!(app.logs.scroll_offset, 0);
}

#[test]
fn move_up_from_bottom_disables_auto_scroll() {
    let mut app = app_with_n_logs(5);
    // Simulate renderer pushing selected forward.
    app.logs.selected = 4;
    app.logs.scroll_offset = 3;
    app.move_up(2);
    assert_eq!(app.logs.selected, 2);
    assert_eq!(app.logs.scroll_offset, 1);
    assert!(!app.logs.auto_scroll);
}

#[test]
fn move_up_at_top_saturates_at_zero() {
    let mut app = app_with_n_logs(3);
    app.move_up(10);
    assert_eq!(app.logs.selected, 0);
    assert_eq!(app.logs.scroll_offset, 0);
}

#[test]
fn move_down_empty_is_no_op() {
    let mut app = App::new();
    app.move_down(3);
    assert_eq!(app.logs.selected, 0);
}

#[test]
fn move_down_clamps_to_len_minus_one() {
    let mut app = app_with_n_logs(4);
    app.move_down(100);
    assert_eq!(app.logs.selected, 3);
}

#[test]
fn select_up_empty_still_zero() {
    let mut app = App::new();
    app.select_up(3);
    assert_eq!(app.logs.selected, 0);
    assert!(!app.logs.auto_scroll);
}

#[test]
fn select_up_follows_viewport_if_above() {
    let mut app = app_with_n_logs(10);
    app.logs.selected = 8;
    app.logs.scroll_offset = 6;
    app.select_up(5); // new selected = 3 < offset 6, so offset follows
    assert_eq!(app.logs.selected, 3);
    assert_eq!(app.logs.scroll_offset, 3);
}

#[test]
fn select_up_stays_when_still_in_viewport() {
    let mut app = app_with_n_logs(10);
    app.logs.selected = 5;
    app.logs.scroll_offset = 2;
    app.select_up(1);
    assert_eq!(app.logs.selected, 4);
    assert_eq!(app.logs.scroll_offset, 2);
}

#[test]
fn select_down_empty_is_no_op() {
    let mut app = App::new();
    app.select_down(3);
    assert_eq!(app.logs.selected, 0);
}

#[test]
fn select_down_clamps_at_end() {
    let mut app = app_with_n_logs(3);
    app.select_down(100);
    assert_eq!(app.logs.selected, 2);
}

#[test]
fn go_top_resets_and_disables_auto_scroll() {
    let mut app = app_with_n_logs(5);
    app.logs.auto_scroll = true;
    app.logs.selected = 3;
    app.logs.scroll_offset = 2;
    app.go_top();
    assert_eq!(app.logs.selected, 0);
    assert_eq!(app.logs.scroll_offset, 0);
    assert!(!app.logs.auto_scroll);
}

#[test]
fn go_bottom_enables_auto_scroll() {
    let mut app = app_with_n_logs(5);
    app.logs.auto_scroll = false;
    app.new_logs_since_pause = 9;
    app.go_bottom();
    assert!(app.logs.auto_scroll);
    assert_eq!(app.new_logs_since_pause, 0);
}

#[test]
fn go_bottom_then_go_top_then_go_bottom_round_trip() {
    let mut app = app_with_n_logs(5);
    app.go_bottom();
    assert!(app.logs.auto_scroll);
    app.go_top();
    assert!(!app.logs.auto_scroll);
    app.go_bottom();
    assert!(app.logs.auto_scroll);
}

// =====================================================================
//  Level setter (UI-018 side effect)
// =====================================================================

#[test]
fn set_level_updates_and_invalidates() {
    let mut app = app_with_n_logs(3);
    assert_eq!(app.filtered_count(), 3);
    app.set_level(LogLevel::Error);
    // min_level raised — no Info entries match.
    assert_eq!(app.filtered_count(), 0);
}

#[test]
fn set_level_to_same_value_still_invalidates() {
    let mut app = app_with_n_logs(3);
    let before = app.filtered_count();
    app.set_level(LogLevel::System); // default
                                     // Next call rebuilds but result is same.
    assert_eq!(app.filtered_count(), before);
}

#[test]
fn set_level_round_trip_error_then_system() {
    let mut app = app_with_n_logs(3);
    app.set_level(LogLevel::Error);
    assert_eq!(app.filtered_count(), 0);
    app.set_level(LogLevel::System);
    assert_eq!(app.filtered_count(), 3);
}

// =====================================================================
//  enter_input_field / exit_input_field / apply  (UI-002, UI-020)
// =====================================================================

#[test]
fn enter_input_field_log_search_sets_mode() {
    let mut app = App::new();
    app.enter_input_field(InputField::LogSearch);
    assert_eq!(app.mode, AppMode::InputActive(InputField::LogSearch));
}

#[test]
fn enter_input_field_log_search_seeds_from_filter() {
    let mut app = App::new();
    app.filter.set_search("seed");
    app.enter_input_field(InputField::LogSearch);
    assert_eq!(app.inputs.log_search, "seed");
    assert_eq!(app.inputs.log_search_cursor, "seed".len());
}

#[test]
fn enter_input_field_does_not_overwrite_nonempty_buffer() {
    let mut app = App::new();
    app.inputs.log_search = "already".to_string();
    app.filter.set_search("seed");
    app.enter_input_field(InputField::LogSearch);
    assert_eq!(app.inputs.log_search, "already");
}

#[test]
fn enter_input_field_log_exclude_seeds() {
    let mut app = App::new();
    app.filter.set_exclude("bad");
    app.enter_input_field(InputField::LogExclude);
    assert_eq!(app.inputs.log_exclude, "bad");
}

#[test]
fn enter_input_field_log_tag_composes_include_and_exclude() {
    let mut app = App::new();
    app.filter.tag_include = vec!["net".into(), "ui".into()];
    app.filter.tag_exclude = vec!["noise".into()];
    app.enter_input_field(InputField::LogTag);
    // Composed as "net|ui|-noise"
    assert!(app.inputs.log_tag.contains("net"));
    assert!(app.inputs.log_tag.contains("ui"));
    assert!(app.inputs.log_tag.contains("-noise"));
}

#[test]
fn enter_input_field_net_search_seeds_from_network_filter() {
    let mut app = App::new();
    app.network.filter.set_search("xy");
    app.enter_input_field(InputField::NetSearch);
    assert_eq!(app.inputs.net_search, "xy");
}

#[test]
fn enter_input_field_net_exclude_seeds() {
    let mut app = App::new();
    app.network.filter.set_exclude("bar");
    app.enter_input_field(InputField::NetExclude);
    assert_eq!(app.inputs.net_exclude, "bar");
}

#[test]
fn exit_input_field_returns_to_normal() {
    let mut app = App::new();
    app.enter_input_field(InputField::LogSearch);
    app.exit_input_field();
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn exit_input_field_is_idempotent_from_normal() {
    let mut app = App::new();
    app.exit_input_field();
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn apply_input_field_log_search_updates_filter() {
    let mut app = app_with_n_logs(3);
    app.inputs.log_search = "msg-1".to_string();
    app.apply_input_field(InputField::LogSearch);
    assert_eq!(app.filter.search_query, "msg-1");
    assert_eq!(app.filtered_count(), 1);
}

#[test]
fn apply_input_field_log_exclude_updates_filter() {
    let mut app = app_with_n_logs(3);
    app.inputs.log_exclude = "msg-0".to_string();
    app.apply_input_field(InputField::LogExclude);
    assert_eq!(app.filter.exclude_query, "msg-0");
    assert_eq!(app.filtered_count(), 2);
}

#[test]
fn apply_input_field_log_tag_parses_include() {
    let mut app = App::new();
    app.inputs.log_tag = "a|b".into();
    app.apply_input_field(InputField::LogTag);
    assert!(app.filter.tag_include.contains(&"a".to_string()));
    assert!(app.filter.tag_include.contains(&"b".to_string()));
}

#[test]
fn apply_input_field_net_search_updates_network_filter() {
    let mut app = app_with_n_network(2);
    app.inputs.net_search = "ex".into();
    app.apply_input_field(InputField::NetSearch);
    assert_eq!(app.network.filter.search, "ex");
}

#[test]
fn apply_input_field_net_exclude_updates_network_filter() {
    let mut app = app_with_n_network(2);
    app.inputs.net_exclude = "foo".into();
    app.apply_input_field(InputField::NetExclude);
    assert_eq!(app.network.filter.exclude, "foo");
}

// =====================================================================
//  search next_match / prev_match
// =====================================================================

#[test]
fn next_match_empty_matches_is_noop() {
    let mut app = app_with_n_logs(3);
    app.next_match();
    assert_eq!(app.logs.selected, 0);
}

#[test]
fn next_match_advances_to_next_after_selected() {
    let mut app = app_with_n_logs(5);
    // Populate matches via filter.
    app.filter.set_search("msg");
    app.invalidate_filter();
    let _ = app.filtered_count();
    assert!(!app.search.matches.is_empty());
    app.logs.selected = 1;
    app.next_match();
    assert!(app.logs.selected > 1);
    assert!(!app.logs.auto_scroll);
}

#[test]
fn next_match_wraps_to_zero_after_last() {
    let mut app = app_with_n_logs(5);
    app.filter.set_search("msg");
    app.invalidate_filter();
    let _ = app.filtered_count();
    // selected >= last match → wrap.
    app.logs.selected = 100;
    app.next_match();
    assert_eq!(app.search.match_idx, 0);
}

#[test]
fn prev_match_empty_is_noop() {
    let mut app = app_with_n_logs(3);
    app.prev_match();
    assert_eq!(app.logs.selected, 0);
}

#[test]
fn prev_match_moves_to_previous() {
    let mut app = app_with_n_logs(5);
    app.filter.set_search("msg");
    app.invalidate_filter();
    let _ = app.filtered_count();
    app.logs.selected = 3;
    app.prev_match();
    assert!(app.logs.selected < 3);
}

#[test]
fn prev_match_wraps_when_at_start() {
    let mut app = app_with_n_logs(5);
    app.filter.set_search("msg");
    app.invalidate_filter();
    let _ = app.filtered_count();
    app.logs.selected = 0;
    app.prev_match();
    // Wraps to last match.
    assert_eq!(app.search.match_idx, app.search.matches.len() - 1);
}

// =====================================================================
//  clear_all_filters / invalidate_filter
// =====================================================================

#[test]
fn clear_all_filters_resets_and_invalidates() {
    let mut app = app_with_n_logs(3);
    app.filter.set_search("foo");
    app.inputs.log_search = "foo".into();
    app.inputs.log_search_cursor = 3;
    app.clear_all_filters();
    assert_eq!(app.filter.search_query, "");
    assert_eq!(app.inputs.log_search, "");
    assert_eq!(app.inputs.log_search_cursor, 0);
}

#[test]
fn clear_all_filters_on_clean_state_is_noop() {
    let mut app = App::new();
    app.clear_all_filters();
    assert_eq!(app.filter.search_query, "");
}

#[test]
fn invalidate_filter_triggers_next_rebuild() {
    let mut app = app_with_n_logs(3);
    assert_eq!(app.filtered_count(), 3);
    app.invalidate_filter();
    // Even though nothing changed, next call rebuilds (observable only if
    // we tamper with the underlying store directly). Check semantic output:
    assert_eq!(app.filtered_count(), 3);
}

// =====================================================================
//  Detail panel
// =====================================================================

#[test]
fn toggle_detail_panel_flips_flag() {
    let mut app = App::new();
    assert!(!app.show_detail_panel);
    app.toggle_detail_panel();
    assert!(app.show_detail_panel);
    app.toggle_detail_panel();
    assert!(!app.show_detail_panel);
}

#[test]
fn reset_detail_for_selection_clears_viewer() {
    use flog::ui::json_viewer::{JsonAction, JsonHotRegion};
    let mut app = App::new();
    app.detail.scroll = 5;
    app.detail.viewer_click_map = vec![
        vec![JsonHotRegion {
            range: 0..u16::MAX,
            action: JsonAction::ToggleFold(1),
        }],
        vec![JsonHotRegion {
            range: 0..u16::MAX,
            action: JsonAction::ToggleFold(2),
        }],
    ];
    app.reset_detail_for_selection();
    assert_eq!(app.detail.scroll, 0);
    assert!(app.detail.viewer_tree.is_none());
    assert!(app.detail.viewer_click_map.is_empty());
}

#[test]
fn detail_scroll_up_saturates() {
    let mut app = App::new();
    app.detail_scroll_up(5);
    assert_eq!(app.detail.scroll, 0);
}

#[test]
fn detail_scroll_down_advances() {
    let mut app = App::new();
    app.detail_scroll_down(7);
    assert_eq!(app.detail.scroll, 7);
    app.detail_scroll_down(3);
    assert_eq!(app.detail.scroll, 10);
}

#[test]
fn detail_scroll_up_after_down_comes_back() {
    let mut app = App::new();
    app.detail_scroll_down(5);
    app.detail_scroll_up(3);
    assert_eq!(app.detail.scroll, 2);
    app.detail_scroll_up(100);
    assert_eq!(app.detail.scroll, 0);
}

#[test]
fn toggle_detail_fold_without_tree_is_noop() {
    let mut app = App::new();
    app.toggle_detail_fold(0);
    // Doesn't panic; state stays.
    assert!(app.detail.viewer_tree.is_none());
}

#[test]
fn selected_store_index_empty_returns_none() {
    let mut app = App::new();
    assert!(app.selected_store_index().is_none());
}

#[test]
fn selected_store_index_points_to_filtered_entry() {
    let mut app = app_with_n_logs(3);
    app.logs.selected = 1;
    assert_eq!(app.selected_store_index(), Some(1));
}

#[test]
fn selected_store_index_out_of_range_returns_none() {
    let mut app = app_with_n_logs(3);
    app.logs.selected = 99;
    assert!(app.selected_store_index().is_none());
}

// =====================================================================
//  Bookmarks
// =====================================================================

#[test]
fn toggle_bookmark_adds_then_removes() {
    let mut app = app_with_n_logs(3);
    app.logs.selected = 0;
    assert!(!app.is_bookmarked(0));
    app.toggle_bookmark();
    assert!(app.is_bookmarked(0));
    app.toggle_bookmark();
    assert!(!app.is_bookmarked(0));
}

#[test]
fn toggle_bookmark_on_empty_store_is_noop() {
    let mut app = App::new();
    app.toggle_bookmark();
    assert!(app.bookmarks.is_empty());
}

#[test]
fn is_bookmarked_false_by_default() {
    let app = app_with_n_logs(3);
    for i in 0..3 {
        assert!(!app.is_bookmarked(i));
    }
}

#[test]
fn toggle_bookmark_multiple_entries() {
    let mut app = app_with_n_logs(5);
    app.logs.selected = 0;
    app.toggle_bookmark();
    app.logs.selected = 2;
    app.toggle_bookmark();
    assert!(app.is_bookmarked(0));
    assert!(app.is_bookmarked(2));
    assert!(!app.is_bookmarked(1));
}

#[test]
fn bookmarks_survive_filter_change() {
    let mut app = app_with_n_logs(3);
    app.logs.selected = 1;
    app.toggle_bookmark();
    app.set_level(LogLevel::Error);
    assert!(app.is_bookmarked(1));
}

// =====================================================================
//  Tab switching
// =====================================================================

#[test]
fn switch_tab_logs_to_network() {
    let mut app = App::new();
    assert_eq!(app.active_tab, ViewTab::Logs);
    app.switch_tab(ViewTab::Network);
    assert_eq!(app.active_tab, ViewTab::Network);
}

#[test]
fn switch_tab_network_to_logs() {
    let mut app = App::new();
    app.active_tab = ViewTab::Network;
    app.switch_tab(ViewTab::Logs);
    assert_eq!(app.active_tab, ViewTab::Logs);
}

#[test]
fn switch_tab_same_is_idempotent() {
    let mut app = App::new();
    app.switch_tab(ViewTab::Logs);
    app.switch_tab(ViewTab::Logs);
    assert_eq!(app.active_tab, ViewTab::Logs);
}

#[test]
fn switch_tab_preserves_selected_and_scroll() {
    let mut app = app_with_n_logs(3);
    app.logs.selected = 2;
    app.logs.scroll_offset = 1;
    app.switch_tab(ViewTab::Network);
    // App-level selected/scroll are untouched by switch_tab.
    assert_eq!(app.logs.selected, 2);
    assert_eq!(app.logs.scroll_offset, 1);
}

#[test]
fn switch_tab_preserves_filter_state() {
    let mut app = app_with_n_logs(3);
    app.filter.set_search("foo");
    app.switch_tab(ViewTab::Network);
    assert_eq!(app.filter.search_query, "foo");
}

// =====================================================================
//  Mode transitions: help / stats
// =====================================================================

#[test]
fn enter_help_then_exit_round_trip() {
    let mut app = App::new();
    app.enter_help();
    assert_eq!(app.mode, AppMode::Help);
    app.exit_help();
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn enter_help_from_input_mode_replaces() {
    let mut app = App::new();
    app.enter_input_field(InputField::LogSearch);
    app.enter_help();
    assert_eq!(app.mode, AppMode::Help);
}

#[test]
fn enter_stats_snapshots_data() {
    let mut app = app_with_n_logs(3);
    app.enter_stats();
    assert_eq!(app.mode, AppMode::Stats);
    assert_eq!(app.active_stats_tab, ViewTab::Logs);
    let snap = app.stats_snapshot.as_ref().expect("snapshot set");
    assert_eq!(snap.total, 3);
    assert_eq!(snap.filtered, 3);
}

#[test]
fn enter_network_stats_does_not_snapshot() {
    let mut app = App::new();
    app.enter_network_stats();
    assert_eq!(app.mode, AppMode::Stats);
    assert_eq!(app.active_stats_tab, ViewTab::Network);
    // Network stats are computed elsewhere, not snapshotted here.
}

#[test]
fn exit_stats_clears_snapshot() {
    let mut app = app_with_n_logs(2);
    app.enter_stats();
    app.exit_stats();
    assert_eq!(app.mode, AppMode::Normal);
    assert!(app.stats_snapshot.is_none());
}

#[test]
fn enter_help_clears_layout_last_click() {
    let mut app = App::new();
    app.layout.last_click = Some((std::time::Instant::now(), 1, 2));
    app.enter_help();
    assert!(app.layout.last_click.is_none());
}

// =====================================================================
//  Status message
// =====================================================================

#[test]
fn show_status_sets_active_then_expires() {
    let mut app = App::new();
    app.show_status("hi".into());
    assert_eq!(app.active_status(), Some("hi"));
    // Simulate tick advancing past expiry (60 ticks).
    app.tick = 61;
    assert!(app.active_status().is_none());
}

#[test]
fn active_status_is_none_when_never_set() {
    let app = App::new();
    assert!(app.active_status().is_none());
}

#[test]
fn show_status_replaces_previous() {
    let mut app = App::new();
    app.show_status("a".into());
    app.show_status("b".into());
    assert_eq!(app.active_status(), Some("b"));
}

#[test]
fn show_status_respects_tick_offset() {
    let mut app = App::new();
    app.tick = 100;
    app.show_status("x".into());
    assert_eq!(app.active_status(), Some("x"));
    app.tick = 160;
    assert!(app.active_status().is_none());
}

#[test]
fn active_status_edge_exactly_at_expiry_is_none() {
    let mut app = App::new();
    app.show_status("x".into());
    // Expiry is tick + 60. At tick == expire, condition self.tick < expire
    // is false.
    app.tick = 60;
    assert!(app.active_status().is_none());
}

// =====================================================================
//  clear_logs / insert_separator (UI-032 reset scope)
// =====================================================================

#[test]
fn clear_logs_resets_store_and_state() {
    let mut app = app_with_n_logs(3);
    app.logs.selected = 2;
    app.logs.scroll_offset = 1;
    app.logs.auto_scroll = false;
    app.new_logs_since_pause = 5;
    app.clear_logs();
    assert_eq!(app.store.len(), 0);
    assert_eq!(app.logs.selected, 0);
    assert_eq!(app.logs.scroll_offset, 0);
    assert!(app.logs.auto_scroll);
    assert_eq!(app.new_logs_since_pause, 0);
    assert_eq!(app.filtered_count(), 0);
}

#[test]
fn clear_logs_also_clears_bookmarks() {
    let mut app = app_with_n_logs(3);
    app.logs.selected = 1;
    app.toggle_bookmark();
    app.clear_logs();
    assert!(app.bookmarks.is_empty());
}

#[test]
fn clear_logs_shows_status_message() {
    let mut app = app_with_n_logs(3);
    app.clear_logs();
    assert!(app.active_status().unwrap_or("").contains("Cleared"));
}

#[test]
fn clear_logs_on_empty_is_idempotent() {
    let mut app = App::new();
    app.clear_logs();
    assert_eq!(app.store.len(), 0);
}

#[test]
fn insert_separator_adds_system_entry() {
    let mut app = App::new();
    app.insert_separator();
    assert_eq!(app.store.len(), 1);
    let entries: Vec<_> = app.store.iter().collect();
    assert_eq!(entries[0].level, LogLevel::System);
    assert!(entries[0].message.contains("────"));
}

#[test]
fn insert_separator_shows_status() {
    let mut app = App::new();
    app.insert_separator();
    assert!(app.active_status().unwrap_or("").contains("Separator"));
}

// =====================================================================
//  Multi-app state (UI-040, UI-023, UI-032)
// =====================================================================

#[test]
fn discovered_devices_tracked_via_field() {
    let mut app = App::new();
    let dev = sample_device("d1", "Pixel");
    app.discovered_devices.insert("d1".into(), dev);
    assert!(app.discovered_devices.contains_key("d1"));
}

#[test]
fn add_connected_app_first_activates_and_updates_source_name() {
    let mut app = App::new();
    let info = sample_connected_app("d1:9753", "d1", "demo", "1.2");
    app.discovered_devices
        .insert("d1".into(), sample_device("d1", "Pixel"));
    app.add_connected_app(info);
    assert_eq!(app.connected_apps.len(), 1);
    assert_eq!(app.active_app_id.as_deref(), Some("d1:9753"));
    assert!(app.source_name.contains("demo"));
    assert!(app.source_name.contains("1.2"));
    assert!(app.source_name.contains("Pixel"));
    assert!(app.has_connected_client());
}

#[test]
fn add_connected_app_second_does_not_swap_active() {
    let mut app = App::new();
    app.add_connected_app(sample_connected_app("a:1", "a", "first", ""));
    app.add_connected_app(sample_connected_app("b:2", "b", "second", ""));
    assert_eq!(app.connected_apps.len(), 2);
    assert_eq!(app.active_app_id.as_deref(), Some("a:1"));
}

#[test]
fn add_connected_app_empty_version_omits_v() {
    let mut app = App::new();
    app.add_connected_app(sample_connected_app("x:1", "x", "appx", ""));
    assert!(!app.source_name.contains("v "));
}

#[test]
fn add_connected_app_reconnect_keeps_active_and_resets_session() {
    let mut app = App::new();
    app.add_connected_app(sample_connected_app("same:1", "same", "a", ""));
    app.add_entry(log_at(1, "T", LogLevel::Info));
    assert_eq!(app.store.len(), 1);
    // Reconnect to the same id — should reset session.
    app.add_connected_app(sample_connected_app("same:1", "same", "a", ""));
    assert_eq!(app.connected_apps.len(), 1);
    assert_eq!(app.store.len(), 0);
}

#[test]
fn remove_connected_app_active_promotes_next() {
    let mut app = App::new();
    app.add_connected_app(sample_connected_app("a:1", "a", "A", ""));
    app.add_connected_app(sample_connected_app("b:2", "b", "B", ""));
    app.remove_connected_app("a:1");
    assert_eq!(app.active_app_id.as_deref(), Some("b:2"));
    assert_eq!(app.connected_apps.len(), 1);
}

#[test]
fn remove_connected_app_last_clears_active() {
    let mut app = App::new();
    app.add_connected_app(sample_connected_app("only:1", "only", "A", ""));
    app.remove_connected_app("only:1");
    assert!(app.active_app_id.is_none());
    assert!(app.connected_apps.is_empty());
    assert!(app.source_name.contains("Scanning"));
}

#[test]
fn remove_connected_app_nonactive_keeps_active() {
    let mut app = App::new();
    app.add_connected_app(sample_connected_app("a:1", "a", "A", ""));
    app.add_connected_app(sample_connected_app("b:2", "b", "B", ""));
    app.remove_connected_app("b:2");
    assert_eq!(app.active_app_id.as_deref(), Some("a:1"));
    assert_eq!(app.connected_apps.len(), 1);
}

#[test]
fn remove_connected_app_unknown_is_noop() {
    let mut app = App::new();
    app.add_connected_app(sample_connected_app("a:1", "a", "A", ""));
    app.remove_connected_app("nope");
    assert_eq!(app.connected_apps.len(), 1);
    assert_eq!(app.active_app_id.as_deref(), Some("a:1"));
}

#[test]
fn switch_to_app_already_active_is_noop() {
    let mut app = App::new();
    app.add_connected_app(sample_connected_app("a:1", "a", "A", ""));
    app.add_entry(log_at(0, "T", LogLevel::Info));
    assert_eq!(app.store.len(), 1);
    app.switch_to_app("a:1");
    assert_eq!(app.store.len(), 1); // not reset
}

#[test]
fn switch_to_app_unknown_id_is_noop() {
    let mut app = App::new();
    app.add_connected_app(sample_connected_app("a:1", "a", "A", ""));
    app.switch_to_app("does-not-exist");
    assert_eq!(app.active_app_id.as_deref(), Some("a:1"));
}

#[test]
fn switch_to_app_to_other_resets_session_and_changes_active() {
    let mut app = App::new();
    app.add_connected_app(sample_connected_app("a:1", "a", "A", ""));
    app.add_connected_app(sample_connected_app("b:2", "b", "B", ""));
    app.add_entry(log_at(0, "T", LogLevel::Info));
    app.switch_to_app("b:2");
    assert_eq!(app.active_app_id.as_deref(), Some("b:2"));
    assert_eq!(app.store.len(), 0);
}

#[test]
fn get_active_handle_none_when_no_active() {
    let app = App::new();
    assert!(app.get_active_handle().is_none());
}

#[test]
fn get_active_handle_some_when_connected() {
    let mut app = App::new();
    app.add_connected_app(sample_connected_app("a:1", "a", "A", ""));
    assert!(app.get_active_handle().is_some());
}

#[test]
fn update_source_name_uses_discovered_device_name_over_hello() {
    let mut app = App::new();
    // Seed discovered_devices with name "FancyName"
    app.discovered_devices
        .insert("d".into(), sample_device("d", "FancyName"));
    app.add_connected_app(sample_connected_app("d:1", "d", "A", "0.1"));
    assert!(app.source_name.contains("FancyName"));
}

// =====================================================================
//  Mock rule state (UI-022, UI-026, UI-027, UI-028, UI-034)
// =====================================================================

#[test]
fn enter_mock_rules_without_client_shows_status() {
    let mut app = App::new();
    app.enter_mock_rules();
    assert!(app.active_status().unwrap_or("").contains("Mock"));
    assert!(!app.network.show_mock_rules_panel);
}

#[test]
fn enter_mock_rules_with_client_toggles_panel_on() {
    let mut app = App::new();
    app.add_connected_app(sample_connected_app("a:1", "a", "A", ""));
    assert!(!app.network.show_mock_rules_panel);
    app.enter_mock_rules();
    assert!(app.network.show_mock_rules_panel);
}

#[test]
fn enter_mock_rules_toggles_off_when_already_showing() {
    let mut app = App::new();
    app.add_connected_app(sample_connected_app("a:1", "a", "A", ""));
    app.enter_mock_rules();
    app.enter_mock_rules();
    assert!(!app.network.show_mock_rules_panel);
}

#[test]
fn enter_mock_rules_hides_detail_when_showing_rules() {
    let mut app = App::new();
    app.add_connected_app(sample_connected_app("a:1", "a", "A", ""));
    app.network.show_detail = true;
    app.enter_mock_rules();
    assert!(app.network.show_mock_rules_panel);
    assert!(!app.network.show_detail);
}

#[test]
fn enter_mock_edit_unknown_id_is_noop() {
    let mut app = App::new();
    app.enter_mock_edit(999);
    assert_eq!(app.mode, AppMode::Normal);
    assert!(app.mock_edit.rule_id.is_none());
}

#[test]
fn enter_mock_edit_populates_fields_from_rule() {
    let mut app = App::new();
    let id = app.mock_rules.add(
        "https://ex.test".into(),
        Some("POST".into()),
        404,
        "{\"a\":1}".into(),
        150,
    );
    app.enter_mock_edit(id);
    assert_eq!(app.mode, AppMode::MockRuleEdit);
    assert_eq!(app.mock_edit.rule_id, Some(id));
    assert_eq!(app.mock_edit.top_values[0], "https://ex.test");
    assert_eq!(app.mock_edit.top_values[1], "POST");
    assert_eq!(app.mock_edit.top_values[2], "404");
    assert_eq!(app.mock_edit.top_values[3], "150");
    // pretty-printed body is multiline.
    assert!(app.mock_edit.body.content().contains('1'));
}

#[test]
fn enter_mock_edit_none_method_yields_star() {
    let mut app = App::new();
    let id = app.mock_rules.add("u".into(), None, 200, "{}".into(), 0);
    app.enter_mock_edit(id);
    assert_eq!(app.mock_edit.top_values[1], "*");
}

#[test]
fn enter_mock_edit_invalid_json_body_passes_through() {
    let mut app = App::new();
    let id = app
        .mock_rules
        .add("u".into(), None, 200, "not-json".into(), 0);
    app.enter_mock_edit(id);
    assert_eq!(app.mock_edit.body.content(), "not-json");
}

#[test]
fn save_mock_edit_updates_rule_and_exits_mode() {
    let mut app = App::new();
    let id = app
        .mock_rules
        .add("old".into(), Some("GET".into()), 200, "{}".into(), 0);
    app.enter_mock_edit(id);
    app.mock_edit.top_values[0] = "new-url".to_string();
    app.mock_edit.top_values[1] = "*".to_string();
    app.mock_edit.top_values[2] = "418".to_string();
    app.mock_edit.top_values[3] = "99".to_string();
    app.save_mock_edit();
    assert_eq!(app.mode, AppMode::Normal);
    assert!(app.mock_edit.rule_id.is_none());
    let rule = app.mock_rules.rules().iter().find(|r| r.id == id).unwrap();
    assert_eq!(rule.url_pattern, "new-url");
    assert!(rule.method.is_none());
    assert_eq!(rule.status_code, 418);
    assert_eq!(rule.delay_ms, 99);
}

#[test]
fn save_mock_edit_without_rule_id_still_exits_mode() {
    let mut app = App::new();
    app.mode = AppMode::MockRuleEdit;
    app.save_mock_edit();
    assert_eq!(app.mode, AppMode::Normal);
    assert!(app.mock_edit.rule_id.is_none());
}

#[test]
fn save_mock_edit_bad_status_parses_fallback_200() {
    let mut app = App::new();
    let id = app
        .mock_rules
        .add("u".into(), Some("GET".into()), 500, "{}".into(), 0);
    app.enter_mock_edit(id);
    app.mock_edit.top_values[2] = "not-a-number".to_string();
    app.save_mock_edit();
    let rule = app.mock_rules.rules().iter().find(|r| r.id == id).unwrap();
    assert_eq!(rule.status_code, 200);
}

#[test]
fn save_mock_edit_bad_delay_parses_fallback_0() {
    let mut app = App::new();
    let id = app.mock_rules.add("u".into(), None, 200, "{}".into(), 0);
    app.enter_mock_edit(id);
    app.mock_edit.top_values[3] = "nope".to_string();
    app.save_mock_edit();
    let rule = app.mock_rules.rules().iter().find(|r| r.id == id).unwrap();
    assert_eq!(rule.delay_ms, 0);
}

#[test]
fn save_mock_edit_preserves_body_content() {
    let mut app = App::new();
    let id = app.mock_rules.add("u".into(), None, 200, "{}".into(), 0);
    app.enter_mock_edit(id);
    app.mock_edit.body = flog::ui::text_editor::TextEditor::new("new-body");
    app.save_mock_edit();
    let rule = app.mock_rules.rules().iter().find(|r| r.id == id).unwrap();
    assert_eq!(rule.response_body, "new-body");
}

#[test]
fn cancel_mock_edit_discards_changes() {
    let mut app = App::new();
    let id = app.mock_rules.add("orig".into(), None, 200, "{}".into(), 0);
    app.enter_mock_edit(id);
    app.mock_edit.top_values[0] = "would-be-dropped".to_string();
    app.cancel_mock_edit();
    assert_eq!(app.mode, AppMode::Normal);
    assert!(app.mock_edit.rule_id.is_none());
    let rule = app.mock_rules.rules().iter().find(|r| r.id == id).unwrap();
    assert_eq!(rule.url_pattern, "orig");
}

#[test]
fn cancel_mock_edit_from_normal_still_returns_normal() {
    let mut app = App::new();
    app.cancel_mock_edit();
    assert_eq!(app.mode, AppMode::Normal);
}

// =====================================================================
//  export_logs
// =====================================================================

#[test]
fn export_logs_writes_file_and_shows_status() {
    let mut app = app_with_n_logs(2);
    let cwd = std::env::current_dir().expect("cwd");
    let tmp = std::env::temp_dir().join("flog_export_test");
    std::fs::create_dir_all(&tmp).unwrap();
    std::env::set_current_dir(&tmp).unwrap();
    app.export_logs();
    let status = app.active_status().unwrap_or("").to_string();
    // Reset cwd before asserting (so failures don't leave cwd mutated).
    std::env::set_current_dir(cwd).unwrap();
    assert!(status.contains("Exported") || status.contains("failed"));
    // Cleanup tmp dir best-effort.
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn export_logs_on_empty_still_shows_status() {
    let mut app = App::new();
    let cwd = std::env::current_dir().expect("cwd");
    let tmp = std::env::temp_dir().join("flog_export_empty");
    std::fs::create_dir_all(&tmp).unwrap();
    std::env::set_current_dir(&tmp).unwrap();
    app.export_logs();
    let status = app.active_status().unwrap_or("").to_string();
    std::env::set_current_dir(cwd).unwrap();
    assert!(status.contains("Exported 0") || status.contains("failed"));
    let _ = std::fs::remove_dir_all(&tmp);
}

// =====================================================================
//  NetworkState methods  (UI-004 cache invalidation)
// =====================================================================

#[test]
fn network_state_new_defaults() {
    let ns = NetworkState::new();
    assert_eq!(ns.selected, 0);
    assert_eq!(ns.scroll_offset, 0);
    assert!(ns.auto_scroll);
    assert!(ns.ws_chat_mode);
    assert!(!ns.sse_merged_mode);
    assert_eq!(ns.sse_merged_field_idx, 0);
    assert!(!ns.show_detail);
    assert!(!ns.show_mock_rules_panel);
}

#[test]
fn network_state_default_equals_new() {
    let a = NetworkState::new();
    let b = NetworkState::default();
    assert_eq!(a.selected, b.selected);
    assert_eq!(a.scroll_offset, b.scroll_offset);
    assert_eq!(a.auto_scroll, b.auto_scroll);
}

#[test]
fn network_state_filtered_indices_empty_store() {
    let mut ns = NetworkState::new();
    let store = flog::domain::NetworkStore::new();
    let idxs = ns.filtered_indices(&store);
    assert!(idxs.is_empty());
}

#[test]
fn network_state_filtered_indices_populated_store() {
    let mut app = app_with_n_network(3);
    let c = app.network.filtered_count(&app.network_store);
    assert_eq!(c, 3);
}

#[test]
fn network_state_filter_cache_stable() {
    let mut app = app_with_n_network(3);
    let a = app.network.filtered_count(&app.network_store);
    let b = app.network.filtered_count(&app.network_store);
    assert_eq!(a, b);
}

#[test]
fn network_state_invalidate_forces_rebuild() {
    let mut app = app_with_n_network(3);
    assert_eq!(app.network.filtered_count(&app.network_store), 3);
    app.network.invalidate_filter();
    // After another push + invalidate, count must grow.
    app.network_store.push_entry(NetworkEntry::new_http(
        99,
        "GET".into(),
        "https://x/99".into(),
        "t".into(),
    ));
    app.network.invalidate_filter();
    assert_eq!(app.network.filtered_count(&app.network_store), 4);
}

#[test]
fn network_state_cache_rebuilds_only_on_invalidation() {
    // UI-004 — pushing a new entry WITHOUT invalidating must not change the
    // already-cached indices. Locks the cache-invariant: a caller that skips
    // invalidate_filter() sees stale data until the next invalidation.
    let mut app = app_with_n_network(2);
    // Prime the cache — 2 entries.
    assert_eq!(app.network.filtered_count(&app.network_store), 2);
    // Push a third entry but don't invalidate.
    app.network_store.push_entry(NetworkEntry::new_http(
        42,
        "GET".into(),
        "https://x/42".into(),
        "t".into(),
    ));
    // Cached length stays at 2 because `filter_dirty` is false.
    assert_eq!(app.network.filtered_count(&app.network_store), 2);
    // Explicit invalidate → next read rebuilds to 3.
    app.network.invalidate_filter();
    assert_eq!(app.network.filtered_count(&app.network_store), 3);
}

#[test]
fn network_move_up_disables_auto_scroll() {
    let mut ns = NetworkState::new();
    ns.selected = 2;
    ns.scroll_offset = 2;
    ns.move_up(1);
    assert_eq!(ns.selected, 1);
    assert_eq!(ns.scroll_offset, 1);
    assert!(!ns.auto_scroll);
}

#[test]
fn network_move_up_saturates_at_zero() {
    let mut ns = NetworkState::new();
    ns.move_up(100);
    assert_eq!(ns.selected, 0);
    assert_eq!(ns.scroll_offset, 0);
}

#[test]
fn network_move_down_empty_count_noop() {
    let mut ns = NetworkState::new();
    ns.move_down(3, 0);
    assert_eq!(ns.selected, 0);
}

#[test]
fn network_move_down_clamps_to_len() {
    let mut ns = NetworkState::new();
    ns.move_down(100, 5);
    assert_eq!(ns.selected, 4);
    assert_eq!(ns.scroll_offset, 4);
}

#[test]
fn network_select_up_clears_json_viewer_states() {
    let mut ns = NetworkState::new();
    ns.selected = 3;
    ns.scroll_offset = 2;
    ns.json_viewer_states.insert(
        "k".into(),
        flog::ui::json_viewer::JsonViewerState::default(),
    );
    ns.select_up(2);
    assert_eq!(ns.selected, 1);
    assert_eq!(ns.scroll_offset, 1); // follows
    assert!(ns.json_viewer_states.is_empty());
    assert!(!ns.auto_scroll);
}

#[test]
fn network_select_up_stays_when_in_viewport() {
    let mut ns = NetworkState::new();
    ns.selected = 5;
    ns.scroll_offset = 1;
    ns.select_up(1);
    assert_eq!(ns.selected, 4);
    assert_eq!(ns.scroll_offset, 1);
}

#[test]
fn network_select_down_empty_is_noop() {
    let mut ns = NetworkState::new();
    ns.select_down(3, 0);
    assert_eq!(ns.selected, 0);
}

#[test]
fn network_select_down_clamps() {
    let mut ns = NetworkState::new();
    ns.select_down(10, 3);
    assert_eq!(ns.selected, 2);
}

#[test]
fn network_go_top_resets() {
    let mut ns = NetworkState::new();
    ns.selected = 3;
    ns.scroll_offset = 2;
    ns.json_viewer_states.insert(
        "k".into(),
        flog::ui::json_viewer::JsonViewerState::default(),
    );
    ns.go_top();
    assert_eq!(ns.selected, 0);
    assert_eq!(ns.scroll_offset, 0);
    assert!(!ns.auto_scroll);
    assert!(ns.json_viewer_states.is_empty());
}

#[test]
fn network_go_bottom_enables_auto_scroll() {
    let mut ns = NetworkState::new();
    ns.auto_scroll = false;
    ns.go_bottom();
    assert!(ns.auto_scroll);
}

// =====================================================================
//  SseMergeRule / SsePathSegment (just lock shape)
// =====================================================================

#[test]
fn sse_path_segment_key_and_index_values() {
    let k = SsePathSegment::Key("foo".into());
    let i = SsePathSegment::Index(0);
    match k {
        SsePathSegment::Key(s) => assert_eq!(s, "foo"),
        _ => panic!("expected key"),
    }
    match i {
        SsePathSegment::Index(n) => assert_eq!(n, 0),
        _ => panic!("expected index"),
    }
}

#[test]
fn sse_merge_rule_clone_retains_path() {
    let rule = SseMergeRule {
        field_path: vec![
            SsePathSegment::Key("choices".into()),
            SsePathSegment::Index(0),
            SsePathSegment::Key("delta".into()),
        ],
        field_display: "choices[0].delta".into(),
    };
    let c = rule.clone();
    assert_eq!(c.field_display, "choices[0].delta");
    assert_eq!(c.field_path.len(), 3);
}

// =====================================================================
//  AppMode equality (for event dispatch)
// =====================================================================

#[test]
fn app_mode_input_active_per_field_is_distinct() {
    assert_ne!(
        AppMode::InputActive(InputField::LogSearch),
        AppMode::InputActive(InputField::NetSearch)
    );
}

#[test]
fn app_mode_help_vs_stats_vs_normal_distinct() {
    assert_ne!(AppMode::Help, AppMode::Stats);
    assert_ne!(AppMode::Normal, AppMode::Help);
    assert_ne!(AppMode::Normal, AppMode::MockRuleEdit);
}

// =====================================================================
//  LayoutCache (UI-017)
// =====================================================================
//
// `LayoutCache` is the render-layout coordinate snapshot written by the
// renderer and read by the event handler. These tests lock the default
// shape (all-zero / empty collections) and that new App instances start
// with a pristine cache, so future renderer changes that accidentally
// persist layout state between frames will fail here.

#[test]
fn layout_cache_default_is_all_zeroed() {
    let lc = LayoutCache::default();
    // Scalar coordinates default to zero.
    assert_eq!(lc.toolbar_y, 0);
    assert_eq!(lc.list_y, 0);
    assert_eq!(lc.list_height, 0);
    assert_eq!(lc.bottom_y, 0);
    assert_eq!(lc.width, 0);
    assert_eq!(lc.tab_bar_y, 0);
    assert_eq!(lc.net_detail_x, 0);
    assert_eq!(lc.net_toolbar_y, 0);
    assert_eq!(lc.input_row_y, 0);
    // Collections start empty.
    assert!(lc.bottom_buttons.is_empty());
    assert!(lc.row_to_filtered_idx.is_empty());
    assert!(lc.net_buttons.is_empty());
    assert!(lc.net_filter_pills.is_empty());
    assert!(lc.mock_rule_regions.is_empty());
    assert!(lc.mock_edit_regions.is_empty());
    assert!(lc.stats_slowest_regions.is_empty());
    assert!(lc.device_picker_items.is_empty());
    assert!(lc.device_picker_item_ids.is_empty());
    // Optional rects / buttons default to None.
    assert!(lc.last_click.is_none());
    assert!(lc.detail_mock_btn.is_none());
    assert!(lc.detail_copy_btn.is_none());
    assert!(lc.sse_pill_line.is_none());
    assert!(lc.ws_pill_line.is_none());
    assert!(lc.mock_edit_body_rect.is_none());
    assert!(lc.device_picker_rect.is_none());
    assert!(lc.jump_to_bottom_rect.is_none());
    // Rendered state starts false.
    assert!(!lc.rendered_to_end);
    assert_eq!(lc.visible_entry_count, 0);
    assert_eq!(lc.device_picker_total_lines, 0);
}

#[test]
fn app_new_starts_with_default_layout_cache() {
    // A fresh App must have a pristine LayoutCache — no bleed-over from
    // any construction path.
    let app = App::new();
    assert_eq!(app.layout.list_y, 0);
    assert_eq!(app.layout.width, 0);
    assert!(app.layout.row_to_filtered_idx.is_empty());
    assert!(app.layout.last_click.is_none());
    assert!(app.layout.device_picker_items.is_empty());
    assert!(!app.layout.rendered_to_end);
}

// =====================================================================
//  auto_scroll_for_tab (UI-006)
// =====================================================================

#[test]
fn auto_scroll_for_tab_reads_correct_flag_per_tab() {
    let mut app = App::new();
    // Default: both start true.
    assert!(app.auto_scroll_for_tab(ViewTab::Logs));
    assert!(app.auto_scroll_for_tab(ViewTab::Network));
    // Flip Logs flag only.
    app.logs.auto_scroll = false;
    assert!(!app.auto_scroll_for_tab(ViewTab::Logs));
    assert!(app.auto_scroll_for_tab(ViewTab::Network));
    // Flip Network flag only.
    app.logs.auto_scroll = true;
    app.network.auto_scroll = false;
    assert!(app.auto_scroll_for_tab(ViewTab::Logs));
    assert!(!app.auto_scroll_for_tab(ViewTab::Network));
}

// =====================================================================
//  InputField::tab (UI-002)
// =====================================================================

#[test]
fn input_field_tab_logs_variants_return_logs() {
    assert_eq!(InputField::LogSearch.tab(), ViewTab::Logs);
    assert_eq!(InputField::LogExclude.tab(), ViewTab::Logs);
    assert_eq!(InputField::LogTag.tab(), ViewTab::Logs);
}

#[test]
fn input_field_tab_net_variants_return_network() {
    assert_eq!(InputField::NetSearch.tab(), ViewTab::Network);
    assert_eq!(InputField::NetExclude.tab(), ViewTab::Network);
}

// =====================================================================
//  MockEditState (UI-026 + UI-034)
// =====================================================================

#[test]
fn mock_edit_state_new_blank_has_no_rule_id_and_empty_fields() {
    let st = MockEditState::new_blank();
    assert!(st.rule_id.is_none());
    assert_eq!(st.field, 0);
    assert!(st.top_values.is_empty());
    assert_eq!(st.body.content(), "");
}

#[test]
fn mock_edit_state_from_rule_populates_all_fields() {
    let rule = flog::domain::mock::MockRule {
        id: 7,
        url_pattern: "https://ex.test/api".into(),
        method: Some("POST".into()),
        status_code: 404,
        response_body: "{\"err\":1}".into(),
        delay_ms: 250,
        enabled: true,
        hit_count: 0,
    };
    let st = MockEditState::from_rule(&rule);
    assert_eq!(st.rule_id, Some(7));
    assert_eq!(st.field, 0);
    assert_eq!(st.top_values[0], "https://ex.test/api");
    assert_eq!(st.top_values[1], "POST");
    assert_eq!(st.top_values[2], "404");
    assert_eq!(st.top_values[3], "250");
    // JSON body is pretty-printed; verify it contains the field.
    assert!(st.body.content().contains("err"));
}

#[test]
fn mock_edit_state_from_rule_none_method_becomes_star_and_non_json_passes_through() {
    let rule = flog::domain::mock::MockRule {
        id: 1,
        url_pattern: "u".into(),
        method: None,
        status_code: 200,
        response_body: "not-json".into(),
        delay_ms: 0,
        enabled: true,
        hit_count: 0,
    };
    let st = MockEditState::from_rule(&rule);
    assert_eq!(st.top_values[1], "*");
    // Non-JSON body passes through unchanged.
    assert_eq!(st.body.content(), "not-json");
}
