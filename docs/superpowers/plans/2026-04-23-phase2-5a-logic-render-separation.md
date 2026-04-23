# Phase 2.5A — Logic/Render Separation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract pure-logic functions from the top render files (`ui/logs/mod.rs`, `ui/network/mod.rs`, `ui/network/detail.rs`, `event.rs`, `app.rs`) so that Phase 2.5B can write characterization tests against pure functions instead of against `Frame`/`TestBackend`. No behavior change. No design choices. The new functions are *extraction by copy-lift*, not by redesign.

**Architecture:** Each extraction follows a strict recipe: (1) identify a self-contained block of computation inside a render function that doesn't mutate the frame, (2) give it a conservative signature that mirrors the block's current inputs/outputs, (3) call it from the original site so rendering behavior is byte-identical, (4) add one small smoke test per new function asserting the extraction is live. We do NOT reshape state, redesign abstractions, or rename things — that is Phase 3's job. We only move code so Phase 2.5B can point tests at it.

**Tech Stack:** Rust, ratatui. No new dependencies. `cargo-llvm-cov` for after/before coverage delta.

**Parent spec:** `docs/superpowers/specs/2026-04-22-project-cleanup-design.md` §4.2.2
**Audit source:** `docs/superpowers/audit/03-ui.md` (UI-008, UI-010, UI-029 give the extraction targets; 00-index.md "Notes for Phase 2.5" gives the testability verdicts).

---

## Guardrails (REPEAT — READ BEFORE EVERY TASK)

Every subagent prompt inherits these rules. Violating any is cause to abort the task and report back.

1. **No design choices.** If extracting a function requires picking a name for a new concept, introducing a new type, merging two concepts, or changing a signature's *shape* (e.g. flatten a tuple, split a struct) — STOP. That is Phase 3 work. Extract the function with the most boring possible signature that literally mirrors the current computation.
2. **Behavior byte-identical.** After extraction, the rendered output must be unchanged. Visual regression = rollback.
3. **Conservative signatures.** Extracted functions take the data they already use, return what the inline block already produces. No speculative args. No Option-ification. No Result-ification. No generics. If the block currently reads `app.network.entries[idx]`, the extracted function takes `&[NetworkEntry]` and an index, not `&NetworkState`.
4. **One small smoke test per new function.** The test asserts the extraction is wired correctly — it is NOT a characterization test (those belong to 2.5B). The smoke test just covers the happy path to prove the function exists and runs. 3-10 lines max.
5. **File placement: in-place.** New pure functions live in the SAME file they were extracted from (private `fn`, or `pub(crate)` if crossed-module), unless the block already contains obvious sibling logic in a separate module. No new files unless the extraction naturally groups more than 3 functions together (then create a sibling file named `*_logic.rs` alongside the original).
6. **Each task ends with**: `cargo test` green, `cargo clippy --all-targets -- -D warnings` green, `cargo fmt --check` clean. If clippy flags the new function (e.g. `too_many_arguments`), that's a signal the signature is wrong — either simplify or add a tracking `#[allow]` with `// Phase 3 redesign — see Audit <ID>`.
7. **Cannot-extract escape hatch.** If a candidate block is too coupled to mutate to extract safely (e.g. the block mutates `app` in three places via method calls), STOP and record in `docs/superpowers/journal/phase-2.5a-notes.md` as "candidate X declined, reason Y, assigned to Phase 3 as Audit-NEW-ID". Then add a new D-class Audit entry in `03-ui.md` for the needed Phase 3 refactor.
8. **No touching B-class bug behavior.** If extraction would normalize/fix a B-class bug (e.g. DOM-018 overlapping highlight ranges), the extracted function must preserve the bug. Phase 2.5B writes the red test; Phase 3 fixes. 2.5A is behavior-neutral.

---

## Extraction Targets (in order of difficulty)

Eight concrete extractions, ordered easy→hard. Each gets its own task. All pulled from Audit.

| # | Function | Source | Audit |
|---|---|---|---|
| 1 | `handle_sse_field_navigation(current_idx, count, direction) -> usize` | `src/event.rs` SSE merged mode key handling | UI-008 |
| 2 | `compute_visible_entry_range(total_filtered, offset, height) -> (start, end)` | `src/ui/logs/mod.rs` viewport logic | UI-010, UI-029 |
| 3 | `compute_entry_screen_height(entry, max_wrap_lines, terminal_width) -> usize` | `src/ui/logs/mod.rs` wrap logic | UI-010, UI-029 |
| 4 | `repeat_bar_normalized(count, max_w) -> usize` | `src/ui/logs/mod.rs:107-111` | UI-030 (keeps magic 50 — Phase 3 names it) |
| 5 | `compute_visible_network_range(total_filtered, offset, height) -> (start, end)` | `src/ui/network/mod.rs` viewport logic | UI-029 |
| 6 | `group_ws_messages_for_chat(...)` — expose existing domain fn to ui tests | `src/domain/ws_chat.rs` → already pure, just add tests | baseline 92% covered — skip extraction, go straight to tests in 2.5B |
| 7 | `sse_merge_field_paths(...)` — expose existing domain fn | `src/domain/sse_merge.rs` → already pure | baseline 91% covered — skip extraction |
| 8 | `detect_click_region_*(click_x, click_y, layout) -> Option<ClickKind>` | `src/event.rs` normal-mode mouse routing | UI-009, UI-016 |

