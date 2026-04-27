//! Pure parsing + message dispatch.
//!
//! `dispatch_client_message` is the bridge between the WS protocol
//! layer and the app state machine: every `ClientMessage` delivered by
//! the connector passes through here and is turned into either a
//! `LogEntry` push or a `NetworkStore` update.

use crate::app::App;
use crate::domain;
use crate::input::ClientMessage;

/// Pattern: `[LEVEL][Tag] message` — used to parse raw log text.
/// Optionally preceded by `[epoch_ms]` which is ignored (timestamp
/// comes from the message field). Applied against the first line
/// only; stack frames on subsequent lines are extracted separately.
pub(crate) static RAW_LOG_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"^(?:\[\d{10,13}\])?\[(\w+)\]\[([^\]]+)\]\s?(.*)$").unwrap()
});

/// Detects Dart stack frame lines: `#N ...`.
static STACK_FRAME_RE: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"^#\d+\s").unwrap());

/// Split a message body into (leading_text, Option<stacktrace>).
///
/// The stacktrace begins at the first line matching `#\d+ ` and
/// continues to the end. Both halves are returned with trailing
/// newlines trimmed.
pub(crate) fn split_stacktrace(body: &str) -> (String, Option<String>) {
    let mut split_at: Option<usize> = None;
    let mut cursor = 0usize;
    for line in body.split_inclusive('\n') {
        let line_no_nl = line.strip_suffix('\n').unwrap_or(line);
        if STACK_FRAME_RE.is_match(line_no_nl) {
            split_at = Some(cursor);
            break;
        }
        cursor += line.len();
    }
    match split_at {
        Some(idx) => {
            let head = body[..idx].trim_end_matches(['\n', ' ']).to_string();
            let stack = body[idx..].trim_end_matches('\n').to_string();
            let stack_opt = if stack.is_empty() { None } else { Some(stack) };
            (head, stack_opt)
        }
        None => (body.trim_end_matches('\n').to_string(), None),
    }
}

/// Convert epoch milliseconds to HH:MM:SS.mmm.
pub(crate) fn format_ts(ms: u64) -> String {
    let secs = ms / 1000;
    let millis = ms % 1000;
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        (secs % 86400) / 3600,
        (secs % 3600) / 60,
        secs % 60,
        millis
    )
}

/// Dispatch a client message to the app.
pub(crate) fn dispatch_client_message(app: &mut App, msg: ClientMessage) {
    match msg {
        ClientMessage::Hello { .. } => {
            // Hello is handled at connection time, nothing more to do here
        }
        ClientMessage::Log {
            level,
            tag,
            message,
            error,
            stack_trace,
            timestamp,
        } => {
            // Match `[LEVEL][Tag] ...` against the first line only; remaining lines may
            // carry error text and/or a Dart stack trace (`#N ...` + asynchronous suspension).
            let (first_line, rest) = match message.split_once('\n') {
                Some((head, tail)) => (head, Some(tail)),
                None => (message.as_str(), None),
            };

            let entry = if let (Some(level), Some(tag)) = (level, tag) {
                // Structured log from FlogLogger — level/tag provided explicitly.
                let log_level =
                    domain::LogLevel::from_str(&level).unwrap_or(domain::LogLevel::Info);
                let mut e = domain::LogEntry::new(log_level, tag, message);
                e.error = error;
                e.stacktrace = stack_trace;
                if let Some(ts) = timestamp {
                    e.timestamp = format_ts(ts);
                }
                e
            } else if let Some(caps) = RAW_LOG_RE.captures(first_line) {
                // Raw text matching [LEVEL][Tag] format (e.g. AuraLogger via debugPrint).
                let level_str = caps.get(1).unwrap().as_str();
                let tag_str = caps.get(2).unwrap().as_str();
                let msg_str = caps.get(3).unwrap().as_str();
                let log_level =
                    domain::LogLevel::from_str(level_str).unwrap_or(domain::LogLevel::Debug);

                let (extra_body, stacktrace) = match rest {
                    Some(r) => split_stacktrace(r),
                    None => (String::new(), None),
                };

                // Treat non-stack text after the first line as continuation of the message.
                let full_msg = if extra_body.is_empty() {
                    msg_str.to_string()
                } else {
                    format!("{msg_str}\n{extra_body}")
                };

                let mut e = domain::LogEntry::new(log_level, tag_str, full_msg);
                e.stacktrace = stacktrace;
                if let Some(ts) = timestamp {
                    e.timestamp = format_ts(ts);
                }
                e
            } else {
                // Unstructured raw text (e.g. Flutter framework output via debugPrint).
                // Still split off `#N ...` stack frames so the list view can collapse them.
                let (body, stacktrace) = split_stacktrace(&message);
                let mut e = domain::LogEntry::new(domain::LogLevel::Debug, "debugPrint", body);
                e.stacktrace = stacktrace;
                if let Some(ts) = timestamp {
                    e.timestamp = format_ts(ts);
                }
                e
            };
            app.add_entry(entry);
        }
        ClientMessage::Net { msg } => {
            app.network_store.process_message(msg);
            app.network.invalidate_filter();
        }
    }
}
