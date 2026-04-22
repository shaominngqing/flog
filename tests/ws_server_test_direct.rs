//! Integration test for the flog Direct Socket connector.
//!
//! Simulates a flog_dart server and verifies the connector can connect,
//! receive messages, and send downstream commands.

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn test_connector_connects_and_receives_messages() {
    // Start a mock flog_dart server
    // Use port 0 to let OS assign an available port
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    // Spawn connector in background
    let connector_task = tokio::spawn(async move {
        flog::input::connector::connect(&format!("ws://127.0.0.1:{}", port)).await
    });

    // Accept the connection from connector
    let (stream, _) = listener.accept().await.unwrap();
    let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
    let (mut sink, mut read_stream) = ws.split();

    // Send Hello (simulating flog_dart sending hello on connect)
    sink.send(Message::Text(
        r#"{"type":"hello","device":"TestDevice","app":"com.test","os":"android"}"#.into(),
    ))
    .await
    .unwrap();

    // Connector should succeed and return events + handle
    let (mut event_rx, handle) = connector_task.await.unwrap().unwrap();

    // Should receive Connected event
    let event = event_rx.recv().await.unwrap();
    assert!(matches!(
        event,
        flog::input::connector::ConnectorEvent::Connected(_)
    ));

    // Send a Log message (simulating flog_dart pushing data)
    sink.send(Message::Text(
        r#"{"type":"log","level":"info","tag":"Test","message":"hello"}"#.into(),
    ))
    .await
    .unwrap();

    let event = event_rx.recv().await.unwrap();
    assert!(matches!(
        event,
        flog::input::connector::ConnectorEvent::Message(_)
    ));

    // Test downstream: send mock_sync from flog to device
    handle.send_mock_sync("[]".to_string());

    // Mock server should receive it
    if let Some(Ok(Message::Text(text))) = read_stream.next().await {
        let text_str: &str = &text;
        assert!(text_str.contains("mock_sync"));
    } else {
        panic!("Expected mock_sync message");
    }
}