Targets 6 and 7 are **no-op extractions** (the code is already pure) — we list them to make Phase 2.5B's work list complete. Only targets 1-5 and 8 need actual code movement.

Target 8 (click-region detection) is the hardest — mouse routing is heavily coupled to `LayoutCache`. If the subagent can't extract it cleanly (see guardrail 7), it becomes a new D-class Audit entry and defers to Phase 3.

---

## File Structure

All changes stay within existing files. No new files created unless a single task's extraction groups 3+ related pure functions (then a sibling `*_logic.rs` is created — none of targets 1-5, 8 reach that threshold).

**Files modified:**
- `src/event.rs` (targets 1, 8)
- `src/ui/logs/mod.rs` (targets 2, 3, 4)
- `src/ui/network/mod.rs` (target 5)

**Files created:** none expected.

**Files with new `#[cfg(test)] mod tests`**: same as modified files (tests go inline).

---

## Pre-flight (Task 0)

### Task 0: Baseline capture

**Files:** (read-only)

- [ ] **Step 0.1: Confirm HEAD**

Run: `git log --oneline -1`
Expected: `70ed4ef chore(audit): record Phase 2.5 coverage baseline`

- [ ] **Step 0.2: Confirm tests green + clippy green + fmt clean**

Run:
```bash
cargo test 2>&1 | grep "test result:"
cargo clippy --all-targets -- -D warnings 2>&1 | tail -3
cargo fmt --check && echo "fmt clean"
```

Expected:
- 4 `test result: ok` lines (204 lib + 209 bin + 1 integration + 0 doc)
- clippy `Finished` with no warnings
- `fmt clean`

- [ ] **Step 0.3: Pre-flight coverage snapshot**

Run:
```bash
cargo llvm-cov --summary-only 2>&1 | tail -3 > /tmp/phase2-5a-pre-coverage.txt
cat /tmp/phase2-5a-pre-coverage.txt
```

Expected: the `TOTAL` line — `22540 ... 31.48%` or similar.

Record for later comparison. The post-Phase-2.5A total should be slightly higher (extracted functions are now testable surface; the smoke tests add some real coverage).

---

## Task 1: Extract `handle_sse_field_navigation` (UI-008)

**Files:**
- Modify: `src/event.rs`

**Audit evidence (UI-008):** SSE merged mode 'j'/'k' keys each execute ~30 lines of nested conditionals that include a field-index increment/decrement clamped to `[0, count)`. The increment math is pure and stands out.

**Proposed signature (audit):**
```rust
fn handle_sse_field_navigation(current_idx: usize, count: usize, direction: Direction) -> usize
```

But `Direction` is not an existing type in this codebase. **Conservative change**: use `i32` (+1 / -1) or a local `enum Direction { Up, Down }` defined in `event.rs` next to the function. Define the enum locally — avoids introducing a type into `domain/`.

- [ ] **Step 1.1: Dispatch subagent**

Use `subagent_type: "general-purpose"`. Prompt:

```
You are executing Phase 2.5A Task 1. Read these first:
- docs/superpowers/plans/2026-04-23-phase2-5a-logic-render-separation.md (the Guardrails section especially)
- docs/superpowers/audit/03-ui.md entry UI-008

Scope: src/event.rs ONLY. Do not touch any other file.

Goal: extract a pure function `handle_sse_field_navigation` that
encapsulates the field-index increment/decrement clamping logic currently
inline in the SSE merged mode 'j' and 'k' key handlers.

Steps:

1. Use Read on src/event.rs lines 1340-1420 to find the exact j/k handlers
   under `if app.network.sse_merged_mode && app.network.show_detail`.

2. Identify the 2 inline computations (one per key). The math should look
   roughly like:
     let new_idx = (app.network.sse_merged_field_idx + 1).min(count - 1);
   and
     let new_idx = app.network.sse_merged_field_idx.saturating_sub(1);

3. At the bottom of src/event.rs (inside the existing module, before any
   `mod tests`), define:

   ```rust
   /// Phase 2.5A — extracted from UI-008.
   /// Direction for SSE merged field navigation.
   enum SseNavDir {
       Up,
       Down,
   }

   /// Pure: given current field index and total count, return the new index
   /// after one navigation step. Saturates at 0 and count-1. If count is 0,
   /// returns current_idx unchanged (caller is responsible for not calling
   /// when no fields exist).
   fn handle_sse_field_navigation(current_idx: usize, count: usize, dir: SseNavDir) -> usize {
       if count == 0 {
           return current_idx;
       }
       match dir {
           SseNavDir::Up => current_idx.saturating_sub(1),
           SseNavDir::Down => (current_idx + 1).min(count - 1),
       }
   }
   ```

   Use `Edit` to insert this block.

4. Replace each inline computation at the two key handlers with a call
   to the new function, preserving the rest of the surrounding side
   effects (app.network.sse_merged_field_idx assignment, rule rebuild).
   The call site should look like:
     let new_idx = handle_sse_field_navigation(
         app.network.sse_merged_field_idx,
         count,
         SseNavDir::Down,  // or Up
     );
     app.network.sse_merged_field_idx = new_idx;

5. Add a `#[cfg(test)] mod tests` at end of src/event.rs if none exists,
   or append to it. Add:

   ```rust
   #[cfg(test)]
   mod tests {
       use super::*;

       #[test]
       fn sse_nav_down_increments_up_to_bound() {
           assert_eq!(handle_sse_field_navigation(0, 3, SseNavDir::Down), 1);
           assert_eq!(handle_sse_field_navigation(1, 3, SseNavDir::Down), 2);
           assert_eq!(handle_sse_field_navigation(2, 3, SseNavDir::Down), 2); // saturate
       }

       #[test]
       fn sse_nav_up_saturates_at_zero() {
           assert_eq!(handle_sse_field_navigation(2, 3, SseNavDir::Up), 1);
           assert_eq!(handle_sse_field_navigation(1, 3, SseNavDir::Up), 0);
           assert_eq!(handle_sse_field_navigation(0, 3, SseNavDir::Up), 0); // saturate
       }

       #[test]
       fn sse_nav_empty_is_noop() {
           assert_eq!(handle_sse_field_navigation(0, 0, SseNavDir::Up), 0);
           assert_eq!(handle_sse_field_navigation(5, 0, SseNavDir::Down), 5);
       }
   }
   ```

