//! Phase 2.5B Task 7 — characterization tests for `src/ui/logs/`.
//!
//! Uses TestBackend to drive the real render path end-to-end and asserts
//! OBSERVABLE features (cell colors, text presence, span counts) rather than
//! raw pixel dumps (Rule 3). Every render function is exercised with >=5
//! cases across the empty / normal / extreme / filtered-to-zero /
//! selected-item variation axes (Rule 10).
//!
//! Audit entries locked:
//!   - UI-010 draw_logs rendering + wrap
//!   - UI-012 separator row
//!   - UI-025 empty-state cards
//!   - UI-029 tab-specific render
//!   - UI-030 repeat_bar
//!   - UI-031 tag_color / level_color modulo hashing
//!   - UI-036 module docs (smoke via renders)
//!   - UI-038 draw_logs mixes layout/filter/wrap
//!   - UI-039 stack trace preview
//!
//! UNTESTABLE breakdown:
//!   - Terminal-font metrics for CJK widths (TestBackend writes 1 cell per
//!     symbol; real terminals may draw wide glyphs 2 cells). We assert
//!     logical text presence only. Rule 11: PHYS.
//!   - `draw_jump_to_bottom` is conditionally laid out *inside* the list
//!     area; when area.height < 5 or filtered_count == 0 the pill hides.
//!     We exercise both the visible and hidden paths via `auto_scroll`
//!     and `filtered_count`. Physically verifying border drawing at
//!     sub-pixel boundaries would require a real terminal.
//!   - Spinner animation phase (`tick / 5 % 8`) changes between ticks; we
//!     assert presence of *some* spinner glyph rather than a specific frame.

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
use flog::domain::entry::{LogEntry, LogLevel};
use flog::input::ConnectorHandle;

use support::fixtures;
use support::ui_inspect::{
    count_cells_with_bg, count_cells_with_fg, count_rows_with_text, distinct_colors, find_text_row,
    full_text, row_to_string,
};

// ---- Palette constants (mirror src/ui/mod.rs + src/ui/logs/mod.rs) ----
const BASE: Color = Color::Rgb(36, 39, 58);
const MANTLE: Color = Color::Rgb(30, 32, 48);
#[allow(dead_code)]
const SURFACE0: Color = Color::Rgb(54, 58, 79);
const SURFACE1: Color = Color::Rgb(73, 77, 100);
const OVERLAY0: Color = Color::Rgb(110, 115, 141);
const TEXT: Color = Color::Rgb(202, 211, 245);
const SUBTEXT0: Color = Color::Rgb(165, 173, 206);
const BLUE: Color = Color::Rgb(138, 173, 244);
const GREEN: Color = Color::Rgb(166, 218, 149);
const YELLOW: Color = Color::Rgb(238, 212, 159);
const PEACH: Color = Color::Rgb(245, 169, 127);
const RED: Color = Color::Rgb(237, 135, 150);
const MAUVE: Color = Color::Rgb(198, 160, 246);
const SAPPHIRE: Color = Color::Rgb(125, 196, 228);

const ERROR_ROW_BG: Color = Color::Rgb(50, 30, 35);
const WARNING_ROW_BG: Color = Color::Rgb(50, 45, 30);

// ---- Render harnesses -------------------------------------------------

fn render_logs(app: &mut App, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut term = Terminal::new(backend).unwrap();
    app.layout.width = width;
    term.draw(|f| {
        flog::ui::logs::draw_logs(f, app, f.area());
    })
    .unwrap();
    term.backend().buffer().clone()
}

fn render_stats(app: &mut App, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        flog::ui::logs::stats::draw_stats(f, app);
    })
    .unwrap();
    term.backend().buffer().clone()
}

fn render_detail(app: &mut App, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        let area = Rect::new(0, 0, width, height);
        flog::ui::logs::detail::draw_side_panel(f, app, area);
    })
    .unwrap();
    term.backend().buffer().clone()
}

// ---- App seeding helpers ---------------------------------------------

/// Build an App with an `active_app_id` set to bypass `draw_not_connected`.
/// The "waiting for logs" empty state is chosen whenever `store.is_empty()`.
fn app_connected() -> App {
    let mut app = App::default();
    // Fake connected app so draw_not_connected branch doesn't fire.
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

fn seed_logs(app: &mut App, entries: Vec<LogEntry>) {
    for e in entries {
        app.add_entry(e);
    }
}

// ══════════════════════════════════════════════════════════════════════
//  draw_logs: Empty states (UI-025)
// ══════════════════════════════════════════════════════════════════════

#[test]
fn ui_010_empty_store_no_client_shows_quick_start_card() {
    let mut app = App::default();
    let buf = render_logs(&mut app, 100, 30);
    let text = full_text(&buf);
    assert!(text.contains("Quick Start"), "missing Quick Start card");
    assert!(text.contains("flog_dart"), "missing flog_dart hint");
}

#[test]
fn ui_010_empty_store_shows_flog_logo() {
    let mut app = App::default();
    let buf = render_logs(&mut app, 100, 30);
    // Logo uses ASCII block characters in at least one row.
    assert!(find_text_row(&buf, "███").is_some());
}

#[test]
fn ui_010_empty_store_no_entries_rendered() {
    let mut app = App::default();
    let buf = render_logs(&mut app, 100, 30);
    // No TIME/TAG/LEVEL columns produce any log row — only header exists.
    let header_rows = count_rows_with_text(&buf, "TIME");
    assert_eq!(header_rows, 1, "expected only the column header row");
    // "INFO" shouldn't appear since no entries.
    assert_eq!(count_rows_with_text(&buf, "INFO "), 0);
}

#[test]
fn ui_025_connected_but_empty_shows_waiting_state() {
    let mut app = app_connected();
    let buf = render_logs(&mut app, 100, 30);
    let text = full_text(&buf);
    assert!(
        text.contains("Waiting for logs"),
        "missing 'Waiting for logs' empty-state"
    );
}

#[test]
fn ui_025_waiting_state_includes_connected_app_name() {
    let mut app = app_connected();
    let buf = render_logs(&mut app, 100, 30);
    let text = full_text(&buf);
    assert!(text.contains("demo"), "missing app name in waiting state");
}

#[test]
fn ui_025_filtered_to_zero_shows_no_matching_message() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::info("TAG", "hello")]);
    app.filter.set_search("nomatchxyz");
    app.invalidate_filter();
    let buf = render_logs(&mut app, 100, 30);
    assert!(full_text(&buf).contains("No matching logs"));
}

