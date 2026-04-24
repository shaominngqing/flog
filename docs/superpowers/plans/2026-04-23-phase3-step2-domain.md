# Phase 3 Step 3.2 — Domain Layer Redesign

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans. Steps use `- [ ]` checkboxes.

**Goal:** Resolve 14 audit entries across `src/domain/` — 2 B (bug fix + un-ignore red tests), 5 A (redesign but preserve behaviour), 9 D (structural redesigns: unified filters, typed protocol messages, split parser, fold-on-drain, write-only field decision, factory builder, FilterState encapsulation).

**Architecture:** Seven redesigns with clean seams between them. Each introduces a new abstraction that makes its scope testable in isolation. Existing Phase 2.5B characterization tests (640 lib + 654 bin unit + 148 app + integration) stay green throughout — any one goes red means the redesign changed behaviour and must be reverted.

**Tech Stack:** Rust 1.x, serde (already in Cargo.toml). No new deps.

**Parent spec:** `docs/superpowers/specs/2026-04-22-project-cleanup-design.md` §5
**Audit source:** `docs/superpowers/audit/02-domain.md` entries DOM-001/002/003/004/005/006/007/008/011/018/019/020/024/025

---

## 1. 旧设计问题

### Bugs (B) — will un-ignore red tests
- **DOM-003**: `NetworkStore::handle_res()` silently drops `Response` messages whose id has no matching `Request`. Data loss, no diagnostic.
- **DOM-018**: `FilterState::search_positions()` returns overlapping ranges when OR-terms overlap (`"the|e"` on `"thee"` → `[0..3, 2..3]`).

### Structural (D)
- **DOM-001**: Three filter enums (`ProtocolFilter`, `MethodFilter`, `StatusFilter`) have identical shape (All/specific variants + `next()` + `label()`) but no shared trait.
- **DOM-002**: `FlogNetMessage` has `t: String` — an untyped type tag. No validation that `res` follows `req` or `chunk` follows a stream start.
- **DOM-005**: `FilterState` publicly exposes `search_regex: Option<Regex>` + `search_plain: Vec<String>` side-by-side. Callers can get them out of sync.
- **DOM-006**: `FlogNetMessage` is a 20-field struct with `Option<T>` on almost every field. Behaviour depends on which `Option` is `Some`.
- **DOM-008**: `src/domain/structured_parser.rs` is 693 lines mixing (a) tolerant JSON parser for log payloads (b) structured log format parser.
- **DOM-011**: `LogStore::add_entry()` drains oldest 10% when full, but does NOT fold consecutive duplicates during/after drain (DOM-011 evidence).
- **DOM-019**: `network_filter.rs` and `filter.rs` both implement match-logic (level filter, tag+search) in parallel. No shared trait/helper.
- **DOM-024**: `NetworkEntry::new_http/new_sse/new_ws` repeat 10+ field initialisers. Same in `NetworkStore::{handle_req, handle_chunk, handle_ws_msg}`.
- **DOM-025**: `SseChunk.seq/size/timestamp` + `WsMessage.timestamp` are write-only (constructed at ingest, never read at render). Either wire into UI or drop.

### Correct-but-ugly (A)
- **DOM-004**: `FilterState` combines level + tag + search + exclude in one struct. Decision: keep as-is because the four dimensions are applied as a single pipeline (`matches(&LogEntry)`) — splitting would require 4× the glue. Lock behaviour via characterization tests (Phase 2.5B already did).
- **DOM-007**: SSE merge / WS chat / mock rules each extend `NetworkEntry` independently. Decision: keep extensions in separate modules (already correct); only document the extension pattern.
- **DOM-020**: `extract_path()` uses naive string search, not a URL parser. Decision: keep — behaviour is correct on every test case, no known bug. Lock via characterization.

---

## 2. 新设计思路

### 2.1 New module: `src/domain/filter_traits.rs`
Small shared-trait module for the three network filter enums (DOM-001).

