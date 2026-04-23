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

static ANSI_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\x1b\[[0-9;]*m").unwrap());

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

        // Path 1: `I/flutter (PID): content` — main Flutter output
        if let Some(caps) = FLUTTER_PLAIN_RE.captures(line) {
            let raw = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let content = ANSI_RE.replace_all(raw, "").to_string();

            // Try [LEVEL] [Tag] message inside flutter content
            if let Some(bcaps) = BRACKET_LEVEL_RE.captures(&content) {
                let level_str = bcaps.get(1)?.as_str();
                if let Some(level) = LogLevel::from_str(level_str) {
                    let tag = bcaps
                        .get(2)
                        .map(|m| m.as_str().to_string())
                        .unwrap_or("App".into());
                    let message = bcaps.get(3)?.as_str().to_string();
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
            }

            // Plain flutter content (including empty lines from print(''))
            return Some(LogEntry {
                timestamp: String::new(),
                level: LogLevel::System,
                tag: "flutter".to_string(),
                message: content,
                extra_lines: Vec::new(),
                repeat_count: 1,
                source: InputSource::DirectSocket,
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

    #[test]
    fn parser_name_is_generic() {
        let p = GenericParser;
        assert_eq!(p.name(), "Generic");
    }

    #[test]
    fn parse_exception_header() {
        let p = GenericParser;
        let line = "════════╡ EXCEPTION CAUGHT BY WIDGETS LIBRARY ╞════════════";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.level, LogLevel::Error);
        assert_eq!(entry.tag, "Flutter");
        assert!(entry.message.contains("EXCEPTION"));
    }

    #[test]
    fn parse_exception_decoration_line() {
        let p = GenericParser;
        let line = "═══════════════════════════════════════════════════════════";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.level, LogLevel::Error);
        assert_eq!(entry.tag, "Flutter");
    }

    #[test]
    fn parse_handler_line() {
        let p = GenericParser;
        let entry = p.try_parse("Handler: onTap").unwrap();
        assert_eq!(entry.level, LogLevel::Error);
        assert_eq!(entry.tag, "Flutter");
    }

    #[test]
    fn parse_recognizer_line() {
        let p = GenericParser;
        let entry = p.try_parse("Recognizer: TapGestureRecognizer").unwrap();
        assert_eq!(entry.level, LogLevel::Error);
    }

    #[test]
    fn parse_the_following_line() {
        let p = GenericParser;
        let entry = p.try_parse("The following assertion was thrown").unwrap();
        assert_eq!(entry.level, LogLevel::Error);
    }

    #[test]
    fn parse_when_the_exception_line() {
        let p = GenericParser;
        let entry = p
            .try_parse("When the exception was thrown, this was the stack:")
            .unwrap();
        assert_eq!(entry.level, LogLevel::Error);
    }

    #[test]
    fn parse_failed_assertion_line() {
        let p = GenericParser;
        let entry = p
            .try_parse("Failed assertion: line 42: 'foo != null'")
            .unwrap();
        assert_eq!(entry.level, LogLevel::Error);
    }

    #[test]
    fn parse_stacktrace_frame() {
        let p = GenericParser;
        let entry = p
            .try_parse("#0      MyWidget.build (package:my_app/widget.dart:10:5)")
            .unwrap();
        assert_eq!(entry.level, LogLevel::Error);
        assert_eq!(entry.tag, "Flutter");
    }

    #[test]
    fn parse_verbose_logcat() {
        let p = GenericParser;
        let entry = p.try_parse("V/MyTag  (1234): verbose message").unwrap();
        assert_eq!(entry.level, LogLevel::Verbose);
        assert_eq!(entry.tag, "MyTag");
    }

    #[test]
    fn parse_debug_logcat() {
        let p = GenericParser;
        let entry = p.try_parse("D/MyTag  (1234): debug message").unwrap();
        assert_eq!(entry.level, LogLevel::Debug);
        assert_eq!(entry.tag, "MyTag");
    }

    #[test]
    fn parse_fatal_logcat() {
        let p = GenericParser;
        let entry = p.try_parse("F/MyTag  (1234): fatal crash").unwrap();
        assert_eq!(entry.level, LogLevel::Error);
        assert_eq!(entry.tag, "MyTag");
    }

    #[test]
    fn ansi_escape_is_stripped_from_exception() {
        // ANSI color codes should be stripped before EXCEPTION detection
        let p = GenericParser;
        let line = "\x1b[31m════════╡ EXCEPTION CAUGHT ╞════════\x1b[0m";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.level, LogLevel::Error);
        // The stripped message should not contain raw ANSI codes
        assert!(!entry.message.contains("\x1b["));
    }

    #[test]
    fn ansi_escape_stripped_in_flutter_content() {
        let p = GenericParser;
        let line = "I/flutter (1234): \x1b[32mhello\x1b[0m";
        let entry = p.try_parse(line).unwrap();
        assert!(!entry.message.contains("\x1b["));
        assert!(entry.message.contains("hello"));
    }

    #[test]
    fn unmatched_line_returns_none() {
        let p = GenericParser;
        assert!(p.try_parse("totally random text with no pattern").is_none());
        assert!(p.try_parse("").is_none());
    }

    #[test]
    fn bracket_level_unknown_falls_back_to_plain_flutter() {
        // `[NOTALEVEL] ...` — bracket regex matches but LogLevel::from_str returns None;
        // falls through to plain flutter content path.
        let p = GenericParser;
        let line = "I/flutter (1234): [NOTALEVEL] hi";
        let entry = p.try_parse(line).unwrap();
        assert_eq!(entry.level, LogLevel::System);
        assert_eq!(entry.tag, "flutter");
    }
}
