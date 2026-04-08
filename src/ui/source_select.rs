//! Source selection UI — full-screen startup experience.
//!
//! Phases:
//! - ChooseType: animated banner + mode selection menu
//! - ScanningVm: text-based radar animation
//! - ScanningAdb: text-based signal wave animation
//! - PickVmService / PickAdbDevice: styled result list
//!
//! Also handles the dropdown source picker while connected.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use unicode_width::UnicodeWidthStr;

use crate::app::{App, SourceSelectPhase};

// ── Catppuccin Macchiato palette ──

const BASE: Color      = Color::Rgb(36, 39, 58);
const MANTLE: Color    = Color::Rgb(30, 32, 48);
const SURFACE0: Color  = Color::Rgb(54, 58, 79);
const SURFACE1: Color  = Color::Rgb(73, 77, 100);
const OVERLAY0: Color  = Color::Rgb(110, 115, 141);
const TEXT: Color       = Color::Rgb(202, 211, 245);
const BLUE: Color      = Color::Rgb(138, 173, 244);
const SAPPHIRE: Color  = Color::Rgb(125, 196, 228);
const TEAL: Color      = Color::Rgb(139, 213, 202);
const GREEN: Color     = Color::Rgb(166, 218, 149);
const YELLOW: Color    = Color::Rgb(238, 212, 159);
const PEACH: Color     = Color::Rgb(245, 169, 127);
const MAUVE: Color     = Color::Rgb(198, 160, 246);

// ── Banner ASCII art ──

const BANNER: &[&str] = &[
    " ╔═╗╦  ╔═╗╔═╗ ",
    " ╠╣ ║  ║ ║║ ╦ ",
    " ╚  ╩═╝╚═╝╚═╝ ",
];

const BANNER_COLORS: &[Color] = &[BLUE, SAPPHIRE, TEAL, BLUE, MAUVE, SAPPHIRE];

// ══════════════════════════════════════
//  Full-screen source selection (startup)
// ══════════════════════════════════════

pub fn draw_source_select(f: &mut Frame, app: &mut App, area: Rect) {
    f.render_widget(Block::default().style(Style::default().bg(BASE)), area);
    app.layout.source_select_items.clear();

    match &app.source_select.phase {
        Some(SourceSelectPhase::ChooseType) => draw_banner_with_menu(f, app, area),
        Some(SourceSelectPhase::ScanningVm) => draw_vm_scanning(f, app, area),
        Some(SourceSelectPhase::ScanningAdb) => draw_adb_scanning(f, app, area),
        Some(SourceSelectPhase::PickVmService(services)) => {
            let items: Vec<String> = services.iter()
                .map(|s| format!("{} ({})", s.name, extract_port(&s.ws_url)))
                .collect();
            draw_pick_list(f, app, area, "Select VM Service", &items);
        }
        Some(SourceSelectPhase::PickAdbDevice(devices)) => {
            let items: Vec<String> = devices.iter()
                .map(|d| format!("{} ({})", d.model, d.serial))
                .collect();
            draw_pick_list(f, app, area, "Select Device", &items);
        }
        None => {}
    }
}

// ══════════════════════════════════════
//  Phase 1: Banner + Mode Selection
// ══════════════════════════════════════

