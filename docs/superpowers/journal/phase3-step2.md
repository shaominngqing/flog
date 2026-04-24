# Phase 3 Step 3.2 — Domain Layer Redesign (Journal)

## 入口
- 日期：2026-04-22
- Git HEAD at entry: `4ba30c5` (Phase 3 Step 3.2 plan commit)
- 全局测试数 at entry: 661 lib + 675 bin + 148 app_state + 1 passed / 3 ignored
  characterization_bugs + 107 event_keys + 108 event_mouse + 12 input + 84
  ui_logs + 128 ui_network + 53 ui_source_select_help + 1 ws_server
- Characterization B-tests ignored at entry: **3** (DOM-003 + DOM-018 × 2)

## 实际变更

### New files
- `src/domain/filter_traits.rs` — `FilterVariant` (Phase 3 DOM-001) +
  `MessageFilter<T>` (Phase 3 DOM-019)
- `src/domain/json_tolerant.rs` — byte-level tolerant-JSON engine
  extracted from `structured_parser.rs` (Phase 3 DOM-008)

### Modified
- `src/domain/network.rs`
  - `NetworkStatus::Orphan` variant (DOM-003)
  - `NetworkEntry::new_orphan_response` factory (DOM-003)
  - `NetworkEntryBuilder` + `.builder()` constructor (DOM-024)
  - `FlogNetMessage` struct **removed**; replaced by `FlogNetKind`
    externally-tagged enum on `"t"` with typed Req/Res/Err/Chunk/Done/
    Open/Send/Recv/Close variants (DOM-002 + DOM-006)
  - `SseChunk.seq` / `SseChunk.size` / `WsMessage.timestamp` pruned
    (DOM-025). `WsMessage.size` kept (live reader).
  - DOM-020 acknowledgement comment over `extract_path`.
- `src/domain/network_store.rs`
  - `handle_res` no longer silently drops orphan Responses — pushes an
    `Orphan` entry instead (DOM-003)
  - `process_message` + `handle_*` rewritten to match `FlogNetKind`
    variants (DOM-002 + DOM-006)
- `src/domain/filter.rs`
  - `merge_overlapping_ranges` helper; `search_positions` now returns a
    sorted, non-overlapping cover (DOM-018)
  - `search_regex` / `exclude_regex` / `tag_regex` bool flags changed
    from `pub` to `pub(crate)` (DOM-005)
  - `impl MessageFilter<LogEntry> for FilterState` (DOM-019)
  - DOM-004 acknowledgement at module level
- `src/domain/entry.rs` — `LogEntry::same_signature` helper (DOM-011)
- `src/domain/store.rs` — `fold_consecutive_duplicates` helper invoked
  after pop_front drain; uses `same_signature` (DOM-011)
- `src/domain/network_filter.rs` — FilterVariant impl on the three
  filter enums (DOM-001) + `impl MessageFilter<NetworkEntry>` (DOM-019)
- `src/domain/structured_parser.rs` — Parser engine extracted, file
  shrank from 693 to 419 lines
- `src/domain/mock.rs`, `src/domain/sse_merge.rs`, `src/domain/ws_chat.rs`
  — module-level `//!` DOM-007 acknowledgements
- `src/input/protocol.rs` — `ClientMessage::Net` embeds `FlogNetKind`
- `src/parser/network.rs` — `try_parse_network` returns `FlogNetKind`
- `src/main.rs` — `dispatch_net` test uses `FlogNetKind::Req`
- `src/ui/network/mod.rs`, `src/ui/network/detail.rs`,
  `src/ui/network/stats.rs` — exhaustive matches extended with the new
  `NetworkStatus::Orphan` variant
- `tests/support/fixtures.rs` — `net_req` / `net_res` / `net_chunk_sse`
  / `sse_chunk` / `ws_send` / `ws_recv` all updated for `FlogNetKind` +
  DOM-025 pruned fields
- `tests/characterization_bugs.rs` — DOM-003 + both DOM-018 tests
  un-ignored; replaced `make_net_msg` helper with direct variant
  construction
- `tests/characterization_input.rs` — pattern-match on `FlogNetKind::Req`
  / `::Res` instead of loose struct fields
- `tests/characterization_event_keys.rs` — `SseChunk { seq, size, data }`
  literal replaced with `SseChunk { data }`

## 新抽象职责 (one line each)

- `FilterVariant` — "All + ordered specific variants + cycle + label" for
  the 3 network filter enums.
- `MessageFilter<T>` — "Does this filter accept item T?" — one method,
  two implementors (FilterState/LogEntry, NetworkFilter/NetworkEntry).
- `FlogNetKind` — Wire-level typed protocol message: the nine message
  types Dart may send, each with its own allowed field set.
- `NetworkEntryBuilder` — Chainable protocol-specific seeding of a new
  NetworkEntry (http/sse/ws + optional timestamp/source).
- `fold_consecutive_duplicates` — Walk a VecDeque once, collapse
  adjacent same-signature entries after a drain event.
