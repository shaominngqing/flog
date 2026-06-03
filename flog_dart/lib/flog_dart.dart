/// Lightweight structured logger and network inspector bridge for Flutter.
///
/// Sends structured log messages and HTTP/SSE/WebSocket traffic metadata to
/// the flog TUI via a Direct Socket WebSocket server. The authoritative entry
/// point is [Flog.init]; see its dartdoc for the canonical bootstrap example.
library flog_dart;

import 'dart:async';

import 'package:flutter/foundation.dart' show debugPrint;
import 'package:flutter/widgets.dart'
    show AppLifecycleState, WidgetsBinding, WidgetsBindingObserver;
import 'package:package_info_plus/package_info_plus.dart';

import 'src/flog_server.dart';
import 'src/flog_net.dart' show flogEnabled;

export 'src/flog_server.dart' show FlogServer;
export 'src/flog_store.dart' show FlogStore;
export 'src/flog_http_interceptor.dart';
export 'src/flog_mock_interceptor.dart';
export 'src/flog_sse_parser.dart';
export 'src/flog_web_socket.dart';
export 'src/flog_dio.dart' show FlogDio, FlogHttpConfig, SseResponse;
// DART-033 v0.8 — three composable SSE StreamTransformers. Users can
// build their own pipeline, swap the reporter for a custom one, or
// bypass the FlogSseParser shim entirely.
export 'src/sse/byte_decoder.dart' show SseByteDecoder;
export 'src/sse/event.dart' show SseEvent;
export 'src/sse/line_decoder.dart' show SseLineDecoder;
export 'src/sse/reporter.dart' show FlogSseReporter;

/// Top-level entry point for flog_dart.
///
/// ```dart
/// void main() {
///   WidgetsFlutterBinding.ensureInitialized();
///   Flog.init();
///   runApp(MyApp());
/// }
/// ```
class Flog {
  Flog._();

  /// Initialize flog_dart. Call once, as early as possible.
  ///
  /// Synchronous — does not block app startup. App info (name, version,
  /// package) is auto-detected in the background via [PackageInfo].
  static void init({int port = 9753}) {
    if (!flogEnabled) return;

    // Start server and register hooks immediately (synchronous, zero delay).
    FlogServer.instance.start(port: port);
    _FlogLifecycleRestarter.instance.install(port: port);

    // Auto-detect app info in the background — updates before any TUI connects.
    PackageInfo.fromPlatform().then((info) {
      FlogServer.instance.updateAppInfo(
        appName: info.appName,
        appVersion: info.version,
        packageName: info.packageName,
      );
    }).catchError((Object e, StackTrace st) {
      // DART-023: previously swallowed silently, leaving the TUI stuck on
      // the placeholder `app='flutter'` with no diagnostic. Log to the
      // Flutter debug log so the developer at least sees why. (In release
      // builds flogEnabled is false and this branch is never reached, so
      // debugPrint does not leak into production output.)
      debugPrint('flog_dart: PackageInfo.fromPlatform failed: $e');
    });
  }
}

class _FlogLifecycleRestarter with WidgetsBindingObserver {
  static final _FlogLifecycleRestarter instance = _FlogLifecycleRestarter._();
  _FlogLifecycleRestarter._();

  bool _installed = false;
  int _port = 9753;

  void install({required int port}) {
    _port = port;
    if (_installed) return;
    try {
      WidgetsBinding.instance.addObserver(this);
      _installed = true;
    } catch (_) {
      // Preserve Flog.init's historical "callable before binding" behavior.
      // Real Flutter apps call WidgetsFlutterBinding.ensureInitialized()
      // before Flog.init, so they still get lifecycle recovery.
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state != AppLifecycleState.resumed) return;
    unawaited(FlogServer.instance.restart(port: _port));
  }
}

class FlogLogger {
  /// The tag used to identify the source of log messages.
  final String tag;

  /// Enable printing log messages to Flutter console (for debugging).
  /// Default is false — logs only go to flog TUI via socket.
  static bool printToConsole = false;

  /// Creates a logger with the given [tag].
  const FlogLogger(this.tag);

  // ---------------------------------------------------------------------------
  // Full-word methods
  // ---------------------------------------------------------------------------

  void verbose(String msg) => _log('verbose', msg);

  void debug(String msg, {Object? error, StackTrace? stackTrace}) =>
      _log('debug', msg, error: error, stackTrace: stackTrace);

  void info(String msg) => _log('info', msg);

  void warning(String msg, {Object? error, StackTrace? stackTrace}) =>
      _log('warning', msg, error: error, stackTrace: stackTrace);

  void error(String msg, {Object? error, StackTrace? stackTrace}) =>
      _log('error', msg, error: error, stackTrace: stackTrace);

  // ---------------------------------------------------------------------------
  // Single-letter shorthand
  // ---------------------------------------------------------------------------

  void v(String msg) => verbose(msg);

  void d(String msg, {Object? error, StackTrace? stackTrace}) =>
      debug(msg, error: error, stackTrace: stackTrace);

  void i(String msg) => info(msg);

  void w(String msg, {Object? error, StackTrace? stackTrace}) =>
      warning(msg, error: error, stackTrace: stackTrace);

  void e(String msg, {Object? error, StackTrace? stackTrace}) =>
      _log('error', msg, error: error, stackTrace: stackTrace);

  // ---------------------------------------------------------------------------
  // Internal
  // ---------------------------------------------------------------------------

  void _log(String level, String msg, {Object? error, StackTrace? stackTrace}) {
    if (!flogEnabled) return;
    FlogServer.instance.send({
      'type': 'log',
      'level': level,
      'tag': tag,
      'message': msg,
      'error': error?.toString(),
      'stackTrace': stackTrace?.toString(),
      'timestamp': DateTime.now().millisecondsSinceEpoch,
    });
    if (printToConsole) {
      final upperLevel = level.toUpperCase();
      // ignore: avoid_print
      print('[$upperLevel][$tag] $msg');
      if (error != null) {
        // ignore: avoid_print
        print('[$upperLevel][$tag] Error: $error');
      }
      if (stackTrace != null) {
        // ignore: avoid_print
        print('[$upperLevel][$tag] $stackTrace');
      }
    }
  }
}
