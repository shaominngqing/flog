use crate::app::App;
use crate::commands::ai::get::RecordId;
use crate::commands::ai::output::{AiError, AiErrorCode};
use crate::commands::ai::redact::{redact_json_value, redact_text_patterns};
use crate::domain::network::NetworkEntry;

pub fn build_curl(app: &App, id: &RecordId, redact: bool) -> Result<serde_json::Value, AiError> {
    let RecordId::Net(net_id) = id else {
        return Err(curl_requires_network_id());
    };
    let entry = app
        .network_store
        .iter()
        .find(|entry| entry.id == *net_id)
        .ok_or_else(|| record_not_found(&format!("net#{net_id}")))?;

    let mut parts = vec![
        "curl".to_string(),
        "-X".to_string(),
        entry.method.clone(),
        shell_quote(&entry.url),
    ];
    for (name, value) in request_headers(entry, redact) {
        parts.push("-H".to_string());
        parts.push(shell_quote(&format!("{name}: {value}")));
    }
    if let Some(body) = entry.request_body.as_deref() {
        parts.push("--data-raw".to_string());
        parts.push(shell_quote(&redact_body(body, redact)));
    }

    Ok(serde_json::json!({
        "id": format!("net#{net_id}"),
        "method": entry.method,
        "url": entry.url,
        "curl": parts.join(" "),
        "redacted": redact,
    }))
}

fn request_headers(entry: &NetworkEntry, redact: bool) -> Vec<(String, String)> {
    let Some(headers) = entry.request_headers.as_deref() else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(headers) else {
        return Vec::new();
    };
    let value = if redact {
        redact_json_value(&value)
    } else {
        value
    };
    let Some(map) = value.as_object() else {
        return Vec::new();
    };
    map.iter()
        .map(|(name, value)| {
            let value = value
                .as_str()
                .map(ToString::to_string)
                .unwrap_or_else(|| value.to_string());
            (name.clone(), value)
        })
        .collect()
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

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn curl_requires_network_id() -> AiError {
    AiError::new(
        AiErrorCode::RecordNotFound,
        "cURL export is only available for network request ids like net#42.",
        vec!["Run `flog ai net --last 20` to list request ids.".to_string()],
    )
}

fn record_not_found(id: &str) -> AiError {
    AiError::new(
        AiErrorCode::RecordNotFound,
        format!("Record {id} was not found in the replay buffer."),
        vec!["Run `flog ai net --last 20` to refresh request ids.".to_string()],
    )
}

#[cfg(test)]
#[path = "curl_tests.rs"]
mod tests;
