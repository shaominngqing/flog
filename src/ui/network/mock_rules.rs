//! Mock Rules overlay — full-screen view to manage mock rules (view, toggle, delete).

use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
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
    app.layout.mock_rule_regions.clear();

    let rules = app.mock_rules.rules();

    // Clamp selected
    if !rules.is_empty() {
        app.mock_rule_selected = app.mock_rule_selected.min(rules.len() - 1);
    }

    // Draw border
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(SURFACE0))
        .style(Style::default().bg(BASE));
    f.render_widget(block, area);

    let inner = ratatui::layout::Rect::new(
        area.x + 1,
        area.y + 1,
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    );

    // Header row
    if inner.height == 0 {
        return;
    }
    let header_y = inner.y;
    let w = inner.width as usize;
    // Button area: [Edit] [On|Off] [Del] = ~6+1+6+1+5 = 19 chars + padding
    let buttons_w: usize = 22;
    let data_w = w.saturating_sub(buttons_w + 2); // 2 for cursor col
    // Columns: cursor(2) url(flex) method(7) status(6) delay(7) hits(5)
    let method_w = 7;
    let status_w = 6;
    let delay_w = 7;
    let hits_w = 5;
    let url_w = data_w.saturating_sub(method_w + status_w + delay_w + hits_w + 4); // 4 gaps

    let header_text = format!(
        "  {:<url_w$} {:<method_w$} {:<status_w$} {:<delay_w$} {:<hits_w$}",
        "URL Pattern", "Method", "Status", "Delay", "Hits",
        url_w = url_w,
        method_w = method_w,
        status_w = status_w,
        delay_w = delay_w,
        hits_w = hits_w,
    );
    let header_line = Line::from(Span::styled(
        header_text,
        Style::default().fg(SUBTEXT0).bg(SURFACE0).add_modifier(Modifier::BOLD),
    ));
    f.render_widget(
        Paragraph::new(header_line).style(Style::default().bg(SURFACE0)),
        ratatui::layout::Rect::new(inner.x, header_y, inner.width, 1),
    );

    // Data rows
    let max_rows = (inner.height as usize).saturating_sub(1); // minus header
    for (i, rule) in rules.iter().enumerate().take(max_rows) {
        let y = inner.y + 1 + i as u16;
        let is_selected = i == app.mock_rule_selected;
        let row_bg = if is_selected { SURFACE1 } else { BASE };
        let text_color = if rule.enabled { TEXT } else { OVERLAY0 };

        let cursor = if is_selected { "\u{25b8} " } else { "  " };
        let method_text = rule.method.as_deref().unwrap_or("*");
        let status_text = rule.status_code.to_string();
        let delay_text = format!("{}ms", rule.delay_ms);
        let hits_text = rule.hit_count.to_string();

        // Truncate URL pattern
        let url_display = if rule.url_pattern.len() > url_w {
            format!("{}...", &rule.url_pattern[..url_w.saturating_sub(3)])
        } else {
            rule.url_pattern.clone()
        };

        let data_text = format!(
            "{}{:<url_w$} {:<method_w$} {:<status_w$} {:<delay_w$} {:<hits_w$}",
            cursor,
            url_display,
            method_text,
            status_text,
            delay_text,
            hits_text,
            url_w = url_w,
            method_w = method_w,
            status_w = status_w,
            delay_w = delay_w,
            hits_w = hits_w,
        );

        let buttons_x = inner.x + (w.saturating_sub(buttons_w)) as u16;

        // Build the row spans
        let mut spans: Vec<Span> = vec![
            Span::styled(
                data_text,
                Style::default().fg(text_color).bg(row_bg),
            ),
        ];

        // Pad to fill up to buttons
        let data_display_w = 2 + url_w + 1 + method_w + 1 + status_w + 1 + delay_w + 1 + hits_w;
        let pad = w.saturating_sub(data_display_w + buttons_w);
        if pad > 0 {
            spans.push(Span::styled(" ".repeat(pad), Style::default().bg(row_bg)));
        }

        // [Edit] button
        spans.push(Span::styled(
            "[Edit]",
            Style::default().fg(MANTLE).bg(GREEN).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(" ", Style::default().bg(row_bg)));

        // [On/Off] button
        let toggle_text = if rule.enabled { "[Off]" } else { "[On] " };
        spans.push(Span::styled(
            toggle_text,
            Style::default().fg(MANTLE).bg(YELLOW).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(" ", Style::default().bg(row_bg)));

        // [Del] button
        spans.push(Span::styled(
            "[Del]",
            Style::default().fg(MANTLE).bg(RED).add_modifier(Modifier::BOLD),
        ));

        // Remaining pad
        let used = data_display_w + pad + 6 + 1 + 5 + 1 + 5;
        let remain = w.saturating_sub(used);
        if remain > 0 {
            spans.push(Span::styled(" ".repeat(remain), Style::default().bg(row_bg)));
        }

        f.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(row_bg)),
            ratatui::layout::Rect::new(inner.x, y, inner.width, 1),
        );

        // Register click regions
        // Row select region: from x=0 to buttons_start
        app.layout.mock_rule_regions.push((i, "select".to_string(), y, inner.x, buttons_x));

        // Edit button region
        let edit_x = buttons_x;
        app.layout.mock_rule_regions.push((i, "edit".to_string(), y, edit_x, edit_x + 6));

        // Toggle button region
        let toggle_x = edit_x + 7;
        app.layout.mock_rule_regions.push((i, "toggle".to_string(), y, toggle_x, toggle_x + 5));

        // Delete button region
        let del_x = toggle_x + 6;
        app.layout.mock_rule_regions.push((i, "delete".to_string(), y, del_x, del_x + 5));
    }
}

