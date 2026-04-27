//! Direct Socket protocol — message types for flog ↔ flog_dart communication.

use serde::{Deserialize, Serialize};

/// Unique identifier for a connected client.
pub type ClientId = u64;

/// Information about a connected flog_dart client, extracted from Hello message.
///
/// `session_id` is populated from the optional `sessionId` field of the
/// Hello frame (TRANS-014) — older flog_dart clients don't send it, so
/// the field is `None` in that case. Future work can use this to detect
/// app restarts and replay buffered state without breaking the wire
/// format: the field is additive and defaults cleanly.
#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub id: ClientId,
    pub app: String,
    pub app_version: String,
    pub os: String,
    pub package_name: String,
    pub port: u16,
    pub build_mode: String,
    pub connected_at: std::time::Instant,
    pub session_id: Option<String>,
}

/// Messages from Dart client → flog server (upstream).
// Phase 3 DOM-002/006 — the Net variant embeds the typed FlogNetKind
// enum via #[serde(flatten)]. ClientMessage is internally-tagged on
// "type"; FlogNetKind is internally-tagged on "t" — different tag keys
// so the two layers compose cleanly.
//
// TRANS-012 (A-class ack): every live match against `ClientMessage` —
// `dispatch_client_message` in main.rs and the Hello handshake in
// connector.rs — is exhaustive (no `_` wildcard). Adding a new variant
// is therefore a compile-time change that surfaces at every handler.
// The only `_ => ...` arms that look like catch-alls are deliberate
// "expected Hello, got something else" error-reporting branches which
// continue to work if ClientMessage grows new variants (they'd fall
// through the explicit Log/Net checks into an "unrecognized" bucket).
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    #[serde(rename = "hello")]
    Hello {
        #[serde(default)]
        device: Option<String>,
        app: String,
        #[serde(default)]
        #[serde(rename = "appVersion")]
        app_version: Option<String>,
        os: String,
        #[serde(default)]
        #[serde(rename = "packageName")]
        package_name: Option<String>,
        #[serde(default)]
        port: Option<u16>,
        #[serde(default)]
        #[serde(rename = "buildMode")]
        build_mode: Option<String>,
        /// TRANS-014: optional session id, used to correlate reconnects
        /// with a prior session (e.g. after a hot-restart). Additive: older
        /// Dart clients omit it and we default to `None`.
        #[serde(default)]
        #[serde(rename = "sessionId")]
        session_id: Option<String>,
    },
    #[serde(rename = "log")]
    Log {
        #[serde(default)]
        level: Option<String>,
        #[serde(default)]
        tag: Option<String>,
        message: String,
        #[serde(default)]
        error: Option<String>,
        #[serde(rename = "stackTrace")]
        #[serde(default)]
        stack_trace: Option<String>,
        #[serde(default)]
        timestamp: Option<u64>,
    },
    #[serde(rename = "net")]
    Net {
        #[serde(flatten)]
        msg: crate::domain::network::FlogNetKind,
    },
}

/// Messages from flog server → Dart client (downstream).
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    #[serde(rename = "mock_sync")]
    MockSync { rules: String },
    #[serde(rename = "replay")]
    Replay {
        method: String,
        url: String,
        headers: Option<String>,
        body: Option<String>,
    },
    /// Request Dart to replay its entire message buffer.
    ///
    /// Sent when the TUI switches to a different app's session. Dart responds
    /// by iterating its FlogStore buffer and re-sending all stored messages.
    #[serde(rename = "subscribe")]
    Subscribe {},
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
