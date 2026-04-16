# flog_dart

Flutter companion for [flog](https://github.com/shaominngqing/flog) terminal log viewer.

Structured logging, Network Inspector (HTTP/SSE/WS), mock interceptor, system log capture. Tree-shakes to zero in release builds.

## Quick Start

```dart
import 'package:flog_dart/flog_dart.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  Flog.init();  // One line — auto hook + server + app info detection
  runApp(MyApp());
}
```

## Network Inspector

```dart
// Replace Dio() with FlogDio() — automatic HTTP/SSE/WS logging + mock
final dio = FlogDio(baseUrl: 'https://api.example.com');
final response = await dio.get('/users');

// SSE streaming
final sse = await dio.sse('/chat/completions', method: 'POST', data: {...});
await for (final event in sse.stream) { ... }
```

## Logging

```dart
final log = FlogLogger('Network');
log.i('-> GET /api/users');
log.e('Connection failed', error: e, stackTrace: st);
```

## System Capture

`Flog.init()` automatically hooks:
- `debugPrint` — all Flutter framework debug output
- `FlutterError.onError` — build/layout/paint errors
- `PlatformDispatcher.onError` — unhandled async errors

## License

MIT
