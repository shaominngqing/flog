//! Test: connect to a discovered Flutter VM Service and verify we can receive events.
//!
//! Run with: cargo test --test ws_connect_test -- --nocapture

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::time::Duration;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

/// Step 1: discover what `ps aux` gives us
#[tokio::test]
async fn test_discover_from_ps() {
    let output = tokio::process::Command::new("ps")
        .args(["aux"])
        .output()
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);

    println!("=== Scanning ps aux for Flutter-related processes ===");

    let mut found_dds = false;
    for line in text.lines() {
        // Print DDS-related lines
        if line.contains("development-service") || line.contains("vm-service-uri") {
            println!("[DDS] {}", line);
            found_dds = true;
        }
        // Print flutter_tools related lines
        if line.contains("flutter_tools") || line.contains("flutter run") {
            println!("[flutter] {}", line);
        }
        // Print any dart observatory/devtools
        if line.contains("devtools") || line.contains("observatory") {
            println!("[devtools] {}", line);
        }
    }

    if !found_dds {
        println!("No DDS process found. Is `flutter run` active?");
        return;
    }

    // Extract the --vm-service-uri (this is the UNDERLYING VM Service that DDS connects to)
    for line in text.lines() {
        if let Some(idx) = line.find("--vm-service-uri=") {
            let rest = &line[idx + 17..];
            let url_end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
            let raw_uri = &rest[..url_end];
            println!("\n--vm-service-uri (underlying VM Service): {}", raw_uri);
        }
        // DDS also has --bind-address and --bind-port
        if line.contains("--bind-port=") {
            if let Some(idx) = line.find("--bind-port=") {
                let rest = &line[idx + 12..];
                let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
                println!("--bind-port (DDS listen port): {}", &rest[..end]);
            }
            if let Some(idx) = line.find("--bind-address=") {
                let rest = &line[idx + 15..];
                let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
                println!("--bind-address (DDS listen address): {}", &rest[..end]);
            }
        }
    }
}

/// Step 2: Try connecting directly to the underlying VM Service (from --vm-service-uri)
#[tokio::test]
async fn test_connect_underlying_vm_service() {
    let uri = match discover_underlying_uri().await {
        Some(u) => u,
        None => { println!("No Flutter process found, skipping"); return; }
    };

    let ws_url = format!("{}/ws", uri.replace("http://", "ws://").trim_end_matches('/'));
    println!("Attempting to connect to UNDERLYING VM Service: {}", ws_url);

    match tokio::time::timeout(Duration::from_secs(3), connect_async(&ws_url)).await {
        Ok(Ok((ws, _))) => {
            println!("Connected OK!");
            let (mut tx, mut rx) = ws.split();

            // Try streamListen
            let sub = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "streamListen",
                "params": { "streamId": "Logging" },
                "id": "1"
            });
            tx.send(Message::Text(sub.to_string().into())).await.unwrap();

            // Read responses for 5 seconds
            let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
            let mut msg_count = 0;
            loop {
                match tokio::time::timeout_at(deadline, rx.next()).await {
                    Ok(Some(Ok(Message::Text(t)))) => {
                        msg_count += 1;
                        let json: Value = serde_json::from_str(&t).unwrap_or(Value::Null);
                        let method = json.get("method").and_then(|m| m.as_str()).unwrap_or("(response)");
                        println!("[msg {}] method={}, size={}", msg_count, method, t.len());
                        if msg_count <= 3 {
                            println!("  content: {}", &t[..t.len().min(200)]);
                        }
                    }
                    Ok(Some(Ok(Message::Close(frame)))) => {
                        println!("Server sent Close: {:?}", frame);
                        break;
                    }
                    Ok(Some(Err(e))) => {
                        println!("WebSocket error: {}", e);
                        break;
                    }
                    Ok(None) => {
                        println!("Stream ended (None)");
                        break;
                    }
                    Ok(Some(Ok(other))) => {
                        println!("[msg] non-text: {:?}", other);
                    }
                    Err(_) => {
                        println!("Timeout (5s). Received {} messages total.", msg_count);
                        break;
                    }
                }
            }
        }
        Ok(Err(e)) => {
            println!("Connection FAILED: {}", e);
        }
        Err(_) => {
            println!("Connection TIMED OUT (3s)");
        }
    }
}

