use super::*;

fn dev(id: &str) -> Device {
    Device {
        id: id.to_string(),
        name: format!("name-{}", id),
        kind: DeviceKind::Local,
    }
}

#[test]
fn add_emits_once_per_id() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut t = DeviceTracker::new(tx);
    assert!(t.add(dev("A")));
    assert!(!t.add(dev("A"))); // duplicate → no event
    let mut count = 0;
    while rx.try_recv().is_ok() {
        count += 1;
    }
    assert_eq!(count, 1);
}

#[test]
fn contains_reflects_add_remove() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut t = DeviceTracker::new(tx);
    assert!(!t.contains("A"));
    t.add(dev("A"));
    assert!(t.contains("A"));
    t.remove("A");
    assert!(!t.contains("A"));
}

#[test]
fn remove_unknown_id_is_silent() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut t = DeviceTracker::new(tx);
    t.remove("never-added");
    assert!(rx.try_recv().is_err());
}

#[test]
fn remove_after_add_emits_removed() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut t = DeviceTracker::new(tx);
    t.add(dev("A"));
    let _ = rx.try_recv(); // Added
    t.remove("A");
    match rx.try_recv().expect("Removed") {
        DeviceEvent::Removed(id) => assert_eq!(id, "A"),
        _ => panic!("expected Removed"),
    }
}

#[test]
fn removed_since_computes_delta_correctly() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut t = DeviceTracker::new(tx);
    t.add(dev("A"));
    t.add(dev("B"));
    t.add(dev("C"));

    // "Current" snapshot says only B remains.
    let current: std::collections::HashSet<String> = std::iter::once("B".to_string()).collect();
    let mut diff = t.removed_since(&current);
    diff.sort();
    assert_eq!(diff, vec!["A".to_string(), "C".to_string()]);
}

#[test]
fn removed_since_empty_when_snapshot_equals_known() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut t = DeviceTracker::new(tx);
    t.add(dev("A"));
    let current: std::collections::HashSet<String> = std::iter::once("A".to_string()).collect();
    assert!(t.removed_since(&current).is_empty());
}

#[test]
fn drain_emits_removed_for_every_known_and_clears() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut t = DeviceTracker::new(tx);
    t.add(dev("A"));
    t.add(dev("B"));
    // Drop the two Added events.
    let _ = rx.try_recv();
    let _ = rx.try_recv();

    t.drain();
    let mut removed: Vec<String> = Vec::new();
    while let Ok(evt) = rx.try_recv() {
        if let DeviceEvent::Removed(id) = evt {
            removed.push(id);
        }
    }
    removed.sort();
    assert_eq!(removed, vec!["A".to_string(), "B".to_string()]);
    // Second drain is a no-op.
    t.drain();
    assert!(rx.try_recv().is_err());
}

#[test]
fn connection_method_maps_device_kind() {
    let local = Device {
        id: "localhost".into(),
        name: "x".into(),
        kind: DeviceKind::Local,
    };
    assert!(matches!(
        local.connection_method(),
        ConnectionMethod::Localhost
    ));

    let android = Device {
        id: "SN123".into(),
        name: "x".into(),
        kind: DeviceKind::Android,
    };
    match android.connection_method() {
        ConnectionMethod::AdbForward { serial } => assert_eq!(serial, "SN123"),
        _ => panic!("expected AdbForward"),
    }

    let ios = Device {
        id: "SN".into(),
        name: "x".into(),
        kind: DeviceKind::IosUsb { device_id: 42 },
    };
    match ios.connection_method() {
        ConnectionMethod::Usbmuxd { device_id } => assert_eq!(device_id, 42),
        _ => panic!("expected Usbmuxd"),
    }
}

#[test]
fn device_event_debug_is_stable() {
    // Debug impls must not panic; guard against accidental derive removal.
    let ev_added = DeviceEvent::Added(dev("A"));
    let ev_removed = DeviceEvent::Removed("A".into());
    assert!(format!("{:?}", ev_added).contains("Added"));
    assert!(format!("{:?}", ev_removed).contains("Removed"));
}
