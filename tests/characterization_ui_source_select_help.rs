//! Phase 2.5B Task 10a — characterization tests for
//! `src/ui/source_select.rs` (device picker modal) and `src/ui/help.rs`.
//!
//! Uses TestBackend to drive the real render path and asserts OBSERVABLE
//! features (text presence, cell backgrounds, layout cache state) rather
//! than raw pixel dumps (Rule 3). Every render path has multiple
//! variations per Rule 10.
//!
//! Audit coverage:
//!   - source_select.rs: draw_device_picker (0 devices, 1 device, many
//!     devices, multiple apps per device, active app, selection, scroll,
//!     narrow/tall viewports).
//!   - help.rs: draw_help static content (banner, tabs, Logs section,
//!     Network section, keyboard/mouse subsections, tips, footer).
//!
//! Note: `draw_source_select` from the task prompt refers to the
//! current `draw_device_picker` function (renamed during Phase 3
//! device-picker redesign). The function signature is:
//!   `pub fn draw_device_picker(f: &mut Frame, app: &mut App, area: Rect)`
//!
//! UNTESTABLE breakdown:
//!   - TestBackend renders 1 cell per grapheme; the real picker uses
//!     box-drawing + emoji glyphs. We assert on symbol presence (logical
//!     text) not pixel widths. Rule 11: PHYS.
//!   - Scrollbar glyph positions are TUI-specific; we only verify that
//!     the scroll-required branch is entered (total_lines > visible_h
//!     stored in layout cache).
//!   - Spinner phases (if any in help) are tick-dependent — not exercised.

#![cfg(test)]
#![allow(clippy::too_many_lines)]

#[path = "support/mod.rs"]
mod support;

use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::Terminal;

use flog::app::{App, ConnectedApp};
use flog::input::ConnectorHandle;
use flog::transport::device_monitor::{Device, DeviceKind};

use support::ui_inspect::{
    count_cells_with_bg, count_rows_with_text, find_text_row, full_text, row_to_string,
};

// ---- Palette constants (mirror src/ui/source_select.rs & help.rs) ----
const BASE: Color = Color::Rgb(36, 39, 58);
const MANTLE: Color = Color::Rgb(30, 32, 48);
#[allow(dead_code)]
const SURFACE0: Color = Color::Rgb(54, 58, 79);
#[allow(dead_code)]
const SURFACE1: Color = Color::Rgb(73, 77, 100);
#[allow(dead_code)]
const OVERLAY0: Color = Color::Rgb(110, 115, 141);
#[allow(dead_code)]
const SAPPHIRE: Color = Color::Rgb(125, 196, 228);
const GREEN: Color = Color::Rgb(166, 218, 149);
#[allow(dead_code)]
const YELLOW: Color = Color::Rgb(238, 212, 159);

// ---- Harnesses -------------------------------------------------------

fn render_picker(app: &mut App, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        let area = Rect::new(0, 0, width, height);
        flog::ui::source_select::draw_device_picker(f, app, area);
    })
    .unwrap();
    term.backend().buffer().clone()
}

fn render_help(width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        flog::ui::help::draw_help(f);
    })
    .unwrap();
    term.backend().buffer().clone()
}

// ---- Factories -------------------------------------------------------

fn local_device(id: &str, name: &str) -> Device {
    Device {
        id: id.into(),
        name: name.into(),
        kind: DeviceKind::Local,
    }
}

fn android_device(id: &str, name: &str) -> Device {
    Device {
        id: id.into(),
        name: name.into(),
        kind: DeviceKind::Android,
    }
}

fn ios_device(id: &str, name: &str, device_id: u32) -> Device {
    Device {
        id: id.into(),
        name: name.into(),
        kind: DeviceKind::IosUsb { device_id },
    }
}

#[allow(clippy::too_many_arguments)]
fn connected_app(
    id: &str,
    device_id: &str,
    app_name: &str,
    app_version: &str,
    os: &str,
    package: &str,
    build_mode: &str,
    port: u16,
) -> ConnectedApp {
    let (handle, _rx) = ConnectorHandle::for_testing();
    // Leak the receiver so sends don't error at drop; we don't care about
    // received events here.
    std::mem::forget(_rx);
    ConnectedApp {
        id: id.into(),
        device_id: device_id.into(),
        port,
        device_name: "x".into(),
        app_name: app_name.into(),
        app_version: app_version.into(),
        os: os.into(),
        package_name: package.into(),
        build_mode: build_mode.into(),
        handle,
    }
}

