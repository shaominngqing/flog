//! Keyword-inference parser (catch-all).
//!
//! Infers log level from keywords in the message text.
//! This is the last parser in the chain — it matches anything from stdin/pipe.

use regex::Regex;
use std::sync::LazyLock;

use super::LogLineParser;
use crate::domain::{InputSource, LogEntry, LogLevel};

// LazyLock regex compilation is deliberate — compiles on first use, O(1)
// thereafter. Audit DOM-014 reviewed and approved. Do not replace with
// runtime-rebuilt regex without profiling first.

/// Keyword sets used by the fallback parser to infer a log level from
/// free-form text. Extracted from inline regex — Phase 3 Step 3.1, see
/// Audit DOM-017. Single source of truth for "what counts as an ERROR
/// keyword", addable without regex-pattern surgery.
///
/// NOTE: the original regex used `fail(ed|ure)?` — captured "fail",
/// "failed", "failure". We spell them out here so future readers don't
/// need to mentally expand regex groups.
pub(crate) const ERROR_KEYWORDS: &[&str] = &[
    "error",
    "exception",
    "fatal",
    "crash",
    "panic",
    "fail",
    "failed",
    "failure",
];

/// Original regex used `warn(ing)?` — captured "warn", "warning".
pub(crate) const WARNING_KEYWORDS: &[&str] = &["warn", "warning", "deprecated", "caution"];

pub(crate) const DEBUG_KEYWORDS: &[&str] = &["debug", "trace", "verbose"];

/// Build a case-insensitive word-boundary regex from a keyword list.
///
/// Phase 3 Step 3.1 — see Audit DOM-017.
fn build_keyword_regex(keywords: &[&str]) -> Regex {
    let pattern = format!(r"(?i)\b({})\b", keywords.join("|"));
    Regex::new(&pattern).expect("keyword regex compiles")
}

static ERROR_RE: LazyLock<Regex> = LazyLock::new(|| build_keyword_regex(ERROR_KEYWORDS));
static WARNING_RE: LazyLock<Regex> = LazyLock::new(|| build_keyword_regex(WARNING_KEYWORDS));
static DEBUG_RE: LazyLock<Regex> = LazyLock::new(|| build_keyword_regex(DEBUG_KEYWORDS));

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

    // ---- Phase 3 Step 3.1 DOM-017: keyword set extraction ----

    #[test]
    fn error_keywords_include_expected_set() {
        // Lock current ERROR_KEYWORDS membership. Phase 3+ changes must
        // update this test deliberately.
        assert!(ERROR_KEYWORDS.contains(&"error"));
        assert!(ERROR_KEYWORDS.contains(&"exception"));
        assert!(ERROR_KEYWORDS.contains(&"fatal"));
        assert!(ERROR_KEYWORDS.contains(&"crash"));
        assert!(ERROR_KEYWORDS.contains(&"panic"));
        assert!(ERROR_KEYWORDS.contains(&"fail"));
        assert!(ERROR_KEYWORDS.contains(&"failed"));
        assert!(ERROR_KEYWORDS.contains(&"failure"));
    }

    #[test]
    fn warning_keywords_include_expected_set() {
        assert!(WARNING_KEYWORDS.contains(&"warn"));
        assert!(WARNING_KEYWORDS.contains(&"warning"));
        assert!(WARNING_KEYWORDS.contains(&"deprecated"));
        assert!(WARNING_KEYWORDS.contains(&"caution"));
    }

    #[test]
    fn debug_keywords_include_expected_set() {
        assert!(DEBUG_KEYWORDS.contains(&"debug"));
        assert!(DEBUG_KEYWORDS.contains(&"trace"));
        assert!(DEBUG_KEYWORDS.contains(&"verbose"));
    }

    #[test]
    fn build_keyword_regex_is_case_insensitive() {
        let re = build_keyword_regex(&["foo", "bar"]);
        assert!(re.is_match("FOO"));
        assert!(re.is_match("Foo"));
        assert!(re.is_match("bar"));
    }

    #[test]
    fn build_keyword_regex_respects_word_boundary() {
        let re = build_keyword_regex(&["log"]);
        assert!(re.is_match("please log this"));
        // "prologue" contains "log" but not at a word boundary
        assert!(!re.is_match("prologue"));
        // "blog" similarly embeds "log"
        assert!(!re.is_match("blog"));
    }

    #[test]
    fn build_keyword_regex_handles_single_keyword() {
        let re = build_keyword_regex(&["alone"]);
        assert!(re.is_match("i am alone"));
        assert!(!re.is_match("standalone"));
    }
}
