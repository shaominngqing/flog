//! Help overlay — comprehensive, visual guide to all flog features.

use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

// Catppuccin Macchiato palette
const BASE: Color = Color::Rgb(36, 39, 58);
const MANTLE: Color = Color::Rgb(30, 32, 48);
const SURFACE0: Color = Color::Rgb(54, 58, 79);
const OVERLAY0: Color = Color::Rgb(110, 115, 141);
const TEXT: Color = Color::Rgb(202, 211, 245);
const BLUE: Color = Color::Rgb(138, 173, 244);
const GREEN: Color = Color::Rgb(166, 218, 149);
const YELLOW: Color = Color::Rgb(238, 212, 159);
const PEACH: Color = Color::Rgb(245, 169, 127);
const RED: Color = Color::Rgb(237, 135, 150);
const MAUVE: Color = Color::Rgb(198, 160, 246);
const SAPPHIRE: Color = Color::Rgb(125, 196, 228);

fn key(s: &str) -> Span<'static> {
    Span::styled(
        format!(" {} ", s),
        Style::default()
            .fg(MANTLE)
            .bg(YELLOW)
            .add_modifier(Modifier::BOLD),
    )
}

fn label(s: &str) -> Span<'static> {
    Span::styled(s.to_string(), Style::default().fg(TEXT))
}

fn dim(s: &str) -> Span<'static> {
    Span::styled(s.to_string(), Style::default().fg(OVERLAY0))
}

fn heading(s: &str) -> Line<'static> {
    Line::from(vec![Span::styled(
        format!("  \u{25c6} {}", s),
        Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
    )])
}

fn subheading(s: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("    {}", s),
        Style::default().fg(SAPPHIRE),
    ))
}

fn blank() -> Line<'static> {
    Line::raw("")
}

fn kv(k: &str, v: &str) -> Line<'static> {
    Line::from(vec![Span::raw("    "), key(k), Span::raw("  "), label(v)])
}

