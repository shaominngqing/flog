import 'package:flog_logger/flog_logger.dart';

// One logger per module — tag is fixed at creation.
final log = FlogLogger('Network');

void main() {
  // Single-letter shorthand (like the `logger` package)
  log.i('-> GET /api/users');
  log.d('Response: 200 OK (128ms)');
  log.w('Retry #2 after timeout');
  log.e('Connection refused', error: 'SocketException');

  // Full-word methods work too (like talker / loggy)
  final auth = FlogLogger('Auth');
  auth.info('Login succeeded');
  auth.warning('Token expires in 5 min');
  auth.error('Refresh failed', error: 'Unauthorized');
}
