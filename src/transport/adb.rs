//! ADB forward for Android device connectivity.

use std::sync::atomic::{AtomicU16, Ordering};
use tokio::process::Command;

/// Monotonic counter — combined with the local-port pool constants below to
/// cycle safely across concurrent `adb forward` allocations without repeating
/// the same port while a previous forward might still be live.
static PORT_COUNTER: AtomicU16 = AtomicU16::new(0);

/// Base port for the adb-forward local-port pool. Chosen to land above the
/// common user-service range (1024–5000) and below the ephemeral floor used
/// by most kernels (typical Linux/macOS start at 32768). `0x4d09` is also
/// free of well-known collisions.
/// Audit ref: TRANS-002.
const ADB_LOCAL_PORT_POOL_BASE: u16 = 19753;

/// Size of the adb-forward local-port pool — the allocator cycles through
/// `ADB_LOCAL_PORT_POOL_BASE..ADB_LOCAL_PORT_POOL_BASE + ADB_LOCAL_PORT_POOL_SIZE`
/// (i.e. 19753..29752). 10_000 ports is more than enough slots for every
/// device × port-scan combination we ever hold open simultaneously.
/// Audit ref: TRANS-002.
const ADB_LOCAL_PORT_POOL_SIZE: u16 = 10000;

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

/// Pure helper: compute the next local port in the
/// `ADB_LOCAL_PORT_POOL_BASE + (offset % ADB_LOCAL_PORT_POOL_SIZE)` cycle.
/// Extracted for testability (TRANS-002). The full `setup_forward` function
/// additionally shells out to `adb`, which is UNTESTABLE: PHYS.
fn next_local_port(offset: u16) -> u16 {
    ADB_LOCAL_PORT_POOL_BASE + (offset % ADB_LOCAL_PORT_POOL_SIZE)
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
        // Offset 0 lands exactly on ADB_LOCAL_PORT_POOL_BASE.
        assert_eq!(next_local_port(0), ADB_LOCAL_PORT_POOL_BASE);
    }

    #[test]
    fn next_local_port_increments_by_one() {
        // Sequential offsets yield sequential ports within the pool.
        assert_eq!(next_local_port(1), ADB_LOCAL_PORT_POOL_BASE + 1);
        assert_eq!(next_local_port(5), ADB_LOCAL_PORT_POOL_BASE + 5);
    }

    #[test]
    fn next_local_port_wraps_around_range() {
        // At offset == ADB_LOCAL_PORT_POOL_SIZE we wrap back to base.
        assert_eq!(
            next_local_port(ADB_LOCAL_PORT_POOL_SIZE),
            ADB_LOCAL_PORT_POOL_BASE
        );
        assert_eq!(
            next_local_port(ADB_LOCAL_PORT_POOL_SIZE + 7),
            ADB_LOCAL_PORT_POOL_BASE + 7
        );
    }

    #[test]
    fn next_local_port_stays_in_19753_29752_band() {
        // Spot-check: every offset maps into the documented band.
        for offset in [
            0u16,
            1,
            100,
            ADB_LOCAL_PORT_POOL_SIZE - 1,
            ADB_LOCAL_PORT_POOL_SIZE,
            u16::MAX,
        ] {
            let p = next_local_port(offset);
            assert!(p >= ADB_LOCAL_PORT_POOL_BASE, "{} below pool base", p);
            assert!(
                p < ADB_LOCAL_PORT_POOL_BASE + ADB_LOCAL_PORT_POOL_SIZE,
                "{} above pool",
                p
            );
        }
    }

    #[test]
    fn adb_local_port_pool_constants_have_expected_values() {
        // TRANS-002: lock the documented port-pool layout so a casual rename
        // or re-tuning is caught by the test suite.
        assert_eq!(ADB_LOCAL_PORT_POOL_BASE, 19753);
        assert_eq!(ADB_LOCAL_PORT_POOL_SIZE, 10000);
    }

    #[test]
    fn allocate_local_port_advances_counter() {
        // Two consecutive calls differ by one in the cycle.
        let a = allocate_local_port();
        let b = allocate_local_port();
        // Same pool, consecutive values (mod ADB_LOCAL_PORT_POOL_SIZE).
        let diff = b.wrapping_sub(a);
        assert_eq!(diff, 1);
    }

    // UNTESTABLE: PHYS shell-out to `adb` — setup_forward() at line 13.
    // UNTESTABLE: PHYS shell-out to `adb` — remove_forward() at line 37.
}
