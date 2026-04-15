//! Device picker dropdown and connection status display.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::transport::device_monitor::{Device, DeviceKind};

// ── Catppuccin Macchiato palette ──

const BASE: Color = Color::Rgb(36, 39, 58);
const MANTLE: Color = Color::Rgb(30, 32, 48);
const SURFACE0: Color = Color::Rgb(54, 58, 79);
const SURFACE1: Color = Color::Rgb(73, 77, 100);
const OVERLAY0: Color = Color::Rgb(110, 115, 141);
const TEXT: Color = Color::Rgb(202, 211, 245);
const BLUE: Color = Color::Rgb(138, 173, 244);
const SAPPHIRE: Color = Color::Rgb(125, 196, 228);
const TEAL: Color = Color::Rgb(139, 213, 202);
const GREEN: Color = Color::Rgb(166, 218, 149);
const MAUVE: Color = Color::Rgb(198, 160, 246);
const RED: Color = Color::Rgb(237, 135, 150);
const YELLOW: Color = Color::Rgb(238, 212, 159);

// ── Banner ASCII art ──

const BANNER: &[&str] = &[" ╔═╗╦  ╔═╗╔═╗ ", " ╠╣ ║  ║ ║║ ╦ ", " ╚  ╩═╝╚═╝╚═╝ "];

const BANNER_COLORS: &[Color] = &[BLUE, SAPPHIRE, TEAL, BLUE, MAUVE, SAPPHIRE];

/// Draw connection status when no clients are connected.
pub fn draw_waiting_for_connection(f: &mut Frame, app: &App, area: Rect) {
    let tick = app.tick;
    let h = area.height as usize;
    let w = area.width as usize;

    let mut lines: Vec<Line> = Vec::new();

    let content_h = BANNER.len() + 6;
    let top_pad = h.saturating_sub(content_h) / 3;

    for _ in 0..top_pad {
        lines.push(fill_line(w));
    }

    for (row, text) in BANNER.iter().enumerate() {
        lines.push(render_banner_line(text, row, tick, w));
    }

    lines.push(centered_text_line("Flutter Log Viewer", w, OVERLAY0));
    lines.push(fill_line(w));

    let spinner = braille_spinner(tick);
    if app.clients.is_empty() {
        let status = format!("{} Waiting for connection on port {}...", spinner, app.server_port);
        lines.push(centered_text_line(&status, w, TEXT));
        lines.push(fill_line(w));

        // Show discovered devices
        if !app.discovered_devices.is_empty() {
            lines.push(centered_text_line("Discovered devices:", w, OVERLAY0));
            for dev in &app.discovered_devices {
                let icon = device_icon(&dev.kind);
                let info = format!("  {} {} ({})", icon, dev.name, dev.id);
                lines.push(centered_text_line(&info, w, OVERLAY0));
            }
        } else {
            lines.push(centered_text_line(
                "Run your Flutter app with flog_dart to connect",
                w,
                OVERLAY0,
            ));
        }
    } else {
        let status = format!("{} Connected", spinner);
        lines.push(centered_text_line(&status, w, GREEN));
        lines.push(fill_line(w));
        for client in &app.clients {
            let client_info = format!("  {} - {} ({})", client.device, client.app, client.os);
            lines.push(centered_text_line(&client_info, w, TEXT));
        }
    }

    while lines.len() < h {
        lines.push(fill_line(w));
    }

    f.render_widget(Paragraph::new(lines).style(Style::default().bg(BASE)), area);
}

/// Draw the device picker dropdown overlay.
pub fn draw_device_picker(f: &mut Frame, app: &mut App, area: Rect) {
    let devices = &app.discovered_devices;
    if devices.is_empty() {
        return;
    }

    let item_count = devices.len();
    let picker_w = 50u16.min(area.width.saturating_sub(4));
    let picker_h = (item_count as u16 + 2).min(area.height.saturating_sub(4)); // +2 for border
    let picker_x = area.width.saturating_sub(picker_w).saturating_sub(1);
    let picker_y = area.height.saturating_sub(picker_h).saturating_sub(1);

    let picker_area = Rect::new(picker_x, picker_y, picker_w, picker_h);

    // Clear area behind dropdown
    f.render_widget(Clear, picker_area);

    let block = Block::default()
        .title(" Devices ")
        .title_style(Style::default().fg(SAPPHIRE).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(SURFACE1))
        .style(Style::default().bg(MANTLE));

    let inner = block.inner(picker_area);

    let mut lines: Vec<Line> = Vec::new();
    let mut click_regions: Vec<(u16, u16, u16, usize)> = Vec::new(); // (y, x_start, x_end, idx)

    for (i, dev) in devices.iter().enumerate() {
        let icon = device_icon(&dev.kind);
        let connected = app.clients.iter().any(|c| {
            // Check if this device is the currently connected one
            c.device.contains(&dev.name) || dev.id == "localhost"
        });

        let is_selected = i == app.device_picker_selected;
        let (fg, bg) = if connected {
            (MANTLE, GREEN)
        } else if is_selected {
            (TEXT, SURFACE1)
        } else {
            (TEXT, MANTLE)
        };

        let label = format!(" {} {} ", icon, dev.name);
        let kind_label = match &dev.kind {
            DeviceKind::Android => " Android",
            DeviceKind::IosUsb { .. } => " iOS USB",
            DeviceKind::Local => " Local",
        };

        let pad = (inner.width as usize).saturating_sub(label.width() + kind_label.len());
        lines.push(Line::from(vec![
            Span::styled(label, Style::default().fg(fg).bg(bg)),
            Span::styled(" ".repeat(pad), Style::default().bg(bg)),
            Span::styled(kind_label.to_string(), Style::default().fg(OVERLAY0).bg(bg)),
        ]));

        click_regions.push((
            inner.y + i as u16,
            inner.x,
            inner.x + inner.width,
            i,
        ));
    }

    f.render_widget(Paragraph::new(lines).block(block), picker_area);

    // Store click regions for event handling
    app.layout.device_picker_items = click_regions;
    app.layout.device_picker_rect = Some((picker_area.x, picker_area.y, picker_area.width, picker_area.height));
}

fn device_icon(kind: &DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Android => "\u{e70e}",  // nf-dev-android — or use simple emoji
        DeviceKind::IosUsb { .. } => "\u{f179}",  // nf-fa-apple
        DeviceKind::Local => "\u{f108}",    // nf-fa-desktop
    }
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
