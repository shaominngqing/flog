//! Network request data types for HTTP, SSE, and WebSocket.

use serde::Deserialize;

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
}

#[derive(Debug, Clone)]
pub struct WsMessage {
    pub direction: WsDirection,
    pub data: String,
    /// Byte size of this message. Read by the Network detail panel + stats.
    pub size: u64,
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
        #[serde(default)]
        ts: Option<u64>,
    },
    /// SSE stream end (no more chunks).
    Done {
        id: u64,
        #[serde(default)]
        duration: Option<u64>,
        #[serde(default)]
        ts: Option<u64>,
    },
    /// WebSocket open.
    Open {
        id: u64,
        #[serde(default)]
        url: Option<String>,
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
            | Self::Send { id, .. }
            | Self::Recv { id, .. }
            | Self::Close { id, .. } => *id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entry_source_default_is_app() {
        let entry =
            NetworkEntry::new_http(1, "GET".into(), "https://example.com".into(), String::new());
        assert_eq!(entry.source, EntrySource::App);
    }

    #[test]
    fn test_entry_source_can_be_set_to_replay() {
        let mut entry =
            NetworkEntry::new_http(1, "GET".into(), "https://example.com".into(), String::new());
        entry.source = EntrySource::Replay;
        assert_eq!(entry.source, EntrySource::Replay);
    }

    #[test]
    fn test_entry_source_can_be_set_to_mocked() {
        let mut entry =
            NetworkEntry::new_http(1, "GET".into(), "https://example.com".into(), String::new());
        entry.source = EntrySource::Mocked;
        assert_eq!(entry.source, EntrySource::Mocked);
    }

    // ==================================================================
    // Phase 2.5B Task 2 — characterization tests
    // ==================================================================

    // ---- DOM-020: extract_path naive string search -------------------

    #[test]
    fn dom_020_extract_path_strips_scheme_and_host_a_http() {
        assert_eq!(extract_path("http://example.com/api/users"), "/api/users");
    }

    #[test]
    fn dom_020_extract_path_strips_scheme_and_host_b_https_with_port() {
        assert_eq!(
            extract_path("https://example.com:8080/api/users?id=1"),
            "/api/users?id=1"
        );
    }

    #[test]
    fn dom_020_extract_path_root_only() {
        assert_eq!(extract_path("https://example.com/"), "/");
    }

    #[test]
    fn dom_020_extract_path_no_path_returns_original() {
        // No slash after host — current behavior returns entire URL
        assert_eq!(extract_path("https://example.com"), "https://example.com");
    }

    #[test]
    fn dom_020_extract_path_no_scheme_returns_original() {
        assert_eq!(extract_path("/api/users"), "/api/users");
        assert_eq!(extract_path("example.com/path"), "example.com/path");
    }

    #[test]
    fn dom_020_extract_path_ipv6_edge_case_fails() {
        // Documented limitation: "http://[::1]:8080/path" — the "::" in
        // IPv6 interacts with extract_path's naive "://" anchor. Current
        // behavior: scheme "http" + after_scheme "[::1]:8080/path" →
        // first '/' is at index 10 → returns "/path".
        assert_eq!(extract_path("http://[::1]:8080/path"), "/path");
    }

    #[test]
    fn dom_020_extract_path_empty_string() {
        assert_eq!(extract_path(""), "");
    }

    #[test]
    fn dom_020_extract_path_query_params() {
        assert_eq!(
            extract_path("https://x.com/search?q=hello&page=2"),
            "/search?q=hello&page=2"
        );
    }

    // ---- DOM-024: factory boilerplate ---------------------------------

    // ---- DOM-024: builder ---------------------------------------------

    #[test]
    fn dom_024_builder_http_sets_protocol_method_and_pending() {
        let e = NetworkEntry::builder(1, "https://x.com/api")
            .http("GET")
            .build();
        assert_eq!(e.id, 1);
        assert_eq!(e.protocol, Protocol::Http);
        assert_eq!(e.method, "GET");
        assert_eq!(e.status, NetworkStatus::Pending);
        assert_eq!(e.path, "/api");
        assert_eq!(e.source, EntrySource::App);
        assert!(e.timestamp.is_empty());
    }

    #[test]
    fn dom_024_builder_sse_sets_protocol_method_and_active() {
        let e = NetworkEntry::builder(2, "https://x.com/stream")
            .sse("GET")
            .build();
        assert_eq!(e.protocol, Protocol::Sse);
        assert_eq!(e.method, "GET");
        assert_eq!(e.status, NetworkStatus::Active);
    }

    #[test]
    fn dom_024_builder_ws_has_empty_method_and_active() {
        let e = NetworkEntry::builder(3, "wss://x.com/ws").ws().build();
        assert_eq!(e.protocol, Protocol::Ws);
        assert_eq!(e.method, "");
        assert_eq!(e.status, NetworkStatus::Active);
    }

    #[test]
    fn dom_024_builder_timestamp_and_source_chainable() {
        let e = NetworkEntry::builder(4, "https://x.com/a")
            .http("POST")
            .timestamp("12:00:00.000")
            .source(EntrySource::Replay)
            .build();
        assert_eq!(e.timestamp, "12:00:00.000");
        assert_eq!(e.source, EntrySource::Replay);
    }

    #[test]
    fn dom_024_legacy_factories_still_delegate_to_builder() {
        // Regression lock: new_http / new_sse / new_ws keep their exact
        // shape after Phase 3 DOM-024 (they now wrap the builder).
        let a = NetworkEntry::new_http(1, "GET".into(), "https://x.com/a".into(), "t".into());
        let b = NetworkEntry::builder(1, "https://x.com/a")
            .http("GET")
            .timestamp("t")
            .build();
        // Compare by all fields that should match.
        assert_eq!(a.id, b.id);
        assert_eq!(a.protocol, b.protocol);
        assert_eq!(a.method, b.method);
        assert_eq!(a.url, b.url);
        assert_eq!(a.path, b.path);
        assert_eq!(a.status, b.status);
        assert_eq!(a.timestamp, b.timestamp);
    }

    // ---- DOM-003: orphan response factory ----------------------------

    #[test]
    fn dom_003_new_orphan_response_sets_orphan_status() {
        let e = NetworkEntry::new_orphan_response(7, Some(404), Some("nope".into()), Some(12));
        assert_eq!(e.id, 7);
        assert_eq!(e.status, NetworkStatus::Orphan);
        assert_eq!(e.http_status, Some(404));
        assert_eq!(e.response_body.as_deref(), Some("nope"));
        assert_eq!(e.response_size, Some(4));
        assert_eq!(e.duration, Some(12));
        assert_eq!(e.method, "?");
        assert_eq!(e.protocol, Protocol::Http);
    }

    #[test]
    fn dom_024_new_http_defaults() {
        let e = NetworkEntry::new_http(1, "GET".into(), "https://x.com/api".into(), "t".into());
        assert_eq!(e.id, 1);
        assert_eq!(e.protocol, Protocol::Http);
        assert_eq!(e.method, "GET");
        assert_eq!(e.url, "https://x.com/api");
        assert_eq!(e.path, "/api");
        assert_eq!(e.status, NetworkStatus::Pending);
        assert_eq!(e.timestamp, "t");
        assert!(e.http_status.is_none());
        assert!(e.duration.is_none());
        assert!(e.request_size.is_none());
        assert!(e.response_size.is_none());
        assert!(e.request_headers.is_none());
        assert!(e.response_headers.is_none());
        assert!(e.request_body.is_none());
        assert!(e.response_body.is_none());
        assert!(e.error.is_none());
        assert!(e.sse_chunks.is_empty());
        assert_eq!(e.sse_total_size, 0);
        assert!(e.ws_messages.is_empty());
        assert!(e.ws_close_code.is_none());
        assert!(e.ws_close_reason.is_none());
        assert_eq!(e.source, EntrySource::App);
    }

    #[test]
    fn dom_024_new_sse_defaults() {
        let e = NetworkEntry::new_sse(1, "GET".into(), "https://x.com/stream".into(), "t".into());
        assert_eq!(e.protocol, Protocol::Sse);
        assert_eq!(e.status, NetworkStatus::Active); // SSE starts active
        assert_eq!(e.method, "GET");
        assert_eq!(e.path, "/stream");
    }

    #[test]
    fn dom_024_new_ws_defaults() {
        let e = NetworkEntry::new_ws(1, "wss://x.com/ws".into(), "t".into());
        assert_eq!(e.protocol, Protocol::Ws);
        assert_eq!(e.status, NetworkStatus::Active);
        assert_eq!(e.method, "");
        assert_eq!(e.path, "/ws");
    }

    // ---- DOM-025: write-only SseChunk fields round-trip --------------
    // Locks the payload shape. The Dart client sends seq/size/ts fields;
    // our deserializer accepts them. Phase 3 may prune these, but it
    // must be intentional — the test makes that decision visible.

    #[test]
    fn dom_025_sse_chunk_has_only_data_after_prune() {
        // Phase 3 DOM-025: seq/size/timestamp were write-only and are
        // pruned from the storage struct. The wire format still accepts
        // them (see dom_025_flog_net_message_accepts_all_fields_from_protocol)
        // for compat, but the domain type only carries data.
        let chunk = SseChunk {
            data: "payload".to_string(),
        };
        assert_eq!(chunk.data, "payload");
    }

    #[test]
    fn dom_025_ws_message_has_direction_data_size_after_prune() {
        // Phase 3 DOM-025: WsMessage.timestamp was write-only; size is
        // kept because detail + stats read it.
        let m = WsMessage {
            direction: WsDirection::Send,
            data: "hi".to_string(),
            size: 2,
        };
        assert_eq!(m.direction, WsDirection::Send);
        assert_eq!(m.data, "hi");
        assert_eq!(m.size, 2);
    }

    #[test]
    fn dom_025_flog_net_kind_chunk_accepts_all_wire_fields() {
        // Phase 3 DOM-002/006/025: Chunk variant retains seq/size/ts on
        // the wire for back-compat. Extra fields Dart may send alongside
        // (e.g. legacy `p`, `method`) are ignored silently.
        let j = r#"{
            "id": 1,
            "t": "chunk",
            "p": "sse",
            "size": 128,
            "data": "payload",
            "seq": 5,
            "ts": 1700000000000
        }"#;
        let parsed: FlogNetKind = serde_json::from_str(j).expect("deserialize");
        match parsed {
            FlogNetKind::Chunk {
                id,
                data,
                size,
                seq,
                ts,
            } => {
                assert_eq!(id, 1);
                assert_eq!(data.as_deref(), Some("payload"));
                assert_eq!(size, Some(128));
                assert_eq!(seq, Some(5));
                assert_eq!(ts, Some(1_700_000_000_000));
            }
            _ => panic!("expected Chunk"),
        }
    }

    #[test]
    fn dom_025_flog_net_kind_req_minimal_payload() {
        // Missing optional fields → None
        let j = r#"{"id": 1, "t": "req"}"#;
        let parsed: FlogNetKind = serde_json::from_str(j).expect("deserialize");
        match parsed {
            FlogNetKind::Req {
                id,
                p,
                method,
                url,
                ts,
                size,
                ..
            } => {
                assert_eq!(id, 1);
                assert!(p.is_none());
                assert!(method.is_none());
                assert!(url.is_none());
                assert!(ts.is_none());
                assert!(size.is_none());
            }
            _ => panic!("expected Req"),
        }
    }

    // ---- display_size per protocol -----------------------------------

    #[test]
    fn display_size_http_uses_response_size() {
        let mut e = NetworkEntry::new_http(1, "GET".into(), "/x".into(), String::new());
        e.response_size = Some(500);
        assert_eq!(e.display_size(), 500);
    }

    #[test]
    fn display_size_http_none_is_zero() {
        let e = NetworkEntry::new_http(1, "GET".into(), "/x".into(), String::new());
        assert_eq!(e.display_size(), 0);
    }

    #[test]
    fn display_size_sse_uses_total() {
        let mut e = NetworkEntry::new_sse(1, "GET".into(), "/x".into(), String::new());
        e.sse_total_size = 999;
        assert_eq!(e.display_size(), 999);
    }

    #[test]
    fn display_size_ws_sums_messages() {
        let mut e = NetworkEntry::new_ws(1, "wss://x".into(), String::new());
        e.ws_messages.push(WsMessage {
            direction: WsDirection::Send,
            data: "a".into(),
            size: 10,
        });
        e.ws_messages.push(WsMessage {
            direction: WsDirection::Recv,
            data: "b".into(),
            size: 25,
        });
        assert_eq!(e.display_size(), 35);
    }

    #[test]
    fn display_size_ws_empty_is_zero() {
        let e = NetworkEntry::new_ws(1, "wss://x".into(), String::new());
        assert_eq!(e.display_size(), 0);
    }
}
