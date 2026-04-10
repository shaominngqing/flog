//! TUI rendering — top-level dispatcher.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::Block,
};
use unicode_width::UnicodeWidthStr;

pub mod help;
pub mod logs;
pub mod network;
pub mod source_select;
mod tab_bar;

use crate::app::{App, AppMode, ViewTab};

// ══════════════════════════════════════
//  Shared Catppuccin Macchiato Palette
// ══════════════════════════════════════

pub const BASE: Color      = Color::Rgb(36, 39, 58);    // #24273a — main bg
pub const MANTLE: Color    = Color::Rgb(30, 32, 48);    // #1e2030 — panels/alt bg
pub const SURFACE0: Color  = Color::Rgb(54, 58, 79);    // #363a4f — subtle borders
pub const SURFACE1: Color  = Color::Rgb(73, 77, 100);   // #494d64 — active borders
pub const OVERLAY0: Color  = Color::Rgb(110, 115, 141);  // #6e738d — muted text
pub const TEXT: Color       = Color::Rgb(202, 211, 245);  // #cad3f5 — main text
pub const SUBTEXT0: Color  = Color::Rgb(165, 173, 206);  // #a5adce — secondary text

pub const BLUE: Color      = Color::Rgb(138, 173, 244);  // #8aadf4 — accent
pub const SAPPHIRE: Color  = Color::Rgb(125, 196, 228);  // #7dc4e4 — links
pub const TEAL: Color      = Color::Rgb(139, 213, 202);  // #8bd5ca — values
pub const GREEN: Color     = Color::Rgb(166, 218, 149);  // #a6da95 — success
pub const YELLOW: Color    = Color::Rgb(238, 212, 159);  // #eed49f — warning
pub const PEACH: Color     = Color::Rgb(245, 169, 127);  // #f5a97f — info emphasis
pub const RED: Color       = Color::Rgb(237, 135, 150);  // #ed8796 — error
pub const MAUVE: Color     = Color::Rgb(198, 160, 246);  // #c6a0f6 — key/label
pub const PINK: Color      = Color::Rgb(245, 189, 230);  // #f5bde6 — special
pub const LAVENDER: Color  = Color::Rgb(183, 189, 248);  // #b7bdf8 — subtle hl

// ══════════════════════════════════════
//  Shared Utility Functions
// ══════════════════════════════════════

/// Wrap text into lines of at most `max_w` display-width characters.
/// Returns at most `max_lines` lines. The last line is truncated with "..." if needed.
pub fn wrap_text(s: &str, max_w: usize, max_lines: usize) -> Vec<String> {
    use unicode_width::UnicodeWidthChar;
    if max_w == 0 || max_lines == 0 { return vec![String::new()]; }

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
                        if tw + tcw > trunc_w { break; }
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

pub fn safe_truncate(s: &str, max_w: usize) -> String {
    if max_w == 0 { return String::new(); }
    if s.width() <= max_w { return s.to_string(); }
    let t = max_w.saturating_sub(3);
    let mut r = String::new();
    let mut w = 0;
    for c in s.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if w + cw > t { break; }
        r.push(c); w += cw;
    }
    r.push_str("...");
    r
}

pub fn safe_pad(s: &str, width: usize) -> String {
    let w = s.width();
    if w >= width {
        let mut r = String::new();
        let mut cw = 0;
        for c in s.chars() {
            let ch_w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
            if cw + ch_w > width { break; }
            r.push(c); cw += ch_w;
        }
        while cw < width { r.push(' '); cw += 1; }
        r
    } else {
        let mut r = s.to_string();
        for _ in 0..(width - w) { r.push(' '); }
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

    // In SourceSelect mode, use full screen for the selection UI
    if app.mode == AppMode::SourceSelect {
        app.layout.list_y = full.y;
        app.layout.list_height = full.height;
        app.layout.bottom_y = full.y + full.height;
        app.layout.width = full.width;
        app.layout.bottom_buttons.clear();
        source_select::draw_source_select(f, app, full);
        return;
    }

    // Layout: tab bar (2 rows) + view content
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),  // tab bar (icon + label + underline)
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
