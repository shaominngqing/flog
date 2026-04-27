//! Shared input field renderer — stateless, three-state background, scroll window.
//!
//! Active-state colors (box background, body text color, cursor glyph style)
//! are now decoupled from the shared palette. Callers build
//! [`InputFieldProps`] via [`InputFieldProps::with_default_style`] to inherit
//! the Catppuccin Macchiato defaults, or set `bg` / `fg` / `cursor_style`
//! explicitly for a custom palette (UI-015).

use ratatui::{
    style::{Color, Modifier, Style},
    text::Span,
};
use unicode_width::UnicodeWidthChar;

use super::{MANTLE, OVERLAY0, SUBTEXT0, SURFACE0, SURFACE1, TEXT, YELLOW};

/// Inputs for rendering one input field.
pub struct InputFieldProps<'a> {
    pub label: &'a str, // e.g., "Search"
    pub hint: &'a str,  // e.g., "(a|b)" shown when idle+empty
    pub value: &'a str, // full buffer
    pub active: bool,
    /// Cursor byte offset into `value` (ignored when !active).
    pub cursor_byte: usize,
    /// Total width the field may consume (label + value box + 1-char gaps).
    pub total_width: u16,
    /// Active-state box background color. Ignored when `active == false`.
    pub bg: Color,
    /// Active-state body text color. Ignored when `active == false`.
    pub fg: Color,
    /// Active-state style for the `_` cursor glyph. When this equals
    /// `Style::default().fg(fg).bg(bg)` the cursor is merged into the
    /// body span (visually identical); otherwise it renders as its own
    /// span so callers can recolor just the caret.
    pub cursor_style: Style,
}

impl<'a> InputFieldProps<'a> {
    /// Build an [`InputFieldProps`] with the shared Catppuccin Macchiato
    /// defaults for `bg`, `fg`, and `cursor_style`. Existing call sites
    /// switch to this factory; the rendered output is unchanged.
    pub fn with_default_style(
        label: &'a str,
        hint: &'a str,
        value: &'a str,
        active: bool,
        cursor_byte: usize,
        total_width: u16,
    ) -> Self {
        Self {
            label,
            hint,
            value,
            active,
            cursor_byte,
            total_width,
            bg: SURFACE1,
            fg: TEXT,
            cursor_style: Style::default().fg(TEXT).bg(SURFACE1),
        }
    }
}

/// Output from render_input_field.
pub struct RenderedInputField {
    pub spans: Vec<Span<'static>>,
    /// Click hit region (inclusive start, exclusive end) relative to caller's row.
    pub hit_x: (u16, u16),
    /// Number of columns consumed (should equal total_width when possible).
    pub used_width: u16,
}

/// Compute the substring of `value` that fits in `box_width` columns, keeping `cursor_byte` visible.
///
/// Returns (display_text, ellipsis_left, ellipsis_right).
/// - When `active = false`, always show from the start (with trailing ellipsis if needed).
/// - When `active = true`, slide the window so the cursor position is visible,
///   with 1 column of right padding for the blinking "_".
pub fn visible_window(
    value: &str,
    cursor_byte: usize,
    box_width: usize,
    active: bool,
) -> (String, bool, bool) {
    if box_width == 0 {
        return (String::new(), false, false);
    }

    // Total display width of value
    let total: usize = value.chars().map(|c| c.width().unwrap_or(0)).sum();

    if total <= box_width {
        return (value.to_string(), false, false);
    }

    if !active {
        // Head + ellipsis suffix: take chars until box_width-1 cols, embed '…'
        let mut out = String::new();
        let mut used = 0usize;
        for ch in value.chars() {
            let w = ch.width().unwrap_or(0);
            if used + w > box_width.saturating_sub(1) {
                break;
            }
            out.push(ch);
            used += w;
        }
        out.push('…'); // embed the ellipsis so the caller gets a ready-to-paint string
        return (out, false, true);
    }

    // Active: slide window to keep cursor visible.
    // Cursor column = width of value[..cursor_byte] (+1 for trailing '_').
    let prefix_width: usize = value[..cursor_byte.min(value.len())]
        .chars()
        .map(|c| c.width().unwrap_or(0))
        .sum();
    let right_edge = prefix_width + 1; // one column reserved for '_'
    let left_edge = right_edge.saturating_sub(box_width);

    let mut out = String::new();
    let mut col = 0usize;
    let mut started = false;
    let ellipsis_left = left_edge > 0;
    for ch in value.chars() {
        let w = ch.width().unwrap_or(0);
        let ch_start = col;
        let ch_end = col + w;
        col = ch_end;
        if ch_end <= left_edge {
            continue;
        }
        if ch_start >= right_edge {
            break;
        }
        if ellipsis_left && !started {
            out.push('…');
            started = true;
            // Ellipsis just consumed 1 col at left_edge; drop the partial char that straddled it.
            if ch_start < left_edge {
                continue;
            }
        }
        // Width budget check: will this char overflow box_width?
        let consumed_so_far: usize = out.chars().map(|c| c.width().unwrap_or(0)).sum();
        if consumed_so_far + w > box_width {
            break;
        }
        out.push(ch);
    }
    let ellipsis_right = right_edge < total;
    (out, ellipsis_left, ellipsis_right)
}

