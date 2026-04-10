//! Domain layer — pure data types and storage, zero UI dependencies.

pub mod entry;
pub mod filter;
pub mod network;
pub mod network_filter;
pub mod network_store;
pub mod store;

pub use entry::{InputSource, LogEntry, LogLevel, ParseResult};
pub use filter::FilterState;
pub use network::{
    FlogNetMessage, NetworkEntry, NetworkStatus, Protocol, SseChunk, WsDirection, WsMessage,
};
pub use network_filter::{MethodFilter, NetworkFilter, ProtocolFilter, StatusFilter};
pub use network_store::NetworkStore;
pub use store::LogStore;
