//! Flutter VM Service WebSocket input source.

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use super::SourceEvent;
use crate::domain::{InputSource, LogEntry, LogLevel};

// Note: parse_stdout_event returns raw lines (not LogEntry) so they go through
// the MultiStrategyParser chain for proper level/tag extraction.

pub struct VmServiceSource {
    receiver: futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
    uri: String,
    /// Buffered stdout lines from Stdout/Stderr events (one WS message can contain multiple lines).
    pending_lines: Vec<StdoutLine>,
}

impl VmServiceSource {
    /// Connect to VM Service. If `proxy_port` is Some, attempts to notify
    /// Dart about the proxy via service extension. Returns (Self, proxy_ok).
    pub async fn new(
        uri: &str,
        proxy_port: Option<u16>,
    ) -> Result<(Self, bool), Box<dyn std::error::Error + Send + Sync>> {
        let (mut ws_stream, _) = connect_async(uri).await?;

        // ── Phase 1: Get isolate ID (before split) ──
        let mut isolate_id: Option<String> = None;
        let mut proxy_ok = false;

        // Send getVM
        let get_vm = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "getVM",
            "id": "flog_getvm"
        });
        ws_stream.send(Message::Text(get_vm.to_string().into())).await.ok();

        // Read responses until we get the getVM result (max 3 seconds)
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout(std::time::Duration::from_millis(500), ws_stream.next()).await {
                Ok(Some(Ok(Message::Text(text)))) => {
                    if let Ok(json) = serde_json::from_str::<Value>(&text.to_string()) {
                        if json.get("id").and_then(|v| v.as_str()) == Some("flog_getvm") {
                            if let Some(isolates) = json.get("result").and_then(|r| r.get("isolates")) {
                                if let Some(first) = isolates.as_array().and_then(|a| a.first()) {
                                    isolate_id = first.get("id").and_then(|v| v.as_str()).map(|s| s.to_string());
                                }
                            }
                            break;
                        }
                    }
                }
                Ok(Some(Ok(_))) | Ok(Some(Err(_))) => continue,
                _ => break,
            }
        }

        // ── Phase 2: Call ext.flog.setProxy with host IP + port ──
        if let (Some(ref iso_id), Some(port)) = (&isolate_id, proxy_port) {
            // Get local network IP so real devices can reach the proxy
            let host_ip = get_local_ip().unwrap_or_else(|| "127.0.0.1".to_string());

            let call = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "ext.flog.setProxy",
                "params": {
                    "isolateId": iso_id,
                    "host": host_ip,
                    "port": port.to_string(),
                },
                "id": "flog_setproxy"
            });
            ws_stream.send(Message::Text(call.to_string().into())).await.ok();

            // Wait for response
            let deadline2 = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
            while tokio::time::Instant::now() < deadline2 {
                match tokio::time::timeout(std::time::Duration::from_millis(500), ws_stream.next()).await {
                    Ok(Some(Ok(Message::Text(text)))) => {
                        if let Ok(json) = serde_json::from_str::<Value>(&text.to_string()) {
                            if json.get("id").and_then(|v| v.as_str()) == Some("flog_setproxy") {
                                proxy_ok = json.get("error").is_none();
                                break;
                            }
                        }
                    }
                    Ok(Some(Ok(_))) | Ok(Some(Err(_))) => continue,
                    _ => break,
                }
            }
        }

        // ── Phase 3: Split and subscribe to streams ──
        let (mut sender, receiver) = ws_stream.split();

        for (id, stream_id) in [("1", "Logging"), ("2", "Stdout"), ("3", "Stderr")] {
            let msg = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "streamListen",
                "params": { "streamId": stream_id },
                "id": id
            });
            let _ = sender.send(Message::Text(msg.to_string().into())).await;
        }

        Ok((Self {
            receiver,
            uri: uri.to_string(),
            pending_lines: Vec::new(),
        }, proxy_ok))
    }

    pub async fn next_event(&mut self) -> Option<SourceEvent> {
        // Drain buffered stdout lines first
        if let Some(sl) = self.pending_lines.pop() {
            return Some(SourceEvent::RawLineWithTimestamp(sl.text, sl.timestamp));
        }

        loop {
            let msg = match self.receiver.next().await {
                Some(Ok(msg)) => msg,
                Some(Err(_)) => continue, // transient error, keep going
                None => return None,      // stream closed
            };

            let text = match msg {
                Message::Text(t) => t,
                Message::Close(_) => return None,
                Message::Ping(_) | Message::Pong(_) => continue,
                _ => continue,
            };

            let json: Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(_) => continue,
            };

            match parse_vm_event(&json) {
                Some(VmEventResult::Single(entry)) => {
                    return Some(SourceEvent::ParsedEntry(entry));
                }
                Some(VmEventResult::StdoutLines(mut lines)) => {
                    // Reverse so pop() yields lines in original order
                    lines.reverse();
                    if let Some(first) = lines.pop() {
                        self.pending_lines = lines;
                        return Some(SourceEvent::RawLineWithTimestamp(
                            first.text,
                            first.timestamp,
                        ));
                    }
                }
                None => {} // Not a log event — skip
            }
        }
    }

    pub fn name(&self) -> &str {
        &self.uri
    }
}

