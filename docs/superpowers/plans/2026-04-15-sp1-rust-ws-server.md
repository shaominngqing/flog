# SP1: Rust WS Server Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace flog's entire input layer (VM Service, ADB logcat, stdin) with a single WebSocket server that accepts flog_dart client connections, receiving logs and network events, and sending mock/replay commands downstream.

**Architecture:** New `input/protocol.rs` defines message types (ClientMessage/ServerMessage). New `input/server.rs` runs a tokio-tungstenite WS server. `main.rs` is rewritten to a simple two-select event loop (server messages + terminal events). Old input sources (discover, vm_service, adb, stdin) are deleted. CLI simplified to `--port`.

**Tech Stack:** Rust, tokio, tokio-tungstenite (server mode), serde_json, ratatui

---

## Task Sequence

| Task | Description | Files |
|------|-------------|-------|
| 1 | Protocol types + serde | Create input/protocol.rs, modify input/mod.rs |
| 2 | WS Server implementation | Create input/server.rs |
| 3 | Integration test | Create tests/ws_server_test.rs |
| 4 | Rewrite CLI | Rewrite cli.rs |
| 5 | Rewrite main.rs event loop | Rewrite main.rs |
| 6 | Modify app.rs state | Modify app.rs |
| 7 | Modify event.rs mock/replay | Modify event.rs |
| 8 | Simplify domain/entry.rs | Modify domain/entry.rs |
| 9 | Delete old input sources | Delete 4 files |
| 10 | Update source_select UI | Modify ui/source_select.rs |
| 11 | Update docs | Modify README.md, CLAUDE.md |

---

### Task 1: Protocol types

**Files:**
- Create: `src/input/protocol.rs`
- Modify: `src/input/mod.rs`

- [ ] **Step 1: Create protocol.rs with message types**

Create `src/input/protocol.rs`:

```rust
//! Direct Socket protocol — message types for flog ↔ flog_dart communication.

use serde::{Deserialize, Serialize};

/// Unique identifier for a connected client.
pub type ClientId = u64;

/// Information about a connected flog_dart client, extracted from Hello message.
#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub id: ClientId,
    pub device: String,
    pub app: String,
    pub os: String,
    pub connected_at: std::time::Instant,
}

/// Messages from Dart client → flog server (upstream).
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    #[serde(rename = "hello")]
    Hello {
        device: String,
        app: String,
        os: String,
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
    Net(crate::domain::network::FlogNetMessage),
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
    fn test_deserialize_hello() {
        let json = r#"{"type":"hello","device":"iPhone 15","app":"com.test","os":"ios"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::Hello { device, app, os } => {
                assert_eq!(device, "iPhone 15");
                assert_eq!(app, "com.test");
                assert_eq!(os, "ios");
            }
            _ => panic!("expected Hello"),
        }
    }

    #[test]
    fn test_deserialize_log() {
        let json = r#"{"type":"log","level":"info","tag":"Net","message":"hello","error":null,"stackTrace":null}"#;
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
    fn test_deserialize_net() {
        let json = r#"{"type":"net","id":1,"t":"req","p":"http","method":"GET","url":"https://example.com"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        assert!(matches!(msg, ClientMessage::Net(_)));
    }

    #[test]
    fn test_serialize_mock_sync() {
        let msg = ServerMessage::MockSync { rules: "[]".to_string() };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("mock_sync"));
        assert!(json.contains("[]"));
    }

    #[test]
    fn test_deserialize_unknown_type() {
        let json = r#"{"type":"unknown","foo":"bar"}"#;
        let result = serde_json::from_str::<ClientMessage>(json);
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Rewrite input/mod.rs**

Replace `src/input/mod.rs` entirely:

```rust
//! Input layer — Direct Socket server for flog_dart communication.

pub mod protocol;
pub mod server;

