//! Network-view help section: keyboard, mouse, protocol pills, detail
//! sections. Returned as a flat `Vec<Line<'static>>` so `draw_help` can
//! concatenate it into the rendered page.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::ui::{MANTLE, MAUVE, PEACH, SAPPHIRE, SURFACE0, TEXT};

use super::super::{blank, dim, heading, kv, mouse_action, subheading};

#[allow(clippy::vec_init_then_push)]
pub(in crate::ui::help) fn lines() -> Vec<Line<'static>> {
    let mut lines: Vec<Line> = Vec::new();

    lines.push(heading("Network View"));
    lines.push(blank());
    lines.push(subheading("\u{2328} Keyboard"));
    lines.push(kv("j / k", "Move selection up/down"));
    lines.push(kv("G / End", "Jump to bottom (resume LIVE)"));
    lines.push(kv("Enter", "Toggle detail panel"));
    lines.push(kv("/", "Focus Search field (live, a|b, /regex/i)"));
    lines.push(kv("\\", "Focus Exclude field (live, a|b, /regex/i)"));
    lines.push(kv("c", "Copy as cURL (HTTP only)"));
    lines.push(kv("y", "Copy response body / SSE merged / WS chat"));
    lines.push(kv("r", "Replay selected request (HTTP only)"));
    lines.push(kv("M", "Create Mock rule from selected request"));
    lines.push(kv("Ctrl+m", "Open Mock rules management panel"));
    lines.push(kv("E", "Expand all JSON sections"));
    lines.push(kv("C", "Collapse all JSON sections (keep root)"));
    lines.push(kv(
        "J / K",
        "Move cursor down/up in JSON viewer (detail open)",
    ));
    lines.push(kv(
        "Enter",
        "Activate action on cursor row (expand/open/copy/fold)",
    ));
    lines.push(kv("o", "Open URL on cursor row in browser"));
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

    lines
}
