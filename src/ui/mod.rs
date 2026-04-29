//! TUI rendering — top-level dispatcher.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;

pub mod device_picker;
pub mod help;
pub mod input_field;
pub mod json_viewer;
pub mod logs;
pub mod network;
mod tab_bar;
pub mod text_editor;

use crate::app::{App, ViewTab};

// ══════════════════════════════════════
//  Shared Catppuccin Macchiato Palette
// ══════════════════════════════════════

pub const BASE: Color = Color::Rgb(36, 39, 58); // #24273a — main bg
pub const MANTLE: Color = Color::Rgb(30, 32, 48); // #1e2030 — panels/alt bg
pub const SURFACE0: Color = Color::Rgb(54, 58, 79); // #363a4f — subtle borders
pub const SURFACE1: Color = Color::Rgb(73, 77, 100); // #494d64 — active borders
pub const OVERLAY0: Color = Color::Rgb(110, 115, 141); // #6e738d — muted text
pub const TEXT: Color = Color::Rgb(202, 211, 245); // #cad3f5 — main text
pub const SUBTEXT0: Color = Color::Rgb(165, 173, 206); // #a5adce — secondary text

pub const BLUE: Color = Color::Rgb(138, 173, 244); // #8aadf4 — accent
pub const SAPPHIRE: Color = Color::Rgb(125, 196, 228); // #7dc4e4 — links
pub const TEAL: Color = Color::Rgb(139, 213, 202); // #8bd5ca — values
pub const GREEN: Color = Color::Rgb(166, 218, 149); // #a6da95 — success
pub const YELLOW: Color = Color::Rgb(238, 212, 159); // #eed49f — warning
pub const PEACH: Color = Color::Rgb(245, 169, 127); // #f5a97f — info emphasis
pub const RED: Color = Color::Rgb(237, 135, 150); // #ed8796 — error
pub const MAUVE: Color = Color::Rgb(198, 160, 246); // #c6a0f6 — key/label
pub const PINK: Color = Color::Rgb(245, 189, 230); // #f5bde6 — special
pub const LAVENDER: Color = Color::Rgb(183, 189, 248); // #b7bdf8 — subtle hl

// ══════════════════════════════════════
//  Offline chip (shared by both status bars)
// ══════════════════════════════════════

/// Render the `OFFLINE` chip shown in the status bar when no app is
/// attached. Returns (spans, width_cells) so callers can place it in
/// their layout arithmetic without re-measuring.
///
/// This is the only "no active connection" chip flog emits. When an
/// app is attached, both status bars fall through to their existing
/// LIVE/`% progress`/`N new` rendering — untouched by this helper.
pub(crate) fn offline_chip() -> (Vec<Span<'static>>, u16) {
    use ratatui::style::Modifier;
    let text = " OFFLINE ";
    let span = Span::styled(
        text,
        Style::default()
            .fg(SUBTEXT0)
            .bg(SURFACE0)
            .add_modifier(Modifier::BOLD),
    );
    (vec![span], text.width() as u16)
}

/// Render the "⇅ <discovered devices>" hint shown next to the OFFLINE
/// chip. Caller places it adjacent to the chip and marks the whole left
/// block (chip + hint) as a single click zone routing to the device
/// picker — callers don't need per-region x-ranges from this helper.
///
/// Text convention (matches existing `1 device` / `N devices` /
/// `No devices found` casings in the codebase):
/// - `0` → ` ⇅ No devices `
/// - `1` → ` ⇅ 1 device `
/// - `N` → ` ⇅ N devices `
pub(crate) fn offline_devices_hint(device_count: usize, bg: Color) -> (Vec<Span<'static>>, u16) {
    let label = match device_count {
        0 => " ⇅ No devices ".to_string(),
        1 => " ⇅ 1 device ".to_string(),
        n => format!(" ⇅ {} devices ", n),
    };
    let w = label.width() as u16;
    let span = Span::styled(label, Style::default().fg(SUBTEXT0).bg(bg));
    (vec![span], w)
}

// ══════════════════════════════════════
//  Shared Utility Functions
// ══════════════════════════════════════

/// Wrap text into lines of at most `max_w` display-width characters.
/// Returns at most `max_lines` lines. The last line is truncated with "..." if needed.
pub fn wrap_text(s: &str, max_w: usize, max_lines: usize) -> Vec<String> {
    use unicode_width::UnicodeWidthChar;
    if max_w == 0 || max_lines == 0 {
        return vec![String::new()];
    }

    let mut result: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_w: usize = 0;

    for ch in s.chars() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
        if current_w + cw > max_w {
            result.push(current);
            current = String::new();
            current_w = 0;

            if result.len() >= max_lines {
                // Truncate last line with "..."
                if let Some(last) = result.last_mut() {
                    let trunc_w = max_w.saturating_sub(3);
                    let mut trimmed = String::new();
                    let mut tw = 0;
                    for tc in last.chars() {
                        let tcw = UnicodeWidthChar::width(tc).unwrap_or(0);
                        if tw + tcw > trunc_w {
                            break;
                        }
                        trimmed.push(tc);
                        tw += tcw;
                    }
                    trimmed.push_str("...");
                    *last = trimmed;
                }
                return result;
            }
        }
        current.push(ch);
        current_w += cw;
    }
    if !current.is_empty() || result.is_empty() {
        result.push(current);
    }
    result
}

