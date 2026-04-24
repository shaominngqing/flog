# Phase 3 Step 3.6 — Event Dispatch Redesign (Journal)

## 入口
- 日期：2026-04-24
- Git HEAD at entry: `9490d98` (master, after Phase 3 Step 3.5 merge +
  UI-042 red lock)
- Tasks 0–2 already landed: `06e126c` (pill constants),
  `95f97d7` (ClickRegion enum scaffold)
- Regression fence at entry: 107 key + 108 mouse characterization tests
- B-class ignored: **1** (`ui_042_a_ws_chat_mode_leaks_when_switching_entries`,
  owned by Step 3.8)

## 实际变更

### New files (this step, Tasks 3–8)
- `src/event/detect.rs` — pure `detect_click_region` + `classify_click`
- `src/event/apply.rs` — `apply_click_region` + mutation helpers
- `src/event/apply_status.rs` — status-bar click handler (split for
  file-size budget)
- `src/event/detect_net.rs` — network-tab region detection (split for
  file-size budget)
- `src/event/keys.rs` — keyboard handlers for Normal / Input / Overlay /
  MockRuleEdit + `handle_mock_edit_mouse`
- `src/event/actions.rs` — clipboard, replay, mock, copy-log helpers
- `src/event/sse_nav.rs` — pure `sse_navigate_fields` (saturating
  semantics locked by UI-008 characterization tests)
- `docs/superpowers/journal/phase3-step6.md` — this file

### Modified
- `src/event/mod.rs` — module-level //! block documenting UI-007
  routing invariants; `handle_normal_mouse` becomes a thin two-phase
  dispatcher (Down(Left) → detect + classify + apply; Down(Right) →
  preserved bookmark toggle; ScrollUp/Down → `handle_scroll`).
- `src/event/click_region.rs` — added `MockRuleEditBtn` variant so the
  mock-rule "edit" action fires on single click (was lost in the
  detect→apply reshape on first pass).
- `src/event/keys.rs` — j/k SSE merged-field arms collapsed from 80
  lines of nested matching to 5-line call sites against
  `sse_nav::sse_navigate_fields` + `apply::apply_sse_field_selection`.
  Inline `// UI-007` comments mark device-picker, Network, and Logs
  branches.
- `src/app.rs` — new `sse_merged_field_count(&mut self)` helper used
  by the new j/k call sites.

## Two-phase mouse dispatch (UI-009 + UI-041)

The pre-refactor `handle_normal_mouse` was 700+ lines of nested
conditionals where detection and mutation were interleaved. The
two-phase design splits these concerns:

1. `detect::detect_click_region(app, x, y) -> Option<ClickRegion>`
   — read-only borrow, walks layout rects in the same priority
   order as the original handler, returns a `ClickRegion` enum.
2. `detect::classify_click(now, x, y, prev) -> ClickClass`
   — pure function; returns `Double` when `prev` is at the same
   (x,y) within `DOUBLE_CLICK_MS`.
3. `apply::apply_click_region(app, region, class, x, y)` — one
   match arm per variant, performs all side-effects.

The dispatcher is now a ~35-line shell that unconditionally updates
`last_click` after apply, preserving the tab-bar / status-bar /
list-row behavior. Wheel scroll is handled by `handle_scroll`, which
branches by (device picker? logs detail panel? network detail? tab?).

### ClickRegion variants added (audit UI-041)

Per the plan's type definition, 31 variants covering:

- Device picker (3): `DevicePickerOutside`, `DevicePickerItem { index }`,
  `DevicePickerScroll { direction }`
- Tab bar (2): `LogsTab`, `NetworkTab`
- Logs view (8): `LogsToolbarLevel(LogLevel)`, `LogsToolbarSearch`,
  `LogsToolbarTag`, `LogsToolbarExclude`, `LogsListRow { row }`,
  `LogsJumpToBottom`, `LogsDetailPanel { line_idx, x }`, `LogsDetailClose`
- Network view (14): toolbar search/exclude, three filter-pill types,
  mock-rules button, list row, detail panel, SSE events/merged/field
  pills, WS chat pill, section toggle, mock/replay/close buttons.
