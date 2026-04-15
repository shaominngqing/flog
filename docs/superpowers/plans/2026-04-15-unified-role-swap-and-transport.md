# Unified Direct Socket: Role Swap + Transport Layer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Swap roles so App = WS Server and flog = WS Client, then add transport layer (flutter devices discovery, adb forward, usbmuxd) for full platform support.

**Architecture:** Dart's FlogClient becomes FlogServer (WS server on :9753). Rust's FlogServer becomes DeviceConnector (WS client that connects to devices). Protocol messages unchanged. New transport/ module handles device discovery via `flutter devices --machine`, adb forward for Android, and usbmuxd for iOS real devices.

**Tech Stack:** Rust (tokio, tokio-tungstenite client, plist), Dart (dart:io HttpServer/WebSocket)

---

## Phase 1: Role Swap

### Task 1: Dart — FlogClient → FlogServer

**Files:**
- Delete: `flog_logger/lib/src/flog_client.dart`
- Create: `flog_logger/lib/src/flog_server.dart`
- Modify: `flog_logger/lib/src/flog_net.dart`
- Modify: `flog_logger/lib/flog_logger.dart`
- Modify: `flog_logger/lib/src/flog_dio.dart`

- [ ] **Step 1: Create flog_server.dart**

Create `flog_logger/lib/src/flog_server.dart`:

```dart
/// WebSocket server that accepts connections from flog TUI.
library;

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:dio/dio.dart';

import 'flog_mock_interceptor.dart';
import 'flog_net.dart' show flogEnabled;

/// Singleton WebSocket server for communicating with flog TUI.
///
/// Listens on `0.0.0.0:{port}` and accepts WebSocket connections from flog.
/// When flog connects, sends a `hello` message and begins pushing data.
/// If flog disconnects, the server keeps listening for reconnection.
class FlogServer {
  /// Singleton instance.
  static final FlogServer instance = FlogServer._();

  FlogServer._();

  HttpServer? _httpServer;
  WebSocket? _ws;
  bool _connected = false;
  bool _started = false;
  Dio? _dio;
  int _port = 9753;

  /// Whether flog TUI is currently connected.
  bool get connected => _connected;

  /// Start the server.
  ///
  /// Call once — subsequent calls are no-ops.
  /// Does nothing if [flogEnabled] is false.
  void start({int port = 9753, Dio? dio}) {
    if (!flogEnabled) return;
    if (_started) return;
    _started = true;
    _port = port;
    _dio = dio;
    _startServer();
  }

  /// Send a JSON message to flog TUI.
  ///
  /// Silently drops the message if not connected.
  void send(Map<String, dynamic> data) {
    if (!_connected || _ws == null) return;
    try {
      _ws!.add(jsonEncode(data));
    } catch (_) {
      // Connection may have closed
    }
  }

  Future<void> _startServer() async {
    try {
      _httpServer = await HttpServer.bind('0.0.0.0', _port);
      _httpServer!.listen(_handleRequest);
    } catch (_) {
      // Port may be in use — flog features unavailable but app runs fine
    }
  }

  void _handleRequest(HttpRequest request) {
    if (WebSocketTransformer.isUpgradeRequest(request)) {
      WebSocketTransformer.upgrade(request).then(_handleWebSocket);
    } else {
      request.response
        ..statusCode = HttpStatus.notFound
        ..close();
    }
  }

  void _handleWebSocket(WebSocket ws) {
    // Close previous connection if any
    _ws?.close();
    _ws = ws;
    _connected = true;

    // Send hello
    final hello = {
      'type': 'hello',
      'device': _deviceName(),
      'app': 'flutter',
      'os': _osName(),
    };
    ws.add(jsonEncode(hello));

    ws.listen(
      (message) {
        if (message is String) {
          _onMessage(message);
        }
      },
      onError: (_) => _onDisconnect(),
      onDone: () => _onDisconnect(),
    );
  }

  void _onMessage(String json) {
    try {
      final data = jsonDecode(json) as Map<String, dynamic>;
      final type = data['type'] as String?;
      switch (type) {
        case 'mock_sync':
          final rulesJson = data['rules'] as String? ?? '[]';
          final rules = (jsonDecode(rulesJson) as List)
              .map((r) => FlogMockRule.fromJson(r as Map<String, dynamic>))
              .toList();
          FlogMockInterceptor.updateRules(rules);
          break;
        case 'replay':
          _handleReplay(data);
          break;
      }
    } catch (_) {
      // Malformed message — ignore
    }
  }

  void _handleReplay(Map<String, dynamic> data) {
    if (_dio == null) return;
    final method = data['method'] as String? ?? 'GET';
    final url = data['url'] as String?;
    if (url == null) return;

    final headersJson = data['headers'] as String?;
    Map<String, dynamic>? headers;
    if (headersJson != null) {
      try {
        headers = jsonDecode(headersJson) as Map<String, dynamic>;
      } catch (_) {}
    }

    final body = data['body'] as String?;

    _dio!
        .request(
          url,
          data: body,
          options: Options(method: method, headers: headers),
        )
        .ignore();
  }

  void _onDisconnect() {
    _connected = false;
    _ws = null;
    // Server keeps listening — flog will reconnect
  }

  String _deviceName() {
    try {
      return Platform.localHostname;
    } catch (_) {
      return 'flutter';
    }
  }

  String _osName() {
    try {
      return Platform.operatingSystem;
    } catch (_) {
      return 'unknown';
    }
  }
}
```

