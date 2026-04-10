//! Tab selector — rendered inline at the start of each view's toolbar.
//!
//! Instead of a dedicated tab row, each view's toolbar begins with the tab
//! selector spans. This saves one row of vertical space and follows the
//! lazygit pattern of embedding navigation in panel chrome.

use ratatui::{
    style::{Modifier, Style},
    text::Span,
};
use crate::app::{App, ViewTab};
use super::{MANTLE, SURFACE0, BLUE, OVERLAY0, GREEN};

/// Returns the tab selector spans and their total width.
/// These should be prepended to each view's toolbar line.
pub fn tab_spans(app: &mut App, bg: super::Color) -> (Vec<Span<'static>>, u16) {
    let logs_style = if app.active_tab == ViewTab::Logs {
        Style::default().fg(BLUE).bg(bg).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(OVERLAY0).bg(bg)
    };
    let net_style = if app.active_tab == ViewTab::Network {
        Style::default().fg(BLUE).bg(bg).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(OVERLAY0).bg(bg)
    };
    let sep_style = Style::default().fg(SURFACE0).bg(bg);

    // " Logs │ Network  "
    let spans = vec![
        Span::styled(" ", Style::default().bg(bg)),
        Span::styled("Logs", logs_style),
        Span::styled(" │ ", sep_style),
        Span::styled("Network", net_style),
        Span::styled("  ", Style::default().bg(bg)),
    ];

    // Click regions: " Logs" = x 1..5, "Network" = x 8..15
    app.layout.tab_logs_x = (1, 5);
    app.layout.tab_network_x = (8, 15);

    let width: u16 = 1 + 4 + 3 + 7 + 2; // 17
    (spans, width)
}
