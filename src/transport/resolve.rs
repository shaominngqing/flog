//! Unified transport-address resolution.
//!
//! Audit ref: TRANS-009 — the three transport paths (Localhost, ADB,
//! usbmuxd) previously lived inline in `main.rs` with ad-hoc error
//! handling. This module turns the "which transport?" decision into a
//! pure function that returns a structured `TransportAddr` value. The
//! caller (main.rs) is still responsible for the shell-out side effects
//! (`adb forward`, `usbmuxd Connect`) — those remain UNTESTABLE: PHYS —
//! but the branching logic is now covered by unit tests and shaped
//! symmetrically across all three platforms.

use crate::transport::device_monitor::{ConnectionMethod, Device};

/// What `resolve_transport_addr` returns — a fully-described transport
/// plan the caller can dispatch on. Each variant carries exactly the
/// inputs the matching side-effectful transport call needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportAddr {
    /// Connect directly to loopback at the target port (macOS app / iOS
    /// simulator).
    Localhost { port: u16 },
    /// Set up `adb -s <serial> forward tcp:<local> tcp:<port>`, then
    /// connect to `ws://localhost:<local>`. The caller allocates the
    /// local port at dispatch time.
    AdbForward { serial: String, port: u16 },
    /// Open a usbmuxd tunnel to `device_id`/`port` and run the WebSocket
    /// handshake over the resulting UnixStream.
    Usbmuxd { device_id: u32, port: u16 },
}

/// Resolve the transport plan for `device` targeting `port`.
///
/// This is pure — no I/O, no syscalls, no allocations beyond the
/// returned `String` serial. Returning `Result` leaves room for future
/// error cases (e.g. `DeviceKind` variants that don't support a given
/// port) without changing callers' signatures.
pub fn resolve_transport_addr(device: &Device, port: u16) -> Result<TransportAddr, ResolveError> {
    match device.connection_method() {
        ConnectionMethod::Localhost => Ok(TransportAddr::Localhost { port }),
        ConnectionMethod::AdbForward { serial } => Ok(TransportAddr::AdbForward { serial, port }),
        ConnectionMethod::Usbmuxd { device_id } => Ok(TransportAddr::Usbmuxd { device_id, port }),
    }
}

/// Error type reserved for future transport-resolution failures.
///
/// Today every device kind resolves successfully, but the `Result` shape
/// means we can reject e.g. "this port is outside the allowed range"
/// without rewriting call sites later.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResolveError {
    // Intentionally empty — see doc above.
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {}
    }
}

impl std::error::Error for ResolveError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::device_monitor::DeviceKind;

    fn local_device() -> Device {
        Device {
            id: "localhost".into(),
            name: "macOS".into(),
            kind: DeviceKind::Local,
        }
    }

    fn android_device(serial: &str) -> Device {
        Device {
            id: serial.into(),
            name: "Pixel".into(),
            kind: DeviceKind::Android,
        }
    }

    fn ios_device(device_id: u32) -> Device {
        Device {
            id: "APPLE-SN".into(),
            name: "iPhone".into(),
            kind: DeviceKind::IosUsb { device_id },
        }
    }

    #[test]
    fn resolve_localhost_produces_localhost_variant() {
        let addr = resolve_transport_addr(&local_device(), 9753).expect("resolve");
        assert_eq!(addr, TransportAddr::Localhost { port: 9753 });
    }

    #[test]
    fn resolve_android_produces_adb_forward_with_serial() {
        let addr = resolve_transport_addr(&android_device("SN-42"), 9755).expect("resolve");
        assert_eq!(
            addr,
            TransportAddr::AdbForward {
                serial: "SN-42".into(),
                port: 9755,
            }
        );
    }

    #[test]
    fn resolve_ios_usb_produces_usbmuxd_with_device_id() {
        let addr = resolve_transport_addr(&ios_device(7), 9760).expect("resolve");
        assert_eq!(
            addr,
            TransportAddr::Usbmuxd {
                device_id: 7,
                port: 9760,
            }
        );
    }
}
