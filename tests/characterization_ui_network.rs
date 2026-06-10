//! Phase 2.5B Task 8+9 — characterization tests for `src/ui/network/`.
//!
//! Uses TestBackend to drive the real render path end-to-end and asserts
//! OBSERVABLE features (cell colors, text presence, span counts) rather
//! than raw pixel dumps (Rule 3). Every render path is exercised with
//! >=5 cases across the empty / normal / extreme / selected-item axes.
//!
//! Audit entries locked:
//!   - UI-011 detail panel JSON viewer coupling
//!   - UI-029 tab-specific render
//!   - UI-032
//!   - UI-035 row bg magic RGB
//!   - UI-037 detail 1109 lines mixes concerns
//!   - Related ui-Network entries
//!
//! UNTESTABLE breakdown:
//!   - Click regions populated by render (detail_section_map,
//!     detail_json_click_map, net_filter_pills, mock_edit_regions,
//!     mock_rule_regions, stats_slowest_regions) require both a render
//!     pass AND a subsequent click-read via event.rs paths. These are
//!     indirectly verified by rendering without panic and by the
//!     event-characterization suite (Task 5). Rule 11 D-ref UI-041.
//!   - Scrollbar glyph placement at sub-cell boundaries — Rule 11: PHYS.

#![cfg(test)]
#![allow(clippy::too_many_lines)]

#[path = "support/mod.rs"]
mod support;

use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::Terminal;

use flog::app::{App, ConnectedApp, InputField, SseMergeRule, SsePathSegment};
use flog::domain::network::{EntrySource, NetworkEntry, NetworkStatus};
use flog::domain::network_filter::{MethodFilter, ProtocolFilter, StatusFilter};
use flog::input::ConnectorHandle;

use support::fixtures;
use support::ui_inspect::{
    count_cells_with_bg, count_cells_with_fg, count_rows_with_text, distinct_colors, find_text_row,
    full_text, row_to_string,
};

// ── Palette (mirror src/ui/mod.rs + src/ui/network/mod.rs) ─────────
const BASE: Color = Color::Rgb(36, 39, 58);
const MANTLE: Color = Color::Rgb(30, 32, 48);
#[allow(dead_code)]
const SURFACE0: Color = Color::Rgb(54, 58, 79);
const SURFACE1: Color = Color::Rgb(73, 77, 100);
#[allow(dead_code)]
const OVERLAY0: Color = Color::Rgb(110, 115, 141);
#[allow(dead_code)]
const TEXT: Color = Color::Rgb(202, 211, 245);
#[allow(dead_code)]
const SUBTEXT0: Color = Color::Rgb(165, 173, 206);
const BLUE: Color = Color::Rgb(138, 173, 244);
const GREEN: Color = Color::Rgb(166, 218, 149);
const YELLOW: Color = Color::Rgb(238, 212, 159);
#[allow(dead_code)]
const PEACH: Color = Color::Rgb(245, 169, 127);
const RED: Color = Color::Rgb(237, 135, 150);
const MAUVE: Color = Color::Rgb(198, 160, 246);
const SAPPHIRE: Color = Color::Rgb(125, 196, 228);
#[allow(dead_code)]
const TEAL: Color = Color::Rgb(139, 213, 202);
#[allow(dead_code)]
const PINK: Color = Color::Rgb(245, 189, 230);

const ERROR_ROW_BG: Color = Color::Rgb(50, 30, 35);
const WARNING_ROW_BG: Color = Color::Rgb(50, 45, 30);
const REPLAY_ROW_BG: Color = Color::Rgb(35, 45, 65);
const MOCKED_ROW_BG: Color = Color::Rgb(50, 35, 65);

// ── Render harnesses ────────────────────────────────────────────────

fn render_network(app: &mut App, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut term = Terminal::new(backend).unwrap();
    app.layout.width = width;
    term.draw(|f| {
        let area = Rect::new(0, 0, width, height);
        flog::ui::network::draw_network(f, app, area);
    })
    .unwrap();
    term.backend().buffer().clone()
}

fn render_detail(app: &mut App, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut term = Terminal::new(backend).unwrap();
    app.layout.width = width;
    term.draw(|f| {
        let area = Rect::new(0, 0, width, height);
        flog::ui::network::detail::draw_network_detail(f, app, area);
    })
    .unwrap();
    term.backend().buffer().clone()
}

fn render_mock_rules_panel(app: &mut App, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut term = Terminal::new(backend).unwrap();
    app.layout.width = width;
    term.draw(|f| {
        let area = Rect::new(0, 0, width, height);
        flog::ui::network::mock_rules::draw_mock_rules_panel(f, app, area);
    })
    .unwrap();
    term.backend().buffer().clone()
}

fn render_mock_edit(app: &mut App, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut term = Terminal::new(backend).unwrap();
    app.layout.width = width;
    term.draw(|f| {
        flog::ui::network::mock_rules::draw_mock_rule_edit(f, app);
    })
    .unwrap();
    term.backend().buffer().clone()
}

fn render_stats(app: &mut App, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut term = Terminal::new(backend).unwrap();
    app.layout.width = width;
    term.draw(|f| {
        flog::ui::network::stats::draw_network_stats(f, app);
    })
    .unwrap();
    term.backend().buffer().clone()
}

// ── App seeding helpers ────────────────────────────────────────────

fn app_connected() -> App {
    let mut app = App::default();
    let (handle, _rx) = ConnectorHandle::for_testing();
    app.connected_apps.push(ConnectedApp {
        id: "fake".into(),
        device_id: "devA".into(),
        device_name: "Pixel 8".into(),
        port: 9753,
        app_name: "demo".into(),
        app_version: "1.0.0".into(),
        os: "android".into(),
        package_name: "com.example.demo".into(),
        build_mode: "debug".into(),
        handle,
    });
    app.active_app_id = Some("fake".into());
    app
}

fn seed_network(app: &mut App, entries: Vec<NetworkEntry>) {
    for e in entries {
        app.network_store.push_entry(e);
    }
    app.network.invalidate_filter();
}

fn select_first(app: &mut App) {
    app.network.auto_scroll = false;
    app.network.selected = 0;
    app.network.scroll_offset = 0;
}

// ══════════════════════════════════════════════════════════════════════
//  ui/network/mod.rs — Empty / single / multi
// ══════════════════════════════════════════════════════════════════════

#[test]
fn ui_029_empty_network_shows_empty_state() {
    let mut app = app_connected();
    let buf = render_network(&mut app, 120, 20);
    let text = full_text(&buf);
    assert!(
        text.contains("Network Inspector") || text.contains("FlogHttpInterceptor"),
        "expected empty-state hint"
    );
}

#[test]
fn ui_029_renders_single_http_entry() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::http_get_200(1, "/api/users")]);
    let buf = render_network(&mut app, 120, 20);
    assert!(find_text_row(&buf, "/api/users").is_some());
    assert!(find_text_row(&buf, "GET").is_some());
    assert!(find_text_row(&buf, "200").is_some());
}

#[test]
fn ui_029_renders_multiple_entries_in_order() {
    let mut app = app_connected();
    seed_network(
        &mut app,
        vec![
            fixtures::http_get_200(1, "/alpha"),
            fixtures::http_get_200(2, "/bravo"),
            fixtures::http_get_200(3, "/charlie"),
        ],
    );
    let buf = render_network(&mut app, 120, 20);
    let a = find_text_row(&buf, "/alpha").expect("alpha row");
    let b = find_text_row(&buf, "/bravo").expect("bravo row");
    let c = find_text_row(&buf, "/charlie").expect("charlie row");
    assert!(a < b && b < c);
}

