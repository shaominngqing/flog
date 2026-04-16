//! Event-driven device discovery.
//!
//! Three parallel sources, all feeding into one DeviceEvent channel:
//! 1. `adb track-devices` — persistent connection, instant Android events
//! 2. usbmuxd `Listen` — persistent connection, instant iOS USB events
//! 3. localhost probe — 1s poll, covers macOS + iOS simulator

use std::process::Stdio;
use tokio::process::Command;
use tokio::sync::mpsc;

/// A discovered device.
#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub kind: DeviceKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeviceKind {
    Android,
    IosUsb { device_id: u32 },
    Local,
}

pub enum ConnectionMethod {
    Localhost,
    AdbForward { serial: String },
    Usbmuxd { device_id: u32 },
}

impl Device {
    pub fn connection_method(&self) -> ConnectionMethod {
        match &self.kind {
            DeviceKind::Android => ConnectionMethod::AdbForward { serial: self.id.clone() },
            DeviceKind::IosUsb { device_id } => ConnectionMethod::Usbmuxd { device_id: *device_id },
            DeviceKind::Local => ConnectionMethod::Localhost,
        }
    }
}

#[derive(Debug)]
pub enum DeviceEvent {
    Added(Device),
    Removed(String),
}

/// Start all device discovery sources in parallel.
/// Returns a channel that receives device events from all sources.
pub fn start_discovery(port: u16) -> mpsc::UnboundedReceiver<DeviceEvent> {
    let (tx, rx) = mpsc::unbounded_channel();

    // Source 1: adb track-devices (Android)
    let tx1 = tx.clone();
    tokio::spawn(async move {
        track_adb_devices(tx1).await;
    });

    // Source 2: usbmuxd Listen (iOS USB, macOS only)
    #[cfg(target_os = "macos")]
    {
        let tx2 = tx.clone();
        tokio::spawn(async move {
            track_usbmuxd_devices(tx2).await;
        });
    }

    // Source 3: localhost probe (macOS / iOS simulator)
    let tx3 = tx.clone();
    tokio::spawn(async move {
        probe_localhost(tx3, port).await;
    });

    rx
}

// ── Source 1: adb track-devices ──

async fn track_adb_devices(tx: mpsc::UnboundedSender<DeviceEvent>) {
    use tokio::io::AsyncReadExt;

    loop {
        let child = Command::new("adb")
            .arg("track-devices")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();

        let mut child = match child {
            Ok(c) => c,
            Err(_) => {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                continue;
            }
        };

        let mut stdout = match child.stdout.take() {
            Some(s) => s,
            None => continue,
        };

        let mut known: std::collections::HashSet<String> = std::collections::HashSet::new();

        // adb track-devices protocol: each update is 4-char hex length + content
        loop {
            // Read 4-byte hex length
            let mut hex_buf = [0u8; 4];
            if stdout.read_exact(&mut hex_buf).await.is_err() {
                break;
            }
            let hex_str = match std::str::from_utf8(&hex_buf) {
                Ok(s) => s,
                Err(_) => break,
            };
            let content_len = match usize::from_str_radix(hex_str, 16) {
                Ok(n) => n,
                Err(_) => break,
            };

            // Read content
            let mut content = vec![0u8; content_len];
            if content_len > 0 {
                if stdout.read_exact(&mut content).await.is_err() {
                    break;
                }
            }

            let text = match String::from_utf8(content) {
                Ok(s) => s,
                Err(_) => continue,
            };

            // Parse device list — each line is SERIAL\tSTATUS
            let mut current: std::collections::HashSet<String> = std::collections::HashSet::new();
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() >= 2 && parts[1] == "device" {
                    current.insert(parts[0].to_string());
                }
            }

            // Diff: find added and removed
            for serial in &current {
                if known.insert(serial.clone()) {
                    let name = adb_device_name(serial).await;
                    let _ = tx.send(DeviceEvent::Added(Device {
                        id: serial.clone(),
                        name,
                        kind: DeviceKind::Android,
                    }));
                }
            }
            let removed: Vec<String> = known.iter().filter(|s| !current.contains(*s)).cloned().collect();
            for serial in removed {
                known.remove(&serial);
                let _ = tx.send(DeviceEvent::Removed(serial));
            }
        }

        // Process exited — cleanup
        for id in known.drain() {
            let _ = tx.send(DeviceEvent::Removed(id));
        }
        let _ = child.kill().await;
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}

/// Query Android device model via adb shell getprop.
async fn adb_device_name(serial: &str) -> String {
    let model = adb_getprop(serial, "ro.product.model").await;
    let brand = adb_getprop(serial, "ro.product.brand").await;
    match (brand, model) {
        (Some(b), Some(m)) => {
            // Capitalize brand first letter
            let brand_cap = capitalize_first(&b);
            // Avoid duplication like "Samsung Samsung Galaxy S24"
            if m.to_lowercase().starts_with(&b.to_lowercase()) {
                m
            } else {
                format!("{} {}", brand_cap, m)
            }
        }
        (None, Some(m)) => m,
        (Some(b), None) => capitalize_first(&b),
        (None, None) => serial.to_string(),
    }
}

