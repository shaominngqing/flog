# Protocol

Wire protocol between flog (the Rust TUI) and flog_dart (the Dart
companion package). Authoritative sources:

- Rust side: `src/input/protocol.rs` (`ClientMessage`, `ServerMessage`,
  `ClientInfo`), `src/domain/network.rs` (`FlogNetKind`).
- Dart side: `flog_dart/lib/src/flog_server.dart`,
  `flog_dart/lib/src/flog_net.dart`, the Dio / SSE / WS interceptors.

Every JSON example below is either copied verbatim from
`src/input/protocol_tests.rs` or from the wire traffic produced by the
integration suite (`tests/ws_server_test_direct.rs` +
`tests/support/fake_flog_server.rs`).

## 1. Overview

WebSocket, base port `9753` (flog_dart binds the first free port in
`[9753, 9762]`; flog scans that same range). Every message is a single
text frame = one JSON object. Full duplex: `ClientMessage` = Dart →
flog (upstream); `ServerMessage` = flog → Dart (downstream). UTF-8
JSON, serde-generated. `ClientMessage` is internally tagged on
`"type"`; `FlogNetKind` (embedded inside `Net`) is internally tagged
on `"t"` — different tag keys so the two layers compose cleanly.

## 2. Connection lifecycle

```
flog                                          flog_dart
 │       TCP SYN + WS Upgrade                     │
 │ ──────────────────────────────────────────────▶│
 │◀─── ClientMessage::Hello  (within 3s) ─────────│
 │─── ServerMessage::MockSync (initial) ─────────▶│
 │◀── ClientMessage::Log / Net (streaming) ───────│
 │─── ServerMessage::Replay / MockSync / Subscribe (on user action) ▶│
 │◀── peer Close / drop ──────────────────────────│
```

### Hello handshake (3-second timeout)

The very first frame on every new connection must be a Hello from the
Dart side. flog waits at most 3 seconds; timeout and protocol
mismatches both result in the connection being dropped:

| Condition                                       | Error surfaced                                        |
|-------------------------------------------------|-------------------------------------------------------|
| No frame within 3 s                             | `"Hello handshake timed out after 3s (port may not be a flog server)"` |
| First frame is binary / close / ping            | `"Expected text frame, got binary"` (or similar)      |
| First frame is valid JSON but wrong variant     | `"Expected Hello, got Log"` / `"…Net"`                |
| First frame is text but not JSON                | `"Expected Hello, got unrecognized JSON"`             |
| Stream closes before any frame                  | `"Stream closed before Hello"`                        |

The 3-second bound exists so port-scan misfires fail fast without
blocking a port slot. Audit trail: TRANS-005.

### After Hello

The reader forwards each `ClientMessage` as `ConnectorEvent::Message`;
the writer pulls serialised `ServerMessage` strings off an mpsc channel
and writes them. Both tasks exit on peer close or I/O error; exit
reasons go to stderr for diagnosability. Audit trail: TRANS-006.

### Subscribe on session switch

When the user switches active app, flog clears local stores and sends
`ServerMessage::Subscribe`. Dart iterates its `FlogStore` and re-sends
every buffered message.

### Reconnect

Per-(device, port) exponential backoff: **2 s → 4 s → 8 s → 16 s →
30 s** (cap). A successful Hello resets the delay. Constants in
`src/run/server.rs`: `RECONNECT_INITIAL_DELAY_SECS`,
`RECONNECT_MAX_DELAY_SECS`, `RECONNECT_BACKOFF_FACTOR`. Audit trail:
TRANS-008.

## 3. ClientMessage (upstream)

Internally tagged on `"type"`. Three variants: `hello`, `log`, `net`.

### 3.1 `hello`

Sent exactly once per connection, immediately after WS upgrade.

Required: `app`, `os`. Everything else is `#[serde(default)]`, so
older clients still deserialize cleanly.

**Minimal:**

```json
{"type":"hello","app":"com.min","os":"android"}
```

**Full (current Dart 0.7.x):**

```json
{
  "type": "hello",
  "device": "Pixel 7",
  "app": "com.example",
  "appVersion": "2.3.4",
  "os": "android",
  "packageName": "com.example.pkg",
  "port": 9753,
  "buildMode": "debug"
}
```

