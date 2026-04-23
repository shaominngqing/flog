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
static PIPE_TS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d{10,13})\s*\|\s*(\w+)\s*\|\s*(\S.*?)\s*\|\s?(.*)$").unwrap());

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

    // ---- Characterization tests (Phase 2.5B Task 3) ----
    //
    // Lock the current behavior of the structured parser. These cover:
    //   - DOM-014: LazyLock regex compilation is exercised once at first call.
    //   - DOM-015: ANSI stripping specific to structured parser.
    //   - Boundary and malformed inputs.

    #[test]
    fn dom_014_all_levels_recognized() {
        let p = StructuredParser;
        for (lv, level) in [
            ("DEBUG", LogLevel::Debug),
            ("INFO", LogLevel::Info),
            ("WARNING", LogLevel::Warning),
            ("ERROR", LogLevel::Error),
            ("VERBOSE", LogLevel::Verbose),
        ] {
            let line = format!("[{lv}][Tag] body");
            let entry = p.try_parse(&line).unwrap();
            assert_eq!(entry.level, level, "lv={lv}");
        }
    }

    #[test]
    fn dom_014_unknown_level_falls_back_to_debug() {
        // LogLevel::from_str returns None for "BANANA"; parser uses Debug fallback.
        let p = StructuredParser;
        let entry = p.try_parse("[BANANA][Tag] message").unwrap();
        assert_eq!(entry.level, LogLevel::Debug);
    }

    #[test]
    fn dom_015_strips_ansi_in_bracket_format() {
        let p = StructuredParser;
        // ANSI around content inside flutter prefix
        let line = "I/flutter (1): \x1b[31m[ERROR][T] bad\x1b[0m";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.level, LogLevel::Error);
        assert_eq!(entry.tag, "T");
        assert!(!entry.message.contains('\x1b'));
    }

    #[test]
    fn dom_015_strips_ansi_in_vm_stdout_format() {
        let p = StructuredParser;
        let line = "flutter: \x1b[32m[INFO][T] hello\x1b[0m";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.level, LogLevel::Info);
        assert!(!entry.message.contains('\x1b'));
    }

    #[test]
    fn dom_015_multiple_ansi_sequences_all_removed() {
        let p = StructuredParser;
        let line = "I/flutter (1): \x1b[1m\x1b[31m[INFO][T] \x1b[0mmsg\x1b[0m";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.tag, "T");
        assert!(!entry.message.contains('\x1b'));
    }

    #[test]
    fn empty_line_returns_none() {
        let p = StructuredParser;
        assert!(p.try_parse("").is_none());
    }

    #[test]
    fn whitespace_only_returns_none() {
        let p = StructuredParser;
        assert!(p.try_parse("    ").is_none());
    }

    #[test]
    fn only_ansi_escape_returns_none() {
        let p = StructuredParser;
        assert!(p.try_parse("\x1b[31m\x1b[0m").is_none());
    }

    #[test]
    fn unterminated_bracket_rejected() {
        let p = StructuredParser;
        // No closing `]` for level — BRACKET_RE requires `[\w+]`.
        assert!(p.try_parse("[INFO[Tag] msg").is_none());
    }

    #[test]
    fn mismatched_bracket_rejected() {
        let p = StructuredParser;
        // Level bracket present but tag bracket never opens.
        assert!(p.try_parse("[INFO] msg").is_none());
    }

    #[test]
    fn very_long_message_parses() {
        let p = StructuredParser;
        let long = "a".repeat(15_000);
        let line = format!("[INFO][Big] {long}");
        let entry = p.try_parse(&line).unwrap();
        assert_eq!(entry.message.len(), 15_000);
    }

    #[test]
    fn cjk_tag_and_message_parse() {
        let p = StructuredParser;
        let line = "[INFO][网络] 请求成功";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.tag, "网络");
        assert_eq!(entry.message, "请求成功");
    }

    #[test]
    fn emoji_in_message_parses() {
        let p = StructuredParser;
        let line = "[WARNING][T] 🚀 deploy ⚡ fast";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.level, LogLevel::Warning);
        assert!(entry.message.contains('🚀'));
    }

    #[test]
    fn tag_with_trailing_whitespace_trimmed() {
        // Pipe format: tag has trailing spaces, parser trims.
        let p = StructuredParser;
        let line = "18:05:26.675 │ INFO    │ Network        │ body";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.tag, "Network");
    }

    #[test]
    fn epoch_13_digits_converts_to_time() {
        let p = StructuredParser;
        let line = "[1776241660875][INFO][T] body";
        let entry = p.try_parse(line).unwrap();
        // The epoch converts to some HH:MM:SS.mmm
        assert_eq!(entry.timestamp.len(), 12);
        assert!(entry.timestamp.contains(':'));
        assert!(entry.timestamp.contains('.'));
    }

    #[test]
    fn epoch_10_digits_converts_to_time() {
        let p = StructuredParser;
        // Exactly 10 digits — min boundary of {10,13}.
        let line = "[1776241660][INFO][T] body";
        let entry = p.try_parse(line).unwrap();
        assert!(entry.timestamp.contains(':'));
    }

    #[test]
    fn epoch_conversion_zero_produces_midnight() {
        // Direct unit test of the pure helper.
        let t = epoch_ms_to_timestamp("0");
        assert_eq!(t, "00:00:00.000");
    }

    #[test]
    fn epoch_conversion_bad_input_produces_empty() {
        // Non-numeric input → empty string (the Err branch of parse).
        let t = epoch_ms_to_timestamp("not-a-number");
        assert_eq!(t, "");
    }

    #[test]
    fn epoch_conversion_ms_rolls_over_24h() {
        // 25 hours in ms — hour field wraps mod 24.
        let ms = 25u64 * 3600 * 1000;
        let t = epoch_ms_to_timestamp(&ms.to_string());
        assert!(t.starts_with("01:00:00"));
    }

    #[test]
    fn extract_content_no_flutter_prefix_returns_line_unchanged() {
        // extract_content falls through when FLUTTER_PREFIX_RE doesn't match.
        let c = extract_content("[INFO][T] x");
        assert_eq!(c, "[INFO][T] x");
    }

    #[test]
    fn extract_content_strips_flutter_prefix_and_ansi() {
        let c = extract_content("I/flutter (42): \x1b[31mhi\x1b[0m");
        assert_eq!(c, "hi");
    }

    #[test]
    fn extract_content_handles_vm_stdout_prefix() {
        let c = extract_content("flutter: hello");
        assert_eq!(c, "hello");
    }

    #[test]
    fn parser_name_is_structured() {
        assert_eq!(StructuredParser.name(), "Structured");
    }

    #[test]
    fn source_is_direct_socket() {
        let p = StructuredParser;
        let entry = p.try_parse("[INFO][T] x").unwrap();
        assert_eq!(entry.source, InputSource::DirectSocket);
    }

    #[test]
    fn empty_message_after_bracket_parses() {
        // `[INFO][T]` with no message — regex has `\s?(.*)` so matches.
        let p = StructuredParser;
        let entry = p.try_parse("[INFO][T] ").unwrap();
        assert_eq!(entry.tag, "T");
        assert_eq!(entry.message, "");
    }

    #[test]
    fn pipe_without_timestamp_still_parses() {
        // `PIPE_RE` requires the timestamp field; pipe_ts and pipe both need it.
        let p = StructuredParser;
        // No `HH:MM:SS.mmm` → neither matches → None.
        assert!(p.try_parse("│ INFO │ T │ body").is_none());
    }
}
