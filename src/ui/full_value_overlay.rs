//! Full-value overlay renderer (Task 5).
//!
//! Shown when the user activates `ExpandFullValue` on a truncated string
//! node in the JSON detail viewer. The overlay displays the raw string
//! in a centred modal box with scrolling.
//!
//! ## Interaction
//! - Click `[ ✓ Copy ]` button → copies text + closes.
//! - Press `Enter` / `y` → copies text + closes.
//! - Clicking anywhere else inside the modal → no action.
//! - Clicking outside the modal or pressing `Esc` → closes without copying.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, AppMode};
use crate::ui::{LAVENDER, MANTLE, OVERLAY0, SURFACE0, SURFACE1, TEXT};

/// Width of the `[ ✓ Copy ]` button text (10 terminal columns).
pub(crate) const COPY_BTN_TEXT: &str = " [ ✓ Copy ] ";
/// Horizontal offset from the modal's left edge where the button starts.
pub(crate) const COPY_BTN_X_OFFSET: u16 = 2;

/// Compute the overlay rect (mirrors the renderer so the mouse handler can
/// use the same geometry without reading layout cache).
pub(crate) fn overlay_rect(terminal_area: Rect) -> Rect {
    let w = (terminal_area.width as f32 * 0.70) as u16;
    let h = (terminal_area.height as f32 * 0.70) as u16;
    let x = terminal_area.width.saturating_sub(w) / 2;
    let y = terminal_area.height.saturating_sub(h) / 2;
    Rect::new(x, y, w, h)
}

/// Render the full-value overlay on top of the current frame.
///
/// Noop when `app.mode` is not `AppMode::FullValueOverlay`.
/// Must be called **last** in the draw pass so it overlays every other widget.
pub fn draw_full_value_overlay(f: &mut Frame, app: &App) {
    let AppMode::FullValueOverlay(ref state) = app.mode else {
        return;
    };

    let overlay_rect = overlay_rect(f.area());

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(LAVENDER))
        .title(Line::from(Span::raw(" Full value ")))
        .style(Style::default().bg(SURFACE0));

    let inner = block.inner(overlay_rect);

    f.render_widget(Clear, overlay_rect);
    f.render_widget(block, overlay_rect);

    // Reserve 2 bottom rows for: separator + button strip.
    // If the modal is too tiny to show anything useful, skip the split.
    let (text_area, separator_y, button_y) = if inner.height >= 4 {
        let text_h = inner.height - 2;
        let text_area = Rect::new(inner.x, inner.y, inner.width, text_h);
        let sep_y = inner.y + text_h;
        let btn_y = inner.y + text_h + 1;
        (text_area, Some(sep_y), Some(btn_y))
    } else {
        (inner, None, None)
    };

    // ── Scrollable text ──
    let paragraph = Paragraph::new(state.text.clone())
        .style(Style::default().fg(TEXT).bg(SURFACE0))
        .wrap(Wrap { trim: false })
        .scroll((state.scroll as u16, 0));
    f.render_widget(paragraph, text_area);

    // ── Separator ──
    if let Some(sep_y) = separator_y {
        let sep_line = Line::from(Span::styled(
            "\u{2500}".repeat(inner.width as usize),
            Style::default().fg(SURFACE1),
        ));
        let sep_rect = Rect::new(inner.x, sep_y, inner.width, 1);
        f.render_widget(Paragraph::new(sep_line), sep_rect);
    }

    // ── Button row ──
    if let Some(btn_y) = button_y {
        let btn_rect = Rect::new(inner.x, btn_y, inner.width, 1);
        let btn_w = COPY_BTN_TEXT.width() as u16;
        let hint = "  j/k scroll · Enter copy · Esc close";
        let hint_clipped = if inner.width > COPY_BTN_X_OFFSET + btn_w + 2 {
            let avail = (inner.width - COPY_BTN_X_OFFSET - btn_w) as usize;
            if hint.width() > avail {
                &hint[..avail]
            } else {
                hint
            }
        } else {
            ""
        };

        let padding = " ".repeat(COPY_BTN_X_OFFSET as usize);
        let row = Line::from(vec![
            Span::raw(padding),
            Span::styled(
                COPY_BTN_TEXT,
                Style::default()
                    .fg(MANTLE)
                    .bg(LAVENDER)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(hint_clipped, Style::default().fg(OVERLAY0)),
        ]);
        f.render_widget(Paragraph::new(row), btn_rect);
    }
}
