//! Localhost probe source: scans base_port..base_port+9 for a
//! running flog_dart WS server on the macOS host or iOS simulator.

use super::{Device, DeviceEvent, DeviceKind};
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::mpsc;

/// flog_dart binds `base_port..base_port+9` — we scan the same range.
const PORT_SCAN_RANGE: u16 = 10;
const POLL_INTERVAL: Duration = Duration::from_secs(1);
const TCP_TIMEOUT: Duration = Duration::from_millis(200);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(3);

/// Probe `base_port..base_port+9` on loopback to detect a macOS app or
/// iOS simulator running flog_dart.
///
/// A listening port alone isn't proof of flog — any process can bind it.
/// On a transition from "no ports open" to "some open", we do a one-shot
/// WS handshake on the first open port and only emit Added if the peer
/// sends a valid `hello` frame. Ports that fail handshake are remembered
/// as "not flog" until they close (then we'll retry).
///
/// Identity (os, app name) comes from the hello frame, never from guessing.
pub async fn probe(tx: mpsc::UnboundedSender<DeviceEvent>, base_port: u16) {
    use futures_util::future::join_all;

    let ports: Vec<u16> = (0..PORT_SCAN_RANGE).map(|o| base_port + o).collect();
    let mut verified = false;
    let mut non_flog: std::collections::HashSet<u16> = std::collections::HashSet::new();

    loop {
        // Parallel TCP scan — total latency stays bounded by TCP_TIMEOUT
        // regardless of how many ports we check.
        let open_ports: Vec<u16> = join_all(ports.iter().map(|&p| tcp_open(p)))
            .await
            .into_iter()
            .flatten()
            .collect();

        // A port that closed and reopens deserves a fresh handshake
        // attempt — drop it from the "not flog" memo.
        non_flog.retain(|p| open_ports.contains(p));

        if verified {
            if open_ports.is_empty() {
                let _ = tx.send(DeviceEvent::Removed("localhost".to_string()));
                verified = false;
                non_flog.clear();
            }
        } else {
            for &p in &open_ports {
                if non_flog.contains(&p) {
                    continue;
                }
                match handshake(p).await {
                    Some(hello) => {
                        let _ = tx.send(DeviceEvent::Added(Device {
                            id: "localhost".to_string(),
                            name: device_name(&hello).await,
                            kind: DeviceKind::Local,
                        }));
                        verified = true;
                        break;
                    }
                    None => {
                        non_flog.insert(p);
                    }
                }
            }
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Returns true when a TCP connection to 127.0.0.1:{port} succeeds within
/// `TCP_TIMEOUT`. The timeout future yields `Result<std::io::Result<_>, _>`
/// — the outer `Result` carries "timed out" and the inner carries the
/// connect result. `is_ok_and(Result::is_ok)` accepts only the
/// "both finished cleanly and stream was built" branch.
///
/// Audit ref: TRANS-007 (previously the inline `Ok(Ok(_))` pattern was
/// correct but fragile-looking).
async fn is_port_open(port: u16) -> bool {
    let addr = format!("127.0.0.1:{}", port);
    tokio::time::timeout(TCP_TIMEOUT, tokio::net::TcpStream::connect(&addr))
        .await
        .is_ok_and(|r| r.is_ok())
}

async fn tcp_open(port: u16) -> Option<u16> {
    if is_port_open(port).await {
        Some(port)
    } else {
        None
    }
}

struct Hello {
    os: String,
    app: String,
}

/// Confirm that the peer on `port` is a flog_dart server by reading its
/// `hello` frame. Closes the connection immediately — main.rs owns the
/// real long-lived session (which would otherwise receive a full replay
/// on every probe).
async fn handshake(port: u16) -> Option<Hello> {
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::Message;

    let url = format!("ws://127.0.0.1:{}", port);
    let (ws, _) = tokio::time::timeout(HANDSHAKE_TIMEOUT, tokio_tungstenite::connect_async(&url))
        .await
        .ok()?
        .ok()?;
    let (_sink, mut read) = ws.split();

    let first = tokio::time::timeout(HANDSHAKE_TIMEOUT, read.next())
        .await
        .ok()??
        .ok()?;
    let text = match first {
        Message::Text(t) => t,
        _ => return None,
    };
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    if v.get("type").and_then(|t| t.as_str()) != Some("hello") {
        return None;
    }
    Some(Hello {
        os: v
            .get("os")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string(),
        app: v
            .get("app")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string(),
    })
    // ws drops here — the kernel closes the connection.
}

async fn device_name(hello: &Hello) -> String {
    // The tag in the UI already says "Simulator" — here we return the
    // concrete simulator model when simctl knows about a booted one,
    // otherwise a short fallback.
    match hello.os.to_lowercase().as_str() {
        "ios" => booted_simulator_name()
            .await
            .unwrap_or_else(|| "Simulator".to_string()),
        _ if !hello.app.is_empty() => hello.app.clone(),
        _ => "Simulator".to_string(),
    }
}

/// Ask `simctl` for the name of the currently-booted simulator, if any.
async fn booted_simulator_name() -> Option<String> {
    let out = Command::new("xcrun")
        .args(["simctl", "list", "devices", "booted", "--json"])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_simctl_booted(&out.stdout)
}

/// Parse `xcrun simctl list devices booted --json` stdout and return the
/// first booted device's name. Extracted as a pure helper so the JSON
/// traversal is covered without invoking xcrun.
pub(super) fn parse_simctl_booted(stdout: &[u8]) -> Option<String> {
    let json: serde_json::Value = serde_json::from_slice(stdout).ok()?;
    let runtimes = json.get("devices")?.as_object()?;
    for devices in runtimes.values() {
        let Some(arr) = devices.as_array() else {
            continue;
        };
        for dev in arr {
            if dev.get("state").and_then(|s| s.as_str()) == Some("Booted") {
                if let Some(name) = dev.get("name").and_then(|n| n.as_str()) {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests;