```rust
pub trait FilterVariant: Sized + Copy + PartialEq {
    fn all() -> Self;                    // "all-match" sentinel
    fn label(&self) -> &'static str;     // UI display
    fn next(&self) -> Self;              // cycle to next variant (for pill click)
    fn variants() -> &'static [Self];    // ordered list for cycling
}
```

Each of `ProtocolFilter`, `MethodFilter`, `StatusFilter` implements `FilterVariant`. Existing `pub fn next(&self)` / `as_str()` / etc. become trait impl methods. **No behaviour change** — same cycling order, same labels.

### 2.2 `NetworkStore::handle_res` fix (DOM-003)
Currently: orphan response silently dropped.
Fix: store orphan entries so DOM-003 red test turns green. Add a dedicated orphan slot or a log-level warning; most minimal fix:

```rust
fn handle_res(&mut self, msg: FlogNetMessage) {
    if let Some(entry) = self.find_by_id_mut(msg.id) {
        // existing update logic
    } else {
        // NEW: create a minimal orphan entry so the response is at least
        // visible in the inspector. Mark it as "orphan" in status.
        self.entries.push_back(NetworkEntry::new_orphan_response(msg));
    }
}
```

Add `NetworkEntry::new_orphan_response(msg) -> NetworkEntry` factory. Add `NetworkStatus::Orphan` variant (or equivalent flag). DOM-003 red test un-ignored.

### 2.3 `FilterState::search_positions` dedup (DOM-018)
After collecting all match positions, sort by start, then **merge overlapping ranges** before returning. Minimal change — one helper fn. Un-ignore both DOM-018 red tests.

### 2.4 FlogNetMessage type-safe variants (DOM-002 + DOM-006 combined)

Current: `FlogNetMessage { t: String, id: u64, method: Option<String>, url: Option<String>, status: Option<u16>, data: Option<String>, ... }` — 20-field loose bag.

New: externally-tagged enum (serde handles this):

```rust
#[derive(Debug, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum FlogNetKind {
    Req { id: u64, p: String, method: String, url: String, /* ... */ },
    Res { id: u64, status: u16, /* ... */ },
    Chunk { id: u64, data: String, seq: Option<u32>, /* ... */ },
    WsSend { id: u64, data: String, /* ... */ },
    WsRecv { id: u64, data: String, /* ... */ },
    WsClose { id: u64, code: Option<u16>, reason: Option<String> },
}
```

**IMPORTANT — protocol compat**: spec §5.8 red line says "不改 ClientMessage / ServerMessage WS 协议字段". But DOM-002/006 are about the **internal** `FlogNetMessage` struct, which is deserialized from Dart's JSON. The wire format (field names, optionality) must stay identical — and since serde deserialization is the only caller, we verify by:
1. Round-trip every fixture `FlogNetMessage` JSON (from existing tests)
2. Every test that constructs a `FlogNetMessage` compiles against the new enum (the struct literals change shape — this is an internal refactor, not a protocol change)

Consumer code (`NetworkStore::handle_*`) matches on the variant instead of checking `msg.t == "req"` + Option unwraps.

### 2.5 `FilterState` encapsulation (DOM-005)
Today: `pub search_regex: Option<Regex>, pub search_plain: Vec<String>`. Make both `pub(crate)` or private; expose `pub fn search_matches(text: &str) -> bool`. Callers that need `search_positions()` use that; no one outside needs the internals.

### 2.6 Structured parser split (DOM-008)
`src/domain/structured_parser.rs` (693 lines) → two files:
- `src/domain/json_tolerant.rs` — tolerant JSON parser (currently lines roughly 300-693 per structure)
- `src/domain/structured_parser.rs` kept — structured log format parser only (lines 1-300)

Both keep existing pub-surface. Tests in each file move with their code. **The 693-line file was in the yellow zone per §5.5 — splitting moves both halves to green.**

### 2.7 LogStore fold-on-drain (DOM-011)
Current `add_entry`: on capacity drain, drops oldest 10% without folding. If a loop emits 100K identical entries, drain keeps 90K uncompressed.

