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

/// Events produced by the connector for the main event loop.
#[derive(Debug)]
pub enum ConnectorEvent {
    Connected(ClientInfo),
    Disconnected,
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

    // Read first message — should be Hello from the app (3s timeout)
    let client_info = match tokio::time::timeout(std::time::Duration::from_secs(3), ws_read.next())
        .await
    {
        Ok(Some(Ok(Message::Text(text)))) => match serde_json::from_str::<ClientMessage>(&text) {
            Ok(ClientMessage::Hello {
                app,
                app_version,
                os,
                package_name,
                port,
                build_mode,
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
            },
            _ => return Err("First message was not Hello".into()),
        },
        Ok(_) => return Err("No Hello received".into()),
        Err(_) => return Err("Hello timeout (not a flog server?)".into()),
    };

    let _ = event_tx.send(ConnectorEvent::Connected(client_info));

    // Spawn writer task
    tokio::spawn(async move {
        while let Some(json) = cmd_rx.recv().await {
            if ws_sink.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
    });

    // Spawn reader task
    let event_tx_clone = event_tx.clone();
    tokio::spawn(async move {
        while let Some(msg_result) = ws_read.next().await {
            match msg_result {
                Ok(Message::Text(text)) => {
                    if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
                        let _ = event_tx_clone.send(ConnectorEvent::Message(client_msg));
                    }
                }
                Ok(Message::Close(_)) => break,
                Err(_) => break,
                _ => {}
            }
        }
        let _ = event_tx_clone.send(ConnectorEvent::Disconnected);
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
}
