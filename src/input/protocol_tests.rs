use super::*;

#[test]
fn test_deserialize_hello_new_format() {
    let json = r#"{"type":"hello","app":"com.test","appVersion":"1.0.0","os":"ios","packageName":"com.example.test","port":9753}"#;
    let msg: ClientMessage = serde_json::from_str(json).unwrap();
    match msg {
        ClientMessage::Hello {
            device,
            app,
            app_version,
            os,
            package_name,
            port,
            ..
        } => {
            assert_eq!(device, None);
            assert_eq!(app, "com.test");
            assert_eq!(app_version, Some("1.0.0".to_string()));
            assert_eq!(os, "ios");
            assert_eq!(package_name, Some("com.example.test".to_string()));
            assert_eq!(port, Some(9753));
        }
        _ => panic!("expected Hello"),
    }
}

#[test]
fn test_deserialize_hello_legacy_format() {
    // Old Dart clients still send `device` and no `packageName`/`port`
    let json = r#"{"type":"hello","device":"iPhone 15","app":"com.test","appVersion":"1.0.0","os":"ios"}"#;
    let msg: ClientMessage = serde_json::from_str(json).unwrap();
    match msg {
        ClientMessage::Hello {
            device,
            app,
            package_name,
            port,
            ..
        } => {
            assert_eq!(device, Some("iPhone 15".to_string()));
            assert_eq!(app, "com.test");
            assert_eq!(package_name, None);
            assert_eq!(port, None);
        }
        _ => panic!("expected Hello"),
    }
}

#[test]
fn test_deserialize_log() {
    let json = r#"{"type":"log","level":"info","tag":"Net","message":"hello"}"#;
    let msg: ClientMessage = serde_json::from_str(json).unwrap();
    match msg {
        ClientMessage::Log {
            level,
            tag,
            message,
            ..
        } => {
            assert_eq!(level, Some("info".to_string()));
            assert_eq!(tag, Some("Net".to_string()));
            assert_eq!(message, "hello");
        }
        _ => panic!("expected Log"),
    }
}

#[test]
fn test_deserialize_raw_log() {
    let json = r#"{"type":"log","message":"[INFO][Network] → GET /api/scene-types","timestamp":1776324216539}"#;
    let msg: ClientMessage = serde_json::from_str(json).unwrap();
    match msg {
        ClientMessage::Log {
            level,
            tag,
            message,
            timestamp,
            ..
        } => {
            assert_eq!(level, None);
            assert_eq!(tag, None);
            assert!(message.contains("[INFO][Network]"));
            assert_eq!(timestamp, Some(1776324216539));
        }
        _ => panic!("expected Log"),
    }
}

#[test]
fn test_deserialize_log_with_optional_fields() {
    let json = r#"{"type":"log","level":"error","tag":"DB","message":"fail","error":"timeout","stackTrace":"at main.dart:1","timestamp":1713100800000}"#;
    let msg: ClientMessage = serde_json::from_str(json).unwrap();
    match msg {
        ClientMessage::Log {
            error,
            stack_trace,
            timestamp,
            ..
        } => {
            assert_eq!(error, Some("timeout".to_string()));
            assert_eq!(stack_trace, Some("at main.dart:1".to_string()));
            assert_eq!(timestamp, Some(1713100800000));
        }
        _ => panic!("expected Log"),
    }
}

#[test]
fn test_deserialize_net() {
    let json = r#"{"type":"net","id":1,"t":"req","p":"http","method":"GET","url":"https://example.com"}"#;
    let msg: ClientMessage = serde_json::from_str(json).unwrap();
    use crate::domain::network::FlogNetKind;
    match msg {
        ClientMessage::Net { msg } => assert!(matches!(msg, FlogNetKind::Req { .. })),
        _ => panic!("expected Net"),
    }
}

