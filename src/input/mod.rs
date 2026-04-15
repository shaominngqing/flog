//! Input layer — Direct Socket server for flog_dart communication.

pub mod protocol;
pub mod server;

// Legacy modules — kept temporarily for compilation, removed in Task 9
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

pub use protocol::{ClientId, ClientInfo, ClientMessage, ServerMessage};
pub use server::{FlogServer, ServerEvent, ServerHandle};
