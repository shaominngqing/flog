import 'dart:convert';

import 'package:dio/dio.dart';

import 'flog_net.dart';

/// Predicate to decide whether a request should be logged.
typedef FlogHttpFilter = bool Function(RequestOptions options);

/// Dio interceptor that emits flog_net protocol messages for HTTP traffic.
///
/// ```dart
/// final dio = Dio();
/// dio.interceptors.add(FlogHttpInterceptor());
/// ```
class FlogHttpInterceptor extends Interceptor {
  /// Whether to include request headers in log output.
  final bool includeRequestHeaders;

  /// Whether to include response headers in log output.
  final bool includeResponseHeaders;

  /// Whether to include request body in log output.
  final bool includeRequestBody;

  /// Whether to include response body in log output.
  final bool includeResponseBody;

  /// Maximum body size in bytes to log. Bodies exceeding this are truncated.
  /// Defaults to 10 KB.
  final int maxBodySize;

  /// Optional filter predicate. When provided, only requests for which
  /// [filter] returns `true` are logged.
  final FlogHttpFilter? filter;

  /// Maps `requestOptions.hashCode` to the assigned flog_net request ID.
  final Map<int, int> _idMap = {};

  /// Maps `requestOptions.hashCode` to the request start timestamp.
  final Map<int, DateTime> _startMap = {};

  /// Creates a [FlogHttpInterceptor].
  FlogHttpInterceptor({
    this.includeRequestHeaders = true,
    this.includeResponseHeaders = true,
    this.includeRequestBody = true,
    this.includeResponseBody = true,
    this.maxBodySize = 10 * 1024,
    this.filter,
  });

  @override
  void onRequest(RequestOptions options, RequestInterceptorHandler handler) {
    if (filter != null && !filter!(options)) {
      handler.next(options);
      return;
    }

    final id = nextNetId();
    final key = options.hashCode;
    _idMap[key] = id;
    _startMap[key] = DateTime.now();

    final data = <String, dynamic>{
      'id': id,
      't': 'req',
      'p': 'http',
      'method': options.method,
      'url': options.uri.toString(),
    };

    if (includeRequestHeaders) {
      data['headers'] = options.headers;
    }

    if (includeRequestBody && options.data != null) {
      data['body'] = _truncate(_encodeBody(options.data));
    }

    emitNet(data);
    handler.next(options);
  }

  @override
  void onResponse(Response<dynamic> response, ResponseInterceptorHandler handler) {
    final key = response.requestOptions.hashCode;
    final id = _idMap.remove(key);
    final start = _startMap.remove(key);

    if (id == null) {
      handler.next(response);
      return;
    }

    final duration = start != null
        ? DateTime.now().difference(start).inMilliseconds
        : null;

    final data = <String, dynamic>{
      'id': id,
      't': 'res',
      'p': 'http',
      'status': response.statusCode,
    };

    if (duration != null) {
      data['duration'] = duration;
    }

    if (includeResponseHeaders) {
      data['headers'] = response.headers.map;
    }

    if (includeResponseBody && response.data != null) {
      data['body'] = _truncate(_encodeBody(response.data));
    }

    emitNet(data);
    handler.next(response);
  }

  @override
  void onError(DioException err, ErrorInterceptorHandler handler) {
    final key = err.requestOptions.hashCode;
    final id = _idMap.remove(key);
    final start = _startMap.remove(key);

    if (id == null) {
      handler.next(err);
      return;
    }

    final duration = start != null
        ? DateTime.now().difference(start).inMilliseconds
        : null;

    final data = <String, dynamic>{
      'id': id,
      't': 'err',
      'p': 'http',
      'error': err.message ?? err.type.toString(),
    };

    if (duration != null) {
      data['duration'] = duration;
    }

    emitNet(data);
    handler.next(err);
  }

  String _encodeBody(dynamic body) {
    if (body is String) return body;
    if (body is Map || body is List) {
      try {
        return jsonEncode(body);
      } catch (_) {
        return body.toString();
      }
    }
    return body.toString();
  }

  String _truncate(String value) {
    if (value.length > maxBodySize) {
      return '${value.substring(0, maxBodySize)}... (truncated)';
    }
    return value;
  }
}