Fix: when drain happens, run the consecutive-duplicate fold across the drained remainder. Add one test: seed 200K identical entries, verify final len < 100K and fold ran.

### 2.8 Shared filter matcher (DOM-019)
Extract a trait `MessageFilter<T>` with `matches(&self, item: &T) -> bool`. `FilterState` implements for `LogEntry`; `NetworkFilter` implements for `NetworkEntry`. Today they're structurally parallel; the trait formalises it so future additions share shape.

### 2.9 NetworkEntry builder (DOM-024)
Extract `NetworkEntryBuilder` or use a single `fn new(id, protocol)` + chained setters. Reduces 3× `new_*` factories to one builder. Internal refactor, pub API unchanged.

### 2.10 SseChunk/WsMessage write-only fields (DOM-025)
Decision per audit proposed_action option 1: **prune write-only fields**.
- `SseChunk.seq` — delete (never read)
- `SseChunk.size` — delete (never read)
- `SseChunk.timestamp` — delete (never read)
- `WsMessage.timestamp` — delete (never read)
- `WsMessage.size` — KEEP (read in event.rs:1093, detail.rs:642/790, network.rs:166)

**Protocol impact**: spec §5.8 red line is about `ClientMessage`/`ServerMessage` on-the-wire format. `SseChunk`/`WsMessage` are internal storage. The wire-level `FlogNetMessage::Chunk` variant already has these as optional — dropping them from internal storage doesn't affect the wire format (serde just drops unknown fields as they're stripped). The field deletion is a storage-shape change, not a protocol change.

Keep the `#[serde(default)]` on `FlogNetMessage::Chunk.seq/size/timestamp` so Dart-side can still send them without deserialization error; they're just discarded at ingest.

Verify: run integration tests that Dart `FlogNetMessage` JSON still deserializes.

---

## 3. 迁移策略

10 tasks in dependency order. Each commits individually.

| Task | Scope | Commits |
|---|---|---|
| 0 | Pre-flight baseline | — |
| 1 | DOM-003 B-fix + un-ignore red test | 1 |
| 2 | DOM-018 B-fix + un-ignore 2 red tests | 1 |
| 3 | DOM-001 FilterVariant trait | 1 |
| 4 | DOM-008 split structured_parser.rs → json_tolerant.rs | 1 |
| 5 | DOM-025 prune write-only fields | 1 |
| 6 | DOM-024 NetworkEntry builder | 1 |
| 7 | DOM-002+006 FlogNetKind typed enum | 1 |
| 8 | DOM-011 fold-on-drain | 1 |
| 9 | DOM-005 FilterState encapsulation + DOM-019 shared trait | 1 |
| 10 | DOM-004/007/020 A entries — acknowledge + journal | 1 |

After every task: `cargo test` all green + `clippy -D warnings` + `fmt`. Red → stop.

**Characterization test invariant**: all 148 (app_state) + 107 (event_keys) + 108 (event_mouse) + 12 (input) + 84 (ui_logs) + 128 (ui_network) + 53 (ui_source_select_help) + 1 (ws_server) integration tests and all in-file parser/domain `#[cfg(test)]` tests MUST stay green.

### What MUST NOT change
- Wire-level `FlogNetMessage` JSON format (field names + optional-ness).
- `NetworkEntry` public read-side API (callers in UI/event read many fields).
- `FilterState::matches(&LogEntry) -> bool` semantics.
- `LogStore::add_entry` semantics (same level, same tag, same body → fold).

### What MAY change
- Internal struct layouts.
- Private method names.
- Non-public field visibility.
- Test structure (may re-group tests when files split).

---

## 4. File Structure

**New files**:
- `src/domain/filter_traits.rs` (~50 lines) — `FilterVariant` trait
- `src/domain/json_tolerant.rs` (~400 lines, split from structured_parser.rs)

