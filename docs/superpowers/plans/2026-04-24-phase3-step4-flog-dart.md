# Phase 3 Step 3.4 — flog_dart Redesign

> REQUIRED SUB-SKILL: superpowers:subagent-driven-development or executing-plans.

**Goal:** Resolve 30 flog_dart audit entries. 9 B-fixes, 16 D redesigns (DART-024/025 deferred to Phase 5), 3 A acks.

**Architecture:** flog_dart is published on pub.dev; external users depend on `FlogDio`, `FlogLogger`, `FlogSseParser`, `FlogWebSocket`, `Flog.init()`. Spec §5.8 red line: **public API signatures stay the same**, EXCEPT where a B-class audit entry documents a bug that requires changing them (DART-001/002 SSE parser API, DART-003 add missing flog() fn or remove misleading docstring).

**Parent spec:** `docs/superpowers/specs/2026-04-22-project-cleanup-design.md` §5
**Audit:** `docs/superpowers/audit/04-flog-dart.md`

## Entries

| Id | Class | Summary |
|---|---|---|
| DART-001 | B | SSE parser drops events after first `data:` line — rewrite parser loop |
| DART-002 | B | Untracked test expects `wrapTyped` + `SseEvent` — implement the API |
| DART-003 | B | Docstring references `flog()` top-level fn — either add it or remove doc |
| DART-004 | B | FlogMockInterceptor.onRequest runs without flogEnabled guard — add early return |
| DART-005 | B | VM Service ext not registered — call `developer.registerExtension` |
| DART-006 | B | `stream` single-subscription despite broadcast doc — `asBroadcastStream` |
| DART-007 | B | `_truncate` char-vs-byte — fix budget calculation |
| DART-008 | B | `_idMap` leak on early reject — clean up on onError/onResponse returning before |
| DART-009 | B | `emitNet` mutates caller map — clone first |
| DART-010 | D | `FlogDio` 504 lines — split into auxiliary + core |
| DART-011 | D | Interceptor ordering unguarded — assert order in ctor; or freeze `interceptors` list |
| DART-012 | D | Static rule list — document rationale (process-wide by design per Sync API) |
| DART-013 | D | Mock match semantics load-bearing — document + extract `MockMatcher` helper |
| DART-014 | D | `flog_mocked` magic string — `const _MOCKED_EXTRAS_KEY = 'flog_mocked'` |
| DART-015 | D | Port-scan `[base, base+9]` — name `const _PORT_SCAN_COUNT = 10` + doc |
| DART-016 | D | `_startServer` silent fail on all ports taken — throw or log |
| DART-017 | D | Replay fire-and-forget — log replay error |
| DART-018 | D | `FlogWebSocket.fromChannel` duplicate setup — extract `_initFromChannel` |
| DART-019 | D | `<binary: N bytes>` magic — `const _BINARY_FORMAT` + helper |
| DART-020 | D | `FlogStore` 50000 capacity — named const + doc |
| DART-021 | D | `nextNetId` / `emitNet` public — move to `internal/` subdir or make private |
| DART-022 | D | `FlogServer.start` dead params — remove or wire |
| DART-023 | D | `Flog.init` silent catchError — log the error |
| DART-024 | D | README docs — **defer to Phase 5** |
| DART-025 | D | CHANGELOG gaps — **defer to Phase 5** |
| DART-026 | D | `FlogDio.sse` crashes on null body — null-safety guard |
| DART-027 | D | Mocked-response path duplication — extract `_emitHttpCompletion` |
| DART-030 | A | Per-client listen error swallow — ack |
| DART-031 | A | `_handleSubscribe` "temporarily remove from set" — ack |
| DART-032 | A | `_passThroughSse` near-duplicate of wrap loop — can unify but low priority — ack |

## Tasks

Execute in order. One commit per task. Skip DART-024 + DART-025 (Phase 5).

### Task 0: pre-flight
- Verify HEAD `39efe3d`, Rust tests green.
- Check `cd flog_dart && flutter test` — currently failing tests: all DART-001+002 reds in flog_sse_parser_test.dart + any DART-003..009 reds that are `skip:`'d or `should_panic`'d. Record baseline.