/// Step 3: Try finding and connecting to the DDS proxy port
#[tokio::test]
async fn test_find_and_connect_dds() {
    // DDS starts with --bind-port=0, which means it picks a random port.
    // The DDS port is printed to flutter run's stdout.
    // But we can also try: look at the DDS process's open listening ports.

    // First, find the DDS process PID
    let output = tokio::process::Command::new("ps")
        .args(["aux"])
        .output()
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);

    let mut dds_pid: Option<String> = None;
    let mut underlying_port: Option<String> = None;

    for line in text.lines() {
        if line.contains("development-service") && line.contains("--vm-service-uri=") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() > 1 {
                dds_pid = Some(parts[1].to_string());
            }
            // Extract the underlying port from --vm-service-uri
            if let Some(idx) = line.find("--vm-service-uri=http://127.0.0.1:") {
                let rest = &line[idx + 34..]; // skip "--vm-service-uri=http://127.0.0.1:"
                let end = rest.find('/').unwrap_or(rest.len());
                underlying_port = Some(rest[..end].to_string());
            }
        }
    }

    let dds_pid = match dds_pid {
        Some(p) => p,
        None => { println!("No DDS process found, skipping"); return; }
    };

    println!("DDS PID: {}", dds_pid);
    println!("Underlying VM Service port: {:?}", underlying_port);

    // Use lsof to find what ports the DDS process is listening on
    let lsof = tokio::process::Command::new("lsof")
        .args(["-i", "-P", "-n", "-p", &dds_pid])
        .output()
        .await;

    match lsof {
        Ok(output) => {
            let lsof_text = String::from_utf8_lossy(&output.stdout);
            println!("\n=== lsof output for DDS PID {} ===", dds_pid);
            for line in lsof_text.lines() {
                println!("  {}", line);
            }

            // Find LISTEN ports that are NOT the underlying port
            let underlying = underlying_port.as_deref().unwrap_or("");
            let mut dds_port = None;
            for line in lsof_text.lines() {
                if line.contains("LISTEN") {
                    // Extract port from something like "127.0.0.1:12345"
                    if let Some(addr_part) = line.split_whitespace().nth(8) {
                        if let Some(port) = addr_part.rsplit(':').next() {
                            if port != underlying {
                                println!("\nFound DDS listening port: {}", port);
                                dds_port = Some(port.to_string());
                            }
                        }
                    }
                }
            }

            if let Some(port) = dds_port {
                // Try to find the auth token by connecting via HTTP
                let http_url = format!("http://127.0.0.1:{}", port);
                println!("\nTrying HTTP GET to DDS: {}", http_url);

                // DDS serves a redirect or a page with the ws URL
                if let Ok(Ok(mut stream)) = tokio::time::timeout(
                    Duration::from_secs(2),
                    tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port))
                ).await {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let req = format!("GET / HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n", port);
                    let _ = stream.write_all(req.as_bytes()).await;

                    let mut buf = vec![0u8; 4096];
                    if let Ok(Ok(n)) = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buf)).await {
                        let resp = String::from_utf8_lossy(&buf[..n]);
                        println!("HTTP response from DDS:\n{}", resp);
                    }
                }

                // Try connecting WebSocket WITHOUT auth token (will likely fail)
                let ws_url_no_auth = format!("ws://127.0.0.1:{}/ws", port);
                println!("\nTrying WS connect without auth token: {}", ws_url_no_auth);
                match tokio::time::timeout(Duration::from_secs(2), connect_async(&ws_url_no_auth)).await {
                    Ok(Ok(_)) => println!("Connected without auth! (unexpected)"),
                    Ok(Err(e)) => println!("Failed (expected): {}", e),
                    Err(_) => println!("Timed out"),
                }
            }
        }
        Err(e) => {
            println!("lsof failed: {}", e);
        }
    }
}

