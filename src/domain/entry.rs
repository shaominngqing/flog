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
        match s.trim() {
            "VERBOSE" | "V" => Some(Self::Verbose),
            "DEBUG" | "D" => Some(Self::Debug),
            "INFO" | "I" => Some(Self::Info),
            "WARNING" | "WARN" | "W" => Some(Self::Warning),
            "ERROR" | "SEVERE" | "E" => Some(Self::Error),
            _ => None,
        }
    }

    /// Convert a VM Service level integer (0-2000) to LogLevel.
    pub fn from_vm_service_level(level: i64) -> Self {
        match level {
            0..=499 => Self::Verbose,
            500..=799 => Self::Debug,
            800..=899 => Self::Info,
            900..=999 => Self::Warning,
            _ => Self::Error,
        }
    }
}

/// Where the log entry came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputSource {
    Adb,
    VmService,
    Stdin,
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
            source: InputSource::Adb,
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
