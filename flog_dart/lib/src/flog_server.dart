/// WebSocket server that accepts connections from flog TUI.
///
/// Supports multiple simultaneous flog TUI clients. Each new client receives
/// a full replay of buffered messages from [FlogStore], then seamlessly
/// transitions to receiving live messages.
library;

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:ui' show PlatformDispatcher;

import 'package:dio/dio.dart';
import 'package:flutter/foundation.dart';

import 'flog_mock_interceptor.dart';
import 'flog_net.dart' show flogEnabled;
import 'flog_store.dart';

/// Singleton WebSocket server for communicating with flog TUI.
///
/// Listens on `0.0.0.0:{port}` and accepts WebSocket connections from flog.
/// When flog connects, sends a `hello` message, replays all buffered data
/// from [FlogStore], then begins pushing live data.
class FlogServer {
  /// Number of consecutive ports the server will try when the base port is
  /// already in use: `[basePort, basePort + portScanCount)`. Must stay in
  /// sync with the Rust TUI's discovery scan (`src/transport/`), which
  /// currently walks 9753..=9762 (= 10 ports). If you bump this, bump the
  /// TUI side too. (DART-015.)
  static const int portScanCount = 10;

  static final FlogServer instance = FlogServer._();
  FlogServer._();

  HttpServer? _httpServer;
  Future<void>? _startFuture;
  bool _started = false;
  bool _hooksInstalled = false;
  Dio? _dio;
  int _port = 9753;
  String _appName = 'flutter';
  String _appVersion = '';
  String _packageName = '';

  /// All connected flog TUI clients.
  final Set<WebSocket> _clients = {};

  /// Whether at least one flog TUI client is connected.
  bool get connected => _clients.isNotEmpty;

  /// Initialize flog: register system hooks and start the WebSocket server.
  ///
  /// Call this as early as possible in your app (e.g. right after
  /// `WidgetsFlutterBinding.ensureInitialized()`). Safe to call multiple
  /// times — only the first call takes effect.
  ///
  /// App-identity metadata (appName / appVersion / packageName) is set by
  /// [updateAppInfo], which [Flog.init] calls asynchronously via
  /// `PackageInfo.fromPlatform()`. Earlier versions of `start` also took
  /// those three params, but the only production caller (`Flog.init`)
  /// never forwarded them — they were dead code. Removed per DART-022.
  void start({int port = 9753}) {
    if (!flogEnabled) return;
    if (_started) return;
    _started = true;
    _port = port;
    if (!_hooksInstalled) {
      _hooksInstalled = true;
      _installSystemHooks();
    }
    _startFuture = _startServer();
  }

  /// Restart the WebSocket server without reinstalling system log hooks.
  ///
  /// iOS can leave app-owned sockets in a stale state after the app is
  /// suspended and later resumed. Rebinding on resume gives flog a fresh
  /// listener while preserving buffered logs in [FlogStore].
  Future<void> restart({int? port}) async {
    if (!flogEnabled) return;
    final targetPort = port ?? _port;
    final pendingStart = _startFuture;
    if (pendingStart != null) {
      try {
        await pendingStart;
      } catch (_) {}
    }
    for (final ws in _clients.toList()) {
      try {
        await ws.close();
      } catch (_) {}
    }
    _clients.clear();
    try {
      await _httpServer?.close(force: true);
    } catch (_) {}
    _httpServer = null;
    _started = false;
    start(port: targetPort);
  }

  /// Update app info after async detection.
  ///
  /// Called by [flog] after [PackageInfo.fromPlatform] resolves.
  /// The hello message sent to flog TUI on connect uses these values.
  void updateAppInfo({
    required String appName,
    required String appVersion,
    required String packageName,
  }) {
    _appName = appName;
    _appVersion = appVersion;
    _packageName = packageName;
  }

  /// Register a [Dio] instance for network replay.
  ///
  /// Called by [FlogDio] automatically. When the flog TUI triggers a
  /// replay, this Dio instance is used to re-execute the request.
  void registerDio(Dio dio) {
    _dio = dio;
  }

  // ── System log capture ──

  /// Install hooks to capture Flutter framework output, errors, and
  /// unhandled exceptions. Chains with any existing handlers so
  /// user-installed hooks (e.g. Sentry, Crashlytics) keep working.
  void _installSystemHooks() {
    // 1. debugPrint — captures all output as raw text.
    //    Rust TUI is responsible for parsing level/tag from the content.
    final originalDebugPrint = debugPrint;
    debugPrint = (String? message, {int? wrapWidth}) {
      if (message != null) {
        _recordRawLog(message);
      }
      originalDebugPrint(message, wrapWidth: wrapWidth);
    };

    // 2. FlutterError.onError — captures framework exceptions
    //    (build errors, layout errors, paint errors, red screen).
    final originalFlutterErrorHandler = FlutterError.onError;
    FlutterError.onError = (FlutterErrorDetails details) {
      _recordRawLog(details.exceptionAsString());
      if (details.stack != null) {
        _recordRawLog(details.stack.toString());
      }
      // Chain to previous handler (default: dump to console).
      if (originalFlutterErrorHandler != null) {
        originalFlutterErrorHandler(details);
      }
    };

    // 3. PlatformDispatcher.onError — captures unhandled async errors
    //    outside the Flutter framework (top-level Futures, Isolate errors).
    final originalPlatformErrorHandler = PlatformDispatcher.instance.onError;
    PlatformDispatcher.instance.onError = (Object error, StackTrace stack) {
      _recordRawLog(error.toString());
      _recordRawLog(stack.toString());
      // Chain to previous handler. Return true = handled.
      if (originalPlatformErrorHandler != null) {
        return originalPlatformErrorHandler(error, stack);
      }
      return false;
    };
  }

