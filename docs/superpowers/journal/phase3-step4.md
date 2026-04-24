# Phase 3 Step 3.4 — flog_dart redesign

- **Started from master HEAD:** `e30017d`
- **Spec:** `docs/superpowers/specs/2026-04-22-project-cleanup-design.md` §5, §5.8
- **Audit:** `docs/superpowers/audit/04-flog-dart.md` (30 entries: 3 A, 9 B, 18 D)
- **Plan:** `docs/superpowers/plans/2026-04-24-phase3-step4-flog-dart.md`

## Baseline

Before Step 3.4: `cd flog_dart && flutter test` reported **84 pass · 3 skip · 1 fail** — the lone red was the DART-001/002 compile-time failure in `flog_sse_parser_test.dart` (`Member not found: 'FlogSseParser.wrapTyped'`).

After Step 3.4: **131 pass · 2 skip · 0 fail**. The +47 test delta is the DART-001/002 SSE suite unlocking (45 tests) plus two new regression tests (DART-007 char-boundary walk-back, DART-026 null-body guard). One previously-skipped UNTESTABLE test (DART-026) turned into a real passing test; two red-lock tests (DART-006, DART-007) flipped from "locks the bug" to "locks the fix"; the DART-009 lock test was rewritten around `FlogStore.snapshotForTesting`.

Rust side: `cargo test --lib` reports **710 pass · 0 fail** — unchanged.

## Entry-by-entry disposition

| ID | Class | Action | Commit |
|----|-------|--------|--------|
| DART-001 | B | Parser rewritten to full SSE spec | `6179631` |
| DART-002 | B | `SseEvent` + `wrapTyped` added (additive) | `6179631` |
| DART-003 | B | Misleading library dartdoc shortened | `ff4a710` |
| DART-004 | B | `FlogMockInterceptor.onRequest` early-return on `!flogEnabled` | `b0f1e55` |
| DART-005 | B | Doc-only fix: describe real `mock_sync` WebSocket channel (no VM Service extension exists in practice) | `804ebc0` |
| DART-006 | B | `stream` now `asBroadcastStream()`; red-lock test flipped | `c70d6f0` |
| DART-007 | B | `_truncate` now UTF-8-byte accurate with char-boundary walk-back | `e09805d` |
| DART-008 | B | `_idMap`/`_startMap` replaced by `options.extra` stamping; no more leaks | `77eabf8` |
| DART-009 | B | `emitNet` clones the caller map before stamping `type`/`ts` | `d72ceed` |
| DART-010 | D | `flog_dio.dart` 504 → 475 lines; `sse()` moved to `flog_dio_sse.dart` | `a583de6` |
| DART-011 | D | Dartdoc on FlogDio ctor + debug-mode ordering asserts | `ddbcc02` |
| DART-012 | D | Static rule list rationale documented as a deliberate ack | `ddbcc02` |
| DART-013 | D | Match semantics (substring / case-sensitive / first-match) documented on class dartdoc | `804ebc0`, `ddbcc02` |
| DART-014 | D | `kFlogMockedExtrasKey` const replaces the `'flog_mocked'` literal | `ddbcc02` |
| DART-015 | D | `FlogServer.portScanCount = 10` const, dartdoc references TUI range | `2879350` |
| DART-016 | D | All-ports-taken failure now logs via debugPrint | `2879350` |
| DART-017 | D | Replay errors logged via debugPrint (no more `.ignore()`) | `2879350` |
| DART-018 | D | Constructors share `_initFromChannel(url)` (already done with DART-006; commit docs it) | `c70d6f0`, `a590e9f` |
| DART-019 | D | `binaryFormatPrefix` / `binaryFormatSuffix` / `formatBinaryLabel` | `a590e9f` |
| DART-020 | D | `FlogStore.defaultCapacity` with dartdoc (≈ 25 MB budget) | `666a8a9` |
| DART-021 | D | `@internal` annotation added; public export kept for v0.x back-compat with ignore directive | `27b8157` |
| DART-022 | D | Dead params removed from `FlogServer.start`; tests updated | `3fadd2f` |
| DART-023 | D | `Flog.init` PackageInfo error now debugPrinted | `7820666` |
| DART-024 | D | **Deferred to Phase 5** (README docs) | — |
| DART-025 | D | **Deferred to Phase 5** (CHANGELOG backfill) | — |
| DART-026 | D | `flogSse` returns empty stream on null body; red-lock test flipped to passing regression | `a583de6`, `6c607f8` |
| DART-027 | D | `_emitReq` + `_emitHttpCompletion` helpers unify onResponse paths | `4174080` |
| DART-030 | A | Ack: inline comment on the per-client onError handler | (Task 19 inline) |
| DART-031 | A | Ack: inline comment on the `_handleSubscribe` remove/add dance | (Task 19 inline) |
| DART-032 | A | Ack: `_run<T>` replaces the old duplicate-loop; comment on FlogSseParser documents it | (Task 19 inline) |

