//! HTTP request replay — resends a captured NetworkEntry via reqwest.

use std::sync::Arc;
use std::time::Instant;

use tokio::sync::Mutex;

use crate::app::App;
use crate::domain::network::{EntrySource, NetworkEntry, NetworkStatus, Protocol};

/// High ID offset to avoid collision with app-generated IDs.
const REPLAY_ID_OFFSET: u64 = 10_000_000;

/// Replay a single HTTP request and insert the result into the network store.
pub async fn replay_request(app: Arc<Mutex<App>>, entry: NetworkEntry) {
    if entry.protocol != Protocol::Http {
        return;
    }

    // Build the reqwest request
    let method = match entry.method.to_uppercase().as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "DELETE" => reqwest::Method::DELETE,
        "PATCH" => reqwest::Method::PATCH,
        "HEAD" => reqwest::Method::HEAD,
        "OPTIONS" => reqwest::Method::OPTIONS,
        _ => reqwest::Method::GET,
    };

    let client = match reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            insert_failed_replay(&app, &entry, &format!("Client build error: {}", e)).await;
            return;
        }
    };

    let mut request = client.request(method, &entry.url);

    // Reconstruct headers from JSON string (Dio format: values can be strings or arrays)
    if let Some(ref headers_json) = entry.request_headers {
        if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(headers_json) {
            for (key, val) in &map {
                let val_str = match val {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Array(arr) => {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                    other => other.to_string(),
                };
                if let Ok(header_name) = reqwest::header::HeaderName::from_bytes(key.as_bytes()) {
                    if let Ok(header_value) = reqwest::header::HeaderValue::from_str(&val_str) {
                        request = request.header(header_name, header_value);
                    }
                }
            }
        }
    }

    // Attach body
    if let Some(ref body) = entry.request_body {
        if !body.is_empty() {
            request = request.body(body.clone());
        }
    }

    let start = Instant::now();
    let result = request.send().await;
    let duration_ms = start.elapsed().as_millis() as u64;

    // Assign a new ID for the replay entry
    let replay_id = {
        let a = app.lock().await;
        REPLAY_ID_OFFSET + a.network_store.len() as u64
    };

    let mut replay_entry = NetworkEntry::new_http(
        replay_id,
        entry.method.clone(),
        entry.url.clone(),
        String::new(),
    );
    replay_entry.source = EntrySource::Replay;
    replay_entry.request_headers = entry.request_headers.clone();
    replay_entry.request_body = entry.request_body.clone();
    replay_entry.request_size = entry.request_size;

    match result {
        Ok(response) => {
            let http_status = response.status().as_u16();

            // Collect response headers
            let mut headers_map = serde_json::Map::new();
            for (key, val) in response.headers() {
                let val_str = val.to_str().unwrap_or("").to_string();
                headers_map.insert(key.as_str().to_string(), serde_json::Value::String(val_str));
            }
            let response_headers = serde_json::to_string(&headers_map).ok();

            // Read response body
            let body_bytes = response.bytes().await.unwrap_or_default();
            let response_size = body_bytes.len() as u64;
            let response_body = String::from_utf8_lossy(&body_bytes).to_string();

            replay_entry.status = NetworkStatus::Completed;
            replay_entry.http_status = Some(http_status);
            replay_entry.duration = Some(duration_ms);
            replay_entry.response_headers = response_headers;
            replay_entry.response_body = Some(response_body);
            replay_entry.response_size = Some(response_size);
        }
        Err(e) => {
            replay_entry.status = NetworkStatus::Failed;
            replay_entry.error = Some(e.to_string());
            replay_entry.duration = Some(duration_ms);
        }
    }

    let mut a = app.lock().await;
    a.network_store.push_entry(replay_entry);
    a.network.invalidate_filter();
    a.show_status("Replay complete".to_string());
}

/// Helper: insert a failed replay entry when we can't even build the client.
async fn insert_failed_replay(app: &Arc<Mutex<App>>, entry: &NetworkEntry, error: &str) {
    let replay_id = {
        let a = app.lock().await;
        REPLAY_ID_OFFSET + a.network_store.len() as u64
    };

    let mut replay_entry = NetworkEntry::new_http(
        replay_id,
        entry.method.clone(),
        entry.url.clone(),
        String::new(),
    );
    replay_entry.source = EntrySource::Replay;
    replay_entry.status = NetworkStatus::Failed;
    replay_entry.error = Some(error.to_string());
    replay_entry.request_headers = entry.request_headers.clone();
    replay_entry.request_body = entry.request_body.clone();

    let mut a = app.lock().await;
    a.network_store.push_entry(replay_entry);
    a.network.invalidate_filter();
    a.show_status(format!("Replay failed: {}", error));
}
