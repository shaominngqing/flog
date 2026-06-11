import 'dart:async';

import 'package:web_socket_channel/web_socket_channel.dart';

import 'flog_net.dart' show flogEnabled, nextNetId, emitNet;
import 'timing/timing_clock.dart';
import 'timing/timing_trace.dart';

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

  final FlogTimingClock _clock;
  final int _startUs;
  final int _connectUs;
  final int _openUs;
  final List<FlogTimingEvent> _events = <FlogTimingEvent>[];
  int? _lastEventUs;

  /// Broadcast stream of incoming messages with flog_net instrumentation.
  late final Stream<dynamic> stream;

  /// Creates a [FlogWebSocket] from an existing [WebSocketChannel].
  ///
  /// Use this when you already have a connected channel (e.g. from a server
  /// upgrade). The [url] parameter is used for logging only.
  ///
  /// No `connecting` frame is emitted — the handshake is already complete at
  /// the call site. Only an `open` frame is emitted via [_initFromChannel].
  factory FlogWebSocket.fromChannel(
    WebSocketChannel channel, {
    required String url,
    FlogTimingClock? clock,
  }) {
    final timingClock = clock ?? StopwatchTimingClock();
    final nowUs = timingClock.nowUs();
    final ws = FlogWebSocket._fromConnected(
      channel,
      id: nextNetId(),
      start: DateTime.now(),
      clock: timingClock,
      startUs: nowUs,
      connectUs: nowUs,
      openUs: nowUs,
    );
    ws._initFromChannel(url);
    return ws;
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
    FlogTimingClock? clock,
  }) {
    return _connectAndWrap(
      () async => WebSocketChannel.connect(uri, protocols: protocols),
      url: uri.toString(),
      clock: clock,
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
    FlogTimingClock? clock,
  }) {
    return _connectAndWrap(connect, url: url, clock: clock);
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
    FlogTimingClock? clock,
  }) async {
    final id = nextNetId();
    final start = DateTime.now();
    final timingClock = clock ?? StopwatchTimingClock();
    final startUs = timingClock.nowUs();
    int? readyUs;

    if (flogEnabled) {
      emitNet({
        'id': id,
        't': 'connecting',
        'p': 'ws',
        'url': url,
        'timing': FlogTimingTrace(
          source: 'ws_wrapper',
          startUs: startUs,
          phases: const [
            FlogTimingPhase(
              name: 'handshake',
              status: 'active',
              detail: 'websocket connect + ready',
            ),
          ],
          events: const [],
        ).toJson(),
      });
    }

    WebSocketChannel channel;
    try {
      channel = await connect();
      await channel.ready;
      readyUs = timingClock.nowUs();
    } catch (e) {
      final errorUs = timingClock.nowUs();
      if (flogEnabled) {
        emitNet({
          'id': id,
          't': 'err',
          'p': 'ws',
          'url': url,
          'error': e.toString(),
          'duration': DateTime.now().difference(start).inMilliseconds,
          'timing': FlogTimingTrace(
            source: 'ws_wrapper',
            startUs: startUs,
            endUs: errorUs,
            phases: <FlogTimingPhase>[
              FlogTimingPhase(
                name: 'handshake',
                startUs: startUs,
                endUs: errorUs,
                status: 'errored',
                detail: 'websocket connect + ready failed',
              ),
            ],
            events: const [],
          ).toJson(),
        });
      }
      rethrow;
    }

    final openUs = readyUs;
    final ws = FlogWebSocket._fromConnected(
      channel,
      id: id,
      start: start,
      clock: timingClock,
      startUs: startUs,
      connectUs: openUs,
      openUs: openUs,
    );
    ws._initFromChannel(url);
    return ws;
  }

  /// Private constructor used by [_connectAndWrap] after a successful
  /// handshake. Does NOT call [_initFromChannel] — the caller does that.
  FlogWebSocket._fromConnected(
    this._channel, {
    required int id,
    required DateTime start,
    required FlogTimingClock clock,
    required int startUs,
    required int connectUs,
    required int openUs,
  })  : _id = id,
        _start = start,
        _clock = clock,
        _startUs = startUs,
        _connectUs = connectUs,
        _openUs = openUs;

  /// Shared wiring used by every entry point: emit the `open` flog_net frame,
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
        'timing': _trace(
          _clock.nowUs(),
          includeActive: true,
          activeStatus: 'active',
          activeEndUs: null,
        ).toJson(),
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
          'eventTiming': _event('recv', size).toJson(),
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
          'timing': _trace(_clock.nowUs()).toJson(),
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
        'eventTiming': _event('send', size).toJson(),
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
      final nowUs = _clock.nowUs();

      final data = <String, dynamic>{
        'id': _id,
        't': 'close',
        'p': 'ws',
        'duration': duration,
        'timing': _trace(
          nowUs,
          includeActive: true,
          activeStatus: 'complete',
          activeEndUs: nowUs,
        ).toJson(),
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

  FlogTimingEvent _event(String name, int size) {
    final nowUs = _clock.nowUs();
    final event = FlogTimingEvent(
      name: name,
      atUs: nowUs,
      gapUs: _lastEventUs == null ? null : nowUs - _lastEventUs!,
      size: size,
    );
    _lastEventUs = nowUs;
    _events.add(event);
    return event;
  }

  FlogTimingTrace _trace(
    int endUs, {
    bool includeActive = false,
    String? activeStatus,
    int? activeEndUs,
  }) {
    final phases = <FlogTimingPhase>[
      FlogTimingPhase(
        name: 'handshake',
        startUs: _startUs,
        endUs: _connectUs,
        detail: 'websocket connect + ready',
      ),
    ];

    if (includeActive) {
      phases.add(
        FlogTimingPhase(
          name: 'active',
          startUs: _openUs,
          endUs: activeEndUs,
          status: activeStatus ?? 'active',
          detail: 'socket open period',
        ),
      );
    }

    return FlogTimingTrace(
      source: 'ws_wrapper',
      startUs: _startUs,
      endUs: endUs,
      phases: phases,
      events: List<FlogTimingEvent>.unmodifiable(_events),
    );
  }

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
