//! Domain layer — pure data types and storage, zero UI dependencies.

pub mod entry;
pub mod filter;
pub mod filter_traits;
pub mod json_tolerant;
pub mod mock;
pub mod network;
pub mod network_filter;
pub mod network_store;
pub mod sse_merge;
pub mod store;
pub mod structured_parser;
pub mod ws_chat;

pub use entry::{InputSource, LogEntry, LogLevel, ParseResult};
pub use filter::FilterState;
#[allow(unused_imports)]
pub use filter_traits::FilterVariant;
pub use network_filter::NetworkFilter;
pub use network_store::NetworkStore;
pub use store::LogStore;
