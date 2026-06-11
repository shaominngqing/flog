//! Network detail Timing section renderers.
//!
//! Timing is rendered as a terminal-native timeline: a compact summary first,
//! then a shared time axis and protocol-specific tracks.
//! This file intentionally stays in the yellow size band because it is the
//! protocol orchestration layer; formatting and timeline primitives are split
//! into sibling modules.

use std::collections::HashSet;

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::domain::network::{NetworkEntry, Protocol, WsDirection};
use crate::domain::network_timing::{NetworkTiming, TimingEvent, TimingPhaseStatus};
use crate::ui::json_viewer::JsonHotRegion;
use crate::ui::{MAUVE, OVERLAY0, PEACH, SAPPHIRE, SUBTEXT0, SURFACE0, TEAL, YELLOW};

use super::shared::push_section_header;
use super::KEY_COLOR;

mod format;
mod timeline;

use self::format::{
    bottleneck_phase, event_offset_us, first_event_offset, format_event_offset, format_size,
    format_us, max_event_gap, phase_display_name, total_for_events, total_for_timing,
};
use self::timeline::{
    marker_positions, render_axis, render_bar_track, render_event_table, render_marker_track,
    TimelineEventRow, TimelineSpec, TimingLines,
};

const LABEL_WIDTH: usize = 10;

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

