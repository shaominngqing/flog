//! Local HTTP proxy server with mock rule support.
//!
//! The proxy intercepts HTTP requests from the Flutter app. If a mock rule
//! matches, it returns the canned response; otherwise it forwards the
//! request to the real server via reqwest.

pub mod mock;

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use crate::app::App;
use crate::domain::network::{EntrySource, NetworkEntry, NetworkStatus};

/// High ID offset to avoid collision with app-generated IDs.
const MOCK_ID_OFFSET: u64 = 20_000_000;

/// Start the proxy server, trying ports 9999..10009.
/// Returns the port on success.
pub async fn start_proxy(app: Arc<Mutex<App>>) -> Result<u16, String> {
    let base_port: u16 = 9999;

    for offset in 0..10 {
        let port = base_port + offset;
        let addr = SocketAddr::from(([127, 0, 0, 1], port));

        match TcpListener::bind(addr).await {
            Ok(listener) => {
                {
                    let mut a = app.lock().await;
                    a.proxy_port = Some(port);
                    a.proxy_running = true;
                }

                // Spawn the accept loop
                let app_clone = Arc::clone(&app);
                tokio::spawn(async move {
                    accept_loop(listener, app_clone).await;
                });

                // Auto adb reverse so real devices can reach the proxy
                let _ = std::process::Command::new("adb")
                    .args(["reverse", &format!("tcp:{}", port), &format!("tcp:{}", port)])
                    .output();

                return Ok(port);
            }
            Err(_) => continue,
        }
    }

    Err("Could not bind proxy to any port (9999-10008)".to_string())
}

/// Accept loop: handles each incoming connection in a new task.
async fn accept_loop(listener: TcpListener, app: Arc<Mutex<App>>) {
    loop {
        let (stream, _) = match listener.accept().await {
            Ok(conn) => conn,
            Err(_) => continue,
        };

        let io = TokioIo::new(stream);
        let app_conn = Arc::clone(&app);

        tokio::spawn(async move {
            let service = service_fn(move |req| {
                let app_req = Arc::clone(&app_conn);
                async move { handle_request(app_req, req).await }
            });

            if let Err(_e) = http1::Builder::new()
                .preserve_header_case(true)
                .serve_connection(io, service)
                .await
            {
                // Connection error — silently ignore
            }
        });
    }
}

/// Handle a single proxied HTTP request.
async fn handle_request(
    app: Arc<Mutex<App>>,
    req: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let method = req.method().to_string();
    let uri = req.uri().to_string();

    // Determine the target URL.
    // Priority: x-flog-target header (set by FlogDio interceptor) > absolute URI > Host header
    let has_flog_target = req.headers().contains_key("x-flog-target");
    let target_url = if let Some(target) = req.headers().get("x-flog-target") {
        target.to_str().unwrap_or(&uri).to_string()
    } else if uri.starts_with("http://") || uri.starts_with("https://") {
        uri.clone()
    } else if let Some(host) = req.headers().get("host") {
        let host_str = host.to_str().unwrap_or("localhost");
        format!("https://{}{}", host_str, uri)
    } else {
        uri.clone()
    };

    // Mark Dart as connected when we see x-flog-target header
    if has_flog_target {
        let mut a = app.lock().await;
        if !a.proxy_dart_connected {
            a.proxy_dart_connected = true;
        }
    }

    // Check mock rules
    let mock_match = {
        let mut a = app.lock().await;
        a.mock_rules.find_match(&target_url, &method)
    };

    if let Some(rule) = mock_match {
        // Apply delay if configured
        if rule.delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(rule.delay_ms)).await;
        }

        // Record mocked entry in network store
        let mock_id = {
            let a = app.lock().await;
            MOCK_ID_OFFSET + a.network_store.len() as u64
        };

        let mut entry =
            NetworkEntry::new_http(mock_id, method.clone(), target_url.clone(), String::new());
        entry.source = EntrySource::Mocked;
        entry.status = NetworkStatus::Completed;
        entry.http_status = Some(rule.status_code);
        entry.response_body = Some(rule.response_body.clone());
        entry.response_size = Some(rule.response_body.len() as u64);
        entry.duration = Some(rule.delay_ms);

        {
            let mut a = app.lock().await;
            a.network_store.push_entry(entry);
            a.network.invalidate_filter();
        }

        // Return mock response
        let response = Response::builder()
            .status(rule.status_code)
            .header("content-type", "application/json")
            .header("x-flog-mock", "true")
            .body(Full::new(Bytes::from(rule.response_body)))
            .unwrap_or_else(|_| Response::new(Full::new(Bytes::from("mock error"))));

        return Ok(response);
    }

    // No mock match — forward to real server
    forward_request(&app, &method, &target_url, req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_proxy_binds_to_port() {
        let app = Arc::new(Mutex::new(crate::app::App::new()));
        let result = start_proxy(Arc::clone(&app)).await;
        assert!(result.is_ok());

        let port = result.unwrap();
        assert!(port >= 9999);

        let a = app.lock().await;
        assert!(a.proxy_running);
        assert_eq!(a.proxy_port, Some(port));
    }
}