fn draw_banner_with_menu(f: &mut Frame, app: &mut App, area: Rect) {
    let tick = app.tick;
    let h = area.height as usize;
    let w = area.width as usize;

    let sel = app.source_select.selected_idx;

    // Card data: (icon, title, short_desc, detail_lines)
    let cards: &[(&str, &str, &str, &[&str])] = &[
        (
            "\u{25c8}", // ◈
            "VM Service",
            "Dart VM WebSocket",
            &[
                "Connect to Flutter app via Dart VM Service Protocol.",
                "Auto-discovers running Flutter processes on localhost.",
                "Best for: hot reload, debug logging, iOS Simulator.",
            ],
        ),
        (
            "\u{25a3}", // ▣
            "ADB",
            "Android Debug Bridge",
            &[
                "Stream logs from Android device via `adb logcat`.",
                "Supports USB-connected devices and emulators.",
                "Best for: native Android logs, release builds, crash traces.",
            ],
        ),
    ];

    // Layout sizing
    let banner_h = BANNER.len(); // 3
    let card_h = 3usize; // each card: border-top + content + border-bot (rendered inline)
    let cards_total = cards.len() * card_h + (cards.len() - 1); // cards + gaps between
    let detail_h = 5usize; // separator + detail lines
    let hint_h = 2usize;
    let total_content = banner_h + 2 + cards_total + 1 + detail_h + hint_h;
    let top_pad = h.saturating_sub(total_content) / 3;

    let menu_w = 54usize.min(w.saturating_sub(6));
    let menu_pad = w.saturating_sub(menu_w) / 2;

    let mut lines: Vec<Line> = Vec::new();

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

    // ── Cards ──
    for (i, &(icon, title, short_desc, _detail)) in cards.iter().enumerate() {
        let is_sel = i == sel;
        let card_bg = if is_sel { SURFACE0 } else { MANTLE };
        let border_color = if is_sel { BLUE } else { SURFACE0 };
        let title_color = if is_sel { BLUE } else { TEXT };
        let icon_color = if is_sel { BLUE } else { SURFACE1 };
        let desc_color = if is_sel { SAPPHIRE } else { OVERLAY0 };
        let marker = if is_sel { "\u{25b8} " } else { "  " };

        // Top border of card
        let corner_l = if is_sel { "┌" } else { "┌" };
        let corner_r = if is_sel { "┐" } else { "┐" };
        {
            let border_inner = "─".repeat(menu_w.saturating_sub(2));
            let mut spans = vec![
                Span::styled(" ".repeat(menu_pad), Style::default().bg(BASE)),
                Span::styled(format!("{}{}{}", corner_l, border_inner, corner_r), Style::default().fg(border_color).bg(BASE)),
            ];
            pad_line(&mut spans, w);
            lines.push(Line::from(spans));
        }

        // Card content row: │ ▸ ◈ VM Service    Dart VM WebSocket │
        {
            let inner_w = menu_w.saturating_sub(2);
            let text_content = format!("{}{} {}  {}", marker, icon, title, short_desc);
            let text_pad = inner_w.saturating_sub(text_content.width());

            let item_y = area.y + lines.len() as u16;
            let menu_x = menu_pad as u16 + area.x;
            app.layout.source_select_items.push((item_y, menu_x, menu_x + menu_w as u16, i));

            let mut spans = vec![
                Span::styled(" ".repeat(menu_pad), Style::default().bg(BASE)),
                Span::styled("│", Style::default().fg(border_color).bg(card_bg)),
                Span::styled(format!(" {}", marker), Style::default().fg(title_color).bg(card_bg)),
                Span::styled(format!("{} ", icon), Style::default().fg(icon_color).bg(card_bg).add_modifier(Modifier::BOLD)),
                Span::styled(format!("{}", title), Style::default().fg(title_color).bg(card_bg).add_modifier(Modifier::BOLD)),
                Span::styled("  ", Style::default().bg(card_bg)),
                Span::styled(short_desc.to_string(), Style::default().fg(desc_color).bg(card_bg)),
                Span::styled(" ".repeat(text_pad), Style::default().bg(card_bg)),
                Span::styled("│", Style::default().fg(border_color).bg(card_bg)),
            ];
            pad_line(&mut spans, w);
            lines.push(Line::from(spans));
        }

        // Bottom border of card
        {
            let border_inner = "─".repeat(menu_w.saturating_sub(2));
            let mut spans = vec![
                Span::styled(" ".repeat(menu_pad), Style::default().bg(BASE)),
                Span::styled(format!("└{}┘", border_inner), Style::default().fg(border_color).bg(BASE)),
            ];
            pad_line(&mut spans, w);
            lines.push(Line::from(spans));
        }

        // Gap between cards (not after last)
        if i < cards.len() - 1 {
            lines.push(fill_line(w));
        }
    }

    // ── Detail area for selected card ──
    lines.push(fill_line(w));

    // Thin separator
    {
        let sep_w = menu_w.saturating_sub(4);
        let sep = "╌".repeat(sep_w);
        let mut spans = vec![
            Span::styled(" ".repeat(menu_pad + 2), Style::default().bg(BASE)),
            Span::styled(sep, Style::default().fg(SURFACE0).bg(BASE)),
        ];
        pad_line(&mut spans, w);
        lines.push(Line::from(spans));
    }

    lines.push(fill_line(w));

    // Detail lines of currently selected card
    if let Some(&(_icon, _title, _short, detail)) = cards.get(sel) {
        for (di, &detail_line) in detail.iter().enumerate() {
            let fg = if di == detail.len() - 1 { TEAL } else { OVERLAY0 };
            let prefix = if di == 0 { "  " } else { "  " };
            let full = format!("{}{}", prefix, detail_line);
            let pad = menu_pad + 2;
            let mut spans = vec![
                Span::styled(" ".repeat(pad), Style::default().bg(BASE)),
                Span::styled(full, Style::default().fg(fg).bg(BASE)),
            ];
            pad_line(&mut spans, w);
            lines.push(Line::from(spans));
        }
    }

    lines.push(fill_line(w));

    // Hint line
    {
        let hint_pad = menu_pad + 2;
        let mut spans = vec![
            Span::styled(" ".repeat(hint_pad), Style::default().bg(BASE)),
            Span::styled("\u{2191}\u{2193}", Style::default().fg(BLUE).bg(BASE)),
            Span::styled(" navigate  ", Style::default().fg(OVERLAY0).bg(BASE)),
            Span::styled("Enter", Style::default().fg(BLUE).bg(BASE)),
            Span::styled(" select  ", Style::default().fg(OVERLAY0).bg(BASE)),
            Span::styled("q", Style::default().fg(BLUE).bg(BASE)),
            Span::styled(" quit", Style::default().fg(OVERLAY0).bg(BASE)),
        ];
        pad_line(&mut spans, w);
        lines.push(Line::from(spans));
    }

    // Fill remaining
    while lines.len() < h {
        lines.push(fill_line(w));
    }

    f.render_widget(Paragraph::new(lines).style(Style::default().bg(BASE)), area);
}

