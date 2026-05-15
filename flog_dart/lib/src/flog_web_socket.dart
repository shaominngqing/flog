import 'dart:async';

import 'package:web_socket_channel/web_socket_channel.dart';

import 'flog_net.dart' show flogEnabled, nextNetId, emitNet;

/// A WebSocket wrapper that emits flog_net protocol messages for all
/// WebSocket traffic (open, send, receive, close).
///
/// ```dart
/// final ws = await FlogWebSocket.connect(Uri.parse('wss://example.com/ws'));
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

  /// Creates a [FlogWebSocket] from an existing [WebSocketChannel].
  ///
  /// Use this when you already have a connected channel (e.g. from a server
  /// upgrade). The [url] parameter is used for logging only.
  FlogWebSocket.fromChannel(this._channel, {required String url})
      : _id = nextNetId(),
        _start = DateTime.now() {
    _initFromChannel(url);
  }

  /// Establishes a WebSocket connection and registers it with the flog network
  /// panel.
  ///
  /// On success, emits an `open` frame and returns the wrapped socket.
  /// On failure, emits an `err` frame with [uri], the error message, and the
  /// elapsed duration, then rethrows the original exception unchanged.
  static Future<FlogWebSocket> connect(
    Uri uri, {
    Iterable<String>? protocols,
  }) {
    return _connectAndWrap(
      () async => WebSocketChannel.connect(uri, protocols: protocols),
      url: uri.toString(),
    );
  }

  /// Wraps any WebSocket connection factory so that flog can observe the
  /// handshake phase.
  ///
  /// [connect] is an async factory that must return an already-established
  /// [WebSocketChannel]. Use this when you build the channel yourself (e.g.
  /// `dart:io WebSocket.connect` with custom headers):
  ///
  /// ```dart
  /// final ws = await FlogWebSocket.wrap(
  ///   () async {
  ///     final socket = await WebSocket.connect(url, headers: {...});
  ///     return IOWebSocketChannel(socket);
  ///   },
  ///   url: url,
  /// );
  /// ```
  ///
  /// On success, emits an `open` frame and returns the wrapped socket.
  /// On failure, emits an `err` frame and rethrows the original exception.
  static Future<FlogWebSocket> wrap(
    Future<WebSocketChannel> Function() connect, {
    required String url,
  }) {
    return _connectAndWrap(connect, url: url);
  }

  /// Shared implementation for [connect] and [wrap].
  ///
  /// Calls [connect] to obtain a [WebSocketChannel], then awaits
  /// [WebSocketChannel.ready] to surface handshake errors. Emits an `open`
  /// frame on success and an `err` frame (with duration) on failure, then
  /// rethrows.
  static Future<FlogWebSocket> _connectAndWrap(
    Future<WebSocketChannel> Function() connect, {
    required String url,
  }) async {
    final id = nextNetId();
    final start = DateTime.now();

    WebSocketChannel channel;
    try {
      channel = await connect();
      await channel.ready;
    } catch (e) {
      if (flogEnabled) {
        emitNet({
          'id': id,
          't': 'err',
          'p': 'ws',
          'url': url,
          'error': e.toString(),
          'duration': DateTime.now().difference(start).inMilliseconds,
        });
      }
      rethrow;
    }

    final ws = FlogWebSocket._fromConnected(channel, id: id, start: start);
    ws._initFromChannel(url);
    return ws;
  }

  /// Private constructor used by [_connectAndWrap] after a successful
  /// handshake. Does NOT call [_initFromChannel] — the caller does that.
  FlogWebSocket._fromConnected(
    this._channel, {
    required int id,
    required DateTime start,
  })  : _id = id,
        _start = start;

  /// Shared wiring for both constructors: emit the `open` flog_net frame,
  /// then install a broadcast stream so callers who read the dartdoc can
  /// attach multiple listeners without a `Stream has already been listened
  /// to` error.
  void _initFromChannel(String url) {
    if (flogEnabled) {
      emitNet({
        'id': _id,
        't': 'open',
        'p': 'ws',
        'url': url,
      });
    }

    final mapped = _channel.stream.map((message) {
      if (flogEnabled) {
        final display = _formatMessage(message);
        final size = _messageSize(message);

        emitNet({
          'id': _id,
          't': 'recv',
          'p': 'ws',
          'data': display,
          'size': size,
        });
      }

      return message;
    }).handleError((Object error) {
      if (flogEnabled) {
        emitNet({
          'id': _id,
          't': 'err',
          'p': 'ws',
          'error': error.toString(),
        });
      }
      // Re-throw so downstream listeners see the error
      throw error;
    });
    stream = mapped.asBroadcastStream();
  }

  /// Send a message through the WebSocket.
  void send(dynamic message) {
    if (flogEnabled) {
      final display = _formatMessage(message);
      final size = _messageSize(message);

      emitNet({
        'id': _id,
        't': 'send',
        'p': 'ws',
        'data': display,
        'size': size,
      });
    }

    _channel.sink.add(message);
  }

  /// Close the WebSocket connection.
  ///
  /// Optional [closeCode] and [closeReason] are forwarded to the underlying
  /// channel.
  Future<void> close([int? closeCode, String? closeReason]) async {
    if (flogEnabled) {
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
    }

    await _channel.sink.close(closeCode, closeReason);
  }

  /// The underlying sink, for advanced usage.
  WebSocketSink get sink => _channel.sink;

  /// Prefix for the binary-message placeholder string emitted by
  /// [_formatMessage]. The TUI's WS Chat View detects binary frames by
  /// scanning for this marker (`has_binary_content` in
  /// `src/domain/ws_chat.rs`); keep in lockstep with that side.
  /// (DART-019.)
  static const String binaryFormatPrefix = '<binary: ';

  /// Suffix for the binary placeholder (closes the pair started by
  /// [binaryFormatPrefix]).
  static const String binaryFormatSuffix = ' bytes>';

  /// Build the binary placeholder string for a list of [size] bytes.
  static String formatBinaryLabel(int size) =>
      '$binaryFormatPrefix$size$binaryFormatSuffix';

  /// Format a message for display in logs.
  static String _formatMessage(dynamic message) {
    if (message is String) {
      return message;
    } else if (message is List<int>) {
      return formatBinaryLabel(message.length);
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
