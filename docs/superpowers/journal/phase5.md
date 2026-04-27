# Phase 5 — Documentation

**Plan:** `docs/superpowers/plans/2026-04-24-phase5-docs.md`
**Start HEAD:** `7aaed95` (plan commit)
**End HEAD:** this commit

## Outcome

All 8 plan tasks complete, one commit per task.

`cargo test --all`: **2 166 passed, 0 failed, 0 ignored** — zero delta
vs. Phase 4 exit. No code changes in this phase, as required.

## Commits

| # | SHA       | Summary                                                               |
|---|-----------|-----------------------------------------------------------------------|
| 1 | `96ccd24` | `docs(arch): add ARCHITECTURE.md (Phase 5)`                           |
| 2 | `d06c7c4` | `docs(arch): add MODULES.md (Phase 5)`                                |
| 3 | `fb73ac1` | `docs(arch): add PROTOCOL.md (Phase 5)`                               |
| 4 | `52b6329` | `docs(arch): add CONTRIBUTING.md (Phase 5)`                           |
| 5 | `65d6ab3` | `docs: update CLAUDE.md for Phase 4 exit state (Phase 5)`             |
| 6 | `6cfadb4` | `docs: refresh README.md + README_EN.md (Phase 5)`                    |
| 7 | `364fb64` | `docs(flog_dart): refresh README + CHANGELOG (Phase 5 DART-024/025)`  |
| 8 | *(this)*  | `docs(journal): Phase 5 — documentation complete`                     |

## New engineering docs (Task 1–4)

| File                        | Lines | Target range |
|-----------------------------|-------|--------------|
| `docs/ARCHITECTURE.md`      | 600   | 400–600      |
| `docs/MODULES.md`           | 842   | 600–900      |
| `docs/PROTOCOL.md`          | 433   | 200–400      |
| `docs/CONTRIBUTING.md`      | 339   | 200–300      |

PROTOCOL and CONTRIBUTING slightly exceed their stretch targets but
the content is all load-bearing reference material (one JSON example
per wire variant + field tables; full audit taxonomy + rules + file
budget + commit format + release flow + AI reading order). Tightening
further would elide content the plan explicitly requires.

## Existing docs updated (Task 5–7)

| File                        | Lines before | Lines after | Delta  |
|-----------------------------|--------------|-------------|--------|
| `CLAUDE.md`                 | 141          | 169         | +28    |
| `README.md` (Chinese)       | 272          | 286         | +14    |
| `README_EN.md`              | 127          | 280         | +153   |
| `flog_dart/README.md`       | 49           | 254         | +205   |
| `flog_dart/CHANGELOG.md`    | 27           | 202         | +175   |

`README_EN.md` was the most stale: it still referenced `flog_logger`,
`--uri`, `--adb`, `--stdin` — all removed in the Direct Socket
migration. Rewritten end-to-end to mirror `README.md` as a sibling
translation. No content duplicated into a single file; the two remain
language-twin siblings.

## DART-024 / DART-025 resolution (Task 7)

- **DART-024 — README docs gap.** `flog_dart/README.md` previously
  listed features but gave almost no usage detail. Rewritten to cover
  every public surface: `Flog.init`, `FlogLogger`, `FlogDio` +
  `FlogHttpConfig`, manual-interceptor ordering rule (red line),
  `FlogDio.sse` / `FlogSseParser.wrap` / `FlogSseParser.wrapTyped`
  with typed `SseEvent`, `FlogWebSocket`, mock rules sync + matching
  semantics, replay round-trip, `flogEnabled` override matrix.
- **DART-025 — CHANGELOG history gap.** Previously jumped 0.2.0 →
  0.7.1; reconstructed entries for 0.3.0 / 0.4.0 / 0.5.0 / 0.6.0 /
  0.6.1 / 0.6.2 / 0.6.3 / 0.6.4 from `git log -- flog_dart/
  flog_logger/`. Current campaign work (DART-001..027) summarised
  in an Unreleased section with an explicit "Planned for v0.8"
  subsection carrying the DART-033 forward reference.

## DART-033 forward reference

Preserved in two places per plan: `docs/PROTOCOL.md §9.1` and
`flog_dart/CHANGELOG.md` "Planned for v0.8". Both explicitly note
the wire protocol stays unchanged; only the Dart-side SSE API
changes in v0.8.

## Audit trail index (Task 8)

`docs/superpowers/README.md` added — indexes the 4 sub-directories
(`specs/`, `plans/`, `audit/`, `journal/`), describes how artifacts
interconnect, and gives a reading order for a new contributor.

## Cross-doc inconsistencies found + resolved

