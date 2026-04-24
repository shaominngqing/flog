/// Characterization tests for `lib/src/flog_web_socket.dart`.
///
/// Audit entries locked by this file:
///   - DART-006 (B, FIXED Phase 3 Step 3.4): `stream` is now a proper
///     broadcast stream via asBroadcastStream(); multiple listeners work.
///   - DART-018 (D): FlogWebSocket() and FlogWebSocket.fromChannel()
///     duplicate setup. Both constructors emit identical `open` nets and
///     produce equivalent state.
///   - DART-019 (D): `<binary: N bytes>` magic string format. Locked so
///     the TUI's WS Chat View marker detection remains stable until
///     Phase 3 structures the payload.
library;

import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:web_socket_channel/web_socket_channel.dart';

import 'package:flog_dart/flog_dart.dart';

/// Fake WebSocketChannel backed by a pair of StreamControllers. Lets us
/// drive the 'incoming' stream directly and assert on outgoing 'sink'
/// writes without touching a real socket.
class _FakeChannel implements WebSocketChannel {
  final StreamController<dynamic> _incoming = StreamController<dynamic>();
  final List<dynamic> _outgoing = [];
  final Completer<void> _closed = Completer<void>();
  int? _closeCode;
  String? _closeReason;
  late final _FakeSink _sink = _FakeSink(this);

  void push(dynamic message) => _incoming.add(message);
  void pushError(Object error) => _incoming.addError(error);
  Future<void> closeIncoming() => _incoming.close();

  List<dynamic> get outgoing => List.unmodifiable(_outgoing);

  @override
  int? get closeCode => _closeCode;

  @override
  String? get closeReason => _closeReason;

  @override
  Stream<dynamic> get stream => _incoming.stream;

  @override
  WebSocketSink get sink => _sink;

  @override
  Future<void> get ready => Future.value();

  @override
  String? get protocol => null;

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

class _FakeSink implements WebSocketSink {
  final _FakeChannel _channel;
  _FakeSink(this._channel);

  @override
  void add(dynamic data) {
    _channel._outgoing.add(data);
  }

  @override
  Future<void> close([int? closeCode, String? closeReason]) async {
    _channel._closeCode = closeCode;
    _channel._closeReason = closeReason;
    if (!_channel._closed.isCompleted) _channel._closed.complete();
    await _channel._incoming.close();
  }

  @override
  void addError(Object error, [StackTrace? stackTrace]) {}

  @override
  Future<dynamic> addStream(Stream<dynamic> stream) async {}

