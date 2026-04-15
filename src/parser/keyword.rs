//! Keyword-inference parser (catch-all).
//!
//! Infers log level from keywords in the message text.
//! This is the last parser in the chain — it matches anything from stdin/pipe.

use regex::Regex;
use std::sync::LazyLock;

use super::LogLineParser;
use crate::domain::{InputSource, LogEntry, LogLevel};

static ERROR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(error|exception|fatal|crash|panic|fail(ed|ure)?)\b").unwrap()
});

static WARNING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(warn(ing)?|deprecated|caution)\b").unwrap());

static DEBUG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(debug|trace|verbose)\b").unwrap());

pub struct KeywordParser;

impl KeywordParser {
    fn infer_level(text: &str) -> LogLevel {
        if ERROR_RE.is_match(text) {
            LogLevel::Error
        } else if WARNING_RE.is_match(text) {
            LogLevel::Warning
        } else if DEBUG_RE.is_match(text) {
            LogLevel::Debug
        } else {
            LogLevel::Info
        }
    }
}

impl LogLineParser for KeywordParser {
    fn name(&self) -> &'static str {
        "Keyword"
    }

    fn try_parse(&self, line: &str) -> Option<LogEntry> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        let level = Self::infer_level(trimmed);

        Some(LogEntry {
            timestamp: String::new(),
            level,
            tag: "App".to_string(),
            message: trimmed.to_string(),
            extra_lines: Vec::new(),
            repeat_count: 1,
            source: InputSource::DirectSocket,
            error: None,
            stacktrace: None,
        })
    }

    fn try_continuation(&self, _line: &str) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_error() {
        let p = KeywordParser;
        let entry = p.try_parse("Something failed with an error").unwrap();
        assert_eq!(entry.level, LogLevel::Error);
    }

    #[test]
    fn infers_warning() {
        let p = KeywordParser;
        let entry = p.try_parse("Warning: deprecated API call").unwrap();
        assert_eq!(entry.level, LogLevel::Warning);
    }

    #[test]
    fn infers_info_default() {
        let p = KeywordParser;
        let entry = p.try_parse("Application started on port 8080").unwrap();
        assert_eq!(entry.level, LogLevel::Info);
    }

    #[test]
    fn ignores_empty() {
        let p = KeywordParser;
        assert!(p.try_parse("   ").is_none());
    }
}
