//! Device picker and connection status display.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
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
const SUBTEXT0: Color = Color::Rgb(165, 173, 203);
const BLUE: Color = Color::Rgb(138, 173, 244);
const SAPPHIRE: Color = Color::Rgb(125, 196, 228);
const TEAL: Color = Color::Rgb(139, 213, 202);
const GREEN: Color = Color::Rgb(166, 218, 149);
const MAUVE: Color = Color::Rgb(198, 160, 246);

// ── Banner ASCII art ──

const BANNER: &[&str] = &[" ╔═╗╦  ╔═╗╔═╗ ", " ╠╣ ║  ║ ║║ ╦ ", " ╚  ╩═╝╚═╝╚═╝ "];

const BANNER_COLORS: &[Color] = &[BLUE, SAPPHIRE, TEAL, BLUE, MAUVE, SAPPHIRE];

/// Lines per device card.
const CARD_HEIGHT: u16 = 4; // name + id + type + separator

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
    let status = format!("{} Waiting for connection on port {}...", spinner, app.server_port);
    lines.push(centered_text_line(&status, w, TEXT));
    lines.push(fill_line(w));

    if !app.discovered_devices.is_empty() {
        lines.push(centered_text_line("Discovered devices:", w, OVERLAY0));
        for dev in &app.discovered_devices {
            let kind = kind_label(&dev.kind);
            let info = format!("  {} ({}) — {}", dev.name, dev.id, kind);
            lines.push(centered_text_line(&info, w, SUBTEXT0));
        }
    } else {
        lines.push(centered_text_line(
            "Run your Flutter app with flog_dart to connect",
            w,
            OVERLAY0,
        ));
    }

    while lines.len() < h {
        lines.push(fill_line(w));
    }

    f.render_widget(Paragraph::new(lines).style(Style::default().bg(BASE)), area);
}