async fn adb_getprop(serial: &str, prop: &str) -> Option<String> {
    let output = Command::new("adb")
        .args(["-s", serial, "shell", "getprop", prop])
        .output()
        .await
        .ok()?;
    if output.status.success() {
        let val = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if val.is_empty() { None } else { Some(val) }
    } else {
        None
    }
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

// ── Source 2: usbmuxd Listen (macOS only) ──

#[cfg(target_os = "macos")]
async fn track_usbmuxd_devices(tx: mpsc::UnboundedSender<DeviceEvent>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    const USBMUXD_SOCKET: &str = "/var/run/usbmuxd";
    const HEADER_SIZE: usize = 16;

    loop {
        let stream = match UnixStream::connect(USBMUXD_SOCKET).await {
            Ok(s) => s,
            Err(_) => {
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                continue;
            }
        };

        let (mut read_half, mut write_half) = stream.into_split();

        // Send Listen request
        let request = plist::Value::Dictionary({
            let mut d = plist::Dictionary::new();
            d.insert("MessageType".into(), "Listen".into());
            d.insert("ClientVersionString".into(), "flog".into());
            d.insert("ProgName".into(), "flog".into());
            d
        });
        let mut body = Vec::new();
        if request.to_writer_xml(&mut body).is_err() {
            continue;
        }
        let length = (HEADER_SIZE + body.len()) as u32;
        let mut header = Vec::with_capacity(HEADER_SIZE);
        header.extend_from_slice(&length.to_le_bytes());
        header.extend_from_slice(&1u32.to_le_bytes());
        header.extend_from_slice(&8u32.to_le_bytes());
        header.extend_from_slice(&1u32.to_le_bytes());
        if write_half.write_all(&header).await.is_err() || write_half.write_all(&body).await.is_err() {
            continue;
        }

        loop {
            let mut hdr = [0u8; HEADER_SIZE];
            if read_half.read_exact(&mut hdr).await.is_err() {
                break;
            }
            let length = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]) as usize;
            let body_len = length.saturating_sub(HEADER_SIZE);
            let mut body = vec![0u8; body_len];
            if read_half.read_exact(&mut body).await.is_err() {
                break;
            }

            let value = match plist::Value::from_reader(std::io::Cursor::new(body)) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let dict = match value.as_dictionary() {
                Some(d) => d,
                None => continue,
            };

            let msg_type = dict.get("MessageType").and_then(|v| v.as_string()).unwrap_or("");
            match msg_type {
                "Attached" => {
                    if let Some(props) = dict.get("Properties").and_then(|v| v.as_dictionary()) {
                        let device_id = props.get("DeviceID").and_then(|v| v.as_unsigned_integer()).unwrap_or(0) as u32;
                        let serial = props.get("SerialNumber").and_then(|v| v.as_string()).unwrap_or("").to_string();
                        let device_name = props.get("DeviceName").and_then(|v| v.as_string()).unwrap_or("").to_string();
                        if !serial.is_empty() {
                            let name = if device_name.is_empty() {
                                format!("iOS ({})", &serial[..8.min(serial.len())])
                            } else {
                                device_name
                            };
                            let _ = tx.send(DeviceEvent::Added(Device {
                                id: serial.clone(),
                                name,
                                kind: DeviceKind::IosUsb { device_id },
                            }));
                        }
                    }
                }
                "Detached" => {
                    let device_id = dict.get("DeviceID").and_then(|v| v.as_unsigned_integer()).unwrap_or(0);
                    let _ = tx.send(DeviceEvent::Removed(format!("usbmuxd:{}", device_id)));
                }
                _ => {}
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}

// ── Source 3: localhost probe ──

/// Periodically try to connect to localhost:{port} to detect
/// macOS apps and iOS simulator apps.
async fn probe_localhost(tx: mpsc::UnboundedSender<DeviceEvent>, port: u16) {
    let addr = format!("127.0.0.1:{}", port);
    let mut was_reachable = false;

    loop {
        // Quick TCP probe — just check if something is listening
        let reachable = tokio::time::timeout(
            std::time::Duration::from_millis(200),
            tokio::net::TcpStream::connect(&addr),
        )
        .await
        .is_ok();

        if reachable && !was_reachable {
            let name = detect_local_device_name().await;
            let _ = tx.send(DeviceEvent::Added(Device {
                id: "localhost".to_string(),
                name,
                kind: DeviceKind::Local,
            }));
            was_reachable = true;
        } else if !reachable && was_reachable {
            let _ = tx.send(DeviceEvent::Removed("localhost".to_string()));
            was_reachable = false;
        }

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

/// Detect the name of the local device (booted simulator or macOS).
async fn detect_local_device_name() -> String {
    // Try to find a booted iOS simulator
    if let Some(name) = booted_simulator_name().await {
        return name;
    }
    "macOS".to_string()
}

/// Query `xcrun simctl list devices booted` for the booted simulator name.
async fn booted_simulator_name() -> Option<String> {
    let output = Command::new("xcrun")
        .args(["simctl", "list", "devices", "booted", "--json"])
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let devices = json.get("devices")?.as_object()?;
    // Find first booted device across all runtimes
    for (_runtime, device_list) in devices {
        if let Some(arr) = device_list.as_array() {
            for dev in arr {
                if dev.get("state").and_then(|s| s.as_str()) == Some("Booted") {
                    if let Some(name) = dev.get("name").and_then(|n| n.as_str()) {
                        return Some(name.to_string());
                    }
                }
            }
        }
    }
    None
}
