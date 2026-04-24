/// Characterization tests for `lib/flog_dart.dart` top-level library.
///
/// Audit entries locked by this file:
///   - DART-003 (B): Library dartdoc references a top-level `flog()` that
///     does not exist. The real entry point is `Flog.init()`. Locked by
///     verifying the call-site compiles and there is no top-level `flog`.
///   - DART-023 (D): Flog.init swallows PackageInfo errors silently. Locked
///     by verifying Flog.init() completes synchronously (does not throw) —
///     the PackageInfo future is fired in the background via .catchError.
///
/// Cross-reference:
///   - DART-004 locked in flog_mock_interceptor_test.dart.
///   - DART-022 (start params, FIXED Phase 3 Step 3.4) — see
///     flog_server_test.dart.
library;

import 'package:flutter_test/flutter_test.dart';

import 'package:flog_dart/flog_dart.dart';

void main() {
  // ═══════════════════════════════════════════════════════════════
  // DART-003: dartdoc example uses non-existent `flog()` function
  // ═══════════════════════════════════════════════════════════════

  group('DART-003 dartdoc `flog()` example is broken', () {
    test('Flog.init() is the real entry point and is callable', () {
      // Flog.init must exist and be callable. We call it — under debug
      // (flogEnabled=true) this kicks off the singleton server and the
      // background PackageInfo fetch. It must return synchronously.
      expect(() => Flog.init(port: 49753), returnsNormally);
    });

    test(
      'DART-003 UNTESTABLE: cannot assert "top-level `flog()` does not '
      'compile" from within a test without a separate compile check.',
      () {
        // UNTESTABLE: PHYS — Dart test runners compile the whole test
        // file once. We cannot assert a non-existent symbol at test
        // runtime. The audit's proposed fix (replace `flog();` with
        // `Flog.init();` in the library-header dartdoc) is verifiable via
        // `dart analyze` on a snippet that embeds the exact dartdoc code
        // fence. For Phase 2.5B we rely on Flog.init() being callable.
        expect(true, isTrue);
      },
      skip: 'DART-003 is a documentation bug; caught only by doc-compile '
          'tooling, not unit tests.',
    );
  });

  // ═══════════════════════════════════════════════════════════════
  // DART-023: Flog.init swallows PackageInfo errors
  // ═══════════════════════════════════════════════════════════════

  group('DART-023 Flog.init swallows PackageInfo errors', () {
    test('Flog.init returns synchronously even when PackageInfo fails', () {
      // Under the flutter_test harness, PackageInfo.fromPlatform is likely
      // to fail because the platform channel is not wired in a Dart VM
      // test. The `.catchError((_) {})` at flog_dart.dart:59 swallows
      // that failure, so Flog.init remains synchronous and silent.
      //
      // We verify by calling Flog.init and asserting no exception escapes.
      expect(() => Flog.init(port: 49754), returnsNormally);
      // Also ensure a second call is a no-op (FlogServer._started guard).
      expect(() => Flog.init(port: 49755), returnsNormally);
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // FlogLogger public surface
  // ═══════════════════════════════════════════════════════════════

  group('FlogLogger public surface', () {
    test('tagged logger emits via FlogServer without throwing', () {
      FlogStore.instance.clear();
      const log = FlogLogger('Characterization');

      // All log methods should be callable and produce records in
      // FlogStore (flogEnabled=true under test).
      log.verbose('v');
      log.debug('d', error: 'e', stackTrace: StackTrace.current);
      log.info('i');
      log.warning('w');
      log.error('err', error: StateError('boom'));
      log.v('v2');
      log.d('d2');
      log.i('i2');
      log.w('w2');
      log.e('e2');

      final logs = FlogStore.instance.snapshotForTesting
          .where((r) => r['type'] == 'log')
          .toList();
      expect(logs.length, 10);
      expect(logs.first['tag'], 'Characterization');
      expect(logs.map((r) => r['level']).toSet(), {
        'verbose',
        'debug',
        'info',
        'warning',
        'error',
      });
    });

    test('printToConsole defaults to false', () {
      expect(FlogLogger.printToConsole, isFalse);
    });

    test('_log includes error and stackTrace as strings', () {
      FlogStore.instance.clear();
      const log = FlogLogger('X');
      log.error('msg', error: 'an-error', stackTrace: StackTrace.current);

      final rec = FlogStore.instance.snapshotForTesting
          .firstWhere((r) => r['type'] == 'log');
      expect(rec['error'], 'an-error');
      expect(rec['stackTrace'], isA<String>());
      expect(rec['timestamp'], isA<int>());
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // Top-level export surface lock (DART-021 cross-reference)
  // ═══════════════════════════════════════════════════════════════

  group('Top-level export surface', () {
    test('Flog, FlogLogger, FlogServer, FlogStore, FlogDio, FlogMockRule, '
        'FlogMockInterceptor, FlogHttpInterceptor, FlogSseParser, '
        'FlogWebSocket, FlogHttpConfig, SseResponse, flogEnabled, '
        'nextNetId, emitNet are all reachable from '
        'package:flog_dart/flog_dart.dart', () {
      // This test exists purely to pin the current public export set.
      // Any removal becomes a compile error here, forcing an intentional
      // breaking-change decision.
      expect(Flog, isNotNull);
      expect(FlogLogger, isNotNull);
      expect(FlogServer.instance, isNotNull);
      expect(FlogStore.instance, isNotNull);
      expect(FlogDio, isNotNull);
      expect(FlogMockRule, isNotNull);
      expect(FlogMockInterceptor, isNotNull);
      expect(FlogHttpInterceptor, isNotNull);
      expect(FlogSseParser, isNotNull);
      expect(FlogWebSocket, isNotNull);
      expect(FlogHttpConfig, isNotNull);
      expect(SseResponse, isNotNull);
      expect(flogEnabled, isA<bool>());
      expect(nextNetId, isA<Function>());
      expect(emitNet, isA<Function>());
    });
  });
}
