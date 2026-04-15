# SP1: Rust WS Server — Replace Input Layer with Direct Socket

## Goal

Replace flog's entire input layer (VM Service, ADB logcat, stdin) with a single WebSocket server that accepts connections from flog_dart clients. All data (logs, network events, mock rules, replay) flows through this one channel. No fallbacks, no degradation — if the socket isn't connected, flog simply waits.

## Context

This is Sub-project 1 of a 3-part architecture upgrade:
- **SP1 (this):** Rust WS Server + remove old input layer
- **SP2:** Dart FlogClient (replaces print/developer.log emission)
- **SP3:** Transport layer (adb reverse + usbmuxd for platform connectivity)

## Protocol

### Connection

- flog TUI starts a WebSocket server on `0.0.0.0:{port}` (default 9753)
- flog_dart clients connect to `ws://localhost:{port}`
- Multiple clients supported (multi-device)
- No authentication (dev tool, localhost only)

### Message Format

All messages are JSON with a `type` field. Single WebSocket connection carries both directions.

### Upstream (Dart → flog)

**Hello (handshake, first message after connect):**
```json
{
  "type": "hello",
  "device": "iPhone 15 Pro",
  "app": "com.example.myapp",
  "os": "ios"
}
```

**Log (structured log entry):**
```json
{
  "type": "log",
  "level": "info",
  "tag": "Network",
  "message": "GET /api/users completed in 234ms",
  "error": null,
  "stackTrace": null,
  "timestamp": 1713100800000
}
```

Level values: `verbose`, `debug`, `info`, `warning`, `error`.

**Net (network event — wraps existing FlogNetMessage):**
```json
{
  "type": "net",
  "id": 1,
  "t": "req",
  "p": "http",
  "method": "GET",
  "url": "https://api.example.com/users",
  "headers": "{...}",
  "body": null
}
```

The fields inside `net` messages are identical to the current `FlogNetMessage` struct. The only addition is the outer `type: "net"` wrapper. All existing `t` values work: `req`, `res`, `err`, `open`, `close`, `chunk`, `done`, `send`, `recv`.

### Downstream (flog → Dart)

**Mock rules sync:**
```json
{
  "type": "mock_sync",
  "rules": [
    {
      "url_pattern": "/api/users",
      "method": "GET",
      "status": 200,
      "body": "{\"users\": []}",
      "delay": 0,
      "enabled": true
    }
  ]
}
```

**Replay request:**
```json
{
  "type": "replay",
  "method": "GET",
  "url": "https://api.example.com/users",
  "headers": "{...}",
  "body": null
}
```

## Architecture

### New Files

**`src/input/protocol.rs`** — Message type definitions and serde

Defines:
- `ClientMessage` enum: Hello, Log, Net variants
- `ServerMessage` enum: MockSync, Replay variants
- `ClientInfo` struct: device name, app name, os, connection timestamp
- Deserialization from JSON with proper error handling for unknown/malformed messages

**`src/input/server.rs`** — WebSocket server and client session management

Responsibilities:
- Start tokio-tungstenite WS server on configurable port
- Accept incoming connections, spawn per-client task
- First message must be `Hello` — extract `ClientInfo`, register client
- Route upstream messages through `mpsc::UnboundedSender<(ClientId, ClientMessage)>` to main event loop
- Hold per-client downstream sender for mock_sync/replay pushes
- Handle disconnection: remove client, log status
- Handle reconnection: same device reconnects, replace old session
- Expose: `broadcast_mock_sync(rules_json)`, `send_replay(client_id, entry_json)`

**`tests/ws_server_test.rs`** — Integration test

Tests:
- Server starts and accepts connection
- Hello handshake registers client info
- Log messages are received and parsed
- Net messages are received and parsed
- Mock sync is sent downstream and received by client
- Client disconnect is detected
- Client reconnect works
- Multiple clients can connect simultaneously
- Malformed messages are handled gracefully (logged, not crash)

### Rewritten Files

**`src/input/mod.rs`** — Simplified

Remove all old source types. New exports:
```rust
pub mod protocol;
pub mod server;

pub use protocol::{ClientMessage, ServerMessage, ClientInfo};
pub use server::FlogServer;
```

**`src/main.rs`** — New event loop

Current structure (complex multi-source):
```
source_manager → spawn/cancel sources → SourceCommand channel
run_vm_service → VM WS + mock_rx select
start_adb → logcat read loop
start_stdin → stdin read loop
run_loop → TUI render + terminal events
```

New structure (simple):
```
main:
  1. Parse CLI args (--port, --level, --tag)
  2. Create App
  3. Start FlogServer on port
  4. Enter run_loop:
     select! {
       client_msg = server.next_message() => dispatch_client_message(app, msg)
       term_event = terminal.next() => handle_terminal_event(app, event)
     }
```

`dispatch_client_message` handles:
- `ClientMessage::Hello` → update app.clients, show status
- `ClientMessage::Log` → construct LogEntry, call `app.add_entry()`
- `ClientMessage::Net` → call `app.network_store.process_message()`

**`src/cli.rs`** — Simplified

