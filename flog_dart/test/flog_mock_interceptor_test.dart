/// Characterization tests for `lib/src/flog_mock_interceptor.dart`.
///
/// Audit entries locked by this file:
///   - DART-004 (B): onRequest runs mock logic even when flogEnabled is false.
///     UNTESTABLE: PHYS — flogEnabled is a compile-time const (!dart.vm.product).
///     Tests run under debug (flogEnabled=true). We document the current
///     debug-mode behavior and accept that the release-mode tree-shake
///     property cannot be verified from within `dart test`. Phase 3
///     introduces a runtime flag variant if needed.
///
///   - DART-012 (D): Mock rule list is process-wide static; instances share
///     state. Locked so Phase 3's FlogMockStore redesign preserves
///     backwards-compat semantics for default users.
///
///   - DART-013 (D): URL match is substring (String.contains), case-sensitive;
///     first-match-wins; method filter is case-insensitive.
///
///   - DART-014 (D): `flog_mocked` magic string is written to
///     options.extra by the interceptor. Contract with FlogHttpInterceptor.
library;

import 'package:dio/dio.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:flog_dart/flog_dart.dart';

/// Minimal RequestInterceptorHandler capture implementation. Captures the
/// outcome (resolve/next/reject) so tests can assert which branch ran.
///
/// `noSuchMethod` handles the unused `_BaseHandler.future` / `isCompleted`
/// members that we do not exercise.
class _CapturingHandler implements RequestInterceptorHandler {
  Response<dynamic>? resolvedResponse;
  bool resolvedWithCallFollowing = false;
  bool resolveCalled = false;

  RequestOptions? nextOptions;
  bool nextCalled = false;

  DioException? rejectedError;
  bool rejectCalled = false;
  bool rejectedCallFollowingErrorInterceptor = false;

  @override
  void next(RequestOptions requestOptions) {
    nextCalled = true;
    nextOptions = requestOptions;
  }

  @override
  void resolve(Response response, [bool callFollowingResponseInterceptor = false]) {
    resolveCalled = true;
    resolvedResponse = response;
    resolvedWithCallFollowing = callFollowingResponseInterceptor;
  }

