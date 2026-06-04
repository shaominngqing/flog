use crate::app::App;
use crate::domain::diagnostics::collect_notable;
use crate::domain::network::{NetworkEntry, NetworkStatus, Protocol};
use crate::domain::{LogEntry, LogLevel};

use super::output::{CollectionMeta, SnapshotPayload, Summary};
use super::redact::{preview_text, redact_json_value, redact_text_patterns};

#[derive(Debug, Clone)]
pub struct SnapshotBuildOptions {
    pub last: usize,
    pub include_headers: bool,
    pub include_body: bool,
    pub redact: bool,
    pub ports_scanned: Vec<u16>,
    pub wait_ms: u64,
    pub settle_ms: u64,
    pub complete: bool,
    pub warnings: Vec<String>,
}

pub fn build_snapshot(app: &App, options: SnapshotBuildOptions) -> SnapshotPayload {
    let logs_all = app.store.iter().cloned().collect::<Vec<_>>();
    let network_all = app.network_store.iter().cloned().collect::<Vec<_>>();
    let logs_slice = tail(&logs_all, options.last);
    let network = network_all
        .iter()
        .map(|entry| network_value(entry, &options))
        .collect();

    let notable = collect_notable(&logs_all, &network_all)
        .into_iter()
        .filter_map(|item| serde_json::to_value(item).ok())
        .collect();

    SnapshotPayload {
        app: None,
        collection: CollectionMeta {
            ports_scanned: options.ports_scanned,
            wait_ms: options.wait_ms,
            settle_ms: options.settle_ms,
            complete: options.complete,
            warnings: options.warnings,
        },
        summary: summarize(&logs_all, &network_all),
        notable,
        logs: logs_slice
            .iter()
            .enumerate()
            .map(|(offset, log)| {
                let absolute = logs_all.len().saturating_sub(logs_slice.len()) + offset;
                log_value(absolute, log)
            })
            .collect(),
        network,
        screenshot: None,
        diagnostics: Vec::new(),
    }
}

fn summarize(logs: &[LogEntry], network: &[NetworkEntry]) -> Summary {
    Summary {
        logs: logs.len(),
        errors: logs
            .iter()
            .filter(|log| matches!(log.level, LogLevel::Error))
            .count(),
        warnings: logs
            .iter()
            .filter(|log| matches!(log.level, LogLevel::Warning))
            .count(),
        network: network.len(),
        failed_requests: network
            .iter()
            .filter(|entry| {
                matches!(entry.status, NetworkStatus::Failed)
                    || entry.http_status.is_some_and(|status| status >= 400)
            })
            .count(),
        active_sse: network
            .iter()
            .filter(|entry| {
                entry.protocol == Protocol::Sse && entry.status == NetworkStatus::Active
            })
            .count(),
        websockets: network
            .iter()
            .filter(|entry| entry.protocol == Protocol::Ws)
            .count(),
    }
}

fn log_value(index: usize, log: &LogEntry) -> serde_json::Value {
    serde_json::json!({
        "id": format!("log#{index}"),
        "timestamp": log.timestamp,
        "level": log.level.as_str(),
        "tag": log.tag,
        "message": log.message,
        "stacktrace": log.stacktrace.as_ref().map(|s| preview_text(s, 800)),
        "repeat_count": log.repeat_count,
    })
}

fn network_value(entry: &NetworkEntry, options: &SnapshotBuildOptions) -> serde_json::Value {
    serde_json::json!({
        "id": format!("net#{}", entry.id),
        "protocol": protocol_label(entry.protocol),
        "method": entry.method,
        "url": entry.url,
        "status": entry.http_status,
        "network_status": status_label(entry.status),
        "duration_ms": entry.duration,
        "request": {
            "headers": headers_value(entry.request_headers.as_deref(), options.include_headers, options.redact),
            "body": body_value(entry.request_body.as_deref(), options.include_body, options.redact),
        },
        "response": {
            "headers": headers_value(entry.response_headers.as_deref(), options.include_headers, options.redact),
            "body": body_value(entry.response_body.as_deref(), options.include_body, options.redact),
        },
        "sse": if entry.protocol == Protocol::Sse {
            serde_json::json!({"chunks": entry.sse_chunks.len()})
        } else {
            serde_json::Value::Null
        },
    })
}

fn tail<T>(items: &[T], limit: usize) -> &[T] {
    let start = items.len().saturating_sub(limit);
    &items[start..]
}

fn protocol_label(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::Http => "http",
        Protocol::Sse => "sse",
        Protocol::Ws => "ws",
    }
}

fn status_label(status: NetworkStatus) -> &'static str {
    match status {
        NetworkStatus::Pending => "pending",
        NetworkStatus::Active => "active",
        NetworkStatus::Completed => "completed",
        NetworkStatus::Failed => "failed",
        NetworkStatus::Orphan => "orphan",
    }
}

fn headers_value(headers: Option<&str>, include: bool, redact: bool) -> serde_json::Value {
    if !include {
        return serde_json::Value::String("redacted".to_string());
    }
    let Some(headers) = headers else {
        return serde_json::Value::Null;
    };
    let value = serde_json::from_str(headers).unwrap_or(serde_json::Value::Null);
    if redact {
        redact_json_value(&value)
    } else {
        value
    }
}

fn body_value(body: Option<&str>, include: bool, redact: bool) -> serde_json::Value {
    let Some(body) = body else {
        return serde_json::json!({"present": false});
    };
    if !include {
        return serde_json::json!({"present": true});
    }

    let body = if redact {
        redact_body(body)
    } else {
        body.to_string()
    };
    serde_json::to_value(preview_text(&body, 1200)).unwrap_or(serde_json::Value::Null)
}

fn redact_body(body: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(body) {
        Ok(value) => redact_json_value(&value).to_string(),
        Err(_) => redact_text_patterns(body),
    }
}

impl SnapshotBuildOptions {
    #[cfg(test)]
    pub fn for_tests() -> Self {
        Self {
            last: 300,
            include_headers: false,
            include_body: false,
            redact: true,
            ports_scanned: vec![9753],
            wait_ms: 5000,
            settle_ms: 750,
            complete: true,
            warnings: Vec::new(),
        }
    }
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;
