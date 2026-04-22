# Audit 04 — flog_dart

Scope: flog_dart/lib/ (all), flog_dart/test/ (read-only), README/CHANGELOG.

Auditor: Phase 1 Subagent 4 (read-only)
Date: 2026-04-22

## Findings

```yaml
id: DART-001
label: B
location: flog_dart/lib/src/flog_sse_parser.dart:212-242
title: SSE parser drops all events after the first `data:` line in a chunk
evidence: |
  `_processDecoded` loops through lines, but on the first `trimmed.startsWith('data:')`
  match it calls `onData(incomplete, payload); return;`, exiting the function.
  Any subsequent data: lines present in the same decoded chunk are silently lost.

      for (final line in lines) {
        final trimmed = line.trim();
        if (trimmed.isEmpty) continue;
        if (trimmed.startsWith('data:')) {
          final payload = trimmed.substring(5).trim();
          if (payload == '[DONE]') continue;
          onData(incomplete, payload);
          return;            // ← bails out, dropping remaining lines
        }
      }

  The untracked test `flog_sse_parser_test.dart` has an explicit regression test
  "multiple JSON chunks in single TCP packet" that asserts both events survive.
  With the current implementation the test can never pass.
proposed_action: |
  Expected behavior: every `data:` line encountered in the decoded chunk must
  produce an event (or collapse into a multi-line `data:` block per SSE spec).
  Fix: accumulate payloads in a list and emit all of them, not just the first.
  Drive by re-enabling the test suite once the signature matches (see DART-002).
risk: high
```

```yaml
id: DART-002
label: B
location: flog_dart/lib/src/flog_sse_parser.dart:23-243 (API surface)
title: Untracked test suite references FlogSseParser.wrapTyped + SseEvent that do not exist
evidence: |
  flog_dart/test/flog_sse_parser_test.dart imports from
  `package:flog_dart/src/flog_sse_parser.dart` and exercises:

    FlogSseParser.wrapTyped(...)          // does not exist in source
    SseEvent(event: ..., data: ..., id: ..., retry: ...)   // class missing
    expect(events[0].retry, 3000);        // retry field unimplemented
    BOM stripping, CRLF, multi-line data join, comment lines, [DONE] filter,
    stream-end flush — tests expect behavior the source does not provide.

  Result: the test file will not compile against the current lib/. If this test
  was ever intended to ship, either the source lost `wrapTyped` / `SseEvent` in a
  previous refactor, or the tests were written against a planned API that never
  landed.
proposed_action: |
  Expected behavior: parser exposes (a) raw `wrap()` returning `Stream<String>`
  for data-only consumers, and (b) `wrapTyped()` returning `Stream<SseEvent>`
  with event/id/retry/data fields per SSE spec (BOM strip, CRLF, multi-line
  data join, comment skip, [DONE] filter, end-of-stream flush). Re-introduce
  the missing type and method; use the existing test file as the behavioral
  contract.
risk: high
```

```yaml
id: DART-003
label: B
location: flog_dart/lib/flog_dart.dart:4-13
title: Library docstring documents a top-level `flog()` function that does not exist
evidence: |
  /// Sends structured log messages to flog TUI via Direct Socket.
  ///
  /// ```dart
  /// // Initialize once, as early as possible:
  /// flog();
  ///
  /// // Then use FlogLogger anywhere:
  /// ...
  /// ```

  No top-level `flog()` is declared or exported. The real entry point is
  `Flog.init()` defined lower in the same file. A user copying this example
  from pub.dev gets "The function 'flog' isn't defined."
proposed_action: |
  Expected behavior: example code in the library-level dartdoc compiles.
  Fix: replace `flog();` with `Flog.init();` (matches the second example at
  line 33-38 of the same file) and delete the redundant "Top-level entry
  point" class dartdoc duplication.
risk: low
```

```yaml
id: DART-004
label: B
location: flog_dart/lib/src/flog_mock_interceptor.dart:59-95
title: FlogMockInterceptor.onRequest runs mock logic even when flogEnabled is false
evidence: |
  @override
  void onRequest(RequestOptions options, RequestInterceptorHandler handler) {
    final url = options.uri.toString();
    final method = options.method;
    for (final rule in _rules) {
      if (!rule.enabled) continue;
      ...
    }
    handler.next(options);
  }

  There is no `if (!flogEnabled)` early-return. FlogDio only *inserts* this
  interceptor when flogEnabled is true, so in AOT-release the insert never
  runs and the loop never executes. But: (a) users who wire FlogMockInterceptor
  manually onto a plain Dio will have mock code active in release, breaking the
  "zero overhead in production" claim; (b) the class + its String.contains loop
  are not flogEnabled-guarded, so tree-shaking cannot drop them when the type
  is still referenced elsewhere.