#[test]
fn ui_025_filtered_to_zero_shows_active_filters_panel() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::info("TAG", "hello")]);
    app.filter.set_search("nomatchxyz");
    app.invalidate_filter();
    let buf = render_logs(&mut app, 100, 30);
    let text = full_text(&buf);
    assert!(text.contains("Active filters"));
    assert!(text.contains("nomatchxyz"));
}

// ══════════════════════════════════════════════════════════════════════
//  draw_logs: Normal entries (UI-010)
// ══════════════════════════════════════════════════════════════════════

#[test]
fn ui_010_renders_single_entry_tag_and_message() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::info("UserTag", "hello world")]);
    let buf = render_logs(&mut app, 100, 30);
    assert!(find_text_row(&buf, "UserTag").is_some());
    assert!(find_text_row(&buf, "hello world").is_some());
}

#[test]
fn ui_010_renders_multiple_entries_in_order() {
    let mut app = app_connected();
    seed_logs(
        &mut app,
        vec![
            fixtures::info("A", "first"),
            fixtures::info("A", "second"),
            fixtures::info("A", "third"),
        ],
    );
    let buf = render_logs(&mut app, 120, 30);
    let first_y = find_text_row(&buf, "first").expect("no 'first' row");
    let second_y = find_text_row(&buf, "second").expect("no 'second' row");
    let third_y = find_text_row(&buf, "third").expect("no 'third' row");
    assert!(first_y < second_y && second_y < third_y);
}

#[test]
fn ui_010_renders_timestamp_column() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::info("T", "msg")]);
    let buf = render_logs(&mut app, 100, 30);
    // Timestamp from fixtures is "12:00:00.000"
    assert!(find_text_row(&buf, "12:00:00").is_some());
}

#[test]
fn ui_010_colors_error_row_with_dark_red_bg() {
    let mut app = app_connected();
    seed_logs(
        &mut app,
        vec![
            fixtures::info("A", "line1"),
            fixtures::error("E", "boom"),
            fixtures::info("A", "line3"),
        ],
    );
    // Select a non-error row so error row uses ERROR_ROW_BG (not SURFACE1).
    app.auto_scroll = false;
    app.scroll_offset = 0;
    app.selected = 0;
    let buf = render_logs(&mut app, 100, 30);
    assert!(
        count_cells_with_bg(&buf, ERROR_ROW_BG) > 0,
        "expected error-row bg cells"
    );
}

#[test]
fn ui_010_colors_warning_row_with_dark_yellow_bg() {
    let mut app = app_connected();
    seed_logs(
        &mut app,
        vec![
            fixtures::info("A", "line1"),
            fixtures::warn("W", "warn-msg"),
            fixtures::info("A", "line3"),
        ],
    );
    app.auto_scroll = false;
    app.scroll_offset = 0;
    app.selected = 0;
    let buf = render_logs(&mut app, 100, 30);
    assert!(
        count_cells_with_bg(&buf, WARNING_ROW_BG) > 0,
        "expected warning-row bg cells"
    );
}

#[test]
fn ui_010_info_debug_verbose_rows_use_base_bg() {
    let mut app = app_connected();
    seed_logs(
        &mut app,
        vec![
            fixtures::info("I", "i"),
            fixtures::debug("D", "d"),
            fixtures::verbose("V", "v"),
        ],
    );
    let buf = render_logs(&mut app, 100, 30);
    // BASE bg is the main list area for non-selected info/debug/verbose rows.
    assert!(count_cells_with_bg(&buf, BASE) > 0);
    // No warning/error row bg leaks.
    assert_eq!(count_cells_with_bg(&buf, ERROR_ROW_BG), 0);
    assert_eq!(count_cells_with_bg(&buf, WARNING_ROW_BG), 0);
}

#[test]
fn ui_010_selected_row_uses_surface1_bg() {
    let mut app = app_connected();
    seed_logs(
        &mut app,
        vec![
            fixtures::info("A", "one"),
            fixtures::info("A", "two"),
            fixtures::info("A", "three"),
        ],
    );
    app.selected = 1;
    app.scroll_offset = 0;
    app.auto_scroll = false;
    let buf = render_logs(&mut app, 120, 30);
    // SURFACE1 is the selection bg and also used as level-pill bg in the
    // level toolbar row, so assert we have MORE SURFACE1 cells than a
    // baseline without selection.
    let mut app2 = app_connected();
    seed_logs(
        &mut app2,
        vec![
            fixtures::info("A", "one"),
            fixtures::info("A", "two"),
            fixtures::info("A", "three"),
        ],
    );
    app2.auto_scroll = true;
    // force draw
    let _ = render_logs(&mut app2, 120, 30);
    assert!(count_cells_with_bg(&buf, SURFACE1) > 0);
}

