//! Logs toolbar — two operator rows: inputs (Search/Exclude/Tag) and levels + counts.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, AppMode};
use crate::domain::LogLevel;

use super::{level_color, BLUE, MANTLE, OVERLAY0, RED, SUBTEXT0, SURFACE0, SURFACE1, TEXT, YELLOW};

// ══════════════════════════════════════
//  Level pill helpers (shared with list)
// ══════════════════════════════════════

/// Returns (fg, bg, bold) for a level pill badge.
pub(super) fn level_badge(level: LogLevel) -> (Color, Color, bool) {
    match level {
        LogLevel::Verbose => (OVERLAY0, SURFACE0, false),
        LogLevel::Debug => (SUBTEXT0, SURFACE0, false),
        LogLevel::Info => (MANTLE, BLUE, true),
        LogLevel::Warning => (MANTLE, YELLOW, true),
        LogLevel::Error => (MANTLE, RED, true),
        LogLevel::System => (OVERLAY0, SURFACE0, false),
    }
}

/// Render level as a styled pill with fixed LEVEL_WIDTH-char width.
pub(super) fn level_pill(level: LogLevel, row_bg: Color) -> Span<'static> {
    let (fg, bg, bold) = level_badge(level);
    // On highlighted rows (error/warning bg), pull level pill bg down to MANTLE for contrast.
    let pill_bg = if (row_bg == super::ERROR_ROW_BG || row_bg == super::WARNING_ROW_BG)
        && matches!(
            level,
            LogLevel::Debug | LogLevel::Verbose | LogLevel::System
        ) {
        MANTLE
    } else {
        bg
    };
    let label = level.as_str();
    let total_pad = super::LEVEL_WIDTH.saturating_sub(label.len());
    let left_pad = total_pad / 2;
    let right_pad = total_pad - left_pad;
    let text = format!("{}{}{}", " ".repeat(left_pad), label, " ".repeat(right_pad));
    let mut style = Style::default().fg(fg).bg(pill_bg);
    if bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    Span::styled(text, style)
}

// ══════════════════════════════════════
//  Toolbar draws
// ══════════════════════════════════════

pub(super) fn draw_toolbar_op1(f: &mut Frame, app: &mut App, area: Rect) {
    use crate::app::InputField;
    use crate::ui::input_field::{render_input_field, InputFieldProps};

    let bg = MANTLE;
    let w = area.width;

    // Split width into 3 equal-ish slices for Search | Exclude | Tag, with 4-col gaps.
    let gap: u16 = 4;
    let inner = w.saturating_sub(gap * 2);
    let per = inner / 3;
    let rem = inner - per * 3;
    let widths = [
        per + (if rem > 0 { 1 } else { 0 }),
        per + (if rem > 1 { 1 } else { 0 }),
        per,
    ];

    let mut spans: Vec<Span> = Vec::new();
    let mut x: u16 = 0;

    let fields: [(InputField, &str, &str); 3] = [
        (InputField::LogSearch, "Search", "a|b, regex: /pat/"),
        (InputField::LogExclude, "Exclude", "a|b, regex: /pat/"),
        (InputField::LogTag, "Tag", "+inc|-exc"),
    ];

    for (i, (field, label, hint)) in fields.iter().enumerate() {
        let active = matches!(app.mode, AppMode::InputActive(f) if f == *field);
        let value = app.inputs.buffer(*field).to_string();
        let cursor_byte = app.inputs.cursor(*field);

        let out = render_input_field(
            InputFieldProps {
                label,
                hint,
                value: &value,
                active,
                cursor_byte,
                total_width: widths[i],
            },
            x,
        );

        match field {
            InputField::LogSearch => app.layout.log_search_x = out.hit_x,
            InputField::LogExclude => app.layout.log_exclude_x = out.hit_x,
            InputField::LogTag => app.layout.log_tag_x = out.hit_x,
            _ => {}
        }

        spans.extend(out.spans);
        x += out.used_width;

        if i < 2 {
            spans.push(Span::styled(
                " ".repeat(gap as usize),
                Style::default().bg(bg),
            ));
            x += gap;
        }
    }

    // Pad remaining
    let used: u16 = spans.iter().map(|s| s.content.width() as u16).sum();
    let pad = w.saturating_sub(used);
    if pad > 0 {
        spans.push(Span::styled(
            " ".repeat(pad as usize),
            Style::default().bg(bg),
        ));
    }

    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)),
        area,
    );
}

