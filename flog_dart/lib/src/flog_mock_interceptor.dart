import 'dart:convert';
import 'package:dio/dio.dart';

import 'flog_net.dart' show flogEnabled;

/// The `options.extra` key FlogMockInterceptor sets to mark a request as
/// mocked. FlogHttpInterceptor reads it back to emit `mocked: true` on
/// the response log. Exposed as a single const so the magic string lives
/// in one place (DART-014).
const String kFlogMockedExtrasKey = 'flog_mocked';

/// A single mock rule received from flog via VM Service extension.
class FlogMockRule {
  /// Substring pattern matched against the request URL.
  final String urlPattern;

  /// Optional HTTP method filter (e.g. "GET", "POST").
  final String? method;

  /// HTTP status code to return.
  final int statusCode;

  /// Response body to return.
  final String responseBody;

  /// Optional delay in milliseconds before returning the response.
  final int delayMs;

  /// Whether this rule is active.
  final bool enabled;

  FlogMockRule({
    required this.urlPattern,
    this.method,
    required this.statusCode,
    required this.responseBody,
    this.delayMs = 0,
    required this.enabled,
  });

  factory FlogMockRule.fromJson(Map<String, dynamic> json) {
    return FlogMockRule(
      urlPattern: json['url_pattern'] ?? '',
      method: json['method'],
      statusCode: json['status_code'] ?? 200,
      responseBody: json['response_body'] ?? '{}',
      delayMs: json['delay_ms'] ?? 0,
      enabled: json['enabled'] ?? true,
    );
  }
}

/// Dio interceptor that intercepts requests matching mock rules from flog.
///
/// Rules are synced from the flog TUI over the WebSocket control channel:
/// the TUI sends a `{"type":"mock_sync","rules":"<json array>"}` frame and
/// `FlogServer._onMessage` calls [updateRules]. There is no
/// `ext.flog.syncMockRules` VM Service extension — earlier versions
/// documented one, but the production channel has always been WebSocket.
///
/// When a request matches an enabled rule the interceptor resolves with a
/// canned response instead of hitting the network. Match semantics, in
/// order:
///
/// 1. URL match is **substring-based** (`url.contains(rule.urlPattern)`).
///    No glob / regex / exact-match variants.
/// 2. URL match is **case-sensitive**.
/// 3. If a rule has a non-null `method`, it is compared case-insensitively.
/// 4. **First matching rule wins**. Later rules for the same URL are dead.
///
/// ### Rule-list scope (DART-012 ack)
///
/// Rules live on `FlogMockInterceptor._rules` as process-wide static state,
/// mirroring the single-TUI-drives-all-apps sync channel: the flog TUI
/// broadcasts one rule table and expects every FlogDio in the same isolate
/// to obey. Per-Dio scoping would split the rule table and break that
/// contract, so the global store is deliberate. Tests that want isolation
/// should call `updateRules([])` in setUp/tearDown.
class FlogMockInterceptor extends Interceptor {
  static List<FlogMockRule> _rules = [];

  /// Update the current set of mock rules (called by the VM Service extension handler).
  static void updateRules(List<FlogMockRule> rules) {
    _rules = rules;
  }

  @override
  void onRequest(RequestOptions options, RequestInterceptorHandler handler) {
    // Guard: in AOT-release `flogEnabled` is a `const false`, so the whole
    // rule loop is tree-shaken. Manual wiring onto a plain Dio also short-
    // circuits here. Mirrors FlogHttpInterceptor.onRequest.
    if (!flogEnabled) {
      handler.next(options);
      return;
    }

    final url = options.uri.toString();
    final method = options.method;

    for (final rule in _rules) {
      if (!rule.enabled) continue;
      if (!url.contains(rule.urlPattern)) continue;
      if (rule.method != null &&
          rule.method!.toLowerCase() != method.toLowerCase()) {
        continue;
      }

      // Mark this request as mocked so FlogHttpInterceptor can flag it.
      // Key name is shared with flog_http_interceptor.dart (DART-014).
      options.extra[kFlogMockedExtrasKey] = true;

      final response = Response(
        requestOptions: options,
        statusCode: rule.statusCode,
        data: _tryParseJson(rule.responseBody),
      );

      // Use callFollowing: true so subsequent interceptors (like ApiResponseInterceptor)
      // still process the response (e.g. unwrap {code, message, result} envelope)
      if (rule.delayMs > 0) {
        Future.delayed(Duration(milliseconds: rule.delayMs), () {
          handler.resolve(response, true);
        });
        return;
      }

      handler.resolve(response, true);
      return;
    }

    handler.next(options);
  }

  static dynamic _tryParseJson(String body) {
    try {
      return jsonDecode(body);
    } catch (_) {
      return body;
    }
  }
}
