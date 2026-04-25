# Phase 3 Step 3.8 — UI Network View + UI-042 Fix

> REQUIRED SUB-SKILL: superpowers:subagent-driven-development or executing-plans.

**Goal:** Split `src/ui/network/detail.rs` (1116 lines) and `src/ui/network/mod.rs` (747 lines). Resolve UI-011 (json_viewer state coupling), UI-037 (detail 1109 mix). **Fix UI-042** (WS chat↔raw mode state leak, B-class red lock currently ignored).

**Architecture:** `detail.rs` `draw_network_detail` has one 860-line body that linearly renders General / Query Params / Headers / Body / SSE / WS / Error sections. Extract each into a `render_section_X` helper in its own module: `detail/general.rs`, `detail/http_body.rs`, `detail/sse.rs`, `detail/ws.rs`, `detail/error.rs`. Shared helpers (`push_section_header`, `push_kv_single`, `push_kv_wrapped`, `render_json_section`, `parse_query_params`, `url_decode`, `http_status_text`) go into `detail/shared.rs`. `mod.rs` stays the coordinator (`draw_network_detail`).

`mod.rs` (747) split: extract `draw_table_body` (234 lines) into `table.rs`, `draw_empty_network` + `draw_network_status_bar` into `status_bar.rs`. Color/format helpers stay in `mod.rs` (shared).

UI-042 fix: on WS chat↔raw toggle, purge the opposite mode's `collapsed_sections` keys (`WS_GROUP#*` vs `WS#*`) and reset `json_viewer_states` entries keyed on stale indices. Un-ignore `ui_042_a_chat_to_raw_toggle_clears_mode_specific_collapse_keys` — must go green.

**Red line:** no render output changes for Logs tab / SSE / WS chat mode / WS raw mode. 128 ui_network + other characterization tests green at every commit.

## Tasks

### Task 0 — pre-flight
Verify HEAD, `cargo test --all` green (incl. 1 UI-042 ignored), clippy clean, fmt clean. Baseline: detail.rs 1116, mod.rs 747.

### Task 1 — Convert `ui/network/detail.rs` → `ui/network/detail/mod.rs` + `detail/shared.rs`
`git mv src/ui/network/detail.rs src/ui/network/detail/mod.rs`. Create `detail/shared.rs`. Move: `push_section_header`, `push_kv_single`, `push_kv_wrapped`, `render_json_section`, `render_json_section_with_depth`, `parse_query_params`, `url_decode`, `http_status_text`, `path_without_query`. Keep `pub(super)` visibility. Characterization green.
Commit: `refactor(ui/network/detail): extract shared helpers (Phase 3 UI-037 step 1)`

### Task 2 — Extract `detail/general.rs` + `detail/http_body.rs`
Move "General" section render block (URL, method, status, duration, size) into `detail/general.rs::render_general`. Move "Query Params" / "Headers" / "Request Body" / "Response Body" sections into `detail/http_body.rs` (one `render_http_section` per sub-section or a few cohesive fns).
Commit: `refactor(ui/network/detail): extract general + http_body (Phase 3 UI-037 step 2)`

### Task 3 — Extract `detail/sse.rs` + `detail/ws.rs`
Move SSE Events + Merged-view render blocks into `detail/sse.rs`. Move WS Messages (both Chat mode and Raw mode render blocks) into `detail/ws.rs`. `detail/ws.rs` will house the UI-042 fix in Task 5.
Commit: `refactor(ui/network/detail): extract sse + ws (Phase 3 UI-037 step 3)`

### Task 4 — Extract `detail/error.rs` + shrink `detail/mod.rs`
Move "Error" section render into `detail/error.rs`. Trim `detail/mod.rs` to just coordination (layout, scroll, call each render_X, Paragraph render). Target: `detail/mod.rs` < 350 lines.
Commit: `refactor(ui/network/detail): extract error + shrink mod (Phase 3 UI-037 step 4)`

### Task 5 — UI-042 fix: purge stale collapse keys on mode toggle
In `src/app.rs` (or wherever the toggle happens — likely `src/event/apply.rs` now per Step 3.6): when `ws_chat_mode` flips, drop any `collapsed_sections` entry prefixed with the OLD mode's marker. Chat uses `WS_GROUP#*`, Raw uses `WS#*`. Also clear `json_viewer_states` entries keyed on `ws_*` ids (they're rebuilt on next render anyway).

Add a `fn toggle_ws_chat_mode(&mut self)` on `App` or `NetworkState` that encapsulates the toggle + purge. Callers (WS pill click in `event/apply.rs`) use it.

Un-ignore `tests/characterization_bugs.rs::ui_042_a_chat_to_raw_toggle_clears_mode_specific_collapse_keys` — delete the `#[ignore]` attribute. Must go green.

+2 tests in the bugs file or a dedicated test: toggle twice returns clean state; toggle with unrelated keys present preserves them.
Commit: `fix(app/network): purge stale collapse keys on WS mode toggle (Phase 3 UI-042)`

### Task 6 — Split `ui/network/mod.rs` (747 lines)
Extract `draw_table_body` (234 lines) into `ui/network/table.rs`. Extract `draw_empty_network` + `draw_network_status_bar` into `ui/network/status_bar.rs`. Color/format helpers (`method_color`, `status_color`, `duration_color`, `format_duration`, `format_size`, `protocol_pill`) stay in `mod.rs` as they're shared with `detail/` submodules and `filter.rs`.

Target: `ui/network/mod.rs` < 400 lines.
Commit: `refactor(ui/network): extract table + status_bar (Phase 3 UI-010 mirror)`

### Task 7 — UI-011 JsonViewerPane fingerprint (conditional)
If journal/time permits, address UI-011 by adding a `fingerprint: u64` to the `json_viewer_states` entries so stale states get rebuilt on tree changes (the audit entry says logs already does this via `viewer_text_fingerprint`; network doesn't). Skip if Task 5 already took >30 minutes of subagent time — journal-ack instead.
Commit: `refactor(ui/network/detail): JsonViewerPane fingerprint (Phase 3 UI-011)` or skip.

### Task 8 — exit gate + journal
Full `cargo test --all` + clippy -D + fmt. Verify every `src/ui/network/**/*.rs` < 500 lines. UI-042 now green (no longer ignored). Write `docs/superpowers/journal/phase3-step8.md` with file deltas + UI-042 resolution note + any Task-7 deferral. Commit: `docs(journal): Phase 3 Step 3.8 — UI Network + UI-042 fix complete`

## Exit gates
- All 128 ui_network + 84 ui_logs + 215 event + 5 bugs (incl. un-ignored UI-042) characterization tests green
- Every `src/ui/network/**/*.rs` < 500 lines
- UI-042 red lock now green (0 ignored bugs in the Rust characterization_bugs suite — only the architecture-class acks remain)
- clippy -D warnings clean

## 红线
- No render output changes in Logs tab, SSE chunks, or WS messages display.
- `draw_network`, `draw_network_detail` public signatures preserved.
- No new deps. No public API additions.
- UI-042 fix must NOT change which sections are shown by default (collapsed-by-default vs expanded-by-default convention is load-bearing per existing tests).
