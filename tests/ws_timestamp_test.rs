//! Test: inspect Stdout event structure to check if timestamp exists.
//!
//! Run with: cargo test --test ws_timestamp_test -- --nocapture

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

async fn discover_dds_url() -> Option<String> {
    let output = tokio::process::Command::new("ps")
        .args(["aux"]).output().await.ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if line.contains("development-service") {
            if let Some(idx) = line.find("--vm-service-uri=") {
                let rest = &line[idx + 17..];
                let url_end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
                let http_url = rest[..url_end].trim_end_matches('/').to_string();
                if http_url.starts_with("http://127.0.0.1:") {
                    let host_port = http_url.strip_prefix("http://")?.split('/').next()?;
                    let path = http_url.strip_prefix(&format!("http://{}", host_port))?;
                    let ws_path = format!("{}/ws", path.trim_end_matches('/'));
                    let mut stream = tokio::time::timeout(
                        Duration::from_secs(2),
                        tokio::net::TcpStream::connect(host_port),
                    ).await.ok()?.ok()?;
                    let req = format!("GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n", ws_path, host_port);
                    stream.write_all(req.as_bytes()).await.ok()?;
                    let mut buf = vec![0u8; 4096];
                    let n = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buf)).await.ok()?.ok()?;
                    let resp = String::from_utf8_lossy(&buf[..n]);
                    if resp.starts_with("HTTP/1.1 302") {
                        for line in resp.lines() {
                            if line.to_lowercase().starts_with("location:") {
                                let loc = line["location:".len()..].trim();
                                if loc.starts_with("http://") {
                                    return Some(format!("{}ws", loc.replace("http://", "ws://")));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

#[tokio::test]
async fn test_inspect_stdout_and_logging_events() {
    let ws_url = match discover_dds_url().await {
        Some(u) => u,
        None => { println!("No DDS found"); return; }
    };

    println!("Connecting to: {}", ws_url);
    let (ws, _) = connect_async(&ws_url).await.unwrap();
    let (mut tx, mut rx) = ws.split();

    // Subscribe to all streams
    for (id, stream_id) in [("1", "Logging"), ("2", "Stdout"), ("3", "Stderr")] {
        let sub = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "streamListen",
            "params": { "streamId": stream_id },
            "id": id,
        });
        tx.send(Message::Text(sub.to_string().into())).await.unwrap();
    }

    // Collect events for 10 seconds, print FULL JSON of first few Stdout and Logging events
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut stdout_count = 0;
    let mut logging_count = 0;

    loop {
        match tokio::time::timeout_at(deadline, rx.next()).await {
            Ok(Some(Ok(Message::Text(t)))) => {
                let json: Value = match serde_json::from_str(&t) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                // Skip responses
                if json.get("id").is_some() { continue; }

                let method = json.get("method").and_then(|m| m.as_str()).unwrap_or("");
                if method != "streamNotify" { continue; }

                let stream_id = json.pointer("/params/streamId").and_then(|s| s.as_str()).unwrap_or("?");
                let event = json.pointer("/params/event");

                match stream_id {
                    "Stdout" | "Stderr" => {
                        stdout_count += 1;
                        if stdout_count <= 3 {
                            println!("\n=== {} event #{} (FULL JSON) ===", stream_id, stdout_count);
                            if let Some(evt) = event {
                                println!("{}", serde_json::to_string_pretty(evt).unwrap_or_default());

                                // Also decode the bytes to show the text
                                if let Some(bytes) = evt.get("bytes").and_then(|b| b.as_str()) {
                                    if let Ok(decoded) = base64_decode_simple(bytes) {
                                        println!("--- decoded text ---");
                                        println!("{}", String::from_utf8_lossy(&decoded));
                                    }
                                }

                                // Print ALL top-level keys
                                println!("--- top-level keys ---");
                                if let Some(obj) = evt.as_object() {
                                    for (k, v) in obj {
                                        let preview = v.to_string();
                                        println!("  {}: {}", k, &preview[..preview.len().min(100)]);
                                    }
                                }
                            }
                        }
                    }
                    "Logging" => {
                        logging_count += 1;
                        if logging_count <= 3 {
                            println!("\n=== Logging event #{} (FULL JSON) ===", logging_count);
                            if let Some(evt) = event {
                                println!("{}", serde_json::to_string_pretty(evt).unwrap_or_default());

                                // Print ALL top-level keys
                                println!("--- top-level keys ---");
                                if let Some(obj) = evt.as_object() {
                                    for (k, v) in obj {
                                        let preview = v.to_string();
                                        println!("  {}: {}", k, &preview[..preview.len().min(100)]);
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Some(Ok(_))) => continue,
            Ok(Some(Err(e))) => { println!("Error: {}", e); continue; }
            Ok(None) => { println!("Stream ended"); break; }
            Err(_) => {
                println!("\nTimeout. Stdout events: {}, Logging events: {}", stdout_count, logging_count);
                break;
            }
        }
    }
}

fn base64_decode_simple(input: &str) -> Result<Vec<u8>, ()> {
    let table = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut buf = Vec::new();
    let mut bits: u32 = 0;
    let mut bit_count: u32 = 0;
    for &b in input.as_bytes() {
        if b == b'=' || b == b'\n' || b == b'\r' { continue; }
        let val = table.iter().position(|&c| c == b).ok_or(())? as u32;
        bits = (bits << 6) | val;
        bit_count += 6;
        if bit_count >= 8 {
            bit_count -= 8;
            buf.push((bits >> bit_count) as u8);
            bits &= (1 << bit_count) - 1;
        }
    }
    Ok(buf)
}
