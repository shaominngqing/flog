//! Generic log format parser.
//!
//! Recognizes common patterns found in Flutter/Dart logging:
//!   - `[LEVEL] [Tag] message`
//!   - `[LEVEL] message`
//!   - Flutter framework exception blocks (`══╡ EXCEPTION ╞══`)
//!   - Plain `I/flutter (PID): message` (no structured `[LEVEL][Tag]` formatting)

use regex::Regex;
use std::sync::LazyLock;

use super::LogLineParser;
use crate::domain::{InputSource, LogEntry, LogLevel};

// LazyLock regex compilation is deliberate — compiles on first use, O(1)
// thereafter. Audit DOM-014 reviewed and approved. Do not replace with
// runtime-rebuilt regex without profiling first.

/// `[LEVEL] [Tag] message` or `[LEVEL] message`
static BRACKET_LEVEL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[(\w+)\]\s*(?:\[(\w+)\]\s*)?(.+)$").unwrap());

/// Flutter framework exception header
static EXCEPTION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"══+╡\s*EXCEPTION\s*╞══+").unwrap());

/// Dart stacktrace frame: `#N  ClassName.method (package:...)`
static STACKTRACE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^#\d+\s+").unwrap());

/// `I/flutter (PID): content` or `flutter: content` — main Flutter output
static FLUTTER_PLAIN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:I/flutter\s*\(\s*\d+\)|flutter):\s?(.*)$").unwrap());

/// Any logcat line with tag/priority: `X/Tag(PID): content`
/// Captures: priority(1), tag(2), content(3)
static LOGCAT_LINE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([VDIWEF])/(\S+)\s*\(\s*\d+\):\s?(.*)$").unwrap());

// ANSI stripping lives in parser::util — Phase 3 DOM-015.
use super::util::ANSI_RE;

pub struct GenericParser;

impl GenericParser {
    /// Try to parse a `I/flutter (pid): ...` or `flutter: ...` style line.
    ///
    /// Returns `Some` when the flutter prefix matches. The embedded
    /// content may itself be `[LEVEL][Tag] msg` shaped — in that case,
    /// the level and tag are lifted out; otherwise the whole content
    /// is stored as a System-level message tagged `flutter`.
    ///
    /// Phase 3 Step 3.1 — see Audit DOM-016.
    //
    // WHY this order (prefixed → structured, not the reverse):
    // `I/flutter (PID): [WARN][Net] slow` is ALSO a valid [LEVEL][Tag]
    // pattern if you only look at the embedded content, so we must
    // first strip the `I/flutter (PID):` wrapper; otherwise
    // try_parse_flutter_structured would match on the raw logcat line
    // and mis-tag the whole thing as tag=`Net`, losing the flutter
    // wrapper signal entirely.
    fn try_parse_flutter_prefixed(line: &str) -> Option<LogEntry> {
        let caps = FLUTTER_PLAIN_RE.captures(line)?;
        let raw = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let content = ANSI_RE.replace_all(raw, "").to_string();

        // Try structured [LEVEL][Tag] lift first; fall through to plain
        // if the level string is unrecognized or the pattern doesn't
        // match.
        Some(
            Self::try_parse_flutter_structured(&content)
                .unwrap_or_else(|| Self::build_flutter_plain(content)),
        )
    }

    /// When flutter content starts with `[LEVEL][Tag]` AND the level is
    /// recognized by `LogLevel::from_str`, build a lifted `LogEntry`.
    /// Returns `None` if either the bracket shape or the level string
    /// doesn't match — caller falls back to `build_flutter_plain`.
    ///
    /// Phase 3 Step 3.1 — see Audit DOM-016.
    fn try_parse_flutter_structured(content: &str) -> Option<LogEntry> {
        let bcaps = BRACKET_LEVEL_RE.captures(content)?;
        let level_str = bcaps.get(1)?.as_str();
        let level = LogLevel::from_str(level_str)?;
        let tag = bcaps
            .get(2)
            .map(|m| m.as_str().to_string())
            .unwrap_or("App".into());
        let message = bcaps.get(3)?.as_str().to_string();
        Some(LogEntry {
            timestamp: String::new(),
            level,
            tag,
            message,
            extra_lines: Vec::new(),
            repeat_count: 1,
            source: InputSource::DirectSocket,
            error: None,
            stacktrace: None,
        })
    }

