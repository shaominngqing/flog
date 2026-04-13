/// Lightweight structured logger for Flutter.
///
/// Outputs `[LEVEL][Tag] message` format that
/// [flog](https://github.com/shaomingqing/flog) parses natively.
///
/// ```dart
/// final log = FlogLogger('Network');
/// log.i('-> GET /api/users');
/// log.e('Connection failed', error: e, stackTrace: st);
/// ```
library flog_dart;

import 'src/flog_net.dart' show flogEnabled;

export 'src/flog_net.dart' show nextNetId, emitNet, flogEnabled;
export 'src/flog_http_interceptor.dart';
export 'src/flog_sse_parser.dart';
export 'src/flog_web_socket.dart';

class FlogLogger {
  /// The tag used to identify the source of log messages.
  final String tag;

  /// Creates a logger with the given [tag].
  ///
  /// Typically one instance per module or class:
  /// ```dart
  /// final log = FlogLogger('Network');
  /// ```
  const FlogLogger(this.tag);

  // ---------------------------------------------------------------------------
  // Full-word methods (talker / loggy style)
  // ---------------------------------------------------------------------------

  void verbose(String msg) => _log('VERBOSE', msg);

  void debug(String msg, {Object? error, StackTrace? stackTrace}) =>
      _log('DEBUG', msg, error: error, stackTrace: stackTrace);

  void info(String msg) => _log('INFO', msg);

  void warning(String msg, {Object? error, StackTrace? stackTrace}) =>
      _log('WARNING', msg, error: error, stackTrace: stackTrace);

  void error(String msg, {Object? error, StackTrace? stackTrace}) =>
      _log('ERROR', msg, error: error, stackTrace: stackTrace);

  // ---------------------------------------------------------------------------
  // Single-letter shorthand (logger style)
  // ---------------------------------------------------------------------------

  void v(String msg) => verbose(msg);

  void d(String msg, {Object? error, StackTrace? stackTrace}) =>
      debug(msg, error: error, stackTrace: stackTrace);

  void i(String msg) => info(msg);

  void w(String msg, {Object? error, StackTrace? stackTrace}) =>
      warning(msg, error: error, stackTrace: stackTrace);

  void e(String msg, {Object? error, StackTrace? stackTrace}) =>
      _log('ERROR', msg, error: error, stackTrace: stackTrace);

  // ---------------------------------------------------------------------------
  // Internal
  // ---------------------------------------------------------------------------

  void _log(String level, String msg, {Object? error, StackTrace? stackTrace}) {
    if (!flogEnabled) return;
    // ignore: avoid_print
    print('[$level][$tag] $msg');
    if (error != null) {
      // ignore: avoid_print
      print('[$level][$tag] Error: $error');
    }
    if (stackTrace != null) {
      // ignore: avoid_print
      print('[$level][$tag] $stackTrace');
    }
  }
}
