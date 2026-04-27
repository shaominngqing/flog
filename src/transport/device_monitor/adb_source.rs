//! adb track-devices source: streams Added/Removed events for
//! Android real devices + AVDs directly from the adb daemon.

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
async fn read_stream<R: tokio::io::AsyncRead + Unpin>(mut stdout: R, tracker: &mut DeviceTracker) {
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
mod tests;