proposed_action: |
  Expected behavior: every public entry point in flog_dart short-circuits on
  `!flogEnabled` so AOT eliminates the body.
  Fix: add `if (!flogEnabled) { handler.next(options); return; }` at the top of
  onRequest. Mirror what FlogHttpInterceptor already does (flog_http_interceptor.dart:65).
risk: medium
```

```yaml
id: DART-005
label: B
location: flog_dart/lib/src/flog_mock_interceptor.dart:48, flog_dart/lib/src/flog_server.dart:222-241
title: `ext.flog.syncMockRules` VM Service extension is documented but not registered
evidence: |
  flog_mock_interceptor.dart:48:
  /// Rules are synced from the flog TUI via VM Service extension
  /// (`ext.flog.syncMockRules`). When a request matches an enabled rule,

  A full-repo grep for `registerExtension` / `ext.flog` returns zero hits in
  lib/ — there is no `developer.registerExtension('ext.flog.syncMockRules', …)`.
  The sole sync channel is the WebSocket `mock_sync` message handled in
  flog_server.dart:_onMessage. CLAUDE.md also still references
  `ext.flog.syncMockRules`.

  Behavioral consequence: the documented extension does not exist; any external
  tool trying to call it via `postEvent`/`callServiceExtension` will fail.
proposed_action: |
  Expected behavior: either the VM Service extension exists (old Direct-Socket-
  less architecture) or the doc says "synced via WebSocket mock_sync message".
  Current design is WebSocket-only, so the fix is documentation: update the
  class dartdoc on FlogMockInterceptor and the matching section in CLAUDE.md
  to describe the real channel.
risk: low
```

```yaml
id: DART-006
label: B
location: flog_dart/lib/src/flog_web_socket.dart:27, 45, 90
title: `stream` is declared a broadcast stream in dartdoc but is a single-subscription map
evidence: |
  /// Broadcast stream of incoming messages with flog_net instrumentation.
  late final Stream<dynamic> stream;
  ...
  stream = _channel.stream.map((message) { ... }).handleError(...);

  `_channel.stream` is the channel's single-subscription stream. `.map(...)`
  preserves single-subscription semantics. A caller who reads the dartdoc and
  attaches two listeners gets "Stream has already been listened to." at
  runtime.
proposed_action: |
  Expected behavior: either match the docstring (wrap in `.asBroadcastStream()`)
  or correct the docstring. asBroadcastStream() is the less-surprising choice
  because Flipper-style tooling often wants multiple subscribers.
risk: medium
```

```yaml
id: DART-007
label: B
location: flog_dart/lib/src/flog_http_interceptor.dart:258-263
title: _truncate compares char count against a byte-size budget
evidence: |
  /// Maximum body size in bytes to log. Bodies exceeding this are truncated.
  final int maxBodySize;
  ...
  String _truncate(String value) {
    if (value.length > maxBodySize) {
      return '${value.substring(0, maxBodySize)}... (truncated)';
    }
    return value;
  }

  `value.length` on a Dart String is the UTF-16 code-unit count, not the byte
  count. A CJK-heavy JSON payload under the documented byte budget can be
  truncated, and an ASCII-heavy payload exceeding the byte budget may slip
  through. The field name says "bytes" but the code measures code units.
proposed_action: |
  Expected behavior: if the field is "maxBodySize in bytes", measure bytes
  (utf8.encode(value).length) or rename the field to `maxBodyChars`. Cheaper
  option: rename + update dartdoc, truncation correctness is unaffected.
