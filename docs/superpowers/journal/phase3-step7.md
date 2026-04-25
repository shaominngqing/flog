# Phase 3 Step 3.7 — UI Logs View Split (Journal)

## 入口

- 日期：2026-04-24 → 2026-04-25
- Git HEAD at entry: `f35946b` (master, after the step 3.7 plan landed)
- Regression fence at entry:
  - `characterization_ui_logs` — 84 tests
  - `characterization_ui_source_select_help` — 53 tests
  - `characterization_event_keys` — 107 tests
  - `characterization_event_mouse` — 108 tests
  - `characterization_bugs` — 4 passing + 1 ignored (UI-042, owned by Step 3.8)
- Baseline file sizes:
  - `src/ui/logs/mod.rs` — 1494 lines
  - `src/ui/help.rs` — 543 lines

## Commits in order

| # | SHA | Summary |
|---|-----|---------|
| 1 | `3945d7b` | `refactor(ui/help): import shared palette (Phase 3 UI-013)` |
| 2 | `48b106c` | `refactor(ui/logs): extract toolbar submodule (Phase 3 UI-010 step 1)` |
| 3 | `66db477` | `refactor(ui/logs): extract status_bar submodule (Phase 3 UI-010 step 2)` |
| 4 | `e14d8df` | `refactor(ui/logs): extract list submodule (Phase 3 UI-010 step 3)` |
| 4b | `3f50f29` | `refactor(ui/logs): relocate highlight_with_filter into highlight.rs` |
| 5 | `b0b475f` | `refactor(ui/logs): extract empty_states submodule (Phase 3 UI-010 step 4)` |
| 6 | `ad89930` | `refactor(ui/help): content split (Phase 3 UI-014)` |

Task 4 landed as two commits: the bulk `draw_log_list` extraction left
`list.rs` at 510 lines — 10 over the 500-line exit gate. Rather than
amending Task 4, a small follow-up (`3f50f29`) relocates
`highlight_with_filter` into `highlight.rs` where it belongs next to
`auto_highlight`. The logical Task 4 is the two commits together.

## Before / after — logs module

| File                              | Before | After |
|-----------------------------------|--------|-------|
| `src/ui/logs/mod.rs`              | 1494   | 302   |
| `src/ui/logs/toolbar.rs`          | —      | 259   |
| `src/ui/logs/status_bar.rs`       | —      | 197   |
| `src/ui/logs/list.rs`             | —      | 474   |
| `src/ui/logs/empty_states.rs`     | —      | 304   |
| `src/ui/logs/highlight.rs`        | 132    | 173   |
| `src/ui/logs/jump.rs`             | 43     | 43    |
| `src/ui/logs/stats.rs`            | 135    | 135   |
| **logs submodule total**          | 1804   | 1887  |

Total line count crept up slightly (per-file headers + doc comments);
every file now comfortably fits the 500-line budget, and the heaviest
function (`draw_log_list`) is isolated in `list.rs`.

## Before / after — help module

| File                                | Before | After |
|-------------------------------------|--------|-------|
| `src/ui/help.rs`                    | 543    | —     |
| `src/ui/help/mod.rs`                | —      | 278   |
| `src/ui/help/content/mod.rs`        | —      | 8     |
| `src/ui/help/content/logs.rs`       | —      | 202   |
| `src/ui/help/content/network.rs`    | —      | 119   |
| **help module total**               | 543    | 607   |

Task 6 was triggered because Task 1 left `help.rs` at 559 lines
(deduped the 12 lines of palette, added a 28-line regression test).
The split turned `help.rs` into a directory with per-view content
submodules; every file now under 500 lines.

## Test delta

| Suite                                   | Before | After |
|-----------------------------------------|--------|-------|
| lib tests                               | 741    | 742   |
| bin tests                               | 756    | 757   |
| characterization_ui_logs                | 84     | 84    |
| characterization_ui_source_select_help  | 53     | 53    |
| characterization_event_keys             | 107    | 107   |
| characterization_event_mouse            | 108    | 108   |
| characterization_bugs                   | 4 + 1i | 4 + 1i |
| network_parser_test                     | 157    | 157   |
| ui_components                           | 128    | 128   |
| ws_connect_test                         | 14     | 14    |
| ws_server_test_direct                   | 1      | 1     |
| **Net delta**                           | —      | +2    |

Net +2 comes from the single regression test added in Task 1
(`help_content_bg_uses_shared_base_palette`), counted once per crate
(lib + bin). Zero characterization tests were modified; the UI-042
ignore stays ignored.

## Exit gate

- `cargo test --all` — every suite green at every commit
- `cargo clippy --all-targets -- -D warnings` — clean at every commit
- `cargo fmt -- --check` — clean at every commit
- Every `src/ui/logs/*.rs` and `src/ui/help/**.rs` under 500 lines
- No public API changes: `ui::logs::draw_logs` and `ui::help::draw_help`
  keep their signatures; extracted submodule functions are `pub(super)`
- UI-042 stays ignored — owned by Step 3.8

## Deferrals / notes

- **Task 4 needed a follow-up commit.** The plan underestimated
  `list.rs` by ~10 lines. Relocating `highlight_with_filter` into
  `highlight.rs` brings `list.rs` to 474 while keeping the highlight
  logic in the right neighborhood.
- **Task 5 kept `GRAD` inside `empty_states.rs`.** The plan proposed a
  `gradient_line` helper lives in `empty_states.rs`; it does, and it
  takes its palette locally via `GRAD` rather than pulling from
  `crate::ui`. The gradient sequence is intentionally different from
  the shared palette (it cycles blue→sapphire→teal→green→teal→sapphire
  for ASCII-art shading), so this stays a module-local constant.
- **Task 6 — not skipped.** Task 1 did drop 12 palette lines, but the
  test-case addition brought the net change back over 500. The split
  landed as planned.

Next step (3.8) picks up from a clean master with a small, cohesive
logs module and a sectioned help overlay.