/// Wrap text that may contain literal `\n` into display lines.
///
/// Each `\n` starts a new visual line (so a Dart stack trace keeps one frame per row);
/// within a segment, wrapping is done by display width via [`wrap_text`]. The overall
/// line count is capped at `max_lines`, with the last line truncated using `...`.
pub fn wrap_multiline(s: &str, max_w: usize, max_lines: usize) -> Vec<String> {
    if max_w == 0 || max_lines == 0 {
        return vec![String::new()];
    }
    let mut out: Vec<String> = Vec::new();
    for (i, segment) in s.split('\n').enumerate() {
        if i > 0 && out.is_empty() {
            // preserve a leading blank line if the input starts with '\n'
            out.push(String::new());
        }
        let remaining = max_lines.saturating_sub(out.len());
        if remaining == 0 {
            break;
        }
        if segment.is_empty() {
            out.push(String::new());
            continue;
        }
        for wl in wrap_text(segment, max_w, remaining) {
            out.push(wl);
            if out.len() >= max_lines {
                break;
            }
        }
        if out.len() >= max_lines {
            break;
        }
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

pub fn safe_truncate(s: &str, max_w: usize) -> String {
    if max_w == 0 {
        return String::new();
    }
    if s.width() <= max_w {
        return s.to_string();
    }
    let t = max_w.saturating_sub(3);
    let mut r = String::new();
    let mut w = 0;
    for c in s.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if w + cw > t {
            break;
        }
        r.push(c);
        w += cw;
    }
    r.push_str("...");
    r
}

pub fn draw_separator_rule(f: &mut Frame, area: Rect) {
    let rule: String = "─".repeat(area.width as usize);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            rule,
            Style::default().fg(SURFACE0).bg(MANTLE),
        )))
        .style(Style::default().bg(MANTLE)),
        area,
    );
}

pub fn safe_pad(s: &str, width: usize) -> String {
    let w = s.width();
    if w >= width {
        let mut r = String::new();
        let mut cw = 0;
        for c in s.chars() {
            let ch_w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
            if cw + ch_w > width {
                break;
            }
            r.push(c);
            cw += ch_w;
        }
        while cw < width {
            r.push(' ');
            cw += 1;
        }
        r
    } else {
        let mut r = s.to_string();
        for _ in 0..(width - w) {
            r.push(' ');
        }
        r
    }
}

// ══════════════════════════════════════
//  Main Draw — Top-Level Dispatcher
// ══════════════════════════════════════

