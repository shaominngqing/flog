//! Tab bar renderer — 1-row pill-style tab selector.

use super::{BLUE, GREEN, MANTLE, MAUVE, OVERLAY0, SUBTEXT0};
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
    if area.height < 1 {
        return;
    }

    let bg = MANTLE;
    let w = area.width as usize;

    // Active tab rendered as a solid pill; inactive as plain text.
    // Layout: "  " + [LogsPill] + "  " + [NetPill] + (pad) + right-side context

    let logs_active = app.active_tab == ViewTab::Logs;
    let net_active = app.active_tab == ViewTab::Network;

    // Pill styles
    let active_pill = Style::default()
        .fg(MANTLE)
        .bg(BLUE)
        .add_modifier(Modifier::BOLD);
    let inactive_text = Style::default().fg(OVERLAY0).bg(bg);

    let logs_text = if logs_active {
        " ▤ Logs "
    } else {
        "▤ Logs"
    };
    let net_text = if net_active {
        " ⇄ Network "
    } else {
        "⇄ Network"
    };

    let mut spans1: Vec<Span> = Vec::new();

    let logs_start_col = 2usize;
    spans1.push(Span::styled("  ", Style::default().bg(bg))); // 2-space left margin
    spans1.push(Span::styled(
        logs_text.to_string(),
        if logs_active {
            active_pill
        } else {
            inactive_text
        },
    ));
    let logs_end_col = logs_start_col + logs_text.width();

    spans1.push(Span::styled("  ", Style::default().bg(bg))); // gap between tabs
    let net_start_col = logs_end_col + 2;
    spans1.push(Span::styled(
        net_text.to_string(),
        if net_active {
            active_pill
        } else {
            inactive_text
        },
    ));
    let net_end_col = net_start_col + net_text.width();

    // Right-side: [Platform] AppName  (no LIVE, no underline)
    let active_app = app
        .active_app_id
        .as_ref()
        .and_then(|id| app.connected_apps.iter().find(|a| &a.id == id));

    let mut right_spans: Vec<Span> = Vec::new();
    if let Some(ca) = active_app {
        let (plat_label, plat_bg) = match ca.os.to_lowercase().as_str() {
            s if s.contains("android") => (" Android ", GREEN),
            s if s.contains("ios") => (" iOS ", BLUE),
            _ => (" Sim ", MAUVE),
        };
        right_spans.push(Span::styled(
            plat_label.to_string(),
            Style::default()
                .fg(MANTLE)
                .bg(plat_bg)
                .add_modifier(Modifier::BOLD),
        ));
        right_spans.push(Span::styled("  ", Style::default().bg(bg)));
        right_spans.push(Span::styled(
            ca.app_name.clone(),
            Style::default().fg(SUBTEXT0).bg(bg),
        ));
        right_spans.push(Span::styled("  ", Style::default().bg(bg))); // trailing pad
    }

    let used_left: usize = spans1.iter().map(|s| s.content.width()).sum();
    let used_right: usize = right_spans.iter().map(|s| s.content.width()).sum();
    let pad = w.saturating_sub(used_left + used_right);
    spans1.push(Span::styled(" ".repeat(pad), Style::default().bg(bg)));
    spans1.extend(right_spans);

    // Register click regions
    app.layout.tab_logs_x = (logs_start_col as u16, logs_end_col as u16);
    app.layout.tab_network_x = (net_start_col as u16, net_end_col as u16);

    // Render a single row (no underline row)
    f.render_widget(
        Paragraph::new(Line::from(spans1)).style(Style::default().bg(bg)),
        area,
    );
}
