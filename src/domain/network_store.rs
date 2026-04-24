//! Ring-buffer storage for network entries with message processing.

use std::collections::VecDeque;

use crate::domain::network::{
    FlogNetMessage, NetworkEntry, NetworkStatus, Protocol, SseChunk, WsDirection, WsMessage,
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

    pub fn process_message(&mut self, msg: FlogNetMessage) {
        match msg.t.as_str() {
            "req" => self.handle_req(msg),
            "res" => self.handle_res(msg),
            "err" => self.handle_err(msg),
            "chunk" => self.handle_chunk(msg),
            "done" => self.handle_done(msg),
            "open" => self.handle_open(msg),
            "send" => self.handle_ws_msg(msg, WsDirection::Send),
            "recv" => self.handle_ws_msg(msg, WsDirection::Recv),
            "close" => self.handle_close(msg),
            _ => {}
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

    fn handle_req(&mut self, msg: FlogNetMessage) {
        self.ensure_capacity();

        let method = msg.method.unwrap_or_default();
        let url = msg.url.unwrap_or_default();
        let protocol = match msg.p.as_deref() {
            Some("sse") => Protocol::Sse,
            Some("ws") => Protocol::Ws,
            _ => Protocol::Http,
        };

        let mut entry = match protocol {
            Protocol::Http => NetworkEntry::new_http(msg.id, method, url, String::new()),
            Protocol::Sse => NetworkEntry::new_sse(msg.id, method, url, String::new()),
            Protocol::Ws => NetworkEntry::new_ws(msg.id, url, String::new()),
        };

        if let Some(headers) = msg.headers {
            entry.request_headers = Some(headers.to_string());
        }
        if let Some(body) = msg.body {
            entry.request_size = Some(body.len() as u64);
            entry.request_body = Some(body);
        }
        if let Some(size) = msg.size {
            entry.request_size = Some(size);
        }
        if let Some(ts) = msg.ts {
            entry.timestamp = format_ts(ts);
        }

        self.entries.push_back(entry);
    }

    fn handle_res(&mut self, msg: FlogNetMessage) {
        if let Some(entry) = self.find_by_id_mut(msg.id) {
            entry.status = NetworkStatus::Completed;
            if msg.mocked == Some(true) {
                entry.source = crate::domain::network::EntrySource::Mocked;
            }
            entry.http_status = msg.status;
            entry.duration = msg.duration;
            entry.error = msg.error;
            if let Some(headers) = msg.headers {
                entry.response_headers = Some(headers.to_string());
            }
            if let Some(body) = msg.body {
                entry.response_size = Some(body.len() as u64);
                entry.response_body = Some(body);
            }
            if let Some(size) = msg.size {
                entry.response_size = Some(size);
            }
        } else {
            // DOM-003 fix: a Response arrived with no matching Request — don't
            // drop it silently. Surface as an Orphan entry so the user can see
            // that data was received without context.
            self.ensure_capacity();
            let entry =
                NetworkEntry::new_orphan_response(msg.id, msg.status, msg.body, msg.duration);
            self.entries.push_back(entry);
        }
    }

    fn handle_err(&mut self, msg: FlogNetMessage) {
        if let Some(entry) = self.find_by_id_mut(msg.id) {
            entry.status = NetworkStatus::Failed;
            entry.error = msg.error;
            entry.duration = msg.duration;
        }
    }

    fn handle_chunk(&mut self, msg: FlogNetMessage) {
        if let Some(entry) = self.find_by_id_mut(msg.id) {
            let data = msg.data.unwrap_or_default();
            let size = msg.size.unwrap_or(data.len() as u64);
            entry.sse_total_size += size;
            // DOM-025: seq/size/ts are accepted on the wire but dropped at
            // ingest — no UI consumer reads them.
            entry.sse_chunks.push(SseChunk { data });
        }
    }

    fn handle_done(&mut self, msg: FlogNetMessage) {
        if let Some(entry) = self.find_by_id_mut(msg.id) {
            entry.status = NetworkStatus::Completed;
            entry.duration = msg.duration;
        }
    }

    fn handle_open(&mut self, msg: FlogNetMessage) {
        self.ensure_capacity();

        let url = msg.url.unwrap_or_default();
        let mut entry = NetworkEntry::new_ws(msg.id, url, String::new());
        if let Some(ts) = msg.ts {
            entry.timestamp = format_ts(ts);
        }
        self.entries.push_back(entry);
    }

    fn handle_ws_msg(&mut self, msg: FlogNetMessage, direction: WsDirection) {
        if let Some(entry) = self.find_by_id_mut(msg.id) {
            let data = msg.data.unwrap_or_default();
            let size = msg.size.unwrap_or(data.len() as u64);

            // DOM-025: per-message timestamp is accepted on the wire but
            // not stored — no UI consumer reads it.
            entry.ws_messages.push(WsMessage {
                direction,
                data,
                size,
            });
        }
    }

    fn handle_close(&mut self, msg: FlogNetMessage) {
        if let Some(entry) = self.find_by_id_mut(msg.id) {
            entry.status = NetworkStatus::Completed;
            entry.ws_close_code = msg.code;
            entry.ws_close_reason = msg.reason;
            entry.duration = msg.duration;
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

    // Helper: construct a default FlogNetMessage with id and type set.
    fn msg(id: u64, t: &str) -> FlogNetMessage {
        FlogNetMessage {
            id,
            t: t.to_string(),
            p: None,
            method: None,
            url: None,
            status: None,
            duration: None,
            headers: None,
            body: None,
            size: None,
            data: None,
            seq: None,
            chunks: None,
            code: None,
            reason: None,
            error: None,
            mocked: None,
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
        let mut m = msg(1, "req");
        m.method = Some("GET".into());
        m.url = Some("https://x.com".into());
        store.process_message(m);
        assert_eq!(store.len(), 1);
        store.clear();
        assert!(store.is_empty());
    }

    #[test]
    fn iter_yields_entries_in_order() {
        let mut store = NetworkStore::new();
        for id in 1..=3 {
            let mut m = msg(id, "req");
            m.method = Some("GET".into());
            m.url = Some(format!("https://x.com/{}", id));
            store.process_message(m);
        }
        let ids: Vec<u64> = store.iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![1, 2, 3]);
    }

    // ---- DOM-002: state machine for FlogNetMessage --------------------
    // Current behavior: no transition validation. Each test locks a
    // specific "bad transition" outcome so Phase 3 changes are visible.

    #[test]
    fn dom_002_res_without_req_surfaces_orphan_entry_a() {
        // Phase 3 DOM-003: an orphan Response is no longer silently dropped;
        // it is pushed as a placeholder entry with NetworkStatus::Orphan.
        let mut store = NetworkStore::new();
        let mut m = msg(42, "res");
        m.status = Some(200);
        m.body = Some("orphan".into());
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
        let mut m = msg(42, "err");
        m.error = Some("boom".into());
        store.process_message(m);
        assert!(store.is_empty());
    }

    #[test]
    fn dom_002_chunk_without_req_drops_silently_c() {
        let mut store = NetworkStore::new();
        let mut m = msg(42, "chunk");
        m.data = Some("hi".into());
        store.process_message(m);
        assert!(store.is_empty());
    }

    #[test]
    fn dom_002_done_without_req_drops_silently_d() {
        let mut store = NetworkStore::new();
        store.process_message(msg(42, "done"));
        assert!(store.is_empty());
    }

    #[test]
    fn dom_002_close_without_open_drops_silently_e() {
        let mut store = NetworkStore::new();
        store.process_message(msg(42, "close"));
        assert!(store.is_empty());
    }

    #[test]
    fn dom_002_ws_send_without_open_drops_silently_f() {
        let mut store = NetworkStore::new();
        let mut m = msg(42, "send");
        m.data = Some("hi".into());
        store.process_message(m);
        assert!(store.is_empty());
    }

    #[test]
    fn dom_002_ws_recv_without_open_drops_silently_g() {
        let mut store = NetworkStore::new();
        let mut m = msg(42, "recv");
        m.data = Some("hi".into());
        store.process_message(m);
        assert!(store.is_empty());
    }

    #[test]
    fn dom_002_second_req_with_same_id_creates_new_entry() {
        // Current behavior: handle_req always pushes a new entry.
        let mut store = NetworkStore::new();
        let mut m1 = msg(1, "req");
        m1.method = Some("GET".into());
        m1.url = Some("https://x.com".into());
        store.process_message(m1);

        let mut m2 = msg(1, "req");
        m2.method = Some("POST".into());
        m2.url = Some("https://y.com".into());
        store.process_message(m2);

        assert_eq!(store.len(), 2);
    }

    #[test]
    fn dom_002_chunk_on_http_protocol_still_appends_to_sse_chunks() {
        // Current behavior: find_by_id_mut locates the HTTP entry, chunk
        // is appended to its sse_chunks field. This is arguably buggy per
        // audit but is locked as current behavior.
        let mut store = NetworkStore::new();
        let mut m1 = msg(1, "req");
        m1.method = Some("GET".into());
        m1.url = Some("https://x.com".into());
        store.process_message(m1);

        let mut m2 = msg(1, "chunk");
        m2.data = Some("stream-like".into());
        store.process_message(m2);

        let entry = store.get(0).unwrap();
        assert_eq!(entry.protocol, Protocol::Http);
        assert_eq!(entry.sse_chunks.len(), 1);
    }

    #[test]
    fn dom_002_unknown_message_type_ignored() {
        let mut store = NetworkStore::new();
        store.process_message(msg(1, "unknown_type"));
        assert!(store.is_empty());
    }

    // ---- DOM-006: FlogNetMessage loose typing -------------------------
    // Lock current handling of optional fields at the handle_* boundary.

    #[test]
    fn dom_006_req_missing_method_defaults_to_empty() {
        let mut store = NetworkStore::new();
        let mut m = msg(1, "req");
        m.url = Some("https://x.com".into());
        store.process_message(m);
        assert_eq!(store.get(0).unwrap().method, "");
    }

    #[test]
    fn dom_006_req_missing_url_defaults_to_empty() {
        let mut store = NetworkStore::new();
        let mut m = msg(1, "req");
        m.method = Some("GET".into());
        store.process_message(m);
        assert_eq!(store.get(0).unwrap().url, "");
    }

    #[test]
    fn dom_006_req_unknown_protocol_defaults_to_http() {
        let mut store = NetworkStore::new();
        let mut m = msg(1, "req");
        m.p = Some("magic".into());
        m.url = Some("https://x.com".into());
        store.process_message(m);
        assert_eq!(store.get(0).unwrap().protocol, Protocol::Http);
    }

    #[test]
    fn dom_006_req_no_protocol_defaults_to_http() {
        let mut store = NetworkStore::new();
        let mut m = msg(1, "req");
        m.url = Some("https://x.com".into());
        store.process_message(m);
        assert_eq!(store.get(0).unwrap().protocol, Protocol::Http);
    }

    #[test]
    fn dom_006_req_sse_protocol() {
        let mut store = NetworkStore::new();
        let mut m = msg(1, "req");
        m.p = Some("sse".into());
        m.url = Some("https://x.com/stream".into());
        m.method = Some("GET".into());
        store.process_message(m);
        assert_eq!(store.get(0).unwrap().protocol, Protocol::Sse);
    }

    #[test]
    fn dom_006_req_ws_protocol() {
        let mut store = NetworkStore::new();
        let mut m = msg(1, "req");
        m.p = Some("ws".into());
        m.url = Some("wss://x.com".into());
        store.process_message(m);
        assert_eq!(store.get(0).unwrap().protocol, Protocol::Ws);
    }

    // ---- Full happy-path flows ---------------------------------------

    #[test]
    fn handle_req_stores_headers_body_and_timestamp() {
        let mut store = NetworkStore::new();
        let mut m = msg(1, "req");
        m.method = Some("POST".into());
        m.url = Some("https://x.com/api".into());
        m.headers = Some(serde_json::json!({"Content-Type": "application/json"}));
        m.body = Some("{\"a\":1}".into());
        m.ts = Some(1_700_000_000_000);
        store.process_message(m);

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
        let mut m = msg(1, "req");
        m.method = Some("POST".into());
        m.url = Some("https://x.com".into());
        m.body = Some("abc".into());
        m.size = Some(999);
        store.process_message(m);
        // size after body is applied last → 999
        assert_eq!(store.get(0).unwrap().request_size, Some(999));
    }

    #[test]
    fn handle_res_updates_matched_entry() {
        let mut store = NetworkStore::new();
        let mut m1 = msg(1, "req");
        m1.method = Some("GET".into());
        m1.url = Some("https://x.com".into());
        store.process_message(m1);

        let mut m2 = msg(1, "res");
        m2.status = Some(200);
        m2.duration = Some(50);
        m2.headers = Some(serde_json::json!({"X-Test": "1"}));
        m2.body = Some("ok".into());
        store.process_message(m2);

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
        let mut m1 = msg(1, "req");
        m1.method = Some("GET".into());
        m1.url = Some("https://x.com".into());
        store.process_message(m1);

        let mut m2 = msg(1, "res");
        m2.status = Some(200);
        m2.body = Some("ok".into());
        m2.size = Some(10_000);
        store.process_message(m2);
        assert_eq!(store.get(0).unwrap().response_size, Some(10_000));
    }

    #[test]
    fn handle_res_mocked_flag_sets_source() {
        let mut store = NetworkStore::new();
        let mut m1 = msg(1, "req");
        m1.method = Some("GET".into());
        m1.url = Some("https://x.com".into());
        store.process_message(m1);

        let mut m2 = msg(1, "res");
        m2.status = Some(200);
        m2.mocked = Some(true);
        store.process_message(m2);
        assert_eq!(store.get(0).unwrap().source, EntrySource::Mocked);
    }

    #[test]
    fn handle_res_mocked_false_keeps_app_source() {
        let mut store = NetworkStore::new();
        let mut m1 = msg(1, "req");
        m1.method = Some("GET".into());
        m1.url = Some("https://x.com".into());
        store.process_message(m1);

        let mut m2 = msg(1, "res");
        m2.status = Some(200);
        m2.mocked = Some(false);
        store.process_message(m2);
        assert_eq!(store.get(0).unwrap().source, EntrySource::App);
    }

    #[test]
    fn handle_err_sets_failed_status() {
        let mut store = NetworkStore::new();
        let mut m1 = msg(1, "req");
        m1.method = Some("GET".into());
        m1.url = Some("https://x.com".into());
        store.process_message(m1);

        let mut m2 = msg(1, "err");
        m2.error = Some("timeout".into());
        m2.duration = Some(30_000);
        store.process_message(m2);

        let e = store.get(0).unwrap();
        assert_eq!(e.status, NetworkStatus::Failed);
        assert_eq!(e.error.as_ref().unwrap(), "timeout");
        assert_eq!(e.duration, Some(30_000));
    }

    #[test]
    fn handle_chunk_appends_to_sse_chunks() {
        let mut store = NetworkStore::new();
        let mut m1 = msg(1, "req");
        m1.p = Some("sse".into());
        m1.method = Some("GET".into());
        m1.url = Some("https://x.com/stream".into());
        store.process_message(m1);

        let mut m2 = msg(1, "chunk");
        m2.data = Some("hello".into());
        m2.seq = Some(0);
        m2.size = Some(5);
        store.process_message(m2);

        let mut m3 = msg(1, "chunk");
        m3.data = Some(" world".into());
        m3.seq = Some(1);
        store.process_message(m3);

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
        let mut m1 = msg(1, "req");
        m1.p = Some("sse".into());
        m1.url = Some("https://x.com".into());
        store.process_message(m1);

        let mut m2 = msg(1, "chunk");
        m2.data = Some("a".into());
        store.process_message(m2);
        let mut m3 = msg(1, "chunk");
        m3.data = Some("b".into());
        store.process_message(m3);

        let chunks = &store.get(0).unwrap().sse_chunks;
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].data, "a");
        assert_eq!(chunks[1].data, "b");
    }

    #[test]
    fn handle_chunk_defaults_data_to_empty_string() {
        let mut store = NetworkStore::new();
        let mut m1 = msg(1, "req");
        m1.p = Some("sse".into());
        m1.url = Some("https://x.com".into());
        store.process_message(m1);

        let m2 = msg(1, "chunk");
        store.process_message(m2);
        let e = store.get(0).unwrap();
        assert_eq!(e.sse_chunks[0].data, "");
    }

    #[test]
    fn handle_done_completes_entry() {
        let mut store = NetworkStore::new();
        let mut m1 = msg(1, "req");
        m1.p = Some("sse".into());
        m1.url = Some("https://x.com".into());
        store.process_message(m1);

        let mut m2 = msg(1, "done");
        m2.duration = Some(500);
        store.process_message(m2);
        let e = store.get(0).unwrap();
        assert_eq!(e.status, NetworkStatus::Completed);
        assert_eq!(e.duration, Some(500));
    }

    #[test]
    fn handle_open_creates_ws_entry() {
        let mut store = NetworkStore::new();
        let mut m = msg(1, "open");
        m.url = Some("wss://x.com/ws".into());
        m.ts = Some(1_700_000_000_000);
        store.process_message(m);
        let e = store.get(0).unwrap();
        assert_eq!(e.protocol, Protocol::Ws);
        assert_eq!(e.url, "wss://x.com/ws");
        assert_eq!(e.timestamp.len(), 12);
    }

    #[test]
    fn handle_open_missing_url_defaults_to_empty() {
        let mut store = NetworkStore::new();
        let m = msg(1, "open");
        store.process_message(m);
        assert_eq!(store.get(0).unwrap().url, "");
    }

    #[test]
    fn handle_send_and_recv_append_ws_messages() {
        let mut store = NetworkStore::new();
        let mut m0 = msg(1, "open");
        m0.url = Some("wss://x.com".into());
        store.process_message(m0);

        let mut m1 = msg(1, "send");
        m1.data = Some("ping".into());
        store.process_message(m1);

        let mut m2 = msg(1, "recv");
        m2.data = Some("pong".into());
        store.process_message(m2);

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
        let mut m0 = msg(1, "open");
        m0.url = Some("wss://x.com".into());
        store.process_message(m0);

        let mut m1 = msg(1, "send");
        m1.data = Some("abc".into());
        m1.size = Some(100);
        store.process_message(m1);

        assert_eq!(store.get(0).unwrap().ws_messages[0].size, 100);
    }

    #[test]
    fn handle_close_sets_close_fields() {
        let mut store = NetworkStore::new();
        let mut m0 = msg(1, "open");
        m0.url = Some("wss://x.com".into());
        store.process_message(m0);

        let mut m1 = msg(1, "close");
        m1.code = Some(1000);
        m1.reason = Some("Normal".into());
        m1.duration = Some(5_000);
        store.process_message(m1);

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
        let mut m = msg(9_999_999, "req");
        m.method = Some("GET".into());
        m.url = Some("/overflow".into());
        store.process_message(m);
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
        let mut m1 = msg(1, "req");
        m1.method = Some("GET".into());
        m1.url = Some("https://a".into());
        store.process_message(m1);

        let mut m2 = msg(1, "req");
        m2.method = Some("GET".into());
        m2.url = Some("https://b".into());
        store.process_message(m2);

        let mut r = msg(1, "res");
        r.status = Some(200);
        store.process_message(r);

        // First entry still Pending, second Completed
        assert_eq!(store.get(0).unwrap().status, NetworkStatus::Pending);
        assert_eq!(store.get(1).unwrap().status, NetworkStatus::Completed);
    }
}