6. Run:
   - cargo build (must succeed)
   - cargo test 2>&1 | grep "test result:" (all 4 lines must be ok;
     bin count should go UP by 3 from 209 to 212)
   - cargo clippy --all-targets -- -D warnings (must pass)
   - cargo fmt --check (must be clean)

7. Do NOT commit. Report:
   - git diff --stat
   - cargo test counts
   - clippy status
   - Anything that deviated from the plan
```

- [ ] **Step 1.2: Review and verify**

Main Claude checks:
- `git diff src/event.rs` — only SSE 'j' and 'k' handlers changed + new helper + new tests; nothing else
- `cargo test` output shows bin test count went from 209 to 212
- `cargo clippy --all-targets -- -D warnings` passes

If any check fails, dispatch a fix subagent with the specific failure quoted.

- [ ] **Step 1.3: Commit**

```bash
git add src/event.rs
git commit -m "$(cat <<'EOF'
refactor(event): extract handle_sse_field_navigation pure fn (Phase 2.5A UI-008)

Factors the index-clamping math out of the inline j/k key handlers in
SSE merged mode into a pure function. Adds 3 smoke tests proving
extraction is live. No behavior change.

Guardrails: conservative signature (usize + count + local enum), no
redesign. Local SseNavDir enum kept private to event.rs — exposing it
is a Phase 3 decision.

Audit: docs/superpowers/audit/03-ui.md UI-008
Spec:  docs/superpowers/specs/2026-04-22-project-cleanup-design.md §4.2.2

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Extract `compute_visible_entry_range` (UI-010, UI-029)

**Files:**
- Modify: `src/ui/logs/mod.rs`

**Audit evidence (UI-010):** `draw_logs`/`draw_log_list` computes the visible slice of filtered entries inline during rendering. Given `offset`, `height`, and the number of filtered entries, the math is pure.

**Conservative signature:**
```rust
fn compute_visible_entry_range(total_filtered: usize, offset: usize, height: usize) -> (usize, usize)
```

Returns `(start, end)` where `start = offset.min(total_filtered)` and `end = (start + height).min(total_filtered)`. This is the literal pattern the renderer uses today.

- [ ] **Step 2.1: Dispatch subagent**

Use `subagent_type: "general-purpose"`. Prompt:

```
You are executing Phase 2.5A Task 2. Read Guardrails in
docs/superpowers/plans/2026-04-23-phase2-5a-logic-render-separation.md.

Scope: src/ui/logs/mod.rs ONLY.

Goal: extract the viewport visible-range computation from the render
pipeline into a private pure function.

Steps:

1. Use Read on src/ui/logs/mod.rs lines 116-250 (draw_logs and
   draw_log_list) to find where `offset`, `height`, and filtered-entry
   slicing intersect. The computation should look like:

     let start = scroll_offset.min(total);
     let end = (start + height).min(total);
     // then slice entries[start..end]

   If the current code uses different variable names, LITERALLY mirror
   them — don't rename. If the current code clamps differently (e.g.
   `offset.saturating_sub(something)`), mirror that exactly.

2. Define at the top of src/ui/logs/mod.rs (after the existing `use`
   statements, before the first pub fn):

   ```rust
   /// Phase 2.5A — extracted from UI-010.
   /// Pure: given total count, scroll offset, and viewport height,
   /// return the (start, end) half-open index range of entries that
   /// should be rendered. Clamps both ends to [0, total].
   pub(crate) fn compute_visible_entry_range(
       total_filtered: usize,
       offset: usize,
       height: usize,
   ) -> (usize, usize) {
       let start = offset.min(total_filtered);
       let end = start.saturating_add(height).min(total_filtered);
       (start, end)
   }
   ```

   The exact body must mirror the inline version. If the inline code
   uses `offset.min(total)` without saturating_add, remove
   saturating_add from the extracted function and use plain `+`. Do
   NOT introduce new safety — mirror existing.

3. Replace the inline computation with a call:

     let (start, end) = compute_visible_entry_range(total, offset, height);
     // (where `height` is whatever variable the inline code used)

   Leave the downstream slice `entries[start..end]` unchanged.

4. Add to the existing `#[cfg(test)] mod tests` in src/ui/logs/mod.rs
   (create one at end of file if none exists):

   ```rust
   #[cfg(test)]
   mod tests {
       use super::*;

       #[test]
       fn visible_range_basic_window() {
           assert_eq!(compute_visible_entry_range(100, 10, 20), (10, 30));
       }

       #[test]
       fn visible_range_clamps_end_to_total() {
           assert_eq!(compute_visible_entry_range(15, 10, 20), (10, 15));
       }

       #[test]
       fn visible_range_clamps_start_to_total() {
           assert_eq!(compute_visible_entry_range(5, 100, 20), (5, 5));
       }

       #[test]
       fn visible_range_empty() {
           assert_eq!(compute_visible_entry_range(0, 0, 10), (0, 0));
       }
   }
   ```