// ══════════════════════════════════════
//  Phase 2: VM Service — Text Radar
// ══════════════════════════════════════

fn draw_vm_scanning(f: &mut Frame, app: &mut App, area: Rect) {
    let tick = app.tick;
    let h = area.height as usize;
    let w = area.width as usize;

    // Text-based radar using concentric ring characters
    // The radar is drawn as lines of text, centered on screen
    let radar_size = 9usize; // radius in chars (half-height because chars are ~2:1)
    let radar_h = radar_size; // vertical radius in rows
    let radar_w = radar_size * 2; // horizontal radius in cols

    let total_content = radar_h * 2 + 1 + 5; // radar + text below
    let top_pad = h.saturating_sub(total_content) / 3;

    let mut lines: Vec<Line> = Vec::new();

    // Top padding
    for _ in 0..top_pad {
        lines.push(Line::from(Span::styled(" ".repeat(w), Style::default().bg(BASE))));
    }

    // Draw radar rows
    let angle = (tick as f64 * 0.1) % (2.0 * std::f64::consts::PI);
    let cx = radar_w as f64;
    let cy = radar_h as f64;

    for row in 0..=(radar_h * 2) {
        let ry = row as f64 - cy; // -cy..cy
        let mut row_chars: Vec<(char, Color)> = Vec::new();

        for col in 0..=(radar_w * 2) {
            let rx = col as f64 - cx; // -cx..cx
            // Normalize to circle space (compensate for char aspect ratio ~2:1)
            let nx = rx / cx;
            let ny = ry / cy;
            let dist = (nx * nx + ny * ny).sqrt();
            let char_angle = ny.atan2(nx);

            // Determine character and color
            let (ch, color) = radar_char(dist, char_angle, angle, tick);
            row_chars.push((ch, color));
        }

        // Build line with spans, centered
        let radar_total_w = radar_w * 2 + 1;
        let pad_left = w.saturating_sub(radar_total_w) / 2;

        let mut spans: Vec<Span> = vec![
            Span::styled(" ".repeat(pad_left), Style::default().bg(BASE)),
        ];

        // Group consecutive same-color chars into spans
        let mut current_str = String::new();
        let mut current_color = row_chars[0].1;
        for &(ch, color) in &row_chars {
            if color == current_color {
                current_str.push(ch);
            } else {
                spans.push(Span::styled(
                    std::mem::take(&mut current_str),
                    Style::default().fg(current_color).bg(BASE),
                ));
                current_str.push(ch);
                current_color = color;
            }
        }
        if !current_str.is_empty() {
            spans.push(Span::styled(current_str, Style::default().fg(current_color).bg(BASE)));
        }

        let used: usize = spans.iter().map(|s| s.content.width()).sum();
        if used < w {
            spans.push(Span::styled(" ".repeat(w - used), Style::default().bg(BASE)));
        }
        lines.push(Line::from(spans));
    }

    // Blank line
    lines.push(Line::from(Span::styled(" ".repeat(w), Style::default().bg(BASE))));

    // Spinner + label (centered)
    let spinner = braille_spinner(tick);
    let label = format!("{} Scanning for VM Services...", spinner);
    lines.push(centered_text_line(&label, w, TEXT));

    // Progress bar (centered)
    lines.push(render_progress_line(tick, w, BLUE));

    // Hint
    lines.push(centered_text_line("Run 'flutter run' in another terminal", w, OVERLAY0));

    // Esc hint
    {
        let esc_text = "Esc back";
        let pad = w.saturating_sub(esc_text.len()) / 2;
        let mut spans = vec![Span::styled(" ".repeat(pad), Style::default().bg(BASE))];
        spans.push(Span::styled("Esc", Style::default().fg(BLUE).bg(BASE)));
        spans.push(Span::styled(" back", Style::default().fg(OVERLAY0).bg(BASE)));
        let used: usize = spans.iter().map(|s| s.content.width()).sum();
        if used < w {
            spans.push(Span::styled(" ".repeat(w - used), Style::default().bg(BASE)));
        }
        lines.push(Line::from(spans));
    }

    // Fill remaining
    while lines.len() < h {
        lines.push(Line::from(Span::styled(" ".repeat(w), Style::default().bg(BASE))));
    }

    f.render_widget(Paragraph::new(lines).style(Style::default().bg(BASE)), area);
}