  @override
  void reject(DioException error, [bool callFollowingErrorInterceptor = false]) {
    rejectCalled = true;
    rejectedError = error;
    rejectedCallFollowingErrorInterceptor = callFollowingErrorInterceptor;
  }

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

RequestOptions _opts(String url, {String method = 'GET'}) {
  final uri = Uri.parse(url);
  return RequestOptions(
    path: uri.path + (uri.hasQuery ? '?${uri.query}' : ''),
    baseUrl: '${uri.scheme}://${uri.authority}',
    method: method,
  );
}

void main() {
  setUp(() {
    // Clear the static rule list to isolate tests. DART-012: currently
    // `_rules` is process-wide static with no reset; updateRules(empty) is
    // the public channel for clearing.
    FlogMockInterceptor.updateRules([]);
  });

  tearDownAll(() {
    FlogMockInterceptor.updateRules([]);
  });

  // ═══════════════════════════════════════════════════════════════
  // DART-004: mock logic runs in debug mode even without Flog.init
  // ═══════════════════════════════════════════════════════════════

  group('DART-004 mock logic runs unconditionally in debug', () {
    test('onRequest evaluates rules regardless of Flog.init having been called',
        () {
      // UNTESTABLE: PHYS — flogEnabled is a compile-time const
      // (!dart.vm.product). Tests run under debug (flogEnabled=true). We
      // document the current debug-mode behavior and accept that the
      // release-mode tree-shake property cannot be verified from within
      // `dart test`. Phase 3 introduces a runtime flag variant if needed.
      FlogMockInterceptor.updateRules([
        FlogMockRule(
          urlPattern: 'example.com',
          statusCode: 200,
          responseBody: '{"ok": true}',
          enabled: true,
        ),
      ]);

      final interceptor = FlogMockInterceptor();
      final handler = _CapturingHandler();
      interceptor.onRequest(_opts('https://example.com/foo'), handler);

      // Currently matches and resolves — even though Flog.init was never
      // called. Phase 3 guards this behind flogEnabled.
      expect(handler.resolveCalled, isTrue);
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // DART-012: static rule list shared across instances
  // ═══════════════════════════════════════════════════════════════

  group('DART-012 static _rules shared across FlogMockInterceptor instances',
      () {
    test('two independently constructed interceptors see the same rules', () {
      FlogMockInterceptor.updateRules([
        FlogMockRule(
          urlPattern: '/shared',
          statusCode: 201,
          responseBody: '{"from":"static"}',
          enabled: true,
        ),
      ]);

      final a = FlogMockInterceptor();
      final b = FlogMockInterceptor();
      final handlerA = _CapturingHandler();
      final handlerB = _CapturingHandler();

      a.onRequest(_opts('https://example.com/shared'), handlerA);
      b.onRequest(_opts('https://example.com/shared'), handlerB);

      expect(handlerA.resolveCalled, isTrue);
      expect(handlerB.resolveCalled, isTrue);
      expect(handlerA.resolvedResponse?.statusCode, 201);
      expect(handlerB.resolvedResponse?.statusCode, 201);
    });

    test('updateRules replaces the entire list (not additive)', () {
      FlogMockInterceptor.updateRules([
        FlogMockRule(
            urlPattern: '/one',
            statusCode: 200,
            responseBody: '{}',
            enabled: true),
      ]);
      FlogMockInterceptor.updateRules([
        FlogMockRule(
            urlPattern: '/two',
            statusCode: 200,
            responseBody: '{}',
            enabled: true),
      ]);

      final h1 = _CapturingHandler();
      final h2 = _CapturingHandler();
      FlogMockInterceptor().onRequest(_opts('https://example.com/one'), h1);
      FlogMockInterceptor().onRequest(_opts('https://example.com/two'), h2);

      // /one should no longer match (first updateRules was clobbered).
      expect(h1.nextCalled, isTrue);
      expect(h1.resolveCalled, isFalse);
      // /two matches the current set.
      expect(h2.resolveCalled, isTrue);
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // DART-013: match semantics (substring, case-sensitive, first-win)
  // ═══════════════════════════════════════════════════════════════

  group('DART-013 URL match uses String.contains (substring)', () {
    test('urlPattern matches as substring, not exact', () {
      FlogMockInterceptor.updateRules([
        FlogMockRule(
          urlPattern: '/api/users',
          statusCode: 200,
          responseBody: '{"hit":true}',
          enabled: true,
        ),
      ]);

      final handler = _CapturingHandler();
      FlogMockInterceptor().onRequest(
        _opts('https://example.com/api/users/42/profile'),
        handler,
      );
      expect(handler.resolveCalled, isTrue);
    });

    test('urlPattern is case-sensitive', () {
      FlogMockInterceptor.updateRules([
        FlogMockRule(
          urlPattern: '/API/Users',
          statusCode: 200,
          responseBody: '{}',
          enabled: true,
        ),
      ]);

      final handler = _CapturingHandler();
      FlogMockInterceptor().onRequest(
        _opts('https://example.com/api/users'),
        handler,
      );
      // Phase 1 behavior: case mismatch = no match, falls through to next().
      expect(handler.resolveCalled, isFalse);
      expect(handler.nextCalled, isTrue);
    });

    test('first matching rule wins; later rules are dead', () {
      FlogMockInterceptor.updateRules([
        FlogMockRule(
            urlPattern: '/api',
            statusCode: 201,
            responseBody: '{"which":"first"}',
            enabled: true),
        FlogMockRule(
            urlPattern: '/api',
            statusCode: 418,
            responseBody: '{"which":"second"}',
            enabled: true),
      ]);

      final handler = _CapturingHandler();
      FlogMockInterceptor()
          .onRequest(_opts('https://example.com/api/x'), handler);

      expect(handler.resolveCalled, isTrue);
      expect(handler.resolvedResponse?.statusCode, 201,
          reason: 'First-match-wins: the second rule is unreachable.');
    });

    test('disabled rules are skipped', () {
      FlogMockInterceptor.updateRules([
        FlogMockRule(
            urlPattern: '/foo',
            statusCode: 500,
            responseBody: '',
            enabled: false),
        FlogMockRule(
            urlPattern: '/foo',
            statusCode: 200,
            responseBody: '{"ok":true}',
            enabled: true),
      ]);

      final handler = _CapturingHandler();
      FlogMockInterceptor()
          .onRequest(_opts('https://example.com/foo'), handler);

      expect(handler.resolveCalled, isTrue);
      expect(handler.resolvedResponse?.statusCode, 200);
    });

    test('method filter is case-insensitive when rule.method is set', () {
      FlogMockInterceptor.updateRules([
        FlogMockRule(
          urlPattern: '/echo',
          method: 'post',
          statusCode: 200,
          responseBody: '{}',
          enabled: true,
        ),
      ]);

      final hGet = _CapturingHandler();
      FlogMockInterceptor().onRequest(
        _opts('https://example.com/echo', method: 'GET'),
        hGet,
      );
      expect(hGet.resolveCalled, isFalse);
      expect(hGet.nextCalled, isTrue);

      final hPost = _CapturingHandler();
      FlogMockInterceptor().onRequest(
        _opts('https://example.com/echo', method: 'POST'),
        hPost,
      );
      expect(hPost.resolveCalled, isTrue);
    });

    test('rule.method == null matches any method', () {
      FlogMockInterceptor.updateRules([
        FlogMockRule(
          urlPattern: '/any',
          method: null,
          statusCode: 200,
          responseBody: '{}',
          enabled: true,
        ),
      ]);

      for (final m in ['GET', 'POST', 'DELETE', 'PATCH']) {
        final h = _CapturingHandler();
        FlogMockInterceptor()
            .onRequest(_opts('https://example.com/any', method: m), h);
        expect(h.resolveCalled, isTrue, reason: 'method $m should match');
      }
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // DART-014: flog_mocked magic key written to options.extra
  // ═══════════════════════════════════════════════════════════════

  group('DART-014 flog_mocked extra key contract', () {
    test('matched requests have options.extra[flog_mocked] == true', () {
      FlogMockInterceptor.updateRules([
        FlogMockRule(
            urlPattern: '/m',
            statusCode: 200,
            responseBody: '{}',
            enabled: true),
      ]);

      final opts = _opts('https://example.com/m');
      final handler = _CapturingHandler();
      FlogMockInterceptor().onRequest(opts, handler);

      expect(opts.extra['flog_mocked'], true,
          reason: 'DART-014: key is the magic string `flog_mocked`. '
              'FlogHttpInterceptor reads this exact key; any rename must '
              'update both sides.');
    });

    test('unmatched requests leave options.extra untouched', () {
      FlogMockInterceptor.updateRules([
        FlogMockRule(
            urlPattern: '/nomatch',
            statusCode: 200,
            responseBody: '{}',
            enabled: true),
      ]);

      final opts = _opts('https://example.com/something-else');
      final handler = _CapturingHandler();
      FlogMockInterceptor().onRequest(opts, handler);

      expect(opts.extra.containsKey('flog_mocked'), isFalse);
      expect(handler.nextCalled, isTrue);
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // FlogMockRule.fromJson shape (defensive defaults)
  // ═══════════════════════════════════════════════════════════════

  group('FlogMockRule.fromJson defensive defaults', () {
    test('missing fields receive defaults', () {
      final rule = FlogMockRule.fromJson(<String, dynamic>{});
      expect(rule.urlPattern, '');
      expect(rule.method, isNull);
      expect(rule.statusCode, 200);
      expect(rule.responseBody, '{}');
      expect(rule.delayMs, 0);
      expect(rule.enabled, isTrue);
    });

    test('present fields round-trip', () {
      final rule = FlogMockRule.fromJson({
        'url_pattern': '/x',
        'method': 'POST',
        'status_code': 418,
        'response_body': '{"teapot":true}',
        'delay_ms': 250,
        'enabled': false,
      });
      expect(rule.urlPattern, '/x');
      expect(rule.method, 'POST');
      expect(rule.statusCode, 418);
      expect(rule.responseBody, '{"teapot":true}');
      expect(rule.delayMs, 250);
      expect(rule.enabled, isFalse);
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // Response body JSON parsing: falls back to raw string on parse error
  // ═══════════════════════════════════════════════════════════════

  group('Mock response body JSON parsing', () {
    test('valid JSON body is parsed into Map', () {
      FlogMockInterceptor.updateRules([
        FlogMockRule(
          urlPattern: '/json',
          statusCode: 200,
          responseBody: '{"a":1}',
          enabled: true,
        ),
      ]);

      final handler = _CapturingHandler();
      FlogMockInterceptor()
          .onRequest(_opts('https://example.com/json'), handler);

      expect(handler.resolveCalled, isTrue);
      expect(handler.resolvedResponse?.data, {'a': 1});
    });

    test('non-JSON body falls through as raw string', () {
      FlogMockInterceptor.updateRules([
        FlogMockRule(
          urlPattern: '/text',
          statusCode: 200,
          responseBody: 'hello world',
          enabled: true,
        ),
      ]);

      final handler = _CapturingHandler();
      FlogMockInterceptor()
          .onRequest(_opts('https://example.com/text'), handler);

      expect(handler.resolveCalled, isTrue);
      expect(handler.resolvedResponse?.data, 'hello world');
    });

    test('callFollowingResponseInterceptor is true on resolve', () {
      FlogMockInterceptor.updateRules([
        FlogMockRule(
          urlPattern: '/chain',
          statusCode: 200,
          responseBody: '{}',
          enabled: true,
        ),
      ]);

      final handler = _CapturingHandler();
      FlogMockInterceptor()
          .onRequest(_opts('https://example.com/chain'), handler);

      expect(handler.resolvedWithCallFollowing, isTrue,
          reason:
              'Current behavior: mock interceptor passes callFollowing=true '
              'so downstream response interceptors still run (e.g. envelope '
              'unwrap).');
    });
  });
}
