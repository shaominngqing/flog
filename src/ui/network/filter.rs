//! Network toolbar renderer — URL filter, protocol/method/status dropdowns.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::App;
use crate::domain::network_filter::{MethodFilter, ProtocolFilter, StatusFilter};
use crate::ui::safe_pad;

// Import shared palette from parent
use super::super::{
    MANTLE, SURFACE1, OVERLAY0, TEXT,
    BLUE, GREEN, YELLOW, PEACH, RED, SAPPHIRE,
};

pub fn draw_network_toolbar(f: &mut Frame, app: &mut App, area: Rect) {
    let bg = MANTLE;

    // Tab selector (replaces logo)
    let (tab_spans, tab_w) = super::super::tab_bar::tab_spans(app, bg);
    let mut spans: Vec<Span> = tab_spans;
    let mut x: u16 = tab_w;

    // URL search
    let sw: usize = 24;
    let search_text = if app.network.filter.search.is_empty() {
        "filter url...".to_string()
    } else {
        app.network.filter.search.clone()
    };
    let ss = if app.network.filter.search.is_empty() {
        Style::default().fg(OVERLAY0).bg(bg)
    } else {
        Style::default().fg(YELLOW).bg(bg)
    };
    spans.push(Span::styled("/", Style::default().fg(OVERLAY0).bg(bg)));
    x += 1;
    spans.push(Span::styled(safe_pad(&search_text, sw), ss));
    x += sw as u16;
    spans.push(Span::styled("  ", Style::default().bg(bg)));
    x += 2;

    // Protocol filter
    let proto_label = app.network.filter.protocol.as_str();
    let proto_style = if app.network.filter.protocol != ProtocolFilter::All {
        Style::default().fg(MANTLE).bg(BLUE).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(OVERLAY0).bg(bg)
    };
    spans.push(Span::styled(format!(" {} ", proto_label), proto_style));
    x += proto_label.len() as u16 + 2;
    spans.push(Span::styled(" ", Style::default().bg(bg)));
    x += 1;

    // Method filter
    let method_label = app.network.filter.method.as_str();
    let method_style = if app.network.filter.method != MethodFilter::All {
        Style::default().fg(MANTLE).bg(GREEN).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(OVERLAY0).bg(bg)
    };
    spans.push(Span::styled(format!(" {} ", method_label), method_style));
    x += method_label.len() as u16 + 2;
    spans.push(Span::styled(" ", Style::default().bg(bg)));
    x += 1;

    // Status filter
    let status_label = app.network.filter.status.as_str();
    let status_style = match app.network.filter.status {
        StatusFilter::All => Style::default().fg(OVERLAY0).bg(bg),
        StatusFilter::Failed => Style::default().fg(MANTLE).bg(RED).add_modifier(Modifier::BOLD),
        StatusFilter::Completed => Style::default().fg(MANTLE).bg(GREEN).add_modifier(Modifier::BOLD),
        StatusFilter::Active => Style::default().fg(MANTLE).bg(PEACH).add_modifier(Modifier::BOLD),
        StatusFilter::Pending => Style::default().fg(TEXT).bg(SURFACE1),
    };
    spans.push(Span::styled(format!(" {} ", status_label), status_style));
    x += status_label.len() as u16 + 2;

    // Fill remaining
    let rem = area.width.saturating_sub(x);
    if rem > 0 {
        spans.push(Span::styled(" ".repeat(rem as usize), Style::default().bg(bg)));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}
