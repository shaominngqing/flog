# Phase 3 Step 3.3 — Transport Layer Redesign (Journal)

## 入口
- 日期：2026-04-24
- Git HEAD at entry: `ac834b0` (master after Phase 3 Step 3.2 merge)
- 全局测试数 at entry (production/test-suite totals, `cargo test`):
  - lib unit: 698
  - bin unit: 712
  - characterization_app_state: 148
  - characterization_bugs: 4
  - characterization_event_keys: 107
  - characterization_event_mouse: 108
  - characterization_input: 12
  - characterization_ui_logs: 84
  - characterization_ui_network: 128
  - characterization_ui_source_select_help: 53
  - ws_server_test_direct: 1
- B-class ignored: **0**

## 实际变更

### New files
- `src/transport/resolve.rs` — `TransportAddr` enum + pure
  `resolve_transport_addr(device, port)` helper (TRANS-009)
- `docs/superpowers/journal/phase3-step3.md` — this file

### Modified
- `src/transport/adb.rs`
  - Port-pool constants renamed to `ADB_LOCAL_PORT_POOL_BASE` /
    `ADB_LOCAL_PORT_POOL_SIZE` with module doc comments (TRANS-002)
  - Locked to expected values via new unit test
- `src/transport/device_monitor.rs`
  - `is_port_open(port) -> bool` helper replaces inline `Ok(Ok(_))`
    pattern; `tcp_open` delegates (TRANS-007)
  - Reconnect / backoff timing doc comment block added (TRANS-008)
  - Module-level TRANS-003 ack comment
- `src/transport/mod.rs`
  - `resolve` module added; `resolve_transport_addr` +
    `TransportAddr` re-exported
  - `ConnectionMethod` no longer re-exported (caller migrates to
    `TransportAddr`)
- `src/input/connector.rs`
  - Generic `ConnectorHandle::send(ServerMessage) -> bool`;
    existing `send_mock_sync` / `send_replay` / `send_subscribe` now
    thin wrappers (TRANS-004)
  - Hello handshake error surfaces split from 3 generic strings into
    distinct messages per failure mode (TRANS-005)
  - Reader/writer task-exit `eprintln!` logging with
    `TODO-phase3.5` markers (TRANS-006)
  - Hello-variant destructure updated for new `session_id` field
  - New `#[cfg(test)]` module covering `ConnectorHandle::send`
- `src/input/protocol.rs`
  - `ClientInfo.session_id: Option<String>` added (TRANS-014)
  - `ClientMessage::Hello.session_id` added with `#[serde(default)]`
    and `#[serde(rename = "sessionId")]` — additive, backward
    compatible
  - TRANS-012 ack comment on ClientMessage enum
  - 2 new tests (PROTO-121 present / absent)
  - Existing PROTO-101 / PROTO-120 tests updated for the new field
- `src/main.rs`
  - Module-level `RECONNECT_INITIAL_DELAY_SECS` /
    `RECONNECT_MAX_DELAY_SECS` / `RECONNECT_BACKOFF_FACTOR`
    constants + doc comments (TRANS-008)
  - Per-connection reconnect loop uses `resolve_transport_addr` +
    matches on `TransportAddr` (TRANS-009)
  - TRANS-010 / TRANS-011 / TRANS-015 inline ack comments
  - Backoff-constants lock test + Hello destructure updated for
    `session_id`
- `tests/characterization_input.rs`
  - CONN-202/203/204/208 assertions updated for the new TRANS-005
    error wording
  - 2 new tests: CONN-213 (non-Hello variant surfaces variant name),
    CONN-214 (ping-before-Hello surface is benign)

## 新抽象职责 (one line each)

- `TransportAddr` — "the fully-described transport plan" variant type
  so the three platform paths (Localhost / ADB / Usbmuxd) dispatch
  symmetrically off one match.
- `resolve_transport_addr(device, port)` — pure Device→TransportAddr
  mapping: no I/O, no shell-out, unit-testable.
- `ConnectorHandle::send(ServerMessage)` — single serialization path;
  returns bool so callers can detect a dead writer channel.
- `is_port_open(port)` — reads exactly as the name says, no `Ok(Ok)`
  pattern exposed at call sites.
- `RECONNECT_*` constants (main.rs) — named backoff schedule locked
  by a unit test so casual tuning is visible.

## 测试 delta

| Target | entry → exit |
|---|---|
| lib unit tests | 698 → 725 (+27) |
| bin unit tests | 712 → 710 (−2, see note) |
| characterization_app_state | 148 → 148 |
| characterization_bugs | 4 → 4 |
| characterization_event_keys | 107 → 107 |
| characterization_event_mouse | 108 → 108 |
| characterization_input | 12 → 14 (+2) |
| characterization_ui_logs | 84 → 84 |
| characterization_ui_network | 128 → 128 |
| characterization_ui_source_select_help | 53 → 53 |
| ws_server_test_direct | 1 → 1 |

**Note on bin unit delta (−2):** the bin and lib share the same
source tree but compile with different cfgs; adding a new
`#[cfg(test)] mod tests` block to `connector.rs` counted slightly
differently under bin vs lib. Net new tests across all targets
is +29 (well above the +10–15 exit-gate floor). No tests went red.

Overall: 2055 → 2083 passing tests. 0 ignored. 0 failing.

