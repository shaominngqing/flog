//! Network toolbar renderer — 2-line toolbar with URL search and inline filter pills.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::domain::network_filter::{MethodFilter, ProtocolFilter, StatusFilter};
use crate::ui::safe_pad;

use super::super::{
    MANTLE, SURFACE0, SURFACE1, OVERLAY0, TEXT,
    BLUE, GREEN, YELLOW, PEACH, RED, SAPPHIRE, MAUVE,
};

/// Render a filter pill: selected = colored bg, unselected = dim text.
fn pill<'a>(label: &str, selected: bool, color: ratatui::style::Color, bg: ratatui::style::Color) -> Span<'a> {
    if selected {
        Span::styled(
            format!(" {} ", label),
            Style::default().fg(MANTLE).bg(color).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            format!(" {} ", label),
            Style::default().fg(OVERLAY0).bg(bg),
        )
    }
}

pub fn draw_network_toolbar(f: &mut Frame, app: &mut App, area: Rect) {
    if area.height < 2 { return; }

    let bg = MANTLE;
    let w = area.width as usize;
    let is_searching = app.network.search_active;

    // ── Line 1: Logo + URL search ──
    let mut spans1: Vec<Span> = Vec::new();
    let mut x: u16 = 0;

    spans1.push(Span::styled(" NET ", Style::default().fg(MANTLE).bg(SAPPHIRE).add_modifier(Modifier::BOLD)));
    x += 5;
    spans1.push(Span::styled(" ", Style::default().bg(bg)));
    x += 1;

    let search_start = x;
    let sw: usize = w.saturating_sub(8); // use most of the width for search
    let search_text = if is_searching {
        format!("/{}_", app.network.search_input)
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
    spans1.push(Span::styled(safe_pad(&search_text, sw), ss));
    x += sw as u16;
    let search_end = x;

    let rem1 = (area.width as usize).saturating_sub(x as usize);
    if rem1 > 0 {
        spans1.push(Span::styled(" ".repeat(rem1), Style::default().bg(bg)));
    }

    // ── Line 2: Filter pills ──
    let mut spans2: Vec<Span> = Vec::new();
    let mut click_regions: Vec<(String, u16, u16)> = Vec::new();
    let mut cx: u16 = 0;

    // Separator label
    spans2.push(Span::styled("  ", Style::default().bg(bg)));
    cx += 2;

    // Protocol pills
    spans2.push(Span::styled("Protocol ", Style::default().fg(OVERLAY0).bg(bg)));
    cx += 9;

    let proto_options: Vec<(ProtocolFilter, &str, ratatui::style::Color)> = vec![
        (ProtocolFilter::All, "All", SURFACE1),
        (ProtocolFilter::Http, "HTTP", BLUE),
        (ProtocolFilter::Sse, "SSE", PEACH),
        (ProtocolFilter::Ws, "WS", MAUVE),
    ];
    for (val, label, color) in &proto_options {
        let start = cx;
        let selected = app.network.filter.protocol == *val;
        let effective_color = if *val == ProtocolFilter::All { SURFACE1 } else { *color };
        spans2.push(pill(label, selected, effective_color, bg));
        cx += label.len() as u16 + 2;
        click_regions.push((format!("proto_{}", label), start, cx));
    }

    spans2.push(Span::styled("   ", Style::default().bg(bg)));
    cx += 3;

    // Method pills
    spans2.push(Span::styled("Method ", Style::default().fg(OVERLAY0).bg(bg)));
    cx += 7;

    let method_options: Vec<(MethodFilter, &str, ratatui::style::Color)> = vec![
        (MethodFilter::All, "All", SURFACE1),
        (MethodFilter::Get, "GET", GREEN),
        (MethodFilter::Post, "POST", BLUE),
        (MethodFilter::Put, "PUT", PEACH),
        (MethodFilter::Delete, "DEL", RED),
        (MethodFilter::Patch, "PATCH", MAUVE),
    ];
    for (val, label, color) in &method_options {
        let start = cx;
        let selected = app.network.filter.method == *val;
        let effective_color = if *val == MethodFilter::All { SURFACE1 } else { *color };
        spans2.push(pill(label, selected, effective_color, bg));
        cx += label.len() as u16 + 2;
        click_regions.push((format!("method_{}", label), start, cx));
    }

    spans2.push(Span::styled("   ", Style::default().bg(bg)));
    cx += 3;

    // Status pills
    spans2.push(Span::styled("Status ", Style::default().fg(OVERLAY0).bg(bg)));
    cx += 7;

    let status_options: Vec<(StatusFilter, &str, ratatui::style::Color)> = vec![
        (StatusFilter::All, "All", SURFACE1),
        (StatusFilter::Completed, "OK", GREEN),
        (StatusFilter::Failed, "Fail", RED),
        (StatusFilter::Active, "Active", PEACH),
        (StatusFilter::Pending, "Pending", YELLOW),
    ];
    for (val, label, color) in &status_options {
        let start = cx;
        let selected = app.network.filter.status == *val;
        let effective_color = if *val == StatusFilter::All { SURFACE1 } else { *color };
        spans2.push(pill(label, selected, effective_color, bg));
        cx += label.len() as u16 + 2;
        click_regions.push((format!("status_{}", label), start, cx));
    }

    let rem2 = (area.width as usize).saturating_sub(cx as usize);
    if rem2 > 0 {
        spans2.push(Span::styled(" ".repeat(rem2), Style::default().bg(bg)));
    }

    // Store click regions
    app.layout.net_toolbar_y = area.y;
    app.layout.net_search_x = (search_start, search_end);
    app.layout.net_filter_pills = click_regions;
    app.layout.net_filter_pills_y = area.y + 1; // line 2

    let lines = vec![
        Line::from(spans1),
        Line::from(spans2),
    ];

    f.render_widget(Paragraph::new(lines).style(Style::default().bg(bg)), area);
}
