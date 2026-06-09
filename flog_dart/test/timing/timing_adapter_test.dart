import 'dart:typed_data';

import 'package:dio/dio.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flog_dart/src/timing/timing_adapter.dart';
import 'package:flog_dart/src/timing/timing_clock.dart';
import 'package:flog_dart/src/timing/timing_trace.dart';

class _FakeAdapter implements HttpClientAdapter {
  @override
  Future<ResponseBody> fetch(
    RequestOptions options,
    Stream<Uint8List>? requestStream,
    Future<void>? cancelFuture,
  ) async {
    return ResponseBody(
      Stream<Uint8List>.fromIterable([
        Uint8List.fromList([1, 2, 3]),
      ]),
      200,
      headers: {
        Headers.contentTypeHeader: ['text/plain'],
      },
    );
  }

  @override
  void close({bool force = false}) {}
}

void main() {
  test('wrapper stores timing trace in RequestOptions.extra', () async {
    final clock = ManualTimingClock();
    final adapter = FlogTimingHttpClientAdapter.wrap(
      _FakeAdapter(),
      clock: clock,
    );
    final options = RequestOptions(path: '/x', baseUrl: 'https://example.com');

    clock.advanceUs(10);
    final body = await adapter.fetch(options, null, null);
    expect(body.statusCode, 200);
    expect(options.extra.containsKey(kFlogTimingTraceExtraKey), isTrue);

    await body.stream.toList();
    final trace = options.extra[kFlogTimingTraceExtraKey] as FlogTimingTrace;
    expect(trace.toJson()['source'], 'custom_adapter');
    expect(trace.events.map((event) => event.name), ['first_byte', 'complete']);
  });
}
