//! WebSocket client that connects to flog_dart's server on a device.

use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

use super::protocol::{ClientInfo, ClientMessage, ServerMessage};

/// A handle for sending downstream messages to the connected device.
#[derive(Clone)]
pub struct ConnectorHandle {
    tx: mpsc::UnboundedSender<String>,
}

/// Reason a connection was torn down. Structured so status-bar messages
/// can switch on concrete variants instead of parsing error strings.
///
/// Replaces the pre-existing `eprintln!` reports on reader/writer exit
/// which leaked through `EnterAlternateScreen` and polluted the TUI.
#[derive(Debug, Clone)]
pub enum DisconnectReason {
    /// Peer sent a WebSocket Close frame (normal shutdown — app closed).
    PeerClosed,
    /// Reader returned an error (e.g. connection reset).
    ReadError(String),
    /// Writer's `ws_sink.send` returned an error.
    WriteError(String),
    /// The `ConnectorHandle`'s cmd channel closed (all handles dropped).
    WriterChannelClosed,
}

/// Events produced by the connector for the main event loop.
#[derive(Debug)]
pub enum ConnectorEvent {
    Connected(ClientInfo),
    Disconnected { reason: DisconnectReason },
    Message(ClientMessage),
}

impl ConnectorHandle {
    /// Serialize a `ServerMessage` and enqueue it for the writer task.
    ///
    /// Returns `true` when the message was serialized and successfully
    /// queued, `false` when either serialization failed (should not happen
    /// for well-formed variants) or the channel is closed (writer task
    /// exited — typically because the connection dropped).
    ///
    /// Audit ref: TRANS-004. This is the single generic send path;
    /// the `send_mock_sync` / `send_replay` / `send_subscribe` methods
    /// remain as thin wrappers for call-site readability.
    pub fn send(&self, msg: ServerMessage) -> bool {
        match serde_json::to_string(&msg) {
            Ok(json) => self.tx.send(json).is_ok(),
            Err(_) => false,
        }
    }

    pub fn send_mock_sync(&self, rules_json: String) {
        self.send(ServerMessage::MockSync { rules: rules_json });
    }

    pub fn send_replay(
        &self,
        method: String,
        url: String,
        headers: Option<String>,
        body: Option<String>,
    ) {
        self.send(ServerMessage::Replay {
            method,
            url,
            headers,
            body,
        });
    }

    /// Request the Dart app to replay its entire message buffer.
    ///
    /// Used when the TUI switches to this app's session — clears local stores
    /// first, then this triggers a full data re-delivery from the Dart side.
    pub fn send_subscribe(&self) {
        self.send(ServerMessage::Subscribe {});
    }

    /// Construct a dangling handle for characterization tests.
    ///
    /// The returned handle is paired with an `UnboundedReceiver` the caller
    /// owns — sends on the handle either land in that receiver or no-op once
    /// the receiver is dropped. This exists so state-machine tests can build
    /// `ConnectedApp` values without spinning up a real WebSocket.
    #[doc(hidden)]
    pub fn for_testing() -> (Self, mpsc::UnboundedReceiver<String>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { tx }, rx)
    }
}

/// Connect to a flog_dart server at the given WebSocket URL.
pub async fn connect(
    ws_url: &str,
) -> Result<
    (mpsc::UnboundedReceiver<ConnectorEvent>, ConnectorHandle),
    Box<dyn std::error::Error + Send + Sync>,
> {
    let (ws_stream, _) = tokio_tungstenite::connect_async(ws_url).await?;
    let (ws_sink, ws_read) = ws_stream.split();
    setup_connection(ws_sink, ws_read).await
}

/// Connect to a flog_dart server over an existing stream (e.g., usbmuxd tunnel).
/// Performs WebSocket handshake over the stream, then processes messages.
pub async fn connect_stream<S>(
    stream: S,
    url: &str,
) -> Result<
    (mpsc::UnboundedReceiver<ConnectorEvent>, ConnectorHandle),
    Box<dyn std::error::Error + Send + Sync>,
>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let (ws_stream, _) = tokio_tungstenite::client_async(url, stream).await?;
    let (ws_sink, ws_read) = ws_stream.split();
    setup_connection(ws_sink, ws_read).await
}

/// Common setup after WebSocket connection is established.
async fn setup_connection<S>(
    mut ws_sink: SplitSink<WebSocketStream<S>, Message>,
    mut ws_read: SplitStream<WebSocketStream<S>>,
) -> Result<
    (mpsc::UnboundedReceiver<ConnectorEvent>, ConnectorHandle),
    Box<dyn std::error::Error + Send + Sync>,
