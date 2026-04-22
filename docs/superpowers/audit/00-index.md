# Audit consolidated index — 2026-04-22

Phase 1 of the flog cleanup. Summarizes the user-actionable findings from
the four audit reports so the user can quickly adjudicate priority and
gate entry to Phase 2.

## Summary counts

| Scope | A | B | C | D | E | Total |
|---|---|---|---|---|---|---|
| 01-transport | 6 | 1 | 0 | 6 | 2 | 15 |
| 02-domain    | 5 | 2 | 0 | 14 | 4 | 25 |
| 03-ui        | 13 | 0 | 0 | 26 | 1 | 40 |
| 04-flog-dart | 3 | 9 | 0 | 18 | 2 | 32 |
| **Total**    | **27** | **12** | **0** | **64** | **9** | **112** |

**DOM-025 added during Phase 2 Task 2** — see "Addenda" section at the
end of this file.

C = 0 as required: all C-class entries resolved with user in Task 3 and
reclassified into A/B/D/E.

## B-class findings — prioritized (bugs to fix in Phase 3)

12 confirmed bugs. Phase 2.5 writes one `#[ignore = "bug: <id>"]` test per
entry (red/should_panic). Phase 3 resolves each and flips the test to green.

- **[HIGH]** `DOM-003` — HTTP response without prior request should error, but silently does nothing
    location: `src/domain/network_store.rs:108-127`  (from 02-domain.md)
- **[HIGH]** `DART-001` — SSE parser drops all events after the first `data:` line in a chunk
    location: `flog_dart/lib/src/flog_sse_parser.dart:212-242`  (from 04-flog-dart.md)
- **[HIGH]** `DART-002` — Untracked test suite references FlogSseParser.wrapTyped + SseEvent that do not exist
    location: `flog_dart/lib/src/flog_sse_parser.dart:23-243 (API surface)`  (from 04-flog-dart.md)
- **[MEDIUM]** `DOM-018` — search_positions() can return overlapping ranges if OR terms overlap
    location: `src/domain/filter.rs:201-229`  (from 02-domain.md)
- **[MEDIUM]** `DART-004` — FlogMockInterceptor.onRequest runs mock logic even when flogEnabled is false
    location: `flog_dart/lib/src/flog_mock_interceptor.dart:59-95`  (from 04-flog-dart.md)
- **[MEDIUM]** `DART-006` — `stream` is declared a broadcast stream in dartdoc but is a single-subscription map
    location: `flog_dart/lib/src/flog_web_socket.dart:27, 45, 90`  (from 04-flog-dart.md)
- **[MEDIUM]** `DART-008` — _idMap/_startMap leak when an earlier interceptor rejects/resolves before FlogHttpInterceptor runs
    location: `flog_dart/lib/src/flog_http_interceptor.dart:47-48, 64-100, 182-244`  (from 04-flog-dart.md)
- **[LOW]** `TRANS-007` — tcp_open uses Ok(Ok(_)) pattern; logic correct but fragile
    location: `src/transport/device_monitor.rs:560-566`  (from 01-transport.md)
- **[LOW]** `DART-003` — Library docstring documents a top-level `flog()` function that does not exist
    location: `flog_dart/lib/flog_dart.dart:4-13`  (from 04-flog-dart.md)
- **[LOW]** `DART-005` — `ext.flog.syncMockRules` VM Service extension is documented but not registered
    location: `flog_dart/lib/src/flog_mock_interceptor.dart:48, flog_dart/lib/src/flog_server.dart:222-241`  (from 04-flog-dart.md)
- **[LOW]** `DART-007` — _truncate compares char count against a byte-size budget
    location: `flog_dart/lib/src/flog_http_interceptor.dart:258-263`  (from 04-flog-dart.md)
- **[LOW]** `DART-009` — emitNet mutates caller-owned Map with protocol metadata
    location: `flog_dart/lib/src/flog_net.dart:19-24`  (from 04-flog-dart.md)

**Observation**: flog_dart owns 9 of 12 B-class bugs, with 3 HIGH-severity
issues. The most important discovery is **DART-002**: the untracked
`flog_dart/test/` suite references APIs that don't exist in `lib/`, meaning
tests were written against a design that wasn't implemented (or has since
drifted). This must be adjudicated before Phase 2 — decide whether to commit
the tests first (making them the expected behavior) or treat them as stale.

## Phase 3 redesign scope — D-class by module

64 architecture findings grouped by target module to feed Phase 3 step
planning directly.

### Parser layer (src/parser/)
3 findings.

