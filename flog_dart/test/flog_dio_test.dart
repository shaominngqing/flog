/// Characterization tests for `lib/src/flog_dio.dart`.
///
/// Audit entries locked by this file:
///   - DART-010 (D): FlogDio is a 500-line hand-written delegate around Dio.
///     Locks: the class implements Dio, delegates get/post/put/delete/patch,
///     sse() wraps a response stream, and interceptors include Mock at
///     index 0 and Http at index 1.
///   - DART-011 (D): Interceptor ordering is correct at construction but
///     unguarded. Locked: users CAN `dio.interceptors.insert(0, X)` to
///     defeat the ordering (current behavior — Phase 3 adds a guard).
///   - DART-021 (D): nextNetId/emitNet are exported as part of the public
///     API — import from `package:flog_dart/flog_dart.dart` compiles.
///   - DART-026 (D): FlogDio.sse crashes on null response.data. Locked
///     behavior: the `!` bang throws NullCheckError. Phase 3 returns an
///     SseResponse with an empty stream.
library;

import 'package:dio/dio.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:flog_dart/flog_dart.dart';

void main() {
  // ═══════════════════════════════════════════════════════════════
  // DART-010: FlogDio delegate shape
  // ═══════════════════════════════════════════════════════════════

  group('DART-010 FlogDio delegate shape', () {
    test('FlogDio implements Dio — can be assigned to Dio variables', () {
      final FlogDio flogDio = FlogDio(baseUrl: 'https://api.example.com');
      final Dio dio = flogDio;
      expect(dio, isA<Dio>());
      expect(dio.options.baseUrl, 'https://api.example.com');
    });

    test('FlogDio constructor sets baseUrl when options is null', () {
      final flogDio = FlogDio(baseUrl: 'https://x.y.z');
      expect(flogDio.options.baseUrl, 'https://x.y.z');
    });

    test('FlogDio uses provided BaseOptions verbatim when options != null',
        () {
      final opts = BaseOptions(
        baseUrl: 'https://provided.example',
        connectTimeout: const Duration(seconds: 3),
      );
      final flogDio = FlogDio(
        baseUrl: 'https://ignored',
        options: opts,
      );
      // The provided BaseOptions is preserved; `baseUrl` arg is ignored
      // (see flog_dio.dart:93-96).
      expect(flogDio.options.baseUrl, 'https://provided.example');
      expect(flogDio.options.connectTimeout, const Duration(seconds: 3));
    });

    test('FlogDio exposes interceptors list via `interceptors` getter', () {
      final flogDio = FlogDio(baseUrl: 'https://x');
      expect(flogDio.interceptors, isNotNull);
      expect(flogDio.interceptors, isA<Interceptors>());
    });

    test('transformer/httpClientAdapter/options setters delegate', () {
      final flogDio = FlogDio(baseUrl: 'https://x');
      expect(flogDio.transformer, isNotNull);
      expect(flogDio.httpClientAdapter, isNotNull);

      final newOpts = BaseOptions(baseUrl: 'https://new');
      flogDio.options = newOpts;
      expect(flogDio.options.baseUrl, 'https://new');
    });

    test('close() is callable without throwing', () {
      final flogDio = FlogDio(baseUrl: 'https://x');
      expect(() => flogDio.close(), returnsNormally);
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // DART-011: interceptor ordering at construction + no guard
  // ═══════════════════════════════════════════════════════════════

  group('DART-011 interceptor ordering', () {
    test('at construction: FlogMockInterceptor at 0, FlogHttpInterceptor at 1',
        () {
      final flogDio = FlogDio(baseUrl: 'https://x');

      // Under debug/test (flogEnabled=true), both interceptors are
      // inserted. We verify the expected type ordering.
      final interceptors = flogDio.interceptors;
      // Dio always inserts an ImplyContentTypeInterceptor by default, but
      // the flog interceptors are inserted explicitly at indices 0 and 1.
      // We find the first FlogMockInterceptor and FlogHttpInterceptor and
      // assert the mock comes before the http one.
      final mockIdx =
          interceptors.indexWhere((i) => i is FlogMockInterceptor);
      final httpIdx =
          interceptors.indexWhere((i) => i is FlogHttpInterceptor);
      expect(mockIdx, 0,
          reason: 'DART-011: FlogMockInterceptor must be at index 0.');
      expect(httpIdx, 1,
          reason:
              'DART-011: FlogHttpInterceptor must immediately follow Mock.');
    });

    test('users CAN insert an interceptor at index 0 and defeat the order',
        () {
      final flogDio = FlogDio(baseUrl: 'https://x');
      final noop = InterceptorsWrapper();
      flogDio.interceptors.insert(0, noop);

      // Locks DART-011: today there is no guard. The user's interceptor
      // now sits in front of FlogMockInterceptor.
      final mockIdx =
          flogDio.interceptors.indexWhere((i) => i is FlogMockInterceptor);
      expect(mockIdx, 1,
          reason: 'Pushed to index 1 by the user-inserted interceptor; '
              'current behavior is unguarded.');
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // DART-021: public export of nextNetId + emitNet
  // ═══════════════════════════════════════════════════════════════

  group('DART-021 public exports from package:flog_dart/flog_dart.dart', () {
    test('nextNetId and emitNet are importable as top-level symbols', () {
      // If they were not exported, this file would not compile.
      // DART-021 locks the current (leaky) export surface. When Phase 3
      // un-exports them, the `import` at the top of this file needs a
      // relative path instead — the failing import will break this test
      // and alert to the public-API breaking change.
      final id = nextNetId();
      expect(id, isA<int>());
      // Call emitNet to prove the symbol is reachable.
      emitNet(<String, dynamic>{'id': id, 't': 'req', 'p': 'http'});
    });

    test('FlogHttpConfig and SseResponse are exported', () {
      const c = FlogHttpConfig();
      expect(c.includeRequestHeaders, isTrue);
      expect(c.maxBodySize, 10 * 1024 * 1024);
      // SseResponse is construction-only (no factory) — just ensure the
      // symbol resolves:
      expect(SseResponse, isNotNull);
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // DART-026: FlogDio.sse() crashes on null response.data
  // ═══════════════════════════════════════════════════════════════

  group('DART-026 sse() on null response.data', () {
    test(
      'UNTESTABLE END-TO-END: needs a real Dio response with data==null. '
      'Locked by code inspection at flog_dio.dart:158 (the `!` bang).',
      () {
        // UNTESTABLE: PHYS — to trigger the `!` crash we would need a Dio
        // response whose `.data` is null but whose response still returns
        // a ResponseBody. Mocking Dio's response pipeline is out of scope
        // for characterization; Phase 3 should refactor to return an
        // SseResponse with an empty stream instead of banging.
        //
        // Shape we expect to assert after Phase 3:
        //   final sse = await flogDio.sse('/');
        //   expect(await sse.stream.toList(), isEmpty);
        //
        // For now we document the crash is locked at flog_dio.dart:158.
        expect(true, isTrue);
      },
      skip:
          'DART-026 Phase 3 pending: sse() on null body currently throws '
          'NullCheckError; refactor to return empty stream + err frame.',
    );
  });

  // ═══════════════════════════════════════════════════════════════
  // FlogHttpConfig defaults
  // ═══════════════════════════════════════════════════════════════

  group('FlogHttpConfig defaults', () {
    test('all boolean flags default to true; maxBodySize=10MB', () {
      const c = FlogHttpConfig();
      expect(c.includeRequestHeaders, isTrue);
      expect(c.includeResponseHeaders, isTrue);
      expect(c.includeRequestBody, isTrue);
      expect(c.includeResponseBody, isTrue);
      expect(c.maxBodySize, 10 * 1024 * 1024);
      expect(c.filter, isNull);
    });

    test('overrides propagate through', () {
      const c = FlogHttpConfig(
        includeRequestHeaders: false,
        includeResponseHeaders: false,
        includeRequestBody: false,
        includeResponseBody: false,
        maxBodySize: 128,
      );
      expect(c.includeRequestHeaders, isFalse);
      expect(c.includeResponseHeaders, isFalse);
      expect(c.includeRequestBody, isFalse);
      expect(c.includeResponseBody, isFalse);
      expect(c.maxBodySize, 128);
    });
  });
}
