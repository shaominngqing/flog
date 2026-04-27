# Changelog

All notable changes to `flog_dart`. This project follows
[Semantic Versioning](https://semver.org/).

## 0.7.3 — 2026-04-24

Consolidation release of the Phase 3 / 4 / 5 cleanup campaign. All
B-class bugs identified in the Phase 1 audit are fixed; README +
CHANGELOG rewritten; public API is unchanged from 0.7.2 (breaking
changes deferred to v0.8 per DART-033). Safe to pick up as a
drop-in upgrade.

### DART-024 / DART-025 audit resolution (Phase 5)

- README rewritten — covers `Flog.init`, `FlogDio` + `FlogHttpConfig`,
  SSE (`FlogDio.sse` / `FlogSseParser.wrap` / `FlogSseParser.wrapTyped`
  + typed `SseEvent`), `FlogWebSocket`, mock rules sync semantics,
  replay round-trip, `flogEnabled` override matrix, and the planned
  v0.8 breaking-change set.
- CHANGELOG gap-filled back to v0.1.0 from git history.
- Forward reference added for the planned v0.8 release (DART-033) —
  see "Planned for v0.8" below.

### Phase 3 Step 3.4 — flog_dart redesign (B+D-class fixes)

Bug fixes and refactors landed during the Phase 3 cleanup campaign
(2026-04-22 → 2026-04-24). These ship in the current `0.7.2` line.

Bug fixes (B-class):

- **DART-001** — `FlogSseParser` now correctly handles W3C-compliant
  multi-event-per-chunk payloads and multi-line `data:` joins. The
  previous parser dropped every event after the first `data:` in a
  single chunk.
- **DART-002** — `FlogSseParser.wrapTyped` and the `SseEvent` type
  are now implemented. The pre-existing `flog_dart/test/` suite (which
  referenced these APIs before they existed) now compiles and passes.
- **DART-004** — `FlogMockInterceptor.onRequest` is now a no-op when
  `flogEnabled == false` (previously ran mock-matching logic in
  release builds too, adding per-request overhead).
- **DART-006** — `FlogWebSocket.stream` is now an actual broadcast
  stream (previously documented as broadcast but implemented as
  single-subscription).
- **DART-007** — `FlogHttpInterceptor._truncate` compares against a
  byte budget using byte length rather than character count (previously
  could truncate multi-byte UTF-8 mid-rune).
- **DART-008** — `FlogHttpInterceptor` cleans up its internal `_idMap`
  / `_startMap` on every exit path, including when an earlier
  interceptor rejected or resolved the request.
- **DART-009** — `emitNet` now clones the caller-supplied map before
  stamping protocol metadata (`type`, `id`, timestamp). Previously
  mutated caller-owned maps.

Architecture (D-class):

- **DART-010** — `FlogDio.sse` extracted into `flog_dio_sse.dart` to
  keep `flog_dio.dart` below the 500-line budget. Public API unchanged.
- **DART-011..DART-014** — Mock extras key (`flog_mocked`) extracted to
  a named constant; match semantics (substring, first-match-wins,
  case-sensitive) documented on `FlogMockInterceptor`.
- **DART-015..DART-017** — `FlogServer` port-scan range is now a named
  `_portScanRange = 10` constant; the server logs a clear error when
  all 10 ports are taken; `_handleReplay` now surfaces Dio errors via
  `debugPrint` instead of silently fire-and-forget.
- **DART-018 / DART-019** — `FlogWebSocket.fromChannel` deduplicated
  with the primary constructor; the `<binary: N bytes>` format string
  extracted to a named constant.
- **DART-020** — `FlogStore` capacity (50 000) lifted to a named
  constant.
- **DART-021** — `nextNetId` and `emitNet` marked `@internal`. Still
  exported from the library barrel for v0.x back-compat (see
  "Planned for v0.8" below).
- **DART-022** — Dead `appName` / `appVersion` / `packageName`
  parameters removed from `FlogServer.start`. Callers now use
  `FlogServer.updateAppInfo(...)`.
- **DART-023** — `Flog.init` now logs `PackageInfo` failures via
  `debugPrint` instead of swallowing them silently.
- **DART-026** — `FlogDio.sse` now guards against a null
  `response.data` so empty-body streams do not crash.
- **DART-027** — HTTP interceptor emit path refactored: a new
  `_emitHttpCompletion` helper deduplicates ~30 lines of request-emit
  logic shared between the real-response and mocked-response paths.

## Planned for v0.8

Breaking release. **Wire protocol stays unchanged** — flog TUI 0.4.x
will continue to work against v0.7.x and v0.8.x; migration only
affects Dart-side SSE call sites.

- **DART-033** — SSE subsystem redesign:
  - `FlogSseParser` becomes a proper `StreamTransformer<List<int>,
    SseEvent>`; parsing and telemetry (the `emitNet` chunk logging)
    separate into two composable transformers.
  - `FlogDio.sse` returns the raw byte stream alongside the typed
    `SseEvent` stream so callers can pick whichever layer they need.
  - Hard byte-buffer limit added; oversized events error out rather
    than OOM the isolate.
  - UTF-8 decode cost reduced by buffering at the byte layer.
- `nextNetId` and `emitNet` removed from the public library barrel
  (DART-021). Callers that still depend on them should import from
  `package:flog_dart/src/flog_net.dart` (and note that file is
  `@internal` — it may move again in v1.0).

## 0.7.2 — 2026-04-22

- `flogEnabled` default now recognises `--dart-define=APP_FLAVOR`:
  - `APP_FLAVOR=release` → disabled (tree-shaken away).
  - `APP_FLAVOR=alpha` or any other value → enabled.
  - Unset → falls back to the original `!dart.vm.product` derivation.
- Explicit `--dart-define=FLOG_ENABLED=...` continues to take
  precedence over both derivations.

## 0.7.1 — 2026-04-21

- **FlogHttpInterceptor** — `onError` now emits HTTP status code,
  response headers, and response body for server error responses
  (4xx / 5xx). Previously only a generic error string was sent,
  causing flog to show "failed" instead of the actual status code.

## 0.6.4 — 2026-04-20

- Rename the library file from `flog_logger.dart` to `flog_dart.dart`
  so `package:flog_dart/flog_dart.dart` is the canonical import.

## 0.6.3 — 2026-04-20

- Add a `flog_dart.dart` barrel file so
  `package:flog_dart/flog_dart.dart` resolves cleanly.

## 0.6.2 — 2026-04-20

- Remove the upper bound on `package_info_plus` to avoid false
  resolution conflicts.

## 0.6.1 — 2026-04-20

- Widen `package_info_plus` constraint to support v9+.

## 0.6.0 — 2026-04-20

- **New:** `Flog.init()` — top-level bootstrap. Auto-detects app
  name / version / package name via `package_info_plus`; registers
  `debugPrint` + `FlutterError.onError` + `PlatformDispatcher.onError`
  hooks so framework logs + crashes reach the TUI automatically.
- `FlogDio` decoupled from the logger bootstrap — previously
  constructing a `FlogDio` implicitly started the WS server; now
  `Flog.init()` is the only entry point that starts the server.
- System log capture: `debugPrint` and Flutter error hooks are now
  captured as regular `LogEntry`s.

## 0.5.0 — 2026-04-19

- **Data source architecture:** `FlogStore` introduced — a 50 000-entry
  FIFO ring buffer that stores every log + network frame as it's
  produced. When a TUI connects (or resubscribes after a session
  switch), the buffer is replayed and then the app transitions
  seamlessly to live.
- System log capture hooks registered at startup.

## 0.4.0 — 2026-04-18

- **Direct Socket architecture:** the app now hosts a WebSocket server
  (`FlogServer`); flog TUI is the client. Logs and network frames no
  longer flow through VM Service `print` / `developer.log` — they
  travel directly over the WS channel.
- `FlogClient` (earlier short-lived rename) removed; `FlogServer` is
  the single transport component on the Dart side.
- Multi-app connection framework: flog TUI can attach to several
  running apps simultaneously and switch between them with per-app
  session isolation.

## 0.3.0 — 2026-04-17

- **`FlogDio`** — drop-in `Dio` replacement that auto-inserts
  `FlogMockInterceptor` + `FlogHttpInterceptor` at the front of the
  chain.
- **Interceptor-based mock system** (replaces the earlier proxy-server
  approach): rules authored in the flog TUI sync over the control
  channel; matching requests resolve locally with the canned response.
- Mocked responses emit `flog_net` `req` + `res` frames so they appear
  in the inspector tagged `Mocked`.
- Release builds tree-shake `flog_dart` to zero via `flogEnabled`.
- iOS real device support via `developer.log` fallback alongside
  `print` for the initial channel.

## 0.2.0 — 2026-04-15

- **FlogHttpInterceptor** — Dio interceptor for HTTP request /
  response logging, emitted to flog's Network Inspector via the
  shared `emitNet` helper.
- **FlogSseParser** — SSE stream wrapper with chunk-level logging.
- **FlogWebSocket** — WebSocket wrapper with send / recv message
  logging.
- Shared `emitNet()` helper using the `[INFO][flog_net]` protocol
  prefix.
- All interceptors configurable: headers, body, max size, filter
  predicate.

## 0.1.0 — 2026-04-10

- Initial release.
- `FlogLogger` class with tag-based structured logging.
- Full-word methods: `verbose()`, `debug()`, `info()`, `warning()`,
  `error()`.
- Single-letter shorthand: `v()`, `d()`, `i()`, `w()`, `e()`.
- Optional `error` + `stackTrace` named parameters on
  `debug` / `warning` / `error`.
