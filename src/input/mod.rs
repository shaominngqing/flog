//! Input layer — Direct Socket connector for flog_dart communication.

pub mod connector;
pub mod protocol;

pub use connector::{connect, connect_stream, ConnectorEvent, ConnectorHandle};
pub use protocol::{ClientId, ClientInfo, ClientMessage, ServerMessage};
