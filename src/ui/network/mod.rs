//! Network Inspector view — placeholder.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use crate::app::App;
use super::{BASE, OVERLAY0, SURFACE1, BLUE};

pub fn draw_network(f: &mut Frame, _app: &mut App, area: Rect) {
    let mid_y = area.height / 2;
    let mut lines: Vec<Line> = Vec::new();
    for _ in 0..mid_y.saturating_sub(2) {
        lines.push(Line::raw(""));
    }
    lines.push(Line::from(Span::styled(
        "    Network Inspector",
        Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "    Add FlogHttpInterceptor to your Dio instance",
        Style::default().fg(OVERLAY0),
    )));
    lines.push(Line::from(Span::styled(
        "    to see network requests here.",
        Style::default().fg(SURFACE1),
    )));
    f.render_widget(Paragraph::new(lines).style(Style::default().bg(BASE)), area);
}
