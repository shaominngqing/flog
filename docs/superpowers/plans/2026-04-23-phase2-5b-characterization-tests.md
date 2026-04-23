# Phase 2.5B — Characterization Tests Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a real safety net. Every A-class behavior must be locked by a green test; every D-class behavior must be frozen so Phase 3 redesign cannot silently change it; every B-class bug has a red/ignored test that Phase 3 must flip to green. Coverage is the output, not the target — the target is "Phase 3 cannot break anything without a test screaming."

**Architecture:** Pure functions (all of Phase 2.5A's extracted fns, all of domain/, all of parser/) get unit tests in `#[cfg(test)] mod tests` blocks. UI render code gets `ratatui::backend::TestBackend` snapshot tests that assert **semantic observable features** — "error rows have the red background color," "tab indicator changes," "filter pill layout sequence" — not byte-exact pixel dumps. Transport gets integration tests in `tests/` against a fake WS server + fake adb/usbmuxd shims. flog_dart gets per-B-bug red tests; the existing `flog_dart/test/flog_sse_parser_test.dart` stays red until Phase 3 implements wrapTyped + SseEvent.

**Tech Stack:** Rust `#[cfg(test)]`, `ratatui::backend::TestBackend`, `tokio-tungstenite` for fake WS server. No new deps beyond what's already in Cargo.toml (ratatui + tokio already provide what we need). Dart `test` package for flog_dart.

**Parent spec:** `docs/superpowers/specs/2026-04-22-project-cleanup-design.md` §4.2
**Audit source:** `docs/superpowers/audit/00-index.md` (27 A, 12 B, 65 D, 9 E, 113 total after DOM-025 + UI-041)

---

## The North Star

**Phase 3 must not be able to change any A/D behavior without a failing test.**

Everything in this plan is a mechanical consequence of that rule. If a test doesn't serve that rule, remove it. If a behavior has no test enforcing the rule, it's a gap.

Corollary: coverage numbers are **diagnostic output**, not gates. If line coverage is 78% but I can point to an A entry with no characterization test, we're not done. If line coverage is 62% but every A/B/D is locked, we are done.

---

## Hard Rules

Every subagent prompt in this phase inherits these. Violations cause task rollback.

### Rule 1: Every A/B/D entry MUST have a locking test

| Class | Entry count | What test looks like |
|---|---|---|
| A — correct but ugly (27) | 27 | Green `#[test] fn <id>_<name>()` that asserts current behavior. Phase 3 rewriting the function must not change the test's pass/fail outcome. |
| B — confirmed bug (12) | 12 | Red or `#[ignore = "bug: <id>, fix in Phase 3"]` test asserting the **expected** behavior (not the buggy current behavior). Phase 3 fixing the bug flips the test from ignored→passing. |
| D — architecture smell (65) | 65 | Green `#[test]` that locks the CURRENT observable behavior of whatever D entry flags. When Phase 3 redesigns the module, the test still passes (because behavior didn't change — only structure did). Serves as the "behavior snapshot" protecting the redesign. |

Total tests added: at least 27 + 12 + 65 = **104 new test cases**, likely more (one entry often wants 2-3 tests covering different branches).

### Rule 2: Coverage gates — stricter than Phase 2.5 spec originally said

Based on user directive "尽可能高，不能妥协":

| Scope | Branch coverage | Line coverage | Notes |
|---|---|---|---|
| `src/domain/` | ≥ 90% | ≥ 90% | Pure logic, no excuse |
| `src/parser/` | ≥ 90% | ≥ 90% | Pure logic, no excuse |
| Phase 2.5A extracted fns | — | ≥ 95% | They were extracted precisely to be testable |
| `src/event.rs` (pure handlers split off from 2.5A) | — | ≥ 95% | |
| `src/event.rs` (remaining mouse router) | — | ≥ 75% line via TestBackend | UNTESTABLE lines get `// UNTESTABLE: <reason>` comments |
| `src/app.rs` | — | ≥ 75% | State transitions are purer than they look — many are TestBackend-testable via setting state and verifying store mutations |
| `src/ui/logs/mod.rs` | — | ≥ 75% | |
| `src/ui/network/mod.rs` | — | ≥ 75% | |
| `src/ui/network/detail.rs` | — | ≥ 75% | Largest UI file, complex folding |
| `src/ui/source_select.rs` | — | ≥ 70% | |
| `src/ui/help.rs` | — | ≥ 70% | Static render, easy |
| `src/ui/network/mock_rules.rs` | — | ≥ 70% | |
| Other ui/* (filter, stats, logs/detail/, etc.) | — | ≥ 70% | |
| `src/transport/` | — | ≥ 70% | Needs fake-server integration tests |
| `src/input/` | — | ≥ 85% | Protocol = pure serde, handshake = small |
| `src/main.rs` | — | ≥ 50% | Mostly bootstrap; some paths unreachable without full terminal |
| **Project overall line coverage** | — | **≥ 80%** | This is the bar. Starting from 32.27%. |

Every module BELOW its target at Task N's end: the Task N subagent writes UNTESTABLE annotations or fails the task. No phantom "pass" on insufficient coverage.

### Rule 3: UI tests assert observable features, not pixels

(User chose this in the pre-plan question.)

- **Assert**: semantic facts extracted from TestBackend buffer
  - "At position (x, y) the background color is `ERROR_ROW_BG`"
  - "The string 'Logs' appears in the tab bar and has a specific style"
  - "After pressing 'k', the `app.network.selected` decreased by 1"
- **DO NOT assert**: raw buffer dump via `insta::assert_snapshot!`
- **Helpers**: `tests/support/ui_inspect.rs` provides functions like:
  ```rust
  fn count_cells_with_bg(buf: &Buffer, bg: Color) -> usize
  fn find_text(buf: &Buffer, needle: &str) -> Option<(u16, u16)>
  fn style_at(buf: &Buffer, x: u16, y: u16) -> Style
  ```
- Benefit: Phase 3 can change fonts, spacing, border characters, wrap rules without breaking tests. Breaks only on genuine semantic changes.

### Rule 4: Transport integration tests use a fake server

- New helper crate or module: `tests/support/fake_flog_server.rs`
- Spins up a WS server on `127.0.0.1:0` (auto-assigned port), accepts one client, speaks the `ClientMessage`/`ServerMessage` protocol
- Tests point `ConnectorHandle` at that port and drive scenarios:
  - Normal connect + hello + disconnect
  - Hello timeout (fake server never sends hello)
  - Malformed hello (fake sends binary frame)
  - Mid-session disconnect with reconnect
  - Multiple sequential apps on same device (discovery + switch)
  - ADB forward simulated via a second fake accepting on a second port

### Rule 5: flog_dart gets per-B-bug red tests only

Not the A/D zoo. Reason: flog_dart test infra is less mature (no CI, pubspec setup is heavier), and Phase 3 DART step will rewrite large chunks of flog_dart anyway. Put safety net only where it matters: bugs we KNOW we have.

- DART-001 + DART-002: already covered by `flog_dart/test/flog_sse_parser_test.dart` (red, stays red)
- DART-003 through DART-009 (7 more B entries): each gets a test. Where a test infra doesn't exist yet (e.g. no DI for VM service extension), create the minimum scaffolding.

### Rule 6: Every test must survive Phase 3

A test that breaks because Phase 3 renamed a function is a badly-written test. Tests target **observable behavior**, not structural shapes. Concretely:

- Do NOT assert on private function names
- Do NOT import from private modules (use `pub(crate)` boundaries already established)
- Assertions on `Vec::len()` + `contains()` combinations, not on `Vec` field types
- For UI: assert cells, spans, styles, events caused — not "line 3 equals string X"
- If Phase 3 renames `MockRuleStore::find_match` to `MockRuleStore::lookup`, my test on "insert rule, lookup finds it" should still pass — just rename the call site. The assertion about behavior doesn't change.

### Rule 7: Tests live close to code

- Pure-fn tests: same-file `#[cfg(test)] mod tests`
- UI snapshot-style tests: same-file `#[cfg(test)] mod tests`, using `TestBackend` (ratatui 0.25+ supports it in-proc)
- Integration tests: `tests/characterization_<area>.rs`
- Shared helpers: `tests/support/*.rs` with `mod support;` in each `tests/*.rs` that needs them
- flog_dart: `flog_dart/test/<feature>_test.dart`

### Rule 8: No commits with ignored tests except the 12 B-bug ignored tests

If a test is ignored for any reason other than "B-class bug waiting for Phase 3 fix," it's a bad test — fix or delete.

---

## Test File Inventory (expected outputs)

New files:

- `tests/support/mod.rs` — re-exports
- `tests/support/ui_inspect.rs` — TestBackend buffer inspection helpers
- `tests/support/fake_flog_server.rs` — tokio-tungstenite fake WS server for transport integration
- `tests/support/fixtures.rs` — sample LogEntry / NetworkEntry / FlogNetMessage factories
- `tests/characterization_domain.rs` — A/D lockers for domain/ where same-file tests aren't enough
- `tests/characterization_ui_logs.rs` — Logs tab TestBackend tests
- `tests/characterization_ui_network.rs` — Network tab TestBackend tests
- `tests/characterization_transport.rs` — integration with fake server
- `tests/characterization_event_dispatch.rs` — key/mouse handlers against seeded App state
- `tests/characterization_bugs.rs` — the 12 B red/ignored tests
- `flog_dart/test/flog_mock_interceptor_test.dart` — DART-004 etc
- `flog_dart/test/flog_http_interceptor_test.dart` — DART-007/008
- `flog_dart/test/flog_web_socket_test.dart` — DART-006
- `flog_dart/test/flog_server_test.dart` — DART-005
- `flog_dart/test/flog_net_test.dart` — DART-009
- `flog_dart/test/flog_library_test.dart` — DART-003

Files modified (tests added to existing `#[cfg(test)] mod tests`):

- Every `src/domain/*.rs` (augment existing tests to 90%)
- Every `src/parser/*.rs`
- `src/input/protocol.rs`, `src/input/connector.rs`
- `src/event.rs`, `src/app.rs`, `src/ui/**/mod.rs`

---

## Task Breakdown

Tasks ordered by leverage: **most-coverage-per-effort first**, so if something blows up we have safety net sooner rather than later. Each task ends with a commit. Phase 2.5B uses one commit per task (not one per phase), because the phase is large and mid-phase rollback needs granularity.

```
Task 0:  pre-flight, capture current coverage
Task 1:  test support scaffolding (tests/support/*, fake_flog_server, ui_inspect)
Task 2:  domain layer — fill to 90% (covers ~15 A/D entries)
Task 3:  parser layer — fill to 90% (covers ~5 A/D entries)
Task 4:  input/ layer — protocol + connector unit tests to 85%
Task 5:  event.rs — pure handler tests + TestBackend-driven mouse/key tests (includes UI-007/008/009/016/020 etc)
Task 6:  app.rs — state transition tests (UI-002/004/017/022/026/028/034/040 etc)
Task 7:  ui/logs — Logs tab TestBackend tests + draw_logs path coverage
Task 8:  ui/network — Network tab TestBackend tests
Task 9:  ui/network/detail — SSE/WS/JSON detail TestBackend tests (UI-011/037 — biggest file)
Task 10: ui/source_select + ui/help + ui/network/mock_rules — remaining UI
Task 11: transport/ — integration tests via fake server
Task 12: the 12 B-class bug red/ignored tests (centralized in tests/characterization_bugs.rs)
Task 13: flog_dart B-class tests (DART-003..009)
Task 14: final coverage verification + journal + phase commit
```

---

## Pre-flight (Task 0)

### Task 0: Pre-flight verification

**Files:** (read-only)

- [ ] **Step 0.1: Confirm HEAD**

Run: `git log --oneline -1`
Expected: `2322b62 docs(journal): Phase 2.5A — logic/render separation complete`

- [ ] **Step 0.2: Baseline state**

```bash
cargo test 2>&1 | grep "test result:"
cargo clippy --all-targets -- -D warnings 2>&1 | tail -2
cargo fmt --check && echo fmt clean
cargo llvm-cov --summary-only 2>&1 | tail -3 > /tmp/phase2-5b-pre-coverage.txt
cat /tmp/phase2-5b-pre-coverage.txt
```

Expected:
- 222 lib + 227 bin + 1 integration + 0 doc all green
- clippy clean
- fmt clean
- TOTAL line ~ 32.27%

---

## Task 1: Test support scaffolding

**Files:**
- Create: `tests/support/mod.rs`
- Create: `tests/support/ui_inspect.rs`
- Create: `tests/support/fake_flog_server.rs`
- Create: `tests/support/fixtures.rs`

This task builds the reusable helpers every later task uses. It contains no audit lockers itself, but every subsequent task depends on what it exposes.

### Task 1.1: `tests/support/mod.rs` — re-exports

- [ ] **Step 1.1.1: Create module root**

Write `tests/support/mod.rs`:

```rust
//! Shared helpers for Phase 2.5B characterization tests.
//!
//! Each tests/characterization_*.rs crate includes this via:
//!   #[path = "support/mod.rs"] mod support;
//!
//! Contains:
//! - ui_inspect: TestBackend buffer assertions
//! - fake_flog_server: fake WS server for transport tests
//! - fixtures: LogEntry/NetworkEntry/FlogNetMessage factories

pub mod fake_flog_server;
pub mod fixtures;
pub mod ui_inspect;
```

### Task 1.2: `tests/support/ui_inspect.rs` — TestBackend helpers

- [ ] **Step 1.2.1: Write the helper file**

Write `tests/support/ui_inspect.rs`:

```rust
//! Inspect ratatui TestBackend buffers by observable feature.
//!
//! Assertions target semantic facts ("there's a red cell," "this text appears
//! with this fg color") not raw pixel dumps. Phase 3 can refactor render
//! internals without breaking these tests as long as the user-visible
//! behavior stays the same.

use ratatui::buffer::Buffer;
use ratatui::style::Color;
use ratatui::style::Style;

/// Count cells whose background matches the given color.
pub fn count_cells_with_bg(buf: &Buffer, bg: Color) -> usize {
    (0..buf.area.height)
        .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
        .filter(|&(x, y)| buf[(x, y)].style().bg == Some(bg))
        .count()
}

/// Count cells whose foreground matches the given color.
pub fn count_cells_with_fg(buf: &Buffer, fg: Color) -> usize {
    (0..buf.area.height)
        .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
        .filter(|&(x, y)| buf[(x, y)].style().fg == Some(fg))
        .count()
}

/// Find the first row where `needle` appears. Returns the row index.
pub fn find_text_row(buf: &Buffer, needle: &str) -> Option<u16> {
    for y in 0..buf.area.height {
        let row: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect::<Vec<_>>().join("");
        if row.contains(needle) {
            return Some(y);
        }
    }
    None
}

/// Dump an entire row as a String. For debug / helper use in tests.
pub fn row_to_string(buf: &Buffer, y: u16) -> String {
    (0..buf.area.width)
        .map(|x| buf[(x, y)].symbol().to_string())
        .collect::<Vec<_>>()
        .join("")
}

/// Get the style at a specific cell.
pub fn style_at(buf: &Buffer, x: u16, y: u16) -> Style {
    buf[(x, y)].style()
}

/// Assert that at least `min` cells have the given background color,
/// panicking with a useful message otherwise.
pub fn assert_min_cells_with_bg(buf: &Buffer, bg: Color, min: usize, ctx: &str) {
    let n = count_cells_with_bg(buf, bg);
    assert!(
        n >= min,
        "{ctx}: expected at least {min} cells with bg {:?}, got {n}",
        bg
    );
}
```

Note: the `buf[(x, y)]` syntax assumes ratatui's `Buffer: Index<(u16, u16)>` impl which exists as of recent versions. If your cargo tree says otherwise, use `buf.get(x, y)`. The subagent executing this will verify in Step 1.2.2.

- [ ] **Step 1.2.2: Verify it compiles**

Add a minimal `tests/_support_compile_check.rs` to make cargo compile the support module:

```rust
#[path = "support/mod.rs"]
mod support;

#[test]
fn support_module_compiles() {
    // Compile-only test — if this file links, support/ is healthy.
}
```

Run: `cargo test --test _support_compile_check`
Expected: 1 passed.

If buffer indexing API is different, edit `ui_inspect.rs` to use the correct API (check `cargo doc --open` or the error message). Once compiles, delete `_support_compile_check.rs` — it was just a sanity check.

### Task 1.3: `tests/support/fixtures.rs` — test data factories

- [ ] **Step 1.3.1: Write fixtures**

Write `tests/support/fixtures.rs`:

```rust
//! Factories for LogEntry, NetworkEntry, FlogNetMessage, etc.
//! Minimal data that still triggers parser/renderer code paths.

use flog::domain::entry::{LogEntry, LogLevel};
use flog::domain::network::{
    EntrySource, FlogNetMessage, NetworkEntry, NetworkStatus, Protocol, SseChunk, WsDirection,
    WsMessage,
};

/// Bare INFO entry with the given tag and message. Other fields default-ish.
pub fn info(tag: &str, message: &str) -> LogEntry {
    LogEntry {
        timestamp: "12:00:00.000".to_string(),
        level: LogLevel::Info,
        tag: tag.to_string(),
        message: message.to_string(),
        extra_lines: Vec::new(),
        repeat_count: 1,
        source: flog::domain::entry::InputSource::Dart,
        error: None,
        stacktrace: None,
    }
}

pub fn at_level(level: LogLevel, tag: &str, message: &str) -> LogEntry {
    LogEntry { level, ..info(tag, message) }
}

pub fn with_stack(tag: &str, message: &str, err: &str, stack: &str) -> LogEntry {
    LogEntry {
        error: Some(err.to_string()),
        stacktrace: Some(stack.to_string()),
        ..info(tag, message)
    }
}

/// Minimal HTTP NetworkEntry (GET 200).
pub fn http_get_200(id: u64, url: &str) -> NetworkEntry {
    NetworkEntry {
        id,
        protocol: Protocol::Http,
        status: NetworkStatus::Completed,
        method: "GET".to_string(),
        url: url.to_string(),
        path: url.to_string(),
        host: "".to_string(),
        status_code: Some(200),
        duration_ms: Some(42),
        req_size: 0,
        res_size: 128,
        timestamp: "12:00:00.000".to_string(),
        source: EntrySource::App,
        req_headers: Vec::new(),
        req_query: Vec::new(),
        req_body: None,
        res_headers: Vec::new(),
        res_body: None,
        sse_chunks: Vec::new(),
        ws_messages: Vec::new(),
    }
}
```

NOTE: if `LogEntry` / `NetworkEntry` struct shape in src/domain/ differs from the above (fields added/removed by Phase 2), the subagent must `Read` src/domain/entry.rs and src/domain/network.rs first and adjust. Do not add Default impls; spell out every field.

### Task 1.4: `tests/support/fake_flog_server.rs` — fake WS server

- [ ] **Step 1.4.1: Write fake server**

Write `tests/support/fake_flog_server.rs`:

```rust
//! Fake flog WS server used by transport integration tests.
//!
//! Spawns a tokio task listening on 127.0.0.1:0 (auto-assigned port),
//! accepts one client, speaks ClientMessage/ServerMessage. Shut down via
//! drop.

use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

pub struct FakeServer {
    pub addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
}

impl FakeServer {
    /// Start a fake server. `behavior` decides how it reacts to connections.
    pub async fn spawn(behavior: Behavior) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        let (tx, rx) = oneshot::channel();

        tokio::spawn(async move {
            tokio::select! {
                _ = run_server(listener, behavior) => {},
                _ = rx => {},
            }
        });

        Self { addr, shutdown: Some(tx) }
    }
}

impl Drop for FakeServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

#[derive(Debug, Clone)]
pub enum Behavior {
    /// Accept, send a well-formed Hello, then idle.
    NormalHello {
        device: Option<String>,
        app: String,
    },
    /// Accept but never send anything (tests Hello timeout).
    Silent,
    /// Accept, send a binary frame instead of Hello (tests Hello rejection).
    BinaryFrame,
    /// Accept, send a malformed JSON text frame.
    MalformedJson,
    /// Accept + send Hello + disconnect immediately after.
    HelloThenDisconnect {
        app: String,
    },
}

async fn run_server(listener: TcpListener, behavior: Behavior) {
    loop {
        let Ok((stream, _)) = listener.accept().await else { return };
        let b = behavior.clone();
        tokio::spawn(async move {
            use tokio_tungstenite::accept_async;
            use tokio_tungstenite::tungstenite::Message;
            use futures_util::SinkExt;

            let mut ws = match accept_async(stream).await {
                Ok(w) => w,
                Err(_) => return,
            };
            match b {
                Behavior::NormalHello { device, app } => {
                    let hello = serde_json::json!({
                        "type": "hello",
                        "device": device,
                        "app": app,
                    });
                    let _ = ws.send(Message::Text(hello.to_string().into())).await;
                    // Keep open
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    }
                }
                Behavior::Silent => {
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                }
                Behavior::BinaryFrame => {
                    let _ = ws.send(Message::Binary(vec![0u8; 32].into())).await;
                }
                Behavior::MalformedJson => {
                    let _ = ws.send(Message::Text("not json".into())).await;
                }
                Behavior::HelloThenDisconnect { app } => {
                    let hello = serde_json::json!({
                        "type": "hello",
                        "app": app,
                    });
                    let _ = ws.send(Message::Text(hello.to_string().into())).await;
                    let _ = ws.close(None).await;
                }
            }
        });
    }
}
```

- [ ] **Step 1.4.2: Compile check**

Add brief test to `tests/_support_compile_check.rs` (re-create if deleted):

```rust
#[path = "support/mod.rs"]
mod support;

#[tokio::test]
async fn fake_server_spawns() {
    let s = support::fake_flog_server::FakeServer::spawn(
        support::fake_flog_server::Behavior::Silent,
    )
    .await;
    assert!(s.addr.port() > 0);
}

#[test]
fn fixtures_compile() {
    let _ = support::fixtures::info("tag", "msg");
    let _ = support::fixtures::http_get_200(1, "https://x.test");
}
```

Run: `cargo test --test _support_compile_check`
Expected: 2 passed.

Keep this file — it's useful as a "smoke check that support/ is healthy."

### Task 1.5: Commit

- [ ] **Step 1.5.1: Commit**

```bash
git add tests/ Cargo.toml
cargo clippy --all-targets -- -D warnings 2>&1 | tail -2
cargo fmt --check && echo fmt clean
cargo test --test _support_compile_check 2>&1 | grep "test result:"

git commit -m "$(cat <<'EOF'
test(support): Phase 2.5B scaffolding — ui_inspect + fake_flog_server + fixtures

Shared test helpers for Tasks 2-13. Each characterization_*.rs test
crate pulls these via `#[path = "support/mod.rs"] mod support;`.

- ui_inspect: count_cells_with_bg/fg, find_text_row, style_at,
  assert_min_cells_with_bg — used for Rule 3 "observable features
  not pixels" UI tests.
- fake_flog_server: tokio-tungstenite fake with 5 behaviors (NormalHello,
  Silent, BinaryFrame, MalformedJson, HelloThenDisconnect) for Rule 4
  transport integration tests.
- fixtures: LogEntry/NetworkEntry factories to reduce boilerplate in
  domain tests.

_support_compile_check.rs stays as a health check.

Spec: docs/superpowers/specs/2026-04-22-project-cleanup-design.md §4.2
Plan: docs/superpowers/plans/2026-04-23-phase2-5b-characterization-tests.md

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Domain layer — fill to 90% branch+line

**Files:**
- Modify: every `src/domain/*.rs` (augment existing tests)
- Possibly create: `tests/characterization_domain.rs` (for cross-module lockers)

**Audit entries covered:** DOM-001 (filter enum unification — lock current behavior), DOM-002 (state machine transition order), DOM-004 (FilterState internal structure), DOM-005 (regex+plain-text coupling), DOM-006 (FlogNetMessage loose typing), DOM-007, DOM-008 (structured_parser), DOM-011 (LogStore drain+fold), DOM-014, DOM-016, DOM-019 (filter parallel impls), DOM-020, DOM-021, DOM-022 (session), DOM-024 (factory boilerplate), DOM-025 (write-only fields — lock that they still serialize).

This is the biggest-leverage task. Domain is pure, hasn't been hit yet, and covers 15 A/D entries.

### Task 2.1: Dispatch domain subagent

- [ ] **Step 2.1.1: Dispatch**

Use `subagent_type: "general-purpose"`. Prompt:

```
You are executing Phase 2.5B Task 2. Read:
- docs/superpowers/plans/2026-04-23-phase2-5b-characterization-tests.md
  (Hard Rules 1-8, this task's block)
- docs/superpowers/audit/02-domain.md (all A + D entries)

Scope: src/domain/*.rs ONLY. Do NOT touch src/parser/, src/ui/, src/event.rs,
src/app.rs, tests/ outside of the one new tests/characterization_domain.rs
if you need it.

Goal: every A and D entry in 02-domain.md has at least one green
characterization test locking current behavior. Plus branch+line
coverage of src/domain/ raised to ≥ 90%.

For each audit entry A or D in 02-domain.md, add a test whose name starts
with the audit id lowercased:
  #[test]
  fn dom_001_status_filter_all_matches_every_status() { ... }
  #[test]
  fn dom_002_res_without_req_drops_silently() { ... }   (this one is documenting
                                                         current behavior, NOT
                                                         asserting it's correct —
                                                         see Rule 6. It becomes a
                                                         reference point that
                                                         Phase 3 deliberately
                                                         changes.)

Method:
1. For every audit A/D id: read the entry, understand what it flags, add a
   test asserting the CURRENT behavior (not what's desirable). These are
   "characterization" tests — they describe "the code currently does X".
2. For coverage gaps after step 1: add more tests. Look at `cargo llvm-cov
   --summary-only` and `cargo llvm-cov --html` to find uncovered branches.
3. Test placement:
   - In-file `#[cfg(test)] mod tests` preferred.
   - Cross-module lockers (e.g. "MockRuleStore matches flow through filter
     then produces response") go in tests/characterization_domain.rs.
4. For DOM-025 (write-only fields): add a test asserting that SseChunk
   serializes via serde with those fields present when set. Locks the
   "payload shape" so Phase 3 can make an informed decision.
5. For B entries in 02-domain.md (DOM-003, DOM-018): do NOT write tests
   in this task. Task 12 collects all B red tests together.

Verification:
- cargo test — all green
- cargo llvm-cov --summary-only | grep "domain/" — every row ≥ 90% line,
  ≥ 90% branch (where branches are nonzero)
- cargo clippy --all-targets -- -D warnings
- cargo fmt --check

If a domain file cannot reach 90% because of genuinely-unreachable code
(e.g. an impossible match arm behind a #[non_exhaustive] that no real
caller produces), add `// UNTESTABLE: <reason>` on each such line and list
them in the report. This is permitted but every UNTESTABLE line must have
a concrete reason.

Do NOT commit. Report:
- git diff --stat
- new test count added
- cargo llvm-cov summary, domain/* rows
- list of A/D audit ids with their locking test names
- list of UNTESTABLE annotations added (if any)
```

- [ ] **Step 2.1.2: Review and commit**

Independently verify:
```bash
cargo test 2>&1 | grep "test result:"
cargo llvm-cov --summary-only 2>&1 | grep "domain/"
cargo clippy --all-targets -- -D warnings 2>&1 | tail -2
cargo fmt --check
```

Every `domain/*.rs` line coverage row should be ≥ 90%. If not, dispatch a fix subagent with the specific file(s) to push further.

Once green:
```bash
git add src/domain/ tests/
git commit -m "$(cat <<'EOF'
test(domain): Phase 2.5B Task 2 — characterization to ≥ 90% coverage

Locks behavior for every A + D entry in docs/superpowers/audit/02-domain.md.
Each test is named `<id>_<description>` so Phase 3 rewrites can trace
which pre-existing behavior the test protects.

Entries covered: DOM-001, 002, 004, 005, 006, 007, 008, 011, 014, 016,
019, 020, 021, 022, 024, 025.

B entries (DOM-003, DOM-018) deferred to Task 12 for centralized red
tests.

Coverage delta:
- domain/filter.rs: <before>% → <after>%
- domain/network_filter.rs: <before>% → <after>%
- domain/mock.rs: <before>% → <after>%
- (etc — fill from cargo llvm-cov output)

Spec: docs/superpowers/specs/2026-04-22-project-cleanup-design.md §4.2
Plan: docs/superpowers/plans/2026-04-23-phase2-5b-characterization-tests.md

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Parser layer — fill to 90%

Same recipe as Task 2, scope limited to `src/parser/*.rs`.

### Task 3.1: Dispatch

- [ ] **Step 3.1.1: Dispatch parser subagent**

Use `subagent_type: "general-purpose"`. Prompt is the same structure as Task 2.1.1, with these adjustments:

```
Scope: src/parser/*.rs ONLY.
Audit: docs/superpowers/audit/02-domain.md entries DOM-013, DOM-015, DOM-017
(parser layer D-class findings). Plus any A entries inside 02-domain.md
that reference parser code.

Goal: src/parser/*.rs branch+line coverage ≥ 90%.

Key characterization tests to add:
- DOM-013: parser chain fall-through order — feed input that only the 3rd
  strategy handles, verify the expected parser wins.
- DOM-015: each parser strategy has its own ANSI-strip pattern. Feed ANSI
  escape sequences; verify each strategy strips them.
- DOM-017: keyword parser inference priority. Feed ambiguous input, verify
  current inference outcome.

Plus: every match arm in every parser's main dispatch function should have
a test case that hits it. Goal: 90% line + 90% branch.

Verification: same as Task 2.
```

- [ ] **Step 3.1.2: Review + commit**

Same as Task 2.1.2, scoped to `src/parser/`.

---

## Task 4: input/ layer — protocol + connector to 85%

**Files:**
- Modify: `src/input/protocol.rs`, `src/input/connector.rs`
- Possibly extend: `tests/characterization_domain.rs` or new `tests/characterization_input.rs`

### Task 4.1: Dispatch

- [ ] **Step 4.1.1: Dispatch**

Use `subagent_type: "general-purpose"`. Prompt:

```
You are executing Phase 2.5B Task 4.

Scope: src/input/protocol.rs + src/input/connector.rs ONLY.
Audit: docs/superpowers/audit/01-transport.md entries TRANS-004, TRANS-012,
TRANS-014 (input layer D-class).

Goal: src/input/* branch+line coverage ≥ 85%.

Method:

(a) protocol.rs is pure serde.  Add tests in
    `src/input/protocol.rs#[cfg(test)] mod tests` that
    round-trip every ClientMessage variant and every ServerMessage variant.
    Pattern:

    #[test]
    fn trans_014_hello_serde_roundtrip() {
        let msg = ClientMessage::Hello {
            device: Some("iPhone".into()),
            app: "myapp".into(),
            ...
        };
        let j = serde_json::to_string(&msg).unwrap();
        let back: ClientMessage = serde_json::from_str(&j).unwrap();
        assert!(matches!(back, ClientMessage::Hello { .. }));
    }

    Also: feed each variant serialized as JSON and assert deserialization
    succeeds. Feed malformed JSON and assert it errors (locks the current
    error behavior).

(b) connector.rs has the WS handshake + message loop. Use
    tests/support/fake_flog_server.rs. Add a NEW file
    tests/characterization_input.rs:

    #[path = "support/mod.rs"]
    mod support;

    use support::fake_flog_server::{FakeServer, Behavior};

    #[tokio::test]
    async fn trans_004_connector_connects_receives_hello() { ... }
    #[tokio::test]
    async fn trans_005_hello_timeout_returns_err() { ... }
    #[tokio::test]
    async fn trans_005_malformed_frame_rejected() { ... }
    ...

    Write 6-10 integration tests spanning all FakeServer Behaviors.

Verification:
- cargo test — all green (integration tests work with tokio runtime)
- cargo llvm-cov --summary-only | grep "input/" — ≥ 85% line
- clippy + fmt

Do NOT commit. Report normal fields.
```

- [ ] **Step 4.1.2: Review + commit**

---

## Task 5: event.rs — pure handlers + TestBackend mouse/key

**Files:**
- Modify: `src/event.rs` (augment existing tests)
- Create: `tests/characterization_event_dispatch.rs`

**Audit covered:** UI-001 (magic consts), UI-007 (state-machine routing), UI-008 (already smoke-tested, add fuller chars), UI-009 (blocked by UI-041 for mouse, so use TestBackend + state assertions), UI-016 (magic coordinate clicks), UI-020 (input field escape), UI-024 (scroll constants).

### Task 5.1: Dispatch

- [ ] **Step 5.1.1: Dispatch**

Use `subagent_type: "general-purpose"`. Prompt:

```
You are executing Phase 2.5B Task 5.

Scope: src/event.rs + new tests/characterization_event_dispatch.rs ONLY.

Audit: docs/superpowers/audit/03-ui.md UI-001, UI-007, UI-008, UI-009,
UI-016, UI-020, UI-024, UI-041 (which blocks pure click-region extraction).

Goal: src/event.rs line coverage ≥ 75%. (Branch coverage may be lower
because of mouse routing nesting — aim as high as feasible, document
UNTESTABLE lines with reasons.)

Approach:

(1) Pure handlers already in event.rs (handle_sse_field_navigation from
    Phase 2.5A): add more branches to the in-file tests if any are
    uncovered.

(2) Key dispatch: in tests/characterization_event_dispatch.rs, construct
    a seeded App, feed key events via the public handle_key entry, assert
    App state changes. Example:

    #[test]
    fn ui_007_k_in_logs_normal_mode_moves_selected_up() {
        let mut app = seeded_app_with_entries(10);
        app.selected = 5;
        flog::event::handle_key(&mut app, KeyEvent::from(KeyCode::Char('k')));
        assert_eq!(app.selected, 4);
    }

    Cover:
    - All keys in handle_normal_key for both tabs
    - All keys in handle_input_key for each InputField variant
    - All keys in handle_overlay_key (Help, Stats)
    - MockRuleEdit keys
    - Ctrl combos

(3) Mouse dispatch: despite UI-041 blocking pure-function extraction, we
    can still test via seeded App + feed MouseEvent, assert state
    transitions. Example:

    #[test]
    fn ui_016_click_on_tab_bar_network_switches_tab() {
        let mut app = App::new();
        app.layout.tab_bar_rect = Some(Rect { ... });
        let click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: TAB_NETWORK_X,
            row: TAB_BAR_Y,
            modifiers: KeyModifiers::empty(),
        };
        flog::event::handle_mouse(&mut app, click);
        assert_eq!(app.active_tab, ViewTab::Network);
    }

    UI-016 is about magic coordinate numbers — write tests for every
    documented click region, so Phase 3's UI-041 refactor can run these
    same tests against the new ClickRegion enum.

(4) UNTESTABLE lines: if a branch requires a full TUI terminal (crossterm
    events that don't reach App state), annotate:
      // UNTESTABLE: requires real terminal backend
    Keep the count small (target: < 30 lines).

Verification: cargo test + cargo llvm-cov | grep event.rs → ≥ 75% line.
Clippy + fmt.

Do NOT commit. Report.
```

- [ ] **Step 5.1.2: Review + commit**

---

## Task 6: app.rs — state transitions

**Files:**
- Modify: `src/app.rs`
- Possibly create: `tests/characterization_app_state.rs`

**Audit covered:** UI-002, UI-003, UI-004, UI-005, UI-006, UI-017, UI-018, UI-022, UI-023, UI-026, UI-027, UI-028, UI-032, UI-034, UI-040 (15 UI entries inside app.rs scope).

### Task 6.1: Dispatch

- [ ] **Step 6.1.1: Dispatch**

Prompt structure same as prior tasks. Key scope rules:

```
Scope: src/app.rs + tests/characterization_app_state.rs ONLY.
Audit: 15 UI entries in 03-ui.md that point at app.rs.

Target: src/app.rs line coverage ≥ 75%.

Each UI-00X/02X/03X/04X entry gets a named test. Many are state-transition
tests:
  enter_search / exit_search
  enter_mock_rules / enter_mock_edit / save_mock_edit / cancel_mock_edit
  toggle_auto_scroll
  invalidate_filter + filtered_count
  switch_tab
  enter_help / enter_stats
  layout cache set/read
  app.network.sse_merged_mode toggle
  multi-app: active_app_id switch, connected_apps add/remove,
  discovered_devices add/remove
  scroll state: move_up, move_down, select_up, select_down, go_top,
  go_bottom — each tested against both tabs

Use the existing test patterns in tests/characterization_event_dispatch.rs
to drive App + assert state after calls.

Verification: cargo llvm-cov | grep app.rs → ≥ 75%. Clippy + fmt.

Do NOT commit. Report.
```

- [ ] **Step 6.1.2: Review + commit**

---

## Task 7: ui/logs — Logs tab TestBackend

**Files:**
- Modify: `src/ui/logs/mod.rs`, `src/ui/logs/detail/*`, `src/ui/logs/stats.rs`, `src/ui/logs/timeline.rs`, `src/ui/logs/highlight.rs`, `src/ui/logs/jump.rs`
- Create: `tests/characterization_ui_logs.rs`

**Audit covered:** UI-010, UI-012, UI-025, UI-029, UI-030, UI-031, UI-036, UI-038, UI-039 (9 entries).

### Task 7.1: Dispatch

- [ ] **Step 7.1.1: Dispatch**

```
Scope: src/ui/logs/** + tests/characterization_ui_logs.rs ONLY.
Audit: 9 UI entries pointing at ui/logs/*.
Target: every ui/logs/*.rs file ≥ 75% line; aggregate ui/logs/ ≥ 75%.

Approach — Rule 3 observable features:

For each A/D entry, design a test via TestBackend:
  use ratatui::backend::TestBackend;
  use ratatui::Terminal;
  use flog::app::App;

  #[test]
  fn ui_010_draw_logs_empty_store_renders_placeholder() {
      let mut app = App::new();
      let backend = TestBackend::new(80, 24);
      let mut term = Terminal::new(backend).unwrap();
      term.draw(|f| flog::ui::logs::draw_logs(f, &mut app, f.size())).unwrap();
      let buf = term.backend().buffer();
      assert!(support::ui_inspect::find_text_row(buf, "Quick Start").is_some());
  }

  #[test]
  fn ui_031_tag_colors_cycle_through_palette() {
      // Feed app with distinct tags, render, assert each tag's
      // representative cell has a known palette color from TAG_COLORS.
  }

  #[test]
  fn ui_038_long_message_wraps_to_max_wrap_lines() {
      // Feed app with a very long message, render at width 80, count
      // rows occupied by that entry. Assert ≤ MAX_WRAP_LINES.
  }

Do NOT assert byte-exact buffer content. Assert:
- cells with specific colors appear at specific row/col ranges
- specific strings appear in specific rows (find_text_row)
- scroll offset changes cause different entries to be at the top row
- auto_scroll behavior when selected reaches bottom

For every uncovered line, add a test that exercises it or
document with UNTESTABLE.

Verification: cargo llvm-cov | grep "ui/logs" → each file ≥ 75%.

Do NOT commit. Report.
```

- [ ] **Step 7.1.2: Review + commit**

---

## Task 8: ui/network — Network tab TestBackend

Same recipe as Task 7, scope `src/ui/network/mod.rs` + `src/ui/network/stats.rs` + `src/ui/network/filter.rs`.

### Task 8.1: Dispatch

- [ ] **Step 8.1.1: Dispatch**

```
Scope: src/ui/network/mod.rs, src/ui/network/stats.rs,
src/ui/network/filter.rs, + tests/characterization_ui_network.rs.
Audit: UI-029, UI-032, UI-035 and related.
Target: each ≥ 75% line.
```

Structurally identical to Task 7.1.1, re-scoped.

- [ ] **Step 8.1.2: Review + commit**

---

## Task 9: ui/network/detail — SSE / WS / JSON detail

**Files:**
- Modify: `src/ui/network/detail.rs`
- Extend: `tests/characterization_ui_network.rs`

**Audit covered:** UI-011, UI-037 (the 1109-line detail renderer).

### Task 9.1: Dispatch

- [ ] **Step 9.1.1: Dispatch**

```
Scope: src/ui/network/detail.rs ONLY.
Audit: UI-011, UI-037.
Target: ≥ 75% line.

Specific scenarios to cover:
- Regular HTTP detail: headers + body (JSON / text / empty)
- HTTP with query params
- SSE detail:
  - Normal Events mode (chunks listed)
  - Merged mode (merged field shown)
  - Empty SSE (no chunks yet)
- WS detail:
  - Chat mode ON (messages grouped)
  - Raw mode (each message shown independently)
  - Binary messages display label
- Mocked row: verify MOCKED_ROW_BG applied
- Replayed row: verify REPLAY_ROW_BG applied
- Error row (status ≥ 400): verify ERROR_ROW_BG applied
- Warning row (4xx-nonerror threshold): verify WARNING_ROW_BG applied

For JSON fold logic: call a render, assert fold state; then toggle via
event, re-render, assert toggled cells.

Do NOT commit. Report.
```

- [ ] **Step 9.1.2: Review + commit**

---

## Task 10: ui/source_select + ui/help + ui/network/mock_rules

**Files:**
- Modify: `src/ui/source_select.rs`, `src/ui/help.rs`, `src/ui/network/mock_rules.rs`, `src/ui/tab_bar.rs`, `src/ui/input_field.rs`, `src/ui/text_editor.rs`, `src/ui/mod.rs`

**Audit covered:** UI-013, UI-014, UI-015, UI-019, UI-021, UI-033.

### Task 10.1: Dispatch

- [ ] **Step 10.1.1: Dispatch**

```
Scope: src/ui/source_select.rs, src/ui/help.rs,
src/ui/network/mock_rules.rs, src/ui/tab_bar.rs, src/ui/input_field.rs,
src/ui/text_editor.rs, src/ui/mod.rs.
Audit: UI-013, UI-014, UI-015, UI-019, UI-021, UI-033.
Target: each ≥ 70% line.

help.rs at 534 lines is mostly static content — easy to cover by rendering
it at different terminal sizes and counting non-whitespace rows.

source_select.rs at 898 lines has 4 row-builder fns (all touched by
Phase 2 vec! cleanup). Test each with different device state combinations:
- No devices discovered
- 1 device, not connected
- 1 device, connected
- Multiple devices, mixed states
- Device with long name (truncation)

mock_rules.rs: render each rule-edit-field state (4 fields), verify
cursor style, verify body editor focus.

input_field.rs + text_editor.rs are already near 93-97% — fill the
remaining branches.

Do NOT commit. Report.
```

- [ ] **Step 10.1.2: Review + commit**

---

## Task 11: transport — integration tests

**Files:**
- Modify: `src/transport/device_monitor.rs`, `src/transport/adb.rs`, `src/transport/usbmuxd.rs` (in-file unit tests)
- Create: `tests/characterization_transport.rs`

**Audit covered:** TRANS-002, TRANS-003, TRANS-006, TRANS-008, TRANS-009, TRANS-011, TRANS-013 + the TRANS-015 new entry if added.

### Task 11.1: Dispatch

- [ ] **Step 11.1.1: Dispatch**

```
Scope: src/transport/** + tests/characterization_transport.rs.
Audit: TRANS-002 through TRANS-013.
Target: src/transport/ ≥ 70% line aggregate (device_monitor.rs has
shell-out paths that are OS-specific — those can be UNTESTABLE).

Concrete tests (in tests/characterization_transport.rs):

#[tokio::test]
async fn trans_006_reader_task_receives_log_messages() {
    let server = FakeServer::spawn(Behavior::NormalHello { app: "x".into(), device: None }).await;
    // Add: fake server sends a Log message after Hello
    // Assert: ConnectorEvent::Message received downstream
}

#[tokio::test]
async fn trans_008_reconnect_after_disconnect() { ... }

#[tokio::test]
async fn trans_007_tcp_open_returns_some_when_port_listening() {
    // Needs the tcp_open fn to be pub(crate) or reachable. If private,
    // add an in-file test.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    assert_eq!(tcp_open(port).await, Some(port));
}

#[tokio::test]
async fn trans_007_tcp_open_returns_none_when_no_listener() {
    assert_eq!(tcp_open(1).await, None); // port 1 is typically nobody
}

For device_monitor.rs: shell-out to `flutter devices --machine` isn't
testable without mocking. Put the JSON-parse logic in a pure function
if it isn't already; test that pure function with fixture JSON.
If the shell-out is inline and the refactor-to-extract would be too
invasive for Phase 2.5B, annotate UNTESTABLE with reason "shell-out to
flutter CLI" — Phase 3 refactors.

adb.rs setup_forward / remove_forward: shell out to `adb forward`.
Same treatment — parsing-pure vs CLI-effect split.

usbmuxd.rs: protocol parsing is testable against fixture plist bytes;
actual Unix-socket Connect is not. Split the same way.

Do NOT commit. Report.
```

- [ ] **Step 11.1.2: Review + commit**

---

## Task 12: The 12 B-bug tests (centralized red/ignored)

**Files:**
- Create: `tests/characterization_bugs.rs`

### Task 12.1: Write B-class red tests

- [ ] **Step 12.1.1: Dispatch**

```
Scope: tests/characterization_bugs.rs ONLY. No src/ changes.

Read docs/superpowers/audit/00-index.md B list (12 entries). For each
Rust-side B entry (DOM-003, DOM-018, TRANS-007), write a test asserting
the EXPECTED behavior (per the entry's proposed_action). The test will
fail on current code because the bug is present, so mark:

  #[ignore = "bug: DOM-003, fix in Phase 3"]
  #[test]
  fn dom_003_response_without_request_reports_error() {
      let mut store = NetworkStore::new();
      let res_msg = FlogNetMessage {
          t: "res".into(),
          id: 999, // no matching req
          ...
      };
      // Expected (per proposed_action): error returned or orphan stored
      let result = store.process_message(res_msg);
      // Assert expected behavior — this will fail on current buggy code.
      // Which means the #[ignore] is necessary; Phase 3 removes #[ignore]
      // and the test passes.
      assert!(result.is_err() || store.has_orphan(999));
  }

For TRANS-007: the audit entry says the Ok(Ok(_)) pattern is fragile,
not broken. The bug is readability / maintainability. Phase 2.5B can
test this as a characterization (green test of current behavior). It
was labeled B but borderline — check the entry's evidence. If current
behavior IS correct, downgrade the test to green (remove #[ignore]).

For DART-* B entries: those are flog_dart tests, Task 13 handles them.

For DOM-018 (overlapping highlight ranges): expected is no overlaps.
Test input "the end" with OR query "the|e"; assert resulting
search_positions contains no overlapping pairs.

Verification:
- cargo test — B-class tests all either ignored or passing
- cargo test -- --include-ignored shows them as failing (proves they
  actually test the expected bug-fixed behavior)

Do NOT commit. Report.
```

- [ ] **Step 12.1.2: Verify and commit**

```bash
cargo test 2>&1 | grep "test result:"
cargo test -- --include-ignored 2>&1 | grep "test result:"   # at least 3 failures expected (the Rust B tests)
```

Commit:

```bash
git add tests/characterization_bugs.rs
git commit -m "$(cat <<'EOF'
test(bugs): Phase 2.5B Task 12 — B-class ignored red tests

Rust-side B entries each get an #[ignore = "bug: <id>, fix in Phase 3"]
test asserting the EXPECTED behavior. Running with --include-ignored
produces failures, proving the tests are real (not stubs).

Entries covered:
- DOM-003: response without request should error/store-as-orphan
- DOM-018: search_positions should not return overlapping ranges
- TRANS-007: tcp_open readability — downgraded to green (behavior is
  correct, just hard to read)

flog_dart B entries handled by Task 13.

Phase 3 removes each #[ignore] as it fixes the corresponding bug.

Spec: docs/superpowers/specs/2026-04-22-project-cleanup-design.md §4.2

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 13: flog_dart B-class tests

**Files:**
- Create: `flog_dart/test/flog_mock_interceptor_test.dart`
- Create: `flog_dart/test/flog_http_interceptor_test.dart`
- Create: `flog_dart/test/flog_web_socket_test.dart`
- Create: `flog_dart/test/flog_server_test.dart`
- Create: `flog_dart/test/flog_net_test.dart`
- Create: `flog_dart/test/flog_library_test.dart`
- Modify: `flog_dart/pubspec.yaml` (add flutter_test / test dev dep if missing)

### Task 13.1: Dispatch

- [ ] **Step 13.1.1: Dispatch**

```
Scope: flog_dart/test/ + flog_dart/pubspec.yaml ONLY.
Audit: DART-001 (already covered by existing flog_sse_parser_test.dart),
DART-002 (same), DART-003 through DART-009 (7 to write).

For each DART-003..009:
- Read the entry in 04-flog-dart.md
- Write a new test file that asserts the expected behavior
- Mark the top of the file with a comment: "Red: Phase 3 DART-00X fix
  makes this green."
- Use Dart's expect() assertions

DART-003: library docstring claims flog() exists; test asserts a flog()
  top-level function returns a FlogLogger when called. Currently will
  fail to compile — use a conditional compile guard or expectLater to
  defer the compile error to test runtime.

DART-004: FlogMockInterceptor.onRequest with flogEnabled=false should
  skip matching logic. Test by... (harder — flogEnabled is compile-time)
  Alternative: use a runtime flag or accept this test as
  "run-under-release-mode" which Dart test runner can't easily do.
  If untestable without significant infra, write it as a commented-out
  block explaining why, and note in flog_dart/test/README.md.

DART-005: ext.flog.syncMockRules registered. Test by calling
  developer.extension(...) and asserting success (or error if not
  registered).

DART-006: `stream` is single-subscription but docs claim broadcast.
  Test by subscribing twice; assert second subscription errors.

DART-007: _truncate char vs byte. Test by feeding a multi-byte UTF-8
  string and asserting the byte budget is correctly counted.

DART-008: _idMap leak. Test by triggering an early-reject interceptor
  and asserting _idMap doesn't grow. (Private field access may require
  @visibleForTesting getter.)

DART-009: emitNet mutates caller map. Test by calling emitNet and
  asserting the passed Map is unchanged afterwards.

Run: cd flog_dart && dart test 2>&1 | tail
Expected: new tests either fail red (confirming the bug) or the DART-004
-style test is documented as untestable.

Do NOT commit. Report.
```

- [ ] **Step 13.1.2: Verify and commit**

```bash
cd flog_dart && dart test 2>&1 | tail -20
cd ..
git status
```

Tests will fail red — expected. That's the Phase 2.5B B-class contract.

```bash
git add flog_dart/test/ flog_dart/pubspec.yaml
git commit -m "$(cat <<'EOF'
test(flog_dart): Phase 2.5B Task 13 — DART B-class red tests

Each of DART-003..009 gets a red test. Combined with the existing
flog_sse_parser_test.dart (DART-001 + 002), all 9 DART B entries
have red coverage.

Where a test is truly untestable under current infra (flogEnabled
compile-time), the test is stubbed with a README explanation.

Phase 3 DART step removes the redness.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 14: Final coverage verification + journal + phase commit

- [ ] **Step 14.1: Coverage verdict**

```bash
cargo test 2>&1 | grep "test result:"
cargo clippy --all-targets -- -D warnings 2>&1 | tail -3
cargo fmt --check && echo fmt clean
cargo llvm-cov --summary-only 2>&1 | tee /tmp/phase2-5b-post-coverage.txt | tail -50
```

Verify every target module hits its line-coverage target from "Rule 2" table. If any miss, go back and fix — do not proceed until every target is met.

Overall project target: ≥ 80% line. Compare:
```bash
grep TOTAL /tmp/phase2-5b-pre-coverage.txt /tmp/phase2-5b-post-coverage.txt
```

- [ ] **Step 14.2: Audit-entry lock-off checklist**

```bash
# Every A/D entry in audit should have a matching test name prefix.
python3 <<'PY'
import re, glob
# Collect every audit id A+D (not B, those are Task 12/13).
ids = set()
for f in sorted(glob.glob("docs/superpowers/audit/0[1-4]-*.md")):
    content = open(f).read()
    for b in re.findall(r"```yaml\s*\n(.*?)\n```", content, re.DOTALL):
        lab = re.search(r"^label:\s*([ABDE])\s*$", b, re.MULTILINE)
        idm = re.search(r"^id:\s*(\S+)", b, re.MULTILINE)
        if lab and idm and lab.group(1) in ("A", "D"):
            ids.add(idm.group(1))

import subprocess
out = subprocess.check_output(["rg", "-o", r"fn (trans|dom|ui|dart)_\d+", "-r", r"$1_$2", "src/", "tests/"], text=True, stderr=subprocess.DEVNULL)
# Extract ids covered by test names
covered = set()
for line in out.splitlines():
    m = re.search(r"(trans|dom|ui|dart)_(\d+)", line, re.I)
    if m:
        covered.add(f"{m.group(1).upper()}-{int(m.group(2)):03d}")

missing = ids - covered
print(f"A+D entries: {len(ids)}")
print(f"Covered by test name: {len(covered)}")
print(f"Missing: {sorted(missing)}")
PY
```

If any A/D id is missing, add a test covering it and re-run until the missing set is empty.

- [ ] **Step 14.3: Write phase journal**

Create `docs/superpowers/journal/phase-2.5b.md` with:
- Entry state: 222 lib + 227 bin tests, coverage 32.27%
- Exit state: <N> lib + <M> bin, coverage <X%>
- Per-task commit hashes
- A/D/B entry coverage map
- Every UNTESTABLE annotation (with file:line)
- Known gaps + why they're acceptable
- Handoff to Phase 3: the test suite IS the safety net. Any Phase 3 commit that changes test outcomes without explicit approval is a regression.

- [ ] **Step 14.4: Phase commit**

```bash
git add docs/superpowers/journal/phase-2.5b.md
git commit -m "$(cat <<'EOF'
docs(journal): Phase 2.5B — characterization tests complete

Safety net built. Every A and D audit entry has a green characterization
test locking current behavior. Every B-class bug has a red/ignored test
asserting expected behavior (Phase 3 flips them to green).

Coverage: 32.27% → <X>% line. Target modules all at their gate levels
(domain ≥ 90%, parser ≥ 90%, input ≥ 85%, event.rs/app.rs/ui ≥ 75%,
transport ≥ 70%).

Test count: 222 lib + 227 bin + 1 integration → <A> lib + <B> bin +
<C> integration.

New shared test infra:
- tests/support/ui_inspect.rs — Rule 3 observable-feature TestBackend assertions
- tests/support/fake_flog_server.rs — Rule 4 transport integration
- tests/support/fixtures.rs — Rule 7 LogEntry/NetworkEntry factories

UNTESTABLE annotations: <N> lines, all documented in phase-2.5b.md.

Phase 3 can now safely redesign any A/D-locked behavior; the tests
will scream on regression.

Spec: docs/superpowers/specs/2026-04-22-project-cleanup-design.md §4.2
Plan: docs/superpowers/plans/2026-04-23-phase2-5b-characterization-tests.md

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Phase 2.5B acceptance checklist

- [ ] Tasks 1-13 each committed separately
- [ ] Task 14 phase journal commit
- [ ] Every A + D audit entry has a test named `<id>_<description>` in src/ or tests/
- [ ] Every B audit entry has a red/ignored test (3 Rust + 7 flog_dart + 2 flog_sse_parser_test)
- [ ] Coverage gates per Rule 2 all met
- [ ] cargo test all green (ignored tests not counted as failures)
- [ ] cargo clippy --all-targets -- -D warnings green
- [ ] cargo fmt --check clean
- [ ] Project overall coverage ≥ 80% line
- [ ] `docs/superpowers/journal/phase-2.5b.md` written

---

## Risks & mitigations

| Risk | Mitigation |
|---|---|
| Snapshot tests blow up in Phase 3 | Rule 3 assert-observable-features keeps surface small |
| Subagent writes low-quality tests just to hit coverage | Rule 6: every test must "survive Phase 3" (target observable behavior). Reviewer rejects structural tests. |
| UNTESTABLE count balloons | Rule 2 requires reason + list in journal. Review catches >30 per file. |
| Fake WS server flaky | Use auto-assigned port 127.0.0.1:0, short timeouts, proper shutdown via Drop. |
| DART-004 flogEnabled compile-time untestable | Accept as documented untestable; note in phase-2.5b.md; Phase 3 DART step may introduce runtime-test variant |
| Coverage tool run too slow to iterate | cargo-llvm-cov has --no-report mode for mid-work fast runs; only full --summary at Task 14 |

---

## Downstream dependencies

Phase 3 step planning reads from:
- Every A/D-locked test — the Phase 3 step for each module must keep these passing
- Every B red test — each phase-3 step must flip at least the B tests in its scope
- phase-2.5b.md journal's "UNTESTABLE" list — some Phase 3 refactors exist specifically to make those lines testable
