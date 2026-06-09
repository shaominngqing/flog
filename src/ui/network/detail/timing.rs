//! Network detail Timing section renderers.
//!
//! File-size note: this module intentionally keeps the Timing section in one
//! place while the feature lands because the protocol-specific renderers share
//! the same section-map/json-map append contract. Split once it grows again.

use std::collections::HashSet;

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::domain::network::{NetworkEntry, Protocol, WsDirection};
use crate::domain::network_timing::{NetworkTiming, TimingEvent, TimingPhase, TimingPhaseStatus};
use crate::ui::json_viewer::JsonHotRegion;
use crate::ui::{
    sanitize_for_cell, GREEN, MAUVE, OVERLAY0, PEACH, SAPPHIRE, SUBTEXT0, SURFACE0, TEAL, TEXT,
    YELLOW,
};

use super::shared::push_section_header;
use super::KEY_COLOR;

pub(super) fn render_timing(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    entry: &NetworkEntry,
    collapsed_sections: &HashSet<String>,
    inner_w: usize,
) {
    if !has_timing(entry) {
        return;
    }

    let sec = "Timing";
    let is_collapsed = collapsed_sections.contains(sec);
    push_section_header(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        sec,
        is_collapsed,
    );
    if is_collapsed {
        return;
    }

    match entry.protocol {
        Protocol::Http => render_http(
            lines,
            section_map,
            json_click_map,
            json_section_keys,
            entry,
            inner_w,
        ),
        Protocol::Sse => render_sse(
            lines,
            section_map,
            json_click_map,
            json_section_keys,
            entry,
            inner_w,
        ),
        Protocol::Ws => render_ws(
            lines,
            section_map,
            json_click_map,
            json_section_keys,
            entry,
            inner_w,
        ),
    }

    push_plain(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        Line::raw(""),
    );
}

fn has_timing(entry: &NetworkEntry) -> bool {
    entry.timing.is_some()
        || entry
            .sse_chunks
            .iter()
            .any(|chunk| chunk.event_timing.is_some())
        || entry
            .ws_messages
            .iter()
            .any(|message| message.event_timing.is_some())
}

fn push_plain(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    line: Line<'static>,
) {
    lines.push(line);
    section_map.push(None);
    json_click_map.push(Vec::new());
    json_section_keys.push(None);
}

fn render_http(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    entry: &NetworkEntry,
    inner_w: usize,
) {
    let Some(timing) = entry.timing.as_ref() else {
        return;
    };

    render_summary(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        entry,
        timing,
    );
    render_phase_table(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        timing,
        inner_w,
    );
    render_events(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        "Milestones",
        &timing.events,
        6,
    );
    render_notes(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        timing,
    );
}

fn render_sse(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    entry: &NetworkEntry,
    inner_w: usize,
) {
    if let Some(timing) = entry.timing.as_ref() {
        render_summary(
            lines,
            section_map,
            json_click_map,
            json_section_keys,
            entry,
            timing,
        );
        render_phase_table(
            lines,
            section_map,
            json_click_map,
            json_section_keys,
            timing,
            inner_w,
        );
    }

    let events: Vec<TimingEvent> = entry
        .sse_chunks
        .iter()
        .enumerate()
        .filter_map(|(idx, chunk)| {
            chunk.event_timing.as_ref().map(|event| {
                let mut event = event.clone();
                if event.name == "event" {
                    event.name = format!("event #{}", idx + 1);
                }
                if event.size.is_none() {
                    event.size = Some(chunk.data.len() as u64);
                }
                event
            })
        })
        .collect();

    render_event_summary(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        "Events",
        entry.sse_chunks.len(),
        &events,
    );
    render_events(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        "Event gaps",
        &events,
        8,
    );

    if let Some(timing) = entry.timing.as_ref() {
        render_notes(
            lines,
            section_map,
            json_click_map,
            json_section_keys,
            timing,
        );
    }
}

fn render_ws(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    entry: &NetworkEntry,
    inner_w: usize,
) {
    if let Some(timing) = entry.timing.as_ref() {
        render_summary(
            lines,
            section_map,
            json_click_map,
            json_section_keys,
            entry,
            timing,
        );
        render_phase_table(
            lines,
            section_map,
            json_click_map,
            json_section_keys,
            timing,
            inner_w,
        );
    }

    let events: Vec<TimingEvent> = entry
        .ws_messages
        .iter()
        .enumerate()
        .filter_map(|(idx, message)| {
            message.event_timing.as_ref().map(|event| {
                let mut event = event.clone();
                if event.name == "event" {
                    event.name = match message.direction {
                        WsDirection::Send => format!("send #{}", idx + 1),
                        WsDirection::Recv => format!("recv #{}", idx + 1),
                    };
                }
                if event.size.is_none() {
                    event.size = Some(message.size);
                }
                event
            })
        })
        .collect();

    render_event_summary(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        "Messages",
        entry.ws_messages.len(),
        &events,
    );
    render_events(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        "Message timeline",
        &events,
        8,
    );

    if let Some(timing) = entry.timing.as_ref() {
        render_notes(
            lines,
            section_map,
            json_click_map,
            json_section_keys,
            timing,
        );
    }
}

