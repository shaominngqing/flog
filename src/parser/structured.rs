//! Parser for the structured `[LEVEL][Tag]` format.
//!
//! Recognizes two formats:
//!   1. Bracket: `[LEVEL][Tag] message`       (via print / stdout)
//!   2. Pipe:    `HH:MM:SS.mmm │ LEVEL │ Tag │ message`  (legacy)
//!
//! Both may be wrapped in a Flutter prefix:
//!   `I/flutter (PID): [INFO][Network] ...`
//!   `flutter: [DEBUG][GoalRepo] ...`

use regex::Regex;
use std::sync::LazyLock;

use super::LogLineParser;
use crate::domain::{InputSource, LogEntry, LogLevel};

static ANSI_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\x1b\[[0-9;]*m").unwrap());

/// Matches ADB logcat format: `I/flutter (PID): content`
/// and VM Service stdout format: `flutter: content`
static FLUTTER_PREFIX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:I/flutter\s*\(\s*\d+\)|flutter):\s?(.*)$").unwrap());

/// Bracket format: `[LEVEL][Tag] message`
static BRACKET_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[(\w+)\]\[([^\]]+)\]\s?(.*)$").unwrap());

/// Bracket format with epoch timestamp: `[1776241660875][LEVEL][Tag] message`
static BRACKET_TS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[(\d{10,13})\]\[(\w+)\]\[([^\]]+)\]\s?(.*)$").unwrap());

/// Pipe format: `HH:MM:SS.mmm │ LEVEL │ Tag │ message`
static PIPE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\d{2}:\d{2}:\d{2}\.\d{3})\s*│\s*(\w+)\s*│\s*(\S.*?)\s*│\s?(.*)$").unwrap()
});

/// Pipe format with epoch timestamp: `1776241660875|LEVEL|Tag|message` (with optional spaces)
static PIPE_TS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\d{10,13})\s*\|\s*(\w+)\s*\|\s*(\S.*?)\s*\|\s?(.*)$").unwrap()
});

/// Convert epoch milliseconds to HH:MM:SS.mmm format.
fn epoch_ms_to_timestamp(ms_str: &str) -> String {
    if let Ok(ms) = ms_str.parse::<u64>() {
        let secs = ms / 1000;
        let millis = ms % 1000;
        format!(
            "{:02}:{:02}:{:02}.{:03}",
            (secs / 3600) % 24,
            (secs / 60) % 60,
            secs % 60,
            millis
        )
    } else {
        String::new()
    }
}

/// Extracts the Flutter payload from a logcat line, stripping ANSI codes.
fn extract_content(line: &str) -> String {
    let raw = match FLUTTER_PREFIX_RE.captures(line) {
        Some(caps) => caps.get(1).map(|m| m.as_str()).unwrap_or(""),
        None => line,
    };
    ANSI_RE.replace_all(raw, "").to_string()
}

pub struct StructuredParser;

