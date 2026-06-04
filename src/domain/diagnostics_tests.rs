use super::*;
use crate::domain::network::{NetworkEntry, NetworkStatus, Protocol, SseChunk};
use crate::domain::{LogEntry, LogLevel};

#[test]
fn diagnostics_include_error_logs() {
    let logs = vec![LogEntry::new(
        LogLevel::Error,
        "Repo",
        "failed".to_string(),
    )];
    let items = collect_notable(&logs, &[]);
    assert_eq!(items[0].kind, "error_log");
    assert_eq!(items[0].severity, DiagnosticSeverity::Error);
    assert_eq!(items[0].evidence, vec!["log#0"]);
}

#[test]
fn diagnostics_include_failed_http_status() {
    let mut entry =
        NetworkEntry::new_http(42, "GET".to_string(), "/x".to_string(), String::new());
    entry.status = NetworkStatus::Completed;
    entry.http_status = Some(500);

    let items = collect_notable(&[], &[entry]);

    assert_eq!(items[0].kind, "http_error_status");
    assert_eq!(items[0].evidence, vec!["net#42"]);
}

#[test]
fn diagnostics_include_completed_empty_sse_merge() {
    let mut entry =
        NetworkEntry::new_sse(7, "POST".to_string(), "/sse".to_string(), String::new());
    entry.status = NetworkStatus::Completed;
    entry.sse_chunks.push(SseChunk {
        data: "{\"choices\":[{\"delta\":{\"content\":\"\"}}]}".to_string(),
    });

    let items = collect_notable(&[], &[entry]);

    assert_eq!(items[0].kind, "completed_empty_sse_merge");
    assert_eq!(items[0].severity, DiagnosticSeverity::Warning);
}

#[test]
fn diagnostics_include_abnormal_ws_close() {
    let mut entry = NetworkEntry::new_ws(9, "wss://x".to_string(), String::new());
    entry.protocol = Protocol::Ws;
    entry.ws_close_code = Some(1006);

    let items = collect_notable(&[], &[entry]);

    assert_eq!(items[0].kind, "websocket_abnormal_close");
}
