//! Unit tests for the input field renderer + visible window helper.
//!
//! Kept in a dedicated module file to keep `mod.rs` within the 500-line
//! red-line budget.

use ratatui::style::Style;

use super::super::{SURFACE0, SURFACE1, TEXT, YELLOW};
use super::{render_input_field, visible_window, InputFieldProps};

#[test]
fn visible_window_short_fits() {
    let (out, l, r) = visible_window("hello", 5, 10, false);
    assert_eq!(out, "hello");
    assert!(!l && !r);
}

#[test]
fn visible_window_idle_truncates_tail() {
    let (out, l, r) = visible_window("abcdefghij", 0, 5, false);
    // box_width=5 → keep 4 chars + '…' embedded, total 5 cols
    assert_eq!(out, "abcd…");
    assert!(!l);
    assert!(r);
}

#[test]
fn visible_window_active_wide_char_no_overflow() {
    // "ab中de" where 中 has width 2. cursor at end (byte 7, width 6).
    // box_width=3, active=true → right_edge=7, left_edge=4
    let (out, _l, _r) = visible_window("ab中de", 7, 3, true);
    // Output must be <= 3 columns wide
    let out_w: usize = out
        .chars()
        .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0))
        .sum();
    assert!(out_w <= 3, "got '{}' width {}", out, out_w);
}

#[test]
fn render_input_field_narrow_fallback() {
    let r = render_input_field(
        InputFieldProps::with_default_style("Search", "(a|b)", "", false, 0, 5),
        0,
    );
    assert_eq!(r.used_width, 5);
    assert_eq!(r.hit_x, (0, 5));
}

#[test]
fn render_input_field_hit_x_covers_label() {
    let r = render_input_field(
        InputFieldProps::with_default_style("Search", "(a|b)", "", false, 0, 30),
        5,
    );
    // hit_x should start at x_offset (5), not at x_offset+label_w
    assert_eq!(r.hit_x.0, 5);
    assert_eq!(r.hit_x.1, 5 + r.used_width);
}

#[test]
fn visible_window_active_keeps_cursor_visible() {
    // value length 10, box=5, cursor at end (byte 10)
    let (out, l, r) = visible_window("abcdefghij", 10, 5, true);
    // Window right-edge = 11, left-edge = 6 → show fghij prefixed by '…'
    // With 1 col for ellipsis, 4 chars fit.
    assert!(out.starts_with('…'));
    assert!(out.ends_with('j'));
    assert!(l);
    assert!(!r);
}

#[test]
fn visible_window_zero_box() {
    let (out, _, _) = visible_window("abc", 0, 0, false);
    assert_eq!(out, "");
}

// ── Phase 2.5B Task 10b additions ────────────────────────────────
//
// Fill remaining uncovered branches: idle+empty hint rendering, active+
// empty state, cursor at start, cursor mid-string slide window, exact-fit
// box, CJK wide char slide.

#[test]
fn visible_window_idle_exact_fit_no_ellipsis() {
    // total == box_width → returns unchanged.
    let (out, l, r) = visible_window("abcde", 3, 5, false);
    assert_eq!(out, "abcde");
    assert!(!l && !r);
}

#[test]
fn visible_window_active_cursor_at_start_no_left_ellipsis() {
    // cursor at byte 0, short string fits → returns whole string
    let (out, l, r) = visible_window("abc", 0, 5, true);
    assert_eq!(out, "abc");
    assert!(!l && !r);
}

#[test]
fn visible_window_active_cursor_in_middle() {
    // 20 chars, box=5, cursor at byte 10 → slide so cursor visible
    let value = "abcdefghijklmnopqrst";
    let (out, _l, _r) = visible_window(value, 10, 5, true);
    let out_w: usize = out
        .chars()
        .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0))
        .sum();
    assert!(
        out_w <= 5,
        "window exceeded box_width: {:?} w={}",
        out,
        out_w
    );
}

#[test]
fn render_input_field_idle_empty_shows_hint() {
    let r = render_input_field(
        InputFieldProps::with_default_style("Search", "regex", "", false, 0, 30),
        0,
    );
    // spans: [label, body]
    assert_eq!(r.spans.len(), 2);
    let body = &r.spans[1].content;
    assert!(
        body.contains("regex"),
        "body should contain hint: {:?}",
        body
    );
}