**Legacy (pre-0.6.x Dart):**

```json
{"type":"hello","device":"iPhone 15","app":"com.test","appVersion":"1.0.0","os":"ios"}
```

**With session id (TRANS-014, Dart sends on app restart):**

```json
{"type":"hello","app":"com.x","os":"ios","sessionId":"abc-123"}
```

Fields are captured into `ClientInfo`; see §5.

Unknown extra JSON fields are **silently ignored** (no
`#[serde(deny_unknown_fields)]` — forward compat).

### 3.2 `log`

Sent for every `FlogLogger` call and for every captured
`debugPrint` / `FlutterError.onError` / `PlatformDispatcher.onError`
event. Required: `message`.

**Structured log:**

```json
{"type":"log","level":"info","tag":"Net","message":"hello"}
```

**With error + stack trace:**

```json
{
  "type": "log",
  "level": "error",
  "tag": "DB",
  "message": "fail",
  "error": "timeout",
  "stackTrace": "at main.dart:1",
  "timestamp": 1713100800000
}
```

**Raw `print()` capture (no level/tag):**

```json
{"type":"log","message":"[INFO][Network] → GET /api/scene-types","timestamp":1776324216539}
```

When `level` / `tag` are absent, flog's parser chain (see
[ARCHITECTURE.md §7](ARCHITECTURE.md)) infers them from the
`message` content.

`timestamp` is optional epoch milliseconds. When absent, flog stamps
the frame with its own wall-clock time on arrival.

### 3.3 `net`

Wraps a `FlogNetKind` variant via `#[serde(flatten)]`. The flattened
discriminator lives in `"t"` (to keep it disjoint from the outer
`"type"` tag).

**Request start (HTTP):**

```json
{
  "type": "net",
  "t": "req",
  "id": 42,
  "p": "http",
  "method": "POST",
  "url": "https://api.example.com/v1/x",
  "headers": {"Content-Type": "application/json"},
  "body": "{}",
  "ts": 1700000000000
}
```

**Response:**

```json
{"type":"net","t":"res","id":7,"status":200,"duration":99,"size":1024}
```

See §4 for the full list of `FlogNetKind` variants.

### 3.4 Malformed / rejected input

Any `ClientMessage` that fails serde deserialize is silently dropped —
no error frame is sent back. Failure modes covered by tests: unknown
`type`, missing required field, non-JSON body, unknown inner
`FlogNetKind` `"t"`. Future variants must land on the Rust side before
the Dart side sends them.

## 4. FlogNetKind (upstream, inside `Net`)

Internally tagged on `"t"`, `#[serde(rename_all = "lowercase")]`.
Every variant has `id: u64` and `#[serde(default)] ts: Option<u64>`
(epoch ms). Unused protocol fields are kept with `#[serde(default)]`
so forward-compatible Dart clients can ship extra fields without
breaking deserialization. `SseChunk` / `WsMessage` storage prunes
the unused wire fields per DOM-025 (see [ARCHITECTURE.md §6](ARCHITECTURE.md)).

| Variant | `"t"` | Applies to | Notes |
|---------|-------|-----------|-------|
| `Req`   | `"req"` | HTTP / SSE / WS start | `p` selects the protocol (`"http"`/`"sse"`/`"ws"`). |
| `Res`   | `"res"` | HTTP response         | Includes `mocked: Option<bool>` — if `true`, the entry is tagged `EntrySource::Mocked`. |
| `Err`   | `"err"` | HTTP transport error  | No full response captured. |
| `Chunk` | `"chunk"` | SSE stream data     | `data` is appended to the `NetworkEntry.sse_chunks`. |
| `Done`  | `"done"` | SSE stream end       | Finalises duration. |
| `Open`  | `"open"` | WebSocket open       |                           |
| `Send`  | `"send"` | WS outbound message  | `direction = Send`.       |
| `Recv`  | `"recv"` | WS inbound message   | `direction = Recv`.       |
| `Close` | `"close"` | WS close            | Carries close code + reason. |

### 4.1 `req` — request start

```json
{"type":"net","t":"req","id":1,"p":"http","method":"GET","url":"https://example.com/api","headers":{"Accept":"application/json"},"body":null,"size":0,"ts":1700000000000}
```

