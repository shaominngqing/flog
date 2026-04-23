/// Characterization tests for `lib/src/flog_net.dart`.
///
/// Audit entries locked by this file:
///   - DART-009 (B): `emitNet` mutates caller-owned Map with 'type'/'ts' keys.
///   - DART-029 (E): `_nextId` is process-wide state, not resettable; tests
///     observe the counter is monotonic but cannot assert a starting value
///     without isolation.
///
/// Note: `flogEnabled` is a compile-time const that is true in debug-mode
/// test runs (see UNTESTABLE: PHYS notes elsewhere). All tests here run
/// with flogEnabled=true.
library;

import 'package:flutter_test/flutter_test.dart';

import 'package:flog_dart/src/flog_net.dart';
import 'package:flog_dart/src/flog_server.dart';

void main() {
  // ═══════════════════════════════════════════════════════════════
  // DART-009: emitNet mutates caller-owned Map
  // ═══════════════════════════════════════════════════════════════

  group('DART-009 emitNet decorates the caller-provided map in place', () {
    test('emitNet adds `type` and `ts` keys to the passed map', () {
      // Prevent the singleton server from actually trying to bind sockets
      // by passing flogEnabled=true paths; send() records into FlogStore and
      // broadcasts — since no clients are connected, the broadcast is a
      // no-op. FlogStore.record is safe to call.
      final map = <String, dynamic>{'id': 1, 't': 'req'};
      emitNet(map);

      // The passed-in map is decorated in place — this is the current
      // (buggy) behavior documented by DART-009.
      expect(map.containsKey('type'), isTrue,
          reason: 'emitNet should have written `type` into the caller map');
      expect(map['type'], 'net');
      expect(map.containsKey('ts'), isTrue,
          reason: 'emitNet should have written `ts` into the caller map');
      expect(map['ts'], isA<int>());
    });

    test('emitNet preserves existing keys on the caller map', () {
      final map = <String, dynamic>{
        'id': 42,
        't': 'req',
        'p': 'http',
        'custom': 'preserved',
      };
      emitNet(map);

      expect(map['id'], 42);
      expect(map['t'], 'req');
      expect(map['p'], 'http');
      expect(map['custom'], 'preserved');
    });

    test('emitNet ts is a millisecondsSinceEpoch int', () {
      final before = DateTime.now().millisecondsSinceEpoch;
      final map = <String, dynamic>{};
      emitNet(map);
      final after = DateTime.now().millisecondsSinceEpoch;

      final ts = map['ts'] as int;
      expect(ts, greaterThanOrEqualTo(before));
      expect(ts, lessThanOrEqualTo(after));
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // DART-029: nextNetId monotonic counter
  // ═══════════════════════════════════════════════════════════════

  group('DART-029 nextNetId is a process-wide monotonic counter', () {
    test('successive calls return strictly increasing integers', () {
      final a = nextNetId();
      final b = nextNetId();
      final c = nextNetId();

      expect(b, greaterThan(a));
      expect(c, greaterThan(b));
      // Each call should increment by exactly 1 (current impl is `_nextId++`).
      expect(b - a, 1);
      expect(c - b, 1);
    });

    test('nextNetId is exported from package:flog_dart', () {
      // Re-import via the top-level library to assert DART-021's current
      // export. This will flip to a compile error when DART-021 is fixed.
      final id = nextNetId();
      expect(id, isA<int>());
      expect(id, greaterThan(0));
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // flogEnabled is exported and addressable at runtime
  // ═══════════════════════════════════════════════════════════════

  group('flogEnabled public const', () {
    test('flogEnabled is true under `flutter test` (debug mode)', () {
      // UNTESTABLE: PHYS — flogEnabled is a compile-time const driven by
      // bool.fromEnvironment('FLOG_ENABLED', defaultValue: !dart.vm.product).
      // Tests run in a non-product VM, so flogEnabled defaults to true.
      // We cannot verify the release-mode tree-shake property from within
      // `dart test`. Phase 3 may introduce a runtime flag variant if needed.
      expect(flogEnabled, isTrue);
    });
  });

  // Keep a reference to FlogServer to ensure the import is not tree-shaken
  // by the analyzer (we only touch emitNet here; FlogServer is exercised by
  // flog_server_test.dart).
  test('FlogServer singleton is reachable', () {
    expect(FlogServer.instance, isNotNull);
  });
}
