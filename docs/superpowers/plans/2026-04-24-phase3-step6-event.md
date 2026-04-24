# Phase 3 Step 3.6 — Event Dispatch Redesign

> REQUIRED SUB-SKILL: superpowers:subagent-driven-development or executing-plans.

**Goal:** Resolve 4 D audit entries in `src/event.rs` (1733 lines). The biggest deliverable is UI-041 — extract a `ClickRegion` enum + pure `detect_click_region()` function so mouse coverage can climb from ~61% to ≥95% via Rule 2 gates in later phases. This step makes that possible by separating detection from mutation.

**Architecture:** Two-phase mouse dispatch: (1) pure `detect_click_region(x, y, layout, state) -> Option<ClickRegion>` (read-only, testable); (2) `apply_click_region(app, region, class)` that performs all mutations. Existing 700+-line `handle_normal_mouse` becomes a thin shell that calls those two. Sub-handlers (device picker, tab bar, detail panel) extracted for SSE merged (UI-008), mouse region handlers (UI-009), magic coords (UI-016).

**Red line (spec §5.8):** no behavior change — every characterization test in `tests/characterization_event_*.rs` stays green. Variant names of `ClickRegion` are internal (not serialized). `KeyEvent` / `MouseEvent` handlers retain current signatures.

**Parent spec:** `docs/superpowers/specs/2026-04-22-project-cleanup-design.md` §5.3 Step 3.6
**Audit:** `docs/superpowers/audit/03-ui.md` (UI-007, UI-008, UI-009, UI-016, UI-041)

## Entries

| Id | Class | Problem |
|---|---|---|
| UI-007 | D | handle_normal_key: invariants (tab vs device_picker vs mode) not explicit — keys silently fall through line ~1524 catch-all |
| UI-008 | D | SSE merged mode j/k handlers: 40+ lines of nested conditionals, duplicated logic, coupled to render state |
| UI-009 | D | handle_normal_mouse: 700+ lines, nested device picker / detail panel / tab bar / toolbar / filter pills / mock rules / SSE pills / WS pills / list |
| UI-016 | D | SSE/WS pill click detection uses magic `+1`, `header_w`, `" Events ".len()` without named constants |
| UI-041 | D | No `ClickRegion` enum → detection and mutation interleaved → untestable as pure function (Phase 2.5A verdict) |

## New types

### ClickRegion enum (UI-041)

```rust
// src/event/click_region.rs (new file)
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClickRegion {
    // Device picker
    DevicePickerOutside,
    DevicePickerItem { index: usize },
    DevicePickerScroll { direction: ScrollDir },

    // Tab bar
    LogsTab,
    NetworkTab,

    // Logs view
    LogsToolbarLevel(LogLevel),
    LogsToolbarSearch,
    LogsToolbarTag,
    LogsToolbarExclude,
    LogsListRow { row: u16 },
    LogsJumpToBottom,
    LogsDetailPanel { line_idx: usize, x: u16 },
    LogsDetailClose,

    // Network view
    NetworkToolbarSearch,
    NetworkToolbarExclude,
    NetworkProtocolPill(ProtocolFilter),
    NetworkMethodPill(MethodFilter),
    NetworkStatusPill(StatusFilter),
    NetworkMockRulesBtn,
    NetworkListRow { row: u16 },
    NetworkDetailPanel { line_idx: usize, x: u16 },
    NetworkDetailSseEventsPill,
    NetworkDetailSseMergedPill,
    NetworkDetailSseFieldPill { idx: usize },
    NetworkDetailWsChatPill,
    NetworkDetailSectionToggle { section_key: String },
    NetworkDetailMockBtn,
    NetworkDetailReplayBtn,
    NetworkDetailClose,

    // Mock rules side panel
    MockRuleRow { index: usize },
    MockRuleToggle { index: usize },
    MockRuleDelete { index: usize },
    MockRuleAdd,
    MockRuleClose,

    // Status bar / other
    StatusBar,
    Scrollbar { axis: Axis, direction: ScrollDir },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScrollDir { Up, Down }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Axis { Vertical, Horizontal }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClickClass { Single, Double }
```

Notes:
- Variant count ≈ 30 (bigger than audit's ~20 estimate; we've absorbed UI-008/UI-016 pills here).
- `pub(crate)` — internal only, no external contract.
- Fields carry the *minimum* state needed by `apply_click_region` — no borrowed refs, enum is `Clone`.

### Magic-constant names (UI-016)

```rust
// src/event/pills.rs (new small file, or top of click_region.rs)
pub(crate) const SSE_EVENTS_PILL: &str = " Events ";
pub(crate) const SSE_MERGED_PILL: &str = " Merged ";
pub(crate) const WS_CHAT_PILL: &str = " Chat ";
pub(crate) const WS_LIST_PILL: &str = " List ";
pub(crate) const PILL_PADDING: usize = 1;
```

## Tasks

Execute in order. One commit per task.