impl LogLineParser for StructuredParser {
    fn name(&self) -> &'static str {
        "Structured"
    }

    fn try_parse(&self, line: &str) -> Option<LogEntry> {
        let content = extract_content(line);

        // Try bracket format with epoch timestamp: [1776241660875][LEVEL][Tag] message
        if let Some(caps) = BRACKET_TS_RE.captures(&content) {
            let timestamp = epoch_ms_to_timestamp(caps.get(1)?.as_str());
            let level = LogLevel::from_str(caps.get(2)?.as_str()).unwrap_or(LogLevel::Debug);
            let tag = caps.get(3)?.as_str().trim().to_string();
            let message = caps.get(4)?.as_str().to_string();
            return Some(LogEntry {
                timestamp,
                level,
                tag,
                message,
                extra_lines: Vec::new(),
                repeat_count: 1,
                source: InputSource::DirectSocket,
                error: None,
                stacktrace: None,
            });
        }

        // Try pipe format with epoch timestamp: 1776241660875|LEVEL|Tag|message
        if let Some(caps) = PIPE_TS_RE.captures(&content) {
            let timestamp = epoch_ms_to_timestamp(caps.get(1)?.as_str());
            let level = LogLevel::from_str(caps.get(2)?.as_str()).unwrap_or(LogLevel::Debug);
            let tag = caps.get(3)?.as_str().trim().to_string();
            let message = caps.get(4)?.as_str().to_string();
            return Some(LogEntry {
                timestamp,
                level,
                tag,
                message,
                extra_lines: Vec::new(),
                repeat_count: 1,
                source: InputSource::DirectSocket,
                error: None,
                stacktrace: None,
            });
        }

        // Try bracket format: [LEVEL][Tag] message
        if let Some(caps) = BRACKET_RE.captures(&content) {
            let level = LogLevel::from_str(caps.get(1)?.as_str()).unwrap_or(LogLevel::Debug);
            let tag = caps.get(2)?.as_str().trim().to_string();
            let message = caps.get(3)?.as_str().to_string();
            return Some(LogEntry {
                timestamp: String::new(),
                level,
                tag,
                message,
                extra_lines: Vec::new(),
                repeat_count: 1,
                source: InputSource::DirectSocket,
                error: None,
                stacktrace: None,
            });
        }

        // Try pipe format: HH:MM:SS.mmm │ LEVEL │ Tag │ message
        if let Some(caps) = PIPE_RE.captures(&content) {
            let timestamp = caps.get(1)?.as_str().to_string();
            let level = LogLevel::from_str(caps.get(2)?.as_str()).unwrap_or(LogLevel::Debug);
            let tag = caps.get(3)?.as_str().trim().to_string();
            let message = caps.get(4)?.as_str().to_string();
            return Some(LogEntry {
                timestamp,
                level,
                tag,
                message,
                extra_lines: Vec::new(),
                repeat_count: 1,
                source: InputSource::DirectSocket,
                error: None,
                stacktrace: None,
            });
        }

        None
    }

    fn try_continuation(&self, _line: &str) -> Option<String> {
        // No continuation — every structured-format line is self-contained
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bracket_with_flutter_prefix() {
        let p = StructuredParser;
        let line = "I/flutter (14114): [INFO][Network] → GET /api/scene-types";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.tag, "Network");
        assert!(entry.message.contains("GET"));
    }

    #[test]
    fn parse_bracket_raw() {
        let p = StructuredParser;
        let line = "[DEBUG][GoalRepo] Loaded 42 scene types";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.level, LogLevel::Debug);
        assert_eq!(entry.tag, "GoalRepo");
        assert!(entry.message.contains("42"));
    }

    #[test]
    fn parse_bracket_vm_service_stdout() {
        let p = StructuredParser;
        let line = "flutter: [WARNING][SessionCoord] GoAway received";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.level, LogLevel::Warning);
        assert_eq!(entry.tag, "SessionCoord");
    }

    #[test]
    fn parse_pipe_format() {
        let p = StructuredParser;
        let line =
            "I/flutter (14114): 18:05:26.675 │ INFO    │ Network        │ → GET /api/scene-types";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.timestamp, "18:05:26.675");
        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.tag, "Network");
        assert!(entry.message.contains("GET"));
    }

    #[test]
    fn ignores_non_flutter() {
        let p = StructuredParser;
        assert!(p.try_parse("W/1.raster(13383): type=1400").is_none());
    }

    #[test]
    fn strips_ansi() {
        let p = StructuredParser;
        let line = "I/flutter (14114): \x1b[34m[INFO][Network] test\x1b[0m";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.tag, "Network");
    }

    #[test]
    fn parse_bracket_with_epoch_timestamp() {
        let p = StructuredParser;
        let line = "I/flutter (29942): [1776241660875][INFO][Network] → GET /api/users";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.tag, "Network");
        assert!(entry.message.contains("GET"));
        assert!(!entry.timestamp.is_empty());
        assert!(entry.timestamp.contains(":"));
    }

    #[test]
    fn parse_pipe_with_epoch_timestamp() {
        let p = StructuredParser;
        let line = "1776241660875|INFO|Network|→ GET /api/users";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.tag, "Network");
        assert!(entry.message.contains("GET"));
        assert!(!entry.timestamp.is_empty());
    }

    #[test]
    fn parse_pipe_with_epoch_timestamp_spaces() {
        let p = StructuredParser;
        let line = "1776241660875 | INFO | Network | → GET /api/users";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.tag, "Network");
    }
}
