# Phase 2 — Mechanical Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Pass `cargo clippy --all-targets -- -D warnings` with zero warnings and remove all confirmed dead code, without making a single design judgement. Everything that would require picking a name, a signature, or an abstraction is deferred to Phase 3.

**Architecture:** Four scope-bounded subagents run sequentially (not parallel — see §Merge Strategy), each fixing only its own scope's mechanical issues. After each subagent finishes, main Claude verifies `cargo test` + `cargo clippy -- -D warnings` + `cargo fmt --check` on top of the accumulated diff before merging to master. Phase ends with a single consolidated commit or a small chain of per-scope commits (see §Commit Strategy).

**Tech Stack:** Rust 1.x, clippy, cargo fmt, standard `cargo test`. No new dependencies.

**Parent spec:** `docs/superpowers/specs/2026-04-22-project-cleanup-design.md` §4.1
**Audit source:** `docs/superpowers/audit/` (E-class total: 9 after Phase 1 C-review reclassified DOM-012)

---

## Scope Re-verification (important)

Phase 1 audit flagged several `#[allow(dead_code)]` markers as E-class. **Cross-check during plan writing revealed three misjudgements** — the underlying fields and methods are actually live. The audit finding is still valid but its scope changes: **remove the misplaced `#[allow(dead_code)]` markers**, not the code.

| Audit entry | Originally proposed | After re-verification | Phase 2 action |
|---|---|---|---|
| DOM-009 `NetworkEntry.seq / size / timestamp` | remove fields | **fields are used** (20+ call sites) | remove the incorrect `#[allow(dead_code)]` markers only |
| DOM-010 Mock methods | remove | `find_match`, `hit_count`, `MockRule::next`, `ProtocolFilter::as_str` all used (some only in tests — still live) | remove incorrect markers; delete only `MockRuleStore::enabled_count`, `is_empty`, `MockRule::hit_count` getter if truly unused |
| DOM-023 `Protocol::as_str` | remove | no call site | **remove the method** and the `#[allow(dead_code)]` marker |

Detailed per-method verification is in **Task 3** (Domain scope).

---

## File Structure

No new files. No file moves (TRANS-013 "archive replay.rs to docs/" requires a design decision about archival convention — deferred to Phase 3 or §Out-of-scope).

Files modified by scope:

**Transport scope:**
- `src/transport/usbmuxd.rs` — delete `UsbDevice` struct + `list_devices()` fn (both impl and non-macOS stub)
- `src/transport/adb.rs` — delete `is_available()` fn

**Domain scope:**
- `src/domain/network.rs` — remove 7 incorrect `#[allow(dead_code)]` markers
- `src/domain/network_filter.rs` — delete `MethodFilter::next`, `StatusFilter::next`, `ProtocolFilter::next` if unused (verify in task); remove other incorrect markers
- `src/domain/mock.rs` — delete `MockRuleStore::enabled_count` + `is_empty` if unused; remove the `#[allow(dead_code)]` markers on `find_match` and `hit_count` which are live
- `src/domain/store.rs` — delete `LogStore::append_continuation` (DOM-012) + `LogStore::clear`; add `impl Default`
- `src/domain/entry.rs` — investigate and purge parser `Continuation` variant (DOM-012, coordinated with parser subagent)
- `src/domain/filter.rs` — `manual_pattern_char_comparison` fix at line 239
- `src/domain/structured_parser.rs` — fix `approx_constant` error (line 465), `items_after_test_module` if any
- `src/domain/network_store.rs` — add `impl Default` for `NetworkStore`
- `src/domain/network_filter.rs` — add `impl Default` for `NetworkFilter`

**Parser scope** (part of domain merge):
- `src/parser/*.rs` — remove `Continuation` variant references if present (coordinated with DOM-012 removal)

**UI + event + app scope:**
- `src/app.rs` — `empty_line_after_doc_comments` fix at line 355; add `impl Default for NetworkState`; add `impl Default for App`
- `src/ui/json_viewer/state.rs` — delete `expand_all` + `collapse_all` (UI-019) — `event.rs` will keep using its manual loop. Optionally unify them in Phase 3, not Phase 2
- `src/ui/logs/mod.rs` — `unnecessary_cast` fix at line 186
- `src/ui/logs/detail/renderers.rs` — `manual_strip` fix at line 176-181; `useless_conversion` fix at line 260
- `src/ui/network/mod.rs` — `items_after_test_module` fix (move `draw_network_status_bar` before `mod tests`)
- `src/ui/network/filter.rs` — `unnecessary_cast` fix at line 44
- `src/ui/source_select.rs` — 4× `vec_init_then_push` fixes (lines 387, 454, 489, 529)