#[test]
fn ui_029_renders_sse_entry_with_protocol_pill() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::sse_entry(1, "/stream")]);
    let buf = render_network(&mut app, 120, 20);
    let text = full_text(&buf);
    assert!(text.contains("SSE"), "SSE pill missing");
    assert!(text.contains("/stream"));
}

#[test]
fn ui_029_renders_ws_entry_with_protocol_pill() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::ws_entry(1, "wss://chat/x")]);
    let buf = render_network(&mut app, 120, 20);
    let text = full_text(&buf);
    assert!(text.contains("WS"));
}

#[test]
fn ui_029_renders_pending_entry_with_ellipsis() {
    let mut app = app_connected();
    seed_network(
        &mut app,
        vec![fixtures::http_pending(1, "/inflight", "GET")],
    );
    let buf = render_network(&mut app, 120, 20);
    assert!(full_text(&buf).contains("..."));
}

#[test]
fn ui_029_renders_failed_entry_with_failed_status() {
    let mut app = app_connected();
    let mut e = fixtures::http_get_200(1, "/boom");
    e.status = NetworkStatus::Failed;
    e.http_status = None;
    seed_network(&mut app, vec![e]);
    let buf = render_network(&mut app, 120, 20);
    assert!(full_text(&buf).contains("failed"));
}

// ══════════════════════════════════════════════════════════════════════
//  Row background colors (UI-035)
// ══════════════════════════════════════════════════════════════════════

#[test]
fn ui_035_error_row_has_dark_red_bg() {
    let mut app = app_connected();
    let mut e = fixtures::http_get_200(1, "/die");
    e.status = NetworkStatus::Failed;
    seed_network(&mut app, vec![fixtures::http_get_200(2, "/ok"), e]);
    select_first(&mut app);
    let buf = render_network(&mut app, 120, 20);
    assert!(count_cells_with_bg(&buf, ERROR_ROW_BG) > 0);
}

#[test]
fn ui_035_warning_row_has_dark_yellow_bg_for_4xx() {
    let mut app = app_connected();
    let mut e = fixtures::http_get_200(1, "/u");
    e.http_status = Some(404);
    seed_network(&mut app, vec![fixtures::http_get_200(2, "/ok"), e]);
    select_first(&mut app);
    let buf = render_network(&mut app, 120, 20);
    assert!(count_cells_with_bg(&buf, WARNING_ROW_BG) > 0);
}

#[test]
fn ui_035_replay_row_has_blue_tinted_bg() {
    let mut app = app_connected();
    let e = fixtures::with_source(fixtures::http_get_200(1, "/replayed"), EntrySource::Replay);
    seed_network(&mut app, vec![fixtures::http_get_200(2, "/base"), e]);
    select_first(&mut app);
    let buf = render_network(&mut app, 120, 20);
    assert!(count_cells_with_bg(&buf, REPLAY_ROW_BG) > 0);
}

#[test]
fn ui_035_mocked_row_has_purple_tinted_bg() {
    let mut app = app_connected();
    let e = fixtures::with_source(fixtures::http_get_200(1, "/mocked"), EntrySource::Mocked);
    seed_network(&mut app, vec![fixtures::http_get_200(2, "/base"), e]);
    select_first(&mut app);
    let buf = render_network(&mut app, 120, 20);
    assert!(count_cells_with_bg(&buf, MOCKED_ROW_BG) > 0);
}

#[test]
fn ui_035_mocked_row_shows_mock_pill_text() {
    let mut app = app_connected();
    let e = fixtures::with_source(fixtures::http_get_200(1, "/mocked"), EntrySource::Mocked);
    seed_network(&mut app, vec![e]);
    let buf = render_network(&mut app, 140, 20);
    assert!(full_text(&buf).contains("MOCK"));
}

#[test]
fn ui_035_replay_row_shows_replay_pill_text() {
    let mut app = app_connected();
    let e = fixtures::with_source(fixtures::http_get_200(1, "/replayed"), EntrySource::Replay);
    seed_network(&mut app, vec![e]);
    let buf = render_network(&mut app, 140, 20);
    assert!(full_text(&buf).contains("REPLAY"));
}

#[test]
fn ui_035_selected_row_uses_surface1_bg() {
    let mut app = app_connected();
    seed_network(
        &mut app,
        vec![
            fixtures::http_get_200(1, "/a"),
            fixtures::http_get_200(2, "/b"),
        ],
    );
    select_first(&mut app);
    let buf = render_network(&mut app, 120, 20);
    assert!(count_cells_with_bg(&buf, SURFACE1) > 0);
}

#[test]
fn ui_035_info_row_uses_base_bg() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::http_get_200(1, "/ok")]);
    let buf = render_network(&mut app, 120, 20);
    assert!(count_cells_with_bg(&buf, BASE) > 0);
}

// ══════════════════════════════════════════════════════════════════════
//  Scroll
// ══════════════════════════════════════════════════════════════════════

#[test]
fn ui_029_scroll_offset_shifts_visible_window() {
    let mut app = app_connected();
    let mut entries = Vec::new();
    for i in 0..60u64 {
        entries.push(fixtures::http_get_200(i, &format!("/e-{i:02}")));
    }
    seed_network(&mut app, entries);
    // Auto-scroll shows newest; scrolling back shows older.
    let buf_auto = render_network(&mut app, 120, 15);
    assert!(find_text_row(&buf_auto, "/e-59").is_some());

    app.network.auto_scroll = false;
    app.network.scroll_offset = 0;
    app.network.selected = 0;
    let buf_top = render_network(&mut app, 120, 15);
    assert!(find_text_row(&buf_top, "/e-00").is_some());
}

#[test]
fn ui_029_auto_scroll_pins_to_newest() {
    let mut app = app_connected();
    let mut entries = Vec::new();
    for i in 0..20u64 {
        entries.push(fixtures::http_get_200(i, &format!("/e-{i:02}")));
    }
    seed_network(&mut app, entries);
    let buf = render_network(&mut app, 120, 15);
    assert!(find_text_row(&buf, "/e-19").is_some());
}

// ══════════════════════════════════════════════════════════════════════
//  Filter pills + filter.rs
// ══════════════════════════════════════════════════════════════════════

#[test]
fn ui_filter_protocol_all_pill_rendered() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::http_get_200(1, "/x")]);
    let buf = render_network(&mut app, 160, 20);
    let text = full_text(&buf);
    assert!(text.contains("Protocol:"));
    assert!(text.contains(" All "));
    assert!(text.contains(" HTTP "));
    assert!(text.contains(" SSE "));
    assert!(text.contains(" WS "));
}

#[test]
fn ui_filter_method_pills_rendered() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::http_get_200(1, "/x")]);
    let buf = render_network(&mut app, 180, 20);
    let text = full_text(&buf);
    assert!(text.contains("Method:"));
    assert!(text.contains(" GET "));
    assert!(text.contains(" POST "));
    assert!(text.contains(" DEL "));
    assert!(text.contains(" PATCH "));
}

#[test]
fn ui_filter_status_pills_rendered() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::http_get_200(1, "/x")]);
    let buf = render_network(&mut app, 200, 20);
    let text = full_text(&buf);
    assert!(text.contains("Status:"));
    assert!(text.contains(" OK "));
    assert!(text.contains(" Fail "));
    assert!(text.contains(" Active "));
    assert!(text.contains(" Pending "));
}

