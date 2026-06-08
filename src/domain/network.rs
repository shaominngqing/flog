//! Network request data types for HTTP, SSE, and WebSocket.

use serde::Deserialize;

use crate::domain::network_timing::{NetworkTiming, TimingEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Http,
    Sse,
    Ws,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkStatus {
    Pending,
    Active,
    Completed,
    Failed,
    /// A response arrived whose id has no matching request entry.
    /// Phase 3 DOM-003 fix: orphan responses are now surfaced instead of
    /// silently dropped, so the user sees data loss in the inspector.
    Orphan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WsDirection {
    Send,
    Recv,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntrySource {
    App,
    Replay,
    Mocked,
}

#[derive(Debug, Clone)]
pub struct SseChunk {
    pub data: String,
    #[allow(dead_code)]
    pub event_timing: Option<TimingEvent>,
}

#[derive(Debug, Clone)]
pub struct WsMessage {
    pub direction: WsDirection,
    pub data: String,
    /// Byte size of this message. Read by the Network detail panel + stats.
    pub size: u64,
    #[allow(dead_code)]
    pub event_timing: Option<TimingEvent>,
}

#[derive(Debug, Clone)]
pub struct NetworkEntry {
    pub id: u64,
    pub protocol: Protocol,
    pub timestamp: String,
    pub method: String,
    pub url: String,
    pub path: String,
    pub status: NetworkStatus,
    pub http_status: Option<u16>,
    pub duration: Option<u64>,
    pub request_size: Option<u64>,
    pub response_size: Option<u64>,
    pub request_headers: Option<String>,
    pub response_headers: Option<String>,
    pub request_body: Option<String>,
    pub response_body: Option<String>,
    pub error: Option<String>,
    pub sse_chunks: Vec<SseChunk>,
    pub sse_total_size: u64,
    pub ws_messages: Vec<WsMessage>,
    pub ws_close_code: Option<u16>,
    pub ws_close_reason: Option<String>,
    pub source: EntrySource,
    #[allow(dead_code)]
    pub timing: Option<NetworkTiming>,
}

impl NetworkEntry {
    /// Start a builder. Phase 3 DOM-024. Prefer this over the
    /// `new_http` / `new_sse` / `new_ws` factories for new call sites.
    pub fn builder(id: u64, url: impl Into<String>) -> NetworkEntryBuilder {
        NetworkEntryBuilder::new(id, url.into())
    }

    pub fn new_http(id: u64, method: String, url: String, timestamp: String) -> Self {
        Self::builder(id, url)
            .http(method)
            .timestamp(timestamp)
            .build()
    }

    pub fn new_sse(id: u64, method: String, url: String, timestamp: String) -> Self {
        Self::builder(id, url)
            .sse(method)
            .timestamp(timestamp)
            .build()
    }

    pub fn new_ws(id: u64, url: String, timestamp: String) -> Self {
        Self::builder(id, url).ws().timestamp(timestamp).build()
    }

    /// Create a placeholder entry for a `Response` whose id has no matching
    /// `Request` — surfaces the orphan in the inspector instead of silently
    /// dropping it. Phase 3 DOM-003.
    pub fn new_orphan_response(
        id: u64,
        status_code: Option<u16>,
        res_body: Option<String>,
        duration: Option<u64>,
    ) -> Self {
        let size = res_body.as_ref().map(|b| b.len() as u64);
        Self {
            id,
            protocol: Protocol::Http,
            timestamp: String::new(),
            method: "?".into(),
            url: "<orphan response>".into(),
            path: "<orphan>".into(),
            status: NetworkStatus::Orphan,
            http_status: status_code,
            duration,
            request_size: None,
            response_size: size,
            request_headers: None,
            response_headers: None,
            request_body: None,
            response_body: res_body,
            error: None,
            sse_chunks: Vec::new(),
            sse_total_size: 0,
            ws_messages: Vec::new(),
            ws_close_code: None,
            ws_close_reason: None,
            source: EntrySource::App,
            timing: None,
        }
    }

    pub fn display_size(&self) -> u64 {
        match self.protocol {
            Protocol::Http => self.response_size.unwrap_or(0),
            Protocol::Sse => self.sse_total_size,
            Protocol::Ws => self.ws_messages.iter().map(|m| m.size).sum(),
        }
    }
}

// DOM-020 acknowledged — naive string search, behaviour correct on all
// known inputs. Locked by characterization tests dom_020_extract_path_*.
// Revisit only if URL parsing requirements change (e.g. pulling in the
// `url` crate for RFC 3986 normalisation).
fn extract_path(url: &str) -> String {
    if let Some(pos) = url.find("://") {
        let after_scheme = &url[pos + 3..];
        if let Some(slash) = after_scheme.find('/') {
            return after_scheme[slash..].to_string();
        }
    }
    url.to_string()
}

/// Builder for [`NetworkEntry`]. Phase 3 DOM-024.
///
/// Replaces the three near-identical `new_http` / `new_sse` / `new_ws`
/// factories. The only fields that differ between protocols are
/// `protocol`, initial `status`, and `method` (empty for WS). Everything
/// else is default — the builder centralises that shape.
pub struct NetworkEntryBuilder {
    entry: NetworkEntry,
}

impl NetworkEntryBuilder {
    /// Start a builder with defaults for an HTTP `Pending` request.
    /// Callers immediately override with [`Self::http`], [`Self::sse`],
    /// or [`Self::ws`] to pin the protocol + initial status.
    pub fn new(id: u64, url: String) -> Self {
        let path = extract_path(&url);
        Self {
            entry: NetworkEntry {
                id,
                protocol: Protocol::Http,
                timestamp: String::new(),
                method: String::new(),
                url,
                path,
                status: NetworkStatus::Pending,
                http_status: None,
                duration: None,
                request_size: None,
                response_size: None,
                request_headers: None,
                response_headers: None,
                request_body: None,
                response_body: None,
                error: None,
                sse_chunks: Vec::new(),
                sse_total_size: 0,
                ws_messages: Vec::new(),
                ws_close_code: None,
                ws_close_reason: None,
                source: EntrySource::App,
                timing: None,
            },
        }
    }

    /// HTTP request with the given method; initial status is `Pending`.
    pub fn http(mut self, method: impl Into<String>) -> Self {
        self.entry.protocol = Protocol::Http;
        self.entry.method = method.into();
        self.entry.status = NetworkStatus::Pending;
        self
    }

    /// SSE stream with the given HTTP method; initial status is `Active`
    /// because the stream begins on connect.
    pub fn sse(mut self, method: impl Into<String>) -> Self {
        self.entry.protocol = Protocol::Sse;
        self.entry.method = method.into();
        self.entry.status = NetworkStatus::Active;
        self
    }

    /// WebSocket; method is empty (WS has no HTTP method) and initial
    /// status is `Active`.
    pub fn ws(mut self) -> Self {
        self.entry.protocol = Protocol::Ws;
        self.entry.method = String::new();
        self.entry.status = NetworkStatus::Active;
        self
    }

    pub fn timestamp(mut self, ts: impl Into<String>) -> Self {
        self.entry.timestamp = ts.into();
        self
    }

    #[allow(dead_code)]
    pub fn source(mut self, source: EntrySource) -> Self {
        self.entry.source = source;
        self
    }

    pub fn build(self) -> NetworkEntry {
        self.entry
    }
}

/// Wire-level flog_net protocol messages from Dart → flog.
///
/// Phase 3 DOM-002 + DOM-006: replaces the 20-field loose-bag
/// `FlogNetMessage` struct with an externally-tagged enum on `t`.
/// serde's `#[serde(tag = "t")]` preserves the wire format byte-for-byte
/// (same field names, same optional-ness) — the change is internal
/// storage shape only, not a protocol change.
///
/// Every variant keeps unused protocol fields as `#[serde(default)]` so
/// forward-compat Dart clients can send extra fields without breaking
/// deserialization. SseChunk/WsMessage storage prunes those fields per
/// DOM-025.
#[derive(Debug, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum FlogNetKind {
    /// Request start. HTTP: Pending; SSE/WS req with matching `p` seeds
    /// an Active entry.
    Req {
        id: u64,
        #[serde(default)]
        p: Option<String>,
        #[serde(default)]
        method: Option<String>,
        #[serde(default)]
        url: Option<String>,
        #[serde(default)]
        headers: Option<serde_json::Value>,
        #[serde(default)]
        body: Option<String>,
        #[serde(default)]
        size: Option<u64>,
        #[serde(default)]
        ts: Option<u64>,
    },
    /// HTTP response.
    Res {
        id: u64,
        #[serde(default)]
        status: Option<u16>,
        #[serde(default)]
        duration: Option<u64>,
        #[serde(default)]
        headers: Option<serde_json::Value>,
        #[serde(default)]
        body: Option<String>,
        #[serde(default)]
        size: Option<u64>,
        #[serde(default)]
        error: Option<String>,
        #[serde(default)]
        mocked: Option<bool>,
        #[serde(default)]
        timing: Option<NetworkTiming>,
        #[serde(default)]
        ts: Option<u64>,
    },
    /// HTTP error — transport failure before a full response.
    Err {
        id: u64,
        #[serde(default)]
        error: Option<String>,
        #[serde(default)]
        duration: Option<u64>,
        #[serde(default)]
        timing: Option<NetworkTiming>,
        #[serde(default)]
        ts: Option<u64>,
    },
    /// SSE chunk. `seq` and `size` are kept on the wire but discarded at
    /// ingest per DOM-025 — the domain storage only keeps `data`.
    Chunk {
        id: u64,
        #[serde(default)]
        data: Option<String>,
        #[serde(default)]
        size: Option<u64>,
        #[serde(default)]
        seq: Option<u32>,
        #[serde(default, rename = "eventTiming")]
        event_timing: Option<TimingEvent>,
        #[serde(default)]
        ts: Option<u64>,
    },
    /// SSE stream end (no more chunks).
    Done {
        id: u64,
        #[serde(default)]
        duration: Option<u64>,
        #[serde(default)]
        timing: Option<NetworkTiming>,
        #[serde(default)]
        ts: Option<u64>,
    },
    /// WebSocket open.
    Open {
        id: u64,
        #[serde(default)]
        url: Option<String>,
        #[serde(default)]
        timing: Option<NetworkTiming>,
        #[serde(default)]
        ts: Option<u64>,
    },
    /// WebSocket handshake started — not yet complete.
    /// TUI shows a Pending entry. Followed by `Open` (success) or `Err` (failure).
    Connecting {
        id: u64,
        #[serde(default)]
        url: Option<String>,
        #[serde(default)]
        timing: Option<NetworkTiming>,
        #[serde(default)]
        ts: Option<u64>,
    },
    /// WebSocket outbound message.
    Send {
        id: u64,
        #[serde(default)]
        data: Option<String>,
        #[serde(default)]
        size: Option<u64>,
        #[serde(default, rename = "eventTiming")]
        event_timing: Option<TimingEvent>,
        #[serde(default)]
        ts: Option<u64>,
    },
    /// WebSocket inbound message.
    Recv {
        id: u64,
        #[serde(default)]
        data: Option<String>,
        #[serde(default)]
        size: Option<u64>,
        #[serde(default, rename = "eventTiming")]
        event_timing: Option<TimingEvent>,
        #[serde(default)]
        ts: Option<u64>,
    },
    /// WebSocket close.
    Close {
        id: u64,
        #[serde(default)]
        code: Option<u16>,
        #[serde(default)]
        reason: Option<String>,
        #[serde(default)]
        duration: Option<u64>,
        #[serde(default)]
        timing: Option<NetworkTiming>,
        #[serde(default)]
        ts: Option<u64>,
    },
}

impl FlogNetKind {
    /// The id of the request/stream this message belongs to.
    pub fn id(&self) -> u64 {
        match self {
            Self::Req { id, .. }
            | Self::Res { id, .. }
            | Self::Err { id, .. }
            | Self::Chunk { id, .. }
            | Self::Done { id, .. }
            | Self::Open { id, .. }
            | Self::Connecting { id, .. }
            | Self::Send { id, .. }
            | Self::Recv { id, .. }
            | Self::Close { id, .. } => *id,
        }
    }
}

#[cfg(test)]
#[path = "network_tests.rs"]
mod tests;