pub(super) fn draw_toolbar_op2(f: &mut Frame, app: &mut App, area: Rect) {
    let bg = MANTLE;
    let mut spans: Vec<Span> = Vec::new();
    let mut x: u16 = 0;

    spans.push(Span::styled(" ", Style::default().bg(bg)));
    x += 1;

    // "Level: " label
    let level_label = "Level: ";
    spans.push(Span::styled(
        level_label,
        Style::default().fg(SUBTEXT0).bg(bg),
    ));
    x += level_label.width() as u16;

    app.layout.levels_x = x;
    for (label, level) in &[
        ("S", LogLevel::System),
        ("V", LogLevel::Verbose),
        ("D", LogLevel::Debug),
        ("I", LogLevel::Info),
        ("W", LogLevel::Warning),
        ("E", LogLevel::Error),
    ] {
        let (fg, bg_c, bold) = level_badge(*level);
        let style = if app.filter.min_level == *level {
            let mut s =
                Style::default()
                    .fg(fg)
                    .bg(if bg_c == Color::Reset { SURFACE1 } else { bg_c });
            if bold {
                s = s.add_modifier(Modifier::BOLD);
            }
            s
        } else if app.filter.min_level > *level {
            Style::default()
                .fg(SURFACE0)
                .bg(bg)
                .add_modifier(Modifier::DIM)
        } else {
            Style::default().fg(level_color(*level)).bg(bg)
        };
        spans.push(Span::styled(format!(" {} ", label), style));
        x += 3;
    }

    // Spacer
    spans.push(Span::styled("    ", Style::default().bg(bg)));
    x += 4;

    // "Filtered: N/M"
    let filtered_label = "Filtered: ";
    spans.push(Span::styled(
        filtered_label,
        Style::default().fg(SUBTEXT0).bg(bg),
    ));
    x += filtered_label.width() as u16;

    let count_text = format!("{}/{}", app.filtered_count(), app.store.len());
    let cw = count_text.width() as u16;
    spans.push(Span::styled(
        count_text,
        Style::default()
            .fg(TEXT)
            .bg(bg)
            .add_modifier(Modifier::BOLD),
    ));
    x += cw;

    // "Match: N/M" — only when search is active and has results
    if !app.search.matches.is_empty() {
        spans.push(Span::styled("    ", Style::default().bg(bg)));
        x += 4;
        let match_label = "Match: ";
        spans.push(Span::styled(
            match_label,
            Style::default().fg(SUBTEXT0).bg(bg),
        ));
        x += match_label.width() as u16;
        let match_text = format!("{}/{}", app.search.match_idx + 1, app.search.matches.len());
        let mw = match_text.width() as u16;
        spans.push(Span::styled(
            match_text,
            Style::default()
                .fg(YELLOW)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ));
        x += mw;
    }

    // Bookmarks: ●N (only when any)
    if !app.bookmarks.is_empty() {
        spans.push(Span::styled("    ", Style::default().bg(bg)));
        x += 4;
        let bm_label = "Bookmarks: ";
        spans.push(Span::styled(bm_label, Style::default().fg(SUBTEXT0).bg(bg)));
        x += bm_label.width() as u16;
        let bm = format!("●{}", app.bookmarks.len());
        let bw = bm.width() as u16;
        spans.push(Span::styled(bm, Style::default().fg(YELLOW).bg(bg)));
        x += bw;
    }

    // Pad to fill row
    let pad = area.width.saturating_sub(x);
    if pad > 0 {
        spans.push(Span::styled(
            " ".repeat(pad as usize),
            Style::default().bg(bg),
        ));
    }

    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)),
        area,
    );
}