>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<String>();

    // Read first message — should be Hello from the app (3s timeout).
    //
    // WHY 3 seconds: flog_dart emits its Hello synchronously the instant
    // it accepts a WS upgrade, so on macOS/Linux the handshake typically
    // completes in <50ms. The slowest observed case is an iOS simulator
    // that's still booting its app process at the moment we connect —
    // 2.5s in the worst run we've measured. 3s captures that without
    // making port-scanner false matches (landing on a random HTTP server
    // that WILL complete the WS upgrade but never speaks our protocol)
    // wait noticeably longer than they need to before erroring out.
    //
    // Error surfaces (TRANS-005):
    //   - Timeout fires → "Hello handshake timed out after 3s (port may
    //     not be a flog server)".
    //   - First frame is binary / ping / close → "Expected text frame,
    //     got binary" (binary/other non-text).
    //   - First frame is text but doesn't deserialize to ClientMessage,
    //     or deserializes to a non-Hello variant → "Expected Hello, got
    //     <variant>".
    let first = match tokio::time::timeout(std::time::Duration::from_secs(3), ws_read.next()).await
    {
        Ok(first) => first,
        Err(_) => {
            return Err("Hello handshake timed out after 3s (port may not be a flog server)".into())
        }
    };
    let client_info = match first {
        Some(Ok(Message::Text(text))) => match serde_json::from_str::<ClientMessage>(&text) {
            Ok(ClientMessage::Hello {
                app,
                app_version,
                os,
                package_name,
                port,
                build_mode,
                session_id,
                ..
            }) => ClientInfo {
                id: 1,
                app,
                app_version: app_version.unwrap_or_default(),
                os,
                package_name: package_name.unwrap_or_default(),
                port: port.unwrap_or(0),
                build_mode: build_mode.unwrap_or_default(),
                connected_at: std::time::Instant::now(),
                session_id,
            },
            Ok(other) => {
                let variant = match other {
                    ClientMessage::Log { .. } => "Log",
                    ClientMessage::Net { .. } => "Net",
                    ClientMessage::Hello { .. } => unreachable!("matched above"),
                };
                return Err(format!("Expected Hello, got {variant}").into());
            }
            Err(_) => return Err("Expected Hello, got unrecognized JSON".into()),
        },
        Some(Ok(Message::Binary(_))) => return Err("Expected text frame, got binary".into()),
        Some(Ok(_)) => return Err("Expected text frame, got non-text control frame".into()),
        Some(Err(e)) => return Err(format!("Read error before Hello: {e}").into()),
        None => return Err("Stream closed before Hello".into()),
    };

    let _ = event_tx.send(ConnectorEvent::Connected(client_info));

    // Coordinate writer ↔ reader teardown. Whichever half dies first records
    // the reason in `writer_reason` and signals via `writer_dead`; the reader
    // always emits the final `Disconnected { reason }` event.
    //
    // Critical: previously if the writer died (cmd channel closed or send
    // error), the reader could block indefinitely on `ws_read.next()`. This
    // meant connection teardown was asymmetric depending on which half
    // failed first. The notify_one + shared reason slot make teardown
    // symmetric and bounded-time.
    //
    // std::sync::Mutex (not tokio::sync::Mutex) is deliberate: the critical
    // section is just an Option<T> store (no await, no panicking ops), so
    // tokio::Mutex would add hop-between-executor overhead for zero gain.
    let writer_dead: std::sync::Arc<tokio::sync::Notify> =
        std::sync::Arc::new(tokio::sync::Notify::new());
    let writer_reason: std::sync::Arc<std::sync::Mutex<Option<DisconnectReason>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));

    let writer_dead_w = std::sync::Arc::clone(&writer_dead);
    let writer_reason_w = std::sync::Arc::clone(&writer_reason);
    tokio::spawn(async move {
        let reason = loop {
            match cmd_rx.recv().await {
                Some(json) => {
                    if let Err(e) = ws_sink.send(Message::Text(json.into())).await {
                        break DisconnectReason::WriteError(e.to_string());
                    }
                }
                None => break DisconnectReason::WriterChannelClosed,
            }
        };
        *writer_reason_w.lock().unwrap() = Some(reason);
        // Best-effort: drop sink so the reader's next poll may yield
        // None/Err. The notify_one() below is the guaranteed wakeup path —
        // the sink drop is a shortcut, not a contract.
        drop(ws_sink);
        // notify_one (not notify_waiters) because it stores a permit that
        // persists across the reader loop's iterations; notify_waiters
        // would race with the reader's select! re-registration.
        writer_dead_w.notify_one();
    });

    // Reader task.
    let event_tx_clone = event_tx.clone();
    let writer_reason_r = std::sync::Arc::clone(&writer_reason);
    let writer_dead_r = std::sync::Arc::clone(&writer_dead);
    tokio::spawn(async move {
        let final_reason = loop {
            tokio::select! {
                msg = ws_read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
                                let _ = event_tx_clone.send(ConnectorEvent::Message(client_msg));
                            }
                        }
                        Some(Ok(Message::Close(_))) => break DisconnectReason::PeerClosed,
                        Some(Err(e)) => break DisconnectReason::ReadError(e.to_string()),
                        Some(Ok(_)) => {}
                        None => {
                            break writer_reason_r
                                .lock()
                                .unwrap()
                                .clone()
                                .unwrap_or(DisconnectReason::PeerClosed);
                        }
                    }
                }
                _ = writer_dead_r.notified() => {
                    break writer_reason_r
                        .lock()
                        .unwrap()
                        .clone()
                        .unwrap_or(DisconnectReason::WriterChannelClosed);
                }
            }
        };
        let _ = event_tx_clone.send(ConnectorEvent::Disconnected {
            reason: final_reason,
        });
    });

    Ok((event_rx, ConnectorHandle { tx: cmd_tx }))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── TRANS-004: generic ConnectorHandle::send ─────────────────────────

    #[test]
    fn send_enqueues_serialized_server_message() {
        let (handle, mut rx) = ConnectorHandle::for_testing();
        assert!(handle.send(ServerMessage::Subscribe {}));
        let json = rx.try_recv().expect("message delivered");
        assert_eq!(json, r#"{"type":"subscribe"}"#);
    }

    #[test]
    fn send_returns_false_when_receiver_dropped() {
        // If the writer task (receiver) goes away, send should not panic
        // and should report false so callers can detect lost connections.
        let (handle, rx) = ConnectorHandle::for_testing();
        drop(rx);
        assert!(!handle.send(ServerMessage::Subscribe {}));
    }

    #[test]
    fn send_mock_sync_wrapper_produces_mock_sync_json() {
        let (handle, mut rx) = ConnectorHandle::for_testing();
        handle.send_mock_sync("[]".to_string());
        let json = rx.try_recv().expect("message delivered");
        assert!(json.contains(r#""type":"mock_sync""#));
        assert!(json.contains(r#""rules":"[]""#));
    }

    #[test]
    fn send_replay_wrapper_produces_replay_json() {
        let (handle, mut rx) = ConnectorHandle::for_testing();
        handle.send_replay(
            "GET".to_string(),
            "https://example.com".to_string(),
            None,
            None,
        );
        let json = rx.try_recv().expect("message delivered");
        assert!(json.contains(r#""type":"replay""#));
        assert!(json.contains(r#""method":"GET""#));
    }

    // Disconnect reason coverage. These run a real loopback WS server to
    // exercise the actual setup_connection teardown paths — not mocks.
    use futures_util::SinkExt;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn disconnect_reason_peer_close_carried() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            let hello = r#"{"type":"hello","app":"x","os":"t"}"#;
            ws.send(Message::Text(hello.into())).await.unwrap();
            ws.close(None).await.unwrap();
        });

        let url = format!("ws://{}", addr);
        let (mut rx, _handle) = connect(&url).await.unwrap();

        let mut saw_connected = false;
        let mut got_disconnect: Option<DisconnectReason> = None;
        while let Some(evt) = rx.recv().await {
            match evt {
                ConnectorEvent::Connected(_) => saw_connected = true,
                ConnectorEvent::Disconnected { reason } => {
                    got_disconnect = Some(reason);
                    break;
                }
                ConnectorEvent::Message(_) => {}
            }
        }
        assert!(saw_connected);
        assert!(matches!(got_disconnect, Some(DisconnectReason::PeerClosed)));
    }

    #[tokio::test]
    async fn writer_drop_triggers_reader_disconnect() {
        // Handle dropped → writer's cmd_rx.recv returns None → writer exits
        // with WriterChannelClosed → notify_one → reader unblocks and emits
        // Disconnected. Proves teardown is bounded-time regardless of which
        // half dies first.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            let hello = r#"{"type":"hello","app":"x","os":"t"}"#;
            ws.send(Message::Text(hello.into())).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        });

        let url = format!("ws://{}", addr);
        let (mut rx, handle) = connect(&url).await.unwrap();
        let _ = rx.recv().await; // consume Connected
        drop(handle);

        let disc = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("reader should disconnect within 2s after handle drop");
        assert!(
            matches!(
                disc,
                Some(ConnectorEvent::Disconnected {
                    reason: DisconnectReason::WriterChannelClosed
                })
            ),
            "expected WriterChannelClosed on handle drop, got {:?}",
            disc
        );
    }
}
