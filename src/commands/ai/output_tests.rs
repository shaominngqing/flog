use super::*;

#[test]
fn error_envelope_serializes_code_message_and_next_actions() {
    let json = serde_json::to_value(AiEnvelope::error(
        "snapshot",
        AiError::new(
            AiErrorCode::NoFlogAppFound,
            "No flog_dart app responded on ports 9753-9762 within 5s.",
            vec!["Run `flog ai doctor --format json`".to_string()],
        ),
    ))
    .unwrap();

    assert_eq!(json["ok"], false);
    assert_eq!(json["meta"]["schema_version"], 1);
    assert_eq!(json["error"]["code"], "no_flog_app_found");
    assert_eq!(
        json["error"]["next_actions"][0],
        "Run `flog ai doctor --format json`"
    );
}

#[test]
fn success_envelope_omits_error() {
    let payload = SnapshotPayload::empty_for_tests();
    let json = serde_json::to_value(AiEnvelope::snapshot(payload)).unwrap();
    assert_eq!(json["ok"], true);
    assert!(json.get("error").is_none());
}
