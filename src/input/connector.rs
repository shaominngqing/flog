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
    pub fn send_mock_sync(&self, rules_json: String) {
        let msg = ServerMessage::MockSync { rules: rules_json };
        if let Ok(json) = serde_json::to_string(&msg) {
            let _ = self.tx.send(json);
        }
    }

    pub fn send_replay(
        &self,
        method: String,
        url: String,
        headers: Option<String>,
        body: Option<String>,
    ) {
        let msg = ServerMessage::Replay {
            method,
            url,
            headers,
            body,
        };
        if let Ok(json) = serde_json::to_string(&msg) {
            let _ = self.tx.send(json);
        }
    }

    /// Request the Dart app to replay its entire message buffer.
    ///
    /// Used when the TUI switches to this app's session — clears local stores
    /// first, then this triggers a full data re-delivery from the Dart side.
    pub fn send_subscribe(&self) {
        let msg = ServerMessage::Subscribe {};
        if let Ok(json) = serde_json::to_string(&msg) {
            let _ = self.tx.send(json);
        }
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