**Modified**:
- `src/domain/network_filter.rs` — 3 enums implement FilterVariant (DOM-001)
- `src/domain/network_store.rs` — handle_res orphan fix (DOM-003)
- `src/domain/filter.rs` — search_positions merge + FilterState encapsulation + MessageFilter trait (DOM-018 + 005 + 019)
- `src/domain/network.rs` — FlogNetKind enum + NetworkEntryBuilder + field pruning (DOM-002 + 006 + 024 + 025)
- `src/domain/structured_parser.rs` — becomes smaller after DOM-008 split
- `src/domain/store.rs` — fold-on-drain (DOM-011)
- `src/domain/mod.rs` — re-exports for new files

**No changes**: `src/domain/entry.rs`, `src/domain/mock.rs`, `src/domain/sse_merge.rs`, `src/domain/ws_chat.rs` (A entries acknowledged, not touched).

---

## 5. Tasks

### Task 0: Pre-flight

- [ ] **Run**:
```bash
git log --oneline -1   # expect: b0429d6 Phase 3 Step 3.1 journal
cargo test 2>&1 | grep "test result:"   # expect all green
cargo clippy --all-targets -- -D warnings 2>&1 | tail -2
cargo fmt --check && echo fmt clean
cargo llvm-cov --summary-only 2>&1 | grep domain/ > /tmp/phase3-step2-pre.txt
```

### Task 1: DOM-003 — orphan response handling (B-fix)

**Files**: `src/domain/network.rs`, `src/domain/network_store.rs`, `tests/characterization_bugs.rs`

**Steps**:

- [ ] **1.1** Read `NetworkStatus` enum in `src/domain/network.rs`. Add variant `Orphan` (represents "response arrived without matching request").

- [ ] **1.2** Add factory in `src/domain/network.rs`:
```rust
impl NetworkEntry {
    /// Create a placeholder entry for a Response message whose id
    /// has no matching Request. Phase 3 DOM-003.
    pub fn new_orphan_response(id: u64, status_code: Option<u16>, res_body: Option<String>) -> Self {
        NetworkEntry {
            id,
            protocol: Protocol::Http,  // assume HTTP; unknown
            status: NetworkStatus::Orphan,
            method: "?".into(),
            url: "<orphan response>".into(),
            path: "<orphan>".into(),
            http_status: status_code,
            // ... rest with sensible defaults
        }
    }
}
```

Use Read first to get the exact `NetworkEntry` struct shape (Phase 2.5B fixtures have it).

- [ ] **1.3** Modify `NetworkStore::handle_res` in `src/domain/network_store.rs`. Replace the silent-drop else-arm with:
```rust
} else {
    // DOM-003 fix: orphan Response, no matching Request. Push as a visible
    // entry so the user sees the data arrived.
    let entry = NetworkEntry::new_orphan_response(
        msg.id,
        msg.status,
        msg.data.clone(),
    );
    self.entries.push_back(entry);
}
```

- [ ] **1.4** Un-ignore the red test in `tests/characterization_bugs.rs`:
```rust
// Remove: #[ignore = "bug: DOM-003, fix in Phase 3"]
#[test]
fn dom_003_response_without_request_should_not_drop_silently() { ... }
```

Test currently asserts "store non-empty after orphan response" — which the fix satisfies.

- [ ] **1.5** Verify:
```bash
cargo test dom_003 2>&1 | grep "test result"
cargo test 2>&1 | grep "test result:"
cargo clippy --all-targets -- -D warnings 2>&1 | tail -2
cargo fmt && cargo fmt --check
```
Expect: dom_003 test now green (was ignored red); overall test count +1 green −1 ignored.

- [ ] **1.6** Commit:
```
fix(domain): DOM-003 orphan response handling (Phase 3 B-fix)
```

### Task 2: DOM-018 — search_positions non-overlap (B-fix)

**Files**: `src/domain/filter.rs`, `tests/characterization_bugs.rs`

**Steps**:

- [ ] **2.1** Read `FilterState::search_positions` in `src/domain/filter.rs`. Locate the final `return positions;` line.

