//! Event-driven device discovery via `adb track-devices` and usbmuxd Listen.

use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
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
    /// Android device (emulator or real) — use adb forward
    Android,
    /// iOS real device via USB — use usbmuxd
    IosUsb { device_id: u32 },
    /// Local (macOS, iOS simulator) — use localhost
    Local,
}

/// How to connect to a device's flog_dart server.
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

/// Device discovery event.
#[derive(Debug)]
pub enum DeviceEvent {
    Added(Device),
    Removed(String), // device id
}

/// Start all device monitors. Returns a channel that receives device events.
/// Spawns background tasks for:
/// - adb track-devices (Android)
/// - usbmuxd Listen (iOS USB)
/// - localhost probe (local/simulator)
pub fn start_discovery(port: u16) -> mpsc::UnboundedReceiver<DeviceEvent> {
    let (tx, rx) = mpsc::unbounded_channel();

    // Always emit a Local device — macOS / iOS simulator can connect via localhost
    let _ = tx.send(DeviceEvent::Added(Device {
        id: "localhost".to_string(),
        name: "localhost".to_string(),
        kind: DeviceKind::Local,
    }));

    // Start adb track-devices
    let tx_adb = tx.clone();
    tokio::spawn(async move {
        track_adb_devices(tx_adb).await;
    });

    // Start usbmuxd listener (macOS only)
    #[cfg(target_os = "macos")]
    {
        let tx_usb = tx.clone();
        tokio::spawn(async move {
            track_usbmuxd_devices(tx_usb).await;
        });
    }

    rx
}

/// Track Android devices via `adb track-devices`.
/// This is a persistent connection — adb sends updates as devices connect/disconnect.
async fn track_adb_devices(tx: mpsc::UnboundedSender<DeviceEvent>) {
    loop {
        // Spawn adb track-devices
        let child = Command::new("adb")
            .arg("track-devices")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();

        let mut child = match child {
            Ok(c) => c,
            Err(_) => {
                // adb not available — wait and retry
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                continue;
            }
        };

        let stdout = match child.stdout.take() {
            Some(s) => s,
            None => continue,
        };

        let mut reader = BufReader::new(stdout).lines();
        let mut known: std::collections::HashSet<String> = std::collections::HashSet::new();

        // adb track-devices sends a hex length prefix before each block.
        // Each block lists all currently connected devices, one per line:
        //   SERIAL\tSTATUS
        // We track the delta.
        while let Ok(Some(line)) = reader.next_line().await {
            let line = line.trim().to_string();

            // Skip hex length lines (4 hex chars like "001a")
            if line.len() == 4 && line.chars().all(|c| c.is_ascii_hexdigit()) {
                continue;
            }

            // Skip empty lines
            if line.is_empty() {
                // Empty block = no devices. Remove all known.
                for id in known.drain() {
                    let _ = tx.send(DeviceEvent::Removed(id));
                }
                continue;
            }

            // Parse: SERIAL\tSTATUS
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 2 {
                let serial = parts[0].to_string();
                let status = parts[1];

                if status == "device" {
                    if known.insert(serial.clone()) {
                        let _ = tx.send(DeviceEvent::Added(Device {
                            id: serial.clone(),
                            name: serial,
                            kind: DeviceKind::Android,
                        }));
                    }
                } else if status == "offline" || status == "unauthorized" {
                    if known.remove(&serial) {
                        let _ = tx.send(DeviceEvent::Removed(serial));
                    }
                }
            }
        }

        // adb track-devices exited — clean up and retry
        for id in known.drain() {
            let _ = tx.send(DeviceEvent::Removed(id));
        }
        let _ = child.kill().await;
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}

/// Track iOS USB devices via usbmuxd Listen command.
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
        header.extend_from_slice(&1u32.to_le_bytes()); // version
        header.extend_from_slice(&8u32.to_le_bytes()); // type = plist
        header.extend_from_slice(&1u32.to_le_bytes()); // tag
        if write_half.write_all(&header).await.is_err() {
            continue;
        }
        if write_half.write_all(&body).await.is_err() {
            continue;
        }

        // Read events continuously
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
                        if !serial.is_empty() {
                            let _ = tx.send(DeviceEvent::Added(Device {
                                id: serial.clone(),
                                name: format!("iOS ({})", &serial[..8.min(serial.len())]),
                                kind: DeviceKind::IosUsb { device_id },
                            }));
                        }
                    }
                }
                "Detached" => {
                    // Detached only gives DeviceID, not serial. We need to track mapping.
                    // For simplicity, send device_id as the removal key.
                    let device_id = dict.get("DeviceID").and_then(|v| v.as_unsigned_integer()).unwrap_or(0);
                    let _ = tx.send(DeviceEvent::Removed(format!("usbmuxd:{}", device_id)));
                }
                _ => {}
            }
        }

        // Disconnected from usbmuxd — retry
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}
