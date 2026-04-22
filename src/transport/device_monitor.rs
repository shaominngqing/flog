//! Event-driven device discovery.
//!
//! Three parallel sources feed a single `DeviceEvent` channel:
//!
//! | Source     | Mechanism                       | Covers                         |
//! |------------|---------------------------------|--------------------------------|
//! | `adb`      | `adb track-devices` stream      | Android real devices + AVDs    |
//! | `usbmuxd`  | `Listen` on unix socket (macOS) | iOS real devices over USB      |
//! | `local`    | TCP probe + WS handshake        | macOS app & iOS simulator      |
//!
//! Each source is self-contained in its own inline module and shares a small
//! helper (`DeviceTracker`) that encapsulates the "known set + emit Added /
//! Removed + drain on disconnect" pattern.

use tokio::sync::mpsc;

// ── Public types ────────────────────────────────────────────────────────────

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
            DeviceKind::Android => ConnectionMethod::AdbForward {
                serial: self.id.clone(),
            },
            DeviceKind::IosUsb { device_id } => ConnectionMethod::Usbmuxd {
                device_id: *device_id,
            },
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
pub fn start_discovery(port: u16) -> mpsc::UnboundedReceiver<DeviceEvent> {
    let (tx, rx) = mpsc::unbounded_channel();

    tokio::spawn(adb_source::track(tx.clone()));

    #[cfg(target_os = "macos")]
    tokio::spawn(usbmuxd_source::track(tx.clone()));

    tokio::spawn(local_source::probe(tx, port));

    rx
}

// ── Shared tracker ──────────────────────────────────────────────────────────

/// Tracks the set of device ids currently reported by one source, dedups
/// Added events, and guarantees every Added has a matching Removed — including
/// when the source's connection dies unexpectedly (via `drain`).
struct DeviceTracker {
    tx: mpsc::UnboundedSender<DeviceEvent>,
    known: std::collections::HashSet<String>,
}

impl DeviceTracker {
    fn new(tx: mpsc::UnboundedSender<DeviceEvent>) -> Self {
        Self {
            tx,
            known: std::collections::HashSet::new(),
        }
    }

    /// Emit Added if the device id hasn't been seen yet. Returns whether a new
    /// event was emitted — callers can skip expensive name queries otherwise.
    fn add(&mut self, device: Device) -> bool {
        if self.known.insert(device.id.clone()) {
            let _ = self.tx.send(DeviceEvent::Added(device));
            true
        } else {
            false
        }
    }

    fn contains(&self, id: &str) -> bool {
        self.known.contains(id)
    }

    fn remove(&mut self, id: &str) {
        if self.known.remove(id) {
            let _ = self.tx.send(DeviceEvent::Removed(id.to_string()));
        }
    }

    /// Returns ids that are in `known` but not in `current`. Useful after
    /// receiving a full state snapshot from the source.
    fn removed_since(&self, current: &std::collections::HashSet<String>) -> Vec<String> {
        self.known
            .iter()
            .filter(|id| !current.contains(*id))
            .cloned()
            .collect()
    }

    /// Emit Removed for every known device and clear the set. Called when the
    /// underlying source disconnects so stale devices don't linger in the UI.
    fn drain(&mut self) {
        for id in self.known.drain() {
            let _ = self.tx.send(DeviceEvent::Removed(id));
        }
    }
}

// ── Source 1: adb track-devices ─────────────────────────────────────────────

mod adb_source {
    use super::{Device, DeviceEvent, DeviceKind, DeviceTracker};
    use std::process::Stdio;
    use std::time::Duration;
    use tokio::io::AsyncReadExt;
    use tokio::process::Command;
    use tokio::sync::mpsc;

    /// Sleep between reconnect attempts when `adb track-devices` dies cleanly.
    const RECONNECT_DELAY: Duration = Duration::from_secs(3);
    /// Sleep when `adb` is not installed — infrequent polling is enough.
    const ADB_MISSING_DELAY: Duration = Duration::from_secs(30);

