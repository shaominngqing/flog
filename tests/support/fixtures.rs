//! Test data factories. Every field is spelled out by design so that
//! Phase 3 adding a field to `LogEntry` / `NetworkEntry` breaks these
//! fixtures — at which point update them, don't introduce Default impls.
#![allow(dead_code)]

use flog::domain::entry::{InputSource, LogEntry, LogLevel};
use flog::domain::network::{
    EntrySource, FlogNetKind, NetworkEntry, NetworkStatus, SseChunk, WsDirection, WsMessage,
};

// ---- LogEntry factories -----------------------------------------------------

fn base_log(level: LogLevel, tag: &str, message: &str) -> LogEntry {
    LogEntry {
        timestamp: "12:00:00.000".to_string(),
        level,
        tag: tag.to_string(),
        message: message.to_string(),
        extra_lines: Vec::new(),
        repeat_count: 1,
        source: InputSource::DirectSocket,
        error: None,
        stacktrace: None,
    }
}

/// Minimal INFO LogEntry.
pub fn info(tag: &str, message: &str) -> LogEntry {
    base_log(LogLevel::Info, tag, message)
}

pub fn debug(tag: &str, message: &str) -> LogEntry {
    base_log(LogLevel::Debug, tag, message)
}

pub fn warn(tag: &str, message: &str) -> LogEntry {
    base_log(LogLevel::Warning, tag, message)
}

pub fn error(tag: &str, message: &str) -> LogEntry {
    base_log(LogLevel::Error, tag, message)
}

pub fn verbose(tag: &str, message: &str) -> LogEntry {
    base_log(LogLevel::Verbose, tag, message)
}

pub fn with_stack(tag: &str, message: &str, err: &str, stack: &str) -> LogEntry {
    LogEntry {
        error: Some(err.to_string()),
        stacktrace: Some(stack.to_string()),
        ..base_log(LogLevel::Error, tag, message)
    }
}

/// Conventional visual separator entry used by the log store in some flows.
pub fn separator() -> LogEntry {
    base_log(LogLevel::System, "────", "")
}

// ---- NetworkEntry factories -------------------------------------------------

fn base_http(id: u64, method: &str, url: &str) -> NetworkEntry {
    NetworkEntry::new_http(
        id,
        method.to_string(),
        url.to_string(),
        "12:00:00.000".to_string(),
    )
}

/// Completed HTTP GET with status 200.
pub fn http_get_200(id: u64, url: &str) -> NetworkEntry {
    let mut e = base_http(id, "GET", url);
    e.status = NetworkStatus::Completed;
    e.http_status = Some(200);
    e.duration = Some(42);
    e.response_size = Some(128);
    e
}

/// Failed HTTP POST with status 500.
pub fn http_post_500(id: u64, url: &str) -> NetworkEntry {
    let mut e = base_http(id, "POST", url);
    e.status = NetworkStatus::Completed;
    e.http_status = Some(500);
    e.duration = Some(120);
    e.request_size = Some(64);
    e.response_size = Some(256);
    e
}

/// Pending HTTP request (no response yet).
pub fn http_pending(id: u64, url: &str, method: &str) -> NetworkEntry {
    let mut e = base_http(id, method, url);
    e.status = NetworkStatus::Pending;
    e
}

/// Active SSE entry.
pub fn sse_entry(id: u64, url: &str) -> NetworkEntry {
    NetworkEntry::new_sse(
        id,
        "GET".to_string(),
        url.to_string(),
        "12:00:00.000".to_string(),
    )
}

/// Active WS entry.
pub fn ws_entry(id: u64, url: &str) -> NetworkEntry {
    NetworkEntry::new_ws(id, url.to_string(), "12:00:00.000".to_string())
}

pub fn sse_chunk(_seq: u32, data: &str) -> SseChunk {
    // Phase 3 DOM-025: seq/size are dropped from the storage type. The
    // `_seq` parameter is kept to avoid churn at call sites.
    SseChunk {
        data: data.to_string(),
        event_timing: None,
    }
}

pub fn ws_send(data: &str) -> WsMessage {
    WsMessage {
        direction: WsDirection::Send,
        size: data.len() as u64,
        data: data.to_string(),
        event_timing: None,
    }
}

pub fn ws_recv(data: &str) -> WsMessage {
    WsMessage {
        direction: WsDirection::Recv,
        size: data.len() as u64,
        data: data.to_string(),
        event_timing: None,
    }
}

/// Attach an `EntrySource` to an entry (helper to avoid one-line clones
/// at call sites).
pub fn with_source(mut entry: NetworkEntry, source: EntrySource) -> NetworkEntry {
    entry.source = source;
    entry
}

// ---- FlogNetKind factories ------------------------------------------------
// Phase 3 DOM-002/006 replaced the FlogNetMessage loose-bag struct with
// a typed enum. These helpers build the Req/Res/Chunk variants used by
// the integration tests.

/// Request-start message (HTTP protocol).
pub fn net_req(id: u64, method: &str, url: &str) -> FlogNetKind {
    FlogNetKind::Req {
        id,
        p: Some("http".to_string()),
        method: Some(method.to_string()),
        url: Some(url.to_string()),
        headers: None,
        body: None,
        size: None,
        ts: None,
    }
}

/// Response message.
pub fn net_res(id: u64, status: u16) -> FlogNetKind {
    FlogNetKind::Res {
        id,
        status: Some(status),
        duration: Some(42),
        headers: None,
        body: None,
        size: None,
        error: None,
        mocked: None,
        timing: None,
        ts: None,
    }
}

/// SSE chunk.
pub fn net_chunk_sse(id: u64, seq: u32, data: &str) -> FlogNetKind {
    FlogNetKind::Chunk {
        id,
        data: Some(data.to_string()),
        size: Some(data.len() as u64),
        seq: Some(seq),
        event_timing: None,
        ts: None,
    }
}
