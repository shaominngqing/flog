# Phase 3 Step 3.10 — Cross-cutting Cleanup

> REQUIRED SUB-SKILL: superpowers:subagent-driven-development or executing-plans.

**Goal:** Split the remaining >500-line files by separating embedded unit tests from production code. Tackle the deferred UI-003 (LogsViewState symmetry with NetworkState). Close Phase 3.

**Current ≥500-line files:**
```
1609  src/transport/device_monitor.rs  (60 tests embedded)
1461  src/app.rs                       (0 embedded tests — pure production)
1089  src/domain/network_store.rs      (48 tests embedded)
 944  src/domain/filter.rs             (59 tests embedded)
 767  src/main.rs                      (15 tests embedded)
 734  src/domain/network.rs            (29 tests embedded)
 656  src/domain/network_filter.rs     (42 tests embedded)
 639  src/input/protocol.rs            (32 tests embedded)
 508  src/parser/generic.rs            (33 tests embedded)
 507  src/session.rs                   (21 tests embedded)
```

**Strategy:** For every file where `#[cfg(test)] mod tests { ... }` is the bulk of the file, move the test module into a sibling `_tests.rs` file via `include!` pattern OR `#[path]` submodule pattern. Production code stays under 500 lines; test code under 500; both visible in git blame. For `app.rs` (no embedded tests), do the UI-003 LogsViewState extraction — that's the real architecture work.

**Red line:** no behavior change. All characterization + in-module tests green at every commit.

## Tasks

### Task 0 — pre-flight
Verify HEAD, `cargo test --all` green, clippy clean, fmt clean. Baseline recorded above.

### Task 1 — Extract UI-003 LogsViewState (the deferred item)
Bundle `app.selected`, `app.scroll_offset`, `app.auto_scroll` (Logs-specific) into a new `LogsViewState` struct mirroring `NetworkState`. Initial step: add the struct with fields + `Default` impl, populate from existing App fields, and provide `pub fn logs(&mut self) -> &mut LogsViewState` accessor. Existing field accessors on `App` become thin delegates (`pub fn selected(&self) -> usize { self.logs.selected }`) OR direct reads throughout the codebase migrate to `app.logs.selected`.

**Approach:** if migrating 190 call sites would exceed ~300 diff lines, do the minimum: add the struct + delegates, leave call sites unchanged. Phase 4 can do mass rename. Target: `src/app.rs` loses ~30-50 lines (the Logs fields → struct).

+3 tests: `LogsViewState::default` initializes to 0/0/true; `app.logs` accessor reads the struct; changing `logs.selected` persists.
Commit: `refactor(app): LogsViewState symmetry with NetworkState (Phase 3 UI-003)`

### Task 2 — Split test modules (batch)
For each file below, move its `#[cfg(test)] mod tests { ... }` block into a sibling file via the `#[path]` pattern. Example for `device_monitor.rs`:

```rust
// In device_monitor.rs, replace the inline tests mod with:
#[cfg(test)]
#[path = "device_monitor_tests.rs"]
mod tests;
```

Create `src/transport/device_monitor_tests.rs` with the test block body (everything inside the old `mod tests { }`). The `use super::*;` line stays at the top of the new file.

Files to process in this order (biggest first):
1. `src/transport/device_monitor.rs` → `device_monitor_tests.rs`
2. `src/domain/network_store.rs` → `network_store_tests.rs`
3. `src/domain/filter.rs` → `filter_tests.rs`
4. `src/domain/network.rs` → `network_tests.rs`
5. `src/main.rs` → `main_tests.rs`  ⚠ main.rs is a binary, verify `#[path]` pattern works; if not, skip this entry — main.rs tests are integration-style
6. `src/domain/network_filter.rs` → `network_filter_tests.rs`
7. `src/input/protocol.rs` → `protocol_tests.rs`
8. `src/parser/generic.rs` → `generic_tests.rs`
9. `src/session.rs` → `session_tests.rs`

Verify after each: production file <500 lines, `cargo test --all` green. One commit per file (9 commits) OR one combined commit if the mechanical sweep is clean (`refactor: split test modules into sibling files to honor 500-line budget (Phase 3 UI-036 mirror)`). Use judgment — combined is fine if no variation per file.

Target: every production `.rs` file < 500 lines after this task.

### Task 3 — Audit residual acks + phase3 index journal
Scan all Phase 3 step journals (`docs/superpowers/journal/phase3-step{1..9}.md`). Check each audit entry in `docs/superpowers/audit/00-index.md` has been resolved or has a documented deferral. Add a `## Residual acks` section to a new `docs/superpowers/journal/phase3-step10.md` capturing:
- Every audit id that Phase 3 closed (with step reference)
- Every audit id explicitly deferred (DART-024/025 → Phase 5; DART-033 → flog_dart v0.8; UI-003 resolved this step)
- Confirm 0 ignored tests in `tests/characterization_bugs.rs`

No code change. Commit: `docs(journal): Phase 3 Step 3.10 + Phase 3 consolidated ack index`

### Task 4 — exit gate
Full `cargo test --all` + `cargo clippy --all-targets -- -D warnings` + `cargo fmt -- --check`. Every production file <500 lines. Confirm Phase 3 is complete. Journal updated. Commit (if not in Task 3): `docs(journal): Phase 3 complete`

## Exit gates
- Every production `src/**/*.rs` file <500 lines (test files under 500 too, but that's secondary)
- All characterization + unit tests green
- 0 ignored tests in `tests/characterization_bugs.rs` (UI-042 stays green from Step 3.8; all B-class Rust bugs resolved)
- Phase 3 journal index documents every audit resolution
- `cargo clippy --all-targets -- -D warnings` clean

## 红线
- No behavior change. Test module extraction is mechanical — same assertions, same fixtures, same helpers.
- No public API changes. LogsViewState is additive; existing App field reads keep working via delegates if migration is deferred.
- `#[path]` pattern should be used for test extraction; do NOT convert the module into a directory just to house tests (that'd bloat the module tree).
- No new deps.
