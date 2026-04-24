//! Ring-buffer storage for network entries with message processing.

use std::collections::VecDeque;

use crate::domain::network::{
    FlogNetKind, NetworkEntry, NetworkStatus, Protocol, SseChunk, WsDirection, WsMessage,
};

const MAX_ENTRIES: usize = 10_000;

pub struct NetworkStore {
    entries: VecDeque<NetworkEntry>,
}

impl NetworkStore {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
        }
    }

    pub fn process_message(&mut self, msg: FlogNetKind) {
        match msg {
            FlogNetKind::Req {
                id,
                p,
                method,
                url,
                headers,
                body,
                size,
                ts,
            } => self.handle_req(id, p, method, url, headers, body, size, ts),
            FlogNetKind::Res {
                id,
                status,
                duration,
                headers,
                body,
                size,
                error,
                mocked,
                ts: _,
            } => self.handle_res(id, status, duration, headers, body, size, error, mocked),
            FlogNetKind::Err {
                id,
                error,
                duration,
                ts: _,
            } => self.handle_err(id, error, duration),
            FlogNetKind::Chunk {
                id,
                data,
                size,
                seq: _,
                ts: _,
            } => self.handle_chunk(id, data, size),
            FlogNetKind::Done {
                id,
                duration,
                ts: _,
            } => self.handle_done(id, duration),
            FlogNetKind::Open { id, url, ts } => self.handle_open(id, url, ts),
            FlogNetKind::Send {
                id,
                data,
                size,
                ts: _,
            } => self.handle_ws_msg(id, data, size, WsDirection::Send),
            FlogNetKind::Recv {
                id,
                data,
                size,
                ts: _,
            } => self.handle_ws_msg(id, data, size, WsDirection::Recv),
            FlogNetKind::Close {
                id,
                code,
                reason,
                duration,
                ts: _,
            } => self.handle_close(id, code, reason, duration),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&NetworkEntry> {
        self.entries.get(index)
    }

    pub fn iter(&self) -> impl Iterator<Item = &NetworkEntry> {
        self.entries.iter()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Add a pre-built entry directly (used by Replay and Mock).
    pub fn push_entry(&mut self, entry: NetworkEntry) {
        self.ensure_capacity();
        self.entries.push_back(entry);
    }

    fn ensure_capacity(&mut self) {
        if self.entries.len() >= MAX_ENTRIES {
            self.entries.pop_front();
        }
    }

    fn find_by_id_mut(&mut self, id: u64) -> Option<&mut NetworkEntry> {
        self.entries.iter_mut().rev().find(|e| e.id == id)
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_req(
        &mut self,
        id: u64,
        p: Option<String>,
        method: Option<String>,
        url: Option<String>,
        headers: Option<serde_json::Value>,
        body: Option<String>,
        size: Option<u64>,
        ts: Option<u64>,
    ) {
        self.ensure_capacity();

        let method = method.unwrap_or_default();
        let url = url.unwrap_or_default();
        let protocol = match p.as_deref() {
            Some("sse") => Protocol::Sse,
            Some("ws") => Protocol::Ws,
            _ => Protocol::Http,
        };

        let mut entry = match protocol {
            Protocol::Http => NetworkEntry::new_http(id, method, url, String::new()),
            Protocol::Sse => NetworkEntry::new_sse(id, method, url, String::new()),
            Protocol::Ws => NetworkEntry::new_ws(id, url, String::new()),
        };

        if let Some(h) = headers {
            entry.request_headers = Some(h.to_string());
        }
        if let Some(b) = body {
            entry.request_size = Some(b.len() as u64);
            entry.request_body = Some(b);
        }
        if let Some(s) = size {
            entry.request_size = Some(s);
        }
        if let Some(t) = ts {
            entry.timestamp = format_ts(t);
        }

        self.entries.push_back(entry);
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_res(
        &mut self,
        id: u64,
        status: Option<u16>,
        duration: Option<u64>,
        headers: Option<serde_json::Value>,
        body: Option<String>,
        size: Option<u64>,
        error: Option<String>,
        mocked: Option<bool>,
    ) {
        if let Some(entry) = self.find_by_id_mut(id) {
            entry.status = NetworkStatus::Completed;
            if mocked == Some(true) {
                entry.source = crate::domain::network::EntrySource::Mocked;
            }
            entry.http_status = status;
            entry.duration = duration;
            entry.error = error;
            if let Some(h) = headers {
                entry.response_headers = Some(h.to_string());
            }
            if let Some(b) = body {
                entry.response_size = Some(b.len() as u64);
                entry.response_body = Some(b);
            }
            if let Some(s) = size {
                entry.response_size = Some(s);
            }
        } else {
            // DOM-003 fix: a Response arrived with no matching Request — don't
            // drop it silently. Surface as an Orphan entry so the user can see
            // that data was received without context.
            self.ensure_capacity();
            let entry = NetworkEntry::new_orphan_response(id, status, body, duration);
            self.entries.push_back(entry);
        }
    }

    fn handle_err(&mut self, id: u64, error: Option<String>, duration: Option<u64>) {
        if let Some(entry) = self.find_by_id_mut(id) {
            entry.status = NetworkStatus::Failed;
            entry.error = error;
            entry.duration = duration;
        }
    }

    fn handle_chunk(&mut self, id: u64, data: Option<String>, size: Option<u64>) {
        if let Some(entry) = self.find_by_id_mut(id) {
            let data = data.unwrap_or_default();
            let chunk_size = size.unwrap_or(data.len() as u64);
            entry.sse_total_size += chunk_size;
            // DOM-025: seq/size/ts are accepted on the wire but dropped at
            // ingest — no UI consumer reads them.
            entry.sse_chunks.push(SseChunk { data });
        }
    }

    fn handle_done(&mut self, id: u64, duration: Option<u64>) {
        if let Some(entry) = self.find_by_id_mut(id) {
            entry.status = NetworkStatus::Completed;
            entry.duration = duration;
        }
    }

    fn handle_open(&mut self, id: u64, url: Option<String>, ts: Option<u64>) {
        self.ensure_capacity();

        let url = url.unwrap_or_default();
        let mut entry = NetworkEntry::new_ws(id, url, String::new());
        if let Some(t) = ts {
            entry.timestamp = format_ts(t);
        }
        self.entries.push_back(entry);
    }

    fn handle_ws_msg(
        &mut self,
        id: u64,
        data: Option<String>,
        size: Option<u64>,
        direction: WsDirection,
    ) {
        if let Some(entry) = self.find_by_id_mut(id) {
            let data = data.unwrap_or_default();
            let msg_size = size.unwrap_or(data.len() as u64);

            // DOM-025: per-message timestamp is accepted on the wire but
            // not stored — no UI consumer reads it.
            entry.ws_messages.push(WsMessage {
                direction,
                data,
                size: msg_size,
            });
        }
    }

    fn handle_close(
        &mut self,
        id: u64,
        code: Option<u16>,
        reason: Option<String>,
        duration: Option<u64>,
    ) {
        if let Some(entry) = self.find_by_id_mut(id) {
            entry.status = NetworkStatus::Completed;
            entry.ws_close_code = code;
            entry.ws_close_reason = reason;
            entry.duration = duration;
        }
    }
}

impl Default for NetworkStore {
    fn default() -> Self {
        Self::new()
    }
}

fn format_ts(millis: u64) -> String {
    let secs = millis / 1000;
    let ms = millis % 1000;
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}.{:03}", h, m, s, ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::network::EntrySource;

    // Test-only variant factories. Phase 3 DOM-002/006 replaced the
    // 20-field FlogNetMessage struct with a typed enum; tests now build
    // variants directly instead of mutating a mega-struct.

    fn req(id: u64) -> FlogNetKind {
        FlogNetKind::Req {
            id,
            p: None,
            method: None,
            url: None,
            headers: None,
            body: None,
            size: None,
            ts: None,
        }
    }

    fn res(id: u64) -> FlogNetKind {
        FlogNetKind::Res {
            id,
            status: None,
            duration: None,
            headers: None,
            body: None,
            size: None,
            error: None,
            mocked: None,
            ts: None,
        }
    }

    // Convenience variant constructors used across tests.
    fn req_http(id: u64, method: &str, url: &str) -> FlogNetKind {
        FlogNetKind::Req {
            id,
            p: None,
            method: Some(method.to_string()),
            url: Some(url.to_string()),
            headers: None,
            body: None,
            size: None,
            ts: None,
        }
    }

    fn req_sse(id: u64, method: &str, url: &str) -> FlogNetKind {
        FlogNetKind::Req {
            id,
            p: Some("sse".to_string()),
            method: Some(method.to_string()),
            url: Some(url.to_string()),
            headers: None,
            body: None,
            size: None,
            ts: None,
        }
    }

    fn req_ws(id: u64, url: &str) -> FlogNetKind {
        FlogNetKind::Req {
            id,
            p: Some("ws".to_string()),
            method: None,
            url: Some(url.to_string()),
            headers: None,
            body: None,
            size: None,
            ts: None,
        }
    }

    fn chunk(id: u64, data: Option<&str>) -> FlogNetKind {
        FlogNetKind::Chunk {
            id,
            data: data.map(|s| s.to_string()),
            size: None,
            seq: None,
            ts: None,
        }
    }

    fn open(id: u64, url: &str) -> FlogNetKind {
        FlogNetKind::Open {
            id,
            url: Some(url.to_string()),
            ts: None,
        }
    }

    fn send_msg(id: u64, data: &str) -> FlogNetKind {
        FlogNetKind::Send {
            id,
            data: Some(data.to_string()),
            size: None,
            ts: None,
        }
    }

    fn recv_msg(id: u64, data: &str) -> FlogNetKind {
        FlogNetKind::Recv {
            id,
            data: Some(data.to_string()),
            size: None,
            ts: None,
        }
    }

    #[test]
    fn test_push_entry() {
        let mut store = NetworkStore::new();
        let mut entry =
            NetworkEntry::new_http(999, "GET".into(), "https://test.com".into(), String::new());
        entry.source = EntrySource::Replay;

        store.push_entry(entry);
        assert_eq!(store.len(), 1);
        assert_eq!(store.get(0).unwrap().source, EntrySource::Replay);
        assert_eq!(store.get(0).unwrap().id, 999);
    }

    // ==================================================================
    // Phase 2.5B Task 2 — characterization tests
    // ==================================================================

    // ---- Basic store invariants --------------------------------------

    #[test]
    fn new_store_is_empty() {
        let store = NetworkStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
        assert!(store.get(0).is_none());
    }

    #[test]
    fn default_store_is_empty() {
        let store = NetworkStore::default();
        assert!(store.is_empty());
    }

    #[test]
    fn clear_empties_store() {
        let mut store = NetworkStore::new();
        store.process_message(req_http(1, "GET", "https://x.com"));
        assert_eq!(store.len(), 1);
        store.clear();
        assert!(store.is_empty());
    }

    #[test]
    fn iter_yields_entries_in_order() {
        let mut store = NetworkStore::new();
        for id in 1..=3 {
            store.process_message(req_http(id, "GET", &format!("https://x.com/{}", id)));
        }
        let ids: Vec<u64> = store.iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![1, 2, 3]);
    }

    // ---- DOM-002: state machine for FlogNetKind ----------------------
    // Current behavior: no transition validation. Each test locks a
    // specific "bad transition" outcome so Phase 3 changes are visible.

    #[test]
    fn dom_002_res_without_req_surfaces_orphan_entry_a() {
        // Phase 3 DOM-003: an orphan Response is no longer silently dropped;
        // it is pushed as a placeholder entry with NetworkStatus::Orphan.
        let mut store = NetworkStore::new();
        let m = FlogNetKind::Res {
            id: 42,
            status: Some(200),
            duration: None,
            headers: None,
            body: Some("orphan".into()),
            size: None,
            error: None,
            mocked: None,
            ts: None,
        };
        store.process_message(m);
        assert_eq!(store.len(), 1);
        let e = store.get(0).unwrap();
        assert_eq!(e.id, 42);
        assert_eq!(e.status, NetworkStatus::Orphan);
        assert_eq!(e.http_status, Some(200));
        assert_eq!(e.response_body.as_deref(), Some("orphan"));
    }

    #[test]
    fn dom_002_err_without_req_drops_silently_b() {
        let mut store = NetworkStore::new();
        store.process_message(FlogNetKind::Err {
            id: 42,
            error: Some("boom".into()),
            duration: None,
            ts: None,
        });
        assert!(store.is_empty());
    }

    #[test]
    fn dom_002_chunk_without_req_drops_silently_c() {
        let mut store = NetworkStore::new();
        store.process_message(chunk(42, Some("hi")));
        assert!(store.is_empty());
    }

    #[test]
    fn dom_002_done_without_req_drops_silently_d() {
        let mut store = NetworkStore::new();
        store.process_message(FlogNetKind::Done {
            id: 42,
            duration: None,
            ts: None,
        });
        assert!(store.is_empty());
    }

    #[test]
    fn dom_002_close_without_open_drops_silently_e() {
        let mut store = NetworkStore::new();
        store.process_message(FlogNetKind::Close {
            id: 42,
            code: None,
            reason: None,
            duration: None,
            ts: None,
        });
        assert!(store.is_empty());
    }

    #[test]
    fn dom_002_ws_send_without_open_drops_silently_f() {
        let mut store = NetworkStore::new();
        store.process_message(send_msg(42, "hi"));
        assert!(store.is_empty());
    }

    #[test]
    fn dom_002_ws_recv_without_open_drops_silently_g() {
        let mut store = NetworkStore::new();
        store.process_message(recv_msg(42, "hi"));
        assert!(store.is_empty());
    }

    #[test]
    fn dom_002_second_req_with_same_id_creates_new_entry() {
        // Current behavior: handle_req always pushes a new entry.
        let mut store = NetworkStore::new();
        store.process_message(req_http(1, "GET", "https://x.com"));
        store.process_message(req_http(1, "POST", "https://y.com"));
        assert_eq!(store.len(), 2);
    }

    #[test]
    fn dom_002_chunk_on_http_protocol_still_appends_to_sse_chunks() {
        // Current behavior: find_by_id_mut locates the HTTP entry, chunk
        // is appended to its sse_chunks field. This is arguably buggy per
        // audit but is locked as current behavior.
        let mut store = NetworkStore::new();
        store.process_message(req_http(1, "GET", "https://x.com"));
        store.process_message(chunk(1, Some("stream-like")));

        let entry = store.get(0).unwrap();
        assert_eq!(entry.protocol, Protocol::Http);
        assert_eq!(entry.sse_chunks.len(), 1);
    }

    #[test]
    fn dom_002_unknown_message_type_is_rejected_at_parse_time() {
        // Phase 3 DOM-002: after the FlogNetKind enum, unknown message
        // types no longer reach process_message — they fail at serde
        // deserialization with an "unknown variant" error.
        let j = r#"{"t":"unknown_type","id":1}"#;
        assert!(serde_json::from_str::<FlogNetKind>(j).is_err());
    }

    // ---- DOM-006: typed FlogNetKind boundary --------------------------
    // Lock the handle_* boundary's handling of optional fields after the
    // typed-enum refactor. Tests build variants directly.

    #[test]
    fn dom_006_req_missing_method_defaults_to_empty() {
        let mut store = NetworkStore::new();
        store.process_message(FlogNetKind::Req {
            id: 1,
            p: None,
            method: None,
            url: Some("https://x.com".into()),
            headers: None,
            body: None,
            size: None,
            ts: None,
        });
        assert_eq!(store.get(0).unwrap().method, "");
    }

    #[test]
    fn dom_006_req_missing_url_defaults_to_empty() {
        let mut store = NetworkStore::new();
        store.process_message(FlogNetKind::Req {
            id: 1,
            p: None,
            method: Some("GET".into()),
            url: None,
            headers: None,
            body: None,
            size: None,
            ts: None,
        });
        assert_eq!(store.get(0).unwrap().url, "");
    }

    #[test]
    fn dom_006_req_unknown_protocol_defaults_to_http() {
        let mut store = NetworkStore::new();
        store.process_message(FlogNetKind::Req {
            id: 1,
            p: Some("magic".into()),
            method: None,
            url: Some("https://x.com".into()),
            headers: None,
            body: None,
            size: None,
            ts: None,
        });
        assert_eq!(store.get(0).unwrap().protocol, Protocol::Http);
    }

    #[test]
    fn dom_006_req_no_protocol_defaults_to_http() {
        let mut store = NetworkStore::new();
        store.process_message(req_http(1, "GET", "https://x.com"));
        assert_eq!(store.get(0).unwrap().protocol, Protocol::Http);
    }

    #[test]
    fn dom_006_req_sse_protocol() {
        let mut store = NetworkStore::new();
        store.process_message(req_sse(1, "GET", "https://x.com/stream"));
        assert_eq!(store.get(0).unwrap().protocol, Protocol::Sse);
    }

    #[test]
    fn dom_006_req_ws_protocol() {
        let mut store = NetworkStore::new();
        store.process_message(req_ws(1, "wss://x.com"));
        assert_eq!(store.get(0).unwrap().protocol, Protocol::Ws);
    }

    // ---- Full happy-path flows ---------------------------------------

    #[test]
    fn handle_req_stores_headers_body_and_timestamp() {
        let mut store = NetworkStore::new();
        store.process_message(FlogNetKind::Req {
            id: 1,
            p: None,
            method: Some("POST".into()),
            url: Some("https://x.com/api".into()),
            headers: Some(serde_json::json!({"Content-Type": "application/json"})),
            body: Some("{\"a\":1}".into()),
            size: None,
            ts: Some(1_700_000_000_000),
        });

        let e = store.get(0).unwrap();
        assert!(e.request_headers.as_ref().unwrap().contains("Content-Type"));
        assert_eq!(e.request_body.as_ref().unwrap(), "{\"a\":1}");
        assert_eq!(e.request_size, Some(7));
        // timestamp formatted HH:MM:SS.mmm
        assert_eq!(e.timestamp.len(), 12);
        assert_eq!(&e.timestamp[2..3], ":");
    }

    #[test]
    fn handle_req_size_field_overrides_body_len() {
        let mut store = NetworkStore::new();
        store.process_message(FlogNetKind::Req {
            id: 1,
            p: None,
            method: Some("POST".into()),
            url: Some("https://x.com".into()),
            headers: None,
            body: Some("abc".into()),
            size: Some(999),
            ts: None,
        });
        // size after body is applied last → 999
        assert_eq!(store.get(0).unwrap().request_size, Some(999));
    }

    #[test]
    fn handle_res_updates_matched_entry() {
        let mut store = NetworkStore::new();
        store.process_message(req_http(1, "GET", "https://x.com"));
        store.process_message(FlogNetKind::Res {
            id: 1,
            status: Some(200),
            duration: Some(50),
            headers: Some(serde_json::json!({"X-Test": "1"})),
            body: Some("ok".into()),
            size: None,
            error: None,
            mocked: None,
            ts: None,
        });

        let e = store.get(0).unwrap();
        assert_eq!(e.status, NetworkStatus::Completed);
        assert_eq!(e.http_status, Some(200));
        assert_eq!(e.duration, Some(50));
        assert!(e.response_headers.as_ref().unwrap().contains("X-Test"));
        assert_eq!(e.response_body.as_ref().unwrap(), "ok");
        assert_eq!(e.response_size, Some(2));
    }

    #[test]
    fn handle_res_size_field_overrides_body_len() {
        let mut store = NetworkStore::new();
        store.process_message(req_http(1, "GET", "https://x.com"));
        store.process_message(FlogNetKind::Res {
            id: 1,
            status: Some(200),
            duration: None,
            headers: None,
            body: Some("ok".into()),
            size: Some(10_000),
            error: None,
            mocked: None,
            ts: None,
        });
        assert_eq!(store.get(0).unwrap().response_size, Some(10_000));
    }

    #[test]
    fn handle_res_mocked_flag_sets_source() {
        let mut store = NetworkStore::new();
        store.process_message(req_http(1, "GET", "https://x.com"));
        store.process_message(FlogNetKind::Res {
            id: 1,
            status: Some(200),
            duration: None,
            headers: None,
            body: None,
            size: None,
            error: None,
            mocked: Some(true),
            ts: None,
        });
        assert_eq!(store.get(0).unwrap().source, EntrySource::Mocked);
    }

    #[test]
    fn handle_res_mocked_false_keeps_app_source() {
        let mut store = NetworkStore::new();
        store.process_message(req_http(1, "GET", "https://x.com"));
        store.process_message(FlogNetKind::Res {
            id: 1,
            status: Some(200),
            duration: None,
            headers: None,
            body: None,
            size: None,
            error: None,
            mocked: Some(false),
            ts: None,
        });
        assert_eq!(store.get(0).unwrap().source, EntrySource::App);
    }

    #[test]
    fn handle_err_sets_failed_status() {
        let mut store = NetworkStore::new();
        store.process_message(req_http(1, "GET", "https://x.com"));
        store.process_message(FlogNetKind::Err {
            id: 1,
            error: Some("timeout".into()),
            duration: Some(30_000),
            ts: None,
        });

        let e = store.get(0).unwrap();
        assert_eq!(e.status, NetworkStatus::Failed);
        assert_eq!(e.error.as_ref().unwrap(), "timeout");
        assert_eq!(e.duration, Some(30_000));
    }

    #[test]
    fn handle_chunk_appends_to_sse_chunks() {
        let mut store = NetworkStore::new();
        store.process_message(req_sse(1, "GET", "https://x.com/stream"));
        store.process_message(FlogNetKind::Chunk {
            id: 1,
            data: Some("hello".into()),
            size: Some(5),
            seq: Some(0),
            ts: None,
        });
        store.process_message(FlogNetKind::Chunk {
            id: 1,
            data: Some(" world".into()),
            size: None,
            seq: Some(1),
            ts: None,
        });

        let e = store.get(0).unwrap();
        assert_eq!(e.sse_chunks.len(), 2);
        assert_eq!(e.sse_chunks[0].data, "hello");
        assert_eq!(e.sse_chunks[1].data, " world");
        // DOM-025: sse_total_size is the live aggregate; per-chunk
        // seq/size/timestamp are accepted on the wire but dropped.
        assert_eq!(e.sse_total_size, 11);
    }

    #[test]
    fn handle_chunk_stores_data_in_order() {
        // DOM-025: SseChunk.seq was dropped. Lock ordering via the data
        // field: chunks push to the back in the order received.
        let mut store = NetworkStore::new();
        store.process_message(req_sse(1, "GET", "https://x.com"));
        store.process_message(chunk(1, Some("a")));
        store.process_message(chunk(1, Some("b")));

        let chunks = &store.get(0).unwrap().sse_chunks;
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].data, "a");
        assert_eq!(chunks[1].data, "b");
    }

    #[test]
    fn handle_chunk_defaults_data_to_empty_string() {
        let mut store = NetworkStore::new();
        store.process_message(req_sse(1, "GET", "https://x.com"));
        store.process_message(chunk(1, None));
        let e = store.get(0).unwrap();
        assert_eq!(e.sse_chunks[0].data, "");
    }

    #[test]
    fn handle_done_completes_entry() {
        let mut store = NetworkStore::new();
        store.process_message(req_sse(1, "GET", "https://x.com"));
        store.process_message(FlogNetKind::Done {
            id: 1,
            duration: Some(500),
            ts: None,
        });
        let e = store.get(0).unwrap();
        assert_eq!(e.status, NetworkStatus::Completed);
        assert_eq!(e.duration, Some(500));
    }

    #[test]
    fn handle_open_creates_ws_entry() {
        let mut store = NetworkStore::new();
        store.process_message(FlogNetKind::Open {
            id: 1,
            url: Some("wss://x.com/ws".into()),
            ts: Some(1_700_000_000_000),
        });
        let e = store.get(0).unwrap();
        assert_eq!(e.protocol, Protocol::Ws);
        assert_eq!(e.url, "wss://x.com/ws");
        assert_eq!(e.timestamp.len(), 12);
    }

    #[test]
    fn handle_open_missing_url_defaults_to_empty() {
        let mut store = NetworkStore::new();
        store.process_message(FlogNetKind::Open {
            id: 1,
            url: None,
            ts: None,
        });
        assert_eq!(store.get(0).unwrap().url, "");
    }

    #[test]
    fn handle_send_and_recv_append_ws_messages() {
        let mut store = NetworkStore::new();
        store.process_message(open(1, "wss://x.com"));
        store.process_message(send_msg(1, "ping"));
        store.process_message(recv_msg(1, "pong"));

        let e = store.get(0).unwrap();
        assert_eq!(e.ws_messages.len(), 2);
        assert_eq!(e.ws_messages[0].direction, WsDirection::Send);
        assert_eq!(e.ws_messages[0].data, "ping");
        assert_eq!(e.ws_messages[0].size, 4);
        assert_eq!(e.ws_messages[1].direction, WsDirection::Recv);
        assert_eq!(e.ws_messages[1].data, "pong");
    }

    #[test]
    fn handle_ws_msg_size_override() {
        let mut store = NetworkStore::new();
        store.process_message(open(1, "wss://x.com"));
        store.process_message(FlogNetKind::Send {
            id: 1,
            data: Some("abc".into()),
            size: Some(100),
            ts: None,
        });

        assert_eq!(store.get(0).unwrap().ws_messages[0].size, 100);
    }

    #[test]
    fn handle_close_sets_close_fields() {
        let mut store = NetworkStore::new();
        store.process_message(open(1, "wss://x.com"));
        store.process_message(FlogNetKind::Close {
            id: 1,
            code: Some(1000),
            reason: Some("Normal".into()),
            duration: Some(5_000),
            ts: None,
        });

        let e = store.get(0).unwrap();
        assert_eq!(e.status, NetworkStatus::Completed);
        assert_eq!(e.ws_close_code, Some(1000));
        assert_eq!(e.ws_close_reason.as_ref().unwrap(), "Normal");
        assert_eq!(e.duration, Some(5_000));
    }

    // ---- Capacity / ring buffer behavior ------------------------------

    #[test]
    fn push_entry_evicts_oldest_at_capacity() {
        let mut store = NetworkStore::new();
        for id in 0..MAX_ENTRIES as u64 {
            let e = NetworkEntry::new_http(id, "GET".into(), format!("/{}", id), String::new());
            store.push_entry(e);
        }
        assert_eq!(store.len(), MAX_ENTRIES);

        let overflow =
            NetworkEntry::new_http(9_999_999, "GET".into(), "/overflow".into(), String::new());
        store.push_entry(overflow);
        // len stays at cap; oldest (id=0) evicted
        assert_eq!(store.len(), MAX_ENTRIES);
        assert_eq!(store.get(0).unwrap().id, 1);
        assert_eq!(store.get(MAX_ENTRIES - 1).unwrap().id, 9_999_999);
    }

    #[test]
    fn handle_req_evicts_oldest_at_capacity() {
        let mut store = NetworkStore::new();
        for id in 0..MAX_ENTRIES as u64 {
            let e = NetworkEntry::new_http(id, "GET".into(), format!("/{}", id), String::new());
            store.push_entry(e);
        }
        store.process_message(req_http(9_999_999, "GET", "/overflow"));
        assert_eq!(store.len(), MAX_ENTRIES);
        assert_eq!(store.get(0).unwrap().id, 1);
    }

    // ---- format_ts branches -----------------------------------------

    #[test]
    fn format_ts_zero_epoch() {
        assert_eq!(format_ts(0), "00:00:00.000");
    }

    #[test]
    fn format_ts_wraps_day_boundary() {
        // 25 hours exactly → wraps to 01:00:00.000
        let ms = 25 * 3600 * 1000;
        assert_eq!(format_ts(ms), "01:00:00.000");
    }

    #[test]
    fn format_ts_with_milliseconds() {
        // 1 hour 2 min 3 sec 456 ms
        let ms = (3600 + 2 * 60 + 3) * 1000 + 456;
        assert_eq!(format_ts(ms), "01:02:03.456");
    }

    // ---- find_by_id_mut: most-recent-wins -----------------------------

    #[test]
    fn find_by_id_mut_returns_most_recent_when_duplicate() {
        // Two reqs with id=1. Subsequent res should update the *second* one.
        let mut store = NetworkStore::new();
        store.process_message(req_http(1, "GET", "https://a"));
        store.process_message(req_http(1, "GET", "https://b"));
        store.process_message(FlogNetKind::Res {
            id: 1,
            status: Some(200),
            duration: None,
            headers: None,
            body: None,
            size: None,
            error: None,
            mocked: None,
            ts: None,
        });

        // First entry still Pending, second Completed
        assert_eq!(store.get(0).unwrap().status, NetworkStatus::Pending);
        assert_eq!(store.get(1).unwrap().status, NetworkStatus::Completed);
    }

    // ---- DOM-002/006: wire-format round-trips ------------------------

    #[test]
    fn dom_006_flog_net_kind_req_deserializes_from_dart_wire_format() {
        let j = r#"{"t":"req","id":1,"p":"http","method":"GET","url":"https://x.com","headers":{"A":"1"},"body":"","size":0,"ts":1700000000000}"#;
        let k: FlogNetKind = serde_json::from_str(j).expect("deserialize");
        match k {
            FlogNetKind::Req {
                id,
                p,
                method,
                url,
                ts,
                ..
            } => {
                assert_eq!(id, 1);
                assert_eq!(p.as_deref(), Some("http"));
                assert_eq!(method.as_deref(), Some("GET"));
                assert_eq!(url.as_deref(), Some("https://x.com"));
                assert_eq!(ts, Some(1_700_000_000_000));
            }
            _ => panic!("expected Req"),
        }
    }

    #[test]
    fn dom_006_flog_net_kind_chunk_accepts_dropped_fields_on_wire() {
        // DOM-025: seq/size/ts still accepted on the wire; stored type
        // only keeps data.
        let j = r#"{"t":"chunk","id":1,"data":"x","seq":5,"size":1,"ts":1}"#;
        let k: FlogNetKind = serde_json::from_str(j).expect("deserialize");
        assert!(matches!(k, FlogNetKind::Chunk { .. }));
    }

    #[test]
    fn dom_006_flog_net_kind_rejects_unknown_variant() {
        let j = r#"{"t":"never_heard_of_it","id":1}"#;
        assert!(serde_json::from_str::<FlogNetKind>(j).is_err());
    }

    #[test]
    fn dom_006_flog_net_kind_rejects_missing_id() {
        let j = r#"{"t":"req","method":"GET"}"#;
        assert!(serde_json::from_str::<FlogNetKind>(j).is_err());
    }

    #[test]
    fn dom_006_flog_net_kind_id_helper_covers_all_variants() {
        let variants = [
            req(7),
            res(7),
            FlogNetKind::Err {
                id: 7,
                error: None,
                duration: None,
                ts: None,
            },
            chunk(7, None),
            FlogNetKind::Done {
                id: 7,
                duration: None,
                ts: None,
            },
            FlogNetKind::Open {
                id: 7,
                url: None,
                ts: None,
            },
            send_msg(7, ""),
            recv_msg(7, ""),
            FlogNetKind::Close {
                id: 7,
                code: None,
                reason: None,
                duration: None,
                ts: None,
            },
        ];
        for v in variants {
            assert_eq!(v.id(), 7);
        }
    }
}
