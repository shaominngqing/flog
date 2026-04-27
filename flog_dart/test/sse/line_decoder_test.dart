import 'package:flog_dart/src/sse/event.dart';
import 'package:flog_dart/src/sse/line_decoder.dart';
import 'package:flutter_test/flutter_test.dart';

/// Feed a single text chunk through the decoder and return the events.
Future<List<SseEvent>> _parse(String input) async {
  final stream = Stream<String>.value(input).transform(const SseLineDecoder());
  return stream.toList();
}

/// Feed a list of text chunks (simulating split-across-chunks delivery)
/// and return the events.
Future<List<SseEvent>> _parseChunks(List<String> chunks) async {
  final stream = Stream<String>.fromIterable(chunks)
      .transform(const SseLineDecoder());
  return stream.toList();
}

void main() {
  group('SseLineDecoder', () {
    test('1. single event with data field', () async {
      final events = await _parse('data: hello\n\n');
      expect(events.length, 1);
      expect(events[0].data, 'hello');
      expect(events[0].event, isNull);
    });

    test('2. multiple events in one chunk', () async {
      final events = await _parse('data: a\n\ndata: b\n\ndata: c\n\n');
      expect(events.map((e) => e.data).toList(), ['a', 'b', 'c']);
    });

    test('3. multi-line data joined with newline', () async {
      final events = await _parse('data: line1\ndata: line2\ndata: line3\n\n');
      expect(events.length, 1);
      expect(events[0].data, 'line1\nline2\nline3');
    });

    test('4. event: resets after dispatch', () async {
      final events = await _parse(
        'event: greet\ndata: hi\n\ndata: plain\n\n',
      );
      expect(events.length, 2);
      expect(events[0].event, 'greet');
      expect(events[0].data, 'hi');
      expect(events[1].event, isNull);
      expect(events[1].data, 'plain');
    });

    test('5. id: persists across events (per W3C)', () async {
      final events = await _parse(
        'id: abc\ndata: one\n\ndata: two\n\nid: def\ndata: three\n\n',
      );
      expect(events.length, 3);
      expect(events[0].id, 'abc');
      expect(events[1].id, 'abc'); // persists
      expect(events[2].id, 'def');
    });

    test('6. id containing NUL is rejected', () async {
      final events = await _parse(
        'id: valid\ndata: a\n\nid: bad\x00id\ndata: b\n\n',
      );
      expect(events[0].id, 'valid');
      // Bad id is rejected — previous id remains.
      expect(events[1].id, 'valid');
    });

    test('7. retry: accepts non-negative ints, drops garbage', () async {
      final events = await _parse(
        'retry: 3000\ndata: a\n\nretry: not-a-number\ndata: b\n\n'
        'retry: -5\ndata: c\n\n',
      );
      // `retry` is per-event (cleared at dispatch, per spec it's a
      // reconnection-time hint that a client applies, not a persistent
      // field). Garbage and negative values are silently dropped.
      expect(events[0].retry, 3000);
      expect(events[1].retry, isNull); // garbage dropped, no prev value
      expect(events[2].retry, isNull); // negative dropped
    });

    test('8. comment lines captured, do not fire events', () async {
      final events = await _parse(': ping\n: keepalive\n\ndata: msg\n\n');
      expect(events.length, 1);
      expect(events[0].comments, isNull);
      expect(events[0].data, 'msg');
    });

    test('9. \\r\\n and \\r line endings accepted', () async {
      final crlf = await _parse('data: one\r\n\r\n');
      expect(crlf[0].data, 'one');

      final cr = await _parse('data: two\r\r');
      expect(cr[0].data, 'two');

      final mixed = await _parse('data: a\r\ndata: b\r\n\r\n');
      expect(mixed[0].data, 'a\nb');
    });

    test('10. field split across chunks is preserved', () async {
      final events = await _parseChunks(
        ['data: hel', 'lo\ndata: wo', 'rld\n\n'],
      );
      expect(events.length, 1);
      expect(events[0].data, 'hello\nworld');
    });

    test('11. stream-end flushes pending event (no trailing blank line)',
        () async {
      final events = await _parse('data: tail');
      expect(events.length, 1);
      expect(events[0].data, 'tail');
    });

    test('12. empty data field produces an event with empty data', () async {
      final events = await _parse('data:\n\n');
      expect(events.length, 1);
      expect(events[0].data, '');
    });

    test('13. first space after colon stripped; subsequent spaces preserved',
        () async {
      final events = await _parse('data:  two-spaces\n\n');
      expect(events[0].data, ' two-spaces');
    });

    test('14. event with only id/retry (no data) does not dispatch',
        () async {
      final events = await _parse('id: 42\nretry: 1000\n\n');
      expect(events, isEmpty);
    });
  });
}
