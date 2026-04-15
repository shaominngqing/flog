import 'package:dio/dio.dart';

import 'flog_server.dart';
import 'flog_http_interceptor.dart';
import 'flog_mock_interceptor.dart';
import 'flog_net.dart' show flogEnabled;
import 'flog_sse_parser.dart';

/// Configuration for [FlogDio]'s automatic HTTP interception.
class FlogHttpConfig {
  /// Whether to include request headers in log output.
  final bool includeRequestHeaders;

  /// Whether to include response headers in log output.
  final bool includeResponseHeaders;

  /// Whether to include request body in log output.
  final bool includeRequestBody;

  /// Whether to include response body in log output.
  final bool includeResponseBody;

  /// Maximum body size in bytes to log. Bodies exceeding this are truncated.
  final int maxBodySize;

  /// Optional filter predicate. When provided, only requests for which
  /// [filter] returns `true` are logged.
  final FlogHttpFilter? filter;

  /// Creates a [FlogHttpConfig] with sensible defaults.
  const FlogHttpConfig({
    this.includeRequestHeaders = true,
    this.includeResponseHeaders = true,
    this.includeRequestBody = true,
    this.includeResponseBody = true,
    this.maxBodySize = 10 * 1024 * 1024,
    this.filter,
  });
}

/// Response object returned by [FlogDio.sse].
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

/// A drop-in replacement for [Dio] that automatically instruments HTTP
/// requests for the flog Network Inspector.
///
/// When [flogEnabled] is true, a [FlogHttpInterceptor] is inserted so all
/// requests are logged without manual setup. A [FlogMockInterceptor] is also
/// inserted before the HTTP interceptor so mock rules from flog take effect.
///
/// Also provides a convenience [sse] method for Server-Sent Events streams.
///
/// ```dart
/// final dio = FlogDio(baseUrl: 'https://api.example.com');
/// final response = await dio.get('/users');
///
/// // SSE streaming
/// final sse = await dio.sse('/events');
/// await for (final event in sse.stream) {
///   print(event);
/// }
/// ```
class FlogDio implements Dio {
  /// The underlying [Dio] instance that handles all HTTP operations.
  final Dio _inner;

  /// Creates a [FlogDio] instance.
  ///
  /// If [baseUrl] is provided and [options] is null, sets the base URL on the
  /// default options. If [flogEnabled] is true, a [FlogMockInterceptor] and
  /// [FlogHttpInterceptor] are automatically inserted.
  FlogDio({
    String? baseUrl,
    FlogHttpConfig? flogConfig,
    BaseOptions? options,
    int flogPort = 9753,
  }) : _inner = Dio(options ?? BaseOptions(baseUrl: baseUrl ?? '')) {
    if (baseUrl != null && options == null) {
      _inner.options.baseUrl = baseUrl;
    }

    if (flogEnabled) {
      final config = flogConfig ?? const FlogHttpConfig();

      // Start FlogServer to accept connections from flog TUI
      FlogServer.instance.start(
        port: flogPort,
        dio: _inner,
      );

      // Mock interceptor first — intercepts before real network
      _inner.interceptors.insert(0, FlogMockInterceptor());

      // HTTP logging interceptor second — logs all requests (including mocked ones)
      _inner.interceptors.insert(
        1,
        FlogHttpInterceptor(
          includeRequestHeaders: config.includeRequestHeaders,
          includeResponseHeaders: config.includeResponseHeaders,
          includeRequestBody: config.includeRequestBody,
          includeResponseBody: config.includeResponseBody,
          maxBodySize: config.maxBodySize,
          filter: config.filter,
        ),
      );
    }
  }

  /// Sends an HTTP request and returns a parsed SSE stream.
  ///
  /// The response stream is automatically wrapped with [FlogSseParser.wrap]
  /// so SSE events appear in flog's Network Inspector.
  ///
  /// ```dart
  /// final sse = await dio.sse('/chat/completions',
  ///   method: 'POST',
  ///   data: {'prompt': 'hello'},
  /// );
  /// await for (final event in sse.stream) {
  ///   print(event);
  /// }
  /// ```
  Future<SseResponse> sse(
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

    final response = await _inner.request<ResponseBody>(
      path,
      data: data,
      queryParameters: queryParameters,
      options: mergedOptions,
    );

    final url = response.requestOptions.uri.toString();
    final wrappedStream = FlogSseParser.wrap(
      response.data!.stream,
      url: url,
      method: method,
    );

    return SseResponse(
      headers: response.headers,
      statusCode: response.statusCode,
      stream: wrappedStream,
    );
  }

