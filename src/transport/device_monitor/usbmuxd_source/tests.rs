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
