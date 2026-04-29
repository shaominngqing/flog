//! Logs status bar + column header.
//!
//! Bottom row: LIVE pill / toast, counts, app context, and action buttons
//! (Clear / Export / Stats / Help / Quit). Column header sits above the
//! log list and labels each visual column.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::app::App;

use super::{safe_pad, GREEN, MANTLE, OVERLAY0, RED, SAPPHIRE, SUBTEXT0, SURFACE0, TEXT, YELLOW};
use super::{LEVEL_WIDTH, TAG_WIDTH, TIME_WIDTH};

pub(super) fn draw_column_header(f: &mut Frame, area: Rect) {
    // Match row layout exactly: cursor(1) + bm(2) + TIME(12) + " " + LEVEL(9) + " " + TAG(14) + " " + MESSAGE
    let header_style = Style::default().fg(OVERLAY0).bg(MANTLE);
    let text = format!(
        "{}{}{} {} {} {}",
        " ",                            // cursor (1)
        "  ",                           // bookmark (2)
        safe_pad("TIME", TIME_WIDTH),   // 12
        safe_pad("LEVEL", LEVEL_WIDTH), // 9
        safe_pad("TAG", TAG_WIDTH),     // 14
        "MESSAGE",
    );
    let w = area.width as usize;
    let pad = w.saturating_sub(text.width());
    let line = Line::from(vec![
        Span::styled(text, header_style),
        Span::styled(" ".repeat(pad), Style::default().bg(MANTLE)),
    ]);
    f.render_widget(
        Paragraph::new(line).style(Style::default().bg(MANTLE)),
        area,
    );
}

pub(super) fn draw_status_bar(f: &mut Frame, app: &mut App, area: Rect) {
    let bg = MANTLE;

    // Left group: toast / OFFLINE chip / (LIVE pill + counts + app context)
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
        } else if app.active_app_id.is_none() {
            // No app attached. Show OFFLINE chip + discovered-devices hint.
            // The entire region (chip + hint) routes to the device picker
            // via StatusBar click — in the OFFLINE state there's no log
            // list to "jump to bottom", so we deliberately own x=0 too.
            let (chip_spans, chip_w) = crate::ui::offline_chip();
            let (hint_spans, hint_w) =
                crate::ui::offline_devices_hint(app.discovered_devices.len(), bg);
            let mut spans: Vec<Span> = chip_spans;
            spans.extend(hint_spans);
            let total_w = chip_w + hint_w;
            (spans, total_w, (0u16, total_w))
        } else {
            let (live_text, live_style) = if app.logs.auto_scroll {
                let dot = match (app.tick / 8) % 4 {
                    0 => "●",
                    1 => "◉",
                    2 => "●",
                    _ => "○",
                };
                (
                    format!(" {} LIVE ", dot),
                    Style::default()
                        .fg(MANTLE)
                        .bg(GREEN)
                        .add_modifier(Modifier::BOLD),
                )
            } else if app.new_logs_since_pause > 0 {
                (
                    format!(" {} new ", app.new_logs_since_pause),
                    Style::default()
                        .fg(MANTLE)
                        .bg(YELLOW)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                let total = app.filtered_count();
                let vis = app.layout.visible_entry_count.max(1);
                let max_off = total.saturating_sub(vis);
                let pct = if max_off > 0 {
                    ((app.logs.scroll_offset.min(max_off)) * 100) / max_off
                } else {
                    100
                };
                (
                    format!(" {}% ", pct.min(100)),
                    Style::default().fg(TEXT).bg(SURFACE0),
                )
            };

            let total = app.store.len();
            let filtered = app.filtered_count();
            let counts = format!("  {}/{}  ", filtered, total);

            let ctx = app
                .active_app_id
                .as_ref()
                .and_then(|id| app.connected_apps.iter().find(|a| &a.id == id))
                .map(|ca| {
                    let v = if ca.app_version.is_empty() {
                        String::new()
                    } else {
                        format!(" v{}", ca.app_version)
                    };
                    let dev = if ca.device_name.is_empty() {
                        ca.device_id.clone()
                    } else {
                        ca.device_name.clone()
                    };
                    format!("{}{} · {} · :{}", ca.app_name, v, dev, ca.port)
                })
                .unwrap_or_default();

            let lw = live_text.width() as u16;
            let cw = counts.width() as u16;
            let ctxw = (ctx.width() + 2) as u16; // +2 for "⇅ "
            let sx = (lw + cw, lw + cw + ctxw);
            let w = lw + cw + ctxw;
            (
                vec![
                    Span::styled(live_text, live_style),
                    Span::styled(counts, Style::default().fg(SUBTEXT0).bg(bg)),
                    Span::styled(
                        "⇅ ".to_string(),
                        Style::default()
                            .fg(SAPPHIRE)
                            .bg(bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        ctx,
                        Style::default()
                            .fg(SUBTEXT0)
                            .bg(bg)
                            .add_modifier(Modifier::UNDERLINED),
                    ),
                ],
                w,
                sx,
            )
        };

    app.layout.source_info_x = source_x;

    // Right group: unified SURFACE0 buttons with SUBTEXT0 label; Quit in RED
    let button_style = Style::default().fg(SUBTEXT0).bg(SURFACE0);
    let quit_style = Style::default().fg(RED).bg(SURFACE0);
    let buttons: Vec<(&str, &str, Style)> = vec![
        ("clear", "  Clear  ", button_style),
        ("export", "  Export  ", button_style),
        ("stats", "  Stats  ", button_style),
        ("help", "  Help  ", button_style),
        ("quit", "  Quit  ", quit_style),
    ];

    let bw: u16 = buttons.iter().map(|(_, l, _)| l.width() as u16 + 1).sum();
    let spacer = area.width.saturating_sub(left_width + bw).max(1);

    let mut spans = left_spans;
    spans.push(Span::styled(
        " ".repeat(spacer as usize),
        Style::default().bg(bg),
    ));

    let mut xc = left_width + spacer;
    app.layout.bottom_buttons.clear();
    for (i, (name, label, style)) in buttons.iter().enumerate() {
        let start = xc;
        spans.push(Span::styled(*label, *style));
        xc += label.width() as u16;
        app.layout.bottom_buttons.push((name, start, xc));
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
