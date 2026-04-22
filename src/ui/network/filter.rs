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

use super::super::{
    BLUE, GREEN, MANTLE, MAUVE, OVERLAY0, PEACH, RED, SUBTEXT0, SURFACE1, YELLOW,
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

/// Row 1: Search + Exclude input fields + (right-aligned) count.
pub fn draw_network_op1(f: &mut Frame, app: &mut App, area: Rect, count: usize, total: usize) {
    use crate::app::{AppMode, InputField};
    use crate::ui::input_field::{render_input_field, InputFieldProps};

    let bg = MANTLE;
    let w = area.width as u16;

    // Reserve right side for count text
    let count_text = format!(" {}/{} ", count, total);
    let cw = count_text.width() as u16;

    let avail = w.saturating_sub(cw + 1);
    let gap: u16 = 1;
    let inner = avail.saturating_sub(gap);
    let per = inner / 2;
    let widths = [per, inner - per];

    let mut spans: Vec<Span> = Vec::new();
    let mut x: u16 = 0;

    let fields: [(InputField, &str, &str); 2] = [
        (InputField::NetSearch, "Search", "(a|b)"),
        (InputField::NetExclude, "Exclude", "(a|b)"),
    ];

    for (i, (field, label, hint)) in fields.iter().enumerate() {
        let active = matches!(app.mode, AppMode::InputActive(f) if f == *field);
        let value = app.inputs.buffer(*field).to_string();
        let cursor_byte = app.inputs.cursor(*field);

        let out = render_input_field(
            InputFieldProps {
                label,
                hint,
                value: &value,
                active,
                cursor_byte,
                total_width: widths[i],
            },
            x,
        );

        match field {
            InputField::NetSearch => app.layout.net_search_x = out.hit_x,
            InputField::NetExclude => app.layout.net_exclude_x = out.hit_x,
            _ => {}
        }

        spans.extend(out.spans);
        x += out.used_width;

        if i < 1 {
            spans.push(Span::styled(" ".repeat(gap as usize), Style::default().bg(bg)));
            x += gap;
        }
    }

    // Pad then count (right-aligned)
    let used: u16 = spans.iter().map(|s| s.content.width() as u16).sum();
    let pad = w.saturating_sub(used + cw);
    if pad > 0 {
        spans.push(Span::styled(" ".repeat(pad as usize), Style::default().bg(bg)));
    }
    spans.push(Span::styled(count_text, Style::default().fg(SUBTEXT0).bg(bg)));

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
/// Mirrors row layout: cursor(1) + PROTO(6) + " " + METHOD(8) + " " + URL(flex) + STATUS(10) + " " + TIME(8) + " " + SIZE(8).
pub fn draw_network_column_header(f: &mut Frame, area: Rect) {
    const PROTO_W: usize = 6;
    const METHOD_W: usize = 8;
    const STATUS_W: usize = 10;
    const TIME_W: usize = 8;
    const SIZE_W: usize = 8;

    let header_style = Style::default().fg(OVERLAY0).bg(MANTLE);
    let w = area.width as usize;
    let fixed = 1 + PROTO_W + 1 + METHOD_W + 1 + STATUS_W + 1 + TIME_W + 1 + SIZE_W;
    let url_w = w.saturating_sub(fixed);
    let text = format!(
        "{}{} {} {}{} {} {}",
        " ",                                    // cursor (1)
        crate::ui::safe_pad("PROTO", PROTO_W),  // 6
        crate::ui::safe_pad("METHOD", METHOD_W),// 8
        crate::ui::safe_pad("URL", url_w),      // flex
        crate::ui::safe_pad("STATUS", STATUS_W),// 10
        crate::ui::safe_pad("TIME", TIME_W),    // 8
        crate::ui::safe_pad("SIZE", SIZE_W),    // 8
    );
    let line = Line::from(Span::styled(text, header_style));
    f.render_widget(
        Paragraph::new(line).style(Style::default().bg(MANTLE)),
        area,
    );
}
