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

#[cfg(test)]
mod tracker_tests {
    use super::*;

    fn dev(id: &str) -> Device {
        Device {
            id: id.to_string(),
            name: format!("name-{}", id),
            kind: DeviceKind::Local,
        }
    }

    #[test]
    fn add_emits_once_per_id() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut t = DeviceTracker::new(tx);
        assert!(t.add(dev("A")));
        assert!(!t.add(dev("A"))); // duplicate → no event
        let mut count = 0;
        while rx.try_recv().is_ok() {
            count += 1;
        }
        assert_eq!(count, 1);
    }

    #[test]
    fn contains_reflects_add_remove() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut t = DeviceTracker::new(tx);
        assert!(!t.contains("A"));
        t.add(dev("A"));
        assert!(t.contains("A"));
        t.remove("A");
        assert!(!t.contains("A"));
    }

    #[test]
    fn remove_unknown_id_is_silent() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut t = DeviceTracker::new(tx);
        t.remove("never-added");
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn remove_after_add_emits_removed() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut t = DeviceTracker::new(tx);
        t.add(dev("A"));
        let _ = rx.try_recv(); // Added
        t.remove("A");
        match rx.try_recv().expect("Removed") {
            DeviceEvent::Removed(id) => assert_eq!(id, "A"),
            _ => panic!("expected Removed"),
        }
    }

    #[test]
    fn removed_since_computes_delta_correctly() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut t = DeviceTracker::new(tx);
        t.add(dev("A"));
        t.add(dev("B"));
        t.add(dev("C"));

        // "Current" snapshot says only B remains.
        let current: std::collections::HashSet<String> = std::iter::once("B".to_string()).collect();
        let mut diff = t.removed_since(&current);
        diff.sort();
        assert_eq!(diff, vec!["A".to_string(), "C".to_string()]);
    }

    #[test]
    fn removed_since_empty_when_snapshot_equals_known() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut t = DeviceTracker::new(tx);
        t.add(dev("A"));
        let current: std::collections::HashSet<String> = std::iter::once("A".to_string()).collect();
        assert!(t.removed_since(&current).is_empty());
    }

    #[test]
    fn drain_emits_removed_for_every_known_and_clears() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut t = DeviceTracker::new(tx);
        t.add(dev("A"));
        t.add(dev("B"));
        // Drop the two Added events.
        let _ = rx.try_recv();
        let _ = rx.try_recv();

        t.drain();
        let mut removed: Vec<String> = Vec::new();
        while let Ok(evt) = rx.try_recv() {
            if let DeviceEvent::Removed(id) = evt {
                removed.push(id);
            }
        }
        removed.sort();
        assert_eq!(removed, vec!["A".to_string(), "B".to_string()]);
        // Second drain is a no-op.
        t.drain();
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn connection_method_maps_device_kind() {
        let local = Device {
            id: "localhost".into(),
            name: "x".into(),
            kind: DeviceKind::Local,
        };
        assert!(matches!(
            local.connection_method(),
            ConnectionMethod::Localhost
        ));

        let android = Device {
            id: "SN123".into(),
            name: "x".into(),
            kind: DeviceKind::Android,
        };
        match android.connection_method() {
            ConnectionMethod::AdbForward { serial } => assert_eq!(serial, "SN123"),
            _ => panic!("expected AdbForward"),
        }

        let ios = Device {
            id: "SN".into(),
            name: "x".into(),
            kind: DeviceKind::IosUsb { device_id: 42 },
        };
        match ios.connection_method() {
            ConnectionMethod::Usbmuxd { device_id } => assert_eq!(device_id, 42),
            _ => panic!("expected Usbmuxd"),
        }
    }

    #[test]
    fn device_event_debug_is_stable() {
        // Debug impls must not panic; guard against accidental derive removal.
        let ev_added = DeviceEvent::Added(dev("A"));
        let ev_removed = DeviceEvent::Removed("A".into());
        assert!(format!("{:?}", ev_added).contains("Added"));
        assert!(format!("{:?}", ev_removed).contains("Removed"));
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

    // ── Reconnect / backoff timing (TRANS-008) ──────────────────────────
    //
    // The values below encode a simple cadence:
    //   1. If the `adb` binary is missing, poll every 30s — the user is
    //      likely installing or has PATH misconfigured, and a tighter loop
    //      would just burn CPU while they fix it.
    //   2. If `adb track-devices` starts cleanly but dies (adb server
    //      restart, system sleep, etc.), wait 3s before retrying — long
    //      enough not to hammer a crashed adb daemon, short enough that
    //      the user sees devices come back quickly.

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
            return emulator_name(avd, api);
        }

        let brand = getprop(serial, "ro.product.brand").await;
        let model = getprop(serial, "ro.product.model").await;
        real_device_name(serial, brand, model)
    }

    /// Pure helper: build an emulator display name from optional AVD name and
    /// API level, replacing underscores with spaces in the AVD name.
    pub(super) fn emulator_name(avd: Option<String>, api: Option<String>) -> String {
        match (avd, api) {
            (Some(a), Some(api)) => format!("{} (API {}, Emulator)", a.replace('_', " "), api),
            (Some(a), None) => format!("{} (Emulator)", a.replace('_', " ")),
            (None, Some(api)) => format!("Android Emulator (API {})", api),
            (None, None) => "Android Emulator".to_string(),
        }
    }

    /// Pure helper: build a real-device display name from optional brand/model
    /// and serial fallback. Dedups when `model` already starts with `brand`
    /// (e.g. "Samsung Galaxy S24").
    pub(super) fn real_device_name(
        serial: &str,
        brand: Option<String>,
        model: Option<String>,
    ) -> String {
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

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::io::Cursor;

        // ── capitalize_first ────────────────────────────────────────────
        #[test]
        fn capitalize_first_basic_ascii() {
            assert_eq!(capitalize_first("samsung"), "Samsung");
            assert_eq!(capitalize_first("xiaomi"), "Xiaomi");
        }

        #[test]
        fn capitalize_first_empty_is_empty() {
            assert_eq!(capitalize_first(""), "");
        }

        #[test]
        fn capitalize_first_already_upper_is_noop() {
            assert_eq!(capitalize_first("OnePlus"), "OnePlus");
        }

        #[test]
        fn capitalize_first_multibyte_char() {
            // Non-ASCII first char: µ -> M; ensure we don't panic on byte
            // indexing, and the remainder is preserved.
            let out = capitalize_first("über");
            // Uppercase Ü is two bytes; the tail ("ber") must survive.
            assert!(out.ends_with("ber"));
            assert!(out.starts_with('Ü'));
        }

        // ── read_frame (adb track-devices framing) ──────────────────────

        /// Build a single length-prefixed adb track-devices frame.
        fn make_frame(payload: &str) -> Vec<u8> {
            let mut out = format!("{:04x}", payload.len()).into_bytes();
            out.extend_from_slice(payload.as_bytes());
            out
        }

        #[tokio::test]
        async fn read_frame_parses_single_frame() {
            let bytes = make_frame("SERIAL1\tdevice\n");
            let mut cur = Cursor::new(bytes);
            let frame = read_frame(&mut cur).await;
            assert_eq!(frame.as_deref(), Some("SERIAL1\tdevice\n"));
        }

        #[tokio::test]
        async fn read_frame_empty_frame_ok() {
            // "0000" + zero-byte body is a valid "no devices" announcement.
            let bytes = b"0000".to_vec();
            let mut cur = Cursor::new(bytes);
            let frame = read_frame(&mut cur).await;
            assert_eq!(frame.as_deref(), Some(""));
        }

        #[tokio::test]
        async fn read_frame_truncated_header_returns_none() {
            let bytes = b"00".to_vec(); // only 2 bytes instead of 4
            let mut cur = Cursor::new(bytes);
            assert!(read_frame(&mut cur).await.is_none());
        }

        #[tokio::test]
        async fn read_frame_non_hex_header_returns_none() {
            // "XXXX" is not a valid hex length.
            let bytes = b"XXXX".to_vec();
            let mut cur = Cursor::new(bytes);
            assert!(read_frame(&mut cur).await.is_none());
        }

        #[tokio::test]
        async fn read_frame_truncated_body_returns_none() {
            // Header says 10 bytes, only 5 supplied.
            let mut bytes = b"000a".to_vec();
            bytes.extend_from_slice(b"short");
            let mut cur = Cursor::new(bytes);
            assert!(read_frame(&mut cur).await.is_none());
        }

        // ── read_stream (Added/Removed emission from adb track-devices) ─

        fn make_stream(frames: &[&str]) -> Vec<u8> {
            let mut out = Vec::new();
            for f in frames {
                out.extend_from_slice(&make_frame(f));
            }
            out
        }

        #[tokio::test]
        async fn read_stream_emits_added_for_online_device() {
            // NOTE: `read_stream` calls `device_name(serial)` which shells out
            // to `adb shell getprop`. Since adb is unlikely to know about
            // fake serial "ABC123", getprop fails and name falls back to
            // `serial.to_string()`. The Added event is still produced, which
            // is the logic we want to verify here.
            // UNTESTABLE: PHYS sub-call — device_name() at line 242 shells
            // out; we rely on its documented failure-fallback path.
            let bytes = make_stream(&["TESTDEV\tdevice\n"]);
            let mut cur = Cursor::new(bytes);

            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let mut tracker = DeviceTracker::new(tx);
            read_stream(&mut cur, &mut tracker).await;

            let evt = rx.try_recv().expect("Added event");
            match evt {
                DeviceEvent::Added(dev) => {
                    assert_eq!(dev.id, "TESTDEV");
                    assert!(matches!(dev.kind, DeviceKind::Android));
                }
                _ => panic!("expected Added"),
            }
        }

        #[tokio::test]
        async fn read_stream_ignores_offline_and_unauthorized() {
            let bytes = make_stream(&["DEV1\toffline\nDEV2\tunauthorized\n"]);
            let mut cur = Cursor::new(bytes);

            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let mut tracker = DeviceTracker::new(tx);
            read_stream(&mut cur, &mut tracker).await;

            // No Added events should have been emitted.
            assert!(rx.try_recv().is_err());
        }

        #[tokio::test]
        async fn read_stream_removed_when_device_disappears() {
            // First frame: device online. Second frame: empty → Removed.
            let bytes = make_stream(&["GONEDEV\tdevice\n", ""]);
            let mut cur = Cursor::new(bytes);

            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let mut tracker = DeviceTracker::new(tx);
            read_stream(&mut cur, &mut tracker).await;

            // Drain events: Added then Removed.
            let mut saw_added = false;
            let mut saw_removed = false;
            while let Ok(evt) = rx.try_recv() {
                match evt {
                    DeviceEvent::Added(d) if d.id == "GONEDEV" => saw_added = true,
                    DeviceEvent::Removed(id) if id == "GONEDEV" => saw_removed = true,
                    _ => {}
                }
            }
            assert!(saw_added, "should have seen Added");
            assert!(saw_removed, "should have seen Removed");
        }

        #[tokio::test]
        async fn read_stream_malformed_line_is_skipped() {
            // No tab → filter_map returns None → line ignored.
            let bytes = make_stream(&["malformed no tab here\n"]);
            let mut cur = Cursor::new(bytes);

            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let mut tracker = DeviceTracker::new(tx);
            read_stream(&mut cur, &mut tracker).await;

            assert!(rx.try_recv().is_err());
        }

        // ── emulator_name ───────────────────────────────────────────────
        #[test]
        fn emulator_name_both_present_underscores_replaced() {
            assert_eq!(
                emulator_name(Some("Pixel_7_API_34".into()), Some("34".into())),
                "Pixel 7 API 34 (API 34, Emulator)"
            );
        }

        #[test]
        fn emulator_name_avd_only() {
            assert_eq!(
                emulator_name(Some("Test_AVD".into()), None),
                "Test AVD (Emulator)"
            );
        }

        #[test]
        fn emulator_name_api_only() {
            assert_eq!(
                emulator_name(None, Some("33".into())),
                "Android Emulator (API 33)"
            );
        }

        #[test]
        fn emulator_name_neither() {
            assert_eq!(emulator_name(None, None), "Android Emulator");
        }

        // ── real_device_name ───────────────────────────────────────────
        #[test]
        fn real_device_name_brand_model_deduped() {
            // Model starts with brand (case-insensitive) → use model only.
            assert_eq!(
                real_device_name(
                    "S1",
                    Some("samsung".into()),
                    Some("Samsung Galaxy S24".into())
                ),
                "Samsung Galaxy S24"
            );
        }

        #[test]
        fn real_device_name_brand_model_concatenated() {
            // Brand doesn't appear in model → capitalize brand + model.
            assert_eq!(
                real_device_name("S1", Some("oneplus".into()), Some("Nord 3".into())),
                "Oneplus Nord 3"
            );
        }

        #[test]
        fn real_device_name_model_only() {
            assert_eq!(real_device_name("S1", None, Some("Mi 11".into())), "Mi 11");
        }

        #[test]
        fn real_device_name_brand_only_capitalized() {
            assert_eq!(real_device_name("S1", Some("pixel".into()), None), "Pixel");
        }

        #[test]
        fn real_device_name_nothing_uses_serial() {
            assert_eq!(real_device_name("SN-123", None, None), "SN-123");
        }

        // UNTESTABLE: PHYS — Command::new("adb") in track() at line 151 and
        // getprop() at line 277. Requires the `adb` binary on PATH.
    }
}