/// Draw the device picker as a centered modal with device cards.
pub fn draw_device_picker(f: &mut Frame, app: &mut App, area: Rect) {
    let devices = &app.discovered_devices;
    if devices.is_empty() {
        return;
    }

    // ── Size: centered, 60% width, up to 70% height ──
    let picker_w = (area.width * 3 / 5).max(40).min(area.width.saturating_sub(4));
    let content_h = devices.len() as u16 * CARD_HEIGHT;
    let picker_h = (content_h + 2).max(10).min(area.height * 7 / 10); // +2 border, min 10
    let picker_x = (area.width.saturating_sub(picker_w)) / 2;
    let picker_y = (area.height.saturating_sub(picker_h)) / 2;

    let picker_area = Rect::new(picker_x, picker_y, picker_w, picker_h);

    f.render_widget(Clear, picker_area);

    let title = format!(" Devices ({}) ", devices.len());
    let block = Block::default()
        .title(title)
        .title_style(Style::default().fg(SAPPHIRE).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(SURFACE1))
        .style(Style::default().bg(MANTLE));

    let inner = block.inner(picker_area);
    let inner_w = inner.width as usize;

    // ── Build card lines ──
    let mut all_lines: Vec<Line> = Vec::new();
    // Map: for each rendered line, which device index does it belong to (for click detection)
    let mut line_to_device: Vec<Option<usize>> = Vec::new();

    for (i, dev) in devices.iter().enumerate() {
        let is_selected = i == app.device_picker_selected;
        let is_connected = app.connected && is_device_connected(app, dev);

        let card_bg = if is_connected {
            SURFACE1
        } else if is_selected {
            SURFACE0
        } else {
            MANTLE
        };

        let status_indicator = if is_connected {
            Span::styled(" ● ", Style::default().fg(GREEN).bg(card_bg).add_modifier(Modifier::BOLD))
        } else {
            Span::styled("   ", Style::default().bg(card_bg))
        };

        // Line 1: icon + name + status
        let kind = kind_label(&dev.kind);
        let name_line = format!(" {} ", dev.name);
        let kind_pill_style = match &dev.kind {
            DeviceKind::Android => Style::default().fg(MANTLE).bg(GREEN).add_modifier(Modifier::BOLD),
            DeviceKind::IosUsb { .. } => Style::default().fg(MANTLE).bg(BLUE).add_modifier(Modifier::BOLD),
            DeviceKind::Local => Style::default().fg(MANTLE).bg(MAUVE).add_modifier(Modifier::BOLD),
        };
        let kind_pill = format!(" {} ", kind);
        let pad1 = inner_w.saturating_sub(3 + name_line.width() + kind_pill.width() + 1);
        all_lines.push(Line::from(vec![
            status_indicator.clone(),
            Span::styled(name_line, Style::default().fg(TEXT).bg(card_bg).add_modifier(Modifier::BOLD)),
            Span::styled(" ".repeat(pad1), Style::default().bg(card_bg)),
            Span::styled(kind_pill, kind_pill_style),
            Span::styled(" ", Style::default().bg(card_bg)),
        ]));
        line_to_device.push(Some(i));

        // Line 2: device ID
        let id_text = format!("   ID: {}", dev.id);
        let pad2 = inner_w.saturating_sub(id_text.width());
        all_lines.push(Line::from(vec![
            Span::styled(id_text, Style::default().fg(SUBTEXT0).bg(card_bg)),
            Span::styled(" ".repeat(pad2), Style::default().bg(card_bg)),
        ]));
        line_to_device.push(Some(i));

        // Line 3: connection info
        let conn_text = if is_connected {
            "   Status: Connected".to_string()
        } else {
            "   Status: Available".to_string()
        };
        let conn_fg = if is_connected { GREEN } else { OVERLAY0 };
        let pad3 = inner_w.saturating_sub(conn_text.width());
        all_lines.push(Line::from(vec![
            Span::styled(conn_text, Style::default().fg(conn_fg).bg(card_bg)),
            Span::styled(" ".repeat(pad3), Style::default().bg(card_bg)),
        ]));
        line_to_device.push(Some(i));

        // Line 4: separator
        if i < devices.len() - 1 {
            let sep = "\u{2500}".repeat(inner_w);
            all_lines.push(Line::from(Span::styled(sep, Style::default().fg(SURFACE0).bg(MANTLE))));
            line_to_device.push(None);
        }
    }

    // ── Scroll handling ──
    let visible_h = inner.height as usize;
    let total_lines = all_lines.len();

    // Ensure selected device is visible
    let selected_start = app.device_picker_selected * CARD_HEIGHT as usize;
    let scroll = if selected_start < app.device_picker_scroll {
        selected_start
    } else if selected_start + CARD_HEIGHT as usize > app.device_picker_scroll + visible_h {
        (selected_start + CARD_HEIGHT as usize).saturating_sub(visible_h)
    } else {
        app.device_picker_scroll
    };
    app.device_picker_scroll = scroll;

    let visible_lines: Vec<Line> = all_lines
        .into_iter()
        .skip(scroll)
        .take(visible_h)
        .collect();

    f.render_widget(Paragraph::new(visible_lines).block(block), picker_area);

    // Scrollbar
    if total_lines > visible_h {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::default().fg(SAPPHIRE))
            .track_style(Style::default().fg(SURFACE0));
        let max_scroll = total_lines.saturating_sub(visible_h);
        let mut state = ScrollbarState::new(max_scroll).position(scroll.min(max_scroll));
        f.render_stateful_widget(scrollbar, inner, &mut state);
    }

    // ── Store click regions ──
    // Map visible line Y → device index
    let mut click_regions: Vec<(u16, u16, u16, usize)> = Vec::new();
    for (vi, line_idx) in (scroll..scroll + visible_h).enumerate() {
        if let Some(Some(dev_idx)) = line_to_device.get(line_idx) {
            click_regions.push((
                inner.y + vi as u16,
                inner.x,
                inner.x + inner.width,
                *dev_idx,
            ));
        }
    }

    app.layout.device_picker_items = click_regions;
    app.layout.device_picker_rect = Some((picker_area.x, picker_area.y, picker_area.width, picker_area.height));
}

fn is_device_connected(app: &App, dev: &Device) -> bool {
    // Simple heuristic: check if any connected client matches this device
    if dev.id == "localhost" {
        // localhost could be iOS sim or macOS — connected if we have any local client
        app.clients.iter().any(|c| c.os == "ios" || c.os == "macos")
    } else {
        // Android or iOS USB — match by device name fragment
        app.clients.iter().any(|c| c.device.contains(&dev.name) || c.device.contains(&dev.id))
    }
}

fn kind_label(kind: &DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Android => "Android",
        DeviceKind::IosUsb { .. } => "iOS USB",
        DeviceKind::Local => "Local",
    }
}

fn device_icon(_kind: &DeviceKind) -> &'static str {
    // Simple text icons that work in any terminal
    match _kind {
        DeviceKind::Android => "\u{e70e}",
        DeviceKind::IosUsb { .. } => "\u{f179}",
        DeviceKind::Local => "\u{f108}",
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
    let text_w = text.width();
    let pad = total_w.saturating_sub(text_w) / 2;
    let mut spans = vec![
        Span::styled(" ".repeat(pad), Style::default().bg(BASE)),
        Span::styled(text.to_string(), Style::default().fg(fg).bg(BASE)),
    ];
    let used = pad + text_w;
    if used < total_w {
        spans.push(Span::styled(
            " ".repeat(total_w - used),
            Style::default().bg(BASE),
        ));
    }
    Line::from(spans)
}
