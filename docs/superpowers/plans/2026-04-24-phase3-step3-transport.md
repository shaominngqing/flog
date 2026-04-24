# Phase 3 Step 3.3 — Transport Layer Redesign

> REQUIRED SUB-SKILL: superpowers:subagent-driven-development or executing-plans.

**Goal:** Resolve 14 transport/input audit entries. 1 B-fix (TRANS-007), 7 D redesigns (TRANS-002/004/006/009/012/014 + acknowledge TRANS-100..105 already done), 6 A-class ack (TRANS-003/005/008/010/011/015).

**Architecture:** Shell-out / wire-format red lines per spec §5.8. TRANS-012 + TRANS-014 touch protocol types — additions only, no field renames. `ConnectorHandle::send` generalization is non-breaking (new API, existing `send_mock_sync`/`send_replay` thin wrappers).

**Parent spec:** `docs/superpowers/specs/2026-04-22-project-cleanup-design.md` §5
**Audit:** `docs/superpowers/audit/01-transport.md`

## 旧设计问题（摘要）

| Id | Class | Scope | 问题 |
|---|---|---|---|
| TRANS-002 | D | adb.rs | 19753/10000 port-cycle magic numbers no names |
| TRANS-003 | A | device_monitor.rs | 654 lines yellow — ack (4 cohesive inline modules) |
| TRANS-004 | D | input/connector.rs | `send_mock_sync/send_replay/send_subscribe` 3 methods, no generic `send(ServerMessage)` |
| TRANS-005 | A | input/connector.rs | Hello timeout message text conflates 2 cases — fix wording |
| TRANS-006 | D | input/connector.rs | `tokio::spawn` reader/writer tasks fire-and-forget, silently die |
| TRANS-007 | B | device_monitor.rs | `Ok(Ok(_))` pattern fragile — wrap in `is_port_open` helper |
| TRANS-008 | A | device_monitor.rs | Reconnect backoff magic constants — name them |
| TRANS-009 | D | main.rs | 3 transport paths (Local/ADB/Usbmuxd) asymmetric setup code |
| TRANS-010 | A | main.rs | Inactive-app drop intentional — add doc comment |
| TRANS-011 | A | main.rs | Retry loop correct but unlogged — ack |
| TRANS-012 | D | input/protocol.rs | No compile-time check every ClientMessage/ServerMessage variant is handled |
| TRANS-014 | D | input/protocol.rs | ClientInfo lacks session_id / connect_ts metadata |
| TRANS-015 | A | main.rs | Device discovery lacks error recovery for malformed events — ack |
| TRANS-100..105 | D | (various) | Already resolved during Phase 2.5B Task 11 — ack |

## 新设计思路（key decisions）

### TRANS-002 — constants
`src/transport/adb.rs`: `PORT_BASE: u16 = 19753;` → rename to `ADB_LOCAL_PORT_POOL_BASE` with doc comment; `PORT_RANGE: u16 = 10000;` → `ADB_LOCAL_PORT_POOL_SIZE`. Existing names already exist per audit; this is comment + rename only. (Already done in Phase 2.5B per TRANS-100 ack — verify and commit if missing.)

### TRANS-004 — ConnectorHandle::send
Add `pub fn send(&self, msg: ServerMessage) -> bool`. Existing `send_mock_sync`/`send_replay`/`send_subscribe` become thin wrappers. Opens door for future downstream message types without API explosion.

### TRANS-005 — error message clarity
In `src/input/connector.rs`, Hello timeout path:
- `"Hello timeout (not a flog server?)"` → `"Hello handshake timed out after 3s (port may not be a flog server)"`
- `"First message was not Hello"` → preserve variant info: `"Expected Hello, got {variant:?}"`
- Binary frame → new separate error: `"Expected text frame, got binary"`