5. Run verification as Task 1 Step 1.1 item 6 (tests + clippy + fmt).
   Expected new test count delta: +4 on bin (from 212 to 216 after Task 1).

6. Do NOT commit. Report.
```

- [ ] **Step 2.2: Review**

Same checks as Task 1. Spot-check: the `draw_logs` rendering output must not have moved — if you have a way to manually run `cargo run` on a log fixture and visually confirm, do so (optional, time permitting).

- [ ] **Step 2.3: Commit**

```bash
git add src/ui/logs/mod.rs
git commit -m "$(cat <<'EOF'
refactor(ui/logs): extract compute_visible_entry_range (Phase 2.5A UI-010)

Factors viewport slicing math out of draw_logs into a pure pub(crate)
function. Adds 4 smoke tests. No behavior change.

Audit: docs/superpowers/audit/03-ui.md UI-010, UI-029
Spec:  docs/superpowers/specs/2026-04-22-project-cleanup-design.md §4.2.2

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Extract `compute_entry_screen_height` (UI-010, UI-029)

**Files:**
- Modify: `src/ui/logs/mod.rs`

**Audit evidence:** Per-entry wrap height (how many screen rows a single `LogEntry` occupies given terminal width) is computed inline during rendering. The formula is based on `entry.message.len()`, `max_wrap_lines`, and width. Pure.

- [ ] **Step 3.1: Dispatch subagent**

Use `subagent_type: "general-purpose"`. Prompt:

