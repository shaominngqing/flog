//! Help overlay — comprehensive, visual guide to all flog features.
//!
//! Split by section (banner/tabs/setup/tips in this file; per-view content
//! under `content/`). Shared line builders live here so every section can
//! reuse the same `key`/`label`/`dim`/`heading`/`kv`/`mouse_action` helpers.

mod content;

use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::ui::{BASE, BLUE, GREEN, MANTLE, OVERLAY0, SAPPHIRE, TEXT, YELLOW};

// ── Shared inline builders ─────────────────────────────────────────
// These are the small primitives each section uses to compose Lines.

pub(super) fn key(s: &str) -> Span<'static> {
    Span::styled(
        format!(" {} ", s),
        Style::default()
            .fg(MANTLE)
            .bg(YELLOW)
            .add_modifier(Modifier::BOLD),
    )
}

pub(super) fn label(s: &str) -> Span<'static> {
    Span::styled(s.to_string(), Style::default().fg(TEXT))
}

pub(super) fn dim(s: &str) -> Span<'static> {
    Span::styled(s.to_string(), Style::default().fg(OVERLAY0))
}

pub(super) fn heading(s: &str) -> Line<'static> {
    Line::from(vec![Span::styled(
        format!("  \u{25c6} {}", s),
        Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
    )])
}

pub(super) fn subheading(s: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("    {}", s),
        Style::default().fg(SAPPHIRE),
    ))
}

pub(super) fn blank() -> Line<'static> {
    Line::raw("")
}

pub(super) fn kv(k: &str, v: &str) -> Line<'static> {
    Line::from(vec![Span::raw("    "), key(k), Span::raw("  "), label(v)])
}

pub(super) fn mouse_action(action: &str, result: &str) -> Line<'static> {
    Line::from(vec![
        Span::raw("    "),
        Span::styled(
            format!("{:<22}", action),
            Style::default().fg(crate::ui::PEACH),
        ),
        label(result),
    ])
}

// ── Entry point ────────────────────────────────────────────────────

#[allow(clippy::vec_init_then_push)]
pub fn draw_help(f: &mut Frame) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(f.area());

    // ── Nav bar ──
    let w = f.area().width as usize;
    let nav = Line::from(vec![
        Span::styled(
            " \u{2190} Back ",
            Style::default()
                .fg(MANTLE)
                .bg(BLUE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ", Style::default().bg(MANTLE)),
        Span::styled(
            "flog Help",
            Style::default()
                .fg(TEXT)
                .bg(MANTLE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " ".repeat(w.saturating_sub(20)),
            Style::default().bg(MANTLE),
        ),
    ]);
    f.render_widget(Paragraph::new(nav), chunks[0]);

    // ── Content ──
    let mut lines: Vec<Line> = Vec::new();

    // Banner
    lines.push(blank());
    lines.push(Line::from(vec![
        Span::styled(
            "  flog ",
            Style::default()
                .fg(MANTLE)
                .bg(BLUE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " Flutter Log Viewer",
            Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(dim(
        "  Terminal-native log viewer for Flutter developers",
    )));
    lines.push(blank());

    // Tabs
    lines.push(heading("Tab Navigation"));
    lines.push(blank());
    lines.push(Line::from(vec![
        Span::raw("    "),
        Span::styled(
            " \u{25a4} Logs ",
            Style::default()
                .fg(MANTLE)
                .bg(BLUE)
                .add_modifier(Modifier::BOLD),
        ),
        dim("  Real-time log stream with filtering    "),
        key("1"),
        dim(" or click"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("    "),
        Span::styled(
            " \u{21c4} Network ",
            Style::default()
                .fg(MANTLE)
                .bg(SAPPHIRE)
                .add_modifier(Modifier::BOLD),
        ),
        dim("  HTTP/SSE/WS request inspector     "),
        key("2"),
        dim(" or click"),
    ]));
    lines.push(blank());

    // Per-view content (keyboard, mouse, search, detail for Logs; similar for Network).
    lines.extend(content::logs::lines());
    lines.extend(content::network::lines());

    // Setup
    lines.push(heading("Setup (Dart)"));
    lines.push(blank());
    lines.push(Line::from(vec![
        Span::raw("    "),
        dim("Add to your Dio instance:"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("    "),
        Span::styled("dio.interceptors.add(", Style::default().fg(TEXT)),
        Span::styled("FlogHttpInterceptor()", Style::default().fg(GREEN)),
        Span::styled(");", Style::default().fg(TEXT)),
    ]));
    lines.push(Line::from(vec![
        Span::raw("    "),
        Span::styled("\u{26a0} ", Style::default().fg(YELLOW)),
        dim("Must be added BEFORE response-modifying interceptors"),
    ]));
    lines.push(blank());
    lines.push(Line::from(vec![
        Span::raw("    "),
        dim("For SSE:  "),
        Span::styled(
            "FlogSseParser.wrap(stream, url: url)",
            Style::default().fg(GREEN),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::raw("    "),
        dim("For WS:   "),
        Span::styled("FlogWebSocket.connect(url)", Style::default().fg(GREEN)),
    ]));
    lines.push(blank());

    // Tips
    lines.push(heading("Tips"));
    lines.push(blank());
    lines.push(Line::from(vec![
        Span::raw("    "),
        Span::styled("\u{2022} ", Style::default().fg(GREEN)),
        dim("Filters and bookmarks persist between sessions"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("    "),
        Span::styled("\u{2022} ", Style::default().fg(GREEN)),
        dim("Option+drag (macOS) for quick text selection anytime"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("    "),
        Span::styled("\u{2022} ", Style::default().fg(GREEN)),
        dim("flog auto-reconnects when Flutter app restarts"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("    "),
        Span::styled("\u{2022} ", Style::default().fg(GREEN)),
        dim("Ring buffer holds 100K logs, network stores 10K requests"),
    ]));
    lines.push(blank());

    lines.push(Line::from(vec![
        Span::raw("  "),
        dim("Press "),
        key("Esc"),
        dim(" or "),
        key("?"),
        dim(" or click "),
        Span::styled(
            " \u{2190} Back ",
            Style::default()
                .fg(MANTLE)
                .bg(BLUE)
                .add_modifier(Modifier::BOLD),
        ),
        dim(" to close"),
    ]));
    lines.push(blank());

    f.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::NONE)
                    .style(Style::default().bg(BASE)),
            )
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    // UI-013: help overlay must use the shared crate::ui palette, not private constants.
    // The help content area (chunks[1]) paints its background with BASE.
    #[test]
    fn help_content_bg_uses_shared_base_palette() {
        let backend = TestBackend::new(80, 30);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(draw_help).unwrap();
        let buf = term.backend().buffer().clone();
        // Row 0 is the nav bar (MANTLE bg); row 1 onwards is the content block on BASE.
        // Pick a column in the middle of a content row.
        let cell = &buf[(5, 5)];
        assert_eq!(
            cell.style().bg,
            Some(crate::ui::BASE),
            "help content row should use shared BASE palette color, got {:?}",
            cell.style().bg
        );
    }
}