**flog_dart scope:**
- `flog_dart/lib/flog_dart.dart` — fix DART-028 (duplicate dartdoc example) but **only after DART-003 is fixed in Phase 3**; since DART-003 is a D/A-class issue (not Phase 2), **DART-028 is deferred to Phase 3** as well

---

## Out-of-scope (deferred to Phase 3)

These clippy warnings require design decisions, so they do NOT belong in Phase 2:

| Warning | Location | Why deferred |
|---|---|---|
| `large_enum_variant` ClientMessage | `src/input/protocol.rs:24` | Boxing `FlogNetMessage` changes the memory layout of a WS protocol variant. Decision must weigh cache locality vs heap allocation per message. Design judgement → Phase 3 (Transport step). |
| `too_many_arguments` `render_json_section_with_depth` | `src/ui/network/detail.rs:991` | Requires designing a new parameter struct or extracting state. → Phase 3 (UI Network step). |
| `too_many_arguments` `push_device_top` | `src/ui/source_select.rs:351` | Same → Phase 3 (UI shared components step). |
| `should_implement_trait` `LogLevel::from_str` | `src/domain/entry.rs:26` | Implementing `std::str::FromStr` changes the API (caller style `LogLevel::from_str(s)` stays but the return type changes from `Option<Self>` to `Result<Self, _>`). → Phase 3 (Domain step). |
| TRANS-013 archive `replay.rs` | `src/replay.rs` | Moving to `docs/archived/` is an archival convention choice. → Phase 3 (Transport step) or keep as-is permanently. |

These warnings stay as warnings in Phase 2. Phase 2's clippy-green bar is achieved by `#[allow(clippy::SPECIFIC_LINT)]` with a comment `// Phase 3 redesign — see Audit <ID>` on each of the above call sites, so overall `-D warnings` still passes but the lint is locally suppressed **with a tracking reference**.

---

## Commit Strategy

Spec §4.1 says "merge as 1 commit" by main Claude after all 4 subagents finish. This plan follows that: **one final commit for Phase 2**.

Internally, during execution, each subagent produces one logical diff that main Claude reviews. Between subagent merges, the tree is kept clean: subagent A's changes are merged (uncommitted, kept as working-tree changes), then subagent B works against the updated tree, and so on. Only after all 4 scopes pass unified `cargo test` + `clippy -- -D warnings` + `fmt --check` is the single Phase 2 commit made.