### Task 0: pre-flight

- Verify HEAD `672a78d`, cargo test green, clippy clean, fmt clean.
- `wc -l src/event.rs` → baseline 1733.
- Count current characterization tests:
  - `tests/characterization_event_keys.rs` → 107
  - `tests/characterization_event_mouse.rs` → 108
  - **These are the regression fence — all must stay green at every commit.**

### Task 1: UI-016 — Name SSE/WS pill constants

- Create `src/event/mod.rs` → make `event` a module directory. (Currently `src/event.rs`. Move to `src/event/mod.rs`; `src/event/` directory holds submodules.)
- Add `src/event/pills.rs` with `SSE_EVENTS_PILL`, `SSE_MERGED_PILL`, `WS_CHAT_PILL`, `WS_LIST_PILL`, `PILL_PADDING` constants.
- Replace hardcoded `" Events ".len()`, `" Merged ".len()`, `" Chat ".len()`, `" List ".len()` strings in event.rs lines ~295-330, ~395-430 with references to these constants.
- +2 tests: pill constants unchanged (guard against accidental whitespace edits).
- Verify all 107 + 108 characterization tests green.
- Commit: `refactor(event): name SSE/WS pill constants (Phase 3 UI-016)`

### Task 2: UI-041 — Introduce ClickRegion enum (scaffold)

- Create `src/event/click_region.rs` with the `ClickRegion`, `ScrollDir`, `Axis`, `ClickClass` enums as specified above. No detection or dispatch yet — just the type definitions.
- `pub(crate)` visibility.
- +3 unit tests in the same file: `ClickRegion::LogsTab` equality, clone, debug-format contains variant name.
- Commit: `refactor(event): ClickRegion enum scaffold (Phase 3 UI-041 step 1)`

### Task 3: UI-041 — Extract detect_click_region (pure fn)

- Create `src/event/detect.rs`.
- Add `pub(crate) fn detect_click_region(app: &App, x: u16, y: u16) -> Option<ClickRegion>`.
  - Read-only borrow. No mutations. Consumes `app.layout`, `app.active_tab`, `app.show_device_picker`, `app.mode`, `app.network.show_detail`, etc.
  - Port the region-detection logic from `handle_normal_mouse` (lines 37-727) — but **just detection**, not effects. Returns the enum.
  - For double-click: return just the `ClickRegion`, not the class. Classification happens outside (Task 4).
- Leave `handle_normal_mouse` untouched for now. This task just creates the function.
- +10 tests: one per major region kind (device picker, tabs, filter pill, detail SSE pill, mock row, list row, status bar, scrollbar, outside device picker, invalid coords).
- Commit: `refactor(event): pure detect_click_region (Phase 3 UI-041 step 2)`

### Task 4: UI-041 — Extract classify_click + apply_click_region

- In `src/event/detect.rs`: add `pub(crate) fn classify_click(now: Instant, x: u16, y: u16, prev: Option<(Instant, u16, u16)>) -> ClickClass`.
  - Returns `Double` if `prev` is Some, same `(x,y)` within `DOUBLE_CLICK_MS`; else `Single`.
  - Pure function; no app borrow.
- In new `src/event/apply.rs`: add `pub(crate) fn apply_click_region(app: &mut App, region: ClickRegion, class: ClickClass)`.
  - Performs all mutations corresponding to each region.
  - Port side-effect code from `handle_normal_mouse` into this dispatch (one match arm per variant).
  - **Do NOT delete `handle_normal_mouse` yet** — that happens in Task 5.
- +6 tests: `classify_click` single vs double, timeout → single, different coords → single; `apply_click_region(ClickRegion::LogsTab)` → switches tab; `apply_click_region(ClickRegion::DevicePickerOutside)` → closes picker; `apply_click_region(ClickRegion::NetworkProtocolPill(..))` → toggles filter.
- Commit: `refactor(event): classify_click + apply_click_region (Phase 3 UI-041 step 3)`

### Task 5: UI-009 + UI-041 — Rewrite handle_normal_mouse as two-phase

- Replace `handle_normal_mouse` body with:

```rust
fn handle_normal_mouse(app: &mut App, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let (x, y) = (mouse.column, mouse.row);
            let now = Instant::now();
            if let Some(region) = detect::detect_click_region(app, x, y) {
                let class = detect::classify_click(now, x, y, app.layout.last_click);
                apply::apply_click_region(app, region, class);
            }
            app.layout.last_click = Some((now, x, y));
        }
        MouseEventKind::ScrollUp => handle_scroll(app, mouse, ScrollDir::Up),
        MouseEventKind::ScrollDown => handle_scroll(app, mouse, ScrollDir::Down),
        _ => {}
    }
}
```

