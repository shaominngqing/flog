# Phase 2 Journal — Mechanical Cleanup

## 入口
- 日期：2026-04-22
- Git HEAD at entry: `dea1190` (Phase 2 plan commit)
- Baseline tests: 212 lib + 217 bin + 1 integration + 0 doc = 430
- Baseline clippy: 34 warnings + 1 error (PI approximation blocking test compile)
- 执行者：主 Claude + 4 scope subagents (sequential)
- 执行模式：Inline + 停机点（半自动）

## 时间线
- Task 0 pre-flight: baseline recorded, clean tree verified
- Task 1 transport subagent: deleted `UsbDevice`, `list_devices()`,
  `is_available()`. Clippy 34 → 31. Tests still 212+217+1. Zero scope drift.
- Task 2 domain+parser subagent: fixed PI approx error, removed parser
  `Continuation` variant (DOM-012), deleted `Protocol::as_str`,
  `LogStore::clear`, `LogStore::append_continuation`, unused
  `next`/`as_str`/`is_active` on filter enums; added `impl Default`
  for LogStore/NetworkStore/NetworkFilter; scoped `enabled_count` to
  `#[cfg(test)]`; added Phase-3-tracking `#[allow]` on
  `LogLevel::from_str`. Clippy 31 → 19. Tests 212 → 207 (−5 deleted
  Continuation tests).
- Task 2 discovered DOM-025: `SseChunk.seq/size/timestamp` and
  `WsMessage.timestamp` are **write-only** — subagent correctly refused
  to delete (protocol shape decision) and kept `#[allow(dead_code)]`.
  Added as new audit entry in `02-domain.md` + `00-index.md`.
- Task 3 flog_dart: no-op by design. DART-028/029 depend on Phase-3
  upstream work. Discovered a small unused-import in `flog_server.dart`
  (not in Audit) — also deferred to Phase 3 DART step to preserve
  "no ad-hoc scope creep" discipline. Wrote `phase-2-notes.md`.
- Task 4 UI+event+app subagent: 6 mechanical clippy equivalence rewrites
  (2× unnecessary_cast, manual_strip, useless_conversion,
  items_after_test_module, 4× vec_init_then_push), added `impl Default`
  for `App` and `NetworkState`, deleted empty-line-after-doc, removed
  UI-019 `expand_all`/`collapse_all` + 3 associated tests, deferred 2
  `too_many_arguments` with tracking comments. Tests 207 → 204.
- Task 5 final sweep: discovered plan gap — `ClientMessage
  large_enum_variant` in `src/input/protocol.rs` was not anyone's scope.
  Added `#[allow(clippy::large_enum_variant)]` with Phase-3-tracking
  comment directly. Ran `cargo fmt` (cleaned pre-existing fmt drift on
  master). Verified all gates pass.

## 关键统计（Phase 2 出口）

| 指标 | Entry | Exit | Δ |
|---|---|---|---|
| cargo test (unit, lib) | 212 | 204 | −8 |
| cargo test (unit, bin) | 217 | 209 | −8 |
| cargo test (integration) | 1 | 1 | 0 |
| cargo clippy warnings | 34 | 0 | −34 |
| `-D warnings` gate | FAIL | **PASS** | ✓ |
| cargo fmt --check | FAIL (master dirty) | **PASS** | ✓ |
| Files modified in src/ | 0 | 27 | +27 |
| Lines deleted (src/) | 0 | ~490 | |
| Lines added (src/) | 0 | ~315 | |
| Net src/ churn | 0 | −175 lines | |

Test count delta −8 (4× duplicated in lib+bin = 4 unique deletions):
- 5× parser `Continuation` tests in `src/parser/generic.rs` (DOM-012)
- 3× json_viewer bulk-state tests in `src/ui/json_viewer/state.rs`
  (UI-019)

All deletions paired with removed code — no behavior regression.

## 意外发现（Phase 6 methodology 原材料）