pub use protocol::{ClientId, ClientInfo, ClientMessage, ServerMessage};
pub use server::FlogServer;
```

- [ ] **Step 3: Fix FlogNetMessage serde**

The `Net` variant wraps `FlogNetMessage` which already has `#[derive(Deserialize)]` (in `src/domain/network.rs` line 193). But `FlogNetMessage` has an `id` field and the outer JSON also has `id` from the serde tag. We need to ensure no conflict.

Actually, looking at the protocol: for `Net` messages the outer wrapper only has `type: "net"`, all other fields belong to `FlogNetMessage`. So we need to use `#[serde(flatten)]` instead of a tuple variant:

Change the `Net` variant in protocol.rs to:

```rust
    #[serde(rename = "net")]
    Net {
        #[serde(flatten)]
        msg: crate::domain::network::FlogNetMessage,
    },
```

- [ ] **Step 4: Build and run tests**

Run: `cargo build`
Expected: Compiles (server.rs doesn't exist yet, but mod.rs declares it — this will fail).

Actually, comment out `pub mod server;` temporarily so it compiles:

```rust
pub mod protocol;
// pub mod server;  // TODO: Task 2
```

Run: `cargo test input::protocol`
Expected: All 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/input/protocol.rs src/input/mod.rs
git commit -m "feat(socket): add protocol types for Direct Socket communication"
```

---

### Task 2: WS Server implementation

**Files:**
- Create: `src/input/server.rs`
- Modify: `src/input/mod.rs` (uncomment server module)

- [ ] **Step 1: Create server.rs**

Create `src/input/server.rs`:

```rust
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

    /// Send a replay command to the first connected client (or a specific one).
    pub fn send_replay(&self, method: String, url: String, headers: Option<String>, body: Option<String>) {
        let msg = ServerMessage::Replay { method, url, headers, body };
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
    let client_info = match ws_stream_rx.next().await {
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
                    // Register client
                    {
                        let mut map = clients.lock().await;
                        map.insert(id, ClientSender { tx: down_tx.clone(), info: info.clone() });
                    }
                    let _ = event_tx.send(ServerEvent::ClientConnected(info.clone()));
                    info
                }
                _ => return, // First message wasn't Hello — drop connection
            }
        }
        _ => return,
    };

    // Read loop
    while let Some(msg_result) = ws_stream_rx.next().await {
        match msg_result {
            Ok(Message::Text(text)) => {
                match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(client_msg) => {
                        let _ = event_tx.send(ServerEvent::Message(id, client_msg));
                    }
                    Err(_) => {
                        // Malformed message — skip silently
                    }
                }
            }
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => {} // Ping/Pong/Binary — ignore
        }
    }

    // Client disconnected
    {
        let mut map = clients.lock().await;
        map.remove(&id);
    }
    let _ = event_tx.send(ServerEvent::ClientDisconnected(client_info.id));
    writer.abort();
}
```

- [ ] **Step 2: Uncomment server module in mod.rs**

In `src/input/mod.rs`, change `// pub mod server;` to `pub mod server;`.

- [ ] **Step 3: Build**

Run: `cargo build`
Expected: Compiles.

- [ ] **Step 4: Commit**

```bash
git add src/input/server.rs src/input/mod.rs
git commit -m "feat(socket): implement WS server with client session management"
```

---

### Task 3: Integration test

**Files:**
- Create: `tests/ws_server_test.rs`

- [ ] **Step 1: Write integration test**

Create `tests/ws_server_test.rs`:

