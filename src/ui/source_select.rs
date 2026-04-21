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
    let quickstart_h = if app.discovered_devices.is_empty() { 7 } else { 0 };
    let devlist_h = if app.discovered_devices.is_empty() {
        0
    } else {
        app.discovered_devices.len() + 2
    };
    let content_h = BANNER.len() + 6 + quickstart_h + devlist_h;
    let top_pad = h.saturating_sub(content_h) / 3;

    for _ in 0..top_pad {
        lines.push(fill_line(w));
    }

    for (row, text) in BANNER.iter().enumerate() {
        lines.push(render_banner_line(text, row, tick, w));
    }

    lines.push(centered_text_line(
        "Flutter Log Viewer · Network Inspector",
        w,
        OVERLAY0,
    ));
    lines.push(fill_line(w));

    let spinner = braille_spinner(tick);
    let status = format!("{}  Waiting for connection on port {}...", spinner, app.server_port);
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
        let box_w: usize = 46;
        let box_left_pad = w.saturating_sub(box_w) / 2;
        let border_style = Style::default().fg(SURFACE0).bg(BASE);
        let border_line = |left: char, mid: char, right: char| {
            let mut spans = vec![
                Span::styled(" ".repeat(box_left_pad), Style::default().bg(BASE)),
                Span::styled(left.to_string(), border_style),
                Span::styled(mid.to_string().repeat(box_w - 2), border_style),
                Span::styled(right.to_string(), border_style),
            ];
            let total = box_left_pad + box_w;
            if total < w {
                spans.push(Span::styled(" ".repeat(w - total), Style::default().bg(BASE)));
            }
            Line::from(spans)
        };
        let content_line = |text: &str| {
            let inner_w = box_w - 2;
            let text_w = text.width();
            let right = inner_w.saturating_sub(text_w);
            let mut spans = vec![
                Span::styled(" ".repeat(box_left_pad), Style::default().bg(BASE)),
                Span::styled("│", border_style),
                Span::styled(text.to_string(), Style::default().fg(SUBTEXT0).bg(BASE)),
                Span::styled(" ".repeat(right), Style::default().bg(BASE)),
                Span::styled("│", border_style),
            ];
            let total = box_left_pad + box_w;
            if total < w {
                spans.push(Span::styled(" ".repeat(w - total), Style::default().bg(BASE)));
            }
            Line::from(spans)
        };

        lines.push(border_line('┌', '─', '┐'));
        lines.push(content_line("  Quick Start                               "));
        lines.push(content_line("   1. Add flog_dart to your Flutter app     "));
        lines.push(content_line("   2. Run your app in debug mode            "));
        lines.push(content_line("   3. flog will auto-connect                "));
        lines.push(content_line("                                            "));
        lines.push(border_line('└', '─', '┘'));
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

    // ── Size: fixed 3/4 height, scrollable content ──
    let picker_w = (area.width * 2 / 3).max(50).min(area.width.saturating_sub(4));
    let picker_h = (area.height * 3 / 4).max(6);
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

    // ── Empty state: no devices discovered ──
    if device_order.is_empty() {
        let empty_lines = vec![
            Line::from(Span::styled(" ".repeat(inner_w), Style::default().bg(MANTLE))),
            {
                let text = "No devices found";
                let pad_l = inner_w.saturating_sub(text.width()) / 2;
                let pad_r = inner_w.saturating_sub(pad_l + text.width());
                Line::from(vec![
                    Span::styled(" ".repeat(pad_l), Style::default().bg(MANTLE)),
                    Span::styled(text, Style::default().fg(OVERLAY0).bg(MANTLE)),
                    Span::styled(" ".repeat(pad_r), Style::default().bg(MANTLE)),
                ])
            },
            Line::from(Span::styled(" ".repeat(inner_w), Style::default().bg(MANTLE))),
            {
                let text = "Run your Flutter app with flog_dart";
                let pad_l = inner_w.saturating_sub(text.width()) / 2;
                let pad_r = inner_w.saturating_sub(pad_l + text.width());
                Line::from(vec![
                    Span::styled(" ".repeat(pad_l), Style::default().bg(MANTLE)),
                    Span::styled(text, Style::default().fg(SURFACE1).bg(MANTLE)),
                    Span::styled(" ".repeat(pad_r), Style::default().bg(MANTLE)),
                ])
            },
        ];
        f.render_widget(Paragraph::new(empty_lines).block(block), picker_area);
        app.layout.device_picker_items = Vec::new();
        app.layout.device_picker_rect = Some((picker_area.x, picker_area.y, picker_area.width, picker_area.height));
        app.layout.device_picker_item_ids = Vec::new();
        app.layout.device_picker_total_lines = 0;
        return;
    }

    // ── Pass 1: Build layout items (device headers + app cards) ──
    // Each item has a height so we can compute Y offsets and scroll.
    enum PickerItem {
        DeviceHeader {
            name: String,
            kind: DeviceKind,
            id: String,
        },
        AppCard {
            sel_idx: usize,
            app_id: String,
            is_active: bool,
            app_name: String,
            app_version: String,
            package_name: String,
            os: String,
            build_mode: String,
            port: u16,
        },
        Waiting,
        Separator,
    }

    impl PickerItem {
        fn height(&self) -> u16 {
            match self {
                // pill+name, id+conn, blank
                PickerItem::DeviceHeader { .. } => 3,
                // border(1) + content(4) + border(1) + spacing(1)
                PickerItem::AppCard { .. } => 7,
                PickerItem::Waiting => 1,
                PickerItem::Separator => 2,
            }
        }
    }

    let mut items: Vec<PickerItem> = Vec::new();
    let mut selectable_ids: Vec<String> = Vec::new();

    for (di, device_id) in device_order.iter().enumerate() {
        let dev = app.discovered_devices.get(device_id);
        let dev_name = dev.map(|d| d.name.clone()).unwrap_or_else(|| device_id.clone());
        let dev_kind = dev.map(|d| d.kind.clone()).unwrap_or(DeviceKind::Local);

        items.push(PickerItem::DeviceHeader {
            name: dev_name,
            kind: dev_kind,
            id: device_id.clone(),
        });

        let app_list = apps_by_device.get(device_id);
        if let Some(list) = app_list {
            for ca in list {
                let sel_idx = selectable_ids.len();
                selectable_ids.push(ca.id.clone());
                items.push(PickerItem::AppCard {
                    sel_idx,
                    app_id: ca.id.clone(),
                    is_active: app.active_app_id.as_deref() == Some(&ca.id),
                    app_name: ca.app_name.clone(),
                    app_version: ca.app_version.clone(),
                    os: ca.os.clone(),
                    package_name: ca.package_name.clone(),
                    build_mode: ca.build_mode.clone(),
                    port: ca.port,
                });
            }
        } else {
            items.push(PickerItem::Waiting);
        }

        if di < device_order.len() - 1 {
            items.push(PickerItem::Separator);
        }
    }

    // ── Clamp selection ──
    let max_sel = selectable_ids.len().saturating_sub(1);
    if app.device_picker_selected > max_sel {
        app.device_picker_selected = max_sel;
    }

    // ── Compute Y offsets ──
    let total_content_h: u16 = items.iter().map(|it| it.height()).sum();
    let visible_h = inner.height;

    // Find the Y offset of the selected card
    let mut selected_y: u16 = 0;
    let mut selected_h: u16 = 0;
    {
        let mut y = 0u16;
        for it in &items {
            if let PickerItem::AppCard { sel_idx, .. } = it {
                if *sel_idx == app.device_picker_selected {
                    selected_y = y;
                    selected_h = it.height();
                    break;
                }
            }
            y += it.height();
        }
    }

    // Scroll to keep selected card visible
    let scroll = &mut app.device_picker_scroll;
    let scroll_u16 = *scroll as u16;
    if selected_y < scroll_u16 {
        *scroll = selected_y.saturating_sub(1) as usize;
    } else if selected_y + selected_h > scroll_u16 + visible_h {
        *scroll = (selected_y + selected_h).saturating_sub(visible_h) as usize;
    }
    let scroll_offset = *scroll as u16;

    // ── Pass 2: Render each item at its absolute position ──
    // First render the outer block
    f.render_widget(block, picker_area);

    let card_margin = 3u16; // left margin for app cards inside picker
    let card_w = inner.width.saturating_sub(card_margin * 2);

    let mut click_regions: Vec<(u16, u16, u16, usize)> = Vec::new();
    let mut y = 0u16; // content Y (before scroll)

    for item in &items {
        let h = item.height();
        // Skip items above the scroll viewport
        if y + h <= scroll_offset {
            y += h;
            continue;
        }
        // Stop if below viewport
        if y >= scroll_offset + visible_h {
            break;
        }

        let screen_y = inner.y + y.saturating_sub(scroll_offset);
        let remaining = (inner.y + visible_h).saturating_sub(screen_y);
        let visible_item_h = h.min(remaining);

        match item {
            PickerItem::DeviceHeader { name, kind, id } => {
                let platform_label = match kind {
                    DeviceKind::Android => "Android",
                    DeviceKind::IosUsb { .. } => "iOS",
                    DeviceKind::Local => "Simulator",
                };
                let pill_style = match kind {
                    DeviceKind::Android => Style::default().fg(MANTLE).bg(GREEN).add_modifier(Modifier::BOLD),
                    DeviceKind::IosUsb { .. } => Style::default().fg(MANTLE).bg(BLUE).add_modifier(Modifier::BOLD),
                    DeviceKind::Local => Style::default().fg(MANTLE).bg(MAUVE).add_modifier(Modifier::BOLD),
                };
                let conn_type = match kind {
                    DeviceKind::Android => "ADB",
                    DeviceKind::IosUsb { .. } => "USB",
                    DeviceKind::Local => "Simulator",
                };
                let id_display = if id.len() > 20 {
                    format!("{}...{}", &id[..8], &id[id.len()-4..])
                } else {
                    id.clone()
                };

                let pill_text = format!(" {} ", platform_label);
                let iw = inner.width as usize;

                let mut lines: Vec<Line> = Vec::new();
                // Line 1: pill + name
                let name_text = format!(" {}", name);
                let used1 = 2 + pill_text.width() + name_text.width();
                let pad1 = iw.saturating_sub(used1);
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default().bg(MANTLE)),
                    Span::styled(pill_text, pill_style),
                    Span::styled(name_text, Style::default().fg(TEXT).bg(MANTLE).add_modifier(Modifier::BOLD)),
                    Span::styled(" ".repeat(pad1), Style::default().bg(MANTLE)),
                ]));
                // Line 2: id · conn
                let id_line = format!("  {} \u{00b7} {}", id_display, conn_type);
                let pad2 = iw.saturating_sub(id_line.width());
                lines.push(Line::from(vec![
                    Span::styled(id_line, Style::default().fg(OVERLAY0).bg(MANTLE)),
                    Span::styled(" ".repeat(pad2), Style::default().bg(MANTLE)),
                ]));
                // Line 3: blank
                lines.push(Line::from(Span::styled(" ".repeat(iw), Style::default().bg(MANTLE))));

                let area = Rect::new(inner.x, screen_y, inner.width, visible_item_h);
                f.render_widget(
                    Paragraph::new(lines).style(Style::default().bg(MANTLE)),
                    area,
                );
            }
            PickerItem::AppCard {
                sel_idx, app_id: _, is_active, app_name, app_version,
                package_name, os, build_mode, port,
            } => {
                let is_selected = *sel_idx == app.device_picker_selected;
                let (border_color, card_bg) = if *is_active {
                    (BLUE, SURFACE1)
                } else if is_selected {
                    (SAPPHIRE, SURFACE0)
                } else {
                    (SURFACE0, MANTLE)
                };

                let label_fg = OVERLAY0;
                let value_fg = if *is_active { TEXT } else { SUBTEXT0 };
                let unknown_fg = SURFACE1;

                // Card area with margin
                let card_area = Rect::new(
                    inner.x + card_margin,
                    screen_y,
                    card_w,
                    (h - 1).min(visible_item_h), // -1 for spacing line
                );

                let card_block = Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(border_color))
                    .style(Style::default().bg(card_bg));

                let card_inner = card_block.inner(card_area);
                let ciw = card_inner.width as usize;

                // Build card content lines
                let mut card_lines: Vec<Line> = Vec::new();

                // Line 1: indicator + name + [ACTIVE] + port
                let indicator = if *is_active {
                    Span::styled(" \u{25cf} ", Style::default().fg(GREEN).bg(card_bg).add_modifier(Modifier::BOLD))
                } else {
                    Span::styled(" \u{25cb} ", Style::default().fg(TEAL).bg(card_bg))
                };
                let label = if app_version.is_empty() {
                    app_name.clone()
                } else {
                    format!("{} v{}", app_name, app_version)
                };
                let active_tag = if *is_active { " ACTIVE" } else { "" };
                let port_text = format!("Port: {}", port);
                let fixed = 3 + label.width() + active_tag.len() + port_text.width();
                let pad = ciw.saturating_sub(fixed);
                let mut spans = vec![
                    indicator,
                    Span::styled(label, Style::default().fg(TEXT).bg(card_bg).add_modifier(Modifier::BOLD)),
                ];
                if *is_active {
                    spans.push(Span::styled(
                        active_tag.to_string(),
                        Style::default().fg(GREEN).bg(card_bg).add_modifier(Modifier::BOLD),
                    ));
                }
                spans.push(Span::styled(" ".repeat(pad), Style::default().bg(card_bg)));
                spans.push(Span::styled(port_text, Style::default().fg(SAPPHIRE).bg(card_bg)));
                card_lines.push(Line::from(spans));

                // Detail rows
                let details: [(&str, &str); 3] = [
                    ("Package", package_name.as_str()),
                    ("Platform", os.as_str()),
                    ("Mode", build_mode.as_str()),
                ];
                for (lbl, val) in details {
                    let left = format!("   {}: ", lbl);
                    let (display, fg) = if val.is_empty() {
                        ("unknown", unknown_fg)
                    } else {
                        (val, value_fg)
                    };
                    let used = left.width() + display.width();
                    let dpad = ciw.saturating_sub(used);
                    card_lines.push(Line::from(vec![
                        Span::styled(left, Style::default().fg(label_fg).bg(card_bg)),
                        Span::styled(display.to_string(), Style::default().fg(fg).bg(card_bg)),
                        Span::styled(" ".repeat(dpad), Style::default().bg(card_bg)),
                    ]));
                }

                f.render_widget(
                    Paragraph::new(card_lines).block(card_block),
                    card_area,
                );

                // Click regions for this card
                for row in 0..card_area.height {
                    let ry = card_area.y + row;
                    if ry >= inner.y && ry < inner.y + visible_h {
                        click_regions.push((ry, card_area.x, card_area.x + card_area.width, *sel_idx));
                    }
                }
            }
            PickerItem::Waiting => {
                let iw = inner.width as usize;
                let text = "     Waiting for app...";
                let pad = iw.saturating_sub(text.width());
                let area = Rect::new(inner.x, screen_y, inner.width, 1);
                f.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::styled(text, Style::default().fg(OVERLAY0).bg(MANTLE)),
                        Span::styled(" ".repeat(pad), Style::default().bg(MANTLE)),
                    ])),
                    area,
                );
            }
            PickerItem::Separator => {
                let iw = inner.width as usize;
                let sep = "\u{2500}".repeat(iw);
                let mut lines = vec![
                    Line::from(Span::styled(sep, Style::default().fg(SURFACE0).bg(MANTLE))),
                ];
                lines.push(Line::from(Span::styled(" ".repeat(iw), Style::default().bg(MANTLE))));
                let area = Rect::new(inner.x, screen_y, inner.width, visible_item_h);
                f.render_widget(
                    Paragraph::new(lines).style(Style::default().bg(MANTLE)),
                    area,
                );
            }
        }

        y += h;
    }

    // Scrollbar
    let total_lines = total_content_h as usize;
    let vis_h = visible_h as usize;
    if total_lines > vis_h {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::default().fg(SAPPHIRE))
            .track_style(Style::default().fg(SURFACE0));
        let max_scroll = total_lines.saturating_sub(vis_h);
        let pos = (app.device_picker_scroll).min(max_scroll);
        let mut state = ScrollbarState::new(max_scroll).position(pos);
        f.render_stateful_widget(scrollbar, inner, &mut state);
    }

    app.layout.device_picker_items = click_regions;
    app.layout.device_picker_rect = Some((picker_area.x, picker_area.y, picker_area.width, picker_area.height));
    app.layout.device_picker_item_ids = selectable_ids;
    app.layout.device_picker_total_lines = total_lines;
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