For SSE / WS, `p` is `"sse"` / `"ws"`; the entry enters `Active`
immediately (SSE starts on connect; WS stays open until `Close`).

### 4.2 `res` — HTTP response

```json
{"type":"net","t":"res","id":1,"status":200,"duration":42,"headers":{"Content-Type":"application/json"},"body":"{\"ok\":true}","size":11,"mocked":null,"ts":1700000000042}
```

A `Res` whose `id` has no matching `Req` surfaces as an orphan entry
(`method = "?"`, `url = "<orphan response>"`, status = `Orphan`) rather
than being dropped. Audit trail: DOM-003.

### 4.3 `err`, `chunk`, `done`, `open`, `send`, `recv`, `close`

```json
{"type":"net","t":"err","id":1,"error":"SocketException","duration":30000}
{"type":"net","t":"chunk","id":2,"data":"{\"delta\":\"Hel\"}","size":15,"seq":1,"ts":1700000001000}
{"type":"net","t":"done","id":2,"duration":5000,"ts":1700000006000}
{"type":"net","t":"open","id":3,"url":"wss://example.com/ws","ts":1700000000000}
{"type":"net","t":"send","id":3,"data":"{\"type\":\"ping\"}","size":14,"ts":1700000001000}
{"type":"net","t":"recv","id":3,"data":"{\"type\":\"pong\"}","size":14,"ts":1700000001050}
{"type":"net","t":"close","id":3,"code":1000,"reason":"normal closure","duration":60000}
```

Notes:

- `chunk.seq` and `chunk.size` are dropped at ingest (DOM-025) — only
  `data` is stored.
- Binary WS frames are forwarded as `"<binary: N bytes>"` strings by
  `flog_dart/lib/src/flog_web_socket.dart`; Rust treats them as text.
  Audit trail: DART-019.

## 5. ClientInfo (Rust-side, derived from Hello)

Not a wire type — the Rust-side representation of the Hello fields
plus local metadata. Lives in `src/input/protocol.rs`:

```rust
pub struct ClientInfo {
    pub id: ClientId,          // local u64 counter, not from wire
    pub app: String,
    pub app_version: String,   // "" when missing
    pub os: String,
    pub package_name: String,  // "" when missing
    pub port: u16,             // 0 when missing
    pub build_mode: String,    // "" when missing
    pub connected_at: Instant, // wall-clock on connect
    pub session_id: Option<String>, // from Hello.sessionId (TRANS-014)
}
```

`session_id` defaults to `None` for older Dart clients that don't ship
the field. Future work can use it to detect app restarts and replay
buffered state without breaking the wire format.

## 6. ServerMessage (downstream)

Internally tagged on `"type"`. Three variants: `mock_sync`, `replay`,
`subscribe`.

### 6.1 `mock_sync`

Sent once right after Hello (so newly-connected apps learn the
current rules) and again whenever the user edits the mock rules panel.

```json
{"type":"mock_sync","rules":"[{\"id\":1,\"url_pattern\":\"/api\",\"status_code\":200,\"response_body\":\"{\\\"ok\\\":true}\",\"enabled\":true}]"}
```

**Note:** `rules` is a JSON **string**, not a JSON array — it's the
output of `MockRuleStore::to_json_string()` round-tripped as a string
field. The Dart side parses it with `jsonDecode` in
`FlogMockInterceptor.updateRules`. Audit trail: DART-014.

Rule matching semantics (Dart-side): substring on `url_pattern`,
first-match-wins, case-sensitive; optional `method` filter applies
when set. Rules with `enabled: false` are skipped. Audit trail:
DART-013.

### 6.2 `replay`

Sent when the user hits **Replay** on a request in the Network detail
panel. Causes the Dart side to re-execute the request through the
same `Dio` instance, so any active mock rules apply.

```json
{
  "type": "replay",
  "method": "GET",
  "url": "https://example.com/api",
  "headers": null,
  "body": null
}
```

`headers` and `body` use `Option<String>` on the Rust side; when
absent they serialise as JSON `null` (not omitted). The Dart side
treats `null` as "no header override" / "no body".

### 6.3 `subscribe`

Sent when the user switches the active app in the device picker —
tells the Dart side to iterate its `FlogStore` and re-send every
buffered message as fresh `Log` / `Net` frames.

