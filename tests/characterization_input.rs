//! Phase 2.5B Task 4 — characterization tests for `src/input/connector.rs`.
//!
//! Exercises every branch of `setup_connection` via the fake flog WS
//! server so the coverage gates (line >= 90 %, branch >= 90 %) hold.
//!
//! Audit refs:
//!   - TRANS-004 ConnectorHandle downstream-message specifics
//!   - TRANS-005 Hello timeout error surfaces
//!   - TRANS-012 ClientMessage variant validation

#![allow(clippy::unwrap_used)]

use std::time::Duration;

use flog::input::connector::{connect, connect_stream, ConnectorEvent};
use tokio::time::timeout;

#[path = "support/mod.rs"]
mod support;

use support::fake_flog_server::{Behavior, FakeServer};

/// Every test is wrapped in this 5-second outer bound — if the bound ever
/// fires we investigate, not bump the timeout.
const TEST_BUDGET: Duration = Duration::from_secs(5);

fn ws_url(server: &FakeServer) -> String {
    format!("ws://{}", server.addr)
}

// -----------------------------------------------------------------
// CONN-201: happy path — Hello is received and Connected event emitted.
// -----------------------------------------------------------------

#[tokio::test]
async fn conn_201_connects_and_receives_hello() {
    timeout(TEST_BUDGET, async {
        let server = FakeServer::spawn(Behavior::NormalHello {
            device: Some("TestDevice".to_string()),
            app: "com.test".to_string(),
        })
        .await;

        let (mut event_rx, _handle) = connect(&ws_url(&server)).await.expect("connect");

        let event = event_rx.recv().await.expect("event");
        match event {
            ConnectorEvent::Connected(info) => {
                assert_eq!(info.app, "com.test");
                assert_eq!(info.os, "test");
                assert_eq!(info.id, 1);
                // Optional-in-protocol fields default to the empty variants
                // when absent from the Hello frame.
                assert_eq!(info.app_version, "");
                assert_eq!(info.package_name, "");
                assert_eq!(info.port, 0);
                assert_eq!(info.build_mode, "");
            }
            other => panic!("expected Connected, got {:?}", other),
        }
    })
    .await
    .expect("test completed within budget");
}

// -----------------------------------------------------------------
// CONN-202: server never sends anything → 3s hello timeout returns Err.
// -----------------------------------------------------------------

#[tokio::test]
async fn conn_202_times_out_when_server_silent() {
    timeout(TEST_BUDGET, async {
        let server = FakeServer::spawn(Behavior::Silent).await;

        let result = connect(&ws_url(&server)).await;
        let err = result.err().expect("silent server must produce Err");
        let msg = err.to_string();
        // Locks TRANS-005: current phrasing contains "Hello timeout".
        assert!(
            msg.contains("Hello timeout"),
            "unexpected error message: {msg}"
        );
    })
    .await
    .expect("test completed within budget");
}

// -----------------------------------------------------------------
// CONN-203: binary frame where Hello expected → "No Hello received".
// -----------------------------------------------------------------

#[tokio::test]
async fn conn_203_rejects_binary_frame() {
    timeout(TEST_BUDGET, async {
        let server = FakeServer::spawn(Behavior::BinaryFrame).await;

        let result = connect(&ws_url(&server)).await;
        let err = result.err().expect("binary frame must produce Err");
        let msg = err.to_string();
        assert!(
            msg.contains("No Hello received"),
            "unexpected error message: {msg}"
        );
    })
    .await
    .expect("test completed within budget");
}

// -----------------------------------------------------------------
// CONN-204: malformed JSON text → "First message was not Hello".
// -----------------------------------------------------------------

#[tokio::test]
async fn conn_204_rejects_malformed_json() {
    timeout(TEST_BUDGET, async {
        let server = FakeServer::spawn(Behavior::MalformedJson).await;

        let result = connect(&ws_url(&server)).await;
        let err = result.err().expect("malformed json must produce Err");
        let msg = err.to_string();
        assert!(
            msg.contains("First message was not Hello"),
            "unexpected error message: {msg}"
        );
    })
    .await
    .expect("test completed within budget");
}

// -----------------------------------------------------------------
// CONN-205: server closes after Hello → Disconnected event is emitted.
// -----------------------------------------------------------------

#[tokio::test]
async fn conn_205_handles_server_disconnect_after_hello() {
    timeout(TEST_BUDGET, async {
        let server = FakeServer::spawn(Behavior::HelloThenDisconnect {
            app: "com.closer".to_string(),
        })
        .await;

        let (mut event_rx, _handle) = connect(&ws_url(&server)).await.expect("connect");

        // First event: Connected from the Hello.
        match event_rx.recv().await.expect("connected") {
            ConnectorEvent::Connected(info) => assert_eq!(info.app, "com.closer"),
            other => panic!("expected Connected, got {:?}", other),
        }

        // Second event: Disconnected once the reader loop observes Close.
        match event_rx.recv().await.expect("disconnected") {
            ConnectorEvent::Disconnected => {}
            other => panic!("expected Disconnected, got {:?}", other),
        }
    })
    .await
    .expect("test completed within budget");
}

