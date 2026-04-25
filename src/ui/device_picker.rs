//! Device picker and connection status display.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState,
    },
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
const SAPPHIRE: Color = Color::Rgb(125, 196, 228);
const TEAL: Color = Color::Rgb(139, 213, 202);
const GREEN: Color = Color::Rgb(166, 218, 149);

/// A content line for the device picker: full-width spans + optional click target (sel_idx).
struct PickerLine {
    spans: Vec<Span<'static>>,
    click_target: Option<usize>,
}

impl PickerLine {
    fn plain(w: usize, bg: Color) -> Self {
        PickerLine {
            spans: vec![Span::styled(" ".repeat(w), Style::default().bg(bg))],
            click_target: None,
        }
    }
}

/// Draw the device picker as a centered modal. Each device is a rounded container
/// that visually wraps its app cards. Active app uses double-line border.
pub fn draw_device_picker(f: &mut Frame, app: &mut App, area: Rect) {
    // ── Build tree: group apps by device_id ──
    let mut device_order: Vec<String> = Vec::new();
    let mut apps_by_device: std::collections::HashMap<String, Vec<crate::app::ConnectedApp>> =
        std::collections::HashMap::new();
    for ca in &app.connected_apps {
        apps_by_device
            .entry(ca.device_id.clone())
            .or_default()
            .push(ca.clone());
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

    // ── Size ──
    let picker_w = (area.width * 2 / 3)
        .max(60)
        .min(area.width.saturating_sub(4));
    let picker_h = (area.height * 3 / 4).max(8);
    let picker_x = (area.width.saturating_sub(picker_w)) / 2;
    let picker_y = (area.height.saturating_sub(picker_h)) / 2;
    let picker_area = Rect::new(picker_x, picker_y, picker_w, picker_h);
    f.render_widget(Clear, picker_area);

    // ── Outer modal block ──
    let title_top = format!(" Devices ({}) ", device_count);
    let hints = " ↑↓ navigate  ⏎ connect  esc cancel ";
    let block = Block::default()
        .title(title_top)
        .title_style(Style::default().fg(SAPPHIRE).add_modifier(Modifier::BOLD))
        .title_bottom(
            Line::from(Span::styled(hints, Style::default().fg(OVERLAY0))).right_aligned(),
        )
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(SURFACE1))
        .style(Style::default().bg(BASE));

    let inner = block.inner(picker_area);
    let inner_w = inner.width as usize;
    let inner_h = inner.height as usize;

    // ── Empty state ──
    if device_order.is_empty() {
        let bg = BASE;
        let center_text = |text: &str, fg: Color| -> Line<'static> {
            let pad_l = inner_w.saturating_sub(text.width()) / 2;
            let pad_r = inner_w.saturating_sub(pad_l + text.width());
            Line::from(vec![
                Span::styled(" ".repeat(pad_l), Style::default().bg(bg)),
                Span::styled(text.to_string(), Style::default().fg(fg).bg(bg)),
                Span::styled(" ".repeat(pad_r), Style::default().bg(bg)),
            ])
        };
        let mut lines: Vec<Line> = vec![
            Line::from(Span::styled(" ".repeat(inner_w), Style::default().bg(bg))),
            center_text("No devices found", OVERLAY0),
            Line::from(Span::styled(" ".repeat(inner_w), Style::default().bg(bg))),
            center_text("Run your Flutter app with flog_dart", SURFACE1),
        ];
        while lines.len() < inner_h {
            lines.push(Line::from(Span::styled(
                " ".repeat(inner_w),
                Style::default().bg(bg),
            )));
        }
        f.render_widget(Paragraph::new(lines).block(block), picker_area);
        app.layout.device_picker_items = Vec::new();
        app.layout.device_picker_rect = Some((
            picker_area.x,
            picker_area.y,
            picker_area.width,
            picker_area.height,
        ));
        app.layout.device_picker_item_ids = Vec::new();
        app.layout.device_picker_total_lines = 0;
        return;
    }

    // ── Build content lines ──
    //
    // Layout (widths inside `inner`):
    //   col 0            → modal bg (BASE gutter)
    //   col 1 .. dev_w+1 → device container (width = inner_w - 2)
    //   col dev_w+1 ..   → modal bg (BASE gutter)
    //
    // Inside a device container (width dev_w):
    //   col 0            → device │
    //   col 1 .. 4       → card gutter (3 spaces of MANTLE) — actually 3 cols
    //   col 4 .. dev_w-4 → app card (width = dev_w - 7)
    //   col dev_w-4 .. dev_w-1 → card right gutter (3 spaces of MANTLE)
    //   col dev_w-1      → device │
    //
    // Card widths must be ≥ 20 or we fall back to no-indent.

    let device_gutter = 1u16; // cols of BASE on each side of device container
    let dev_w = inner_w.saturating_sub(2 * device_gutter as usize);

    let card_indent = 3u16; // MANTLE gutter inside device container, left & right of card
    let card_w = dev_w.saturating_sub(2 * card_indent as usize);

    let mut lines: Vec<PickerLine> = Vec::new();
    let mut selectable_ids: Vec<String> = Vec::new();

    // One blank line of BASE at top for breathing room.
    lines.push(PickerLine::plain(inner_w, BASE));

    for (di, device_id) in device_order.iter().enumerate() {
        let dev = app.discovered_devices.get(device_id);
        let dev_name = dev
            .map(|d| d.name.clone())
            .unwrap_or_else(|| device_id.clone());
        let dev_kind = dev.map(|d| d.kind.clone()).unwrap_or(DeviceKind::Local);

        // Device header: platform tag + name + connection + short id
        let (platform_tag, conn_label) = match &dev_kind {
            DeviceKind::Android => ("Android", "ADB"),
            DeviceKind::IosUsb { .. } => ("iOS", "USB"),
            DeviceKind::Local => ("Simulator", "localhost"),
        };

        // Short id for display (device_id is typically the UDID / hostname key).
        let id_short = shorten_id(device_id);

        // Device top border: `╭─ [iOS] iPhone 17 ─── USB · 00008150...401C ─╮`
        push_device_top(
            &mut lines,
            inner_w,
            dev_w,
            device_gutter,
            platform_tag,
            &dev_name,
            conn_label,
            &id_short,
        );

        // Inside-device blank row
        push_device_inner_blank(&mut lines, inner_w, dev_w, device_gutter);

        // App cards (or Waiting)
        let list = apps_by_device.get(device_id);
        match list {
            Some(apps) if !apps.is_empty() => {
                for (ai, ca) in apps.iter().enumerate() {
                    let sel_idx = selectable_ids.len();
                    selectable_ids.push(ca.id.clone());
                    let is_active = app.active_app_id.as_deref() == Some(&ca.id);
                    let is_selected = app.device_picker_selected == sel_idx;

                    push_app_card(
                        &mut lines,
                        inner_w,
                        dev_w,
                        device_gutter,
                        card_indent,
                        card_w,
                        sel_idx,
                        is_active,
                        is_selected,
                        &ca.app_name,
                        &ca.app_version,
                        &ca.package_name,
                        &ca.os,
                        &ca.build_mode,
                        ca.port,
                    );

                    // Blank inside device between cards (but not after last)
                    if ai + 1 < apps.len() {
                        push_device_inner_blank(&mut lines, inner_w, dev_w, device_gutter);
                    }
                }
            }
            _ => {
                // Waiting row inside device container
                push_waiting_row(&mut lines, inner_w, dev_w, device_gutter);
            }
        }

        // Inside-device blank row before bottom border
        push_device_inner_blank(&mut lines, inner_w, dev_w, device_gutter);

        // Device bottom border
        push_device_bottom(&mut lines, inner_w, dev_w, device_gutter);

        // Blank line of BASE between devices
        if di + 1 < device_order.len() {
            lines.push(PickerLine::plain(inner_w, BASE));
        }
    }

    // Trailing BASE blank for breathing room
    lines.push(PickerLine::plain(inner_w, BASE));

    // ── Clamp selection ──
    let max_sel = selectable_ids.len().saturating_sub(1);
    if app.device_picker_selected > max_sel {
        app.device_picker_selected = max_sel;
    }

    // ── Scroll: keep selected card visible ──
    // Find top/bottom lines for the selected card
    let mut sel_top: Option<usize> = None;
    let mut sel_bot: usize = 0;
    for (idx, l) in lines.iter().enumerate() {
        if l.click_target == Some(app.device_picker_selected) {
            if sel_top.is_none() {
                sel_top = Some(idx);
            }
            sel_bot = idx;
        }
    }
    let total_lines = lines.len();
    let visible_h = inner_h;
    if let Some(top) = sel_top {
        let scroll = app.device_picker_scroll;
        if top < scroll {
            app.device_picker_scroll = top;
        } else if sel_bot >= scroll + visible_h {
            app.device_picker_scroll = sel_bot + 1 - visible_h;
        }
    }
    let max_scroll = total_lines.saturating_sub(visible_h);
    if app.device_picker_scroll > max_scroll {
        app.device_picker_scroll = max_scroll;
    }
    let scroll_offset = app.device_picker_scroll;

    // ── Render block + content ──
    f.render_widget(block, picker_area);

    let mut click_regions: Vec<(u16, u16, u16, usize)> = Vec::new();
    let mut out_lines: Vec<Line> = Vec::with_capacity(visible_h);

    for row in 0..visible_h {
        let src_idx = scroll_offset + row;
        if src_idx >= total_lines {
            out_lines.push(Line::from(Span::styled(
                " ".repeat(inner_w),
                Style::default().bg(BASE),
            )));
            continue;
        }
        let l = &lines[src_idx];
        out_lines.push(Line::from(l.spans.clone()));

        if let Some(sel) = l.click_target {
            // Card spans from col device_gutter+card_indent to col dev_w - card_indent (within inner coords).
            let x_start = inner.x + device_gutter + card_indent;
            let x_end = inner.x + device_gutter + dev_w as u16 - card_indent;
            let y = inner.y + row as u16;
            click_regions.push((y, x_start, x_end, sel));
        }
    }

    f.render_widget(Paragraph::new(out_lines), inner);

    // Scrollbar
    if total_lines > visible_h {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::default().fg(SAPPHIRE))
            .track_style(Style::default().fg(SURFACE0));
        let mut state = ScrollbarState::new(max_scroll).position(scroll_offset);
        f.render_stateful_widget(scrollbar, inner, &mut state);
    }

    app.layout.device_picker_items = click_regions;
    app.layout.device_picker_rect = Some((
        picker_area.x,
        picker_area.y,
        picker_area.width,
        picker_area.height,
    ));
    app.layout.device_picker_item_ids = selectable_ids;
    app.layout.device_picker_total_lines = total_lines;
}