/// Parse result from a VM Service event — can produce multiple source events.
enum VmEventResult {
    /// A single parsed log entry (from Logging stream).
    Single(LogEntry),
    /// Raw lines with timestamps to be parsed by MultiStrategyParser (from Stdout/Stderr).
    StdoutLines(Vec<StdoutLine>),
}

fn parse_vm_event(json: &Value) -> Option<VmEventResult> {
    if json.get("method")?.as_str()? != "streamNotify" {
        return None;
    }

    let params = json.get("params")?;
    let stream_id = params.get("streamId")?.as_str()?;
    let event = params.get("event")?;

    match stream_id {
        "Logging" => parse_logging_event(event).map(VmEventResult::Single),
        "Stdout" | "Stderr" => parse_stdout_event(event).map(VmEventResult::StdoutLines),
        _ => None,
    }
}

fn parse_logging_event(event: &Value) -> Option<LogEntry> {
    let rec = event.get("logRecord")?;

    let message = rec
        .get("message")
        .and_then(|m| m.get("valueAsString"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let tag = rec
        .get("loggerName")
        .and_then(|v| {
            v.get("valueAsString")
                .and_then(|s| s.as_str())
                .or_else(|| v.as_str())
        })
        .unwrap_or("App")
        .to_string();

    let level = rec
        .get("level")
        .and_then(|v| {
            v.as_i64().or_else(|| {
                v.get("valueAsString")
                    .and_then(|s| s.as_str())
                    .and_then(|s| s.parse::<i64>().ok())
            })
        })
        .map(LogLevel::from_vm_service_level)
        .unwrap_or(LogLevel::Info);

    let time_ms = rec.get("time").and_then(|v| v.as_i64()).unwrap_or(0);
    let timestamp = format_epoch_ms(time_ms);

    let error = rec
        .get("error")
        .and_then(|v| v.get("valueAsString"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let stacktrace = rec
        .get("stackTrace")
        .and_then(|v| v.get("valueAsString"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Some(LogEntry {
        timestamp,
        level,
        tag,
        message,
        extra_lines: Vec::new(),
        repeat_count: 1,
        source: InputSource::VmService,
        error,
        stacktrace,
    })
}

/// Parsed stdout line with timestamp from the VM Service event.
struct StdoutLine {
    timestamp: String,
    text: String,
}

fn parse_stdout_event(event: &Value) -> Option<Vec<StdoutLine>> {
    let bytes = event.get("bytes")?.as_str()?;
    let decoded = base64_decode(bytes)?;
    let text = String::from_utf8_lossy(&decoded).to_string();

    let timestamp = event
        .get("timestamp")
        .and_then(|v| v.as_i64())
        .map(format_epoch_ms)
        .unwrap_or_default();

    let lines: Vec<StdoutLine> = text
        .lines()
        .map(|l| l.to_string())
        .filter(|l| !l.trim().is_empty())
        .map(|l| StdoutLine {
            timestamp: timestamp.clone(),
            text: l,
        })
        .collect();
    if lines.is_empty() {
        return None;
    }
    Some(lines)
}

fn format_epoch_ms(ms: i64) -> String {
    let secs = ms / 1000;
    let millis = (ms % 1000).unsigned_abs();
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        (secs % 86400) / 3600,
        (secs % 3600) / 60,
        secs % 60,
        millis
    )
}

fn base64_decode(input: &str) -> Option<Vec<u8>> {
    let table = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut buf = Vec::new();
    let mut bits: u32 = 0;
    let mut bit_count: u32 = 0;
    for &b in input.as_bytes() {
        if b == b'=' || b == b'\n' || b == b'\r' {
            continue;
        }
        let val = table.iter().position(|&c| c == b)? as u32;
        bits = (bits << 6) | val;
        bit_count += 6;
        if bit_count >= 8 {
            bit_count -= 8;
            buf.push((bits >> bit_count) as u8);
            bits &= (1 << bit_count) - 1;
        }
    }
    Some(buf)
}

/// Get the first non-loopback IPv4 address of this machine.
fn get_local_ip() -> Option<String> {
    use std::net::UdpSocket;
    // Connect to a public address to determine the local IP
    // (no actual data is sent)
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let addr = socket.local_addr().ok()?;
    Some(addr.ip().to_string())
}
