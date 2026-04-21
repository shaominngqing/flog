//! Network toolbar renderer — 2-row toolbar matching Logs (search+count / pill groups).

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
    BLUE, GREEN, MANTLE, MAUVE, OVERLAY0, PEACH, RED, SUBTEXT0, SURFACE0, SURFACE1, TEXT, YELLOW,
};

/// Render a filter pill: selected = bright colored bg, unselected = outline only (MANTLE bg).
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
            Style::default().fg(color).bg(MANTLE),
        )
    }
}

/// Row 1: "/" + search box + (right-aligned) count.
pub fn draw_network_op1(f: &mut Frame, app: &mut App, area: Rect, count: usize, total: usize) {
    let bg = MANTLE;
    let w = area.width as u16;
    let is_searching = app.network.search_active;

    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::styled(" ", Style::default().bg(bg)));

    let si = if is_searching {
        Style::default().fg(MANTLE).bg(YELLOW)
    } else {
        Style::default().fg(OVERLAY0).bg(bg)
    };
    spans.push(Span::styled("/", si));

    let sw: usize = 40;
    let s = if is_searching {
        format!("{}_", app.network.search_input)
    } else if app.network.filter.search.is_empty() {
        "filter url...".to_string()
    } else {
        app.network.filter.search.clone()
    };
    let ss = if is_searching {
        Style::default().fg(TEXT).bg(SURFACE0)
    } else if !app.network.filter.search.is_empty() {
        Style::default().fg(YELLOW).bg(bg)
    } else {
        Style::default().fg(OVERLAY0).bg(bg)
    };
    app.layout.net_search_x = (1, 1 + 1 + sw as u16);
    spans.push(Span::styled(safe_pad(&s, sw), ss));

    let used: u16 = spans.iter().map(|x| x.content.width() as u16).sum();
    let count_text = format!(" {}/{} ", count, total);
    let cw = count_text.width() as u16;
    let pad = w.saturating_sub(used + cw);
    spans.push(Span::styled(
        " ".repeat(pad as usize),
        Style::default().bg(bg),
    ));
    spans.push(Span::styled(
        count_text,
        Style::default().fg(SUBTEXT0).bg(bg),
    ));

    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)),
        area,
    );
}

/// Row 2: protocol pills │ method pills │ status pills.
pub fn draw_network_op2(f: &mut Frame, app: &mut App, area: Rect) {
    let bg = MANTLE;
    let mut spans: Vec<Span> = Vec::new();
    let mut x: u16 = 0;
    app.layout.net_filter_pills.clear();
    app.layout.net_filter_pills_y = area.y;

    spans.push(Span::styled(" ", Style::default().bg(bg)));
    x += 1;

    // Protocol
    let proto = app.network.filter.protocol;
    let proto_pills: &[(&str, ProtocolFilter, ratatui::style::Color)] = &[
        ("All", ProtocolFilter::All, GREEN),
        ("HTTP", ProtocolFilter::Http, BLUE),
        ("SSE", ProtocolFilter::Sse, GREEN),
        ("WS", ProtocolFilter::Ws, PEACH),
    ];
    for (label, val, color) in proto_pills {
        let selected = proto == *val;
        let p = pill(label, selected, *color);
        let start = x;
        x += p.content.width() as u16;
        app.layout
            .net_filter_pills
            .push((format!("proto_{}", label), start, x));
        spans.push(p);
        spans.push(Span::styled(" ", Style::default().bg(bg)));
        x += 1;
    }

    spans.push(Span::styled(" │ ", Style::default().fg(SURFACE1).bg(bg)));
    x += 3;

    // Method
    let method = app.network.filter.method;
    let method_pills: &[(&str, MethodFilter, ratatui::style::Color)] = &[
        ("All", MethodFilter::All, GREEN),
        ("GET", MethodFilter::Get, GREEN),
        ("POST", MethodFilter::Post, BLUE),
        ("PUT", MethodFilter::Put, PEACH),
        ("DEL", MethodFilter::Delete, RED),
        ("PATCH", MethodFilter::Patch, MAUVE),
    ];
    for (label, val, color) in method_pills {
        let selected = method == *val;
        let p = pill(label, selected, *color);
        let start = x;
        x += p.content.width() as u16;
        app.layout
            .net_filter_pills
            .push((format!("method_{}", label), start, x));
        spans.push(p);
        spans.push(Span::styled(" ", Style::default().bg(bg)));
        x += 1;
    }

    spans.push(Span::styled(" │ ", Style::default().fg(SURFACE1).bg(bg)));
    x += 3;

    // Status
    let status = app.network.filter.status;
    let status_pills: &[(&str, StatusFilter, ratatui::style::Color)] = &[
        ("All", StatusFilter::All, GREEN),
        ("OK", StatusFilter::Completed, GREEN),
        ("Fail", StatusFilter::Failed, RED),
        ("Active", StatusFilter::Active, YELLOW),
        ("Pending", StatusFilter::Pending, OVERLAY0),
    ];
    for (label, val, color) in status_pills {
        let selected = status == *val;
        let p = pill(label, selected, *color);
        let start = x;
        x += p.content.width() as u16;
        app.layout
            .net_filter_pills
            .push((format!("status_{}", label), start, x));
        spans.push(p);
        spans.push(Span::styled(" ", Style::default().bg(bg)));
        x += 1;
    }

    let rem = area.width.saturating_sub(x);
    if rem > 0 {
        spans.push(Span::styled(
            " ".repeat(rem as usize),
            Style::default().bg(bg),
        ));
    }

    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)),
        area,
    );
}

/// Column header row above the request list.
pub fn draw_network_column_header(f: &mut Frame, area: Rect) {
    let header_style = Style::default().fg(OVERLAY0).bg(MANTLE);
    let w = area.width as usize;
    let right_cluster = " STATUS   TIME    SIZE ";
    let right_w = right_cluster.width();
    let left = " PROTO  METHOD  URL";
    let left_w = left.width();
    let pad = w.saturating_sub(left_w + right_w);
    let line = Line::from(vec![
        Span::styled(left.to_string(), header_style),
        Span::styled(" ".repeat(pad), Style::default().bg(MANTLE)),
        Span::styled(right_cluster.to_string(), header_style),
    ]);
    f.render_widget(
        Paragraph::new(line).style(Style::default().bg(MANTLE)),
        area,
    );
}
