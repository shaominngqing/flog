# Audit 02 — Domain Layer + Parser

Scope: src/domain/ (all), src/parser/ (all), src/session.rs.

Auditor: Phase 1 Subagent 2 (read-only)
Date: 2026-04-22

## Findings

```yaml
id: DOM-001
label: D
location: src/domain/network_filter.rs:7-134
title: Three separate enums (StatusFilter, MethodFilter, ProtocolFilter) duplicate identical structure
evidence: |
  pub enum StatusFilter {
      All, Pending, Active, Completed, Failed,
  }
  pub enum MethodFilter {
      All, Get, Post, Put, Delete, Patch,
  }
  pub enum ProtocolFilter {
      All, Http, Sse, Ws,
  }
  
  Each implements .matches() and .next() with identical patterns.
  No shared abstraction; code is copy-paste repeated 3 times.
proposed_action: |
  Design a generic FilterOption<T> trait:
  - impl trait FilterOption { fn matches(...), fn as_str(), fn next() }
  - Create enum wrappers: enum NetworkFilterType { Status(...), Method(...), Protocol(...) }
  - Centralizes match logic and reduces 200 lines → 100
risk: low
```

```yaml
id: DOM-002
label: D
location: src/domain/network_store.rs:22-35
title: State machine for FlogNetMessage has no validation of transition order
evidence: |
  pub fn process_message(&mut self, msg: FlogNetMessage) {
      match msg.t.as_str() {
          "req" => self.handle_req(msg),
          "res" => self.handle_res(msg),
          "err" => self.handle_err(msg),
          "chunk" => self.handle_chunk(msg),
          ...
      }
  }
  
  No check for: 
  - "res" without prior "req" (creates entry with id that may not exist)
  - Second "req" with same id (overwrites in-flight state)
  - "chunk" on non-SSE protocol (find_by_id_mut silently succeeds)
proposed_action: |
  Add entry state enum: enum EntryState { WaitingResponse, Streaming, Complete }
  Track in NetworkEntry. Check transitions in process_message().
  Return Result<(), &'static str> for invalid transitions.
risk: medium
```

```yaml
id: DOM-003
label: B
location: src/domain/network_store.rs:108-127
title: HTTP response without prior request should error, but silently does nothing
evidence: |
  fn handle_res(&mut self, msg: FlogNetMessage) {
      if let Some(entry) = self.find_by_id_mut(msg.id) {
          entry.status = NetworkStatus::Completed;
          ...
      }
      // No else — if entry not found, response is dropped
  }
  
  Result: orphaned responses disappear; response data is lost.
  User cannot debug why network inspector shows incomplete entries.
proposed_action: |
  Expected: "res" messages should only arrive after "req" with matching id.
  Fix: Store orphaned responses or return Result with diagnostic info.
  Add test: process_message(FlogNetMessage { t: "res", ... }) with no prior req.
risk: high
```

```yaml
id: DOM-004
label: A
location: src/domain/filter.rs:1-420
title: FilterState combines three orthogonal filter dimensions into one struct
evidence: |
  pub struct FilterState {
      pub min_level: LogLevel,           // dimension 1: severity
      pub tag_include/exclude: Vec<...>, // dimension 2: tag matching
      pub search_query/regex: String,    // dimension 3: full-text search
      pub exclude_query/regex: String,   // dimension 4: exclusion
  }
  
  Single matches() method handles all four independently.
  No logical coupling; each can be tested/modified independently.
  But line 420 file size is yellow (500-800 threshold).
proposed_action: |
  No redesign needed — behavior is correct. But split into:
  - struct LevelFilter { min_level: LogLevel }
  - struct TagFilter { include, exclude, tag_regex, compiled_tag_* }
  - struct SearchFilter { query, regex, compiled_regex, compiled_plain }
  - struct ExcludeFilter { ... }
  Call all from outer FilterState.matches() which delegates to each sub-filter.
  Reduces DOM-004 to ~200 lines each, improves testability.
risk: low
```

```yaml
id: DOM-005
label: D
location: src/domain/filter.rs:14-24
title: Compiled regex and plain-text parts both live in FilterState with no encapsulation
evidence: |
  pub search_query: String,
  pub search_regex: bool,
  compiled_regex: Option<Regex>,
  compiled_search_plain: Vec<String>,
  pub exclude_query: String,
  pub exclude_regex: bool,
  compiled_exclude: Option<Regex>,
  compiled_exclude_plain: Vec<String>,
  
  User-facing public fields: query, regex flag
  Internal: compiled_*. But no way to prevent desync
  (e.g., set_search changes query but not compiled_regex).
proposed_action: |
  Create newtype SearchPattern { query: String, compiled: Compiled }
  enum Compiled { Regex(Regex), Plain(Vec<String>) }
  Hide compiled_* fields, expose only via set_search/set_exclude.
  Invariant: query and compiled are always in sync.
risk: low
```