- `DOM-013` — MultiStrategyParser hard-wires parser chain; no way to add/remove parsers without code change  (`src/parser/mod.rs:29-43`)
- `DOM-015` — Parser modules each define their own ANSI stripping logic; not shared  (`src/parser/generic.rs:14-34, src/parser/structured.rs:28-39`)
- `DOM-017` — Keyword parser uses three LazyLock Regex for inference; no priority/weighting  (`src/parser/keyword.rs:12-21`)

### Domain layer (src/domain/)
9 findings.

- `DOM-001` — Three separate enums (StatusFilter, MethodFilter, ProtocolFilter) duplicate identical structure  (`src/domain/network_filter.rs:7-134`)
- `DOM-002` — State machine for FlogNetMessage has no validation of transition order  (`src/domain/network_store.rs:22-35`)
- `DOM-005` — Compiled regex and plain-text parts both live in FilterState with no encapsulation  (`src/domain/filter.rs:14-24`)
- `DOM-006` — FlogNetMessage is loosely-typed struct with optional fields; protocol behavior scattered  (`src/domain/network.rs:194-215`)
- `DOM-008` — Large 693-line file with two responsibilities: JSON tolerant parsing + structured log parsing  (`src/domain/structured_parser.rs:1-693`)
- `DOM-011` — LogStore.add_entry() implements 1-entry ring buffer but no consecutive-dup folding on drain  (`src/domain/store.rs:37-45`)
- `DOM-019` — Three parallel implementations of filter logic (level, tag+search, tag+search) not unified  (`src/domain/network_filter.rs:136-161, src/domain/filter.rs:117-197`)
- `DOM-024` — NetworkEntry factory methods (new_http, new_sse, new_ws) repeat boilerplate  (`src/domain/network.rs:90-117, src/domain/network_store.rs:74-106`)
- `DOM-025` — SseChunk.seq/size/timestamp and WsMessage.timestamp are write-only fields  (`src/domain/network.rs`)  *(discovered during Phase 2 Task 2)*

### Transport layer (src/transport/ + src/input/ + main.rs lifecycle)
6 findings.

- `TRANS-002` — Port cycling magic numbers lack conceptual naming  (`src/transport/adb.rs:6-15`)
- `TRANS-004` — ConnectorHandle leaks message-type specifics; no abstraction for "send downstream message"  (`src/input/connector.rs:28-62`)
- `TRANS-006` — Reader/writer task spawn creates fire-and-forget async tasks with no monitoring  (`src/input/connector.rs:140-164`)
- `TRANS-009` — Cross-platform transport paths are responsibility-asymmetric  (`src/main.rs:240-275`)
- `TRANS-012` — ServerMessage and ClientMessage variants are not validated for completeness  (`src/input/protocol.rs:20-84`)
- `TRANS-014` — ClientInfo struct missing session/identity metadata  (`src/input/protocol.rs:40-56`)

### flog_dart
18 findings.

- `DART-010` — FlogDio class is a 500-line hand-written Dio delegate, yellow on §5.5 file rule  (`flog_dart/lib/src/flog_dio.dart:80-504`)
- `DART-011` — Interceptor ordering is correct but unguarded against user manipulation  (`flog_dart/lib/src/flog_dio.dart:98-120`)
- `DART-012` — FlogMockInterceptor uses a process-wide static rule list  (`flog_dart/lib/src/flog_mock_interceptor.dart:51-57`)
- `DART-013` — Mock match semantics (substring, first-match-wins, case-sensitive) are load-bearing and undocumented  (`flog_dart/lib/src/flog_mock_interceptor.dart:60-95`)
- `DART-014` — Magic string `flog_mocked` in options.extra — concept not extracted  (`flog_dart/lib/src/flog_mock_interceptor.dart:72-73, flog_dart/lib/src/flog_http_interceptor.dart:111`)
- `DART-015` — Port-scan range [basePort, basePort+9] is a magic 10  (`flog_dart/lib/src/flog_server.dart:165-180`)
- `DART-016` — _startServer silently succeeds-without-binding if all 10 ports are taken  (`flog_dart/lib/src/flog_server.dart:165-180`)
- `DART-017` — _handleReplay fires-and-forgets Dio.request with no error surfacing  (`flog_dart/lib/src/flog_server.dart:261-280`)
- `DART-018` — FlogWebSocket.fromChannel and primary constructor duplicate ~40 lines of setup  (`flog_dart/lib/src/flog_web_socket.dart:32-116`)
- `DART-019` — Binary format `<binary: N bytes>` is a magic string repeated in formatter + size  (`flog_dart/lib/src/flog_web_socket.dart:168-188`)
- `DART-020` — FlogStore capacity=50000 is a hardcoded magic constant  (`flog_dart/lib/src/flog_store.dart:24`)
- `DART-021` — `nextNetId` and `emitNet` exported from public API but are internal helpers  (`flog_dart/lib/flog_dart.dart:21`)
- `DART-022` — FlogServer.start's appName/appVersion/packageName parameters are dead on Flog.init path  (`flog_dart/lib/src/flog_server.dart:48-63, flog_dart/lib/flog_dart.dart:46-60`)
- `DART-023` — Flog.init swallows PackageInfo errors silently with empty `.catchError((_) {})`  (`flog_dart/lib/flog_dart.dart:39-61`)
- `DART-024` — README lacks mock rules + replay + removal docs; lists features but not usage  (`flog_dart/README.md:1-48`)
- `DART-025` — CHANGELOG jumps 0.2.0 → 0.7.1, missing five releases of history  (`flog_dart/CHANGELOG.md:1-20`)
- `DART-026` — FlogDio.sse assumes response.data is non-null and crashes on empty-body streams  (`flog_dart/lib/src/flog_dio.dart:137-168`)
- `DART-027` — Mocked-response path duplicates ~30 lines of request-emit logic  (`flog_dart/lib/src/flog_http_interceptor.dart:102-145`)

