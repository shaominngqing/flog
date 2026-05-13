//! Logs-view help section: keyboard, mouse, search & filter syntax,
//! detail panel. Returned as a flat `Vec<Line<'static>>` so `draw_help`
//! can concatenate it into the rendered page.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::ui::{BLUE, GREEN, MANTLE, OVERLAY0, RED, SAPPHIRE, SURFACE0, TEXT, YELLOW};

use super::super::{blank, dim, heading, key, kv, mouse_action, subheading};

#[allow(clippy::vec_init_then_push)]
pub(in crate::ui::help) fn lines() -> Vec<Line<'static>> {
    let mut lines: Vec<Line> = Vec::new();

    lines.push(heading("Logs View"));
    lines.push(blank());
    lines.push(subheading("\u{2328} Keyboard"));
    lines.push(kv("j / k", "Move selection up/down"));
    lines.push(kv("\u{2191} \u{2193}", "Move selection up/down"));
    lines.push(kv("PgUp/Dn", "Scroll 20 entries"));
    lines.push(kv("Home", "Jump to top"));
    lines.push(kv("G / End", "Jump to bottom (resume LIVE)"));
    lines.push(kv("/", "Focus Search field (live, a|b, /regex/i)"));
    lines.push(kv("\\", "Focus Exclude field (live, a|b, /regex/i)"));
    lines.push(kv("n / N", "Next / previous match"));
    lines.push(kv("t", "Focus Tag filter (live, +inc|-exc)"));
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

    // Row 1 description
    lines.push(Line::from(vec![
        Span::raw("    "),
        dim("Row 1 hosts three input fields: "),
        Span::styled("Search", Style::default().fg(YELLOW)),
        dim(" / "),
        Span::styled("Exclude", Style::default().fg(YELLOW)),
        dim(" / "),
        Span::styled("Tag", Style::default().fg(YELLOW)),
        dim("."),
    ]));
    lines.push(blank());

    // Activation
    lines.push(Line::from(vec![
        Span::raw("    "),
        dim("Click a field or press "),
        key("/"),
        dim(" (Search) / "),
        key("\\"),
        dim(" (Exclude) / "),
        key("t"),
        dim(" (Tag) to activate."),
    ]));
    lines.push(Line::from(vec![
        Span::raw("    "),
        dim("Typing filters the list "),
        Span::styled("live", Style::default().fg(GREEN)),
        dim(" — no Enter needed. Click outside or press "),
        key("Esc"),
        dim(" to blur."),
    ]));
    lines.push(blank());

    // Syntax — Search / Exclude
    lines.push(Line::from(vec![
        Span::raw("    "),
        Span::styled("Search / Exclude syntax", Style::default().fg(SAPPHIRE)),
    ]));
    lines.push(Line::from(vec![
        Span::raw("      "),
        Span::styled("a|b|c", Style::default().fg(GREEN)),
        dim("            OR match — any of the terms (plain substring)"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("      "),
        Span::styled("/pattern/", Style::default().fg(GREEN)),
        dim("        regex mode — pipe is passed through to the engine"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("      "),
        Span::styled("/pattern/i", Style::default().fg(GREEN)),
        dim("       regex, case-insensitive"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("      "),
        dim("Exclude drops any row that matches (inverse of Search)."),
    ]));
    lines.push(blank());

    // Syntax — Tag
    lines.push(Line::from(vec![
        Span::raw("    "),
        Span::styled("Tag syntax", Style::default().fg(SAPPHIRE)),
    ]));
    lines.push(Line::from(vec![
        Span::raw("      "),
        Span::styled("+network|-flog_net", Style::default().fg(GREEN)),
        dim("   include network, exclude flog_net (pipe-separated)"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("      "),
        dim("Tag match is exact (case-insensitive); use regex via "),
        Span::styled("*", Style::default().fg(YELLOW)),
        dim(" or "),
        Span::styled(".", Style::default().fg(YELLOW)),
        dim(" in the pattern."),
    ]));
    lines.push(blank());

    // Level
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
        dim("  (row 2) to set minimum level"),
    ]));
    lines.push(blank());

    lines.push(subheading("\u{1f4cb} Detail Panel"));
    lines.push(kv("J / K", "Move cursor down/up in JSON viewer"));
    lines.push(kv(
        "Enter",
        "Activate action on cursor row (expand/open/copy/fold)",
    ));
    lines.push(kv("o", "Open URL on cursor row in browser"));
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

    lines
}
