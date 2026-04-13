//! Auto-discover Flutter VM Service from running processes.
//!
//! Scans `ps aux` for `--vm-service-uri=http://127.0.0.1:PORT/TOKEN=/` pattern.
//! Then follows the 302 redirect to find the DDS proxy URL with auth token.

use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

#[derive(Clone)]
pub struct DiscoveredService {
    pub ws_url: String,
    pub name: String,
}

/// Find ALL running Flutter VM Services by scanning process args.
pub async fn find_all_vm_services() -> Vec<DiscoveredService> {
    let output = match tokio::process::Command::new("ps")
        .args(["aux"])
        .output()
        .await
    {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let mut results = Vec::new();

    for line in text.lines() {
        if !line.contains("development-service") {
            continue;
        }
        if let Some(idx) = line.find("--vm-service-uri=") {
            let rest = &line[idx + 17..];
            let url_end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
            let http_url = rest[..url_end].trim_end_matches('/');

            if http_url.starts_with("http://127.0.0.1:") {
                // Only connect via DDS proxy to avoid blocking flutter run.
                // If DDS isn't ready yet, skip — outer scan loop retries in 100ms.
                if let Some(url) = resolve_dds_url(http_url).await {
                    let ws_url = format!("{}ws", url.replace("http://", "ws://"));
                    let name = query_vm_name(&url).await;
                    results.push(DiscoveredService { ws_url, name });
                }
            }
        }
    }

    results
}

/// Find the first running Flutter VM Service (convenience wrapper).
pub async fn find_vm_service() -> Option<DiscoveredService> {
    find_all_vm_services().await.into_iter().next()
}

/// Discover the DDS proxy WebSocket URL by hitting the original VM Service root path.
///
/// When DDS is active, `GET /TOKEN=/` returns a 302 with a location like:
///   `http://127.0.0.1:PORT/DDS_TOKEN=/devtools/?uri=ws://127.0.0.1:PORT/DDS_TOKEN=/ws`
///
/// We extract the `ws://` URL from the `uri=` query parameter.
async fn resolve_dds_url(underlying_http_url: &str) -> Option<String> {
    let host_port = underlying_http_url
        .strip_prefix("http://")?
        .split('/')
        .next()?;
    let path = underlying_http_url.strip_prefix(&format!("http://{}", host_port))?;
    // Keep trailing slash — the VM Service only 302s on the exact path with `/`
    let root_path = if path.ends_with('/') {
        path.to_string()
    } else {
        format!("{}/", path)
    };

    let mut stream = timeout(Duration::from_millis(300), TcpStream::connect(host_port))
        .await
        .ok()?
        .ok()?;

    let req = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        root_path, host_port,
    );
    stream.write_all(req.as_bytes()).await.ok()?;

    let mut buf = vec![0u8; 4096];
    let n = timeout(Duration::from_millis(300), stream.read(&mut buf))
        .await
        .ok()?
        .ok()?;
    let resp = String::from_utf8_lossy(&buf[..n]);

    if !resp.starts_with("HTTP/1.1 302") {
        return None;
    }

    for line in resp.lines() {
        let lower = line.to_lowercase();
        if !lower.starts_with("location:") {
            continue;
        }
        let location = line["location:".len()..].trim();

        // Extract ws:// URL from uri= query parameter
        if let Some(uri_start) = location.find("uri=ws") {
            let ws_part = &location[uri_start + 4..]; // skip "uri="
            let ws_url = ws_part.split('&').next().unwrap_or(ws_part);
            // URL-decode: %3A -> :, %2F -> /
            let decoded = ws_url
                .replace("%3A", ":")
                .replace("%3a", ":")
                .replace("%2F", "/")
                .replace("%2f", "/")
                .replace("%3D", "=")
                .replace("%3d", "=");
            if decoded.starts_with("ws://") {
                // Return the base URL (strip /ws suffix, we add it later)
                let base = decoded.trim_end_matches("/ws").trim_end_matches('/');
                return Some(format!("{}/", base.replace("ws://", "http://")));
            }
        }

        // Fallback: location itself is the DDS HTTP URL
        if location.starts_with("http://") {
            // Extract base URL (before /devtools or any path)
            if let Some(token_end) = location.find("/devtools") {
                return Some(location[..token_end].to_string() + "/");
            }
            return Some(location.to_string());
        }
    }

    None
}

async fn query_vm_name(http_url: &str) -> String {
    let host_port = http_url
        .strip_prefix("http://")
        .and_then(|s| s.split('/').next())
        .unwrap_or("127.0.0.1");
    let path = http_url
        .strip_prefix(&format!("http://{}", host_port))
        .unwrap_or("/");
    let get_vm_path = format!("{}getVM", path);

    if let Ok(Ok(mut stream)) =
        timeout(Duration::from_millis(500), TcpStream::connect(host_port)).await
    {
        let req = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            get_vm_path, host_port
        );
        let _ = stream.write_all(req.as_bytes()).await;

        let mut buf = vec![0u8; 4096];
        if let Ok(Ok(n)) = timeout(Duration::from_millis(500), stream.read(&mut buf)).await {
            let resp = String::from_utf8_lossy(&buf[..n]);
            if let Some(i) = resp.find("\"operatingSystem\":\"") {
                let rest = &resp[i + 19..];
                if let Some(end) = rest.find('"') {
                    return rest[..end].to_string();
                }
            }
        }
    }
    "Flutter".to_string()
}
