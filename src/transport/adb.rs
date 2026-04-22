//! ADB forward for Android device connectivity.

use std::sync::atomic::{AtomicU16, Ordering};
use tokio::process::Command;

/// Monotonic counter — combined with PORT_BASE/PORT_RANGE to cycle safely.
static PORT_COUNTER: AtomicU16 = AtomicU16::new(0);
const PORT_BASE: u16 = 19753;
const PORT_RANGE: u16 = 10000; // cycle through 19753..29752

/// Set up adb forward for an Android device.
/// Returns the local port that maps to the device's target port.
pub async fn setup_forward(serial: &str, device_port: u16) -> Option<u16> {
    let offset = PORT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let local_port = PORT_BASE + (offset % PORT_RANGE);

    let output = Command::new("adb")
        .args([
            "-s",
            serial,
            "forward",
            &format!("tcp:{}", local_port),
            &format!("tcp:{}", device_port),
        ])
        .output()
        .await
        .ok()?;

    if output.status.success() {
        Some(local_port)
    } else {
        None
    }
}

/// Remove adb forward for a device.
pub async fn remove_forward(serial: &str, local_port: u16) {
    let _ = Command::new("adb")
        .args([
            "-s",
            serial,
            "forward",
            "--remove",
            &format!("tcp:{}", local_port),
        ])
        .output()
        .await;
}

/// Check if adb is available.
pub async fn is_available() -> bool {
    Command::new("adb")
        .arg("version")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}