### App state machine (app.rs)
8 findings.

- `UI-002` — InputField enum mixes log-only and network-only fields, no validation  (`src/app.rs:13-35`)
- `UI-004` — NetworkState.filtered_indices caches with mutable laziness pattern  (`src/app.rs:146-175`)
- `UI-006` — Scroll state identity unclear — auto_scroll flag scattered across two types  (`src/app.rs:145-270 (NetworkState), src/app.rs:372-435 (App), src/ui/logs/mod.rs, src/ui/network/mod.rs`)
- `UI-017` — LayoutCache mixed into App state — separation of concerns issue  (`src/app.rs:372-435 (App struct)`)
- `UI-026` — Mock rule editor detail state fields are scattered across multiple structs  (`src/app.rs lines 1050-1106`)
- `UI-028` — Mock rule state machine transitions unclear (new vs edit, save vs cancel)  (`src/app.rs:1000-1166 (entire mock rule section)`)
- `UI-034` — enter_mock_edit deeply nested, difficult to understand  (`src/app.rs:973-1010 (entire enter_mock_edit section)`)
- `UI-040` — Multi-app connection state (active_app_id, connected_apps, discovered_devices) lacks invariant documentation  (`src/app.rs:395-411`)

### Event dispatch (event.rs)
8 findings.

