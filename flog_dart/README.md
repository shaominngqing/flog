# flog_dart

Flutter companion for [flog](https://github.com/shaominngqing/flog) — a
terminal-native log viewer + network inspector.

`flog_dart` runs a WebSocket server inside your Flutter app on port
9753 (or the first free port in `[9753, 9762]`). When you open the
`flog` TUI on your dev machine, it connects to the app and streams
logs, HTTP / SSE / WebSocket traffic, and synchronises mock rules.

Everything is compile-time-disabled in release builds — AOT tree-shakes
the whole package out of the binary.

## Installation

`flog_dart` is debug-only. Put it under `dev_dependencies` to keep it
out of release dependency resolution:

```yaml
dev_dependencies:
  flog_dart: ^0.7.2
```

If you need to reference types from `flog_dart` in your own code
(e.g. a subclass of `FlogLogger`), use a regular `dependencies:` entry;
`flogEnabled` still tree-shakes the implementation out in release builds.

## Quick start

One line in `main()`:

```dart
import 'package:flog_dart/flog_dart.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  Flog.init();
  runApp(MyApp());
}
```

`Flog.init({int port = 9753})` is synchronous and zero-blocking. It:

1. Starts `FlogServer` on the first free port in `[port, port+9]`.
2. Registers hooks for `debugPrint`, `FlutterError.onError`, and
   `PlatformDispatcher.onError` so every framework log and crash ends
   up in flog.
3. Fetches app name / version / package name via `package_info_plus`
   in the background and publishes it to any connected TUI.

## Logging

```dart
final log = FlogLogger('Network');
log.i('-> GET /api/users');
log.e('Connection failed', error: e, stackTrace: st);
```

Shorthands: `v` / `d` / `i` / `w` / `e`. Full-word equivalents:
`verbose` / `debug` / `info` / `warning` / `error`. Pass `error:` +
`stackTrace:` on `debug` / `warning` / `error` when you have them.

Set `FlogLogger.printToConsole = true` to also echo every log to the
Flutter debug console — handy when you're not attached to the flog TUI
but still want to see what `flog_dart` would have sent.

## Network inspector — `FlogDio`

Drop-in `Dio` replacement:

```dart
import 'package:flog_dart/flog_dart.dart';

final dio = FlogDio(baseUrl: 'https://api.example.com');
final response = await dio.get('/users');
```

`FlogDio` inserts two interceptors at the front of the chain:

1. `FlogMockInterceptor` — checks the mock rules synced from the flog
   TUI. If a rule matches, it resolves the request locally with the
   canned response; flog still sees it and tags it `Mocked`.
2. `FlogHttpInterceptor` — records the request, response, error,
   headers, and bodies (subject to `FlogHttpConfig`).

Add your own interceptors after `FlogDio` construction with the usual
`dio.interceptors.add(...)`. They run **after** flog's pair, so
business-layer interceptors cannot hide responses from flog.

### Configuration

```dart
final dio = FlogDio(
  baseUrl: 'https://api.example.com',
  flogConfig: FlogHttpConfig(
    includeRequestHeaders: true,
    includeResponseHeaders: true,
    includeRequestBody: true,
    includeResponseBody: true,
    maxBodySize: 10 * 1024 * 1024,
    filter: (options) => !options.path.startsWith('/health'), // exclude noisy endpoints
  ),
);
```

### Manual integration (without `FlogDio`)

If you can't switch to `FlogDio`, add the interceptor manually:

```dart
final dio = Dio();
dio.interceptors.addAll([
  FlogHttpInterceptor(),        // MUST be first
  ApiResponseInterceptor(),
  LoggingInterceptor(),
]);
```

`FlogHttpInterceptor` must run before any interceptor that might
`handler.reject()` the response — otherwise flog never sees the
response and the request stays `Pending` forever.

## SSE

Via `FlogDio.sse`:

```dart
final response = await dio.sse('/chat/completions',
  method: 'POST',
  data: {'model': 'gpt-4', 'messages': [...]},
);
await for (final data in response.stream) {
  print(data); // raw data frames as joined strings
}
```

The returned `SseResponse` exposes a `Stream<String>` of joined-`data:`
payloads per event.

For typed access including `event:` / `id:` / `retry:` fields, use
`FlogSseParser.wrapTyped`:

```dart
final events = FlogSseParser.wrapTyped(
  response.data!.stream,
  url: '/chat/completions',
  method: 'POST',
);
await for (final SseEvent e in events) {
  // e.data, e.event, e.id, e.retry, e.comments
}
```

`SseEvent` models a single W3C EventSource event: `event` (type, or
`null` for the default `message`), `data` (multi-line `data:` lines
joined with `\n`), `id`, `retry` (ms), and any preceding `:` comments.

Or against a raw byte stream with `FlogSseParser.wrap` (returns only
the `data` payload per event):

```dart
await for (final data in FlogSseParser.wrap(
  response.data!.stream,
  url: '/api/chat/completions',
  method: 'POST',
)) {
  final json = jsonDecode(data);
  // ...
}
```

## WebSocket

`FlogWebSocket` wraps `web_socket_channel`:

```dart
final ws = await FlogWebSocket.connect('wss://example.com/ws');
ws.send(jsonEncode({'type': 'hello'}));
ws.stream.listen((data) => print(data)); // broadcast stream (DART-006)
await ws.close();
```

Every `send`, every inbound frame, and the eventual close event go to
flog. Binary frames are forwarded as `"<binary: N bytes>"` (see
[PROTOCOL §4.3](../docs/PROTOCOL.md)).

## Mocking

Mock rules are authored in the flog TUI (Network tab → `M` to open the
mock rules panel, or `M` on a selected request to fork one from it).
Each rule has a URL pattern (substring match), optional method filter,
status code, response body, optional delay, and enabled flag.

When you create or toggle a rule the TUI sends it to the running app
over the WebSocket control channel (`{"type":"mock_sync","rules":"…"}`).
`FlogMockInterceptor` updates its in-memory rules and begins
intercepting matching requests. Matched requests are still logged by
`FlogHttpInterceptor` and appear in the inspector tagged `Mocked`.

Matching semantics: substring on the full URL, first-match-wins,
case-sensitive. Optional `method` filter applies when set.

## Replay

In the flog TUI, selecting a captured HTTP request in the Network tab
and pressing `r` sends a `{"type":"replay"}` frame to the app. The
Dart side re-executes the request through the same `Dio` instance, so
mock rules still apply. The new request is logged with
`EntrySource::Replay`.

## `flogEnabled` tree-shaking

`flogEnabled` is a compile-time constant:

| Condition                                 | `flogEnabled` |
|-------------------------------------------|---------------|
| Default debug build                       | `true`        |
| Default release build (`dart.vm.product`) | `false`       |
| `--dart-define=APP_FLAVOR=release`        | `false`       |
| `--dart-define=APP_FLAVOR=<anything else>`| `true`        |
| `--dart-define=FLOG_ENABLED=true`         | `true`        |
| `--dart-define=FLOG_ENABLED=false`        | `false`       |

When `flogEnabled == false`, every entry point in `flog_dart` is
`if (!flogEnabled) return;` at the top, so AOT removes the whole
package from the final binary.

## Tests

```bash
cd flog_dart
dart test
dart analyze
```

The `flog_dart/test/` suite includes the W3C-compliant SSE parser
contract (DART-001/002 regression tests) — they exercise `FlogSseParser`
directly against realistic SSE streams.

## v0.8 breaking changes (planned)

v0.8 will reshape the SSE subsystem into a clean
`StreamTransformer<List<int>, SseEvent>`, expose the raw stream
alongside the typed events, and add byte-buffer limits (audit
DART-033). **Wire protocol stays unchanged** — flog TUI 0.4.x will
keep working with both v0.7.x and v0.8.x. Migration will only touch
Dart-side SSE call sites.

See [`docs/PROTOCOL.md §9.1`](../docs/PROTOCOL.md#91-flog_dart-v08-breaking-changes-dart-033-forward-ref)
in the flog repository for detail.

## License

MIT.