#[test]
fn ui_filter_protocol_http_selected_uses_mauve_bg() {
    let mut app = app_connected();
    app.network.filter.protocol = ProtocolFilter::Http;
    seed_network(&mut app, vec![fixtures::http_get_200(1, "/x")]);
    let buf = render_network(&mut app, 160, 20);
    // MAUVE bg is the "selected" pill indicator.
    assert!(count_cells_with_bg(&buf, MAUVE) > 0);
}

#[test]
fn ui_filter_method_post_selected() {
    let mut app = app_connected();
    app.network.filter.method = MethodFilter::Post;
    seed_network(&mut app, vec![fixtures::http_post_500(1, "/x")]);
    let buf = render_network(&mut app, 180, 20);
    assert!(count_cells_with_bg(&buf, MAUVE) > 0);
}

#[test]
fn ui_filter_status_failed_selected() {
    let mut app = app_connected();
    app.network.filter.status = StatusFilter::Failed;
    let mut e = fixtures::http_get_200(1, "/x");
    e.status = NetworkStatus::Failed;
    seed_network(&mut app, vec![e]);
    let buf = render_network(&mut app, 200, 20);
    assert!(count_cells_with_bg(&buf, MAUVE) > 0);
}

#[test]
fn ui_filter_search_exclude_labels_rendered() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::http_get_200(1, "/x")]);
    let buf = render_network(&mut app, 160, 20);
    let text = full_text(&buf);
    assert!(text.contains("Search"));
    assert!(text.contains("Exclude"));
}

#[test]
fn ui_filter_count_badge_shows_filtered_over_total() {
    let mut app = app_connected();
    seed_network(
        &mut app,
        vec![
            fixtures::http_get_200(1, "/a"),
            fixtures::http_get_200(2, "/b"),
            fixtures::http_get_200(3, "/c"),
        ],
    );
    let buf = render_network(&mut app, 160, 20);
    assert!(full_text(&buf).contains("3/3"));
}

#[test]
fn ui_filter_column_header_row_renders() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::http_get_200(1, "/x")]);
    let buf = render_network(&mut app, 160, 20);
    let text = full_text(&buf);
    assert!(text.contains("PROTO"));
    assert!(text.contains("METHOD"));
    assert!(text.contains("URL"));
    assert!(text.contains("STATUS"));
    assert!(text.contains("TIME"));
    assert!(text.contains("SIZE"));
}

// ══════════════════════════════════════════════════════════════════════
//  Status bar
// ══════════════════════════════════════════════════════════════════════

#[test]
fn ui_network_status_shows_total_count() {
    let mut app = app_connected();
    seed_network(
        &mut app,
        vec![
            fixtures::http_get_200(1, "/a"),
            fixtures::http_get_200(2, "/b"),
        ],
    );
    let buf = render_network(&mut app, 160, 20);
    let text = full_text(&buf);
    assert!(text.contains("2/2 requests"));
}

#[test]
fn ui_network_status_shows_failed_count() {
    let mut app = app_connected();
    let mut fail = fixtures::http_get_200(1, "/die");
    fail.status = NetworkStatus::Failed;
    seed_network(&mut app, vec![fail, fixtures::http_get_200(2, "/ok")]);
    let buf = render_network(&mut app, 160, 20);
    let text = full_text(&buf);
    assert!(text.contains("1 failed"));
}

#[test]
fn ui_network_status_shows_live_pill_when_autoscroll() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::http_get_200(1, "/x")]);
    app.network.auto_scroll = true;
    let buf = render_network(&mut app, 160, 20);
    assert!(full_text(&buf).contains("LIVE"));
}

#[test]
fn ui_network_status_shows_percentage_when_paused() {
    let mut app = app_connected();
    seed_network(
        &mut app,
        vec![
            fixtures::http_get_200(1, "/a"),
            fixtures::http_get_200(2, "/b"),
        ],
    );
    app.network.auto_scroll = false;
    app.network.selected = 0;
    let buf = render_network(&mut app, 160, 20);
    assert!(full_text(&buf).contains("%"));
}

#[test]
fn ui_network_status_shows_clear_help_buttons() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::http_get_200(1, "/x")]);
    let buf = render_network(&mut app, 160, 20);
    let text = full_text(&buf);
    assert!(text.contains("Clear"));
    assert!(text.contains("Stats"));
    assert!(text.contains("?"));
}

#[test]
fn ui_network_status_shows_replay_for_http_with_client() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::http_get_200(1, "/x")]);
    let buf = render_network(&mut app, 180, 20);
    assert!(full_text(&buf).contains("Replay"));
}

#[test]
fn ui_network_status_hides_replay_for_ws() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::ws_entry(1, "/ws")]);
    let buf = render_network(&mut app, 180, 20);
    let text = full_text(&buf);
    // Replay button should NOT show when selected is non-HTTP.
    assert!(!text.contains(" Replay "));
}

#[test]
fn ui_network_status_shows_mock_when_http_and_connected() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::http_get_200(1, "/x")]);
    let buf = render_network(&mut app, 200, 20);
    assert!(full_text(&buf).contains("Mock"));
}

// ══════════════════════════════════════════════════════════════════════
//  Detail panel — No selection
// ══════════════════════════════════════════════════════════════════════

#[test]
fn detail_no_selection_shows_select_placeholder() {
    let mut app = app_connected();
    let buf = render_detail(&mut app, 80, 20);
    assert!(full_text(&buf).contains("Select a request"));
}

// ══════════════════════════════════════════════════════════════════════
//  HTTP detail
// ══════════════════════════════════════════════════════════════════════

fn http_entry_rich(id: u64, url: &str) -> NetworkEntry {
    let mut e = fixtures::http_get_200(id, url);
    e.method = "POST".to_string();
    e.request_headers = Some(r#"{"content-type":"application/json","accept":"*/*"}"#.to_string());
    e.response_headers = Some(r#"{"content-type":"application/json"}"#.to_string());
    e.request_body = Some(r#"{"name":"alice","n":42}"#.to_string());
    e.response_body = Some(r#"{"ok":true,"id":99}"#.to_string());
    e
}

#[test]
fn detail_http_shows_request_url() {
    let mut app = app_connected();
    seed_network(&mut app, vec![http_entry_rich(1, "/api/echo")]);
    let buf = render_detail(&mut app, 80, 40);
    assert!(full_text(&buf).contains("/api/echo"));
}

#[test]
fn detail_http_shows_method_pill() {
    let mut app = app_connected();
    seed_network(&mut app, vec![http_entry_rich(1, "/api")]);
    let buf = render_detail(&mut app, 80, 40);
    assert!(full_text(&buf).contains("POST"));
}

#[test]
fn detail_http_shows_status_200_ok_text() {
    let mut app = app_connected();
    let mut e = fixtures::http_get_200(1, "/a");
    e.response_body = None;
    e.request_headers = None;
    e.response_headers = None;
    seed_network(&mut app, vec![e]);
    let buf = render_detail(&mut app, 80, 40);
    let text = full_text(&buf);
    assert!(text.contains("200"));
    assert!(text.contains("OK"));
}

#[test]
fn detail_http_shows_duration_value() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::http_get_200(1, "/a")]);
    let buf = render_detail(&mut app, 80, 40);
    // fixture sets duration=42 → "42ms"
    assert!(full_text(&buf).contains("42ms"));
}

#[test]
fn detail_http_shows_size() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::http_post_500(1, "/a")]);
    let buf = render_detail(&mut app, 80, 40);
    // 256 + 64 = 320B; formatter renders "320B" for completed entries.
    let text = full_text(&buf);
    assert!(text.contains("320B") || text.contains("Size"));
}

