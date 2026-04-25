//! Network view empty-state + status bar renderers.
//!
//! Phase 3 Step 3.8 (UI-010 mirror): extracted from `ui/network/mod.rs`.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::domain::network::{NetworkStatus, Protocol};

use super::super::{
    BASE, BLUE, GREEN, LAVENDER, MANTLE, MAUVE, OVERLAY0, PEACH, RED, SAPPHIRE, SUBTEXT0, SURFACE0,
    SURFACE1, TEXT,
};

pub(super) fn draw_empty_network(f: &mut Frame, app: &mut App, area: Rect) {
    let mid_y = area.height / 2;
    let mut lines: Vec<Line> = Vec::new();

    for _ in 0..mid_y.saturating_sub(3) {
        lines.push(Line::raw(""));
    }

    if app.network_store.is_empty() {
        lines.push(Line::from(Span::styled(
            "    Network Inspector",
            Style::default().fg(SAPPHIRE).add_modifier(Modifier::BOLD),
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
    } else {
        lines.push(Line::from(Span::styled(
            "          \u{2205}", // empty set symbol
            Style::default().fg(SURFACE1),
        )));
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "    No matching requests",
            Style::default().fg(OVERLAY0),
        )));
        lines.push(Line::from(Span::styled(
            "    Try adjusting your filters",
            Style::default().fg(SURFACE1),
        )));
    }

    f.render_widget(Paragraph::new(lines).style(Style::default().bg(BASE)), area);
}

pub(super) fn draw_network_status_bar(f: &mut Frame, app: &mut App, area: Rect) {
    let bg = MANTLE;

    // Left side: toast OR normal info (with source_name for device switching)
    let (left_spans, left_width, source_x) =
        if let Some(msg) = app.active_status().map(|s| s.to_string()) {
            let ok_text = " OK ";
            let msg_text = format!(" {} ", msg);
            let w = ok_text.width() + msg_text.width();
            (
                vec![
                    Span::styled(
                        ok_text,
                        Style::default()
                            .fg(MANTLE)
                            .bg(GREEN)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(msg_text, Style::default().fg(TEXT).bg(bg)),
                ],
                w as u16,
                (0u16, 0u16),
            )
        } else {
            // Count stats
            let total = app.network_store.len();
            let filtered = app.network.filtered_indices(&app.network_store).len();
            let failed_count = app
                .network_store
                .iter()
                .filter(|e| {
                    e.status == NetworkStatus::Failed
                        || e.http_status.map(|c| c >= 400).unwrap_or(false)
                })
                .count();

            let (live_text, live_style) = if app.network.auto_scroll {
                let dot = match (app.tick / 8) % 4 {
                    0 => "\u{25cf}",
                    1 => "\u{25c9}",
                    2 => "\u{25cf}",
                    _ => "\u{25cb}",
                };
                (
                    format!(" {} LIVE ", dot),
                    Style::default()
                        .fg(MANTLE)
                        .bg(GREEN)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                let pct = if filtered > 0 {
                    ((app.network.selected + 1) * 100) / filtered
                } else {
                    100
                };
                (
                    format!(" {}% ", pct.min(100)),
                    Style::default().fg(TEXT).bg(SURFACE0),
                )
            };

            let info = format!(" {}/{} requests", filtered, total);
            let failed_info = if failed_count > 0 {
                format!("  {} failed", failed_count)
            } else {
                String::new()
            };

            // Source name (device info, clickable for device picker)
            let device = if app.source_name.is_empty() {
                String::new()
            } else {
                format!(" {}", app.source_name)
            };

            let lw = live_text.width();
            let iw = info.width() + failed_info.width();
            let dw = device.width();
            let sx_start = (lw + iw) as u16;
            let sx_end = sx_start + dw as u16;

            let mut spans = vec![
                Span::styled(live_text, live_style),
                Span::styled(info, Style::default().fg(SUBTEXT0).bg(bg)),
            ];
            if !failed_info.is_empty() {
                spans.push(Span::styled(failed_info, Style::default().fg(RED).bg(bg)));
            }
            if !device.is_empty() {
                spans.push(Span::styled(
                    device,
                    Style::default()
                        .fg(SUBTEXT0)
                        .bg(bg)
                        .add_modifier(Modifier::UNDERLINED),
                ));
            }

            let w = lw + iw + dw;
            (spans, w as u16, (sx_start, sx_end))
        };

    app.layout.source_info_x = source_x;

    // Check if selected entry is HTTP (some buttons are HTTP-only)
    let selected_is_http = {
        let sel = app.network.selected;
        let indices = app.network.filtered_indices(&app.network_store);
        indices
            .get(sel)
            .and_then(|&idx| app.network_store.get(idx))
            .map(|e| e.protocol == Protocol::Http)
            .unwrap_or(false)
    };

    let mut buttons: Vec<(&str, &str, Style)> = Vec::new();

    if selected_is_http {
        buttons.push((
            "replay",
            " Replay ",
            Style::default()
                .fg(MANTLE)
                .bg(BLUE)
                .add_modifier(Modifier::BOLD),
        ));
        buttons.push((
            "curl",
            " Copy as cURL ",
            Style::default()
                .fg(MANTLE)
                .bg(GREEN)
                .add_modifier(Modifier::BOLD),
        ));
    }

    buttons.push((
        "response",
        " Copy Response ",
        Style::default()
            .fg(MANTLE)
            .bg(SAPPHIRE)
            .add_modifier(Modifier::BOLD),
    ));

    if selected_is_http && app.has_connected_client() {
        buttons.push((
            "mock",
            " Mock ",
            Style::default()
                .fg(MANTLE)
                .bg(MAUVE)
                .add_modifier(Modifier::BOLD),
        ));
    }

    buttons.push((
        "stats",
        " Stats ",
        Style::default()
            .fg(MANTLE)
            .bg(LAVENDER)
            .add_modifier(Modifier::BOLD),
    ));
    buttons.push((
        "clear",
        " Clear ",
        Style::default()
            .fg(MANTLE)
            .bg(PEACH)
            .add_modifier(Modifier::BOLD),
    ));
    buttons.push((
        "help",
        " ? ",
        Style::default()
            .fg(MANTLE)
            .bg(OVERLAY0)
            .add_modifier(Modifier::BOLD),
    ));

    let bw: u16 = buttons.iter().map(|(_, l, _)| l.width() as u16 + 1).sum();
    let spacer = area.width.saturating_sub(left_width + bw).max(1);

    let mut spans = left_spans;
    spans.push(Span::styled(
        " ".repeat(spacer as usize),
        Style::default().bg(bg),
    ));

    // Store button click regions
    let mut xc = left_width + spacer;
    app.layout.net_buttons.clear();
    for (i, (name, label, style)) in buttons.iter().enumerate() {
        let start = xc;
        spans.push(Span::styled(*label, *style));
        xc += label.width() as u16;
        app.layout.net_buttons.push((name.to_string(), start, xc));
        if i < buttons.len() - 1 {
            spans.push(Span::styled(" ", Style::default().bg(bg)));
            xc += 1;
        }
    }

    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)),
        area,
    );
}
