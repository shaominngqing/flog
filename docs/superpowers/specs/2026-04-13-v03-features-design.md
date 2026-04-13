# flog v0.3 Feature Design

Date: 2026-04-13

## Overview

Five features for flog v0.3, in priority order:

1. **P0** — Release zero-code (tree-shaking)
2. **P0** — FlogDio wrapper
3. **P1** — Request Replay
4. **P1** — Performance marks + Stats panel
5. **P2** — Mock / Local proxy

---

## 1. Release Zero-Code (Tree-Shaking)

### Goal

Release build produces zero flog_dart runtime code. No overhead, no log output, no JSON serialization — as if the package didn't exist.

### Mechanism

Dart's `const bool.fromEnvironment` + AOT tree-shaking:

```dart
const _enabled = bool.fromEnvironment(
  'FLOG_ENABLED',
  defaultValue: !bool.fromEnvironment('dart.vm.product'),
);
```

- `dart.vm.product` is `true` in release → `_enabled = false`
- debug/profile builds → `_enabled = true` (automatic)
- User override: `flutter run --dart-define=FLOG_ENABLED=true`

### Changes

| File | Change |
|------|--------|
| `flog_net.dart` | `emitNet()`: early return if `!_enabled` |
| `flog_logger.dart` | `_log()`: early return if `!_enabled` |
| `FlogHttpInterceptor` | `onRequest/onResponse/onError`: `handler.next()` immediately if `!_enabled` |
| `FlogSseParser` | `.wrap()`: return pass-through stream (no emit) if `!_enabled` |
| `FlogWebSocket` | Delegate only, no emit, if `!_enabled` |

The `_enabled` constant is defined once in `flog_net.dart` and imported everywhere.

### Effect

Dart AOT compiler sees `_enabled` as `const false` in release → all `if (!_enabled) return` branches inline to unconditional return → JSON serialization code tree-shaken away.

---

## 2. FlogDio Wrapper

### Goal

One-line Dio replacement with automatic HTTP interception + SSE convenience method. Users never need to know about `FlogHttpInterceptor` or `FlogSseParser`.

### API

```dart
// Replace Dio() with FlogDio()
final dio = FlogDio(
  baseUrl: 'https://api.example.com',
  // All Dio parameters pass through
);

// HTTP — transparent, works like Dio
final response = await dio.get('/api/users');

// SSE — convenience method
final sse = await dio.sse('/api/chat/stream',
  method: 'POST',
  data: {'prompt': 'hello'},
  options: Options(receiveTimeout: Duration(seconds: 60)),
);

print(sse.headers);       // access response headers
print(sse.statusCode);    // access status code

await for (final event in sse.stream) {
  final json = jsonDecode(event);
  // process
}
```

### Design

**FlogDio extends Dio:**

- Constructor auto-inserts `FlogHttpInterceptor` at position 0
- Custom interceptor config via optional `flogConfig` parameter: `FlogDio(flogConfig: FlogHttpConfig(includeRequestBody: false))`
- Release mode (`!_enabled`): no interceptor inserted, `.sse()` returns raw stream

**SseResponse class:**

```dart
class SseResponse {
  final Headers headers;
  final int? statusCode;
  final Stream<String> stream;
}
```

Consistent with Dio's `Response` mental model — user gets a response object with a stream inside.

**`.sse()` method internals:**

1. Call `this.request(path, options: Options(responseType: ResponseType.stream))`
2. Wrap response stream with `FlogSseParser.wrap()`
3. Return `SseResponse` with headers, statusCode, and wrapped stream

**WebSocket:** Not included. `FlogWebSocket` remains independent (WS has nothing to do with Dio).

### New File

`flog_logger/lib/src/flog_dio.dart` — ~80 lines.

Export from `flog_logger.dart`.

---

## 3. Request Replay

### Goal

Replay any HTTP request directly from flog TUI. Rust-side sends the request using reqwest.

### Interaction

- **Shortcut:** `r` on selected HTTP request
- **Button:** `Replay` in Network status bar
- Only works for HTTP protocol (not SSE/WS)

### Implementation (Rust)

- Add `reqwest` crate dependency (async HTTP client)
- On `r` key / Replay button click:
  1. Extract method, url, headers, body from selected `NetworkEntry`
  2. `tokio::spawn` async request — does not block UI
  3. Status bar shows "Replaying..." during request
  4. Result inserted into `NetworkStore` as new `NetworkEntry`
  5. Success: status bar shows "Replay: 200 OK (42ms)"
  6. Failure: status bar shows error message

### Visual Distinction

Replay entries in the network list have a **light background tint** (Catppuccin Surface0 or Surface1) to distinguish from normal requests. The method column shows the original method (GET, POST, etc.) and a small replay indicator.

### Data Model Change

Add `source: EntrySource` field to `NetworkEntry`:

```rust
pub enum EntrySource {
    App,      // Normal request from Dart app
    Replay,   // Replayed from flog
    Mocked,   // Returned mock data via proxy
}
```

Default is `App`. Replay sets `Replay`. Proxy mock-hit sets `Mocked`. UI uses this to apply background tint.

### Limitations

- Request goes directly from Rust, bypasses Dart interceptor chain
- Headers copied as-is from original request (tokens may be expired → 401 is expected)

---

## 4. Performance Marks + Stats Panel

### Goal

Slow request highlighting in the network list + a statistics panel with aggregated metrics.

### Slow Request Highlighting

Duration column color based on elapsed time (Completed requests only):

| Duration | Color |
|----------|-------|
| < 500ms | Green (current) |
| 500ms – 1s | Yellow (Catppuccin Yellow) |
| > 1s | Red (Catppuccin Red) |

