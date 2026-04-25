//! Scroll clamping + click-region extraction for the device picker.
//!
//! Takes the fully built list of [`PickerLine`]s, applies the
//! keep-selected-visible scroll rule, and produces the ordered list of
//! `Line<'static>`s for `Paragraph` along with `(y, x_start, x_end, sel)`
//! click regions that [`App::layout.device_picker_items`] stores for
//! mouse-hit-testing.

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::palette::BASE;
use super::row::PickerLine;

/// Result of scroll-clamping: the scroll offset that was applied.
pub(super) struct ScrollResult {
    pub scroll_offset: usize,
    pub total_lines: usize,
    pub max_scroll: usize,
    pub visible_h: usize,
}

/// Clamp `device_picker_selected` and `device_picker_scroll` so the
/// currently selected card stays visible within `inner_h`.
pub(super) fn clamp_scroll(
    lines: &[PickerLine],
    selected: &mut usize,
    scroll: &mut usize,
    selectable_count: usize,
    inner_h: usize,
) -> ScrollResult {
    // Clamp selection
    let max_sel = selectable_count.saturating_sub(1);
    if *selected > max_sel {
        *selected = max_sel;
    }

    // Find top/bottom lines for the selected card
    let mut sel_top: Option<usize> = None;
    let mut sel_bot: usize = 0;
    for (idx, l) in lines.iter().enumerate() {
        if l.click_target == Some(*selected) {
            if sel_top.is_none() {
                sel_top = Some(idx);
            }
            sel_bot = idx;
        }
    }
    let total_lines = lines.len();
    let visible_h = inner_h;
    if let Some(top) = sel_top {
        if top < *scroll {
            *scroll = top;
        } else if sel_bot >= *scroll + visible_h {
            *scroll = sel_bot + 1 - visible_h;
        }
    }
    let max_scroll = total_lines.saturating_sub(visible_h);
    if *scroll > max_scroll {
        *scroll = max_scroll;
    }

    ScrollResult {
        scroll_offset: *scroll,
        total_lines,
        max_scroll,
        visible_h,
    }
}

/// One click region in the modal: `(y, x_start, x_end, sel_idx)`.
pub(super) type ClickRegion = (u16, u16, u16, usize);

/// Rendered viewport: rows ready for `Paragraph` + per-row click regions.
pub(super) struct Viewport {
    pub out_lines: Vec<Line<'static>>,
    pub click_regions: Vec<ClickRegion>,
}

/// Render the visible slice of `lines` into `Line<'static>`s and collect
/// the click regions for cards visible in the viewport.
pub(super) fn build_viewport(
    lines: &[PickerLine],
    inner: Rect,
    inner_w: usize,
    dev_w: usize,
    device_gutter: u16,
    card_indent: u16,
    scroll: &ScrollResult,
) -> Viewport {
    let mut click_regions: Vec<ClickRegion> = Vec::new();
    let mut out_lines: Vec<Line<'static>> = Vec::with_capacity(scroll.visible_h);

    for row in 0..scroll.visible_h {
        let src_idx = scroll.scroll_offset + row;
        if src_idx >= scroll.total_lines {
            out_lines.push(Line::from(Span::styled(
                " ".repeat(inner_w),
                Style::default().bg(BASE),
            )));
            continue;
        }
        let l = &lines[src_idx];
        out_lines.push(Line::from(l.spans.clone()));

        if let Some(sel) = l.click_target {
            // Card spans from col device_gutter+card_indent to col dev_w - card_indent (within inner coords).
            let x_start = inner.x + device_gutter + card_indent;
            let x_end = inner.x + device_gutter + dev_w as u16 - card_indent;
            let y = inner.y + row as u16;
            click_regions.push((y, x_start, x_end, sel));
        }
    }
    Viewport {
        out_lines,
        click_regions,
    }
}
