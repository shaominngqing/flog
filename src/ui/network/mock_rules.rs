//! Mock Rules — side panel view and editor overlay for managing mock rules.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::app::App;

use super::super::{
    BASE, BLUE, GREEN, MANTLE, MAUVE, OVERLAY0, RED, SUBTEXT0, SURFACE0, SURFACE1, TEXT, YELLOW,
};

pub fn draw_mock_rule_edit(f: &mut Frame, app: &mut App) {
    let area = f.area();
    app.layout.mock_edit_regions.clear();
    app.layout.mock_edit_body_rect = None;

    // Centered overlay with comfortable margins
    let mx: u16 = 6;
    let my: u16 = 3;
    let outer = ratatui::layout::Rect::new(
        mx,
        my,
        area.width.saturating_sub(mx * 2),
        area.height.saturating_sub(my * 2),
    );

    // Clear underlying content so nothing bleeds through
    f.render_widget(Clear, outer);
    f.render_widget(Block::default().style(Style::default().bg(BASE)), outer);

    // Draw border block
    let title_text = " Edit Mock Rule ";
    let block = Block::default()
        .title(title_text)
        .title_style(Style::default().fg(TEXT).add_modifier(Modifier::BOLD))
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
    let label_w: u16 = 18;
    let input_w = inner.width.saturating_sub(label_w + 2);

    let labels = [
        "URL Pattern:  ",
        "Method:       ",
        "Status Code:  ",
        "Delay (ms):   ",
    ];
    let field_names = ["url", "method", "status", "delay"];

    let mut y = inner.y;
    y += 1; // blank line after title

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
        app.layout
            .mock_edit_regions
            .push((field_name.to_string(), y, input_x, input_x + input_w));

        y += 1;
    }

    // ── Extra spacing before body ──
    y += 1;
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

    // Store body CONTENT rect (inside border) for mouse handling
    app.layout.mock_edit_body_rect = Some((
        body_x + 1,
        y + 1,
        body_w.saturating_sub(2),
        body_h.saturating_sub(2),
    ));
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
                        spans.extend(
                            line.spans
                                .iter()
                                .map(|s| Span::styled(s.content.to_string(), s.style)),
                        );
                        spans.push(Span::styled(" ", Style::default().fg(BASE).bg(TEXT)));
                    } else {
                        // Find the character at cursor position (snap to char boundary)
                        let snap_col = if raw.is_char_boundary(col) {
                            col
                        } else {
                            // Walk backwards to find a valid char boundary
                            (0..col)
                                .rev()
                                .find(|&i| raw.is_char_boundary(i))
                                .unwrap_or(0)
                        };
                        let before = &raw[..snap_col];
                        let cursor_char = &raw[snap_col
                            ..snap_col
                                + raw[snap_col..].chars().next().map_or(1, |c| c.len_utf8())];
                        let after_start = snap_col + cursor_char.len();
                        let after = &raw[after_start..];

                        // Re-colorize segments
                        let before_lines = crate::ui::json_viewer::colorize_json_text(before);
                        if let Some(bl) = before_lines.first() {
                            spans.extend(
                                bl.spans
                                    .iter()
                                    .map(|s| Span::styled(s.content.to_string(), s.style)),
                            );
                        }
                        // Cursor char with reversed style
                        spans.push(Span::styled(
                            cursor_char.to_string(),
                            Style::default().fg(BASE).bg(TEXT),
                        ));
                        // After cursor
                        let after_lines = crate::ui::json_viewer::colorize_json_text(after);
                        if let Some(al) = after_lines.first() {
                            spans.extend(
                                al.spans
                                    .iter()
                                    .map(|s| Span::styled(s.content.to_string(), s.style)),
                            );
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
            let scrollbar_area =
                ratatui::layout::Rect::new(body_x + body_w, content_y, 1, content_h);
            let mut scrollbar_state =
                ScrollbarState::new(total.saturating_sub(visible)).position(scroll);
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

/// Draw mock rules as a side panel (replaces detail panel).
pub fn draw_mock_rules_panel(f: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    app.layout.mock_rule_regions.clear();

    let block = Block::default()
        .title(" Mock Rules ")
        .title_style(Style::default().fg(MAUVE).add_modifier(Modifier::BOLD))
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

    if inner.height < 2 || inner.width < 20 {
        return;
    }

    let rules = app.mock_rules.rules();
    let w = inner.width as usize;

    if rules.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " No mock rules. Press M to create.",
                Style::default().fg(OVERLAY0),
            )))
            .style(Style::default().bg(BASE)),
            ratatui::layout::Rect::new(inner.x, inner.y, inner.width, 1),
        );
        return;
    }

    // Clamp selection
    let selected = app.mock_rule_selected.min(rules.len().saturating_sub(1));
    app.mock_rule_selected = selected;

    let max_rows = inner.height as usize;

    for (i, rule) in rules.iter().take(max_rows).enumerate() {
        let y = inner.y + i as u16;
        let is_selected = i == selected;
        let row_bg = if is_selected { SURFACE1 } else { BASE };
        let text_color = if rule.enabled { TEXT } else { OVERLAY0 };

        let cursor = if is_selected { "\u{25b8} " } else { "  " };
        let enabled_icon = if rule.enabled {
            Span::styled("\u{2713}", Style::default().fg(GREEN).bg(row_bg))
        } else {
            Span::styled("\u{2717}", Style::default().fg(RED).bg(row_bg))
        };

        // URL pattern (truncated)
        let method_str = rule.method.as_deref().unwrap_or("*");
        let info = format!(
            "{}{} {} {}",
            cursor, method_str, rule.status_code, rule.url_pattern
        );
        let info_w = w.saturating_sub(20); // reserve space for buttons
        let info_display = if info.len() > info_w {
            format!("{}...", &info[..info_w.saturating_sub(3)])
        } else {
            format!("{:<width$}", info, width = info_w)
        };

        let buttons_x = inner.x + info_w as u16 + 1;

        let mut spans = vec![
            enabled_icon,
            Span::styled(info_display, Style::default().fg(text_color).bg(row_bg)),
        ];

        // Inline buttons
        spans.push(Span::styled(
            "[Edit]",
            Style::default()
                .fg(MANTLE)
                .bg(GREEN)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(" ", Style::default().bg(row_bg)));

        let toggle_text = if rule.enabled { "[Off]" } else { "[On] " };
        spans.push(Span::styled(
            toggle_text,
            Style::default()
                .fg(MANTLE)
                .bg(YELLOW)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(" ", Style::default().bg(row_bg)));

        spans.push(Span::styled(
            "[Del]",
            Style::default()
                .fg(MANTLE)
                .bg(RED)
                .add_modifier(Modifier::BOLD),
        ));

        // Fill remaining
        let used: usize = spans.iter().map(|s| s.content.width()).sum();
        if used < w {
            spans.push(Span::styled(
                " ".repeat(w - used),
                Style::default().bg(row_bg),
            ));
        }

        f.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(row_bg)),
            ratatui::layout::Rect::new(inner.x, y, inner.width, 1),
        );

        // Register click regions
        app.layout
            .mock_rule_regions
            .push((i, "select".to_string(), y, inner.x, buttons_x));

        let edit_x = buttons_x;
        app.layout
            .mock_rule_regions
            .push((i, "edit".to_string(), y, edit_x, edit_x + 6));

        let toggle_x = edit_x + 7;
        app.layout
            .mock_rule_regions
            .push((i, "toggle".to_string(), y, toggle_x, toggle_x + 5));

        let del_x = toggle_x + 6;
        app.layout
            .mock_rule_regions
            .push((i, "delete".to_string(), y, del_x, del_x + 5));
    }
}
