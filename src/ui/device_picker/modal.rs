//! Modal-frame helpers for the device picker: the outer rounded block,
//! the empty-state ("No devices found") render, and the scrollbar
//! overlay. The orchestration happens in [`super::draw_device_picker`].

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState,
    },
    Frame,
};
use unicode_width::UnicodeWidthStr;

use super::palette::{BASE, OVERLAY0, SAPPHIRE, SURFACE0, SURFACE1};

/// Compute the centered modal rectangle that the picker renders into.
pub(super) fn compute_modal_area(area: Rect) -> Rect {
    let picker_w = (area.width * 2 / 3)
        .max(60)
        .min(area.width.saturating_sub(4));
    let picker_h = (area.height * 3 / 4).max(8);
    let picker_x = (area.width.saturating_sub(picker_w)) / 2;
    let picker_y = (area.height.saturating_sub(picker_h)) / 2;
    Rect::new(picker_x, picker_y, picker_w, picker_h)
}

/// Build the outer block (title, hints, rounded borders, base bg).
pub(super) fn build_modal_block(device_count: usize) -> Block<'static> {
    let title_top = format!(" Devices ({}) ", device_count);
    let hints = " ↑↓ navigate  ⏎ connect  esc cancel ";
    Block::default()
        .title(title_top)
        .title_style(Style::default().fg(SAPPHIRE).add_modifier(Modifier::BOLD))
        .title_bottom(
            Line::from(Span::styled(hints, Style::default().fg(OVERLAY0))).right_aligned(),
        )
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(SURFACE1))
        .style(Style::default().bg(BASE))
}

/// Clear the modal rect and optionally render the empty-state body.
///
/// Returns `true` when the empty-state branch handled rendering and the
/// caller should return early.
pub(super) fn render_empty_state(
    f: &mut Frame,
    picker_area: Rect,
    block: Block<'static>,
    inner_w: usize,
    inner_h: usize,
) {
    let bg = BASE;
    let center_text = |text: &str, fg: ratatui::style::Color| -> Line<'static> {
        let pad_l = inner_w.saturating_sub(text.width()) / 2;
        let pad_r = inner_w.saturating_sub(pad_l + text.width());
        Line::from(vec![
            Span::styled(" ".repeat(pad_l), Style::default().bg(bg)),
            Span::styled(text.to_string(), Style::default().fg(fg).bg(bg)),
            Span::styled(" ".repeat(pad_r), Style::default().bg(bg)),
        ])
    };
    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(" ".repeat(inner_w), Style::default().bg(bg))),
        center_text("No devices found", OVERLAY0),
        Line::from(Span::styled(" ".repeat(inner_w), Style::default().bg(bg))),
        center_text("Run your Flutter app with flog_dart", SURFACE1),
    ];
    while lines.len() < inner_h {
        lines.push(Line::from(Span::styled(
            " ".repeat(inner_w),
            Style::default().bg(bg),
        )));
    }
    f.render_widget(Paragraph::new(lines).block(block), picker_area);
}

/// Render the right-edge scrollbar when the content exceeds the viewport.
pub(super) fn render_scrollbar(
    f: &mut Frame,
    inner: Rect,
    max_scroll: usize,
    scroll_offset: usize,
) {
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .thumb_style(Style::default().fg(SAPPHIRE))
        .track_style(Style::default().fg(SURFACE0));
    let mut state = ScrollbarState::new(max_scroll).position(scroll_offset);
    f.render_stateful_widget(scrollbar, inner, &mut state);
}

/// Clear the modal area to ensure clean overlay rendering.
pub(super) fn clear_modal_area(f: &mut Frame, picker_area: Rect) {
    f.render_widget(Clear, picker_area);
}