  // ---------------------------------------------------------------------------
  // Dio interface delegation
  // ---------------------------------------------------------------------------

  @override
  BaseOptions get options => _inner.options;

  @override
  set options(BaseOptions value) => _inner.options = value;

  @override
  Interceptors get interceptors => _inner.interceptors;

  @override
  HttpClientAdapter get httpClientAdapter => _inner.httpClientAdapter;

  @override
  set httpClientAdapter(HttpClientAdapter value) =>
      _inner.httpClientAdapter = value;

  @override
  Transformer get transformer => _inner.transformer;

  @override
  set transformer(Transformer value) => _inner.transformer = value;

  @override
  void close({bool force = false}) => _inner.close(force: force);

  @override
  Future<Response<T>> head<T>(
    String path, {
    Object? data,
    Map<String, dynamic>? queryParameters,
    Options? options,
    CancelToken? cancelToken,
  }) =>
      _inner.head<T>(
        path,
        data: data,
        queryParameters: queryParameters,
        options: options,
        cancelToken: cancelToken,
      );

  @override
  Future<Response<T>> headUri<T>(
    Uri uri, {
    Object? data,
    Options? options,
    CancelToken? cancelToken,
  }) =>
      _inner.headUri<T>(uri,
          data: data, options: options, cancelToken: cancelToken);

  @override
  Future<Response<T>> get<T>(
    String path, {
    Object? data,
    Map<String, dynamic>? queryParameters,
    Options? options,
    CancelToken? cancelToken,
    ProgressCallback? onReceiveProgress,
  }) =>
      _inner.get<T>(
        path,
        data: data,
        queryParameters: queryParameters,
        options: options,
        cancelToken: cancelToken,
        onReceiveProgress: onReceiveProgress,
      );

  @override
  Future<Response<T>> getUri<T>(
    Uri uri, {
    Object? data,
    Options? options,
    CancelToken? cancelToken,
    ProgressCallback? onReceiveProgress,
  }) =>
      _inner.getUri<T>(
        uri,
        data: data,
        options: options,
        cancelToken: cancelToken,
        onReceiveProgress: onReceiveProgress,
      );

  @override
  Future<Response<T>> post<T>(
    String path, {
    Object? data,
    Map<String, dynamic>? queryParameters,
    Options? options,
    CancelToken? cancelToken,
    ProgressCallback? onSendProgress,
    ProgressCallback? onReceiveProgress,
  }) =>
      _inner.post<T>(
        path,
        data: data,
        queryParameters: queryParameters,
        options: options,
        cancelToken: cancelToken,
        onSendProgress: onSendProgress,
        onReceiveProgress: onReceiveProgress,
      );

  @override
  Future<Response<T>> postUri<T>(
    Uri uri, {
    Object? data,
    Options? options,
    CancelToken? cancelToken,
    ProgressCallback? onSendProgress,
    ProgressCallback? onReceiveProgress,
  }) =>
      _inner.postUri<T>(
        uri,
        data: data,
        options: options,
        cancelToken: cancelToken,
        onSendProgress: onSendProgress,
        onReceiveProgress: onReceiveProgress,
      );

  @override
  Future<Response<T>> put<T>(
    String path, {
    Object? data,
    Map<String, dynamic>? queryParameters,
    Options? options,
    CancelToken? cancelToken,
    ProgressCallback? onSendProgress,
    ProgressCallback? onReceiveProgress,
  }) =>
      _inner.put<T>(
        path,
        data: data,
        queryParameters: queryParameters,
        options: options,
        cancelToken: cancelToken,
        onSendProgress: onSendProgress,
        onReceiveProgress: onReceiveProgress,
      );

  @override
  Future<Response<T>> putUri<T>(
    Uri uri, {
    Object? data,
    Options? options,
    CancelToken? cancelToken,
    ProgressCallback? onSendProgress,
    ProgressCallback? onReceiveProgress,
  }) =>
      _inner.putUri<T>(
        uri,
        data: data,
        options: options,
        cancelToken: cancelToken,
        onSendProgress: onSendProgress,
        onReceiveProgress: onReceiveProgress,
      );

