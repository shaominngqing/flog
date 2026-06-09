import 'package:flutter_test/flutter_test.dart';
import 'package:flog_dart/src/timing/timing_trace.dart';

void main() {
  group('FlogTimingTrace', () {
    test('serializes using wire field names', () {
      final trace = FlogTimingTrace(
        version: 1,
        source: 'flog_adapter',
        clock: 'monotonic_us',
        startUs: 0,
        endUs: 126000,
        connection: const FlogTimingConnection(
          id: 'https://api.example.com:443#3',
          reused: false,
          protocol: 'http/1.1',
        ),
        phases: const [
          FlogTimingPhase(
            name: 'ttfb',
            startUs: 62000,
            endUs: 104000,
            status: 'complete',
            confidence: 'exact',
          ),
        ],
        events: const [
          FlogTimingEvent(
            name: 'first_byte',
            atUs: 104000,
            gapUs: 42000,
            size: 1,
          ),
        ],
        notes: const ['TLS boundary approximated by adapter'],
      );

      final json = trace.toJson();
      expect(json['v'], 1);
      expect(json['source'], 'flog_adapter');
      expect(json['clock'], 'monotonic_us');
      expect(json['startUs'], 0);
      expect(json['endUs'], 126000);
      expect(json['connection']['id'], 'https://api.example.com:443#3');
      expect(json['phases'][0]['name'], 'ttfb');
      expect(json['events'][0]['gapUs'], 42000);
      expect(json['notes'], ['TLS boundary approximated by adapter']);
    });

    test('durationUs is null until endUs is present', () {
      final trace = FlogTimingTrace(
        version: 1,
        source: 'ws_wrapper',
        clock: 'monotonic_us',
        startUs: 10,
        phases: const [],
        events: const [],
        notes: const [],
      );

      expect(trace.durationUs, isNull);
      expect(trace.finish(30).durationUs, 20);
    });
  });
}