- [ ] **2.2** Before return, merge overlapping ranges. Add private helper:
```rust
/// Merge overlapping ranges in a sorted-by-start Vec. Phase 3 DOM-018.
fn merge_overlapping_ranges(mut ranges: Vec<std::ops::Range<usize>>) -> Vec<std::ops::Range<usize>> {
    if ranges.len() <= 1 {
        return ranges;
    }
    ranges.sort_by_key(|r| r.start);
    let mut merged = Vec::with_capacity(ranges.len());
    let mut cur = ranges[0].clone();
    for r in ranges.into_iter().skip(1) {
        if r.start <= cur.end {
            // Overlap or touch → extend cur.
            cur.end = cur.end.max(r.end);
        } else {
            merged.push(cur);
            cur = r;
        }
    }
    merged.push(cur);
    merged
}
```

Apply at end of `search_positions`: `merge_overlapping_ranges(positions)`.

- [ ] **2.3** Un-ignore both red tests in `tests/characterization_bugs.rs`:
- `dom_018_search_positions_no_overlapping_ranges`
- `dom_018_search_positions_overlapping_or_terms_on_shared_text`

- [ ] **2.4** Verify cargo test green + clippy + fmt.

- [ ] **2.5** Commit:
```
fix(domain/filter): DOM-018 merge overlapping search_positions (Phase 3 B-fix)
```

### Task 3: DOM-001 — FilterVariant trait

**Files**: `src/domain/filter_traits.rs` (new), `src/domain/network_filter.rs`, `src/domain/mod.rs`

**Steps**:

- [ ] **3.1** Create `src/domain/filter_traits.rs` with the trait (per Section 2.1).

- [ ] **3.2** Register `pub mod filter_traits;` in `src/domain/mod.rs`.

- [ ] **3.3** Read `src/domain/network_filter.rs`. For each of `ProtocolFilter`, `MethodFilter`, `StatusFilter`:
- Keep the existing enum variants unchanged.
- Convert existing `pub fn next(&self) / label(&self) / all()` methods into trait impl `impl FilterVariant for ProtocolFilter { ... }`. Keep the same bodies.
- If both a free-function `next()` and a trait method exist, keep trait method and remove free-function (or make it delegate).

- [ ] **3.4** Add 3 unit tests (one per enum) asserting `FilterVariant::all()` + `next()` cycles produce the same sequence the old free-fns produced.

- [ ] **3.5** Verify cargo test green.

- [ ] **3.6** Commit:
```
refactor(domain/network_filter): FilterVariant trait unifies 3 filter enums (Phase 3 DOM-001)
```

### Task 4: DOM-008 — split structured_parser.rs

**Files**: `src/domain/structured_parser.rs` (shrinks), `src/domain/json_tolerant.rs` (new), `src/domain/mod.rs`

**Steps**:

- [ ] **4.1** Read `src/domain/structured_parser.rs` fully. Identify:
- Section A: tolerant JSON parser (roughly the second half — search for `fn parse_tolerant_json` or the function name).
- Section B: structured log format parser (first half — `try_parse`, bracket/pipe handlers).

- [ ] **4.2** Create `src/domain/json_tolerant.rs`. Move section A code verbatim:
- Function definitions
- Any supporting constants / types
- `#[cfg(test)] mod tests` that test JSON parsing specifically

Update imports: add `use` statements for types from parent module.

- [ ] **4.3** Remove section A from `structured_parser.rs`. Add `use super::json_tolerant::*;` for any JSON-parser callers that remain in structured_parser.

- [ ] **4.4** Register `pub mod json_tolerant;` in `src/domain/mod.rs`.

- [ ] **4.5** Verify:
- `cargo build` succeeds
- `wc -l src/domain/structured_parser.rs src/domain/json_tolerant.rs` — each < 500
- `cargo test` all green (characterization tests for tolerant JSON should still pass wherever they are)
- clippy + fmt

- [ ] **4.6** Commit:
```
refactor(domain): split structured_parser.rs → json_tolerant.rs (Phase 3 DOM-008)
```