- `UI-001` — Magic constants for scroll and input should be named constants  (`src/event.rs:1-30`)
- `UI-007` — State machine routing lacks clear guard conditions — keys can silently fail  (`src/event.rs:14-30`)
- `UI-008` — SSE merged mode key handling deeply nested, difficult to test  (`src/event.rs:1342-1413`)
- `UI-009` — Normal mode mouse handling exceeds 700 lines, lacks extraction  (`src/event.rs:35-727`)
- `UI-016` — SSE/WS pill click detection uses magic coordinates without comments  (`src/event.rs lines 295-330, 395-430`)
- `UI-020` — Input field character editing lacks escape handling  (`src/event.rs lines 1528-1547`)
- `UI-024` — Scroll amount hardcoded differently across contexts  (`src/event.rs line 10 (SCROLL_LINES) vs src/app.rs (scroll methods)`)
- `UI-036` — Module documentation (//!) missing or minimal across UI layer  (`src/event.rs:1, src/app.rs:1, src/ui/*/mod.rs`)

### UI Logs view (src/ui/logs/)
3 findings.

- `UI-030` — repeat_bar function uses magic threshold 50 for visual bar width  (`src/ui/logs/mod.rs:107-111`)
- `UI-031` — Color selection logic uses modulo hashing without seeding, prone to palette clash  (`src/ui/logs/mod.rs:40-68 (tag_color, level_color, etc. functions)`)
- `UI-038` — Logs renderer mixes layout, filtering, wrapping, and detail panel logic  (`src/ui/logs/mod.rs:1358 lines`)

### UI Network view (src/ui/network/)
3 findings.

- `UI-021` — mock_rules render module mixes mock rule display with field editing  (`src/ui/network/mock_rules.rs:493 lines`)
- `UI-035` — Network row background colors for error/warning/replay/mocked are magic RGB outside palette  (`src/ui/network/mod.rs:33-36`)
- `UI-037` — Network detail renderer is the largest red-light file and mixes concerns  (`src/ui/network/detail.rs:1109 lines`)

### UI shared components (json_viewer / input_field / text_editor / source_select / tab_bar / help / cli.rs)
4 findings.

- `UI-013` — help.rs duplicates palette constants instead of importing  (`src/ui/help.rs:534 lines`)
- `UI-014` — text_editor and help both eligible for smaller split  (`src/ui/text_editor.rs:503 lines, src/ui/help.rs:534 lines`)
- `UI-015` — input_field component exposes internal render details in public API  (`src/ui/mod.rs:260 lines, src/ui/input_field.rs:285 lines`)
- `UI-033` — Input field rendering uses hardcoded padding and width calculations  (`src/ui/input_field.rs:285 lines`)

### Session (src/session.rs)
2 findings.

- `DOM-021` — Session load/save uses magic u8 constants; level mapping not extracted  (`src/session.rs:27-68`)
- `DOM-022` — Session filter reconstruction from tag_include/exclude is fragile  (`src/session.rs:70-79`)

## Notes for Phase 2 (Mechanical)

E-class total: 9. Distribution:
- 01-transport: 2 (UsbDevice/list_devices dead code; archived replay.rs)
- 02-domain: 4 (including the former C-class DOM-012 — reclassified to E for
  removal of LogStore::append_continuation + Continuation variant)
- 03-ui: 1 (expand_all / collapse_all marked `#[allow(dead_code)]`)
- 04-flog-dart: 2

Phase 2 dispatches 4 subagents in parallel (transport / domain / ui+event /
flog_dart) and each applies only its scope's E-class + clippy whitelist
fixes. Merge order per spec §4.1: transport → domain → flog_dart → ui.

## Notes for Phase 2.5 (Characterization)

From the UI audit's Phase 2.5 testability verdicts:

- `src/event.rs` — **needs TestBackend fallback** for mouse routing (depends
  on LayoutCache coordinates). Keyboard routing can be pure-function
  extracted first.
- `src/app.rs` — **mixed**. State-machine transitions and scroll logic are
  pure-function extractable. MockEdit state and LayoutCache mixing need
  Phase 3 redesign first.
- `src/ui/logs/mod.rs` (logic only) — **pure-function separable**. Viewport
  computation, entry wrapping, format functions are pure.
- `src/ui/network/detail.rs` (logic only) — **pure-function separable**. SSE
  merge, WS chat grouping, JSON fold state are pure; click regions use
  TestBackend.

Phase 2.5 subagents should prefer pure-function testing where possible and
fall back to `ratatui::backend::TestBackend` snapshot testing only for the
event/mouse routing and the per-tab render integration tests.

## Gate check for Phase 2 entry

- [x] Every C-class resolved into A/B/D/E (Task 3 — C count = 0)
- [x] B list reviewed by user
- [x] `flog_dart/test/` disposition decided: **option A**
- [ ] User confirms Phase 2 may begin

## Resolved: DART-002 disposition

Chosen: **option A — commit the untracked `flog_dart/test/` suite as-is.**

The existing tests (BOM stripping, CRLF handling, multi-line `data:` join,
`retry` field, comments, stream-end flush) encode the intended SSE parser
contract. They become the authoritative red test for Phase 2.5, and Phase 3
implements the missing `FlogSseParser.wrapTyped` + `SseEvent` APIs so they
turn green.

Concrete downstream impact:

- **Phase 1 Task 5 commit** — includes `flog_dart/test/` in the staged set,
  so the tests enter git history *before* Phase 2 touches them. They stay
  red on `dart test` (expected — they fail to compile against current lib/).
- **Phase 2** — must not delete or modify `flog_dart/test/`.
- **Phase 2.5 flog_dart subagent** — notes `flog_dart/test/flog_sse_parser_test.dart`
  as pre-existing red tests that double as DART-001 + DART-002's B-class
  tests. No new `#[ignore]`-equivalent is written for those; the `dart test`
  failure itself is the tracking signal.
- **Phase 3 flog_dart step** — DART-001 + DART-002 resolution is simply
  "make `dart test flog_dart/` pass, without changing the test file's
  expectations."

## Addenda

### DOM-025 — discovered Phase 2 Task 2

Full detail in `02-domain.md`. Short version:

- `SseChunk.seq`, `SseChunk.size`, `SseChunk.timestamp`, `WsMessage.timestamp`
  are constructed but never read.
- Originating audit entry DOM-009 mis-identified them as live because of
  sloppy grep matching (different types with same field names are live).
- Phase 2 kept the fields with `#[allow(dead_code)]` markers. Phase 3
  Domain step must decide: prune protocol fields, or wire them into the
  render layer.
