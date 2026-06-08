use crate::domain::network::FlogNetKind;
use crate::input::ClientMessage;

use super::client_message_value;

#[test]
fn watch_log_message_value_redacts_bearer_tokens() {
    let value = client_message_value(&ClientMessage::Log {
        level: Some("error".to_string()),
        tag: Some("Auth".to_string()),
        message: "failed with Bearer abc.def.ghi".to_string(),
        error: Some("Bearer secret-token".to_string()),
        stack_trace: None,
        timestamp: Some(123),
    });

    assert_eq!(value["type"], "log");
    assert_eq!(value["level"], "error");
    assert_eq!(value["tag"], "Auth");
    assert_eq!(value["message"], "failed with Bearer [redacted]");
    assert_eq!(value["error"], "Bearer [redacted]");
    assert_eq!(value["timestamp"], 123);
}

#[test]
fn watch_net_response_value_keeps_ai_lookup_id_and_status() {
    let value = client_message_value(&ClientMessage::Net {
        msg: FlogNetKind::Res {
            id: 42,
            status: Some(500),
            duration: Some(87),
            headers: None,
            body: None,
            size: Some(2048),
            error: Some("Bearer api-token".to_string()),
            mocked: Some(false),
            timing: None,
            ts: Some(456),
        },
    });

    assert_eq!(value["type"], "net");
    assert_eq!(value["id"], "net#42");
    assert_eq!(value["net_id"], 42);
    assert_eq!(value["kind"], "res");
    assert_eq!(value["status"], 500);
    assert_eq!(value["duration_ms"], 87);
    assert_eq!(value["size"], 2048);
    assert_eq!(value["error"], "Bearer [redacted]");
    assert_eq!(value["mocked"], false);
    assert_eq!(value["timestamp"], 456);
}

#[test]
fn watch_net_chunk_value_uses_preview_not_full_body() {
    let data = "x".repeat(700);
    let value = client_message_value(&ClientMessage::Net {
        msg: FlogNetKind::Chunk {
            id: 7,
            data: Some(data),
            size: Some(700),
            seq: Some(3),
            event_timing: None,
            ts: None,
        },
    });

    assert_eq!(value["type"], "net");
    assert_eq!(value["kind"], "chunk");
    assert_eq!(value["data"]["present"], true);
    assert_eq!(value["data"]["truncated"], true);
    assert_eq!(value["data"]["original_bytes"], 700);
    assert_eq!(
        value["data"]["preview"].as_str().unwrap().chars().count(),
        500
    );
}