/// Determine char and color for a radar cell.
fn radar_char(dist: f64, char_angle: f64, sweep_angle: f64, tick: u64) -> (char, Color) {
    if dist > 1.05 {
        return (' ', BASE);
    }

    // Center dot
    if dist < 0.08 {
        return ('◉', BLUE);
    }

    // Concentric rings
    let ring_radii = [0.33, 0.66, 1.0];
    let mut on_ring = false;
    for &r in &ring_radii {
        if (dist - r).abs() < 0.06 {
            on_ring = true;
            break;
        }
    }

    // Sweep arm detection
    let mut angle_diff = (char_angle - sweep_angle).rem_euclid(2.0 * std::f64::consts::PI);
    if angle_diff > std::f64::consts::PI {
        angle_diff = 2.0 * std::f64::consts::PI - angle_diff;
    }

    // Sweep trail (wider = more visible)
    let trail_width = 0.4;
    let in_trail = angle_diff < trail_width;
    let trail_intensity = if in_trail { 1.0 - (angle_diff / trail_width) } else { 0.0 };

    // Pulse ring
    let pulse_r = ((tick as f64 * 0.06) % 1.2).min(1.0);
    let on_pulse = (dist - pulse_r).abs() < 0.05;

    if trail_intensity > 0.7 && dist > 0.1 {
        // Bright sweep
        if on_ring { return ('◆', BLUE); }
        return ('░', BLUE);
    } else if trail_intensity > 0.4 && dist > 0.1 {
        // Medium trail
        if on_ring { return ('◇', SAPPHIRE); }
        return ('░', SAPPHIRE);
    } else if trail_intensity > 0.1 && dist > 0.1 {
        // Fading trail
        return ('·', SURFACE1);
    } else if on_pulse {
        return ('·', SAPPHIRE);
    } else if on_ring {
        return ('·', SURFACE0);
    } else if dist < 1.0 {
        // Cross hairs at center
        if (dist > 0.1) && ((char_angle.abs() < 0.03) || ((char_angle - std::f64::consts::PI).abs() < 0.03)
            || ((char_angle - std::f64::consts::FRAC_PI_2).abs() < 0.03)
            || ((char_angle + std::f64::consts::FRAC_PI_2).abs() < 0.03)) {
            return ('·', SURFACE0);
        }
        return (' ', BASE);
    }

    (' ', BASE)
}

// ══════════════════════════════════════
//  Phase 3: ADB — Text Wave Animation
// ══════════════════════════════════════

fn draw_adb_scanning(f: &mut Frame, app: &mut App, area: Rect) {
    let tick = app.tick;
    let h = area.height as usize;
    let w = area.width as usize;

    // ADB animation: a device icon with radiating signal waves, all text-based
    let anim_h = 13usize;
    let total_content = anim_h + 5;
    let top_pad = h.saturating_sub(total_content) / 3;

    let mut lines: Vec<Line> = Vec::new();

    // Top padding
    for _ in 0..top_pad {
        lines.push(Line::from(Span::styled(" ".repeat(w), Style::default().bg(BASE))));
    }

    // Device + waves animation
    // The device is centered, with signal waves radiating from both sides
    let phase = (tick as f64 * 0.15) % 6.0;
    let phase2 = ((tick as f64 * 0.15) + 2.0) % 6.0;
    let phase3 = ((tick as f64 * 0.15) + 4.0) % 6.0;

    let device_art = [
        "  ╭───────╮  ",
        "  │ ┌───┐ │  ",
        "  │ │   │ │  ",
        "  │ │ ▶ │ │  ",
        "  │ │   │ │  ",
        "  │ └───┘ │  ",
        "  │  ───  │  ",
        "  ╰───────╯  ",
        "      │      ",
        "      │      ",
        "    ┌─┴─┐   ",
        "    │USB│   ",
        "    └───┘   ",
    ];

    // Build wave characters for each row
    for (row_idx, device_line) in device_art.iter().enumerate() {
        let dev_w = device_line.width();
        let pad_left = w.saturating_sub(dev_w + 20) / 2; // extra space for waves

        let mut spans: Vec<Span> = vec![
            Span::styled(" ".repeat(pad_left), Style::default().bg(BASE)),
        ];

        // Left waves (only for rows 1-6, the "screen" area)
        if row_idx >= 1 && row_idx <= 6 {
            let wave_str = wave_chars_at(row_idx as f64 - 3.5, phase, phase2, phase3, true);
            for (ch, color) in wave_str {
                spans.push(Span::styled(ch.to_string(), Style::default().fg(color).bg(BASE)));
            }
        } else {
            spans.push(Span::styled("          ", Style::default().bg(BASE)));
        }

        // Device line
        if row_idx == 3 {
            // The play button row
            let play_color = if (tick / 15) % 2 == 0 { GREEN } else { TEAL };
            // Split at the ▶ character
            let parts: Vec<&str> = device_line.splitn(3, '▶').collect();
            if parts.len() == 3 {
                spans.push(Span::styled(parts[0], Style::default().fg(SAPPHIRE).bg(BASE)));
                spans.push(Span::styled("▶", Style::default().fg(play_color).bg(BASE)));
                spans.push(Span::styled(parts[2], Style::default().fg(SAPPHIRE).bg(BASE)));
            } else {
                spans.push(Span::styled(*device_line, Style::default().fg(SAPPHIRE).bg(BASE)));
            }
        } else if row_idx >= 8 {
            // USB cable area
            let cable_color = if (tick / 10) % 3 == 0 { PEACH } else { YELLOW };
            spans.push(Span::styled(*device_line, Style::default().fg(cable_color).bg(BASE)));
        } else {
            spans.push(Span::styled(*device_line, Style::default().fg(SAPPHIRE).bg(BASE)));
        }

        // Right waves
        if row_idx >= 1 && row_idx <= 6 {
            let wave_str = wave_chars_at(row_idx as f64 - 3.5, phase, phase2, phase3, false);
            for (ch, color) in wave_str {
                spans.push(Span::styled(ch.to_string(), Style::default().fg(color).bg(BASE)));
            }
        }

        let used: usize = spans.iter().map(|s| s.content.width()).sum();
        if used < w {
            spans.push(Span::styled(" ".repeat(w - used), Style::default().bg(BASE)));
        }
        lines.push(Line::from(spans));
    }

    // Blank line
    lines.push(Line::from(Span::styled(" ".repeat(w), Style::default().bg(BASE))));

    // Spinner + label
    let spinner = braille_spinner(tick);
    let label = format!("{} Scanning for ADB devices...", spinner);
    lines.push(centered_text_line(&label, w, TEXT));

    // Progress bar
    lines.push(render_progress_line(tick, w, PEACH));

    // Hint
    lines.push(centered_text_line("Plug in your device or start an emulator", w, OVERLAY0));

    // Esc hint
    {
        let esc_text = "Esc back";
        let pad = w.saturating_sub(esc_text.len()) / 2;
        let mut spans = vec![Span::styled(" ".repeat(pad), Style::default().bg(BASE))];
        spans.push(Span::styled("Esc", Style::default().fg(PEACH).bg(BASE)));
        spans.push(Span::styled(" back", Style::default().fg(OVERLAY0).bg(BASE)));
        let used: usize = spans.iter().map(|s| s.content.width()).sum();
        if used < w {
            spans.push(Span::styled(" ".repeat(w - used), Style::default().bg(BASE)));
        }
        lines.push(Line::from(spans));
    }

    // Fill remaining
    while lines.len() < h {
        lines.push(Line::from(Span::styled(" ".repeat(w), Style::default().bg(BASE))));
    }

    f.render_widget(Paragraph::new(lines).style(Style::default().bg(BASE)), area);
}