// -----------------------------------------------------------------
// CONN-206: log message after hello traverses reader task into channel.
// -----------------------------------------------------------------

#[tokio::test]
async fn conn_206_receives_log_after_hello() {
    timeout(TEST_BUDGET, async {
        let server = FakeServer::spawn(Behavior::HelloPlusOneLog {
            app: "com.logapp".to_string(),
            level: "info".to_string(),
            tag: "Net".to_string(),
            message: "ping".to_string(),
        })
        .await;

        let (mut event_rx, _handle) = connect(&ws_url(&server)).await.expect("connect");

        // Connected first …
        assert!(matches!(
            event_rx.recv().await.expect("connected"),
            ConnectorEvent::Connected(_)
        ));

        // … then the forwarded Log client message.
        match event_rx.recv().await.expect("log message") {
            ConnectorEvent::Message(flog::input::protocol::ClientMessage::Log {
                level,
                tag,
                message,
                ..
            }) => {
                assert_eq!(level.as_deref(), Some("info"));
                assert_eq!(tag.as_deref(), Some("Net"));
                assert_eq!(message, "ping");
            }
            other => panic!("expected Log message, got {:?}", other),
        }
    })
    .await
    .expect("test completed within budget");
}

// -----------------------------------------------------------------
// CONN-207: net req/res pair — two Message events follow Connected.
// -----------------------------------------------------------------

#[tokio::test]
async fn conn_207_receives_net_after_hello() {
    timeout(TEST_BUDGET, async {
        let server = FakeServer::spawn(Behavior::HelloPlusNetPair {
            app: "com.netapp".to_string(),
            id: 77,
            url: "https://example.com/api".to_string(),
        })
        .await;

        let (mut event_rx, _handle) = connect(&ws_url(&server)).await.expect("connect");

        assert!(matches!(
            event_rx.recv().await.expect("connected"),
            ConnectorEvent::Connected(_)
        ));

        // Two Net messages — request then response.
        use flog::domain::network::FlogNetKind;
        match event_rx.recv().await.expect("net req") {
            ConnectorEvent::Message(flog::input::protocol::ClientMessage::Net { msg }) => match msg
            {
                FlogNetKind::Req { id, url, .. } => {
                    assert_eq!(id, 77);
                    assert_eq!(url.as_deref(), Some("https://example.com/api"));
                }
                other => panic!("expected Req variant, got {:?}", other),
            },
            other => panic!("expected Net req, got {:?}", other),
        }
        match event_rx.recv().await.expect("net res") {
            ConnectorEvent::Message(flog::input::protocol::ClientMessage::Net { msg }) => match msg
            {
                FlogNetKind::Res {
                    id,
                    status,
                    duration,
                    ..
                } => {
                    assert_eq!(id, 77);
                    assert_eq!(status, Some(200));
                    assert_eq!(duration, Some(42));
                }
                other => panic!("expected Res variant, got {:?}", other),
            },
            other => panic!("expected Net res, got {:?}", other),
        }
    })
    .await
    .expect("test completed within budget");
}

// -----------------------------------------------------------------
// CONN-208: bad "type" tag in first text frame → rejected as non-Hello.
// -----------------------------------------------------------------

#[tokio::test]
async fn conn_208_rejects_hello_with_bad_type() {
    timeout(TEST_BUDGET, async {
        let server = FakeServer::spawn(Behavior::HelloWithBadType).await;

        let result = connect(&ws_url(&server)).await;
        let err = result.err().expect("bad-type hello must produce Err");
        let msg = err.to_string();
        assert!(
            msg.contains("First message was not Hello"),
            "unexpected error message: {msg}"
        );
    })
    .await
    .expect("test completed within budget");
}

// -----------------------------------------------------------------
// CONN-209: connect() to an unreachable address returns Err synchronously.
// Covers the `connect_async(...).await?` failure branch of `connect`.
// -----------------------------------------------------------------

#[tokio::test]
async fn conn_209_connect_to_unreachable_addr_returns_err() {
    timeout(TEST_BUDGET, async {
        // Port 1 on loopback is reliably closed; connect_async must fail
        // before we ever reach setup_connection.
        let result = connect("ws://127.0.0.1:1").await;
        assert!(result.is_err(), "expected connect error, got Ok");
    })
    .await
    .expect("test completed within budget");
}

