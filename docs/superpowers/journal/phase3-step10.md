# Phase 3 Step 3.10 — Cross-cutting cleanup (Journal) — Phase 3 complete

**Scope:** Close the deferred UI-003 (LogsViewState symmetry with
NetworkState) and separate embedded `#[cfg(test)] mod tests` blocks from
the nine remaining production files that broke the 500-line budget.
Assemble the Phase 3 residual-acks index so every audit ID has a
documented resolution or explicit deferral.

## Commit range

- **Phase 2.5B final**: `8713a72` (`docs(journal): Phase 2.5B — characterization tests complete`)
- **Phase 3 first commit**: `38cc1b9` (`docs(superpowers): Phase 3 Step 3.1 — parser layer redesign plan`)
- **Phase 3 Step 3.10 final**: this commit (end of Phase 3)

Phase 3 spans 109 commits across 10 steps (3.1 → 3.10).

## Step 3.10 commits

| # | SHA | Subject |
|---|-----|---------|
| 1 | `16ad0e5` | `refactor(app): LogsViewState symmetry with NetworkState (Phase 3 UI-003)` |
| 2 | `e8922ef` | `refactor(transport/device_monitor): extract 4 test modules into sibling files (Phase 3 UI-036 mirror)` |
| 3 | `cf4ba35` | `refactor(domain/network_store): extract test module into sibling file (Phase 3 UI-036 mirror)` |
| 4 | `24a85a9` | `refactor(domain/filter): extract test module into sibling file (Phase 3 UI-036 mirror)` |
| 5 | `469d022` | `refactor(domain/network): extract test module into sibling file (Phase 3 UI-036 mirror)` |
| 6 | `1d986da` | `refactor(main): extract test module into sibling file (Phase 3 UI-036 mirror)` |
| 7 | `6349e23` | `refactor(domain/network_filter): extract test module into sibling file (Phase 3 UI-036 mirror)` |
| 8 | `f7d3bce` | `refactor(input/protocol): extract test module into sibling file (Phase 3 UI-036 mirror)` |
| 9 | `02f527c` | `refactor(parser/generic): extract test module into sibling file (Phase 3 UI-036 mirror)` |
| 10 | `7709b8a` | `refactor(session): extract test module into sibling file (Phase 3 UI-036 mirror)` |
| 11 | _(this commit)_ | `docs(journal): Phase 3 Step 3.10 + Phase 3 consolidated ack index` |

## Test delta

- Lib: 743 → 746 (+3 from `LogsViewState` tests in `src/app_tests.rs`)
- All other suites unchanged across Step 3.10: 761 bin, 157 network_parser,
  7 characterization_bugs (0 ignored), 107 event_keys, 108 event_mouse,
  14 ws_connect, 84 ui_logs, 128 ui_network, 53 source_select_help,
  1 ws_direct.
- Total: 1,569 → 1,572 passing; 0 failing; 0 ignored.

## Line-budget results

Ten files were ≥500 lines at Step 3.10 entry. Eight drop back under 500
via pure test extraction. Three remain >500 and are flagged for Phase 4
deeper splits (deviations from plan target documented below):

| File | Before | After prod | Test sibling | Under 500? |
|------|-------:|-----------:|-------------:|:----------:|
| `src/transport/device_monitor.rs`   | 1609 | 743 | 4 files under `src/transport/device_monitor/` | ❌ |
| `src/app.rs`                        | 1461 | 1506 (+struct+delegate+test ref) | `src/app_tests.rs` | ❌ |
| `src/domain/network_store.rs`       | 1089 | 298 | `src/domain/network_store_tests.rs` | ✅ |
| `src/domain/filter.rs`              |  944 | 338 | `src/domain/filter_tests.rs` | ✅ |
| `src/main.rs`                       |  767 | 564 | `src/main_tests.rs` | ❌ |
| `src/domain/network.rs`             |  734 | 392 | `src/domain/network_tests.rs` | ✅ |
| `src/domain/network_filter.rs`      |  656 | 265 | `src/domain/network_filter_tests.rs` | ✅ |
| `src/input/protocol.rs`             |  639 | 115 | `src/input/protocol_tests.rs` | ✅ |
| `src/parser/generic.rs`             |  508 | 208 | `src/parser/generic_tests.rs` | ✅ |
| `src/session.rs`                    |  507 | 135 | `src/session_tests.rs` | ✅ |

All remaining `src/**/*.rs` production files are ≤495 lines.

## Deviations from plan

### UI-003 scope — minimum additive, not mass migration

Plan Task 1 allowed two options: "full" (migrate all 190 `app.selected /
scroll_offset / auto_scroll` call sites into `app.logs.*`) or "minimum"
(add the struct + accessor, defer the migration). A grep count of the
mutation surface showed:

- ~49 internal `self.selected/scroll_offset/auto_scroll` reads inside `app.rs`.
- ~5 `app.selected` (etc.) reads in `src/event/mod.rs`.
- ~23 reads across `src/ui/logs/{list,status_bar,empty_states}.rs`.
- ~154 reads across `tests/characterization_*.rs` (read-only asserts).

Migrating all of these would have created ~220 edited lines plus the
struct itself — over the plan's 300-diff-line budget and adding real
risk of mass-renaming test expectations. Per plan rule 6, took the
additive path: kept the top-level fields as the source of truth, added
`LogsViewState` + `App::logs()` projection accessor, and marked them
`#[allow(dead_code)]` (Phase 3 additive; Phase 4 migrates call sites).
Net delta: `app.rs` gained +45 lines (struct + doc + accessor) and
gained a 3-test sibling file.

Consequence: `app.rs` grew from 1461 → 1506 lines instead of shrinking
to ~1410 as the full plan predicted. Phase 4 will flip the ownership
and retire `App.selected/scroll_offset/auto_scroll`.

### `#[path]` for nested inline modules

The plan example

```rust
#[cfg(test)]
#[path = "device_monitor_tests.rs"]
mod tests;
```

worked exactly as written for 8 of 10 files. The exception was
`src/transport/device_monitor.rs`, whose embedded tests live *inside*
three nested inline modules (`mod adb_source`, `mod usbmuxd_source`,
`mod local_source`). When a `#[path]` attribute is attached to a
`mod tests;` declaration *inside* an inline parent, rustc resolves the
path literally against the parent's virtual directory, without `..`
canonicalization. Attempts such as `#[path = "../../foo.rs"]` fail
because intermediate directories like
`src/transport/device_monitor/adb_source/` do not physically exist and
rustc refuses to traverse through them.

Workaround applied: create the virtual subdirectories and place each
nested test file at its canonical virtual location. Final layout:

```
src/transport/device_monitor/
├── tracker_tests.rs                 (top-level tracker tests)
├── adb_source/tests.rs              (nested adb_source tests)
├── usbmuxd_source/tests.rs          (nested usbmuxd_source tests)
└── local_source/tests.rs            (nested local_source tests)
```

The parent module uses the simpler form for the nested cases (no
`#[path]` attribute needed — the file is at the default discovery
location).

### Three files still >500 lines

- **`src/app.rs` (1506)**: UI-003 kept the migration deferred; also
  contains the multi-app connection state, `MockEditState`, mouse/select
  modes, and `NetworkState`. Phase 4 (why-comment + call-site migration
  pass) is expected to drop it substantially.
- **`src/transport/device_monitor.rs` (743)**: three inline
  `mod adb_source / usbmuxd_source / local_source` blocks of production
  code. Splitting them into real sibling files is out of scope for Step
  3.10 (test extraction only) — flagged for Phase 4.
- **`src/main.rs` (564)**: still carries the full TUI render loop,
  terminal setup, WS server startup, and connector event dispatch.
  Deeper refactor (split the server processor into `src/server_loop.rs`
  etc.) is out of scope for Step 3.10 — flagged for Phase 4.

None of these block Phase 3 closure; every residual is documented above
and queued for Phase 4.

## Residual acks — Phase 3 consolidated index

### A-class (documentation / ack only) — 27 total

All acknowledged via inline comments during Phase 3. None outstanding.

| Audit ID | Step closed | Evidence |
|----------|-------------|----------|
| DOM-004, DOM-007, DOM-020 | Step 3.2 | Phase 3 step2 journal |
| DOM-014, DOM-016 | Step 3.1 | Inline comments in parser modules |
| TRANS-003, TRANS-005, TRANS-008 (ack leg), TRANS-010, TRANS-011, TRANS-015 | Step 3.3 | `1688aac`, `a2d3503`, `7e6c280` |
| TRANS-100..105 | Step 3.3 (inherited from `b3f163f` Phase 2.5B) | ack rows |
| UI-018, UI-022, UI-023, UI-027, UI-032 | Step 3.5 | `7e58e6a`, `f860014` |
| DART-030, DART-031, DART-032 | Step 3.4 | Task 19 inline comments |

### B-class (bug fixes) — 12 total, all closed