/// Generate wave characters for a given vertical offset from center.
fn wave_chars_at(vert_offset: f64, phase: f64, phase2: f64, phase3: f64, is_left: bool) -> Vec<(char, Color)> {
    let mut result = Vec::new();
    let waves_w = 10;

    let phases = [phase, phase2, phase3];
    let wave_symbols: &[char] = if is_left {
        &['(', '(', '(']
    } else {
        &[')', ')', ')']
    };

    // Vertical attenuation: waves are strongest at center
    let vert_atten = 1.0 - (vert_offset.abs() / 4.0).min(1.0);
    if vert_atten < 0.1 {
        for _ in 0..waves_w {
            result.push((' ', BASE));
        }
        return result;
    }

    for i in 0..waves_w {
        let pos = if is_left { waves_w - 1 - i } else { i };
        let dist = pos as f64;

        // Check if any wave is at this position
        let mut ch = ' ';
        let mut color = BASE;

        for (wi, &p) in phases.iter().enumerate() {
            if p > 5.0 { continue; } // faded out
            let wave_pos = p * 1.8; // wave speed
            let wave_dist = (dist - wave_pos).abs();

            if wave_dist < 1.2 * vert_atten {
                let intensity = 1.0 - (wave_dist / (1.2 * vert_atten));
                if intensity > 0.6 {
                    ch = wave_symbols[wi];
                    color = BLUE;
                } else if intensity > 0.3 {
                    ch = wave_symbols[wi];
                    color = SAPPHIRE;
                } else {
                    ch = '·';
                    color = SURFACE1;
                }
                break;
            }
        }

        result.push((ch, color));
    }

    result
}

// ══════════════════════════════════════
//  Phase 4: Pick from list
// ══════════════════════════════════════