1. **Plan gap: `ClientMessage` scope ownership**. `src/input/protocol.rs`
   was outside every scope subagent's explicit boundary. The
   `large_enum_variant` warning couldn't be addressed by any Task 1-4
   subagent without breaking their scope rules. Discovered at Task 5's
   exit-gate check. Fixed by main Claude directly. Lesson: when writing
   a plan, every warning in the current baseline must map to a named
   task — orphaned warnings silently break the exit gate.

2. **Audit misjudgement → new audit entry (DOM-025)**. Three audit
   entries (DOM-009/010/023) relied on grep that matched field names
   across unrelated types. Phase 2 execution surfaced the truth. Pattern:
   *audit findings need per-type verification, not flat textual grep.*
   Added DOM-025 for write-only fields; left the `#[allow(dead_code)]`
   markers in place as evidence.

3. **Test-only items as idiomatic Rust pattern**. `enabled_count` on
   MockRuleStore is called only from tests, but `#[cfg(test)] pub fn`
   works cleanly without `#[allow(dead_code)]`. `find_match` has the
   same property but clippy's `len_without_is_empty` forced us to keep
   `is_empty` with an allow. Minor mechanical dance, not a deep issue.

4. **`cargo fmt` baseline drift on master**. Phase 2 Task 1 discovered
   `fmt --check` was already failing on master before any Phase 2 edits
   (pre-existing diffs in domain/filter.rs, domain/network_filter.rs,
   etc.). Fixed by Task 5's final `cargo fmt`. Future phases inherit
   clean fmt baseline. Lesson: every phase start should `fmt --check`
   and surface drift.

5. **Sequential execution worked smoothly**. No scope conflicts between
   Task 1 (transport), 2 (domain), 4 (UI). Each subagent started from
   a clean known state; no merge surprises. Confirms spec §2's choice
   to run Phase 2 subagents sequentially.

## 出口
- 日期：2026-04-22
- Git HEAD at exit: (filled at commit)
- 验收门槛 (spec §4.1)：
  - [x] `cargo clippy --all-targets -- -D warnings` passes (0 warnings)
  - [x] `cargo test` all green: 204 lib + 209 bin + 1 integration + 0 doc
  - [x] `cargo fmt --check` clean
  - [x] 1 consolidated commit
  - [x] `docs/superpowers/journal/phase-2.md` written

## 移交 Phase 2.5 事项

- **Phase 2.5 不需要再改任何 clippy warning** — baseline 已清零。
- **Deferred-to-Phase-3 追踪注释** (共 4 处 `#[allow]` + tracking comment)：
  - `src/domain/entry.rs:26` — `LogLevel::from_str` → implement `FromStr`
  - `src/input/protocol.rs:22` — `ClientMessage large_enum_variant` → box decision
  - `src/ui/network/detail.rs:991` — `render_json_section_with_depth` 8 args → param struct
  - `src/ui/source_select.rs:351` — `push_device_top` 8 args → param struct
  Phase 3 step 计划时这 4 处都要映射到具体 step。
- **新 Audit 条目**: DOM-025 (write-only SseChunk/WsMessage fields)。
  Phase 3 Domain step 会处理。
- **Baseline for Phase 2.5**:
  - Test count: 204 + 209 + 1 = total 414 unique tests
  - Clippy: 0
  - All source files still within §5.5 line limits as of Phase 2 (no file
    crossed thresholds during Phase 2)
- **flog_dart 未处理的 info-级 items**:
  - `flog_dart/lib/src/flog_server.dart:11` — unused `dart:ui` import
    (defer to Phase 3 DART step)
  - `flog_dart/test/flog_sse_parser_test.dart:4` — missing flutter_test
    dependency (DART-002 scope, Phase 3)
- **Phase 2.5 入口 🛑 停机点**（spec §9）：选覆盖率工具（llvm-cov vs
  tarpaulin）+ 跑当前 baseline 覆盖率 —— 用户决定后再开工。