### Task 5: DOM-025 — prune write-only fields

**Files**: `src/domain/network.rs`, `src/domain/network_store.rs`, anywhere else that constructs `SseChunk`/`WsMessage`

**Steps**:

- [ ] **5.1** Read `src/domain/network.rs`. Confirm `SseChunk.seq`/`size`/`timestamp` + `WsMessage.timestamp` are the write-only set (DOM-025 audit verified this).

- [ ] **5.2** Grep to confirm zero read sites:
```bash
grep -rn "\.timestamp\b" src/ | grep -v "LogEntry\|NetworkEntry" | head
grep -rn "chunk\.seq\|\.seq" src/ | grep -i "sse\|chunk"
grep -rn "chunk\.size" src/
```
If any read sites appear, STOP and report.

- [ ] **5.3** Remove the four fields from `SseChunk` and `WsMessage` struct definitions. Remove corresponding `#[allow(dead_code)]` attributes.

- [ ] **5.4** Remove assignments to those fields from `NetworkStore::handle_chunk` / `handle_ws_send` / `handle_ws_recv`.

- [ ] **5.5** Update any tests that construct `SseChunk`/`WsMessage` with the removed fields. Use Grep `SseChunk {` and `WsMessage {`.

- [ ] **5.6** `FlogNetMessage` wire-level: keep `#[serde(default)]` on `seq/size/timestamp` so Dart clients can still send them (just silently dropped at ingest). Don't remove the wire fields — that would break protocol compat (spec §5.8).

- [ ] **5.7** Update DOM-025 audit entry resolution (edit `docs/superpowers/audit/02-domain.md` — change proposed_action "Phase 3 Domain step decide" to "Phase 3 Step 3.2 applied option 1 — fields pruned from storage, kept on wire for compat").

- [ ] **5.8** Verify cargo test green + clippy + fmt.

- [ ] **5.9** Commit:
```
refactor(domain): prune write-only SseChunk/WsMessage fields (Phase 3 DOM-025)
```

### Task 6: DOM-024 — NetworkEntry builder

**Files**: `src/domain/network.rs`

**Steps**:

- [ ] **6.1** Read the three current factories: `NetworkEntry::new_http/new_sse/new_ws`. Identify common fields (probably all of them set defaults except protocol/method).

- [ ] **6.2** Add builder:
```rust
impl NetworkEntry {
    pub fn builder(id: u64, url: impl Into<String>) -> NetworkEntryBuilder { ... }
}

pub struct NetworkEntryBuilder {
    entry: NetworkEntry,
}

impl NetworkEntryBuilder {
    pub fn http(mut self, method: impl Into<String>) -> Self { ... }
    pub fn sse(mut self, method: impl Into<String>) -> Self { ... }
    pub fn ws(mut self, method: impl Into<String>) -> Self { ... }
    pub fn build(self) -> NetworkEntry { self.entry }
}
```

- [ ] **6.3** Keep existing `new_http/new_sse/new_ws` as thin wrappers around the builder (for back-compat with tests and `NetworkStore::handle_*`). These can be marked `#[doc(hidden)]` if desired; no need to delete.

- [ ] **6.4** Add 3 unit tests covering builder.http/sse/ws → assert correct `Protocol`/`method`/defaults.

- [ ] **6.5** Verify cargo test green.

- [ ] **6.6** Commit:
```
refactor(domain/network): NetworkEntry builder reduces factory boilerplate (Phase 3 DOM-024)
```

### Task 7: DOM-002 + DOM-006 — FlogNetKind typed enum

**Files**: `src/domain/network.rs`, `src/domain/network_store.rs`, `src/input/protocol.rs` (consumer)

**IMPORTANT**: this is the biggest task. Do it last of the redesigns so the new abstractions (builder from Task 6, orphan from Task 1, filters from Task 3) are already in place.

**Steps**:

- [ ] **7.1** Read current `FlogNetMessage` struct + all call sites (grep). Build a table: every `msg.t == "X"` variant → which fields it uses.

