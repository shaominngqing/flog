//! Detail side panel — chrome (header, block, scrollbar) + section-based body.
//!
//! Body content is modelled as an ordered `Vec<Section>` (see `section.rs`);
//! each section is dispatched to a `SectionRenderer` (see `renderers.rs`)
//! that knows how to turn it into styled rows.

mod renderers;
mod section;

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

use renderers::{HeadingRenderer, JsonRenderer, ProseRenderer, StackRenderer};
use section::{Section, SectionRenderer};

const MANTLE: Color = Color::Rgb(30, 32, 48);
const SURFACE0: Color = Color::Rgb(54, 58, 79);
const OVERLAY0: Color = Color::Rgb(110, 115, 141);
const BLUE: Color = Color::Rgb(138, 173, 244);
const TEAL: Color = Color::Rgb(139, 213, 202);
const RED: Color = Color::Rgb(237, 135, 150);
const YELLOW: Color = Color::Rgb(238, 212, 159);
const SAPPHIRE: Color = Color::Rgb(125, 196, 228);

// ══════════════════════════════════════
//  Side Panel Renderer
// ══════════════════════════════════════

pub fn draw_side_panel(f: &mut Frame, app: &mut App, area: Rect) {
    let store_idx = match app.selected_store_index() {
        Some(idx) => idx,
        None => {
            app.layout.detail_copy_btn = None;
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

    // ── Body: section list dispatched to per-type renderers ──
    app.detail.viewer_click_map.clear();

    // When the selected entry changes we want JSON fold state to reset to its
    // default-depth expansion. Two different entries can coincidentally share
    // the same node-count, so rely on a fingerprint of the message body rather
    // than tree shape alone.
    let fingerprint: u64 = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        full_msg.hash(&mut h);
        h.finish()
    };
    if app.detail.viewer_text_fingerprint != fingerprint {
        app.detail.viewer_state = crate::ui::json_viewer::JsonViewerState::default();
        app.detail.viewer_text_fingerprint = fingerprint;
    }
    // Clear any cached tree — JsonRenderer re-populates it if a JSON section exists.
    app.detail.viewer_tree = None;

    let sections = section::build_sections(&full_msg);
    let mut body_rows: Vec<section::RenderRow> = Vec::new();
    for sec in &sections {
        let rows = match sec {
            Section::Prose(_) => ProseRenderer.render(sec, inner_w, &mut app.detail),
            Section::Heading(_) => HeadingRenderer.render(sec, inner_w, &mut app.detail),
            Section::StackTrace(_) => StackRenderer.render(sec, inner_w, &mut app.detail),
            Section::Json { .. } => JsonRenderer.render(sec, inner_w, &mut app.detail),
        };
        body_rows.extend(rows);
    }

    let body_height = inner_h.saturating_sub(all_lines.len());
    let full_body_len = body_rows.len();
    let scroll = app.detail.scroll.min(full_body_len);

    app.detail.viewer_click_map = body_rows
        .iter()
        .skip(scroll)
        .take(body_height)
        .map(|r| r.hot_regions.clone())
        .collect();

    let mut visible: Vec<Line<'static>> = body_rows
        .into_iter()
        .skip(scroll)
        .take(body_height)
        .map(|r| r.line)
        .collect();

    // Highlight the cursor row with SURFACE0 background.
    if let Some(cursor) = app.detail.viewer_cursor {
        if let Some(line) = visible.get_mut(cursor) {
            let highlighted_spans: Vec<ratatui::text::Span<'static>> = line
                .spans
                .iter()
                .map(|s| {
                    let style = s.style.patch(Style::default().bg(SURFACE0));
                    ratatui::text::Span::styled(s.content.clone(), style)
                })
                .collect();
            *line = Line::from(highlighted_spans);
        }
    }

    all_lines.extend(visible);

    let total_content = app.detail.header_lines + full_body_len;

    // Clickable [ c Copy ] pill pinned to the right side of the title row.
    // The BLUE pill sits on top of the border (y == area.y), so record its
    // screen rect so the mouse handler can route hits to copy_current_log.
    let copy_label = " Copy ";
    let copy_w = copy_label.chars().count() as u16;
    let copy_x_end = area.x + area.width;
    let copy_x_start = copy_x_end.saturating_sub(copy_w);
    app.layout.detail_copy_btn = Some((area.y, copy_x_start, copy_x_end));

    let block = Block::default()
        .title(Span::styled(
            " Details ",
            Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
        ))
        .title(
            Line::from(Span::styled(
                copy_label,
                Style::default()
                    .fg(MANTLE)
                    .bg(SAPPHIRE)
                    .add_modifier(Modifier::BOLD),
            ))
            .right_aligned(),
        )
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