#[test]
fn test_serialize_mock_sync() {
    let msg = ServerMessage::MockSync {
        rules: "[]".to_string(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("mock_sync"));
    assert!(json.contains("[]"));
}

#[test]
fn test_serialize_replay() {
    let msg = ServerMessage::Replay {
        method: "GET".to_string(),
        url: "https://example.com".to_string(),
        headers: None,
        body: None,
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("replay"));
    assert!(json.contains("GET"));
}

#[test]
fn test_serialize_subscribe() {
    let msg = ServerMessage::Subscribe {};
    let json = serde_json::to_string(&msg).unwrap();
    assert_eq!(json, r#"{"type":"subscribe"}"#);
}

#[test]
fn test_deserialize_unknown_type() {
    let json = r#"{"type":"unknown","foo":"bar"}"#;
    let result = serde_json::from_str::<ClientMessage>(json);
    assert!(result.is_err());
}

// ==================================================================
// Phase 2.5B Task 4 — characterization tests for input/protocol.rs
// Audit refs: TRANS-012 (variant validation), TRANS-014 (ClientInfo metadata)
// ==================================================================

// ---- PROTO-101: ClientMessage::Hello variants --------------------

#[test]
fn proto_101_hello_minimal_required_fields_only() {
    // Only app and os are required by the current schema. Everything
    // else is #[serde(default)] → Option::None.
    let json = r#"{"type":"hello","app":"com.min","os":"android"}"#;
    let msg: ClientMessage = serde_json::from_str(json).unwrap();
    match msg {
        ClientMessage::Hello {
            device,
            app,
            app_version,
            os,
            package_name,
            port,
            build_mode,
            session_id,
        } => {
            assert_eq!(device, None);
            assert_eq!(app, "com.min");
            assert_eq!(app_version, None);
            assert_eq!(os, "android");
            assert_eq!(package_name, None);
            assert_eq!(port, None);
            assert_eq!(build_mode, None);
            assert_eq!(session_id, None);
        }
        _ => panic!("expected Hello"),
    }
}

#[test]
fn proto_101_hello_all_fields_present_including_build_mode() {
    let json = r#"{
        "type":"hello",
        "device":"Pixel 7",
        "app":"com.example",
        "appVersion":"2.3.4",
        "os":"android",
        "packageName":"com.example.pkg",
        "port":9753,
        "buildMode":"debug"
    }"#;
    let msg: ClientMessage = serde_json::from_str(json).unwrap();
    match msg {
        ClientMessage::Hello {
            device,
            app,
            app_version,
            os,
            package_name,
            port,
            build_mode,
            session_id,
        } => {
            assert_eq!(device, Some("Pixel 7".to_string()));
            assert_eq!(app, "com.example");
            assert_eq!(app_version, Some("2.3.4".to_string()));
            assert_eq!(os, "android");
            assert_eq!(package_name, Some("com.example.pkg".to_string()));
            assert_eq!(port, Some(9753));
            assert_eq!(build_mode, Some("debug".to_string()));
            // `sessionId` absent → default None.
            assert_eq!(session_id, None);
        }
        _ => panic!("expected Hello"),
    }
}

// ---- PROTO-102: ClientMessage::Log variants ----------------------

#[test]
fn proto_102_log_message_only_minimal() {
    // Only `message` is required; all else #[serde(default)].
    let json = r#"{"type":"log","message":"plain"}"#;
    let msg: ClientMessage = serde_json::from_str(json).unwrap();
    match msg {
        ClientMessage::Log {
            level,
            tag,
            message,
            error,
            stack_trace,
            timestamp,
        } => {
            assert_eq!(level, None);
            assert_eq!(tag, None);
            assert_eq!(message, "plain");
            assert_eq!(error, None);
            assert_eq!(stack_trace, None);
            assert_eq!(timestamp, None);
        }
        _ => panic!("expected Log"),
    }
}

// ---- PROTO-103: ClientMessage::Net via flatten -------------------

#[test]
fn proto_103_net_variant_flattens_flog_net_kind_req() {
    // Net uses #[serde(flatten)] on the typed FlogNetKind enum.
    // Inbound-only (no serialize). Phase 3 DOM-002/006.
    use crate::domain::network::FlogNetKind;
    let json = r#"{
        "type":"net",
        "t":"req",
        "id":42,
        "p":"http",
        "method":"POST",
        "url":"https://api.example.com/v1/x",
        "headers":{"Content-Type":"application/json"},
        "body":"{}",
        "ts":1700000000000
    }"#;
    let m: ClientMessage = serde_json::from_str(json).unwrap();
    match m {
        ClientMessage::Net { msg } => match msg {
            FlogNetKind::Req {
                id,
                p,
                method,
                url,
                ts,
                ..
            } => {
                assert_eq!(id, 42);
                assert_eq!(p.as_deref(), Some("http"));
                assert_eq!(method.as_deref(), Some("POST"));
                assert_eq!(url.as_deref(), Some("https://api.example.com/v1/x"));
                assert_eq!(ts, Some(1_700_000_000_000));
            }
            _ => panic!("expected Req variant"),
        },
        _ => panic!("expected Net"),
    }
}

