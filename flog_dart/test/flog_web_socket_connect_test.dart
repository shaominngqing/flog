/// Tests for [FlogWebSocket.connect] and [FlogWebSocket.wrap].
///
/// Covers:
///   - wrap(factory throws) → rethrows original exception
///   - wrap(channel.ready throws) → rethrows original exception
///   - wrap(factory throws) → emits err frame (t/p/url/error/duration)
///   - connect symbol exists (API-shape assertion)
///   - wrap symbol exists (API-shape assertion)
///   - fromChannel is still present (smoke / back-compat)
///   - connecting frame emitted before open on success
///   - connecting frame emitted before err on failure
library;

import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:web_socket_channel/web_socket_channel.dart';

import 'package:flog_dart/flog_dart.dart';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// All net frames currently in the store.
List<Map<String, dynamic>> _nets() => FlogStore.instance.snapshotForTesting
    .where((m) => m['type'] == 'net')
    .toList(growable: false);

/// A fake [WebSocketChannel] whose [ready] future throws [_error].
///
/// The factory itself succeeds — only the handshake phase fails. This
/// exercises the `await channel.ready` path inside [FlogWebSocket._connectAndWrap].
class _FailingReadyChannel implements WebSocketChannel {
  _FailingReadyChannel(this._error);

  final Object _error;

  @override
  Future<void> get ready => Future.error(_error);

  @override
  Stream<dynamic> get stream => const Stream.empty();

  @override
  WebSocketSink get sink => _NullSink();

  @override
  int? get closeCode => null;

  @override
  String? get closeReason => null;

  @override
  String? get protocol => null;

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

/// A [WebSocketChannel] whose [ready] future completes normally.
class _SucceedingChannel implements WebSocketChannel {
  @override
  Future<void> get ready => Future.value();

  @override
  Stream<dynamic> get stream => const Stream.empty();

  @override
  WebSocketSink get sink => _NullSink();

  @override
  String? get protocol => null;

  @override
  int? get closeCode => null;

  @override
  String? get closeReason => null;

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

/// A no-op [WebSocketSink] used by [_FailingReadyChannel].
class _NullSink implements WebSocketSink {
  @override
  Future<dynamic> get done => Future.value(null);

  @override
  void add(dynamic data) {}

  @override
  void addError(Object error, [StackTrace? stackTrace]) {}

  @override
  Future<dynamic> addStream(Stream<dynamic> stream) => Future.value(null);

