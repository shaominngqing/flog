import 'dart:async';
import 'dart:typed_data';

import 'package:dio/dio.dart';

import 'timing_clock.dart';
import 'timing_stream.dart';
import 'timing_trace.dart';

const String kFlogTimingTraceExtraKey = '_flog_timing_trace';

class FlogTimingHttpClientAdapter implements HttpClientAdapter {
  final HttpClientAdapter _inner;
  final FlogTimingClock _clock;
  final String _source;

  FlogTimingHttpClientAdapter.wrap(
    HttpClientAdapter inner, {
    FlogTimingClock? clock,
    String source = 'custom_adapter',
  })  : _inner = inner,
        _clock = clock ?? StopwatchTimingClock(),
        _source = source;

  @override
  Future<ResponseBody> fetch(
    RequestOptions options,
    Stream<Uint8List>? requestStream,
    Future<void>? cancelFuture,
  ) async {
    final startUs = _clock.nowUs();
    var trace = FlogTimingTrace(
      source: _source,
      startUs: startUs,
      phases: const [],
      events: const [],
    );
    options.extra[kFlogTimingTraceExtraKey] = trace;

    try {
      final response = await _inner.fetch(options, requestStream, cancelFuture);
      final headersUs = _clock.nowUs();
      final recorder = TimingStreamRecorder(clock: _clock);
      final wrappedStream = recorder.wrap(response.stream).transform(
        StreamTransformer<Uint8List, Uint8List>.fromHandlers(
          handleDone: (sink) {
            final endUs = _clock.nowUs();
            final firstByteUs = recorder.firstByteUs;
            final bodyPhase = FlogTimingPhase(
              name: 'body',
              startUs: firstByteUs,
              endUs: endUs,
              detail: firstByteUs == null
                  ? 'no body bytes were observed'
                  : 'response body stream from first byte to complete',
              status: firstByteUs == null ? 'skipped' : 'complete',
              confidence: firstByteUs == null ? 'unavailable' : 'exact',
            );
            final phases = <FlogTimingPhase>[
              FlogTimingPhase(
                name: 'request_to_headers',
                startUs: startUs,
                endUs: headersUs,
                detail: 'adapter request to response headers',
              ),
              bodyPhase,
            ];
            trace = trace.copyWith(
              endUs: endUs,
              phases: phases,
              events: recorder.events,
            );
            options.extra[kFlogTimingTraceExtraKey] = trace;
            sink.close();
          },
        ),
      );

      return ResponseBody(
        wrappedStream,
        response.statusCode,
        statusMessage: response.statusMessage,
        isRedirect: response.isRedirect,
        redirects: response.redirects,
        headers: response.headers,
      );
    } catch (error) {
      final endUs = _clock.nowUs();
      trace = trace.copyWith(
        endUs: endUs,
        phases: <FlogTimingPhase>[
          FlogTimingPhase(
            name: 'request_to_headers',
            startUs: startUs,
            endUs: endUs,
            status: 'errored',
            confidence: 'exact',
            detail: error.toString(),
          ),
        ],
      );
      options.extra[kFlogTimingTraceExtraKey] = trace;
      rethrow;
    }
  }

  @override
  void close({bool force = false}) {
    _inner.close(force: force);
  }
}
