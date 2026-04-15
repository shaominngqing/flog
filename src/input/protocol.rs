//! Direct Socket protocol — message types for flog ↔ flog_dart communication.

use serde::{Deserialize, Serialize};

/// Unique identifier for a connected client.
pub type ClientId = u64;

/// Information about a connected flog_dart client, extracted from Hello message.
#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub id: ClientId,
    pub device: String,
    pub app: String,
    pub os: String,
    pub connected_at: std::time::Instant,
}

/// Messages from Dart client → flog server (upstream).
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    #[serde(rename = "hello")]
    Hello {
        device: String,
        app: String,
        os: String,
    },
    #[serde(rename = "log")]
    Log {
        level: String,
        tag: String,
        message: String,
        #[serde(default)]
        error: Option<String>,
        #[serde(rename = "stackTrace")]
        #[serde(default)]
        stack_trace: Option<String>,
        #[serde(default)]
        timestamp: Option<u64>,
    },
    #[serde(rename = "net")]
    Net {
        #[serde(flatten)]
        msg: crate::domain::network::FlogNetMessage,
    },
}

/// Messages from flog server → Dart client (downstream).
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    #[serde(rename = "mock_sync")]
    MockSync { rules: String },
    #[serde(rename = "replay")]
    Replay {
        method: String,
        url: String,
        headers: Option<String>,
        body: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_hello() {
        let json = r#"{"type":"hello","device":"iPhone 15","app":"com.test","os":"ios"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::Hello { device, app, os } => {
                assert_eq!(device, "iPhone 15");
                assert_eq!(app, "com.test");
                assert_eq!(os, "ios");
            }
            _ => panic!("expected Hello"),
        }
    }

    #[test]
    fn test_deserialize_log() {
        let json = r#"{"type":"log","level":"info","tag":"Net","message":"hello"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::Log { level, tag, message, .. } => {
                assert_eq!(level, "info");
                assert_eq!(tag, "Net");
                assert_eq!(message, "hello");
            }
            _ => panic!("expected Log"),
        }
    }

    #[test]
    fn test_deserialize_log_with_optional_fields() {
        let json = r#"{"type":"log","level":"error","tag":"DB","message":"fail","error":"timeout","stackTrace":"at main.dart:1","timestamp":1713100800000}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::Log { error, stack_trace, timestamp, .. } => {
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
        assert!(matches!(msg, ClientMessage::Net { .. }));
    }

    #[test]
    fn test_serialize_mock_sync() {
        let msg = ServerMessage::MockSync { rules: "[]".to_string() };
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
    fn test_deserialize_unknown_type() {
        let json = r#"{"type":"unknown","foo":"bar"}"#;
        let result = serde_json::from_str::<ClientMessage>(json);
        assert!(result.is_err());
    }
}
