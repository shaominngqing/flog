/// Integration tests for the v0.8 [SseResponse] shape — `.stream` legacy
/// behavior preserved, `.events` exposes the full [SseEvent], and both
/// fields share one subscription to the underlying byte stream (DART-033
/// step 5).
library;

import 'package:dio/dio.dart';
import 'package:flog_dart/src/flog_dio_sse.dart';
import 'package:flog_dart/src/sse/event.dart';
import 'package:flutter_test/flutter_test.dart';

/// Resolves every request with a fixed SSE body (as `ResponseBody`) so we
/// can exercise [flogSse] without a real network.
class _SseFixtureInterceptor extends Interceptor {
  final String body;
  final int statusCode;
  _SseFixtureInterceptor(this.body) : statusCode = 200;

  @override
  void onRequest(RequestOptions options, RequestInterceptorHandler handler) {
    final responseBody = ResponseBody.fromString(
      body,
      statusCode,
      headers: {
        'content-type': ['text/event-stream'],
      },
    );
    handler.resolve(
      Response<ResponseBody>(
        requestOptions: options,
        statusCode: statusCode,
        data: responseBody,
      ),
      false,
    );
  }
}

Dio _dioWithBody(String body) {
  final dio = Dio(BaseOptions(baseUrl: 'https://sse.example.invalid'));
  dio.interceptors.add(_SseFixtureInterceptor(body));
  return dio;
}

void main() {
  group('DART-033 SseResponse v0.8', () {
    test('1. .stream preserves legacy v0.7 data-only behavior', () async {
      // A couple of events plus the OpenAI-style [DONE] terminator — which
      // v0.7 consumers expect to be filtered out before they see it.
      final dio = _dioWithBody(
        'data: hello\n\ndata: world\n\ndata: [DONE]\n\n',
      );
      // ignore: deprecated_member_use_from_same_package
      final sse = await flogSse(dio, '/chat');
      // ignore: deprecated_member_use_from_same_package
      final dataList = await sse.stream.toList();
      expect(dataList, ['hello', 'world']);
    });

    test('2. .events surfaces event:, id:, and retry: fields (typed)',
        () async {
      final body = 'event: greet\ndata: hi\nid: abc\n\n'
          'retry: 500\ndata: second\n\n'
          'data: [DONE]\n\n';
      final dio = _dioWithBody(body);
      final sse = await flogSse(dio, '/chat');
      final events = await sse.events.toList();

      // 3 events: first with event:+id:+data, second with retry:+data,
      // third with the [DONE] sentinel (NOT filtered in .events).
      expect(events.length, 3);

      expect(events[0].event, 'greet');
      expect(events[0].id, 'abc');
      expect(events[0].data, 'hi');

      // id: persists per W3C; event: resets.
      expect(events[1].event, isNull);
      expect(events[1].id, 'abc');
      expect(events[1].retry, 500);
      expect(events[1].data, 'second');

      // The [DONE] sentinel reaches .events verbatim.
      expect(events[2].data, '[DONE]');
    });

    test(
      '3. .stream and .events share one subscription (single-sub contract)',
      () async {
        // Per v0.8 contract: `.stream` is a `.where/.map` projection over
        // the same underlying `Stream<SseEvent>` as `.events`. Listening to
        // BOTH must fail on the second listen with a StateError — exactly
        // the Dart single-subscription-stream semantics.
        final dio = _dioWithBody('data: one\n\ndata: two\n\n');
        final sse = await flogSse(dio, '/chat');

        // First listen on .events — succeeds.
        final eventsFuture = sse.events.toList();

        // Second listen on .stream — must throw.
        expect(
          // ignore: deprecated_member_use_from_same_package
          () => sse.stream.listen((_) {}),
          throwsA(isA<StateError>()),
        );

        final got = await eventsFuture;
        expect(got.map((e) => e.data).toList(), ['one', 'two']);
      },
    );

    test('4. .options exposes the final RequestOptions', () async {
      final dio = _dioWithBody('data: x\n\n');
      final sse = await flogSse(dio, '/path');
      expect(sse.options, isA<RequestOptions>());
      expect(sse.options.uri.toString(), contains('/path'));
    });

    test('5. null-body response still produces both empty streams',
        () async {
      // Edge case: interceptor resolves with data=null (e.g. 204 No
      // Content). Both .stream and .events must be empty — not throw.
      final dio = Dio(BaseOptions(baseUrl: 'https://x.invalid'));
      dio.interceptors.add(
        InterceptorsWrapper(onRequest: (opts, handler) {
          handler.resolve(
            Response<ResponseBody>(
              requestOptions: opts,
              statusCode: 204,
              data: null,
            ),
            false,
          );
        }),
      );
      final sse = await flogSse(dio, '/events');
      // ignore: deprecated_member_use_from_same_package
      expect(await sse.stream.toList(), isEmpty);
      expect(await sse.events.toList(), isEmpty);
      expect(sse.statusCode, 204);
    });
  });
}
