use super::*;
use crate::commands::ai::output::AiErrorCode;
use crate::transport::TransportAddr;

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
