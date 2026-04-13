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

/// Pipe format: `HH:MM:SS.mmm │ LEVEL │ Tag │ message`
static PIPE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\d{2}:\d{2}:\d{2}\.\d{3})\s*│\s*(\w+)\s*│\s*(\S.*?)\s*│\s?(.*)$").unwrap()
});

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

        // Try bracket format first: [LEVEL][Tag] message
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
                source: InputSource::Adb,
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
                source: InputSource::Adb,
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
}