- Mock rules side panel (5) — including the Task-5-added
  `MockRuleEditBtn { index }` to distinguish single-click edit from
  double-click row select.
- Status bar (2): `StatusBar`, `Scrollbar { axis, direction }`

The enum is `pub(crate)`, `Clone`, `PartialEq`, `Debug`. No external
contract.

## Before / after line counts

| File                          | Before | After |
|-------------------------------|--------|-------|
| `src/event.rs` (pre-refactor) | 1738   | —     |
| `src/event/mod.rs`            | —      | 455   |
| `src/event/detect.rs`         | —      | 408   |
| `src/event/detect_net.rs`     | —      | 172   |
| `src/event/apply.rs`          | —      | 495   |
| `src/event/apply_status.rs`   | —      | 79    |
| `src/event/keys.rs`           | —      | 417   |
| `src/event/actions.rs`        | —      | 331   |
| `src/event/click_region.rs`   | 142    | 142   |
| `src/event/sse_nav.rs`        | —      | 53    |
| `src/event/pills.rs`          | 51     | 51    |
| **Total (event submodule)**   | 1738   | 2603  |

Every `src/event/*.rs` file is under 500 lines. The total line count
went up (as expected) due to test additions and comment-heavy new
modules; the net code complexity dropped — the longest function
(`handle_normal_mouse`) went from 700+ lines to ~35.

## Deferrals

- **Task 7 — restructure `handle_normal_key` into per-context
  sub-functions:** DEFERRED per the plan's conditional branch. The
  full split (`handle_device_picker_key` + `handle_logs_tab_key` +
  `handle_network_tab_key`) would touch >300 lines of diff for
  readability gain that is already achieved by the invariant doc
  comments. Commit message uses the `docs(event): routing
  invariants (Phase 3 UI-007 ack)` variant to signal this.

- **SSE merge-rule wrap semantics:** The plan sketched
  `sse_navigate_fields` as wrap-around (j at max-1 → 0, k at 0 →
  max-1). The UI-008 characterization tests
  (`ui_008_sse_merged_j_saturates_at_max` + `_k_saturates_at_zero`)
  pin the shipped *saturating* behavior. Per the red line ("if
  behavior diverges, fix code not test"), `sse_navigate_fields` is
  saturating. The function is still pure and unit-tested; wrap can
  be considered a future UX change, separate from this refactor.

## Test delta

| Task | Added tests | Running total in event module |
|------|-------------|--------------------------------|
| 1 (pills)           | +2 | pills.rs                          |
| 2 (ClickRegion)     | +3 | click_region.rs                   |
| 3 (detect)          | +11 | detect.rs                        |
| 4 (apply + classify) | +7 | apply.rs                         |
| 5 (two-phase)       | +4 | event/mod.rs                     |
| 6 (SSE nav)         | +5 (3 nav + 2 apply) | sse_nav.rs + apply.rs |
| 7 (routing docs)    | +2 | keys.rs                          |
| **Net structural delta** | **+34** | |

The Phase 2.5A `handle_sse_field_navigation` helper + its 3 unit
tests were retired along with its caller in Task 6; their replacement
is `sse_nav::sse_navigate_fields` with its 3 saturating tests.

Final test counts (after Step 3.6):
- lib: 741 (up from 698 at entry to Phase 3 Step 3.3)
- bin: 756
- characterization_event_keys: 107 (unchanged; regression fence green)
- characterization_event_mouse: 108 (unchanged; regression fence green)
- characterization_bugs: 4 + 1 ignored (UI-042, owned by Step 3.8)
- all other characterization suites: unchanged

## Exit gate

- `cargo test --all` — clean, every suite green
- `cargo clippy --all-targets -- -D warnings` — clean
- `cargo fmt -- --check` — clean
- Every `src/event/*.rs` file under 500 lines
- 107 + 108 characterization fence green at every commit along the way
- +34 structural tests added; 0 removed from characterization suites
- B-class UI-042 stays ignored; this step does not touch it

Next step (3.7 or 3.8) can pick up from a clean master.