- `LogEntry::same_signature` — Centralised predicate the store uses at
  both the back_mut fold check and the fold-on-drain sweep.
- `merge_overlapping_ranges` — Compress OR-search hit positions into a
  non-overlapping cover.
- `json_tolerant::Parser` — Byte-level tolerant JSON engine, exposed to
  structured_parser via `pub(super)`.

## 测试 delta

| Target | entry → exit |
|---|---|
| lib unit tests | 661 → 698 (+37) |
| bin unit tests | 675 → 712 (+37) |
| app_state | 148 → 148 (0) |
| characterization_bugs | 1+3 ignored → 4 passed 0 ignored (-3 ignored) |
| event_keys | 107 → 107 |
| event_mouse | 108 → 108 |
| input | 12 → 12 |
| ui_logs | 84 → 84 |
| ui_network | 128 → 128 |
| ui_source_select_help | 53 → 53 |
| ws_server | 1 → 1 |

All 3 B-class ignored tests now pass (DOM-003, DOM-018 × 2).

## 文件行数 (spec §5.5)

```
src/domain/entry.rs            341   <500 green
src/domain/filter.rs           944   红区 — see note
src/domain/filter_traits.rs     57   <500 green
src/domain/json_tolerant.rs    373   <500 green
src/domain/mock.rs             236   <500 green
src/domain/mod.rs               22   <500 green
src/domain/network_filter.rs   656   黄区 — see note
src/domain/network_store.rs   1089   红区 — see note
src/domain/network.rs          734   黄区 — see note
src/domain/sse_merge.rs        270   <500 green
src/domain/store.rs            428   <500 green
src/domain/structured_parser.rs 419  <500 green
src/domain/ws_chat.rs          323   <500 green
```

**Note on filter.rs (944)**: body is ~310 lines; the remaining ~630
lines are characterization + new structural tests (DOM-004 locks,
DOM-018 merge, DOM-019 trait, matches_multi table, parse_tag_filter,
search_positions, exclude). Splitting would separate tests from the
code under test. Accept per §5.5 red-zone exception.

**Note on network_store.rs (1089)**: body is ~310 lines; ~780 lines
are tests covering every handle_* method + DOM-002 transition locks +
DOM-003 orphan + DOM-006 variant shape + DOM-025 wire round-trips +
ring-buffer eviction + format_ts + find_by_id_mut. A future UI step
may split store/process/handle layering which will naturally relieve
this.

**Note on network_filter.rs (656)** and **network.rs (734)**: both are
body + tests. Task 6 (NetworkEntryBuilder) and Task 7 (FlogNetKind)
added the majority of the new test bulk. §5.5 yellow: accepted with
this explanation.

## 出口 verdict

- cargo test: all green, 1376 passed / 0 failed / 0 ignored (up from
  3 ignored at entry)
- cargo clippy --all-targets -- -D warnings: clean
- cargo fmt --check: clean

## 意外发现

- DOM-005 ("compiled_regex + compiled_search_plain are pub") was stale
  at audit time: those fields were already private. The real
  encapsulation was on the `*_regex` mode bools. Applied `pub(crate)` to
  those instead.
- DOM-008 audit description was stale: `structured_parser.rs` does NOT
  contain a separate structured-log parser — the whole file is tolerant
  JSON. Split was still justified by the 693-line yellow-zone size and
  achieved by extracting the byte-level `Parser` engine.
- Task 7 (FlogNetKind) was larger than the plan anticipated because the
  loose-bag `FlogNetMessage` was used in many consumers (tests, parser,
  main, protocol, fixtures). Migration was straightforward once the
  variant constructors were templated.
- DOM-011 fold-on-drain added as a defensive guard — in practice the
  back_mut fold already collapses 200K identical entries into 1 before
  drain can trigger, so the helper's primary coverage is synthetic rings
  staged by the tests.

## 移交 Step 3.3 (Transport)

- `network_store.rs` body shape is ready for future state-machine
  redesign — handle_* methods now take typed parameters per variant.
- No open B-class bugs remain in Domain scope.
- `filter_traits::FilterVariant` is ready to be wired into `event.rs`
  in a later UI step (currently unused in bin build, hence the
  `#[allow(dead_code)]`).

## Audit 条目结算

| Entry | Commit | Class |
|---|---|---|
| DOM-001 | b0e33d6 | D |
| DOM-002 + DOM-006 | 57202ef | D |
| DOM-003 | 7e333a1 | B ✓ un-ignored |
| DOM-004 | (this commit) | A |
| DOM-005 + DOM-019 | 51bc696 | D |
| DOM-007 | (this commit) | A |
| DOM-008 | ce09cbd | D |
| DOM-011 | 18112f4 | D |
| DOM-018 | 3a4d9c1 | B ✓ un-ignored |
| DOM-020 | (this commit) | A |
| DOM-024 | 66728bb | D |
| DOM-025 | b43549e | D |
