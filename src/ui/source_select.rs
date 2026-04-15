//! Connection status display.
//!
//! This module has been simplified from the original source selection UI.
//! The Direct Socket architecture means flog_dart clients connect to us,
//! so we no longer need complex source discovery/selection.

#![allow(dead_code)]

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use unicode_width::UnicodeWidthStr;

use crate::app::App;

// ── Catppuccin Macchiato palette ──

const BASE: Color = Color::Rgb(36, 39, 58);
const OVERLAY0: Color = Color::Rgb(110, 115, 141);
const TEXT: Color = Color::Rgb(202, 211, 245);
const BLUE: Color = Color::Rgb(138, 173, 244);
const SAPPHIRE: Color = Color::Rgb(125, 196, 228);
const TEAL: Color = Color::Rgb(139, 213, 202);
const GREEN: Color = Color::Rgb(166, 218, 149);
const MAUVE: Color = Color::Rgb(198, 160, 246);

// ── Banner ASCII art ──

const BANNER: &[&str] = &[" ╔═╗╦  ╔═╗╔═╗ ", " ╠╣ ║  ║ ║║ ╦ ", " ╚  ╩═╝╚═╝╚═╝ "];

const BANNER_COLORS: &[Color] = &[BLUE, SAPPHIRE, TEAL, BLUE, MAUVE, SAPPHIRE];

/// Draw connection status when no clients are connected.
pub fn draw_waiting_for_connection(f: &mut Frame, app: &App, area: Rect) {
    let tick = app.tick;
    let h = area.height as usize;
    let w = area.width as usize;

    let mut lines: Vec<Line> = Vec::new();

    // Calculate vertical centering
    let content_h = BANNER.len() + 6;
    let top_pad = h.saturating_sub(content_h) / 3;

    // Top padding
    for _ in 0..top_pad {
        lines.push(fill_line(w));
    }

    // Banner
    for (row, text) in BANNER.iter().enumerate() {
        lines.push(render_banner_line(text, row, tick, w));
    }

    // Subtitle
    lines.push(centered_text_line("Flutter Log Viewer", w, OVERLAY0));
    lines.push(fill_line(w));

    // Connection status
    let spinner = braille_spinner(tick);
    if app.clients.is_empty() {
        let status = format!("{} Waiting for connection on port {}...", spinner, app.server_port);
        lines.push(centered_text_line(&status, w, TEXT));
        lines.push(fill_line(w));
        lines.push(centered_text_line(
            "Run your Flutter app with flog_dart to connect",
            w,
            OVERLAY0,
        ));
    } else {
        // Show connected clients
        let status = format!("{} Connected clients:", spinner);
        lines.push(centered_text_line(&status, w, GREEN));
        lines.push(fill_line(w));
        for client in &app.clients {
            let client_info = format!("  {} - {} ({})", client.device, client.app, client.os);
            lines.push(centered_text_line(&client_info, w, TEXT));
        }
    }

    // Fill remaining
    while lines.len() < h {
        lines.push(fill_line(w));
    }

    f.render_widget(Paragraph::new(lines).style(Style::default().bg(BASE)), area);
}

// ══════════════════════════════════════
//  Helpers
// ══════════════════════════════════════

fn fill_line(w: usize) -> Line<'static> {
    Line::from(Span::styled(" ".repeat(w), Style::default().bg(BASE)))
}

fn braille_spinner(tick: u64) -> &'static str {
    match (tick / 4) % 8 {
        0 => "\u{28fe}",
        1 => "\u{28fd}",
        2 => "\u{28fb}",
        3 => "\u{28bf}",
        4 => "\u{287f}",
        5 => "\u{28df}",
        6 => "\u{28ef}",
        _ => "\u{28f7}",
    }
}

fn render_banner_line(text: &str, row: usize, tick: u64, total_w: usize) -> Line<'static> {
    let banner_w = text.width();
    let pad_left = total_w.saturating_sub(banner_w) / 2;

    let mut spans: Vec<Span> = vec![Span::styled(
        " ".repeat(pad_left),
        Style::default().bg(BASE),
    )];

    for (ci, ch) in text.chars().enumerate() {
        if ch == ' ' {
            spans.push(Span::styled(" ", Style::default().bg(BASE)));
        } else {
            let color_idx = (ci + row + tick as usize / 3) % BANNER_COLORS.len();
            spans.push(Span::styled(
                ch.to_string(),
                Style::default()
                    .fg(BANNER_COLORS[color_idx])
                    .bg(BASE)
                    .add_modifier(Modifier::BOLD),
            ));
        }
    }

    let used: usize = spans.iter().map(|s| s.content.width()).sum();
    if used < total_w {
        spans.push(Span::styled(
            " ".repeat(total_w - used),
            Style::default().bg(BASE),
        ));
    }

    Line::from(spans)
}

fn centered_text_line(text: &str, total_w: usize, fg: Color) -> Line<'static> {
    let pad = total_w.saturating_sub(text.len()) / 2;
    let mut spans = vec![
        Span::styled(" ".repeat(pad), Style::default().bg(BASE)),
        Span::styled(text.to_string(), Style::default().fg(fg).bg(BASE)),
    ];
    let used = pad + text.len();
    if used < total_w {
        spans.push(Span::styled(
            " ".repeat(total_w - used),
            Style::default().bg(BASE),
        ));
    }
    Line::from(spans)
}