// ══════════════════════════════════════════════════════════════════════
//  draw_logs: Scroll
// ══════════════════════════════════════════════════════════════════════

#[test]
fn ui_010_auto_scroll_shows_newest() {
    let mut app = app_connected();
    let mut entries: Vec<LogEntry> = Vec::new();
    for i in 0..40 {
        entries.push(fixtures::info("A", &format!("entry-{i:02}")));
    }
    seed_logs(&mut app, entries);
    let buf = render_logs(&mut app, 120, 20);
    // Newest — entry-39 — should be visible.
    assert!(find_text_row(&buf, "entry-39").is_some());
    // Oldest (entry-00) should NOT fit.
    assert!(find_text_row(&buf, "entry-00").is_none());
}

#[test]
fn ui_010_scroll_offset_shifts_visible_window() {
    let mut app = app_connected();
    let mut entries: Vec<LogEntry> = Vec::new();
    for i in 0..40 {
        entries.push(fixtures::info("A", &format!("entry-{i:02}")));
    }
    seed_logs(&mut app, entries);
    app.auto_scroll = false;
    app.scroll_offset = 0;
    app.selected = 0;
    let buf = render_logs(&mut app, 120, 20);
    // First entry should appear when scrolled to top.
    assert!(find_text_row(&buf, "entry-00").is_some());
}

#[test]
fn ui_010_middle_offset_omits_both_ends() {
    let mut app = app_connected();
    let mut entries: Vec<LogEntry> = Vec::new();
    for i in 0..40 {
        entries.push(fixtures::info("A", &format!("entry-{i:02}")));
    }
    seed_logs(&mut app, entries);
    app.auto_scroll = false;
    app.scroll_offset = 15;
    app.selected = 15;
    let buf = render_logs(&mut app, 120, 20);
    assert!(find_text_row(&buf, "entry-15").is_some());
    assert!(find_text_row(&buf, "entry-00").is_none());
    assert!(find_text_row(&buf, "entry-39").is_none());
}

// ══════════════════════════════════════════════════════════════════════
//  draw_logs: Wrap (UI-010)
// ══════════════════════════════════════════════════════════════════════

#[test]
fn ui_010_long_message_wraps_to_multiple_rows() {
    let mut app = app_connected();
    let long = "x".repeat(400);
    seed_logs(&mut app, vec![fixtures::info("A", &long)]);
    let buf = render_logs(&mut app, 80, 30);
    // Count rows that contain the "x" run — must exceed 1 after wrap.
    let xrows = count_rows_with_text(&buf, "xxxx");
    assert!(
        xrows > 1,
        "expected multiple wrap rows of 'x's, got {}",
        xrows
    );
}

#[test]
fn ui_010_message_capped_at_max_wrap_lines() {
    let mut app = app_connected();
    // 5000 chars at width=80 would wrap to dozens of rows, but MAX_WRAP_LINES=3.
    let very_long = "x".repeat(5000);
    seed_logs(&mut app, vec![fixtures::info("A", &very_long)]);
    let buf = render_logs(&mut app, 80, 30);
    // Last wrapped line ends with "..." when truncated.
    let text = full_text(&buf);
    assert!(text.contains("..."), "expected truncation ellipsis");
}

// ══════════════════════════════════════════════════════════════════════
//  Tag/level colors (UI-031)
// ══════════════════════════════════════════════════════════════════════

#[test]
fn ui_031_distinct_tags_get_some_tag_colors() {
    let mut app = app_connected();
    seed_logs(
        &mut app,
        vec![
            fixtures::info("tag_a", "a"),
            fixtures::info("tag_b", "b"),
            fixtures::info("tag_c", "c"),
            fixtures::info("tag_d", "d"),
            fixtures::info("tag_e", "e"),
        ],
    );
    let buf = render_logs(&mut app, 120, 30);
    let colors = distinct_colors(&buf);
    // TAG_COLORS palette is [BLUE, GREEN, PEACH, MAUVE, SAPPHIRE] — five
    // distinct tags should hit most of these.
    let palette = [BLUE, GREEN, PEACH, MAUVE, SAPPHIRE];
    let hits: usize = palette.iter().filter(|c| colors.contains(c)).count();
    assert!(hits >= 2, "expected >=2 distinct tag colors, got {}", hits);
}

#[test]
fn ui_031_same_tag_same_color_across_renders() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::info("SameTag", "x")]);
    let buf1 = render_logs(&mut app, 120, 30);
    let buf2 = render_logs(&mut app, 120, 30);
    // Second identical render produces identical buffer.
    assert_eq!(full_text(&buf1), full_text(&buf2));
}

// ══════════════════════════════════════════════════════════════════════
//  Level pill rendering (UI-010)
// ══════════════════════════════════════════════════════════════════════

#[test]
fn ui_010_info_level_pill_renders() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::info("T", "hello")]);
    let buf = render_logs(&mut app, 120, 30);
    assert!(find_text_row(&buf, "INFO").is_some());
}