```yaml
id: DOM-006
label: D
location: src/domain/network.rs:194-215
title: FlogNetMessage is loosely-typed struct with optional fields; protocol behavior scattered
evidence: |
  pub struct FlogNetMessage {
      pub id: u64,
      pub t: String,  // message type as bare string
      pub p: Option<String>,  // protocol
      pub method: Option<String>,  // HTTP only
      pub data: Option<String>,   // SSE chunk or WS msg
      pub chunks: Option<u32>,    // SSE only
      pub code: Option<u16>,      // WS close only
      ...
  }
  
  network_store.rs lines 79-89 have if-let chains to decode protocol.
  No type safety; mixing HTTP/SSE/WS fields in one struct.
proposed_action: |
  Create enum FlogNetMessagePayload:
  - Http { method, url, status, headers, body }
  - Sse { data, seq, chunks }
  - Ws { direction: SendRecv, data, code }
  Message handlers become pattern matches, eliminate chains.
risk: low
```

```yaml
id: DOM-007
label: A
location: src/domain/sse_merge.rs:1-264, src/domain/ws_chat.rs:1-319, src/domain/mock.rs:1-229
title: Three feature-specific blobs (SSE merge, WS chat, mock rules) extend NetworkEntry independently
evidence: |
  Mock rules: only for HTTP, synced via "mock_sync" message
  SSE merge: auto-detect field paths, concatenate across chunks
  WS chat: group messages by type, merge deltas, detect binary
  
  Each owns logic, tests, but no shared abstraction.
  Example: both SSE merge and WS chat extract JSON, detect patterns.
  Repeated: extract_type (WS) vs extract_field_paths (SSE) vs parse logic
proposed_action: |
  Design trait ProtocolExtension { extract_preview(&self) -> String }
  Create: HttpExtension (mock rules), SseExtension (field merge), WsExtension (chat)
  Move protocol-specific logic into owned modules, called from UI layer.
  Reduces code duplication in detail view rendering.
risk: low
```

```yaml
id: DOM-008
label: D
location: src/domain/structured_parser.rs:1-693
title: Large 693-line file with two responsibilities: JSON tolerant parsing + structured log parsing
evidence: |
  Lines 1-100: pub fn parse_whole() and pub fn find_and_parse()
  — tolerant JSON parser (Dart Map → Value)
  Lines 100-362: struct Parser, helper functions
  — parsing engine
  Lines 362-692: tests only
  
  No module boundary; tests consume 40% of file size.
  But tests are load-bearing (verify Dart Map edge cases).
proposed_action: |
  Keep as single unit. Justification: large match tables (parse_bare_value, classify_bare)
  and comprehensive test suite for protocol/format specs are valid reasons.
  Add module-level comment (//!) describing "tolerant text-to-JSON" contract.
  Consider: extract tests to tests/structured_parser_tests.rs if file grows >800.
risk: low
```

```yaml
id: DOM-009
label: E
location: src/domain/network.rs:13, 40, 47, 59, 67-68
title: Multiple #[allow(dead_code)] markers on actually-unused fields
evidence: |
  pub seq: u32,     // marked #[allow(dead_code)]
  pub size: u64,    // marked #[allow(dead_code)]
  pub timestamp: String,  // marked #[allow(dead_code)]
  
  Confirmed: grep shows no reference to these fields in codebase.
  Protocol fields sent by Dart client but not used in rendering.
proposed_action: |
  Remove #[allow(dead_code)] markers and fields OR
  add FlogNetMessage.seq/size/timestamp to NetworkEntry for audit trail.
  Decision: likely not needed; remove fields and clean up markers.
risk: low
```

```yaml
id: DOM-010
label: E
location: src/domain/mock.rs:23, 58, 73, 116
title: Multiple #[allow(dead_code)] on pub methods and fields
evidence: |
  pub fn as_str(&self) -> &'static str  // line 28 network_filter.rs
  pub fn next(&self) -> Self            // line 39 network_filter.rs
  pub fn hit_count: u32                 // line 24 mock.rs
  pub fn find_match(&mut self, ...) -> Option<MockRule>  // line 74
  pub fn is_empty(&self) -> bool        // line 117
  
  All marked #[allow(dead_code)] but no evidence of use in UI layer.
proposed_action: |
  Audit UI layer (scope 03) to confirm if used. If not, remove.
  If used only in tests, move to #[cfg(test)] block or remove.
risk: low
```