```rust
#[derive(Parser)]
struct Cli {
    /// Server port (default: 9753)
    #[arg(long, default_value = "9753")]
    port: u16,

    /// Initial minimum log level
    #[arg(long)]
    level: Option<String>,

    /// Initial tag filter
    #[arg(long)]
    tag: Option<String>,
}
```

Remove: `--uri`, `--adb`, `-s/--device`, `--stdin`, `InputMode` enum.

### Modified Files

**`src/app.rs`**

Remove:
- `last_source_type: Option<LastSourceType>`
- `source_command_tx: Option<UnboundedSender<SourceCommand>>`
- `is_vm_service_connected()` method
- `LastSourceType` enum
- `SourceCommand` enum (currently in main.rs but used by app)

Add:
- `clients: Vec<ClientInfo>` — connected flog_dart clients
- `server_tx: Option<ServerHandle>` — handle to send downstream messages (mock_sync, replay)

Modify:
- `mock_sync_tx` → replaced by `server_tx.broadcast_mock_sync()`
- `replay_tx` → replaced by `server_tx.send_replay()`
- `connected` → derived from `!clients.is_empty()`
- `source_name` → derived from first client's device/app name

**`src/event.rs`**

Modify `trigger_mock_sync()`:
```rust
fn trigger_mock_sync(app: &App) {
    if let Some(ref server) = app.server_tx {
        let json = app.mock_rules.to_json_string();
        server.broadcast_mock_sync(json);
    }
}
```

Modify `replay_selected()`:
```rust
fn replay_selected(app: &mut App) {
    // ... get entry ...
    if let Some(ref server) = app.server_tx {
        server.send_replay(entry_json);
    }
}
```

Remove `is_vm_service_connected()` checks — mock/replay available whenever any client is connected. Use `!app.clients.is_empty()` instead.

**`src/domain/entry.rs`**

Simplify `InputSource`:
```rust
pub enum InputSource {
    DirectSocket,
}
```

Remove `from_vm_service_level()`. Log levels now come as strings from the protocol, converted via a simple match.

**`src/ui/source_select.rs`**

Rewrite from "choose connection mode" to "show connected devices":
- Display list of connected clients (device name, app, OS, connected duration)
- If no clients: show "Waiting for connection on port {port}..."
- No interactive selection needed (clients connect automatically)

**`src/ui/network/mod.rs`**

Status bar: change source info display from "WS → host" / "ADB → device" to show client device name from `app.clients`.

### Deleted Files

| File | Lines | Reason |
|------|-------|--------|
| `src/input/discover.rs` | 172 | VM Service discovery |
| `src/input/vm_service.rs` | 305 | VM Service WebSocket client |
| `src/input/adb.rs` | 120 | ADB logcat source |
| `src/input/stdin_source.rs` | 40 | stdin pipe source |

### Unchanged

- All UI rendering (logs/, network/, json_viewer, help, tab_bar, text_editor)
- Domain storage (LogStore, NetworkStore, MockRuleStore)
- Domain filtering (FilterState, NetworkFilter)
- Domain types (NetworkEntry, LogEntry structure, FlogNetMessage)
- Session persistence (session.rs)
- Parser chain (retained in codebase but not in main path — Log messages arrive pre-structured)

## Edge Cases

1. **No clients connected:** flog shows "Waiting for connection on port 9753..." in source area. All UI functional but empty. Mock/Replay buttons hidden.

2. **Client sends malformed JSON:** Log warning to flog's own status bar, don't crash. Skip the message.

3. **Client sends unknown message type:** Ignore silently.

4. **Client disconnects mid-stream:** Remove from clients list, update UI. Network entries from that client remain visible.

5. **Multiple clients:** Each client's data goes into the same LogStore/NetworkStore (like current behavior with one source). Client name shown in source info. Mock rules broadcast to all clients.

6. **Port already in use:** Show clear error on startup: "Port 9753 is already in use. Try --port 9754". Exit cleanly.

7. **Very large messages (big JSON body):** WebSocket handles framing. No special handling needed beyond existing NetworkStore size limits.

8. **Hot reload in Flutter app:** Client WebSocket may disconnect briefly. flog_dart reconnects automatically (3-second poll). flog shows brief disconnection in status, then reconnects.

## Dependency Changes (Cargo.toml)

- **Keep:** `tokio-tungstenite` (reuse for WS server instead of WS client)
- **Keep:** `tokio`, `serde_json`, `ratatui`, `crossterm`, `reqwest`, `regex`, `clap`
- **Remove:** Nothing from Cargo.toml (tokio-tungstenite switches from client to server use)

## Files Summary

| Action | Count | Files |
|--------|-------|-------|
| Create | 3 | input/protocol.rs, input/server.rs, tests/ws_server_test.rs |
| Rewrite | 3 | input/mod.rs, main.rs, cli.rs |
| Modify | 5 | app.rs, event.rs, domain/entry.rs, ui/source_select.rs, ui/network/mod.rs |
| Delete | 4 | input/discover.rs, input/vm_service.rs, input/adb.rs, input/stdin_source.rs |
| Update | 2 | README.md, CLAUDE.md |
| Total | 17 | |