fn draw_pick_list(f: &mut Frame, app: &mut App, area: Rect, title: &str, items: &[String]) {
    let tick = app.tick;
    let h = area.height as usize;
    let w = area.width as usize;
    let sel = app.source_select.selected_idx;

    let mut lines: Vec<Line> = Vec::new();

    // Small banner at top
    let banner_h = BANNER.len();
    let list_h = items.len() + 4; // border + items + border + hint
    let total = banner_h + 2 + list_h;
    let top_pad = h.saturating_sub(total) / 3;

    for _ in 0..top_pad {
        lines.push(Line::from(Span::styled(" ".repeat(w), Style::default().bg(BASE))));
    }

    for (row, text) in BANNER.iter().enumerate() {
        lines.push(render_banner_line(text, row, tick, w));
    }

    lines.push(Line::from(Span::styled(" ".repeat(w), Style::default().bg(BASE))));
    lines.push(Line::from(Span::styled(" ".repeat(w), Style::default().bg(BASE))));

    // Panel
    let panel_w = 50usize.min(w.saturating_sub(4));
    let panel_pad = w.saturating_sub(panel_w) / 2;

    // Top border with title
    let title_str = format!(" {} ", title);
    let border_rem = panel_w.saturating_sub(2 + title_str.len());
    let border_top = format!(
        "{}┌{}{}{}┐",
        " ".repeat(panel_pad),
        title_str,
        "─".repeat(border_rem),
        ""
    );
    // Build properly
    {
        let mut spans = vec![Span::styled(" ".repeat(panel_pad), Style::default().bg(BASE))];
        spans.push(Span::styled("┌", Style::default().fg(SURFACE0).bg(BASE)));
        spans.push(Span::styled(&title_str, Style::default().fg(BLUE).bg(BASE).add_modifier(Modifier::BOLD)));
        let remaining = panel_w.saturating_sub(2 + title_str.width());
        spans.push(Span::styled("─".repeat(remaining), Style::default().fg(SURFACE0).bg(BASE)));
        spans.push(Span::styled("┐", Style::default().fg(SURFACE0).bg(BASE)));
        let used: usize = spans.iter().map(|s| s.content.width()).sum();
        if used < w { spans.push(Span::styled(" ".repeat(w - used), Style::default().bg(BASE))); }
        lines.push(Line::from(spans));
    }
    let _ = border_top; // suppress unused

    // Items
    for (i, item) in items.iter().enumerate() {
        let is_sel = i == sel;
        let row_bg = if is_sel { SURFACE0 } else { MANTLE };
        let marker = if is_sel { " \u{25b8} " } else { "   " };
        let style = if is_sel {
            Style::default().fg(BLUE).bg(row_bg).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT).bg(row_bg)
        };

        let content = format!("{}{}", marker, item);
        let inner_w = panel_w.saturating_sub(2);
        let content_pad = inner_w.saturating_sub(content.width());

        let item_y = area.y + lines.len() as u16;
        let panel_x = panel_pad as u16 + area.x;

        // Only register if visible
        if (lines.len()) < h.saturating_sub(2) {
            app.layout.source_select_items.push((item_y, panel_x, panel_x + panel_w as u16, i));
        }

        let mut spans = vec![
            Span::styled(" ".repeat(panel_pad), Style::default().bg(BASE)),
            Span::styled("│", Style::default().fg(SURFACE0).bg(MANTLE)),
            Span::styled(content, style),
            Span::styled(" ".repeat(content_pad), Style::default().bg(row_bg)),
            Span::styled("│", Style::default().fg(SURFACE0).bg(MANTLE)),
        ];
        let used: usize = spans.iter().map(|s| s.content.width()).sum();
        if used < w { spans.push(Span::styled(" ".repeat(w - used), Style::default().bg(BASE))); }
        lines.push(Line::from(spans));
    }

    // Hint row
    {
        let inner_w = panel_w.saturating_sub(2);
        let mut spans = vec![
            Span::styled(" ".repeat(panel_pad), Style::default().bg(BASE)),
            Span::styled("│", Style::default().fg(SURFACE0).bg(MANTLE)),
            Span::styled(" \u{2191}\u{2193}", Style::default().fg(BLUE).bg(MANTLE)),
            Span::styled(" navigate ", Style::default().fg(OVERLAY0).bg(MANTLE)),
            Span::styled("Enter", Style::default().fg(BLUE).bg(MANTLE)),
            Span::styled(" select ", Style::default().fg(OVERLAY0).bg(MANTLE)),
            Span::styled("Esc", Style::default().fg(BLUE).bg(MANTLE)),
            Span::styled(" back", Style::default().fg(OVERLAY0).bg(MANTLE)),
        ];
        let content_used: usize = spans.iter().skip(2).map(|s| s.content.width()).sum();
        let hint_pad = inner_w.saturating_sub(content_used);
        spans.push(Span::styled(" ".repeat(hint_pad), Style::default().bg(MANTLE)));
        spans.push(Span::styled("│", Style::default().fg(SURFACE0).bg(MANTLE)));
        let used: usize = spans.iter().map(|s| s.content.width()).sum();
        if used < w { spans.push(Span::styled(" ".repeat(w - used), Style::default().bg(BASE))); }
        lines.push(Line::from(spans));
    }

    // Bottom border
    {
        let mut spans = vec![Span::styled(" ".repeat(panel_pad), Style::default().bg(BASE))];
        spans.push(Span::styled(
            format!("└{}┘", "─".repeat(panel_w.saturating_sub(2))),
            Style::default().fg(SURFACE0).bg(BASE),
        ));
        let used: usize = spans.iter().map(|s| s.content.width()).sum();
        if used < w { spans.push(Span::styled(" ".repeat(w - used), Style::default().bg(BASE))); }
        lines.push(Line::from(spans));
    }

    // Fill remaining
    while lines.len() < h {
        lines.push(Line::from(Span::styled(" ".repeat(w), Style::default().bg(BASE))));
    }

    f.render_widget(Paragraph::new(lines).style(Style::default().bg(BASE)), area);
}

// ══════════════════════════════════════
//  Source Dropdown (WiFi-like, while connected)
// ══════════════════════════════════════

/// Number of visible item slots in the dropdown list.
const DROPDOWN_VISIBLE_ITEMS: usize = 5;

