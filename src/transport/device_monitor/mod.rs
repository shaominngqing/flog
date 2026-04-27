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
//! Each source lives in its own submodule and shares a small helper
//! (`DeviceTracker`) that encapsulates the "known set + emit Added /
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
mod tracker_tests;

// ── Source 1: adb track-devices ─────────────────────────────────────────────

mod adb_source;

// ── Source 2: usbmuxd Listen (macOS only) ───────────────────────────────────

#[cfg(target_os = "macos")]
mod usbmuxd_source;

// ── Source 3: localhost probe ───────────────────────────────────────────────

mod local_source;
