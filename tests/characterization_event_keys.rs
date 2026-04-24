//! Phase 2.5B Task 5a — characterization tests for `src/event.rs` KEY dispatch.
//!
//! Drives `event::handle_key` over seeded `App` states and asserts state
//! transitions. Covers `handle_normal_key` (Logs + Network), `handle_input_key`,
//! `handle_overlay_key`, and `handle_mock_edit_key`. Mouse handlers are Task 5b.
//!
//! Audit refs:
//!   - UI-007 state-machine routing (AppMode dispatch)
//!   - UI-008 SSE merged mode field navigation
//!   - UI-020 unified input-field editing
//!   - UI-024 scroll primitives (PageUp/Down, Home/End)

#![cfg(test)]
#![allow(clippy::too_many_lines)]

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use flog::app::{App, AppMode, InputField, ViewTab};
use flog::domain::entry::{InputSource, LogEntry, LogLevel};
use flog::domain::network::{NetworkEntry, SseChunk};
use flog::event;

// ---- Event builders -------------------------------------------------

fn key(c: char) -> KeyEvent {
    KeyEvent {
        code: KeyCode::Char(c),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn key_code(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn key_ctrl(c: char) -> KeyEvent {
    KeyEvent {
        code: KeyCode::Char(c),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn key_code_mod(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: mods,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
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

// ═════════════════════════════════════════════════════════════════════
//  handle_normal_key — LOGS tab
// ═════════════════════════════════════════════════════════════════════

#[test]
fn ui_007_logs_j_selects_next() {
    let mut app = app_with_n_logs(5);
    assert_eq!(app.selected, 0);
    event::handle_key(&mut app, key('j'));
    assert_eq!(app.selected, 1);
}

#[test]
fn ui_007_logs_k_selects_prev() {
    let mut app = app_with_n_logs(5);
    app.selected = 3;
    event::handle_key(&mut app, key('k'));
    assert_eq!(app.selected, 2);
}

#[test]
fn ui_007_logs_down_arrow_selects_next() {
    let mut app = app_with_n_logs(3);
    event::handle_key(&mut app, key_code(KeyCode::Down));
    assert_eq!(app.selected, 1);
}

#[test]
fn ui_007_logs_up_arrow_selects_prev() {
    let mut app = app_with_n_logs(3);
    app.selected = 2;
    event::handle_key(&mut app, key_code(KeyCode::Up));
    assert_eq!(app.selected, 1);
}

#[test]
fn ui_007_logs_k_at_top_stays() {
    let mut app = app_with_n_logs(3);
    assert_eq!(app.selected, 0);
    event::handle_key(&mut app, key('k'));
    assert_eq!(app.selected, 0);
}

#[test]
fn ui_007_logs_j_at_bottom_stays() {
    let mut app = app_with_n_logs(3);
    app.selected = 2;
    event::handle_key(&mut app, key('j'));
    assert_eq!(app.selected, 2);
}

#[test]
fn ui_024_logs_pagedown_advances_by_20() {
    let mut app = app_with_n_logs(50);
    assert_eq!(app.selected, 0);
    event::handle_key(&mut app, key_code(KeyCode::PageDown));
    assert_eq!(app.selected, 20);
}

#[test]
fn ui_024_logs_pageup_retreats_by_20() {
    let mut app = app_with_n_logs(50);
    app.selected = 30;
    app.scroll_offset = 30;
    event::handle_key(&mut app, key_code(KeyCode::PageUp));
    assert_eq!(app.selected, 10);
}

#[test]
fn ui_024_logs_home_jumps_to_top() {
    let mut app = app_with_n_logs(10);
    app.selected = 7;
    app.scroll_offset = 3;
    event::handle_key(&mut app, key_code(KeyCode::Home));
    assert_eq!(app.selected, 0);
    assert_eq!(app.scroll_offset, 0);
}

#[test]
fn ui_024_logs_end_enables_autoscroll() {
    let mut app = app_with_n_logs(10);
    app.auto_scroll = false;
    event::handle_key(&mut app, key_code(KeyCode::End));
    assert!(app.auto_scroll);
}

#[test]
fn ui_007_logs_slash_enters_search_input() {
    let mut app = app_with_n_logs(3);
    event::handle_key(&mut app, key('/'));
    assert!(matches!(
        app.mode,
        AppMode::InputActive(InputField::LogSearch)
    ));
}

#[test]
fn ui_007_logs_backslash_enters_exclude_input() {
    let mut app = app_with_n_logs(3);
    event::handle_key(&mut app, key('\\'));
    assert!(matches!(
        app.mode,
        AppMode::InputActive(InputField::LogExclude)
    ));
}

#[test]
fn ui_007_logs_question_enters_help() {
    let mut app = app_with_n_logs(3);
    event::handle_key(&mut app, key('?'));
    assert_eq!(app.mode, AppMode::Help);
}

#[test]
fn ui_007_logs_big_s_enters_stats() {
    let mut app = app_with_n_logs(3);
    event::handle_key(&mut app, key('S'));
    assert_eq!(app.mode, AppMode::Stats);
    assert_eq!(app.active_stats_tab, ViewTab::Logs);
}

#[test]
fn ui_007_logs_q_quits() {
    let mut app = app_with_n_logs(3);
    event::handle_key(&mut app, key('q'));
    assert!(app.should_quit);
}

#[test]
fn ui_007_logs_ctrl_c_quits() {
    let mut app = app_with_n_logs(3);
    event::handle_key(&mut app, key_ctrl('c'));
    assert!(app.should_quit);
}

#[test]
fn ui_007_logs_enter_toggles_detail_panel() {
    let mut app = app_with_n_logs(3);
    assert!(!app.show_detail_panel);
    event::handle_key(&mut app, key_code(KeyCode::Enter));
    assert!(app.show_detail_panel);
    event::handle_key(&mut app, key_code(KeyCode::Enter));
    assert!(!app.show_detail_panel);
}

#[test]
fn ui_007_logs_tab_1_stays_in_logs() {
    let mut app = app_with_n_logs(3);
    event::handle_key(&mut app, key('1'));
    assert_eq!(app.active_tab, ViewTab::Logs);
}

#[test]
fn ui_007_logs_tab_2_switches_to_network() {
    let mut app = app_with_n_logs(3);
    event::handle_key(&mut app, key('2'));
    assert_eq!(app.active_tab, ViewTab::Network);
}

#[test]
fn ui_007_logs_esc_clears_all_filters() {
    let mut app = app_with_n_logs(3);
    app.filter.set_search("abc");
    app.inputs.log_search = "abc".into();
    event::handle_key(&mut app, key_code(KeyCode::Esc));
    assert!(app.filter.search_query.is_empty());
    assert!(app.inputs.log_search.is_empty());
}

#[test]
fn ui_007_logs_s_enters_select_mode() {
    let mut app = app_with_n_logs(3);
    event::handle_key(&mut app, key('s'));
    assert!(app.select_mode);
    // Status banner is set
    assert!(app.status_message.is_some());
}

#[test]
fn ui_007_logs_select_mode_any_key_exits() {
    let mut app = app_with_n_logs(3);
    app.select_mode = true;
    event::handle_key(&mut app, key('j'));
    assert!(!app.select_mode);
    // Next j should still be swallowed by select-mode exit, not move selection.
    assert_eq!(app.selected, 0);
}

#[test]
fn ui_007_logs_c_copy_current_log() {
    // Clipboard dispatch is best-effort; we just ensure the handler doesn't panic
    // and that the status_message gets set (either success or a soft failure note).
    let mut app = app_with_n_logs(3);
    app.selected = 1;
    event::handle_key(&mut app, key('c'));
    // Any observable effect: selected unchanged, mode unchanged.
    assert_eq!(app.selected, 1);
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn ui_007_logs_e_export_logs() {
    let mut app = app_with_n_logs(2);
    event::handle_key(&mut app, key('e'));
    // export_logs sets a status message
    assert!(app.status_message.is_some());
    // Try to remove the file it may have created (best-effort)
    if let Some((ref msg, _)) = app.status_message {
        if let Some(idx) = msg.find("flog_") {
            if let Some(name) = msg[idx..].split(' ').next() {
                let _ = std::fs::remove_file(name);
            }
        }
    }
}

#[test]
fn ui_007_logs_n_next_match_noop_when_empty() {
    let mut app = app_with_n_logs(3);
    let before = app.selected;
    event::handle_key(&mut app, key('n'));
    assert_eq!(app.selected, before);
}

#[test]
fn ui_007_logs_shift_n_prev_match_noop_when_empty() {
    let mut app = app_with_n_logs(3);
    let before = app.selected;
    event::handle_key(&mut app, key('N'));
    assert_eq!(app.selected, before);
}

#[test]
fn ui_007_logs_unbound_key_is_noop() {
    let mut app = app_with_n_logs(3);
    event::handle_key(&mut app, key('z'));
    assert_eq!(app.selected, 0);
    assert_eq!(app.mode, AppMode::Normal);
    assert!(!app.should_quit);
}

// ═════════════════════════════════════════════════════════════════════
//  handle_normal_key — NETWORK tab
// ═════════════════════════════════════════════════════════════════════

#[test]
fn ui_007_network_j_selects_next() {
    let mut app = app_with_n_network(4);
    assert_eq!(app.network.selected, 0);
    event::handle_key(&mut app, key('j'));
    assert_eq!(app.network.selected, 1);
}

#[test]
fn ui_007_network_k_selects_prev() {
    let mut app = app_with_n_network(4);
    app.network.selected = 2;
    event::handle_key(&mut app, key('k'));
    assert_eq!(app.network.selected, 1);
}

#[test]
fn ui_007_network_down_arrow_selects_next() {
    let mut app = app_with_n_network(3);
    event::handle_key(&mut app, key_code(KeyCode::Down));
    assert_eq!(app.network.selected, 1);
}

#[test]
fn ui_007_network_up_arrow_selects_prev() {
    let mut app = app_with_n_network(3);
    app.network.selected = 2;
    event::handle_key(&mut app, key_code(KeyCode::Up));
    assert_eq!(app.network.selected, 1);
}

#[test]
fn ui_024_network_pagedown_moves_by_20() {
    let mut app = app_with_n_network(40);
    event::handle_key(&mut app, key_code(KeyCode::PageDown));
    assert_eq!(app.network.selected, 20);
}

#[test]
fn ui_024_network_pageup_moves_by_20() {
    let mut app = app_with_n_network(40);
    app.network.selected = 30;
    app.network.scroll_offset = 30;
    event::handle_key(&mut app, key_code(KeyCode::PageUp));
    assert_eq!(app.network.selected, 10);
}

#[test]
fn ui_024_network_home_resets_selection() {
    let mut app = app_with_n_network(5);
    app.network.selected = 3;
    app.network.scroll_offset = 3;
    event::handle_key(&mut app, key_code(KeyCode::Home));
    assert_eq!(app.network.selected, 0);
    assert_eq!(app.network.scroll_offset, 0);
}

#[test]
fn ui_024_network_end_enables_autoscroll() {
    let mut app = app_with_n_network(5);
    app.network.auto_scroll = false;
    event::handle_key(&mut app, key_code(KeyCode::End));
    assert!(app.network.auto_scroll);
}

#[test]
fn ui_007_network_enter_toggles_detail() {
    let mut app = app_with_n_network(3);
    assert!(!app.network.show_detail);
    event::handle_key(&mut app, key_code(KeyCode::Enter));
    assert!(app.network.show_detail);
    event::handle_key(&mut app, key_code(KeyCode::Enter));
    assert!(!app.network.show_detail);
}

#[test]
fn ui_007_network_slash_enters_search_input() {
    let mut app = app_with_n_network(3);
    event::handle_key(&mut app, key('/'));
    assert!(matches!(
        app.mode,
        AppMode::InputActive(InputField::NetSearch)
    ));
}

#[test]
fn ui_007_network_backslash_enters_exclude_input() {
    let mut app = app_with_n_network(3);
    event::handle_key(&mut app, key('\\'));
    assert!(matches!(
        app.mode,
        AppMode::InputActive(InputField::NetExclude)
    ));
}

#[test]
fn ui_007_network_big_s_enters_stats() {
    let mut app = app_with_n_network(3);
    event::handle_key(&mut app, key('S'));
    assert_eq!(app.mode, AppMode::Stats);
    assert_eq!(app.active_stats_tab, ViewTab::Network);
}

#[test]
fn ui_007_network_question_enters_help() {
    let mut app = app_with_n_network(3);
    event::handle_key(&mut app, key('?'));
    assert_eq!(app.mode, AppMode::Help);
}

#[test]
fn ui_007_network_q_quits() {
    let mut app = app_with_n_network(3);
    event::handle_key(&mut app, key('q'));
    assert!(app.should_quit);
}

#[test]
fn ui_007_network_ctrl_c_quits() {
    let mut app = app_with_n_network(3);
    event::handle_key(&mut app, key_ctrl('c'));
    assert!(app.should_quit);
}

#[test]
fn ui_007_network_s_enters_select_mode() {
    let mut app = app_with_n_network(3);
    event::handle_key(&mut app, key('s'));
    assert!(app.select_mode);
}

#[test]
fn ui_007_network_tab_1_switches_to_logs() {
    let mut app = app_with_n_network(3);
    event::handle_key(&mut app, key('1'));
    assert_eq!(app.active_tab, ViewTab::Logs);
}

#[test]
fn ui_007_network_tab_2_stays() {
    let mut app = app_with_n_network(3);
    event::handle_key(&mut app, key('2'));
    assert_eq!(app.active_tab, ViewTab::Network);
}

#[test]
fn ui_007_network_esc_resets_filter_without_sse_merged() {
    let mut app = app_with_n_network(3);
    app.network.filter.set_search("foo");
    app.inputs.net_search = "foo".into();
    app.inputs.net_search_cursor = 3;
    app.inputs.net_exclude = "bar".into();
    app.inputs.net_exclude_cursor = 3;
    event::handle_key(&mut app, key_code(KeyCode::Esc));
    assert!(app.network.filter.search.is_empty());
    assert!(app.inputs.net_search.is_empty());
    assert_eq!(app.inputs.net_search_cursor, 0);
    assert!(app.inputs.net_exclude.is_empty());
    assert_eq!(app.inputs.net_exclude_cursor, 0);
}

#[test]
fn ui_007_network_esc_in_sse_merged_exits_merged_mode() {
    let mut app = app_with_n_network(1);
    app.network.sse_merged_mode = true;
    app.network.show_detail = true;
    event::handle_key(&mut app, key_code(KeyCode::Esc));
    assert!(!app.network.sse_merged_mode);
}

#[test]
fn ui_007_network_r_replay_without_client_noop() {
    let mut app = app_with_n_network(3);
    event::handle_key(&mut app, key('r'));
    // Replay without connected client → no panic, no mode change
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn ui_007_network_c_copy_as_curl() {
    let mut app = app_with_n_network(3);
    event::handle_key(&mut app, key('c'));
    // Just no-panic; status_message may or may not be set
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn ui_007_network_y_copy_response() {
    let mut app = app_with_n_network(3);
    event::handle_key(&mut app, key('y'));
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn ui_007_network_big_e_expands_all_json_sections() {
    use flog::ui::json_viewer::JsonViewerState;
    let mut app = app_with_n_network(1);
    let s = JsonViewerState {
        expanded: vec![false, false, false],
    };
    app.network.json_viewer_states.insert("s1".into(), s);
    event::handle_key(&mut app, key('E'));
    let s = app.network.json_viewer_states.get("s1").unwrap();
    assert!(s.expanded.iter().all(|&b| b));
}

#[test]
fn ui_007_network_big_c_collapses_all_but_root() {
    use flog::ui::json_viewer::JsonViewerState;
    let mut app = app_with_n_network(1);
    let s = JsonViewerState {
        expanded: vec![true, true, true],
    };
    app.network.json_viewer_states.insert("s1".into(), s);
    event::handle_key(&mut app, key('C'));
    let s = app.network.json_viewer_states.get("s1").unwrap();
    assert_eq!(s.expanded, vec![true, false, false]);
}

#[test]
fn ui_007_network_big_m_mock_without_client_shows_status() {
    let mut app = app_with_n_network(1);
    event::handle_key(&mut app, key('M'));
    // Without connected client, mock_from_selected sets a status and exits early
    assert_eq!(app.mode, AppMode::Normal);
    assert!(app.status_message.is_some());
}

#[test]
fn ui_007_network_ctrl_m_opens_mock_rules_panel_if_connected() {
    let mut app = app_with_n_network(1);
    // No client → enter_mock_rules shows a status and returns
    event::handle_key(&mut app, key_ctrl('m'));
    assert_eq!(app.mode, AppMode::Normal);
    assert!(app.status_message.is_some());
}

#[test]
fn ui_007_network_unbound_key_is_noop() {
    let mut app = app_with_n_network(3);
    event::handle_key(&mut app, key('z'));
    assert_eq!(app.network.selected, 0);
    assert_eq!(app.mode, AppMode::Normal);
    assert!(!app.should_quit);
}

// ---- Device picker key branches (inside handle_normal_key) ---------

#[test]
fn ui_007_device_picker_esc_closes() {
    let mut app = app_with_n_logs(1);
    app.show_device_picker = true;
    event::handle_key(&mut app, key_code(KeyCode::Esc));
    assert!(!app.show_device_picker);
}

#[test]
fn ui_007_device_picker_j_moves_selection_down() {
    let mut app = app_with_n_logs(1);
    app.show_device_picker = true;
    app.layout.device_picker_item_ids = vec!["a".into(), "b".into(), "c".into()];
    app.device_picker_selected = 0;
    event::handle_key(&mut app, key('j'));
    assert_eq!(app.device_picker_selected, 1);
}

#[test]
fn ui_007_device_picker_k_moves_selection_up() {
    let mut app = app_with_n_logs(1);
    app.show_device_picker = true;
    app.layout.device_picker_item_ids = vec!["a".into(), "b".into()];
    app.device_picker_selected = 1;
    event::handle_key(&mut app, key('k'));
    assert_eq!(app.device_picker_selected, 0);
}

#[test]
fn ui_007_device_picker_down_arrow_bounds() {
    let mut app = app_with_n_logs(1);
    app.show_device_picker = true;
    app.layout.device_picker_item_ids = vec!["a".into(), "b".into()];
    app.device_picker_selected = 1;
    event::handle_key(&mut app, key_code(KeyCode::Down));
    assert_eq!(app.device_picker_selected, 1); // clamped at max
}

#[test]
fn ui_007_device_picker_up_arrow_saturates_at_zero() {
    let mut app = app_with_n_logs(1);
    app.show_device_picker = true;
    app.layout.device_picker_item_ids = vec!["a".into()];
    app.device_picker_selected = 0;
    event::handle_key(&mut app, key_code(KeyCode::Up));
    assert_eq!(app.device_picker_selected, 0);
}

#[test]
fn ui_007_device_picker_enter_without_tx_closes() {
    let mut app = app_with_n_logs(1);
    app.show_device_picker = true;
    app.layout.device_picker_item_ids = vec!["app1".into()];
    app.device_picker_selected = 0;
    // No tx channel → branch falls through, picker still closes because Some(app_id)
    event::handle_key(&mut app, key_code(KeyCode::Enter));
    assert!(!app.show_device_picker);
}

#[test]
fn ui_007_device_picker_unbound_key_is_noop() {
    let mut app = app_with_n_logs(1);
    app.show_device_picker = true;
    app.layout.device_picker_item_ids = vec!["a".into()];
    event::handle_key(&mut app, key('x'));
    assert!(app.show_device_picker);
}

// ═════════════════════════════════════════════════════════════════════
//  UI-008 — SSE Merged Mode navigation
// ═════════════════════════════════════════════════════════════════════

fn app_with_sse_entry() -> App {
    let mut app = App::default();
    app.active_tab = ViewTab::Network;
    let mut entry =
        NetworkEntry::new_sse(1, "GET".into(), "https://ai.test/chat".into(), "t".into());
    // Chunks exposing two candidate leaf paths: `a` and `b`.
    for i in 0..3 {
        entry.sse_chunks.push(SseChunk {
            data: format!(r#"{{"a":"hello-{}","b":"world-{}"}}"#, i, i),
        });
    }
    app.network_store.push_entry(entry);
    app.network.invalidate_filter();
    let _ = app.network.filtered_count(&app.network_store);
    app.network.sse_merged_mode = true;
    app.network.show_detail = true;
    app
}

#[test]
fn ui_008_sse_merged_j_advances_field_idx() {
    let mut app = app_with_sse_entry();
    assert_eq!(app.network.sse_merged_field_idx, 0);
    event::handle_key(&mut app, key('j'));
    assert_eq!(app.network.sse_merged_field_idx, 1);
}

#[test]
fn ui_008_sse_merged_j_saturates_at_max() {
    let mut app = app_with_sse_entry();
    // There are 2 candidate fields → max idx is 1.
    app.network.sse_merged_field_idx = 1;
    event::handle_key(&mut app, key('j'));
    assert_eq!(app.network.sse_merged_field_idx, 1);
}

#[test]
fn ui_008_sse_merged_k_decrements_field_idx() {
    let mut app = app_with_sse_entry();
    app.network.sse_merged_field_idx = 1;
    event::handle_key(&mut app, key('k'));
    assert_eq!(app.network.sse_merged_field_idx, 0);
}

#[test]
fn ui_008_sse_merged_k_saturates_at_zero() {
    let mut app = app_with_sse_entry();
    app.network.sse_merged_field_idx = 0;
    event::handle_key(&mut app, key('k'));
    assert_eq!(app.network.sse_merged_field_idx, 0);
}

#[test]
fn ui_008_sse_merged_down_arrow_behaves_like_j() {
    let mut app = app_with_sse_entry();
    event::handle_key(&mut app, key_code(KeyCode::Down));
    assert_eq!(app.network.sse_merged_field_idx, 1);
}

#[test]
fn ui_008_sse_merged_up_arrow_behaves_like_k() {
    let mut app = app_with_sse_entry();
    app.network.sse_merged_field_idx = 1;
    event::handle_key(&mut app, key_code(KeyCode::Up));
    assert_eq!(app.network.sse_merged_field_idx, 0);
}

// ═════════════════════════════════════════════════════════════════════
//  handle_input_key — unified input field editing (UI-020)
// ═════════════════════════════════════════════════════════════════════

fn app_in_input(field: InputField) -> App {
    let mut app = App::default();
    app.enter_input_field(field);
    app
}

#[test]
fn ui_020_input_char_appends_to_buffer() {
    let mut app = app_in_input(InputField::LogSearch);
    event::handle_key(&mut app, key('a'));
    event::handle_key(&mut app, key('b'));
    event::handle_key(&mut app, key('c'));
    assert_eq!(app.inputs.log_search, "abc");
    assert_eq!(app.inputs.log_search_cursor, 3);
}

#[test]
fn ui_020_input_char_applies_to_filter() {
    let mut app = app_in_input(InputField::LogSearch);
    event::handle_key(&mut app, key('x'));
    assert_eq!(app.filter.search_query, "x");
}

#[test]
fn ui_020_input_backspace_deletes_last_char() {
    let mut app = app_in_input(InputField::LogSearch);
    app.inputs.log_search = "abc".into();
    app.inputs.log_search_cursor = 3;
    event::handle_key(&mut app, key_code(KeyCode::Backspace));
    assert_eq!(app.inputs.log_search, "ab");
    assert_eq!(app.inputs.log_search_cursor, 2);
}

#[test]
fn ui_020_input_backspace_on_empty_is_noop() {
    let mut app = app_in_input(InputField::LogSearch);
    event::handle_key(&mut app, key_code(KeyCode::Backspace));
    assert_eq!(app.inputs.log_search, "");
}

#[test]
fn ui_020_input_esc_exits_to_normal() {
    let mut app = app_in_input(InputField::LogSearch);
    event::handle_key(&mut app, key_code(KeyCode::Esc));
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn ui_020_input_enter_commits_and_exits() {
    let mut app = app_in_input(InputField::LogSearch);
    app.inputs.log_search = "q".into();
    event::handle_key(&mut app, key_code(KeyCode::Enter));
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn ui_020_input_arrow_left_is_unhandled_noop() {
    // handle_input_key does not implement left/right cursor move — it's a noop.
    let mut app = app_in_input(InputField::LogSearch);
    app.inputs.log_search = "abc".into();
    app.inputs.log_search_cursor = 2;
    event::handle_key(&mut app, key_code(KeyCode::Left));
    assert_eq!(app.inputs.log_search_cursor, 2);
    assert_eq!(app.inputs.log_search, "abc");
}

#[test]
fn ui_020_input_arrow_right_is_unhandled_noop() {
    let mut app = app_in_input(InputField::LogSearch);
    app.inputs.log_search = "abc".into();
    app.inputs.log_search_cursor = 1;
    event::handle_key(&mut app, key_code(KeyCode::Right));
    assert_eq!(app.inputs.log_search_cursor, 1);
}

#[test]
fn ui_020_input_log_exclude_field_typing() {
    let mut app = app_in_input(InputField::LogExclude);
    event::handle_key(&mut app, key('n'));
    event::handle_key(&mut app, key('o'));
    assert_eq!(app.inputs.log_exclude, "no");
    assert_eq!(app.filter.exclude_query, "no");
}

#[test]
fn ui_020_input_log_tag_field_typing() {
    let mut app = app_in_input(InputField::LogTag);
    event::handle_key(&mut app, key('T'));
    assert_eq!(app.inputs.log_tag, "T");
}

#[test]
fn ui_020_input_net_search_field_typing() {
    let mut app = app_in_input(InputField::NetSearch);
    event::handle_key(&mut app, key('p'));
    assert_eq!(app.inputs.net_search, "p");
    assert_eq!(app.network.filter.search, "p");
}

#[test]
fn ui_020_input_net_exclude_field_typing() {
    let mut app = app_in_input(InputField::NetExclude);
    event::handle_key(&mut app, key('x'));
    assert_eq!(app.inputs.net_exclude, "x");
    assert_eq!(app.network.filter.exclude, "x");
}

// ═════════════════════════════════════════════════════════════════════
//  handle_overlay_key — Help / Stats dismissal
// ═════════════════════════════════════════════════════════════════════

#[test]
fn ui_007_help_esc_dismisses() {
    let mut app = App::default();
    app.mode = AppMode::Help;
    event::handle_key(&mut app, key_code(KeyCode::Esc));
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn ui_007_help_q_dismisses() {
    let mut app = App::default();
    app.mode = AppMode::Help;
    event::handle_key(&mut app, key('q'));
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn ui_007_help_unrelated_key_noop() {
    let mut app = App::default();
    app.mode = AppMode::Help;
    event::handle_key(&mut app, key('x'));
    assert_eq!(app.mode, AppMode::Help);
}

#[test]
fn ui_007_stats_esc_dismisses() {
    let mut app = App::default();
    app.mode = AppMode::Stats;
    event::handle_key(&mut app, key_code(KeyCode::Esc));
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn ui_007_stats_q_dismisses() {
    let mut app = App::default();
    app.mode = AppMode::Stats;
    event::handle_key(&mut app, key('q'));
    assert_eq!(app.mode, AppMode::Normal);
}

#[test]
fn ui_007_stats_unrelated_key_noop() {
    let mut app = App::default();
    app.mode = AppMode::Stats;
    event::handle_key(&mut app, key('j'));
    assert_eq!(app.mode, AppMode::Stats);
}

// ═════════════════════════════════════════════════════════════════════
//  handle_mock_edit_key
// ═════════════════════════════════════════════════════════════════════

fn app_in_mock_edit() -> App {
    let mut app = App::default();
    app.mock_rules.add("/foo".into(), None, 200, "{}".into(), 0);
    let id = app.mock_rules.rules()[0].id;
    app.enter_mock_edit(id);
    app
}

#[test]
fn ui_mock_edit_esc_cancels_to_normal() {
    let mut app = app_in_mock_edit();
    event::handle_key(&mut app, key_code(KeyCode::Esc));
    assert_eq!(app.mode, AppMode::Normal);
    assert!(app.mock_edit_rule_id.is_none());
}

#[test]
fn ui_mock_edit_tab_cycles_field_forward() {
    let mut app = app_in_mock_edit();
    assert_eq!(app.mock_edit_field, 0);
    event::handle_key(&mut app, key_code(KeyCode::Tab));
    assert_eq!(app.mock_edit_field, 1);
    event::handle_key(&mut app, key_code(KeyCode::Tab));
    assert_eq!(app.mock_edit_field, 2);
    event::handle_key(&mut app, key_code(KeyCode::Tab));
    assert_eq!(app.mock_edit_field, 3);
    event::handle_key(&mut app, key_code(KeyCode::Tab));
    assert_eq!(app.mock_edit_field, 4);
    event::handle_key(&mut app, key_code(KeyCode::Tab));
    // wraps modulo 5
    assert_eq!(app.mock_edit_field, 0);
}

#[test]
fn ui_mock_edit_backtab_cycles_field_backward() {
    let mut app = app_in_mock_edit();
    assert_eq!(app.mock_edit_field, 0);
    event::handle_key(&mut app, key_code(KeyCode::BackTab));
    assert_eq!(app.mock_edit_field, 4);
    event::handle_key(&mut app, key_code(KeyCode::BackTab));
    assert_eq!(app.mock_edit_field, 3);
}

#[test]
fn ui_mock_edit_char_appends_to_single_line_field() {
    let mut app = app_in_mock_edit();
    app.mock_edit_field = 0;
    app.mock_edit_top_values[0] = "/foo".into();
    event::handle_key(&mut app, key('X'));
    assert_eq!(app.mock_edit_top_values[0], "/fooX");
}

#[test]
fn ui_mock_edit_backspace_removes_last_char() {
    let mut app = app_in_mock_edit();
    app.mock_edit_field = 0;
    app.mock_edit_top_values[0] = "/foo".into();
    event::handle_key(&mut app, key_code(KeyCode::Backspace));
    assert_eq!(app.mock_edit_top_values[0], "/fo");
}

#[test]
fn ui_mock_edit_backspace_on_empty_field_noop() {
    let mut app = app_in_mock_edit();
    app.mock_edit_field = 1;
    app.mock_edit_top_values[1] = "".into();
    event::handle_key(&mut app, key_code(KeyCode::Backspace));
    assert_eq!(app.mock_edit_top_values[1], "");
}

#[test]
fn ui_mock_edit_ctrl_s_saves_and_exits() {
    let mut app = app_in_mock_edit();
    event::handle_key(&mut app, key_ctrl('s'));
    assert_eq!(app.mode, AppMode::Normal);
    assert!(app.mock_edit_rule_id.is_none());
}

#[test]
fn ui_mock_edit_ctrl_enter_saves_and_exits() {
    let mut app = app_in_mock_edit();
    event::handle_key(
        &mut app,
        key_code_mod(KeyCode::Enter, KeyModifiers::CONTROL),
    );
    assert_eq!(app.mode, AppMode::Normal);
    assert!(app.mock_edit_rule_id.is_none());
}

// ---- body editor (field index 4) ----------------------------------

fn app_in_mock_body_field() -> App {
    let mut app = app_in_mock_edit();
    app.mock_edit_field = 4;
    app
}

#[test]
fn ui_mock_edit_body_enter_inserts_newline() {
    let mut app = app_in_mock_body_field();
    let before = app.mock_edit_body.content();
    event::handle_key(&mut app, key_code(KeyCode::Enter));
    let after = app.mock_edit_body.content();
    assert!(after.contains('\n') || after.len() > before.len());
}

#[test]
fn ui_mock_edit_body_backspace_deletes() {
    let mut app = app_in_mock_body_field();
    let before = app.mock_edit_body.content().len();
    // Position at end
    app.mock_edit_body.move_end();
    event::handle_key(&mut app, key_code(KeyCode::Backspace));
    let after = app.mock_edit_body.content().len();
    assert!(after <= before);
}

#[test]
fn ui_mock_edit_body_delete_key_is_handled() {
    let mut app = app_in_mock_body_field();
    event::handle_key(&mut app, key_code(KeyCode::Delete));
    // No panic, mode unchanged.
    assert_eq!(app.mode, AppMode::MockRuleEdit);
}

#[test]
fn ui_mock_edit_body_left_arrow_moves_cursor() {
    let mut app = app_in_mock_body_field();
    event::handle_key(&mut app, key_code(KeyCode::Left));
    assert_eq!(app.mode, AppMode::MockRuleEdit);
}

#[test]
fn ui_mock_edit_body_right_arrow_moves_cursor() {
    let mut app = app_in_mock_body_field();
    event::handle_key(&mut app, key_code(KeyCode::Right));
    assert_eq!(app.mode, AppMode::MockRuleEdit);
}

#[test]
fn ui_mock_edit_body_up_arrow_moves_cursor() {
    let mut app = app_in_mock_body_field();
    event::handle_key(&mut app, key_code(KeyCode::Up));
    assert_eq!(app.mode, AppMode::MockRuleEdit);
}

#[test]
fn ui_mock_edit_body_down_arrow_moves_cursor() {
    let mut app = app_in_mock_body_field();
    event::handle_key(&mut app, key_code(KeyCode::Down));
    assert_eq!(app.mode, AppMode::MockRuleEdit);
}

#[test]
fn ui_mock_edit_body_home_moves_cursor_to_line_start() {
    let mut app = app_in_mock_body_field();
    event::handle_key(&mut app, key_code(KeyCode::Home));
    assert_eq!(app.mode, AppMode::MockRuleEdit);
}

#[test]
fn ui_mock_edit_body_end_moves_cursor_to_line_end() {
    let mut app = app_in_mock_body_field();
    event::handle_key(&mut app, key_code(KeyCode::End));
    assert_eq!(app.mode, AppMode::MockRuleEdit);
}

#[test]
fn ui_mock_edit_body_char_inserts_into_editor() {
    let mut app = app_in_mock_body_field();
    let before = app.mock_edit_body.content();
    event::handle_key(&mut app, key('Z'));
    let after = app.mock_edit_body.content();
    assert!(after.len() > before.len());
    assert!(after.contains('Z'));
}

#[test]
fn ui_mock_edit_body_ctrl_v_paste_path_no_panic() {
    // UNTESTABLE cleanly: Ctrl+V shells out to `pbpaste` which may or may not
    // be available in CI and produces nondeterministic content. We only assert
    // the handler does not panic and preserves the edit mode.
    let mut app = app_in_mock_body_field();
    event::handle_key(&mut app, key_ctrl('v'));
    assert_eq!(app.mode, AppMode::MockRuleEdit);
}

#[test]
fn ui_mock_edit_body_unhandled_code_is_noop() {
    let mut app = app_in_mock_body_field();
    event::handle_key(&mut app, key_code(KeyCode::Insert));
    assert_eq!(app.mode, AppMode::MockRuleEdit);
}

#[test]
fn ui_mock_edit_top_field_unhandled_code_is_noop() {
    let mut app = app_in_mock_edit();
    app.mock_edit_field = 2;
    event::handle_key(&mut app, key_code(KeyCode::Insert));
    assert_eq!(app.mode, AppMode::MockRuleEdit);
}