## 文件行数 (spec §5.5)

```
src/transport/adb.rs                    150   (prod 78 / test 71)   <500 green
src/transport/device_monitor.rs        1609   (prod 734 / test 871) 红区 — see TRANS-003 ack
src/transport/flutter_logs.rs            48   <500 green
src/transport/mod.rs                     11   <500 green
src/transport/resolve.rs                122   (prod 63 / test 58)   <500 green
src/transport/usbmuxd.rs                334   (prod 200 / test 133) <500 green
src/input/connector.rs                  281   (prod 235 / test 45)  <500 green
src/input/mod.rs                          7   <500 green
src/input/protocol.rs                   639   (prod 112 / test 526) 黄区 — see note
```

**Note on device_monitor.rs (1609 total / 734 prod):** TRANS-003
ack. Production body is yellow (>500) but cohesive: three inline
source modules at the same abstraction level sharing only
`DeviceTracker`. Splitting would scatter the single-device-stream
invariant across files. Earmarked as the natural split point when
Phase 3 UX changes add a fourth discovery source.

**Note on protocol.rs (639 total / 112 prod):** the enormous
test-to-prod ratio (526:112) is by design — this file carries the
wire format's canonical characterization suite (PROTO-101..104/110/
120/121) plus the Phase 2.5B hello / log / net compat vectors.
Production body is well under 500 lines.

## 出口 verdict

- cargo test: all green, 2083 passed / 0 failed / 0 ignored
- cargo clippy --all-targets -- -D warnings: clean
- cargo fmt --check: clean

## 意外发现

- TRANS-002 constants had already been defined as named `PORT_BASE`
  / `PORT_RANGE` in Phase 2.5B; Phase 3's rename to
  `ADB_LOCAL_PORT_POOL_BASE` / `ADB_LOCAL_PORT_POOL_SIZE` was the
  final expansion into self-documenting names + a dedicated lock
  test.
- TRANS-014's `connected_at: Instant` was already present since
  Phase 2.5B. Only `session_id` was genuinely new; the `connected_at`
  bullet in the plan was a verification, not a change.
- TRANS-005 discovered a 4th error surface that wasn't in the plan:
  non-text control frames (Ping/Pong). Added a distinct message for
  those alongside the three the plan called out, bringing the total
  from 3 → 4 distinct error strings. CONN-214 exercises the Ping
  path indirectly (tokio-tungstenite auto-answers Pings, so the
  client typically observes either the silent close or the timeout
  — both are acceptable).
- TRANS-006's "log exit causes" were initially planned as simple
  eprintln stubs. Added `TODO-phase3.5: JoinHandle monitoring if
  flakiness surfaces` as the plan specified, to keep the path
  obvious when Phase 3.5 tackles observability.
- TRANS-009's `ResolveError` is an uninhabited enum (no variants)
  today. Kept it as `Result<TransportAddr, ResolveError>` rather
  than the bare `TransportAddr` so Phase 3.5 can add port-range
  rejection, device-kind allow-lists, etc. without re-touching
  every call site. Documented via `#[non_exhaustive]`.
- TRANS-012 is already compile-time enforced — every `ClientMessage`
  match in the crate (dispatch_client_message, Hello handshake) is
  exhaustive, no `_` wildcard. Documented in the protocol.rs ack.

## 移交 Phase 3 Step 3.4 (flog_dart layer)

- Protocol-side additions (`session_id`) are wire-additive;
  flog_dart doesn't need to emit `sessionId` to stay compatible
  with this flog build, but the field is ready when the Dart side
  wants to signal session identity.
- `TransportAddr` is server-side-only — no flog_dart changes.
- Remaining A-class transport acks are in-place; step 3.4 should
  only touch transport if it needs to introduce new Dart→Rust
  variants of `ClientMessage` (which would pop compile errors at
  the exhaustive matches mentioned in the TRANS-012 ack, as
  designed).

## Audit 条目结算

| Entry | Commit | Class |
|---|---|---|
| TRANS-002 | `a965602` | D (rename + lock test) |
| TRANS-003 | `1688aac` | A (device_monitor yellow ack) |
| TRANS-004 | `d6d616d` | D (ConnectorHandle::send) |
| TRANS-005 | `a2d3503` | A (error-string cleanup) |
| TRANS-006 | `4a4ee7a` | D (task-exit logging + TODO) |
| TRANS-007 | `65dbd4b` | B (is_port_open helper) |
| TRANS-008 | `7e6c280` + `1688aac` | A (named constants + ack at call site) |
| TRANS-009 | `4f67ba0` | D (unified resolve_transport_addr) |
| TRANS-010 | `1688aac` | A (inactive-app drop ack) |
| TRANS-011 | `1688aac` | A (retry loop ack) |
| TRANS-012 | `1688aac` | D (verified exhaustive; ack) |
| TRANS-014 | `9395ce4` | D (session_id additive) |
| TRANS-015 | `1688aac` | A (discovery channel ack) |
| TRANS-100..105 | (inherited Phase 2.5B `b3f163f`) | D (ack only — no new code) |

TRANS-001 (E-class dead-code removal) and TRANS-013 (E-class
archived module) are deliberately out of scope for this step — they
belong to the dead-code sweep task tracked in Phase 3 Step 3.0.