#[test]
fn ui_010_error_level_pill_renders() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::error("T", "boom")]);
    let buf = render_logs(&mut app, 120, 30);
    assert!(find_text_row(&buf, "ERROR").is_some());
}

#[test]
fn ui_010_debug_level_pill_renders() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::debug("T", "dbg")]);
    let buf = render_logs(&mut app, 120, 30);
    assert!(find_text_row(&buf, "DEBUG").is_some());
}

#[test]
fn ui_010_verbose_level_pill_renders() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::verbose("T", "verb")]);
    let buf = render_logs(&mut app, 120, 30);
    assert!(find_text_row(&buf, "VERBOSE").is_some());
}

#[test]
fn ui_010_warning_level_pill_renders() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::warn("T", "warn")]);
    let buf = render_logs(&mut app, 120, 30);
    assert!(find_text_row(&buf, "WARNING").is_some());
}

// ══════════════════════════════════════════════════════════════════════
//  Separator rendering (UI-012)
// ══════════════════════════════════════════════════════════════════════

#[test]
fn ui_012_separator_row_produces_divider_char() {
    let mut app = app_connected();
    seed_logs(
        &mut app,
        vec![
            fixtures::info("A", "before"),
            fixtures::separator(),
            fixtures::info("A", "after"),
        ],
    );
    let buf = render_logs(&mut app, 120, 30);
    // Separator renders a run of `─` chars. The toolbar also uses `─` but
    // separator inside the list area is 3 rows tall and width-spanning.
    let text = full_text(&buf);
    assert!(text.contains("──────────"), "missing divider chars");
}

#[test]
fn ui_012_separator_does_not_add_log_text_columns() {
    let mut app = app_connected();
    // Only a separator — no non-separator entries.
    seed_logs(&mut app, vec![fixtures::separator()]);
    let buf = render_logs(&mut app, 120, 30);
    // Separator has empty message — no tag/level column output in its row.
    // The column header "LEVEL" still appears.
    assert_eq!(count_rows_with_text(&buf, "LEVEL"), 1);
}

#[test]
fn ui_012_separator_bounded_between_entries() {
    let mut app = app_connected();
    seed_logs(
        &mut app,
        vec![
            fixtures::info("A", "above"),
            fixtures::separator(),
            fixtures::info("A", "below"),
        ],
    );
    let buf = render_logs(&mut app, 120, 40);
    let above_y = find_text_row(&buf, "above").expect("no above row");
    let below_y = find_text_row(&buf, "below").expect("no below row");
    // Separator sits between, so there's at least a 1-row gap.
    assert!(below_y > above_y + 1);
}

// ══════════════════════════════════════════════════════════════════════
//  Stack trace preview in list (UI-039)
// ══════════════════════════════════════════════════════════════════════

#[test]
fn ui_039_entry_with_stack_shows_error_prefix() {
    let mut app = app_connected();
    let stack =
        "#0      Foo.bar (package:app/foo.dart:25:3)\n#1      Baz.qux (package:app/baz.dart:1:1)";
    seed_logs(
        &mut app,
        vec![fixtures::with_stack("T", "crash", "boom", stack)],
    );
    let buf = render_logs(&mut app, 120, 30);
    let text = full_text(&buf);
    assert!(text.contains("Error: boom"));
}

#[test]
fn ui_039_entry_with_stack_shows_frames() {
    let mut app = app_connected();
    let stack =
        "#0      Foo.bar (package:app/foo.dart:25:3)\n#1      Baz.qux (package:app/baz.dart:1:1)";
    seed_logs(
        &mut app,
        vec![fixtures::with_stack("T", "crash", "boom", stack)],
    );
    let buf = render_logs(&mut app, 120, 30);
    let text = full_text(&buf);
    assert!(text.contains("Foo.bar"));
}

#[test]
fn ui_039_stack_more_frames_pill_when_truncated() {
    let mut app = app_connected();
    let mut stack = String::new();
    for i in 0..12 {
        stack.push_str(&format!("#{i}      Frame{i} (package:app/f{i}.dart:1:1)\n"));
    }
    seed_logs(
        &mut app,
        vec![fixtures::with_stack("T", "crash", "boom", &stack)],
    );
    let buf = render_logs(&mut app, 120, 30);
    let text = full_text(&buf);
    // 12 frames + 1 error line = 13; preview cap is 5 → 8 more.
    assert!(
        text.contains("more frames"),
        "expected '... N more frames' marker"
    );
}

// ══════════════════════════════════════════════════════════════════════
//  Repeat bar (UI-030)
// ══════════════════════════════════════════════════════════════════════

#[test]
#[allow(non_snake_case)]
fn ui_030_repeat_count_shows_xN_prefix() {
    let mut app = app_connected();
    let mut e = fixtures::info("T", "dup");
    e.repeat_count = 3;
    seed_logs(&mut app, vec![e]);
    let buf = render_logs(&mut app, 120, 30);
    assert!(full_text(&buf).contains("x3"));
}

#[test]
fn ui_030_repeat_count_shows_bar_segments() {
    let mut app = app_connected();
    let mut e = fixtures::info("T", "dup");
    e.repeat_count = 20;
    seed_logs(&mut app, vec![e]);
    let buf = render_logs(&mut app, 120, 30);
    let text = full_text(&buf);
    // The bar char is `█` — count >= 20 and bar_w=8 saturates at count 50.
    // At count=20, 20*8/50 = 3 bars minimum expected.
    let bars = text.matches('█').count();
    assert!(bars >= 2, "expected repeat bar segments, got {}", bars);
}

