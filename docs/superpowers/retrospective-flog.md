# flog cleanup campaign — retrospective

**Campaign window:** 2026-04-22 → 2026-04-24 (three calendar days, six
phases, one working branch `master`).
**Repo:** https://github.com/shaomingqing/flog — a terminal-native log
viewer + network inspector for Flutter developers (Rust TUI) plus a
Dart companion package `flog_dart` published to pub.dev.
**Ground rules:** every phase produced a single exit-journal commit;
every claim below is traceable to a file under
`docs/superpowers/{audit,plans,journal}/` or to `git log`.

This document is the flog-specific retrospective. The generalised
"how the pattern worked" lives alongside in
`docs/superpowers/ai-long-workflow-methodology.md`.

## 1. Starting state (2026-04-22, HEAD `5888264`)

Snapshot recorded in `docs/superpowers/audit/.baseline.md` before
Phase 1 audit ran.

| Metric                                 | Value at entry |
|----------------------------------------|----------------|
| `git log` total (entire project life)  | 262 commits    |
| `cargo test` (lib + integration)       | 217 unit + 1 integration (218 green) |
| `cargo clippy --all-targets -D warn`   | **fails** — 1 error + ~18 warnings |
| `cargo fmt --check`                    | clean          |
| Rust LOC under `src/**/*.rs`           | 18 469         |
| Dart LOC under `flog_dart/lib/**`      | 1 834          |
| Files > 800 lines (red)                | **5** — `event.rs` 1677, `ui/logs/mod.rs` 1358, `app.rs` 1167, `ui/network/detail.rs` 1109, `ui/source_select.rs` 898 |
| Files 500–800 (yellow)                 | **6** — `ui/json_viewer/render.rs`, `ui/network/mod.rs`, `domain/structured_parser.rs`, `transport/device_monitor.rs`, `main.rs`, `flog_dart/lib/src/flog_dio.dart` |
| Total files > 500 lines                | **11**         |
| Engineering docs (`docs/*.md`)         | 0 — only root README, README_EN, CLAUDE.md, and the two flog_dart docs existed |
| Known bugs in audit                    | unknown — no audit had been run |

