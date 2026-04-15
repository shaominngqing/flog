# SP2: Dart FlogClient — Replace emitNet with Direct Socket

## Goal

Replace flog_dart's print/developer.log emission with a WebSocket client that connects to flog TUI's server. All data (logs, network events) flows through the socket. Mock rules and replay commands are received through the same socket. VM Service extension is removed.

## Context

SP1 (done) built the Rust WS server on port 9753. SP2 builds the Dart client that connects to it. After SP2, flog_dart and flog TUI communicate entirely through a single WebSocket — no more print(), developer.log(), or VM Service extensions.

## New File

### `lib/src/flog_client.dart`

Singleton WebSocket client that manages the connection to flog TUI.

```dart
class FlogClient {
  static final FlogClient instance = FlogClient._();
  FlogClient._();

  WebSocketChannel? _channel;
  bool _connected = false;
  Timer? _reconnectTimer;
  String _host = 'localhost';
  int _port = 9753;
  bool _started = false;

  /// Start the client. Call once (from FlogDio constructor or manually).
  /// Does nothing if flogEnabled is false.
  void start({String host = 'localhost', int port = 9753});

  /// Send a message to flog TUI. Silently drops if not connected.
  void send(Map<String, dynamic> data);

  /// Internal: attempt WebSocket connection.
  void _connect();

  /// Internal: handle incoming server messages (mock_sync, replay).
  void _onMessage(String json);

  /// Internal: schedule reconnect after 3 seconds.
  void _scheduleReconnect();
}
```

**Connection lifecycle:**
1. `start()` called → begins connection loop
2. Connects to `ws://{host}:{port}`
3. On connect → sends `hello` message with device info, sets `_connected = true`
4. Listens for downstream messages (mock_sync, replay)
5. On disconnect → sets `_connected = false`, schedules reconnect in 3 seconds
6. `send()` checks `_connected` — if false, silently drops the message

**Hello message:**
```json
{
  "type": "hello",
  "device": "iPhone 15 Pro",  // from Platform or dart:io
  "app": "com.example.app",   // package name if available, else "flutter"
  "os": "ios"                 // "android" | "ios" | "macos" | "windows" | "linux" | "web"
}
```

**Downstream message handling:**
- `mock_sync` → `FlogMockInterceptor.updateRules(rules)`
- `replay` → execute HTTP request via a stored Dio reference

**Replay handling:**
FlogClient needs a reference to Dio to execute replay requests. FlogDio passes itself during start:
```dart
FlogClient.instance.start(dio: this);
```
When replay arrives, FlogClient calls `dio.request(url, options: ...)` and the interceptors handle logging the response automatically.

## Modified Files

### `lib/src/flog_net.dart`

Replace emitNet implementation:

```dart
void emitNet(Map<String, dynamic> data) {
  if (!flogEnabled) return;
  data['type'] = 'net';
  data['ts'] = DateTime.now().millisecondsSinceEpoch;
  FlogClient.instance.send(data);
}
```

Remove: `print('[INFO][$_tag] $json')` and `developer.log(json, name: _tag)`.
Remove: `import 'dart:developer'`.

`nextNetId()` stays unchanged.

### `lib/flog_logger.dart` (FlogLogger class)

Replace `_log()`:

```dart
void _log(String level, String msg, {Object? error, StackTrace? stackTrace}) {
  if (!flogEnabled) return;
  FlogClient.instance.send({
    'type': 'log',
    'level': level.toLowerCase(),
    'tag': tag,
    'message': msg,
    'error': error?.toString(),
    'stackTrace': stackTrace?.toString(),
    'timestamp': DateTime.now().millisecondsSinceEpoch,
  });
  if (_printToConsole) {
    // ignore: avoid_print
    print('[$level][$tag] $msg');
  }
}
```

Add static config:
```dart
static bool _printToConsole = false;

/// Enable printing to Flutter console (for debugging flog_dart itself).
static set printToConsole(bool value) => _printToConsole = value;
```

### `lib/src/flog_dio.dart`

Remove VM Service extension registration (the entire `developer.registerExtension('ext.flog.syncMockRules', ...)` block).

Remove `import 'dart:developer'`.

Add FlogClient start in constructor:
```dart
if (flogEnabled) {
  FlogClient.instance.start(dio: _inner, host: host, port: port);
  // ... insert interceptors ...
}
```

Add optional host/port parameters to FlogDio constructor:
```dart
FlogDio({
  String? baseUrl,
  FlogHttpConfig? flogConfig,
  BaseOptions? options,
  String flogHost = 'localhost',
  int flogPort = 9753,
})
```

### `lib/src/flog_http_interceptor.dart`

No changes. It calls `emitNet()` which is modified internally.

### `lib/src/flog_sse_parser.dart`

No changes. It calls `emitNet()` which is modified internally.

### `lib/src/flog_web_socket.dart`

No changes. It calls `emitNet()` which is modified internally.

### `lib/src/flog_mock_interceptor.dart`

No changes. `updateRules()` is called by FlogClient instead of VM Service extension handler. Same interface.

### `pubspec.yaml`

No new dependencies needed. `web_socket_channel` is already a dependency.

## Timestamps

All messages now carry timestamps from Dart side:

- **Log messages**: `timestamp` field (milliseconds since epoch)
- **Net messages**: `ts` field added to every emitNet call (milliseconds since epoch)

Rust side (`dispatch_client_message` in main.rs) already handles the `timestamp` field for Log. For Net, `FlogNetMessage` in `src/domain/network.rs` needs a new optional `ts` field, and network_store should use it when creating entries.

## Edge Cases

1. **flog TUI not running**: FlogClient connects every 3 seconds, fails silently. All `send()` calls are silently dropped. Zero impact on the Flutter app's functionality.

2. **flog TUI restarts**: FlogClient detects disconnect, reconnects within 3 seconds, sends new `hello`. flog TUI sees it as a new client.

3. **Hot reload**: FlogClient is a singleton with static field. Survives hot reload. Connection persists.

4. **Hot restart**: FlogClient is re-created. New connection established. Previous connection cleaned up by flog TUI (client disconnect event).

5. **Release build**: `flogEnabled = false`. FlogClient.start() returns immediately without connecting. All send() calls return immediately. Tree-shaking removes dead code.

6. **Multiple FlogDio instances**: FlogClient is singleton — `start()` called multiple times is idempotent (only first call connects).

7. **Large messages**: WebSocket handles framing. No special handling needed. Same max body size limits as current emitNet.

8. **Replay with authentication**: Replay uses the same Dio instance (with cookies/interceptors), so auth is preserved.

## Files Summary

| Action | File |
|--------|------|
| Create | `lib/src/flog_client.dart` |
| Modify | `lib/src/flog_net.dart` |
| Modify | `lib/flog_logger.dart` |
| Modify | `lib/src/flog_dio.dart` |
| Modify | `pubspec.yaml` (version bump) |
| Modify (Rust) | `src/domain/network.rs` — add `ts` field to FlogNetMessage |
| Modify (Rust) | `src/domain/network_store.rs` — use ts for entry timestamps |
| No change | `lib/src/flog_http_interceptor.dart` |
| No change | `lib/src/flog_sse_parser.dart` |
| No change | `lib/src/flog_web_socket.dart` |
| No change | `lib/src/flog_mock_interceptor.dart` |