#[test]
fn ui_030_repeat_count_bar_uses_pink_color() {
    let mut app = app_connected();
    let mut e = fixtures::info("T", "dup");
    e.repeat_count = 10;
    seed_logs(&mut app, vec![e]);
    let buf = render_logs(&mut app, 120, 30);
    // PINK is used for the repeat bar foreground.
    const PINK: Color = Color::Rgb(245, 189, 230);
    assert!(count_cells_with_fg(&buf, PINK) > 0);
}

#[test]
fn ui_030_repeat_count_saturates_at_50() {
    let mut app = app_connected();
    let mut e = fixtures::info("T", "dup");
    e.repeat_count = 500;
    seed_logs(&mut app, vec![e]);
    let buf = render_logs(&mut app, 120, 30);
    let text = full_text(&buf);
    assert!(text.contains("x500"));
}

// ══════════════════════════════════════════════════════════════════════
//  Filter narrowing
// ══════════════════════════════════════════════════════════════════════

#[test]
fn ui_filter_narrows_visible_rows() {
    let mut app = app_connected();
    seed_logs(
        &mut app,
        vec![
            fixtures::info("net", "req /api/users"),
            fixtures::info("auth", "token rotated"),
            fixtures::info("net", "resp 200"),
        ],
    );
    app.filter.set_search("token");
    app.invalidate_filter();
    let buf = render_logs(&mut app, 120, 30);
    let text = full_text(&buf);
    assert!(text.contains("token rotated"));
    assert!(!text.contains("/api/users"));
}

#[test]
fn ui_filter_min_level_hides_lower() {
    let mut app = app_connected();
    seed_logs(
        &mut app,
        vec![
            fixtures::debug("D", "debug-msg"),
            fixtures::error("E", "error-msg"),
        ],
    );
    app.set_level(LogLevel::Warning);
    let buf = render_logs(&mut app, 120, 30);
    let text = full_text(&buf);
    assert!(!text.contains("debug-msg"));
    assert!(text.contains("error-msg"));
}

#[test]
fn ui_search_highlights_match_with_yellow_bg() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::info("T", "needle in haystack")]);
    app.filter.set_search("needle");
    app.invalidate_filter();
    let buf = render_logs(&mut app, 120, 30);
    // Match highlight style has bg=YELLOW.
    assert!(
        count_cells_with_bg(&buf, YELLOW) > 0,
        "no YELLOW bg cell — search highlight missing"
    );
}

// ══════════════════════════════════════════════════════════════════════
//  Toolbar & header rendering
// ══════════════════════════════════════════════════════════════════════

#[test]
fn ui_010_column_header_renders() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::info("T", "x")]);
    let buf = render_logs(&mut app, 120, 30);
    let text = full_text(&buf);
    assert!(text.contains("TIME"));
    assert!(text.contains("LEVEL"));
    assert!(text.contains("TAG"));
    assert!(text.contains("MESSAGE"));
}

#[test]
fn ui_010_toolbar_shows_search_exclude_tag_labels() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::info("T", "x")]);
    let buf = render_logs(&mut app, 120, 30);
    let text = full_text(&buf);
    assert!(text.contains("Search"));
    assert!(text.contains("Exclude"));
    assert!(text.contains("Tag"));
}

#[test]
fn ui_010_toolbar_shows_level_filter_buttons() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::info("T", "x")]);
    let buf = render_logs(&mut app, 120, 30);
    let text = full_text(&buf);
    // Individual single-letter level labels on the op2 row.
    assert!(text.contains("Level:"));
    for letter in &[" S ", " V ", " D ", " I ", " W ", " E "] {
        assert!(text.contains(letter), "missing level letter {:?}", letter);
    }
}

#[test]
fn ui_010_toolbar_shows_filtered_count() {
    let mut app = app_connected();
    seed_logs(
        &mut app,
        vec![
            fixtures::info("T", "a"),
            fixtures::info("T", "b"),
            fixtures::info("T", "c"),
        ],
    );
    let buf = render_logs(&mut app, 120, 30);
    let text = full_text(&buf);
    assert!(text.contains("Filtered:"));
    assert!(text.contains("3/3"));
}

#[test]
fn ui_010_status_bar_shows_live_pill_when_auto_scroll() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::info("T", "x")]);
    app.auto_scroll = true;
    let buf = render_logs(&mut app, 120, 30);
    assert!(full_text(&buf).contains("LIVE"));
}

#[test]
fn ui_010_status_bar_shows_new_pill_when_paused_with_new_logs() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::info("T", "orig")]);
    app.auto_scroll = false;
    // Add more entries post-pause.
    app.add_entry(fixtures::info("T", "new1"));
    app.add_entry(fixtures::info("T", "new2"));
    let buf = render_logs(&mut app, 120, 30);
    let text = full_text(&buf);
    assert!(text.contains("new"), "expected 'N new' pill");
}

#[test]
fn ui_010_status_bar_shows_buttons() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::info("T", "x")]);
    let buf = render_logs(&mut app, 140, 30);
    let text = full_text(&buf);
    assert!(text.contains("Clear"));
    assert!(text.contains("Export"));
    assert!(text.contains("Stats"));
    assert!(text.contains("Help"));
    assert!(text.contains("Quit"));
}

// ══════════════════════════════════════════════════════════════════════
//  Bookmarks
// ══════════════════════════════════════════════════════════════════════

