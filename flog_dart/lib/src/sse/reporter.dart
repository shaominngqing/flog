import 'dart:async';

import '../flog_net.dart' show flogEnabled, nextNetId, emitNet;
import '../timing/timing_clock.dart';
import '../timing/timing_trace.dart';
import 'event.dart';

/// Telemetry tee for an SSE event stream. Emits flog_net protocol messages
/// (`req` / `chunk` / `res` / `err`) while passing every event downstream
/// untouched.
///
/// Usage:
///
/// ```dart
/// byteStream
///   .transform(const SseByteDecoder())
///   .transform(const SseLineDecoder())
///   .transform(FlogSseReporter(url: url, method: 'GET'))
///   .map((e) => e.data); // or: .listen(...)
/// ```
///
/// **Tree-shaking**: when [flogEnabled] is `false` (release builds without
/// `FLOG_ENABLED=true`), construction returns a passthrough sink — no
/// `emitNet` calls, no timestamps captured, no per-event counter — so AOT
/// elimination collapses the transformer to an identity.
class FlogSseReporter extends StreamTransformerBase<SseEvent, SseEvent> {
  /// The request URL, recorded in the `req` frame.
  final String url;

  /// The HTTP method (defaults to `GET`), recorded in the `req` frame.
  final String method;

  /// Monotonic clock used for timing metadata. Tests pass a manual clock;
  /// production defaults to [StopwatchTimingClock].
  final FlogTimingClock? clock;

  const FlogSseReporter({
    required this.url,
    this.method = 'GET',
    this.clock,
  });

  @override
  Stream<SseEvent> bind(Stream<SseEvent> stream) {
    if (!flogEnabled) {
      // Pure passthrough. No allocations, no emissions.
      return stream;
    }

    final controller = StreamController<SseEvent>(sync: false);
    final id = nextNetId();
    final start = DateTime.now();
    final timingClock = clock ?? StopwatchTimingClock();
    final startUs = timingClock.nowUs();
    int? lastEventUs;
    final events = <FlogTimingEvent>[];
    int seq = 0;
    late StreamSubscription<SseEvent> sub;

    void emit(String type, Map<String, dynamic> extra) {
      emitNet(<String, dynamic>{
        'id': id,
        't': type,
        'p': 'sse',
        ...extra,
      });
    }

    // Subscribe eagerly so `req` fires exactly at the moment the pipeline
    // starts, matching v0.7 semantics.
    scheduleMicrotask(() {
      emit('req', {'method': method, 'url': url});
    });

    sub = stream.listen(
      (event) {
        seq++;
        final nowUs = timingClock.nowUs();
        final timingEvent = FlogTimingEvent(
          name: 'chunk',
          atUs: nowUs,
          gapUs: lastEventUs == null ? null : nowUs - lastEventUs!,
          size: event.data.length,
        );
        lastEventUs = nowUs;
        events.add(timingEvent);
        emit('chunk', {
          'data': event.data,
          'seq': seq,
          'size': event.data.length,
          'eventTiming': timingEvent.toJson(),
        });
        controller.add(event);
      },
      onError: (Object e, StackTrace st) {
        final endUs = timingClock.nowUs();
        final firstEventUs = events
            .where((entry) => entry.name == 'chunk')
            .map((entry) => entry.atUs)
            .firstWhere((value) => value != null, orElse: () => null);
        emit('err', {
          'error': e.toString(),
          'timing': FlogTimingTrace(
            source: 'sse_reporter',
            startUs: startUs,
            endUs: endUs,
            phases: _donePhases(
              startUs: startUs,
              endUs: endUs,
              firstEventUs: firstEventUs,
            ),
            events: events,
          ).toJson(),
        });
        controller.addError(e, st);
      },
      onDone: () {
        final duration = DateTime.now().difference(start).inMilliseconds;
        final endUs = timingClock.nowUs();
        final firstEventUs = events
            .where((entry) => entry.name == 'chunk')
            .map((entry) => entry.atUs)
            .firstWhere((value) => value != null, orElse: () => null);
        emit('done', {
          'duration': duration,
          'chunks': seq,
          'timing': FlogTimingTrace(
            source: 'sse_reporter',
            startUs: startUs,
            endUs: endUs,
            phases: _donePhases(
              startUs: startUs,
              endUs: endUs,
              firstEventUs: firstEventUs,
            ),
            events: events,
          ).toJson(),
        });
        controller.close();
      },
      cancelOnError: false,
    );

    controller.onCancel = () => sub.cancel();
    return controller.stream;
  }

  List<FlogTimingPhase> _donePhases({
    required int startUs,
    required int endUs,
    required int? firstEventUs,
  }) {
    if (firstEventUs == null) {
      return <FlogTimingPhase>[
        FlogTimingPhase(
          name: 'wait_first_event',
          startUs: startUs,
          endUs: endUs,
          status: 'complete',
          detail: 'no SSE chunks were emitted',
        ),
        FlogTimingPhase(
          name: 'receive_stream',
          startUs: endUs,
          endUs: endUs,
          status: 'unavailable',
          confidence: 'unavailable',
          detail: 'no event stream content',
        ),
      ];
    }

    return <FlogTimingPhase>[
      FlogTimingPhase(
        name: 'wait_first_event',
        startUs: startUs,
        endUs: firstEventUs,
        detail: 'waiting for first SSE chunk',
      ),
      FlogTimingPhase(
        name: 'receive_stream',
        startUs: firstEventUs,
        endUs: endUs,
        detail: 'SSE chunks received',
      ),
    ];
  }
}
