# DevTools-Grade Network Timing Design

- **Date**: 2026-06-08
- **Scope**: Rust TUI network detail panel + `flog_dart` HTTP/SSE/WS instrumentation
- **Status**: Design approved by user, pending implementation plan

---

## 0. Context

flog currently records network request duration as a single total value.
`flog_dart` stamps `FlogHttpInterceptor.onRequest` with a start time and
emits `duration` on response/error. The Rust side stores that value in
`NetworkEntry.duration` and displays it in the Network table and detail
General section.

That is enough for "this request was slow", but not enough for "why was
it slow". The goal of this design is to add DevTools-grade timing to the
Network detail panel, covering HTTP, SSE, and WebSocket traffic with
diagnostic value as the first priority.

The feature must respect the repository boundaries:

- `domain/` stores pure timing data, with no TUI framework dependency.
- `ui/` reads `App` and writes only layout cache.
- The Network list remains unchanged; timing detail appears only in the
  right-side detail panel.
- The wire protocol evolves additively. Old `flog_dart` clients that do
  not send timing continue to work.

---

## 1. Goals

1. Show where time was spent for HTTP requests:
   `queued`, `dns`, `tcp`, `tls`, `request_upload`, `ttfb`, `download`,
   `decode`, `total`.
2. Show stream-specific timing for SSE:
   connection latency, first event latency, stream lifetime, event gaps,
   and notable idle gaps.
3. Show connection/message timing for WebSocket:
   connecting, handshake, open lifetime, send/recv timeline, idle gaps,
   close.
4. Preserve data honesty. Unavailable, approximate, inferred, reused, and
   cancelled phases are shown explicitly instead of being rendered as
   fake `0ms` values.
5. Keep the existing Network list columns and behavior unchanged.
6. Keep the Rust TUI implementation terminal-native and consistent with
   the existing Catppuccin Macchiato ratatui style.

## 2. Non-Goals

- Do not clone Chrome DevTools pixel-for-pixel.
- Do not require a browser or GUI frontend.
- Do not make DNS/TCP/TLS appear when a custom adapter or reused
  connection does not expose those phases.
- Do not remove or reinterpret the existing `duration` field.
- Do not add timing filters or timing columns to the Network table in the
  first implementation.

---

## 3. User-Approved Direction

The selected architecture is **Hybrid timing with extension hooks**.

`FlogDio` should install or wrap a timing-aware adapter by default so the
default experience has high fidelity. The model must also allow custom
adapters and future native hooks to provide timing through the same data
shape.

The product standard is **diagnostic value first**:

- show the phases that help identify the bottleneck;
- attach confidence and status metadata to every phase;
- degrade visibly when precision is not available.

The UI direction is:

- no Network list changes;
- add a `Timing` section in the detail panel after `General`;
- use ratatui-colored spans, pills, bold section headers, and Unicode block
  characters for waterfall/timeline bars;
- use protocol-specific detail content for HTTP, SSE, and WS.

---

## 4. Dart Architecture

### 4.1 Timing Core

Add a small internal timing model under `flog_dart/lib/src/timing/`.

Suggested files:

- `timing_trace.dart`
- `timing_phase.dart`
- `timing_event.dart`
- `timing_connection.dart`
- `timing_clock.dart`

Core types:

```dart
class FlogTimingTrace {
  final int version;
  final String source;
  final String clock; // monotonic_us
  final int startUs;
  final int? endUs;
  final FlogTimingConnection? connection;
  final List<FlogTimingPhase> phases;
  final List<FlogTimingEvent> events;
  final List<String> notes;
}

class FlogTimingPhase {
  final String name;
  final int? startUs;
  final int? endUs;
  final String status;     // complete | active | unavailable | reused | skipped | cancelled | errored
  final String confidence; // exact | approx | inferred | unavailable
  final String? detail;
}

class FlogTimingEvent {
  final String name;
  final int atUs;
  final int? gapUs;
  final int? size;
  final String? detail;
}
```

Use monotonic microseconds inside a trace. Wall-clock `ts` remains for
list timestamps and does not drive phase duration math.

### 4.2 HTTP Collection

Add `FlogTimingHttpClientAdapter`, installed by default through `FlogDio`.
If a user already supplied a custom adapter, wrap it with a timing wrapper
instead of replacing it.

Default `dart:io` path captures:

- adapter entry and exit;
- connection creation through `HttpClient.connectionFactory` where
  available;
- connection identity and reuse state where it can be inferred;
- headers received;
- first body byte;
- body download completion;
- decode/transform time;
- response/error/cancel terminal state.

Custom adapter path captures only what is visible around the adapter and
response stream. DNS/TCP/TLS become unavailable with a note such as:

`custom adapter did not expose socket timing`

Browser/web path also degrades explicitly because socket-level phases are
not available.

### 4.3 SSE Collection

SSE timing builds on the HTTP request trace and then adds stream timing in
`FlogSseReporter`.

Record:

- `connect`;
- `first_event`;
- `stream_open`;
- `stream_total`;
- per-event timing points;
- idle gaps above a threshold.

SSE error, done, and cancel all finalize the trace so entries do not remain
permanently active.

### 4.4 WebSocket Collection

`FlogWebSocket` keeps a separate timing trace.

Record:

- `connecting` from call entry;
- `handshake` until `WebSocketChannel.ready` completes;
- `open` lifetime;
- send and receive events;
- idle gaps above a threshold;
- close code/reason/duration.

For `FlogWebSocket.fromChannel`, handshake is unavailable because the
channel is already connected. The UI should display that as a note, not as
a zero-duration handshake.

---

## 5. Wire Protocol

The protocol change is additive.

`res`, `err`, `done`, `open`, and `close` may include a full `timing`
object:

```json
{
  "type": "net",
  "t": "res",
  "id": 42,
  "status": 200,
  "duration": 126,
  "timing": {
    "v": 1,
    "source": "flog_adapter",
    "clock": "monotonic_us",
    "startUs": 0,
    "endUs": 126000,
    "connection": {
      "id": "https://api.example.com:443#3",
      "reused": false,
      "protocol": "http/1.1"
    },
    "phases": [
      {
        "name": "ttfb",
        "startUs": 62000,
        "endUs": 104000,
        "status": "complete",
        "confidence": "exact"
      }
    ],
    "events": [
      {"name": "headers", "atUs": 62000},
      {"name": "first_byte", "atUs": 104000}
    ],
    "notes": ["TLS boundary approximated by adapter"]
  }
}
```

`chunk`, `send`, and `recv` should use lightweight event timing instead of
repeating a full trace:

```json
{
  "type": "net",
  "t": "chunk",
  "id": 2,
  "seq": 4,
  "data": "...",
  "size": 208,
  "eventTiming": {"atUs": 1259000, "gapUs": 812000}
}
```

Rust deserialization uses `#[serde(default)]` and optional fields so older
clients remain compatible. Unknown fields continue to be ignored as they
are today.

---

## 6. Rust Domain Design

Add `src/domain/network_timing.rs` and re-export the types from
`domain/mod.rs` or use them through `domain::network`.

Suggested Rust types:

```rust
pub struct NetworkTiming {
    pub version: u16,
    pub source: TimingSource,
    pub clock: TimingClock,
    pub start_us: Option<u64>,
    pub end_us: Option<u64>,
    pub connection: Option<TimingConnection>,
    pub phases: Vec<TimingPhase>,
    pub events: Vec<TimingEvent>,
    pub notes: Vec<String>,
}

pub struct TimingPhase {
    pub name: String,
    pub start_us: Option<u64>,
    pub end_us: Option<u64>,
    pub status: TimingPhaseStatus,
    pub confidence: TimingConfidence,
    pub detail: Option<String>,
}

pub struct TimingEvent {
    pub name: String,
    pub at_us: u64,
    pub gap_us: Option<u64>,
    pub size: Option<u64>,
    pub detail: Option<String>,
}
```

Extend existing network data:

```rust
pub struct NetworkEntry {
    pub timing: Option<NetworkTiming>,
    ...
}

pub struct SseChunk {
    pub data: String,
    pub event_timing: Option<TimingEvent>,
}

pub struct WsMessage {
    pub direction: WsDirection,
    pub data: String,
    pub size: u64,
    pub event_timing: Option<TimingEvent>,
}
```

`NetworkStore` stores full timing on terminal/state messages and event
timing on `chunk`, `send`, and `recv`. This supersedes the current
behavior where some wire timestamps are accepted but dropped because there
was no UI consumer.

---

## 7. TUI Design

### 7.1 Placement

Add `src/ui/network/detail/timing.rs`.

In `src/ui/network/detail/mod.rs`, render `Timing` after `General` and
before query params / headers / body sections.

The section is omitted when `entry.timing` is absent and there is no
event timing to show. Existing entries from older clients therefore keep
the current UI.

### 7.2 Shared Visual Language

Use existing styling conventions:

- section header: `▼ Timing` in `SAPPHIRE`, bold;
- keys: `MAUVE`;
- values: `TEAL`, `GREEN`, `YELLOW`, `PEACH`, `RED` depending on meaning;
- source/protocol/connection pills: colored `Span`s with fg/bg/bold;
- bars: Unicode block characters such as `█`, `▌`, `▏`;
- muted notes: `SUBTEXT0` / `OVERLAY0`.

Ratatui supports styled spans with foreground/background and modifiers,
matching the current UI style.

### 7.3 HTTP Detail Layout

HTTP uses a phase table with a colored waterfall:

```text
 ▼ Timing   ADAPTER  HTTP/1.1  conn #3
   Total: 126ms   Bottleneck: TTFB 42ms   Trust: 6 exact 2 approx

   Phase              Time    Trust       Waterfall
   queued              2ms    exact       ▏▌
   dns                 7ms    approx        ▏██
   tcp                16ms    exact           ▏████
   tls                22ms    approx              ▏█████
   upload              3ms    exact                    ▏▌
   ttfb               42ms    exact                     ▏██████████
   download           19ms    exact                                ▏████▌
   decode              8ms    exact                                     ▏██

   markers  headers +62ms  first byte +104ms  complete +126ms
   notes    TLS boundary is approximate; connection was opened for this request.
```