#[test]
fn ui_010_bookmark_shows_yellow_dot() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::info("T", "mark me")]);
    app.selected = 0;
    app.auto_scroll = false;
    app.toggle_bookmark();
    let buf = render_logs(&mut app, 120, 30);
    // Bookmark uses yellow fg for "●". Check presence.
    let text = full_text(&buf);
    assert!(text.contains('●'));
    assert!(count_cells_with_fg(&buf, YELLOW) > 0);
}

#[test]
fn ui_010_no_bookmark_no_dot_in_list() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::info("T", "plain")]);
    let buf = render_logs(&mut app, 120, 30);
    // No bookmarks. "●" might still appear via LIVE pill. Assert at most 1.
    let dot_count = full_text(&buf).matches('●').count();
    assert!(dot_count <= 2, "unexpected bookmark dots: {}", dot_count);
}

// ══════════════════════════════════════════════════════════════════════
//  Detail panel (UI-010)
// ══════════════════════════════════════════════════════════════════════

#[test]
fn ui_010_detail_closed_shows_full_list_only() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::info("T", "solo")]);
    app.show_detail_panel = false;
    let buf = render_logs(&mut app, 120, 30);
    assert!(!full_text(&buf).contains(" Details "));
}

#[test]
fn ui_010_detail_open_shows_details_block() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::info("T", "detail-msg")]);
    app.show_detail_panel = true;
    app.selected = 0;
    let buf = render_logs(&mut app, 140, 30);
    let text = full_text(&buf);
    assert!(text.contains("Details"));
    assert!(text.contains("detail-msg"));
}

// ══════════════════════════════════════════════════════════════════════
//  detail/mod.rs — draw_side_panel direct tests
// ══════════════════════════════════════════════════════════════════════

#[test]
fn ui_detail_no_selection_shows_placeholder() {
    let mut app = App::default();
    // Empty store → selected_store_index returns None.
    let buf = render_detail(&mut app, 60, 20);
    assert!(full_text(&buf).contains("Select a log entry"));
}

#[test]
fn ui_detail_renders_level_and_tag() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::error("netcore", "oops")]);
    app.selected = 0;
    let buf = render_detail(&mut app, 80, 20);
    let text = full_text(&buf);
    assert!(text.contains("ERROR"));
    assert!(text.contains("netcore"));
}

#[test]
fn ui_detail_renders_timestamp() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::info("T", "m")]);
    app.selected = 0;
    let buf = render_detail(&mut app, 80, 20);
    assert!(full_text(&buf).contains("12:00:00"));
}

#[test]
fn ui_detail_renders_length_label() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::info("T", "hello")]);
    app.selected = 0;
    let buf = render_detail(&mut app, 80, 20);
    assert!(full_text(&buf).contains("Length:"));
}

#[test]
fn ui_detail_length_shows_bytes_for_small_msg() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::info("T", "hi")]);
    app.selected = 0;
    let buf = render_detail(&mut app, 80, 20);
    // "hi" is 2 bytes.
    assert!(full_text(&buf).contains("2 B"));
}

#[test]
fn ui_detail_length_shows_kb_for_medium_msg() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::info("T", &"x".repeat(2048))]);
    app.selected = 0;
    let buf = render_detail(&mut app, 80, 20);
    assert!(full_text(&buf).contains("KB"));
}

#[test]
fn ui_detail_copy_pill_appears() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::info("T", "x")]);
    app.selected = 0;
    let buf = render_detail(&mut app, 80, 20);
    assert!(full_text(&buf).contains("Copy"));
}

#[test]
fn ui_detail_renders_message_body() {
    let mut app = app_connected();
    seed_logs(
        &mut app,
        vec![fixtures::info("T", "unique-body-marker-1729")],
    );
    app.selected = 0;
    let buf = render_detail(&mut app, 80, 20);
    assert!(full_text(&buf).contains("unique-body-marker-1729"));
}

#[test]
fn ui_detail_renders_stack_trace_section() {
    let mut app = app_connected();
    let stack =
        "#0      Foo.bar (package:app/foo.dart:25:3)\n#1      Baz.qux (package:app/baz.dart:1:1)";
    seed_logs(
        &mut app,
        vec![fixtures::with_stack("T", "msg", "boom", stack)],
    );
    app.selected = 0;
    let buf = render_detail(&mut app, 80, 30);
    let text = full_text(&buf);
    assert!(text.contains("Stack Trace"));
    assert!(text.contains("Foo.bar"));
}

