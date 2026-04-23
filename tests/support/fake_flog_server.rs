//! Fake flog WS server for transport integration tests.
//!
//! Spawns a tokio task listening on 127.0.0.1:0 (auto-assigned port),
//! accepts one client, and speaks a programmable scripted behavior. The
//! server shuts down when the `FakeServer` is dropped.
//!
//! NOTE: the `push_text` method is intentionally omitted from this
//! first revision — wiring a channel back into the spawn task adds
//! enough complexity that the Task 1 plan flags it as follow-up work.
//! Tests needing to push messages mid-session should add a new
//! Behavior variant that emits the desired sequence.
#![allow(dead_code)]

use std::net::SocketAddr;

use futures_util::SinkExt;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

/// A running fake flog WS server. Drop to shut down.
pub struct FakeServer {
    pub addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
}

#[derive(Debug, Clone)]
pub enum Behavior {
    /// Send a well-formed Hello, then idle forever.
    NormalHello { device: Option<String>, app: String },
    /// Accept but never send anything.
    Silent,
    /// Send a binary frame where text is expected.
    BinaryFrame,
    /// Send malformed JSON text.
    MalformedJson,
    /// Send Hello, then close the connection.
    HelloThenDisconnect { app: String },
    /// Send Hello, then send one Log message, then idle.
    HelloPlusOneLog {
        app: String,
        level: String,
        tag: String,
        message: String,
    },
    /// Send Hello, then a req/res pair, then idle.
    HelloPlusNetPair { app: String, id: u64, url: String },
    /// Accept, send a hello whose 'type' field is wrong.
    HelloWithBadType,
}

impl FakeServer {
    /// Spawn a new fake server listening on 127.0.0.1:0.
    pub async fn spawn(behavior: Behavior) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        let (tx, rx) = oneshot::channel::<()>();

        tokio::spawn(async move {
            tokio::select! {
                _ = run_server(listener, behavior) => {},
                _ = rx => {},
            }
        });

        Self {
            addr,
            shutdown: Some(tx),
        }
    }
}

impl Drop for FakeServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

async fn run_server(listener: TcpListener, behavior: Behavior) {
    loop {
        let (stream, _) = match listener.accept().await {
            Ok(pair) => pair,
            Err(_) => return,
        };
        let b = behavior.clone();
        tokio::spawn(async move {
            handle_connection(stream, b).await;
        });
    }
}

async fn handle_connection(stream: tokio::net::TcpStream, behavior: Behavior) {
    let mut ws = match accept_async(stream).await {
        Ok(w) => w,
        Err(_) => return,
    };

    match behavior {
        Behavior::NormalHello { device, app } => {
            let hello = serde_json::json!({
                "type": "hello",
                "device": device,
                "app": app,
                "os": "test",
            });
            let _ = ws.send(Message::Text(hello.to_string().into())).await;
            idle_forever().await;
        }
        Behavior::Silent => {
            idle_forever().await;
        }
        Behavior::BinaryFrame => {
            let _ = ws.send(Message::Binary(vec![0u8; 32].into())).await;
            idle_forever().await;
        }
        Behavior::MalformedJson => {
            let _ = ws.send(Message::Text("not json".into())).await;
            idle_forever().await;
        }
        Behavior::HelloThenDisconnect { app } => {
            let hello = serde_json::json!({
                "type": "hello",
                "app": app,
                "os": "test",
            });
            let _ = ws.send(Message::Text(hello.to_string().into())).await;
            let _ = ws.close(None).await;
        }
        Behavior::HelloPlusOneLog {
            app,
            level,
            tag,
            message,
        } => {
            let hello = serde_json::json!({
                "type": "hello",
                "app": app,
                "os": "test",
            });
            let _ = ws.send(Message::Text(hello.to_string().into())).await;
            let log = serde_json::json!({
                "type": "log",
                "level": level,
                "tag": tag,
                "message": message,
            });
            let _ = ws.send(Message::Text(log.to_string().into())).await;
            idle_forever().await;
        }
        Behavior::HelloPlusNetPair { app, id, url } => {
            let hello = serde_json::json!({
                "type": "hello",
                "app": app,
                "os": "test",
            });
            let _ = ws.send(Message::Text(hello.to_string().into())).await;
            let req = serde_json::json!({
                "type": "net",
                "t": "req",
                "id": id,
                "p": "http",
                "method": "GET",
                "url": url,
            });
            let _ = ws.send(Message::Text(req.to_string().into())).await;
            let res = serde_json::json!({
                "type": "net",
                "t": "res",
                "id": id,
                "status": 200,
                "duration": 42,
            });
            let _ = ws.send(Message::Text(res.to_string().into())).await;
            idle_forever().await;
        }
        Behavior::HelloWithBadType => {
            let bad = serde_json::json!({
                "type": "greetings",
                "app": "x",
                "os": "test",
            });
            let _ = ws.send(Message::Text(bad.to_string().into())).await;
            idle_forever().await;
        }
    }
}

async fn idle_forever() {
    // Sleep long enough that most tests will finish and drop the
    // server before this returns.
    tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
}
