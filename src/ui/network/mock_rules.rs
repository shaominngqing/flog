//! Mock Rules overlay — full-screen view to manage mock rules (view, toggle, delete).

use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::app::App;

// Catppuccin Macchiato palette
const BASE: Color = Color::Rgb(36, 39, 58);
const TEXT: Color = Color::Rgb(202, 211, 245);
const SUBTEXT0: Color = Color::Rgb(165, 173, 203);
const SURFACE0: Color = Color::Rgb(54, 58, 79);
const SURFACE1: Color = Color::Rgb(65, 69, 89);
const MANTLE: Color = Color::Rgb(30, 32, 48);
const GREEN: Color = Color::Rgb(166, 218, 149);
const YELLOW: Color = Color::Rgb(238, 212, 159);
const RED: Color = Color::Rgb(237, 135, 150);
const BLUE: Color = Color::Rgb(138, 173, 244);
const MAUVE: Color = Color::Rgb(198, 160, 246);
const OVERLAY0: Color = Color::Rgb(110, 115, 141);

pub fn draw_mock_rules(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title bar
            Constraint::Min(3),    // table body
            Constraint::Length(1), // footer
        ])
        .split(f.area());

    // ── Title bar ──
    let total = app.mock_rules.len();
    let enabled = app.mock_rules.enabled_count();
    let w = f.area().width as usize;
    let count_text = format!("{} rules ({} enabled)", total, enabled);
    let title_text = "Mock Rules";
    let back_text = " \u{2190} Back ";
    let pad_len = w.saturating_sub(back_text.width() + 2 + title_text.width() + 2 + count_text.width() + 2);

    let title = Line::from(vec![
        Span::styled(
            back_text,
            Style::default()
                .fg(MANTLE)
                .bg(MAUVE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ", Style::default().bg(MANTLE)),
        Span::styled(
            title_text,
            Style::default()
                .fg(TEXT)
                .bg(MANTLE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ", Style::default().bg(MANTLE)),
        Span::styled(
            &count_text,
            Style::default().fg(SUBTEXT0).bg(MANTLE),
        ),
        Span::styled(
            " ".repeat(pad_len),
            Style::default().bg(MANTLE),
        ),
    ]);
    f.render_widget(Paragraph::new(title).style(Style::default().bg(MANTLE)), chunks[0]);

    // ── Table body ──
    if total == 0 {
        draw_empty_state(f, chunks[1]);
    } else {
        draw_rules_table(f, app, chunks[1]);
    }

    // ── Footer ──
    let footer = Line::from(vec![
        Span::styled(" ", Style::default().bg(MANTLE)),
        Span::styled(
            " Enter ",
            Style::default()
                .fg(MANTLE)
                .bg(GREEN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" edit  ", Style::default().fg(SUBTEXT0).bg(MANTLE)),
        Span::styled(
            " Space ",
            Style::default()
                .fg(MANTLE)
                .bg(YELLOW)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" toggle  ", Style::default().fg(SUBTEXT0).bg(MANTLE)),
        Span::styled(
            " d ",
            Style::default()
                .fg(MANTLE)
                .bg(RED)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" delete  ", Style::default().fg(SUBTEXT0).bg(MANTLE)),
        Span::styled(
            " Esc ",
            Style::default()
                .fg(MANTLE)
                .bg(BLUE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" back", Style::default().fg(SUBTEXT0).bg(MANTLE)),
        Span::styled(
            " ".repeat(w.saturating_sub(56)),
            Style::default().bg(MANTLE),
        ),
    ]);
    f.render_widget(Paragraph::new(footer).style(Style::default().bg(MANTLE)), chunks[2]);
}

fn draw_empty_state(f: &mut Frame, area: ratatui::layout::Rect) {
    let mid_y = area.height / 2;
    let mut lines: Vec<Line> = Vec::new();

    for _ in 0..mid_y.saturating_sub(2) {
        lines.push(Line::raw(""));
    }

    lines.push(Line::from(Span::styled(
        "    No mock rules configured.",
        Style::default().fg(OVERLAY0),
    )));
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "    Press M on a network request to create one.",
        Style::default().fg(SURFACE1),
    )));

    f.render_widget(
        Paragraph::new(lines)
            .style(Style::default().bg(BASE))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_rules_table(f: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let rules = app.mock_rules.rules();

    // Clamp selected
    if !rules.is_empty() {
        app.mock_rule_selected = app.mock_rule_selected.min(rules.len() - 1);
    }

    // Build header
    let header = Row::new(vec![
        Cell::from(" ").style(Style::default().fg(SUBTEXT0).add_modifier(Modifier::BOLD)),
        Cell::from("URL Pattern").style(Style::default().fg(SUBTEXT0).add_modifier(Modifier::BOLD)),
        Cell::from("Method").style(Style::default().fg(SUBTEXT0).add_modifier(Modifier::BOLD)),
        Cell::from("Status").style(Style::default().fg(SUBTEXT0).add_modifier(Modifier::BOLD)),
        Cell::from("Delay").style(Style::default().fg(SUBTEXT0).add_modifier(Modifier::BOLD)),
        Cell::from("Hits").style(Style::default().fg(SUBTEXT0).add_modifier(Modifier::BOLD)),
        Cell::from("Enabled").style(Style::default().fg(SUBTEXT0).add_modifier(Modifier::BOLD)),
    ])
    .style(Style::default().bg(SURFACE0));

    // Build rows
    let rows: Vec<Row> = rules
        .iter()
        .enumerate()
        .map(|(i, rule)| {
            let is_selected = i == app.mock_rule_selected;
            let row_bg = if is_selected { SURFACE1 } else { BASE };
            let text_color = if rule.enabled { TEXT } else { OVERLAY0 };

            let cursor = if is_selected {
                "\u{25b8}" // ▸
            } else {
                " "
            };

            let method_text = rule.method.as_deref().unwrap_or("*");
            let status_text = rule.status_code.to_string();
            let delay_text = format!("{}ms", rule.delay_ms);
            let hits_text = rule.hit_count.to_string();
            let (enabled_text, enabled_color) = if rule.enabled {
                ("\u{2713}", GREEN) // ✓
            } else {
                ("\u{2717}", RED)   // ✗
            };

            Row::new(vec![
                Cell::from(cursor).style(Style::default().fg(if is_selected { BLUE } else { BASE }).bg(row_bg)),
                Cell::from(rule.url_pattern.clone()).style(Style::default().fg(text_color).bg(row_bg)),
                Cell::from(method_text.to_string()).style(Style::default().fg(if rule.enabled { MAUVE } else { OVERLAY0 }).bg(row_bg)),
                Cell::from(status_text).style(Style::default().fg(if rule.enabled { GREEN } else { OVERLAY0 }).bg(row_bg)),
                Cell::from(delay_text).style(Style::default().fg(if rule.enabled { YELLOW } else { OVERLAY0 }).bg(row_bg)),
                Cell::from(hits_text).style(Style::default().fg(if rule.enabled { SUBTEXT0 } else { OVERLAY0 }).bg(row_bg)),
                Cell::from(enabled_text).style(Style::default().fg(enabled_color).bg(row_bg)),
            ])
            .style(Style::default().bg(row_bg))
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(2),     // cursor
            Constraint::Min(20),       // URL pattern
            Constraint::Length(8),     // Method
            Constraint::Length(8),     // Status
            Constraint::Length(8),     // Delay
            Constraint::Length(6),     // Hits
            Constraint::Length(8),     // Enabled
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(SURFACE0))
            .style(Style::default().bg(BASE)),
    );

    f.render_widget(table, area);
}

pub fn draw_mock_rule_edit(f: &mut Frame, app: &mut App) {
    let area = f.area();

    // Center the edit overlay — 68 chars wide, 20 lines tall
    let overlay_w: u16 = 68.min(area.width.saturating_sub(4));
    let overlay_h: u16 = 20.min(area.height.saturating_sub(2));
    let overlay_x = (area.width.saturating_sub(overlay_w)) / 2;
    let overlay_y = (area.height.saturating_sub(overlay_h)) / 2;

    let overlay_rect = ratatui::layout::Rect::new(overlay_x, overlay_y, overlay_w, overlay_h);

    // Draw border block
    let block = Block::default()
        .title(" Edit Mock Rule ")
        .title_style(
            Style::default()
                .fg(TEXT)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BLUE))
        .style(Style::default().bg(BASE));

    f.render_widget(block, overlay_rect);

    // Inner area (inside the border)
    let inner = ratatui::layout::Rect::new(
        overlay_rect.x + 1,
        overlay_rect.y + 1,
        overlay_rect.width.saturating_sub(2),
        overlay_rect.height.saturating_sub(2),
    );

    let field = app.mock_edit_field;
    let values = &app.mock_edit_values;
    if values.len() < 5 {
        return;
    }

    let label_w: u16 = 16;
    let input_w = inner.width.saturating_sub(label_w + 4);

    let labels = ["URL Pattern:", "Method:", "Status Code:", "Delay (ms):", "Response Body:"];

    let mut y = inner.y + 1;

    for (i, label) in labels.iter().enumerate() {
        if y >= inner.y + inner.height.saturating_sub(2) {
            break;
        }

        let is_focused = i == field;
        let label_style = if is_focused {
            Style::default().fg(BLUE).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(SUBTEXT0)
        };
        let field_bg = if is_focused { SURFACE1 } else { SURFACE0 };

        // For the body field (index 4), we use multiple lines
        if i == 4 {
            // Label on its own line
            let label_span = Span::styled(format!("  {}", label), label_style);
            f.render_widget(
                Paragraph::new(Line::from(label_span)).style(Style::default().bg(BASE)),
                ratatui::layout::Rect::new(inner.x, y, inner.width, 1),
            );
            y += 1;

            // Body text area (remaining space minus 2 lines for footer)
            let body_h = inner.y + inner.height - y - 2;
            let body_h = body_h.max(3).min(5);

            if y + body_h > inner.y + inner.height.saturating_sub(2) {
                break;
            }

            let body_rect =
                ratatui::layout::Rect::new(inner.x + 2, y, inner.width.saturating_sub(4), body_h);

            let body_text = &values[4];
            let display_text = if is_focused {
                format!("{}_", body_text)
            } else {
                body_text.clone()
            };

            let body_para = Paragraph::new(display_text)
                .style(Style::default().fg(TEXT).bg(field_bg))
                .wrap(Wrap { trim: false });

            f.render_widget(body_para, body_rect);
            y += body_h;
        } else {
            // Single-line fields: label + input on same row
            let label_rect = ratatui::layout::Rect::new(inner.x, y, label_w + 2, 1);
            let input_rect = ratatui::layout::Rect::new(inner.x + label_w + 2, y, input_w, 1);

            let label_span = Span::styled(format!("  {}", label), label_style);
            f.render_widget(
                Paragraph::new(Line::from(label_span)).style(Style::default().bg(BASE)),
                label_rect,
            );

            let val = &values[i];
            let display_val = if is_focused {
                format!("{}_", val)
            } else {
                val.clone()
            };

            // Truncate if too long for the field
            let max_chars = input_w.saturating_sub(1) as usize;
            let truncated: String = if display_val.len() > max_chars {
                display_val[display_val.len() - max_chars..].to_string()
            } else {
                // Pad to fill the input field
                format!("{:<width$}", display_val, width = max_chars)
            };

            let input_para = Paragraph::new(Line::from(Span::styled(
                truncated,
                Style::default().fg(TEXT).bg(field_bg),
            )))
            .style(Style::default().bg(field_bg));

            f.render_widget(input_para, input_rect);
            y += 1;
        }

        // Small gap between fields
        y += 1;
    }

    // Footer hints
    let footer_y = overlay_rect.y + overlay_rect.height.saturating_sub(2);
    let footer_rect = ratatui::layout::Rect::new(inner.x, footer_y, inner.width, 1);
    let footer = Line::from(vec![
        Span::styled("  ", Style::default().bg(BASE)),
        Span::styled(
            " Tab ",
            Style::default()
                .fg(MANTLE)
                .bg(MAUVE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" next  ", Style::default().fg(SUBTEXT0).bg(BASE)),
        Span::styled(
            " Ctrl+Enter ",
            Style::default()
                .fg(MANTLE)
                .bg(GREEN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" save  ", Style::default().fg(SUBTEXT0).bg(BASE)),
        Span::styled(
            " Esc ",
            Style::default()
                .fg(MANTLE)
                .bg(RED)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" cancel", Style::default().fg(SUBTEXT0).bg(BASE)),
    ]);
    f.render_widget(
        Paragraph::new(footer).style(Style::default().bg(BASE)),
        footer_rect,
    );
}