## Red-line adherence (§5.8)

- **Public API signatures preserved**, with the explicit exceptions approved by the plan:
  - DART-001/002: `SseEvent` + `wrapTyped` **added** (additive, B-class, approved).
  - DART-003: library-header dartdoc **shortened** (doc-only).
  - DART-022: `FlogServer.start`'s three dead app-identity params **removed** after confirming zero external callers outside the red-lock test that was itself updated.
- DART-021 did NOT remove the public export; `@internal` + ignore-directive is the Phase-3 compromise per plan.
- No new Dart dependencies except the already-transitively-available `package:meta`, which the plan implicitly required by asking for `@internal`.
- `FlogMockInterceptor.onRequest` and `FlogHttpInterceptor` contracts preserved; all changes are additive (guard, helper refactor) or purely internal (extra-key stamping).

## Red-lock test updates

Phase 2.5B Task 13 planted red-lock tests that characterize buggy behavior so it could not regress silently. Where a Step 3.4 task fixed the lock, the test was rewritten in the same commit:

- `flog_web_socket_test.dart` — DART-006 flipped from "second listen throws StateError" to "multiple listeners coexist".
- `flog_http_interceptor_test.dart` — DART-007 CJK test flipped from "locks the code-unit bug" to "locks the byte-budget + char-boundary safety"; new case covers the walk-back path.
- `flog_net_test.dart` — DART-009 flipped from "caller map is mutated" to "caller map is untouched"; timestamp assertion now reads `FlogStore.snapshotForTesting`.
- `flog_server_test.dart` — DART-022 updated to lock the new (port-only) signature.
- `flog_dio_test.dart` — DART-026 converted from skipped UNTESTABLE to live passing regression via `_NullBodyResolvingInterceptor`.

Header comments on each test file were updated to mark the audit IDs as "FIXED Phase 3 Step 3.4".

## File sizes (`flog_dart/lib/**/*.dart`)

All under the §5.5 500-line green zone:

```
143 flog_dart.dart
482 src/flog_dio.dart              (was 504)
 77 src/flog_dio_sse.dart          (new)
305 src/flog_http_interceptor.dart
141 src/flog_mock_interceptor.dart
 54 src/flog_net.dart
334 src/flog_server.dart
417 src/flog_sse_parser.dart
 92 src/flog_store.dart
188 src/flog_web_socket.dart
```

## Notes / deviations

- **DART-005** is explicitly a documentation-only fix (the real channel is already the WebSocket `mock_sync` frame). The plan's Task 4 language suggested registering a VM Service extension but the audit's proposed_action and the pre-existing red-lock test both mark the VM Service route as a documentation myth. We updated three docstrings (flog_mock_interceptor.dart, CLAUDE.md, src/domain/mock.rs) to describe the real path and did not add a new VM Service extension.
- **DART-021** kept the public export per §5.8 red line ("no public API change without B-class + approval"); the `@internal` annotation + `// ignore: invalid_export_of_internal_element` is the compromise documented in the plan.
- **DART-024 / DART-025** (README + CHANGELOG) remain deferred to Phase 5.

## Exit gates

- [x] `flutter test` — 131 pass, 2 skip, 0 fail. 0 unexpected red, 0 unexpected green.
- [x] `cargo test --lib` — 710 pass, 0 fail.
- [x] `dart analyze lib/` — 1 pre-existing info (dart:ui vs flutter/foundation); no new warnings.
- [x] All `flog_dart/lib/**/*.dart` < 500 lines.
- [x] Every B entry passes or is explicitly doc-only (DART-005).
- [x] Every D entry has a code fix, an ack comment, or an explicit Phase-5 defer note.