Merge order per spec §4.1: **transport → domain → flog_dart → ui** (ui last because it's the largest and the most likely to conflict with the other scopes).

---

## Pre-flight (Task 0)

### Task 0: Pre-flight verification

**Files:** (read-only)

- [ ] **Step 0.1: Confirm we're on the expected Phase 1 tip**

Run: `git log --oneline -1`
Expected: `a243f76 chore(audit): Phase 1 — audit reports (4 subagents, A/B/C/D/E classified)` (or later if any hotfix commits landed).

- [ ] **Step 0.2: Confirm tests green and record the pre-Phase-2 clippy baseline**

Run:
```bash
cargo test 2>&1 | tail -5
cargo clippy --all-targets 2>&1 | grep -cE "^(error|warning):" > /tmp/phase2-baseline-clippy-count.txt
cat /tmp/phase2-baseline-clippy-count.txt
```

Expected `cargo test`:
```
test result: ok. 217 passed; 0 failed; 0 ignored; 0 measured; ...
test result: ok. 1 passed; 0 failed; ...
```

Record `/tmp/phase2-baseline-clippy-count.txt` content — it will be compared at Phase 2 exit (expected to drop to 0 or to only the deferred Phase-3 suppressions).

- [ ] **Step 0.3: Confirm no staged changes**

Run: `git status --short`
Expected: only `?? flog_062120.log` untracked (a runtime log file — not part of this phase).

---

## Task 1: Transport scope mechanical cleanup

**Scope:** `src/transport/`, `src/input/connector.rs`, `src/input/protocol.rs` (read only for protocol variant decisions — no change)

**Audit entries addressed:** TRANS-001 (UsbDevice + list_devices), `is_available` warning (not in audit but clearly dead).
**Deferred:** TRANS-013 (archive replay.rs — decision), large_enum_variant on ClientMessage (design).

### Task 1.1: Dispatch Transport subagent

- [ ] **Step 1.1.1: Dispatch a single subagent (model: haiku or sonnet acceptable; mechanical work)**

Use the Agent tool with `subagent_type: "general-purpose"`.

Prompt (verbatim):

```
You are implementing Phase 2 (Mechanical cleanup) of the flog cleanup plan.
Scope: Transport layer only. You MUST NOT touch any other subsystem.

Parent spec: docs/superpowers/specs/2026-04-22-project-cleanup-design.md §4.1
Plan: docs/superpowers/plans/2026-04-22-phase2-mechanical-cleanup.md (Task 1)
Audit: docs/superpowers/audit/01-transport.md

Absolute rules:
1. Zero design judgement. If you start thinking "should I rename this" or
   "should I change this signature" — STOP. That work belongs to Phase 3.
2. Do not alter public APIs of any surviving code (removing truly dead items
   is allowed; changing signatures is NOT).
3. `cargo test` must remain 217 unit + 1 integration green at every commit.
4. `cargo fmt --check` must pass.
5. Do NOT commit yet — produce a clean working tree diff and report back.

Concrete actions:

1. src/transport/usbmuxd.rs — Remove the `UsbDevice` struct and the
   `list_devices()` function from BOTH the macOS impl (around lines 12-61)
   AND the non-macOS stub (around lines 225-233). Keep `connect_device()`
   and `query_device_name()` which are used. Clippy warnings:
     - "struct `UsbDevice` is never constructed"
     - "function `list_devices` is never used"

2. src/transport/adb.rs — Remove `pub async fn is_available()` at ~line 51.
   It is never called. Verify with: `grep -rn is_available src/` — result
   must show only adb.rs before the deletion, zero after.

After both deletions, run:
- `cargo build` — must succeed
- `cargo test` — must pass (217 unit + 1 integration, 0 failed)
- `cargo clippy --all-targets 2>&1 | grep -cE "^(warning|error):"` — record
  the count; it should DROP by at least 3 (UsbDevice + list_devices + is_available).
  Report the new count.

Do NOT stage or commit — leave working-tree changes. Report:
- Files changed (use `git diff --stat`)
- Clippy count before / after
- `cargo test` final line
- Any cases where you chose NOT to make a deletion, with reason

If the audit flagged something that is actually used (cross-reference any
reference via grep), do NOT delete it — report the discrepancy instead.
```

- [ ] **Step 1.1.2: Review Transport subagent's diff**

Run:
```bash
git diff --stat src/transport/
cargo test 2>&1 | tail -5
cargo clippy --all-targets 2>&1 | grep -cE "^(warning|error):"
cargo fmt --check
```

Expected: files changed = `usbmuxd.rs`, `adb.rs`; tests pass; clippy count drops by 3+; fmt clean.

If clippy count did not drop or tests fail: investigate and re-dispatch with specific fix instructions.

---

## Task 2: Domain + Parser scope mechanical cleanup

**Scope:** `src/domain/`, `src/parser/`, `src/session.rs` (read-only).

**Audit entries addressed:** DOM-009, DOM-010 (partial), DOM-012, DOM-023, also clippy warnings:
- `new_without_default` on LogStore, NetworkFilter, NetworkStore
- `manual_pattern_char_comparison` in filter.rs:239
- `approx_constant` error in structured_parser.rs:465 (the test compile blocker)
- `method 'enabled_count' is never used` in mock.rs:127
- `method 'clear' is never used` in store.rs:72

**Deferred (add `#[allow(clippy::LINT)]` with tracking comment):**
- `should_implement_trait` `LogLevel::from_str` → Phase 3 (Domain step, Audit followup)

### Task 2.1: Dispatch Domain subagent

- [ ] **Step 2.1.1: Dispatch Domain subagent**

Use `subagent_type: "general-purpose"`. Prompt:

```
You are implementing Phase 2 (Mechanical cleanup) of the flog cleanup plan.
Scope: Domain layer + Parser + session.rs only. You MUST NOT touch
transport, UI, event, app, or flog_dart.

Parent spec: docs/superpowers/specs/2026-04-22-project-cleanup-design.md §4.1
Plan: docs/superpowers/plans/2026-04-22-phase2-mechanical-cleanup.md (Task 2)
Audit: docs/superpowers/audit/02-domain.md

Absolute rules (same as Task 1):
1. Zero design judgement.
2. Do not alter public APIs of surviving code.
3. `cargo test` must remain green at every point.
4. `cargo fmt --check` must pass.
5. Do NOT commit — produce a clean working tree diff.

You start from the tree AFTER Task 1 (transport) subagent's changes are
applied. That means UsbDevice, list_devices, and is_available are already
gone. Your changes stack on top.

Concrete actions (in order):

===== A. Fix the cargo-test compile blocker =====

src/domain/structured_parser.rs:465 — the test uses literal 3.14:

  assert!((v["d"].as_f64().unwrap() - 3.14).abs() < 1e-9);

clippy rejects this as "approximate value of PI". Mechanical fix: change
the number used in the test to something that is NOT a PI approximation.
The simplest fix is to change 3.14 to 3.25 (arbitrary, not a known constant)
and update both the JSON literal and the assertion. Alternately, use
`std::f64::consts::PI` if the intent was exact PI — but re-reading the
context (tolerant parser JSON test), the value is arbitrary, so use 3.25.

After edit: `cargo test` must compile and 217 unit + 1 integration pass.

===== B. Remove incorrect `#[allow(dead_code)]` markers from live code =====

src/domain/network.rs — The audit (DOM-009, DOM-023) claimed these fields
are dead. Cross-check confirmed they are LIVE:
  - SseChunk.seq (used in network_store.rs:142)
  - SseChunk.size (used in network_store.rs:144)
  - SseChunk.timestamp (used in multiple render sites)
  - WsMessage.size (used in event.rs:1093, detail.rs:642, 790)
  - WsMessage.timestamp (used in detail.rs render)

Remove the `#[allow(dead_code)]` attributes at lines 13, 40, 47, 50, 59, 67
(verify current line numbers with Read tool first — line numbers may have
shifted). Keep the fields themselves.

HOWEVER: `Protocol::as_str(&self) -> &'static str` at lines 14-22-ish is
actually unused (grep src/ for `Protocol::as_str` → no matches; grep for
`.as_str()` on Protocol enum values also → no matches). Remove BOTH the
method AND its `#[allow(dead_code)]` marker.

===== C. Remove confirmed dead code =====

src/domain/mock.rs:
  - Remove `pub fn enabled_count(&self)` at line ~127. Grep confirms it's
    only used within mock.rs's own tests. The tests also call
    `store.enabled_count()` — those test calls become orphaned and must
    be removed too, OR the method can be kept as `#[cfg(test)]`.
    DECISION (mechanical, no judgement): keep the method but scope it to
    `#[cfg(test)]` since the tests are live.
  - Apply the same treatment to `MockRuleStore::is_empty()` at line ~117
    if the only callers are within tests. Grep first.
  - The `#[allow(dead_code)]` markers at lines 23, 73, 116: read each and
    determine if the attribute is masking a true dead-code warning. If
    the item is actually used, remove the marker. If actually dead, the
    audit reclassified it — consult DOM-010 and follow.

src/domain/store.rs:
  - Remove `LogStore::clear()` at line ~72. Grep src/ for `.clear()` calls
    on a LogStore — zero matches found in the plan-writing cross-check.
  - DOM-012 resolution: remove `LogStore::append_continuation()` at
    lines 49-54.

src/parser/ — Related to DOM-012: remove the `Continuation` variant from
the parser output enum (whichever file defines the variant — use Grep to
locate: `grep -rn "Continuation" src/parser/ src/domain/`). Check the
match arms that produce/consume it and remove them too. The parser uses
`extra_lines` on LogEntry directly during parse, so no functional loss.

===== D. Mechanical clippy fixes =====

src/domain/filter.rs:239 — replace the closure with an array pattern:

Before:
  for part in input.split(|c: char| c == ',' || c == '|') {

After:
  for part in input.split([',', '|']) {

src/domain/network_filter.rs and src/domain/network_filter.rs:
  - `MethodFilter::next`, `StatusFilter::next`, `ProtocolFilter::next` —
    grep src/event.rs for these: if any are used, keep them and remove
    the `#[allow(dead_code)]`. If any are unused (grep shows zero hits
    outside the definition), delete them.

===== E. Add `impl Default` where clippy asks =====

Per clippy `new_without_default`:

src/domain/store.rs — after the `impl LogStore` block, add:
  impl Default for LogStore {
      fn default() -> Self { Self::new() }
  }

src/domain/network_store.rs — after `impl NetworkStore`:
  impl Default for NetworkStore {
      fn default() -> Self { Self::new() }
  }

src/domain/network_filter.rs — after `impl NetworkFilter`:
  impl Default for NetworkFilter {
      fn default() -> Self { Self::new() }
  }

===== F. Defer design-judgement warnings =====

src/domain/entry.rs:26 — `should_implement_trait` on `LogLevel::from_str`.
DO NOT implement `std::str::FromStr`. Instead, add local allow with
tracking comment directly above the function:

  // Phase 3 redesign — see Audit DOM (entry.rs): implement std::str::FromStr.
  #[allow(clippy::should_implement_trait)]
  pub fn from_str(s: &str) -> Option<Self> { ... }

This keeps -D warnings green while recording the deferred work.

===== Final verification =====

After all edits:
- `cargo build` — must succeed
- `cargo test` — must pass (217+1 green, possibly minus any test you had
  to remove together with the dead code; report the new test count)
- `cargo clippy --all-targets -- -D warnings` — should now pass for the
  DOMAIN scope of warnings; unrelated warnings (UI, transport already-done,
  protocol.rs large_enum_variant, etc.) may remain
- `cargo fmt --check` — must pass

Do NOT stage or commit. Report:
- `git diff --stat src/domain/ src/parser/ src/session.rs`
- cargo test final line + any test count delta
- clippy count before/after
- Any audit entry you did NOT address and why
```

- [ ] **Step 2.1.2: Review Domain subagent's diff**

Run:
```bash
git diff --stat src/domain/ src/parser/
cargo test 2>&1 | tail -5
cargo clippy --all-targets 2>&1 | grep -cE "^(warning|error):"
cargo clippy --all-targets -- -D warnings 2>&1 | tail -20
cargo fmt --check
```

Expected:
- `src/domain/` + `src/parser/` changes only
- Tests green (count may drop slightly if tests for removed methods were deleted; note delta)
- Clippy count drops significantly (should be `approx_constant` gone, `new_without_default` ×3 gone, `manual_pattern_char_comparison` gone, plus ≥5 dead-code warnings gone)
- `cargo fmt --check` clean

If clippy still flags domain-scope warnings: dispatch a fix subagent with specific instructions referencing each remaining warning.

---

## Task 3: flog_dart scope mechanical cleanup

**Scope:** `flog_dart/lib/`.

**Audit entries addressed in Phase 2:** none.
- DART-028 (duplicate dartdoc example) depends on DART-003 (D/A-class) which is Phase 3 territory — deferred.
- DART-029 (`_nextId` visibleForTesting reset) depends on DART-021 (un-exporting `nextNetId`) which is Phase 3 — deferred.

### Task 3.1: Verify nothing to do in Phase 2

- [ ] **Step 3.1.1: Verify no Phase-2-eligible flog_dart changes**

Run:
```bash
cd flog_dart && dart analyze 2>&1 | tail -10 && cd ..
```

Expected: any issues flagged by `dart analyze` are in deferred audit entries (D or B class, not E). If `dart analyze` flags a pure stylistic issue with a clearly-mechanical fix (e.g., unused import), address it and note it here.

- [ ] **Step 3.1.2: Record decision**

Write to `docs/superpowers/journal/phase-2-notes.md` (create if absent):

```markdown
### flog_dart scope

All two E-class entries (DART-028, DART-029) depend on upstream
D/A/B-class work that belongs to Phase 3. They remain marked E in the
audit but execute in Phase 3 as sub-steps of their dependencies.
`dart analyze` output (if any non-deferred issues) addressed in
Phase 2: <list any fixes here, or "none">.
```

If there were no fixes, skip to Task 4.

If there WERE fixes, commit them in the final Phase 2 commit along with Rust changes — do not stage yet.

---

## Task 4: UI + event + app scope mechanical cleanup

**Scope:** `src/ui/`, `src/app.rs`, `src/event.rs`, `src/cli.rs`.

**Audit entries addressed:** UI-019 (expand_all/collapse_all), plus clippy:
- `empty_line_after_doc_comments` on app.rs:355
- `new_without_default` on App, NetworkState
- `unnecessary_cast` on logs/mod.rs:186 and network/filter.rs:44
- `manual_strip` on logs/detail/renderers.rs:176
- `useless_conversion` on logs/detail/renderers.rs:260
- `items_after_test_module` on network/mod.rs:477
- 4× `vec_init_then_push` on source_select.rs

**Deferred to Phase 3:**
- `too_many_arguments` on network/detail.rs:991 → Phase 3 UI Network step
- `too_many_arguments` on source_select.rs:351 → Phase 3 UI shared step

### Task 4.1: Dispatch UI subagent

- [ ] **Step 4.1.1: Dispatch UI subagent**

Use `subagent_type: "general-purpose"`. Prompt:

```
You are implementing Phase 2 (Mechanical cleanup) of the flog cleanup plan.
Scope: src/ui/, src/app.rs, src/event.rs, src/cli.rs. This is the largest
scope. You MUST NOT touch src/domain/, src/parser/, src/transport/,
src/input/, or flog_dart/.

Parent spec: docs/superpowers/specs/2026-04-22-project-cleanup-design.md §4.1
Plan: docs/superpowers/plans/2026-04-22-phase2-mechanical-cleanup.md (Task 4)
Audit: docs/superpowers/audit/03-ui.md

Absolute rules:
1. Zero design judgement. If you start thinking about names or signatures
   — STOP and report instead.
2. Do not alter public APIs of any surviving code.
3. `cargo test` must remain green at every point.
4. `cargo fmt --check` must pass.
5. Do NOT commit — leave working-tree changes and report.

You start from the tree AFTER Tasks 1+2+3 are applied (transport, domain,
any flog_dart fixes already in working tree).

===== A. Mechanical clippy equivalence rewrites =====

src/ui/logs/mod.rs:186 — `unnecessary_cast`:
  Before: let w = area.width as u16;
  After:  let w = area.width;

src/ui/network/filter.rs:44 — same pattern, same fix.

src/ui/logs/detail/renderers.rs:176-181 — `manual_strip`:
  Before:
    if trimmed.starts_with('#') {
        let indent_len = line.len() - trimmed.len();
        // ...
        let hash_digits_end = 1 + trimmed[1..].find(|c: char| ...);
    }

  After:
    if let Some(stripped) = trimmed.strip_prefix('#') {
        let indent_len = line.len() - trimmed.len();
        // ...
        let hash_digits_end = 1 + stripped.find(|c: char| ...);
    }

Verify the rest of the block references `stripped` instead of `trimmed[1..]`.

src/ui/logs/detail/renderers.rs:260 — `useless_conversion`:
  Before: .zip(click_map.into_iter())
  After:  .zip(click_map)

src/ui/network/mod.rs:477 — `items_after_test_module`:
  The function `draw_network_status_bar` appears AFTER `mod tests { .. }`.
  Use Read to inspect lines 460-500; cut the function body and paste it
  BEFORE the `mod tests {` line.

src/ui/source_select.rs:387, 454, 489, 529 — four `vec_init_then_push`:
  Each pattern is:
    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(a);
    spans.push(b);
    ...
    spans.push(N);

  Rewrite as:
    let mut spans: Vec<Span<'static>> = vec![
        a,
        b,
        ...
        N,
    ];

  NOTE: if there is any non-push statement between `Vec::new()` and the
  last push (e.g., a conditional that pushes only if some flag is true),
  KEEP Vec::new and `mut` and only collapse the contiguous push runs
  into vec![]. Read each site carefully before transforming. If in doubt,
  leave the smaller transformation (don't over-collapse).

===== B. Add `impl Default` =====

src/app.rs — after `impl NetworkState { pub fn new() ... }`:
  impl Default for NetworkState {
      fn default() -> Self { Self::new() }
  }

src/app.rs — after `impl App { pub fn new() ... }`:
  impl Default for App {
      fn default() -> Self { Self::new() }
  }

===== C. Fix empty_line_after_doc_comments =====

src/app.rs:355 — there is a blank line between `/// Per-app data...` and
the `pub struct ConnectedApp`. Either delete the blank line or add `///`
to it. Delete the blank line (simpler, standard).

===== D. Delete UI-019 expand_all / collapse_all =====

src/ui/json_viewer/state.rs at lines ~51 and ~68:
  - Remove `pub fn expand_all(tree: &Tree, state: &mut JsonViewerState)`
  - Remove `pub fn collapse_all(tree: &Tree, state: &mut JsonViewerState)`
  - Remove their `#[allow(dead_code)]` markers
  - Verify with grep: `grep -rn "expand_all\|collapse_all" src/`
    expected: zero matches outside any tests that reference them (if tests
    exist, remove them too or keep them with the functions — DECIDE:
    no tests, just remove).

Event.rs currently duplicates this logic manually at lines ~1450-1467
for E/K keys. LEAVE EVENT.RS ALONE — unifying it with expand_all would
require picking the right pub API, which is a design decision → Phase 3.
Just remove the dead expand_all/collapse_all in state.rs.

===== E. Defer too_many_arguments (add local allow) =====

src/ui/network/detail.rs:991 — `render_json_section_with_depth` takes 8
args. Add directly above the fn:

  // Phase 3 redesign — see Audit UI-037: extract parameter struct.
  #[allow(clippy::too_many_arguments)]
  fn render_json_section_with_depth(...)

src/ui/source_select.rs:351 — `push_device_top` takes 8 args. Same:

  // Phase 3 redesign — see Audit UI-015/UI-014: extract parameter struct.
  #[allow(clippy::too_many_arguments)]
  fn push_device_top(...)

===== Final verification =====

After all edits:
- `cargo build` — succeed
- `cargo test` — 217 unit + 1 integration (or updated count if earlier
  tasks removed tests) green
- `cargo clippy --all-targets -- -D warnings` — MUST pass now
  (this is the Phase 2 exit gate). If it fails, report the remaining
  warnings — they are either bugs in this task's edits or they hint at
  Phase-3-level design issues that slipped through.
- `cargo fmt --check` — pass

Do NOT commit. Report:
- `git diff --stat` on the UI/app/event/cli paths
- cargo test final
- `cargo clippy --all-targets -- -D warnings 2>&1 | tail -20` output
- `cargo fmt --check` status
- Any surprise encountered
```

- [ ] **Step 4.1.2: Review UI subagent's diff**

Run:
```bash
git diff --stat src/ui/ src/app.rs src/event.rs src/cli.rs
cargo test 2>&1 | tail -5
cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
cargo fmt --check
```

Expected:
- Changes limited to UI/app/event/cli paths (no domain or transport drift)
- Tests green
- **`cargo clippy --all-targets -- -D warnings` now passes** — this is the Phase 2 exit gate
- fmt clean

If clippy still complains: either the subagent missed a fix or there is an unexpected warning. Read the clippy output, dispatch a targeted fix subagent if needed, re-verify.

---

## Task 5: Consolidated verification + commit

- [ ] **Step 5.1: Full tree verification**

Run:
```bash
echo "=== cargo build ===" && cargo build 2>&1 | tail -3
echo "=== cargo test ===" && cargo test 2>&1 | tail -10
echo "=== cargo clippy -D warnings ===" && cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
echo "=== cargo fmt --check ===" && cargo fmt --check && echo "fmt clean"
echo "=== git diff stat ===" && git diff --stat
echo "=== deferred allows ===" && grep -rn "Phase 3 redesign — see Audit" src/ | head -20
```

Expected:
- build: Finished
- tests: 217 unit + 1 integration green (minus any tests deleted together with removed dead code; report the delta)
- clippy: **zero warnings, zero errors** under `-D warnings`
- fmt: clean
- git diff stat: changes spread across transport, domain, UI, app; no flog_dart unless Task 3 found something
- deferred allows: 3-5 lines showing each `#[allow(clippy::...)]` with its "Phase 3 redesign — see Audit" tracking comment

- [ ] **Step 5.2: Update phase-2-notes.md**

Use `Write` or `Edit` on `docs/superpowers/journal/phase-2-notes.md` to add:

```markdown
## Phase 2 delta summary

### Clippy
- Before: <N> warnings + 1 error (from Phase 1 baseline)
- After:  0 warnings, 0 errors with -D warnings

### Tests
- Before: 217 unit + 1 integration
- After:  <M> unit + 1 integration
- Delta reason: <list any tests removed together with dead code>

### Files modified (by scope)
- Transport: src/transport/usbmuxd.rs, src/transport/adb.rs
- Domain: <list from git diff>
- UI: <list from git diff>
- flog_dart: <list or "none">

### Deferred to Phase 3 (tracked via #[allow] + comment)
- <each deferred warning with its tracking comment location>
```

- [ ] **Step 5.3: Write Phase 2 journal**

Create `docs/superpowers/journal/phase-2.md`:

```markdown
# Phase 2 Journal — Mechanical Cleanup

## 入口
- 日期: 2026-04-22
- Git HEAD at entry: a243f76
- 执行者: 主 Claude + 4 scope subagents (sequential, not parallel)
- 执行模式: Inline + 停机点

## 时间线
- <HH:MM> Task 0 pre-flight: baseline clippy count recorded
- <HH:MM> Task 1 transport subagent: <N> warnings removed
- <HH:MM> Task 2 domain+parser subagent: <N> warnings removed
- <HH:MM> Task 3 flog_dart: <action taken or "no-op, all deferred">
- <HH:MM> Task 4 UI+event+app subagent: <N> warnings removed
- <HH:MM> Task 5 final verification + commit

## 意外发现
- <e.g. Audit misjudgements on DOM-009/010/023 that required
  scope re-verification. Only three were truly dead>
- <e.g. tests removed together with dead code: list each>
- <any other surprises>

## 出口
- 日期: 2026-04-22
- Git HEAD at exit: <commit hash, fill after Step 5.4>
- 验收门槛 (spec §4.1):
  - [x] cargo clippy --all-targets -- -D warnings passes (zero)
  - [x] cargo test all green
  - [x] cargo fmt --check clean
  - [x] 1 consolidated commit

## 移交 Phase 2.5 事项
- Phase 2.5 不再需要修任何 clippy warning — baseline 已 clean
- Deferred items (Phase 3 redesign tracking):
  - <list each #[allow(clippy::...)] location + audit id>
- Baseline test count moving into 2.5: <M> unit + 1 integration
- flog_dart/test/flog_sse_parser_test.dart 仍是 red (expected, DART-001/002 Phase 3 解决)
```

Fill in all the `<...>` placeholders with real numbers from Step 5.1's output.

- [ ] **Step 5.4: Commit Phase 2**

```bash
git add src/ flog_dart/ docs/superpowers/journal/phase-2.md docs/superpowers/journal/phase-2-notes.md
git status --short
```

Verify only these changes are staged; nothing from `docs/superpowers/audit/` or `docs/superpowers/specs/` or other journal files.

```bash
git commit -m "$(cat <<'EOF'
refactor: Phase 2 — mechanical cleanup, clippy zero-warning

Fixes every clippy warning under `cargo clippy --all-targets -- -D warnings`
by either (a) applying clippy's own machine-equivalent suggestion,
(b) deleting confirmed dead code, (c) adding `impl Default` where new()
is parameterless, or (d) deferring design-judgement warnings with a
local #[allow] plus tracking comment ("Phase 3 redesign — see Audit <ID>").

Zero design choices made. No API signatures changed. Behavior unchanged
by construction.

Dead code removed:
- Transport: UsbDevice struct, list_devices(), is_available()
- Domain: LogStore::{append_continuation, clear}, Protocol::as_str,
  parser Continuation variant (DOM-012, user-approved in Phase 1)
- Mock: enabled_count/is_empty scoped to #[cfg(test)] where used only
  by tests
- UI: json_viewer state.rs expand_all / collapse_all (UI-019)

Deferred to Phase 3 (tracked):
- ClientMessage large_enum_variant (Box<FlogNetMessage>) — protocol layout decision
- render_json_section_with_depth / push_device_top too_many_arguments — parameter struct design
- LogLevel::from_str should_implement_trait — FromStr API decision
- DART-028 / DART-029 — depend on Phase 3 DART-003 / DART-021

Spec: docs/superpowers/specs/2026-04-22-project-cleanup-design.md §4.1
Plan: docs/superpowers/plans/2026-04-22-phase2-mechanical-cleanup.md

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
git log --oneline -4
```

Expected: new commit on top of `a243f76`.

- [ ] **Step 5.5: Post-commit final sanity**

```bash
cargo clippy --all-targets -- -D warnings 2>&1 | tail -3
cargo test 2>&1 | grep "test result:"
```

Expected: clippy clean, tests green.

---

## Task 6: Handoff to Phase 2.5

- [ ] **Step 6.1: Send user a phase-exit message**

```
Phase 2 complete. cargo clippy --all-targets -- -D warnings passes with
zero. cargo test all green (217+1, or delta from dead-code removal
reported in journal).

Commit: <hash from Step 5.4>

🛑 Next stop (per spec §9): coverage tool selection before Phase 2.5.
Choice is cargo-llvm-cov vs cargo-tarpaulin. Neither is installed yet.
Recommend llvm-cov (faster, more accurate on modern Rust). Waiting for
your decision before dispatching writing-plans for Phase 2.5.
```

- [ ] **Step 6.2: Stop**

Do NOT auto-dispatch Phase 2.5. The next decision (coverage tool choice)
requires user input per spec §9.

---

## Phase 2 acceptance checklist (spec §4.1)

- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo test` green (count ≥ Phase 1 baseline minus any dead-code-paired tests)
- [ ] `cargo fmt --check` clean
- [ ] 1 consolidated commit on master
- [ ] `docs/superpowers/journal/phase-2.md` written
- [ ] All deferred warnings have `#[allow(clippy::...)]` + "Phase 3 redesign — see Audit <ID>" tracking comment

---

## Risks & mitigations

| Risk | Mitigation |
|---|---|
| Subagent deletes live code mistaking it for dead | Each scope subagent re-grep-verifies before deleting. The plan pre-identifies 3 known misjudgements (DOM-009/010/023) so subagent Task 2 knows to be careful. |
| Sequential execution takes longer than parallel | Parallel was rejected to avoid style-inconsistent cross-scope edits. If duration becomes unacceptable, we can risk parallel with worktrees in future phases — not this one. |
| Subagent makes a design choice disguised as mechanical | Prompts explicitly forbid it. Reviewer (main Claude) checks diff for unexpected renames or signature changes before merging. |
| Tests removed together with dead code cause count regression | Expected and tracked in phase-2.md journal delta. Counts do not need to stay exactly 217; they need to reflect the removed-code set. |
| `cargo fmt` applies style changes outside scope | Phase 2 fmt only touches files the subagent modified. Running `cargo fmt --check` before commit detects any accidental fmt drift. |

---

## Downstream dependencies

Phase 2.5 (Characterization tests) reads from:
- The zero-warning state achieved here
- `docs/superpowers/journal/phase-2.md` delta summary
- `docs/superpowers/audit/00-index.md` B-class list (red tests for Phase 3 to fix)

Do NOT start Phase 2.5 planning until this plan's Task 5 commit is on master and user has chosen the coverage tool (spec §9 open item).
