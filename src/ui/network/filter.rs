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
    let mut spans: Vec<Span> = Vec::new();
    let mut x: u16 = 0;
    let is_searching = app.network.search_active;

    // Logo
    spans.push(Span::styled(" NET ", Style::default().fg(MANTLE).bg(SAPPHIRE).add_modifier(Modifier::BOLD)));
    x += 5;
    spans.push(Span::styled(" ", Style::default().bg(bg)));
    x += 1;

    // URL search
    let search_start = x;
    let sw: usize = 24;
    let search_text = if is_searching {
        format!("{}_", app.network.search_input)
    } else if app.network.filter.search.is_empty() {
        "/filter url...".to_string()
    } else {
        format!("/{}", app.network.filter.search)
    };
    let ss = if is_searching {
        Style::default().fg(TEXT).bg(SURFACE1)
    } else if app.network.filter.search.is_empty() {
        Style::default().fg(OVERLAY0).bg(bg)
    } else {
        Style::default().fg(YELLOW).bg(bg)
    };
    spans.push(Span::styled(safe_pad(&search_text, sw), ss));
    x += sw as u16;
    let search_end = x;
    spans.push(Span::styled("  ", Style::default().bg(bg)));
    x += 2;

    // Protocol filter — click to cycle
    let proto_start = x;
    let proto_label = format!(" {} \u{25be}", app.network.filter.protocol.as_str()); // ▾
    let proto_style = if app.network.filter.protocol != ProtocolFilter::All {
        Style::default().fg(MANTLE).bg(BLUE).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(OVERLAY0).bg(bg)
    };
    spans.push(Span::styled(&proto_label, proto_style));
    x += proto_label.len() as u16;
    let proto_end = x;
    spans.push(Span::styled("  ", Style::default().bg(bg)));
    x += 2;

    // Method filter — click to cycle
    let method_start = x;
    let method_label = format!(" {} \u{25be}", app.network.filter.method.as_str());
    let method_style = if app.network.filter.method != MethodFilter::All {
        Style::default().fg(MANTLE).bg(GREEN).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(OVERLAY0).bg(bg)
    };
    spans.push(Span::styled(&method_label, method_style));
    x += method_label.len() as u16;
    let method_end = x;
    spans.push(Span::styled("  ", Style::default().bg(bg)));
    x += 2;

    // Status filter — click to cycle
    let status_start = x;
    let status_label = format!(" {} \u{25be}", app.network.filter.status.as_str());
    let status_style = match app.network.filter.status {
        StatusFilter::All => Style::default().fg(OVERLAY0).bg(bg),
        StatusFilter::Failed => Style::default().fg(MANTLE).bg(RED).add_modifier(Modifier::BOLD),
        StatusFilter::Completed => Style::default().fg(MANTLE).bg(GREEN).add_modifier(Modifier::BOLD),
        StatusFilter::Active => Style::default().fg(MANTLE).bg(PEACH).add_modifier(Modifier::BOLD),
        StatusFilter::Pending => Style::default().fg(TEXT).bg(SURFACE1),
    };
    spans.push(Span::styled(&status_label, status_style));
    x += status_label.len() as u16;
    let status_end = x;

    // Fill remaining
    let rem = area.width.saturating_sub(x);
    if rem > 0 {
        spans.push(Span::styled(" ".repeat(rem as usize), Style::default().bg(bg)));
    }

    // Store click regions
    app.layout.net_toolbar_y = area.y;
    app.layout.net_search_x = (search_start, search_end);
    app.layout.net_proto_x = (proto_start, proto_end);
    app.layout.net_method_x = (method_start, method_end);
    app.layout.net_status_x = (status_start, status_end);

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}
