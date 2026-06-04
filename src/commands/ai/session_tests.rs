use super::*;
use crate::commands::ai::output::AiErrorCode;
use crate::transport::{
    device_monitor::Device, device_monitor::DeviceKind, DeviceEvent, TransportAddr,
};

#[test]
fn select_single_app_accepts_one_candidate() {
    let candidates = vec![AiAppCandidate::for_tests("app-a", "Device")];
    let selected = select_candidate(&candidates, None, None).unwrap();
    assert_eq!(selected.app_id, "app-a");
}

#[test]
fn select_multiple_apps_requires_selector() {
    let candidates = vec![
        AiAppCandidate::for_tests("app-a", "Device A"),
        AiAppCandidate::for_tests("app-b", "Device B"),
    ];
    let err = select_candidate(&candidates, None, None).unwrap_err();
    assert!(matches!(err.code, AiErrorCode::MultipleAppsFound));
}

#[test]
fn select_app_matches_name_package_or_id() {
    let candidates = vec![AiAppCandidate {
        app_id: "local:9753".to_string(),
        app_name: "Demo".to_string(),
        app_version: "1.0.0".to_string(),
        os: "ios".to_string(),
        package_name: "com.example.demo".to_string(),
        build_mode: "debug".to_string(),
        device_id: "dev-1".to_string(),
        device_name: "Device".to_string(),
        port: 9753,
        transport: TransportAddr::Localhost { port: 9753 },
    }];
    assert!(select_candidate(&candidates, Some("Demo"), None).is_ok());
    assert!(select_candidate(&candidates, Some("com.example.demo"), None).is_ok());
    assert!(select_candidate(&candidates, Some("local:9753"), None).is_ok());
}

#[tokio::test]
async fn discover_candidates_probes_device_event_before_deadline_expires() {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    tx.send(DeviceEvent::Added(Device {
        id: "localhost".to_string(),
        name: "macOS".to_string(),
        kind: DeviceKind::Local,
    }))
    .unwrap();
    let _keep_channel_open = tx;

    let start = Instant::now();
    let candidates = discover_candidates_from_events(
        9753,
        start + Duration::from_millis(500),
        None,
        None,
        rx,
        |device, port, _deadline| async move {
            if device.id == "localhost" && port == 9753 {
                Some(AiAppCandidate::for_tests("real-app", "macOS"))
            } else {
                None
            }
        },
    )
    .await;

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].app_id, "real-app");
    assert!(
        start.elapsed() < Duration::from_millis(250),
        "device probing should not consume the full discovery deadline"
    );
}

#[tokio::test]
async fn discover_candidates_waits_for_matching_device_selector() {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    tx.send(DeviceEvent::Added(Device {
        id: "localhost".to_string(),
        name: "macOS".to_string(),
        kind: DeviceKind::Local,
    }))
    .unwrap();
    tx.send(DeviceEvent::Added(Device {
        id: "android-serial".to_string(),
        name: "Android".to_string(),
        kind: DeviceKind::Android,
    }))
    .unwrap();
    let _keep_channel_open = tx;

    let candidates = discover_candidates_from_events(
        9753,
        Instant::now() + Duration::from_millis(500),
        None,
        Some("android-serial"),
        rx,
        |device, port, _deadline| async move {
            if port != 9753 {
                return None;
            }
            Some(AiAppCandidate {
                app_id: format!("{}:{port}", device.id),
                app_name: "Demo".to_string(),
                app_version: "1.0.0".to_string(),
                os: "test".to_string(),
                package_name: "com.example.demo".to_string(),
                build_mode: "debug".to_string(),
                device_id: device.id,
                device_name: device.name,
                port,
                transport: TransportAddr::Localhost { port },
            })
        },
    )
    .await;

    let selected = select_candidate(&candidates, None, Some("android-serial")).unwrap();
    assert_eq!(selected.device_id, "android-serial");
}