```json
{"type":"subscribe"}
```

No payload fields.

## 7. Mock rule format (inside `mock_sync.rules`)

The string inside `ServerMessage::MockSync::rules` is a JSON array.
Each rule is a serialisation of `domain::mock::MockRule` (with
`hit_count` skipped). Field names are `snake_case` on the wire.

```json
[
  {
    "id": 1,
    "url_pattern": "/api/users",
    "method": "GET",
    "status_code": 200,
    "response_body": "{\"users\": []}",
    "delay_ms": 0,
    "enabled": true
  }
]
```

| Field           | Type            | Notes                                                        |
|-----------------|-----------------|--------------------------------------------------------------|
| `id`            | int             | flog-assigned, monotonic per `MockRuleStore`                 |
| `url_pattern`   | string          | substring match against the request URL (case-sensitive)     |
| `method`        | string \| null  | e.g. `"GET"`; `null` means "match any method"                |
| `status_code`   | int             | HTTP status to return                                        |
| `response_body` | string          | literal response body                                        |
| `delay_ms`      | int             | optional delay before resolving; 0 = no delay                |
| `enabled`       | bool            | `false` = skip this rule                                     |

The Dart-side interceptor iterates the array in order and returns the
first matching rule, so put more specific patterns earlier.

## 8. Replay semantics

Replay is a round-trip:

1. flog sends `ServerMessage::Replay { method, url, headers, body }`.
2. Dart's `FlogServer._handleReplay` builds and sends a `Dio` request
   back through the same client (and therefore through the active
   `FlogMockInterceptor` — a replay can still land on a mock rule).
3. The request produces new `FlogNetKind::Req` + `Res` / `Chunk` / `Err`
   frames with a fresh `id`, marked `EntrySource::Replay` on the Rust
   side via the `source` field. (Mocked responses from the replay
   path still set `mocked: true` on `Res`, so the entry shows both
   `Replay` and `Mocked` indicators.)

This means replaying a request that was originally mocked will produce
another mocked response — by design, so that mock-rule iteration
stays trivially testable.

Replay currently only applies to HTTP requests. The Rust-side UI
hides the Replay button for SSE / WS entries.

## 9. Compatibility + versioning

There is no explicit protocol version field. Compatibility is instead
maintained by **additive evolution**:

- Every new Hello field uses `#[serde(default)]`.
- Every new FlogNetKind field uses `#[serde(default)]`.
- Unknown extra fields are silently ignored on both sides.
- Variant additions to `ClientMessage` / `FlogNetKind` / `ServerMessage`
  break older peers. Today the `ClientMessage` enum is **exhaustive**
  at every call site — adding a variant is a compile-time change
  across the whole codebase, so no silent drift can happen. Audit
  trail: TRANS-012.

**session_id (TRANS-014)** was added this way: Dart ships it
optionally; Rust captures it into `ClientInfo.session_id = Option<String>`
with `None` as the default. Older Dart clients keep working.

### 9.1 flog_dart v0.8 SSE redesign (DART-033, shipped)

`flog_dart 0.8.0` (2026-04-27) redesigned the SSE subsystem into three
composable `StreamTransformer`s:
`SseByteDecoder` → `SseLineDecoder` → `FlogSseReporter`. Each is
testable in isolation and can be swapped or omitted. The legacy
`FlogSseParser` entry point is preserved as a thin compat shim.
`SseResponse` grows a typed `events: Stream<SseEvent>` alongside the
legacy (now `@Deprecated`) `stream: Stream<String>`.

**Wire protocol is unchanged.** All four `t` values (`req` / `chunk` /
`done` / `err`) emitted over flog_net for SSE continue to match the
v0.7 shape byte-for-byte — flog TUI 0.4.x decodes both v0.7.x and
v0.8.x clients identically.

## 10. Cross-references

- High-level architecture: [ARCHITECTURE.md](ARCHITECTURE.md).
- Per-module detail: [MODULES.md](MODULES.md).
- Audit trail behind every `Audit trail: …` reference in this doc:
  [docs/superpowers/audit/](superpowers/audit/).
- Phase 3 step designs (where the current protocol shape was
  finalised): `docs/superpowers/plans/2026-04-23-phase3-step1-*`
  through `2026-04-24-phase3-step4-flog-dart.md`.