1. **FlogNetMessage vs FlogNetKind.** Existing `CLAUDE.md` referenced
   the pre-Phase-3 `FlogNetMessage` struct. Phase 3 DOM-002/006
   replaced it with a typed `#[serde(tag = "t")]` enum
   `FlogNetKind`. Fixed everywhere it surfaced (CLAUDE.md, new docs).

2. **`flog_dart` version mismatch.** Chinese `README.md` pinned
   `flog_dart: ^0.6.4` while pubspec is on 0.7.2. Bumped in place.

3. **Module path shape.** `CLAUDE.md` referred to `src/app.rs`,
   `src/event.rs`, `main.rs starts WS server` — all wrong after the
   Phase 3 / 4 directory splits. Every path updated to the current
   `src/app/`, `src/event/`, `src/run/` layout; main.rs noted as 93
   lines.

4. **Keyboard shortcut drift.** Chinese `README.md`'s Network table
   was missing `\` (Exclude), `Ctrl+M` (mock panel), and `E`/`C`
   (expand/collapse). Pulled the canonical set from
   `src/ui/help/content/{logs,network}.rs` and extended both
   language tables to match.

5. **Scroll model wording.** `CLAUDE.md` said `auto_scroll` lives on
   `App`; Phase 4 UI-003 completion moved it to `LogsViewState` /
   `NetworkState`. Corrected.

## Audit trail gaps noticed

Two new **E-class** findings filed during `MODULES.md` verification;
per Phase 5 red lines, **not fixed here**:

- **TRANS-016** — `src/transport/flutter_logs.rs` defines `FlutterLogs`
  but nothing in `src/` references it (verified via `rg "flutter_logs|FlutterLogs"
  src/`). Either remove the file or wire it into a "read Flutter logs
  for a chosen device" source.
- **TRANS-017** — Follow-up to TRANS-016: `src/transport/mod.rs` does
  not re-export `flutter_logs`, so the module is compiled-but-dead
  code reachable only via the `pub mod` path. Should be removed
  together with TRANS-016 once triaged.

Both recorded in `docs/MODULES.md` under "Audit trail gaps" at the
bottom of the file. They'll move to `audit/01-transport.md` Addenda
in a future session if the fix isn't merged first.

## Verify-before-write discipline

Every symbol / file / function name referenced in the new docs was
verified against HEAD (`7aaed95`) via `Grep` / `Read` / `Glob` as
the docs were written:

- `TransportAddr` / `resolve_transport_addr` — confirmed in
  `src/transport/resolve.rs`.
- `ConnectorHandle::{send, send_mock_sync, send_replay, send_subscribe, for_testing}`
  — confirmed in `src/input/connector.rs`.
- `FlogNetKind::{Req, Res, Err, Chunk, Done, Open, Send, Recv, Close}`
  — confirmed in `src/domain/network.rs`.
- `AppMode::{Normal, InputActive(InputField), Help, Stats, MockRuleEdit}`
  — confirmed in `src/app/mod.rs`.
- `ClickRegion` variants — confirmed in `src/event/click_region.rs`.
- Reconnect constants (2 / 30 / 2) — confirmed in
  `src/run/server.rs`.
- `MAX_ENTRIES = 100_000` (LogStore), `10_000` (NetworkStore),
  `50000` (FlogStore) — all confirmed in their respective files.
- Keyboard shortcut tables — pulled from
  `src/ui/help/content/{logs,network}.rs` directly.
- Every JSON example in `PROTOCOL.md` traced back to
  `src/input/protocol_tests.rs` or `tests/support/fixtures.rs`.

Where a claim didn't hold up under verification (three instances:
`FlogNetMessage` → `FlogNetKind`, stale keyboard table in the Chinese
README, `auto_scroll` location), the doc was adjusted to match the
code, not the other way around.

## Exit-gate check

- ✅ 4 new engineering docs in `docs/` (ARCHITECTURE, MODULES,
  PROTOCOL, CONTRIBUTING).
- ✅ 5 updated user / AI docs (README, README_EN, CLAUDE.md,
  flog_dart/README, flog_dart/CHANGELOG).
- ✅ All code / file references in docs resolve at HEAD.
- ✅ DART-024, DART-025 resolved.
- ✅ Audit trail index at `docs/superpowers/README.md`.
- ✅ `cargo test --all` still green (2 166 / 0 / 0).

## Hand-off

Phase 6 (retrospective + methodology case study) remains. See
`docs/superpowers/README.md` for the intended output locations
(`retrospectives/`, `methodology/`) and
`specs/2026-04-22-project-cleanup-design.md §7` for the scope.
