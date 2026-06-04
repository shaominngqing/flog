//! Pure AI-oriented diagnostics over logs and network entries.
//!
//! This module produces stable evidence ids for command-layer JSON. It
//! deliberately stays UI-agnostic.

use serde::Serialize;

use crate::domain::entry::{LogEntry, LogLevel};
use crate::domain::network::{NetworkEntry, NetworkStatus, Protocol};
use crate::domain::sse_merge;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NotableDiagnostic {
    pub id: String,
    pub severity: DiagnosticSeverity,
    pub kind: String,
    pub message: String,
    pub evidence: Vec<String>,
    pub next_actions: Vec<String>,
}

pub fn collect_notable(logs: &[LogEntry], network: &[NetworkEntry]) -> Vec<NotableDiagnostic> {
    let mut out = Vec::new();
    for (idx, log) in logs.iter().enumerate() {
        if matches!(log.level, LogLevel::Error) {
            out.push(item(
                format!("diag#log-error-{idx}"),
                DiagnosticSeverity::Error,
                "error_log",
                format!("Error log from tag {}", log.tag),
                vec![format!("log#{idx}")],
            ));
        } else if matches!(log.level, LogLevel::Warning) {
            out.push(item(
                format!("diag#log-warning-{idx}"),
                DiagnosticSeverity::Warning,
                "warning_log",
                format!("Warning log from tag {}", log.tag),
                vec![format!("log#{idx}")],
            ));
        }
    }

    for entry in network {
        if matches!(entry.status, NetworkStatus::Failed) {
            out.push(item(
                format!("diag#net-failed-{}", entry.id),
                DiagnosticSeverity::Error,
                "network_failed",
                format!("Network request failed: {}", entry.url),
                vec![format!("net#{}", entry.id)],
            ));
        }
        if matches!(entry.status, NetworkStatus::Orphan) {
            out.push(item(
                format!("diag#net-orphan-{}", entry.id),
                DiagnosticSeverity::Warning,
                "orphan_response",
                "Response arrived without a matching request".to_string(),
                vec![format!("net#{}", entry.id)],
            ));
        }
        if let Some(status) = entry.http_status {
            if status >= 400 {
                out.push(item(
                    format!("diag#http-status-{}", entry.id),
                    DiagnosticSeverity::Error,
                    "http_error_status",
                    format!("HTTP request returned status {status}"),
                    vec![format!("net#{}", entry.id)],
                ));
            }
        }
        if entry.protocol == Protocol::Sse {
            append_sse_diagnostics(&mut out, entry);
        }
        if entry.protocol == Protocol::Ws {
            append_ws_diagnostics(&mut out, entry);
        }
    }
    out
}

fn append_sse_diagnostics(out: &mut Vec<NotableDiagnostic>, entry: &NetworkEntry) {
    if entry.status == NetworkStatus::Active && !entry.sse_chunks.is_empty() {
        out.push(item(
            format!("diag#sse-active-{}", entry.id),
            DiagnosticSeverity::Info,
            "active_sse_with_chunks",
            "SSE stream has chunks but did not finish during collection".to_string(),
            vec![format!("net#{}", entry.id)],
        ));
    }

    if entry.status == NetworkStatus::Completed && !entry.sse_chunks.is_empty() {
        let chunk_data = entry
            .sse_chunks
            .iter()
            .map(|chunk| chunk.data.as_str())
            .collect::<Vec<_>>();
        let Some((path, _display)) = sse_merge::auto_detect_field(&chunk_data) else {
            return;
        };
        let merged = sse_merge::merge_field(&chunk_data, &path);
        if merged.trim().is_empty() {
            out.push(item(
                format!("diag#sse-empty-{}", entry.id),
                DiagnosticSeverity::Warning,
                "completed_empty_sse_merge",
                "SSE completed but the auto-detected merged text is empty".to_string(),
                vec![format!("net#{}", entry.id)],
            ));
        }
    }
}

fn append_ws_diagnostics(out: &mut Vec<NotableDiagnostic>, entry: &NetworkEntry) {
    if let Some(code) = entry.ws_close_code {
        if code != 1000 && code != 1001 {
            out.push(item(
                format!("diag#ws-close-{}", entry.id),
                DiagnosticSeverity::Warning,
                "websocket_abnormal_close",
                format!("WebSocket closed with abnormal code {code}"),
                vec![format!("net#{}", entry.id)],
            ));
        }
    }
}

fn item(
    id: String,
    severity: DiagnosticSeverity,
    kind: &str,
    message: String,
    evidence: Vec<String>,
) -> NotableDiagnostic {
    NotableDiagnostic {
        id,
        severity,
        kind: kind.to_string(),
        message,
        evidence,
        next_actions: Vec::new(),
    }
}

#[cfg(test)]
#[path = "diagnostics_tests.rs"]
mod tests;