#[test]
fn proto_103_net_variant_response_shape() {
    use crate::domain::network::FlogNetKind;
    let json = r#"{"type":"net","t":"res","id":7,"status":200,"duration":99,"size":1024}"#;
    let m: ClientMessage = serde_json::from_str(json).unwrap();
    match m {
        ClientMessage::Net { msg } => match msg {
            FlogNetKind::Res {
                id,
                status,
                duration,
                size,
                ..
            } => {
                assert_eq!(id, 7);
                assert_eq!(status, Some(200));
                assert_eq!(duration, Some(99));
                assert_eq!(size, Some(1024));
            }
            _ => panic!("expected Res variant"),
        },
        _ => panic!("expected Net"),
    }
}

// ---- PROTO-104: ClientMessage malformed / missing / unknown ------

#[test]
fn proto_104_malformed_json_returns_err() {
    let result = serde_json::from_str::<ClientMessage>("this is not json at all");
    assert!(result.is_err());
}

#[test]
fn proto_104_empty_string_returns_err() {
    let result = serde_json::from_str::<ClientMessage>("");
    assert!(result.is_err());
}

#[test]
fn proto_104_missing_type_tag_returns_err() {
    // No discriminator field → serde internally-tagged enum fails.
    let result = serde_json::from_str::<ClientMessage>(r#"{"app":"x","os":"y"}"#);
    assert!(result.is_err());
}

#[test]
fn proto_104_hello_missing_required_app_returns_err() {
    // `app` is required (no default); deserialize must fail.
    let result = serde_json::from_str::<ClientMessage>(r#"{"type":"hello","os":"ios"}"#);
    assert!(result.is_err());
}

#[test]
fn proto_104_hello_missing_required_os_returns_err() {
    let result = serde_json::from_str::<ClientMessage>(r#"{"type":"hello","app":"x"}"#);
    assert!(result.is_err());
}

#[test]
fn proto_104_log_missing_required_message_returns_err() {
    let result =
        serde_json::from_str::<ClientMessage>(r#"{"type":"log","level":"info","tag":"T"}"#);
    assert!(result.is_err());
}

#[test]
fn proto_104_net_missing_required_id_returns_err() {
    // FlogNetKind::Req requires id.
    let result = serde_json::from_str::<ClientMessage>(r#"{"type":"net","t":"req"}"#);
    assert!(result.is_err());
}

#[test]
fn proto_104_wrong_variant_tag_returns_err() {
    // "greetings" is not a recognized variant.
    let result =
        serde_json::from_str::<ClientMessage>(r#"{"type":"greetings","app":"x","os":"y"}"#);
    assert!(result.is_err());
}

#[test]
fn proto_104_hello_with_unknown_extra_fields_succeeds() {
    // Forward compat: unknown fields are ignored silently (no
    // #[serde(deny_unknown_fields)]).
    let json = r#"{
        "type":"hello",
        "app":"x",
        "os":"y",
        "futureField":"whatever",
        "anotherUnknown":123
    }"#;
    let msg: ClientMessage = serde_json::from_str(json).unwrap();
    assert!(matches!(msg, ClientMessage::Hello { .. }));
}

// ---- PROTO-110: ServerMessage round-trip -------------------------

#[test]
fn proto_110_mock_sync_shape() {
    let msg = ServerMessage::MockSync {
        rules: r#"[{"id":1}]"#.to_string(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    // Exact shape: tag is "type":"mock_sync", rules field is a string.
    assert!(json.contains(r#""type":"mock_sync""#));
    assert!(json.contains(r#""rules":"[{\"id\":1}]""#));
}

#[test]
fn proto_110_replay_with_all_optional_fields_present() {
    let msg = ServerMessage::Replay {
        method: "POST".to_string(),
        url: "https://example.com/x".to_string(),
        headers: Some(r#"{"K":"V"}"#.to_string()),
        body: Some(r#"{"payload":true}"#.to_string()),
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains(r#""type":"replay""#));
    assert!(json.contains(r#""method":"POST""#));
    assert!(json.contains(r#""url":"https://example.com/x""#));
    assert!(json.contains(r#""headers":"#));
    assert!(json.contains(r#""body":"#));
}

#[test]
fn proto_110_replay_with_none_optional_fields_serializes_as_null() {
    // Option<String> with default serde behavior emits "headers":null
    // rather than omitting the field. Lock this shape.
    let msg = ServerMessage::Replay {
        method: "GET".to_string(),
        url: "https://example.com/".to_string(),
        headers: None,
        body: None,
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains(r#""headers":null"#));
    assert!(json.contains(r#""body":null"#));
}

#[test]
fn proto_110_subscribe_exact_shape() {
    let msg = ServerMessage::Subscribe {};
    let json = serde_json::to_string(&msg).unwrap();
    assert_eq!(json, r#"{"type":"subscribe"}"#);
}

// ---- PROTO-120: ClientInfo shape (TRANS-014) ---------------------

#[test]
fn proto_120_client_info_fields_round_trip() {
    // ClientInfo has no Serialize/Deserialize; just assert the field
    // layout is honored by normal construction. This locks the struct
    // shape so TRANS-014 additions (session_id today; protocol_version,
    // device_id in future) are detected as breaking changes.
    let now = std::time::Instant::now();
    let info = ClientInfo {
        id: 1,
        app: "com.t".to_string(),
        app_version: "1.0".to_string(),
        os: "ios".to_string(),
        package_name: "com.t.pkg".to_string(),
        port: 9753,
        build_mode: "debug".to_string(),
        connected_at: now,
        session_id: Some("sess-42".to_string()),
    };
    assert_eq!(info.id, 1);
    assert_eq!(info.app, "com.t");
    assert_eq!(info.app_version, "1.0");
    assert_eq!(info.os, "ios");
    assert_eq!(info.package_name, "com.t.pkg");
    assert_eq!(info.port, 9753);
    assert_eq!(info.build_mode, "debug");
    assert_eq!(info.connected_at, now);
    assert_eq!(info.session_id.as_deref(), Some("sess-42"));
}

// ---- PROTO-121: session_id field behavior (TRANS-014) -----------

#[test]
fn proto_121_hello_with_session_id_deserializes() {
    // TRANS-014: a Dart client that sends `sessionId` must have it
    // captured on the Hello variant.
    let json = r#"{"type":"hello","app":"com.x","os":"ios","sessionId":"abc-123"}"#;
    let msg: ClientMessage = serde_json::from_str(json).unwrap();
    match msg {
        ClientMessage::Hello { session_id, .. } => {
            assert_eq!(session_id.as_deref(), Some("abc-123"));
        }
        _ => panic!("expected Hello"),
    }
}

#[test]
fn proto_121_hello_without_session_id_defaults_to_none() {
    // TRANS-014: additive — older clients that don't know about
    // `sessionId` must still deserialize cleanly with None.
    let json = r#"{"type":"hello","app":"com.legacy","os":"ios"}"#;
    let msg: ClientMessage = serde_json::from_str(json).unwrap();
    match msg {
        ClientMessage::Hello { session_id, .. } => {
            assert_eq!(session_id, None);
        }
        _ => panic!("expected Hello"),
    }
}

#[test]
fn proto_120_client_info_is_cloneable() {
    let info = ClientInfo {
        id: 99,
        app: "a".to_string(),
        app_version: "v".to_string(),
        os: "o".to_string(),
        package_name: "p".to_string(),
        port: 1,
        build_mode: "b".to_string(),
        connected_at: std::time::Instant::now(),
        session_id: None,
    };
    let clone = info.clone();
    assert_eq!(clone.id, info.id);
    assert_eq!(clone.app, info.app);
}