/// Forward request to the real server and return the response.
async fn forward_request(
    app: &Arc<Mutex<App>>,
    method: &str,
    target_url: &str,
    req: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let client = match reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .no_proxy() // don't recurse through ourselves
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            let body = format!("Proxy error: {}", e);
            return Ok(Response::builder()
                .status(502)
                .body(Full::new(Bytes::from(body)))
                .unwrap());
        }
    };

    let reqwest_method = match method.to_uppercase().as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "DELETE" => reqwest::Method::DELETE,
        "PATCH" => reqwest::Method::PATCH,
        "HEAD" => reqwest::Method::HEAD,
        "OPTIONS" => reqwest::Method::OPTIONS,
        _ => reqwest::Method::GET,
    };

    // Forward original headers, excluding proxy-specific ones
    let mut headers = reqwest::header::HeaderMap::new();
    let skip_headers = ["host", "x-flog-target", "content-length", "transfer-encoding"];
    for (key, val) in req.headers() {
        let key_str = key.as_str().to_lowercase();
        if skip_headers.contains(&key_str.as_str()) {
            continue;
        }
        if let Ok(name) = reqwest::header::HeaderName::from_bytes(key.as_str().as_bytes()) {
            if let Ok(v) = reqwest::header::HeaderValue::from_bytes(val.as_bytes()) {
                headers.insert(name, v);
            }
        }
    }

    // Forward original body
    use http_body_util::BodyExt;
    let body_bytes = req.into_body().collect().await.map(|b| b.to_bytes()).unwrap_or_default();

    let start = Instant::now();
    let result = client
        .request(reqwest_method, target_url)
        .headers(headers)
        .body(body_bytes.to_vec())
        .send()
        .await;
    let _duration_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(response) => {
            let status = response.status().as_u16();

            // Collect response headers
            let mut headers_map = serde_json::Map::new();
            for (key, val) in response.headers() {
                let val_str = val.to_str().unwrap_or("").to_string();
                headers_map.insert(key.as_str().to_string(), serde_json::Value::String(val_str));
            }

            let body_bytes = response.bytes().await.unwrap_or_default();

            // Build hyper response
            let mut builder = Response::builder().status(status);
            for (k, v) in &headers_map {
                if let serde_json::Value::String(s) = v {
                    builder = builder.header(k.as_str(), s.as_str());
                }
            }

            Ok(builder
                .body(Full::new(Bytes::from(body_bytes.to_vec())))
                .unwrap_or_else(|_| Response::new(Full::new(Bytes::new()))))
        }
        Err(e) => {
            // Record the error but still try to return something useful
            let _ = app; // app available for future logging
            let body = format!("Proxy error: {}", e);
            Ok(Response::builder()
                .status(502)
                .body(Full::new(Bytes::from(body)))
                .unwrap())
        }
    }
}