/// Step 4: Find the DDS URL from flutter run stdout
/// The actual way Flutter tools exposes DDS:
/// "The Dart VM service is listening on http://127.0.0.1:PORT/TOKEN=/"
/// This is the DDS proxy URL that we should connect to.
#[tokio::test]
async fn test_find_dds_url_from_flutter_logs() {
    // The DDS URL with auth token is typically printed by flutter run.
    // But since we can't read flutter run's stdout, let's try another approach:
    // The DDS process actually serves a VM Service that accepts the same protocol.
    // We need to find its port and auth token.
    //
    // Method: Check if there's a .dart_tool/package_config.json or devtools URI somewhere.

    // Actually, the most reliable way: scan /tmp or look for the DevTools URI
    // flutter_tools writes to ~/.flutter-devtools/

    // Let's try: the DDS port we found via lsof, and try getVM without auth
    let output = tokio::process::Command::new("ps")
        .args(["aux"])
        .output()
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);

    let mut dds_pid: Option<String> = None;

    for line in text.lines() {
        if line.contains("development-service") && line.contains("--vm-service-uri=") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() > 1 {
                dds_pid = Some(parts[1].to_string());
            }
        }
    }

    let dds_pid = match dds_pid {
        Some(p) => p,
        None => { println!("No DDS process found"); return; }
    };

    // Get DDS listening port via lsof
    let lsof = tokio::process::Command::new("lsof")
        .args(["-i", "TCP", "-P", "-n", "-p", &dds_pid, "-sTCP:LISTEN"])
        .output()
        .await
        .unwrap();

    let lsof_text = String::from_utf8_lossy(&lsof.stdout);
    println!("DDS LISTEN ports:\n{}", lsof_text);

    // Extract all listening ports
    let mut dds_ports: Vec<String> = Vec::new();
    for line in lsof_text.lines() {
        if line.contains("LISTEN") {
            if let Some(addr) = line.split_whitespace().nth(8) {
                if let Some(port) = addr.rsplit(':').next() {
                    dds_ports.push(port.to_string());
                }
            }
        }
    }

    println!("DDS listening ports: {:?}", dds_ports);

    // Try each port: connect via HTTP, look for auth token in response
    for port in &dds_ports {
        println!("\n--- Trying port {} ---", port);
        if let Ok(Ok(mut stream)) = tokio::time::timeout(
            Duration::from_secs(2),
            tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port))
        ).await {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let req = format!("GET / HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n", port);
            let _ = stream.write_all(req.as_bytes()).await;

            let mut buf = vec![0u8; 8192];
            if let Ok(Ok(n)) = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buf)).await {
                let resp = String::from_utf8_lossy(&buf[..n]);
                println!("HTTP response ({} bytes):\n{}", n, &resp[..resp.len().min(500)]);

                // Look for auth token pattern in response
                if let Some(idx) = resp.find("ws://") {
                    let ws_part = &resp[idx..];
                    let end = ws_part.find(|c: char| c.is_whitespace() || c == '"' || c == '\'').unwrap_or(ws_part.len());
                    println!("\nFound WS URL in response: {}", &ws_part[..end]);
                }
            }
        }
    }
}