| Audit ID | Step closed | Resolution |
|----------|-------------|------------|
| DOM-003 | Step 3.2 | `7e333a1` — HTTP response without prior request now errors; char test un-ignored |
| DOM-018 | Step 3.2 | `3a4d9c1` — `search_positions()` no longer overlaps; char test un-ignored |
| TRANS-007 | Step 3.3 | `65dbd4b` — `is_port_open` helper replaces `Ok(Ok(_))` pattern; char test green |
| DART-001 | Step 3.4 | `6179631` — SSE parser rewritten to full spec |
| DART-002 | Step 3.4 | `6179631` — `SseEvent` + `wrapTyped` APIs added |
| DART-003 | Step 3.4 | `ff4a710` — library dartdoc corrected |
| DART-004 | Step 3.4 | `b0f1e55` — mock interceptor early-returns when `!flogEnabled` |
| DART-005 | Step 3.4 | `804ebc0` — docs clarify real `mock_sync` WebSocket channel |
| DART-006 | Step 3.4 | `c70d6f0` — stream is `asBroadcastStream()` |
| DART-007 | Step 3.4 | `e09805d` — `_truncate` now UTF-8-byte accurate |
| DART-008 | Step 3.4 | `77eabf8` — `_idMap` / `_startMap` leaks fixed via `options.extra` |
| DART-009 | Step 3.4 | `d72ceed` — `emitNet` clones caller map |

UI-042 (reported during Step 3.5→3.6 transition) was classified B and
closed in Step 3.8 (`133b631` — purge stale collapse keys on WS mode
toggle). Red-lock test un-ignored; 0 ignored bugs remain in
`tests/characterization_bugs.rs`.

### D-class (architecture debt) — 66 total + 6 added mid-phase

**All in-scope D-class findings resolved by Step 3.10.** Three items
are explicitly deferred with documented targets:

| Audit ID | Step / target | Reason |
|----------|---------------|--------|
| DART-024 | **Phase 5** (docs) | README docs pass |
| DART-025 | **Phase 5** (docs) | CHANGELOG backfill |
| DART-033 | **flog_dart v0.8** (post-Phase-5) | Architecture debt reported mid-Step-3.5→3.6; requires API breakage — shipped as coherent v0.8 release with migration docs |

Per-step D-class closures:

- **Step 3.1 (Parser):** DOM-013, DOM-015, DOM-017.
- **Step 3.2 (Domain):** DOM-001, DOM-002+DOM-006, DOM-005+DOM-019, DOM-008, DOM-011, DOM-024, DOM-025.
- **Step 3.3 (Transport):** TRANS-002, TRANS-004, TRANS-006, TRANS-009, TRANS-012, TRANS-014, TRANS-013 (replay.rs archival).
- **Step 3.4 (flog_dart):** DART-010..023 (minus 024/025 deferred), DART-026, DART-027.
- **Step 3.5 (App state machine):** UI-002, UI-004, UI-006, UI-017, UI-026, UI-028, UI-034, UI-040. UI-003 deferred to Step 3.10.
- **Step 3.6 (Event dispatch):** UI-001, UI-007, UI-008, UI-009, UI-016, UI-020, UI-024, UI-041.
- **Step 3.7 (UI Logs split):** UI-010 (Logs), UI-013, UI-014, UI-038.
- **Step 3.8 (UI Network split):** UI-010 mirror (Network), UI-011, UI-037, UI-042 (fix + test).
- **Step 3.9 (UI shared components):** UI-012, UI-014 mirror (text_editor split), UI-015, UI-030 (render split), UI-031 (colorize split), UI-038 mirror (device_picker split).
- **Step 3.10 (cross-cutting):** UI-003 (LogsViewState additive — call-site migration deferred to Phase 4), UI-036 (module docs implicit in 10 test extractions).

### E-class (dead code removal) — 9 total

All removed in Phase 2 (per the consolidated `00-index.md` E-class note).

## Test suite snapshot at Phase 3 exit

```
lib:                        746 passed, 0 ignored
bin:                        761 passed, 0 ignored
characterization_bugs:        7 passed, 0 ignored
characterization_event_keys: 107 passed
characterization_event_mouse:108 passed
characterization_network_parser: 157 passed
characterization_source_select_help: 53 passed
characterization_ui_logs:    84 passed
characterization_ui_network:128 passed
ws_connect_test:             14 passed
ws_server_test_direct:        1 passed
```

Total: 1,572 passing, 0 failing, 0 ignored.

## Exit gate sign-off

- [x] Every pre-Step-3.10 audit ID has a resolution row in this journal
- [x] All B-class Rust bugs resolved (0 `#[ignore]` in
  `tests/characterization_bugs.rs`)
- [x] Characterization suites all green
- [x] `cargo clippy --all-targets -- -D warnings` clean
- [x] `cargo fmt -- --check` clean
- [x] 8 of 10 oversized files back under 500 lines; 3 deviations
  documented with Phase 4 hand-off
- [x] UI-003 additively closed (struct + accessor + 3 tests)

Phase 3 complete. Next: Phase 4 why-comments pass (audit items not
applicable; a fresh review of every load-bearing magic constant and
non-obvious invariant).