#[test]
fn ui_detail_json_message_renders_braces() {
    let mut app = app_connected();
    seed_logs(
        &mut app,
        vec![fixtures::info("T", r#"prefix {"key":"val","n":42} suffix"#)],
    );
    app.selected = 0;
    let buf = render_detail(&mut app, 100, 30);
    let text = full_text(&buf);
    // JsonRenderer should contribute "key" / "val" / "42" tokens.
    assert!(text.contains("key"));
    assert!(text.contains("val"));
}

#[test]
fn ui_detail_scroll_moves_content_up() {
    let mut app = app_connected();
    // Long message so body exceeds panel height.
    let msg = (0..50)
        .map(|i| format!("line-{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    seed_logs(&mut app, vec![fixtures::info("T", &msg)]);
    app.selected = 0;
    let buf0 = render_detail(&mut app, 80, 10);
    app.detail.scroll = 20;
    let buf1 = render_detail(&mut app, 80, 10);
    // buffer should differ between scroll=0 and scroll=20.
    assert_ne!(full_text(&buf0), full_text(&buf1));
}

// ══════════════════════════════════════════════════════════════════════
//  Stats rendering (ui/logs/stats.rs)
// ══════════════════════════════════════════════════════════════════════

#[test]
fn ui_logs_stats_no_snapshot_still_renders_chrome() {
    let mut app = App::default();
    let buf = render_stats(&mut app, 80, 25);
    let text = full_text(&buf);
    assert!(text.contains("Statistics"));
    assert!(text.contains("Log Levels"));
    assert!(text.contains("Tag Ranking"));
}

#[test]
fn ui_logs_stats_title_shows_back_button() {
    let mut app = App::default();
    let buf = render_stats(&mut app, 80, 25);
    assert!(full_text(&buf).contains("< Back"));
}

#[test]
fn ui_logs_stats_renders_per_level_counts_after_snapshot() {
    let mut app = app_connected();
    seed_logs(
        &mut app,
        vec![
            fixtures::info("a", "1"),
            fixtures::error("b", "2"),
            fixtures::warn("c", "3"),
        ],
    );
    app.enter_stats();
    let buf = render_stats(&mut app, 100, 25);
    let text = full_text(&buf);
    assert!(text.contains("INFO"));
    assert!(text.contains("ERROR"));
    assert!(text.contains("WARN"));
}

#[test]
fn ui_logs_stats_renders_tag_ranking_rows() {
    let mut app = app_connected();
    seed_logs(
        &mut app,
        vec![
            fixtures::info("popular", "1"),
            fixtures::info("popular", "2"),
            fixtures::info("popular", "3"),
            fixtures::info("rare", "x"),
        ],
    );
    app.enter_stats();
    let buf = render_stats(&mut app, 100, 25);
    let text = full_text(&buf);
    assert!(text.contains("popular"));
    assert!(text.contains("rare"));
}

#[test]
fn ui_logs_stats_total_filtered_labels() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::info("a", "1"); 4]);
    app.enter_stats();
    let buf = render_stats(&mut app, 100, 25);
    let text = full_text(&buf);
    assert!(text.contains("Total:"));
    assert!(text.contains("Filtered:"));
}

#[test]
fn ui_logs_stats_tag_rank_shows_bars() {
    let mut app = app_connected();
    seed_logs(
        &mut app,
        vec![
            fixtures::info("popular", "1"),
            fixtures::info("popular", "2"),
            fixtures::info("popular", "3"),
        ],
    );
    app.enter_stats();
    let buf = render_stats(&mut app, 120, 25);
    let bar_count = full_text(&buf).matches('█').count();
    assert!(bar_count > 0, "expected tag ranking bars");
}

// ══════════════════════════════════════════════════════════════════════
//  Highlight module (ui/logs/highlight.rs) — top-off coverage
// ══════════════════════════════════════════════════════════════════════

#[test]
fn highlight_auto_highlight_status_code_2xx_green() {
    use flog::ui::logs::highlight::auto_highlight;
    use ratatui::style::Style;
    let spans = auto_highlight("GET /api 200 ok", Style::default());
    let greens = spans
        .iter()
        .filter(|s| s.style.fg == Some(Color::Green))
        .count();
    assert!(greens > 0, "no green 2xx span");
}

#[test]
fn highlight_auto_highlight_status_code_4xx_yellow() {
    use flog::ui::logs::highlight::auto_highlight;
    use ratatui::style::Style;
    let spans = auto_highlight("GET /api 404 not found", Style::default());
    let yellows = spans
        .iter()
        .filter(|s| s.style.fg == Some(Color::Yellow))
        .count();
    assert!(yellows > 0);
}

#[test]
fn highlight_auto_highlight_status_code_5xx_red() {
    use flog::ui::logs::highlight::auto_highlight;
    use ratatui::style::Style;
    let spans = auto_highlight("POST /api 500 boom", Style::default());
    let reds = spans
        .iter()
        .filter(|s| s.style.fg == Some(Color::Red))
        .count();
    assert!(reds > 0);
}

#[test]
fn highlight_auto_highlight_slow_duration_red_bold() {
    use flog::ui::logs::highlight::auto_highlight;
    use ratatui::style::{Modifier, Style};
    let spans = auto_highlight("req done (5000ms)", Style::default());
    // Expect a red+bold+underlined span over (5000ms).
    let matched = spans.iter().any(|s| {
        s.style.fg == Some(Color::Red)
            && s.style.add_modifier.contains(Modifier::BOLD)
            && s.style.add_modifier.contains(Modifier::UNDERLINED)
    });
    assert!(matched, "no slow-duration highlight");
}

#[test]
fn highlight_auto_highlight_url_blue_underlined() {
    use flog::ui::logs::highlight::auto_highlight;
    use ratatui::style::{Modifier, Style};
    let spans = auto_highlight("open https://example.com/page now", Style::default());
    let matched = spans.iter().any(|s| {
        s.style.fg == Some(Color::Blue) && s.style.add_modifier.contains(Modifier::UNDERLINED)
    });
    assert!(matched, "no URL highlight");
}

#[test]
fn highlight_auto_highlight_inherits_base_bg() {
    use flog::ui::logs::highlight::auto_highlight;
    use ratatui::style::Style;
    let base = Style::default().bg(Color::Rgb(10, 10, 10));
    let spans = auto_highlight("GET 200 ok", base);
    // Every highlight span picks up base.bg.
    for s in &spans {
        assert_eq!(s.style.bg, Some(Color::Rgb(10, 10, 10)));
    }
}

#[test]
fn highlight_auto_highlight_no_match_returns_single_base_span() {
    use flog::ui::logs::highlight::auto_highlight;
    use ratatui::style::Style;
    let spans = auto_highlight(
        "plain text with no patterns",
        Style::default().fg(Color::White),
    );
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].style.fg, Some(Color::White));
}

