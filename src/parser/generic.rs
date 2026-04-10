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

/// `[LEVEL] [Tag] message` or `[LEVEL] message`
static BRACKET_LEVEL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\[(\w+)\]\s*(?:\[(\w+)\]\s*)?(.+)$").unwrap()
});

/// Flutter framework exception header
static EXCEPTION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"══+╡\s*EXCEPTION\s*╞══+").unwrap()
});

/// Dart stacktrace frame: `#N  ClassName.method (package:...)`
static STACKTRACE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^#\d+\s+").unwrap()
});

/// `I/flutter (PID): content` or `flutter: content` — main Flutter output
static FLUTTER_PLAIN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:I/flutter\s*\(\s*\d+\)|flutter):\s?(.*)$").unwrap());

/// Any logcat line with tag/priority: `X/Tag(PID): content`
/// Captures: priority(1), tag(2), content(3)
static LOGCAT_LINE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([VDIWEF])/(\S+)\s*\(\s*\d+\):\s?(.*)$").unwrap());

static ANSI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1b\[[0-9;]*m").unwrap());

pub struct GenericParser;

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
                source: InputSource::Adb,
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
                    source: InputSource::Adb,
                    error: None,
                    stacktrace: None,
                });
            }
        }

        // Path 1: `I/flutter (PID): content` — main Flutter output
        if let Some(caps) = FLUTTER_PLAIN_RE.captures(line) {
            let raw = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let content = ANSI_RE.replace_all(raw, "").to_string();

            // Try [LEVEL] [Tag] message inside flutter content
            if let Some(bcaps) = BRACKET_LEVEL_RE.captures(&content) {
                let level_str = bcaps.get(1)?.as_str();
                if let Some(level) = LogLevel::from_str(level_str) {
                    let tag = bcaps.get(2).map(|m| m.as_str().to_string()).unwrap_or("App".into());
                    let message = bcaps.get(3)?.as_str().to_string();
                    return Some(LogEntry {
                        timestamp: String::new(), level, tag, message,
                        extra_lines: Vec::new(), repeat_count: 1,
                        source: InputSource::Adb, error: None, stacktrace: None,
                    });
                }
            }

            // Plain flutter content (including empty lines from print(''))
            return Some(LogEntry {
                timestamp: String::new(),
                level: LogLevel::System,
                tag: "flutter".to_string(),
                message: content,
                extra_lines: Vec::new(),
                repeat_count: 1,
                source: InputSource::Adb,
                error: None,
                stacktrace: None,
            });
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
                    timestamp: String::new(), level, tag, message: content,
                    extra_lines: Vec::new(), repeat_count: 1,
                    source: InputSource::Adb, error: None, stacktrace: None,
                });
            }
        }

        None
    }

    fn try_continuation(&self, line: &str) -> Option<String> {
        let caps = FLUTTER_PLAIN_RE.captures(line)?;
        let raw = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let content = ANSI_RE.replace_all(raw, "").to_string();

        // Indented content that doesn't match [LEVEL] format → continuation
        if content.starts_with("  ") && !BRACKET_LEVEL_RE.is_match(content.trim_start()) {
            return Some(content);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bracket_level_tag() {
        let p = GenericParser;
        let line = "I/flutter (1234): [INFO] [Network] GET /api/users";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.tag, "Network");
        assert!(entry.message.contains("GET"));
    }

    #[test]
    fn parse_bracket_level_only() {
        let p = GenericParser;
        let line = "I/flutter (1234): [ERROR] Something went wrong";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.level, LogLevel::Error);
        assert_eq!(entry.tag, "App");
    }

    #[test]
    fn parse_plain_flutter() {
        let p = GenericParser;
        let line = "I/flutter (1234): Hello world";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.level, LogLevel::System);
        assert_eq!(entry.message, "Hello world");
    }

    #[test]
    fn parse_empty_flutter_print() {
        let p = GenericParser;
        let line = "I/flutter (1234): ";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.level, LogLevel::System);
        assert_eq!(entry.tag, "flutter");
    }

    #[test]
    fn parse_flutter_jni_warning() {
        let p = GenericParser;
        let line = "W/FlutterJNI(1234): some engine warning";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.level, LogLevel::Warning);
        assert_eq!(entry.tag, "FlutterJNI");
    }

    #[test]
    fn parse_dart_vm_error() {
        let p = GenericParser;
        let line = "E/DartVM  (1234): Unhandled exception";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.level, LogLevel::Error);
        assert_eq!(entry.tag, "DartVM");
    }

    #[test]
    fn parses_any_logcat_tag() {
        // GenericParser now accepts any logcat format — ADB filter handles noise
        let p = GenericParser;
        let entry = p.try_parse("I/System.out(1234): some output").unwrap();
        assert_eq!(entry.tag, "System.out");
        assert_eq!(entry.level, LogLevel::Info);
    }

    // ── Continuation tests ──

    #[test]
    fn continuation_indented_query() {
        let p = GenericParser;
        let line = "I/flutter (20294):   query: {_productId: 66000001}";
        let cont = p.try_continuation(line).unwrap();
        assert!(cont.contains("query"));
    }

    #[test]
    fn continuation_indented_body() {
        let p = GenericParser;
        let line = "I/flutter (20294):   body:  {messages: [{role: user}]}";
        let cont = p.try_continuation(line).unwrap();
        assert!(cont.contains("body"));
    }

    #[test]
    fn no_continuation_for_bracket_level() {
        let p = GenericParser;
        // [INFO][Clog] lines should NOT be treated as continuation
        let line = "I/flutter (20294): [INFO][Clog] /network/request {status: 200}";
        assert!(p.try_continuation(line).is_none());
    }

    #[test]
    fn no_continuation_for_non_indented() {
        let p = GenericParser;
        let line = "I/flutter (20294): some plain message";
        assert!(p.try_continuation(line).is_none());
    }

    #[test]
    fn continuation_vm_service_indented() {
        let p = GenericParser;
        let line = "flutter:   query: {version: 1.0.0}";
        let cont = p.try_continuation(line).unwrap();
        assert!(cont.contains("query"));
    }

    // VM Service stdout format tests (flutter: prefix instead of I/flutter (PID):)
    #[test]
    fn parse_vm_stdout_bracket_level_tag() {
        let p = GenericParser;
        let line = "flutter: [INFO][Network] GET /aura-lang-be/api/user-courses";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.tag, "Network");
        assert!(entry.message.contains("GET"));
    }

    #[test]
    fn parse_vm_stdout_error() {
        let p = GenericParser;
        let line = "flutter: [ERROR][Network] x 404 /aura-lang-be/api/episodes/0 (521ms)";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.level, LogLevel::Error);
        assert_eq!(entry.tag, "Network");
    }

    #[test]
    fn parse_vm_stdout_plain() {
        let p = GenericParser;
        let line = "flutter: some plain message";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.level, LogLevel::System);
        assert_eq!(entry.tag, "flutter");
        assert_eq!(entry.message, "some plain message");
    }
}
