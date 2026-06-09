/// Characterization tests for `lib/src/flog_http_interceptor.dart`.
///
/// Audit entries locked by this file:
///   - DART-007 (B, FIXED Phase 3 Step 3.4): _truncate now measures UTF-8
///     bytes and slices at a character boundary so CJK payloads honor the
///     byte budget accurately.
///   - DART-008 (B): _idMap/_startMap leak when a downstream interceptor
///     resolves/rejects before FlogHttpInterceptor.onResponse fires. Locks
///     the current leak so Phase 3's Expando/extra-based fix is detectable.
///   - DART-027 (D): Mocked-response path duplicates req-emit logic. Locks
///     the current dual-emit (req + res) behavior when flog_mocked==true.
///
/// Uses FlogStore.snapshotForTesting (a @visibleForTesting getter added to
/// lib/src/flog_store.dart — see commit notes) to read back the emitted
/// records without spinning up a real WebSocket.
library;

import 'package:dio/dio.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:flog_dart/flog_dart.dart';
import 'package:flog_dart/src/timing/timing_adapter.dart';
import 'package:flog_dart/src/timing/timing_trace.dart';

class _ReqHandler implements RequestInterceptorHandler {
  RequestOptions? passed;
  bool next_ = false;
  Response<dynamic>? resolved;
  DioException? rejected;

  @override
  void next(RequestOptions requestOptions) {
    next_ = true;
    passed = requestOptions;
  }

  @override
  void resolve(Response response, [bool callFollowing = false]) {
    resolved = response;
  }

  @override
  void reject(DioException error, [bool callFollowing = false]) {
    rejected = error;
  }

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

class _ResHandler implements ResponseInterceptorHandler {
  Response<dynamic>? passed;
  bool next_ = false;

  @override
  void next(Response response) {
    next_ = true;
    passed = response;
  }

  @override
  void resolve(Response response) {}

  @override
  void reject(DioException error, [bool callFollowing = false]) {}

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

class _ErrHandler implements ErrorInterceptorHandler {
  DioException? passed;
  bool next_ = false;

  @override
  void next(DioException err) {
    next_ = true;
    passed = err;
  }

  @override
  void resolve(Response response) {}

  @override
  void reject(DioException error) {}

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

RequestOptions _opts(String url, {String method = 'GET', Object? data}) {
  final uri = Uri.parse(url);
  return RequestOptions(
    path: uri.path + (uri.hasQuery ? '?${uri.query}' : ''),
    baseUrl: '${uri.scheme}://${uri.authority}',
    method: method,
    data: data,
  );
}

/// Returns only the `net`-typed records stored in FlogStore since last clear().
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
  // DART-007 (FIXED): _truncate honors the UTF-8 byte budget
  // ═══════════════════════════════════════════════════════════════

