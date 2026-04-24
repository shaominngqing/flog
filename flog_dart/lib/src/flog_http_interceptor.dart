import 'dart:convert';

import 'package:dio/dio.dart';

import 'flog_net.dart' show flogEnabled, nextNetId, emitNet;
import 'flog_mock_interceptor.dart' show kFlogMockedExtrasKey;

/// Predicate to decide whether a request should be logged.
typedef FlogHttpFilter = bool Function(RequestOptions options);

/// Dio interceptor that emits flog_net protocol messages for HTTP traffic.
///
/// **Important:** Add this interceptor **before** any interceptor that modifies
/// or rejects responses (e.g., an envelope-unwrapping interceptor that calls
/// `handler.reject()` on business errors). Otherwise, `FlogHttpInterceptor`
/// won't see the original response and failed requests will appear stuck
/// in "Pending" state in flog's Network Inspector.
///
/// ```dart
/// final dio = Dio();
/// dio.interceptors.addAll([
///   FlogHttpInterceptor(),        // ← must be before response interceptors
///   ApiResponseInterceptor(),     // envelope unwrap, may reject
///   LoggingInterceptor(),
/// ]);
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

  // DART-008: Previously we kept two hashCode-keyed maps
  // (_idMap/_startMap) and only freed entries in onResponse/onError. If a
  // downstream interceptor resolved or rejected a request before reaching
  // this interceptor's response phase, the entries leaked and the maps
  // grew unbounded. RequestOptions is also mutable (headers, redirects),
  // so hashCode was a fragile key.
  //
  // New strategy: stamp the assigned id + start timestamp directly onto
  // `options.extra` (using private keys). The state now lives on the
  // request object itself, gets GC'd when the request is discarded, and
  // is addressable without any side table.
  static const String _kIdExtraKey = '_flog_id';
  static const String _kStartExtraKey = '_flog_start_ms';

  /// Creates a [FlogHttpInterceptor].
  FlogHttpInterceptor({
    this.includeRequestHeaders = true,
    this.includeResponseHeaders = true,
    this.includeRequestBody = true,
    this.includeResponseBody = true,
    this.maxBodySize = 10 * 1024 * 1024,
    this.filter,
  });

  @override
  void onRequest(RequestOptions options, RequestInterceptorHandler handler) {
    if (!flogEnabled) {
      handler.next(options);
      return;
    }

    if (filter != null && !filter!(options)) {
      handler.next(options);
      return;
    }

    final id = nextNetId();
    options.extra[_kIdExtraKey] = id;
    options.extra[_kStartExtraKey] = DateTime.now().millisecondsSinceEpoch;

    final url = options.uri.toString();

    final data = <String, dynamic>{
      'id': id,
      't': 'req',
      'p': 'http',
      'method': options.method,
      'url': url,
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
  void onResponse(
      Response<dynamic> response, ResponseInterceptorHandler handler) {
    if (!flogEnabled) {
      handler.next(response);
      return;
    }

    // Handle mocked responses — FlogMockInterceptor runs first, resolves,
    // and stamps `kFlogMockedExtrasKey` on options.extra. Our onRequest is
    // therefore skipped, so no `_flog_id` is present on the response path.
    final isMocked =
        response.requestOptions.extra[kFlogMockedExtrasKey] == true;
    if (isMocked) {
      final id = nextNetId();
      _emitReq(id, response.requestOptions);
      _emitHttpCompletion(
        id: id,
        response: response,
        duration: 0,
        mocked: true,
      );
      handler.next(response);
      return;
    }

    final id = response.requestOptions.extra.remove(_kIdExtraKey) as int?;
    final startMs =
        response.requestOptions.extra.remove(_kStartExtraKey) as int?;

    if (id == null) {
      handler.next(response);
      return;
    }

    final duration = startMs != null
        ? DateTime.now().millisecondsSinceEpoch - startMs
        : null;
    _emitHttpCompletion(
      id: id,
      response: response,
      duration: duration,
      mocked: false,
    );
    handler.next(response);
  }

  /// Emit a `req` frame for a mocked request (normal requests already had
  /// their `req` emitted by [onRequest]).
  void _emitReq(int id, RequestOptions options) {
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
  }

  /// Emit the terminal `res` frame for a completed HTTP request.
  ///
  /// Single place to decide which fields ride on the response envelope
  /// (DART-027). Both the happy path (onResponse) and the mocked path
  /// share this helper so future additions (e.g. query-string policy,
  /// new truncation rules) cannot drift between the two.
  void _emitHttpCompletion({
    required int id,
    required Response<dynamic> response,
    required int? duration,
    required bool mocked,
  }) {
    final data = <String, dynamic>{
      'id': id,
      't': 'res',
      'p': 'http',
      'status': response.statusCode,
    };
    if (duration != null) {
      data['duration'] = duration;
    }
    if (includeResponseHeaders && !mocked) {
      // Mocked responses don't have real headers to expose.
      data['headers'] = response.headers.map;
    }
    if (includeResponseBody && response.data != null) {
      data['body'] = _truncate(_encodeBody(response.data));
    }
    if (mocked) {
      data['mocked'] = true;
    }
    emitNet(data);
  }

  @override
  void onError(DioException err, ErrorInterceptorHandler handler) {
    if (!flogEnabled) {
      handler.next(err);
      return;
    }

    final id = err.requestOptions.extra.remove(_kIdExtraKey) as int?;
    final startMs =
        err.requestOptions.extra.remove(_kStartExtraKey) as int?;

    if (id == null) {
      handler.next(err);
      return;
    }

    final duration = startMs != null
        ? DateTime.now().millisecondsSinceEpoch - startMs
        : null;

    final response = err.response;

    // When the server returned an actual HTTP response (4xx/5xx), emit it as
    // a normal response so flog shows the status code, headers, and body.
    if (response != null) {
      final data = <String, dynamic>{
        'id': id,
        't': 'res',
        'p': 'http',
        'status': response.statusCode,
        'error': err.message ?? err.type.toString(),
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
    } else {
      // No HTTP response at all (timeout, DNS failure, connection refused, etc.)
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
    }

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

  /// Truncate [value] to at most [maxBodySize] UTF-8 bytes.
  ///
  /// [String.length] is a UTF-16 code-unit count, not a byte count, so
  /// CJK-heavy payloads would undercount and ASCII-only payloads
  /// overcount when measured that way. Encode once, then slice at a safe
  /// UTF-8 boundary so we never emit a half-character.
  String _truncate(String value) {
    final bytes = utf8.encode(value);
    if (bytes.length <= maxBodySize) return value;
    // Walk back from the byte budget to the nearest character boundary.
    // UTF-8 continuation bytes are `10xxxxxx`, i.e. `(b & 0xC0) == 0x80`.
    int end = maxBodySize;
    while (end > 0 && (bytes[end] & 0xC0) == 0x80) {
      end--;
    }
    final head = utf8.decode(bytes.sublist(0, end), allowMalformed: true);
    return '$head... (truncated)';
  }
}
