//! Tab bar renderer.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;
use crate::app::{App, ViewTab};
use super::{MANTLE, BLUE, OVERLAY0};

pub fn draw_tab_bar(f: &mut Frame, app: &mut App, area: Rect) {
    let bg = MANTLE;
    let logs_style = if app.active_tab == ViewTab::Logs {
        Style::default().fg(BLUE).bg(bg).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        Style::default().fg(OVERLAY0).bg(bg)
    };
    let net_style = if app.active_tab == ViewTab::Network {
        Style::default().fg(BLUE).bg(bg).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        Style::default().fg(OVERLAY0).bg(bg)
    };
    let mut spans = vec![
        Span::styled("  ", Style::default().bg(bg)),
        Span::styled(" Logs ", logs_style),
        Span::styled("    ", Style::default().bg(bg)),
        Span::styled(" Network ", net_style),
    ];
    let used: usize = spans.iter().map(|s| s.content.width()).sum();
    let rem = (area.width as usize).saturating_sub(used);
    if rem > 0 {
        spans.push(Span::styled(" ".repeat(rem), Style::default().bg(bg)));
    }
    app.layout.tab_logs_x = (2, 8);
    app.layout.tab_network_x = (12, 21);
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}