    /// Follow `adb track-devices` forever, translating its stream into
    /// DeviceEvents. Handles real devices and AVD emulators uniformly.
    pub async fn track(tx: mpsc::UnboundedSender<DeviceEvent>) {
        loop {
            let mut child = match Command::new("adb")
                .arg("track-devices")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(c) => c,
                Err(_) => {
                    tokio::time::sleep(ADB_MISSING_DELAY).await;
                    continue;
                }
            };

            let Some(stdout) = child.stdout.take() else {
                let _ = child.kill().await;
                tokio::time::sleep(RECONNECT_DELAY).await;
                continue;
            };

            let mut tracker = DeviceTracker::new(tx.clone());
            read_stream(stdout, &mut tracker).await;

            // Stream ended — the adb server crashed or restarted. Drop every
            // device we had attributed to this source before reconnecting, so
            // we don't leave phantom entries.
            tracker.drain();
            let _ = child.kill().await;
            tokio::time::sleep(RECONNECT_DELAY).await;
        }
    }

    /// Read the adb track-devices protocol: each message is a 4-char ASCII hex
    /// length followed by that many bytes of `serial\tstatus` lines.
    async fn read_stream<R: tokio::io::AsyncRead + Unpin>(
        mut stdout: R,
        tracker: &mut DeviceTracker,
    ) {
        loop {
            let Some(text) = read_frame(&mut stdout).await else {
                return;
            };

            // Snapshot of the serials currently in `device` state.
            let current: std::collections::HashSet<String> = text
                .lines()
                .filter_map(|line| {
                    let mut parts = line.trim().split('\t');
                    let serial = parts.next()?;
                    let status = parts.next()?;
                    // We only care about fully-online devices. Transient states
                    // like `offline` / `unauthorized` naturally translate into
                    // "not present" and a Removed event if we'd seen it.
                    (status == "device" && !serial.is_empty()).then(|| serial.to_string())
                })
                .collect();

            for serial in &current {
                if !tracker.contains(serial) {
                    let name = device_name(serial).await;
                    tracker.add(Device {
                        id: serial.clone(),
                        name,
                        kind: DeviceKind::Android,
                    });
                }
            }
            for serial in tracker.removed_since(&current) {
                tracker.remove(&serial);
            }
        }
    }

    /// Read one length-prefixed frame. Returns None on any read error or
    /// malformed header, which signals the caller to tear down the connection.
    async fn read_frame<R: tokio::io::AsyncRead + Unpin>(stdout: &mut R) -> Option<String> {
        let mut hex = [0u8; 4];
        stdout.read_exact(&mut hex).await.ok()?;
        let len = usize::from_str_radix(std::str::from_utf8(&hex).ok()?, 16).ok()?;
        let mut buf = vec![0u8; len];
        if len > 0 {
            stdout.read_exact(&mut buf).await.ok()?;
        }
        String::from_utf8(buf).ok()
    }

    /// Build a display name for an Android device or emulator.
    ///
    /// Emulators get a distinct label so the user can tell them apart from
    /// real devices. For real devices we use brand + model, deduping when the
    /// model already starts with the brand name (e.g. "Samsung Galaxy S24").
    async fn device_name(serial: &str) -> String {
        let is_emulator = serial.starts_with("emulator-")
            || getprop(serial, "ro.kernel.qemu").await.as_deref() == Some("1");

        if is_emulator {
            let avd = getprop(serial, "ro.boot.qemu.avd_name").await.or(getprop(
                serial,
                "ro.kernel.qemu.avd_name",
            )
            .await);
            let api = getprop(serial, "ro.build.version.sdk").await;
            return match (avd, api) {
                (Some(a), Some(api)) => format!("{} (API {}, Emulator)", a.replace('_', " "), api),
                (Some(a), None) => format!("{} (Emulator)", a.replace('_', " ")),
                (None, Some(api)) => format!("Android Emulator (API {})", api),
                (None, None) => "Android Emulator".to_string(),
            };
        }

        let brand = getprop(serial, "ro.product.brand").await;
        let model = getprop(serial, "ro.product.model").await;
        match (brand, model) {
            (Some(b), Some(m)) => {
                if m.to_lowercase().starts_with(&b.to_lowercase()) {
                    m
                } else {
                    format!("{} {}", capitalize_first(&b), m)
                }
            }
            (None, Some(m)) => m,
            (Some(b), None) => capitalize_first(&b),
            (None, None) => serial.to_string(),
        }
    }

    async fn getprop(serial: &str, prop: &str) -> Option<String> {
        let out = Command::new("adb")
            .args(["-s", serial, "shell", "getprop", prop])
            .output()
            .await
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let val = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!val.is_empty()).then_some(val)
    }

    fn capitalize_first(s: &str) -> String {
        let mut chars = s.chars();
        match chars.next() {
            None => String::new(),
            Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        }
    }
}

