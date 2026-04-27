# Phase 4 — why-comments + residual splits + UI-003 migration

**Plan:** `docs/superpowers/plans/2026-04-24-phase4-comments.md`
**Start HEAD:** `056d664` (Phase 3 journal merged)
**End HEAD:** `<journal commit>`

## Outcome

- All six plan Tasks complete.
- Every production `src/**/*.rs` file ≤ 500 lines.
- Full test suite green at every intermediate commit: **2166 passed, 0 failed, 0 ignored** — zero delta vs. the Phase 3 exit baseline.
- `cargo clippy --all-targets -- -D warnings` clean.
- `cargo fmt -- --check` clean.

## File size delta

| File                                    | Before | After | Change |
|-----------------------------------------|--------|-------|--------|
| `src/app.rs`  → `src/app/mod.rs`        | 1506   | 484   | −1022  |
| `src/transport/device_monitor.rs`       | 743    | 146   | −597   |
| `src/main.rs`                           | 564    | 93    | −471   |

The three hot files shrunk by 2090 lines total; the content was moved into focused submodules (see below).

## UI-003 migration (Task 1)

Field move on `App`:

| Before          | After                    |
|-----------------|--------------------------|
| `app.selected`       | `app.logs.selected`      |
| `app.scroll_offset`  | `app.logs.scroll_offset` |
| `app.auto_scroll`    | `app.logs.auto_scroll`   |

Delegate fields + `App::logs()` projection method removed; `logs: LogsViewState` is the single source of truth.

**Call sites touched:** 193 external + 46 internal = **239 sites**.

Per file:
- `src/app/mod.rs` — 46 (internal self.*)
- `src/event/mod.rs` — 5
- `src/ui/logs/list.rs` — 25
- `src/ui/logs/status_bar.rs` — 2
- `src/ui/logs/empty_states.rs` — 1
- `src/app_tests.rs` — rewritten (3 tests) to assert on `app.logs` directly
- `tests/characterization_app_state.rs` — 71
- `tests/characterization_ui_logs.rs` — 34 (33 auto + 1 `app2` hand-fixed)
- `tests/characterization_event_keys.rs` — 30
- `tests/characterization_event_mouse.rs` — 20

## Splits (Task 2–4)

### src/app/ (Task 2)

| File                         | Lines | Contents |
|------------------------------|-------|----------|
| `mod.rs`                     | 484   | `AppMode`/`ViewTab`/`InputField`, `App` struct + core impl (`new`, data input, filter cache, `selected_store_index`, stats compute, clear/export/status) |
| `state_structs.rs`           | 124   | `SearchState`, `InputBuffers`, `StatsSnapshot`, `DetailState`, `LogsViewState` |
| `network_state.rs`           | 199   | `NetworkState` + UI-004 filter cache |
| `scroll.rs`                  | 72    | `move_*`, `select_*`, `go_*`, `set_level` |
| `input_fields.rs`            | 137   | `enter_/exit_/apply_input_field`, `next/prev_match`, `clear_all_filters` |
| `detail.rs`                  | 31    | detail-panel toggles + scroll |
| `mode.rs`                    | 66    | tab switching, help/stats transitions, bookmarks |
| `mock_edit.rs`               | 166   | `MockEditState` + mock edit state machine |
| `multi_app.rs`               | 151   | `ConnectedApp` + add/remove/switch |
| `sse_merge.rs`               | 41    | `SseMergeRule`/`SsePathSegment` + `sse_merged_field_count` |
| `layout_cache.rs`            | 101   | `LayoutCache` |

Clippy's `field_reassign_with_default` lint drove one design choice: `filtered_indices` + `filter_dirty` stay private to `mod.rs` (`impl App` methods in sibling files use `invalidate_filter()` instead of direct field access). `compute_stats` is `pub(super)` so `mode.rs::enter_stats` can call it.

### src/transport/device_monitor/ (Task 3)

| File                   | Lines | Contents |
|------------------------|-------|----------|
| `mod.rs`               | 146   | `Device`/`DeviceKind`/`ConnectionMethod` types, `start_discovery`, `DeviceTracker` |
| `adb_source.rs`        | 196   | `adb track-devices` source (Android + AVDs) |
| `usbmuxd_source.rs`    | 199   | macOS usbmuxd `Listen` source (iOS USB) |
| `local_source.rs`      | 197   | localhost port-scan source (macOS host + iOS sim) |

Per-source tests (`device_monitor/<source>/tests.rs`) and `tracker_tests.rs` were already sibling files from Step 3.10; `mod tests;` resolution works unchanged after the split. The TRANS-003 "kept whole" ack on the old module doc was dropped since the rationale no longer applies.

