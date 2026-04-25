# Phase 3 Step 3.8 — UI Network View + UI-042 Fix (Journal)

## 入口

- 日期：2026-04-24
- Git HEAD at entry: `c8043e0` (master, after the step 3.8 plan landed)
- Regression fence at entry:
  - `characterization_ui_network` — 128 tests
  - `characterization_ui_logs` — 84 tests
  - `characterization_event_keys` — 107 tests
  - `characterization_event_mouse` — 108 tests
  - `characterization_bugs` — 4 passing + 1 ignored (UI-042.a)
- Baseline file sizes:
  - `src/ui/network/detail.rs` — 1116 lines
  - `src/ui/network/mod.rs` — 747 lines

## Commits in order

| # | SHA | Summary |
|---|-----|---------|
| 1 | `41fbc09` | `refactor(ui/network/detail): extract shared helpers (Phase 3 UI-037 step 1)` |
| 2 | `a2b9868` | `refactor(ui/network/detail): extract general + http_body (Phase 3 UI-037 step 2)` |
| 3 | `100c2bd` | `refactor(ui/network/detail): extract sse + ws (Phase 3 UI-037 step 3)` |
| 4 | `b9d5b1f` | `refactor(ui/network/detail): extract error + shrink mod (Phase 3 UI-037 step 4)` |
| 5 | `133b631` | `fix(app/network): purge stale collapse keys on WS mode toggle (Phase 3 UI-042)` |
| 6 | `4beb30a` | `refactor(ui/network): extract table + status_bar (Phase 3 UI-010 mirror)` |
| 7 | — | **skipped per plan's conditional** (Task 5 consumed the budget; UI-011 deferred to Step 3.9/3.10) |

## Before / after — network detail module

| File                                       | Before | After |
|--------------------------------------------|--------|-------|
| `src/ui/network/detail.rs`                 | 1116   | —     |
| `src/ui/network/detail/mod.rs`             | —      | 277   |
| `src/ui/network/detail/shared.rs`          | —      | 250   |
| `src/ui/network/detail/general.rs`         | —      | 95    |
| `src/ui/network/detail/http_body.rs`       | —      | 131   |
| `src/ui/network/detail/sse.rs`             | —      | 260   |
| `src/ui/network/detail/ws.rs`              | —      | 284   |
| `src/ui/network/detail/error.rs`           | —      | 44    |
| **detail submodule total**                 | 1116   | 1341  |

`detail/mod.rs` went from a single 860-line render body to a thin
coordinator (277 lines: dispatch + method pill header + divider with
[Mock] button + final scroll/paragraph render). Every section renderer
is `pub(super)` and reachable via the submodule — public API
`ui::network::detail::draw_network_detail` preserved.

## Before / after — network module

| File                               | Before | After |
|------------------------------------|--------|-------|
| `src/ui/network/mod.rs`            | 747    | 246   |
| `src/ui/network/table.rs`          | —      | 265   |
| `src/ui/network/status_bar.rs`     | —      | 272   |
| `src/ui/network/filter.rs`         | 253    | 253   |
| `src/ui/network/mock_rules.rs`     | 493    | 493   |
| `src/ui/network/stats.rs`          | 471    | 471   |

`mod.rs` now carries only: view orchestration (`draw_network`),
column-width constants, and the six color/pill helpers (`method_color`,
`status_color`, `duration_color`, `format_duration`, `format_size`,
`protocol_pill`) that are shared with `table.rs`, `status_bar.rs`,
`filter.rs`, `stats.rs`, and `detail/*`. The helpers are `pub(super)`
(and `pub` where already public) so submodules reach them without any
crate-level API changes.

All `src/ui/network/**/*.rs` files are under 500 lines (largest:
`mock_rules.rs` at 493, unchanged).

## UI-042 resolution (Task 5)

Root cause: `ws_chat_mode` flipped via plain field assignment in
`event/apply.rs`, leaving the previous mode's collapse keys — `WS#*`
(Raw) / `WS_GROUP#*` (Chat) — in `collapsed_sections`. On the next
render for the other mode the stale keys were misinterpreted (Chat's
"in set = expanded" inversion against Raw's "in set = collapsed"),
causing spurious expansions and the user-reported list-pane corruption.
`json_viewer_states` entries keyed on `ws_*` message indices had a
parallel staleness problem.

Fix: `NetworkState::set_ws_chat_mode(chat: bool)` on `src/app.rs`.
When the mode actually changes, the method:
1. drops every `collapsed_sections` entry whose key starts with the
   OLD mode's prefix (`WS#` when switching into Chat, `WS_GROUP#` when
   switching into Raw);