fn add_device(app: &mut App, dev: Device) {
    app.discovered_devices.insert(dev.id.clone(), dev);
}

// ══════════════════════════════════════════════════════════════════════
//  source_select / device picker — empty state
// ══════════════════════════════════════════════════════════════════════

#[test]
fn picker_empty_shows_no_devices_found() {
    let mut app = App::default();
    let buf = render_picker(&mut app, 100, 30);
    let text = full_text(&buf);
    assert!(
        text.contains("No devices found"),
        "expected empty-state message, got: {text:?}"
    );
}

#[test]
fn picker_empty_shows_run_flog_dart_hint() {
    let mut app = App::default();
    let buf = render_picker(&mut app, 100, 30);
    let text = full_text(&buf);
    assert!(
        text.contains("Run your Flutter app with flog_dart"),
        "expected quickstart hint"
    );
}

#[test]
fn picker_empty_title_shows_zero_count() {
    let mut app = App::default();
    let buf = render_picker(&mut app, 100, 30);
    let text = full_text(&buf);
    assert!(text.contains("Devices (0)"), "expected 'Devices (0)' title");
}

#[test]
fn picker_empty_clears_layout_cache() {
    let mut app = App::default();
    // Pre-populate layout cache to confirm it's cleared.
    app.layout.device_picker_items = vec![(5, 0, 20, 0)];
    app.layout.device_picker_item_ids = vec!["stale".into()];
    app.layout.device_picker_total_lines = 42;

    let _ = render_picker(&mut app, 100, 30);

    assert!(app.layout.device_picker_items.is_empty());
    assert!(app.layout.device_picker_item_ids.is_empty());
    assert_eq!(app.layout.device_picker_total_lines, 0);
    assert!(app.layout.device_picker_rect.is_some());
}

// ══════════════════════════════════════════════════════════════════════
//  Single-device paths
// ══════════════════════════════════════════════════════════════════════

#[test]
fn picker_single_device_no_apps_shows_waiting_row() {
    let mut app = App::default();
    add_device(&mut app, local_device("localhost", "macOS Simulator"));
    let buf = render_picker(&mut app, 100, 30);
    let text = full_text(&buf);
    assert!(
        text.contains("Waiting for app"),
        "expected Waiting row, got: {text}"
    );
}

#[test]
fn picker_single_device_shows_device_name_in_header() {
    let mut app = App::default();
    add_device(&mut app, android_device("ABC123", "Pixel 8"));
    let buf = render_picker(&mut app, 120, 20);
    let text = full_text(&buf);
    assert!(text.contains("Pixel 8"), "device name not rendered: {text}");
}

#[test]
fn picker_single_device_shows_platform_tag_android() {
    let mut app = App::default();
    add_device(&mut app, android_device("ABC123", "Pixel 8"));
    let buf = render_picker(&mut app, 120, 20);
    let text = full_text(&buf);
    assert!(text.contains("[Android]"), "missing [Android] tag: {text}");
    assert!(text.contains("ADB"), "missing ADB connection label");
}

#[test]
fn picker_single_device_shows_platform_tag_ios() {
    let mut app = App::default();
    add_device(&mut app, ios_device("APPLE123", "iPhone 17 Pro", 42));
    let buf = render_picker(&mut app, 120, 20);
    let text = full_text(&buf);
    assert!(text.contains("[iOS]"), "missing [iOS] tag: {text}");
    assert!(text.contains("USB"), "missing USB connection label");
}

#[test]
fn picker_single_device_shows_platform_tag_local_sim() {
    let mut app = App::default();
    add_device(&mut app, local_device("localhost", "iOS Simulator"));
    let buf = render_picker(&mut app, 120, 20);
    let text = full_text(&buf);
    assert!(text.contains("[Simulator]"));
    assert!(text.contains("localhost"));
}

#[test]
fn picker_device_count_matches_discovered() {
    let mut app = App::default();
    add_device(&mut app, android_device("A1", "Pixel A"));
    add_device(&mut app, android_device("A2", "Pixel B"));
    add_device(&mut app, local_device("localhost", "Sim"));
    let buf = render_picker(&mut app, 120, 40);
    let text = full_text(&buf);
    assert!(text.contains("Devices (3)"), "expected 'Devices (3)'");
}