### Task 1: DART-001 + DART-002 SSE parser rewrite (HIGH)
- Read `flog_dart/test/flog_sse_parser_test.dart` — the test file defines the expected API (`FlogSseParser.wrapTyped`, `SseEvent { event, data, id, retry, comments }`).
- Rewrite `flog_dart/lib/src/flog_sse_parser.dart`:
  - Add `class SseEvent` with fields per test expectations
  - Add `static Stream<SseEvent> wrapTyped(Stream<String> raw)` method implementing the full SSE spec (BOM strip, CRLF, multi-line data concatenation, retry field, comments ignored, stream-end flush, multiple data lines in one chunk)
  - Keep existing pass-through `wrap` method for back-compat
- Run `flutter test` — `flog_sse_parser_test.dart` becomes fully green.
- Commit: `fix(flog_dart): SSE parser wrapTyped + SseEvent — DART-001 + DART-002`

### Task 2: DART-003 flog() top-level fn
- Option A (preferred): remove the misleading docstring reference. Edit `flog_dart/lib/flog_dart.dart` to remove any `flog()` references in library dartdoc.
- Option B: add a thin top-level `flog()` fn that returns a singleton logger.
- Pick A — simpler + no new API exposure.
- Commit: `docs(flog_dart): remove misleading flog() docstring — DART-003`

### Task 3: DART-004 FlogMockInterceptor flogEnabled guard
- `flog_dart/lib/src/flog_mock_interceptor.dart` onRequest: add `if (!flogEnabled) return handler.next(options);` as first line.
- Verify tests.
- Commit: `fix(flog_dart/mock): guard onRequest with flogEnabled — DART-004`

### Task 4: DART-005 register VM Service extension
- `flog_dart/lib/src/flog_server.dart` or `flog_mock_interceptor.dart`: call `developer.registerExtension('ext.flog.syncMockRules', (method, params) async { ... })` from init.
- Handler parses params.rules JSON into FlogMockInterceptor static list.
- Test: simulate `Service.getInfo().callMethod('ext.flog.syncMockRules', {rules: '...'})` or assert the extension is registered.
- Commit: `fix(flog_dart): register ext.flog.syncMockRules VM service extension — DART-005`

### Task 5: DART-006 FlogWebSocket stream broadcast
- In `flog_dart/lib/src/flog_web_socket.dart`: wrap the `stream` getter with `.asBroadcastStream()` so multiple `listen()` calls don't StateError.
- Verify existing test `DART-006 stream broadcast` flips from red to green.
- Commit: `fix(flog_dart/ws): expose stream as broadcast — DART-006`

### Task 6: DART-007 _truncate byte budget
- `flog_dart/lib/src/flog_http_interceptor.dart` `_truncate`: change `.length` (char count) to `.codeUnits.length` or explicit UTF-8 byte count; test with CJK input.
- Commit: `fix(flog_dart/http): _truncate byte-accurate — DART-007`

### Task 7: DART-008 _idMap leak on early reject
- `flog_dart/lib/src/flog_http_interceptor.dart`: in `onRequest`, `onError`, `onResponse`, ensure `_idMap.remove(key)` runs in every exit path (not just the happy path).
- Commit: `fix(flog_dart/http): _idMap cleanup on all exit paths — DART-008`

### Task 8: DART-009 emitNet don't mutate caller map
- `flog_dart/lib/src/flog_net.dart`: in `emitNet(Map<String, dynamic> data)`, `final m = Map<String, dynamic>.from(data);` then mutate `m`, not `data`.
- Commit: `fix(flog_dart/net): emitNet clones caller map — DART-009`

### Task 9: DART-010 FlogDio split
- `flog_dart/lib/src/flog_dio.dart` is 504 lines. Split:
  - Core delegate → keep in flog_dio.dart (~300 lines)
  - SSE convenience method + body wrapping → new `flog_dart/lib/src/flog_dio_sse.dart`
- Or: move `sse()` method body into a free function in a new file; flog_dio's `sse()` delegates.
- Verify file sizes < 500.
- Commit: `refactor(flog_dart/dio): split sse convenience into flog_dio_sse — DART-010`

### Task 10: DART-011..014 interceptor + mock consts
- DART-011 ack: document interceptor ordering in FlogDio ctor dartdoc + add assertion `assert(dio.interceptors[0] is FlogMockInterceptor);` in debug mode
- DART-014: `const _MOCKED_EXTRAS_KEY = 'flog_mocked';` + use it instead of literal
- DART-012 ack + DART-013 doc: module-level dartdoc explaining rule list semantics
- Commit: `refactor(flog_dart): name mock extras key + document semantics — DART-011/012/013/014`