risk: low
```

```yaml
id: DART-008
label: B
location: flog_dart/lib/src/flog_http_interceptor.dart:47-48, 64-100, 182-244
title: _idMap/_startMap leak when an earlier interceptor rejects/resolves before FlogHttpInterceptor runs
evidence: |
  /// Maps `requestOptions.hashCode` to the assigned flog_net request ID.
  final Map<int, int> _idMap = {};
  final Map<int, DateTime> _startMap = {};

  onRequest inserts into the maps. Entries are only removed in onResponse and
  onError. A request can terminate via handler.resolve() in a downstream
  interceptor without ever reaching the response phase on FlogHttpInterceptor,
  or it can be cancelled before response. Each such path leaves an orphan entry
  in both maps. Over a long session the maps grow unboundedly.

  Related: hashCode collisions on RequestOptions are unlikely but not
  impossible, and RequestOptions is mutable (headers, uri via redirects) — a
  hashCode derived from mutable state is a fragile key.
proposed_action: |
  Expected behavior: the flog_net id bookkeeping has bounded memory.
  Fix: use an Expando<int> keyed on RequestOptions identity, which GC handles
  automatically, or stamp the id into `options.extra['flog_id']` at onRequest
  and read it back at onResponse/onError. Either removes the map and the
  leak.
risk: medium
```

```yaml
id: DART-009
label: B
location: flog_dart/lib/src/flog_net.dart:19-24
title: emitNet mutates caller-owned Map with protocol metadata
evidence: |
  void emitNet(Map<String, dynamic> data) {
    if (!flogEnabled) return;
    data['type'] = 'net';
    data['ts'] = DateTime.now().millisecondsSinceEpoch;
    FlogServer.instance.send(data);
  }

  Callers pass a `<String, dynamic>{}` literal each call so today this is
  harmless, but the signature invites bugs: any caller who shares or inspects
  the map after emitNet sees extra `type`/`ts` keys. Also the signature cannot
  distinguish "this is a fully-built flog_net message" from "decorate this
  ad-hoc map", making the contract with FlogServer.send unclear.
proposed_action: |
  Expected behavior: public helper either clones or accepts a payload the
  caller does not own. Fix: copy into a new map before decorating
  (`final out = {...data, 'type': 'net', 'ts': now};`) or accept a value
  object (`FlogNetMessage`) to make the protocol explicit.
risk: low
```

```yaml
id: DART-010
label: D
location: flog_dart/lib/src/flog_dio.dart:80-504
title: FlogDio class is a 500-line hand-written Dio delegate, yellow on §5.5 file rule
evidence: |
  `class FlogDio implements Dio { final Dio _inner; ... }` followed by 14
  override methods that each do nothing but forward every parameter to
  `_inner.*`. The only non-delegation are the constructor (interceptor
  insertion) and `sse()`. If Dio adds/renames/removes a method, FlogDio
  breaks silently until the next `dart analyze` run.

  Project memory + feedback_interceptor_ordering.md treat interceptor wiring
  as the real responsibility of this file — the delegation is scaffolding.
proposed_action: |
  Redesign sketch: drop the `implements Dio` claim and expose Dio directly.
  Two options:
    1. Make FlogDio a factory / extension that hands back a `Dio` instance
       with interceptors pre-wired + a `sse()` extension method:
         final Dio dio = FlogDio.create(baseUrl: ...);
         await dio.sse(...)  // via extension on Dio
    2. Keep the wrapper but compose instead of re-implement:
         class FlogDio { final Dio dio; ... }
         // users call `flogDio.dio.get(...)` — no 400-line delegation.
  Both cut the file to ~100 lines and remove the Dio-upstream coupling risk.
  Passes the §5.5 green-zone threshold.
risk: medium
```

```yaml
id: DART-011
label: D
location: flog_dart/lib/src/flog_dio.dart:98-120
title: Interceptor ordering is correct but unguarded against user manipulation
evidence: |
  if (flogEnabled) {
    ...
    _inner.interceptors.insert(0, FlogMockInterceptor());
    _inner.interceptors.insert(1, FlogHttpInterceptor(...));
  }

  Ordering is correct at construction (Mock=0, Http=1). But the public getter
  `interceptors` exposes the live list (flog_dio.dart:181). A user who does
  `dio.interceptors.insert(0, MyInterceptor())` or
  `dio.interceptors.clear()` silently defeats the ordering contract that
  feedback_interceptor_ordering.md identifies as the single largest v0.2.0
  debugging pain.

  There is also no API to *remove* the flog interceptors — once inserted, they
  live until the Dio is closed.
