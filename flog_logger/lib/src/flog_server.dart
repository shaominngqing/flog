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
class FlogServer {
  static final FlogServer instance = FlogServer._();
  FlogServer._();

  HttpServer? _httpServer;
  WebSocket? _ws;
  bool _connected = false;
  bool _started = false;
  Dio? _dio;
  int _port = 9753;
  String _appName = 'flutter';
  String _appVersion = '';

  bool get connected => _connected;

  void start({int port = 9753, Dio? dio, String appName = 'flutter', String appVersion = ''}) {
    if (!flogEnabled) return;
    if (_started) return;
    _started = true;
    _port = port;
    _dio = dio;
    _startServer();
  }

  void send(Map<String, dynamic> data) {
    if (!_connected || _ws == null) return;
    try {
      _ws!.add(jsonEncode(data));
    } catch (_) {}
  }

  Future<void> _startServer() async {
    try {
      _httpServer = await HttpServer.bind('0.0.0.0', _port);
      _httpServer!.listen(_handleRequest);
    } catch (_) {
      // Port in use — flog features unavailable but app runs fine
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
    ws.add(jsonEncode({
      'type': 'hello',
      'device': _deviceName(),
      'app': 'flutter',
      'os': _osName(),
    }));

    ws.listen(
      (message) {
        if (message is String) _onMessage(message);
      },
      onError: (_) => _onDisconnect(),
      onDone: () => _onDisconnect(),
    );
  }

  void _onMessage(String json) {
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
      }
    } catch (_) {}
  }

  void _handleReplay(Map<String, dynamic> data) {
    if (_dio == null) return;
    final method = data['method'] as String? ?? 'GET';
    final url = data['url'] as String?;
    if (url == null) return;

    Map<String, dynamic>? headers;
    final headersJson = data['headers'] as String?;
    if (headersJson != null) {
      try { headers = jsonDecode(headersJson) as Map<String, dynamic>; } catch (_) {}
    }

    _dio!.request(url, data: data['body'], options: Options(method: method, headers: headers)).ignore();
  }

  void _onDisconnect() {
    _connected = false;
    _ws = null;
  }

  String _deviceName() {
    try {
      // On iOS simulator, localHostname returns Mac's name.
      // Use operatingSystem + version for a more useful name.
      final os = Platform.operatingSystem;
      final version = Platform.operatingSystemVersion;
      if (os == 'ios') {
        return 'iOS $version';
      } else if (os == 'android') {
        return 'Android $version';
      } else if (os == 'macos') {
        return 'macOS';
      }
      return Platform.localHostname;
    } catch (_) {
      return 'flutter';
    }
  }

  String _osName() {
    try { return Platform.operatingSystem; } catch (_) { return 'unknown'; }
  }
}
