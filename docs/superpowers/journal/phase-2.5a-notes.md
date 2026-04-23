# Phase 2.5A execution notes

## Task 6 outcome — VERDICT_C

Click-region extraction declined (`detect_click_region` pure fn not
safe to extract under Guardrails). See new audit entry `UI-041`
in `docs/superpowers/audit/03-ui.md` for the full blocker analysis.

Three blockers:
1. Mutations interleaved with region detection (at least 4 places:
   device picker close, double-click state update, filter pill invalidation,
   SSE merged-mode setup).
2. Decision tree nested 5+ levels deep (picker > detail > jump > tab >
   network-tab > {toolbar, pills, mock-rules, detail-scroll > {sse, ws,
   section-toggle, json-fold}}).
3. No existing ClickKind enum to reuse — any extraction would have to
   invent a ~20-variant enum, a design choice that belongs in Phase 3.

Consequence for Phase 2.5B: mouse-routing characterization tests fall
back to `ratatui::backend::TestBackend` snapshot tests (fragile but
unavoidable) until Phase 3 UI Event step decouples detection from
effect dispatch.

No source code modified in Task 6.

## Task 7 — no-op extractions

Targets 6 and 7 listed in the plan header are already pure functions
in src/domain/ws_chat.rs and src/domain/sse_merge.rs. No code change
needed — Phase 2.5B can point characterization tests directly at:

- `crate::domain::ws_chat::group_messages(&[WsMessage]) -> Vec<MessageGroup>`
  (baseline line coverage: 94.47%)
- `crate::domain::ws_chat::has_binary_content(&WsMessage) -> bool`
  (baseline line coverage: 94.47%)
- `crate::domain::sse_merge::extract_field_paths(&[SseChunk]) -> Vec<FieldPath>`
  (baseline line coverage: 90.72%)
- `crate::domain::sse_merge::merge_field(&[SseChunk], path) -> String`
  (baseline line coverage: 90.72%)

## Plan deviations in completed tasks

- **Task 2 (logs viewport)**: original plan assumed a fixed `(start, end)`
  window. Reality: logs use row-walking with variable-height rows (3 for
  separators, up to MAX_WRAP_LINES for entries). Extracted function
  signature changed to `compute_visible_entry_start(total, offset) -> usize`
  (only the start clamp is actually pure). End computation doesn't exist
  inline.
- **Task 3 (entry wrap)**: instead of creating `compute_entry_screen_height`,
  the natural extraction was to pull a pure-on-LogEntry inner from the
  existing `entry_row_count_from_store` helper. Named `entry_row_count(&LogEntry, full_width)`.
  Test fixtures needed explicit LogEntry construction (no `Default` impl).
- **Task 1 (SSE nav)**: the Up arm call site passes `usize::MAX` as count,
  preserving original "always decrement" semantic without moving the
  candidates computation earlier.

## Extracted functions summary (Phase 2.5B handoff)

| Fn | File | Signature |
|---|---|---|
| `handle_sse_field_navigation` | src/event.rs | `(usize, usize, SseNavDir) -> usize` |
| `compute_visible_entry_start` | src/ui/logs/mod.rs | `(usize, usize) -> usize` |
| `entry_row_count` | src/ui/logs/mod.rs | `(&LogEntry, usize) -> usize` |
| `repeat_bar_normalized` | src/ui/logs/mod.rs | `(usize, usize) -> usize` |
| `compute_visible_network_range` | src/ui/network/mod.rs | `(usize, usize, usize) -> (usize, usize)` |

Five functions total. Each has 3-4 smoke tests already wired.

Also available for Phase 2.5B without extraction:
- `crate::domain::ws_chat::*` — already pure (Task 7)
- `crate::domain::sse_merge::*` — already pure (Task 7)
- `crate::domain::filter::*` — 80.58% baseline coverage
- `crate::domain::network_filter::*` — 85.42% baseline coverage (at target)