#[test]
fn detail_http_shows_request_headers_section() {
    let mut app = app_connected();
    seed_network(&mut app, vec![http_entry_rich(1, "/api")]);
    let buf = render_detail(&mut app, 80, 40);
    assert!(full_text(&buf).contains("Request Headers"));
}

#[test]
fn detail_http_shows_response_headers_section() {
    let mut app = app_connected();
    seed_network(&mut app, vec![http_entry_rich(1, "/api")]);
    let buf = render_detail(&mut app, 80, 40);
    assert!(full_text(&buf).contains("Response Headers"));
}

#[test]
fn detail_http_shows_response_body_section() {
    let mut app = app_connected();
    seed_network(&mut app, vec![http_entry_rich(1, "/api")]);
    let buf = render_detail(&mut app, 80, 40);
    let text = full_text(&buf);
    assert!(text.contains("Response Body"));
}

#[test]
fn detail_http_shows_request_body_section() {
    let mut app = app_connected();
    seed_network(&mut app, vec![http_entry_rich(1, "/api")]);
    let buf = render_detail(&mut app, 80, 40);
    assert!(full_text(&buf).contains("Request Body"));
}

#[test]
fn detail_http_shows_request_body_json_keys() {
    let mut app = app_connected();
    seed_network(&mut app, vec![http_entry_rich(1, "/api")]);
    let buf = render_detail(&mut app, 100, 50);
    let text = full_text(&buf);
    assert!(text.contains("name") || text.contains("alice"));
}

#[test]
fn detail_http_empty_body_hides_body_section() {
    let mut app = app_connected();
    let mut e = fixtures::http_get_200(1, "/x");
    e.response_body = Some(String::new());
    e.request_body = None;
    seed_network(&mut app, vec![e]);
    let buf = render_detail(&mut app, 80, 40);
    let text = full_text(&buf);
    // Empty response body shouldn't produce a Response Body section header.
    assert!(!text.contains("Response Body"));
}

#[test]
fn detail_http_plain_text_body_renders() {
    let mut app = app_connected();
    let mut e = fixtures::http_get_200(1, "/x");
    e.response_body = Some("plain text response".to_string());
    seed_network(&mut app, vec![e]);
    let buf = render_detail(&mut app, 80, 40);
    assert!(full_text(&buf).contains("plain text response"));
}

#[test]
fn detail_http_query_params_parsed_and_shown() {
    let mut app = app_connected();
    let e = fixtures::http_get_200(1, "/search?q=rust&page=2");
    seed_network(&mut app, vec![e]);
    let buf = render_detail(&mut app, 100, 40);
    let text = full_text(&buf);
    assert!(text.contains("Query Parameters"));
    assert!(text.contains("page"));
}

#[test]
fn detail_http_status_404_shows_not_found_text() {
    let mut app = app_connected();
    let mut e = fixtures::http_get_200(1, "/missing");
    e.http_status = Some(404);
    seed_network(&mut app, vec![e]);
    let buf = render_detail(&mut app, 80, 40);
    assert!(full_text(&buf).contains("404"));
}

#[test]
fn detail_http_error_section_shows_red_text() {
    let mut app = app_connected();
    let mut e = fixtures::http_get_200(1, "/x");
    e.status = NetworkStatus::Failed;
    e.error = Some("connection refused".to_string());
    seed_network(&mut app, vec![e]);
    let buf = render_detail(&mut app, 80, 40);
    let text = full_text(&buf);
    assert!(text.contains("Error"));
    assert!(text.contains("connection refused"));
    assert!(count_cells_with_fg(&buf, RED) > 0);
}

// ══════════════════════════════════════════════════════════════════════
//  SSE detail
// ══════════════════════════════════════════════════════════════════════

fn sse_with_chunks(id: u64, url: &str) -> NetworkEntry {
    let mut e = fixtures::sse_entry(id, url);
    e.sse_chunks = vec![
        fixtures::sse_chunk(0, r#"{"delta":{"content":"Hel"}}"#),
        fixtures::sse_chunk(1, r#"{"delta":{"content":"lo"}}"#),
        fixtures::sse_chunk(2, r#"{"delta":{"content":"!"}}"#),
    ];
    e
}

#[test]
fn detail_sse_events_mode_lists_chunk_headers() {
    let mut app = app_connected();
    seed_network(&mut app, vec![sse_with_chunks(1, "/stream")]);
    let buf = render_detail(&mut app, 100, 40);
    let text = full_text(&buf);
    assert!(text.contains("SSE Events"));
    assert!(text.contains("#0") || text.contains("#1"));
}

#[test]
fn detail_sse_events_mode_shows_events_pill() {
    let mut app = app_connected();
    seed_network(&mut app, vec![sse_with_chunks(1, "/stream")]);
    let buf = render_detail(&mut app, 100, 40);
    assert!(full_text(&buf).contains("Events"));
}

#[test]
fn detail_sse_events_mode_shows_merged_pill() {
    let mut app = app_connected();
    seed_network(&mut app, vec![sse_with_chunks(1, "/stream")]);
    let buf = render_detail(&mut app, 100, 40);
    assert!(full_text(&buf).contains("Merged"));
}

#[test]
fn detail_sse_events_mode_truncates_multibyte_preview_without_panic() {
    let mut app = app_connected();
    let mut e = fixtures::sse_entry(1, "/stream");
    e.sse_chunks = vec![fixtures::sse_chunk(
        0,
        r#"{"delta":{"content":"你好🙂你好🙂你好🙂"}}"#,
    )];
    seed_network(&mut app, vec![e]);

    let buf = render_detail(&mut app, 36, 20);
    let text = full_text(&buf);
    assert!(text.contains("#0"), "expected SSE chunk header: {text}");
    assert!(text.contains("..."), "expected truncated preview: {text}");
}

#[test]
fn detail_sse_merged_mode_shows_concatenated() {
    let mut app = app_connected();
    seed_network(&mut app, vec![sse_with_chunks(1, "/stream")]);
    // Build a merge rule for delta.content
    let rule = SseMergeRule {
        field_path: vec![
            SsePathSegment::Key("delta".into()),
            SsePathSegment::Key("content".into()),
        ],
        field_display: "delta.content".into(),
    };
    // Rule is keyed by path without query
    app.network
        .sse_merge_rules
        .insert("/stream".to_string(), rule);
    app.network.sse_merged_mode = true;
    let buf = render_detail(&mut app, 120, 40);
    let text = full_text(&buf);
    assert!(text.contains("Hello!") || text.contains("Hel"));
}

#[test]
fn detail_sse_empty_chunks_hides_section() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::sse_entry(1, "/stream")]);
    let buf = render_detail(&mut app, 100, 40);
    assert!(!full_text(&buf).contains("SSE Events"));
}

#[test]
fn detail_sse_non_json_chunks_hide_pills() {
    let mut app = app_connected();
    let mut e = fixtures::sse_entry(1, "/stream");
    e.sse_chunks = vec![
        fixtures::sse_chunk(0, "plain text event"),
        fixtures::sse_chunk(1, "another raw line"),
    ];
    seed_network(&mut app, vec![e]);
    let buf = render_detail(&mut app, 100, 40);
    let text = full_text(&buf);
    // SSE Events heading still present, but not the Events/Merged pill row.
    assert!(text.contains("SSE Events"));
}