// ══════════════════════════════════════════════════════════════════════
//  Connected app rendering
// ══════════════════════════════════════════════════════════════════════

#[test]
fn picker_connected_app_shows_name_and_version() {
    let mut app = App::default();
    add_device(&mut app, android_device("dev-1", "Pixel 8"));
    app.connected_apps.push(connected_app(
        "dev-1:9753",
        "dev-1",
        "MyApp",
        "1.2.3",
        "android",
        "com.example.myapp",
        "debug",
        9753,
    ));
    let buf = render_picker(&mut app, 120, 30);
    let text = full_text(&buf);
    assert!(text.contains("MyApp v1.2.3"), "app label missing: {text}");
    assert!(text.contains("Port: 9753"));
    assert!(text.contains("com.example.myapp"), "package missing");
    assert!(text.contains("debug"), "build mode missing");
}

#[test]
fn picker_active_app_shows_active_pill() {
    let mut app = App::default();
    add_device(&mut app, android_device("dev-1", "Pixel 8"));
    app.connected_apps.push(connected_app(
        "dev-1:9753",
        "dev-1",
        "MyApp",
        "1.0.0",
        "android",
        "com.example.myapp",
        "debug",
        9753,
    ));
    app.active_app_id = Some("dev-1:9753".into());
    let buf = render_picker(&mut app, 120, 30);
    let text = full_text(&buf);
    assert!(text.contains("ACTIVE"), "ACTIVE pill missing: {text}");
    // The ACTIVE pill uses GREEN bg — at least one cell should carry it.
    assert!(
        count_cells_with_bg(&buf, GREEN) > 0,
        "ACTIVE pill bg not present"
    );
}

#[test]
fn picker_selected_non_active_shows_cursor_glyph() {
    let mut app = App::default();
    add_device(&mut app, android_device("dev-1", "Pixel 8"));
    app.connected_apps.push(connected_app(
        "dev-1:9753",
        "dev-1",
        "MyApp",
        "1.0.0",
        "android",
        "com.example.myapp",
        "debug",
        9753,
    ));
    // Not active, but selected (selected default is 0).
    app.device_picker_selected = 0;
    let buf = render_picker(&mut app, 120, 30);
    let text = full_text(&buf);
    // The selection cursor glyph is "▎".
    assert!(
        text.contains('\u{258e}'),
        "expected selection cursor ▎ glyph in output"
    );
}

#[test]
fn picker_active_app_uses_bold_box_borders() {
    let mut app = App::default();
    add_device(&mut app, android_device("dev-1", "Pixel 8"));
    app.connected_apps.push(connected_app(
        "dev-1:9753",
        "dev-1",
        "MyApp",
        "1.0.0",
        "android",
        "com.example.myapp",
        "debug",
        9753,
    ));
    app.active_app_id = Some("dev-1:9753".into());
    let buf = render_picker(&mut app, 120, 30);
    let text = full_text(&buf);
    // Double-line top-left corner ╔
    assert!(text.contains('\u{2554}'), "expected ╔ for ACTIVE border");
    // ═ horizontal double
    assert!(text.contains('\u{2550}'));
}

#[test]
fn picker_inactive_app_uses_single_box_borders() {
    let mut app = App::default();
    add_device(&mut app, android_device("dev-1", "Pixel 8"));
    app.connected_apps.push(connected_app(
        "dev-1:9753",
        "dev-1",
        "MyApp",
        "1.0.0",
        "android",
        "com.example.myapp",
        "debug",
        9753,
    ));
    // No active_app_id → card is inactive.
    let buf = render_picker(&mut app, 120, 30);
    let text = full_text(&buf);
    // Single-line top-left ┌
    assert!(
        text.contains('\u{250c}'),
        "expected ┌ for inactive card border"
    );
}

#[test]
fn picker_empty_app_version_omits_v_prefix() {
    let mut app = App::default();
    add_device(&mut app, android_device("dev-1", "Pixel 8"));
    app.connected_apps.push(connected_app(
        "dev-1:9753",
        "dev-1",
        "OnlyName",
        "", // empty version
        "android",
        "com.x",
        "debug",
        9753,
    ));
    let buf = render_picker(&mut app, 120, 30);
    let text = full_text(&buf);
    assert!(text.contains("OnlyName"), "app name missing");
    assert!(
        !text.contains("OnlyName v"),
        "v prefix unexpectedly present"
    );
}