- [ ] **Step 2: Update flog_net.dart**

Replace `import 'flog_client.dart';` with `import 'flog_server.dart';` and `FlogClient.instance.send` with `FlogServer.instance.send`:

```dart
/// Internal helper for flog_net protocol.
library;

import 'flog_server.dart';

const flogEnabled = bool.fromEnvironment(
  'FLOG_ENABLED',
  defaultValue: !bool.fromEnvironment('dart.vm.product'),
);

int _nextId = 1;

int nextNetId() => _nextId++;

void emitNet(Map<String, dynamic> data) {
  if (!flogEnabled) return;
  data['type'] = 'net';
  data['ts'] = DateTime.now().millisecondsSinceEpoch;
  FlogServer.instance.send(data);
}
```

- [ ] **Step 3: Update flog_logger.dart**

Replace `FlogClient` references with `FlogServer`:
- `import 'src/flog_client.dart'` → `import 'src/flog_server.dart'`
- `export 'src/flog_client.dart' show FlogClient` → `export 'src/flog_server.dart' show FlogServer`
- In `_log()`: `FlogClient.instance.send` → `FlogServer.instance.send`

- [ ] **Step 4: Update flog_dio.dart**

- Replace `import 'flog_client.dart'` with `import 'flog_server.dart'`
- In constructor: `FlogClient.instance.start(host:, port:, dio:)` → `FlogServer.instance.start(port: flogPort, dio: _inner)`
- Remove `flogHost` parameter (server doesn't need host, it binds to 0.0.0.0)

- [ ] **Step 5: Delete flog_client.dart**

```bash
rm flog_logger/lib/src/flog_client.dart
```

- [ ] **Step 6: Verify Dart**

Run: `dart analyze flog_logger/lib/`
Expected: No errors.

- [ ] **Step 7: Commit**

```bash
git add -A flog_logger/
git commit -m "feat(dart): swap FlogClient → FlogServer (App is now WS server)"
```

---

### Task 2: Rust — FlogServer → DeviceConnector

**Files:**
- Delete: `src/input/server.rs`
- Create: `src/input/connector.rs`
- Modify: `src/input/mod.rs`
- Modify: `src/main.rs`
- Modify: `src/app.rs`
- Modify: `src/event.rs`
- Modify: `tests/ws_server_test_direct.rs`

- [ ] **Step 1: Create connector.rs**

Create `src/input/connector.rs` — a WS client that connects to a device and processes messages:

```rust
//! WebSocket client that connects to flog_dart's server on a device.

use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use super::protocol::{ClientId, ClientInfo, ClientMessage, ServerMessage};

/// A handle for sending downstream messages to the connected device.
#[derive(Clone)]
pub struct ConnectorHandle {
    tx: mpsc::UnboundedSender<String>,
}

/// Events produced by the connector for the main event loop.
#[derive(Debug)]
pub enum ConnectorEvent {
    /// Connected to a device and received Hello.
    Connected(ClientInfo),
    /// Disconnected from device.
    Disconnected,
    /// A message received from the device.
    Message(ClientMessage),
}

impl ConnectorHandle {
    /// Send mock rules to the connected device.
    pub fn send_mock_sync(&self, rules_json: String) {
        let msg = ServerMessage::MockSync { rules: rules_json };
        if let Ok(json) = serde_json::to_string(&msg) {
            let _ = self.tx.send(json);
        }
    }

    /// Send a replay command to the connected device.
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
}

/// Connect to a flog_dart server at the given WebSocket URL.
/// Returns a stream of events and a handle for sending commands.
pub async fn connect(
    ws_url: &str,
) -> Result<
    (
        mpsc::UnboundedReceiver<ConnectorEvent>,
        ConnectorHandle,
    ),
    Box<dyn std::error::Error + Send + Sync>,
> {
    let (ws_stream, _) = tokio_tungstenite::connect_async(ws_url).await?;
    let (mut ws_sink, mut ws_read) = ws_stream.split();

    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<String>();

    // Read first message — should be Hello from the app
    let client_info = match ws_read.next().await {
        Some(Ok(Message::Text(text))) => {
            match serde_json::from_str::<ClientMessage>(&text) {
                Ok(ClientMessage::Hello { device, app, os }) => ClientInfo {
                    id: 1,
                    device,
                    app,
                    os,
                    connected_at: std::time::Instant::now(),
                },
                _ => {
                    return Err("First message was not Hello".into());
                }
            }
        }
        _ => {
            return Err("No Hello received".into());
        }
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
```

- [ ] **Step 2: Update input/mod.rs**

Replace `src/input/mod.rs`:

```rust
//! Input layer — Direct Socket connector for flog_dart communication.

pub mod protocol;
pub mod connector;

pub use protocol::{ClientId, ClientInfo, ClientMessage, ServerMessage};
pub use connector::{ConnectorEvent, ConnectorHandle, connect};
```

- [ ] **Step 3: Delete server.rs**

```bash
rm src/input/server.rs
```

- [ ] **Step 4: Update app.rs**

Replace `ServerHandle` with `ConnectorHandle`:
- `use crate::input::{ClientInfo, ServerHandle}` → `use crate::input::{ClientInfo, ConnectorHandle}`
- `pub server_handle: Option<ServerHandle>` → `pub connector_handle: Option<ConnectorHandle>`
- `server_handle: None` → `connector_handle: None`

- [ ] **Step 5: Update event.rs**

Replace all `server_handle` references:
- `app.server_handle` → `app.connector_handle`
- `handle.broadcast_mock_sync` → `handle.send_mock_sync`

- [ ] **Step 6: Update main.rs**

Rewrite main.rs to use connector instead of server. The key change: flog is now a WS client that connects to `ws://localhost:{port}`.

Replace the server startup + event processing with a connector loop:

```rust
use input::{ClientMessage, ConnectorEvent, connect};

// In main(), replace server startup with connector loop:

    // Spawn connector task — connects to device and retries on disconnect
    let app_for_connector = Arc::clone(&app);
    let port = cli.port;
    tokio::spawn(async move {
        loop {
            let url = format!("ws://localhost:{}", port);
            match connect(&url).await {
                Ok((mut event_rx, handle)) => {
                    {
                        let mut a = app_for_connector.lock().await;
                        a.connector_handle = Some(handle.clone());
                    }
                    while let Some(event) = event_rx.recv().await {
                        let mut a = app_for_connector.lock().await;
                        match event {
                            ConnectorEvent::Connected(info) => {
                                a.source_name = format!("{} ({})", info.device, info.app);
                                a.connected = true;
                                a.clients.push(info.clone());
                                a.show_status(format!("Connected: {} - {}", info.device, info.app));
                                // Sync mock rules to newly connected device
                                let json = a.mock_rules.to_json_string();
                                handle.send_mock_sync(json);
                            }
                            ConnectorEvent::Disconnected => {
                                a.clients.clear();
                                a.connected = false;
                                a.connector_handle = None;
                                a.source_name = format!("Scanning... (port {})", a.server_port);
                                a.show_status("Disconnected".to_string());
                                a.clear_session_data();
                                break; // Exit inner loop to reconnect
                            }
                            ConnectorEvent::Message(msg) => {
                                dispatch_client_message(&mut a, msg);
                            }
                        }
                    }
                }
                Err(_) => {
                    // Connection failed — retry after delay
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }
    });
```

Remove all `FlogServer` imports and usage. Remove `ServerEvent`.

- [ ] **Step 7: Update integration test**

Rewrite `tests/ws_server_test_direct.rs` to test the new direction: test starts a mock WS server (simulating flog_dart), then connects to it via the connector.

```rust
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn test_connector_connects_and_receives_messages() {
    // Start a mock flog_dart server
    let listener = TcpListener::bind("127.0.0.1:19754").await.unwrap();

    // Spawn connector
    let connector_task = tokio::spawn(async {
        flog::input::connector::connect("ws://127.0.0.1:19754").await
    });

    // Accept connection
    let (stream, _) = listener.accept().await.unwrap();
    let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
    let (mut sink, mut stream_rx) = ws.split();

    // Send Hello (simulating flog_dart)
    sink.send(Message::Text(
        r#"{"type":"hello","device":"TestDevice","app":"com.test","os":"android"}"#.into(),
    )).await.unwrap();

    // Connector should succeed
    let (mut event_rx, handle) = connector_task.await.unwrap().unwrap();

    // Should receive Connected event
    let event = event_rx.recv().await.unwrap();
    assert!(matches!(event, flog::input::connector::ConnectorEvent::Connected(_)));

    // Send a Log message (simulating flog_dart sending data)
    sink.send(Message::Text(
        r#"{"type":"log","level":"info","tag":"Test","message":"hello"}"#.into(),
    )).await.unwrap();

    let event = event_rx.recv().await.unwrap();
    assert!(matches!(event, flog::input::connector::ConnectorEvent::Message(_)));

    // Test downstream: send mock_sync
    handle.send_mock_sync("[]".to_string());

    // Mock server should receive it
    if let Some(Ok(Message::Text(text))) = stream_rx.next().await {
        let text_str: &str = &text;
        assert!(text_str.contains("mock_sync"));
    }
}
```

- [ ] **Step 8: Build and test**

Run: `cargo build`
Run: `cargo test`
Expected: All pass.

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "feat(rust): swap FlogServer → DeviceConnector (flog is now WS client)"
```

---

## Phase 2: Transport Layer

### Task 3: Device monitor (flutter devices)

**Files:**
- Create: `src/transport/mod.rs`
- Create: `src/transport/device_monitor.rs`
- Modify: `src/main.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create transport module**

Create `src/transport/mod.rs`:

```rust
//! Transport layer — device discovery and connection routing.

pub mod device_monitor;
pub mod adb;
pub mod usbmuxd;

pub use device_monitor::{DeviceMonitor, FlutterDevice};
```

- [ ] **Step 2: Create device_monitor.rs**

Create `src/transport/device_monitor.rs`:

```rust
//! Device discovery via `flutter devices --machine`.

use std::process::Stdio;
use tokio::process::Command;

#[derive(Debug, Clone, PartialEq)]
pub struct FlutterDevice {
    pub name: String,
    pub id: String,
    pub platform: String,
    pub emulator: bool,
}

pub struct DeviceMonitor {
    known_devices: Vec<FlutterDevice>,
}

impl DeviceMonitor {
    pub fn new() -> Self {
        Self {
            known_devices: Vec::new(),
        }
    }

    /// Scan for devices. Returns (new_devices, removed_devices).
    pub async fn scan(&mut self) -> (Vec<FlutterDevice>, Vec<FlutterDevice>) {
        let devices = Self::query_flutter_devices().await;

        let new: Vec<FlutterDevice> = devices
            .iter()
            .filter(|d| !self.known_devices.contains(d))
            .cloned()
            .collect();

        let removed: Vec<FlutterDevice> = self
            .known_devices
            .iter()
            .filter(|d| !devices.contains(d))
            .cloned()
            .collect();

        self.known_devices = devices;
        (new, removed)
    }

    pub fn devices(&self) -> &[FlutterDevice] {
        &self.known_devices
    }

    async fn query_flutter_devices() -> Vec<FlutterDevice> {
        let output = match Command::new("flutter")
            .args(["devices", "--machine"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .await
        {
            Ok(o) if o.status.success() => o.stdout,
            _ => return Vec::new(),
        };

        let json_str = match String::from_utf8(output) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        let arr: Vec<serde_json::Value> = match serde_json::from_str(&json_str) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };

        arr.iter()
            .filter_map(|v| {
                let name = v.get("name")?.as_str()?.to_string();
                let id = v.get("id")?.as_str()?.to_string();
                let platform = v.get("targetPlatform")?.as_str()?.to_string();
                let emulator = v.get("emulator")?.as_bool().unwrap_or(false);
                // Skip non-mobile platforms
                if platform == "darwin" || platform.starts_with("web-") {
                    return None;
                }
                Some(FlutterDevice {
                    name,
                    id,
                    platform,
                    emulator,
                })
            })
            .collect()
    }
}

/// Determine how to connect to a device.
pub enum ConnectionMethod {
    /// Direct localhost — iOS simulator, macOS
    Localhost,
    /// adb forward — Android devices
    AdbForward { serial: String },
    /// usbmuxd — iOS real device
    Usbmuxd { udid: String },
}

impl FlutterDevice {
    pub fn connection_method(&self) -> ConnectionMethod {
        if self.platform.starts_with("android") {
            ConnectionMethod::AdbForward {
                serial: self.id.clone(),
            }
        } else if self.platform == "ios" && !self.emulator {
            ConnectionMethod::Usbmuxd {
                udid: self.id.clone(),
            }
        } else {
            ConnectionMethod::Localhost
        }
    }
}
```

- [ ] **Step 3: Add transport module to lib.rs and main.rs**

In `src/lib.rs`, add `pub mod transport;`.

In `src/main.rs`, add `mod transport;` (if not using lib.rs approach).

- [ ] **Step 4: Update main.rs connector loop to use device monitor**

Replace the simple `ws://localhost:{port}` connection with device-aware connection:

```rust
    // Spawn device monitor + connector task
    let app_for_connector = Arc::clone(&app);
    let port = cli.port;
    tokio::spawn(async move {
        let mut monitor = transport::DeviceMonitor::new();
        loop {
            // Scan for devices
            let (new_devices, _removed) = monitor.scan().await;

            // Try to connect to first available device
            for device in &new_devices {
                let ws_url = match device.connection_method() {
                    transport::device_monitor::ConnectionMethod::Localhost => {
                        format!("ws://localhost:{}", port)
                    }
                    transport::device_monitor::ConnectionMethod::AdbForward { ref serial } => {
                        let local_port = transport::adb::setup_forward(serial, port).await;
                        match local_port {
                            Some(lp) => format!("ws://localhost:{}", lp),
                            None => continue,
                        }
                    }
                    transport::device_monitor::ConnectionMethod::Usbmuxd { ref udid } => {
                        // TODO: Task 5 implements this
                        continue;
                    }
                };

                // Try connecting
                if let Ok((mut event_rx, handle)) = input::connect(&ws_url).await {
                    // ... same event processing as Task 2 ...
                }
            }

            // Also try localhost directly (for macOS / iOS simulator without flutter devices)
            if monitor.devices().is_empty() {
                let url = format!("ws://localhost:{}", port);
                if let Ok((mut event_rx, handle)) = input::connect(&url).await {
                    // ... process events ...
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });
```

- [ ] **Step 5: Build and test**

Run: `cargo build`
Expected: Compiles (adb and usbmuxd modules are stubs).

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(transport): add device monitor with flutter devices discovery"
```

---

### Task 4: ADB forward

**Files:**
- Create: `src/transport/adb.rs`

- [ ] **Step 1: Create adb.rs**

```rust
//! ADB forward for Android device connectivity.

use std::sync::atomic::{AtomicU16, Ordering};
use tokio::process::Command;

/// Base port for adb forward allocations (increments per device).
static NEXT_LOCAL_PORT: AtomicU16 = AtomicU16::new(19753);

/// Set up adb forward for an Android device.
/// Returns the local port that maps to the device's target port, or None on failure.
pub async fn setup_forward(serial: &str, device_port: u16) -> Option<u16> {
    let local_port = NEXT_LOCAL_PORT.fetch_add(1, Ordering::SeqCst);

    let status = Command::new("adb")
        .args([
            "-s",
            serial,
            "forward",
            &format!("tcp:{}", local_port),
            &format!("tcp:{}", device_port),
        ])
        .output()
        .await
        .ok()?;

    if status.status.success() {
        Some(local_port)
    } else {
        None
    }
}

/// Remove adb forward for a device.
pub async fn remove_forward(serial: &str, local_port: u16) {
    let _ = Command::new("adb")
        .args([
            "-s",
            serial,
            "forward",
            "--remove",
            &format!("tcp:{}", local_port),
        ])
        .output()
        .await;
}

/// Check if adb is available in PATH.
pub async fn is_available() -> bool {
    Command::new("adb")
        .arg("version")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}
```

- [ ] **Step 2: Build and test**

Run: `cargo build`

- [ ] **Step 3: Commit**

```bash
git add src/transport/adb.rs
git commit -m "feat(transport): add adb forward for Android devices"
```

---

### Task 5: usbmuxd for iOS real devices

**Files:**
- Create: `src/transport/usbmuxd.rs`
- Modify: `Cargo.toml` (add plist dependency)

- [ ] **Step 1: Add plist dependency**

In `Cargo.toml`, add:
```toml
plist = "1"
```

- [ ] **Step 2: Create usbmuxd.rs**

```rust
//! usbmuxd protocol client for iOS real device connectivity.
//!
//! Connects to the usbmuxd Unix socket (/var/run/usbmuxd) on macOS
//! and performs device listing and TCP port forwarding.

use std::collections::HashMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

const USBMUXD_SOCKET: &str = "/var/run/usbmuxd";

/// A USB-connected iOS device discovered via usbmuxd.
#[derive(Debug, Clone)]
pub struct UsbDevice {
    pub device_id: u32,
    pub serial_number: String, // UDID
}

/// List all USB-connected iOS devices.
pub async fn list_devices() -> Result<Vec<UsbDevice>, Box<dyn std::error::Error + Send + Sync>> {
    let mut stream = UnixStream::connect(USBMUXD_SOCKET).await?;

    // Send ListDevices request
    let request = plist::Value::Dictionary({
        let mut d = plist::Dictionary::new();
        d.insert("MessageType".to_string(), plist::Value::String("ListDevices".to_string()));
        d.insert("ClientVersionString".to_string(), plist::Value::String("flog".to_string()));
        d.insert("ProgName".to_string(), plist::Value::String("flog".to_string()));
        d
    });
    send_plist(&mut stream, &request, 1).await?;

    // Read response
    let (_, response) = recv_plist(&mut stream).await?;

    // Parse device list
    let mut devices = Vec::new();
    if let Some(plist::Value::Array(device_list)) = response.as_dictionary().and_then(|d| d.get("DeviceList")) {
        for dev in device_list {
            if let Some(props) = dev.as_dictionary().and_then(|d| d.get("Properties")).and_then(|p| p.as_dictionary()) {
                let device_id = props.get("DeviceID").and_then(|v| v.as_unsigned_integer()).unwrap_or(0) as u32;
                let serial = props.get("SerialNumber").and_then(|v| v.as_string()).unwrap_or("").to_string();
                if !serial.is_empty() {
                    devices.push(UsbDevice { device_id, serial_number: serial });
                }
            }
        }
    }

    Ok(devices)
}

/// Connect to a specific port on an iOS device via usbmuxd.
/// Returns a TcpStream-like connection to the device port.
pub async fn connect_device(
    device_id: u32,
    port: u16,
) -> Result<UnixStream, Box<dyn std::error::Error + Send + Sync>> {
    let mut stream = UnixStream::connect(USBMUXD_SOCKET).await?;

    // Port must be in network byte order (big-endian)
    let port_be = (port as u32).to_be();

    let request = plist::Value::Dictionary({
        let mut d = plist::Dictionary::new();
        d.insert("MessageType".to_string(), plist::Value::String("Connect".to_string()));
        d.insert("DeviceID".to_string(), plist::Value::Integer(device_id.into()));
        d.insert("PortNumber".to_string(), plist::Value::Integer(port_be.into()));
        d.insert("ClientVersionString".to_string(), plist::Value::String("flog".to_string()));
        d.insert("ProgName".to_string(), plist::Value::String("flog".to_string()));
        d
    });
    send_plist(&mut stream, &request, 2).await?;

    // Read response
    let (_, response) = recv_plist(&mut stream).await?;

    // Check result
    let result_code = response
        .as_dictionary()
        .and_then(|d| d.get("Number"))
        .and_then(|v| v.as_unsigned_integer())
        .unwrap_or(u64::MAX);

    if result_code != 0 {
        return Err(format!("usbmuxd Connect failed with code {}", result_code).into());
    }

    // Stream is now a direct tunnel to the device port
    Ok(stream)
}

// ── Protocol helpers ──

/// usbmuxd message header: length(4) + version(4) + type(4) + tag(4)
const HEADER_SIZE: usize = 16;

async fn send_plist(
    stream: &mut UnixStream,
    value: &plist::Value,
    tag: u32,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut body = Vec::new();
    value.to_writer_xml(&mut body)?;

    let length = (HEADER_SIZE + body.len()) as u32;
    let version: u32 = 1;
    let msg_type: u32 = 8; // plist

    let mut header = Vec::with_capacity(HEADER_SIZE);
    header.extend_from_slice(&length.to_le_bytes());
    header.extend_from_slice(&version.to_le_bytes());
    header.extend_from_slice(&msg_type.to_le_bytes());
    header.extend_from_slice(&tag.to_le_bytes());

    stream.write_all(&header).await?;
    stream.write_all(&body).await?;
    stream.flush().await?;
    Ok(())
}

async fn recv_plist(
    stream: &mut UnixStream,
) -> Result<(u32, plist::Value), Box<dyn std::error::Error + Send + Sync>> {
    let mut header = [0u8; HEADER_SIZE];
    stream.read_exact(&mut header).await?;

    let length = u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as usize;
    let tag = u32::from_le_bytes([header[12], header[13], header[14], header[15]]);

    let body_len = length - HEADER_SIZE;
    let mut body = vec![0u8; body_len];
    stream.read_exact(&mut body).await?;

    let value = plist::Value::from_reader(std::io::Cursor::new(body))?;
    Ok((tag, value))
}
```

- [ ] **Step 3: Wire usbmuxd into device monitor connection**

In main.rs, update the `Usbmuxd` branch to use the new module:

```rust
transport::device_monitor::ConnectionMethod::Usbmuxd { ref udid } => {
    // Find usbmuxd device ID from UDID
    if let Ok(usb_devices) = transport::usbmuxd::list_devices().await {
        if let Some(usb_dev) = usb_devices.iter().find(|d| d.serial_number == *udid) {
            if let Ok(tunnel) = transport::usbmuxd::connect_device(usb_dev.device_id, port).await {
                // Upgrade tunnel to WebSocket
                // tokio_tungstenite can work on any AsyncRead+AsyncWrite
                let ws = tokio_tungstenite::client_async(
                    format!("ws://localhost:{}", port),
                    tunnel,
                ).await;
                // ... process connection ...
            }
        }
    }
    continue;
}
```

Note: `tokio_tungstenite::client_async` takes any stream, including `UnixStream`. This performs the WebSocket upgrade over the usbmuxd tunnel.

- [ ] **Step 4: Build**

Run: `cargo build`

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(transport): add usbmuxd protocol for iOS real device connectivity"
```

---

### Task 6: Integration and docs

**Files:**
- Modify: `README.md`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Update README**

Update the architecture section to describe the new model (App = server, flog = client), platform support matrix, and usage.

- [ ] **Step 2: Update CLAUDE.md**

Update architecture docs with:
- New input/ module (connector.rs replacing server.rs)
- New transport/ module (device_monitor, adb, usbmuxd)
- Updated data flow

- [ ] **Step 3: Final build + test**

Run: `cargo build --release && cargo test`
Run: `dart analyze flog_logger/lib/`
Run: `cargo install --path .`

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "docs: update README and CLAUDE.md for unified Direct Socket architecture"
```