pub fn draw(f: &mut Frame, app: &mut App) {
    app.tick += 1;

    let full = f.area();
    f.render_widget(Block::default().style(Style::default().bg(BASE)), full);

    // Layout: tab bar (2 rows) + view content
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // tab bar
            Constraint::Min(3),    // view content
        ])
        .split(full);

    app.layout.tab_bar_y = rows[0].y;
    app.layout.width = full.width;

    tab_bar::draw_tab_bar(f, app, rows[0]);

    match app.active_tab {
        ViewTab::Logs => logs::draw_logs(f, app, rows[1]),
        ViewTab::Network => network::draw_network(f, app, rows[1]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_multiline_splits_on_newline() {
        let s = "first line\n#0 frame a\n#1 frame b";
        let out = wrap_multiline(s, 80, 50);
        assert_eq!(out, vec!["first line", "#0 frame a", "#1 frame b"]);
    }

    #[test]
    fn wrap_multiline_wraps_inside_long_segment() {
        let s = "short\naaaaaaaaaaaaaaaaaaaa";
        let out = wrap_multiline(s, 10, 50);
        assert_eq!(out, vec!["short", "aaaaaaaaaa", "aaaaaaaaaa"]);
    }

    #[test]
    fn wrap_multiline_preserves_blank_lines() {
        let s = "a\n\nb";
        let out = wrap_multiline(s, 10, 50);
        assert_eq!(out, vec!["a", "", "b"]);
    }

    #[test]
    fn wrap_multiline_respects_line_cap() {
        let s = "a\nb\nc\nd\ne";
        let out = wrap_multiline(s, 10, 3);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0], "a");
    }

    // ── Phase 2.5B Task 10b additions ────────────────────────────────
    //
    // Cover the shared helpers that power toolbar / detail rendering.

    #[test]
    fn safe_pad_shorter_than_width_pads_spaces() {
        let r = safe_pad("hi", 5);
        assert_eq!(r, "hi   ");
        assert_eq!(r.len(), 5);
    }

    #[test]
    fn safe_pad_exactly_width_returns_unchanged() {
        let r = safe_pad("hello", 5);
        assert_eq!(r, "hello");
    }

    #[test]
    fn safe_pad_longer_than_width_truncates_no_ellipsis() {
        let r = safe_pad("abcdefgh", 5);
        // safe_pad truncates to fit exactly `width` cells (no ellipsis).
        assert_eq!(r.width(), 5);
        assert!("abcdefgh".starts_with(&r));
    }

    #[test]
    fn safe_pad_cjk_respects_display_width() {
        // "中" is width 2
        let r = safe_pad("中", 5);
        assert_eq!(r.width(), 5);
        assert!(r.starts_with('中'));
    }

    #[test]
    fn safe_pad_cjk_truncates_by_display_width() {
        // "中" width=2, so safe_pad with width=1 truncates the wide char and pads with spaces.
        let r = safe_pad("中a", 1);
        assert_eq!(r.width(), 1);
    }

    #[test]
    fn safe_truncate_short_enough_returns_unchanged() {
        let r = safe_truncate("hello", 10);
        assert_eq!(r, "hello");
    }

    #[test]
    fn safe_truncate_exactly_fits() {
        let r = safe_truncate("hello", 5);
        assert_eq!(r, "hello");
    }

    #[test]
    fn safe_truncate_long_cuts_and_ellipses() {
        let r = safe_truncate("abcdefghij", 7);
        // max_w - 3 = 4 chars + "..."
        assert_eq!(r, "abcd...");
    }

    #[test]
    fn safe_truncate_zero_width_returns_empty() {
        let r = safe_truncate("abc", 0);
        assert_eq!(r, "");
    }

    #[test]
    fn safe_truncate_cjk_width_aware() {
        // "中中中" total width 6; with max_w=5, trunc_w=2 → keep "中" (width 2) + "..."
        let r = safe_truncate("中中中", 5);
        assert!(r.ends_with("..."));
        assert!(r.starts_with('中'));
        // Width is "中" (2) + "..." (3) = 5.
        assert!(r.width() <= 5);
    }

    #[test]
    fn wrap_text_zero_width_returns_empty_placeholder() {
        let out = wrap_text("anything", 0, 3);
        assert_eq!(out, vec![""]);
    }

    #[test]
    fn wrap_text_zero_max_lines_returns_empty_placeholder() {
        let out = wrap_text("anything", 10, 0);
        assert_eq!(out, vec![""]);
    }

    #[test]
    fn wrap_text_exactly_width_one_line() {
        let out = wrap_text("abcde", 5, 5);
        assert_eq!(out, vec!["abcde"]);
    }

    #[test]
    fn wrap_text_splits_on_width_boundary() {
        let out = wrap_text("abcdefghij", 5, 5);
        assert_eq!(out, vec!["abcde", "fghij"]);
    }

    #[test]
    fn wrap_text_max_lines_cap_with_ellipsis() {
        let out = wrap_text("aaaaaaaaaaaaaaaaaaaa", 5, 2);
        // Hit cap of 2 lines then truncate last with "..."
        assert_eq!(out.len(), 2);
        assert!(out[1].ends_with("..."));
    }

    #[test]
    fn wrap_text_empty_string_yields_one_empty_line() {
        let out = wrap_text("", 10, 5);
        assert_eq!(out, vec![""]);
    }

    #[test]
    fn wrap_multiline_zero_width_returns_empty() {
        let out = wrap_multiline("abc\ndef", 0, 5);
        assert_eq!(out, vec![""]);
    }

    #[test]
    fn wrap_multiline_zero_lines_returns_empty() {
        let out = wrap_multiline("abc\ndef", 10, 0);
        assert_eq!(out, vec![""]);
    }

    #[test]
    fn wrap_multiline_leading_newline_preserves_blank() {
        let out = wrap_multiline("\nsecond", 10, 5);
        assert_eq!(out, vec!["", "second"]);
    }

    #[test]
    fn draw_separator_rule_renders_horizontal_line() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let backend = TestBackend::new(10, 1);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            let area = Rect::new(0, 0, 10, 1);
            draw_separator_rule(f, area);
        })
        .unwrap();
        let buf = term.backend().buffer().clone();
        let row: String = (0..10).map(|x| buf[(x, 0)].symbol().to_string()).collect();
        assert!(
            row.contains('─'),
            "expected separator rule char '─' in: {:?}",
            row
        );
    }

    #[test]
    fn draw_top_level_dispatches_logs_tab() {
        use crate::app::{App, ViewTab};
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let mut app = App::new();
        app.active_tab = ViewTab::Logs;
        let backend = TestBackend::new(80, 12);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| draw(f, &mut app)).unwrap();
        // Tick incremented by draw.
        assert!(app.tick >= 1);
        assert_eq!(app.layout.width, 80);
    }

    #[test]
    fn draw_top_level_dispatches_network_tab() {
        use crate::app::{App, ViewTab};
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let mut app = App::new();
        app.active_tab = ViewTab::Network;
        let backend = TestBackend::new(80, 12);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| draw(f, &mut app)).unwrap();
        assert_eq!(app.layout.tab_bar_y, 0);
    }
}