The test count is load-bearing for the "before": 217 tests for
~18 kLOC of Rust and ~1.8 kLOC of Dart is ~30% coverage by the tooling
we ran later (`cargo llvm-cov` measured the baseline at 31.48% line /
31.30% region after Phase 2.5B's Task 0 recorded it).

## 2. Ending state (2026-04-24, HEAD = Phase 6 journal commit)

| Metric                                 | Value at exit  | Δ vs entry     |
|----------------------------------------|----------------|----------------|
| Campaign commits                       | 159 + this doc's 3 = **162** | n/a (see §10) |
| `cargo test --all`                     | **2 166 green, 0 failed, 0 ignored** (12 test binaries) | +1 948 |
| `flutter test` (flog_dart)             | **133 pass, 2 skip, 0 fail** | from 84 pass / 3 skip / 1 fail (DART-001/002 compile-time red) |
| `cargo clippy --all-targets -D warn`   | clean          | fixed          |
| `cargo fmt --check`                    | clean          | unchanged      |
| Rust LOC under `src/**/*.rs`           | 28 050         | +9 581 (mostly sibling-file `_tests.rs`, plus split boilerplate) |
| Dart LOC under `flog_dart/lib/**`      | 2 233          | +399           |
| **Files > 500 lines (production)**     | **0**          | −11            |
| Files > 500 lines including `_tests.rs`| 3              | sibling-file test modules are exempt from the budget by design (§CONTRIBUTING) |
| Line coverage (`cargo llvm-cov`)       | **90.54%**     | +59.06pp       |
| Region coverage                        | 89.75%         | +58.45pp       |
| Function coverage                      | 93.18%         | +43.69pp       |
| Engineering docs (`docs/*.md`)         | 4              | +4 — `ARCHITECTURE.md`, `MODULES.md`, `PROTOCOL.md`, `CONTRIBUTING.md` |
| User docs refreshed                    | 5              | `README.md`, `README_EN.md`, `CLAUDE.md`, `flog_dart/README.md`, `flog_dart/CHANGELOG.md` |
| Audit paper trail                      | 4 scope files + 1 index + 19 journals + 32 plans + 15 specs | new |

Coverage measurement was recorded per module — every hot module clears
its Rule 2 gate except four PHYS/D-ref-marked gaps (`main.rs` 45.70%,
`event.rs` 61.02%, `transport/adb.rs` 49.18%, `transport/usbmuxd.rs`
73.02%; `replay.rs` 2.43%, archived as D-ref TRANS-013). See
`docs/superpowers/journal/phase-2.5b.md` for the full per-module
table.

## 3. Six phases at a glance

Commits per phase are counted from `git log --oneline` between the
previous phase's journal and the current phase's journal (inclusive of
the current's).

### Phase 0 — Brainstorm & scope (`f3b2a12`, 1 commit)
Produced `specs/2026-04-22-project-cleanup-design.md` (667 lines:
six-phase roadmap, A/B/C/D/E audit taxonomy, 500-line file budget,
Rules 1–11 for test density/observability, release-flow rules for
`flog_dart`). Also
`journal/phase-0-brainstorming.md` (decision trail — why 500 lines and
not 800, why the C-class gets resolved before Phase 2, etc.).

### Phase 1 — Audit (`5888264` through `a243f76`, 4 commits)
4 read-only subagents ran in parallel over `transport/`, `domain/`,
`ui/`, and `flog_dart/`. Each produced an audit markdown with every
finding labelled A/B/C/D/E. Consolidated into
`audit/00-index.md` with a user gate before Phase 2. Initial count:
115 findings (27 A, 13 B, 0 C — all resolved to A/B/D/E in Task 3 —
66 D, 9 E). Later addenda added DOM-025, UI-041, UI-042, DART-033 and
TRANS-100..105 (new pure helpers extracted during Phase 2.5B).

### Phase 2 — Mechanical cleanup (`dea1190` through `1c81e1e`, 2 commits)
A single subagent pass resolved all 9 E-class entries, fixed clippy's
error + warnings, cleaned dead code (`LogStore::clear`, `adb::is_available`,
`UsbDevice`), and installed `#[derive(Default)]` / `impl Default`
where clippy demanded. Exit: clippy 0 warnings, fmt clean, test count
unchanged.

### Phase 2.5A — Logic/render separation (`d95acd6` through `2322b62`, 8 commits)
"Testability phase." Extracted pure helpers from renderers so render
logic could be unit-tested without `TestBackend`:
`compute_visible_entry_start`, `entry_row_count`, `repeat_bar_normalized`,
`compute_visible_network_range`, `handle_sse_field_navigation`.
UI-041 discovered mid-stream: the normal-mode mouse handler could not
be pure-extracted without a Phase 3 redesign (recorded as addendum in
the audit index, blocked event.rs coverage at 61% until Phase 3 Step
3.6 unblocked it).

### Phase 2.5B — Characterization test harness (`0710387` through `8713a72`, 16 commits)
The single largest investment: build the regression fence that makes
Phase 3 safe. 14 task-commits dispatched across subagents (mostly
sequential — parallel attempts hit worktree lock issues). Delivered:
- `tests/support/{ui_inspect,fake_flog_server,fixtures,mod}.rs` — shared
  test scaffolding (`Behavior` enum for fake server script, TUI line
  matchers, factory functions)
- 8 new integration crates under `tests/characterization_*.rs`
- +1 525 tests net (414 → 1 939 green, +3 `#[ignore = "bug: <id>"]`
  red tests for DOM-003 + DOM-018 × 2)
- Coverage hop 31.48% → 90.54% line

### Phase 3 — Redesign (`38cc1b9` through `8fe941e`, 98 commits, 10 steps)
The longest phase and the one that consumed the most tokens. Each
step was a separate plan + subagent dispatch + journal. Steps 3.1–3.10:

| Step | Scope | Audit ids closed | Commits |
|------|-------|------------------|---------|
| 3.1  | parser/ | DOM-013/015/016/017 + LazyLock ack | 6 |
| 3.2  | domain/ | DOM-001/002/003/005/006/008/011/018/019/024/025 (incl. 2 B-fixes unlocking DOM-003 + DOM-018) | 11 |
| 3.3  | transport/ | TRANS-002/004/005/006/008/009/014 + A-class acks | 10 |
| 3.4  | flog_dart | DART-001..009 (all B), DART-010..027 (D) | 16 |
| 3.5  | app state | UI-002/004/006/017/026/028/034/040 | 8 |
| 3.6  | event dispatch | UI-001/007/008/009/016/041 + UI-042 red lock | 10 |
| 3.7  | ui/logs | UI-010 (split), UI-013, UI-014 | 8 |
| 3.8  | ui/network + UI-042 fix | UI-037 (detail split), UI-010 mirror, UI-042 B-fix | 7 |
| 3.9  | ui/shared | UI-012 (source_select → device_picker rename + split), UI-014, UI-015, UI-030, UI-031, UI-038 | 8 |
| 3.10 | cross-cutting | UI-003 (partial — LogsViewState skeleton), UI-036 (test-module sibling extractions) | 14 |

The Phase 3 commit count (98) dwarfs every other phase because each
step split its work across 5–15 intermediate commits so a single
subagent turn stayed under the event-budget watchdog (~15–20 min).

### Phase 4 — Residual splits + why-comments (`056d664` through `49102df`, 6 commits)
Mopped up the three files that were still over 500 at Phase 3 exit
(`app.rs` 1506, `device_monitor.rs` 743, `main.rs` 564) + finished
the UI-003 LogsViewState migration (239 call sites changed) + added
10 why-comments at specified hotspots + deleted stale TODOs. Exit:
every production Rust file ≤ 500 lines.

### Phase 5 — Documentation (`7aaed95` through `1b2cbdd`, 8 commits)
4 new engineering docs (`ARCHITECTURE.md` 600L, `MODULES.md` 842L,
`PROTOCOL.md` 433L, `CONTRIBUTING.md` 339L) + refreshes to 5 existing
docs. Closed DART-024 (README gap) + DART-025 (CHANGELOG backfill).
Added `docs/superpowers/README.md` as the audit-trail index. No code
changes; test count unchanged.

### Phase 6 — Retrospective + methodology (this phase, 3 commits)
Produces this file + `ai-long-workflow-methodology.md` +
`journal/phase6.md`. No code changes; test count unchanged.

## 4. Bug tally — the 13 B-class entries

The audit's B class is "confirmed bug, user-observable if triggered."
Phase 2.5B wrote one `#[ignore = "bug: <id>"]` red test per entry;
Phase 3 un-ignored it in the same commit as the fix. Entries are
ordered by severity as in `audit/00-index.md`.

| ID         | Severity | Surface                                         | Red test planted | Fix commit | Who caught it |
|------------|----------|-------------------------------------------------|------------------|------------|---------------|
| DOM-003    | HIGH     | HTTP response without prior request silently dropped | Phase 2.5B Task 12 `218e78f` (`#[ignore]`) | Phase 3 Step 3.2 `7e333a1` | audit subagent (02-domain) |
| DART-001   | HIGH     | SSE parser dropped events after first `data:` line | Phase 1 (pre-existing `flog_dart/test/`) | Phase 3 Step 3.4 `6179631` | audit subagent (04-flog-dart) |
| DART-002   | HIGH     | `FlogSseParser.wrapTyped` + `SseEvent` APIs referenced by tests but not in `lib/` | Phase 1 (same file — compile error) | Phase 3 Step 3.4 `6179631` | audit subagent (04-flog-dart) |
| DOM-018 (a) | MEDIUM  | `search_positions()` could return overlapping ranges when OR terms overlap | Phase 2.5B Task 12 `218e78f` | Phase 3 Step 3.2 `3a4d9c1` | audit subagent (02-domain) |
| DOM-018 (b) | MEDIUM  | (second ignored case — plain-mode OR overlap merge) | Phase 2.5B Task 12 `218e78f` | Phase 3 Step 3.2 `3a4d9c1` | same |
| DART-004   | MEDIUM   | `FlogMockInterceptor.onRequest` ran even when `flogEnabled=false` | Phase 2.5B Task 13 `51320b7` | Phase 3 Step 3.4 `b0f1e55` | audit subagent (04-flog-dart) |
| DART-006   | MEDIUM   | `FlogWebSocket.stream` documented broadcast, actually single-subscription | Phase 2.5B Task 13 `51320b7` | Phase 3 Step 3.4 `c70d6f0` | audit subagent (04-flog-dart) |
| DART-008   | MEDIUM   | `_idMap`/`_startMap` leaked when earlier interceptor shortcut | Phase 2.5B Task 13 `51320b7` | Phase 3 Step 3.4 `77eabf8` | audit subagent (04-flog-dart) |
| TRANS-007  | LOW      | `tcp_open` used `Ok(Ok(_))` pattern — correct but fragile | Phase 2.5B Task 11 `b3f163f` (green, audit said "correct but fragile") | Phase 3 Step 3.3 `65d6ab3` (is_port_open helper) | audit subagent (01-transport) |
| DART-003   | LOW      | Library dartdoc referenced a nonexistent top-level `flog()` | Phase 2.5B Task 13 | Phase 3 Step 3.4 `ff4a710` | audit subagent (04-flog-dart) |
| DART-005   | LOW      | `ext.flog.syncMockRules` VM Service extension documented but never registered | Phase 2.5B Task 13 | Phase 3 Step 3.4 `804ebc0` (doc-only) | audit subagent (04-flog-dart) |
| DART-007   | LOW      | `_truncate` compared char count against a byte budget (UTF-8 corruption on CJK) | Phase 2.5B Task 13 | Phase 3 Step 3.4 `e09805d` | audit subagent (04-flog-dart) |
| DART-009   | LOW      | `emitNet` mutated caller-owned map | Phase 2.5B Task 13 | Phase 3 Step 3.4 `d72ceed` | audit subagent (04-flog-dart) |
| **UI-042** | MEDIUM (addendum) | WS chat ↔ raw toggle leaked collapse-key state, corrupting adjacent render | Phase 3 Step 3.6 `95f97d7` (red lock) | Phase 3 Step 3.8 `133b631` | **user**, mid-campaign (2026-04-24) |

**14 B-class bugs** total — one more than the "13" the audit initially
reported, because UI-042 was filed as an addendum mid-stream.

Notable dispositions:
- The audit labelled **TRANS-007** as "correct-but-fragile" — so its
  Phase 2.5B test was green, not red. The Phase 3 fix extracted
  `is_port_open()` and named the pattern, which the existing green
  test keeps locked.
- **DART-002** was unusual: the audit discovered that `flog_dart/test/`
  (untracked in git, held in the user's working tree) referenced APIs
  that didn't exist in `lib/`. Phase 1 Task 5 committed the test file
  as-is, making it the authoritative spec; Phase 3 Step 3.4 implemented
  `SseEvent` + `wrapTyped` to turn the compile-failure green. This is
  test-driven-development after the fact — the tests were written by
  a previous session and became the fence that shaped the
  implementation.

## 5. Architecture changes

Major before/after reshapes. Sizes are lines of code per `wc -l`.

| Item                                       | Before              | After                                          |
|--------------------------------------------|---------------------|------------------------------------------------|
| `src/event.rs` — single-file TUI dispatch  | 1 677 lines         | `src/event/` — 10 files, largest 495 (apply.rs) |
| `src/app.rs` — App struct + state machine  | 1 506 lines         | `src/app/` — 11 files, largest 484 (mod.rs)   |
| `src/ui/logs/mod.rs` — Logs view           | 1 358 lines         | `src/ui/logs/` — split into toolbar/list/status_bar/empty_states/highlight/timeline/stats/detail/{mod,renderers,section}/jump |
| `src/ui/network/detail.rs` — Network detail| 1 109 lines         | `src/ui/network/detail/` — 7 files (mod.rs 277, shared.rs 250, general.rs 95, http_body.rs 131, sse.rs 260, ws.rs 284, error.rs 44) |
| `src/ui/source_select.rs`                  | 898 lines           | renamed to `src/ui/device_picker/` — 6 files (mod.rs 230, card.rs 352, row.rs 263, modal.rs 99, click_map.rs 119, palette.rs 16) |
| `src/transport/device_monitor.rs`          | 743 lines (at Phase 3 exit) | `src/transport/device_monitor/` — 4 files (mod.rs 146, adb_source.rs 196, usbmuxd_source.rs 199, local_source.rs 197) |
| `src/ui/help.rs`                           | 534 lines           | `src/ui/help/` — mod.rs 278, content/{logs.rs 202, network.rs 119, mod.rs 8} |
| `src/main.rs`                              | 564 lines           | `src/main.rs` 93 lines + `src/run/` — dispatch.rs 140, server.rs 297, render_loop.rs 76 |
| `src/domain/structured_parser.rs`          | 693 lines           | `src/domain/structured_parser.rs` + `src/domain/json_tolerant.rs` (split per DOM-008) |
| `src/domain/network.rs` — `FlogNetMessage` loose struct | — | `FlogNetKind` typed enum (`#[serde(tag = "t")]`) — DOM-002 + DOM-006 |

Several redesigns added new abstractions rather than just splitting:
- `FilterVariant` trait (`domain/network_filter.rs`) unifies the 3
  previously-duplicated filter enums (DOM-001).
- `MessageFilter` trait + `FilterState` encapsulation (DOM-005 + DOM-019).
- `NetworkEntry` builder pattern replaces factory boilerplate (DOM-024).
- `ClickRegion` enum (`event/click_region.rs`) + two-phase
  `detect_click_region` / `apply_click_region` split — the
  design-for-testability moment that unblocked UI-041 (the audit had
  labelled it "cannot be pure-function-tested in current form").
- `MockEditState` bundle collapses scattered `mock_edit_*` fields on
  `App` (UI-026 + UI-034).
- `LayoutCache` struct separates render-layout state from business
  state (UI-017).
- `MultiStrategyParser::with_strategies` constructor allows custom
  parser chains without modifying the default order (DOM-013).

## 6. File-size trajectory

10 production files entered Phase 1 over the 500-line budget. By
Phase 4 exit, the production set was clean:

```
Phase 1 baseline (10 over 500 prod files):
  src/event.rs                       1677
  src/ui/logs/mod.rs                 1358
  src/app.rs                         1167
  src/ui/network/detail.rs           1109
  src/ui/source_select.rs             898
  src/ui/json_viewer/render.rs        745
  src/ui/network/mod.rs               700
  src/domain/structured_parser.rs     693
  src/transport/device_monitor.rs     654
  src/main.rs                         546
  (flog_dart/lib/src/flog_dio.dart    504 — Dart side)

Phase 4 exit (0 over 500 prod files):
  — every production .rs ≤ 500 lines
  — sibling-file _tests.rs files are above 500 (filter_tests 606,
    network_store_tests 791, protocol_tests 526) — by design;
    CONTRIBUTING.md documents test files are exempt from the budget
    because they grow linearly with observable scenarios
```

The budget is a signal, not a hard law. Phase 3 Step 3.9 and Step 3.10
in particular made tactical calls to split files one level deeper
than the plan called for (see §8 "What didn't work — over-split").

## 7. Test trajectory

| Phase       | `cargo test` (lib) | Coverage (line) | Notes |
|-------------|--------------------|------------------|-------|
| Entry (Phase 0) | 217            | 31.48%           | 1 `cargo test --test` smoke test |
| Phase 2 exit    | 217            | 31.48%           | no behavior changes |
| Phase 2.5A exit | 222 (+5 pure-fn extracts) | 31.79%   | modest — helpers wired in |
| **Phase 2.5B exit** | **640 lib + 1 299 integration = 1 939** | **90.54%** | 3 ignored (DOM-003 + 2×DOM-018) |
| Phase 3 exit    | 1 507 lib equivalent (balancing sibling _tests.rs extraction) | 90.54%+ (never dropped) | 0 ignored (all red tests flipped) |
| Phase 4 exit    | 2 166 total      | ≥90%             | +test count from UI-003 migration touching characterization tests |
| Phase 5 exit    | 2 166 total      | unchanged        | docs-only |
| Phase 6 exit    | 2 166 total      | unchanged        | docs-only |

Characterization crates — their purpose and final size:

| Crate                                       | Final test count | Purpose |
|---------------------------------------------|------------------|---------|
| `tests/characterization_bugs.rs`            | 7                | B-class locked; all flipped green by Phase 3 Step 3.2 + 3.6 + 3.8 |
| `tests/characterization_app_state.rs`       | 157              | App state machine transitions, scroll, multi-app, mock edit |
| `tests/characterization_event_keys.rs`      | 107              | keyboard dispatch per AppMode × tab |
| `tests/characterization_event_mouse.rs`     | 108              | TestBackend mouse routing (UI-041 unblocked Phase 3 Step 3.6) |
| `tests/characterization_input.rs`           | 14               | FakeFlogServer driving `input/connector.rs` round trips |
| `tests/characterization_ui_logs.rs`         | 84               | Logs view TestBackend snapshots |
| `tests/characterization_ui_network.rs`      | 128              | Network view TestBackend snapshots (incl. UI-042 guard) |
| `tests/characterization_ui_source_select_help.rs` | 53         | picker + help TestBackend |
| `tests/ws_server_test_direct.rs`            | 1                | existing pre-campaign smoke |

Dart side: `flutter test` moved from 84 pass / 3 skip / 1 fail
(DART-001 compile error) to 133 pass / 2 skip / 0 fail.

## 8. Deferred items

Items explicitly punted out of scope, each with a forward reference.

- **DART-024 / DART-025** — README + CHANGELOG gaps. Deferred from
  Phase 3 Step 3.4 (plan choice — content work, not code). Closed in
  Phase 5 Task 7 (`364fb64`).
- **DART-033** — flog_dart SSE subsystem architecture debt (layering
  mix, closure-variable state, duplicate parser paths). Filed as a
  D-class addendum 2026-04-24 by an external reviewer audit.
  Decision: deferred to a flog_dart v0.8 breaking release **after
  Phase 5** — v0.8 ships separately. Phase 5 wrote the migration
  note (`docs/PROTOCOL.md §9.1`, `flog_dart/CHANGELOG.md "Planned for
  v0.8"`). No compromise: the DART-001/002 correctness fix already
  landed in Phase 3, the remaining debt is architectural and should
  land with a migration doc, not squeezed in.
- **UI-011** — JsonViewerPane state-ownership fingerprint — partial.
  Step 3.8 consumed the budget with detail/mod.rs split + UI-042
  fix; Step 3.9/3.10 picked up only half (the rename + component
  colocation). Remaining half — making pane fingerprint explicit
  about which keyspace owns which `collapsed_sections` — is noted in
  Step 3.8 journal §"Deferred".
- **TRANS-013** — `src/replay.rs` archival (dead module reachable
  only via `pub mod`). Flagged during Phase 2.5B Task 12 as
  UNTESTABLE D-ref. Deferred indefinitely — the replay flow that
  would use it was never wired and the file is ~50 lines of
  no-op code.
- **TRANS-016 / TRANS-017** — `src/transport/flutter_logs.rs`
  compiled-but-dead; discovered during Phase 5's `MODULES.md`
  verification. Recorded in `docs/MODULES.md "Audit trail gaps"`;
  will migrate to the `audit/01-transport.md` addenda in a future
  session if not fixed first. Phase 5 red line forbade fixing them
  inline.
- **`src/event/mod.rs` 61% coverage** — PHYS-documented. The
  unreached 38% is the `handle_normal_mouse` dispatcher driving real
  `ratatui::backend::CrosstermBackend`. Phase 3 Step 3.6's two-phase
  split unblocks it in principle; to raise coverage further,
  TestBackend variants of the dispatcher's outer shell would be
  needed, which is out of scope for this campaign.
- **`src/main.rs` + `src/run/server.rs` bootstrap paths 45–60%** —
  PHYS-documented. Tokio runtime setup, signal handlers, real
  terminal enter/leave are classic bootstrap code without pure
  seams.

## 9. What surprised us

Three honest surprises worth naming:

1. **An external reviewer found a bug we thought was fixed (DART-001).**
   After Phase 3 Step 3.4 shipped the parser rewrite, an external
   audit flagged that our test inputs didn't cover W3C SSE's
   "multiple-event-per-chunk" scenario. We re-read the spec — our
   parser actually handled it correctly, but the reviewer's repro
   input was a `return`-delimited (Mac Classic) stream the spec
   explicitly doesn't support. The parser correctly rejected it. We
   still added a regression-guard test (`06eccd4`
   `test(flog_dart/sse): DART-001 repro guards — W3C multi-line data
   + multi-event-per-chunk`) because "the bug was filed → we need
   evidence it cannot recur" is cheaper than "the bug was filed →
   we argue from the spec." Honest accounting cost one extra test.

2. **The `C = 0` discipline paid off bigger than expected.** Phase 1
   Task 3 forced every C-class (ambiguous) finding to be resolved
   with the user before Phase 2 began. We thought this was bureaucratic
   overhead; it turned out to be the single clearest dividing line
   between "work that can be parallelised to subagents" and "work
   the user has to decide first." Every subagent mis-implementation
   after Phase 2 traced back to an A/B/D/E finding, not to a C-class
   gap.

3. **The user found UI-042 before we did.** At the transition between
   Step 3.5 and Step 3.6, the user clicked around in the TUI and saw
   a WS-chat → raw toggle corrupt the list pane. None of our 108
   mouse characterization tests had caught it. The bug was the
   `ws_chat_mode` field being flipped without purging
   `collapsed_sections` — stale keys survived the mode swap. We
   wrote the red-lock test (Phase 3 Step 3.6 `95f97d7`) before the
   fix (Step 3.8 `133b631`). The lesson: 90% coverage is not 100%
   behavior coverage, because coverage is "lines executed" and bugs
   live in state interactions between lines.

## 10. Total commit count

The campaign counts:
- First campaign commit: `f3b2a12` (Phase 0 design + journal), 2026-04-22 18:54
- Last campaign commit (Phase 6 exit): added by this phase's Task 3
- Commit count since campaign start: **159 before Phase 6 + 3 new =
  162 total commits** over three days.

Per-phase commit distribution:
| Phase | Commits |
|-------|---------|
| 0     | 1       |
| 1     | 3       |
| 2     | 2       |
| 2.5A  | 8       |
| 2.5B  | 16      |
| 3     | 98 (all 10 steps) |
| 4     | 6       |
| 5     | 8       |
| 6     | 3       |

Phase 3 dominates not because the work was larger but because each
step's work was split across 5–15 commits (one per audit-cluster or
one per file-split) to keep individual commits reviewable and keep
any one subagent turn under the runtime's event-truncation budget.
The same volume of work committed as 1 per phase-step would have
been ~10 commits; splitting further was an intentional subagent-
safety choice, not bureaucratic.

## 11. Lessons for this codebase (future work, read me first)

If you're the next contributor looking at flog:

1. **UI-012 rename is load-bearing.** The old `ui/source_select`
   name was wrong — the module is a device picker, not a source
   selector. Every doc, comment, and test name was migrated in
   Step 3.9 (`0441135`). If you see `source_select` anywhere, it's
   stale.

2. **`collapsed_sections` ownership is per-pane.** After UI-042, the
   convention is: every pane that mutates `collapsed_sections` must
   purge its own keyspace on mode transitions. Keys are namespaced
   by prefix (`WS#*` for raw mode, `WS_GROUP#*` for chat mode, etc.).
   See `src/app/mod.rs::purge_ws_collapse_keys`.

3. **Two-phase mouse dispatch is the seam for further event work.**
   `event/detect.rs::detect_click_region` is a pure function over
   (App ref, x, y) → Option<ClickRegion>. Any new click target goes
   in `ClickRegion` (enum in `event/click_region.rs`). Any new
   mutation goes in `event/apply.rs::apply_click_region`. The
   dispatcher itself (`event/mod.rs::handle_normal_mouse`) should
   stay ~35 lines. Do not re-interleave detection and mutation — it
   will tank UI-041 coverage.

4. **Ignored tests carry audit ids.** If you see
   `#[ignore = "bug: <id>"]` in a commit, the id points to an
   `audit/*.md` entry explaining the bug. When you fix it, un-ignore
   in the same commit. No shadow TODOs.

5. **Sibling-file test modules are the default.** `src/foo.rs` pairs
   with `src/foo_tests.rs` (same module, `#[cfg(test)] mod tests;`).
   UI-036 migrated every in-file `mod tests` block to a sibling
   file. New modules should do the same. The 500-line budget does
   not apply to `*_tests.rs` files by design (CONTRIBUTING §5.5).

6. **The `FlogNetKind` enum is the wire protocol.** `FlogNetMessage`
   (pre-Phase-3) is gone. When adding a new protocol variant, add it
   to `FlogNetKind` (`#[serde(tag = "t")]`) in
   `src/domain/network.rs`. PROTOCOL.md lists the wire-level examples.

7. **flog_dart v0.8 is planned but not shipped.** Any SSE-subsystem
   work on the Dart side should coordinate with DART-033 — the v0.8
   plan is the "open" item. Until v0.8 ships, treat the parser as
   stable and do not refactor the layer boundaries.

8. **`cargo llvm-cov` is the coverage source of truth.** Baseline
   lives in `docs/superpowers/audit/.coverage-phase2-5b-final.txt`.
   Phase 3 treated "coverage must not drop" as a hard gate. Future
   work should do the same.

## 12. Honest costs

- **Real-world elapsed:** three calendar days, likely ~20–30 hours of
  the user's attention (session start 2026-04-22 morning, session
  end 2026-04-24 evening). Not 3 person-days of continuous work —
  subagents ran in the background, the user supervised, reviewed,
  and occasionally intervened.
- **Subagent rounds:** ~15–20 across Phase 2.5B (14 task-commits)
  + Phase 3 (10 steps × 1–2 subagent rounds each) + Phase 4 + Phase
  5 task subagents. Not every commit was a subagent — many of the
  mechanical task-commits were inline work.
- **Subagent retries:** at least 3 task-commits hit "Truncated event
  message received" (Task 5 of Phase 2.5B had to be split into 5a +
  5b before the subagent could complete). The fix was always
  smaller task scope, never "trust the subagent harder."
