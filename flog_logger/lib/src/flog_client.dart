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

    _dio!
        .request(
          url,
          data: body,
          options: Options(
            method: method,
            headers: headers,
          ),
        )
        .ignore();
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
