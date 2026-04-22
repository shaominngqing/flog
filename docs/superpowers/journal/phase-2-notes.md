# Phase 2 execution notes

## Scope re-verification discoveries

During plan writing and Task 2 execution, we discovered that several Audit
E-class entries had cross-check issues. Each was resolved conservatively.

### DOM-009 / DOM-023 misjudgements → DOM-025 added

Phase 1 audit entry DOM-009 claimed SseChunk/WsMessage had dead
`seq`/`size`/`timestamp` fields. Plan writing believed them live based on
grep that matched the same field names on different types (LogEntry,
NetworkEntry — both live). Phase 2 Task 2 executing the deletion ran into
compiler dead-code warnings proving the subset on SseChunk + WsMessage is
actually write-only (constructed, never read).

Resolution:
- Subagent correctly refused to delete (protocol-shape decision belongs
  to Phase 3)
- Kept `#[allow(dead_code)]` markers
- Added new D-class entry **DOM-025** to `02-domain.md` + `00-index.md`
  describing the write-only fields and Phase 3 options

This exemplifies the spec §5.2 rule: "途中发现新架构问题 → 不自作主张，加入
Audit → 下次 planning 处理."

### DOM-010 `MockRule::hit_count` / `find_match`

Audit claimed dead. Cross-check confirmed live (used inside
`src/domain/mock.rs` test cases). Compile target without `--test` sees
them as dead, triggering warning.

Resolution: kept the `#[allow(dead_code)]` markers — the pattern "item is
only used in tests but we want a runtime-facing API defined" is a known
Rust dead-code-warning false positive.

### DOM-010 `MockRuleStore::enabled_count`

Audit said delete. Subagent confirmed zero non-test callers. Wrapped the
method as `#[cfg(test)]` so it compiles only with the test target. This
pattern keeps the test helper alive without a `#[allow(dead_code)]`
escape hatch.

### DOM-010 `MockRuleStore::is_empty`

Audit said delete. Subagent tried, but `len()` being live makes clippy
fire `len_without_is_empty`. Kept with `#[allow(dead_code)]` marker —
the pair len/is_empty is an idiomatic API even when one is unused.

### Dead parser `Continuation` variant (DOM-012 follow-up)

User approved removal in Phase 1 C-review. Subagent successfully removed
the variant and its 5 supporting tests from src/parser/generic.rs, the
trait method from src/parser/mod.rs, and stub implementations from
keyword.rs + structured.rs. Test count drop: 5 (expected, paired with
removed code).

## flog_dart scope (Task 3)

All two E-class entries (DART-028, DART-029) depend on upstream
D/A/B-class work that belongs to Phase 3. They remain marked E in the
audit but execute in Phase 3 as sub-steps of their dependencies.

`dart analyze flog_dart/` baseline run:
- 163 issues total — the vast majority (160+) are DART-001/002 test vs
  lib mismatch (Phase 3 SSE parser rewrite makes them go green)
- 2 info-level findings not covered by existing audit:
  - `lib/src/flog_server.dart:11` — unused `import 'dart:ui';` (shadowed
    by `package:flutter/foundation.dart`). Single-line mechanical
    deletion but not listed in Phase 1 audit.
  - `test/flog_sse_parser_test.dart:4` — `flutter_test` not in pubspec
    dependencies. Part of DART-002 scope (the test file is stale against
    lib/). Phase 3 handles.

Decision: **defer the `dart:ui` unused-import to Phase 3 DART step**
rather than perform an ad-hoc Phase 2 edit outside the planned scope.
Keeping Phase 2 strictly to the planned/audited E-class items preserves
the "no design judgement, no scope creep" discipline.

If, after Phase 3 DART step completes, the `dart:ui` fix has not been
made, it falls through to Phase 4/5 cleanup or a follow-up commit.

### Phase 2 delta summary

Clippy:
- Before (Phase 2 entry): 34 warnings + errors
- After Task 1 (transport): 31
- After Task 2 (domain+parser): 19
- After Task 4 (UI+event+app): will be recorded at Task 5

Tests:
- Before: 212 lib + 217 bin + 1 integration + 0 doc = 430 total (test-path counts)
- After Task 2: 207 lib + 212 bin + 1 integration + 0 doc = 420
- Delta: -5 from removal of parser Continuation variant tests

Files modified (so far):
- Transport: `src/transport/usbmuxd.rs`, `src/transport/adb.rs`
- Domain: `src/domain/entry.rs`, `filter.rs`, `mock.rs`, `network.rs`,
  `network_filter.rs`, `network_store.rs`, `store.rs`, `structured_parser.rs`
- Parser: `src/parser/generic.rs`, `keyword.rs`, `mod.rs`, `structured.rs`
- flog_dart: none
- UI: pending Task 4

Deferred to Phase 3 (tracked via `#[allow] + comment`):
- `LogLevel::from_str` → `should_implement_trait` (domain entry.rs)
- Pending at Task 4: `render_json_section_with_depth`,
  `push_device_top` `too_many_arguments`
- DART-028, DART-029 → upstream-dependent
- DART dart:ui unused-import → Phase 3 DART step
- DOM-025 (newly discovered write-only fields) → Phase 3 domain step
