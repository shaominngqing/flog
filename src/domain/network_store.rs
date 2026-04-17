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
    #[allow(dead_code)]
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
            let seq = msg.seq.unwrap_or(entry.sse_chunks.len() as u32);
            entry.sse_total_size += size;
            entry.sse_chunks.push(SseChunk { seq, data, size });
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

            entry.ws_messages.push(WsMessage {
                direction,
                data,
                size,
                timestamp: String::new(),
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

    #[test]
    fn test_push_entry() {
        let mut store = NetworkStore::new();
        let mut entry = NetworkEntry::new_http(999, "GET".into(), "https://test.com".into(), String::new());
        entry.source = EntrySource::Replay;

        store.push_entry(entry);
        assert_eq!(store.len(), 1);
        assert_eq!(store.get(0).unwrap().source, EntrySource::Replay);
        assert_eq!(store.get(0).unwrap().id, 999);
    }
}
