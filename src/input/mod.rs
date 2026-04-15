//! Input layer — Direct Socket server for flog_dart communication.

pub mod protocol;
pub mod server;

pub use protocol::{ClientId, ClientInfo, ClientMessage, ServerMessage};
pub use server::{FlogServer, ServerEvent, ServerHandle};
