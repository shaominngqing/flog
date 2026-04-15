//! Integration test for the flog Direct Socket WS server.

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn test_direct_socket_hello_and_messages() {
    // Start server on a high test port
    let mut server = flog::input::server::FlogServer::start(19753)
        .await
        .expect("Failed to start server");

    // Connect client
    let (mut ws, _) = connect_async("ws://127.0.0.1:19753")
        .await
        .expect("Failed to connect");

    // Send Hello
    ws.send(Message::Text(
        r#"{"type":"hello","device":"TestDevice","app":"com.test","os":"android"}"#.into(),
    ))
    .await
    .unwrap();

    // Server should receive ClientConnected
    let event = server.next_event().await.unwrap();
    match event {
        flog::input::server::ServerEvent::ClientConnected(info) => {
            assert_eq!(info.device, "TestDevice");
            assert_eq!(info.app, "com.test");
            assert_eq!(info.os, "android");
        }
        _ => panic!("Expected ClientConnected, got {:?}", "other"),
    }

    // Send a Log message
    ws.send(Message::Text(
        r#"{"type":"log","level":"info","tag":"Test","message":"hello world"}"#.into(),
    ))
    .await
    .unwrap();

    // Server should receive the Log message
    let event = server.next_event().await.unwrap();
    match event {
        flog::input::server::ServerEvent::Message(_, msg) => match msg {
            flog::input::protocol::ClientMessage::Log {
                level,
                tag,
                message,
                ..
            } => {
                assert_eq!(level, "info");
                assert_eq!(tag, "Test");
                assert_eq!(message, "hello world");
            }
            _ => panic!("Expected Log message"),
        },
        _ => panic!("Expected Message event"),
    }

    // Send a Net message
    ws.send(Message::Text(
        r#"{"type":"net","id":1,"t":"req","p":"http","method":"GET","url":"https://example.com"}"#
            .into(),
    ))
    .await
    .unwrap();

    let event = server.next_event().await.unwrap();
    match event {
        flog::input::server::ServerEvent::Message(_, msg) => {
            assert!(matches!(msg, flog::input::protocol::ClientMessage::Net { .. }));
        }
        _ => panic!("Expected Net Message event"),
    }

    // Test downstream: mock sync
    let handle = server.handle();
    handle.broadcast_mock_sync("[]".to_string());

    // Client should receive mock_sync — use a timeout to avoid hanging
    let timeout = tokio::time::timeout(std::time::Duration::from_secs(2), ws.next()).await;
    match timeout {
        Ok(Some(Ok(Message::Text(text)))) => {
            let text_str: &str = &text;
            assert!(text_str.contains("mock_sync"), "Expected mock_sync, got: {}", text_str);
        }
        other => panic!("Expected mock_sync message, got: {:?}", other),
    }

    // Disconnect
    ws.close(None).await.unwrap();

    // Server should receive ClientDisconnected — use timeout
    let timeout = tokio::time::timeout(std::time::Duration::from_secs(2), server.next_event()).await;
    match timeout {
        Ok(Some(flog::input::server::ServerEvent::ClientDisconnected(_))) => {}
        other => panic!("Expected ClientDisconnected, got: {:?}", other),
    }
}