Pending/Active requests keep their current styling.

### Network Stats Panel

- **Shortcut:** `S` (uppercase, consistent with Logs tab Stats)
- **Style:** Full-screen overlay, same pattern as Logs Stats / Help overlay
- **Layout:** Uses ratatui `Table` widget with polished Catppuccin styling

**Sections:**

1. **Summary Row**
   - Total requests / Success / Failed / In-progress

2. **Latency Table**
   - Average, P50, P95, P99
   - Styled table with header row

3. **Slowest Requests (Top 5)**
   - Table: Rank | URL (truncated) | Method | Status | Duration
   - Clickable rows → close stats and jump to that request in the list

4. **Status Code Distribution**
   - Table: Status Range | Count | Percentage
   - 2xx, 3xx, 4xx, 5xx groupings

5. **Per-Domain Stats**
   - Table: Domain | Request Count | Avg Duration | Error Rate
   - Sorted by request count descending

---

## 5. Mock / Local Proxy

### Goal

flog runs a local HTTP proxy. Matching requests return mock data; non-matching requests are forwarded transparently. Zero configuration on the Dart side when using FlogDio.

### Architecture

```
Dart App (FlogDio, baseUrl: https://api.example.com)
    ↓ (proxy configured via VM Service Extension)
flog proxy (localhost:<port>)
    ├─ matches mock rule → return mock response
    └─ no match → forward to real server → return real response
```

### Proxy Lifecycle

1. flog starts → binds local HTTP proxy on port 9999
2. Port 9999 occupied → try 10000, 10001... (up to 10 attempts)
3. flog connects to VM Service (existing flow)
4. flog calls `ext.flog.setProxy` service extension with actual port
5. FlogDio receives callback → configures Dio `httpClientAdapter` to use proxy
6. All HTTP requests now flow through flog proxy
7. flog exits → proxy gone → FlogDio detects failure → auto fallback to direct connection + warning log

### Dart Side (FlogDio)

```dart
// In FlogDio constructor (debug mode only):
developer.registerExtension('ext.flog.setProxy', (method, params) async {
  final port = params['port'];
  // Configure Dio httpClientAdapter to use localhost:port as proxy
  return ServiceExtensionResponse.result('{}');
});
```

No user code changes. FlogDio handles everything internally.

### Rust Side

- `hyper` crate for local HTTP server (proxy)
- Incoming request → check mock rules → hit: return mock → miss: `reqwest` forward to real server
- All proxied requests automatically recorded in `NetworkStore`

### UI — Adaptive by Connection Mode

**VM Service mode:** Full Mock UI visible
**ADB / stdin mode:** All Mock UI hidden (buttons, shortcuts, status indicator — none shown)

### Proxy Status Indicator

Displayed in Network tab toolbar, right-aligned:

```
⇄ Network                                    ● Proxy :9999 (2 rules)
```

States:
- `● Proxy :9999 (2 rules)` — running, green dot, shows port + rule count
- `● Proxy :9999 (no rules)` — running, no rules configured, gray dot
- `○ Proxy :9999 connecting...` — waiting for Dart to connect, yellow dot
- `● Proxy :9999 active` — Dart connected, no rules yet, blue dot

### Mock Rule Management

**Create mock from request:**
- Select a request → `M` key or `Mock` status bar button
- Opens mock editor overlay, pre-filled with:
  - URL pattern (exact match of current URL)
  - HTTP method
  - Original response status code + body
  - Optional: response delay (simulate latency)
- `Enter` to save, `Esc` to cancel

**Mock rule list:**
- `Ctrl+M` opens full-screen rule list overlay
- Table: URL Pattern | Method | Status | Delay | Hit Count | Enabled
- Actions per rule:
  - `Space` — toggle enabled/disabled
  - `d` — delete rule
  - `Enter` — edit rule

### Visual Distinction

Mock-hit requests in the network list have **Mauve purple light background**.

Detail panel General section shows `Mocked: Yes` + which rule matched.

### Limitations

- HTTP only (SSE/WS do not go through proxy)
- HTTP proxy only, no HTTPS interception (no self-signed cert complexity)
- Mock only available in VM Service connection mode

---

## Feature Matrix by Connection Mode

| Feature | Shortcut | VM Service | ADB | stdin |
|---------|----------|-----------|-----|-------|
| Network monitoring | — | Yes | Yes | Yes |
| Replay | `r` / button | Yes | Yes | Yes |
| Performance marks | auto | Yes | Yes | Yes |
| Network Stats | `S` | Yes | Yes | Yes |
| Mock | `M` / `Ctrl+M` | Yes | Hidden | Hidden |
| Proxy status | — | Shown | Hidden | Hidden |

## Status Bar Buttons (Network Tab)

**VM Service mode:**
```
 Replay  Mock  curl  Copy Response  Clear                           ? 
```

**ADB / stdin mode:**
```
 Replay  curl  Copy Response  Clear                                 ? 
```

---

## New Dependencies

### Dart (flog_dart)
- No new dependencies (dart:developer is SDK built-in)

### Rust (flog)
- `reqwest` — async HTTP client (for Replay + proxy forwarding)
- `hyper` — HTTP server (for proxy)

---

## New Files

### Dart
- `flog_logger/lib/src/flog_dio.dart` — FlogDio class + SseResponse

### Rust (estimated)
- `src/proxy/mod.rs` — Proxy server + mock rule engine
- `src/proxy/mock.rs` — Mock rule types and matching
- `src/ui/network/stats.rs` — Network statistics panel
- `src/replay.rs` — Replay request logic