```rust
//! Integration test for the flog WS server.

use tokio_tungstenite::connect_async;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn test_server_accepts_hello_and_log() {
    // Start server on a random available port
    let server = flog::input::server::FlogServer::start(0).await;
    // Port 0 won't work with our current impl — use a fixed test port
    // Actually, let's use a high port unlikely to conflict
    drop(server);

    let mut server = flog::input::server::FlogServer::start(19753).await.unwrap();

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
        _ => panic!("Expected ClientConnected"),
    }

    // Send a Log message
    ws.send(Message::Text(
        r#"{"type":"log","level":"info","tag":"Test","message":"hello world"}"#.into(),
    ))
    .await
    .unwrap();

    // Server should receive the message
    let event = server.next_event().await.unwrap();
    match event {
        flog::input::server::ServerEvent::Message(_, msg) => {
            match msg {
                flog::input::protocol::ClientMessage::Log { level, tag, message, .. } => {
                    assert_eq!(level, "info");
                    assert_eq!(tag, "Test");
                    assert_eq!(message, "hello world");
                }
                _ => panic!("Expected Log message"),
            }
        }
        _ => panic!("Expected Message event"),
    }

    // Send a Net message
    ws.send(Message::Text(
        r#"{"type":"net","id":1,"t":"req","p":"http","method":"GET","url":"https://example.com"}"#.into(),
    ))
    .await
    .unwrap();

    let event = server.next_event().await.unwrap();
    assert!(matches!(event, flog::input::server::ServerEvent::Message(_, flog::input::protocol::ClientMessage::Net { .. })));

    // Test downstream: mock sync
    let handle = server.handle();
    handle.broadcast_mock_sync("[]".to_string());

    // Client should receive mock_sync
    if let Some(Ok(Message::Text(text))) = ws.next().await {
        assert!(text.contains("mock_sync"));
        assert!(text.contains("[]"));
    } else {
        panic!("Expected mock_sync message");
    }

    // Disconnect
    ws.close(None).await.unwrap();

    // Server should receive ClientDisconnected
    let event = server.next_event().await.unwrap();
    assert!(matches!(event, flog::input::server::ServerEvent::ClientDisconnected(_)));
}
```

- [ ] **Step 2: Run test**

Run: `cargo test --test ws_server_test -- --nocapture`
Expected: Test passes.

- [ ] **Step 3: Commit**

```bash
git add tests/ws_server_test.rs
git commit -m "test(socket): add integration test for WS server"
```

---

### Task 4: Rewrite CLI

**Files:**
- Rewrite: `src/cli.rs`

- [ ] **Step 1: Replace cli.rs**

Replace `src/cli.rs` entirely:

```rust
//! Command-line argument parsing.

use clap::Parser;

/// flog — Flutter Log Viewer & Network Inspector.
///
/// Starts a WebSocket server and waits for flog_dart clients to connect.
#[derive(Parser, Debug)]
#[command(name = "flog", version, about)]
pub struct Cli {
    /// Server port for flog_dart connections
    #[arg(long, default_value = "9753")]
    pub port: u16,

    /// Initial minimum log level (v/d/i/w/e)
    #[arg(long, value_parser = parse_level)]
    pub level: Option<crate::domain::LogLevel>,

    /// Initial tag filter
    #[arg(long)]
    pub tag: Option<String>,
}

fn parse_level(s: &str) -> Result<crate::domain::LogLevel, String> {
    match s.to_lowercase().as_str() {
        "v" | "verbose" => Ok(crate::domain::LogLevel::Verbose),
        "d" | "debug" => Ok(crate::domain::LogLevel::Debug),
        "i" | "info" => Ok(crate::domain::LogLevel::Info),
        "w" | "warn" | "warning" => Ok(crate::domain::LogLevel::Warning),
        "e" | "error" => Ok(crate::domain::LogLevel::Error),
        _ => Err(format!("unknown level '{}', use v/d/i/w/e", s)),
    }
}
```

- [ ] **Step 2: Build**

Run: `cargo build`
Expected: Fails — main.rs still references old `InputMode`, `Cli::input_mode()`, etc. That's fine, Task 5 fixes it.

- [ ] **Step 3: Commit**

```bash
git add src/cli.rs
git commit -m "refactor(cli): simplify to --port only, remove VM/ADB/stdin args"
```

---

### Task 5: Rewrite main.rs

**Files:**
- Rewrite: `src/main.rs`

This is the largest task. The entire file is rewritten to use the new WS server.

