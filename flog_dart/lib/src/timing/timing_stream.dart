import 'dart:async';
import 'dart:typed_data';

import 'timing_clock.dart';
import 'timing_trace.dart';

class TimingStreamRecorder {
  final FlogTimingClock clock;
  final List<FlogTimingEvent> events = <FlogTimingEvent>[];
  int? _lastUs;
  int _totalBytes = 0;
  bool _sawFirstByte = false;
  int? _firstByteUs;

  TimingStreamRecorder({required this.clock});

  int get totalBytes => _totalBytes;

  int? get firstByteUs => _firstByteUs;

  Stream<Uint8List> wrap(Stream<Uint8List> input) {
    late StreamController<Uint8List> controller;
    StreamSubscription<Uint8List>? subscription;

    controller = StreamController<Uint8List>(
      onListen: () {
        subscription = input.listen(
          (chunk) {
            final now = clock.nowUs();
            final gap = _lastUs == null ? null : now - _lastUs!;
            _lastUs = now;
            _totalBytes += chunk.length;
            events.add(FlogTimingEvent(
              name: _sawFirstByte ? 'chunk' : 'first_byte',
              atUs: now,
              gapUs: gap,
              size: chunk.length,
            ));
            _sawFirstByte = true;
            _firstByteUs ??= now;
            controller.add(chunk);
          },
          onError: (Object error, StackTrace stackTrace) {
            final now = clock.nowUs();
            events.add(FlogTimingEvent(
              name: 'stream_error',
              atUs: now,
              size: _totalBytes,
              detail: error.toString(),
            ));
            controller.addError(error, stackTrace);
          },
          onDone: () {
            final now = clock.nowUs();
            events.add(FlogTimingEvent(
              name: 'complete',
              atUs: now,
              size: _totalBytes,
            ));
            controller.close();
          },
          cancelOnError: false,
        );
      },
      onPause: () => subscription?.pause(),
      onResume: () => subscription?.resume(),
      onCancel: () => subscription?.cancel(),
    );

    return controller.stream;
  }
}