fn render_summary(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    entry: &NetworkEntry,
    timing: &NetworkTiming,
) {
    let total_us = timing.total_duration_us().or_else(|| {
        entry
            .duration
            .map(|duration_ms| duration_ms.saturating_mul(1_000))
    });
    let bottleneck = bottleneck_phase(&timing.phases)
        .and_then(|phase| phase.duration_us().map(|duration| (phase, duration)));
    let bottleneck_text = bottleneck
        .map(|(phase, duration)| format!("{} {}", cell_text(&phase.name), format_us(duration)))
        .unwrap_or_else(|| "-".to_string());

    push_plain(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        Line::from(vec![
            Span::styled("   Total ", Style::default().fg(KEY_COLOR)),
            Span::styled(
                total_us.map(format_us).unwrap_or_else(|| "-".to_string()),
                Style::default().fg(YELLOW).add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("Bottleneck ", Style::default().fg(KEY_COLOR)),
            Span::styled(
                bottleneck_text,
                Style::default().fg(PEACH).add_modifier(Modifier::BOLD),
            ),
        ]),
    );

    let source = format!("{:?}", timing.source).to_lowercase();
    let clock = format!("{:?}", timing.clock).to_lowercase();
    push_plain(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        Line::from(vec![
            Span::styled("   Source ", Style::default().fg(KEY_COLOR)),
            Span::styled(source, Style::default().fg(SUBTEXT0)),
            Span::raw("   "),
            Span::styled("Clock ", Style::default().fg(KEY_COLOR)),
            Span::styled(clock, Style::default().fg(SUBTEXT0)),
        ]),
    );

    if let Some(connection) = &timing.connection {
        let reused = if connection.reused { "reused" } else { "new" };
        let protocol = connection.protocol.as_deref().unwrap_or("-");
        let id = connection.id.as_deref().unwrap_or("-");
        push_plain(
            lines,
            section_map,
            json_click_map,
            json_section_keys,
            Line::from(vec![
                Span::styled("   Connection ", Style::default().fg(KEY_COLOR)),
                Span::styled(reused, Style::default().fg(TEAL)),
                Span::raw("   "),
                Span::styled(cell_text(protocol), Style::default().fg(SUBTEXT0)),
                Span::raw("   "),
                Span::styled(cell_text(id), Style::default().fg(OVERLAY0)),
            ]),
        );
    }
}

fn render_phase_table(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    timing: &NetworkTiming,
    inner_w: usize,
) {
    if timing.phases.is_empty() {
        return;
    }

    push_plain(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        Line::from(Span::styled(
            "   Phase              Time    Trust       Waterfall",
            Style::default().fg(OVERLAY0),
        )),
    );

    let max_bar_w = inner_w.saturating_sub(43).min(30);
    let total = timing
        .total_duration_us()
        .or_else(|| {
            timing
                .phases
                .iter()
                .filter_map(TimingPhase::duration_us)
                .max()
        })
        .unwrap_or(1)
        .max(1);

    for phase in &timing.phases {
        let duration = phase.duration_us().unwrap_or(0);
        let bar = "█".repeat(bar_cells(duration, total, max_bar_w));
        let trust = format!("{:?}", phase.confidence).to_lowercase();
        let color = match phase.status {
            TimingPhaseStatus::Complete => PEACH,
            TimingPhaseStatus::Active => SAPPHIRE,
            TimingPhaseStatus::Reused | TimingPhaseStatus::Skipped => OVERLAY0,
            TimingPhaseStatus::Errored | TimingPhaseStatus::Cancelled => YELLOW,
            TimingPhaseStatus::Unavailable | TimingPhaseStatus::Unknown => SURFACE0,
        };
        push_plain(
            lines,
            section_map,
            json_click_map,
            json_section_keys,
            Line::from(vec![
                Span::raw(format!("   {:<17}", truncate_cell(&phase.name, 17))),
                Span::styled(
                    format!("{:>7}", format_us(duration)),
                    Style::default().fg(TEAL),
                ),
                Span::raw("   "),
                Span::styled(format!("{:<10}", trust), Style::default().fg(GREEN)),
                Span::styled(bar, Style::default().fg(color)),
            ]),
        );
    }
}

fn render_event_summary(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    label: &str,
    total_count: usize,
    events: &[TimingEvent],
) {
    let worst_gap = events.iter().filter_map(|event| event.gap_us).max();
    let bytes = events.iter().filter_map(|event| event.size).sum::<u64>();
    push_plain(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        Line::from(vec![
            Span::styled(format!("   {} ", label), Style::default().fg(KEY_COLOR)),
            Span::styled(total_count.to_string(), Style::default().fg(TEAL)),
            Span::raw("   "),
            Span::styled("Worst gap ", Style::default().fg(KEY_COLOR)),
            Span::styled(
                worst_gap.map(format_us).unwrap_or_else(|| "-".to_string()),
                Style::default().fg(PEACH).add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("Bytes ", Style::default().fg(KEY_COLOR)),
            Span::styled(bytes.to_string(), Style::default().fg(SUBTEXT0)),
        ]),
    );
}

fn render_events(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    title: &str,
    events: &[TimingEvent],
    limit: usize,
) {
    if events.is_empty() {
        return;
    }

    push_plain(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        Line::from(Span::styled(
            format!("   {}", title),
            Style::default().fg(OVERLAY0),
        )),
    );

    for event in events.iter().take(limit) {
        let at = event
            .at_us
            .map(format_us)
            .unwrap_or_else(|| "-".to_string());
        let gap = event
            .gap_us
            .map(format_us)
            .unwrap_or_else(|| "-".to_string());
        let size = event
            .size
            .map(|size| format!("{}B", size))
            .unwrap_or_else(|| "-".to_string());
        push_plain(
            lines,
            section_map,
            json_click_map,
            json_section_keys,
            Line::from(vec![
                Span::styled("   • ", Style::default().fg(MAUVE)),
                Span::styled(
                    format!("{:<16}", truncate_cell(&event.name, 16)),
                    Style::default().fg(TEXT),
                ),
                Span::styled(format!("{:>8}", at), Style::default().fg(SUBTEXT0)),
                Span::raw("  gap "),
                Span::styled(format!("{:>8}", gap), Style::default().fg(TEAL)),
                Span::raw("  "),
                Span::styled(format!("{:>7}", size), Style::default().fg(OVERLAY0)),
            ]),
        );
    }
}

fn render_notes(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    timing: &NetworkTiming,
) {
    for note in &timing.notes {
        push_plain(
            lines,
            section_map,
            json_click_map,
            json_section_keys,
            Line::from(vec![
                Span::styled("   note ", Style::default().fg(OVERLAY0)),
                Span::styled(cell_text(note), Style::default().fg(SUBTEXT0)),
            ]),
        );
    }
}

fn format_us(us: u64) -> String {
    if us >= 1_000_000 {
        let seconds = us as f64 / 1_000_000.0;
        if us.is_multiple_of(1_000_000) {
            format!("{}s", us / 1_000_000)
        } else {
            format!("{:.1}s", seconds)
        }
    } else if us >= 1_000 {
        format!("{}ms", us / 1_000)
    } else {
        format!("{}us", us)
    }
}

fn bottleneck_phase(phases: &[TimingPhase]) -> Option<&TimingPhase> {
    phases
        .iter()
        .filter(|phase| phase.status == TimingPhaseStatus::Complete)
        .max_by_key(|phase| phase.duration_us().unwrap_or(0))
}

fn bar_cells(value: u64, total: u64, max_w: usize) -> usize {
    if max_w == 0 {
        return 0;
    }
    if total == 0 || value == 0 {
        return 1;
    }
    (((value as f64 / total as f64) * max_w as f64).ceil() as usize).clamp(1, max_w)
}

fn cell_text(value: &str) -> String {
    sanitize_for_cell(value).into_owned()
}

fn truncate_cell(value: &str, max_chars: usize) -> String {
    let value = cell_text(value);
    let count = value.chars().count();
    if count <= max_chars {
        value
    } else {
        let keep = max_chars.saturating_sub(1);
        format!("{}…", value.chars().take(keep).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::network_timing::{TimingConfidence, TimingPhase, TimingPhaseStatus};

    fn phase(name: &str, start_us: u64, end_us: u64) -> TimingPhase {
        TimingPhase {
            name: name.to_string(),
            start_us: Some(start_us),
            end_us: Some(end_us),
            status: TimingPhaseStatus::Complete,
            confidence: TimingConfidence::Exact,
            detail: None,
        }
    }

    #[test]
    fn format_us_uses_ms_and_seconds() {
        assert_eq!(format_us(999), "999us");
        assert_eq!(format_us(1_000), "1ms");
        assert_eq!(format_us(126_000), "126ms");
        assert_eq!(format_us(1_500_000), "1.5s");
    }

    #[test]
    fn bottleneck_picks_longest_complete_phase() {
        let phases = vec![
            phase("dns", 0, 7_000),
            phase("ttfb", 62_000, 104_000),
            phase("decode", 104_000, 112_000),
        ];
        let found = bottleneck_phase(&phases).expect("bottleneck");
        assert_eq!(found.name, "ttfb");
    }

    #[test]
    fn bar_width_is_bounded() {
        assert_eq!(bar_cells(0, 100, 20), 1);
        assert_eq!(bar_cells(50, 100, 20), 10);
        assert_eq!(bar_cells(100, 100, 20), 20);
    }
}