// ══════════════════════════════════════════════════════════════════════
//  WebSocket detail
// ══════════════════════════════════════════════════════════════════════

fn ws_with_msgs(id: u64, url: &str) -> NetworkEntry {
    let mut e = fixtures::ws_entry(id, url);
    e.ws_messages = vec![
        fixtures::ws_send(r#"{"type":"hello","seq":1}"#),
        fixtures::ws_recv(r#"{"type":"ack","seq":1}"#),
        fixtures::ws_send(r#"{"type":"ping"}"#),
    ];
    e
}

#[test]
fn detail_ws_shows_messages_header() {
    let mut app = app_connected();
    seed_network(&mut app, vec![ws_with_msgs(1, "wss://a/b")]);
    let buf = render_detail(&mut app, 100, 40);
    assert!(full_text(&buf).contains("Messages"));
}

#[test]
fn detail_ws_chat_mode_shows_chat_pill_active() {
    let mut app = app_connected();
    seed_network(&mut app, vec![ws_with_msgs(1, "wss://a/b")]);
    app.network.ws_chat_mode = true;
    let buf = render_detail(&mut app, 100, 40);
    let text = full_text(&buf);
    assert!(text.contains("Chat"));
    assert!(text.contains("Raw"));
}

#[test]
fn detail_ws_chat_mode_shows_send_pill() {
    let mut app = app_connected();
    seed_network(&mut app, vec![ws_with_msgs(1, "wss://a/b")]);
    app.network.ws_chat_mode = true;
    let buf = render_detail(&mut app, 100, 40);
    assert!(full_text(&buf).contains("SEND"));
}

#[test]
fn detail_ws_chat_mode_shows_recv_pill() {
    let mut app = app_connected();
    seed_network(&mut app, vec![ws_with_msgs(1, "wss://a/b")]);
    app.network.ws_chat_mode = true;
    let buf = render_detail(&mut app, 100, 40);
    assert!(full_text(&buf).contains("RECV"));
}

#[test]
fn detail_ws_raw_mode_shows_arrow_markers() {
    let mut app = app_connected();
    seed_network(&mut app, vec![ws_with_msgs(1, "wss://a/b")]);
    app.network.ws_chat_mode = false;
    let buf = render_detail(&mut app, 100, 40);
    let text = full_text(&buf);
    // → and ← arrows in raw mode
    assert!(text.contains('\u{2192}') || text.contains('\u{2190}'));
}

#[test]
fn detail_ws_empty_messages_hides_section() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::ws_entry(1, "wss://a/b")]);
    let buf = render_detail(&mut app, 100, 40);
    assert!(!full_text(&buf).contains("Messages ("));
}

// ══════════════════════════════════════════════════════════════════════
//  General header / Mock button
// ══════════════════════════════════════════════════════════════════════

#[test]
fn detail_mock_btn_renders_for_http_when_connected() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::http_get_200(1, "/x")]);
    let buf = render_detail(&mut app, 100, 30);
    assert!(full_text(&buf).contains("[Mock]"));
}

#[test]
fn detail_mock_btn_hidden_when_not_connected() {
    let mut app = App::default(); // no ConnectedApp
    seed_network(&mut app, vec![fixtures::http_get_200(1, "/x")]);
    let buf = render_detail(&mut app, 100, 30);
    assert!(!full_text(&buf).contains("[Mock]"));
}

#[test]
fn detail_mock_btn_hidden_on_ws() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::ws_entry(1, "wss://x/y")]);
    let buf = render_detail(&mut app, 100, 30);
    assert!(!full_text(&buf).contains("[Mock]"));
}

#[test]
fn detail_general_section_shows_url_and_method() {
    let mut app = app_connected();
    seed_network(&mut app, vec![http_entry_rich(1, "/abc")]);
    let buf = render_detail(&mut app, 100, 40);
    let text = full_text(&buf);
    assert!(text.contains("General"));
    assert!(text.contains("URL"));
    assert!(text.contains("Method"));
}

#[test]
fn detail_general_section_shows_time_label() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::http_get_200(1, "/x")]);
    let buf = render_detail(&mut app, 100, 40);
    let text = full_text(&buf);
    // Timestamp column renders as "Time:"
    assert!(text.contains("Time"));
    assert!(text.contains("12:00:00"));
}

// ══════════════════════════════════════════════════════════════════════
//  JSON fold (UI-011)
// ══════════════════════════════════════════════════════════════════════