pub fn draw_source_dropdown(f: &mut Frame, app: &mut App, status_bar_y: u16) {
    let tick = app.tick;
    let tab = app.dropdown.tab;
    let sel = app.dropdown.selected;

    // Build list of items (excluding the currently connected one)
    let discovered: Vec<(String, String, bool)> = if tab == 0 {
        app.dropdown.discovered_vm.iter()
            .map(|s| {
                let is_current = app.connected && app.source_name.contains(&extract_port(&s.ws_url));
                (format!("{} :{}", s.name, extract_port(&s.ws_url)), s.ws_url.clone(), is_current)
            })
            .collect()
    } else {
        app.dropdown.discovered_adb.iter()
            .map(|d| {
                let is_current = app.connected && app.source_name.contains(&d.model);
                (format!("{} ({})", d.model, d.serial), d.serial.clone(), is_current)
            })
            .collect()
    };

    let selectable: Vec<(&str, &str)> = discovered.iter()
        .filter(|(_, _, is_current)| !is_current)
        .map(|(label, id, _)| (label.as_str(), id.as_str()))
        .collect();
    let selectable_count = selectable.len();

    // Clamp selected & compute scroll_offset to keep selected visible
    app.dropdown.selected = sel.min(selectable_count.saturating_sub(1));
    let sel = app.dropdown.selected;
    if sel < app.dropdown.scroll_offset {
        app.dropdown.scroll_offset = sel;
    } else if sel >= app.dropdown.scroll_offset + DROPDOWN_VISIBLE_ITEMS {
        app.dropdown.scroll_offset = sel + 1 - DROPDOWN_VISIBLE_ITEMS;
    }
    let scroll = app.dropdown.scroll_offset;

    // Fixed panel height:
    //   border(1) + tab(1) + connected(1) + separator(1)
    //   + 5 item slots + scanning(1) + border(1) = 12
    let panel_h = 12u16.min(status_bar_y);
    let panel_w = 56u16.min(f.area().width.saturating_sub(4));
    let panel_x = app.layout.source_info_x.0.min(f.area().width.saturating_sub(panel_w));
    let panel_y = status_bar_y.saturating_sub(panel_h);

    let panel = Rect::new(panel_x, panel_y, panel_w, panel_h);
    f.render_widget(Clear, panel);

    app.layout.dropdown_rect = Some((panel_x, panel_y, panel_w, panel_h));
    app.layout.dropdown_items.clear();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(SURFACE0))
        .style(Style::default().bg(MANTLE));

    let inner = block.inner(panel);
    f.render_widget(block, panel);

    let mut lines: Vec<Line> = Vec::new();

    // ── Tab row ──
    let vm_tab_style = if tab == 0 {
        Style::default().fg(MANTLE).bg(BLUE).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(OVERLAY0).bg(MANTLE)
    };
    let adb_tab_style = if tab == 1 {
        Style::default().fg(MANTLE).bg(BLUE).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(OVERLAY0).bg(MANTLE)
    };

    let tab_row_y = inner.y;
    let vm_tab_end = inner.x + 12;
    let adb_tab_start = inner.x + 13;
    let adb_tab_end = inner.x + 18;
    app.layout.dropdown_tab_row = Some((tab_row_y, vm_tab_end, adb_tab_start, adb_tab_end));

    lines.push(Line::from(vec![
        Span::styled(" VM Service ", vm_tab_style),
        Span::styled(" ", Style::default().bg(MANTLE)),
        Span::styled(" ADB ", adb_tab_style),
    ]));

    // ── Connected status ──
    if app.connected {
        lines.push(Line::from(vec![
            Span::styled(" \u{25cf} ", Style::default().fg(GREEN).bg(MANTLE)),
            Span::styled(&app.source_name, Style::default().fg(GREEN).bg(MANTLE)),
        ]));
    } else {
        lines.push(Line::from(Span::styled(" Not connected", Style::default().fg(OVERLAY0).bg(MANTLE))));
    }

    // ── Separator ──
    let sep_w = inner.width.saturating_sub(1) as usize;
    lines.push(Line::from(Span::styled(
        format!(" {}", "\u{2500}".repeat(sep_w.saturating_sub(1))),
        Style::default().fg(SURFACE0).bg(MANTLE),
    )));

    // ── Scrollable item list (5 visible slots) ──
    let visible_end = (scroll + DROPDOWN_VISIBLE_ITEMS).min(selectable_count);
    let has_above = scroll > 0;
    let has_below = visible_end < selectable_count;
    let mut rendered_items = 0usize;

    for si in scroll..visible_end {
        let (label, _) = selectable[si];
        let is_sel = si == sel;
        let marker = if is_sel { " \u{25b8} " } else { "   " };
        let style = if is_sel {
            Style::default().fg(BLUE).bg(SURFACE0).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT).bg(MANTLE)
        };

        let item_y = inner.y + lines.len() as u16;
        app.layout.dropdown_items.push((item_y, inner.x, inner.x + inner.width, si));
        lines.push(Line::from(Span::styled(format!("{}{}", marker, label), style)));
        rendered_items += 1;
    }

    // Fill remaining item slots with empty lines
    for slot in rendered_items..DROPDOWN_VISIBLE_ITEMS {
        if selectable_count == 0 && slot == 0 {
            lines.push(Line::from(Span::styled(" No devices found", Style::default().fg(OVERLAY0).bg(MANTLE))));
        } else {
            lines.push(Line::from(Span::styled("", Style::default().bg(MANTLE))));
        }
    }

    // ── Scanning indicator + scroll hints ──
    let spinner = braille_spinner(tick);
    let mut scan_spans = vec![
        Span::styled(format!(" {} ", spinner), Style::default().fg(BLUE).bg(MANTLE)),
        Span::styled("Scanning...", Style::default().fg(OVERLAY0).bg(MANTLE)),
    ];
    if has_above || has_below {
        let arrow = if has_above && has_below { " \u{2191}\u{2193}" }
            else if has_above { " \u{2191}" }
            else { " \u{2193}" };
        scan_spans.push(Span::styled(arrow, Style::default().fg(SURFACE1).bg(MANTLE)));
    }
    lines.push(Line::from(scan_spans));

    f.render_widget(Paragraph::new(lines).style(Style::default().bg(MANTLE)), inner);
}