### Task 11: DART-015..017 server constants + error surfacing
- DART-015: `const _PORT_SCAN_COUNT = 10;`
- DART-016: when all ports taken, `throw FlogServerException('no ports available')` or log warning
- DART-017: replay errors logged via `debugPrint`
- Commit: `fix(flog_dart/server): port-scan const + log replay errors — DART-015/016/017`

### Task 12: DART-018..019 ws dedup + binary const
- DART-018: `FlogWebSocket.fromChannel` delegates to a private `_initFromChannel` shared with primary constructor
- DART-019: `const _BINARY_FORMAT_PREFIX = '<binary: ';` with helper `_formatBinary(int bytes)`
- Commit: `refactor(flog_dart/ws): dedup constructors + binary format const — DART-018/019`

### Task 13: DART-020 FlogStore capacity const
- `const FlogStore.defaultCapacity = 50000;` with dartdoc explaining choice
- Commit: `refactor(flog_dart/store): named capacity constant — DART-020`

### Task 14: DART-021 public API minimize
- Move `nextNetId` + `emitNet` to `lib/src/internal/net_ids.dart` with `@internal` annotation.
- Remove from `lib/flog_dart.dart` exports.
- **CAUTION**: spec §5.8 red line says no public API change without B-class + user approval. This is D-class. Check if any external caller relies on the export.
- Decision: **ADD `@internal` annotation but keep pub export for back-compat**; Phase 5 README update discourages use; future v1.0 release removes.
- Commit: `refactor(flog_dart): mark nextNetId/emitNet @internal — DART-021`

### Task 15: DART-022 dead params
- `FlogServer.start(appName, appVersion, packageName)` — check if any of these are actually used. If fully dead, remove. If partially used (e.g., `appName` set but never read), remove unused fields.
- **CAUTION**: signature change could affect external users. Check if Flog.init is the only caller.
- If Flog.init is indeed the only path: safe to remove or make Optional with default null.
- Commit: `refactor(flog_dart/server): remove dead start() params — DART-022`

### Task 16: DART-023 init catchError logging
- `flog_dart/lib/flog_dart.dart` Flog.init: `.catchError((e, st) { debugPrint('flog_dart: PackageInfo failed: $e'); });`
- Commit: `fix(flog_dart/init): log PackageInfo error — DART-023`

### Task 17: DART-026 sse null body guard
- `flog_dart/lib/src/flog_dio.dart` sse(): check `response.data` nullability explicitly; on null, return `const Stream<String>.empty()` or equivalent.
- Commit: `fix(flog_dart/dio): sse null body guard — DART-026`

### Task 18: DART-027 mock response dedup
- `flog_dart/lib/src/flog_http_interceptor.dart`: extract `_emitHttpCompletion(entry, response)` helper; both `onResponse` and mock path use it.
- Commit: `refactor(flog_dart/http): _emitHttpCompletion helper — DART-027`

### Task 19: A-class acks + journal
- DART-030/031/032: inline comments pointing to audit IDs.
- DART-024/025: explicit "deferred to Phase 5" in journal.
- Write `docs/superpowers/journal/phase3-step4.md`.
- Run `flutter test` + update flog_dart characterization tests as needed (some tests that were "red locks" now go green; update their `skip:` / expected-failure markers).
- Commit: `docs(journal): Phase 3 Step 3.4 — flog_dart redesign complete`

## Exit gates

- `cd flog_dart && flutter test` — 0 failed, 0 unexpected red, 0 unexpected green
- Rust side `cargo test` / `clippy -D warnings` / `fmt` — unchanged (Rust shouldn't be affected)
- All `flog_dart/lib/**/*.dart` < 500 lines
- Every B test now passes
- Every D entry has either a code fix or explicit defer-to-Phase-5 note
- Journal complete

## 红线

- `FlogLogger` / `FlogDio` / `Flog.init` / `FlogSseParser` / `FlogWebSocket` public API signatures preserved EXCEPT:
  - DART-001/002 adds `SseEvent` + `wrapTyped` (B-class, additive)
  - DART-003 removes misleading docstring (doc-only)
  - DART-022 may remove dead params (verify zero external callers first)
- No new Dart dependencies beyond what's already in pubspec.
- `FlogMockInterceptor.onRequest` / `FlogHttpInterceptor` contracts preserved.
