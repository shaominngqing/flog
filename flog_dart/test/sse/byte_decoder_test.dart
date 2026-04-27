import 'dart:async';
import 'dart:convert';

import 'package:flog_dart/src/sse/byte_decoder.dart';
import 'package:flutter_test/flutter_test.dart';

/// Emit [raw] as a sequence of UTF-8 byte chunks split at [splitPoints].
Stream<List<int>> _chunked(List<int> bytes, List<int> splitPoints) async* {
  int prev = 0;
  for (final sp in splitPoints) {
    final end = sp.clamp(prev, bytes.length);
    if (end > prev) yield bytes.sublist(prev, end);
    prev = end;
  }
  if (prev < bytes.length) yield bytes.sublist(prev);
}

/// Wrap a single buffer as `Stream<List<int>>` (not `Stream<Uint8List>` as
/// `_single(utf8.encode(..))` would produce).
Stream<List<int>> _single(List<int> bytes) => Stream<List<int>>.value(bytes);

void main() {
  group('SseByteDecoder', () {
    test('1. empty stream yields an empty output stream', () async {
      final out = await const Stream<List<int>>.empty()
          .transform(const SseByteDecoder())
          .toList();
      expect(out, isEmpty);
    });

    test('2. single-chunk ASCII passes through intact', () async {
      final out = await _single(utf8.encode('hello world'))
          .transform(const SseByteDecoder())
          .toList();
      expect(out.join(), 'hello world');
    });

    test('3. multi-chunk ASCII concatenates lossless', () async {
      final bytes = utf8.encode('data: abc\n\ndata: def\n\n');
      final out = await _chunked(bytes, [5, 12, 18])
          .transform(const SseByteDecoder())
          .toList();
      expect(out.join(), 'data: abc\n\ndata: def\n\n');
    });

    test('4. UTF-8 2-byte char split across chunks is reassembled', () async {
      // 'é' is 0xC3 0xA9 (2 bytes).
      final bytes = utf8.encode('aéb');
      expect(bytes, [0x61, 0xC3, 0xA9, 0x62]);
      // Split right between the two bytes of é.
      final out = await _chunked(bytes, [2])
          .transform(const SseByteDecoder())
          .toList();
      expect(out.join(), 'aéb');
    });

    test('5. UTF-8 3-byte char split across chunks is reassembled', () async {
      // '汉' is E6 B1 89 (3 bytes).
      final bytes = utf8.encode('x汉y');
      expect(bytes.length, 5);
      // Split after first byte of 汉 (offset 2) and then mid-char (offset 3).
      final out = await _chunked(bytes, [2, 3])
          .transform(const SseByteDecoder())
          .toList();
      expect(out.join(), 'x汉y');
    });

    test('6. UTF-8 4-byte emoji split across chunks is reassembled',
        () async {
      // '😀' is F0 9F 98 80 (4 bytes).
      final bytes = utf8.encode('[😀]');
      expect(bytes.length, 6);
      // Split after each byte of the emoji to exercise the back-off loop.
      final out = await _chunked(bytes, [2, 3, 4, 5])
          .transform(const SseByteDecoder())
          .toList();
      expect(out.join(), '[😀]');
    });

    test('7. leading BOM is stripped exactly once', () async {
      final bytes = <int>[0xEF, 0xBB, 0xBF, ...utf8.encode('data: x\n\n')];
      final out = await _single(bytes)
          .transform(const SseByteDecoder())
          .toList();
      expect(out.join(), 'data: x\n\n');

      // Second subscription on a new decoder, no BOM: passes through.
      final out2 = await _single(utf8.encode('nobom'))
          .transform(const SseByteDecoder())
          .toList();
      expect(out2.join(), 'nobom');

      // A BOM-looking character mid-stream (not at start) is preserved —
      // it's only stripped from the very first decoded chunk.
      final mid = utf8.encode('a﻿b');
      final out3 = await _single(mid)
          .transform(const SseByteDecoder())
          .toList();
      expect(out3.join(), 'a﻿b');
    });

    test('8. buffer overrun past maxBufferBytes throws', () async {
      // Produce a large run of bytes that cannot be decoded as UTF-8 boundary
      // — use a continuation-only sequence so the decoder keeps retaining.
      // 0x80 is a bare continuation byte — always invalid as a leading byte.
      // Feed enough to exceed the cap.
      final chunk = List<int>.filled(100, 0x80);
      final source = Stream<List<int>>.fromIterable(
        List<List<int>>.generate(20, (_) => chunk),
      );

      final completer = Completer<Object>();
      source
          .transform(const SseByteDecoder(maxBufferBytes: 512))
          .listen(
            (_) {},
            onError: (Object e) {
              if (!completer.isCompleted) completer.complete(e);
            },
            onDone: () {
              if (!completer.isCompleted) {
                completer.complete(Exception('no error raised'));
              }
            },
          );

      final result = await completer.future;
      expect(result, isA<StateError>());
      expect(
        (result as StateError).message,
        contains('buffer exceeded'),
      );
    });

    test('9. byte-level backpressure preserved (pause/resume works)',
        () async {
      // Feed a stream with distinct chunks; pause after first event, resume,
      // ensure no loss.
      final raw = 'alpha\nbeta\n';
      final bytes = utf8.encode(raw);
      final received = <String>[];
      final done = Completer<void>();
      final sub = _chunked(bytes, [3, 6])
          .transform(const SseByteDecoder())
          .listen(
            received.add,
            onDone: done.complete,
          );

      // Yield to let the first chunk land, then pause.
      await Future<void>.delayed(const Duration(milliseconds: 5));
      sub.pause();
      await Future<void>.delayed(const Duration(milliseconds: 5));
      sub.resume();

      // Wait for stream completion.
      await done.future;
      await sub.cancel();
      expect(received.join(), raw);
    });
  });
}