  /// Record raw text into FlogStore. No level/tag wrapping —
  /// the Rust TUI parser will extract structure from the content.
  void _recordRawLog(String message) {
    send({
      'type': 'log',
      'message': message,
      'timestamp': DateTime.now().millisecondsSinceEpoch,
    });
  }

  /// Record a message to [FlogStore] and broadcast to all connected clients.
  void send(Map<String, dynamic> data) {
    if (!flogEnabled) return;

    // Always record, even if no clients are connected.
    FlogStore.instance.record(data);

    if (_clients.isEmpty) return;

    final json = jsonEncode(data);
    final disconnected = <WebSocket>[];
    for (final ws in _clients) {
      try {
        ws.add(json);
      } catch (_) {
        disconnected.add(ws);
      }
    }
    for (final ws in disconnected) {
      _removeClient(ws);
    }
  }

  Future<void> _startServer() async {
    // Try binding to basePort + 0..portScanCount-1. This allows multiple
    // flog-instrumented apps on the same device to coexist.
    final basePort = _port;
    Object? lastError;
    for (int offset = 0; offset < portScanCount; offset++) {
      try {
        final tryPort = basePort + offset;
        _httpServer = await HttpServer.bind('0.0.0.0', tryPort);
        _port = tryPort;
        _httpServer!.listen(_handleRequest);
        return;
      } catch (e) {
        lastError = e;
        // Port in use — try next.
      }
    }
    // DART-016: surface the failure instead of silently succeeding
    // without binding. FlogStore still records, but no TUI can connect
    // until the user frees a port.
    debugPrint(
      'flog_dart: FlogServer failed to bind any port in '
      '[$basePort .. ${basePort + portScanCount - 1}]; '
      'last error: $lastError',
    );
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
    // 1. Send hello with app info
    ws.add(
      jsonEncode({
        'type': 'hello',
        'app': _appName,
        'appVersion': _appVersion,
        'os': _osName(),
        'packageName': _packageName,
        'port': _port,
        'buildMode': _buildMode(),
      }),
    );

    // 2. Replay entire buffer — delivers all historical data.
    //    Dart is single-threaded, so no new messages can be produced during
    //    this synchronous iteration. The transition to live is seamless.
    FlogStore.instance.replayTo(ws);

    // 3. Add to broadcast set — from now on, live messages flow naturally.
    _clients.add(ws);

    // 4. Listen for incoming messages from flog TUI.
    // DART-030 ack: onError currently drops the exception object on the
    // floor. Behavior is correct (a broken WS is removed from the set)
    // but we lose observability on malformed frames or reconnect storms.
    // Low priority — the server keeps running either way; future work
    // may thread the error into FlogLogger debug.
    ws.listen(
      (message) {
        if (message is String) _onMessage(message, ws);
      },
      onError: (_) => _removeClient(ws),
      onDone: () => _removeClient(ws),
    );
  }

  void _onMessage(String json, WebSocket ws) {
    try {
      final data = jsonDecode(json) as Map<String, dynamic>;
      switch (data['type'] as String?) {
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
        case 'subscribe':
          _handleSubscribe(ws);
          break;
      }
    } catch (_) {}
  }

  /// Handle a subscribe request: re-deliver the full buffer to this client.
  ///
  /// This is triggered when the flog TUI switches sessions. The client
  /// clears its local stores and asks us to replay everything.
  ///
  /// DART-031 ack: the remove/add dance around replayTo is a no-op on
  /// Dart's single-isolate event loop, but kept for documentation — if
  /// Dart ever gains true threading, a proper synchronization primitive
  /// (not a remove/add race) would be required. Current behavior is
  /// correct; ack tracked in audit.
  void _handleSubscribe(WebSocket ws) {
    _clients.remove(ws);
    FlogStore.instance.replayTo(ws);
    _clients.add(ws);
  }

  void _handleReplay(Map<String, dynamic> data) {
    if (_dio == null) return;
    final method = data['method'] as String? ?? 'GET';
    final url = data['url'] as String?;
    if (url == null) return;

    Map<String, dynamic>? headers;
    final headersJson = data['headers'] as String?;
    if (headersJson != null) {
      try {
        headers = jsonDecode(headersJson) as Map<String, dynamic>;
      } catch (_) {}
    }

    // DART-017: previously `.ignore()` swallowed every replay error.
    // Surface failures via debugPrint so the developer can see why a
    // replay never shows up in the TUI. Interceptors on the registered
    // Dio still emit their normal `req` / `res` / `err` frames, so the
    // TUI also sees the result inline; this log is a last-resort
    // diagnostic for the fire-and-forget case.
    unawaited(() async {
      try {
        await _dio!.request(
          url,
          data: data['body'],
          options: Options(method: method, headers: headers),
        );
      } catch (e) {
        debugPrint('flog_dart: replay failed for $method $url: $e');
      }
    }());
  }

  void _removeClient(WebSocket ws) {
    _clients.remove(ws);
    try {
      ws.close();
    } catch (_) {}
  }

  String _osName() {
    try {
      return Platform.operatingSystem;
    } catch (_) {
      return 'unknown';
    }
  }

  static String _buildMode() {
    const isProduct = bool.fromEnvironment('dart.vm.product');
    const isProfile = bool.fromEnvironment('dart.vm.profile');
    if (isProduct) return 'release';
    if (isProfile) return 'profile';
    return 'debug';
  }
}