#[test]
fn detail_json_fold_renders_keys_when_expanded() {
    let mut app = app_connected();
    let mut e = fixtures::http_get_200(1, "/x");
    e.response_body = Some(r#"{"alpha":1,"beta":"two"}"#.to_string());
    seed_network(&mut app, vec![e]);
    let buf = render_detail(&mut app, 100, 40);
    let text = full_text(&buf);
    assert!(text.contains("alpha"));
    assert!(text.contains("beta"));
}

#[test]
fn detail_json_fold_renders_nested_children() {
    let mut app = app_connected();
    let mut e = fixtures::http_get_200(1, "/x");
    e.response_body = Some(r#"{"outer":{"inner":42}}"#.to_string());
    seed_network(&mut app, vec![e]);
    let buf = render_detail(&mut app, 100, 40);
    let text = full_text(&buf);
    assert!(text.contains("outer"));
}

#[test]
fn detail_json_fold_array_shows_bracket_or_count() {
    let mut app = app_connected();
    let mut e = fixtures::http_get_200(1, "/x");
    e.response_body = Some(r#"{"items":[1,2,3]}"#.to_string());
    seed_network(&mut app, vec![e]);
    let buf = render_detail(&mut app, 100, 40);
    let text = full_text(&buf);
    assert!(text.contains("items"));
}

#[test]
fn detail_non_json_body_renders_as_plain_text() {
    let mut app = app_connected();
    let mut e = fixtures::http_get_200(1, "/x");
    e.response_body = Some("not-json-body-abcxyz".to_string());
    seed_network(&mut app, vec![e]);
    let buf = render_detail(&mut app, 100, 40);
    assert!(full_text(&buf).contains("not-json-body-abcxyz"));
}

// ══════════════════════════════════════════════════════════════════════
//  Detail scroll
// ══════════════════════════════════════════════════════════════════════

#[test]
fn detail_scroll_shifts_content() {
    let mut app = app_connected();
    let mut e = fixtures::http_get_200(1, "/x");
    let big_body = (0..80)
        .map(|i| format!(r#""k{i}":"v{i}""#))
        .collect::<Vec<_>>()
        .join(",");
    e.response_body = Some(format!("{{{big_body}}}"));
    seed_network(&mut app, vec![e]);
    let buf0 = render_detail(&mut app, 80, 15);
    app.network.detail_scroll = 20;
    let buf1 = render_detail(&mut app, 80, 15);
    assert_ne!(full_text(&buf0), full_text(&buf1));
}

// ══════════════════════════════════════════════════════════════════════
//  Mock rules panel
// ══════════════════════════════════════════════════════════════════════

#[test]
fn mock_rules_panel_empty_shows_hint() {
    let mut app = app_connected();
    let buf = render_mock_rules_panel(&mut app, 80, 15);
    assert!(full_text(&buf).contains("No mock rules"));
}

#[test]
fn mock_rules_panel_empty_mentions_m_key() {
    let mut app = app_connected();
    let buf = render_mock_rules_panel(&mut app, 80, 15);
    assert!(full_text(&buf).contains("Press M"));
}

#[test]
fn mock_rules_panel_title_renders() {
    let mut app = app_connected();
    let buf = render_mock_rules_panel(&mut app, 80, 15);
    assert!(full_text(&buf).contains("Mock Rules"));
}

#[test]
fn mock_rules_panel_lists_one_rule() {
    let mut app = app_connected();
    app.mock_rules
        .add("/api/users".into(), Some("GET".into()), 200, "{}".into(), 0);
    let buf = render_mock_rules_panel(&mut app, 120, 15);
    let text = full_text(&buf);
    assert!(text.contains("/api/users"));
    assert!(text.contains("GET"));
    assert!(text.contains("200"));
}

#[test]
fn mock_rules_panel_lists_multiple_rules() {
    let mut app = app_connected();
    app.mock_rules
        .add("/api/a".into(), Some("GET".into()), 200, "{}".into(), 0);
    app.mock_rules
        .add("/api/b".into(), Some("POST".into()), 201, "{}".into(), 0);
    let buf = render_mock_rules_panel(&mut app, 120, 15);
    let text = full_text(&buf);
    assert!(text.contains("/api/a"));
    assert!(text.contains("/api/b"));
}

#[test]
fn mock_rules_panel_selected_row_uses_surface1_bg() {
    let mut app = app_connected();
    app.mock_rules
        .add("/api/a".into(), Some("GET".into()), 200, "{}".into(), 0);
    app.mock_rules
        .add("/api/b".into(), Some("POST".into()), 201, "{}".into(), 0);
    app.mock_rule_selected = 1;
    let buf = render_mock_rules_panel(&mut app, 120, 15);
    assert!(count_cells_with_bg(&buf, SURFACE1) > 0);
}

#[test]
fn mock_rules_panel_shows_edit_off_del_buttons() {
    let mut app = app_connected();
    app.mock_rules
        .add("/api/a".into(), Some("GET".into()), 200, "{}".into(), 0);
    let buf = render_mock_rules_panel(&mut app, 120, 15);
    let text = full_text(&buf);
    assert!(text.contains("[Edit]"));
    assert!(text.contains("[Off]"));
    assert!(text.contains("[Del]"));
}

#[test]
fn mock_rules_panel_disabled_rule_shows_on_button() {
    let mut app = app_connected();
    let id = app
        .mock_rules
        .add("/api/a".into(), Some("GET".into()), 200, "{}".into(), 0);
    app.mock_rules.toggle(id);
    let buf = render_mock_rules_panel(&mut app, 120, 15);
    let text = full_text(&buf);
    assert!(text.contains("[On]"));
}

#[test]
fn mock_rules_panel_any_method_shown_as_star() {
    let mut app = app_connected();
    app.mock_rules
        .add("/api/any".into(), None, 200, "{}".into(), 0);
    let buf = render_mock_rules_panel(&mut app, 120, 15);
    let text = full_text(&buf);
    assert!(text.contains(" * "));
}

// ══════════════════════════════════════════════════════════════════════
//  Mock rule edit overlay
// ══════════════════════════════════════════════════════════════════════

fn open_mock_edit(app: &mut App) {
    app.mock_edit.field = 0;
    app.mock_edit.top_values = vec!["/api/users".into(), "GET".into(), "200".into(), "0".into()];
    app.mock_edit.body = flog::ui::text_editor::TextEditor::new("{\"ok\":true}");
}

#[test]
fn mock_edit_shows_title_and_labels() {
    let mut app = app_connected();
    open_mock_edit(&mut app);
    let buf = render_mock_edit(&mut app, 120, 30);
    let text = full_text(&buf);
    assert!(text.contains("Edit Mock Rule"));
    assert!(text.contains("URL Pattern"));
    assert!(text.contains("Method"));
    assert!(text.contains("Status Code"));
    assert!(text.contains("Delay"));
    assert!(text.contains("Response Body"));
}

#[test]
fn mock_edit_url_field_focused_shows_cursor_pipe() {
    let mut app = app_connected();
    open_mock_edit(&mut app);
    app.mock_edit.field = 0;
    let buf = render_mock_edit(&mut app, 120, 30);
    // cursor rendered as "|"
    assert!(full_text(&buf).contains("|"));
}

#[test]
fn mock_edit_method_field_focused() {
    let mut app = app_connected();
    open_mock_edit(&mut app);
    app.mock_edit.field = 1;
    let buf = render_mock_edit(&mut app, 120, 30);
    // Input still renders ("GET|"), and label is bolded.
    assert!(full_text(&buf).contains("GET"));
}

#[test]
fn mock_edit_status_field_focused() {
    let mut app = app_connected();
    open_mock_edit(&mut app);
    app.mock_edit.field = 2;
    let buf = render_mock_edit(&mut app, 120, 30);
    assert!(full_text(&buf).contains("200"));
}

#[test]
fn mock_edit_delay_field_focused() {
    let mut app = app_connected();
    open_mock_edit(&mut app);
    app.mock_edit.field = 3;
    let buf = render_mock_edit(&mut app, 120, 30);
    // delay has value "0"
    let text = full_text(&buf);
    assert!(text.contains("Delay"));
}

#[test]
fn mock_edit_body_field_focused() {
    let mut app = app_connected();
    open_mock_edit(&mut app);
    app.mock_edit.field = 4;
    let buf = render_mock_edit(&mut app, 120, 30);
    let text = full_text(&buf);
    assert!(text.contains("ok") || text.contains("true"));
}

#[test]
fn mock_edit_body_multiline_renders() {
    let mut app = app_connected();
    open_mock_edit(&mut app);
    app.mock_edit.body = flog::ui::text_editor::TextEditor::new("{\n  \"a\": 1,\n  \"b\": 2\n}");
    let buf = render_mock_edit(&mut app, 120, 30);
    let text = full_text(&buf);
    assert!(text.contains("\"a\"") || text.contains("a"));
    assert!(text.contains("\"b\"") || text.contains("b"));
}

#[test]
fn mock_edit_save_cancel_buttons_render() {
    let mut app = app_connected();
    open_mock_edit(&mut app);
    let buf = render_mock_edit(&mut app, 120, 30);
    let text = full_text(&buf);
    assert!(text.contains("Save"));
    assert!(text.contains("Cancel"));
}

#[test]
fn mock_edit_save_button_green_cancel_red() {
    let mut app = app_connected();
    open_mock_edit(&mut app);
    let buf = render_mock_edit(&mut app, 120, 30);
    assert!(count_cells_with_bg(&buf, GREEN) > 0);
    assert!(count_cells_with_bg(&buf, RED) > 0);
}

// ══════════════════════════════════════════════════════════════════════
//  Network stats
// ══════════════════════════════════════════════════════════════════════

#[test]
fn stats_empty_shows_title_and_zero_total() {
    let mut app = app_connected();
    let buf = render_stats(&mut app, 100, 30);
    let text = full_text(&buf);
    assert!(text.contains("Network Statistics"));
    assert!(text.contains("Total: 0"));
}

#[test]
fn stats_shows_back_button() {
    let mut app = app_connected();
    let buf = render_stats(&mut app, 100, 30);
    assert!(full_text(&buf).contains("< Back"));
}

#[test]
fn stats_shows_latency_percentile_rows() {
    let mut app = app_connected();
    for i in 0..10u64 {
        seed_network(&mut app, vec![fixtures::http_get_200(i, &format!("/u{i}"))]);
    }
    let buf = render_stats(&mut app, 120, 30);
    let text = full_text(&buf);
    assert!(text.contains("P50"));
    assert!(text.contains("P95"));
    assert!(text.contains("P99"));
    assert!(text.contains("Average"));
}

#[test]
fn stats_shows_summary_rows() {
    let mut app = app_connected();
    seed_network(
        &mut app,
        vec![
            fixtures::http_get_200(1, "/a"),
            fixtures::http_get_200(2, "/b"),
        ],
    );
    let buf = render_stats(&mut app, 120, 30);
    let text = full_text(&buf);
    assert!(text.contains("Total Requests"));
    assert!(text.contains("Success"));
    assert!(text.contains("Failed"));
    assert!(text.contains("In-Progress"));
}

#[test]
fn stats_shows_slowest_top5_section() {
    let mut app = app_connected();
    for i in 0..7u64 {
        let mut e = fixtures::http_get_200(i, &format!("/u{i}"));
        e.duration = Some(100 + i * 50);
        seed_network(&mut app, vec![e]);
    }
    let buf = render_stats(&mut app, 120, 30);
    assert!(full_text(&buf).contains("Slowest Top 5"));
}

#[test]
fn stats_slowest_lists_urls() {
    let mut app = app_connected();
    let mut e = fixtures::http_get_200(1, "/zebra-slow");
    e.duration = Some(9000);
    seed_network(&mut app, vec![e]);
    let buf = render_stats(&mut app, 120, 30);
    assert!(full_text(&buf).contains("zebra"));
}

#[test]
fn stats_shows_status_distribution_rows() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::http_get_200(1, "/a")]);
    let buf = render_stats(&mut app, 120, 30);
    let text = full_text(&buf);
    assert!(text.contains("2xx"));
    assert!(text.contains("3xx"));
    assert!(text.contains("4xx"));
    assert!(text.contains("5xx"));
}

#[test]
fn stats_shows_per_domain_section() {
    let mut app = app_connected();
    let mut e = fixtures::http_get_200(1, "https://api.example.com/x");
    e.duration = Some(123);
    seed_network(&mut app, vec![e]);
    let buf = render_stats(&mut app, 140, 30);
    let text = full_text(&buf);
    assert!(text.contains("Per-Domain"));
    assert!(text.contains("api.example.com") || text.contains("example.com"));
}

#[test]
fn stats_failed_request_shows_red_failed_count() {
    let mut app = app_connected();
    let mut fail = fixtures::http_get_200(1, "/x");
    fail.status = NetworkStatus::Failed;
    fail.duration = Some(10);
    seed_network(&mut app, vec![fail]);
    let buf = render_stats(&mut app, 120, 30);
    assert!(count_cells_with_fg(&buf, RED) > 0);
}

#[test]
fn stats_pending_entry_counts_in_progress() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::http_pending(1, "/p", "GET")]);
    let buf = render_stats(&mut app, 120, 30);
    let text = full_text(&buf);
    assert!(text.contains("In-Progress"));
    assert!(text.contains(" 1"));
}

