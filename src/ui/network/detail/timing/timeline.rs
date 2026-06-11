use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

use crate::domain::network_timing::TimingEvent;
use crate::ui::json_viewer::JsonHotRegion;
use crate::ui::{MAUVE, OVERLAY0, SUBTEXT0, TEAL, TEXT};

use super::format::{
    event_offset_us, format_event_offset_precise, format_size, format_us, truncate_cell,
};
use super::push_plain;

pub(super) struct TimingLines<'a> {
    pub(super) lines: &'a mut Vec<Line<'static>>,
    pub(super) section_map: &'a mut Vec<Option<String>>,
    pub(super) json_click_map: &'a mut Vec<Vec<JsonHotRegion>>,
    pub(super) json_section_keys: &'a mut Vec<Option<String>>,
}

pub(super) struct TimelineSpec {
    pub(super) label_width: usize,
    pub(super) width: usize,
    total_us: u64,
}

pub(super) enum TimelineEventRow {
    Sse,
    Ws,
}

impl TimelineSpec {
    pub(super) fn new(label_width: usize, inner_w: usize, total_us: u64) -> Self {
        Self {
            label_width,
            width: timeline_width(inner_w),
            total_us: total_us.max(1),
        }
    }

    pub(super) fn cell_for(&self, offset_us: u64) -> usize {
        if self.width <= 1 {
            return 0;
        }
        (((offset_us.min(self.total_us) as f64 / self.total_us as f64) * (self.width - 1) as f64)
            .round() as usize)
            .min(self.width - 1)
    }
}

pub(super) fn render_axis(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    spec: &TimelineSpec,
) {
    let tick_count = tick_count(spec.width);
    let mut labels = vec![' '; spec.width];
    let mut axis = vec!['─'; spec.width];

    for idx in 0..=tick_count {
        let pos = if idx == tick_count {
            spec.width.saturating_sub(1)
        } else {
            spec.width.saturating_mul(idx) / tick_count
        };
        axis[pos] = if idx == 0 || idx == tick_count {
            '|'
        } else {
            '┬'
        };

        let label = if idx == 0 {
            "0s".to_string()
        } else {
            format!(
                "+{}",
                format_us(spec.total_us.saturating_mul(idx as u64) / tick_count as u64)
            )
        };
        place_label(&mut labels, pos, &label);
    }

    push_plain(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        Line::from(vec![
            Span::styled(
                format!("   {:<width$} ", "Timeline", width = spec.label_width),
                Style::default().fg(OVERLAY0),
            ),
            Span::styled(
                labels.iter().collect::<String>(),
                Style::default().fg(SUBTEXT0),
            ),
        ]),
    );
    push_plain(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        Line::from(vec![
            Span::raw(format!("   {:<width$} ", "", width = spec.label_width)),
            Span::styled(
                axis.iter().collect::<String>(),
                Style::default().fg(OVERLAY0),
            ),
        ]),
    );
}

pub(super) fn render_bar_track(
    target: &mut TimingLines<'_>,
    spec: &TimelineSpec,
    label: &str,
    offset_us: u64,
    duration_us: u64,
    color: Color,
    suffix: &str,
) {
    let start = spec.cell_for(offset_us);
    let end = spec
        .cell_for(offset_us.saturating_add(duration_us))
        .max(start);
    let mut cells = vec![' '; spec.width];
    for idx in start..=end {
        if idx < cells.len() {
            cells[idx] = '█';
        }
    }
    push_plain(
        target.lines,
        target.section_map,
        target.json_click_map,
        target.json_section_keys,
        Line::from(vec![
            Span::styled(
                format!(
                    "   {:<width$} ",
                    truncate_cell(label, spec.label_width),
                    width = spec.label_width
                ),
                Style::default().fg(TEXT),
            ),
            Span::styled(cells.iter().collect::<String>(), Style::default().fg(color)),
            Span::raw(" "),
            Span::styled(suffix.to_string(), Style::default().fg(SUBTEXT0)),
        ]),
    );
}

pub(super) fn render_marker_track(
    target: &mut TimingLines<'_>,
    spec: &TimelineSpec,
    label: &str,
    positions: &[(usize, char)],
    color: Color,
    suffix: &str,
) {
    let mut cells = vec![(0_u16, ' '); spec.width];
    for (pos, marker) in positions {
        if *pos < cells.len() {
            cells[*pos].0 = cells[*pos].0.saturating_add(1);
            cells[*pos].1 = *marker;
        }
    }
    let rendered = cells
        .iter()
        .map(|(count, marker)| if *count == 0 { ' ' } else { *marker })
        .collect::<String>();
    push_plain(
        target.lines,
        target.section_map,
        target.json_click_map,
        target.json_section_keys,
        Line::from(vec![
            Span::styled(
                format!(
                    "   {:<width$} ",
                    truncate_cell(label, spec.label_width),
                    width = spec.label_width
                ),
                Style::default().fg(TEXT),
            ),
            Span::styled(rendered, Style::default().fg(color)),
            Span::raw(" "),
            Span::styled(suffix.to_string(), Style::default().fg(SUBTEXT0)),
        ]),
    );
}

