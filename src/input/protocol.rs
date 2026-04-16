//! Direct Socket protocol — message types for flog ↔ flog_dart communication.

use serde::{Deserialize, Serialize};

/// Unique identifier for a connected client.
pub type ClientId = u64;

/// Information about a connected flog_dart client, extracted from Hello message.
#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub id: ClientId,
    pub app: String,
    pub app_version: String,
    pub os: String,
    pub package_name: String,
    pub port: u16,
    pub build_mode: String,
    pub connected_at: std::time::Instant,
}

/// Messages from Dart client → flog server (upstream).
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    #[serde(rename = "hello")]
    Hello {
        #[serde(default)]
        device: Option<String>,
        app: String,
        #[serde(default)]
        #[serde(rename = "appVersion")]
        app_version: Option<String>,
        os: String,
        #[serde(default)]
        #[serde(rename = "packageName")]
        package_name: Option<String>,
        #[serde(default)]
        port: Option<u16>,
        #[serde(default)]
        #[serde(rename = "buildMode")]
        build_mode: Option<String>,
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
    fn test_deserialize_hello_new_format() {
        let json = r#"{"type":"hello","app":"com.test","appVersion":"1.0.0","os":"ios","packageName":"com.example.test","port":9753}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::Hello { device, app, app_version, os, package_name, port, .. } => {
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
            ClientMessage::Hello { device, app, package_name, port, .. } => {
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