proposed_action: |
  Redesign sketch: wrap the interceptors list so mutations that would place a
  response-modifying interceptor before FlogHttpInterceptor are detected. Two
  workable shapes:
    1. Runtime guard: override `interceptors` to return a proxy list whose
       `insert(0, …)` pushes to index 2 and whose `clear()` re-inserts the
       flog pair.
    2. Document-and-fail-loud: on first request, if flog interceptors are not
       at positions 0/1, log a warning and re-insert.
  Also add `FlogDio.removeFlogInterceptors()` for tests / opt-out.
risk: medium
```

```yaml
id: DART-012
label: D
location: flog_dart/lib/src/flog_mock_interceptor.dart:51-57
title: FlogMockInterceptor uses a process-wide static rule list
evidence: |
  class FlogMockInterceptor extends Interceptor {
    static List<FlogMockRule> _rules = [];
    static void updateRules(List<FlogMockRule> rules) { _rules = rules; }

  Multiple FlogDio instances share one rule table. In tests this creates
  order-dependent behavior across test files (one test mutates rules, the
  next sees them). In apps with multiple Dios (e.g. auth-bearing vs public)
  there is no way to scope rules per Dio.
proposed_action: |
  Redesign sketch: move the rule store out of the interceptor class into a
  `FlogMockStore` (instance + convenient `FlogMockStore.global`). Interceptors
  take a `FlogMockStore` in their constructor, defaulting to the global one.
  Backwards compatible for FlogDio (always uses global). New feature: tests
  can inject their own store; advanced users can scope rules per Dio.
risk: low
```

```yaml
id: DART-013
label: D
location: flog_dart/lib/src/flog_mock_interceptor.dart:60-95
title: Mock match semantics (substring, first-match-wins, case-sensitive) are load-bearing and undocumented
evidence: |
  if (!url.contains(rule.urlPattern)) continue;
  if (rule.method != null &&
      rule.method!.toLowerCase() != method.toLowerCase()) { continue; }
  ...
  handler.resolve(response, true);
  return;   // ← first match wins, iteration order = insertion order

  Three semantic choices with no documentation:
    1. URL match is substring, not glob/regex/exact.
    2. URL match is case-sensitive (`contains` on Dart String).
    3. First matching rule wins; later rules for the same URL are dead.
  Users on the flog TUI side (where rules are created) have no way to know
  these rules from reading flog_dart docs.
proposed_action: |
  Redesign sketch: extract a `MockMatcher` type with an explicit `match(url,
  method) -> bool` method. Document the three choices in its dartdoc + the
  FlogMockRule dartdoc. Independently decide whether case-sensitivity is
  desired — HTTP URLs are mixed-case for path, so insensitive host + sensitive
  path would mirror RFC 3986. Current Phase-1 scope: just document.
risk: low
```

```yaml
id: DART-014
label: D
location: flog_dart/lib/src/flog_mock_interceptor.dart:72-73, flog_dart/lib/src/flog_http_interceptor.dart:111
title: Magic string `flog_mocked` in options.extra — concept not extracted
evidence: |
  // flog_mock_interceptor.dart:72
  options.extra['flog_mocked'] = true;

  // flog_http_interceptor.dart:111
  final isMocked = response.requestOptions.extra['flog_mocked'] == true;

  The string `'flog_mocked'` is repeated in two files and is the only channel
  by which the mocked/real distinction flows from the mock interceptor to the
  HTTP logger. A typo in one site silently fails.
proposed_action: |
  Extract into a private constant in a shared file (e.g.
  `const _kMockedExtraKey = 'flog_mocked';`) or better, a tiny typed extension:
      extension FlogRequestExtra on RequestOptions {
        bool get flogMocked => extra['flog_mocked'] == true;
        set flogMocked(bool v) => extra['flog_mocked'] = v;
      }
  Callers become `options.flogMocked = true` / `if (response.requestOptions.flogMocked)`.
risk: low
```

```yaml
id: DART-015
label: D
location: flog_dart/lib/src/flog_server.dart:165-180
title: Port-scan range [basePort, basePort+9] is a magic 10
evidence: |
  Future<void> _startServer() async {
    final basePort = _port;
    for (int offset = 0; offset < 10; offset++) {
      try {
        final tryPort = basePort + offset;
        _httpServer = await HttpServer.bind('0.0.0.0', tryPort);
        _port = tryPort;
        ...
      } catch (_) { /* try next */ }
    }
  }

  The TUI side scans `9753..=9762` (10 ports). The 10 is duplicated on two
  sides of the protocol. If someone ever needs to raise the cap (e.g. 20 apps
  on one device) they have to coordinate both sides.
proposed_action: |
  Extract `static const int portScanCount = 10;` at the class level + a
  dartdoc citing the Rust TUI contract. Better yet, make the scan terminate
  with an observable error so the app crashes loudly instead of silently not
  binding.
risk: low
```

```yaml
id: DART-016
label: D
location: flog_dart/lib/src/flog_server.dart:165-180
title: _startServer silently succeeds-without-binding if all 10 ports are taken
evidence: |
  for (int offset = 0; offset < 10; offset++) {
    try {
      _httpServer = await HttpServer.bind('0.0.0.0', tryPort);
      _port = tryPort;
      _httpServer!.listen(_handleRequest);
      return;
    } catch (_) { /* Port in use — try next */ }
  }
  // falls through without error if all fail

  If the user already runs 10 flog-instrumented Dart apps (or 10 other
  processes hold 9753..9762), FlogServer.start() returns with `_started=true`
  and `_httpServer=null`. Every subsequent `send()` call still records into
  FlogStore but no TUI can ever connect. There is no diagnostic.
proposed_action: |
  Surface the failure: either throw (let the caller decide) or set a flag
  exposed by `FlogServer.instance` and log one line via `print()`. CLAUDE.md
  already allows debug-mode print. Also add a dartdoc example showing how to
  catch the bind failure.
risk: low
```

```yaml
id: DART-017
label: D
location: flog_dart/lib/src/flog_server.dart:261-280
title: _handleReplay fires-and-forgets Dio.request with no error surfacing
evidence: |
  void _handleReplay(Map<String, dynamic> data) {
    if (_dio == null) return;
    ...
    _dio!
        .request(url,
            data: data['body'],
            options: Options(method: method, headers: headers))
        .ignore();
  }

  `.ignore()` drops all errors. If the replay request fails (DNS, timeout,
  auth) the TUI sees nothing happen — the user cannot distinguish "replay
  succeeded but server returned nothing interesting" from "replay silently
  died before leaving the device".
proposed_action: |
  Redesign sketch: swallow errors but emit a diagnostic message, either
  through FlogHttpInterceptor (which will fire naturally if replay Dio has
  the interceptor) or via a dedicated `{type:'replay_result', id:..., ok:...}`
  frame. The existing interceptor on _dio should in fact log the replay —
  verify this end-to-end.
risk: low
```

```yaml
id: DART-018
label: D
location: flog_dart/lib/src/flog_web_socket.dart:32-116
title: FlogWebSocket.fromChannel and primary constructor duplicate ~40 lines of setup
evidence: |
  FlogWebSocket(Uri uri, {Iterable<String>? protocols})
      : _channel = WebSocketChannel.connect(uri, protocols: protocols),
        _id = nextNetId(),
        _start = DateTime.now() {
    if (flogEnabled) { emitNet({'id': _id, 't': 'open', ...}); }
    stream = _channel.stream.map((message) { ... }).handleError(...);
  }

  FlogWebSocket.fromChannel(this._channel, {required String url})
      : _id = nextNetId(),
        _start = DateTime.now() {
    if (flogEnabled) { emitNet({'id': _id, 't': 'open', ...}); }
    stream = _channel.stream.map((message) { ... }).handleError(...);
  }

  The bodies are byte-identical except for the url source. Any future change
  (e.g. asBroadcastStream for DART-006) must be made twice.
proposed_action: |
  Extract `_init(String url)` called from both constructors' body. Or
  collapse into a single factory `FlogWebSocket.connect(uri, ...)` and
  `FlogWebSocket.wrap(channel, url: ...)` that share an `_init`.
risk: low
```

```yaml
id: DART-019
label: D
location: flog_dart/lib/src/flog_web_socket.dart:168-188
title: Binary format `<binary: N bytes>` is a magic string repeated in formatter + size
evidence: |
  static String _formatMessage(dynamic message) {
    if (message is String) {
      return message;
    } else if (message is List<int>) {
      return '<binary: ${message.length} bytes>';
    } else {
      return message.toString();
    }
  }

  The TUI's WS Chat View needs to know this marker to render "binary" pills.
  The string is not shared across the protocol boundary; any drift breaks the
  display without a type-level signal.
proposed_action: |
  Either (a) emit a structured marker: `{'data': null, 'size': N,
  'binary': true}` instead of stringifying, letting the TUI render its own
  label, or (b) extract a named constant both sides of the protocol can
  reference. (a) is cleaner; the TUI's ws_chat.rs `has_binary_content` probes
  would simplify.
risk: low
```

```yaml
id: DART-020
label: D
location: flog_dart/lib/src/flog_store.dart:24
title: FlogStore capacity=50000 is a hardcoded magic constant
evidence: |
  /// Maximum number of messages to retain.
  static const int capacity = 50000;

  A 50k cap sized for typical Flutter app sessions. Not tunable, not
  documented as a memory budget (at e.g. 500 bytes avg per message, cap ≈ 25
  MB). Mirrors `src/domain/store.rs`'s 100K cap but with no cross-reference.
proposed_action: |
  Expose a constructor parameter or static setter for capacity, or at least
  add a dartdoc stating the budget ("≈ 25 MB at 500 bytes/msg") and cross-
  reference the Rust side. Phase 3 decision point.
risk: low
```

```yaml
id: DART-021
label: D
location: flog_dart/lib/flog_dart.dart:21
title: `nextNetId` and `emitNet` exported from public API but are internal helpers
evidence: |
  export 'src/flog_net.dart' show nextNetId, emitNet, flogEnabled;

  `nextNetId()` is a process-wide monotonic counter; `emitNet()` is the raw
  protocol write. Neither is part of the advertised public surface (README
  lists FlogLogger / FlogDio / FlogSseParser / FlogWebSocket as the API).
  Exporting them as public makes them part of the breaking-change surface.
proposed_action: |
  Mark both as internal: remove from the export in flog_dart.dart; keep
  package-internal imports working (they already use `import 'flog_net.dart'`
  via relative path). Only `flogEnabled` needs to stay public so users can
  conditionally gate their own code.
risk: low
```

```yaml
id: DART-022
label: D
location: flog_dart/lib/src/flog_server.dart:48-63, flog_dart/lib/flog_dart.dart:46-60
title: FlogServer.start's appName/appVersion/packageName parameters are dead on Flog.init path
evidence: |
  // FlogServer.start
  void start({
    int port = 9753,
    String appName = 'flutter',
    String appVersion = '',
    String packageName = '',
  }) { ...
    _appName = appName;
    _appVersion = appVersion;
    _packageName = packageName;
  }

  // Flog.init
  static void init({int port = 9753}) {
    if (!flogEnabled) return;
    FlogServer.instance.start(port: port);   // ← passes defaults only
    PackageInfo.fromPlatform().then((info) {
      FlogServer.instance.updateAppInfo(...)  // ← real values arrive later
    })...;
  }

  Flog.init never forwards appName/appVersion/packageName to start(). The
  start() parameters are therefore effectively dead on this path, or useful
  only for legacy direct-start callers.
proposed_action: |
  Either (a) remove the appName/appVersion/packageName params from start()
  since Flog.init is the intended entry point and always uses updateAppInfo,
  or (b) forward Flog.init's explicit override values (none today) + pass the
  defaults through. (a) is simpler and removes a source of drift.
risk: low
```

```yaml
id: DART-023
label: D
location: flog_dart/lib/flog_dart.dart:39-61
title: Flog.init swallows PackageInfo errors silently with empty `.catchError((_) {})`
evidence: |
  PackageInfo.fromPlatform().then((info) {
    FlogServer.instance.updateAppInfo(
      appName: info.appName,
      appVersion: info.version,
      packageName: info.packageName,
    );
  }).catchError((_) {});

  If PackageInfo fails (rare but possible on some platforms / test harness),
  the TUI sees `app='flutter'`, `appVersion=''`, `packageName=''` forever. No
  diagnostic, no retry. Silent swallow violates the "verify before delivery"
  feedback principle (project memory).
proposed_action: |
  Log the error via FlogLogger at warning level (which cleanly no-ops in
  release via flogEnabled) and record it into FlogStore so the TUI can
  display it.
risk: low
```

```yaml
id: DART-024
label: D
location: flog_dart/README.md:1-48
title: README lacks mock rules + replay + removal docs; lists features but not usage
evidence: |
  README currently shows:
    - Flog.init() one-liner
    - FlogDio baseUrl + sse()
    - FlogLogger
    - System capture bullet list

  It does NOT document:
    - How mock rules flow from the TUI into the app
    - That interceptor ordering is handled automatically
    - How to remove/disable flog interceptors (follow-up to DART-011)
    - Tree-shaking in release (advertised but not verified in docs)
    - That FlogDio's sse() wraps a stream endpoint, not a generic response
proposed_action: |
  Add sections: "Mock rules", "Replay", "Release builds" (flogEnabled +
  -DFL0G_ENABLED override), and "Removing flog". pub.dev scoring + discovery
  benefit; this is the first-impression file on pub.dev.
risk: low
```

```yaml
id: DART-025
label: D
location: flog_dart/CHANGELOG.md:1-20
title: CHANGELOG jumps 0.2.0 → 0.7.1, missing five releases of history
evidence: |
  ## 0.7.1
  - FlogHttpInterceptor onError emits HTTP status/headers/body for 4xx/5xx.

  ## 0.2.0
  - FlogHttpInterceptor, FlogSseParser, FlogWebSocket, emitNet.

  ## 0.1.0
  - FlogLogger.

  pubspec is at 0.7.1. Versions 0.3 / 0.4 / 0.5 / 0.6 / 0.7.0 are missing —
  which means major features (FlogServer/FlogStore Direct-Socket pivot,
  FlogMockInterceptor, FlogDio drop-in, Flog.init system hooks, appName/
  appVersion auto-detect, replay) have no changelog entry. pub.dev scoring
  penalizes this; users cannot tell what changed between versions they
  upgrade across.
proposed_action: |
  Back-fill the missing entries. Cross-reference git log for dates.
  Ongoing: CONTRIBUTING.md (from Phase 5) should require a CHANGELOG entry
  per user-visible change, including "internal refactor" stubs.
risk: low
```

```yaml
id: DART-026
label: D
location: flog_dart/lib/src/flog_dio.dart:137-168
title: FlogDio.sse assumes response.data is non-null and crashes on empty-body streams
evidence: |
  final response = await _inner.request<ResponseBody>(...);
  final url = response.requestOptions.uri.toString();
  final wrappedStream = FlogSseParser.wrap(
    response.data!.stream,   // ← ! bang
    url: url,
    method: method,
  );

  If the server returns a 204 No Content or the interceptor chain rejects
  with `response.data == null`, the bang throws a NullCheckError that does
  not surface as an SSE error event to the TUI — the caller just sees
  "Null check operator used on a null value".
proposed_action: |
  Handle null: return an SseResponse with an empty broadcast stream and an
  emitted `err` frame so the TUI shows the failure. Bang operators on
  network-driven values are an anti-pattern.
risk: medium
```

```yaml
id: DART-027
label: D
location: flog_dart/lib/src/flog_http_interceptor.dart:102-145
title: Mocked-response path duplicates ~30 lines of request-emit logic
evidence: |
  // onResponse
  final isMocked = response.requestOptions.extra['flog_mocked'] == true;
  if (isMocked) {
    final id = nextNetId();
    ...
    final reqData = {'id': id, 't': 'req', 'p': 'http', ...};
    if (includeRequestHeaders) reqData['headers'] = ...;
    emitNet(reqData);
    final resData = {'id': id, 't': 'res', 'p': 'http', 'status': ..., 'mocked': true};
    ...
    emitNet(resData);
    handler.next(response);
    return;
  }

  The mock branch re-invents the `req` payload that onRequest already knows
  how to build. Any future field (e.g. query string handling, truncation
  rules) must be kept in sync manually. Root cause: the mock interceptor
  resolves before FlogHttpInterceptor.onRequest fires, so the `req` emit is
  skipped and then re-emitted here.
proposed_action: |
  Redesign: either (a) have FlogMockInterceptor emit its own `req` frame
  before `handler.resolve`, turning the HTTP interceptor's responsibility
  into "emit `res` for the id I see in extra", or (b) extract a shared
  `_emitReq(RequestOptions)` helper on a common internal module. (a) keeps
  responsibility clearer.
risk: low
```

```yaml
id: DART-028
label: E
location: flog_dart/lib/flog_dart.dart:30-38
title: Duplicated dartdoc example between library header and Flog.init class
evidence: |
  Library header (line 4-13) shows a `flog()` call (wrong — DART-003).
  Flog class (line 30-38) shows correct `Flog.init()` call.
  After DART-003 is fixed, the two become near-duplicates with only text
  framing differing.
proposed_action: |
  After DART-003 fix, delete the library-header code fence and keep only
  one authoritative example (the Flog class dartdoc).
risk: low
```

```yaml
id: DART-029
label: E
location: flog_dart/lib/src/flog_net.dart:13-16
title: `_nextId` is package-private state but the helper is exported publicly
evidence: |
  int _nextId = 1;
  int nextNetId() => _nextId++;

  The counter is not reset-able, not test-visible, and `nextNetId` is
  exported from flog_dart.dart (see DART-021). Tests that want to assert
  "request got id 1" are forced to run in the same isolate as any prior
  test that called nextNetId. Mechanical fix: make nextNetId internal and
  the counter reset-able for tests.
proposed_action: |
  After DART-021 un-exports nextNetId, add `@visibleForTesting
  void resetNextNetId() { _nextId = 1; }` in flog_net.dart. Mechanical.
risk: low
```

```yaml
id: DART-030
label: A
location: flog_dart/lib/src/flog_server.dart:216-219
title: Error handler silently swallows per-client listen errors
evidence: |
  ws.listen(
    (message) { if (message is String) _onMessage(message, ws); },
    onError: (_) => _removeClient(ws),
    onDone: () => _removeClient(ws),
  );

  Behavior is correct (a broken WS is removed), but the error is dropped on
  the floor. No diagnostic, no rate-limit on reconnect storms. Not a bug —
  the server keeps running — just opaque.
proposed_action: |
  Redesign sketch: accept the error object in onError and record a
  structured log (via FlogLogger) at debug level so operators can distinguish
  "client dropped" from "malformed frame". Current behavior correct, redesign
  improves observability only.
risk: low
```

```yaml
id: DART-031
label: A
location: flog_dart/lib/src/flog_server.dart:246-259
title: _handleSubscribe comment admits the "temporarily remove from set" dance is semantic-only
evidence: |
  // Temporarily remove from broadcast set so the client doesn't receive
  // duplicates of messages that are both in the buffer and newly produced.
  // (In practice, Dart is single-threaded so no new messages arrive during
  // replayTo, but this is semantically correct.)
  _clients.remove(ws);
  FlogStore.instance.replayTo(ws);
  _clients.add(ws);

  Behavior correct (Dart's single-threaded event loop makes the remove/add
  redundant). Ugly because (a) the no-op dance plus disclaimer comment
  together take more space than just doing the add+replay, and (b) if Dart
  ever gains true threading, the remove/add is still not a synchronization
  primitive — it's a race, not a mutex.
proposed_action: |
  Either delete the remove/add pair and update the comment to "Dart is
  single-isolate; no synchronization needed", or if concurrency is expected
  in the future, use a real primitive (queue the broadcast, drain after
  replay). Current Phase-3 action: delete and simplify.
risk: low
```

```yaml
id: DART-032
label: A
location: flog_dart/lib/src/flog_sse_parser.dart:66-142, 150-206
title: _passThroughSse is a near-identical copy of the main wrap loop
evidence: |
  static Stream<String> wrap(...) {
    if (!flogEnabled) {
      return _passThroughSse(byteStream);
    }
    ...
    // ~70 lines of byte-decode + line-split + controller-push logic
  }

  static Stream<String> _passThroughSse(Stream<List<int>> byteStream) {
    // ~50 lines of byte-decode + line-split + controller-push logic
    // identical except for `seq++ / emitEvent / controller.add` calls
  }

  Two copies of the UTF-8-safe chunk decoder + line splitter are a
  maintenance liability. Today's behavior is correct (see separately:
  DART-001/002 for parser bugs), but any fix to one path must be copied to
  the other.
proposed_action: |
  Extract the shared body into a `_createParser(bool withFlog)` helper that
  takes a callback `(data) -> void`; the flog branch passes a callback that
  emits+forwards, the non-flog branch passes a callback that only forwards.
  Side effect: fixes to DART-001 automatically apply to both modes.
risk: low
```

## Summary

| label | count |
|---|---|
| A | 3 |
| B | 9 |
| C | 0 |
| D | 18 |
| E | 2 |
