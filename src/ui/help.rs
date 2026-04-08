//! Help overlay.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};

const ACCENT: Color = Color::Rgb(138, 173, 244);
const BAR_BG: Color = Color::Rgb(30, 32, 48);

pub fn draw_help(f: &mut Frame) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(f.area());

    // ── Unified nav bar ──
    let w = f.area().width as usize;
    let nav = Line::from(vec![
        Span::styled(" < Back ", Style::default().fg(Color::Black).bg(ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled("  Help  ", Style::default().fg(Color::White).bg(BAR_BG)),
        Span::styled(" ".repeat(w.saturating_sub(16)), Style::default().bg(BAR_BG)),
    ]);
    f.render_widget(Paragraph::new(nav), chunks[0]);

    let sections = vec![
        ("Mouse", vec![
            ("Click log row", "Select"),
            ("Double-click", "Open detail (JSON pretty-print)"),
            ("Right-click row", "Toggle bookmark"),
            ("Scroll wheel", "Scroll up/down"),
            ("Click search box", "Start search input"),
            ("Click tag box", "Start tag filter input"),
            ("Click V/D/I/W/E", "Set minimum log level"),
            ("Click < >", "Prev/next search match"),
        ]),
        ("Keyboard", vec![
            ("j/k or arrows", "Move selection"),
            ("PgUp / PgDn", "Scroll page"),
            ("/ then type", "Search (or /regex/i for regex)"),
            ("n / N", "Next / prev match"),
            ("Enter", "Open detail view"),
            ("Esc", "Close overlay / clear filters"),
            ("e", "Export filtered logs to file"),
            ("q / Ctrl+C", "Quit"),
        ]),
        ("Detail View", vec![
            ("Click triangle", "Fold/unfold JSON section"),
            ("Copy button", "Copy message to clipboard"),
            ("Right-click", "Close detail view"),
        ]),
        ("Tips", vec![
            ("Search /regex/i", "Use /pattern/i for case-insensitive regex"),
            ("Tag filter", "Comma-separated, prefix - to exclude (e.g., -Clog)"),
            ("Tag regex", "Use * or . in tag filter for regex mode"),
            ("Session", "Filters and bookmarks are saved between sessions"),
        ]),
    ];

    let mut lines: Vec<Line> = vec![Line::raw("")];
    for (title, items) in &sections {
        lines.push(Line::from(Span::styled(
            format!("  {}", title),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::raw(""));
        for (key, desc) in items {
            lines.push(Line::from(vec![
                Span::styled(format!("    {:<24}", key), Style::default().fg(Color::Rgb(238, 212, 159))),
                Span::raw(*desc),
            ]));
        }
        lines.push(Line::raw(""));
    }

    f.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(Color::Rgb(54, 58, 79))))
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
}
