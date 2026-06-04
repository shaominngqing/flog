use crate::app::App;
use crate::commands::ai::output::{AiError, AiErrorCode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordId {
    Log(usize),
    Net(u64),
    Chunk { net_id: u64, chunk: usize },
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

pub fn lookup_record(app: &App, id: &RecordId) -> Result<serde_json::Value, AiError> {
    match id {
        RecordId::Log(index) => app
            .store
            .get(*index)
            .map(|log| {
                serde_json::json!({
                    "id": format!("log#{index}"),
                    "timestamp": log.timestamp,
                    "level": log.level.as_str(),
                    "tag": log.tag,
                    "message": log.message,
                    "stacktrace": log.stacktrace,
                })
            })
            .ok_or_else(|| record_not_found(&format!("log#{index}"))),
        RecordId::Net(net_id) => app
            .network_store
            .iter()
            .find(|entry| entry.id == *net_id)
            .map(|entry| {
                serde_json::json!({
                    "id": format!("net#{net_id}"),
                    "protocol": format!("{:?}", entry.protocol).to_ascii_lowercase(),
                    "method": entry.method,
                    "url": entry.url,
                    "status": entry.http_status,
                    "network_status": format!("{:?}", entry.status).to_ascii_lowercase(),
                    "request_headers": entry.request_headers,
                    "response_headers": entry.response_headers,
                    "request_body": entry.request_body,
                    "response_body": entry.response_body,
                    "error": entry.error,
                    "sse_chunks": entry.sse_chunks.len(),
                    "ws_messages": entry.ws_messages.len(),
                })
            })
            .ok_or_else(|| record_not_found(&format!("net#{net_id}"))),
        RecordId::Chunk { net_id, chunk } => app
            .network_store
            .iter()
            .find(|entry| entry.id == *net_id)
            .and_then(|entry| entry.sse_chunks.get(*chunk))
            .map(|chunk_value| {
                serde_json::json!({
                    "id": format!("chunk#{net_id}.{chunk}"),
                    "data": chunk_value.data,
                })
            })
            .ok_or_else(|| record_not_found(&format!("chunk#{net_id}.{chunk}"))),
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