```yaml
id: DOM-011
label: D
location: src/domain/store.rs:37-45
title: LogStore.add_entry() implements 1-entry ring buffer but no consecutive-dup folding on drain
evidence: |
  pub fn add_entry(&mut self, entry: LogEntry) -> usize {
      // Smart folding: consecutive identical entries collapse into one.
      if let Some(last) = self.entries.back_mut() {
          if last.tag == entry.tag && ... {
              last.repeat_count += 1;
              return 0;
          }
      }
      if self.entries.len() >= MAX_ENTRIES {
          self.entries.pop_front();
          self.entries.push_back(entry);
          return 1;
      }
  }
  
  Edge case: if last entry is a folded duplicate (repeat_count=5),
  and we drain 1, then add a new identical entry,
  does it fold or create new? Answer: creates new (no look-back after drain).
  Spec says "10% drain when full" but code drains 1 at a time.
proposed_action: |
  Clarify: is consecutive-dup folding meant to survive drain events?
  Current: no (after pop_front, can't see what was popped).
  Fix if needed: maintain prev_drained_sig to fold across drain boundary.
  Add test: add N identical, drain until capacity, add identical → expect fold.
risk: medium
```

```yaml
id: DOM-012
label: E
location: src/domain/store.rs:49-54
title: append_continuation() and its Continuation parser variant are dead code
evidence: |
  #[allow(dead_code)]
  pub fn append_continuation(&mut self, content: String) {
      if let Some(last) = self.entries.back_mut() {
          last.extra_lines.push(content);
      }
  }

  Not found in grep. Parser may return Continuation(..) variant but no
  consumer uses it — extra_lines is populated during parse instead.
  User confirmed (Phase 1 C-review): remove.
proposed_action: |
  Phase 2 mechanical delete:
  1. Remove LogStore::append_continuation() from src/domain/store.rs
  2. Remove the Continuation variant from the parser output enum (grep the
     parser chain for `Continuation` and purge).
  3. Ensure compile + test still green. No behavior change expected —
     extra_lines is already set during parsing.
risk: low
```

```yaml
id: DOM-013
label: D
location: src/parser/mod.rs:29-43
title: MultiStrategyParser hard-wires parser chain; no way to add/remove parsers without code change
evidence: |
  pub fn default_chain() -> Self {
      Self {
          strategies: vec![
              Box::new(structured::StructuredParser),
              Box::new(generic::GenericParser),
              Box::new(keyword::KeywordParser),
          ],
      }
  }
  
  No public new(vec) method; chain is fixed at compile time.
  Tested order matters (structured → generic → keyword fallback).
  But no way to inject custom parsers or reorder for A/B testing.
proposed_action: |
  Add pub fn with_strategies(strategies: Vec<Box<dyn LogLineParser>>) -> Self.
  Keep default_chain() for common case. Allows testing + future extension.
risk: low
```

```yaml
id: DOM-014
label: A
location: src/parser/structured.rs:17, 21, 25, 28, 33, 38
title: Six LazyLock Regex compiled at startup; consider lazy or cached compilation
evidence: |
  static ANSI_RE: LazyLock<Regex> = ...;
  static FLUTTER_PREFIX_RE: LazyLock<Regex> = ...;
  static BRACKET_RE: LazyLock<Regex> = ...;
  static BRACKET_TS_RE: LazyLock<Regex> = ...;
  static PIPE_RE: LazyLock<Regex> = ...;
  static PIPE_TS_RE: LazyLock<Regex> = ...;
  
  Behavior correct: LazyLock compiles on first use.
  Question: are all 6 used on typical log streams?
  Hypothesis: BRACKET_TS and PIPE_TS used only for AuraLogger, others frequent.
proposed_action: |
  No change needed. LazyLock is correct. But profile if startup feels slow.
  Note: future optimization could move ANSI_RE to reusable cache if called per-line.
risk: low
```