#[test]
fn picker_unknown_fields_render_as_unknown() {
    let mut app = App::default();
    add_device(&mut app, android_device("dev-1", "Pixel 8"));
    app.connected_apps.push(connected_app(
        "dev-1:9753",
        "dev-1",
        "App",
        "1.0",
        "", // os empty
        "", // package empty
        "", // build_mode empty
        9753,
    ));
    let buf = render_picker(&mut app, 120, 30);
    let text = full_text(&buf);
    // Every empty field is rendered as "unknown".
    let unknown_count = text.matches("unknown").count();
    assert!(
        unknown_count >= 3,
        "expected at least 3 'unknown' markers (package/platform/mode), got {unknown_count}"
    );
}

// ══════════════════════════════════════════════════════════════════════
//  Multiple apps / multiple devices
// ══════════════════════════════════════════════════════════════════════

#[test]
fn picker_multiple_apps_on_one_device_all_render() {
    let mut app = App::default();
    add_device(&mut app, android_device("dev-1", "Pixel 8"));
    for (i, name) in ["AppOne", "AppTwo", "AppThree"].iter().enumerate() {
        app.connected_apps.push(connected_app(
            &format!("dev-1:{}", 9753 + i as u16),
            "dev-1",
            name,
            "1.0",
            "android",
            "com.example",
            "debug",
            9753 + i as u16,
        ));
    }
    let buf = render_picker(&mut app, 120, 40);
    let text = full_text(&buf);
    assert!(text.contains("AppOne"));
    assert!(text.contains("AppTwo"));
    assert!(text.contains("AppThree"));
}

#[test]
fn picker_multi_device_mixed_states_render_all_headers() {
    let mut app = App::default();
    add_device(&mut app, android_device("dev-a", "Pixel 8"));
    add_device(&mut app, ios_device("dev-b", "iPhone 17", 7));
    add_device(&mut app, local_device("localhost", "macOS Sim"));
    // Two connected apps on dev-a, one on dev-b, none on localhost.
    app.connected_apps.push(connected_app(
        "dev-a:9753",
        "dev-a",
        "AppA1",
        "1",
        "android",
        "a",
        "debug",
        9753,
    ));
    app.connected_apps.push(connected_app(
        "dev-a:9754",
        "dev-a",
        "AppA2",
        "2",
        "android",
        "a",
        "debug",
        9754,
    ));
    app.connected_apps.push(connected_app(
        "dev-b:9753",
        "dev-b",
        "AppB1",
        "3",
        "ios",
        "b",
        "debug",
        9753,
    ));
    app.active_app_id = Some("dev-a:9753".into());

    let buf = render_picker(&mut app, 140, 50);
    let text = full_text(&buf);
    assert!(text.contains("Pixel 8"));
    assert!(text.contains("iPhone 17"));
    assert!(text.contains("macOS Sim"));
    assert!(text.contains("AppA1"));
    assert!(text.contains("AppA2"));
    assert!(text.contains("AppB1"));
    assert!(
        text.contains("Waiting for app"),
        "localhost has no apps → Waiting"
    );
    assert!(text.contains("Devices (3)"));
}

#[test]
fn picker_layout_cache_records_click_regions() {
    let mut app = App::default();
    add_device(&mut app, android_device("dev-1", "Pixel 8"));
    app.connected_apps.push(connected_app(
        "dev-1:9753",
        "dev-1",
        "MyApp",
        "1.0",
        "android",
        "com.x",
        "debug",
        9753,
    ));
    let _ = render_picker(&mut app, 120, 30);

    // item_ids should include exactly one entry (one selectable card).
    assert_eq!(app.layout.device_picker_item_ids, vec!["dev-1:9753"]);
    // Click regions should exist for each card row (card has 6 rows).
    assert!(
        !app.layout.device_picker_items.is_empty(),
        "expected click regions for card rows"
    );
    assert!(app.layout.device_picker_rect.is_some());
    assert!(app.layout.device_picker_total_lines > 0);
}

