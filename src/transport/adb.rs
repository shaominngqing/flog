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
    let local_port = allocate_local_port();

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

/// Pure helper: compute the next local port in the PORT_BASE + (offset %
/// PORT_RANGE) cycle. Extracted for testability (TRANS-002). The full
/// `setup_forward` function additionally shells out to `adb`, which is
/// UNTESTABLE: PHYS.
fn next_local_port(offset: u16) -> u16 {
    PORT_BASE + (offset % PORT_RANGE)
}

/// Reserve the next port from the module-wide cycling pool. Broken out so
/// setup_forward's pure side effects can be driven from tests without
/// invoking the `adb` binary.
fn allocate_local_port() -> u16 {
    let offset = PORT_COUNTER.fetch_add(1, Ordering::Relaxed);
    next_local_port(offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_local_port_within_pool_base() {
        // Offset 0 lands exactly on PORT_BASE.
        assert_eq!(next_local_port(0), PORT_BASE);
    }

    #[test]
    fn next_local_port_increments_by_one() {
        // Sequential offsets yield sequential ports within the pool.
        assert_eq!(next_local_port(1), PORT_BASE + 1);
        assert_eq!(next_local_port(5), PORT_BASE + 5);
    }

    #[test]
    fn next_local_port_wraps_around_range() {
        // At offset == PORT_RANGE we wrap back to PORT_BASE.
        assert_eq!(next_local_port(PORT_RANGE), PORT_BASE);
        assert_eq!(next_local_port(PORT_RANGE + 7), PORT_BASE + 7);
    }

    #[test]
    fn next_local_port_stays_in_19753_29752_band() {
        // Spot-check: every offset maps into the documented band.
        for offset in [0u16, 1, 100, PORT_RANGE - 1, PORT_RANGE, u16::MAX] {
            let p = next_local_port(offset);
            assert!(p >= PORT_BASE, "{} below PORT_BASE", p);
            assert!(p < PORT_BASE + PORT_RANGE, "{} above pool", p);
        }
    }

    #[test]
    fn allocate_local_port_advances_counter() {
        // Two consecutive calls differ by one in the cycle.
        let a = allocate_local_port();
        let b = allocate_local_port();
        // Same pool, consecutive values (mod PORT_RANGE).
        let diff = b.wrapping_sub(a);
        assert_eq!(diff, 1);
    }

    // UNTESTABLE: PHYS shell-out to `adb` — setup_forward() at line 13.
    // UNTESTABLE: PHYS shell-out to `adb` — remove_forward() at line 37.
}
