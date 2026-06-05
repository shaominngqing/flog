use std::time::Duration;

use crate::app::App;
use crate::domain::network::{NetworkEntry, NetworkStatus, Protocol};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetListOptions {
    pub last: usize,
    pub failed: bool,
    pub status: Option<String>,
    pub method: Option<String>,
    pub url: Option<String>,
    pub protocol: Option<Protocol>,
    pub slow: Option<Duration>,
}

pub fn build_net_list(app: &App, options: NetListOptions) -> Vec<serde_json::Value> {
    let method = options.method.as_ref().map(|s| s.to_ascii_uppercase());
    let url = options.url.as_ref().map(|s| s.to_ascii_lowercase());
    let mut matches = app
        .network_store
        .iter()
        .filter(|entry| !options.failed || is_failed(entry))
        .filter(|entry| status_matches(entry.http_status, options.status.as_deref()))
        .filter(|entry| method.as_ref().is_none_or(|method| entry.method == *method))
        .filter(|entry| {
            url.as_ref()
                .is_none_or(|url| entry.url.to_ascii_lowercase().contains(url))
        })
        .filter(|entry| {
            options
                .protocol
                .is_none_or(|protocol| entry.protocol == protocol)
        })
        .filter(|entry| {
            options.slow.is_none_or(|slow| {
                entry
                    .duration
                    .is_some_and(|ms| ms >= slow.as_millis() as u64)
            })
        })
        .collect::<Vec<_>>();

    let start = matches.len().saturating_sub(options.last);
    matches
        .drain(start..)
        .map(network_summary_value)
        .collect::<Vec<_>>()
}

fn network_summary_value(entry: &NetworkEntry) -> serde_json::Value {
    serde_json::json!({
        "id": format!("net#{}", entry.id),
        "protocol": protocol_label(entry.protocol),
        "method": entry.method,
        "url": entry.url,
        "status": entry.http_status,
        "network_status": status_label(entry.status),
        "duration_ms": entry.duration,
        "request_size": entry.request_size,
        "response_size": entry.response_size,
        "sse_chunks": entry.sse_chunks.len(),
        "ws_messages": entry.ws_messages.len(),
        "error": entry.error,
    })
}

fn is_failed(entry: &NetworkEntry) -> bool {
    matches!(entry.status, NetworkStatus::Failed)
        || entry.http_status.is_some_and(|status| status >= 400)
}

fn status_matches(status: Option<u16>, filter: Option<&str>) -> bool {
    let Some(filter) = filter else {
        return true;
    };
    match filter {
        "2xx" => status.is_some_and(|s| (200..=299).contains(&s)),
        "3xx" => status.is_some_and(|s| (300..=399).contains(&s)),
        "4xx" => status.is_some_and(|s| (400..=499).contains(&s)),
        "5xx" => status.is_some_and(|s| (500..=599).contains(&s)),
        exact => exact.parse::<u16>().ok() == status,
    }
}

pub fn protocol_label(protocol: Protocol) -> &'static str {
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

#[cfg(test)]
#[path = "net_tests.rs"]
mod tests;