  @override
  Future<dynamic> close([int? closeCode, String? closeReason]) => Future.value(null);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

void main() {
  setUp(() {
    FlogStore.instance.clear();
  });

  tearDownAll(() {
    FlogStore.instance.clear();
  });

  // =========================================================================
  // wrap — factory throws
  // =========================================================================

  group('FlogWebSocket.wrap — factory function throws', () {
    test('rethrows original exception unchanged', () async {
      final original = StateError('factory failed');
      await expectLater(
        FlogWebSocket.wrap(
          () async => throw original,
          url: 'wss://example.com/ws',
        ),
        throwsA(same(original)),
      );
    });

    test('emits an err frame with t=err, p=ws, url, error, duration', () async {
      const testUrl = 'wss://example.com/ws';
      final original = Exception('connect refused');

      try {
        await FlogWebSocket.wrap(
          () async => throw original,
          url: testUrl,
        );
      } catch (_) {
        // expected rethrow — swallow for assertion below
      }

      final errs = _nets()
          .where((m) => m['t'] == 'err' && m['p'] == 'ws')
          .toList();
      expect(errs, hasLength(1),
          reason: 'exactly one err frame must be emitted on factory failure');

      final frame = errs.first;
      expect(frame['t'], 'err');
      expect(frame['p'], 'ws');
      expect(frame['url'], testUrl);
      expect(frame['error'], contains('connect refused'),
          reason: 'error field must carry the exception message');
      expect(frame['duration'], isA<int>(),
          reason: 'duration must be an integer millisecond count');
    });
  });

  // =========================================================================
  // wrap — channel.ready throws
  // =========================================================================

  group('FlogWebSocket.wrap — channel.ready throws', () {
    test('rethrows original exception unchanged', () async {
      final original = Exception('TLS handshake failed');
      await expectLater(
        FlogWebSocket.wrap(
          () async => _FailingReadyChannel(original),
          url: 'wss://tls.example.com/ws',
        ),
        throwsA(same(original)),
      );
    });

    test('emits an err frame on channel.ready failure', () async {
      const testUrl = 'wss://tls.example.com/ws';
      final original = Exception('TLS handshake failed');

      try {
        await FlogWebSocket.wrap(
          () async => _FailingReadyChannel(original),
          url: testUrl,
        );
      } catch (_) {}

      final errs = _nets()
          .where((m) => m['t'] == 'err' && m['p'] == 'ws')
          .toList();
      expect(errs, hasLength(1));
      expect(errs.first['url'], testUrl);
      expect(errs.first['error'], contains('TLS handshake failed'));
      expect(errs.first['duration'], isA<int>());
    });
  });

  // =========================================================================
  // connecting frame ordering
  // =========================================================================

  group('FlogWebSocket.wrap — connecting frame ordering', () {
    test('emits connecting frame before open on success', () async {
      await FlogWebSocket.wrap(
        () async => _SucceedingChannel(),
        url: 'wss://success.example.com/ws',
      );

      final frames = _nets().where((f) => f['p'] == 'ws').toList();

      final connectingIdx = frames.indexWhere((f) => f['t'] == 'connecting');
      final openIdx = frames.indexWhere((f) => f['t'] == 'open');

      expect(connectingIdx, greaterThanOrEqualTo(0),
          reason: 'connecting frame must exist');
      expect(openIdx, greaterThanOrEqualTo(0),
          reason: 'open frame must exist');
      expect(connectingIdx, lessThan(openIdx),
          reason: 'connecting must come before open');
    });

    test('emits connecting frame before err on failure', () async {
      final err = Exception('refused');

      await expectLater(
        FlogWebSocket.wrap(() async => throw err, url: 'wss://fail.example.com/ws'),
        throwsA(same(err)),
      );

      final frames = _nets().where((f) => f['p'] == 'ws').toList();

      final connectingIdx = frames.indexWhere((f) => f['t'] == 'connecting');
      final errIdx = frames.indexWhere((f) => f['t'] == 'err');

      expect(connectingIdx, greaterThanOrEqualTo(0),
          reason: 'connecting frame must exist');
      expect(errIdx, greaterThanOrEqualTo(0),
          reason: 'err frame must exist');
      expect(connectingIdx, lessThan(errIdx),
          reason: 'connecting must come before err');
    });
  });

  // =========================================================================
  // API-shape assertions
  // =========================================================================

  group('API-shape: symbols exist and are callable', () {
    test('FlogWebSocket.connect is a static method (Future<FlogWebSocket>)',
        () {
      // connect is a static method — verify the symbol exists and has the
      // expected static type by checking that tearoffing it produces a
      // Function value.  We cannot easily call it without a real server,
      // but the static shape is what matters here.
      final fn = FlogWebSocket.connect;
      expect(fn, isA<Function>());
    });

    test('FlogWebSocket.wrap is a static method (Future<FlogWebSocket>)', () {
      final fn = FlogWebSocket.wrap;
      expect(fn, isA<Function>());
    });

    test('FlogWebSocket.fromChannel still exists — back-compat smoke test',
        () {
      // Use the existing _FailingReadyChannel as a cheap stand-in channel;
      // we only need the constructor to succeed, not the stream to work.
      final channel = _FailingReadyChannel(StateError('never'));
      // If fromChannel is removed, this line will not compile.
      final ws = FlogWebSocket.fromChannel(channel, url: 'wss://smoke.test/');
      // Basic sanity: the returned object exposes a stream.
      expect(ws.stream, isNotNull);
    });
  });
}
