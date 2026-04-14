//! Network request data types for HTTP, SSE, and WebSocket.

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Http,
    Sse,
    Ws,
}

impl Protocol {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Http => "HTTP",
            Self::Sse => "SSE",
            Self::Ws => "WS",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkStatus {
    Pending,
    Active,
    Completed,
    Failed,
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
    #[allow(dead_code)]
    pub seq: u32,
    pub data: String,
    #[allow(dead_code)]
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct WsMessage {
    pub direction: WsDirection,
    pub data: String,
    pub size: u64,
    #[allow(dead_code)]
    pub timestamp: String,
}

#[derive(Debug, Clone)]
pub struct NetworkEntry {
    pub id: u64,
    pub protocol: Protocol,
    #[allow(dead_code)]
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
    pub fn new_http(id: u64, method: String, url: String, timestamp: String) -> Self {
        let path = extract_path(&url);
        Self {
            id,
            protocol: Protocol::Http,
            timestamp,
            method,
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
        }
    }

    pub fn new_sse(id: u64, method: String, url: String, timestamp: String) -> Self {
        let path = extract_path(&url);
        Self {
            id,
            protocol: Protocol::Sse,
            timestamp,
            method,
            url,
            path,
            status: NetworkStatus::Active,
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
        }
    }

    pub fn new_ws(id: u64, url: String, timestamp: String) -> Self {
        let path = extract_path(&url);
        Self {
            id,
            protocol: Protocol::Ws,
            timestamp,
            method: String::new(),
            url,
            path,
            status: NetworkStatus::Active,
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

#[derive(Debug, Deserialize)]
pub struct FlogNetMessage {
    pub id: u64,
    pub t: String,
    pub p: Option<String>,
    pub method: Option<String>,
    pub url: Option<String>,
    pub status: Option<u16>,
    pub duration: Option<u64>,
    pub headers: Option<serde_json::Value>,
    pub body: Option<String>,
    pub size: Option<u64>,
    pub data: Option<String>,
    pub seq: Option<u32>,
    pub chunks: Option<u32>,
    pub code: Option<u16>,
    pub reason: Option<String>,
    pub error: Option<String>,
    pub mocked: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entry_source_default_is_app() {
        let entry = NetworkEntry::new_http(1, "GET".into(), "https://example.com".into(), String::new());
        assert_eq!(entry.source, EntrySource::App);
    }

    #[test]
    fn test_entry_source_can_be_set_to_replay() {
        let mut entry = NetworkEntry::new_http(1, "GET".into(), "https://example.com".into(), String::new());
        entry.source = EntrySource::Replay;
        assert_eq!(entry.source, EntrySource::Replay);
    }

    #[test]
    fn test_entry_source_can_be_set_to_mocked() {
        let mut entry = NetworkEntry::new_http(1, "GET".into(), "https://example.com".into(), String::new());
        entry.source = EntrySource::Mocked;
        assert_eq!(entry.source, EntrySource::Mocked);
    }
}