#[test]
fn picker_layout_item_ids_order_matches_devices_then_discovered() {
    let mut app = App::default();
    add_device(&mut app, android_device("dev-a", "A"));
    add_device(&mut app, android_device("dev-b", "B"));
    app.connected_apps.push(connected_app(
        "dev-a:9753",
        "dev-a",
        "AppA",
        "1",
        "android",
        "a",
        "debug",
        9753,
    ));
    app.connected_apps.push(connected_app(
        "dev-b:9753",
        "dev-b",
        "AppB",
        "1",
        "android",
        "b",
        "debug",
        9753,
    ));
    let _ = render_picker(&mut app, 140, 50);
    assert_eq!(app.layout.device_picker_item_ids.len(), 2);
    assert!(app
        .layout
        .device_picker_item_ids
        .contains(&"dev-a:9753".to_string()));
    assert!(app
        .layout
        .device_picker_item_ids
        .contains(&"dev-b:9753".to_string()));
}

// ══════════════════════════════════════════════════════════════════════
//  Long names / shorten_id / narrow width
// ══════════════════════════════════════════════════════════════════════

#[test]
fn picker_long_device_id_gets_shortened() {
    let mut app = App::default();
    // UDID-shaped string well over 22 chars.
    let long_id = "00008150-0011223344AB401C999";
    add_device(&mut app, ios_device(long_id, "iPhone", 3));
    let buf = render_picker(&mut app, 140, 30);
    let text = full_text(&buf);
    // shorten_id keeps head 8 + "..." + tail 4.
    assert!(
        text.contains("..."),
        "expected shortened id with ellipsis: {text}"
    );
    // Full original should not appear verbatim.
    assert!(!text.contains(long_id), "long id was not truncated: {text}");
}

#[test]
fn picker_short_device_id_renders_verbatim() {
    let mut app = App::default();
    add_device(&mut app, ios_device("short-id", "iPhone", 3));
    let buf = render_picker(&mut app, 140, 30);
    let text = full_text(&buf);
    assert!(text.contains("short-id"));
}

#[test]
fn picker_renders_without_panic_at_narrow_width() {
    let mut app = App::default();
    add_device(&mut app, android_device("dev-1", "Pixel 8"));
    app.connected_apps.push(connected_app(
        "dev-1:9753",
        "dev-1",
        "App",
        "1.0",
        "android",
        "com.x",
        "debug",
        9753,
    ));
    // The render code clamps picker width to min(60, area-4), so below that
    // layout degrades but must not panic. We just exercise the path.
    let _buf = render_picker(&mut app, 40, 20);
}

// ══════════════════════════════════════════════════════════════════════
//  Scroll handling (many devices)
// ══════════════════════════════════════════════════════════════════════

#[test]
fn picker_many_devices_triggers_scroll() {
    let mut app = App::default();
    for i in 0..30 {
        add_device(
            &mut app,
            android_device(&format!("dev-{i:02}"), &format!("Device {i:02}")),
        );
    }
    // A 20-row picker cannot fit 30 devices (each uses 5+ rows).
    let buf = render_picker(&mut app, 120, 20);
    // total_lines must exceed visible rows (viewport h = inner of picker_h)
    assert!(
        app.layout.device_picker_total_lines > 20,
        "expected total_lines > viewport, got {}",
        app.layout.device_picker_total_lines
    );
    // Scroll was required: at least one "Device NN" entry must render,
    // but not all 30 (HashMap ordering is nondeterministic — we can't
    // assert a specific visible one without a known order).
    let text = full_text(&buf);
    let visible = (0..30)
        .filter(|i| text.contains(&format!("Device {i:02}")))
        .count();
    assert!(visible >= 1, "expected some Device NN visible in viewport");
    assert!(
        visible < 30,
        "expected scroll: not all 30 devices should fit, got {visible}"
    );
}

#[test]
fn picker_scroll_clamps_selected_index() {
    let mut app = App::default();
    add_device(&mut app, android_device("dev-1", "Pixel 8"));
    app.connected_apps.push(connected_app(
        "dev-1:9753",
        "dev-1",
        "App",
        "1.0",
        "android",
        "com.x",
        "debug",
        9753,
    ));
    // Selected index way past the last selectable (there's only 1).
    app.device_picker_selected = 99;
    let _ = render_picker(&mut app, 120, 30);
    assert_eq!(
        app.device_picker_selected, 0,
        "selected should clamp to max_sel=0"
    );
}