// ══════════════════════════════════════
//  Shared Helpers
// ══════════════════════════════════════

/// A full-width empty line with BASE background.
fn fill_line(w: usize) -> Line<'static> {
    Line::from(Span::styled(" ".repeat(w), Style::default().bg(BASE)))
}

/// Pad the end of a span vec to fill the terminal width.
fn pad_line(spans: &mut Vec<Span<'static>>, w: usize) {
    let used: usize = spans.iter().map(|s| s.content.width()).sum();
    if used < w {
        spans.push(Span::styled(" ".repeat(w - used), Style::default().bg(BASE)));
    }
}

fn braille_spinner(tick: u64) -> &'static str {
    match (tick / 4) % 8 {
        0 => "\u{28fe}", 1 => "\u{28fd}", 2 => "\u{28fb}", 3 => "\u{28bf}",
        4 => "\u{287f}", 5 => "\u{28df}", 6 => "\u{28ef}", _ => "\u{28f7}",
    }
}

/// Render a banner line with animated gradient colors, centered.
fn render_banner_line(text: &str, row: usize, tick: u64, total_w: usize) -> Line<'static> {
    let banner_w = text.width();
    let pad_left = total_w.saturating_sub(banner_w) / 2;

    let mut spans: Vec<Span> = vec![
        Span::styled(" ".repeat(pad_left), Style::default().bg(BASE)),
    ];

    for (ci, ch) in text.chars().enumerate() {
        if ch == ' ' {
            spans.push(Span::styled(" ", Style::default().bg(BASE)));
        } else {
            let color_idx = (ci + row + tick as usize / 3) % BANNER_COLORS.len();
            spans.push(Span::styled(
                ch.to_string(),
                Style::default().fg(BANNER_COLORS[color_idx]).bg(BASE).add_modifier(Modifier::BOLD),
            ));
        }
    }

    let used: usize = spans.iter().map(|s| s.content.width()).sum();
    if used < total_w {
        spans.push(Span::styled(" ".repeat(total_w - used), Style::default().bg(BASE)));
    }

    Line::from(spans)
}

/// Create a centered text line with bg filled.
fn centered_text_line(text: &str, total_w: usize, fg: Color) -> Line<'static> {
    let pad = total_w.saturating_sub(text.len()) / 2;
    let mut spans = vec![
        Span::styled(" ".repeat(pad), Style::default().bg(BASE)),
        Span::styled(text.to_string(), Style::default().fg(fg).bg(BASE)),
    ];
    let used = pad + text.len();
    if used < total_w {
        spans.push(Span::styled(" ".repeat(total_w - used), Style::default().bg(BASE)));
    }
    Line::from(spans)
}

/// Render a centered animated progress bar line.
fn render_progress_line(tick: u64, total_w: usize, accent: Color) -> Line<'static> {
    let bar_w = 32usize.min(total_w.saturating_sub(8));
    let pad = total_w.saturating_sub(bar_w) / 2;

    // Sweep: a bright segment moves left to right
    let phase = ((tick as f64 * 0.05) % 1.0 * bar_w as f64) as usize;
    let glow_w = (bar_w / 5).max(3);

    let mut spans: Vec<Span> = vec![
        Span::styled(" ".repeat(pad), Style::default().bg(BASE)),
    ];

    for i in 0..bar_w {
        let dist = if i >= phase { i - phase } else { phase - i };
        let (ch, color) = if dist < glow_w / 3 {
            ('━', accent)
        } else if dist < glow_w {
            ('─', SURFACE1)
        } else {
            ('┈', SURFACE0)
        };
        spans.push(Span::styled(ch.to_string(), Style::default().fg(color).bg(BASE)));
    }

    let used: usize = spans.iter().map(|s| s.content.width()).sum();
    if used < total_w {
        spans.push(Span::styled(" ".repeat(total_w - used), Style::default().bg(BASE)));
    }
    Line::from(spans)
}

fn extract_port(ws_url: &str) -> String {
    ws_url.split(':').nth(2)
        .and_then(|s| s.split('/').next())
        .unwrap_or("?")
        .to_string()
}