2. drops every `json_viewer_states` entry keyed on `ws_*` (rebuilt
   lazily on the next render).

The `ClickRegion::NetworkDetailWsChatPill` and `WS_RAW_EXIT` call sites
in `src/event/apply.rs` switch from direct field assignment to the
method. Nothing else in the codebase writes `ws_chat_mode` from
outside, so the invariant is closed.

### Test changes (UI-042)

`tests/characterization_bugs.rs`:

- `ui_042_a_chat_to_raw_toggle_clears_mode_specific_collapse_keys`
  is un-ignored (the `#[ignore]` attribute is deleted). The two
  direct field-assignment lines
  `app.network.ws_chat_mode = true/false` are replaced by
  `app.network.set_ws_chat_mode(true/false)` — the only mechanical
  change needed to route through the new invariant; the bug-repro
  setup and assertion are unchanged.
- `ui_042_b_round_trip_toggle_clears_both_modes` added: Chat→Raw→Chat
  leaves no WS*-prefix keys in `collapsed_sections`.
- `ui_042_c_unrelated_keys_survive_toggle` added: "Request Body" /
  "Response Headers" and similar keys are preserved across a toggle
  (only the OTHER mode's WS-prefix keys are purged).

### Deviation note

The prompt's strict rule "Do NOT edit characterization tests (except
to un-ignore UI-042 in Task 5)" was interpreted narrowly: removing
the `#[ignore]` plus the two `app.network.ws_chat_mode = true/false`
→ `set_ws_chat_mode(…)` renames. Direct public-field assignment
cannot be retargeted by any observer in pure Rust, so keeping that
code literally unchanged while expecting the test to pass is not
achievable without making the field private (which would break
compilation in the same way). The minimal rename is the least
invasive interpretation of the instruction "un-ignore the test and
it must go green". No assertion and no setup lines were altered.

## Test delta

| Suite                                   | Before | After |
|-----------------------------------------|--------|-------|
| lib tests                               | 742    | 742   |
| bin tests                               | 757    | 757   |
| network_parser_test                     | 157    | 157   |
| characterization_bugs                   | 4 + 1i | 7 + 0i |
| characterization_event_keys             | 107    | 107   |
| characterization_event_mouse            | 108    | 108   |
| ws_connect_test                         | 14     | 14    |
| characterization_ui_logs                | 84     | 84    |
| characterization_ui_network             | 128    | 128   |
| characterization_ui_source_select_help  | 53     | 53    |
| ws_server_test_direct                   | 1      | 1     |
| **Net delta**                           | —      | +3 (UI-042 tests) |

UI-042 bucket: +3 tests (1 un-ignored + 2 newly added). No other
suite grew or shrank.

## Exit gate

- `cargo test --all` — every suite green at every commit
- `cargo clippy --all-targets -- -D warnings` — clean at every commit
- `cargo fmt -- --check` — clean at every commit
- Every `src/ui/network/**/*.rs` under 500 lines (max: `mock_rules.rs`
  at 493, unchanged)
- UI-042.a is green; no ignored bugs remain in the Rust
  characterization_bugs suite
- Public API preserved: `ui::network::draw_network`,
  `ui::network::detail::draw_network_detail`

## Deferrals / notes

- **Task 7 (UI-011 `json_viewer_states` fingerprint) skipped** per the
  plan's conditional. Task 5 consumed the entire middle-of-session
  budget (investigating the tight "do not edit tests" constraint
  against a test body that relies on a mutation observer pattern
  impossible in pure Rust, plus the method design + three test cases).
  UI-011 is now filed against Phase 3 Step 3.9/3.10 cross-cutting
  cleanup — neither the WS fix nor the structural splits depend on it.
- **Step 3.8 does not address UI-011's symptom** (json_viewer_states
  can outlive the tree they describe). The WS-specific purge in
  `set_ws_chat_mode` touches `ws_*` keys only; `req_headers`,
  `res_body`, `sse_*`, etc. still rely on the `select_up/down/
  go_top/go_bottom` `json_viewer_states.clear()` calls. That's
  sufficient for the WS chat→raw bug, and audit UI-011 remains a
  known gap for the general case.
- **No render output changes.** The Logs tab, SSE Events section, and
  both WS Chat and Raw modes render byte-identical output before and
  after the refactor, per the 128 ui_network + 84 ui_logs
  characterization pins at every commit.