  @override
  Future<Response<T>> patch<T>(
    String path, {
    Object? data,
    Map<String, dynamic>? queryParameters,
    Options? options,
    CancelToken? cancelToken,
    ProgressCallback? onSendProgress,
    ProgressCallback? onReceiveProgress,
  }) =>
      _inner.patch<T>(
        path,
        data: data,
        queryParameters: queryParameters,
        options: options,
        cancelToken: cancelToken,
        onSendProgress: onSendProgress,
        onReceiveProgress: onReceiveProgress,
      );

  @override
  Future<Response<T>> patchUri<T>(
    Uri uri, {
    Object? data,
    Options? options,
    CancelToken? cancelToken,
    ProgressCallback? onSendProgress,
    ProgressCallback? onReceiveProgress,
  }) =>
      _inner.patchUri<T>(
        uri,
        data: data,
        options: options,
        cancelToken: cancelToken,
        onSendProgress: onSendProgress,
        onReceiveProgress: onReceiveProgress,
      );

  @override
  Future<Response<T>> delete<T>(
    String path, {
    Object? data,
    Map<String, dynamic>? queryParameters,
    Options? options,
    CancelToken? cancelToken,
  }) =>
      _inner.delete<T>(
        path,
        data: data,
        queryParameters: queryParameters,
        options: options,
        cancelToken: cancelToken,
      );

  @override
  Future<Response<T>> deleteUri<T>(
    Uri uri, {
    Object? data,
    Options? options,
    CancelToken? cancelToken,
  }) =>
      _inner.deleteUri<T>(uri,
          data: data, options: options, cancelToken: cancelToken);

  @override
  Future<Response> download(
    String urlPath,
    dynamic savePath, {
    ProgressCallback? onReceiveProgress,
    Map<String, dynamic>? queryParameters,
    CancelToken? cancelToken,
    bool deleteOnError = true,
    FileAccessMode fileAccessMode = FileAccessMode.write,
    String lengthHeader = Headers.contentLengthHeader,
    Object? data,
    Options? options,
  }) =>
      _inner.download(
        urlPath,
        savePath,
        onReceiveProgress: onReceiveProgress,
        queryParameters: queryParameters,
        cancelToken: cancelToken,
        deleteOnError: deleteOnError,
        fileAccessMode: fileAccessMode,
        lengthHeader: lengthHeader,
        data: data,
        options: options,
      );

  @override
  Future<Response> downloadUri(
    Uri uri,
    dynamic savePath, {
    ProgressCallback? onReceiveProgress,
    CancelToken? cancelToken,
    bool deleteOnError = true,
    FileAccessMode fileAccessMode = FileAccessMode.write,
    String lengthHeader = Headers.contentLengthHeader,
    Object? data,
    Options? options,
  }) =>
      _inner.downloadUri(
        uri,
        savePath,
        onReceiveProgress: onReceiveProgress,
        cancelToken: cancelToken,
        deleteOnError: deleteOnError,
        fileAccessMode: fileAccessMode,
        lengthHeader: lengthHeader,
        data: data,
        options: options,
      );

  @override
  Future<Response<T>> request<T>(
    String url, {
    Object? data,
    Map<String, dynamic>? queryParameters,
    CancelToken? cancelToken,
    Options? options,
    ProgressCallback? onSendProgress,
    ProgressCallback? onReceiveProgress,
  }) =>
      _inner.request<T>(
        url,
        data: data,
        queryParameters: queryParameters,
        cancelToken: cancelToken,
        options: options,
        onSendProgress: onSendProgress,
        onReceiveProgress: onReceiveProgress,
      );

  @override
  Future<Response<T>> requestUri<T>(
    Uri uri, {
    Object? data,
    CancelToken? cancelToken,
    Options? options,
    ProgressCallback? onSendProgress,
    ProgressCallback? onReceiveProgress,
  }) =>
      _inner.requestUri<T>(
        uri,
        data: data,
        cancelToken: cancelToken,
        options: options,
        onSendProgress: onSendProgress,
        onReceiveProgress: onReceiveProgress,
      );

  @override
  Future<Response<T>> fetch<T>(RequestOptions requestOptions) =>
      _inner.fetch<T>(requestOptions);

  @override
  Dio clone({
    BaseOptions? options,
    Interceptors? interceptors,
    HttpClientAdapter? httpClientAdapter,
    Transformer? transformer,
  }) =>
      _inner.clone(
        options: options,
        interceptors: interceptors,
        httpClientAdapter: httpClientAdapter,
        transformer: transformer,
      );
}