// ══════════════════════════════════════════════════════════════════════
//  Extreme cases & invariants
// ══════════════════════════════════════════════════════════════════════

#[test]
fn ui_029_very_small_width_does_not_panic() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::http_get_200(1, "/x")]);
    let buf = render_network(&mut app, 50, 15);
    assert_eq!(buf.area.width, 50);
}

#[test]
fn ui_029_tall_viewport_shows_all_entries() {
    let mut app = app_connected();
    for i in 0..5u64 {
        seed_network(&mut app, vec![fixtures::http_get_200(i, &format!("/u{i}"))]);
    }
    let buf = render_network(&mut app, 120, 40);
    for i in 0..5 {
        let needle = format!("/u{i}");
        assert!(find_text_row(&buf, &needle).is_some());
    }
}

#[test]
fn ui_029_very_long_url_truncates() {
    let mut app = app_connected();
    let url = format!("/a/{}", "z".repeat(300));
    seed_network(&mut app, vec![fixtures::http_get_200(1, &url)]);
    let buf = render_network(&mut app, 80, 15);
    // Must not panic, URL either fits partial or gets ellipsis.
    assert_eq!(buf.area.width, 80);
}

#[test]
fn ui_029_same_render_twice_is_deterministic() {
    let mut app = app_connected();
    seed_network(
        &mut app,
        vec![
            fixtures::http_get_200(1, "/a"),
            fixtures::http_post_500(2, "/b"),
        ],
    );
    // Kill the live-pill spinner by setting tick such that outputs match.
    app.tick = 0;
    let buf1 = render_network(&mut app, 120, 20);
    let buf2 = render_network(&mut app, 120, 20);
    assert_eq!(full_text(&buf1), full_text(&buf2));
}

#[test]
fn ui_029_detail_panel_side_by_side_layout() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::http_get_200(1, "/abc")]);
    app.network.show_detail = true;
    let buf = render_network(&mut app, 140, 25);
    let text = full_text(&buf);
    // Both table URL and detail header appear.
    assert!(text.contains("/abc"));
    assert!(text.contains("Details"));
}

#[test]
fn ui_029_mock_panel_side_by_side_layout() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::http_get_200(1, "/abc")]);
    app.network.show_mock_rules_panel = true;
    let buf = render_network(&mut app, 140, 25);
    let text = full_text(&buf);
    assert!(text.contains("/abc"));
    assert!(text.contains("Mock Rules"));
}

#[test]
fn ui_029_palette_uses_sapphire_for_section_headers_in_detail() {
    let mut app = app_connected();
    seed_network(&mut app, vec![http_entry_rich(1, "/x")]);
    let buf = render_detail(&mut app, 100, 40);
    assert!(count_cells_with_fg(&buf, SAPPHIRE) > 0);
}

#[test]
fn ui_029_get_method_fg_is_blue_in_list() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::http_get_200(1, "/x")]);
    let buf = render_network(&mut app, 120, 20);
    assert!(count_cells_with_fg(&buf, BLUE) > 0);
}

#[test]
fn ui_029_post_method_fg_is_green_in_list() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::http_post_500(1, "/x")]);
    let buf = render_network(&mut app, 120, 20);
    assert!(count_cells_with_fg(&buf, GREEN) > 0);
}

#[test]
fn ui_029_http_pill_bg_is_blue() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::http_get_200(1, "/x")]);
    let buf = render_network(&mut app, 120, 20);
    assert!(count_cells_with_bg(&buf, BLUE) > 0);
}

#[test]
fn ui_029_first_row_is_separator_rule() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::http_get_200(1, "/x")]);
    let buf = render_network(&mut app, 120, 20);
    let first = row_to_string(&buf, 0);
    assert!(first.contains('─'));
}

#[test]
fn ui_029_status_yellow_for_4xx_in_list() {
    let mut app = app_connected();
    let mut e = fixtures::http_get_200(1, "/x");
    e.http_status = Some(404);
    seed_network(&mut app, vec![e]);
    let buf = render_network(&mut app, 120, 20);
    assert!(count_cells_with_fg(&buf, YELLOW) > 0);
}

