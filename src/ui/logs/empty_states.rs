//! Empty-state placeholders + jump-to-bottom pill.
//!
//! * `draw_not_connected` — no device connected yet (shows Quick Start card).
//! * `draw_waiting_for_logs` — device connected but no logs received.
//! * `draw_no_matching_logs` — logs exist but the filter hides all of them.
//! * `draw_jump_to_bottom` — floating pill shown when scrolled back from tail.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::domain::LogLevel;

use super::jump;
use super::{safe_pad, BASE, BLUE, OVERLAY0, SAPPHIRE, SUBTEXT0, SURFACE0, SURFACE1, TEXT, YELLOW};

// ── ASCII banner ──

const LOGO: [&str; 6] = [
    r"███████╗██╗      ██████╗  ██████╗ ",
    r"██╔════╝██║     ██╔═══██╗██╔════╝ ",
    r"█████╗  ██║     ██║   ██║██║  ███╗",
    r"██╔══╝  ██║     ██║   ██║██║   ██║",
    r"██║     ███████╗╚██████╔╝╚██████╔╝",
    r"╚═╝     ╚══════╝ ╚═════╝  ╚═════╝ ",
];

/// Gradient colors: Catppuccin Macchiato blue → teal → green
const GRAD: [Color; 6] = [
    Color::Rgb(138, 173, 244), // blue
    Color::Rgb(125, 196, 228), // sapphire
    Color::Rgb(139, 213, 202), // teal
    Color::Rgb(166, 218, 149), // green
    Color::Rgb(139, 213, 202), // teal
    Color::Rgb(125, 196, 228), // sapphire
];

fn gradient_line(text: &str) -> Line<'static> {
    let spans: Vec<Span<'static>> = text
        .chars()
        .enumerate()
        .map(|(i, ch)| {
            let color = GRAD[i % GRAD.len()];
            Span::styled(
                ch.to_string(),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            )
        })
        .collect();
    Line::from(spans)
}

fn logo_lines() -> Vec<Line<'static>> {
    LOGO.iter().map(|l| gradient_line(l)).collect()
}

pub(super) fn draw_jump_to_bottom(f: &mut Frame, app: &mut App, area: Rect) {
    if !jump::should_show(app.logs.auto_scroll) {
        app.layout.jump_to_bottom_rect = None;
        return;
    }
    // Skip on empty list — nothing to jump over, and the pill would overlap
    // the "No matching logs" / "Quick Start" empty-state cards.
    if app.filtered_count() == 0 {
        app.layout.jump_to_bottom_rect = None;
        return;
    }
    if area.height < 5 || area.width < 24 {
        app.layout.jump_to_bottom_rect = None;
        return;
    }

    let label_text = jump::label(app.new_logs_since_pause);
    let pill_w = (label_text.width() as u16 + 2).min(area.width.saturating_sub(4));
    let pill_h: u16 = 3;
    let pill_x = area.x + (area.width.saturating_sub(pill_w)) / 2;
    let pill_y = area.y + area.height.saturating_sub(pill_h + 1);

    let border_style = Style::default().fg(SAPPHIRE).bg(BASE);
    let top = format!("╭{}╮", "─".repeat((pill_w - 2) as usize));
    let bot = format!("╰{}╯", "─".repeat((pill_w - 2) as usize));

    let mid = if app.new_logs_since_pause > 0 {
        let total_inner = (pill_w - 2) as usize;
        let base_text = "  ↓ Jump to bottom  ";
        let new_text = format!("{} new  ", app.new_logs_since_pause);
        let used = base_text.width() + new_text.width();
        let pad = total_inner.saturating_sub(used);
        vec![
            Span::styled("│", border_style),
            Span::styled(base_text.to_string(), Style::default().fg(TEXT).bg(BASE)),
            Span::styled(new_text, Style::default().fg(YELLOW).bg(BASE)),
            Span::styled(" ".repeat(pad), Style::default().bg(BASE)),
            Span::styled("│", border_style),
        ]
    } else {
        let total_inner = (pill_w - 2) as usize;
        let base_text = "  ↓ Jump to bottom  ";
        let pad = total_inner.saturating_sub(base_text.width());
        vec![
            Span::styled("│", border_style),
            Span::styled(base_text.to_string(), Style::default().fg(TEXT).bg(BASE)),
            Span::styled(" ".repeat(pad), Style::default().bg(BASE)),
            Span::styled("│", border_style),
        ]
    };

    let pill_area = Rect::new(pill_x, pill_y, pill_w, pill_h);
    let lines = vec![
        Line::from(Span::styled(top, border_style)),
        Line::from(mid),
        Line::from(Span::styled(bot, border_style)),
    ];
    f.render_widget(
        Paragraph::new(lines).style(Style::default().bg(BASE)),
        pill_area,
    );

    app.layout.jump_to_bottom_rect = Some((pill_x, pill_y, pill_w, pill_h));
}

