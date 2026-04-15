# SP2: Dart FlogClient Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace flog_dart's print/developer.log emission with a WebSocket client (`FlogClient`) that connects to flog TUI's server, and receive mock/replay commands through the same socket. Add timestamps to all messages.

**Architecture:** New singleton `FlogClient` manages a WebSocket connection to `ws://localhost:9753` with 3-second reconnect polling. `emitNet()` and `FlogLogger._log()` route through FlogClient.send() instead of print/developer.log. VM Service extension removed from FlogDio. Rust side adds `ts` field to FlogNetMessage.

**Tech Stack:** Dart, web_socket_channel, dio

---

## Task Sequence

| Task | Description | Files |
|------|-------------|-------|
| 1 | Rust: add `ts` field to FlogNetMessage | Rust domain/network.rs |
| 2 | Dart: create FlogClient | Dart lib/src/flog_client.dart |
| 3 | Dart: modify emitNet to use FlogClient | Dart lib/src/flog_net.dart |
| 4 | Dart: modify FlogLogger to use FlogClient | Dart lib/flog_logger.dart |
| 5 | Dart: modify FlogDio (remove VM ext, start FlogClient) | Dart lib/src/flog_dio.dart |
| 6 | End-to-end test | Manual test with Flutter app |

---

### Task 1: Rust — add `ts` field to FlogNetMessage

**Files:**
- Modify: `src/domain/network.rs:194-213`

- [ ] **Step 1: Add ts field to FlogNetMessage**

In `src/domain/network.rs`, add after the `mocked` field (line 212):

```rust
    pub ts: Option<u64>,
```

- [ ] **Step 2: Use ts for entry timestamps**

In `src/domain/network_store.rs`, in `handle_req()`, after creating the entry, set the timestamp from `ts` if present. Find the `handle_req` method and add after entry creation:

```rust
        if let Some(ts) = msg.ts {
            entry.timestamp = format_ts(ts);
        }
```

Add this helper function at the bottom of network_store.rs (before tests):

```rust
fn format_ts(millis: u64) -> String {
    let secs = millis / 1000;
    let ms = millis % 1000;
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}.{:03}", h, m, s, ms)
}
```

- [ ] **Step 3: Build and test**

Run: `cargo build && cargo test`
Expected: Compiles, all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/domain/network.rs src/domain/network_store.rs
git commit -m "feat(protocol): add ts timestamp field to FlogNetMessage"
```

---

### Task 2: Dart — create FlogClient

**Files:**
- Create: `flog_logger/lib/src/flog_client.dart`

- [ ] **Step 1: Create flog_client.dart**

Create `flog_logger/lib/src/flog_client.dart`:

```dart
/// WebSocket client that connects to flog TUI's server.
library;

import 'dart:async';
import 'dart:convert';
import 'dart:io' show Platform;

import 'package:dio/dio.dart';
import 'package:web_socket_channel/io.dart';
import 'package:web_socket_channel/web_socket_channel.dart';

import 'flog_mock_interceptor.dart';
import 'flog_net.dart' show flogEnabled;

/// Singleton WebSocket client for communicating with flog TUI.
///
/// Connects to `ws://{host}:{port}` and:
/// - Sends log and network event messages upstream
/// - Receives mock_sync and replay commands downstream
///
/// Connection lifecycle:
/// 1. [start] is called once (typically from [FlogDio] constructor)
/// 2. Client attempts to connect every 3 seconds until successful
/// 3. On connect, sends a `hello` message with device info
/// 4. On disconnect, automatically reconnects after 3 seconds
class FlogClient {
  /// Singleton instance.
  static final FlogClient instance = FlogClient._();

  FlogClient._();

  WebSocketChannel? _channel;
  StreamSubscription? _subscription;
  bool _connected = false;
  Timer? _reconnectTimer;
  String _host = 'localhost';
  int _port = 9753;
  bool _started = false;
  Dio? _dio;

  /// Whether the client is currently connected to flog TUI.
  bool get connected => _connected;

  /// Start the client connection loop.
  ///
  /// Call once — subsequent calls are no-ops.
  /// Does nothing if [flogEnabled] is false.
  ///
  /// [host] and [port] specify the flog TUI server address.
  /// [dio] is used for executing replay commands received from flog TUI.
  void start({
    String host = 'localhost',
    int port = 9753,
    Dio? dio,
  }) {
    if (!flogEnabled) return;
    if (_started) return;
    _started = true;
    _host = host;
    _port = port;
    _dio = dio;
    _connect();
  }

  /// Send a JSON message to flog TUI.
  ///
  /// Silently drops the message if not connected.
  void send(Map<String, dynamic> data) {
    if (!_connected || _channel == null) return;
    try {
      _channel!.sink.add(jsonEncode(data));
    } catch (_) {
      // Connection may have closed between check and send
    }
  }

  void _connect() {
    if (!flogEnabled) return;
    try {
      final uri = Uri.parse('ws://$_host:$_port');
      _channel = IOWebSocketChannel.connect(uri);
      _setupListeners();
      // Send hello
      final hello = {
        'type': 'hello',
        'device': _deviceName(),
        'app': 'flutter',
        'os': _osName(),
      };
      _channel!.sink.add(jsonEncode(hello));
      _connected = true;
    } catch (_) {
      _connected = false;
      _scheduleReconnect();
    }
  }