// ── Source 2: usbmuxd Listen (macOS only) ───────────────────────────────────

#[cfg(target_os = "macos")]
mod usbmuxd_source {
    use super::{Device, DeviceEvent, DeviceKind, DeviceTracker};
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;
    use tokio::sync::mpsc;

    const SOCKET_PATH: &str = "/var/run/usbmuxd";
    const HEADER_SIZE: usize = 16;
    const RECONNECT_DELAY: Duration = Duration::from_secs(3);
    const SOCKET_MISSING_DELAY: Duration = Duration::from_secs(10);

    /// Follow usbmuxd's `Listen` protocol forever.
    ///
    /// Every time the connection drops (usbmuxd restart, socket unavailable,
    /// transient read error) we drain the tracker so the UI doesn't keep
    /// showing iOS devices that we can no longer see.
    pub async fn track(tx: mpsc::UnboundedSender<DeviceEvent>) {
        loop {
            let stream = match UnixStream::connect(SOCKET_PATH).await {
                Ok(s) => s,
                Err(_) => {
                    tokio::time::sleep(SOCKET_MISSING_DELAY).await;
                    continue;
                }
            };

            let mut tracker = DeviceTracker::new(tx.clone());
            run_session(stream, &mut tracker).await;
            tracker.drain();
            tokio::time::sleep(RECONNECT_DELAY).await;
        }
    }

    /// One full session: send Listen, then loop on inbound Attached/Detached.
    /// Returns when the connection fails — caller handles cleanup.
    async fn run_session(stream: UnixStream, tracker: &mut DeviceTracker) {
        let (mut read_half, mut write_half) = stream.into_split();

        if send_listen(&mut write_half).await.is_err() {
            return;
        }

        // Multiple Attached events can share a serial (different interfaces:
        // USB, network). We only emit one Added per serial and only emit
        // Removed when the *last* DeviceID for that serial detaches.
        let mut device_id_to_serial: std::collections::HashMap<u32, String> =
            std::collections::HashMap::new();

        loop {
            let Some(msg) = read_message(&mut read_half).await else {
                return;
            };
            dispatch(msg, tracker, &mut device_id_to_serial).await;
        }
    }

    async fn send_listen(w: &mut tokio::net::unix::OwnedWriteHalf) -> std::io::Result<()> {
        let body = {
            let dict = plist::Value::Dictionary({
                let mut d = plist::Dictionary::new();
                d.insert("MessageType".into(), "Listen".into());
                d.insert("ClientVersionString".into(), "flog".into());
                d.insert("ProgName".into(), "flog".into());
                d
            });
            let mut buf = Vec::new();
            dict.to_writer_xml(&mut buf)
                .map_err(|e| std::io::Error::other(e.to_string()))?;
            buf
        };

        let total_len = (HEADER_SIZE + body.len()) as u32;
        let mut header = Vec::with_capacity(HEADER_SIZE);
        header.extend_from_slice(&total_len.to_le_bytes());
        header.extend_from_slice(&1u32.to_le_bytes()); // version
        header.extend_from_slice(&8u32.to_le_bytes()); // message type: plist
        header.extend_from_slice(&1u32.to_le_bytes()); // tag

        w.write_all(&header).await?;
        w.write_all(&body).await?;
        Ok(())
    }