- [ ] **Step 1: Replace main.rs**

Replace `src/main.rs` entirely with the new implementation. The key changes:
- No more `source_manager`, `run_vm_service`, `start_adb`, `start_stdin`
- No more `SourceCommand`, `LastSourceType`
- New simple event loop: `select! { server_event, terminal_event }`
- `dispatch_client_message` replaces `dispatch_event`

```rust
//! flog — Flutter Log Viewer & Network Inspector.

mod app;
mod cli;
mod domain;
mod event;
pub mod input;
pub mod parser;
mod replay;
mod session;
mod ui;

use std::io;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::sync::Mutex;

use app::App;
use cli::Cli;
use input::{ClientMessage, ServerEvent};

/// Process a client message and route it to the appropriate store.
fn dispatch_client_message(app: &mut App, msg: ClientMessage) {
    match msg {
        ClientMessage::Hello { .. } => {
            // Already handled by ServerEvent::ClientConnected
        }
        ClientMessage::Log {
            level,
            tag,
            message,
            error,
            stack_trace,
            timestamp,
        } => {
            let log_level = match level.as_str() {
                "verbose" => domain::LogLevel::Verbose,
                "debug" => domain::LogLevel::Debug,
                "info" => domain::LogLevel::Info,
                "warning" => domain::LogLevel::Warning,
                "error" => domain::LogLevel::Error,
                _ => domain::LogLevel::System,
            };
            let mut entry = domain::LogEntry::new(log_level, tag, &message);
            if let Some(e) = error {
                if !e.is_empty() {
                    entry.message.push_str(&format!("\n{}", e));
                }
            }
            if let Some(st) = stack_trace {
                if !st.is_empty() {
                    entry.message.push_str(&format!("\n{}", st));
                }
            }
            if let Some(ts) = timestamp {
                entry.timestamp = format_timestamp(ts);
            }
            app.add_entry(entry);
        }
        ClientMessage::Net { msg } => {
            app.network_store.process_message(msg);
            app.network.invalidate_filter();
        }
    }
}

fn format_timestamp(millis: u64) -> String {
    let secs = millis / 1000;
    let ms = millis % 1000;
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}.{:03}", h, m, s, ms)
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let cli = Cli::parse();

    let app = Arc::new(Mutex::new(App::new()));
    {
        let mut a = app.lock().await;
        session::load_session(&mut a);
        if let Some(level) = cli.level {
            a.filter.min_level = level;
        }
        if let Some(ref tag) = cli.tag {
            a.tag_filter.input = tag.clone();
            a.tag_filter.apply(&mut a.filter);
        }
    }

    // Start WS server
    let mut server = match input::FlogServer::start(cli.port).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to start server on port {}: {}", cli.port, e);
            eprintln!("Try a different port with: flog --port <PORT>");
            std::process::exit(1);
        }
    };

    let server_handle = server.handle();
    {
        let mut a = app.lock().await;
        a.server_handle = Some(server_handle);
        a.server_port = cli.port;
        a.source_name = format!(":{} waiting...", cli.port);
    }

    // Spawn server event processor
    let app_clone = Arc::clone(&app);
    tokio::spawn(async move {
        while let Some(event) = server.next_event().await {
            let mut a = app_clone.lock().await;
            match event {
                ServerEvent::ClientConnected(info) => {
                    a.source_name = format!("{} ({})", info.device, info.app);
                    a.connected = true;
                    a.clients.push(info);
                    a.show_status("Client connected".to_string());
                }
                ServerEvent::ClientDisconnected(id) => {
                    a.clients.retain(|c| c.id != id);
                    if a.clients.is_empty() {
                        a.connected = false;
                        a.source_name = format!(":{} waiting...", a.server_port);
                    } else {
                        let last = a.clients.last().unwrap();
                        a.source_name = format!("{} ({})", last.device, last.app);
                    }
                    a.show_status("Client disconnected".to_string());
                }
                ServerEvent::Message(_, msg) => {
                    dispatch_client_message(&mut a, msg);
                }
            }
        }
    });

    // Install panic hook
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        original_hook(info);
    }));

    // Enter TUI
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(&mut terminal, &app).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    {
        let a = app.lock().await;
        session::save_session(&a);
    }

    result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &Arc<Mutex<App>>,
) -> io::Result<()> {
    let mut mouse_captured = true;

    loop {
        {
            let mut app_guard = app.lock().await;

            // Toggle mouse capture for select mode
            if app_guard.select_mode && mouse_captured {
                let _ = execute!(io::stdout(), DisableMouseCapture);
                mouse_captured = false;
            } else if !app_guard.select_mode && !mouse_captured {
                let _ = execute!(io::stdout(), EnableMouseCapture);
                mouse_captured = true;
            }

            terminal.draw(|f| match app_guard.mode {
                app::AppMode::Help => ui::help::draw_help(f),
                app::AppMode::Stats => match app_guard.active_stats_tab {
                    app::ViewTab::Logs => ui::logs::stats::draw_stats(f, &mut app_guard),
                    app::ViewTab::Network => {
                        ui::network::stats::draw_network_stats(f, &mut app_guard)
                    }
                },
                app::AppMode::MockRuleEdit => {
                    ui::draw(f, &mut app_guard);
                    ui::network::mock_rules::draw_mock_rule_edit(f, &mut app_guard);
                }
                _ => ui::draw(f, &mut app_guard),
            })?;
            if app_guard.should_quit {
                return Ok(());
            }
        }

        if crossterm::event::poll(Duration::from_millis(33))? {
            match crossterm::event::read()? {
                Event::Key(key) => {
                    let mut app = app.lock().await;
                    event::handle_key(&mut app, key);
                }
                Event::Mouse(mouse) => {
                    let mut app = app.lock().await;
                    event::handle_mouse(&mut app, mouse);
                }
                _ => {}
            }
        }
    }
}
```