#[test]
fn render_input_field_idle_with_text_yellow_fg() {
    let r = render_input_field(
        InputFieldProps::with_default_style("Search", "regex", "hello", false, 0, 30),
        0,
    );
    // Body span fg should be YELLOW when has_text && !active.
    let body_span = &r.spans[1];
    assert_eq!(body_span.style.fg, Some(YELLOW));
    assert!(body_span.content.contains("hello"));
}

#[test]
fn render_input_field_active_shows_underscore_cursor() {
    let r = render_input_field(
        InputFieldProps::with_default_style("Search", "regex", "abc", true, 3, 30),
        0,
    );
    let body = &r.spans[1].content;
    assert!(
        body.contains('_'),
        "active body should have cursor '_': {:?}",
        body
    );
    assert!(body.contains("abc"));
    // fg=TEXT when active.
    assert_eq!(r.spans[1].style.fg, Some(TEXT));
}

#[test]
fn render_input_field_active_empty_shows_only_cursor() {
    let r = render_input_field(
        InputFieldProps::with_default_style("S", "h", "", true, 0, 20),
        0,
    );
    let body = &r.spans[1].content;
    assert!(
        body.starts_with('_'),
        "active empty body starts with '_': {:?}",
        body
    );
}

#[test]
fn render_input_field_used_width_consistent_with_hit_x() {
    let x = 12u16;
    let r = render_input_field(
        InputFieldProps::with_default_style("Tag", "pat", "", false, 0, 40),
        x,
    );
    assert_eq!(r.hit_x, (x, x + r.used_width));
}

#[test]
fn render_input_field_long_text_truncates_with_ellipsis_when_idle() {
    let r = render_input_field(
        InputFieldProps::with_default_style(
            "S",
            "",
            "abcdefghijklmnopqrstuvwxyz",
            false,
            0,
            12, // label=" S: " (4) + box=8
        ),
        0,
    );
    let body = &r.spans[1].content;
    assert!(
        body.contains('…'),
        "idle long text should have ellipsis: {:?}",
        body
    );
}

#[test]
fn render_input_field_bg_active_is_surface1() {
    let r = render_input_field(
        InputFieldProps::with_default_style("S", "", "x", true, 1, 20),
        0,
    );
    assert_eq!(r.spans[1].style.bg, Some(SURFACE1));
}

#[test]
fn render_input_field_bg_idle_is_surface0() {
    let r = render_input_field(
        InputFieldProps::with_default_style("S", "", "x", false, 0, 20),
        0,
    );
    assert_eq!(r.spans[1].style.bg, Some(SURFACE0));
}

// ── UI-015 (Phase 3 Step 3.9 Task 6) ──────────────────────────────
//
// Custom active-state palette: caller overrides bg / fg / cursor_style
// so the rendered cells pick up the override (not the Catppuccin
// defaults baked into `with_default_style`). When cursor_style differs
// from the body style, the cursor is emitted as its own span so only
// the caret is restyled.

#[test]
fn render_input_field_custom_palette_propagates_to_cells() {
    use ratatui::style::Color;

    let custom_bg = Color::Rgb(10, 20, 30);
    let custom_fg = Color::Rgb(200, 210, 220);
    let cursor_fg = Color::Rgb(250, 100, 100); // distinct from body fg
    let cursor_bg = custom_bg;
    let mut props = InputFieldProps::with_default_style("S", "", "abc", true, 3, 30);
    props.bg = custom_bg;
    props.fg = custom_fg;
    props.cursor_style = Style::default().fg(cursor_fg).bg(cursor_bg);

    let r = render_input_field(props, 0);

    // Body span: custom bg + fg.
    let body_span = &r.spans[1];
    assert_eq!(body_span.style.bg, Some(custom_bg));
    assert_eq!(body_span.style.fg, Some(custom_fg));
    assert!(body_span.content.contains("abc"));

    // Cursor span (separate because cursor_style differs from body).
    let cursor_span = r
        .spans
        .iter()
        .find(|s| s.content == "_")
        .expect("cursor span should exist when cursor_style != body style");
    assert_eq!(cursor_span.style.fg, Some(cursor_fg));
    assert_eq!(cursor_span.style.bg, Some(cursor_bg));
}
