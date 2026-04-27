# Phase 4 — Why-Comments + Residual Splits

> REQUIRED SUB-SKILL: superpowers:subagent-driven-development or executing-plans.

**Goal:** Add targeted "why" comments where non-obvious behavior lives; tackle the 3 residual oversized files from Phase 3 (`app.rs` 1506, `device_monitor.rs` 743, `main.rs` 564); delete stale TODOs where work has landed; migrate ~190 LogsViewState call sites (the deferred UI-003 migration).

**Principle (per CLAUDE.md):** default to no comments. Only add when the *why* is non-obvious — a hidden constraint, a subtle invariant, a workaround for a specific bug, behavior that would surprise a reader. Do NOT add comments explaining *what* the code does.

**Red line:** no behavior change. All tests green at every commit.

## Tasks

### Task 0 — pre-flight
Verify HEAD, `cargo test --all` green, clippy/fmt clean.

### Task 1 — UI-003 mass migration: `app.{selected,scroll_offset,auto_scroll}` → `app.logs.*`
Mechanical grep+sed across `src/**/*.rs` and `tests/**/*.rs`:
- `app.selected` → `app.logs.selected`
- `app.scroll_offset` → `app.logs.scroll_offset`
- `app.auto_scroll` → `app.logs.auto_scroll` (note: this field name collides with `NetworkState::auto_scroll`; resolve ambiguity explicitly)

After migration, remove the delegate fields from `App` + the `#[allow(dead_code)]` on `LogsViewState` methods. Public accessors that external tests depend on stay as thin delegates OR tests update to `app.logs.*`.

Expected shrink: `app.rs` 1506 → ~1300 (removing ~30-50 lines of field decls + delegates). Still over 500 — Task 2 addresses.

Budget: if the grep+sed sweep touches >400 sites, do it in batches (per module). One commit per batch.
Commit: `refactor(app): migrate to logs.* accessors (Phase 4 UI-003 completion)`

### Task 2 — Split `src/app.rs` (1506 → <500)
`app.rs` has no embedded tests. Extract logical sections:
- `src/app/multi_app.rs` — ConnectedApp management (add_connected_app, remove_connected_app, switch_to_app, connected_apps invariants)
- `src/app/mock_edit.rs` — MockEditState + enter_mock_rules/enter_mock_edit/save_mock_edit/cancel_mock_edit
- `src/app/sse_merge.rs` — SseMergeRule + sse_merged_field_count + rebuild logic
- `src/app/layout_cache.rs` — LayoutCache struct + impl
- `src/app/mod.rs` — AppMode, ViewTab, App struct, high-level coordination

Target: `app/mod.rs` < 500 lines; each extracted file < 500.
Commit: `refactor(app): split into cohesive submodules (Phase 4)`

### Task 3 — Split `src/transport/device_monitor.rs` (743)
Already a module directory (from Step 3.10). Current: `device_monitor.rs` is 743 lines at the top level. Inner modules already separated. Inspect the file — if the remaining 743 lines are the core `DeviceMonitor` struct + polling loop + dispatch, extract:
- `device_monitor/tracker.rs` — DeviceTracker state machine (if exists as nested module)
- `device_monitor/poll.rs` — `flutter devices --machine` polling loop
- `device_monitor/mod.rs` — DeviceMonitor struct + public API

Target: every file < 500 lines.
Commit: `refactor(transport/device_monitor): further split (Phase 4)`

### Task 4 — Split `src/main.rs` (564)
`main.rs` is tokio entry + server event loop + TUI render loop.Extract:
- `src/run/server.rs` — WS server start + event processor spawn
- `src/run/render_loop.rs` — TUI render loop (terminal setup, tick, event dispatch)
- `src/main.rs` — minimal `#[tokio::main] async fn main` that wires them together

Target: `main.rs` < 200 lines; each extract < 500.
Commit: `refactor(main): extract server + render_loop (Phase 4)`