- [ ] **7.2** Introduce `FlogNetKind` enum (per Section 2.4) AS A NEW TYPE. Do NOT remove `FlogNetMessage` yet.
```rust
#[derive(Debug, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum FlogNetKind {
    Req { id: u64, p: String, method: String, url: String, /* query, headers, body */ },
    Res { id: u64, status: u16, /* duration, size, headers, body */ },
    Chunk { id: u64, data: String, /* seq/size/timestamp still #[serde(default)] */ },
    WsSend { id: u64, data: String, /* size */ },
    WsRecv { id: u64, data: String, /* size */ },
    WsClose { id: u64, code: Option<u16>, reason: Option<String> },
}
```

Use Read on `src/input/protocol.rs` to see how `FlogNetMessage` is currently embedded in `ClientMessage::Net`. Match the wire format EXACTLY.

- [ ] **7.3** Add deserialize-round-trip test asserting `serde_json::from_str::<FlogNetKind>(raw_req_json)` succeeds for each variant with fixture JSON from Phase 2.5B.

- [ ] **7.4** Replace `ClientMessage::Net { msg: FlogNetMessage }` with `ClientMessage::Net { msg: FlogNetKind }` in `src/input/protocol.rs`. The wire format `{"type":"net","t":"req",...}` unchanged — serde handles the tagged enum identically.

- [ ] **7.5** Refactor `NetworkStore::handle_*` consumers from `if msg.t == "req" { ... }` chains to `match msg { FlogNetKind::Req { id, ... } => ... }`.

- [ ] **7.6** Delete the old `FlogNetMessage` struct. Its 20 optional fields are replaced by the typed variants.

- [ ] **7.7** Verify:
- `cargo test` — all green (especially every network_store + protocol test)
- tests/support/fixtures.rs may need updating if it constructs FlogNetMessage directly
- clippy + fmt

- [ ] **7.8** Commit:
```
refactor(domain): FlogNetKind typed enum replaces loose FlogNetMessage (Phase 3 DOM-002 + DOM-006)
```

### Task 8: DOM-011 — fold-on-drain

**Files**: `src/domain/store.rs`

**Steps**:

- [ ] **8.1** Read `LogStore::add_entry`. Locate the drain path (`if self.entries.len() >= CAP { ... drain oldest 10% }`).

- [ ] **8.2** After drain, run consecutive-dup fold across the remaining entries:
```rust
fn fold_consecutive_duplicates(entries: &mut VecDeque<LogEntry>) {
    // Walk entries; when entry[i+1] has same level/tag/message as entry[i],
    // increment entry[i].repeat_count and remove entry[i+1].
    let mut i = 0;
    while i + 1 < entries.len() {
        if entries[i].same_signature(&entries[i + 1]) {
            entries[i].repeat_count += 1;
            entries.remove(i + 1);
        } else {
            i += 1;
        }
    }
}
```

Call from `add_entry` after drain.

- [ ] **8.3** Add `LogEntry::same_signature(&self, other: &Self) -> bool` helper if not present. Signature = (level, tag, message).

- [ ] **8.4** Add unit tests:
- Seed 200K identical entries → final len < 100K AND all folded into one (or few) entries with high repeat_count.
- Seed 200K mixed entries → no improper folding.

- [ ] **8.5** Verify cargo test green.

- [ ] **8.6** Commit:
```
fix(domain/store): fold consecutive duplicates on drain (Phase 3 DOM-011)
```

### Task 9: DOM-005 + DOM-019 — FilterState encapsulation + MessageFilter trait

**Files**: `src/domain/filter.rs`, `src/domain/network_filter.rs`

**Steps**:

- [ ] **9.1** `DOM-005`: in `src/domain/filter.rs`, change `pub search_regex: Option<Regex>` + `pub search_plain: Vec<String>` to `pub(crate)`. Verify no outside-crate consumer (grep `src/`).

