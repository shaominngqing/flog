//! Input source abstraction.

pub mod adb;
pub mod discover;
pub mod stdin_source;
pub mod vm_service;

use crate::domain::LogEntry;

pub enum SourceEvent {
    RawLine(String),
    /// Raw line with a known timestamp (e.g., from VM Service Stdout events).
    RawLineWithTimestamp(String, String),
    ParsedEntry(LogEntry),
}