    async fn read_message(r: &mut tokio::net::unix::OwnedReadHalf) -> Option<plist::Dictionary> {
        let mut hdr = [0u8; HEADER_SIZE];
        r.read_exact(&mut hdr).await.ok()?;
        let length = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]) as usize;
        let body_len = length.saturating_sub(HEADER_SIZE);
        let mut body = vec![0u8; body_len];
        if body_len > 0 {
            r.read_exact(&mut body).await.ok()?;
        }
        plist::Value::from_reader(std::io::Cursor::new(body))
            .ok()?
            .into_dictionary()
    }

    async fn dispatch(
        msg: plist::Dictionary,
        tracker: &mut DeviceTracker,
        device_id_to_serial: &mut std::collections::HashMap<u32, String>,
    ) {
        let msg_type = msg.get("MessageType").and_then(|v| v.as_string());
        match msg_type {
            Some("Attached") => handle_attached(msg, tracker, device_id_to_serial).await,
            Some("Detached") => handle_detached(msg, tracker, device_id_to_serial),
            _ => {}
        }
    }

    async fn handle_attached(
        msg: plist::Dictionary,
        tracker: &mut DeviceTracker,
        device_id_to_serial: &mut std::collections::HashMap<u32, String>,
    ) {
        let Some(props) = msg.get("Properties").and_then(|v| v.as_dictionary()) else {
            return;
        };
        let device_id = props
            .get("DeviceID")
            .and_then(|v| v.as_unsigned_integer())
            .unwrap_or(0) as u32;
        let serial = props
            .get("SerialNumber")
            .and_then(|v| v.as_string())
            .unwrap_or("")
            .to_string();
        if serial.is_empty() {
            return;
        }

        device_id_to_serial.insert(device_id, serial.clone());

        if tracker.contains(&serial) {
            // Same phone, new interface (USB vs wifi pairing) — just remember
            // the mapping; don't emit a duplicate Added.
            return;
        }

        let name = props
            .get("DeviceName")
            .and_then(|v| v.as_string())
            .map(str::to_string)
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "iPhone".to_string());
        // DeviceName is usually populated by usbmuxd; if not, lockdownd query
        // as a fallback.
        let name = if name == "iPhone" {
            crate::transport::usbmuxd::query_device_name(device_id)
                .await
                .unwrap_or(name)
        } else {
            name
        };

        tracker.add(Device {
            id: serial,
            name,
            kind: DeviceKind::IosUsb { device_id },
        });
    }

    fn handle_detached(
        msg: plist::Dictionary,
        tracker: &mut DeviceTracker,
        device_id_to_serial: &mut std::collections::HashMap<u32, String>,
    ) {
        let device_id = msg
            .get("DeviceID")
            .and_then(|v| v.as_unsigned_integer())
            .unwrap_or(0) as u32;
        let Some(serial) = device_id_to_serial.remove(&device_id) else {
            return;
        };
        // Only remove the device when *every* interface for this serial has
        // detached — otherwise the phone still has a live tunnel.
        let still_attached = device_id_to_serial.values().any(|s| s == &serial);
        if !still_attached {
            tracker.remove(&serial);
        }
    }
}

// ── Source 3: localhost probe ───────────────────────────────────────────────

mod local_source {
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

    async fn tcp_open(port: u16) -> Option<u16> {
        let addr = format!("127.0.0.1:{}", port);
        match tokio::time::timeout(TCP_TIMEOUT, tokio::net::TcpStream::connect(&addr)).await {
            Ok(Ok(_)) => Some(port),
            _ => None,
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
        let (ws, _) =
            tokio::time::timeout(HANDSHAKE_TIMEOUT, tokio_tungstenite::connect_async(&url))
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
        let json: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
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
}
