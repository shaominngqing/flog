import 'package:dio/dio.dart';

import 'flog_sse_parser.dart';

/// Response object returned by [FlogDio.sse] and [flogSse].
class SseResponse {
  /// The response headers.
  final Headers headers;

  /// The HTTP status code, if available.
  final int? statusCode;

  /// A stream of parsed SSE data payloads, automatically instrumented for
  /// flog's Network Inspector.
  final Stream<String> stream;

  /// Creates an [SseResponse].
  const SseResponse({
    required this.headers,
    required this.statusCode,
    required this.stream,
  });
}

/// Core implementation of the SSE convenience. Used by `FlogDio.sse()`.
///
/// Split out of `flog_dio.dart` (DART-010) to keep that file under the
/// §5.5 line budget. Takes the underlying [Dio] directly so it works
/// whether `FlogDio` is implemented as a `Dio` wrapper or an extension.
///
/// Null-safety (DART-026): if the response body is null (e.g. the server
/// returned 204 No Content or a downstream interceptor resolved with
/// `data: null`), returns an [SseResponse] whose [SseResponse.stream] is
/// empty. The request is still logged by any installed
/// `FlogHttpInterceptor` via the `req` / `res` frames.
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

  final url = response.requestOptions.uri.toString();
  final body = response.data;
  if (body == null) {
    return SseResponse(
      headers: response.headers,
      statusCode: response.statusCode,
      stream: const Stream<String>.empty(),
    );
  }

  final wrappedStream = FlogSseParser.wrap(
    body.stream,
    url: url,
    method: method,
  );

  return SseResponse(
    headers: response.headers,
    statusCode: response.statusCode,
    stream: wrappedStream,
  );
}