    /// Flutter content that didn't match `[LEVEL][Tag]` (or had an
    /// unrecognized level) — treat as a System message tagged `flutter`,
    /// including empty lines from `print('')`.
    ///
    /// Phase 3 Step 3.1 — see Audit DOM-016.
    fn build_flutter_plain(content: String) -> LogEntry {
        LogEntry {
            timestamp: String::new(),
            level: LogLevel::System,
            tag: "flutter".to_string(),
            message: content,
            extra_lines: Vec::new(),
            repeat_count: 1,
            source: InputSource::DirectSocket,
            error: None,
            stacktrace: None,
        }
    }
}

impl LogLineParser for GenericParser {
    fn name(&self) -> &'static str {
        "Generic"
    }

    fn try_parse(&self, line: &str) -> Option<LogEntry> {
        let stripped = ANSI_RE.replace_all(line, "");
        let clean = stripped.as_ref();

        // Flutter exception block (works with or without flutter: prefix)
        if EXCEPTION_RE.is_match(clean) {
            return Some(LogEntry {
                timestamp: String::new(),
                level: LogLevel::Error,
                tag: "Flutter".to_string(),
                message: clean.to_string(),
                extra_lines: Vec::new(),
                repeat_count: 1,
                source: InputSource::DirectSocket,
                error: None,
                stacktrace: None,
            });
        }

        // Flutter exception related lines: decoration, stacktrace, assertion messages
        {
            let trimmed = clean.trim_start();
            if trimmed.starts_with('═')
                || trimmed.starts_with("Handler:")
                || trimmed.starts_with("Recognizer:")
                || trimmed.starts_with("The following")
                || trimmed.starts_with("When the exception")
                || trimmed.starts_with("Failed assertion:")
                || STACKTRACE_RE.is_match(trimmed)
            {
                return Some(LogEntry {
                    timestamp: String::new(),
                    level: LogLevel::Error,
                    tag: "Flutter".to_string(),
                    message: clean.to_string(),
                    extra_lines: Vec::new(),
                    repeat_count: 1,
                    source: InputSource::DirectSocket,
                    error: None,
                    stacktrace: None,
                });
            }
        }

        // Path 1: `I/flutter (PID): content` — main Flutter output.
        // Phase 3 Step 3.1 — see Audit DOM-016. Extracted into
        // try_parse_flutter_prefixed + helpers.
        if let Some(entry) = Self::try_parse_flutter_prefixed(line) {
            return Some(entry);
        }

        // Path 2: Other logcat tags — `X/Tag(PID): content`
        // Catches FlutterJNI, Flutter, DartVM, System.out etc.
        if let Some(caps) = LOGCAT_LINE_RE.captures(line) {
            let priority = caps.get(1)?.as_str();
            let tag = caps.get(2)?.as_str().to_string();
            let content = caps.get(3).map(|m| m.as_str()).unwrap_or("").to_string();

            let level = match priority {
                "V" => LogLevel::Verbose,
                "D" => LogLevel::Debug,
                "I" => LogLevel::Info,
                "W" => LogLevel::Warning,
                "E" | "F" => LogLevel::Error,
                _ => LogLevel::Debug,
            };

            if !content.trim().is_empty() || level >= LogLevel::Warning {
                return Some(LogEntry {
                    timestamp: String::new(),
                    level,
                    tag,
                    message: content,
                    extra_lines: Vec::new(),
                    repeat_count: 1,
                    source: InputSource::DirectSocket,
                    error: None,
                    stacktrace: None,
                });
            }
        }

        None
    }
}

#[cfg(test)]
#[path = "generic_tests.rs"]
mod tests;
