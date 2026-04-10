import 'dart:async';

import 'package:web_socket_channel/web_socket_channel.dart';

import 'flog_net.dart';

/// A WebSocket wrapper that emits flog_net protocol messages for all
/// WebSocket traffic (open, send, receive, close).
///
/// ```dart
/// final ws = FlogWebSocket(Uri.parse('wss://example.com/ws'));
/// ws.stream.listen((message) => print(message));
/// ws.send('hello');
/// await ws.close();
/// ```
class FlogWebSocket {
  /// The underlying [WebSocketChannel].
  final WebSocketChannel _channel;

  /// The flog_net request ID for this connection.
  final int _id;

  /// When the connection was created.
  final DateTime _start;

  /// Broadcast stream of incoming messages with flog_net instrumentation.
  late final Stream<dynamic> stream;

  /// Creates a [FlogWebSocket] that connects to [uri].
  ///
  /// Optional [protocols] are forwarded to [WebSocketChannel.connect].
  FlogWebSocket(Uri uri, {Iterable<String>? protocols})
      : _channel = WebSocketChannel.connect(uri, protocols: protocols),
        _id = nextNetId(),
        _start = DateTime.now() {
    emitNet({
      'id': _id,
      't': 'open',
      'p': 'ws',
      'url': uri.toString(),
    });

    stream = _channel.stream.map((message) {
      final display = _formatMessage(message);
      final size = _messageSize(message);

      emitNet({
        'id': _id,
        't': 'recv',
        'p': 'ws',
        'data': display,
        'size': size,
      });

      return message;
    }).handleError((Object error) {
      emitNet({
        'id': _id,
        't': 'err',
        'p': 'ws',
        'error': error.toString(),
      });
      // Re-throw so downstream listeners see the error
      throw error;
    });
  }

  /// Creates a [FlogWebSocket] from an existing [WebSocketChannel].
  ///
  /// Use this when you already have a connected channel (e.g. from a server
  /// upgrade). The [url] parameter is used for logging only.
  FlogWebSocket.fromChannel(this._channel, {required String url})
      : _id = nextNetId(),
        _start = DateTime.now() {
    emitNet({
      'id': _id,
      't': 'open',
      'p': 'ws',
      'url': url,
    });

    stream = _channel.stream.map((message) {
      final display = _formatMessage(message);
      final size = _messageSize(message);

      emitNet({
        'id': _id,
        't': 'recv',
        'p': 'ws',
        'data': display,
        'size': size,
      });

      return message;
    }).handleError((Object error) {
      emitNet({
        'id': _id,
        't': 'err',
        'p': 'ws',
        'error': error.toString(),
      });
      throw error;
    });
  }

  /// Send a message through the WebSocket.
  void send(dynamic message) {
    final display = _formatMessage(message);
    final size = _messageSize(message);

    emitNet({
      'id': _id,
      't': 'send',
      'p': 'ws',
      'data': display,
      'size': size,
    });

    _channel.sink.add(message);
  }

  /// Close the WebSocket connection.
  ///
  /// Optional [closeCode] and [closeReason] are forwarded to the underlying
  /// channel.
  Future<void> close([int? closeCode, String? closeReason]) async {
    final duration = DateTime.now().difference(_start).inMilliseconds;

    final data = <String, dynamic>{
      'id': _id,
      't': 'close',
      'p': 'ws',
      'duration': duration,
    };

    if (closeCode != null) {
      data['code'] = closeCode;
    }

    if (closeReason != null) {
      data['reason'] = closeReason;
    }

    emitNet(data);

    await _channel.sink.close(closeCode, closeReason);
  }

  /// The underlying sink, for advanced usage.
  WebSocketSink get sink => _channel.sink;

  /// Format a message for display in logs.
  static String _formatMessage(dynamic message) {
    if (message is String) {
      return message;
    } else if (message is List<int>) {
      return '<binary: ${message.length} bytes>';
    } else {
      return message.toString();
    }
  }

  /// Compute the size of a message in bytes.
  static int _messageSize(dynamic message) {
    if (message is String) {
      return message.length;
    } else if (message is List<int>) {
      return message.length;
    } else {
      return message.toString().length;
    }
  }
}
