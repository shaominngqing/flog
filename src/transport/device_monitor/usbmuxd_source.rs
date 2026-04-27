//! usbmuxd `Listen` source (macOS only): streams Added/Removed
//! events for iOS real devices connected over USB.

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
    let (header, body) = encode_listen_frame().map_err(|e| std::io::Error::other(e.to_string()))?;
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
async fn read_message_any<R: tokio::io::AsyncRead + Unpin>(r: &mut R) -> Option<plist::Dictionary> {
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
mod tests;