### src/main.rs → src/run/ (Task 4)

| File                   | Lines | Contents |
|------------------------|-------|----------|
| `src/main.rs`          | 93    | CLI parse, `App::new`, terminal setup, panic hook, session load/save |
| `run/mod.rs`           | 29    | module wiring + test-only re-exports for `main_tests.rs` |
| `run/dispatch.rs`      | 140   | `RAW_LOG_RE`/`STACK_FRAME_RE`, `split_stacktrace`, `format_ts`, `dispatch_client_message` |
| `run/server.rs`        | 297   | `RECONNECT_*` constants, `spawn_device_discovery` (fanout + `connection_task` retry loop), `spawn_switch_app_handler` |
| `run/render_loop.rs`   | 76    | TUI event poll + draw loop |

`main_tests.rs` gained explicit `use crate::app::App; use crate::domain; use crate::input::ClientMessage;` imports since `super::*` now resolves to `crate::run` rather than the bin root. The test suite (`main_tests` — 16 tests) passes unchanged.

## Why-comments (Task 5)

Added to 10 specific spots per the plan, 1 comment per spot (2–8 lines each, `// WHY: ...` style):

| File                                | Topic                                                    |
|-------------------------------------|----------------------------------------------------------|
| `src/parser/generic.rs`             | prefixed → structured parse order                        |
| `src/domain/filter.rs`              | `merge_overlapping_ranges` invariant for OR-term search  |
| `src/domain/store.rs`               | fold-on-push vs fold-on-drain split                      |
| `src/transport/adb.rs`              | 19753 port pool avoids common dev-port collisions        |
| `src/run/server.rs`                 | reconnect backoff 2→30s cap vs flutter-run cycle         |
| `src/input/connector.rs`            | 3s Hello timeout captures iOS sim boot worst case        |
| `src/app/multi_app.rs`              | `switch_to_app` must guard against stale picker ids      |
| `src/domain/network_store.rs`       | 10K cap = 2× Flipper's 5K, sized for LLM streaming bursts |
| `src/ui/json_viewer/palette.rs`     | 6-color cycle fits Catppuccin's accent-contrast budget   |
| `src/event/mod.rs` (top-level `//!`)| two-phase mouse dispatch enables pure detect-phase tests |

**Count per file:** 1 why-comment each, 10 total.

## Stale TODO removal (Task 5)

Grep of `TODO-phase*` / `TODO-phase2` / `TODO-phase3` / `FIXME:` across `src/`:

- `src/input/connector.rs:189` — `TODO-phase3.5` ("if connection flakiness surfaces, promote to tracing / return JoinHandle"). **Kept** per plan — still a pending observation.
- `src/input/connector.rs:208` — `TODO-phase3.5: JoinHandle monitoring if flakiness surfaces`. **Kept** per plan.

**Removed:** 0. (No stale phase-2 or phase-3 TODOs existed — the Phase 3 journal commits had already cleaned them up.)

## Test delta

- **Start:** 2166 passed (Phase 3 exit baseline).
- **End:** 2166 passed.
- **Delta:** 0.

All migrations were mechanical (field move, file reshuffle). The only test edits were:

- `src/app_tests.rs` rewritten (3 tests) — the old `App::logs()` projection test became meaningless after the field move, so the assertions were re-pointed at `app.logs` directly.
- `src/main_tests.rs` gained 3 `use` lines.

No characterization test was edited or ignored.

## Exit gates

- [x] Every production `src/**/*.rs` file ≤ 500 lines.
- [x] 2166 tests pass, 0 ignored.
- [x] 0 stale completed TODOs.
- [x] UI-003 migration complete — no delegates, no `#[allow(dead_code)]` on `LogsViewState`.
- [x] `cargo clippy --all-targets -- -D warnings` clean.
- [x] `cargo fmt -- --check` clean.

## Commits

| SHA       | Summary                                                              |
|-----------|----------------------------------------------------------------------|
| `e123ffd` | Task 1 — migrate to `logs.*` accessors (UI-003 completion), 193 sites |
| `d88d8f3` | Task 2 — split `src/app.rs` into 11-file `src/app/`                  |
| `537666b` | Task 3 — split `src/transport/device_monitor.rs` (743 → 5 files)     |
| `d4b3ce7` | Task 4 — extract server + render_loop; `main.rs` 564 → 93            |
| `98f43d0` | Task 5 — 10 targeted why-comments                                    |
| (this)    | Task 6 — exit gate + journal                                         |