pub(super) fn draw_not_connected(f: &mut Frame, _app: &mut App, area: Rect) {
    let logo_h = LOGO.len() as u16 + 13; // logo + spacing + subtitle + spacer + Quick Start card (7)
    let start_y = area.height.saturating_sub(logo_h) / 2;

    let mut lines: Vec<Line> = Vec::new();
    for _ in 0..start_y {
        lines.push(Line::raw(""));
    }

    for logo_line in logo_lines() {
        lines.push(logo_line);
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "   Flutter Log Viewer · Network Inspector",
        Style::default().fg(OVERLAY0),
    )));
    lines.push(Line::raw(""));

    // Quick Start bordered card
    let indent = "    ";
    let card_w = 46usize;
    let top = format!("{}┌{}┐", indent, "─".repeat(card_w - 2));
    let bot = format!("{}└{}┘", indent, "─".repeat(card_w - 2));
    let border_style = Style::default().fg(SURFACE0);
    let content_fg = Style::default().fg(SUBTEXT0);

    let card_row = |text: &str| -> Line<'static> {
        let inner_w = card_w - 2;
        let pad = inner_w.saturating_sub(text.width());
        Line::from(vec![
            Span::styled(indent.to_string(), Style::default()),
            Span::styled("│".to_string(), border_style),
            Span::styled(text.to_string(), content_fg),
            Span::styled(" ".repeat(pad), Style::default()),
            Span::styled("│".to_string(), border_style),
        ])
    };

    lines.push(Line::from(Span::styled(top, border_style)));
    lines.push(card_row("  Quick Start                               "));
    lines.push(card_row("   1. Add flog_dart to your Flutter app     "));
    lines.push(card_row("   2. Run your app in debug mode            "));
    lines.push(card_row("   3. flog will auto-connect                "));
    lines.push(card_row("                                            "));
    lines.push(Line::from(Span::styled(bot, border_style)));

    f.render_widget(Paragraph::new(lines).style(Style::default().bg(BASE)), area);
}

pub(super) fn draw_waiting_for_logs(f: &mut Frame, app: &mut App, area: Rect) {
    let tick = app.tick;
    let logo_h = LOGO.len() as u16 + 5;
    let start_y = area.height.saturating_sub(logo_h) / 2;

    let spinner = match (tick / 5) % 8 {
        0 => "⣾",
        1 => "⣽",
        2 => "⣻",
        3 => "⢿",
        4 => "⡿",
        5 => "⣟",
        6 => "⣯",
        _ => "⣷",
    };

    let mut lines: Vec<Line> = Vec::new();
    for _ in 0..start_y {
        lines.push(Line::raw(""));
    }

    for logo_line in logo_lines() {
        lines.push(logo_line);
    }
    lines.push(Line::raw(""));

    let subtitle = app
        .active_app_id
        .as_ref()
        .and_then(|id| app.connected_apps.iter().find(|a| &a.id == id))
        .map(|ca| {
            let version = if ca.app_version.is_empty() {
                String::new()
            } else {
                format!(" v{}", ca.app_version)
            };
            format!("   Connected · {}{} ({})", ca.app_name, version, ca.os)
        })
        .unwrap_or_else(|| "   Flutter Log Viewer".to_string());

    lines.push(Line::from(Span::styled(
        subtitle,
        Style::default().fg(OVERLAY0),
    )));
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(format!("   {}  ", spinner), Style::default().fg(BLUE)),
        Span::styled("Waiting for logs...", Style::default().fg(SUBTEXT0)),
    ]));

    f.render_widget(Paragraph::new(lines).style(Style::default().bg(BASE)), area);
}

pub(super) fn draw_no_matching_logs(f: &mut Frame, app: &App, area: Rect) {
    let mid = area.height / 2;
    let mut lines: Vec<Line> = Vec::new();
    for _ in 0..mid.saturating_sub(4) {
        lines.push(Line::raw(""));
    }

    lines.push(Line::from(Span::styled(
        "          \u{2205}",
        Style::default().fg(SURFACE1),
    )));
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "    No matching logs",
        Style::default().fg(OVERLAY0),
    )));
    lines.push(Line::from(Span::styled(
        "    Try adjusting filters or level",
        Style::default().fg(SURFACE1),
    )));
    lines.push(Line::raw(""));

    let mut filter_rows: Vec<String> = Vec::new();
    if !app.filter.search_query.is_empty() {
        filter_rows.push(format!("    search: \"{}\"", app.filter.search_query));
    }
    if !app.filter.exclude_query.is_empty() {
        filter_rows.push(format!("    exclude: \"{}\"", app.filter.exclude_query));
    }
    if app.filter.min_level != LogLevel::System {
        filter_rows.push(format!("    level:  {}+", app.filter.min_level.as_str()));
    }
    let tag_includes: Vec<String> = app
        .filter
        .tag_include
        .iter()
        .map(|t| format!("+{}", t))
        .collect();
    let tag_excludes: Vec<String> = app
        .filter
        .tag_exclude
        .iter()
        .map(|t| format!("-{}", t))
        .collect();
    if !tag_includes.is_empty() || !tag_excludes.is_empty() {
        let combined: Vec<String> = tag_includes.into_iter().chain(tag_excludes).collect();
        filter_rows.push(format!("    tags:   {}", combined.join(" ")));
    }

    if !filter_rows.is_empty() {
        lines.push(Line::from(Span::styled(
            "    ┌─ Active filters ─────────────────┐",
            Style::default().fg(SURFACE0),
        )));
        for r in &filter_rows {
            lines.push(Line::from(vec![
                Span::styled("    │", Style::default().fg(SURFACE0)),
                Span::styled(safe_pad(r, 34), Style::default().fg(SUBTEXT0)),
                Span::styled("│", Style::default().fg(SURFACE0)),
            ]));
        }
        lines.push(Line::from(Span::styled(
            "    └──────────────────────────────────┘",
            Style::default().fg(SURFACE0),
        )));
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "    press esc to clear all",
            Style::default().fg(OVERLAY0),
        )));
    }

    f.render_widget(Paragraph::new(lines).style(Style::default().bg(BASE)), area);
}
