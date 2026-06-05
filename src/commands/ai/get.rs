use crate::app::App;
use crate::commands::ai::output::{AiError, AiErrorCode};
use crate::commands::ai::redact::{preview_text, redact_json_value, redact_text_patterns};
use crate::domain::network::{NetworkEntry, NetworkStatus, Protocol};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordId {
    Log(usize),
    Net(u64),
    Chunk { net_id: u64, chunk: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordDetailMode {
    Summary,
    Detail,
}

pub fn parse_record_id(input: &str) -> Result<RecordId, AiError> {
    if let Some(rest) = input.strip_prefix("log#") {
        return rest
            .parse()
            .map(RecordId::Log)
            .map_err(|_| record_not_found(input));
    }
    if let Some(rest) = input.strip_prefix("net#") {
        return rest
            .parse()
            .map(RecordId::Net)
            .map_err(|_| record_not_found(input));
    }
    if let Some(rest) = input.strip_prefix("chunk#") {
        let Some((net, chunk)) = rest.split_once('.') else {
            return Err(record_not_found(input));
        };
        return Ok(RecordId::Chunk {
            net_id: net.parse().map_err(|_| record_not_found(input))?,
            chunk: chunk.parse().map_err(|_| record_not_found(input))?,
        });
    }
    Err(record_not_found(input))
}

pub fn lookup_record(
    app: &App,
    id: &RecordId,
    mode: RecordDetailMode,
    redact: bool,
) -> Result<serde_json::Value, AiError> {
    match id {
        RecordId::Log(index) => app
            .store
            .get(*index)
            .map(|log| {
                let mut value = serde_json::json!({
                    "id": format!("log#{index}"),
                    "timestamp": log.timestamp,
                    "level": log.level.as_str(),
                    "tag": log.tag,
                    "message": log.message,
                    "repeat_count": log.repeat_count,
                    "has_error": log.error.is_some(),
                    "has_stacktrace": log.stacktrace.is_some(),
                });
                if mode == RecordDetailMode::Detail {
                    value["error"] = optional_text(log.error.as_deref(), 1200, redact);
                    value["stacktrace"] = optional_text(log.stacktrace.as_deref(), 4000, redact);
                }
                value
            })
            .ok_or_else(|| record_not_found(&format!("log#{index}"))),
        RecordId::Net(net_id) => app
            .network_store
            .iter()
            .find(|entry| entry.id == *net_id)
            .map(|entry| network_record_value(entry, mode, redact))
            .ok_or_else(|| record_not_found(&format!("net#{net_id}"))),
        RecordId::Chunk { net_id, chunk } => app
            .network_store
            .iter()
            .find(|entry| entry.id == *net_id)
            .and_then(|entry| entry.sse_chunks.get(*chunk))
            .map(|chunk_value| {
                serde_json::json!({
                    "id": format!("chunk#{net_id}.{chunk}"),
                    "data": body_preview(Some(chunk_value.data.as_str()), 4000, redact),
                })
            })
            .ok_or_else(|| record_not_found(&format!("chunk#{net_id}.{chunk}"))),
    }
}

fn network_record_value(
    entry: &NetworkEntry,
    mode: RecordDetailMode,
    redact: bool,
) -> serde_json::Value {
    let mut value = network_summary(entry);
    if mode == RecordDetailMode::Detail {
        value["request"] = serde_json::json!({
            "headers": headers_value(entry.request_headers.as_deref(), redact),
            "body": body_preview(entry.request_body.as_deref(), 4000, redact),
        });
        value["response"] = serde_json::json!({
            "headers": headers_value(entry.response_headers.as_deref(), redact),
            "body": body_preview(entry.response_body.as_deref(), 4000, redact),
        });
        if entry.protocol == Protocol::Sse {
            value["sse_chunks"] = serde_json::Value::Array(
                entry
                    .sse_chunks
                    .iter()
                    .enumerate()
                    .map(|(index, chunk)| {
                        serde_json::json!({
                            "id": format!("chunk#{}.{}", entry.id, index),
                            "data": body_preview(Some(chunk.data.as_str()), 1200, redact),
                        })
                    })
                    .collect(),
            );
        }
        if entry.protocol == Protocol::Ws {
            value["ws_messages_detail"] = serde_json::Value::Array(
                entry
                    .ws_messages
                    .iter()
                    .enumerate()
                    .map(|(index, message)| {
                        serde_json::json!({
                            "index": index,
                            "direction": format!("{:?}", message.direction).to_ascii_lowercase(),
                            "size": message.size,
                            "data": body_preview(Some(message.data.as_str()), 1200, redact),
                        })
                    })
                    .collect(),
            );
        }
    }
    value
}

fn network_summary(entry: &NetworkEntry) -> serde_json::Value {
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

fn headers_value(headers: Option<&str>, redact: bool) -> serde_json::Value {
    let Some(headers) = headers else {
        return serde_json::Value::Null;
    };
    let value = serde_json::from_str(headers).unwrap_or(serde_json::Value::String(headers.into()));
    if redact {
        redact_json_value(&value)
    } else {
        value
    }
}

fn body_preview(body: Option<&str>, max_chars: usize, redact: bool) -> serde_json::Value {
    let Some(body) = body else {
        return serde_json::json!({"present": false});
    };
    let body = redact_body(body, redact);
    let mut preview = preview_text(&body, max_chars);
    preview.redacted = redact;
    serde_json::to_value(preview).unwrap_or(serde_json::Value::Null)
}

fn optional_text(body: Option<&str>, max_chars: usize, redact: bool) -> serde_json::Value {
    body_preview(body, max_chars, redact)
}

fn redact_body(body: &str, redact: bool) -> String {
    if !redact {
        return body.to_string();
    }
    match serde_json::from_str::<serde_json::Value>(body) {
        Ok(value) => redact_json_value(&value).to_string(),
        Err(_) => redact_text_patterns(body),
    }
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

fn record_not_found(id: &str) -> AiError {
    AiError::new(
        AiErrorCode::RecordNotFound,
        format!("Record {id} was not found in the replay buffer."),
        vec!["Run `flog ai snapshot --format json` to refresh ids.".to_string()],
    )
}

#[cfg(test)]
#[path = "get_tests.rs"]
mod tests;
