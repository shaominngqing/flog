//! Domain layer — pure data types and storage, zero UI dependencies.

pub mod entry;
pub mod filter;
pub mod store;

pub use entry::{InputSource, LogEntry, LogLevel, ParseResult};
pub use filter::FilterState;
pub use store::LogStore;