pub(super) fn push_plain(
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

    render_http_summary(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        entry,
        timing,
    );

    let total_us = total_for_timing(timing, entry.duration);
    let spec = TimelineSpec::new(LABEL_WIDTH, inner_w, total_us);
    render_axis(lines, section_map, json_click_map, json_section_keys, &spec);
    render_phase_tracks(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        timing,
        &spec,
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
    let events: Vec<TimingEvent> = entry
        .sse_chunks
        .iter()
        .enumerate()
        .filter_map(|(idx, chunk)| {
            chunk.event_timing.as_ref().map(|event| {
                let mut event = event.clone();
                if event.name == "event" {
                    event.name = format!("#{}", idx + 1);
                }
                if event.size.is_none() {
                    event.size = Some(chunk.data.len() as u64);
                }
                event
            })
        })
        .collect();

    render_sse_summary(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        entry.timing.as_ref(),
        &events,
    );

    let total_us = total_for_events(entry.timing.as_ref(), &events);
    let spec = TimelineSpec::new(LABEL_WIDTH, inner_w, total_us);
    render_axis(lines, section_map, json_click_map, json_section_keys, &spec);
    if let Some(timing) = entry.timing.as_ref() {
        render_phase_tracks(
            lines,
            section_map,
            json_click_map,
            json_section_keys,
            timing,
            &spec,
        );
    }
    render_sse_track(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        entry,
        &events,
        &spec,
    );
    {
        let mut target = TimingLines {
            lines,
            section_map,
            json_click_map,
            json_section_keys,
        };
        render_event_table(
            &mut target,
            &events,
            entry.timing.as_ref().and_then(|timing| timing.start_us),
            TimelineEventRow::Sse,
            8,
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
    let events: Vec<TimingEvent> = entry
        .ws_messages
        .iter()
        .enumerate()
        .filter_map(|(idx, message)| {
            message.event_timing.as_ref().map(|event| {
                let mut event = event.clone();
                event.name = match message.direction {
                    WsDirection::Send => format!("send #{}", idx + 1),
                    WsDirection::Recv => format!("recv #{}", idx + 1),
                };
                if event.size.is_none() {
                    event.size = Some(message.size);
                }
                event
            })
        })
        .collect();

    render_ws_summary(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        entry,
        entry.timing.as_ref(),
        &events,
    );

    let total_us = total_for_events(entry.timing.as_ref(), &events);
    let spec = TimelineSpec::new(LABEL_WIDTH, inner_w, total_us);
    render_axis(lines, section_map, json_click_map, json_section_keys, &spec);
    if let Some(timing) = entry.timing.as_ref() {
        render_phase_tracks(
            lines,
            section_map,
            json_click_map,
            json_section_keys,
            timing,
            &spec,
        );
    }
    render_ws_tracks(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        entry,
        &events,
        &spec,
    );
    {
        let mut target = TimingLines {
            lines,
            section_map,
            json_click_map,
            json_section_keys,
        };
        render_event_table(
            &mut target,
            &events,
            entry.timing.as_ref().and_then(|timing| timing.start_us),
            TimelineEventRow::Ws,
            8,
        );
    }
}

fn render_http_summary(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    entry: &NetworkEntry,
    timing: &NetworkTiming,
) {
    let total_us = total_for_timing(timing, entry.duration);
    let bottleneck = bottleneck_phase(&timing.phases)
        .and_then(|phase| phase.duration_us().map(|duration| (phase, duration)));
    let bottleneck_text = bottleneck
        .map(|(phase, duration)| {
            format!(
                "{} {}",
                phase_display_name(&phase.name),
                format_us(duration)
            )
        })
        .unwrap_or_else(|| "-".to_string());
    let first = timing
        .events
        .iter()
        .find(|event| event.name == "first_byte")
        .or_else(|| timing.events.first());
    let last = timing.events.last();
    let bytes = timing
        .events
        .iter()
        .filter_map(|event| event.size)
        .sum::<u64>()
        .max(entry.response_size.unwrap_or(0));

    push_plain(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        Line::from(vec![
            Span::styled("   Total ", Style::default().fg(KEY_COLOR)),
            Span::styled(
                format_us(total_us),
                Style::default().fg(YELLOW).add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("Bottleneck ", Style::default().fg(KEY_COLOR)),
            Span::styled(
                bottleneck_text,
                Style::default().fg(PEACH).add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("First byte ", Style::default().fg(KEY_COLOR)),
            Span::styled(
                first
                    .map(|event| format_event_offset(event, timing.start_us))
                    .unwrap_or_else(|| "-".to_string()),
                Style::default().fg(TEAL),
            ),
            Span::raw("   "),
            Span::styled("Last byte ", Style::default().fg(KEY_COLOR)),
            Span::styled(
                last.map(|event| format_event_offset(event, timing.start_us))
                    .unwrap_or_else(|| "-".to_string()),
                Style::default().fg(TEAL),
            ),
            Span::raw("   "),
            Span::styled("Bytes ", Style::default().fg(KEY_COLOR)),
            Span::styled(format_size(bytes), Style::default().fg(SUBTEXT0)),
        ]),
    );
}

fn render_sse_summary(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    timing: Option<&NetworkTiming>,
    events: &[TimingEvent],
) {
    let total_us = total_for_events(timing, events);
    let first_event = first_event_offset(timing, events).unwrap_or_else(|| "-".to_string());
    let worst_gap = max_event_gap(events);
    let bytes = events.iter().filter_map(|event| event.size).sum::<u64>();

    push_plain(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        Line::from(vec![
            Span::styled("   Stream ", Style::default().fg(KEY_COLOR)),
            Span::styled(
                format_us(total_us),
                Style::default().fg(YELLOW).add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("First chunk ", Style::default().fg(KEY_COLOR)),
            Span::styled(first_event, Style::default().fg(TEAL)),
            Span::raw("   "),
            Span::styled("Worst gap ", Style::default().fg(KEY_COLOR)),
            Span::styled(
                worst_gap.map(format_us).unwrap_or_else(|| "-".to_string()),
                Style::default().fg(PEACH).add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("Chunks ", Style::default().fg(KEY_COLOR)),
            Span::styled(events.len().to_string(), Style::default().fg(TEAL)),
            Span::raw("   "),
            Span::styled("Bytes ", Style::default().fg(KEY_COLOR)),
            Span::styled(format_size(bytes), Style::default().fg(SUBTEXT0)),
        ]),
    );
}

fn render_ws_summary(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    entry: &NetworkEntry,
    timing: Option<&NetworkTiming>,
    events: &[TimingEvent],
) {
    let lifetime = total_for_events(timing, events);
    let worst_idle = max_event_gap(events);
    let bytes = entry
        .ws_messages
        .iter()
        .map(|message| message.size)
        .sum::<u64>();
    let send_count = entry
        .ws_messages
        .iter()
        .filter(|message| message.direction == WsDirection::Send)
        .count();
    let recv_count = entry.ws_messages.len().saturating_sub(send_count);

    push_plain(
        lines,
        section_map,
        json_click_map,
        json_section_keys,
        Line::from(vec![
            Span::styled("   Lifetime ", Style::default().fg(KEY_COLOR)),
            Span::styled(
                format_us(lifetime),
                Style::default().fg(YELLOW).add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("Messages ", Style::default().fg(KEY_COLOR)),
            Span::styled(
                format!("{send_count} sent / {recv_count} recv"),
                Style::default().fg(TEAL),
            ),
            Span::raw("   "),
            Span::styled("Worst idle ", Style::default().fg(KEY_COLOR)),
            Span::styled(
                worst_idle.map(format_us).unwrap_or_else(|| "-".to_string()),
                Style::default().fg(PEACH).add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("Bytes ", Style::default().fg(KEY_COLOR)),
            Span::styled(format_size(bytes), Style::default().fg(SUBTEXT0)),
        ]),
    );
}

fn render_phase_tracks(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    timing: &NetworkTiming,
    spec: &TimelineSpec,
) {
    let base_us = timing.start_us.or_else(|| {
        timing
            .phases
            .iter()
            .filter_map(|phase| phase.start_us)
            .min()
    });

    for phase in &timing.phases {
        let duration = phase.duration_us().unwrap_or(0);
        let offset = phase
            .start_us
            .zip(base_us)
            .map(|(start, base)| start.saturating_sub(base))
            .unwrap_or(0);
        let color = match phase.status {
            TimingPhaseStatus::Complete => PEACH,
            TimingPhaseStatus::Active => SAPPHIRE,
            TimingPhaseStatus::Reused | TimingPhaseStatus::Skipped => OVERLAY0,
            TimingPhaseStatus::Errored | TimingPhaseStatus::Cancelled => YELLOW,
            TimingPhaseStatus::Unavailable | TimingPhaseStatus::Unknown => SURFACE0,
        };
        {
            let mut target = TimingLines {
                lines,
                section_map,
                json_click_map,
                json_section_keys,
            };
            render_bar_track(
                &mut target,
                spec,
                &phase_display_name(&phase.name),
                offset,
                duration,
                color,
                &format!(
                    "{} {}",
                    format_us(duration),
                    format!("{:?}", phase.confidence).to_lowercase()
                ),
            );
        }
    }
}

fn render_sse_track(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    entry: &NetworkEntry,
    events: &[TimingEvent],
    spec: &TimelineSpec,
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
            "   Chunk timeline",
            Style::default().fg(OVERLAY0),
        )),
    );

    let start_us = entry.timing.as_ref().and_then(|timing| timing.start_us);
    let positions = marker_positions(events, start_us, spec, '●');
    {
        let mut target = TimingLines {
            lines,
            section_map,
            json_click_map,
            json_section_keys,
        };
        render_marker_track(&mut target, spec, "Chunks", &positions, MAUVE, "");
    }
    {
        let mut target = TimingLines {
            lines,
            section_map,
            json_click_map,
            json_section_keys,
        };
        render_first_and_worst_annotations(&mut target, spec, events, start_us, "gap");
    }
}

fn render_ws_tracks(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    entry: &NetworkEntry,
    events: &[TimingEvent],
    spec: &TimelineSpec,
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
            "   Message timeline",
            Style::default().fg(OVERLAY0),
        )),
    );

    let start_us = entry.timing.as_ref().and_then(|timing| timing.start_us);
    let send_events: Vec<TimingEvent> = events
        .iter()
        .filter(|event| event.name.starts_with("send"))
        .cloned()
        .collect();
    let recv_events: Vec<TimingEvent> = events
        .iter()
        .filter(|event| event.name.starts_with("recv"))
        .cloned()
        .collect();

    {
        let mut target = TimingLines {
            lines,
            section_map,
            json_click_map,
            json_section_keys,
        };
        render_marker_track(
            &mut target,
            spec,
            "→ Send",
            &marker_positions(&send_events, start_us, spec, '→'),
            SAPPHIRE,
            "",
        );
    }
    {
        let mut target = TimingLines {
            lines,
            section_map,
            json_click_map,
            json_section_keys,
        };
        render_marker_track(
            &mut target,
            spec,
            "← Recv",
            &marker_positions(&recv_events, start_us, spec, '←'),
            TEAL,
            "",
        );
    }
    {
        let mut target = TimingLines {
            lines,
            section_map,
            json_click_map,
            json_section_keys,
        };
        render_worst_annotation(&mut target, spec, events, start_us, "idle");
    }
}

fn render_first_and_worst_annotations(
    target: &mut TimingLines<'_>,
    spec: &TimelineSpec,
    events: &[TimingEvent],
    start_us: Option<u64>,
    gap_label: &str,
) {
    if let Some(first) = events.iter().find(|event| event.at_us.is_some()) {
        render_annotation(
            target,
            spec,
            event_offset_us(first, start_us).unwrap_or(0),
            &format!("first {}", format_event_offset(first, start_us)),
        );
    }
    render_worst_annotation(target, spec, events, start_us, gap_label);
}

fn render_worst_annotation(
    target: &mut TimingLines<'_>,
    spec: &TimelineSpec,
    events: &[TimingEvent],
    start_us: Option<u64>,
    gap_label: &str,
) {
    let Some(event) = events
        .iter()
        .filter(|event| event.gap_us.is_some())
        .max_by_key(|event| event.gap_us.unwrap_or(0))
    else {
        return;
    };
    let gap = event.gap_us.unwrap_or(0);
    render_annotation(
        target,
        spec,
        event_offset_us(event, start_us).unwrap_or(0),
        &format!("worst {gap_label} {}", format_us(gap)),
    );
}

fn render_annotation(
    target: &mut TimingLines<'_>,
    spec: &TimelineSpec,
    offset_us: u64,
    text: &str,
) {
    let mut marker = vec![' '; spec.width];
    let cell = spec.cell_for(offset_us);
    if cell < marker.len() {
        marker[cell] = '^';
    }
    push_plain(
        target.lines,
        target.section_map,
        target.json_click_map,
        target.json_section_keys,
        Line::from(vec![
            Span::raw(format!("   {:<width$} ", "", width = spec.label_width)),
            Span::styled(marker.iter().collect::<String>(), Style::default().fg(TEAL)),
            Span::raw(" "),
            Span::styled(text.to_string(), Style::default().fg(TEAL)),
        ]),
    );
}
