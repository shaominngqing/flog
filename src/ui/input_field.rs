//! Shared input field renderer — stateless, three-state background, scroll window.

use ratatui::{
    style::{Modifier, Style},
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
            SURFACE1,
            TEXT,
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

    let mut body = body_text;
    if props.active {
        body.push('_');
    }
    // Pad to box_width
    let body_w = body.width();
    if body_w < box_width {
        body.push_str(&" ".repeat(box_width - body_w));
    }

    spans.push(Span::styled(body, Style::default().fg(text_fg).bg(box_bg)));

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
            InputFieldProps {
                label: "Search",
                hint: "(a|b)",
                value: "",
                active: false,
                cursor_byte: 0,
                total_width: 5,
            },
            0,
        );
        assert_eq!(r.used_width, 5);
        assert_eq!(r.hit_x, (0, 5));
    }

    #[test]
    fn render_input_field_hit_x_covers_label() {
        let r = render_input_field(
            InputFieldProps {
                label: "Search",
                hint: "(a|b)",
                value: "",
                active: false,
                cursor_byte: 0,
                total_width: 30,
            },
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
}
