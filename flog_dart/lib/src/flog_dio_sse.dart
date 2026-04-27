import 'package:dio/dio.dart';

import 'sse/byte_decoder.dart';
import 'sse/event.dart';
import 'sse/line_decoder.dart';
import 'sse/reporter.dart';

/// Response object returned by [FlogDio.sse] and [flogSse].
///
/// v0.8 change: exposes a typed [events] stream alongside the legacy
/// data-only [stream]. Both are views of the SAME underlying byte
/// subscription — consuming one consumes the other.
///
/// Migration note:
///
/// ```dart
/// // 0.7.x — still works, now deprecated:
/// await for (final data in sse.stream) { ... }
///
/// // 0.8.x — preferred, gives you event:, id:, retry:
/// await for (final evt in sse.events) {
///   print('${evt.event ?? "message"}[${evt.id}]: ${evt.data}');
/// }
/// ```
class SseResponse {
  /// The response headers.
  final Headers headers;

  /// The HTTP status code, if available.
  final int? statusCode;

  /// A stream of parsed SSE data payloads, automatically instrumented for
  /// flog's Network Inspector.
  ///
  /// Equivalent to `events.map((e) => e.data).where((d) => d != '[DONE]')`.
  /// Consumes the same underlying byte subscription as [events]; do not
  /// listen to both simultaneously.
  @Deprecated('Use .events for typed access; removed in v1.0')
  final Stream<String> stream;

  /// v0.8: stream of typed [SseEvent] values — exposes the full W3C event
  /// shape (`event:` type, `id:` / `retry:` fields, comment lines).
  ///
  /// Does NOT filter the OpenAI-style `[DONE]` terminator — consumers
  /// switching from [stream] should add `.where((e) => e.data != '[DONE]')`
  /// explicitly.
  final Stream<SseEvent> events;

  /// The underlying request's [RequestOptions] — exposed so callers can
  /// inspect the final URI / headers / method without re-deriving them.
  final RequestOptions options;

  /// Creates an [SseResponse].
  const SseResponse({
    required this.headers,
    required this.statusCode,
    required this.stream,
    required this.events,
    required this.options,
  });
}

/// Core implementation of the SSE convenience. Used by `FlogDio.sse()`.
///
/// Split out of `flog_dio.dart` (DART-010) to keep that file under the
/// §5.5 line budget. Takes the underlying [Dio] directly so it works
/// whether `FlogDio` is implemented as a `Dio` wrapper or an extension.
///
/// v0.8: builds ONE `Stream<SseEvent>` from the underlying response bytes
/// via the three composable transformers ([SseByteDecoder],
/// [SseLineDecoder], [FlogSseReporter]). The legacy `.stream` and the new
/// `.events` are derived views of the same pipeline so there's only one
/// subscription to the underlying byte stream.
///
/// Null-safety (DART-026): if the response body is null (e.g. the server
/// returned 204 No Content or a downstream interceptor resolved with
/// `data: null`), returns an [SseResponse] whose streams are empty. The
/// request is still logged by any installed `FlogHttpInterceptor` via the
/// `req` / `res` frames.
Future<SseResponse> flogSse(
  Dio dio,
  String path, {
  String method = 'GET',
  dynamic data,
  Options? options,
  Map<String, dynamic>? queryParameters,
}) async {
  final mergedOptions = (options ?? Options()).copyWith(
    method: method,
    responseType: ResponseType.stream,
  );

  final response = await dio.request<ResponseBody>(
    path,
    data: data,
    queryParameters: queryParameters,
    options: mergedOptions,
  );

  final reqOptions = response.requestOptions;
  final url = reqOptions.uri.toString();
  final body = response.data;

  if (body == null) {
    return SseResponse(
      headers: response.headers,
      statusCode: response.statusCode,
      stream: const Stream<String>.empty(),
      events: const Stream<SseEvent>.empty(),
      options: reqOptions,
    );
  }

  // Build the event stream as a broadcast-style derived view: the caller
  // picks ONE of `.stream` or `.events` (they share the underlying byte
  // subscription). We can't use `.asBroadcastStream()` because the
  // upstream transforms are single-subscription and we don't want to buffer.
  // `body.stream` is `Stream<Uint8List>`; widen to the transformer's
  // `Stream<List<int>>` input contract via `.cast` (zero-cost — Uint8List
  // IS a List<int>).
  final events = body.stream
      .cast<List<int>>()
      .transform(const SseByteDecoder())
      .transform(const SseLineDecoder())
      .transform(FlogSseReporter(url: url, method: method));

  // `.stream` is a lazy, `.map`-chained projection on `events`. Because
  // both live on the same Stream object (single-subscription), only one
  // listener will succeed; the second will throw
  // `StateError: Stream has already been listened to.` — the documented
  // contract per the v0.8 migration note.
  final dataStream =
      events.where((e) => e.data != '[DONE]').map((e) => e.data);

  return SseResponse(
    headers: response.headers,
    statusCode: response.statusCode,
    stream: dataStream,
    events: events,
    options: reqOptions,
  );
}
