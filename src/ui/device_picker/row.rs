//! Per-row builders for the device picker: device container borders,
//! blank/waiting inner rows, and the shared [`PickerLine`] type. The
//! app-card rendering lives in [`super::card`]. All builders produce
//! [`PickerLine`]s which the modal then renders after scroll clamping.

use ratatui::{
    style::{Modifier, Style},
    text::Span,
};
use unicode_width::UnicodeWidthStr;

use super::palette::{BASE, MANTLE, OVERLAY0, SAPPHIRE, SURFACE0, TEXT};

/// A content line for the device picker: full-width spans + optional click target (sel_idx).
pub(super) struct PickerLine {
    pub spans: Vec<Span<'static>>,
    pub click_target: Option<usize>,
}

impl PickerLine {
    pub fn plain(w: usize, bg: ratatui::style::Color) -> Self {
        PickerLine {
            spans: vec![Span::styled(" ".repeat(w), Style::default().bg(bg))],
            click_target: None,
        }
    }
}

/// Shorten a long device id/UDID for display: keep 8-char prefix + "..." + 4-char suffix.
pub(super) fn shorten_id(id: &str) -> String {
    let w = id.width();
    if w <= 22 {
        return id.to_string();
    }
    // Keep leading 8 chars + ... + trailing 4 chars (byte-based is fine for hex UDIDs).
    let chars: Vec<char> = id.chars().collect();
    if chars.len() <= 14 {
        return id.to_string();
    }
    let head: String = chars.iter().take(8).collect();
    let tail: String = chars.iter().skip(chars.len() - 4).collect();
    format!("{}...{}", head, tail)
}

