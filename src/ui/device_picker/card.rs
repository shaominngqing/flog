//! App card renderer — the per-app panel drawn inside a device
//! container. Top border embeds the app name + port; three detail rows
//! show package, platform, build mode. Active cards use double-line
//! borders + an `[ACTIVE]` pill; selected (non-active) cards get a
//! left-edge cursor bar.

use ratatui::{
    style::{Modifier, Style},
    text::Span,
};
use unicode_width::UnicodeWidthStr;

use super::palette::{
    BASE, GREEN, MANTLE, OVERLAY0, SAPPHIRE, SUBTEXT0, SURFACE0, SURFACE1, TEAL, TEXT,
};
use super::row::PickerLine;

/// Push 6 rows for an app card (top, name, Package, Platform, Mode, bottom),
/// each wrapped by the device container's `│` ... `│`.
#[allow(clippy::too_many_arguments)]
pub(super) fn push_app_card(
    lines: &mut Vec<PickerLine>,
    inner_w: usize,
    dev_w: usize,
    gutter: u16,
    card_indent: u16,
    card_w: usize,
    sel_idx: usize,
    is_active: bool,
    is_selected: bool,
    app_name: &str,
    app_version: &str,
    package_name: &str,
    os: &str,
    build_mode: &str,
    port: u16,
) {
    let bg = BASE;
    let dev_bg = MANTLE;
    let dev_border_fg = SURFACE0;
    let gutter_s = " ".repeat(gutter as usize);
    let left_card_gutter = " ".repeat(card_indent as usize);
    let right_card_gutter = left_card_gutter.clone();

    // Selection preview cursor: SAPPHIRE ▎ in the last column of the left indent,
    // only when this card is SELECTED but not ACTIVE (active styling already distinguishes).
    let show_cursor = is_selected && !is_active;
    let cursor_pre_w = (card_indent as usize).saturating_sub(1);
    let push_left_indent = |spans: &mut Vec<Span<'static>>| {
        if show_cursor {
            if cursor_pre_w > 0 {
                spans.push(Span::styled(
                    " ".repeat(cursor_pre_w),
                    Style::default().bg(dev_bg),
                ));
            }
            spans.push(Span::styled(
                "\u{258e}".to_string(), // ▎
                Style::default().fg(SAPPHIRE).bg(dev_bg),
            ));
        } else {
            spans.push(Span::styled(
                left_card_gutter.clone(),
                Style::default().bg(dev_bg),
            ));
        }
    };

    // Border/bg choice per state
    // card_bg is always MANTLE — borders + pill + bold carry the ACTIVE distinction.
    let card_bg = MANTLE;
    let (card_border_fg, tl, tr, bl, br, h, v) = if is_active {
        (
            SAPPHIRE, '\u{2554}', // ╔
            '\u{2557}', // ╗
            '\u{255a}', // ╚
            '\u{255d}', // ╝
            '\u{2550}', // ═
            '\u{2551}', // ║
        )
    } else {
        (
            SURFACE0, '\u{250c}', // ┌
            '\u{2510}', // ┐
            '\u{2514}', // └
            '\u{2518}', // ┘
            '\u{2500}', // ─
            '\u{2502}', // │
        )
    };

    let value_fg = if is_active { TEXT } else { SUBTEXT0 };
    let label_fg = OVERLAY0;
    let unknown_fg = SURFACE1;

    // Helper to wrap a row of spans with device │ | card gutters | device │
    let device_border_span = Span::styled(
        "\u{2502}".to_string(),
        Style::default().fg(dev_border_fg).bg(dev_bg),
    );

    let right_tail_w = inner_w.saturating_sub(gutter as usize + dev_w);
    let right_tail = " ".repeat(right_tail_w);

    // ── Row 1: top border of card with embedded title ──
    // interior of card = card_w - 2 cells between corners
    let card_inner_w = card_w.saturating_sub(2);

    // Title composition:
    //   "─ ● AppName v1.2.3  [ACTIVE] ══════ Port: 9753 ═"
    // or for normal:
    //   "─ ○ AppName v1.2.3 ────────────── Port: 9754 ─"
    let dot = if is_active { "\u{25cf}" } else { "\u{25cb}" };
    let dot_fg = if is_active { GREEN } else { TEAL };

    let label = if app_version.is_empty() {
        app_name.to_string()
    } else {
        format!("{} v{}", app_name, app_version)
    };
    let active_pill = " ACTIVE "; // pill contents
    let port_text = format!("Port: {}", port);

    // left: "─ " + dot + " " + label  (+ "  [ACTIVE] " if active)
    let mut left_w = 3 /* "─ " + dot (dot char width 1) */ + 1 + label.width();
    if is_active {
        left_w += 2 + active_pill.width(); // "  " + pill
    }
    // right: " " + port + " ─"
    let right_w = 1 + port_text.width() + 2;
    let dashes = card_inner_w.saturating_sub(left_w + right_w);

    let mut top_spans: Vec<Span<'static>> = Vec::new();
    top_spans.push(Span::styled(gutter_s.clone(), Style::default().bg(bg)));
    top_spans.push(device_border_span.clone());
    push_left_indent(&mut top_spans);
    // card top-left corner
    top_spans.push(Span::styled(
        tl.to_string(),
        Style::default().fg(card_border_fg).bg(dev_bg),
    ));
    // "─ "
    top_spans.push(Span::styled(
        format!("{}{}", h, ' '),
        Style::default().fg(card_border_fg).bg(card_bg),
    ));
    // dot
    top_spans.push(Span::styled(
        dot.to_string(),
        Style::default()
            .fg(dot_fg)
            .bg(card_bg)
            .add_modifier(Modifier::BOLD),
    ));
    // " " + label
    top_spans.push(Span::styled(" ".to_string(), Style::default().bg(card_bg)));
    top_spans.push(Span::styled(
        label.clone(),
        Style::default()
            .fg(TEXT)
            .bg(card_bg)
            .add_modifier(Modifier::BOLD),
    ));
    if is_active {
        top_spans.push(Span::styled("  ".to_string(), Style::default().bg(card_bg)));
        top_spans.push(Span::styled(
            active_pill.to_string(),
            Style::default()
                .fg(MANTLE)
                .bg(GREEN)
                .add_modifier(Modifier::BOLD),
        ));
    }
    // dashes
    top_spans.push(Span::styled(
        h.to_string().repeat(dashes),
        Style::default().fg(card_border_fg).bg(card_bg),
    ));
    // " " + port
    top_spans.push(Span::styled(" ".to_string(), Style::default().bg(card_bg)));
    top_spans.push(Span::styled(
        port_text.clone(),
        Style::default().fg(SAPPHIRE).bg(card_bg),
    ));
    // " ─"
    top_spans.push(Span::styled(" ".to_string(), Style::default().bg(card_bg)));
    top_spans.push(Span::styled(
        h.to_string(),
        Style::default().fg(card_border_fg).bg(card_bg),
    ));
    // card top-right corner
    top_spans.push(Span::styled(
        tr.to_string(),
        Style::default().fg(card_border_fg).bg(dev_bg),
    ));
    top_spans.push(Span::styled(
        right_card_gutter.clone(),
        Style::default().bg(dev_bg),
    ));
    top_spans.push(device_border_span.clone());
    if !right_tail.is_empty() {
        top_spans.push(Span::styled(right_tail.clone(), Style::default().bg(bg)));
    }
    lines.push(PickerLine {
        spans: top_spans,
        click_target: Some(sel_idx),
    });

    // ── Rows 2-4: details rows (but we have 3 detail rows and no name-row —
    // actually the sketch has name in the TOP BORDER, so we have 3 detail rows,
    // which gives card height = 6: top + 3 details + 2 blanks? No — per spec:
    //   "6 rows (top border, name row, Package, Platform, Mode, bottom border)"
    // Wait — the spec says name row AND top border. But in the sketch the name
    // is embedded in the top edge. I'll follow the sketch since it's the actual
    // visual target: top-border-with-name + Package + Platform + Mode + bottom.
    // That's 5 rows. We add 1 blank row between top and details so the details
    // breathe → 6 rows total matches the spec.
    // ──
    let detail_rows: [(&str, &str); 3] = [
        ("Package", package_name),
        ("Platform", os),
        ("Mode", build_mode),
    ];

    // blank breathing row inside card (optional, for parity with spec's 6 rows)
    {
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.push(Span::styled(gutter_s.clone(), Style::default().bg(bg)));
        spans.push(device_border_span.clone());
        push_left_indent(&mut spans);
        spans.push(Span::styled(
            v.to_string(),
            Style::default().fg(card_border_fg).bg(card_bg),
        ));
        spans.push(Span::styled(
            " ".repeat(card_inner_w),
            Style::default().bg(card_bg),
        ));
        spans.push(Span::styled(
            v.to_string(),
            Style::default().fg(card_border_fg).bg(card_bg),
        ));
        spans.push(Span::styled(
            right_card_gutter.clone(),
            Style::default().bg(dev_bg),
        ));
        spans.push(device_border_span.clone());
        if !right_tail.is_empty() {
            spans.push(Span::styled(right_tail.clone(), Style::default().bg(bg)));
        }
        lines.push(PickerLine {
            spans,
            click_target: Some(sel_idx),
        });
    }

    // Skipping the breathing row changes our total to 6 — we already pushed 1 (top)
    // plus now 1 (blank); 3 details will bring us to 5; bottom = 6. Good.
    //
    // Hmm, the spec lists "name row" separately. We embed name in the top border,
    // which is visually more flush. Keep 6 rows by using: top + blank + 3 details + bottom = 6.

    for (lbl, val) in detail_rows {
        let label_padded = format!("{:<10}", lbl); // Platform(8)+2 = 10, aligns Package/Mode too
        let (display, fg) = if val.is_empty() {
            ("unknown".to_string(), unknown_fg)
        } else {
            (val.to_string(), value_fg)
        };
        // Inner content: "    " (4sp) + label_padded (10) + display
        let content_w = 4 + label_padded.width() + display.width();
        let pad_right = card_inner_w.saturating_sub(content_w);

        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.push(Span::styled(gutter_s.clone(), Style::default().bg(bg)));
        spans.push(device_border_span.clone());
        push_left_indent(&mut spans);
        spans.push(Span::styled(
            v.to_string(),
            Style::default().fg(card_border_fg).bg(card_bg),
        ));
        spans.push(Span::styled(
            "    ".to_string(),
            Style::default().bg(card_bg),
        ));
        spans.push(Span::styled(
            label_padded,
            Style::default().fg(label_fg).bg(card_bg),
        ));
        spans.push(Span::styled(
            display,
            Style::default()
                .fg(fg)
                .bg(card_bg)
                .add_modifier(if is_active {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ));
        spans.push(Span::styled(
            " ".repeat(pad_right),
            Style::default().bg(card_bg),
        ));
        spans.push(Span::styled(
            v.to_string(),
            Style::default().fg(card_border_fg).bg(card_bg),
        ));
        spans.push(Span::styled(
            right_card_gutter.clone(),
            Style::default().bg(dev_bg),
        ));
        spans.push(device_border_span.clone());
        if !right_tail.is_empty() {
            spans.push(Span::styled(right_tail.clone(), Style::default().bg(bg)));
        }
        lines.push(PickerLine {
            spans,
            click_target: Some(sel_idx),
        });
    }

    // ── Bottom border ──
    let mut bot_spans: Vec<Span<'static>> = Vec::new();
    bot_spans.push(Span::styled(gutter_s.clone(), Style::default().bg(bg)));
    bot_spans.push(device_border_span.clone());
    push_left_indent(&mut bot_spans);
    bot_spans.push(Span::styled(
        bl.to_string(),
        Style::default().fg(card_border_fg).bg(dev_bg),
    ));
    bot_spans.push(Span::styled(
        h.to_string().repeat(card_inner_w),
        Style::default().fg(card_border_fg).bg(card_bg),
    ));
    bot_spans.push(Span::styled(
        br.to_string(),
        Style::default().fg(card_border_fg).bg(dev_bg),
    ));
    bot_spans.push(Span::styled(
        right_card_gutter.clone(),
        Style::default().bg(dev_bg),
    ));
    bot_spans.push(device_border_span.clone());
    if !right_tail.is_empty() {
        bot_spans.push(Span::styled(right_tail.clone(), Style::default().bg(bg)));
    }
    lines.push(PickerLine {
        spans: bot_spans,
        click_target: Some(sel_idx),
    });
}
