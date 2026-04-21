//! Tab bar renderer — prominent 2-line tab selector with ASCII icons.

use super::{BLUE, GREEN, MANTLE, OVERLAY0, SUBTEXT0, YELLOW};
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

    let logs_icon = "▤";
    let net_icon = "⇄";

    let (logs_label_style, logs_icon_style) = if app.active_tab == ViewTab::Logs {
        (
            Style::default().fg(BLUE).bg(bg).add_modifier(Modifier::BOLD),
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
            Style::default().fg(BLUE).bg(bg).add_modifier(Modifier::BOLD),
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
        Span::styled("        ", Style::default().bg(bg)),
        Span::styled(net_icon, net_icon_style),
        Span::styled(" Network", net_label_style),
    ];
    let used_left: usize = spans1.iter().map(|s| s.content.width()).sum();

    // Right-side context: "AppName vX.Y · Device  ● LIVE"
    let active_app = app
        .active_app_id
        .as_ref()
        .and_then(|id| app.connected_apps.iter().find(|a| &a.id == id));

    let mut right_spans: Vec<Span> = Vec::new();
    if let Some(ca) = active_app {
        let app_label = if ca.app_version.is_empty() {
            ca.app_name.clone()
        } else {
            format!("{} v{}", ca.app_name, ca.app_version)
        };
        let device_short = if ca.device_name.is_empty() {
            ca.device_id.clone()
        } else {
            ca.device_name.clone()
        };
        right_spans.push(Span::styled(
            format!("{} · {}", app_label, device_short),
            Style::default().fg(SUBTEXT0).bg(bg),
        ));
        right_spans.push(Span::styled("  ", Style::default().bg(bg)));

        if app.auto_scroll {
            let dot = match (app.tick / 8) % 4 {
                0 => "●",
                1 => "◉",
                2 => "●",
                _ => "○",
            };
            right_spans.push(Span::styled(
                format!(" {} LIVE ", dot),
                Style::default()
                    .fg(MANTLE)
                    .bg(GREEN)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            right_spans.push(Span::styled(
                " PAUSED ".to_string(),
                Style::default()
                    .fg(MANTLE)
                    .bg(YELLOW)
                    .add_modifier(Modifier::BOLD),
            ));
        }
    }

    let used_right: usize = right_spans.iter().map(|s| s.content.width()).sum();

    let pad = w.saturating_sub(used_left + used_right);
    spans1.push(Span::styled(" ".repeat(pad), Style::default().bg(bg)));
    spans1.extend(right_spans);

    // Line 2: underline under active tab
    let logs_start: usize = 3;
    let logs_end: usize = 9;
    let net_start: usize = 17;
    let net_end: usize = 26;

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

    app.layout.tab_logs_x = (logs_start as u16, logs_end as u16);
    app.layout.tab_network_x = (net_start as u16, net_end as u16);

    let lines = vec![Line::from(spans1), Line::from(spans2)];
    f.render_widget(Paragraph::new(lines).style(Style::default().bg(bg)), area);
}