// ── Device picker helpers ──

fn shorten_id(id: &str) -> String {
    let w = id.width();
    if w <= 22 {
        return id.to_string();
    }
    // Keep leading 8 chars + ... + trailing 4 chars (byte-based is fine for hex UDIDs).
    let chars: Vec<char> = id.chars().collect();
    if chars.len() <= 14 {
        return id.to_string();
    }
    let head: String = chars.iter().take(8).collect();
    let tail: String = chars.iter().skip(chars.len() - 4).collect();
    format!("{}...{}", head, tail)
}

/// Device top border:  `╭─ [Tag] Name ───────── Conn · id ─╮`
// Phase 3 redesign — see Audit UI-015/UI-014: extract parameter struct.
#[allow(clippy::too_many_arguments)]
fn push_device_top(
    lines: &mut Vec<PickerLine>,
    inner_w: usize,
    dev_w: usize,
    gutter: u16,
    platform_tag: &str,
    name: &str,
    conn_label: &str,
    id_short: &str,
) {
    let bg = BASE;
    let dev_bg = MANTLE;
    let border_fg = SURFACE0;
    let text_fg = TEXT;
    let subtle_fg = OVERLAY0;

    let gutter_s = " ".repeat(gutter as usize);

    // Compose the top-edge title spans.
    //   left: "─ [Tag] Name "
    //   right: " Conn · id ─"
    // Both counted in dev_w between the corners.

    let tag_text = format!("[{}]", platform_tag);
    let tag_span_w = tag_text.width() + 1 + name.width(); // tag + space + name
    let right_text = format!("{} \u{00b7} {}", conn_label, id_short);
    let right_span_w = right_text.width();

    // Build a composed list of interior chars between `╭` and `╮` (width = dev_w - 2).
    let interior_w = dev_w.saturating_sub(2);
    let left_fixed = 3; // "─ " (── then space) — actually we use "── " then tag + " " + name + " "
    let right_fixed = 3; // " ──" — " " + "──"
    let dashes_needed =
        interior_w.saturating_sub(left_fixed + tag_span_w + right_fixed + right_span_w + 2); // +2 for spaces around right text

    // Assemble spans:
    let mut spans: Vec<Span<'static>> = vec![
        Span::styled(gutter_s.clone(), Style::default().bg(bg)),
        Span::styled(
            "\u{256d}".to_string(), // ╭
            Style::default().fg(border_fg).bg(dev_bg),
        ),
        // left fill: "── " then tag + " " + name + " "
        Span::styled(
            "\u{2500}\u{2500} ".to_string(),
            Style::default().fg(border_fg).bg(dev_bg),
        ),
        Span::styled(
            tag_text,
            Style::default()
                .fg(SAPPHIRE)
                .bg(dev_bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ".to_string(), Style::default().bg(dev_bg)),
        Span::styled(
            name.to_string(),
            Style::default()
                .fg(text_fg)
                .bg(dev_bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ".to_string(), Style::default().bg(dev_bg)),
        Span::styled(
            "\u{2500}".repeat(dashes_needed),
            Style::default().fg(border_fg).bg(dev_bg),
        ),
        Span::styled(" ".to_string(), Style::default().bg(dev_bg)),
        Span::styled(right_text, Style::default().fg(subtle_fg).bg(dev_bg)),
        Span::styled(" ".to_string(), Style::default().bg(dev_bg)),
        Span::styled(
            "\u{2500}\u{2500}".to_string(),
            Style::default().fg(border_fg).bg(dev_bg),
        ),
        Span::styled(
            "\u{256e}".to_string(), // ╮
            Style::default().fg(border_fg).bg(dev_bg),
        ),
    ];
    // Right gutter of BASE
    let used = gutter as usize + dev_w;
    if used < inner_w {
        spans.push(Span::styled(
            " ".repeat(inner_w - used),
            Style::default().bg(bg),
        ));
    }
    lines.push(PickerLine {
        spans,
        click_target: None,
    });
}

/// Device bottom border: `╰──...──╯`
fn push_device_bottom(lines: &mut Vec<PickerLine>, inner_w: usize, dev_w: usize, gutter: u16) {
    let bg = BASE;
    let dev_bg = MANTLE;
    let border_fg = SURFACE0;
    let gutter_s = " ".repeat(gutter as usize);

    let interior_w = dev_w.saturating_sub(2);
    let mut spans: Vec<Span<'static>> = vec![
        Span::styled(gutter_s, Style::default().bg(bg)),
        Span::styled(
            "\u{2570}".to_string(), // ╰
            Style::default().fg(border_fg).bg(dev_bg),
        ),
        Span::styled(
            "\u{2500}".repeat(interior_w),
            Style::default().fg(border_fg).bg(dev_bg),
        ),
        Span::styled(
            "\u{256f}".to_string(), // ╯
            Style::default().fg(border_fg).bg(dev_bg),
        ),
    ];
    let used = gutter as usize + dev_w;
    if used < inner_w {
        spans.push(Span::styled(
            " ".repeat(inner_w - used),
            Style::default().bg(bg),
        ));
    }
    lines.push(PickerLine {
        spans,
        click_target: None,
    });
}

/// Blank row inside a device container: `│` + MANTLE fill + `│`
fn push_device_inner_blank(lines: &mut Vec<PickerLine>, inner_w: usize, dev_w: usize, gutter: u16) {
    let bg = BASE;
    let dev_bg = MANTLE;
    let border_fg = SURFACE0;
    let gutter_s = " ".repeat(gutter as usize);

    let interior_w = dev_w.saturating_sub(2);
    let mut spans: Vec<Span<'static>> = vec![
        Span::styled(gutter_s, Style::default().bg(bg)),
        Span::styled(
            "\u{2502}".to_string(),
            Style::default().fg(border_fg).bg(dev_bg),
        ),
        Span::styled(" ".repeat(interior_w), Style::default().bg(dev_bg)),
        Span::styled(
            "\u{2502}".to_string(),
            Style::default().fg(border_fg).bg(dev_bg),
        ),
    ];
    let used = gutter as usize + dev_w;
    if used < inner_w {
        spans.push(Span::styled(
            " ".repeat(inner_w - used),
            Style::default().bg(bg),
        ));
    }
    lines.push(PickerLine {
        spans,
        click_target: None,
    });
}

/// Waiting row inside a device container.
fn push_waiting_row(lines: &mut Vec<PickerLine>, inner_w: usize, dev_w: usize, gutter: u16) {
    let bg = BASE;
    let dev_bg = MANTLE;
    let border_fg = SURFACE0;
    let gutter_s = " ".repeat(gutter as usize);

    let interior_w = dev_w.saturating_sub(2);
    let text = "\u{25cb} Waiting for app...";
    let text_w = text.width();
    let left_pad = 3usize;
    let right_pad = interior_w.saturating_sub(left_pad + text_w);

    let mut spans: Vec<Span<'static>> = vec![
        Span::styled(gutter_s, Style::default().bg(bg)),
        Span::styled(
            "\u{2502}".to_string(),
            Style::default().fg(border_fg).bg(dev_bg),
        ),
        Span::styled(" ".repeat(left_pad), Style::default().bg(dev_bg)),
        Span::styled(text.to_string(), Style::default().fg(OVERLAY0).bg(dev_bg)),
        Span::styled(" ".repeat(right_pad), Style::default().bg(dev_bg)),
        Span::styled(
            "\u{2502}".to_string(),
            Style::default().fg(border_fg).bg(dev_bg),
        ),
    ];
    let used = gutter as usize + dev_w;
    if used < inner_w {
        spans.push(Span::styled(
            " ".repeat(inner_w - used),
            Style::default().bg(bg),
        ));
    }
    lines.push(PickerLine {
        spans,
        click_target: None,
    });
}