/// Step 5: The correct approach — use `flutter daemon` protocol or read DDS stdout
/// Actually, the simplest: DDS URL is printed to stdout of `flutter run`.
/// We can also find it by looking at `--serve-devtools` and checking the DevTools URL.
///
/// But the REAL question is: can we connect to the underlying VM Service directly?
/// DDS uses a single WebSocket to the underlying VM Service, but the underlying
/// VM Service can accept MULTIPLE clients.
#[tokio::test]
async fn test_connect_underlying_directly() {
    let uri = match discover_underlying_uri().await {
        Some(u) => u,
        None => { println!("No Flutter process found"); return; }
    };

    let ws_url = format!("{}/ws", uri.replace("http://", "ws://").trim_end_matches('/'));
    println!("Connecting to underlying VM Service: {}", ws_url);
    println!("(This is the --vm-service-uri that DDS also connects to)");

    match tokio::time::timeout(Duration::from_secs(5), connect_async(&ws_url)).await {
        Ok(Ok((ws, response))) => {
            println!("Connected! HTTP status: {}", response.status());
            println!("Headers: {:?}", response.headers());

            let (mut tx, mut rx) = ws.split();

            // 1. First try getVM to see if the connection is healthy
            let get_vm = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "getVM",
                "params": {},
                "id": "vm1"
            });
            println!("\nSending getVM...");
            tx.send(Message::Text(get_vm.to_string().into())).await.unwrap();

            // 2. Subscribe to Logging
            let sub_logging = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "streamListen",
                "params": { "streamId": "Logging" },
                "id": "sub1"
            });
            println!("Sending streamListen(Logging)...");
            tx.send(Message::Text(sub_logging.to_string().into())).await.unwrap();

            // 3. Subscribe to Stdout
            let sub_stdout = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "streamListen",
                "params": { "streamId": "Stdout" },
                "id": "sub2"
            });
            println!("Sending streamListen(Stdout)...");
            tx.send(Message::Text(sub_stdout.to_string().into())).await.unwrap();

            // Read all responses
            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            let mut msg_count = 0;
            loop {
                match tokio::time::timeout_at(deadline, rx.next()).await {
                    Ok(Some(Ok(Message::Text(t)))) => {
                        msg_count += 1;
                        let json: Value = serde_json::from_str(&t).unwrap_or(Value::Null);

                        // Check if it's a response or notification
                        if let Some(id) = json.get("id") {
                            // JSON-RPC response
                            let error = json.get("error");
                            if let Some(err) = error {
                                println!("[resp #{}] id={}, ERROR: {}", msg_count, id, err);
                            } else {
                                let result_preview = json.get("result")
                                    .map(|r| {
                                        let s = r.to_string();
                                        if s.len() > 150 { format!("{}...", &s[..150]) } else { s }
                                    })
                                    .unwrap_or_default();
                                println!("[resp #{}] id={}, result: {}", msg_count, id, result_preview);
                            }
                        } else if let Some(method) = json.get("method").and_then(|m| m.as_str()) {
                            // Notification
                            let stream_id = json.pointer("/params/streamId")
                                .and_then(|s| s.as_str())
                                .unwrap_or("?");
                            let event_kind = json.pointer("/params/event/kind")
                                .and_then(|s| s.as_str())
                                .unwrap_or("?");
                            println!("[event #{}] method={}, stream={}, kind={}", msg_count, method, stream_id, event_kind);
                        }
                    }
                    Ok(Some(Ok(Message::Close(frame)))) => {
                        println!("Close frame: {:?}", frame);
                        break;
                    }
                    Ok(Some(Err(e))) => {
                        println!("WS Error: {}", e);
                        // DON'T break — continue to see if more messages come
                    }
                    Ok(None) => {
                        println!("Stream ended");
                        break;
                    }
                    Ok(Some(Ok(other))) => {
                        println!("[other] {:?}", other);
                    }
                    Err(_) => {
                        println!("\nTimeout (10s). Total messages: {}", msg_count);
                        break;
                    }
                }
            }
        }
        Ok(Err(e)) => {
            println!("Connection failed: {}", e);
            println!("This confirms DDS already owns this connection.");
        }
        Err(_) => {
            println!("Connection timed out (5s)");
        }
    }
}

// Helper: extract the underlying VM Service URI from ps aux
async fn discover_underlying_uri() -> Option<String> {
    let output = tokio::process::Command::new("ps")
        .args(["aux"])
        .output()
        .await
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);

    for line in text.lines() {
        if let Some(idx) = line.find("--vm-service-uri=") {
            let rest = &line[idx + 17..];
            let url_end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
            let url = rest[..url_end].trim_end_matches('/').to_string();
            if url.starts_with("http://127.0.0.1:") {
                return Some(url);
            }
        }
    }
    None
}