- **Plan revisions mid-stream:** Phase 2.5B's initial plan was revised
  (`4aab5f7 docs(superpowers): revise Phase 2.5B plan — stricter gates,
  parallel subagents`) after Task 3 came back with shallow tests that
  missed Rule 9 multi-scenario density. Stricter numeric gates
  (Rule 2 per-module coverage, Rule 9 multi-scenario, Rule 10
  per-pub-fn density) were introduced and the remaining tasks
  re-dispatched.
- **User interventions:** plan approval (explicit) at every phase
  boundary; the C-class adjudication at Phase 1 Task 3; the
  "加快进度" nudge when Phase 3 Step 3.4 was debating whether to
  ship the DART-020 constant as `defaultCapacity` or
  `FLOG_STORE_CAPACITY` (user picked one); the UI-042 bug report
  (user caught a bug the suite missed); the final push to ship
  Phase 6 ("不能因为不好做而妥协" — don't compromise because it's
  hard, meaning write the failures into the retrospective honestly,
  which is what this file does).

## 13. Exit status

Every exit gate from every phase's plan cleared. See the journals.
Specifically for Phase 6:

- ✅ `docs/superpowers/retrospective-flog.md` (this file) exists.
- ✅ `docs/superpowers/ai-long-workflow-methodology.md` exists
  (sibling doc).
- ✅ `docs/superpowers/journal/phase6.md` exists (campaign-close
  journal).
- ✅ `docs/superpowers/README.md` updated with an "Outcome" section
  referencing the two new docs.
- ✅ `cargo test --all` green — 2 166 passed, 0 failed, 0 ignored
  (unchanged across Phases 5 and 6; both phases are docs-only).
- ✅ No code changes introduced by Phase 6.

The campaign is closed. Any further work on flog is a new spec.
