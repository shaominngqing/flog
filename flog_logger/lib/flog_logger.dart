/// Lightweight structured logger for Flutter.
///
/// Sends structured log messages to flog TUI via Direct Socket.
///
/// ```dart
/// final log = FlogLogger('Network');
/// log.i('-> GET /api/users');
/// log.e('Connection failed', error: e, stackTrace: st);
/// ```
library flog_dart;

import 'src/flog_server.dart';
import 'src/flog_net.dart' show flogEnabled;

export 'src/flog_net.dart' show nextNetId, emitNet, flogEnabled;
export 'src/flog_server.dart' show FlogServer;
export 'src/flog_http_interceptor.dart';
export 'src/flog_mock_interceptor.dart';
export 'src/flog_sse_parser.dart';
export 'src/flog_web_socket.dart';
export 'src/flog_dio.dart' show FlogDio, FlogHttpConfig, SseResponse;

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
