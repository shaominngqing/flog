//! ADB forward for Android device connectivity.

use std::sync::atomic::{AtomicU16, Ordering};
use tokio::process::Command;

static NEXT_LOCAL_PORT: AtomicU16 = AtomicU16::new(19753);

/// Set up adb forward for an Android device.
/// Returns the local port that maps to the device's target port.
pub async fn setup_forward(serial: &str, device_port: u16) -> Option<u16> {
    let local_port = NEXT_LOCAL_PORT.fetch_add(1, Ordering::SeqCst);

    let output = Command::new("adb")
        .args(["-s", serial, "forward", &format!("tcp:{}", local_port), &format!("tcp:{}", device_port)])
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
        .args(["-s", serial, "forward", "--remove", &format!("tcp:{}", local_port)])
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
