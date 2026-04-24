//! Test data factories. Every field is spelled out by design so that
//! Phase 3 adding a field to `LogEntry` / `NetworkEntry` breaks these
//! fixtures — at which point update them, don't introduce Default impls.
#![allow(dead_code)]

use flog::domain::entry::{InputSource, LogEntry, LogLevel};
use flog::domain::network::{
    EntrySource, FlogNetMessage, NetworkEntry, NetworkStatus, SseChunk, WsDirection, WsMessage,
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
    }
}

pub fn ws_send(data: &str) -> WsMessage {
    WsMessage {
        direction: WsDirection::Send,
        size: data.len() as u64,
        data: data.to_string(),
    }
}

pub fn ws_recv(data: &str) -> WsMessage {
    WsMessage {
        direction: WsDirection::Recv,
        size: data.len() as u64,
        data: data.to_string(),
    }
}

/// Attach an `EntrySource` to an entry (helper to avoid one-line clones
/// at call sites).
pub fn with_source(mut entry: NetworkEntry, source: EntrySource) -> NetworkEntry {
    entry.source = source;
    entry
}

// ---- FlogNetMessage factories ----------------------------------------------

fn empty_flog_net() -> FlogNetMessage {
    FlogNetMessage {
        id: 0,
        t: String::new(),
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

/// Request-start FlogNetMessage.
pub fn net_req(id: u64, method: &str, url: &str) -> FlogNetMessage {
    FlogNetMessage {
        id,
        t: "req".to_string(),
        p: Some("http".to_string()),
        method: Some(method.to_string()),
        url: Some(url.to_string()),
        ..empty_flog_net()
    }
}

/// Response FlogNetMessage.
pub fn net_res(id: u64, status: u16) -> FlogNetMessage {
    FlogNetMessage {
        id,
        t: "res".to_string(),
        status: Some(status),
        duration: Some(42),
        ..empty_flog_net()
    }
}

/// SSE chunk FlogNetMessage.
pub fn net_chunk_sse(id: u64, seq: u32, data: &str) -> FlogNetMessage {
    FlogNetMessage {
        id,
        t: "chunk".to_string(),
        p: Some("sse".to_string()),
        seq: Some(seq),
        data: Some(data.to_string()),
        size: Some(data.len() as u64),
        ..empty_flog_net()
    }
}