### Task 5 — Add why-comments (targeted, no bulk)
Walk through the codebase with a discerning eye. Add a why-comment in these SPECIFIC places (nowhere else unless clearly warranted):

1. `src/parser/generic.rs` — the flutter-structured regex order: explain why `try_parse_flutter_prefixed` runs before `try_parse_flutter_structured` (order-sensitive parse).
2. `src/domain/filter.rs` — the `merge_overlapping_ranges` invariant: why plain-mode OR-terms can overlap (user-written `abc|bcd` vs `thee`) and why sorted+merged is the fix contract.
3. `src/domain/store.rs` — the ring buffer's fold-on-drain semantic: consecutive duplicates are folded only on drain, not on push, to preserve timestamp accuracy.
4. `src/transport/adb.rs` — ADB port pool base 19753 + size 10000: reserved range avoids collision with common dev ports (80, 3000, 8080, 9000).
5. `src/transport/device_monitor.rs` — reconnect backoff constants: exponential backoff starting at 500ms, capped at 30s, rationale for the cap (flutter run cycle timing).
6. `src/input/connector.rs` — Hello handshake timeout of 3s: rationale (flog_dart hello is emitted synchronously on WS accept; 3s captures typical slow iOS sim boot).
7. `src/app.rs` — multi-app switch semantics: why `switch_to_app` requires `id ∈ connected_apps`, vs `discovered_devices` which can contain unattached ids.
8. `src/domain/network_store.rs` — 10K cap rationale: reference Flipper's 5K default, chose 2x to accommodate LLM streaming bursts.
9. `src/ui/json_viewer/render/mod.rs` — depth-color cycling at depth 6: rationale (Catppuccin macchiato has 5 distinct accent colors; cycling avoids exhausting contrast).
10. `src/event/mod.rs` top-level `//!` — the two-phase mouse dispatch invariant (Step 3.6 already added this; verify + expand if thin).

For each: 2-4 line comment max, in a `// WHY: ...` or similar inline-comment style. Do NOT add doc comments where none existed unless the item is now `pub(crate)` and benefits from it.

Also delete stale TODO markers that have been resolved (grep `TODO-phase`, `TODO-phase2`, `TODO-phase3` and verify each is resolved):
- `TODO-phase3.5: JoinHandle monitoring if flakiness surfaces` (src/input/connector.rs) — keep; still pending as an observation, not addressed.
- Any `// phase 2.5b ...` comments — remove if the work is done.
- Any `// FIXME:` without a specific actionable bug — resolve or delete.

Commit: `docs(code): targeted why-comments + remove stale TODOs (Phase 4)`

### Task 6 — exit gate + journal
Full `cargo test --all` + clippy -D + fmt. Verify:
- Every production `src/**/*.rs` file <500 lines
- Zero ignored tests
- Zero stale `TODO-phase*` markers whose phase has shipped

Write `docs/superpowers/journal/phase4.md` with:
- File delta (`app.rs` / `device_monitor.rs` / `main.rs` before/after)
- Count of why-comments added (by file)
- Count of stale TODOs removed
- UI-003 migration site count
- Test count delta (should be ~0 — migration is mechanical)

Commit: `docs(journal): Phase 4 — why-comments + residual splits complete`

## Exit gates
- ✅ All production files <500 lines
- ✅ All tests green (1572+ from Phase 3 exit, +/- mechanical deltas)
- ✅ 0 ignored characterization tests
- ✅ 0 stale completed TODOs
- ✅ UI-003 migration complete (no more delegates)
- ✅ clippy -D warnings clean

## 红线
- No behavior change. This is a polish phase.
- No public API changes (internal reorganization only).
- Why-comments are narrow — add only to the 10 specified spots + any obviously critical invariant discovered along the way. Do NOT over-document.
- Do NOT delete characterization tests. Do NOT add `#[ignore]`.
- No new deps.
