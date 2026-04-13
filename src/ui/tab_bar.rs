//! Tab bar renderer — prominent 2-line tab selector with ASCII icons.

use super::{BLUE, MANTLE, OVERLAY0};
use crate::app::{App, ViewTab};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use unicode_width::UnicodeWidthStr;

pub fn draw_tab_bar(f: &mut Frame, app: &mut App, area: Rect) {
    if area.height < 2 {
        return;
    }

    let bg = MANTLE;
    let w = area.width as usize;

    // ── Line 1: Tab labels with icons ──

    let logs_icon = "▤"; // list icon
    let net_icon = "⇄"; // exchange icon

    let (logs_label_style, logs_icon_style) = if app.active_tab == ViewTab::Logs {
        (
            Style::default()
                .fg(BLUE)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
            Style::default().fg(BLUE).bg(bg),
        )
    } else {
        (
            Style::default().fg(OVERLAY0).bg(bg),
            Style::default().fg(OVERLAY0).bg(bg),
        )
    };

    let (net_label_style, net_icon_style) = if app.active_tab == ViewTab::Network {
        (
            Style::default()
                .fg(BLUE)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
            Style::default().fg(BLUE).bg(bg),
        )
    } else {
        (
            Style::default().fg(OVERLAY0).bg(bg),
            Style::default().fg(OVERLAY0).bg(bg),
        )
    };

    let mut spans1: Vec<Span> = vec![
        Span::styled("   ", Style::default().bg(bg)),
        Span::styled(logs_icon, logs_icon_style),
        Span::styled(" Logs", logs_label_style),
        Span::styled("        ", Style::default().bg(bg)), // spacer between tabs
        Span::styled(net_icon, net_icon_style),
        Span::styled(" Network", net_label_style),
    ];
    let used1: usize = spans1.iter().map(|s| s.content.width()).sum();
    if used1 < w {
        spans1.push(Span::styled(" ".repeat(w - used1), Style::default().bg(bg)));
    }

    // ── Line 2: Active indicator underline ──

    // "   ▤ Logs" = 3 + 1 + 5 = 9 chars for logs tab
    // "        ⇄ Network" starts at 9 + 8 = 17
    let logs_start: usize = 3;
    let logs_end: usize = 9; // "▤ Logs" = 6 chars, starts at 3
    let net_start: usize = 17;
    let net_end: usize = 26; // "⇄ Network" = 9 chars, starts at 17

    let mut line2 = String::with_capacity(w);
    for i in 0..w {
        let is_logs_active = app.active_tab == ViewTab::Logs && i >= logs_start && i < logs_end;
        let is_net_active = app.active_tab == ViewTab::Network && i >= net_start && i < net_end;
        if is_logs_active || is_net_active {
            line2.push('─');
        } else {
            line2.push(' ');
        }
    }

    let underline_style = Style::default().fg(BLUE).bg(bg);
    let bg_style = Style::default().bg(bg);

    // Build line2 with colored segments
    let mut spans2: Vec<Span> = Vec::new();
    let chars: Vec<char> = line2.chars().collect();
    let mut pos = 0;
    while pos < chars.len() {
        let is_dash = chars[pos] == '─';
        let start = pos;
        while pos < chars.len() && (chars[pos] == '─') == is_dash {
            pos += 1;
        }
        let segment: String = chars[start..pos].iter().collect();
        if is_dash {
            spans2.push(Span::styled(segment, underline_style));
        } else {
            spans2.push(Span::styled(segment, bg_style));
        }
    }

    // Store click regions (line 0 of the tab bar area)
    app.layout.tab_logs_x = (logs_start as u16, logs_end as u16);
    app.layout.tab_network_x = (net_start as u16, net_end as u16);

    let lines = vec![Line::from(spans1), Line::from(spans2)];

    f.render_widget(Paragraph::new(lines).style(Style::default().bg(bg)), area);
}
