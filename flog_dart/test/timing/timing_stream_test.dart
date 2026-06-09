import 'dart:async';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:flog_dart/src/timing/timing_clock.dart';
import 'package:flog_dart/src/timing/timing_stream.dart';

void main() {
  test('records first byte, total bytes, gap, and completion', () async {
    final clock = ManualTimingClock();
    final recorder = TimingStreamRecorder(clock: clock);
    final controller = StreamController<Uint8List>(sync: true);

    final out = recorder.wrap(controller.stream);
    final seen = <Uint8List>[];
    final done = out.listen(seen.add);

    clock.advanceUs(100);
    controller.add(Uint8List.fromList([1, 2]));
    clock.advanceUs(50);
    controller.add(Uint8List.fromList([3]));
    clock.advanceUs(25);
    await controller.close();
    await done.asFuture<void>();

    expect(seen.map((bytes) => bytes.length), [2, 1]);
    expect(recorder.events.map((event) => event.name),
        ['first_byte', 'chunk', 'complete']);
    expect(recorder.events[0].atUs, 100);
    expect(recorder.events[1].gapUs, 50);
    expect(recorder.events[2].size, 3);
  });
}