  void _setupListeners() {
    _subscription?.cancel();
    _subscription = _channel!.stream.listen(
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

    _dio!.request(
      url,
      data: body,
      options: Options(
        method: method,
        headers: headers,
      ),
    ).catchError((_) {
      // Replay errors are expected — the response/error will be logged by interceptors
    });
  }

  void _onDisconnect() {
    _connected = false;
    _subscription?.cancel();
    _subscription = null;
    try {
      _channel?.sink.close();
    } catch (_) {}
    _channel = null;
    _scheduleReconnect();
  }

  void _scheduleReconnect() {
    _reconnectTimer?.cancel();
    _reconnectTimer = Timer(const Duration(seconds: 3), _connect);
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

- [ ] **Step 2: Verify no syntax errors**

Run: `cd flog_logger && dart analyze lib/src/flog_client.dart`
Expected: No errors (warnings are OK).

- [ ] **Step 3: Commit**

```bash
git add flog_logger/lib/src/flog_client.dart
git commit -m "feat(dart): create FlogClient WebSocket client singleton"
```

---

### Task 3: Dart — modify emitNet to use FlogClient

**Files:**
- Modify: `flog_logger/lib/src/flog_net.dart`

- [ ] **Step 1: Replace flog_net.dart entirely**

Replace `flog_logger/lib/src/flog_net.dart` with:

```dart
/// Internal helper for flog_net protocol.
library;

import 'flog_client.dart';

/// Master kill-switch.  `dart.vm.product` is true in release builds,
/// so flogEnabled becomes false and AOT tree-shaking removes all flog code.
const flogEnabled = bool.fromEnvironment(
  'FLOG_ENABLED',
  defaultValue: !bool.fromEnvironment('dart.vm.product'),
);

int _nextId = 1;

/// Get next unique request ID.
int nextNetId() => _nextId++;

/// Emit a flog_net protocol message via Direct Socket.
void emitNet(Map<String, dynamic> data) {
  if (!flogEnabled) return;
  data['type'] = 'net';
  data['ts'] = DateTime.now().millisecondsSinceEpoch;
  FlogClient.instance.send(data);
}
```

Removed: `dart:convert`, `dart:developer`, `print()`, `developer.log()`.

- [ ] **Step 2: Verify**

Run: `cd flog_logger && dart analyze lib/src/flog_net.dart`

- [ ] **Step 3: Commit**

```bash
git add flog_logger/lib/src/flog_net.dart
git commit -m "feat(dart): emitNet now sends via FlogClient instead of print/developer.log"
```

---

### Task 4: Dart — modify FlogLogger to use FlogClient

**Files:**
- Modify: `flog_logger/lib/flog_logger.dart`

- [ ] **Step 1: Replace flog_logger.dart entirely**

Replace `flog_logger/lib/flog_logger.dart` with:

```dart
/// Lightweight structured logger for Flutter.
///
/// Sends structured log messages to flog TUI via Direct Socket.
///
/// ```dart
/// final log = FlogLogger('Network');
/// log.i('-> GET /api/users');
/// log.e('Connection failed', error: e, stackTrace: st);
/// ```
library flog_dart;

import 'src/flog_client.dart';
import 'src/flog_net.dart' show flogEnabled;

export 'src/flog_net.dart' show nextNetId, emitNet, flogEnabled;
export 'src/flog_client.dart' show FlogClient;
export 'src/flog_http_interceptor.dart';
export 'src/flog_mock_interceptor.dart';
export 'src/flog_sse_parser.dart';
export 'src/flog_web_socket.dart';
export 'src/flog_dio.dart' show FlogDio, FlogHttpConfig, SseResponse;

class FlogLogger {
  /// The tag used to identify the source of log messages.
  final String tag;

  /// Enable printing log messages to Flutter console (for debugging).
  /// Default is false — logs only go to flog TUI via socket.
  static bool printToConsole = false;

  /// Creates a logger with the given [tag].
  const FlogLogger(this.tag);

  // ---------------------------------------------------------------------------
  // Full-word methods
  // ---------------------------------------------------------------------------

  void verbose(String msg) => _log('verbose', msg);

  void debug(String msg, {Object? error, StackTrace? stackTrace}) =>
      _log('debug', msg, error: error, stackTrace: stackTrace);

  void info(String msg) => _log('info', msg);

  void warning(String msg, {Object? error, StackTrace? stackTrace}) =>
      _log('warning', msg, error: error, stackTrace: stackTrace);

  void error(String msg, {Object? error, StackTrace? stackTrace}) =>
      _log('error', msg, error: error, stackTrace: stackTrace);

  // ---------------------------------------------------------------------------
  // Single-letter shorthand
  // ---------------------------------------------------------------------------

  void v(String msg) => verbose(msg);

  void d(String msg, {Object? error, StackTrace? stackTrace}) =>
      debug(msg, error: error, stackTrace: stackTrace);

  void i(String msg) => info(msg);

  void w(String msg, {Object? error, StackTrace? stackTrace}) =>
      warning(msg, error: error, stackTrace: stackTrace);

  void e(String msg, {Object? error, StackTrace? stackTrace}) =>
      _log('error', msg, error: error, stackTrace: stackTrace);

  // ---------------------------------------------------------------------------
  // Internal
  // ---------------------------------------------------------------------------

  void _log(String level, String msg, {Object? error, StackTrace? stackTrace}) {
    if (!flogEnabled) return;
    FlogClient.instance.send({
      'type': 'log',
      'level': level,
      'tag': tag,
      'message': msg,
      'error': error?.toString(),
      'stackTrace': stackTrace?.toString(),
      'timestamp': DateTime.now().millisecondsSinceEpoch,
    });
    if (printToConsole) {
      final upperLevel = level.toUpperCase();
      // ignore: avoid_print
      print('[$upperLevel][$tag] $msg');
      if (error != null) {
        // ignore: avoid_print
        print('[$upperLevel][$tag] Error: $error');
      }
      if (stackTrace != null) {
        // ignore: avoid_print
        print('[$upperLevel][$tag] $stackTrace');
      }
    }
  }
}
```

Key changes:
- `_log()` sends structured JSON via FlogClient instead of print
- Level strings are lowercase (matches SP1 protocol)
- `printToConsole` static flag (default false)
- Exports `FlogClient`

- [ ] **Step 2: Verify**

Run: `cd flog_logger && dart analyze lib/flog_logger.dart`

- [ ] **Step 3: Commit**

```bash
git add flog_logger/lib/flog_logger.dart
git commit -m "feat(dart): FlogLogger sends structured logs via FlogClient"
```

---

### Task 5: Dart — modify FlogDio

**Files:**
- Modify: `flog_logger/lib/src/flog_dio.dart`

- [ ] **Step 1: Update FlogDio constructor**

In `flog_logger/lib/src/flog_dio.dart`:

1. Remove `import 'dart:developer' as developer;` (line 2)
2. Add `import 'flog_client.dart';` 
3. Add `flogHost` and `flogPort` parameters to constructor
4. Replace VM Service extension block with FlogClient.start()

Replace the constructor (lines 91-135) with:

```dart
  FlogDio({
    String? baseUrl,
    FlogHttpConfig? flogConfig,
    BaseOptions? options,
    String flogHost = 'localhost',
    int flogPort = 9753,
  }) : _inner = Dio(options ?? BaseOptions(baseUrl: baseUrl ?? '')) {
    if (baseUrl != null && options == null) {
      _inner.options.baseUrl = baseUrl;
    }

    if (flogEnabled) {
      final config = flogConfig ?? const FlogHttpConfig();

      // Start FlogClient connection to flog TUI
      FlogClient.instance.start(
        host: flogHost,
        port: flogPort,
        dio: _inner,
      );

      // Mock interceptor first — intercepts before real network
      _inner.interceptors.insert(0, FlogMockInterceptor());

      // HTTP logging interceptor second — logs all requests (including mocked ones)
      _inner.interceptors.insert(
        1,
        FlogHttpInterceptor(
          includeRequestHeaders: config.includeRequestHeaders,
          includeResponseHeaders: config.includeResponseHeaders,
          includeRequestBody: config.includeRequestBody,
          includeResponseBody: config.includeResponseBody,
          maxBodySize: config.maxBodySize,
          filter: config.filter,
        ),
      );
    }
  }
```

Also remove `import 'dart:convert';` at line 1 (no longer needed in this file — check if sse() method uses it; it doesn't, jsonDecode is not used).

Wait — `SseResponse` and `sse()` method don't use jsonDecode. But check the imports. Actually `import 'dart:convert'` may be used elsewhere. Let me check... No, the only place `dart:convert` was used was the VM Service extension handler. Remove it.

- [ ] **Step 2: Verify the full file compiles**

Run: `cd flog_logger && dart analyze lib/src/flog_dio.dart`

- [ ] **Step 3: Run full package analysis**

Run: `cd flog_logger && dart analyze`
Expected: No errors. Warnings about unused imports are OK to fix.

- [ ] **Step 4: Commit**

```bash
git add flog_logger/lib/src/flog_dio.dart
git commit -m "feat(dart): FlogDio starts FlogClient, removes VM Service extension"
```

---

### Task 6: Version bump and final verification

**Files:**
- Modify: `flog_logger/pubspec.yaml`

- [ ] **Step 1: Bump version**

In `flog_logger/pubspec.yaml`, change:
```yaml
version: 0.3.0
```
To:
```yaml
version: 0.4.0
```

- [ ] **Step 2: Full analysis**

Run: `cd flog_logger && dart analyze`
Expected: No errors.

- [ ] **Step 3: Rust build verification**

Run: `cargo build && cargo test`
Expected: All pass.

- [ ] **Step 4: Install flog**

Run: `cargo install --path .`

- [ ] **Step 5: Commit all**

```bash
git add flog_logger/pubspec.yaml
git commit -m "feat(dart): bump flog_dart to v0.4.0 — Direct Socket architecture"
```
