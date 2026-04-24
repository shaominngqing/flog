import 'dart:async';
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:flog_dart/src/flog_sse_parser.dart';

/// Helper: encode a string as a single-chunk byte stream.
Stream<List<int>> _bytesFrom(String raw) {
  return Stream.value(utf8.encode(raw));
}

/// Helper: encode a string as multiple byte-stream chunks, split at indices.
Stream<List<int>> _chunkedBytes(String raw, List<int> splitPoints) async* {
  final bytes = utf8.encode(raw);
  int prev = 0;
  for (final point in splitPoints) {
    final end = point.clamp(prev, bytes.length);
    if (end > prev) yield bytes.sublist(prev, end);
    prev = end;
  }
  if (prev < bytes.length) yield bytes.sublist(prev);
}

/// Helper: wrap _parse for testing. Since _parse is private, we test through
/// the public [FlogSseParser.wrap] API (which in non-flog mode delegates to
/// _parse), and through [FlogSseParser.wrapTyped].
///
/// Note: In test environment, `flogEnabled` is false (dart.vm.product is false
/// in test, but FLOG_ENABLED defaults to true in debug). We test the parsing
/// logic regardless — the flog layer is a thin wrapper.

void main() {
  // ═══════════════════════════════════════════════════════════════
  // Basic Parsing
  // ═══════════════════════════════════════════════════════════════

  group('Basic parsing', () {
    test('parses a single SSE event', () async {
      final stream = FlogSseParser.wrap(
        _bytesFrom('data: hello\n\n'),
        url: 'test',
      );
      final results = await stream.toList();
      expect(results, ['hello']);
    });

    test('parses multiple events in a single chunk', () async {
      final raw = 'data: first\n\ndata: second\n\ndata: third\n\n';
      final stream = FlogSseParser.wrap(_bytesFrom(raw), url: 'test');
      final results = await stream.toList();
      expect(results, ['first', 'second', 'third']);
    });

    test('parses events split across multiple chunks', () async {
      // Split "data: hello\n\ndata: world\n\n" across chunks
      final raw = 'data: hello\n\ndata: world\n\n';
      final stream = FlogSseParser.wrap(
        _chunkedBytes(raw, [8]), // split mid "data: he|llo\n..."
        url: 'test',
      );
      final results = await stream.toList();
      expect(results, ['hello', 'world']);
    });

    test('handles empty data field', () async {
      final stream = FlogSseParser.wrap(
        _bytesFrom('data:\n\n'),
        url: 'test',
      );
      final results = await stream.toList();
      expect(results, ['']);
    });

    test('handles data field without space after colon', () async {
      final stream = FlogSseParser.wrap(
        _bytesFrom('data:no-space\n\n'),
        url: 'test',
      );
      final results = await stream.toList();
      expect(results, ['no-space']);
    });

    test('handles data field with extra spaces (only first removed)', () async {
      final stream = FlogSseParser.wrap(
        _bytesFrom('data:  two-spaces\n\n'),
        url: 'test',
      );
      final results = await stream.toList();
      // Per spec, only the first space after colon is removed
      expect(results, [' two-spaces']);
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // Multi-line data (SSE spec: join with \n)
  // ═══════════════════════════════════════════════════════════════

  group('Multi-line data', () {
    test('joins multiple data lines with newline', () async {
      final raw = 'data: line one\ndata: line two\ndata: line three\n\n';
      final stream = FlogSseParser.wrap(_bytesFrom(raw), url: 'test');
      final results = await stream.toList();
      expect(results, ['line one\nline two\nline three']);
    });

    test('joins data lines including empty ones', () async {
      final raw = 'data: first\ndata:\ndata: third\n\n';
      final stream = FlogSseParser.wrap(_bytesFrom(raw), url: 'test');
      final results = await stream.toList();
      expect(results, ['first\n\nthird']);
    });

    // DART-001 regression: external reviewer reported that the old
    // _processDecoded `return` bug caused `data: {"v":1}\ndata: {"v":2}\n\n`
    // to emit only `{"v":1}`. Per W3C EventSource §9.2, consecutive `data:`
    // lines terminated by a single blank line form ONE event whose data is
    // the field values joined by '\n'. Confirm that behaviour here.
    test('DART-001 repro: two data lines in one chunk become single joined event', () async {
      final raw = 'data: {"v":1}\ndata: {"v":2}\n\n';
      final stream = FlogSseParser.wrap(_bytesFrom(raw), url: 'test');
      final results = await stream.toList();
      expect(results, ['{"v":1}\n{"v":2}']);
    });

    // Paired variant: two *separate* events (each terminated by blank line)
    // in a single chunk — must emit two values, not one.
    test('DART-001 repro: two full events in one chunk emit separately', () async {
      final raw = 'data: {"v":1}\n\ndata: {"v":2}\n\n';
      final stream = FlogSseParser.wrap(_bytesFrom(raw), url: 'test');
      final results = await stream.toList();
      expect(results, ['{"v":1}', '{"v":2}']);
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // Line Endings
  // ═══════════════════════════════════════════════════════════════

  group('Line endings', () {
    test('handles \\r\\n (CRLF) line endings', () async {
      final raw = 'data: hello\r\n\r\ndata: world\r\n\r\n';
      final stream = FlogSseParser.wrap(_bytesFrom(raw), url: 'test');
      final results = await stream.toList();
      expect(results, ['hello', 'world']);
    });

    test('handles \\r (CR only) line endings', () async {
      final raw = 'data: hello\r\rdata: world\r\r';
      final stream = FlogSseParser.wrap(_bytesFrom(raw), url: 'test');
      final results = await stream.toList();
      expect(results, ['hello', 'world']);
    });

    test('handles mixed line endings', () async {
      final raw = 'data: one\n\ndata: two\r\n\r\ndata: three\r\r';
      final stream = FlogSseParser.wrap(_bytesFrom(raw), url: 'test');
      final results = await stream.toList();
      expect(results, ['one', 'two', 'three']);
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // UTF-8 BOM
  // ═══════════════════════════════════════════════════════════════

  group('UTF-8 BOM', () {
    test('strips BOM at stream start', () async {
      // UTF-8 BOM: EF BB BF
      final bom = [0xEF, 0xBB, 0xBF];
      final rest = utf8.encode('data: hello\n\n');
      final stream = FlogSseParser.wrap(
        Stream.value([...bom, ...rest]),
        url: 'test',
      );
      final results = await stream.toList();
      expect(results, ['hello']);
    });

    test('does not strip BOM-like bytes in middle of stream', () async {
      // BOM only stripped from very start; mid-stream it's just a ZWNBSP char
      final raw = 'data: hello\n\ndata: \uFEFFworld\n\n';
      final stream = FlogSseParser.wrap(_bytesFrom(raw), url: 'test');
      final results = await stream.toList();
      expect(results[0], 'hello');
      expect(results[1], '\uFEFFworld');
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // SSE Fields (event, id, retry)
  // ═══════════════════════════════════════════════════════════════

  group('SSE fields via wrapTyped', () {
    test('parses event type', () async {
      final raw = 'event: status\ndata: online\n\n';
      final stream = FlogSseParser.wrapTyped(_bytesFrom(raw), url: 'test');
      final events = await stream.toList();
      expect(events.length, 1);
      expect(events[0].event, 'status');
      expect(events[0].data, 'online');
    });

    test('parses id field', () async {
      final raw = 'id: 42\ndata: payload\n\n';
      final stream = FlogSseParser.wrapTyped(_bytesFrom(raw), url: 'test');
      final events = await stream.toList();
      expect(events[0].id, '42');
      expect(events[0].data, 'payload');
    });

    test('id persists across events', () async {
      final raw = 'id: 1\ndata: first\n\ndata: second\n\n';
      final stream = FlogSseParser.wrapTyped(_bytesFrom(raw), url: 'test');
      final events = await stream.toList();
      expect(events[0].id, '1');
      expect(events[1].id, '1'); // persists
    });

    test('id is overwritten by new id field', () async {
      final raw = 'id: 1\ndata: first\n\nid: 2\ndata: second\n\n';
      final stream = FlogSseParser.wrapTyped(_bytesFrom(raw), url: 'test');
      final events = await stream.toList();
      expect(events[0].id, '1');
      expect(events[1].id, '2');
    });

    test('id containing NULL is ignored', () async {
      final raw = 'id: bad\u0000id\ndata: payload\n\n';
      final stream = FlogSseParser.wrapTyped(_bytesFrom(raw), url: 'test');
      final events = await stream.toList();
      expect(events[0].id, isNull); // NULL → id not set
    });

    test('parses retry field', () async {
      final raw = 'retry: 3000\ndata: payload\n\n';
      final stream = FlogSseParser.wrapTyped(_bytesFrom(raw), url: 'test');
      final events = await stream.toList();
      expect(events[0].retry, 3000);
    });

    test('retry ignores non-numeric values', () async {
      final raw = 'retry: abc\ndata: payload\n\n';
      final stream = FlogSseParser.wrapTyped(_bytesFrom(raw), url: 'test');
      final events = await stream.toList();
      expect(events[0].retry, isNull);
    });

    test('event type resets per event', () async {
      final raw = 'event: ping\ndata: 1\n\ndata: 2\n\n';
      final stream = FlogSseParser.wrapTyped(_bytesFrom(raw), url: 'test');
      final events = await stream.toList();
      expect(events[0].event, 'ping');
      expect(events[1].event, isNull); // reset
    });

    test('all fields combined', () async {
      final raw =
          'event: update\nid: 99\nretry: 5000\ndata: line1\ndata: line2\n\n';
      final stream = FlogSseParser.wrapTyped(_bytesFrom(raw), url: 'test');
      final events = await stream.toList();
      expect(events.length, 1);
      expect(events[0].event, 'update');
      expect(events[0].data, 'line1\nline2');
      expect(events[0].id, '99');
      expect(events[0].retry, 5000);
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // Comments & Unknown Fields
  // ═══════════════════════════════════════════════════════════════

  group('Comments and unknown fields', () {
    test('skips comment lines', () async {
      final raw = ': this is a comment\ndata: hello\n\n';
      final stream = FlogSseParser.wrap(_bytesFrom(raw), url: 'test');
      final results = await stream.toList();
      expect(results, ['hello']);
    });

    test('skips keep-alive comment-only blocks', () async {
      // Many servers send `: ping\n\n` as keep-alive
      final raw = ': ping\n\ndata: real\n\n';
      final stream = FlogSseParser.wrap(_bytesFrom(raw), url: 'test');
      final results = await stream.toList();
      expect(results, ['real']);
    });

    test('ignores unknown field names', () async {
      final raw = 'unknown: value\ndata: hello\n\n';
      final stream = FlogSseParser.wrap(_bytesFrom(raw), url: 'test');
      final results = await stream.toList();
      expect(results, ['hello']);
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // [DONE] Terminator
  // ═══════════════════════════════════════════════════════════════

  group('[DONE] terminator', () {
    test('filters out [DONE] event', () async {
      final raw = 'data: hello\n\ndata: [DONE]\n\n';
      final stream = FlogSseParser.wrap(_bytesFrom(raw), url: 'test');
      final results = await stream.toList();
      expect(results, ['hello']);
    });

    test('[DONE] at end of stream without trailing events', () async {
      final raw = 'data: one\n\ndata: two\n\ndata: [DONE]\n\n';
      final stream = FlogSseParser.wrap(_bytesFrom(raw), url: 'test');
      final results = await stream.toList();
      expect(results, ['one', 'two']);
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // Stream End Without Trailing Blank Line
  // ═══════════════════════════════════════════════════════════════

  group('Stream end flush', () {
    test('flushes pending event when stream ends without blank line', () async {
      // No trailing \n\n — the event should still be emitted on stream close
      final raw = 'data: hello\n';
      final stream = FlogSseParser.wrap(_bytesFrom(raw), url: 'test');
      final results = await stream.toList();
      expect(results, ['hello']);
    });

    test('flushes multi-line event on stream end', () async {
      final raw = 'data: one\ndata: two\n';
      final stream = FlogSseParser.wrap(_bytesFrom(raw), url: 'test');
      final results = await stream.toList();
      expect(results, ['one\ntwo']);
    });

    test('does not emit if no data lines pending', () async {
      final raw = 'data: hello\n\n';
      final stream = FlogSseParser.wrap(_bytesFrom(raw), url: 'test');
      final results = await stream.toList();
      // Only one event, nothing extra on flush
      expect(results, ['hello']);
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // UTF-8 Incomplete Sequences
  // ═══════════════════════════════════════════════════════════════

  group('UTF-8 handling', () {
    test('handles multi-byte char split across chunks', () async {
      // "data: 你好\n\n" — 你 is E4 BD A0, 好 is E5 A5 BD
      final bytes = utf8.encode('data: 你好\n\n');
      // Split in the middle of 你 (after first byte E4)
      final chunk1 = bytes.sublist(0, 7); // "data: " + E4
      final chunk2 = bytes.sublist(7); // BD A0 E5 A5 BD \n\n

      final stream = FlogSseParser.wrap(
        Stream.fromIterable([chunk1, chunk2]),
        url: 'test',
      );
      final results = await stream.toList();
      expect(results, ['你好']);
    });

    test('handles emoji split across chunks', () async {
      // 🎉 is F0 9F 8E 89 (4-byte UTF-8)
      final bytes = utf8.encode('data: 🎉\n\n');
      // Split after first 2 bytes of the emoji
      final splitPoint = 6 + 2; // "data: " (6 bytes) + 2 bytes of emoji
      final chunk1 = bytes.sublist(0, splitPoint);
      final chunk2 = bytes.sublist(splitPoint);

      final stream = FlogSseParser.wrap(
        Stream.fromIterable([chunk1, chunk2]),
        url: 'test',
      );
      final results = await stream.toList();
      expect(results, ['🎉']);
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // Realistic LLM Streaming Scenario
  // ═══════════════════════════════════════════════════════════════

  group('Realistic LLM streaming', () {
    test('multiple JSON chunks in single TCP packet', () async {
      // This is the exact scenario that caused the original bug:
      // two SSE events arrive in one TCP chunk
      final event1 = '{"output":[{"delta":{"content":"H"}}]}';
      final event2 = '{"output":[{"delta":{"content":"ello"}}]}';
      final raw = 'data: $event1\n\ndata: $event2\n\n';

      final stream = FlogSseParser.wrap(_bytesFrom(raw), url: 'test');
      final results = await stream.toList();
      expect(results.length, 2);
      expect(results[0], event1);
      expect(results[1], event2);
    });

    test('many small chunks arriving together', () async {
      final chunks = List.generate(
        10,
        (i) => '{"output":[{"delta":{"content":"word$i "}}]}',
      );
      final raw = chunks.map((c) => 'data: $c\n\n').join();

      final stream = FlogSseParser.wrap(_bytesFrom(raw), url: 'test');
      final results = await stream.toList();
      expect(results.length, 10);
      for (int i = 0; i < 10; i++) {
        expect(results[i], chunks[i]);
      }
    });

    test('OpenAI-style stream with [DONE]', () async {
      final raw =
          'data: {"choices":[{"delta":{"content":"Hi"}}]}\n\n'
          'data: {"choices":[{"delta":{"content":"!"}}]}\n\n'
          'data: [DONE]\n\n';

      final stream = FlogSseParser.wrap(_bytesFrom(raw), url: 'test');
      final results = await stream.toList();
      expect(results.length, 2);
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // Stream Cancellation
  // ═══════════════════════════════════════════════════════════════

  group('Stream cancellation', () {
    test('cancellation stops processing', () async {
      final controller = StreamController<List<int>>();
      final stream = FlogSseParser.wrap(controller.stream, url: 'test');

      final completer = Completer<List<String>>();
      final results = <String>[];

      final sub = stream.listen(
        (data) {
          results.add(data);
          if (results.length == 2) {
            completer.complete(results);
          }
        },
        onDone: () {
          if (!completer.isCompleted) completer.complete(results);
        },
      );

      controller.add(utf8.encode('data: one\n\n'));
      controller.add(utf8.encode('data: two\n\n'));

      final collected = await completer.future;
      expect(collected, ['one', 'two']);

      await sub.cancel();
      // After cancel, adding more data should not cause errors
      controller.add(utf8.encode('data: three\n\n'));
      await controller.close();
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // SseEvent Equality & toString
  // ═══════════════════════════════════════════════════════════════

  group('SseEvent', () {
    test('equality', () {
      const a = SseEvent(event: 'x', data: 'y', id: '1', retry: 100);
      const b = SseEvent(event: 'x', data: 'y', id: '1', retry: 100);
      const c = SseEvent(data: 'z');
      expect(a, equals(b));
      expect(a, isNot(equals(c)));
    });

    test('toString truncates long data', () {
      final longData = 'a' * 200;
      final event = SseEvent(data: longData);
      expect(event.toString(), contains('...'));
    });

    test('toString shows short data fully', () {
      const event = SseEvent(data: 'short');
      expect(event.toString(), contains('short'));
      expect(event.toString(), isNot(contains('...')));
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // Edge Cases
  // ═══════════════════════════════════════════════════════════════

  group('Edge cases', () {
    test('empty stream produces no events', () async {
      final stream = FlogSseParser.wrap(
        const Stream<List<int>>.empty(),
        url: 'test',
      );
      final results = await stream.toList();
      expect(results, isEmpty);
    });

    test('stream of only blank lines produces no events', () async {
      final stream = FlogSseParser.wrap(
        _bytesFrom('\n\n\n\n'),
        url: 'test',
      );
      final results = await stream.toList();
      expect(results, isEmpty);
    });

    test('stream of only comments produces no events', () async {
      final stream = FlogSseParser.wrap(
        _bytesFrom(': comment1\n: comment2\n\n'),
        url: 'test',
      );
      final results = await stream.toList();
      expect(results, isEmpty);
    });

    test('line without colon is treated as field with empty value', () async {
      // Per spec: "fieldname" with no colon → field=fieldname, value=""
      // Since "fieldname" is not data/event/id/retry, it's ignored
      final raw = 'weirdline\ndata: hello\n\n';
      final stream = FlogSseParser.wrap(_bytesFrom(raw), url: 'test');
      final results = await stream.toList();
      expect(results, ['hello']);
    });

    test('data-only stream (no event/id/retry)', () async {
      final stream = FlogSseParser.wrapTyped(
        _bytesFrom('data: simple\n\n'),
        url: 'test',
      );
      final events = await stream.toList();
      expect(events[0].event, isNull);
      expect(events[0].id, isNull);
      expect(events[0].retry, isNull);
      expect(events[0].data, 'simple');
    });

    test('multiple blank lines between events are harmless', () async {
      final raw = 'data: one\n\n\n\ndata: two\n\n';
      final stream = FlogSseParser.wrap(_bytesFrom(raw), url: 'test');
      final results = await stream.toList();
      expect(results, ['one', 'two']);
    });
  });
}