pub fn draw_mock_rule_edit(f: &mut Frame, app: &mut App) {
    let area = f.area();
    app.layout.mock_edit_regions.clear();
    app.layout.mock_edit_body_rect = None;

    // Near full-screen: margin 2 on each side
    let outer = ratatui::layout::Rect::new(2, 1, area.width.saturating_sub(4), area.height.saturating_sub(2));

    // Draw border block
    let title_text = if app.mock_edit_is_new {
        " New Mock Rule "
    } else {
        " Edit Mock Rule "
    };
    let block = Block::default()
        .title(title_text)
        .title_style(
            Style::default()
                .fg(TEXT)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BLUE))
        .style(Style::default().bg(BASE));

    f.render_widget(block, outer);

    // Inner area (inside the border)
    let inner = ratatui::layout::Rect::new(
        outer.x + 2,
        outer.y + 1,
        outer.width.saturating_sub(4),
        outer.height.saturating_sub(2),
    );

    if inner.height < 10 || inner.width < 30 {
        return;
    }

    let field = app.mock_edit_field;
    let label_w: u16 = 16;
    let input_w = inner.width.saturating_sub(label_w + 2);

    let labels = ["URL Pattern:", "Method:", "Status Code:", "Delay (ms):"];
    let field_names = ["url", "method", "status", "delay"];

    let mut y = inner.y;

    // ── Top 4 fields ──
    for (i, (label, field_name)) in labels.iter().zip(field_names.iter()).enumerate() {
        let is_focused = i == field;
        let label_style = if is_focused {
            Style::default().fg(BLUE).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(SUBTEXT0)
        };
        let field_bg = if is_focused { SURFACE1 } else { SURFACE0 };

        // Label
        let label_rect = ratatui::layout::Rect::new(inner.x, y, label_w, 1);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(*label, label_style)))
                .style(Style::default().bg(BASE)),
            label_rect,
        );

        // Input field
        let input_x = inner.x + label_w;
        let input_rect = ratatui::layout::Rect::new(input_x, y, input_w, 1);

        let val = if i < app.mock_edit_top_values.len() {
            &app.mock_edit_top_values[i]
        } else {
            &String::new()
        };

        let max_chars = input_w.saturating_sub(1) as usize;
        let display_val = if is_focused {
            // Show value with cursor
            let visible = if val.len() > max_chars {
                &val[val.len() - max_chars..]
            } else {
                val
            };
            format!("{:<width$}", format!("{}|", visible), width = max_chars)
        } else {
            let visible = if val.len() > max_chars {
                &val[val.len() - max_chars..]
            } else {
                val.as_str()
            };
            format!("{:<width$}", visible, width = max_chars)
        };

        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                display_val,
                Style::default().fg(TEXT).bg(field_bg),
            )))
            .style(Style::default().bg(field_bg)),
            input_rect,
        );

        // Register click region for this field
        app.layout.mock_edit_regions.push((
            field_name.to_string(),
            y,
            input_x,
            input_x + input_w,
        ));

        y += 1;
    }

    // ── Blank line ──
    y += 1;

    // ── Response Body label ──
    let body_focused = field == 4;
    let body_label_style = if body_focused {
        Style::default().fg(BLUE).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(SUBTEXT0)
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled("Response Body:", body_label_style)))
            .style(Style::default().bg(BASE)),
        ratatui::layout::Rect::new(inner.x, y, inner.width, 1),
    );
    y += 1;

    // ── Body editor area ──
    // Remaining height minus 2 for padding + button row
    let body_h = (inner.y + inner.height).saturating_sub(y + 3);
    let body_h = body_h.max(3);

    let body_x = inner.x;
    let body_w = inner.width.saturating_sub(1); // 1 for scrollbar
    let _body_rect = ratatui::layout::Rect::new(body_x, y, body_w, body_h);

    // Store body rect for mouse handling
    app.layout.mock_edit_body_rect = Some((body_x, y, body_w, body_h));
    app.mock_edit_body.visible_height = body_h as usize;

    // Draw body border
    let body_border_style = if body_focused { SURFACE1 } else { SURFACE0 };
    let body_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(body_border_style))
        .style(Style::default().bg(BASE));
    f.render_widget(
        body_block,
        ratatui::layout::Rect::new(body_x, y, body_w + 1, body_h),
    );

    // Render body content inside border
    let content_x = body_x + 1;
    let content_y = y + 1;
    let content_w = body_w.saturating_sub(2);
    let content_h = body_h.saturating_sub(2);

    if content_h > 0 && content_w > 0 {
        // Get colorized JSON lines
        let body_text = app.mock_edit_body.content();
        let colored_lines = crate::ui::json_viewer::colorize_json_text(&body_text);

        let scroll = app.mock_edit_body.scroll_offset;
        let visible = content_h as usize;

        for (vi, line_idx) in (scroll..scroll + visible).enumerate() {
            let ry = content_y + vi as u16;
            let line_rect = ratatui::layout::Rect::new(content_x, ry, content_w, 1);

            if line_idx < colored_lines.len() {
                let line = colored_lines[line_idx].clone();

                if body_focused && line_idx == app.mock_edit_body.cursor_row {
                    // Render line with cursor highlight
                    let raw = app.mock_edit_body.lines[line_idx].as_str();
                    let col = app.mock_edit_body.cursor_col;

                    // Build spans with cursor
                    let mut spans: Vec<Span> = Vec::new();
                    if col >= raw.len() {
                        // Cursor at end — render line normally, then a reversed space
                        spans.extend(line.spans.iter().map(|s| {
                            Span::styled(s.content.to_string(), s.style)
                        }));
                        spans.push(Span::styled(
                            " ",
                            Style::default().fg(BASE).bg(TEXT),
                        ));
                    } else {
                        // Find the character at cursor position
                        let before = &raw[..col];
                        let cursor_char = &raw[col..col + raw[col..].chars().next().map_or(1, |c| c.len_utf8())];
                        let after_start = col + cursor_char.len();
                        let after = &raw[after_start..];

                        // Re-colorize segments
                        let before_lines = crate::ui::json_viewer::colorize_json_text(before);
                        if let Some(bl) = before_lines.first() {
                            spans.extend(bl.spans.iter().map(|s| {
                                Span::styled(s.content.to_string(), s.style)
                            }));
                        }
                        // Cursor char with reversed style
                        spans.push(Span::styled(
                            cursor_char.to_string(),
                            Style::default().fg(BASE).bg(TEXT),
                        ));
                        // After cursor
                        let after_lines = crate::ui::json_viewer::colorize_json_text(after);
                        if let Some(al) = after_lines.first() {
                            spans.extend(al.spans.iter().map(|s| {
                                Span::styled(s.content.to_string(), s.style)
                            }));
                        }
                    }

                    f.render_widget(
                        Paragraph::new(Line::from(spans)).style(Style::default().bg(BASE)),
                        line_rect,
                    );
                } else {
                    f.render_widget(
                        Paragraph::new(line).style(Style::default().bg(BASE)),
                        line_rect,
                    );
                }
            } else {
                // Empty line
                f.render_widget(
                    Paragraph::new("").style(Style::default().bg(BASE)),
                    line_rect,
                );
            }
        }

        // Scrollbar if content exceeds visible area
        let total = app.mock_edit_body.total_lines();
        if total > visible {
            use ratatui::widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState};
            let scrollbar_area = ratatui::layout::Rect::new(
                body_x + body_w,
                content_y,
                1,
                content_h,
            );
            let mut scrollbar_state = ScrollbarState::new(total.saturating_sub(visible))
                .position(scroll);
            f.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(None)
                    .end_symbol(None)
                    .track_style(Style::default().fg(SURFACE0))
                    .thumb_style(Style::default().fg(OVERLAY0)),
                scrollbar_area,
                &mut scrollbar_state,
            );
        }
    }

    y += body_h;
    y += 1;

    // ── Button row ──
    let button_y = y;
    let save_text = " Save ";
    let cancel_text = " Cancel ";
    let total_btn_w = save_text.len() + 4 + cancel_text.len(); // 4 for gaps
    let btn_start_x = inner.x + (inner.width.saturating_sub(total_btn_w as u16)) / 2;

    // Save button
    let save_x = btn_start_x;
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            save_text,
            Style::default()
                .fg(MANTLE)
                .bg(GREEN)
                .add_modifier(Modifier::BOLD),
        ))),
        ratatui::layout::Rect::new(save_x, button_y, save_text.len() as u16, 1),
    );
    app.layout.mock_edit_regions.push((
        "save".to_string(),
        button_y,
        save_x,
        save_x + save_text.len() as u16,
    ));

    // Gap
    let cancel_x = save_x + save_text.len() as u16 + 4;
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            cancel_text,
            Style::default()
                .fg(MANTLE)
                .bg(RED)
                .add_modifier(Modifier::BOLD),
        ))),
        ratatui::layout::Rect::new(cancel_x, button_y, cancel_text.len() as u16, 1),
    );
    app.layout.mock_edit_regions.push((
        "cancel".to_string(),
        button_y,
        cancel_x,
        cancel_x + cancel_text.len() as u16,
    ));
}