- [ ] **Step 2: This won't compile yet** — app.rs needs the new fields (`server_handle`, `server_port`, `clients`). Task 6 handles that. Commit the main.rs rewrite:

```bash
git add src/main.rs
git commit -m "refactor(main): rewrite event loop for Direct Socket server"
```

---

### Task 6: Modify app.rs

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Remove old source management fields and add new ones**

In the `App` struct (around line 357), replace the source management section (lines 386-404):

Replace:
```rust
    // Source management
    pub source_name: String,
    pub status_message: Option<(String, u64)>,
    pub connected: bool,
    pub source_command_tx: Option<tokio::sync::mpsc::UnboundedSender<SourceCommand>>,
    pub replay_tx: Option<tokio::sync::mpsc::UnboundedSender<crate::domain::network::NetworkEntry>>,
    pub last_source_type: Option<LastSourceType>,

    // Mock rules
    pub mock_rules: crate::domain::mock::MockRuleStore,
    pub mock_rule_selected: usize,
    pub mock_edit_rule_id: Option<usize>,
    pub mock_edit_field: usize,
    pub mock_edit_top_values: Vec<String>,
    pub mock_edit_body: crate::ui::text_editor::TextEditor,
    pub mock_sync_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
```

With:
```rust
    // Source management
    pub source_name: String,
    pub status_message: Option<(String, u64)>,
    pub connected: bool,
    pub server_handle: Option<crate::input::server::ServerHandle>,
    pub server_port: u16,
    pub clients: Vec<crate::input::ClientInfo>,

    // Mock rules
    pub mock_rules: crate::domain::mock::MockRuleStore,
    pub mock_rule_selected: usize,
    pub mock_edit_rule_id: Option<usize>,
    pub mock_edit_field: usize,
    pub mock_edit_top_values: Vec<String>,
    pub mock_edit_body: crate::ui::text_editor::TextEditor,
```

- [ ] **Step 2: Update App::new() initialization**