```yaml
id: DOM-015
label: D
location: src/parser/generic.rs:14-34, src/parser/structured.rs:28-39
title: Parser modules each define their own ANSI stripping logic; not shared
evidence: |
  generic.rs: static ANSI_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(...)
  structured.rs: same pattern
  
  Both call ANSI_RE.replace_all(raw, "") identically.
  Regex is identical but duplicated.
proposed_action: |
  Move ANSI_RE to parser/mod.rs as pub and expose pub fn strip_ansi(s: &str) -> String.
  Call from both generic and structured parsers.
  Reduces duplication + ensures consistency.
risk: low
```

```yaml
id: DOM-016
label: A
location: src/parser/generic.rs:88-128
title: Generic parser has complex fallthrough logic (FLUTTER_PLAIN → BRACKET_LEVEL → System)
evidence: |
  if let Some(caps) = FLUTTER_PLAIN_RE.captures(line) {
      let content = ANSI_RE.replace_all(raw, "").to_string();
      if let Some(bcaps) = BRACKET_LEVEL_RE.captures(&content) {
          // Try [LEVEL][Tag] inside flutter content
          return Some(LogEntry { ... });
      }
      // Plain flutter content (including empty lines from print(''))
      return Some(LogEntry { level: LogLevel::System, tag: "flutter", ... });
  }
  
  Three nested if-let; correct behavior but verbose.
  Tests verify all paths work; no logic errors.
proposed_action: |
  Refactor into helper methods:
  - try_parse_flutter_prefixed(line) -> Option<LogEntry>
  - try_parse_flutter_structured(content) -> Option<LogEntry>
  - try_parse_flutter_plain(content) -> LogEntry (infallible)
  Improves readability without changing behavior.
risk: low
```

```yaml
id: DOM-017
label: D
location: src/parser/keyword.rs:12-21
title: Keyword parser uses three LazyLock Regex for inference; no priority/weighting
evidence: |
  static ERROR_RE: LazyLock<Regex> = ...;
  static WARNING_RE: LazyLock<Regex> = ...;
  static DEBUG_RE: LazyLock<Regex> = ...;
  
  If message contains both "error" and "warning", only ERROR_RE checked first.
  Priority is implicit in order; no documented rule.
  Magic strings: "error", "exception", "fatal", "crash", "panic" hard-coded.
proposed_action: |
  Extract keyword sets to named constants:
  const ERROR_KEYWORDS: &[&str] = &["error", "exception", "fatal", ...];
  const WARNING_KEYWORDS: &[&str] = &["warning", "warn", "deprecated", ...];
  Build regex from const at startup. Isolates keyword set for future config.
risk: low
```

```yaml
id: DOM-018
label: B
location: src/domain/filter.rs:201-229
title: search_positions() can return overlapping ranges if OR terms overlap
evidence: |
  pub fn search_positions(&self, text: &str) -> Vec<Range<usize>> {
      if self.search_regex { ... }
      let text_lower = text.to_lowercase();
      let mut positions = Vec::new();
      for part in &self.compiled_search_plain {
          // Find all occurrences of each part
          while let Some(pos) = text_lower[start..].find(&needle) {
              positions.push(abs_start..abs_end);
              start = abs_end;
          }
      }
      positions.sort_by_key(|r| r.start);
      return positions;
  }
  
  If query is "the|e", and text is "the end", can return:
  [0..3 ("the"), 2..3 ("e")] — overlapping ranges.
  UI highlight code may fail or double-render.
proposed_action: |
  Expected: no overlapping highlights. Merge ranges after collect:
  let merged = merge_overlapping_ranges(positions);
  Add test: search_positions("the|e", "the end") should yield [0..3] only.
risk: medium
```

```yaml
id: DOM-019
label: D
location: src/domain/network_filter.rs:136-161, src/domain/filter.rs:117-197
title: Three parallel implementations of filter logic (level, tag+search, tag+search) not unified
evidence: |
  network_filter.rs: StatusFilter.matches() / MethodFilter.matches() / ProtocolFilter.matches()
  filter.rs: matches() method with all four filters (level, tag, search, exclude)
  
  Both do boolean algebra (AND of filters) but no shared abstraction.
  Network uses 3 separate enums; log filter uses combined struct.
  Different approaches = twice the code to maintain.
proposed_action: |
  Design generic Filter<T> trait with matches(item: &T) -> bool.
  Implement: enum LogLevelFilter { Min(LogLevel) }
  Implement: enum TagFilter { Include(Vec<String>), Exclude(...) }
  Implement: enum SearchFilter { Query(String, IsRegex), ... }
  Combine via AND: struct ChainFilter(Vec<Box<dyn Filter<T>>>);
  Allows network and log filters to reuse same machinery.
risk: low
```