/// Render an input field. Layout (spans, left-to-right):
///   " LABEL: "  VALUE_BOX (box_width cols)
///
/// Three-state backgrounds:
///   idle + empty    → box bg SURFACE0 dim, hint shown
///   idle + has text → box bg SURFACE0, text YELLOW
///   active          → box bg SURFACE1, text TEXT + blinking '_' cursor
pub fn render_input_field(props: InputFieldProps<'_>, x_offset: u16) -> RenderedInputField {
    use unicode_width::UnicodeWidthStr;

    // Label segment: " Label: "
    let label_text = format!(" {}: ", props.label);
    let label_w = label_text.width() as u16;

    let box_width = props.total_width.saturating_sub(label_w) as usize;
    let box_width = box_width.max(4); // minimum usable

    // Guard: if total_width is too small to fit label + min box, emit a degraded single-char box.
    if (label_w as usize) + box_width > props.total_width as usize {
        let out_bg = if props.active { SURFACE1 } else { SURFACE0 };
        let width = props.total_width as usize;
        let mut body = String::new();
        for _ in 0..width {
            body.push(' ');
        }
        if width >= 1 {
            body.replace_range(0..1, "…");
        }
        return RenderedInputField {
            spans: vec![Span::styled(body, Style::default().fg(OVERLAY0).bg(out_bg))],
            hit_x: (x_offset, x_offset + props.total_width),
            used_width: props.total_width,
        };
    }

    let has_text = !props.value.is_empty();
    let (label_style, box_bg, text_fg) = if props.active {
        (
            Style::default()
                .fg(YELLOW)
                .bg(MANTLE)
                .add_modifier(Modifier::BOLD),
            props.bg,
            props.fg,
        )
    } else if has_text {
        (Style::default().fg(SUBTEXT0).bg(MANTLE), SURFACE0, YELLOW)
    } else {
        (Style::default().fg(OVERLAY0).bg(MANTLE), SURFACE0, OVERLAY0)
    };

    let mut spans = Vec::new();
    spans.push(Span::styled(label_text, label_style));

    // Body: hint (idle+empty) OR scrolled value
    let inner_w = if props.active {
        box_width.saturating_sub(1) // reserve 1 col for '_'
    } else {
        box_width
    };

    let (body_text, _el_l, _el_r) = if !has_text && !props.active {
        (props.hint.to_string(), false, false)
    } else {
        visible_window(props.value, props.cursor_byte, inner_w, props.active)
    };

    let body_style = Style::default().fg(text_fg).bg(box_bg);

    if props.active {
        // Active: value + cursor + padding. When cursor_style matches body
        // style, emit a single merged span (unchanged render output); when
        // the caller customises cursor_style, split into separate spans so
        // only the caret picks up the override.
        let body_w = body_text.width();
        let cursor_w = 1usize; // '_' is 1 col
        let pad_w = box_width.saturating_sub(body_w + cursor_w);

        if props.cursor_style == body_style {
            let mut merged = body_text;
            merged.push('_');
            if pad_w > 0 {
                merged.push_str(&" ".repeat(pad_w));
            }
            spans.push(Span::styled(merged, body_style));
        } else {
            if !body_text.is_empty() {
                spans.push(Span::styled(body_text, body_style));
            }
            spans.push(Span::styled("_", props.cursor_style));
            if pad_w > 0 {
                spans.push(Span::styled(" ".repeat(pad_w), body_style));
            }
        }
    } else {
        // Idle: body + padding as a single span (cursor_style unused).
        let mut body = body_text;
        let body_w = body.width();
        if body_w < box_width {
            body.push_str(&" ".repeat(box_width - body_w));
        }
        spans.push(Span::styled(body, body_style));
    }

    RenderedInputField {
        spans,
        hit_x: (x_offset, x_offset + label_w + box_width as u16),
        used_width: label_w + box_width as u16,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