pub(super) fn marker_positions(
    events: &[TimingEvent],
    start_us: Option<u64>,
    spec: &TimelineSpec,
    marker: char,
) -> Vec<(usize, char)> {
    events
        .iter()
        .filter_map(|event| {
            event_offset_us(event, start_us).map(|offset| (spec.cell_for(offset), marker))
        })
        .collect()
}

pub(super) fn render_event_table(
    target: &mut TimingLines<'_>,
    events: &[TimingEvent],
    start_us: Option<u64>,
    row_kind: TimelineEventRow,
    limit: usize,
) {
    if events.is_empty() {
        return;
    }

    let gap_label = match row_kind {
        TimelineEventRow::Sse => "Gap",
        TimelineEventRow::Ws => "Idle",
    };
    push_plain(
        target.lines,
        target.section_map,
        target.json_click_map,
        target.json_section_keys,
        Line::from(vec![
            Span::raw("     "),
            Span::styled(
                format!("{:<8}", event_header(&row_kind)),
                Style::default().fg(OVERLAY0),
            ),
            Span::styled(format!("{:>8}", "At"), Style::default().fg(OVERLAY0)),
            Span::raw("  "),
            Span::styled(format!("{:>8}", gap_label), Style::default().fg(OVERLAY0)),
            Span::raw("  "),
            Span::styled(format!("{:>7}", "Size"), Style::default().fg(OVERLAY0)),
        ]),
    );

    for event in events.iter().take(limit) {
        let gap = event
            .gap_us
            .map(format_us)
            .unwrap_or_else(|| "-".to_string());
        let size = event
            .size
            .map(format_size)
            .unwrap_or_else(|| "-".to_string());
        push_plain(
            target.lines,
            target.section_map,
            target.json_click_map,
            target.json_section_keys,
            Line::from(vec![
                Span::styled("   • ", Style::default().fg(MAUVE)),
                Span::styled(
                    format!("{:<8}", event_label(event, &row_kind)),
                    Style::default().fg(TEXT),
                ),
                Span::styled(
                    format!("{:>8}", format_event_offset_precise(event, start_us)),
                    Style::default().fg(SUBTEXT0),
                ),
                Span::raw("  "),
                Span::styled(format!("{:>8}", gap), Style::default().fg(TEAL)),
                Span::raw("  "),
                Span::styled(format!("{:>7}", size), Style::default().fg(OVERLAY0)),
            ]),
        );
    }
}

fn timeline_width(inner_w: usize) -> usize {
    inner_w.saturating_sub(42).clamp(24, 56)
}

fn tick_count(width: usize) -> usize {
    if width >= 48 {
        5
    } else if width >= 36 {
        4
    } else {
        3
    }
}

fn place_label(cells: &mut [char], center: usize, label: &str) {
    let label_len = label.chars().count();
    if label_len == 0 || label_len > cells.len() {
        return;
    }
    let mut start = center.saturating_sub(label_len / 2);
    if start + label_len > cells.len() {
        start = cells.len() - label_len;
    }
    if cells[start..start + label_len].iter().any(|ch| *ch != ' ') {
        return;
    }
    for (idx, ch) in label.chars().enumerate() {
        cells[start + idx] = ch;
    }
}

fn event_label(event: &TimingEvent, row_kind: &TimelineEventRow) -> String {
    match row_kind {
        TimelineEventRow::Sse => truncate_cell(&event.name, 8),
        TimelineEventRow::Ws if event.name.starts_with("send") => "→ send".to_string(),
        TimelineEventRow::Ws if event.name.starts_with("recv") => "← recv".to_string(),
        TimelineEventRow::Ws => truncate_cell(&event.name, 8),
    }
}

fn event_header(row_kind: &TimelineEventRow) -> &'static str {
    match row_kind {
        TimelineEventRow::Sse => "Chunk",
        TimelineEventRow::Ws => "Message",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeline_cell_mapping_uses_request_relative_scale() {
        let spec = TimelineSpec {
            label_width: 10,
            width: 11,
            total_us: 100,
        };
        assert_eq!(spec.cell_for(0), 0);
        assert_eq!(spec.cell_for(50), 5);
        assert_eq!(spec.cell_for(100), 10);
    }
}