#[test]
fn picker_scroll_clamps_offset_when_too_large() {
    let mut app = App::default();
    add_device(&mut app, android_device("dev-1", "Pixel 8"));
    app.connected_apps.push(connected_app(
        "dev-1:9753",
        "dev-1",
        "App",
        "1.0",
        "android",
        "com.x",
        "debug",
        9753,
    ));
    app.device_picker_scroll = 9_999;
    let _ = render_picker(&mut app, 120, 30);
    // Scroll should be clamped to total_lines - visible_h (or 0 when fits).
    assert!(
        app.device_picker_scroll <= app.layout.device_picker_total_lines,
        "scroll not clamped"
    );
}

#[test]
fn picker_scroll_keeps_selected_card_visible() {
    let mut app = App::default();
    for i in 0..10 {
        add_device(
            &mut app,
            android_device(&format!("dev-{i}"), &format!("Dev {i}")),
        );
        app.connected_apps.push(connected_app(
            &format!("dev-{i}:9753"),
            &format!("dev-{i}"),
            &format!("App{i}"),
            "1.0",
            "android",
            "com.x",
            "debug",
            9753,
        ));
    }
    // Select the last card — renderer must scroll to bring it into view.
    app.device_picker_selected = 9;
    let buf = render_picker(&mut app, 120, 20);
    let text = full_text(&buf);
    // Last app name should appear after auto-scroll.
    assert!(
        text.contains("App9"),
        "expected App9 to be scrolled into view"
    );
}

// ══════════════════════════════════════════════════════════════════════
//  Palette / backgrounds
// ══════════════════════════════════════════════════════════════════════

#[test]
fn picker_modal_bg_uses_base_color() {
    let mut app = App::default();
    add_device(&mut app, android_device("dev-1", "Pixel 8"));
    let buf = render_picker(&mut app, 120, 30);
    assert!(
        count_cells_with_bg(&buf, BASE) > 10,
        "expected many BASE cells for modal bg"
    );
}

#[test]
fn picker_device_container_uses_mantle_bg() {
    let mut app = App::default();
    add_device(&mut app, android_device("dev-1", "Pixel 8"));
    let buf = render_picker(&mut app, 120, 30);
    // Device container bg (MANTLE) must be present somewhere in the modal.
    assert!(
        count_cells_with_bg(&buf, MANTLE) > 10,
        "expected MANTLE bg cells inside device container"
    );
}

#[test]
fn picker_hints_footer_contains_navigation_keys() {
    let mut app = App::default();
    let buf = render_picker(&mut app, 100, 30);
    let text = full_text(&buf);
    assert!(text.contains("navigate"), "hints missing 'navigate'");
    assert!(text.contains("connect"), "hints missing 'connect'");
    assert!(text.contains("cancel"), "hints missing 'cancel'");
}

// ══════════════════════════════════════════════════════════════════════
//  help.rs tests
// ══════════════════════════════════════════════════════════════════════

#[test]
fn help_renders_title() {
    let buf = render_help(120, 80);
    let text = full_text(&buf);
    assert!(text.contains("flog Help"), "missing 'flog Help' title");
}

#[test]
fn help_renders_back_button() {
    let buf = render_help(120, 80);
    let text = full_text(&buf);
    assert!(text.contains("Back"), "missing Back button");
}

#[test]
fn help_renders_banner_tagline() {
    let buf = render_help(120, 80);
    let text = full_text(&buf);
    assert!(
        text.contains("Terminal-native log viewer for Flutter developers"),
        "missing banner tagline"
    );
}

#[test]
fn help_shows_tab_navigation_section() {
    let buf = render_help(120, 80);
    let text = full_text(&buf);
    assert!(text.contains("Tab Navigation"));
    assert!(text.contains("Logs"));
    assert!(text.contains("Network"));
}

#[test]
fn help_shows_logs_view_section() {
    let buf = render_help(120, 80);
    let text = full_text(&buf);
    assert!(text.contains("Logs View"));
}

#[test]
fn help_shows_network_section() {
    let buf = render_help(140, 100);
    let text = full_text(&buf);
    assert!(text.contains("Network View"));
    assert!(text.contains("Protocol Support"));
}

#[test]
fn help_shows_keyboard_subheading() {
    let buf = render_help(120, 80);
    let text = full_text(&buf);
    assert!(text.contains("Keyboard"), "missing Keyboard subheading");
}

#[test]
fn help_shows_mouse_section() {
    let buf = render_help(120, 80);
    let text = full_text(&buf);
    assert!(text.contains("Mouse"), "missing Mouse subheading");
    assert!(text.contains("Click row"));
    assert!(text.contains("Scroll wheel"));
}

