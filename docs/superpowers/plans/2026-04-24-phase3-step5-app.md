# Phase 3 Step 3.5 — App State Machine Redesign

> REQUIRED SUB-SKILL: superpowers:subagent-driven-development or executing-plans.

**Goal:** Resolve 8 D + several A audit entries in `src/app.rs`. Status machine lives at the root of every tab and event handler; changes propagate widely. Approach: non-breaking incremental refactors — add new organizing types while keeping old field accessors; deprecate old in a later step. Each task is surgical.

**Architecture:** `App` stays the central container; internal state gets better-organized subtypes. `LayoutCache` becomes a separate struct. `MockEditState` becomes a sub-struct. Per-tab `LogsViewState` / `NetworkViewState` (the latter already exists as `NetworkState`, just needs parity). `auto_scroll` consolidation into each tab state.

**Red line (spec §5.8):** no change to `AppMode` variant names that are already serialized to session.rs (they're enum discriminants, not serialized, so safe to rename internally). No change to WS protocol.

**Parent spec:** §5.3 Step 3.5. **Audit:** `docs/superpowers/audit/03-ui.md` app.rs entries.

## Entries

| Id | Class | Problem |
|---|---|---|
| UI-002 | D | `InputField` enum mixes Logs and Network fields (LogsSearch/LogsExclude/LogsTag vs NetworkSearch/NetworkExclude) — no tab-level type safety |
| UI-003 | D | App struct has `selected` + `scroll_offset` fields that are actually per-tab (Logs); Network has its own. Asymmetry. |
| UI-004 | D | `NetworkState.filtered_indices` cached with mutable laziness; `_filtered_dirty` flag + `RefCell<Option<Vec<usize>>>` |
| UI-006 | D | `auto_scroll` flag lives on `App` (Logs) and `NetworkState` (Network) separately — no unified concept |
| UI-017 | D | `LayoutCache` (tab_bar_rect, list_y, list_height, ...) mixed into App struct — 30+ fields of pure UI layout |
| UI-018 | A | filter dirty-flag pattern — ack (works correctly per Phase 2.5B tests) |
| UI-022 | A | enter_mock_rules / enter_mock_edit naming clarified in audit user adjudication; redesign from Phase 1 C |
| UI-023 | A | multi-app state — ack, documented per Phase 2.5B characterization |
| UI-026 | D | MockEdit fields scattered (mock_edit_rule_id, mock_edit_field, mock_edit_top_values, mock_edit_body) — bundle into MockEditState |
| UI-027 | A | mock save/cancel semantics — ack per Phase 2.5B tests |
| UI-028 | D | Mock rule state machine transitions not explicit — document them |
| UI-032 | A | ack |
| UI-034 | D | enter_mock_edit deeply nested — flatten via MockEditState::from_rule |
| UI-040 | D | Multi-app state invariants not documented — add invariant comment block |

## Tasks

### Task 0: pre-flight
- Verify HEAD `cdcc929`, cargo test green, fmt clean.
- `wc -l src/app.rs` — baseline 1179 lines (was 1167 pre-Phase-3 per Phase 2.5B notes).

### Task 1: UI-017 — Extract LayoutCache struct
- Read `src/app.rs` to find all `self.layout_*` or bare-field layout fields (tab_bar_rect, list_y, list_height, net_detail_x, net_toolbar_y, input_row_y, net_col_header_y, bottom_y, etc).
- Create `pub struct LayoutCache { ... }` with those fields.
- Replace `app.list_y` with `app.layout.list_y` throughout (already done in some places per Phase 2.5B — verify which fields were already migrated to `app.layout.X`).
- If Phase 2.5B already extracted it: verify completeness + add unit tests for `LayoutCache::default()`.
- Add 2 tests asserting default-reset behaviour + post-render population.
- Commit: `refactor(app): LayoutCache struct encapsulates render-layout fields (Phase 3 UI-017)`

### Task 2: UI-026 + UI-034 — Extract MockEditState
- Bundle `mock_edit_rule_id`, `mock_edit_field`, `mock_edit_top_values`, `mock_edit_body` into `struct MockEditState`.
- Add constructor `MockEditState::new_blank()` (for add-new) and `MockEditState::from_rule(&MockRule)` (for edit). This flattens UI-034's nested enter_mock_edit.
- Replace 4 field accesses with `app.mock_edit.X` throughout.
- +3 tests: new_blank defaults, from_rule population, round-trip.
- Commit: `refactor(app): MockEditState bundle replaces scattered mock_edit_* fields (Phase 3 UI-026 + UI-034)`

### Task 3: UI-002 — Tab-scoped InputField variants
- Read current `InputField` enum. If it's flat (LogsSearch, LogsExclude, LogsTag, NetworkSearch, NetworkExclude), split into per-tab enums:

```rust
pub enum LogsInputField { Search, Exclude, Tag }
pub enum NetworkInputField { Search, Exclude }

pub enum InputField {
    Logs(LogsInputField),
    Network(NetworkInputField),
}
```

Or keep flat but add `fn tab(&self) -> ViewTab` method that validates consistency.

- **CAUTION**: `AppMode::InputActive(InputField)` is used in event.rs `handle_input_key`. Changing variant shape propagates. If extensive changes result, use the tab-method approach (minimal disruption) + doc comment explaining the invariant.
- +2 tests: `tab()` returns correct ViewTab per variant.
- Commit: `refactor(app): InputField tab-safety method (Phase 3 UI-002)`

### Task 4: UI-003 — LogsViewState symmetry (optional)
- If NetworkState already exists as a dedicated struct, consider extracting LogsViewState similarly (selected, scroll_offset, auto_scroll, filter).
- Risk assessment: touches ~20 call sites. If clean extraction possible in < 100 lines of diff, do it. Else **acknowledge in journal** — this is a nice-to-have that Phase 3.10 cross-cutting cleanup can finish.
- Commit (if executed): `refactor(app): LogsViewState symmetry with NetworkState (Phase 3 UI-003)`
- Commit (if ack'd): included in Task 9 journal

### Task 5: UI-006 — unified auto_scroll
- If Task 4 extracted LogsViewState, auto_scroll already moved there. If not, at least add a helper `fn auto_scroll_for_tab(&self, tab: ViewTab) -> bool` returning the correct flag per tab.
- +1 test: per-tab auto_scroll query returns the right flag.
- Commit: `refactor(app): tab-aware auto_scroll accessor (Phase 3 UI-006)`

### Task 6: UI-004 — filtered_indices cache documentation
- Read `NetworkState` lazy-cache code. Add rustdoc comment explaining the invariant (`_filtered_dirty` → rebuild on next `filtered_indices()` call).
- If the `RefCell` is un-necessary (filtered_count already maintains it), simplify; else leave and document.
- +1 test asserting cache behaviour (invalidate → next call rebuilds).
- Commit: `refactor(app/network): filtered_indices cache invariant doc (Phase 3 UI-004)`

### Task 7: UI-040 — multi-app invariants
- Add module-level or struct-level doc block explaining:
  - active_app_id == Some(id) ↔ connected_apps contains id
  - discovered_devices can contain ids not in connected_apps (device seen but not attached)
  - connected_apps can contain ids with device_id not in discovered_devices (disconnected device still attached)
  - Switch semantics: `switch_to_app(id)` requires id ∈ connected_apps
  - Remove semantics: `remove_connected_app(id)` clears active_app_id if it was that id
- No code change if characterization tests already cover these. +1 test if any invariant lacks coverage.
- Commit: `docs(app): multi-app state invariants (Phase 3 UI-040)`

### Task 8: UI-028 — mock rule state machine doc
- Add sequence diagram / comment block at enter_mock_rules / enter_mock_edit:
  - Normal → enter_mock_rules → MockRuleEdit (new-rule blank form)
  - Normal → enter_mock_edit(id) → MockRuleEdit (populated form)
  - MockRuleEdit → save_mock_edit → Normal (rule added/updated in store)
  - MockRuleEdit → cancel_mock_edit → Normal (store unchanged)
- No code change. Commit: `docs(app/mock): state machine transitions (Phase 3 UI-028)`

### Task 9: A-class acks + journal
- UI-018, UI-022, UI-023, UI-027, UI-032: inline comments pointing to audit IDs.
- Write `docs/superpowers/journal/phase3-step5.md`.
- Final verify: cargo test + clippy + fmt.
- Commit: `docs(journal): Phase 3 Step 3.5 — app state machine redesign complete`

## Exit gates

- All Phase 2.5B characterization tests green (148 app_state + 107 keys + 108 mouse + 84 logs + 128 network + all domain + transport)
- New structural tests +5-10
- `src/app.rs` under 800 (or if over, journal explains — tests grew)
- `cargo clippy --all-targets -- -D warnings` clean

## 红线

- No `AppMode` variant renames visible to session serialization (check session.rs — AppMode is NOT serialized, so safe).
- No event.rs / ui/* logic changes; this step is app.rs-internal.
- No test deletions — if a characterization test is tightly coupled to a removed field, update test to use new accessor rather than delete.