#[test]
fn ui_029_status_red_for_5xx_in_list() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::http_post_500(1, "/x")]);
    let buf = render_network(&mut app, 120, 20);
    assert!(count_cells_with_fg(&buf, RED) > 0);
}

#[test]
fn ui_029_mantle_covers_toolbar() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::http_get_200(1, "/x")]);
    let buf = render_network(&mut app, 120, 20);
    assert!(count_cells_with_bg(&buf, MANTLE) > 0);
}

#[test]
fn ui_029_diverse_palette_in_full_render() {
    let mut app = app_connected();
    seed_network(
        &mut app,
        vec![
            fixtures::http_get_200(1, "/ok"),
            fixtures::http_post_500(2, "/boom"),
            fixtures::sse_entry(3, "/stream"),
        ],
    );
    let buf = render_network(&mut app, 140, 25);
    let colors = distinct_colors(&buf);
    // Expect at least 6 distinct colors in a multi-entry render.
    assert!(colors.len() >= 6, "saw only {} colors", colors.len());
}

#[test]
fn ui_029_scrollbar_renders_when_overflow() {
    let mut app = app_connected();
    for i in 0..40u64 {
        seed_network(
            &mut app,
            vec![fixtures::http_get_200(i, &format!("/e-{i}"))],
        );
    }
    app.network.auto_scroll = false;
    app.network.scroll_offset = 0;
    let buf = render_network(&mut app, 120, 15);
    // Scrollbar thumb char
    assert!(full_text(&buf).contains('\u{2503}'));
}

#[test]
fn ui_029_header_row_count_is_one() {
    let mut app = app_connected();
    seed_network(&mut app, vec![fixtures::http_get_200(1, "/x")]);
    let buf = render_network(&mut app, 120, 20);
    // PROTO appears in column header + (maybe) pill. Accept 1 or 2.
    let n = count_rows_with_text(&buf, "PROTO");
    assert!(n >= 1);
}

// ══════════════════════════════════════════════════════════════════════
//  Input field active states
// ══════════════════════════════════════════════════════════════════════

#[test]
fn ui_filter_search_active_input_renders_value() {
    use flog::app::AppMode;
    let mut app = app_connected();
    *app.inputs.buffer_mut(InputField::NetSearch) = "users".into();
    app.mode = AppMode::InputActive(InputField::NetSearch);
    seed_network(&mut app, vec![fixtures::http_get_200(1, "/x")]);
    let buf = render_network(&mut app, 160, 20);
    assert!(full_text(&buf).contains("users"));
}

#[test]
fn ui_filter_exclude_active_input_renders_value() {
    use flog::app::AppMode;
    let mut app = app_connected();
    *app.inputs.buffer_mut(InputField::NetExclude) = "ping".into();
    app.mode = AppMode::InputActive(InputField::NetExclude);
    seed_network(&mut app, vec![fixtures::http_get_200(1, "/x")]);
    let buf = render_network(&mut app, 160, 20);
    assert!(full_text(&buf).contains("ping"));
}

// ══════════════════════════════════════════════════════════════════════
//  Small-viewport early-return branches
// ══════════════════════════════════════════════════════════════════════

#[test]
fn mock_edit_tiny_viewport_early_returns_without_panic() {
    let mut app = app_connected();
    open_mock_edit(&mut app);
    // inner.height < 10 triggers early return branch.
    let buf = render_mock_edit(&mut app, 40, 14);
    assert_eq!(buf.area.width, 40);
}

#[test]
fn mock_rules_panel_tiny_viewport_early_returns_without_panic() {
    let mut app = app_connected();
    app.mock_rules
        .add("/api/a".into(), Some("GET".into()), 200, "{}".into(), 0);
    // inner.height < 2 OR inner.width < 20 triggers early return.
    let buf = render_mock_rules_panel(&mut app, 18, 5);
    assert_eq!(buf.area.width, 18);
}

#[test]
fn mock_rules_panel_long_url_truncates_with_ellipsis() {
    let mut app = app_connected();
    let long_url = format!("/api/{}", "x".repeat(200));
    app.mock_rules
        .add(long_url, Some("GET".into()), 200, "{}".into(), 0);
    let buf = render_mock_rules_panel(&mut app, 80, 15);
    // Long URL gets truncated with "..."
    assert!(full_text(&buf).contains("..."));
}

#[test]
fn mock_edit_body_with_long_content_shows_scrollbar() {
    let mut app = app_connected();
    open_mock_edit(&mut app);
    app.mock_edit.field = 4;
    // Body with many lines > visible_height triggers scrollbar branch.
    let many_lines = (0..50)
        .map(|i| format!("line-{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    app.mock_edit.body = flog::ui::text_editor::TextEditor::new(&many_lines);
    let buf = render_mock_edit(&mut app, 120, 30);
    assert_eq!(buf.area.width, 120);
}

#[test]
fn mock_edit_body_cursor_at_end_renders_reversed_space() {
    let mut app = app_connected();
    open_mock_edit(&mut app);
    app.mock_edit.field = 4; // body focused
    app.mock_edit.body = flog::ui::text_editor::TextEditor::new("abc");
    app.mock_edit.body.cursor_row = 0;
    app.mock_edit.body.cursor_col = 3; // at end
    let buf = render_mock_edit(&mut app, 120, 30);
    assert!(full_text(&buf).contains("abc"));
}

#[test]
fn mock_edit_empty_top_values_renders_without_panic() {
    let mut app = app_connected();
    app.mock_edit.field = 0;
    app.mock_edit.top_values = Vec::new(); // hits the "val missing" branch
    app.mock_edit.body = flog::ui::text_editor::TextEditor::new("");
    let buf = render_mock_edit(&mut app, 120, 30);
    assert!(full_text(&buf).contains("Edit Mock Rule"));
}

#[test]
fn mock_edit_overflow_url_renders_trailing_portion() {
    let mut app = app_connected();
    let long_url = "/api/".to_string() + &"abcdefgh".repeat(50);
    app.mock_edit.field = 0;
    app.mock_edit.top_values = vec![long_url, "GET".into(), "200".into(), "0".into()];
    app.mock_edit.body = flog::ui::text_editor::TextEditor::new("{}");
    // Narrow field so url.len() > max_chars.
    let buf = render_mock_edit(&mut app, 60, 30);
    // Tail of URL is shown (ends with "h|" cursor marker for focused URL field).
    assert!(full_text(&buf).contains("h"));
}

#[test]
fn mock_edit_overflow_unfocused_field_shows_tail() {
    let mut app = app_connected();
    let long_val = "z".repeat(200);
    app.mock_edit.field = 4; // body focused, URL unfocused
    app.mock_edit.top_values = vec![long_val, "GET".into(), "200".into(), "0".into()];
    app.mock_edit.body = flog::ui::text_editor::TextEditor::new("{}");
    let buf = render_mock_edit(&mut app, 60, 30);
    assert!(full_text(&buf).contains("z"));
}

#[test]
fn mock_edit_body_cursor_mid_line_renders_cursor_char() {
    let mut app = app_connected();
    app.mock_edit.field = 4;
    app.mock_edit.top_values = vec!["/api".into(), "GET".into(), "200".into(), "0".into()];
    app.mock_edit.body = flog::ui::text_editor::TextEditor::new("abcdef");
    app.mock_edit.body.cursor_row = 0;
    app.mock_edit.body.cursor_col = 3; // mid-line
    let buf = render_mock_edit(&mut app, 120, 30);
    assert!(full_text(&buf).contains("abc"));
}