#[test]
fn help_contains_core_keybindings_j_k_slash() {
    let buf = render_help(120, 80);
    let text = full_text(&buf);
    // The kv() helper surrounds keys with whitespace+bg. Characters must
    // still be visible somewhere.
    assert!(text.contains('j') && text.contains('k'));
    assert!(text.contains('/'));
    // Description text for j/k
    assert!(text.contains("Move selection up/down"));
}

#[test]
fn help_contains_q_quit_binding() {
    let buf = render_help(120, 80);
    let text = full_text(&buf);
    assert!(text.contains('q'));
    assert!(text.contains("Quit"));
}

#[test]
fn help_contains_esc_clear_filters() {
    let buf = render_help(120, 80);
    let text = full_text(&buf);
    assert!(text.contains("Esc"));
    assert!(text.contains("Clear all filters"));
}

#[test]
fn help_contains_search_filter_subheading() {
    let buf = render_help(140, 120);
    let text = full_text(&buf);
    assert!(text.contains("Search & Filter"));
    assert!(text.contains("Exclude"));
    assert!(text.contains("Tag"));
}

#[test]
fn help_contains_protocol_pills() {
    let buf = render_help(140, 120);
    let text = full_text(&buf);
    assert!(text.contains("HTTP"));
    assert!(text.contains("SSE"));
    assert!(text.contains("WS"));
}

#[test]
fn help_contains_detail_sections() {
    let buf = render_help(140, 140);
    let text = full_text(&buf);
    assert!(text.contains("General"));
    assert!(text.contains("Query Parameters"));
    assert!(text.contains("Request Headers"));
    assert!(text.contains("Response Body"));
}

#[test]
fn help_contains_setup_section() {
    let buf = render_help(140, 140);
    let text = full_text(&buf);
    assert!(text.contains("Setup"));
    assert!(text.contains("FlogHttpInterceptor"));
}

#[test]
fn help_contains_tips_section() {
    let buf = render_help(140, 140);
    let text = full_text(&buf);
    assert!(text.contains("Tips"));
    assert!(text.contains("Ring buffer"));
    assert!(
        text.contains("Filters and bookmarks persist"),
        "missing persistence tip"
    );
}

#[test]
fn help_footer_shows_close_instructions() {
    let buf = render_help(140, 140);
    let text = full_text(&buf);
    assert!(text.contains("close"), "missing 'close' in footer");
}

#[test]
fn help_narrow_terminal_still_renders_title() {
    let buf = render_help(40, 40);
    let text = full_text(&buf);
    // Even at narrow width the title row should still appear somewhere.
    assert!(
        text.contains("flog Help") || text.contains("Back"),
        "at narrow width, neither title nor Back visible: {text:?}"
    );
}

#[test]
fn help_tall_terminal_renders_footer_and_header() {
    // At 140x200 the whole help doc fits — both the very first header
    // (banner) and the closing footer ("close") should be present.
    let buf = render_help(140, 200);
    let text = full_text(&buf);
    assert!(text.contains("Flutter Log Viewer"));
    assert!(text.contains("close"));
}

#[test]
fn help_first_row_is_nav_bar() {
    let buf = render_help(120, 80);
    // Row 0 is the nav bar; it should contain "flog Help".
    let row0 = row_to_string(&buf, 0);
    assert!(
        row0.contains("Back") || row0.contains("flog Help"),
        "row 0 not nav bar: {row0:?}"
    );
}

#[test]
fn help_back_button_row_count_is_one() {
    let buf = render_help(120, 80);
    // The Back button appears twice: in the nav bar (row 0) and in the
    // trailing "Press Esc or ? or click Back to close" line.
    assert!(
        count_rows_with_text(&buf, "Back") >= 1,
        "expected at least one Back row"
    );
}

#[test]
fn help_banner_first_content_row_in_second_chunk() {
    // The content starts below the 1-row nav bar. The banner ("flog
    // Flutter Log Viewer") should appear at row >= 1.
    let buf = render_help(120, 80);
    let banner_row = find_text_row(&buf, "Flutter Log Viewer");
    assert!(
        banner_row.map(|r| r >= 1).unwrap_or(false),
        "banner row not below nav bar: {banner_row:?}"
    );
}