- Extract `handle_scroll` as a sibling helper (wheel handling stays in event/mod.rs or a `scroll.rs`).
- Verify **all 108 mouse characterization tests** stay green (regression fence).
- If any test fails: do not change the test; fix the dispatch until it passes. (Rule: characterization tests are contract, not targets to update.)
- +4 tests: double-click on tab bar, scroll up / scroll down, click outside detail panel while picker open.
- `wc -l src/event/mod.rs` → target < 500 (old event.rs was 1733; split reduces mod.rs).
- Commit: `refactor(event): two-phase mouse dispatch (Phase 3 UI-009 + UI-041)`

### Task 6: UI-008 — Extract SSE merged field navigation

- In `src/event/sse_nav.rs` (new): add `pub(crate) fn sse_navigate_fields(current: usize, count: usize, dir: ScrollDir) -> usize`.
  - Pure: wraps around at bounds, no-op if count == 0.
- In `src/app.rs` (or event/apply.rs): add `apply_sse_field_selection(app, new_idx)` that rebuilds the merge rule from the selected field path.
- Replace the ~40-line nested conditional in `handle_normal_key` j/k arms (lines ~1342-1413) with:
  ```rust
  KeyCode::Char('j') | KeyCode::Down
      if app.network.sse_merged_mode && app.network.show_detail =>
  {
      let count = app.sse_merged_field_count();
      let new_idx = sse_navigate_fields(app.network.sse_merged_field_idx, count, ScrollDir::Down);
      apply_sse_field_selection(app, new_idx);
  }
  ```
  (And mirror for k/Up.)
- +3 tests on `sse_navigate_fields`: forward wrap, backward wrap, zero count returns 0.
- +2 tests on `apply_sse_field_selection`: picks new field, rebuilds rule.
- Characterization tests remain green.
- Commit: `refactor(event): SSE merged field navigation (Phase 3 UI-008)`

### Task 7: UI-007 — Document routing invariants + fix silent catch-all

- Add module-level `//!` comment at top of `src/event/mod.rs` explaining:
  - `handle_key` entry point routes by `AppMode` → one of 4 sub-handlers
  - `handle_normal_key` further dispatches by `app.show_device_picker` and `app.active_tab`
  - Invariants: when `show_device_picker == true`, keys route to device-picker handler; when false, tab-handler.
  - Catch-all at end of match is intentional (unhandled keys no-op) — note this explicitly.
- Audit `handle_normal_key` match arms (~line 1297+) for unreachable paths given Network-tab routing. Add `// UI-007: unreachable when active_tab == Network` comments where applicable, or guard the handler so the routing is explicit.
- Restructure if warranted: split `handle_normal_key` into `handle_device_picker_key`, `handle_logs_tab_key`, `handle_network_tab_key` with an explicit top-level match.
  - **Caveat:** if restructuring would touch > 300 lines of diff, skip the split, just add the invariant doc comments + an ack note in journal.
- +2 tests: key routes to logs handler when `active_tab == Logs`; key routes to network handler when `active_tab == Network` (observable via side-effect).
- Characterization tests green.
- Commit: `refactor(event): document routing invariants (Phase 3 UI-007)` or `docs(event): routing invariants (Phase 3 UI-007 ack)` if split deferred.

### Task 8: Exit gate + journal

- Run full test suite: `cargo test --all` + `cargo clippy --all-targets -- -D warnings` + `cargo fmt -- --check`.
- Confirm:
  - All 107 key + 108 mouse characterization tests green (regression fence)
  - New structural tests: Task 1 (+2) + Task 2 (+3) + Task 3 (+10) + Task 4 (+6) + Task 5 (+4) + Task 6 (+5) + Task 7 (+2) = **+32 tests**
  - `src/event/mod.rs` < 500 lines; `src/event/detect.rs`, `src/event/apply.rs`, `src/event/click_region.rs`, `src/event/sse_nav.rs`, `src/event/pills.rs` all < 500.
  - Clippy clean, fmt clean.
- Write `docs/superpowers/journal/phase3-step6.md`:
  - Summary of the two-phase redesign
  - ClickRegion variants added
  - Before/after line counts for event.rs split
  - Any deferrals (if Task 7 split skipped, note it)
  - +32 test delta
- Commit: `docs(journal): Phase 3 Step 3.6 — event dispatch redesign complete`

## Exit gates

- All 107 + 108 characterization tests green
- +32 structural tests
- `src/event/*.rs` each < 500 lines; no single file > 500
- `cargo clippy --all-targets -- -D warnings` clean
- Unblocks Phase 4 / later Rule-2 coverage gate for event.rs mouse ≥ 95%

## 红线

- No test deletions. Characterization tests are the regression fence. If one appears to fail "correctly" (old behavior was wrong), escalate instead of updating — it means our refactor is behavior-changing.
- No new deps.
- `ClickRegion` is `pub(crate)` — not public API.
- `KeyEvent` / `MouseEvent` dispatch signatures (`handle_key`, `handle_mouse`) preserved.
- Magic coord constants stay in sync with renderer (`ui/network/detail.rs` etc). If renderer renames a pill label, the constant must be updated in lockstep — verify with grep after Task 1.
