//! Network toolbar renderer — 2-line toolbar with URL search and inline filter pills.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::domain::network_filter::{MethodFilter, ProtocolFilter, StatusFilter};
use crate::ui::safe_pad;

use super::super::{
    BLUE, GREEN, MANTLE, MAUVE, OVERLAY0, PEACH, RED, SAPPHIRE, SURFACE0, SURFACE1, TEXT, YELLOW,
};

/// Render a filter pill: selected = bright colored bg, unselected = subtle bg.
fn pill<'a>(label: &str, selected: bool, color: ratatui::style::Color) -> Span<'a> {
    if selected {
        Span::styled(
            format!(" {} ", label),
            Style::default()
                .fg(MANTLE)
                .bg(color)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            format!(" {} ", label),
            Style::default().fg(OVERLAY0).bg(SURFACE0),
        )
    }
}

pub fn draw_network_toolbar(f: &mut Frame, app: &mut App, area: Rect) {
    if area.height < 2 {
        return;
    }

    let bg = MANTLE;
    let w = area.width as usize;
    let is_searching = app.network.search_active;

    // ── Line 1: Logo + URL search ──
    let mut spans1: Vec<Span> = Vec::new();
    let mut x: u16 = 0;

    spans1.push(Span::styled(
        " NET ",
        Style::default()
            .fg(MANTLE)
            .bg(SAPPHIRE)
            .add_modifier(Modifier::BOLD),
    ));
    x += 5;
    spans1.push(Span::styled(" ", Style::default().bg(bg)));
    x += 1;

    // Mock status indicator (right side of line 1)
    let mock_text = if app.is_vm_service_connected() {
        let rule_count = app.mock_rules.enabled_count();
        if rule_count > 0 {
            format!("{} mock rules active ", rule_count)
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    let mock_w = mock_text.width();
    let mock_reserved = if mock_w > 0 { mock_w + 1 } else { 0 };

    let search_start = x;
    let sw: usize = w.saturating_sub(x as usize + mock_reserved + 2);
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

    // Mock status indicator (right-aligned on line 1)
    let rem1 = (area.width as usize).saturating_sub(x as usize);
    if !mock_text.is_empty() && rem1 > mock_w {
        let gap = rem1.saturating_sub(mock_w);
        spans1.push(Span::styled(" ".repeat(gap), Style::default().bg(bg)));
        spans1.push(Span::styled(
            mock_text,
            Style::default().fg(GREEN).bg(bg),
        ));
    } else if rem1 > 0 {
        spans1.push(Span::styled(" ".repeat(rem1), Style::default().bg(bg)));
    }

    // ── Line 2: Filter pills with separators ──
    let mut spans2: Vec<Span> = Vec::new();
    let mut click_regions: Vec<(String, u16, u16)> = Vec::new();
    let mut cx: u16 = 0;
    let sep_style = Style::default().fg(SURFACE0).bg(bg);
    let label_style = Style::default().fg(SURFACE1).bg(bg);

    spans2.push(Span::styled("  ", Style::default().bg(bg)));
    cx += 2;

    // ── Protocol group ──
    spans2.push(Span::styled("Protocol ", label_style));
    cx += 9;

    let proto_options: Vec<(ProtocolFilter, &str, ratatui::style::Color)> = vec![
        (ProtocolFilter::All, "All", BLUE),
        (ProtocolFilter::Http, "HTTP", BLUE),
        (ProtocolFilter::Sse, "SSE", PEACH),
        (ProtocolFilter::Ws, "WS", MAUVE),
    ];
    for (val, label, color) in &proto_options {
        let start = cx;
        let selected = app.network.filter.protocol == *val;
        spans2.push(pill(label, selected, *color));
        cx += label.len() as u16 + 2;
        click_regions.push((format!("proto_{}", label), start, cx));
        // Gap between pills
        spans2.push(Span::styled(" ", Style::default().bg(bg)));
        cx += 1;
    }

    // Vertical separator
    spans2.push(Span::styled(" \u{2502} ", sep_style)); // │
    cx += 3;

    // ── Method group ──
    spans2.push(Span::styled("Method ", label_style));
    cx += 7;

    let method_options: Vec<(MethodFilter, &str, ratatui::style::Color)> = vec![
        (MethodFilter::All, "All", GREEN),
        (MethodFilter::Get, "GET", GREEN),
        (MethodFilter::Post, "POST", BLUE),
        (MethodFilter::Put, "PUT", PEACH),
        (MethodFilter::Delete, "DEL", RED),
        (MethodFilter::Patch, "PATCH", MAUVE),
    ];
    for (val, label, color) in &method_options {
        let start = cx;
        let selected = app.network.filter.method == *val;
        spans2.push(pill(label, selected, *color));
        cx += label.len() as u16 + 2;
        click_regions.push((format!("method_{}", label), start, cx));
        spans2.push(Span::styled(" ", Style::default().bg(bg)));
        cx += 1;
    }

    // Vertical separator
    spans2.push(Span::styled(" \u{2502} ", sep_style));
    cx += 3;

    // ── Status group ──
    spans2.push(Span::styled("Status ", label_style));
    cx += 7;

    let status_options: Vec<(StatusFilter, &str, ratatui::style::Color)> = vec![
        (StatusFilter::All, "All", GREEN),
        (StatusFilter::Completed, "OK", GREEN),
        (StatusFilter::Failed, "Fail", RED),
        (StatusFilter::Active, "Active", PEACH),
        (StatusFilter::Pending, "Pending", YELLOW),
    ];
    for (val, label, color) in &status_options {
        let start = cx;
        let selected = app.network.filter.status == *val;
        spans2.push(pill(label, selected, *color));
        cx += label.len() as u16 + 2;
        click_regions.push((format!("status_{}", label), start, cx));
        spans2.push(Span::styled(" ", Style::default().bg(bg)));
        cx += 1;
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

    let lines = vec![Line::from(spans1), Line::from(spans2)];

    f.render_widget(Paragraph::new(lines).style(Style::default().bg(bg)), area);
}