  group('DART-007 _truncate measures UTF-8 bytes', () {
    test('ASCII body exactly at maxBodySize is NOT truncated', () {
      final interceptor = FlogHttpInterceptor(maxBodySize: 10);
      final body = '0123456789'; // 10 ASCII = 10 bytes
      final opts = _opts('https://example.com/x', method: 'POST', data: body);
      interceptor.onRequest(opts, _ReqHandler());

      final nets = _nets();
      expect(nets, hasLength(1));
      expect(nets.first['body'], body,
          reason: 'Body at exactly maxBodySize should pass through.');
    });

    test('ASCII body exceeding maxBodySize gets `... (truncated)` suffix', () {
      final interceptor = FlogHttpInterceptor(maxBodySize: 5);
      final body = 'abcdefghij'; // 10 bytes > 5
      final opts = _opts('https://example.com/x', method: 'POST', data: body);
      interceptor.onRequest(opts, _ReqHandler());

      expect(_nets().first['body'], 'abcde... (truncated)');
    });

    test('CJK body under UTF-8 byte budget passes through untruncated', () {
      // 4 CJK chars = 12 UTF-8 bytes. With a 16-byte budget the payload
      // fits; the pre-fix (code-unit) measurement would have wrongly
      // kept ASCII-sized budgets but is unaffected here — this locks
      // the byte-accurate contract either way.
      final interceptor = FlogHttpInterceptor(maxBodySize: 16);
      final body = '你好世界';
      final opts = _opts('https://example.com/x', method: 'POST', data: body);
      interceptor.onRequest(opts, _ReqHandler());

      expect(_nets().first['body'], body,
          reason: 'DART-007 fix: 12 bytes <= 16 budget, no truncation.');
    });

    test('CJK body slices on a UTF-8 character boundary', () {
      // 你 = 3 bytes, 好 = 3 bytes. With a 5-byte budget we can keep "你"
      // (3 bytes) but not "好" (would push us to 6). The walk-back code
      // must stop at the boundary so the emitted string is valid UTF-8.
      final interceptor = FlogHttpInterceptor(maxBodySize: 5);
      final body = '你好世界';
      final opts = _opts('https://example.com/x', method: 'POST', data: body);
      interceptor.onRequest(opts, _ReqHandler());

      expect(_nets().first['body'], '你... (truncated)',
          reason: 'DART-007 fix: slice lands on a char boundary; never '
              'half a character.');
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // DART-008: _idMap/_startMap leak when downstream resolves early
  // ═══════════════════════════════════════════════════════════════

  group(
      'DART-008 id/start maps leak when response never reaches this '
      'interceptor', () {
    test('onResponse with untracked options falls through without crash', () {
      final interceptor = FlogHttpInterceptor();
      final opts = _opts('https://example.com/untracked');
      final response = Response<dynamic>(
        requestOptions: opts,
        statusCode: 200,
        data: 'ok',
      );
      final handler = _ResHandler();
      interceptor.onResponse(response, handler);

      // DART-008 current behavior: no id in _idMap → falls through to next()
      // without emitting.
      expect(handler.next_, isTrue);
      expect(_nets(), isEmpty);
    });

    test(
      'onRequest tracked + onResponse consumes the id (no double-emit)',
      () {
        final interceptor = FlogHttpInterceptor();
        final opts = _opts('https://example.com/tracked');
        interceptor.onRequest(opts, _ReqHandler());
        final response = Response<dynamic>(
          requestOptions: opts,
          statusCode: 200,
          data: 'ok',
        );
        interceptor.onResponse(response, _ResHandler());

        // A second onResponse for the same opts now finds no id → no-op.
        FlogStore.instance.clear();
        final handler2 = _ResHandler();
        interceptor.onResponse(response, handler2);
        expect(handler2.next_, isTrue);
        expect(_nets(), isEmpty,
            reason: 'Second onResponse for already-consumed opts must not '
                're-emit; proves the id was removed on first onResponse.');
      },
    );
  });

  // ═══════════════════════════════════════════════════════════════
  // DART-027: mocked-response dual-emit (req + res)
  // ═══════════════════════════════════════════════════════════════

  group('DART-027 mocked response emits both req and res', () {
    test('flog_mocked==true triggers req emit + res emit with mocked:true', () {
      final interceptor = FlogHttpInterceptor();
      final opts = _opts('https://example.com/mocked', method: 'POST');
      opts.extra['flog_mocked'] = true;

      final response = Response<dynamic>(
        requestOptions: opts,
        statusCode: 201,
        data: {'ok': true},
      );
      interceptor.onResponse(response, _ResHandler());

      final nets = _nets();
      expect(nets, hasLength(2),
          reason: 'DART-027: mocked path emits both a `req` and a `res`.');
      expect(nets[0]['t'], 'req');
      expect(nets[0]['method'], 'POST');
      expect(nets[0]['url'], 'https://example.com/mocked');
      expect(nets[1]['t'], 'res');
      expect(nets[1]['status'], 201);
      expect(nets[1]['mocked'], true);
      expect(nets[1]['duration'], 0,
          reason: 'Mocked path hardcodes duration=0.');
      expect(nets[0]['id'], nets[1]['id'],
          reason: 'req and res share the same nextNetId value.');
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // Request emission shape
  // ═══════════════════════════════════════════════════════════════

  group('FlogHttpInterceptor.onRequest emission shape', () {
    test('emits {id, t=req, p=http, method, url}', () {
      final interceptor = FlogHttpInterceptor(includeRequestBody: false);
      final opts = _opts('https://example.com/api/x?q=1', method: 'PATCH');
      interceptor.onRequest(opts, _ReqHandler());

      final rec = _nets().single;
      expect(rec['t'], 'req');
      expect(rec['p'], 'http');
      expect(rec['method'], 'PATCH');
      expect(rec['url'], 'https://example.com/api/x?q=1');
      expect(rec['id'], isA<int>());
    });

    test('includeRequestBody=false omits body field', () {
      final interceptor = FlogHttpInterceptor(includeRequestBody: false);
      final opts =
          _opts('https://example.com/x', method: 'POST', data: 'payload');
      interceptor.onRequest(opts, _ReqHandler());

      expect(_nets().single.containsKey('body'), isFalse);
    });

    test('Map body is jsonEncoded', () {
      final interceptor = FlogHttpInterceptor();
      final opts = _opts('https://example.com/x',
          method: 'POST', data: {'a': 1, 'b': 'two'});
      interceptor.onRequest(opts, _ReqHandler());

      final body = _nets().single['body'] as String;
      expect(body.contains('"a":1'), isTrue);
      expect(body.contains('"b":"two"'), isTrue);
    });

    test('filter=false skips emission entirely', () {
      final interceptor = FlogHttpInterceptor(
        filter: (opts) => false,
      );
      final opts = _opts('https://example.com/skipme');
      final h = _ReqHandler();
      interceptor.onRequest(opts, h);

      expect(h.next_, isTrue);
      expect(_nets(), isEmpty);
    });

    test('filter=true allows emission', () {
      final interceptor = FlogHttpInterceptor(
        filter: (opts) => true,
      );
      final opts = _opts('https://example.com/keep');
      interceptor.onRequest(opts, _ReqHandler());

      expect(_nets(), hasLength(1));
    });

    test('includeRequestHeaders=false omits headers', () {
      final interceptor = FlogHttpInterceptor(includeRequestHeaders: false);
      final opts = _opts('https://example.com/x');
      opts.headers['X-Test'] = 'yes';
      interceptor.onRequest(opts, _ReqHandler());

      expect(_nets().single.containsKey('headers'), isFalse);
    });

    test('includeRequestHeaders=true emits headers map', () {
      final interceptor = FlogHttpInterceptor(includeRequestHeaders: true);
      final opts = _opts('https://example.com/x');
      opts.headers['X-Test'] = 'yes';
      interceptor.onRequest(opts, _ReqHandler());

      expect(_nets().single['headers'], isA<Map>());
      expect((_nets().single['headers'] as Map)['X-Test'], 'yes');
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // Error path
  // ═══════════════════════════════════════════════════════════════

  group('FlogHttpInterceptor.onError emission shape', () {
    test('DioException with null response emits t=err', () {
      final interceptor = FlogHttpInterceptor();
      final opts = _opts('https://example.com/timeout');
      interceptor.onRequest(opts, _ReqHandler()); // populate id map

      FlogStore.instance.clear();
      final err = DioException(
        requestOptions: opts,
        type: DioExceptionType.connectionTimeout,
        message: 'Connection timed out',
      );
      interceptor.onError(err, _ErrHandler());

      final rec = _nets().single;
      expect(rec['t'], 'err');
      expect(rec['p'], 'http');
      expect(rec['error'], 'Connection timed out');
    });

    test('DioException with 500 response emits t=res with error field', () {
      final interceptor = FlogHttpInterceptor();
      final opts = _opts('https://example.com/bad');
      interceptor.onRequest(opts, _ReqHandler());

      FlogStore.instance.clear();
      final err = DioException(
        requestOptions: opts,
        type: DioExceptionType.badResponse,
        message: 'HTTP 500',
        response: Response(
          requestOptions: opts,
          statusCode: 500,
          data: 'server error',
        ),
      );
      interceptor.onError(err, _ErrHandler());

      final rec = _nets().single;
      expect(rec['t'], 'res');
      expect(rec['status'], 500);
      expect(rec['error'], 'HTTP 500');
      expect(rec['body'], 'server error');
    });

    test('onError with untracked options falls through without emit', () {
      final interceptor = FlogHttpInterceptor();
      final opts = _opts('https://example.com/never-tracked');
      final err = DioException(
        requestOptions: opts,
        type: DioExceptionType.unknown,
      );
      final h = _ErrHandler();
      interceptor.onError(err, h);

      expect(h.next_, isTrue);
      expect(_nets(), isEmpty);
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // Response body size/shape assertions
  // ═══════════════════════════════════════════════════════════════

  group('FlogHttpInterceptor.onResponse emission shape', () {
    test('normal (non-mocked) response emits duration and status', () {
      final interceptor = FlogHttpInterceptor();
      final opts = _opts('https://example.com/ok');
      interceptor.onRequest(opts, _ReqHandler());

      FlogStore.instance.clear();
      final response = Response<dynamic>(
        requestOptions: opts,
        statusCode: 200,
        data: {'k': 'v'},
      );
      interceptor.onResponse(response, _ResHandler());

      final rec = _nets().single;
      expect(rec['t'], 'res');
      expect(rec['status'], 200);
      expect(rec['duration'], isA<int>());
      expect(rec['duration'], greaterThanOrEqualTo(0));
      // Non-mocked responses do not have mocked=true.
      expect(rec.containsKey('mocked'), isFalse);
    });

    test('normal response emits adapter timing when present', () {
      final interceptor = FlogHttpInterceptor();
      final opts = _opts('https://example.com/ok');
      interceptor.onRequest(opts, _ReqHandler());
      opts.extra[kFlogTimingTraceExtraKey] = const FlogTimingTrace(
        source: 'flog_adapter',
        startUs: 0,
        endUs: 20,
        phases: [],
        events: [],
        notes: [],
      );

      FlogStore.instance.clear();
      final response = Response<dynamic>(
        requestOptions: opts,
        statusCode: 200,
        data: 'ok',
      );
      interceptor.onResponse(response, _ResHandler());

      final rec = _nets().single;
      expect(rec['timing'], isA<Map<String, dynamic>>());
      expect(rec['timing']['source'], 'flog_adapter');
      expect(rec['timing']['endUs'], 20);
    });
  });
}