```yaml
id: DOM-020
label: A
location: src/domain/network.rs:184-192
title: extract_path() is correct but uses naive string search; no URL parsing library
evidence: |
  fn extract_path(url: &str) -> String {
      if let Some(pos) = url.find("://") {
          let after_scheme = &url[pos + 3..];
          if let Some(slash) = after_scheme.find('/') {
              return after_scheme[slash..].to_string();
          }
      }
      url.to_string()
  }
  
  Behavior: "https://example.com:8080/api/users?id=1" → "/api/users?id=1" ✓
  Edge case: "http://[::1]:8080/path" (IPv6) → fails (no RFC 3986 parsing)
  But likely not hit in practice (Dart client uses normal URLs).
proposed_action: |
  No change needed for current use case. But if URL parsing fails,
  consider url::Url or http crate. For now, behavior is adequate.
risk: low
```

```yaml
id: DOM-021
label: D
location: src/session.rs:27-68
title: Session load/save uses magic u8 constants; level mapping not extracted
evidence: |
  pub struct SessionData {
      pub min_level: u8,  // 0=System, 1=Verbose, 2=Debug, 3=Info, 4=Warning, 5=Error
  }
  
  load_session():
      match data.min_level {
          0 => LogLevel::System,
          1 => LogLevel::Verbose,
          ...
      }
  
  save_session():
      match app.filter.min_level {
          LogLevel::System => 0,
          ...
      }
  
  Magic numbers 0-5 repeated in two places; no guarantee stays in sync.
proposed_action: |
  Create enum LevelCode(u8) with consts LevelCode::SYSTEM = 0, etc.
  Implement impl From<LevelCode> for LogLevel and vice versa.
  Use throughout. Eliminates magic numbers, ensures forward compatibility.
risk: low
```

```yaml
id: DOM-022
label: D
location: src/session.rs:70-79
title: Session filter reconstruction from tag_include/exclude is fragile
evidence: |
  pub fn save_session(app: &App) {
      let tag_filter_input: String = app
          .filter
          .tag_include
          .iter()
          .chain(app.filter.tag_exclude.iter().map(|t| format!("-{}", t)))
          .collect::<Vec<_>>()
          .join(",");
  }
  
  Assumptions:
  - "-" prefix is used for exclude (hard-coded format string)
  - tag names cannot contain "-" prefix
  - round-trip: save → load → parse_tag_filter must reconstruct exactly
  
  If tag name starts with "-", round-trip fails.
  Format is implicit; no documentation.
proposed_action: |
  Document format in comment: "Tag filter: `+include,-exclude` (commas separated)"
  Add escape: if tag.contains('-'), escape as `\\-tag`.
  Add test: round-trip with edge-case tag names.
risk: low
```

```yaml
id: DOM-023
label: E
location: src/domain/network.rs:14, 40, 47, 59, 60, 67, 68
title: Protcol as_str() method marked #[allow(dead_code)] but not used
evidence: |
  impl Protocol {
      #[allow(dead_code)]
      pub fn as_str(&self) -> &'static str {
          match self { ... }
      }
  }
  
  grep confirms: no callsite in codebase.
proposed_action: |
  Either remove the method or remove #[allow(dead_code)] if used by UI (audit 03).
  If UI doesn't call it, delete.
risk: low
```

```yaml
id: DOM-024
label: D
location: src/domain/network.rs:90-117, src/domain/network_store.rs:74-106
title: NetworkEntry factory methods (new_http, new_sse, new_ws) repeat boilerplate
evidence: |
  pub fn new_http(id: u64, method: String, url: String, timestamp: String) -> Self {
      let path = extract_path(&url);
      Self {
          id, protocol: Protocol::Http, timestamp, method, url, path,
          status: NetworkStatus::Pending,
          ... all defaults ...
      }
  }
  
  new_sse: identical except protocol: Protocol::Sse, status: Active
  new_ws: identical except method: String::new()
  
  handle_req in network_store also duplicates this logic.
proposed_action: |
  Create builder: NetworkEntry::builder(id, url) -> EntryBuilder
  Define: impl EntryBuilder { fn http(self, method) -> Self { ... } }
  Reduces to 10-line builder, 1-line factory calls.
risk: low
```

## Summary

| label | count |
|---|---|
| A | 5 |
| B | 2 |
| C | 0 |
| D | 13 |
| E | 4 |
