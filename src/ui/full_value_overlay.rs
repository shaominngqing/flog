//! Full-value overlay renderer (Task 5).
//!
//! Shown when the user activates `ExpandFullValue` on a truncated string
//! node in the JSON detail viewer. The overlay displays the raw string
//! in a centred modal box with scrolling. Press Enter/y to copy or Esc
//! to close.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, AppMode};
use crate::ui::{LAVENDER, SURFACE0, TEXT};

/// Render the full-value overlay on top of the current frame.
///
/// Noop when `app.mode` is not `AppMode::FullValueOverlay`.
/// Must be called **last** in the draw pass so it overlays every other widget.
pub fn draw_full_value_overlay(f: &mut Frame, app: &App) {
    let AppMode::FullValueOverlay(ref state) = app.mode else {
        return;
    };

    let area = f.area();
    let w = (area.width as f32 * 0.70) as u16;
    let h = (area.height as f32 * 0.70) as u16;
    let x = area.width.saturating_sub(w) / 2;
    let y = area.height.saturating_sub(h) / 2;
    let overlay_rect = Rect::new(x, y, w, h);

    let title = Line::from(vec![
        Span::raw(" Full value "),
        Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("/click to copy · "),
        Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" to close "),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(LAVENDER))
        .title(title)
        .style(Style::default().bg(SURFACE0));

    let inner = block.inner(overlay_rect);

    f.render_widget(Clear, overlay_rect);
    f.render_widget(block, overlay_rect);

    let paragraph = Paragraph::new(state.text.clone())
        .style(Style::default().fg(TEXT).bg(SURFACE0))
        .wrap(Wrap { trim: false })
        .scroll((state.scroll as u16, 0));

    f.render_widget(paragraph, inner);
}