In `App::new()`, replace the old field initializations for the removed fields with:

```rust
            server_handle: None,
            server_port: 9753,
            clients: Vec::new(),
```

Remove: `source_command_tx: None`, `replay_tx: None`, `last_source_type: None`, `mock_sync_tx: None`.

- [ ] **Step 3: Remove SourceCommand and LastSourceType**

Remove the `SourceCommand` enum and `LastSourceType` enum definitions from app.rs. Also remove any methods that reference them (`send_source_command()`, `is_vm_service_connected()`, `enter_scanning_on_disconnect()`).

Replace `is_vm_service_connected()` usage with a simple helper:

```rust
    pub fn has_connected_client(&self) -> bool {
        self.connected && !self.clients.is_empty()
    }
```

- [ ] **Step 4: Remove source_select phase logic**

Remove `SourceSelectPhase`, `SourceSelectState`, `SourceDropdownState` if they exist, and related methods. The source UI becomes a simple "waiting for connection" / "connected to device" display.

Remove fields: `show_source_dropdown`, `dropdown_scan_requested`, `dropdown`, `source_select`.

- [ ] **Step 5: Update clear_session_data()**

If `clear_session_data()` references old source types, simplify it to just clear logs/network/mock data.

- [ ] **Step 6: Build**

Run: `cargo build`
Expected: Many errors in event.rs (references to removed fields). Task 7 fixes those.

- [ ] **Step 7: Commit**

```bash
git add src/app.rs
git commit -m "refactor(app): replace VM Service/ADB state with Direct Socket client management"
```

---

### Task 7: Modify event.rs

**Files:**
- Modify: `src/event.rs`

- [ ] **Step 1: Update trigger_mock_sync()**

Replace:
```rust
fn trigger_mock_sync(app: &App) {
    if let Some(tx) = &app.mock_sync_tx {
        let json = app.mock_rules.to_json_string();
        let _ = tx.send(json);
    }
}
```

With:
```rust
fn trigger_mock_sync(app: &App) {
    if let Some(ref handle) = app.server_handle {
        let json = app.mock_rules.to_json_string();
        handle.broadcast_mock_sync(json);
    }
}
```

- [ ] **Step 2: Update replay_selected()**

Replace the replay_tx channel send with server handle:

```rust
fn replay_selected(app: &mut App) {
    let indices = app.network.filtered_indices(&app.network_store).to_vec();
    if let Some(&idx) = indices.get(app.network.selected) {
        if let Some(entry) = app.network_store.get(idx).cloned() {
            if entry.protocol == crate::domain::network::Protocol::Http {
                if let Some(ref handle) = app.server_handle {
                    handle.send_replay(
                        entry.method.clone(),
                        entry.url.clone(),
                        entry.request_headers.clone(),
                        entry.request_body.clone(),
                    );
                    app.show_status("Replaying request...".to_string());
                }
            } else {
                app.show_status("Replay is only available for HTTP requests".to_string());
            }
        }
    }
}
```

- [ ] **Step 3: Replace is_vm_service_connected() checks**

Search for all `is_vm_service_connected()` calls and replace with `has_connected_client()`.

- [ ] **Step 4: Remove source_command references**

Remove any code that sends `SourceCommand` messages. Remove the dropdown/source-select event handling if it references old enums.

- [ ] **Step 5: Build and test**