fn mouse_action(action: &str, result: &str) -> Line<'static> {
    Line::from(vec![
        Span::raw("    "),
        Span::styled(format!("{:<22}", action), Style::default().fg(PEACH)),
        label(result),
    ])
}

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

    // ════════════════════════════════════
    //  Banner
    // ════════════════════════════════════
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

    // ════════════════════════════════════
    //  Tabs
    // ════════════════════════════════════
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

    // ════════════════════════════════════
    //  Logs View
    // ════════════════════════════════════
    lines.push(heading("Logs View"));
    lines.push(blank());
    lines.push(subheading("\u{2328} Keyboard"));
    lines.push(kv("j / k", "Move selection up/down"));
    lines.push(kv("\u{2191} \u{2193}", "Move selection up/down"));
    lines.push(kv("PgUp/Dn", "Scroll 20 entries"));
    lines.push(kv("Home", "Jump to top"));
    lines.push(kv("G / End", "Jump to bottom (resume LIVE)"));
    lines.push(kv("/", "Search (supports /regex/i)"));
    lines.push(kv("n / N", "Next / previous match"));
    lines.push(kv("t", "Enter tag filter (comma-sep, -tag to exclude)"));
    lines.push(kv("Enter", "Toggle detail panel"));
    lines.push(kv("c", "Copy selected log to clipboard"));
    lines.push(kv("e", "Export filtered logs to file"));
    lines.push(kv("S", "Statistics view"));
    lines.push(kv("s", "Select mode (terminal text selection)"));
    lines.push(kv("Esc", "Clear all filters"));
    lines.push(kv("q", "Quit"));
    lines.push(blank());

    lines.push(subheading("\u{1f5b1} Mouse"));
    lines.push(mouse_action("Click row", "Select log entry"));
    lines.push(mouse_action("Click same row again", "Toggle detail panel"));
    lines.push(mouse_action("Right-click row", "Toggle bookmark \u{25cf}"));
    lines.push(mouse_action("Scroll wheel", "Scroll log list"));
    lines.push(mouse_action("Click S/V/D/I/W/E", "Set minimum log level"));
    lines.push(mouse_action(
        "Click search / # tag",
        "Start search / tag filter",
    ));
    lines.push(mouse_action(
        "Click \u{21c5} app context",
        "Switch connected app",
    ));
    lines.push(mouse_action(
        "Click Jump-to-bottom pill",
        "Scroll to tail, resume LIVE",
    ));
    lines.push(blank());

    lines.push(subheading("\u{1f50d} Search & Filter"));
    lines.push(Line::from(vec![
        Span::raw("    "),
        dim("Search:  "),
        key("/"),
        dim(" type query "),
        key("Enter"),
        dim("    /regex/i for case-insensitive regex"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("    "),
        dim("Tag:     "),
        key("t"),
        dim(" type "),
        Span::styled("tag1,tag2,-excluded", Style::default().fg(GREEN)),
        dim("    comma-separated, - to exclude"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("    "),
        dim("Level:   click "),
        Span::styled(" S ", Style::default().fg(OVERLAY0).bg(SURFACE0)),
        Span::styled(" V ", Style::default().fg(OVERLAY0).bg(SURFACE0)),
        Span::styled(" D ", Style::default().fg(TEXT).bg(SURFACE0)),
        Span::styled(
            " I ",
            Style::default()
                .fg(MANTLE)
                .bg(BLUE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " W ",
            Style::default()
                .fg(MANTLE)
                .bg(YELLOW)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " E ",
            Style::default()
                .fg(MANTLE)
                .bg(RED)
                .add_modifier(Modifier::BOLD),
        ),
        dim("  to set minimum level"),
    ]));
    lines.push(blank());

    lines.push(subheading("\u{1f4cb} Detail Panel"));
    lines.push(Line::from(vec![
        Span::raw("    "),
        dim("JSON content is rendered as a collapsible tree:"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("      "),
        Span::styled("\u{25bc} ", Style::default().fg(BLUE)),
        Span::styled("{", Style::default().fg(OVERLAY0)),
        dim("                  click "),
        Span::styled("\u{25bc}", Style::default().fg(BLUE)),
        dim(" to collapse"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("      "),
        Span::styled("\u{25b6} ", Style::default().fg(BLUE)),
        Span::styled("{...}", Style::default().fg(OVERLAY0)),
        Span::styled(" (3)", Style::default().fg(OVERLAY0)),
        dim("          click "),
        Span::styled("\u{25b6}", Style::default().fg(BLUE)),
        dim(" to expand"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("    "),
        dim("Scroll detail panel when mouse is over it"),
    ]));
    lines.push(blank());

    // ════════════════════════════════════
    //  Network View
    // ════════════════════════════════════
    lines.push(heading("Network View"));
    lines.push(blank());
    lines.push(subheading("\u{2328} Keyboard"));
    lines.push(kv("j / k", "Move selection up/down"));
    lines.push(kv("G / End", "Jump to bottom (resume LIVE)"));
    lines.push(kv("Enter", "Toggle detail panel"));
    lines.push(kv("/", "Filter by URL"));
    lines.push(kv("c", "Copy as cURL (HTTP only)"));
    lines.push(kv("y", "Copy response body / SSE merged / WS chat"));
    lines.push(kv("r", "Replay selected request (HTTP only)"));
    lines.push(kv("M", "Create Mock rule from selected request"));
    lines.push(kv("Ctrl+m", "Open Mock rules management panel"));
    lines.push(kv("E", "Expand all JSON sections"));
    lines.push(kv("C", "Collapse all JSON sections (keep root)"));
    lines.push(kv("S", "Performance stats"));
    lines.push(kv("s", "Select mode (text selection)"));
    lines.push(kv("Esc", "Clear all filters"));
    lines.push(blank());

    lines.push(subheading("\u{1f5b1} Mouse"));
    lines.push(mouse_action("Click request", "Select and open detail"));
    lines.push(mouse_action(
        "Click filter pills",
        "Toggle protocol/method/status filter",
    ));
    lines.push(mouse_action(
        "Click action buttons",
        "Replay / Copy cURL / Copy Response / Mock / Stats",
    ));
    lines.push(mouse_action("Scroll on detail", "Scroll detail content"));
    lines.push(blank());

    lines.push(subheading("\u{1f50c} Protocol Support"));
    lines.push(Line::from(vec![
        Span::raw("    "),
        Span::styled(" HTTP ", Style::default().fg(TEXT).bg(SURFACE0)),
        dim("  Standard requests (GET, POST, PUT, DELETE, PATCH)"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("    "),
        Span::styled(
            " SSE ",
            Style::default()
                .fg(MANTLE)
                .bg(PEACH)
                .add_modifier(Modifier::BOLD),
        ),
        dim("   Server-Sent Events streaming"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("    "),
        Span::styled(
            " WS ",
            Style::default()
                .fg(MANTLE)
                .bg(MAUVE)
                .add_modifier(Modifier::BOLD),
        ),
        dim("    WebSocket bidirectional messages"),
    ]));
    lines.push(blank());

    lines.push(subheading(
        "\u{1f4e6} Detail Sections (click to expand/collapse)",
    ));
    lines.push(Line::from(vec![
        Span::raw("    "),
        Span::styled(
            "\u{25bc} General",
            Style::default().fg(SAPPHIRE).add_modifier(Modifier::BOLD),
        ),
        dim("          URL, Method, Status, Duration, Size"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("    "),
        Span::styled(
            "\u{25b6} Query Parameters",
            Style::default().fg(SAPPHIRE).add_modifier(Modifier::BOLD),
        ),
        dim("  Parsed from URL ?key=value"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("    "),
        Span::styled(
            "\u{25b6} Request Headers",
            Style::default().fg(SAPPHIRE).add_modifier(Modifier::BOLD),
        ),
        dim("   JSON with fold/unfold"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("    "),
        Span::styled(
            "\u{25b6} Response Body",
            Style::default().fg(SAPPHIRE).add_modifier(Modifier::BOLD),
        ),
        dim("     JSON pretty-print with syntax colors"),
    ]));
    lines.push(blank());

    // ════════════════════════════════════
    //  Setup
    // ════════════════════════════════════
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

    // ════════════════════════════════════
    //  Tips
    // ════════════════════════════════════
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