/// Push 6 rows for an app card (top, name, Package, Platform, Mode, bottom),
/// each wrapped by the device container's `│` ... `│`.
#[allow(clippy::too_many_arguments)]
fn push_app_card(
    lines: &mut Vec<PickerLine>,
    inner_w: usize,
    dev_w: usize,
    gutter: u16,
    card_indent: u16,
    card_w: usize,
    sel_idx: usize,
    is_active: bool,
    is_selected: bool,
    app_name: &str,
    app_version: &str,
    package_name: &str,
    os: &str,
    build_mode: &str,
    port: u16,
) {
    let bg = BASE;
    let dev_bg = MANTLE;
    let dev_border_fg = SURFACE0;
    let gutter_s = " ".repeat(gutter as usize);
    let left_card_gutter = " ".repeat(card_indent as usize);
    let right_card_gutter = left_card_gutter.clone();

    // Selection preview cursor: SAPPHIRE ▎ in the last column of the left indent,
    // only when this card is SELECTED but not ACTIVE (active styling already distinguishes).
    let show_cursor = is_selected && !is_active;
    let cursor_pre_w = (card_indent as usize).saturating_sub(1);
    let push_left_indent = |spans: &mut Vec<Span<'static>>| {
        if show_cursor {
            if cursor_pre_w > 0 {
                spans.push(Span::styled(
                    " ".repeat(cursor_pre_w),
                    Style::default().bg(dev_bg),
                ));
            }
            spans.push(Span::styled(
                "\u{258e}".to_string(), // ▎
                Style::default().fg(SAPPHIRE).bg(dev_bg),
            ));
        } else {
            spans.push(Span::styled(
                left_card_gutter.clone(),
                Style::default().bg(dev_bg),
            ));
        }
    };

    // Border/bg choice per state
    // card_bg is always MANTLE — borders + pill + bold carry the ACTIVE distinction.
    let card_bg = MANTLE;
    let (card_border_fg, tl, tr, bl, br, h, v) = if is_active {
        (
            SAPPHIRE, '\u{2554}', // ╔
            '\u{2557}', // ╗
            '\u{255a}', // ╚
            '\u{255d}', // ╝
            '\u{2550}', // ═
            '\u{2551}', // ║
        )
    } else {
        (
            SURFACE0, '\u{250c}', // ┌
            '\u{2510}', // ┐
            '\u{2514}', // └
            '\u{2518}', // ┘
            '\u{2500}', // ─
            '\u{2502}', // │
        )
    };

    let value_fg = if is_active { TEXT } else { SUBTEXT0 };
    let label_fg = OVERLAY0;
    let unknown_fg = SURFACE1;

    // Helper to wrap a row of spans with device │ | card gutters | device │
    let device_border_span = Span::styled(
        "\u{2502}".to_string(),
        Style::default().fg(dev_border_fg).bg(dev_bg),
    );

    let right_tail_w = inner_w.saturating_sub(gutter as usize + dev_w);
    let right_tail = " ".repeat(right_tail_w);

    // ── Row 1: top border of card with embedded title ──
    // interior of card = card_w - 2 cells between corners
    let card_inner_w = card_w.saturating_sub(2);

    // Title composition:
    //   "─ ● AppName v1.2.3  [ACTIVE] ══════ Port: 9753 ═"
    // or for normal:
    //   "─ ○ AppName v1.2.3 ────────────── Port: 9754 ─"
    let dot = if is_active { "\u{25cf}" } else { "\u{25cb}" };
    let dot_fg = if is_active { GREEN } else { TEAL };

    let label = if app_version.is_empty() {
        app_name.to_string()
    } else {
        format!("{} v{}", app_name, app_version)
    };
    let active_pill = " ACTIVE "; // pill contents
    let port_text = format!("Port: {}", port);

    // left: "─ " + dot + " " + label  (+ "  [ACTIVE] " if active)
    let mut left_w = 3 /* "─ " + dot (dot char width 1) */ + 1 + label.width();
    if is_active {
        left_w += 2 + active_pill.width(); // "  " + pill
    }
    // right: " " + port + " ─"
    let right_w = 1 + port_text.width() + 2;
    let dashes = card_inner_w.saturating_sub(left_w + right_w);

    let mut top_spans: Vec<Span<'static>> = Vec::new();
    top_spans.push(Span::styled(gutter_s.clone(), Style::default().bg(bg)));
    top_spans.push(device_border_span.clone());
    push_left_indent(&mut top_spans);
    // card top-left corner
    top_spans.push(Span::styled(
        tl.to_string(),
        Style::default().fg(card_border_fg).bg(dev_bg),
    ));
    // "─ "
    top_spans.push(Span::styled(
        format!("{}{}", h, ' '),
        Style::default().fg(card_border_fg).bg(card_bg),
    ));
    // dot
    top_spans.push(Span::styled(
        dot.to_string(),
        Style::default()
            .fg(dot_fg)
            .bg(card_bg)
            .add_modifier(Modifier::BOLD),
    ));
    // " " + label
    top_spans.push(Span::styled(" ".to_string(), Style::default().bg(card_bg)));
    top_spans.push(Span::styled(
        label.clone(),
        Style::default()
            .fg(TEXT)
            .bg(card_bg)
            .add_modifier(Modifier::BOLD),
    ));
    if is_active {
        top_spans.push(Span::styled("  ".to_string(), Style::default().bg(card_bg)));
        top_spans.push(Span::styled(
            active_pill.to_string(),
            Style::default()
                .fg(MANTLE)
                .bg(GREEN)
                .add_modifier(Modifier::BOLD),
        ));
    }
    // dashes
    top_spans.push(Span::styled(
        h.to_string().repeat(dashes),
        Style::default().fg(card_border_fg).bg(card_bg),
    ));
    // " " + port
    top_spans.push(Span::styled(" ".to_string(), Style::default().bg(card_bg)));
    top_spans.push(Span::styled(
        port_text.clone(),
        Style::default().fg(SAPPHIRE).bg(card_bg),
    ));
    // " ─"
    top_spans.push(Span::styled(" ".to_string(), Style::default().bg(card_bg)));
    top_spans.push(Span::styled(
        h.to_string(),
        Style::default().fg(card_border_fg).bg(card_bg),
    ));
    // card top-right corner
    top_spans.push(Span::styled(
        tr.to_string(),
        Style::default().fg(card_border_fg).bg(dev_bg),
    ));
    top_spans.push(Span::styled(
        right_card_gutter.clone(),
        Style::default().bg(dev_bg),
    ));
    top_spans.push(device_border_span.clone());
    if !right_tail.is_empty() {
        top_spans.push(Span::styled(right_tail.clone(), Style::default().bg(bg)));
    }
    lines.push(PickerLine {
        spans: top_spans,
        click_target: Some(sel_idx),
    });

    // ── Rows 2-4: details rows (but we have 3 detail rows and no name-row —
    // actually the sketch has name in the TOP BORDER, so we have 3 detail rows,
    // which gives card height = 6: top + 3 details + 2 blanks? No — per spec:
    //   "6 rows (top border, name row, Package, Platform, Mode, bottom border)"
    // Wait — the spec says name row AND top border. But in the sketch the name
    // is embedded in the top edge. I'll follow the sketch since it's the actual
    // visual target: top-border-with-name + Package + Platform + Mode + bottom.
    // That's 5 rows. We add 1 blank row between top and details so the details
    // breathe → 6 rows total matches the spec.
    // ──
    let detail_rows: [(&str, &str); 3] = [
        ("Package", package_name),
        ("Platform", os),
        ("Mode", build_mode),
    ];

    // blank breathing row inside card (optional, for parity with spec's 6 rows)
    {
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.push(Span::styled(gutter_s.clone(), Style::default().bg(bg)));
        spans.push(device_border_span.clone());
        push_left_indent(&mut spans);
        spans.push(Span::styled(
            v.to_string(),
            Style::default().fg(card_border_fg).bg(card_bg),
        ));
        spans.push(Span::styled(
            " ".repeat(card_inner_w),
            Style::default().bg(card_bg),
        ));
        spans.push(Span::styled(
            v.to_string(),
            Style::default().fg(card_border_fg).bg(card_bg),
        ));
        spans.push(Span::styled(
            right_card_gutter.clone(),
            Style::default().bg(dev_bg),
        ));
        spans.push(device_border_span.clone());
        if !right_tail.is_empty() {
            spans.push(Span::styled(right_tail.clone(), Style::default().bg(bg)));
        }
        lines.push(PickerLine {
            spans,
            click_target: Some(sel_idx),
        });
    }

    // Skipping the breathing row changes our total to 6 — we already pushed 1 (top)
    // plus now 1 (blank); 3 details will bring us to 5; bottom = 6. Good.
    //
    // Hmm, the spec lists "name row" separately. We embed name in the top border,
    // which is visually more flush. Keep 6 rows by using: top + blank + 3 details + bottom = 6.

    for (lbl, val) in detail_rows {
        let label_padded = format!("{:<10}", lbl); // Platform(8)+2 = 10, aligns Package/Mode too
        let (display, fg) = if val.is_empty() {
            ("unknown".to_string(), unknown_fg)
        } else {
            (val.to_string(), value_fg)
        };
        // Inner content: "    " (4sp) + label_padded (10) + display
        let content_w = 4 + label_padded.width() + display.width();
        let pad_right = card_inner_w.saturating_sub(content_w);

        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.push(Span::styled(gutter_s.clone(), Style::default().bg(bg)));
        spans.push(device_border_span.clone());
        push_left_indent(&mut spans);
        spans.push(Span::styled(
            v.to_string(),
            Style::default().fg(card_border_fg).bg(card_bg),
        ));
        spans.push(Span::styled(
            "    ".to_string(),
            Style::default().bg(card_bg),
        ));
        spans.push(Span::styled(
            label_padded,
            Style::default().fg(label_fg).bg(card_bg),
        ));
        spans.push(Span::styled(
            display,
            Style::default()
                .fg(fg)
                .bg(card_bg)
                .add_modifier(if is_active {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ));
        spans.push(Span::styled(
            " ".repeat(pad_right),
            Style::default().bg(card_bg),
        ));
        spans.push(Span::styled(
            v.to_string(),
            Style::default().fg(card_border_fg).bg(card_bg),
        ));
        spans.push(Span::styled(
            right_card_gutter.clone(),
            Style::default().bg(dev_bg),
        ));
        spans.push(device_border_span.clone());
        if !right_tail.is_empty() {
            spans.push(Span::styled(right_tail.clone(), Style::default().bg(bg)));
        }
        lines.push(PickerLine {
            spans,
            click_target: Some(sel_idx),
        });
    }

    // ── Bottom border ──
    let mut bot_spans: Vec<Span<'static>> = Vec::new();
    bot_spans.push(Span::styled(gutter_s.clone(), Style::default().bg(bg)));
    bot_spans.push(device_border_span.clone());
    push_left_indent(&mut bot_spans);
    bot_spans.push(Span::styled(
        bl.to_string(),
        Style::default().fg(card_border_fg).bg(dev_bg),
    ));
    bot_spans.push(Span::styled(
        h.to_string().repeat(card_inner_w),
        Style::default().fg(card_border_fg).bg(card_bg),
    ));
    bot_spans.push(Span::styled(
        br.to_string(),
        Style::default().fg(card_border_fg).bg(dev_bg),
    ));
    bot_spans.push(Span::styled(
        right_card_gutter.clone(),
        Style::default().bg(dev_bg),
    ));
    bot_spans.push(device_border_span.clone());
    if !right_tail.is_empty() {
        bot_spans.push(Span::styled(right_tail.clone(), Style::default().bg(bg)));
    }
    lines.push(PickerLine {
        spans: bot_spans,
        click_target: Some(sel_idx),
    });
}