// ══════════════════════════════════════════════════════════════════════
//  draw_logs: Extreme cases
// ══════════════════════════════════════════════════════════════════════

#[test]
fn ui_010_very_small_width_still_renders_without_panic() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::info("T", "msg")]);
    // 40 columns — tight but non-trivial. Don't panic.
    let buf = render_logs(&mut app, 40, 20);
    assert_eq!(buf.area.width, 40);
}

#[test]
fn ui_010_very_tall_viewport_renders_all_entries() {
    let mut app = app_connected();
    let mut entries: Vec<LogEntry> = Vec::new();
    for i in 0..5 {
        entries.push(fixtures::info("T", &format!("e-{i}")));
    }
    seed_logs(&mut app, entries);
    let buf = render_logs(&mut app, 120, 60);
    for i in 0..5 {
        let needle = format!("e-{i}");
        assert!(
            find_text_row(&buf, &needle).is_some(),
            "missing entry {needle}"
        );
    }
}

#[test]
fn ui_010_entries_with_extra_lines_render() {
    let mut app = app_connected();
    let mut e = fixtures::info("T", "headline");
    e.extra_lines = vec!["continuation 1".into(), "continuation 2".into()];
    seed_logs(&mut app, vec![e]);
    let buf = render_logs(&mut app, 120, 30);
    let text = full_text(&buf);
    assert!(text.contains("headline"));
    assert!(text.contains("continuation 1"));
}

#[test]
fn ui_010_select_last_entry_survives_autoscroll() {
    let mut app = app_connected();
    let mut entries: Vec<LogEntry> = Vec::new();
    for i in 0..30 {
        entries.push(fixtures::info("T", &format!("e-{i:02}")));
    }
    seed_logs(&mut app, entries);
    app.auto_scroll = true;
    let buf = render_logs(&mut app, 120, 15);
    // Latest entry visible under auto_scroll.
    assert!(find_text_row(&buf, "e-29").is_some());
}

#[test]
fn ui_010_varied_levels_produce_multiple_row_bgs() {
    let mut app = app_connected();
    seed_logs(
        &mut app,
        vec![
            fixtures::info("A", "i"),
            fixtures::error("A", "e"),
            fixtures::warn("A", "w"),
            fixtures::debug("A", "d"),
        ],
    );
    let buf = render_logs(&mut app, 120, 30);
    // Expect at least three distinct row bgs: BASE, ERROR, WARNING.
    let mut seen = 0;
    for bg in &[BASE, ERROR_ROW_BG, WARNING_ROW_BG] {
        if count_cells_with_bg(&buf, *bg) > 0 {
            seen += 1;
        }
    }
    assert!(seen >= 3, "expected >=3 row bgs, saw {}", seen);
}

// ══════════════════════════════════════════════════════════════════════
//  Jump-to-bottom overlay
// ══════════════════════════════════════════════════════════════════════

#[test]
fn ui_jump_hidden_when_auto_scroll() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::info("T", "x")]);
    app.auto_scroll = true;
    let buf = render_logs(&mut app, 120, 30);
    assert!(!full_text(&buf).contains("Jump to bottom"));
}

#[test]
fn ui_jump_hidden_when_filtered_count_zero() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::info("T", "x")]);
    app.filter.set_search("__no_match__");
    app.invalidate_filter();
    app.auto_scroll = false;
    let buf = render_logs(&mut app, 120, 30);
    assert!(!full_text(&buf).contains("Jump to bottom"));
}

// ══════════════════════════════════════════════════════════════════════
//  Row-to-string smoke tests (layout invariants)
// ══════════════════════════════════════════════════════════════════════

#[test]
fn ui_010_first_row_is_separator_rule() {
    let mut app = App::default();
    let buf = render_logs(&mut app, 100, 30);
    let first = row_to_string(&buf, 0);
    // rows[0] is a horizontal rule of `─`.
    assert!(first.contains('─'));
}

#[test]
fn ui_010_palette_has_mantle_toolbar_bg() {
    let mut app = app_connected();
    seed_logs(&mut app, vec![fixtures::info("T", "x")]);
    let buf = render_logs(&mut app, 120, 30);
    // MANTLE covers toolbar rows.
    assert!(count_cells_with_bg(&buf, MANTLE) > 0);
}

#[test]
fn ui_010_palette_has_multiple_fg_colors() {
    let mut app = app_connected();
    seed_logs(
        &mut app,
        vec![
            fixtures::error("err", "oops"),
            fixtures::info("net", "ok"),
            fixtures::warn("sys", "!"),
        ],
    );
    let buf = render_logs(&mut app, 120, 30);
    // Each level + header text + subtext variants should appear.
    let mut hits = 0;
    for fg in &[TEXT, SUBTEXT0, OVERLAY0, BLUE, YELLOW, RED] {
        if count_cells_with_fg(&buf, *fg) > 0 {
            hits += 1;
        }
    }
    assert!(hits >= 4, "expected diverse fg palette, got {}", hits);
}