// ── Source 2: usbmuxd Listen (macOS only) ───────────────────────────────────

#[cfg(target_os = "macos")]
mod usbmuxd_source {
    use super::{Device, DeviceEvent, DeviceKind, DeviceTracker};
    use std::time::Duration;
    use tokio::io::AsyncWriteExt;
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
        let (header, body) =
            encode_listen_frame().map_err(|e| std::io::Error::other(e.to_string()))?;
        w.write_all(&header).await?;
        w.write_all(&body).await?;
        Ok(())
    }

    /// Build the (header, body) for the Listen request. Pure — no I/O.
    pub(super) fn encode_listen_frame(
    ) -> Result<(Vec<u8>, Vec<u8>), Box<dyn std::error::Error + Send + Sync>> {
        let dict = plist::Value::Dictionary({
            let mut d = plist::Dictionary::new();
            d.insert("MessageType".into(), "Listen".into());
            d.insert("ClientVersionString".into(), "flog".into());
            d.insert("ProgName".into(), "flog".into());
            d
        });
        let mut body = Vec::new();
        dict.to_writer_xml(&mut body)?;

        let total_len = (HEADER_SIZE + body.len()) as u32;
        let mut header = Vec::with_capacity(HEADER_SIZE);
        header.extend_from_slice(&total_len.to_le_bytes());
        header.extend_from_slice(&1u32.to_le_bytes()); // version
        header.extend_from_slice(&8u32.to_le_bytes()); // message type: plist
        header.extend_from_slice(&1u32.to_le_bytes()); // tag
        Ok((header, body))
    }

    async fn read_message(r: &mut tokio::net::unix::OwnedReadHalf) -> Option<plist::Dictionary> {
        read_message_any(r).await
    }

    /// Generic version of `read_message` that works over any `AsyncRead`.
    /// The macOS UnixStream half only takes &mut, so we keep that shape.
    async fn read_message_any<R: tokio::io::AsyncRead + Unpin>(
        r: &mut R,
    ) -> Option<plist::Dictionary> {
        use tokio::io::AsyncReadExt;
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

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::collections::HashMap;

        fn attached_msg(device_id: u64, serial: &str, name: Option<&str>) -> plist::Dictionary {
            let mut props = plist::Dictionary::new();
            props.insert("DeviceID".into(), plist::Value::Integer(device_id.into()));
            props.insert("SerialNumber".into(), serial.into());
            if let Some(n) = name {
                props.insert("DeviceName".into(), n.into());
            }

            let mut outer = plist::Dictionary::new();
            outer.insert("MessageType".into(), "Attached".into());
            outer.insert("Properties".into(), plist::Value::Dictionary(props));
            outer
        }

        fn detached_msg(device_id: u64) -> plist::Dictionary {
            let mut outer = plist::Dictionary::new();
            outer.insert("MessageType".into(), "Detached".into());
            outer.insert("DeviceID".into(), plist::Value::Integer(device_id.into()));
            outer
        }

        #[tokio::test]
        async fn handle_attached_emits_added_once() {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let mut tracker = DeviceTracker::new(tx);
            let mut map: HashMap<u32, String> = HashMap::new();

            handle_attached(
                attached_msg(5, "APPLE-SN-1", Some("My iPhone")),
                &mut tracker,
                &mut map,
            )
            .await;

            let evt = rx.try_recv().expect("Added");
            match evt {
                DeviceEvent::Added(d) => {
                    assert_eq!(d.id, "APPLE-SN-1");
                    assert_eq!(d.name, "My iPhone");
                    assert!(matches!(d.kind, DeviceKind::IosUsb { device_id: 5 }));
                }
                _ => panic!("expected Added"),
            }
            assert_eq!(map.get(&5).map(String::as_str), Some("APPLE-SN-1"));
        }

        #[tokio::test]
        async fn handle_attached_second_interface_does_not_duplicate() {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let mut tracker = DeviceTracker::new(tx);
            let mut map: HashMap<u32, String> = HashMap::new();

            handle_attached(
                attached_msg(5, "APPLE-SN-1", Some("iPhone")),
                &mut tracker,
                &mut map,
            )
            .await;
            // Different DeviceID, same serial (USB + network pairing).
            handle_attached(
                attached_msg(7, "APPLE-SN-1", Some("iPhone")),
                &mut tracker,
                &mut map,
            )
            .await;

            // First Added drained, second attach did not emit Added.
            let mut count = 0;
            while rx.try_recv().is_ok() {
                count += 1;
            }
            assert_eq!(count, 1, "only one Added for same serial");
            assert_eq!(map.len(), 2);
        }

        #[tokio::test]
        async fn handle_attached_empty_serial_is_ignored() {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let mut tracker = DeviceTracker::new(tx);
            let mut map: HashMap<u32, String> = HashMap::new();

            handle_attached(attached_msg(1, "", None), &mut tracker, &mut map).await;
            assert!(rx.try_recv().is_err());
            assert!(map.is_empty());
        }

        #[tokio::test]
        async fn handle_attached_missing_properties_is_skipped() {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let mut tracker = DeviceTracker::new(tx);
            let mut map: HashMap<u32, String> = HashMap::new();

            let mut outer = plist::Dictionary::new();
            outer.insert("MessageType".into(), "Attached".into());
            // No "Properties" key.
            handle_attached(outer, &mut tracker, &mut map).await;
            assert!(rx.try_recv().is_err());
        }

        #[test]
        fn handle_detached_last_interface_removes_device() {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let mut tracker = DeviceTracker::new(tx);
            tracker.add(Device {
                id: "SN".into(),
                name: "iPhone".into(),
                kind: DeviceKind::IosUsb { device_id: 5 },
            });
            // Drain the Added event.
            let _ = rx.try_recv();

            let mut map: HashMap<u32, String> = HashMap::new();
            map.insert(5, "SN".into());
            handle_detached(detached_msg(5), &mut tracker, &mut map);

            match rx.try_recv().expect("Removed") {
                DeviceEvent::Removed(id) => assert_eq!(id, "SN"),
                _ => panic!("expected Removed"),
            }
            assert!(map.is_empty());
        }

        #[test]
        fn handle_detached_other_interface_preserves_device() {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let mut tracker = DeviceTracker::new(tx);
            tracker.add(Device {
                id: "SN".into(),
                name: "iPhone".into(),
                kind: DeviceKind::IosUsb { device_id: 5 },
            });
            let _ = rx.try_recv();

            let mut map: HashMap<u32, String> = HashMap::new();
            map.insert(5, "SN".into());
            map.insert(7, "SN".into()); // Second interface still attached.
            handle_detached(detached_msg(5), &mut tracker, &mut map);

            // No Removed emitted — the other interface still holds it open.
            assert!(rx.try_recv().is_err());
            assert!(map.contains_key(&7));
            assert!(!map.contains_key(&5));
        }

        #[test]
        fn handle_detached_unknown_device_id_is_noop() {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let mut tracker = DeviceTracker::new(tx);
            let mut map: HashMap<u32, String> = HashMap::new();
            handle_detached(detached_msg(999), &mut tracker, &mut map);
            assert!(rx.try_recv().is_err());
        }

        #[tokio::test]
        async fn dispatch_routes_message_types() {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let mut tracker = DeviceTracker::new(tx);
            let mut map: HashMap<u32, String> = HashMap::new();

            dispatch(attached_msg(1, "SN-A", Some("A")), &mut tracker, &mut map).await;
            dispatch(detached_msg(1), &mut tracker, &mut map).await;
            // Unknown type — should do nothing.
            let mut unknown = plist::Dictionary::new();
            unknown.insert("MessageType".into(), "Weirdo".into());
            dispatch(unknown, &mut tracker, &mut map).await;

            // Expect Added then Removed, then nothing more.
            let mut events = Vec::new();
            while let Ok(e) = rx.try_recv() {
                events.push(format!("{:?}", e));
            }
            assert_eq!(events.len(), 2, "exactly Added + Removed: {:?}", events);
        }

        // ── encode_listen_frame ─────────────────────────────────────────
        #[test]
        fn encode_listen_frame_shape_matches_usbmuxd_wire_format() {
            let (header, body) = encode_listen_frame().expect("encode");
            assert_eq!(header.len(), HEADER_SIZE);

            let length = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
            assert_eq!(length as usize, HEADER_SIZE + body.len());
            assert_eq!(
                u32::from_le_bytes([header[4], header[5], header[6], header[7]]),
                1
            );
            assert_eq!(
                u32::from_le_bytes([header[8], header[9], header[10], header[11]]),
                8
            );
            assert_eq!(
                u32::from_le_bytes([header[12], header[13], header[14], header[15]]),
                1
            );

            // Body should parse back as a dict with MessageType=Listen.
            let parsed = plist::Value::from_reader(std::io::Cursor::new(body)).unwrap();
            let d = parsed.as_dictionary().unwrap();
            assert_eq!(d.get("MessageType").unwrap().as_string(), Some("Listen"));
            assert_eq!(d.get("ProgName").unwrap().as_string(), Some("flog"));
        }

        // ── read_message_any (AsyncRead generic) ───────────────────────

        /// Build a raw usbmuxd wire frame for a known plist dict.
        fn build_frame(dict: plist::Dictionary) -> Vec<u8> {
            let val = plist::Value::Dictionary(dict);
            let mut body = Vec::new();
            val.to_writer_xml(&mut body).unwrap();
            let total_len = (HEADER_SIZE + body.len()) as u32;
            let mut out = Vec::with_capacity(HEADER_SIZE + body.len());
            out.extend_from_slice(&total_len.to_le_bytes());
            out.extend_from_slice(&1u32.to_le_bytes());
            out.extend_from_slice(&8u32.to_le_bytes());
            out.extend_from_slice(&1u32.to_le_bytes());
            out.extend_from_slice(&body);
            out
        }

        #[tokio::test]
        async fn read_message_any_parses_valid_frame() {
            let mut d = plist::Dictionary::new();
            d.insert("MessageType".into(), "Attached".into());
            let bytes = build_frame(d);
            let mut cur = std::io::Cursor::new(bytes);
            let parsed = read_message_any(&mut cur).await.expect("dict");
            assert_eq!(
                parsed.get("MessageType").unwrap().as_string(),
                Some("Attached")
            );
        }

        #[tokio::test]
        async fn read_message_any_returns_none_on_truncated_header() {
            let mut cur = std::io::Cursor::new(vec![0u8, 0u8]);
            assert!(read_message_any(&mut cur).await.is_none());
        }

        #[tokio::test]
        async fn read_message_any_returns_none_on_truncated_body() {
            // Header says 32 bytes total, body is only 5 bytes.
            let mut out = Vec::new();
            out.extend_from_slice(&32u32.to_le_bytes());
            out.extend_from_slice(&1u32.to_le_bytes());
            out.extend_from_slice(&8u32.to_le_bytes());
            out.extend_from_slice(&1u32.to_le_bytes());
            out.extend_from_slice(b"short");
            let mut cur = std::io::Cursor::new(out);
            assert!(read_message_any(&mut cur).await.is_none());
        }

        #[tokio::test]
        async fn read_message_any_returns_none_on_non_dict_body() {
            // Craft a plist whose top-level is an array, not a dict.
            let val = plist::Value::Array(vec!["x".into(), "y".into()]);
            let mut body = Vec::new();
            val.to_writer_xml(&mut body).unwrap();
            let total_len = (HEADER_SIZE + body.len()) as u32;
            let mut out = Vec::with_capacity(HEADER_SIZE + body.len());
            out.extend_from_slice(&total_len.to_le_bytes());
            out.extend_from_slice(&1u32.to_le_bytes());
            out.extend_from_slice(&8u32.to_le_bytes());
            out.extend_from_slice(&1u32.to_le_bytes());
            out.extend_from_slice(&body);

            let mut cur = std::io::Cursor::new(out);
            assert!(read_message_any(&mut cur).await.is_none());
        }

        // UNTESTABLE: PHYS — UnixStream::connect(SOCKET_PATH) in track() at
        // line 321. Requires a live /var/run/usbmuxd socket + paired iOS.
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
    mod tests {
        use super::*;

        // ── tcp_open ────────────────────────────────────────────────────

        #[tokio::test]
        async fn tcp_open_detects_listener() {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let result = tcp_open(port).await;
            assert_eq!(result, Some(port));
        }

        #[tokio::test]
        async fn tcp_open_returns_none_when_no_listener() {
            // Port 1 is privileged and nobody normally listens on it; even
            // if it fails to bind due to perms on some systems, the connect
            // will still fail → None. Use it as "unreachable" sentinel.
            let result = tcp_open(1).await;
            assert_eq!(result, None);
        }

        // ── is_port_open (TRANS-007) ────────────────────────────────────

        #[tokio::test]
        async fn is_port_open_true_when_listener_bound() {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            assert!(is_port_open(port).await);
        }

        #[tokio::test]
        async fn is_port_open_false_when_nothing_listens() {
            // Port 1 is privileged and nobody normally listens on it.
            assert!(!is_port_open(1).await);
        }

        // ── device_name (Hello-driven dispatch) ─────────────────────────

        #[tokio::test]
        async fn device_name_falls_back_when_app_and_os_empty() {
            let hello = Hello {
                os: String::new(),
                app: String::new(),
            };
            // No os, no app → "Simulator" string.
            assert_eq!(device_name(&hello).await, "Simulator");
        }

        #[tokio::test]
        async fn device_name_uses_app_for_non_ios_os() {
            let hello = Hello {
                os: "android".into(),
                app: "MyCoolApp".into(),
            };
            assert_eq!(device_name(&hello).await, "MyCoolApp");
        }

        // UNTESTABLE: PHYS — device_name() for os=="ios" shells out via
        // booted_simulator_name() → xcrun at line 634. Its JSON-parsing
        // pure helper is tested below.

        // ── parse_simctl_booted ─────────────────────────────────────────

        #[test]
        fn parse_simctl_booted_finds_first_booted() {
            let json = br#"{
              "devices": {
                "com.apple.CoreSimulator.SimRuntime.iOS-17-4": [
                  { "state": "Shutdown", "name": "iPhone 14" },
                  { "state": "Booted",  "name": "iPhone 15 Pro" }
                ]
              }
            }"#;
            assert_eq!(parse_simctl_booted(json).as_deref(), Some("iPhone 15 Pro"));
        }

        #[test]
        fn parse_simctl_booted_returns_none_when_no_booted() {
            let json = br#"{ "devices": { "rt": [ { "state": "Shutdown", "name": "X" } ] } }"#;
            assert!(parse_simctl_booted(json).is_none());
        }

        #[test]
        fn parse_simctl_booted_returns_none_on_empty_devices() {
            let json = br#"{ "devices": {} }"#;
            assert!(parse_simctl_booted(json).is_none());
        }

        #[test]
        fn parse_simctl_booted_returns_none_on_malformed_json() {
            assert!(parse_simctl_booted(b"not json").is_none());
        }

        #[test]
        fn parse_simctl_booted_skips_non_array_runtime() {
            // A runtime entry that isn't an array should be skipped, not panic.
            let json = br#"{ "devices": { "weird": "not an array",
                                          "rt": [ { "state": "Booted", "name": "OK" } ] } }"#;
            assert_eq!(parse_simctl_booted(json).as_deref(), Some("OK"));
        }

        #[test]
        fn parse_simctl_booted_missing_name_returns_none() {
            // Booted but no "name" key → should fall through to None for
            // this device; overall function returns None (no other Booted).
            let json = br#"{ "devices": { "rt": [ { "state": "Booted" } ] } }"#;
            assert!(parse_simctl_booted(json).is_none());
        }

        // ── handshake (integration with FakeServer) ─────────────────────
        //
        // handshake() is private; we exercise it indirectly via a small
        // TcpListener that scripts the required first-frame behaviors.

        #[tokio::test]
        async fn handshake_returns_some_on_valid_hello() {
            use futures_util::SinkExt;
            use tokio_tungstenite::accept_async;
            use tokio_tungstenite::tungstenite::Message;

            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();

            tokio::spawn(async move {
                if let Ok((stream, _)) = listener.accept().await {
                    let mut ws = accept_async(stream).await.unwrap();
                    let hello = serde_json::json!({
                        "type": "hello",
                        "os": "ios",
                        "app": "DemoApp",
                    });
                    let _ = ws.send(Message::Text(hello.to_string().into())).await;
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            });

            let result = handshake(port).await;
            let hello = result.expect("valid hello");
            assert_eq!(hello.os, "ios");
            assert_eq!(hello.app, "DemoApp");
        }

        #[tokio::test]
        async fn handshake_none_when_first_frame_is_binary() {
            use futures_util::SinkExt;
            use tokio_tungstenite::accept_async;
            use tokio_tungstenite::tungstenite::Message;

            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();

            tokio::spawn(async move {
                if let Ok((stream, _)) = listener.accept().await {
                    if let Ok(mut ws) = accept_async(stream).await {
                        let _ = ws.send(Message::Binary(vec![0u8; 8].into())).await;
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
            });

            assert!(handshake(port).await.is_none());
        }

        #[tokio::test]
        async fn handshake_none_when_type_field_wrong() {
            use futures_util::SinkExt;
            use tokio_tungstenite::accept_async;
            use tokio_tungstenite::tungstenite::Message;

            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();

            tokio::spawn(async move {
                if let Ok((stream, _)) = listener.accept().await {
                    if let Ok(mut ws) = accept_async(stream).await {
                        let wrong =
                            serde_json::json!({ "type": "greetings", "os": "x", "app": "y" });
                        let _ = ws.send(Message::Text(wrong.to_string().into())).await;
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
            });

            assert!(handshake(port).await.is_none());
        }

        #[tokio::test]
        async fn handshake_none_on_malformed_json() {
            use futures_util::SinkExt;
            use tokio_tungstenite::accept_async;
            use tokio_tungstenite::tungstenite::Message;

            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();

            tokio::spawn(async move {
                if let Ok((stream, _)) = listener.accept().await {
                    if let Ok(mut ws) = accept_async(stream).await {
                        let _ = ws.send(Message::Text("not a json".into())).await;
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
            });

            assert!(handshake(port).await.is_none());
        }

        // UNTESTABLE: PHYS shell-out to `xcrun` — booted_simulator_name()
        // at line 632. Its pure JSON parser is covered above.
    }
}