Run: `cargo build`
Expected: Should get closer to compiling. Fix remaining references to removed types.

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/event.rs
git commit -m "refactor(event): mock sync and replay via Direct Socket server handle"
```

---

### Task 8: Simplify domain/entry.rs

**Files:**
- Modify: `src/domain/entry.rs`

- [ ] **Step 1: Simplify InputSource**

Replace:
```rust
pub enum InputSource {
    Adb,
    VmService,
    Stdin,
}
```

With:
```rust
pub enum InputSource {
    DirectSocket,
}
```

- [ ] **Step 2: Remove from_vm_service_level()**

Remove the `from_vm_service_level()` method (lines 36-44).

- [ ] **Step 3: Fix any references**

Search for `InputSource::Adb`, `InputSource::VmService`, `InputSource::Stdin` and replace with `InputSource::DirectSocket` or remove the match arm.

- [ ] **Step 4: Build**

Run: `cargo build`

- [ ] **Step 5: Commit**

```bash
git add src/domain/entry.rs
git commit -m "refactor(domain): simplify InputSource to DirectSocket only"
```

---

### Task 9: Delete old input sources

**Files:**
- Delete: `src/input/discover.rs`
- Delete: `src/input/vm_service.rs`
- Delete: `src/input/adb.rs`
- Delete: `src/input/stdin_source.rs`

- [ ] **Step 1: Delete files**

```bash
rm src/input/discover.rs src/input/vm_service.rs src/input/adb.rs src/input/stdin_source.rs
```

- [ ] **Step 2: Remove stale references**

Search entire codebase for any remaining references to `discover`, `vm_service`, `adb`, `stdin_source`, `VmServiceSource`, `AdbSource`, `StdinSource` and remove them.

- [ ] **Step 3: Clean up replay module**

Check `src/replay.rs` — if it sends replay via old channel, update to use server handle. If it's already handled by event.rs, no change needed.

- [ ] **Step 4: Build and test**

Run: `cargo build`
Expected: Compiles cleanly.

Run: `cargo test`
Expected: All tests pass. Old ws_connect_test files in `tests/` that test VM Service may fail — delete them if they reference removed code.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor(input): remove VM Service, ADB logcat, and stdin input sources"
```

---

### Task 10: Update source_select UI

**Files:**
- Modify: `src/ui/source_select.rs`

- [ ] **Step 1: Simplify to connection status display**

Replace the multi-phase source selection UI with a simple display:
- If no clients connected: show "Waiting for connection on port {port}..." with animated dots
- If clients connected: show device list (device name, app, OS)

Remove all VM Service scanning animations, ADB device scanning, and mode selection menus.

The exact rendering code depends heavily on current source_select.rs structure (which is large). The implementer should:
1. Read the full current file
2. Remove all `SourceSelectPhase::ScanningVm`, `ScanningAdb`, `PickVmService`, `PickAdbDevice` branches
3. Replace `ChooseType` with a simple waiting display
4. Keep the Catppuccin palette constants

- [ ] **Step 2: Update status bar in network/mod.rs**

The source info in the network status bar should display `app.source_name` which is already set by main.rs to show the connected device.

- [ ] **Step 3: Build and test**

Run: `cargo build`
Run: `cargo test`

- [ ] **Step 4: Commit**

```bash
git add src/ui/source_select.rs src/ui/network/mod.rs
git commit -m "refactor(ui): simplify source select to connection status display"
```

---

### Task 11: Update documentation

**Files:**
- Modify: `README.md`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Update README.md**

Update these sections:
- **用法**: Remove VM Service/ADB/stdin modes. New usage is just `flog` or `flog --port 9754`
- **数据源**: Replace VM Service/ADB/stdin descriptions with Direct Socket description
- **安装**: Keep as-is
- **搭配 flog_dart**: Note that flog_dart update (SP2) is required for the new communication channel

- [ ] **Step 2: Update CLAUDE.md**

Update:
- **Architecture layers**: `input/` description changes to server.rs + protocol.rs
- **Data Flow**: Replace VM Service/logcat flow with Direct Socket flow
- **Key Top-Level Modules**: Remove SourceCommand, LastSourceType, add ServerHandle, ClientInfo

- [ ] **Step 3: Build final verification**

Run: `cargo build --release`
Run: `cargo test`
Expected: Everything passes.

- [ ] **Step 4: Commit**

```bash
git add README.md CLAUDE.md
git commit -m "docs: update README and CLAUDE.md for Direct Socket architecture"
```
