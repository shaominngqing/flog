# Phase 3 Step 3.7 — UI Logs View Split

> REQUIRED SUB-SKILL: superpowers:subagent-driven-development or executing-plans.

**Goal:** Split `src/ui/logs/mod.rs` (1494 lines) into cohesive submodules, each <500 lines. Resolve UI-010 (monolithic draw_logs), UI-013 (help palette duplication), UI-014 (help.rs 543 lines).

**Architecture:** Extract four submodules: `toolbar.rs` (toolbar op1 + op2 draws), `status_bar.rs`, `list.rs` (draw_log_list + entry_row_count_from_store + highlight_with_filter), `empty_states.rs` (not_connected / waiting_for_logs / no_matching_logs + logo/gradient helpers). Color/pill helpers stay in `mod.rs` as they're shared. `help.rs` → submodule directory with per-section content files.

**Red line:** no render behavior change. All 84 characterization_ui_logs tests + 53 ui_source_select_help tests stay green.

## Tasks

### Task 0 — pre-flight
- Verify HEAD, `cargo test --all` green, clippy clean, fmt clean.
- Baseline: `src/ui/logs/mod.rs` 1494, `src/ui/help.rs` 543.

### Task 1 — UI-013 help.rs palette dedup
Remove local Catppuccin constants in `src/ui/help.rs` (lines ~11-23). Import from `crate::ui::{BASE, MANTLE, ...}`. Verify rendered output unchanged. +1 test asserting a help buffer cell uses the shared palette color.
Commit: `refactor(ui/help): import shared palette (Phase 3 UI-013)`

### Task 2 — Extract `ui/logs/toolbar.rs`
Move `draw_toolbar_op1` (~206-284) and `draw_toolbar_op2` (~285-404) + `level_pill` + `level_badge` helpers into new `src/ui/logs/toolbar.rs`. Re-export from `mod.rs` as needed. Characterization tests green.
Commit: `refactor(ui/logs): extract toolbar submodule (Phase 3 UI-010 step 1)`

### Task 3 — Extract `ui/logs/status_bar.rs`
Move `draw_status_bar` (~433-590) + `draw_column_header` (~405-432) into new `status_bar.rs`.
Commit: `refactor(ui/logs): extract status_bar submodule (Phase 3 UI-010 step 2)`

### Task 4 — Extract `ui/logs/list.rs`
Move `draw_log_list` (~591-1062) + `entry_row_count_from_store` + `highlight_with_filter` into new `list.rs`. This is the biggest extraction (~500 lines).
Commit: `refactor(ui/logs): extract list submodule (Phase 3 UI-010 step 3)`

### Task 5 — Extract `ui/logs/empty_states.rs`
Move `draw_jump_to_bottom`, `draw_not_connected`, `draw_waiting_for_logs`, `draw_no_matching_logs`, `gradient_line`, `logo_lines` into new `empty_states.rs`.
Commit: `refactor(ui/logs): extract empty_states submodule (Phase 3 UI-010 step 4)`

### Task 6 — UI-014 help.rs content split (conditional)
If `src/ui/help.rs` still >500 lines after Task 1, split into `src/ui/help/mod.rs` + `src/ui/help/content/{keyboard,navigation,network}.rs` per audit proposal. If already under 500 after Task 1 (palette dedup saved ~12 lines, may not be enough), skip and journal-ack instead.
Commit: `refactor(ui/help): content split (Phase 3 UI-014)` or skip.

### Task 7 — exit gate + journal
Final: `cargo test --all` + clippy -D + fmt. Verify every `src/ui/logs/*.rs` and `src/ui/help*.rs` <500 lines. Write `docs/superpowers/journal/phase3-step7.md` with file line deltas + test count.
Commit: `docs(journal): Phase 3 Step 3.7 — UI Logs view split complete`

## Exit gates
- All 84 ui_logs + 53 ui_source_select_help + 215 event (keys+mouse) characterization tests green
- Each `src/ui/logs/*.rs` and `src/ui/help*.rs` < 500 lines
- clippy -D warnings clean

## 红线
- No render output changes — every assertion in characterization suites holds.
- No public API changes in `ui::logs::draw_logs` / `ui::help::draw_help`.
- `mod.rs` remains the public entry point; submodule functions stay `pub(super)` or `pub(crate)`.
