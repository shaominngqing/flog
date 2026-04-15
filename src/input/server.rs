//! WebSocket server for flog_dart client connections.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::Message;

use super::protocol::{ClientId, ClientInfo, ClientMessage, ServerMessage};

/// A handle for sending downstream messages to connected clients.
#[derive(Clone)]
pub struct ServerHandle {
    clients: Arc<Mutex<HashMap<ClientId, ClientSender>>>,
}

struct ClientSender {
    tx: mpsc::UnboundedSender<String>,
    #[allow(dead_code)]
    info: ClientInfo,
}

/// Events produced by the server for the main event loop.
#[derive(Debug)]
pub enum ServerEvent {
    /// A new client connected and sent Hello.
    ClientConnected(ClientInfo),
    /// A client disconnected.
    ClientDisconnected(ClientId),
    /// A message received from a client.
    Message(ClientId, ClientMessage),
}

/// The flog WebSocket server.
pub struct FlogServer {
    event_rx: mpsc::UnboundedReceiver<ServerEvent>,
    handle: ServerHandle,
}

impl FlogServer {
    /// Start the server on the given port. Returns the server instance.
    pub async fn start(port: u16) -> Result<Self, Box<dyn std::error::Error>> {
        let addr = format!("0.0.0.0:{}", port);
        let listener = TcpListener::bind(&addr).await?;

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let clients: Arc<Mutex<HashMap<ClientId, ClientSender>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let clients_clone = Arc::clone(&clients);
        tokio::spawn(async move {
            let mut next_id: ClientId = 1;
            while let Ok((stream, addr)) = listener.accept().await {
                let id = next_id;
                next_id += 1;
                let event_tx = event_tx.clone();
                let clients = Arc::clone(&clients_clone);
                tokio::spawn(handle_client(id, stream, addr, event_tx, clients));
            }
        });

        Ok(Self {
            event_rx,
            handle: ServerHandle { clients },
        })
    }

    /// Get the next server event. Returns None if server is shut down.
    pub async fn next_event(&mut self) -> Option<ServerEvent> {
        self.event_rx.recv().await
    }

    /// Get a handle for sending downstream messages.
    pub fn handle(&self) -> ServerHandle {
        self.handle.clone()
    }
}

impl ServerHandle {
    /// Broadcast mock rules to all connected clients.
    pub fn broadcast_mock_sync(&self, rules_json: String) {
        let msg = ServerMessage::MockSync { rules: rules_json };
        let json = serde_json::to_string(&msg).unwrap_or_default();
        let clients = self.clients.clone();
        tokio::spawn(async move {
            let map = clients.lock().await;
            for sender in map.values() {
                let _ = sender.tx.send(json.clone());
            }
        });
    }

    /// Send a replay command to the first connected client.
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
        let json = serde_json::to_string(&msg).unwrap_or_default();
        let clients = self.clients.clone();
        tokio::spawn(async move {
            let map = clients.lock().await;
            if let Some(sender) = map.values().next() {
                let _ = sender.tx.send(json);
            }
        });
    }

    /// Get list of connected clients.
    pub async fn client_list(&self) -> Vec<ClientInfo> {
        let map = self.clients.lock().await;
        map.values().map(|s| s.info.clone()).collect()
    }
}

async fn handle_client(
    id: ClientId,
    stream: TcpStream,
    _addr: SocketAddr,
    event_tx: mpsc::UnboundedSender<ServerEvent>,
    clients: Arc<Mutex<HashMap<ClientId, ClientSender>>>,
) {
    let ws_stream = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(_) => return,
    };

    let (mut ws_sink, mut ws_stream_rx) = ws_stream.split();

    // Channel for downstream messages to this client
    let (down_tx, mut down_rx) = mpsc::unbounded_channel::<String>();

    // Spawn downstream writer
    let writer = tokio::spawn(async move {
        while let Some(json) = down_rx.recv().await {
            if ws_sink.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
    });

    // Read first message — must be Hello
    let _client_info = match ws_stream_rx.next().await {
        Some(Ok(Message::Text(text))) => {
            match serde_json::from_str::<ClientMessage>(&text) {
                Ok(ClientMessage::Hello { device, app, os }) => {
                    let info = ClientInfo {
                        id,
                        device,
                        app,
                        os,
                        connected_at: std::time::Instant::now(),
                    };
                    {
                        let mut map = clients.lock().await;
                        map.insert(
                            id,
                            ClientSender {
                                tx: down_tx.clone(),
                                info: info.clone(),
                            },
                        );
                    }
                    let _ = event_tx.send(ServerEvent::ClientConnected(info.clone()));
                    info
                }
                _ => return,
            }
        }
        _ => return,
    };

    // Read loop
    while let Some(msg_result) = ws_stream_rx.next().await {
        match msg_result {
            Ok(Message::Text(text)) => {
                if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
                    let _ = event_tx.send(ServerEvent::Message(id, client_msg));
                }
            }
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => {}
        }
    }

    // Client disconnected
    {
        let mut map = clients.lock().await;
        map.remove(&id);
    }
    let _ = event_tx.send(ServerEvent::ClientDisconnected(id));
    writer.abort();
}