/// Device top border:  `╭─ [Tag] Name ───────── Conn · id ─╮`
// Phase 3 redesign — see Audit UI-015/UI-014: extract parameter struct.
#[allow(clippy::too_many_arguments)]
pub(super) fn push_device_top(
    lines: &mut Vec<PickerLine>,
    inner_w: usize,
    dev_w: usize,
    gutter: u16,
    platform_tag: &str,
    name: &str,
    conn_label: &str,
    id_short: &str,
) {
    let bg = BASE;
    let dev_bg = MANTLE;
    let border_fg = SURFACE0;
    let text_fg = TEXT;
    let subtle_fg = OVERLAY0;

    let gutter_s = " ".repeat(gutter as usize);

    // Compose the top-edge title spans.
    //   left: "─ [Tag] Name "
    //   right: " Conn · id ─"
    // Both counted in dev_w between the corners.

    let tag_text = format!("[{}]", platform_tag);
    let tag_span_w = tag_text.width() + 1 + name.width(); // tag + space + name
    let right_text = format!("{} \u{00b7} {}", conn_label, id_short);
    let right_span_w = right_text.width();

    // Build a composed list of interior chars between `╭` and `╮` (width = dev_w - 2).
    let interior_w = dev_w.saturating_sub(2);
    let left_fixed = 3; // "─ " (── then space) — actually we use "── " then tag + " " + name + " "
    let right_fixed = 3; // " ──" — " " + "──"
    let dashes_needed =
        interior_w.saturating_sub(left_fixed + tag_span_w + right_fixed + right_span_w + 2); // +2 for spaces around right text

    // Assemble spans:
    let mut spans: Vec<Span<'static>> = vec![
        Span::styled(gutter_s.clone(), Style::default().bg(bg)),
        Span::styled(
            "\u{256d}".to_string(), // ╭
            Style::default().fg(border_fg).bg(dev_bg),
        ),
        // left fill: "── " then tag + " " + name + " "
        Span::styled(
            "\u{2500}\u{2500} ".to_string(),
            Style::default().fg(border_fg).bg(dev_bg),
        ),
        Span::styled(
            tag_text,
            Style::default()
                .fg(SAPPHIRE)
                .bg(dev_bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ".to_string(), Style::default().bg(dev_bg)),
        Span::styled(
            name.to_string(),
            Style::default()
                .fg(text_fg)
                .bg(dev_bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ".to_string(), Style::default().bg(dev_bg)),
        Span::styled(
            "\u{2500}".repeat(dashes_needed),
            Style::default().fg(border_fg).bg(dev_bg),
        ),
        Span::styled(" ".to_string(), Style::default().bg(dev_bg)),
        Span::styled(right_text, Style::default().fg(subtle_fg).bg(dev_bg)),
        Span::styled(" ".to_string(), Style::default().bg(dev_bg)),
        Span::styled(
            "\u{2500}\u{2500}".to_string(),
            Style::default().fg(border_fg).bg(dev_bg),
        ),
        Span::styled(
            "\u{256e}".to_string(), // ╮
            Style::default().fg(border_fg).bg(dev_bg),
        ),
    ];
    // Right gutter of BASE
    let used = gutter as usize + dev_w;
    if used < inner_w {
        spans.push(Span::styled(
            " ".repeat(inner_w - used),
            Style::default().bg(bg),
        ));
    }
    lines.push(PickerLine {
        spans,
        click_target: None,
    });
}

/// Device bottom border: `╰──...──╯`
pub(super) fn push_device_bottom(
    lines: &mut Vec<PickerLine>,
    inner_w: usize,
    dev_w: usize,
    gutter: u16,
) {
    let bg = BASE;
    let dev_bg = MANTLE;
    let border_fg = SURFACE0;
    let gutter_s = " ".repeat(gutter as usize);

    let interior_w = dev_w.saturating_sub(2);
    let mut spans: Vec<Span<'static>> = vec![
        Span::styled(gutter_s, Style::default().bg(bg)),
        Span::styled(
            "\u{2570}".to_string(), // ╰
            Style::default().fg(border_fg).bg(dev_bg),
        ),
        Span::styled(
            "\u{2500}".repeat(interior_w),
            Style::default().fg(border_fg).bg(dev_bg),
        ),
        Span::styled(
            "\u{256f}".to_string(), // ╯
            Style::default().fg(border_fg).bg(dev_bg),
        ),
    ];
    let used = gutter as usize + dev_w;
    if used < inner_w {
        spans.push(Span::styled(
            " ".repeat(inner_w - used),
            Style::default().bg(bg),
        ));
    }
    lines.push(PickerLine {
        spans,
        click_target: None,
    });
}

/// Blank row inside a device container: `│` + MANTLE fill + `│`
pub(super) fn push_device_inner_blank(
    lines: &mut Vec<PickerLine>,
    inner_w: usize,
    dev_w: usize,
    gutter: u16,
) {
    let bg = BASE;
    let dev_bg = MANTLE;
    let border_fg = SURFACE0;
    let gutter_s = " ".repeat(gutter as usize);

    let interior_w = dev_w.saturating_sub(2);
    let mut spans: Vec<Span<'static>> = vec![
        Span::styled(gutter_s, Style::default().bg(bg)),
        Span::styled(
            "\u{2502}".to_string(),
            Style::default().fg(border_fg).bg(dev_bg),
        ),
        Span::styled(" ".repeat(interior_w), Style::default().bg(dev_bg)),
        Span::styled(
            "\u{2502}".to_string(),
            Style::default().fg(border_fg).bg(dev_bg),
        ),
    ];
    let used = gutter as usize + dev_w;
    if used < inner_w {
        spans.push(Span::styled(
            " ".repeat(inner_w - used),
            Style::default().bg(bg),
        ));
    }
    lines.push(PickerLine {
        spans,
        click_target: None,
    });
}

/// Waiting row inside a device container.
pub(super) fn push_waiting_row(
    lines: &mut Vec<PickerLine>,
    inner_w: usize,
    dev_w: usize,
    gutter: u16,
) {
    let bg = BASE;
    let dev_bg = MANTLE;
    let border_fg = SURFACE0;
    let gutter_s = " ".repeat(gutter as usize);

    let interior_w = dev_w.saturating_sub(2);
    let text = "\u{25cb} Waiting for app...";
    let text_w = text.width();
    let left_pad = 3usize;
    let right_pad = interior_w.saturating_sub(left_pad + text_w);

    let mut spans: Vec<Span<'static>> = vec![
        Span::styled(gutter_s, Style::default().bg(bg)),
        Span::styled(
            "\u{2502}".to_string(),
            Style::default().fg(border_fg).bg(dev_bg),
        ),
        Span::styled(" ".repeat(left_pad), Style::default().bg(dev_bg)),
        Span::styled(text.to_string(), Style::default().fg(OVERLAY0).bg(dev_bg)),
        Span::styled(" ".repeat(right_pad), Style::default().bg(dev_bg)),
        Span::styled(
            "\u{2502}".to_string(),
            Style::default().fg(border_fg).bg(dev_bg),
        ),
    ];
    let used = gutter as usize + dev_w;
    if used < inner_w {
        spans.push(Span::styled(
            " ".repeat(inner_w - used),
            Style::default().bg(bg),
        ));
    }
    lines.push(PickerLine {
        spans,
        click_target: None,
    });
}
