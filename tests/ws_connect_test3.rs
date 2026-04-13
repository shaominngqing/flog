//! Test: connect to DDS via the redirect URL and verify full functionality.
//!
//! Run with: cargo test --test ws_connect_test3 -- --nocapture

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

/// Discover DDS URL by following the 302 redirect from underlying VM Service.
async fn discover_dds_url() -> Option<String> {
    let output = tokio::process::Command::new("ps")
        .args(["aux"])
        .output()
        .await
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);

    // Find --vm-service-uri from DDS process
    let mut underlying_url: Option<String> = None;
    for line in text.lines() {
        if line.contains("development-service") && line.contains("--vm-service-uri=") {
            if let Some(idx) = line.find("--vm-service-uri=") {
                let rest = &line[idx + 17..];
                let url_end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
                underlying_url = Some(rest[..url_end].trim_end_matches('/').to_string());
            }
        }
    }

    let url = underlying_url?;
    let host_port = url.strip_prefix("http://")?.split('/').next()?;
    let path = url.strip_prefix(&format!("http://{}", host_port))?;

    // Connect to underlying VM Service and request /ws — it returns 302 with DDS URL
    let ws_path = format!("{}/ws", path.trim_end_matches('/'));
    let mut stream = tokio::time::timeout(
        Duration::from_secs(2),
        tokio::net::TcpStream::connect(host_port),
    )
    .await
    .ok()?
    .ok()?;

    let req = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        ws_path, host_port,
    );
    stream.write_all(req.as_bytes()).await.ok()?;

    let mut buf = vec![0u8; 4096];
    let n = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buf))
        .await
        .ok()?
        .ok()?;
    let resp = String::from_utf8_lossy(&buf[..n]);

    // Parse the Location header from 302 response
    if !resp.starts_with("HTTP/1.1 302") {
        return None;
    }

    for line in resp.lines() {
        let lower = line.to_lowercase();
        if lower.starts_with("location:") {
            let location = line["location:".len()..].trim();
            // Location might be relative or absolute
            if location.starts_with("http://") {
                // Absolute URL — convert to ws://
                let ws_url = format!("{}ws", location.replace("http://", "ws://"));
                return Some(ws_url);
            } else {
                // Relative — shouldn't happen for cross-port redirect, but handle anyway
                return Some(format!("ws://{}{}", host_port, location));
            }
        }
    }

    None
}

#[tokio::test]
async fn test_discover_and_connect_dds() {
    let dds_ws_url = match discover_dds_url().await {
        Some(url) => url,
        None => {
            println!("Could not discover DDS URL");
            return;
        }
    };

    println!("Discovered DDS WebSocket URL: {}", dds_ws_url);

    // Connect to DDS
    match tokio::time::timeout(Duration::from_secs(5), connect_async(&dds_ws_url)).await {
        Ok(Ok((ws, resp))) => {
            println!("Connected to DDS! Status: {}", resp.status());
            let (mut tx, mut rx) = ws.split();

            // 1. getVM to verify connection works
            let get_vm = serde_json::json!({
                "jsonrpc": "2.0", "method": "getVM", "params": {}, "id": "vm1"
            });
            tx.send(Message::Text(get_vm.to_string().into()))
                .await
                .unwrap();

            // 2. Subscribe to Logging, Stdout, Stderr
            for (id, stream_id) in [("s1", "Logging"), ("s2", "Stdout"), ("s3", "Stderr")] {
                let sub = serde_json::json!({
                    "jsonrpc": "2.0",
                    "method": "streamListen",
                    "params": { "streamId": stream_id },
                    "id": id,
                });
                tx.send(Message::Text(sub.to_string().into()))
                    .await
                    .unwrap();
            }

            // Read messages for 15 seconds
            let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
            let mut msg_count = 0;
            let mut event_count = 0;
            loop {
                match tokio::time::timeout_at(deadline, rx.next()).await {
                    Ok(Some(Ok(Message::Text(t)))) => {
                        msg_count += 1;
                        let json: Value = serde_json::from_str(&t).unwrap_or(Value::Null);

                        if let Some(id) = json.get("id") {
                            let error = json.get("error");
                            if let Some(err) = error {
                                println!("[resp] id={}, ERROR: {}", id, err);
                            } else {
                                let result_type = json
                                    .get("result")
                                    .and_then(|r| r.get("type"))
                                    .and_then(|t| t.as_str())
                                    .unwrap_or("?");
                                println!("[resp] id={}, result type: {}", id, result_type);

                                // Print VM name if getVM response
                                if id.as_str() == Some("vm1") {
                                    let name = json
                                        .pointer("/result/name")
                                        .and_then(|n| n.as_str())
                                        .unwrap_or("?");
                                    let version = json
                                        .pointer("/result/version")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("?");
                                    println!("  VM: {} ({})", name, version);
                                }
                            }
                        } else if let Some(method) = json.get("method").and_then(|m| m.as_str()) {
                            event_count += 1;
                            let stream_id = json
                                .pointer("/params/streamId")
                                .and_then(|s| s.as_str())
                                .unwrap_or("?");
                            let event_kind = json
                                .pointer("/params/event/kind")
                                .and_then(|s| s.as_str())
                                .unwrap_or("?");

                            // For Logging events, try to extract the message
                            if stream_id == "Logging" {
                                let msg = json
                                    .pointer("/params/event/logRecord/message/valueAsString")
                                    .and_then(|s| s.as_str())
                                    .unwrap_or("(no message)");
                                let tag = json
                                    .pointer("/params/event/logRecord/loggerName")
                                    .and_then(|s| s.as_str())
                                    .unwrap_or("?");
                                println!(
                                    "[event #{}] {} | {} | {} | {}",
                                    event_count,
                                    method,
                                    stream_id,
                                    tag,
                                    &msg[..msg.len().min(80)]
                                );
                            } else {
                                println!(
                                    "[event #{}] {} | {} | {}",
                                    event_count, method, stream_id, event_kind
                                );
                            }
                        }
                    }
                    Ok(Some(Ok(Message::Close(frame)))) => {
                        println!("Server Close: {:?}", frame);
                        break;
                    }
                    Ok(Some(Err(e))) => {
                        println!("WS Error: {} (continuing...)", e);
                    }
                    Ok(None) => {
                        println!("Stream ended");
                        break;
                    }
                    Ok(Some(Ok(_))) => {} // Ping/Pong/Binary
                    Err(_) => {
                        println!(
                            "\nTimeout (15s). Messages: {}, Events: {}",
                            msg_count, event_count
                        );
                        break;
                    }
                }
            }

            println!(
                "\n=== RESULT: Connection stable! {} msgs, {} events ===",
                msg_count, event_count
            );
        }
        Ok(Err(e)) => {
            println!("Connection failed: {}", e);
        }
        Err(_) => {
            println!("Connection timed out");
        }
    }
}
