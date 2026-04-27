use super::*;

// ── tcp_open ────────────────────────────────────────────────────

#[tokio::test]
async fn tcp_open_detects_listener() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let result = tcp_open(port).await;
    assert_eq!(result, Some(port));
}

#[tokio::test]
async fn tcp_open_returns_none_when_no_listener() {
    // Port 1 is privileged and nobody normally listens on it; even
    // if it fails to bind due to perms on some systems, the connect
    // will still fail → None. Use it as "unreachable" sentinel.
    let result = tcp_open(1).await;
    assert_eq!(result, None);
}

// ── is_port_open (TRANS-007) ────────────────────────────────────

#[tokio::test]
async fn is_port_open_true_when_listener_bound() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    assert!(is_port_open(port).await);
}

#[tokio::test]
async fn is_port_open_false_when_nothing_listens() {
    // Port 1 is privileged and nobody normally listens on it.
    assert!(!is_port_open(1).await);
}

// ── device_name (Hello-driven dispatch) ─────────────────────────

#[tokio::test]
async fn device_name_falls_back_when_app_and_os_empty() {
    let hello = Hello {
        os: String::new(),
        app: String::new(),
    };
    // No os, no app → "Simulator" string.
    assert_eq!(device_name(&hello).await, "Simulator");
}

#[tokio::test]
async fn device_name_uses_app_for_non_ios_os() {
    let hello = Hello {
        os: "android".into(),
        app: "MyCoolApp".into(),
    };
    assert_eq!(device_name(&hello).await, "MyCoolApp");
}

// UNTESTABLE: PHYS — device_name() for os=="ios" shells out via
// booted_simulator_name() → xcrun at line 634. Its JSON-parsing
// pure helper is tested below.

// ── parse_simctl_booted ─────────────────────────────────────────

#[test]
fn parse_simctl_booted_finds_first_booted() {
    let json = br#"{
      "devices": {
        "com.apple.CoreSimulator.SimRuntime.iOS-17-4": [
          { "state": "Shutdown", "name": "iPhone 14" },
          { "state": "Booted",  "name": "iPhone 15 Pro" }
        ]
      }
    }"#;
    assert_eq!(parse_simctl_booted(json).as_deref(), Some("iPhone 15 Pro"));
}

#[test]
fn parse_simctl_booted_returns_none_when_no_booted() {
    let json = br#"{ "devices": { "rt": [ { "state": "Shutdown", "name": "X" } ] } }"#;
    assert!(parse_simctl_booted(json).is_none());
}

#[test]
fn parse_simctl_booted_returns_none_on_empty_devices() {
    let json = br#"{ "devices": {} }"#;
    assert!(parse_simctl_booted(json).is_none());
}

#[test]
fn parse_simctl_booted_returns_none_on_malformed_json() {
    assert!(parse_simctl_booted(b"not json").is_none());
}

#[test]
fn parse_simctl_booted_skips_non_array_runtime() {
    // A runtime entry that isn't an array should be skipped, not panic.
    let json = br#"{ "devices": { "weird": "not an array",
                                  "rt": [ { "state": "Booted", "name": "OK" } ] } }"#;
    assert_eq!(parse_simctl_booted(json).as_deref(), Some("OK"));
}

#[test]
fn parse_simctl_booted_missing_name_returns_none() {
    // Booted but no "name" key → should fall through to None for
    // this device; overall function returns None (no other Booted).
    let json = br#"{ "devices": { "rt": [ { "state": "Booted" } ] } }"#;
    assert!(parse_simctl_booted(json).is_none());
}

// ── handshake (integration with FakeServer) ─────────────────────
//
// handshake() is private; we exercise it indirectly via a small
// TcpListener that scripts the required first-frame behaviors.

#[tokio::test]
async fn handshake_returns_some_on_valid_hello() {
    use futures_util::SinkExt;
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::Message;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            let mut ws = accept_async(stream).await.unwrap();
            let hello = serde_json::json!({
                "type": "hello",
                "os": "ios",
                "app": "DemoApp",
            });
            let _ = ws.send(Message::Text(hello.to_string().into())).await;
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    });

    let result = handshake(port).await;
    let hello = result.expect("valid hello");
    assert_eq!(hello.os, "ios");
    assert_eq!(hello.app, "DemoApp");
}

#[tokio::test]
async fn handshake_none_when_first_frame_is_binary() {
    use futures_util::SinkExt;
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::Message;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            if let Ok(mut ws) = accept_async(stream).await {
                let _ = ws.send(Message::Binary(vec![0u8; 8].into())).await;
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    });

    assert!(handshake(port).await.is_none());
}

#[tokio::test]
async fn handshake_none_when_type_field_wrong() {
    use futures_util::SinkExt;
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::Message;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            if let Ok(mut ws) = accept_async(stream).await {
                let wrong = serde_json::json!({ "type": "greetings", "os": "x", "app": "y" });
                let _ = ws.send(Message::Text(wrong.to_string().into())).await;
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    });

    assert!(handshake(port).await.is_none());
}

#[tokio::test]
async fn handshake_none_on_malformed_json() {
    use futures_util::SinkExt;
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::Message;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            if let Ok(mut ws) = accept_async(stream).await {
                let _ = ws.send(Message::Text("not a json".into())).await;
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    });

    assert!(handshake(port).await.is_none());
}

// UNTESTABLE: PHYS shell-out to `xcrun` — booted_simulator_name()
// at line 632. Its pure JSON parser is covered above.
