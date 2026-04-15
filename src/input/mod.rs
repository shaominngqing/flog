//! Input layer — Direct Socket connector for flog_dart communication.

pub mod protocol;
pub mod connector;

pub use protocol::{ClientId, ClientInfo, ClientMessage, ServerMessage};
pub use connector::{ConnectorEvent, ConnectorHandle, connect};
