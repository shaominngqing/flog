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
use crate::transport::device_monitor::DeviceKind;

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
        for dev in app.discovered_devices.values() {
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

/// Draw the device picker as a centered modal with tree-level device → app layout.
pub fn draw_device_picker(f: &mut Frame, app: &mut App, area: Rect) {
    // ── Build tree: group apps by device_id ──
    let mut device_order: Vec<String> = Vec::new();
    let mut apps_by_device: std::collections::HashMap<String, Vec<&crate::app::ConnectedApp>> =
        std::collections::HashMap::new();
    for ca in &app.connected_apps {
        apps_by_device.entry(ca.device_id.clone()).or_default().push(ca);
        if !device_order.contains(&ca.device_id) {
            device_order.push(ca.device_id.clone());
        }
    }
    for dev in app.discovered_devices.values() {
        if !device_order.contains(&dev.id) {
            device_order.push(dev.id.clone());
        }
    }

    let device_count = device_order.len();

    // ── Size: fit content, don't over-expand ──
    let picker_w = (area.width * 2 / 3).max(50).min(area.width.saturating_sub(4));
    let est_lines: usize = device_order.iter().enumerate().map(|(i, did)| {
        let app_count = apps_by_device.get(did).map(|v| v.len()).unwrap_or(0);
        // device header(2) + per-app(5: name+pkg+platform+mode+pad) + waiting(1) + separator(1)
        2 + if app_count > 0 { app_count * 5 } else { 1 } + if i < device_order.len() - 1 { 1 } else { 0 }
    }).sum();
    let picker_h = ((est_lines + 2) as u16).max(6).min(area.height * 4 / 5);
    let picker_x = (area.width.saturating_sub(picker_w)) / 2;
    let picker_y = (area.height.saturating_sub(picker_h)) / 2;

    let picker_area = Rect::new(picker_x, picker_y, picker_w, picker_h);
    f.render_widget(Clear, picker_area);

    let title = format!(" Devices ({}) ", device_count);
    let block = Block::default()
        .title(title)
        .title_style(Style::default().fg(SAPPHIRE).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(SURFACE1))
        .style(Style::default().bg(MANTLE));

    let inner = block.inner(picker_area);
    let inner_w = inner.width as usize;

    let mut all_lines: Vec<Line> = Vec::new();
    let mut line_to_selectable: Vec<Option<usize>> = Vec::new();
    let mut selectable_ids: Vec<String> = Vec::new();
    let mut selectable_line_starts: Vec<usize> = Vec::new();

    for (di, device_id) in device_order.iter().enumerate() {
        let dev = app.discovered_devices.get(device_id);
        let dev_name = dev.map(|d| d.name.as_str()).unwrap_or(device_id.as_str());
        let dev_kind = dev.map(|d| &d.kind).cloned().unwrap_or(DeviceKind::Local);

        // ═══ Device header (2 lines on SURFACE0) ═══
        // Line 1: device name + platform pill
        let platform_label = match &dev_kind {
            DeviceKind::Android => "Android",
            DeviceKind::IosUsb { .. } => "iOS",
            DeviceKind::Local => "Simulator",
        };
        let pill_style = match &dev_kind {
            DeviceKind::Android => Style::default().fg(MANTLE).bg(GREEN).add_modifier(Modifier::BOLD),
            DeviceKind::IosUsb { .. } => Style::default().fg(MANTLE).bg(BLUE).add_modifier(Modifier::BOLD),
            DeviceKind::Local => Style::default().fg(MANTLE).bg(MAUVE).add_modifier(Modifier::BOLD),
        };
        let pill_text = format!(" {} ", platform_label);
        let name_text = format!("  {} ", dev_name);
        let pad1 = inner_w.saturating_sub(name_text.width() + pill_text.width() + 1);
        all_lines.push(Line::from(vec![
            Span::styled(name_text, Style::default().fg(TEXT).bg(SURFACE0).add_modifier(Modifier::BOLD)),
            Span::styled(" ".repeat(pad1), Style::default().bg(SURFACE0)),
            Span::styled(pill_text, pill_style),
            Span::styled(" ", Style::default().bg(SURFACE0)),
        ]));
        line_to_selectable.push(None);

        // Line 2: device ID (serial / udid)
        let id_text = format!("  ID: {}", device_id);
        let pad2 = inner_w.saturating_sub(id_text.width());
        all_lines.push(Line::from(vec![
            Span::styled(id_text, Style::default().fg(OVERLAY0).bg(SURFACE0)),
            Span::styled(" ".repeat(pad2), Style::default().bg(SURFACE0)),
        ]));
        line_to_selectable.push(None);

        // ═══ App sub-cards ═══
        let apps = apps_by_device.get(device_id);
        if let Some(app_list) = apps {
            for ca in app_list {
                let sel_idx = selectable_ids.len();
                selectable_ids.push(ca.id.clone());
                selectable_line_starts.push(all_lines.len());

                let is_active = app.active_app_id.as_deref() == Some(&ca.id);
                let is_selected = sel_idx == app.device_picker_selected;
                let row_bg = if is_active {
                    SURFACE1
                } else if is_selected {
                    SURFACE0
                } else {
                    MANTLE
                };

                let label_fg = OVERLAY0;
                let value_fg = if is_active { TEXT } else { SUBTEXT0 };
                let unknown_fg = SURFACE1;

                // Helper: render "     Label: value" line; empty values show "unknown" dimmed
                let push_kv = |lines: &mut Vec<Line>, l2s: &mut Vec<Option<usize>>,
                               label: &str, value: &str, bg: Color, si: usize| {
                    let left = format!("     {}: ", label);
                    let (display, fg) = if value.is_empty() {
                        ("unknown", unknown_fg)
                    } else {
                        (value, value_fg)
                    };
                    let used = left.width() + display.width();
                    let pad = inner_w.saturating_sub(used);
                    lines.push(Line::from(vec![
                        Span::styled(left, Style::default().fg(label_fg).bg(bg)),
                        Span::styled(display.to_string(), Style::default().fg(fg).bg(bg)),
                        Span::styled(" ".repeat(pad), Style::default().bg(bg)),
                    ]));
                    l2s.push(Some(si));
                };

                // Line 1: indicator + app name + version
                let indicator = if is_active {
                    Span::styled("   \u{25cf} ", Style::default().fg(GREEN).bg(row_bg).add_modifier(Modifier::BOLD))
                } else {
                    Span::styled("   \u{25cb} ", Style::default().fg(TEAL).bg(row_bg))
                };
                let app_label = if ca.app_version.is_empty() {
                    ca.app_name.clone()
                } else {
                    format!("{} v{}", ca.app_name, ca.app_version)
                };
                let port_text = format!("Port: {} ", ca.port);
                let pad_a1 = inner_w.saturating_sub(5 + app_label.width() + port_text.width());
                all_lines.push(Line::from(vec![
                    indicator,
                    Span::styled(app_label, Style::default().fg(TEXT).bg(row_bg).add_modifier(Modifier::BOLD)),
                    Span::styled(" ".repeat(pad_a1), Style::default().bg(row_bg)),
                    Span::styled(port_text, Style::default().fg(SAPPHIRE).bg(row_bg)),
                ]));
                line_to_selectable.push(Some(sel_idx));

                // Detail rows — always shown, "unknown" when empty
                push_kv(&mut all_lines, &mut line_to_selectable,
                        "Package", &ca.package_name, row_bg, sel_idx);
                push_kv(&mut all_lines, &mut line_to_selectable,
                        "Platform", &ca.os, row_bg, sel_idx);
                push_kv(&mut all_lines, &mut line_to_selectable,
                        "Mode", &ca.build_mode, row_bg, sel_idx);

                // Bottom padding
                all_lines.push(Line::from(Span::styled(" ".repeat(inner_w), Style::default().bg(row_bg))));
                line_to_selectable.push(Some(sel_idx));
            }
        } else {
            // No apps — "Waiting for app..."
            let wait_text = "     Waiting for app...".to_string();
            let pad_w = inner_w.saturating_sub(wait_text.width());
            all_lines.push(Line::from(vec![
                Span::styled(wait_text, Style::default().fg(OVERLAY0).bg(MANTLE)),
                Span::styled(" ".repeat(pad_w), Style::default().bg(MANTLE)),
            ]));
            line_to_selectable.push(None);
        }

        // Thin separator between device groups
        if di < device_order.len() - 1 {
            let sep = "\u{2500}".repeat(inner_w);
            all_lines.push(Line::from(Span::styled(sep, Style::default().fg(SURFACE0).bg(MANTLE))));
            line_to_selectable.push(None);
        }
    }

    // ── Clamp selection ──
    let max_sel = selectable_ids.len().saturating_sub(1);
    if app.device_picker_selected > max_sel {
        app.device_picker_selected = max_sel;
    }

    // ── Scroll: ensure selected app's first line is visible ──
    let visible_h = inner.height as usize;
    let total_lines = all_lines.len();
    let selected_line = selectable_line_starts.get(app.device_picker_selected).copied().unwrap_or(0);
    // Each app sub-card is 5 lines (name + package + platform + mode + pad)
    let selected_end = selected_line + 5;
    let scroll = if selected_line < app.device_picker_scroll {
        selected_line.saturating_sub(1) // show a bit of context above
    } else if selected_end > app.device_picker_scroll + visible_h {
        selected_end.saturating_sub(visible_h)
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

    // ── Store click regions (all lines belonging to a selectable app) ──
    let mut click_regions: Vec<(u16, u16, u16, usize)> = Vec::new();
    for (vi, line_idx) in (scroll..(scroll + visible_h).min(line_to_selectable.len())).enumerate() {
        if let Some(Some(sel_idx)) = line_to_selectable.get(line_idx) {
            click_regions.push((
                inner.y + vi as u16,
                inner.x,
                inner.x + inner.width,
                *sel_idx,
            ));
        }
    }

    app.layout.device_picker_items = click_regions;
    app.layout.device_picker_rect = Some((picker_area.x, picker_area.y, picker_area.width, picker_area.height));
    app.layout.device_picker_item_ids = selectable_ids;
}

fn kind_label(kind: &DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Android => "Android",
        DeviceKind::IosUsb { .. } => "iOS USB",
        DeviceKind::Local => "Local",
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
