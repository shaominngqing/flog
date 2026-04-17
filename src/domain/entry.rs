//! Core data types shared across all layers.

/// Log severity level. System is lowest — raw `print()` output with no level tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LogLevel {
    System = 0,
    Verbose = 1,
    Debug = 2,
    Info = 3,
    Warning = 4,
    Error = 5,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Verbose => "VERBOSE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warning => "WARNING",
            Self::Error => "ERROR",
            Self::System => "SYSTEM",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "VERBOSE" | "V" => Some(Self::Verbose),
            "DEBUG" | "D" => Some(Self::Debug),
            "INFO" | "I" => Some(Self::Info),
            "WARNING" | "WARN" | "W" => Some(Self::Warning),
            "ERROR" | "SEVERE" | "E" => Some(Self::Error),
            _ => None,
        }
    }
}

/// Where the log entry came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputSource {
    DirectSocket,
}

/// A single parsed log entry.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: LogLevel,
    pub tag: String,
    pub message: String,
    pub extra_lines: Vec<String>,
    pub repeat_count: usize,
    pub source: InputSource,
    pub error: Option<String>,
    pub stacktrace: Option<String>,
}

impl LogEntry {
    /// Create a minimal entry with defaults.
    pub fn new(level: LogLevel, tag: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            timestamp: String::new(),
            level,
            tag: tag.into(),
            message: message.into(),
            extra_lines: Vec::new(),
            repeat_count: 1,
            source: InputSource::DirectSocket,
            error: None,
            stacktrace: None,
        }
    }

    /// Complete message including continuation lines.
    pub fn full_message(&self) -> String {
        if self.extra_lines.is_empty() {
            self.message.clone()
        } else {
            let mut s = self.message.clone();
            for line in &self.extra_lines {
                s.push('\n');
                s.push_str(line);
            }
            s
        }
    }
}

/// Result of parsing a single raw line.
pub enum ParseResult {
    /// A new log entry was recognized.
    NewEntry(LogEntry),
    /// A continuation line belonging to the previous entry.
    Continuation(String),
    /// The line was not recognized / should be ignored.
    Ignored,
}

/// Extract the function+location signature from a Dart stack frame line.
/// Input like `#0      Foo._emit (package:app/foo.dart:25:3)` → `Foo._emit (package:app/foo.dart:25:3)`
/// Returns None for non-frame lines.
fn frame_signature(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('#') {
        return None;
    }
    let after_hash = &trimmed[1..];
    let after_num = after_hash.trim_start_matches(|c: char| c.is_ascii_digit());
    if after_num.is_empty() || !after_num.starts_with(char::is_whitespace) {
        return None;
    }
    Some(after_num.trim_start())
}

/// Collapse consecutive identical stack frames into `{signature} × N` lines.
/// Non-frame lines pass through unchanged.
pub fn collapse_stack_frames(stacktrace: &str) -> Vec<String> {
    let lines: Vec<&str> = stacktrace.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }

    let mut result: Vec<String> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let sig = frame_signature(lines[i]);
        if let Some(current_sig) = sig {
            let start = i;
            i += 1;
            while i < lines.len() {
                if let Some(next_sig) = frame_signature(lines[i]) {
                    if next_sig == current_sig {
                        i += 1;
                        continue;
                    }
                }
                break;
            }
            let count = i - start;
            if count == 1 {
                result.push(lines[start].to_string());
            } else {
                result.push(format!("        {} × {}", current_sig, count));
            }
        } else {
            result.push(lines[i].to_string());
            i += 1;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapse_repeated_frames() {
        let input = "\
#0      Foo._emit (package:app/foo.dart:25:3)
#1      Foo._emit (package:app/foo.dart:27:5)
#2      Foo._emit (package:app/foo.dart:27:5)
#3      Foo._emit (package:app/foo.dart:27:5)
#4      Bar.run (package:app/bar.dart:10:7)";

        let result = collapse_stack_frames(input);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], "#0      Foo._emit (package:app/foo.dart:25:3)");
        assert!(result[1].contains("× 3"));
        assert!(result[1].contains("Foo._emit"));
        assert_eq!(result[2], "#4      Bar.run (package:app/bar.dart:10:7)");
    }

    #[test]
    fn collapse_no_repeats() {
        let input = "\
#0      Foo.a (package:app/foo.dart:1:1)
#1      Bar.b (package:app/bar.dart:2:2)
#2      Baz.c (package:app/baz.dart:3:3)";

        let result = collapse_stack_frames(input);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], "#0      Foo.a (package:app/foo.dart:1:1)");
    }

    #[test]
    fn collapse_preserves_non_frame_lines() {
        let input = "\
Error: Stack Overflow
#0      Foo._emit (package:app/foo.dart:25:3)
#1      Foo._emit (package:app/foo.dart:27:5)
#2      Foo._emit (package:app/foo.dart:27:5)";

        let result = collapse_stack_frames(input);
        assert_eq!(result[0], "Error: Stack Overflow");
        assert_eq!(result[1], "#0      Foo._emit (package:app/foo.dart:25:3)");
        assert!(result[2].contains("× 2"));
    }

    #[test]
    fn collapse_empty_input() {
        let result = collapse_stack_frames("");
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn collapse_single_frame() {
        let input = "#0      Foo.bar (package:app/foo.dart:1:1)";
        let result = collapse_stack_frames(input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], input);
    }
}
