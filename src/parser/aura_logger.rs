//! Parser for the AuraLogger format (flog's own structured output).
//!
//! Recognizes lines like:
//!   `I/flutter (PID): HH:MM:SS.mmm │ LEVEL │ Tag │ message`
//!   `I/flutter (PID):              │       │     │   continuation`

use regex::Regex;
use std::sync::LazyLock;

use super::LogLineParser;
use crate::domain::{InputSource, LogEntry, LogLevel};

static ANSI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1b\[[0-9;]*m").unwrap());

/// Matches ADB logcat format: `I/flutter (PID): content`
/// and VM Service stdout format: `flutter: content`
static FLUTTER_PREFIX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:I/flutter\s*\(\s*\d+\)|flutter):\s?(.*)$").unwrap());

static HEADER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\d{2}:\d{2}:\d{2}\.\d{3})\s*│\s*(\w+)\s*│\s*(\S.*?)\s*│\s?(.*)$").unwrap()
});

static CONT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s+│\s+│\s+│\s*(.*)$").unwrap());

/// Extracts the Flutter payload from a logcat line, stripping ANSI codes.
fn extract_flutter_content(line: &str) -> Option<String> {
    let caps = FLUTTER_PREFIX_RE.captures(line)?;
    let raw = caps.get(1).map(|m| m.as_str()).unwrap_or("");
    Some(ANSI_RE.replace_all(raw, "").to_string())
}

pub struct AuraLoggerParser;

impl LogLineParser for AuraLoggerParser {
    fn name(&self) -> &'static str {
        "AuraLogger"
    }

    fn try_parse(&self, line: &str) -> Option<LogEntry> {
        // Try with flutter prefix first (ADB logcat), then raw line (VM Service stdout)
        let content = extract_flutter_content(line)
            .unwrap_or_else(|| ANSI_RE.replace_all(line, "").to_string());
        let caps = HEADER_RE.captures(&content)?;

        let timestamp = caps.get(1)?.as_str().to_string();
        let level = LogLevel::from_str(caps.get(2)?.as_str()).unwrap_or(LogLevel::Debug);
        let tag = caps.get(3)?.as_str().trim().to_string();
        let message = caps.get(4)?.as_str().to_string();

        Some(LogEntry {
            timestamp,
            level,
            tag,
            message,
            extra_lines: Vec::new(),
            repeat_count: 1,
            source: InputSource::Adb,
            error: None,
            stacktrace: None,
        })
    }

    fn try_continuation(&self, line: &str) -> Option<String> {
        let content = extract_flutter_content(line)
            .unwrap_or_else(|| ANSI_RE.replace_all(line, "").to_string());
        let caps = CONT_RE.captures(&content)?;
        Some(caps.get(1)?.as_str().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_header() {
        let p = AuraLoggerParser;
        let line = "I/flutter (14114): 18:05:26.675 │ INFO    │ Network        │ → GET /api/scene-types";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.timestamp, "18:05:26.675");
        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.tag, "Network");
        assert!(entry.message.contains("GET"));
    }

    #[test]
    fn parse_continuation() {
        let p = AuraLoggerParser;
        let line = "I/flutter (14114):              │         │                │   query: {_productId: 66000001}";
        let cont = p.try_continuation(line).unwrap();
        assert!(cont.contains("query"));
    }

    #[test]
    fn ignores_non_flutter() {
        let p = AuraLoggerParser;
        assert!(p.try_parse("W/1.raster(13383): type=1400").is_none());
    }

    #[test]
    fn strips_ansi() {
        let p = AuraLoggerParser;
        let line = "I/flutter (14114): \x1b[34m18:05:26.675 │ INFO    │ Network        │ test\x1b[0m";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.tag, "Network");
    }
}
