//! Detail side panel — shows selected log entry with JSON formatting and fold/unfold.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
    },
    Frame,
};

use crate::app::App;
use crate::domain::LogLevel;
use crate::ui::json_viewer;

const MANTLE: Color = Color::Rgb(30, 32, 48);
const SURFACE0: Color = Color::Rgb(54, 58, 79);
const OVERLAY0: Color = Color::Rgb(110, 115, 141);
const TEXT: Color = Color::Rgb(202, 211, 245);
const BLUE: Color = Color::Rgb(138, 173, 244);
const TEAL: Color = Color::Rgb(139, 213, 202);
const YELLOW: Color = Color::Rgb(238, 212, 159);
const RED: Color = Color::Rgb(237, 135, 150);
const SAPPHIRE: Color = Color::Rgb(125, 196, 228);

// ══════════════════════════════════════
//  Side Panel Renderer
// ══════════════════════════════════════

pub fn draw_side_panel(f: &mut Frame, app: &mut App, area: Rect) {
    let store_idx = match app.selected_store_index() {
        Some(idx) => idx,
        None => {
            let block = Block::default()
                .title(" Details ")
                .borders(Borders::LEFT)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(SURFACE0))
                .style(Style::default().bg(MANTLE));
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "  Select a log entry",
                    Style::default().fg(OVERLAY0),
                )))
                .block(block),
                area,
            );
            return;
        }
    };

    let entry = match app.store.get(store_idx) {
        Some(e) => e.clone(),
        None => return,
    };

    let inner_h = area.height.saturating_sub(2) as usize;
    let inner_w = area.width.saturating_sub(2) as usize;

    // ── Header ──
    let (lfg, lbg) = match entry.level {
        LogLevel::Info => (MANTLE, BLUE),
        LogLevel::Warning => (MANTLE, YELLOW),
        LogLevel::Error => (MANTLE, RED),
        _ => (OVERLAY0, Color::Reset),
    };
    let ls = if lbg == Color::Reset {
        Style::default().fg(lfg)
    } else {
        Style::default()
            .fg(lfg)
            .bg(lbg)
            .add_modifier(Modifier::BOLD)
    };

    let mut all_lines: Vec<Line> = Vec::new();
    all_lines.push(Line::from(vec![
        Span::styled(format!(" {} ", entry.level.as_str()), ls),
        Span::styled(format!("  {}", entry.tag), Style::default().fg(TEAL)),
    ]));
    if !entry.timestamp.is_empty() {
        all_lines.push(Line::from(Span::styled(
            format!("  {}", entry.timestamp),
            Style::default().fg(OVERLAY0),
        )));
    }
    // Message length info
    let full_msg = entry.full_message();
    let msg_len = full_msg.len();
    let len_display = if msg_len >= 1024 * 1024 {
        format!("{:.1} MB", msg_len as f64 / (1024.0 * 1024.0))
    } else if msg_len >= 1024 {
        format!("{:.1} KB", msg_len as f64 / 1024.0)
    } else {
        format!("{} B", msg_len)
    };
    all_lines.push(Line::from(Span::styled(
        format!("  Length: {}", len_display),
        Style::default().fg(OVERLAY0),
    )));
    all_lines.push(Line::from(Span::styled(
        "\u{2500}".repeat(inner_w),
        Style::default().fg(SURFACE0),
    )));

    // Store header line count for click handling (+ 1 for block border top)
    app.detail.header_lines = all_lines.len() + 1;

    // ── Body with fold/unfold using json_viewer ──
    app.detail.viewer_click_map.clear();
    let total_content;
    match json_viewer::parse(&full_msg) {
        Ok(tree) => {
            // (Re-)initialize state if tree size changed (new entry selected).
            if app.detail.viewer_state.expanded.len() != tree.nodes.len() {
                app.detail.viewer_state = json_viewer::init_state(&tree, 1);
            }

            let body_height = inner_h.saturating_sub(all_lines.len());
            let mut body_click_map: Vec<Option<(String, u32)>> = Vec::new();
            let mut body_lines: Vec<Line<'static>> = Vec::new();
            json_viewer::append_render(
                &mut body_lines,
                &mut body_click_map,
                &tree,
                &app.detail.viewer_state,
                "log_detail",
                "",
                inner_w,
            );

            let full_body_len = body_lines.len();
            let scroll = app.detail.scroll.min(full_body_len);
            // Store click map aligned with visible rows (after scroll).
            app.detail.viewer_click_map = body_click_map
                .iter()
                .skip(scroll)
                .take(body_height)
                .map(|slot| slot.as_ref().map(|(_, id)| *id))
                .collect();

            let visible: Vec<Line<'static>> = body_lines
                .into_iter()
                .skip(scroll)
                .take(body_height)
                .collect();
            all_lines.extend(visible);

            app.detail.viewer_tree = Some(tree);
            total_content = app.detail.header_lines + full_body_len;
        }
        Err(_) => {
            for wl in crate::ui::wrap_text(&full_msg, inner_w, 500) {
                all_lines.push(Line::from(Span::styled(
                    wl,
                    Style::default().fg(TEXT),
                )));
            }
            app.detail.viewer_tree = None;
            total_content = all_lines.len();
        }
    }

    let block = Block::default()
        .title(" Details ")
        .title_style(Style::default().fg(BLUE).add_modifier(Modifier::BOLD))
        .borders(Borders::LEFT)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(SURFACE0))
        .style(Style::default().bg(MANTLE));

    f.render_widget(
        Paragraph::new(all_lines)
            .block(block)
            .wrap(ratatui::widgets::Wrap { trim: false }),
        area,
    );

    // Scrollbar
    if total_content > inner_h {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_symbol("\u{2503}") // ┃
            .track_symbol(Some(" "))
            .thumb_style(Style::default().fg(SAPPHIRE))
            .track_style(Style::default().fg(SURFACE0).bg(MANTLE))
            .begin_symbol(None)
            .end_symbol(None);
        let max_scroll = total_content.saturating_sub(inner_h);
        let mut state = ScrollbarState::new(max_scroll).position(app.detail.scroll.min(max_scroll));
        f.render_stateful_widget(scrollbar, area, &mut state);
    }
}