// -----------------------------------------------------------------
// CONN-211: connect_stream() succeeds over an in-memory duplex pair.
// Exercises the usbmuxd-tunnel code path without real transport.
// -----------------------------------------------------------------

#[tokio::test]
async fn conn_211_connect_stream_succeeds_over_duplex() {
    use futures_util::SinkExt;
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::Message;

    timeout(TEST_BUDGET, async {
        // tokio::io::duplex gives us two halves of an in-memory pipe.
        let (client_side, server_side) = tokio::io::duplex(64 * 1024);

        // Server half: accept the WS upgrade and send a Hello.
        let server_task = tokio::spawn(async move {
            let mut ws = accept_async(server_side).await.unwrap();
            let hello = r#"{"type":"hello","app":"com.duplex","os":"test"}"#;
            ws.send(Message::Text(hello.into())).await.unwrap();
            // Keep the stream open long enough for the client to finish.
            tokio::time::sleep(Duration::from_millis(200)).await;
        });

        let (mut event_rx, _handle) = connect_stream(client_side, "ws://localhost/flog")
            .await
            .expect("connect_stream");

        match event_rx.recv().await.expect("connected") {
            ConnectorEvent::Connected(info) => {
                assert_eq!(info.app, "com.duplex");
                assert_eq!(info.os, "test");
            }
            other => panic!("expected Connected, got {:?}", other),
        }

        let _ = server_task.await;
    })
    .await
    .expect("test completed within budget");
}

// -----------------------------------------------------------------
// CONN-212: writer task breaks cleanly when the peer goes away.
// Covers the `ws_sink.send(...).is_err() => break` branch.
// -----------------------------------------------------------------

#[tokio::test]
async fn conn_212_writer_task_exits_when_peer_disconnects() {
    timeout(TEST_BUDGET, async {
        let server = FakeServer::spawn(Behavior::HelloThenDisconnect {
            app: "com.gone".to_string(),
        })
        .await;

        let (mut event_rx, handle) = connect(&ws_url(&server)).await.expect("connect");
        assert!(matches!(
            event_rx.recv().await.unwrap(),
            ConnectorEvent::Connected(_)
        ));
        // Wait for the reader to report Disconnected so the socket is torn
        // down on both sides.
        loop {
            match event_rx.recv().await {
                Some(ConnectorEvent::Disconnected) => break,
                Some(_) => continue,
                None => break,
            }
        }

        // Now send a downstream message. The writer task must observe the
        // send error and exit cleanly — this test just asserts the call
        // does not panic (fire-and-forget per current design).
        handle.send_subscribe();
        tokio::time::sleep(Duration::from_millis(50)).await;
    })
    .await
    .expect("test completed within budget");
}

// -----------------------------------------------------------------
// CONN-210: ConnectorHandle downstream API (TRANS-004).
// Exercises all three send_* convenience methods and confirms the writer
// task forwards them verbatim to the peer as JSON text frames.
// -----------------------------------------------------------------

#[tokio::test]
async fn conn_210_handle_sends_mock_sync_replay_subscribe() {
    use futures_util::StreamExt;
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::Message;

    timeout(TEST_BUDGET, async {
        // Hand-rolled server (not FakeServer) so we can read the frames
        // the connector sends back to us.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            // Send Hello first to satisfy the handshake.
            use futures_util::SinkExt;
            let hello = r#"{"type":"hello","app":"com.h","os":"test"}"#;
            ws.send(Message::Text(hello.into())).await.unwrap();

            // Read three downstream messages and return their bodies.
            let mut got = Vec::<String>::new();
            for _ in 0..3 {
                if let Some(Ok(Message::Text(t))) = ws.next().await {
                    got.push(t.to_string());
                }
            }
            got
        });

        let (mut event_rx, handle) = connect(&format!("ws://{addr}")).await.expect("connect");
        assert!(matches!(
            event_rx.recv().await.unwrap(),
            ConnectorEvent::Connected(_)
        ));

        handle.send_mock_sync("[]".to_string());
        handle.send_replay(
            "GET".to_string(),
            "https://example.com/r".to_string(),
            Some(r#"{"K":"V"}"#.to_string()),
            Some("body".to_string()),
        );
        handle.send_subscribe();

        let frames = server_task.await.unwrap();
        assert_eq!(frames.len(), 3);
        assert!(frames[0].contains(r#""type":"mock_sync""#));
        assert!(frames[0].contains(r#""rules":"[]""#));
        assert!(frames[1].contains(r#""type":"replay""#));
        assert!(frames[1].contains(r#""method":"GET""#));
        assert!(frames[2].contains(r#""type":"subscribe""#));
    })
    .await
    .expect("test completed within budget");
}