- [ ] **9.2** `DOM-019`: define trait:
```rust
pub trait MessageFilter<T> {
    fn matches(&self, item: &T) -> bool;
}

impl MessageFilter<LogEntry> for FilterState { ... }
impl MessageFilter<NetworkEntry> for NetworkFilter { ... }
```

Put trait in `src/domain/filter_traits.rs` (same file as FilterVariant trait from Task 3).

- [ ] **9.3** Convert existing inherent `matches` methods on both types to `impl MessageFilter`. Existing `pub fn matches(...)` can stay as inherent wrappers OR be removed (trait provides them).

- [ ] **9.4** Add 2 tests: one feeding `&LogEntry` through `FilterState::matches`, one feeding `&NetworkEntry` through `NetworkFilter::matches`. These are tautological but lock the trait impl shape.

- [ ] **9.5** Verify cargo test green + clippy + fmt.

- [ ] **9.6** Commit:
```
refactor(domain): MessageFilter trait + FilterState encapsulation (Phase 3 DOM-005 + DOM-019)
```

### Task 10: DOM-004 + DOM-007 + DOM-020 A-class acknowledgements + journal

**Files**: `src/domain/filter.rs`, `src/domain/sse_merge.rs`, `src/domain/ws_chat.rs`, `src/domain/mock.rs`, `src/domain/network.rs`, journal.

**Steps**:

- [ ] **10.1** Add module-level `//!` doc comment to each of filter.rs, sse_merge.rs, ws_chat.rs, mock.rs pointing to the audit entry being acknowledged + why it stays (per Section 1 decisions).

- [ ] **10.2** Add one-line comment above `extract_path` in network.rs:
```rust
// DOM-020 acknowledged — naive string search, behaviour correct on all
// known inputs. Locked by characterization tests. Revisit if URL parsing
// requirements change.
```

- [ ] **10.3** Write step journal `docs/superpowers/journal/phase3-step2.md` with the Phase 3 template (入口, 实际变更, 新抽象职责, 测试 delta, 出口 verdict, 意外发现, 移交 Step 3.3).

- [ ] **10.4** Run final verification:
```bash
cargo test 2>&1 | grep "test result:"
cargo clippy --all-targets -- -D warnings 2>&1 | tail -2
cargo fmt --check
wc -l src/domain/*.rs   # all < 500
cargo llvm-cov --summary-only 2>&1 | grep domain/
```

- [ ] **10.5** Commit:
```
docs(journal): Phase 3 Step 3.2 — domain layer redesign complete
```

---

## 6. Exit gates (spec §5.4 (g))

- [ ] All A characterization tests green
- [ ] All B tests un-ignored and green (DOM-003 + DOM-018 × 2 = 3 tests)
- [ ] All D characterization tests green
- [ ] New structural tests added per redesign
- [ ] All `src/domain/*.rs` < 500 lines (split guaranteed by Task 4)
- [ ] Every new abstraction has a one-sentence responsibility (in journal)
- [ ] Coverage not regressed

---

## 7. Self-review

**Spec §5 coverage**:
- §5.1 (1) tests green — every task verification step
- §5.1 (2) A tests not red — all tasks
- §5.1 (3) B tests un-ignore — Tasks 1, 2
- §5.1 (4) new test per task — Tasks 1, 2, 3, 4, 6, 7, 8, 9
- §5.1 (5) one module at a time — scope is src/domain/ only (with necessary consumer updates)
- §5.1 (6) design-first — Sections 1–3
- §5.2 audit source — 14 entries mapped
- §5.4 (g) exit gate — Section 6
- §5.5 line rule — structured_parser split guarantees
- §5.8 red line — FlogNetKind change preserves wire format (Task 7.2/7.4 cross-checks)

**Placeholder scan**: fn signatures shown; ellipses in plan body refer to "read-current-shape-then-fill" — acceptable per step instruction to use Read first. No implement-later / TBD / add-validation patterns.

**Type consistency**: `NetworkEntry`, `FlogNetKind` variant names, `FilterVariant`/`MessageFilter` trait names consistent across tasks.