### 7.4 SSE Detail Layout

SSE uses stream and event-gap detail instead of HTTP phases:

```text
 ▼ Timing   SSE REPORTER  STREAM  reused conn
   Open: 18.4s   Events: 142   First event: 421ms   Worst gap: 812ms

   Phase              Time      Trust       Stream
   connect             93ms     exact       ▏██
   first_event        421ms     exact         ▏████████
   stream_open        18.4s     exact       ████████████████████████████

   Event gaps
   #001  +421ms  312B  data: {"delta":"Hel"}
   #002   +18ms  180B  data: {"delta":"lo"}
   #003   +24ms  195B  data: {"delta":"!"}
   gap   +812ms        idle before next event
   #004   +31ms  208B  data: {"finish_reason":"stop"}

   notes    DNS/TCP/TLS unavailable because the HTTP connection was reused.
```

### 7.5 WebSocket Detail Layout

WebSocket uses connection lifetime and message timeline detail:

```text
 ▼ Timing   WS WRAPPER  LIVE  conn #1
   Connected: 2m14s   Messages: 86   Handshake: 117ms   Worst idle: 35.5s

   Phase              Time      Trust       Lifetime
   connecting          38ms     exact       ▏█
   handshake          117ms     exact        ▏███
   open              2m14s      exact       ████████████████████████████
   close                6ms     exact                                ▏▌

   Message timeline
   SEND  +000.182s  41B   {"type":"subscribe"}
   RECV  +000.244s  88B   {"type":"ack"}
   RECV  +012.411s  3.1KB {"type":"delta", ...}
   IDLE  35.5s            before next received frame
   RECV  +048.002s  16B   {"type":"ping"}

   notes    Handshake is measured until WebSocketChannel.ready completes.
```

### 7.6 Narrow Width Behavior

On narrow terminals:

1. keep `Phase`, `Time`, and `Trust`;
2. shrink or omit the waterfall column;
3. wrap notes using existing detail-panel wrapping helpers;
4. keep event timelines scrollable in the same detail panel model.

---

## 8. Error and Degradation Rules

- `timing` absent: hide the Timing section.
- only `duration` present: keep current General duration display.
- reused connection: show `reused conn`; do not display DNS/TCP/TLS as
  `0ms`.
- custom adapter: show available adapter/stream phases and a note for
  unavailable socket phases.
- already-connected WebSocket channel: show handshake unavailable.
- cancelled request: show completed phases and terminal status
  `cancelled`.
- network error: show completed phases and terminal status `errored`.
- malformed timing object: ignore timing for that message if serde rejects
  it; the existing request/response data still renders.

---

## 9. Tests

### Rust

- Protocol tests:
  - missing timing;
  - complete timing;
  - event timing on chunk/send/recv;
  - unknown extra fields remain tolerated.
- Domain/store tests:
  - terminal messages store full timing;
  - SSE chunks preserve event timing and order;
  - WS messages preserve event timing and order;
  - reused/unavailable/approx statuses round-trip.
- UI tests for extracted pure render helpers:
  - phase duration formatting;
  - waterfall sizing;
  - narrow width fallback;
  - protocol-specific section selection.

### Dart

- HTTP success with timing.
- HTTP error with timing.
- HTTP custom adapter fallback timing.
- mocked request timing, including configured delay.
- SSE first event, event gaps, idle gap, done/error/cancel.
- WebSocket connect success/failure, send/recv gaps, close.
- `flogEnabled == false` keeps timing code tree-shakable/no-op.

---

## 10. Implementation Boundaries

Recommended implementation sequence for the follow-up plan:

1. Rust domain/protocol timing types with tests.
2. Rust store ingestion and compatibility tests.
3. TUI Timing section render helpers and protocol-specific detail renderers.
4. Dart timing core.
5. Dart HTTP timing adapter/wrapper.
6. Dart SSE and WebSocket event timing.
7. Documentation updates in `docs/PROTOCOL.md` and module docs.

No implementation should change the Network table in the first pass.

---

## 11. References

- Dio package documentation: <https://pub.dev/packages/dio>
- Dio `ResponseType.stream`: <https://pub.dev/documentation/dio/latest/dio/ResponseType.html>
- Dart `HttpClient.connectionFactory`: <https://api.dart.dev/dart-io/HttpClient/connectionFactory.html>
- Dart `HttpClientResponse`: <https://api.dart.dev/dart-io/HttpClientResponse-class.html>
- `web_socket_channel` package: <https://pub.dev/packages/web_socket_channel>
- Serde `#[serde(default)]`: <https://serde.rs/attr-default.html>
- Ratatui text rendering: <https://ratatui.rs/recipes/render/display-text/>
