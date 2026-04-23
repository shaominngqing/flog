# Phase 2.5A Journal — Logic/Render Separation

## 入口
- 日期：2026-04-23
- Git HEAD at entry: `70ed4ef` (coverage baseline commit)
- Baseline tests: 204 lib + 209 bin + 1 integration + 0 doc
- Baseline coverage: 31.48% line / 31.30% region / 49.49% function
- 执行模式：Inline + scope subagents

## 时间线

- Task 0 pre-flight (commit `d95acd6`+): baseline verified, pre-coverage snapshot recorded
- Task 1 (UI-008 SSE nav) commit `4f4d97a`: extract `handle_sse_field_navigation` + 3 tests. Subagent flagged a call-site subtlety (passing `usize::MAX` for unused count in Up branch) — accepted as safer than reordering candidate computation.
- Task 2 (UI-010/029 logs viewport) commit `8ee3e5f`: Subagent-exposed plan bug — logs use row-walking variable-height rows, NOT fixed (start, end) windows. Fixed by redefining extracted fn as `compute_visible_entry_start(total, offset) -> usize` (only the start clamp is pure). 4 smoke tests.
- Task 3 (UI-010 entry wrap) commit `30725b4`: Extract `entry_row_count(&LogEntry, full_width) -> usize` as pub(crate); old `entry_row_count_from_store` delegates. Subagent also caught another plan bug (test width=40 would make wrap_width=0). 3 smoke tests.
- Task 4 (UI-030 repeat bar) commit `70a2cea`: Simple — direct extraction by main Claude, no subagent needed. `repeat_bar_normalized(count, max_w) -> usize`. Magic constant 50 preserved (Phase 3 UI-030 names it). 4 smoke tests.
- Task 5 (UI-029 network viewport) commit `2650ba8`: Network uses real fixed-window (1-row entries), so true `(start, end)` signature valid. Call site changed from `skip().take()` to explicit slice. 4 smoke tests.
- Task 6 (UI-009/016 click-region): **VERDICT_C — declined**. Exploratory Explore-agent audit found mutations interleaved + 5+ nesting levels + no existing enum. New D-class audit entry `UI-041` added to `03-ui.md` + `00-index.md`. No code change.
- Task 7 (domain no-op): `phase-2.5a-notes.md` records ws_chat/sse_merge as already pure. Committed with Task 8.
- Task 8: this journal + phase commit.

## 关键统计（Phase 2.5A 出口）

| 指标 | Entry | Exit | Δ |
|---|---|---|---|
| cargo test (lib) | 204 | 222 | +18 |
| cargo test (bin) | 209 | 227 | +18 |
| cargo test (integration) | 1 | 1 | 0 |
| cargo clippy warnings | 0 | 0 | 0 |
| cargo fmt | clean | clean | — |
| Line coverage | 31.48% | 32.27% | +0.79% |
| Region coverage | 31.30% | 32.05% | +0.75% |
| Function coverage | 49.49% | 50.83% | +1.34% |
| Pure fns extracted | 0 | 5 | +5 |
| Audit entries added | 0 | 1 (UI-041) | +1 |

Test delta +18 (9 unique, counted in both lib and bin because `event.rs` + `ui/logs/mod.rs` + `ui/network/mod.rs` compile into both targets):
- 3× handle_sse_field_navigation (Task 1)
- 4× compute_visible_entry_start (Task 2)
- 3× entry_row_count (Task 3)
- 4× repeat_bar_normalized (Task 4)
- 4× compute_visible_network_range (Task 5)

## 意外发现（Phase 6 methodology 原材料）

1. **Plan assumptions can be wrong — subagents catch them.** Task 2 plan
   assumed logs had a fixed-window viewport model. Subagent compared plan
   skeleton to reality and flagged the mismatch. I rewrote the signature
   to match actual code. Lesson: a subagent following Guardrails is a
   safety net against planner assumptions.

2. **Extract-then-verify catches half-baked signatures.** Task 2 first
   pass left `let _end = ...` with end returning a value nobody used —
   a smell. Rewrote to return only `start`, cleaner and matches reality.
   Lesson: if the call site discards a return value, the signature is
   wrong.

3. **VERDICT_C path worked exactly as designed.** Click-region
   exploration pre-committed to not writing code until verdict was in.
   Explore agent returned with concrete evidence (5+ nesting levels,
   20-variant enum needed). The "defer to Phase 3" lane captured the
   finding without scope creep.

4. **Some extractions are "a function already exists, just needs a
   purer sibling"** (Task 3's `entry_row_count_from_store` → `entry_row_count`).
   This is cheaper than inventing from scratch. Plan should surface these
   "half-pure already" cases explicitly.

5. **Coverage delta was modest but real** (+0.79% line). The fns are
   actually wired in — not orphans. But Phase 2.5A alone doesn't move
   the needle on the hot files (event.rs 0% → 0.2%, app.rs still 0%,
   logs/mod.rs went up trivially). The bulk of the coverage gain has to
   come from Phase 2.5B's characterization tests on the extracted +
   pre-pure functions.

## 出口

- 日期：2026-04-23
- Git HEAD at exit: (set by Step 8.4 commit)
- 验收门槛：
  - [x] 5 pure fns extracted
  - [x] Each fn has 3-4 smoke tests
  - [x] cargo test / clippy / fmt green at every commit
  - [x] Coverage not regressed — net +0.79% line / +0.75% region
  - [x] Task 6 outcome documented: VERDICT_C → UI-041 audit entry
  - [x] Audit index (`00-index.md`) + 03-ui.md updated to reflect UI-041

## 移交 Phase 2.5B 事项

Phase 2.5B characterization tests can target:

**Newly extracted pure fns (5)**:
- `crate::event::handle_sse_field_navigation` (UI-008)
- `crate::ui::logs::compute_visible_entry_start` (UI-010)
- `crate::ui::logs::entry_row_count` (UI-010)
- `crate::ui::logs::repeat_bar_normalized` (UI-030)
- `crate::ui::network::compute_visible_network_range` (UI-029)

**Already-pure domain (Task 7)**:
- `crate::domain::ws_chat::group_messages` + `has_binary_content` (94.47% baseline)
- `crate::domain::sse_merge::extract_field_paths` + `merge_field` (90.72% baseline)
- `crate::domain::filter::*` (80.58% baseline)
- `crate::domain::network_filter::*` (85.42% baseline — at target)
- `crate::domain::mock::MockRuleStore::find_match` (test-only wrapper)
- `crate::domain::structured_parser::*` (92.62% baseline)
- `crate::domain::entry::LogLevel::*` (94.29% baseline)

**Still requires TestBackend snapshot fallback (UI-041)**:
- `src/event.rs` mouse routing (region detection)
- `ui/logs/mod.rs` rendering pipeline (row-walking beyond the extracted
  start clamp)
- `ui/network/detail.rs` complex render logic

**B-class bug tests for Phase 2.5B (red, ignored)**:
- 12 B entries from audit — see `00-index.md` prioritized list
- 3 HIGH: DOM-003 (response without request), DART-001 (SSE drops after
  first data:), DART-002 (untracked test references non-existent APIs)

**Phase 2.5B coverage target (reminder from user decision)**:
- Not a hard %-gate — "关键行为全覆盖 + 覆盖率作为信号"
- Floor: baseline 31.48% must not regress
- Success signal: if key modules (filter, network_filter, structured_parser)
  keep their 80-95% and the newly extracted fns stay near 100%, the
  characterization safety net is solid enough for Phase 3.
