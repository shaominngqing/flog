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