### TRANS-006 — task monitoring
Add minimal logging: when reader/writer task exits on error, log via `eprintln!`. Keep tasks fire-and-forget (spawning with JoinHandles would complicate ConnectorHandle state machine — that's Phase 3 Step 3.5 territory if needed). Acceptable resolution: add `// TODO-phase3.5: JoinHandle monitoring if flakiness surfaces` and log exits.

### TRANS-007 — is_port_open helper
`src/transport/device_monitor.rs`: wrap `Ok(Ok(_)) => Some(port)` as:
```rust
async fn is_port_open(port: u16) -> bool {
    tokio::time::timeout(TCP_TIMEOUT, TcpStream::connect(format!("127.0.0.1:{port}")))
        .await
        .is_ok_and(|r| r.is_ok())
}
```
`tcp_open` rewrites as `if is_port_open(port).await { Some(port) } else { None }`.

### TRANS-008 — reconnect backoff constants
In `src/transport/device_monitor.rs`: name the magic numbers `RECONNECT_INITIAL_DELAY_MS`, `RECONNECT_MAX_DELAY_MS`, `RECONNECT_BACKOFF_FACTOR`. Existing values unchanged.

### TRANS-009 — transport setup symmetry
Extract `fn resolve_transport_addr(device: &FlutterDevice) -> Result<TransportAddr, Error>`. Each path (Local/ADB/Usbmuxd) returns same type; main.rs's dispatch becomes one match expression. Keep the shell-out call sites (adb forward, usbmuxd connect) UNTESTABLE: PHYS — only extract the decision logic.

### TRANS-010 + TRANS-011 + TRANS-015 — A-class ack
Add `//` why-comments at the identified sites pointing to the audit entry.

### TRANS-012 — variant coverage
Add a compile-time check: `#[deny(non_exhaustive_patterns)]` on the match in `NetworkStore::process_message` (already done in Phase 3.2 FlogNetKind), mirror for `ClientMessage` match in `connector.rs` + `ServerMessage` match in `flog_dart` (flog_dart is Step 3.4's job). For this step: **ack the Rust side is already exhaustive, document in journal**.

### TRANS-014 — ClientInfo metadata
Add optional `session_id: Option<String>` + `connected_at: std::time::Instant` to `ClientInfo`. Wire side: `Hello` variant gains optional `#[serde(default)] session_id: Option<String>`. Dart can ignore; no breaking change.

Wait — ClientInfo already has `connected_at: std::time::Instant` per Phase 2.5B commit I saw earlier. Verify before touching.

### TRANS-100..105 — ack only
Already resolved Phase 2.5B commit b3f163f. Journal documents that fact; no code change.

## Tasks

### Task 0: pre-flight
- Verify HEAD `a3778ce`, tests green, fmt clean.

### Task 1: TRANS-002 adb port constants
- `src/transport/adb.rs`: verify constants named `ADB_LOCAL_PORT_POOL_BASE`/`ADB_LOCAL_PORT_POOL_SIZE` per audit recommendation. If not, rename. Add doc comment explaining port-range choice.
- +1 test: constants match expected 19753/10000.
- Commit: `refactor(transport/adb): name port-pool constants (Phase 3 TRANS-002)`

### Task 2: TRANS-007 is_port_open helper (B-fix)
- Add `is_port_open(port)` helper in `device_monitor.rs`.
- Refactor `tcp_open` to delegate.
- +2 tests (port open / port closed).
- Commit: `refactor(transport/device_monitor): is_port_open helper (Phase 3 TRANS-007)`

### Task 3: TRANS-008 reconnect backoff constants
- Name the magic numbers in `device_monitor.rs`.
- +1 test: verify constant values unchanged.
- Commit: `refactor(transport): name reconnect backoff constants (Phase 3 TRANS-008)`

### Task 4: TRANS-004 ConnectorHandle::send
- Add generic `send(msg: ServerMessage) -> bool` in `src/input/connector.rs`.
- Refactor `send_mock_sync`/`send_replay`/`send_subscribe` as wrappers.
- +2 tests (send delivers JSON to channel; wrappers still work).
- Commit: `feat(input/connector): generic send for ServerMessage (Phase 3 TRANS-004)`

### Task 5: TRANS-005 Hello timeout message
- Update 3 error strings per "新设计思路" §TRANS-005.
- +3 tests asserting new messages (find tests currently asserting old text, update them).
- Commit: `refactor(input/connector): clearer Hello handshake error messages (Phase 3 TRANS-005)`

### Task 6: TRANS-006 reader/writer task logging
- Add `eprintln!("connector reader task exited: {e}")` etc before task break.
- Add `// TODO-phase3.5: JoinHandle monitoring if flakiness surfaces` marker comment.
- No new test (eprintln is PHYS).
- Commit: `refactor(input/connector): log reader/writer task exits (Phase 3 TRANS-006)`

### Task 7: TRANS-009 transport setup symmetry
- Extract `fn resolve_transport_addr(device: &FlutterDevice) -> Result<TransportAddr, Error>` in `src/main.rs` or a new `src/transport/resolve.rs`.
- Three paths (Localhost/AdbForward/Usbmuxd) return the same type.
- Main's dispatch collapses to match.
- +3 tests (one per variant).
- Commit: `refactor(transport): unified resolve_transport_addr (Phase 3 TRANS-009)`

### Task 8: TRANS-014 ClientInfo session metadata
- Verify `ClientInfo` has `connected_at: Instant` (should already from Phase 2.5B).
- Add `session_id: Option<String>` field + wire it through Hello deserialize (optional).
- +2 tests (round-trip session_id present / absent).
- Commit: `feat(input/protocol): ClientInfo session_id + connected_at (Phase 3 TRANS-014)`

### Task 9: A-class acknowledgements + ack TRANS-100..105 + TRANS-012 + TRANS-003/010/011/015
- Add module-level `//!` or inline `//` comments pointing to audit ids.
- TRANS-003/005/008/010/011/015: single-line ack comments at each site.
- TRANS-012: verify match on ClientMessage/ServerMessage is exhaustive (no `_` wildcard). If there's a `_` wildcard, replace with explicit variants. If already exhaustive, ack in journal.
- TRANS-100..105: ack in journal only.
- No new tests.
- Commit: `docs(transport): A-class acknowledgements (Phase 3 TRANS-003/005/008/010/011/012/015/100-105)`

### Task 10: journal + phase-step commit
- Write `docs/superpowers/journal/phase3-step3.md`.
- Final verify: cargo test / clippy / fmt.
- Commit: `docs(journal): Phase 3 Step 3.3 — transport layer redesign complete`

## Exit gates (§5.4 (g))

- All A/D/E characterization tests green
- B test TRANS-007 green (was marked green in Phase 2.5B per audit "logic correct but fragile"; verify)
- New structural tests +10-15
- All `src/transport/*.rs` + `src/input/*.rs` files: production code < 500 lines
- `cargo clippy --all-targets -- -D warnings` clean

## 红线

- No change to `ClientMessage`/`ServerMessage` JSON wire format (spec §5.8). TRANS-014 adds `session_id: Option<String> #[serde(default)]` — additive, Dart can ignore.
- No new dependencies.
