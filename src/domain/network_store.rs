//! Ring-buffer storage for network entries with message processing.

use std::collections::VecDeque;

use crate::domain::network::{
    FlogNetKind, NetworkEntry, NetworkStatus, Protocol, SseChunk, WsDirection, WsMessage,
};
use crate::domain::network_timing::{NetworkTiming, TimingEvent};

// WHY 10_000: Flipper defaults its network plugin buffer to 5K. We
// chose double that because a single LLM streaming session can burn
// 1-2K SSE chunk entries in a minute, and a typical debug session
// keeps multiple conversations in history. 10K at ~1KB per entry = 10MB
// worst-case resident — still well under any reasonable RSS budget for
// a TUI dev tool, and the ring-buffer drops oldest-first so long-lived
// sessions stay bounded.
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
                timing,
            } => self.handle_res(
                id, status, duration, headers, body, size, error, mocked, timing,
            ),
            FlogNetKind::Err {
                id,
                error,
                duration,
                ts: _,
                timing,
            } => self.handle_err(id, error, duration, timing),
            FlogNetKind::Chunk {
                id,
                data,
                size,
                seq: _,
                ts: _,
                event_timing,
            } => self.handle_chunk(id, data, size, event_timing),
            FlogNetKind::Done {
                id,
                duration,
                ts: _,
                timing,
            } => self.handle_done(id, duration, timing),
            FlogNetKind::Open {
                id,
                url,
                ts,
                timing,
            } => self.handle_open(id, url, ts, timing),
            FlogNetKind::Connecting {
                id,
                url,
                ts,
                timing,
            } => self.handle_connecting(id, url, ts, timing),
            FlogNetKind::Send {
                id,
                data,
                size,
                ts: _,
                event_timing,
            } => self.handle_ws_msg(id, data, size, WsDirection::Send, event_timing),
            FlogNetKind::Recv {
                id,
                data,
                size,
                ts: _,
                event_timing,
            } => self.handle_ws_msg(id, data, size, WsDirection::Recv, event_timing),
            FlogNetKind::Close {
                id,
                code,
                reason,
                duration,
                ts: _,
                timing,
            } => self.handle_close(id, code, reason, duration, timing),
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
        timing: Option<NetworkTiming>,
    ) {
        if let Some(entry) = self.find_by_id_mut(id) {
            entry.status = NetworkStatus::Completed;
            if mocked == Some(true) {
                entry.source = crate::domain::network::EntrySource::Mocked;
            }
            entry.http_status = status;
            entry.duration = duration;
            entry.error = error;
            entry.timing = timing;
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
            let mut entry = NetworkEntry::new_orphan_response(id, status, body, duration);
            entry.timing = timing;
            self.entries.push_back(entry);
        }
    }

    fn handle_err(
        &mut self,
        id: u64,
        error: Option<String>,
        duration: Option<u64>,
        timing: Option<NetworkTiming>,
    ) {
        if let Some(entry) = self.find_by_id_mut(id) {
            entry.status = NetworkStatus::Failed;
            entry.error = error;
            entry.duration = duration;
            entry.timing = timing;
        }
    }

    fn handle_chunk(
        &mut self,
        id: u64,
        data: Option<String>,
        size: Option<u64>,
        event_timing: Option<TimingEvent>,
    ) {
        if let Some(entry) = self.find_by_id_mut(id) {
            let data = data.unwrap_or_default();
            let chunk_size = size.unwrap_or(data.len() as u64);
            entry.sse_total_size += chunk_size;
            entry.sse_chunks.push(SseChunk { data, event_timing });
        }
    }

    fn handle_done(&mut self, id: u64, duration: Option<u64>, timing: Option<NetworkTiming>) {
        if let Some(entry) = self.find_by_id_mut(id) {
            entry.status = NetworkStatus::Completed;
            entry.duration = duration;
            entry.timing = timing;
        }
    }

    fn handle_open(
        &mut self,
        id: u64,
        url: Option<String>,
        ts: Option<u64>,
        timing: Option<NetworkTiming>,
    ) {
        if let Some(entry) = self.find_by_id_mut(id) {
            // Upgrade a Pending entry created by a prior `connecting` frame.
            entry.status = NetworkStatus::Active;
            entry.timing = timing;
            if let Some(u) = url {
                if !u.is_empty() {
                    entry.url = u;
                }
            }
            if let Some(t) = ts {
                entry.timestamp = format_ts(t);
            }
        } else {
            // Backward-compat: `fromChannel` or old Dart that emits `open`
            // without a prior `connecting` frame.
            self.ensure_capacity();
            let url = url.unwrap_or_default();
            let mut entry = NetworkEntry::new_ws(id, url, String::new());
            if let Some(t) = ts {
                entry.timestamp = format_ts(t);
            }
            entry.timing = timing;
            self.entries.push_back(entry);
        }
    }

    fn handle_connecting(
        &mut self,
        id: u64,
        url: Option<String>,
        ts: Option<u64>,
        timing: Option<NetworkTiming>,
    ) {
        self.ensure_capacity();
        let url = url.filter(|u| !u.is_empty()).unwrap_or_default();
        let mut entry = NetworkEntry::new_ws(id, url, String::new());
        entry.status = NetworkStatus::Pending;
        if let Some(t) = ts {
            entry.timestamp = format_ts(t);
        }
        entry.timing = timing;
        self.entries.push_back(entry);
    }

    fn handle_ws_msg(
        &mut self,
        id: u64,
        data: Option<String>,
        size: Option<u64>,
        direction: WsDirection,
        event_timing: Option<TimingEvent>,
    ) {
        if let Some(entry) = self.find_by_id_mut(id) {
            let data = data.unwrap_or_default();
            let msg_size = size.unwrap_or(data.len() as u64);

            entry.ws_messages.push(WsMessage {
                direction,
                data,
                size: msg_size,
                event_timing,
            });
        }
    }

    fn handle_close(
        &mut self,
        id: u64,
        code: Option<u16>,
        reason: Option<String>,
        duration: Option<u64>,
        timing: Option<NetworkTiming>,
    ) {
        if let Some(entry) = self.find_by_id_mut(id) {
            entry.status = NetworkStatus::Completed;
            entry.ws_close_code = code;
            entry.ws_close_reason = reason;
            entry.duration = duration;
            entry.timing = timing;
        }
    }
}

impl Default for NetworkStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert epoch milliseconds (UTC) to local-time `HH:MM:SS.mmm`.
fn format_ts(millis: u64) -> String {
    use chrono::{Local, TimeZone};
    match Local.timestamp_millis_opt(millis as i64).single() {
        Some(dt) => dt.format("%H:%M:%S%.3f").to_string(),
        None => String::new(),
    }
}

#[cfg(test)]
#[path = "network_store_tests.rs"]
mod tests;