  @override
  Future<dynamic> get done => _channel._closed.future;
}

List<Map<String, dynamic>> _nets() => FlogStore.instance.snapshotForTesting
    .where((m) => m['type'] == 'net')
    .toList(growable: false);

void main() {
  setUp(() {
    FlogStore.instance.clear();
  });

  tearDownAll(() {
    FlogStore.instance.clear();
  });

  // ═══════════════════════════════════════════════════════════════
  // DART-018: fromChannel produces equivalent state to primary ctor
  // ═══════════════════════════════════════════════════════════════

  group('DART-018 FlogWebSocket constructors produce equivalent state', () {
    test('fromChannel emits an `open` net record with url field', () {
      final channel = _FakeChannel();
      FlogWebSocket.fromChannel(channel, url: 'wss://example.com/a');

      final opens = _nets().where((r) => r['t'] == 'open').toList();
      expect(opens, hasLength(1));
      expect(opens.first['p'], 'ws');
      expect(opens.first['url'], 'wss://example.com/a');
      expect(opens.first['id'], isA<int>());
    });

    test('fromChannel stream is broadcast — multiple listeners coexist '
        '(DART-006 fixed)', () async {
      final channel = _FakeChannel();
      final ws = FlogWebSocket.fromChannel(channel, url: 'wss://x/y');

      // DART-006 fixed: stream is now exposed via asBroadcastStream(), so
      // callers who read the dartdoc ("Broadcast stream of incoming
      // messages ...") can attach multiple listeners without StateError.
      expect(ws.stream.isBroadcast, isTrue);
      final sub1 = ws.stream.listen((_) {});
      final sub2 = ws.stream.listen((_) {});
      await sub1.cancel();
      await sub2.cancel();
    });

    test('fromChannel forwards messages through stream and emits recv', () async {
      final channel = _FakeChannel();
      final ws = FlogWebSocket.fromChannel(channel, url: 'wss://x/y');

      final received = <dynamic>[];
      final sub = ws.stream.listen(received.add);

      channel.push('hello');
      channel.push('world');
      // Let microtasks settle.
      await Future<void>.delayed(const Duration(milliseconds: 10));

      expect(received, ['hello', 'world']);
      final recvs = _nets().where((r) => r['t'] == 'recv').toList();
      expect(recvs, hasLength(2));
      expect(recvs[0]['data'], 'hello');
      expect(recvs[0]['size'], 5);
      expect(recvs[1]['data'], 'world');
      expect(recvs[1]['size'], 5);

      await sub.cancel();
    });

    test('stream error produces an `err` net record and is re-thrown', () async {
      final channel = _FakeChannel();
      final ws = FlogWebSocket.fromChannel(channel, url: 'wss://x/y');

      Object? caughtError;
      final done = Completer<void>();
      ws.stream.listen(
        (_) {},
        onError: (Object e) {
          caughtError = e;
          done.complete();
        },
        cancelOnError: true,
      );

      channel.pushError(StateError('boom'));
      await done.future;

      expect(caughtError, isA<StateError>());
      final errs = _nets().where((r) => r['t'] == 'err').toList();
      expect(errs, hasLength(1));
      expect(errs.first['p'], 'ws');
      expect(errs.first['error'], contains('boom'));
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // send/close emit correct net records
  // ═══════════════════════════════════════════════════════════════

  group('FlogWebSocket.send / close emissions', () {
    test('send(String) emits t=send with data, size==length', () async {
      final channel = _FakeChannel();
      final ws = FlogWebSocket.fromChannel(channel, url: 'wss://x/y');
      FlogStore.instance.clear();

      ws.send('hi');
      expect(channel.outgoing, ['hi']);
      final sends = _nets().where((r) => r['t'] == 'send').toList();
      expect(sends, hasLength(1));
      expect(sends.first['data'], 'hi');
      expect(sends.first['size'], 2);
    });

    test('send(List<int>) emits the `<binary: N bytes>` magic string '
        '(DART-019)', () async {
      final channel = _FakeChannel();
      final ws = FlogWebSocket.fromChannel(channel, url: 'wss://x/y');
      FlogStore.instance.clear();

      final bytes = <int>[1, 2, 3, 4, 5];
      ws.send(bytes);
      final sends = _nets().where((r) => r['t'] == 'send').toList();
      expect(sends, hasLength(1));
      expect(sends.first['data'], '<binary: 5 bytes>',
          reason: 'DART-019 locks the exact magic string so the TUI marker '
              'detection does not drift.');
      expect(sends.first['size'], 5);
    });

    test('close emits t=close with duration; code/reason present when given',
        () async {
      final channel = _FakeChannel();
      final ws = FlogWebSocket.fromChannel(channel, url: 'wss://x/y');
      // Attach a listener so the incoming controller can settle when closed.
      final sub = ws.stream.listen((_) {}, onError: (_) {});
      FlogStore.instance.clear();

      await ws.close(1000, 'bye').timeout(const Duration(seconds: 2));
      await sub.cancel();
      final closes = _nets().where((r) => r['t'] == 'close').toList();
      expect(closes, hasLength(1));
      expect(closes.first['code'], 1000);
      expect(closes.first['reason'], 'bye');
      expect(closes.first['duration'], isA<int>());
      expect(channel.closeCode, 1000);
      expect(channel.closeReason, 'bye');
    });

    test('close without args emits close without code/reason keys', () async {
      final channel = _FakeChannel();
      final ws = FlogWebSocket.fromChannel(channel, url: 'wss://x/y');
      final sub = ws.stream.listen((_) {}, onError: (_) {});
      FlogStore.instance.clear();

      await ws.close().timeout(const Duration(seconds: 2));
      await sub.cancel();
      final rec =
          _nets().firstWhere((r) => r['t'] == 'close', orElse: () => {});
      expect(rec['p'], 'ws');
      expect(rec.containsKey('code'), isFalse);
      expect(rec.containsKey('reason'), isFalse);
    });

    test('sink getter exposes the underlying channel sink', () {
      final channel = _FakeChannel();
      final ws = FlogWebSocket.fromChannel(channel, url: 'wss://x/y');
      expect(ws.sink, isNotNull);
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // DART-019: binary magic string format edge cases
  // ═══════════════════════════════════════════════════════════════

  group('DART-019 binary format magic string edge cases', () {
    test('empty byte list emits `<binary: 0 bytes>`', () {
      final channel = _FakeChannel();
      final ws = FlogWebSocket.fromChannel(channel, url: 'wss://x/y');
      FlogStore.instance.clear();

      ws.send(<int>[]);
      final rec = _nets().firstWhere((r) => r['t'] == 'send');
      expect(rec['data'], '<binary: 0 bytes>');
      expect(rec['size'], 0);
    });

    test('binary size is the list length, not UTF-8 bytes', () {
      final channel = _FakeChannel();
      final ws = FlogWebSocket.fromChannel(channel, url: 'wss://x/y');
      FlogStore.instance.clear();

      ws.send(List<int>.filled(1024, 0));
      final rec = _nets().firstWhere((r) => r['t'] == 'send');
      expect(rec['data'], '<binary: 1024 bytes>');
      expect(rec['size'], 1024);
    });
  });
}
