use crate::domain::network_timing::{NetworkTiming, TimingEvent, TimingPhase, TimingPhaseStatus};
use crate::ui::sanitize_for_cell;

pub(super) fn format_us(us: u64) -> String {
    if us >= 1_000_000 {
        let seconds = us as f64 / 1_000_000.0;
        if us.is_multiple_of(1_000_000) {
            format!("{}s", us / 1_000_000)
        } else {
            format!("{seconds:.1}s")
        }
    } else if us >= 1_000 {
        format!("{}ms", us / 1_000)
    } else {
        format!("{us}us")
    }
}

pub(super) fn format_size(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1}MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes}B")
    }
}

pub(super) fn total_for_timing(timing: &NetworkTiming, fallback_ms: Option<u64>) -> u64 {
    timing
        .total_duration_us()
        .or_else(|| fallback_ms.map(|ms| ms.saturating_mul(1_000)))
        .or_else(|| {
            let min_start = timing
                .phases
                .iter()
                .filter_map(|phase| phase.start_us)
                .min();
            let max_end = timing.phases.iter().filter_map(|phase| phase.end_us).max();
            min_start
                .zip(max_end)
                .map(|(start, end)| end.saturating_sub(start))
        })
        .unwrap_or(1)
        .max(1)
}

pub(super) fn total_for_events(timing: Option<&NetworkTiming>, events: &[TimingEvent]) -> u64 {
    let from_bounds = timing.and_then(NetworkTiming::total_duration_us);
    let start_us = timing.and_then(|timing| timing.start_us);
    let from_events = events
        .iter()
        .filter_map(|event| event.at_us)
        .max()
        .map(|end| start_us.map_or(end, |start| end.saturating_sub(start)));

    from_bounds
        .unwrap_or(0)
        .max(from_events.unwrap_or(0))
        .max(1)
}

pub(super) fn event_offset_us(event: &TimingEvent, start_us: Option<u64>) -> Option<u64> {
    let at_us = event.at_us?;
    Some(start_us.map_or(at_us, |start| at_us.saturating_sub(start)))
}

pub(super) fn format_event_offset(event: &TimingEvent, start_us: Option<u64>) -> String {
    event_offset_us(event, start_us)
        .map(|offset| format!("+{}", format_us(offset)))
        .unwrap_or_else(|| "-".to_string())
}

pub(super) fn format_event_offset_precise(event: &TimingEvent, start_us: Option<u64>) -> String {
    event_offset_us(event, start_us)
        .map(|offset| format!("+{}", format_us_precise(offset)))
        .unwrap_or_else(|| "-".to_string())
}

fn format_us_precise(us: u64) -> String {
    if us >= 1_000_000 {
        format!("{:.3}s", us as f64 / 1_000_000.0)
    } else if us >= 1_000 {
        format!("{:.1}ms", us as f64 / 1_000.0)
    } else {
        format!("{us}us")
    }
}

pub(super) fn first_event_offset(
    timing: Option<&NetworkTiming>,
    events: &[TimingEvent],
) -> Option<String> {
    let first = events.iter().find(|event| event.at_us.is_some())?;
    Some(format_event_offset(
        first,
        timing.and_then(|timing| timing.start_us),
    ))
}

pub(super) fn max_event_gap(events: &[TimingEvent]) -> Option<u64> {
    events.iter().filter_map(|event| event.gap_us).max()
}

pub(super) fn phase_display_name(name: &str) -> String {
    match name {
        "headers" | "request_to_headers" | "ttfb" | "first_byte" => "TTFB".to_string(),
        "body" | "download" | "response_body" => "Download".to_string(),
        "request_body" | "upload" => "Upload".to_string(),
        "dns" => "DNS".to_string(),
        "tcp" | "connect" => "TCP".to_string(),
        "tls" => "TLS".to_string(),
        "handshake" => "Handshake".to_string(),
        "active" => "Active".to_string(),
        "wait_first_event" => "Wait".to_string(),
        "receive_stream" => "Receive".to_string(),
        other => cell_text(other),
    }
}

pub(super) fn bottleneck_phase(phases: &[TimingPhase]) -> Option<&TimingPhase> {
    phases
        .iter()
        .filter(|phase| phase.status == TimingPhaseStatus::Complete)
        .max_by_key(|phase| phase.duration_us().unwrap_or(0))
}

pub(super) fn cell_text(value: &str) -> String {
    sanitize_for_cell(value).into_owned()
}

pub(super) fn truncate_cell(value: &str, max_chars: usize) -> String {
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
    use crate::domain::network_timing::{TimingConfidence, TimingPhaseStatus};

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
    fn event_offsets_are_relative_to_timing_start() {
        let event = TimingEvent {
            name: "chunk".to_string(),
            at_us: Some(72_400_000),
            gap_us: None,
            size: None,
            detail: None,
        };
        assert_eq!(format_event_offset(&event, Some(71_000_000)), "+1.4s");
    }

    #[test]
    fn event_offsets_precise_keep_millisecond_order() {
        let event = TimingEvent {
            name: "chunk".to_string(),
            at_us: Some(1_509_000),
            gap_us: None,
            size: None,
            detail: None,
        };
        assert_eq!(format_event_offset_precise(&event, Some(0)), "+1.509s");
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
}