```
You are executing Phase 2.5A Task 3. Read Guardrails.

Scope: src/ui/logs/mod.rs ONLY.

Goal: extract per-entry wrap-height computation into a pure function.

Steps:

1. Use Read + Grep on src/ui/logs/mod.rs to find the inline wrapping
   logic. Keywords to search: `wrap`, `max_wrap`, `lines()`, `height`,
   `UnicodeWidthStr`, `width`. The pattern is typically:

     let wrapped = wrap_multiline(message, max_width);
     let lines = wrapped.lines().count().min(MAX_WRAP_LINES);

   Or similar. The EXACT pattern depends on what's there — use Read to
   see it.

2. If the inline pattern calls existing helper `wrap_multiline` or
   similar, the extraction is just moving the "lines().count().min(...)"
   tail into a named function. If the inline code is 5+ lines with
   multiple branches (e.g. stack-trace entries get different treatment),
   mirror the branching in the extracted function — DO NOT SIMPLIFY.

3. Define at the top of src/ui/logs/mod.rs:

   ```rust
   /// Phase 2.5A — extracted from UI-010.
   /// Pure: given an entry and terminal constraints, return how many
   /// screen rows it occupies when rendered. Mirrors the inline wrap
   /// logic in draw_log_list exactly.
   pub(crate) fn compute_entry_screen_height(
       entry: &crate::domain::entry::LogEntry,
       max_wrap_lines: usize,
       terminal_width: u16,
   ) -> usize {
       // Copy the inline branching here, substituting `entry` for the
       // local variable name used inline.
       // ...
   }
   ```

   LEAVE the body blank in this step — fill it from what you read in
   step 1. If the inline version references constants defined in
   logs/mod.rs (e.g. `MAX_WRAP_LINES`), the extracted fn uses them
   directly.

4. Replace the inline block with a call to the new function. Any local
   variable names should stay the same — only the computation is
   replaced.

5. Add to `#[cfg(test)] mod tests`:

   ```rust
   #[test]
   fn entry_screen_height_single_line() {
       let entry = crate::domain::entry::LogEntry {
           timestamp: String::new(),
           tag: String::new(),
           level: crate::domain::entry::LogLevel::Info,
           message: "short".to_string(),
           extra_lines: Vec::new(),
           ..Default::default()
       };
       assert!(compute_entry_screen_height(&entry, 5, 80) >= 1);
   }

   #[test]
   fn entry_screen_height_wraps_long_message() {
       let long = "a".repeat(500);
       let entry = crate::domain::entry::LogEntry {
           timestamp: String::new(),
           tag: String::new(),
           level: crate::domain::entry::LogLevel::Info,
           message: long,
           extra_lines: Vec::new(),
           ..Default::default()
       };
       // Long message should wrap to > 1 row on a narrow-ish width.
       assert!(compute_entry_screen_height(&entry, 5, 40) > 1);
   }

   #[test]
   fn entry_screen_height_capped_by_max_wrap() {
       let very_long = "x".repeat(5000);
       let entry = crate::domain::entry::LogEntry {
           timestamp: String::new(),
           tag: String::new(),
           level: crate::domain::entry::LogLevel::Info,
           message: very_long,
           extra_lines: Vec::new(),
           ..Default::default()
       };
       assert_eq!(compute_entry_screen_height(&entry, 3, 40), 3);
   }
   ```

   IMPORTANT — if LogEntry does not implement Default, the `..Default::default()`
   spread will not compile. Use Read on src/domain/entry.rs to see
   LogEntry's actual struct; build the test entries with explicit
   fields for every required field. Do not ADD Default to LogEntry
   (that's a design choice); just write test entries fully.

6. Verify and report. Expected bin test count: 216 → 219.

7. Do NOT commit.
```

- [ ] **Step 3.2: Review**

Check the extracted body mirrors the inline one. Common mistake: the extracted fn forgets to include `extra_lines` lines in the height (if applicable).

- [ ] **Step 3.3: Commit**

```bash
git add src/ui/logs/mod.rs
git commit -m "$(cat <<'EOF'
refactor(ui/logs): extract compute_entry_screen_height (Phase 2.5A UI-010)

Factors per-entry wrap-height computation out of draw_log_list into a
pure pub(crate) function. Adds 3 smoke tests. No behavior change.

Audit: docs/superpowers/audit/03-ui.md UI-010, UI-029

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Extract `repeat_bar_normalized` (UI-030)

**Files:**
- Modify: `src/ui/logs/mod.rs`

**Audit evidence (UI-030):**
```rust
fn repeat_bar(count: usize, max_w: usize) -> String {
    let len = (count.min(50) * max_w) / 50;
    format!("x{} {}", count, "█".repeat(len.min(max_w)))
}
```

The magic `50` is saturation threshold. Phase 3 (UI-030) will name it; Phase 2.5A just extracts the length calculation as a pure function so we can test it without assembling a String.

- [ ] **Step 4.1: Dispatch subagent**

```
You are executing Phase 2.5A Task 4. Read Guardrails.

Scope: src/ui/logs/mod.rs ONLY.

Goal: extract the length calculation from repeat_bar into a pure
function. The magic number 50 stays in place (Phase 3 UI-030 names it).

Steps:

1. Read src/ui/logs/mod.rs:107-111 to confirm the repeat_bar definition.

2. Add a new pure fn next to repeat_bar (same file, same mod):

   ```rust
   /// Phase 2.5A — extracted from UI-030.
   /// Pure: given a count and max width, return how many chars of the
   /// bar should be rendered. Saturates at count 50 (magic constant
   /// preserved from the original; Phase 3 UI-030 renames it).
   pub(crate) fn repeat_bar_normalized(count: usize, max_w: usize) -> usize {
       let len = (count.min(50) * max_w) / 50;
       len.min(max_w)
   }
   ```

3. Replace the inline length computation inside repeat_bar with a call:

   ```rust
   fn repeat_bar(count: usize, max_w: usize) -> String {
       let len = repeat_bar_normalized(count, max_w);
       format!("x{} {}", count, "█".repeat(len))
   }
   ```

4. Add to tests:

   ```rust
   #[test]
   fn repeat_bar_zero_count() {
       assert_eq!(repeat_bar_normalized(0, 20), 0);
   }

   #[test]
   fn repeat_bar_saturates_at_50() {
       // at count=50, returns max_w (full bar)
       assert_eq!(repeat_bar_normalized(50, 20), 20);
       // at count=100, also returns max_w (saturated)
       assert_eq!(repeat_bar_normalized(100, 20), 20);
   }

   #[test]
   fn repeat_bar_proportional() {
       // at count=25 (half of 50), returns max_w/2
       assert_eq!(repeat_bar_normalized(25, 20), 10);
   }

   #[test]
   fn repeat_bar_zero_width() {
       assert_eq!(repeat_bar_normalized(42, 0), 0);
   }
   ```

5. Verify. Expected bin test count: 219 → 223.

6. Do NOT commit.
```

- [ ] **Step 4.2: Review + commit**

```bash
git add src/ui/logs/mod.rs
git commit -m "$(cat <<'EOF'
refactor(ui/logs): extract repeat_bar_normalized (Phase 2.5A UI-030)

Pulls the length calculation out of repeat_bar into a pure function.
The magic constant 50 stays inline per Guardrails; Phase 3 UI-030
renames it to REPEAT_BAR_MAX_COUNT.

Audit: docs/superpowers/audit/03-ui.md UI-030

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Extract `compute_visible_network_range` (UI-029)

**Files:**
- Modify: `src/ui/network/mod.rs`

Mirror of Task 2 but for the Network tab. The extraction should use the SAME signature as `compute_visible_entry_range` if the underlying math is identical. If the Network tab has a subtly different clamp (e.g. uses `show_detail` as an offset adjustment), mirror that exactly.

- [ ] **Step 5.1: Dispatch subagent**

```
You are executing Phase 2.5A Task 5. Read Guardrails.

Scope: src/ui/network/mod.rs ONLY.

Goal: extract the network-tab viewport visible-range computation into
a private pure function. Mirror Task 2 if the math is identical; if
not, mirror the existing network-specific logic literally.

Steps:

1. Read src/ui/network/mod.rs — find draw_network and draw_network_list
   (names may differ; grep for "scroll_offset", "height", "visible").

2. Find the inline visible-range computation. Compare it to
   compute_visible_entry_range in src/ui/logs/mod.rs.
   - If identical: define compute_visible_network_range with the same
     body.
   - If different: extract with the literal differences. Do NOT unify
     the two functions (that's a Phase 3 decision — UI-006 already
     flagged the asymmetry).

3. Add:

   ```rust
   /// Phase 2.5A — extracted from UI-029.
   /// Pure: network-tab viewport slicing. Mirrors the inline logic
   /// in draw_network. May differ from compute_visible_entry_range
   /// in src/ui/logs/mod.rs — Phase 3 UI-006 will decide whether to
   /// unify.
   pub(crate) fn compute_visible_network_range(
       total_filtered: usize,
       offset: usize,
       height: usize,
   ) -> (usize, usize) {
       // Body mirrors the inline version from draw_network. Fill in.
   }
   ```

4. Replace inline call site.

5. Add tests that mirror Task 2's tests but call
   compute_visible_network_range:

   ```rust
   #[test]
   fn network_visible_range_basic_window() {
       assert_eq!(compute_visible_network_range(100, 10, 20), (10, 30));
   }

   #[test]
   fn network_visible_range_clamps_end() {
       assert_eq!(compute_visible_network_range(15, 10, 20), (10, 15));
   }

   #[test]
   fn network_visible_range_empty() {
       assert_eq!(compute_visible_network_range(0, 0, 10), (0, 0));
   }
   ```

6. Verify. Expected bin test count: 223 → 226.

7. Do NOT commit.
```

- [ ] **Step 5.2: Review + commit**

```bash
git add src/ui/network/mod.rs
git commit -m "$(cat <<'EOF'
refactor(ui/network): extract compute_visible_network_range (Phase 2.5A UI-029)

Factors network-tab viewport slicing out of draw_network into a pure
pub(crate) fn. Intentionally NOT unified with compute_visible_entry_range
in ui/logs — audit UI-006 already flagged the asymmetry; Phase 3 decides
whether to merge.

Audit: docs/superpowers/audit/03-ui.md UI-029, UI-006

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Attempt click-region extraction (UI-009, UI-016)

**Files:**
- Modify: `src/event.rs`
- Possibly: new `src/event_click_regions.rs` if extraction groups 3+ fns

**Risk:** this is the hardest target. Mouse routing in `event.rs` references `LayoutCache` (coordinate metadata the renderer populates). A "pure" click-region function takes click `(x, y)` + layout snapshot + returns `Option<ClickKind>`. If the current code mutates `app` inside the click handler rather than returning a kind, extraction is hard.

**Escape hatch (Guardrail 7):** if the subagent can't extract without designing new types or moving mutations, it **records the attempt in `phase-2.5a-notes.md`** and adds a new D-class Audit entry `UI-NEW` pointing to the needed Phase 3 refactor. Phase 3 then does the decoupling properly. Task 6 completes with no code change.

- [ ] **Step 6.1: Dispatch exploratory subagent FIRST**

```
You are executing Phase 2.5A Task 6 — an EXPLORATORY pass. You are NOT
writing code yet. You're deciding whether extraction is safe.

Scope: read-only audit of src/event.rs mouse routing + LayoutCache
usage.

Goal: determine if click-region detection can be extracted into a pure
function without violating Phase 2.5A Guardrails (no design choices,
no new types, no mutation restructuring).

Read:
- docs/superpowers/plans/2026-04-23-phase2-5a-logic-render-separation.md
  Task 6 and Guardrails
- docs/superpowers/audit/03-ui.md UI-009 and UI-016
- src/event.rs normal-mode mouse handler (handle_normal_mouse or
  similar — grep for `MouseEventKind::Down` or `MouseEvent`)
- src/app.rs LayoutCache struct

Evaluate:

1. Can click-region detection be expressed as a function
     fn detect_click_region(x: u16, y: u16, layout: &LayoutCache) -> Option<ClickKind>
   WITHOUT introducing a new ClickKind enum? If every branch would have
   to invent a tag (e.g. TabBar, DevicePicker, FilterPill, ...), then
   ClickKind IS a new type and the answer is NO — return to Phase 3.

2. Does the mouse handler body mutate `app` before knowing the region?
   If yes, extraction is unsafe — the mutations are interleaved with
   the region detection.

3. Are there nested inline regions (e.g. tab bar contains multiple
   buttons; network row contains multiple pills)? How deep is the
   decision tree? If deeper than 3 levels, the "pure function"
   becomes a 200-line match block — likely requires redesign.

Report verdict (NOT code):
  VERDICT_A — Safe to extract. Provide: exact signature that doesn't
    introduce new types (use existing LayoutCache rect fields; return
    Option<existing_enum>). Lines to extract: [start, end]. No design
    choice needed.
  VERDICT_B — Requires new type (ClickKind, Region, whatever). STOP
    here. Write a new D-class audit entry (UI-NEW) in 03-ui.md
    describing the needed Phase 3 refactor. Do NOT attempt extraction.
  VERDICT_C — Mutations interleaved. STOP. New D-class audit entry.

Do NOT modify source code in this task. Only audit and report.
```

- [ ] **Step 6.2: Act on verdict**

If **VERDICT_A**:
  - Dispatch a second subagent to perform the extraction following the signature from Step 6.1's report
  - Follow Task 1-5 recipe (extract + tests + commit)

If **VERDICT_B or VERDICT_C**:
  - Dispatch a subagent to append a new D-class entry to `docs/superpowers/audit/03-ui.md` with id `UI-041` (next available), label `D`, title `Click-region detection cannot be pure-function-tested in current form (Phase 2.5A exploration found this)`, evidence summarizing Step 6.1's findings, proposed_action referring to Phase 3 UI Event step
  - Update `00-index.md` summary count + D-class list to include UI-041
  - Append to `docs/superpowers/journal/phase-2.5a-notes.md`:
    ```
    Task 6 outcome: VERDICT_<B|C>. Click-region extraction declined.
    New audit entry UI-041 created for Phase 3. No src/event.rs
    changes.
    ```
  - Commit only the audit/index/notes updates with a `docs(audit)` message.

---

## Task 7: Confirm domain layer "no-op" extractions (targets 6, 7)

**Files:** (no code changes expected)

Targets 6 (`group_ws_messages_for_chat`) and 7 (`sse_merge_field_paths`) are already pure functions in `src/domain/ws_chat.rs` (92.76% covered) and `src/domain/sse_merge.rs` (91.67% covered). They need no extraction.

- [ ] **Step 7.1: Record in notes**

Append to `docs/superpowers/journal/phase-2.5a-notes.md`:

```markdown
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
```

- [ ] **Step 7.2: Commit notes**

```bash
git add docs/superpowers/journal/phase-2.5a-notes.md
git commit -m "$(cat <<'EOF'
docs(journal): Phase 2.5A Task 7 — domain no-op extractions

Records that ws_chat + sse_merge are already pure and need no Phase
2.5A extraction. Phase 2.5B can target these functions directly.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: Final verification + coverage delta + phase commit

**Files:**
- Create or append: `docs/superpowers/journal/phase-2.5a.md` (the phase journal)

- [ ] **Step 8.1: Full green sweep**

```bash
cargo build 2>&1 | tail -3
cargo test 2>&1 | grep "test result:"
cargo clippy --all-targets -- -D warnings 2>&1 | tail -3
cargo fmt --check && echo fmt clean
```

Expected:
- build: Finished
- tests: 4 "ok" lines, bin count up from baseline 209 by roughly 16 (if Task 6 extracted) or 13 (if Task 6 declined). Similar delta on lib if the tests compile on both targets.
- clippy: no warnings
- fmt: clean

- [ ] **Step 8.2: Coverage delta**

```bash
cargo llvm-cov --summary-only 2>&1 | tail -3 > /tmp/phase2-5a-post-coverage.txt
diff /tmp/phase2-5a-pre-coverage.txt /tmp/phase2-5a-post-coverage.txt || true
```

Expected: the post-TOTAL line shows slightly higher line-coverage % than the pre (new pure functions are 100% covered by their smoke tests; extracted code paths are unchanged in coverage). Order-of-magnitude: pre ~31.48% → post ~32-33% (the extractions add ~100 lines of testable code, ~50-100 of it covered).

This is NOT a gate — it's a sanity check. If post < pre, something was wrong with the extractions (tests were not added); investigate.

- [ ] **Step 8.3: Write phase journal**

Use `Write` on `docs/superpowers/journal/phase-2.5a.md`:

```markdown
# Phase 2.5A Journal — Logic/Render Separation

## 入口
- 日期：2026-04-23
- Git HEAD: 70ed4ef (Phase 2.5 baseline commit)
- Baseline tests: 204 lib + 209 bin + 1 integration + 0 doc
- Baseline coverage: 31.48% (from .coverage-baseline.txt)
- 执行模式：Inline + scope subagents

## 时间线
- Task 0 pre-flight: baseline confirmed
- Task 1 (UI-008 SSE nav): extract handle_sse_field_navigation + 3 tests → commit <hash>
- Task 2 (UI-010/029 logs viewport): extract compute_visible_entry_range + 4 tests → commit <hash>
- Task 3 (UI-010 logs wrap): extract compute_entry_screen_height + 3 tests → commit <hash>
- Task 4 (UI-030 repeat bar): extract repeat_bar_normalized + 4 tests → commit <hash>
- Task 5 (UI-029 network viewport): extract compute_visible_network_range + 3 tests → commit <hash>
- Task 6 (UI-009/016 click region): VERDICT_<A|B|C> → <action taken>
- Task 7 (domain no-op): notes written → commit <hash>
- Task 8: verification + delta + this journal

## 关键统计（Phase 2.5A 出口）

| 指标 | Entry | Exit |
|---|---|---|
| cargo test (bin) | 209 | <count> |
| cargo test (lib) | 204 | <count> |
| cargo clippy | 0 warnings | 0 warnings |
| cargo fmt --check | clean | clean |
| Line coverage | 31.48% | <pct>% |
| New pure fns added | 0 | 5-6 |
| Lines moved (not added/deleted) | 0 | ~40-60 |

## 意外发现
- <fill if any — e.g. VERDICT_B on Task 6 added UI-041>
- <fill — e.g. LogEntry lacks Default, had to build test entries manually>

## 出口
- 日期：2026-04-23
- Git HEAD: <hash of Task 8's commit>
- 验收门槛：
  - [x] 5+ pure fns extracted
  - [x] Each fn has ≥ 3 smoke tests
  - [x] cargo test / clippy / fmt green
  - [x] Coverage not regressed
  - [x] Task 6 outcome documented (extracted, or declined → UI-041)

## 移交 Phase 2.5B 事项
- Phase 2.5B characterization tests can target:
  - 5-6 new pure fns from this phase (signatures listed in tasks 1-5)
  - `crate::domain::ws_chat::*` pure fns (no-op extraction, Task 7)
  - `crate::domain::sse_merge::*` pure fns (no-op extraction, Task 7)
  - `crate::domain::filter::*` (80.58% baseline)
  - `crate::domain::network_filter::*` (85.42% baseline — already at
    Phase 2.5B target)
- Still-untestable (Phase 2.5B must fall back to TestBackend snapshot):
  - `event.rs` mouse routing (see Task 6 verdict)
  - `app.rs` MockEdit state machine (UI-026/028 — Phase 3 redesign
    needed before testing)
  - `ui/source_select.rs` (898 lines, 0% — treat as integration
    snapshot target; pure-fn extraction not attempted in 2.5A)
- B-class bug tests for Phase 2.5B (write red, ignored):
  - DOM-003 (HTTP response without request) — domain pure
  - DOM-018 (overlapping OR highlight ranges) — domain pure
  - 10 remaining B entries (DART-*, TRANS-007) — scope-specific
```

Fill placeholders from the commit chain.

- [ ] **Step 8.4: Final phase commit**

No source code changes in this step — just the journal. The source changes committed incrementally in Tasks 1-5 (and possibly Task 6).

```bash
git add docs/superpowers/journal/phase-2.5a.md
git commit -m "$(cat <<'EOF'
docs(journal): Phase 2.5A — logic/render separation complete

Extracted 5-6 pure functions from render code (event.rs, ui/logs/mod.rs,
ui/network/mod.rs) to unlock Phase 2.5B characterization testing.
Coverage delta: <pre>% → <post>%. All green.

See phase-2.5a.md for per-task detail and the Phase 2.5B handoff list.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: Handoff to Phase 2.5B

- [ ] **Step 9.1: Status summary for Phase 2.5B planning**

Print to user:
```
Phase 2.5A complete. Logic/render separation done. 5-6 new pure fns
extracted, each with 3-4 smoke tests. Coverage nudged from 31.48% →
<X>%. All gates green.

Ready to plan Phase 2.5B (characterization tests). Proceeding
automatically per user's "don't interrupt" directive unless a design
decision comes up.
```

Immediately continue to Phase 2.5B planning via writing-plans skill.

---

## Phase 2.5A acceptance checklist

- [ ] Tasks 1-5 completed with commits
- [ ] Task 6 outcome: either extraction committed OR UI-041 audit entry added
- [ ] Task 7 notes committed
- [ ] cargo test all green
- [ ] cargo clippy --all-targets -- -D warnings green
- [ ] cargo fmt --check clean
- [ ] Coverage ≥ baseline (31.48%)
- [ ] docs/superpowers/journal/phase-2.5a.md written and committed

---

## Risks & mitigations

| Risk | Mitigation |
|---|---|
| Subagent "optimizes" the extracted fn (adds safety, renames) | Guardrails 1, 3 explicit. Each task's prompt shows exact expected signature. Reviewer spots deviation in diff. |
| Extracted fn body diverges from inline original (silent behavior drift) | Tests cover the obvious properties (saturation, clamp, zero cases) but not every pixel. Mitigated by: small extractions (each ≤ 20 lines of real logic), in-place call site preserved, visual spot-check at Task 2 review. |
| LogEntry lacks Default → Task 3 tests won't compile | Task 3 prompt warns about this and tells subagent to build test entries with explicit fields. |
| Task 6 extraction hits VERDICT_B/C → we did extra work for nothing | Planned. New audit entry UI-041 captures the finding for Phase 3. Task 6 still has value even when declined. |
| Coverage goes DOWN instead of UP | Indicates extraction deleted real coverage without replacing it. Task 8 compares and investigates before final commit. |

---

## Downstream dependencies

Phase 2.5B consumes:
- The 5-6 new pure fn signatures from Tasks 1-5 (listed in phase-2.5a.md handoff)
- The ws_chat/sse_merge/filter/network_filter pure surfaces from Task 7
- The Task 6 verdict (dictates whether mouse-routing tests use pure fn or TestBackend snapshot)

Do NOT start Phase 2.5B planning until Task 8's journal commit lands on master.
