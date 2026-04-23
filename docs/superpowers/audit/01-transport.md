# Audit 01 — Transport & Discovery

Scope: `src/transport/`, `src/input/connector.rs`, `src/input/protocol.rs`,
`src/main.rs` (connection lifecycle), `src/replay.rs`.

Auditor: Phase 1 Subagent 1 (read-only)
Date: 2026-04-22

## Findings

```yaml
id: TRANS-001
label: E
location: src/transport/usbmuxd.rs:12-232
title: Dead code — UsbDevice struct and list_devices function unused
evidence: |
  pub struct UsbDevice {
      pub device_id: u32,
      pub serial_number: String,
  }
  
  pub async fn list_devices() -> Result<Vec<UsbDevice>, ...> { ... }
  
  Grep confirms neither UsbDevice nor list_devices appear anywhere
  outside usbmuxd.rs. The device discovery path uses the event-driven
  usbmuxd Listen protocol (handle_attached/handle_detached) instead of
  this one-shot ListDevices approach, which was superseded by the
  architecture redesign to Direct Socket.
risk: low
proposed_action: |
  Remove UsbDevice struct and list_devices() function from both
  the macOS impl (lines 12-61) and the non-macOS stub (lines 225-233).
  Keep connect_device() and query_device_name() which are actively used.
```

```yaml
id: TRANS-002
label: D
location: src/transport/adb.rs:6-15
title: Port cycling magic numbers lack conceptual naming
evidence: |
  static PORT_COUNTER: AtomicU16 = AtomicU16::new(0);
  const PORT_BASE: u16 = 19753;
  const PORT_RANGE: u16 = 10000; // cycle through 19753..29752
  
  let offset = PORT_COUNTER.fetch_add(1, Ordering::Relaxed);
  let local_port = PORT_BASE + (offset % PORT_RANGE);
  
  The magic numbers 19753 and 10000 represent "avoid OS ephemeral ports"
  and "cycle pool size" but the concept ("adb local port pool") is not
  extracted as a named type or const grouping.
risk: low
proposed_action: |
  Create a named configuration struct or module-level documentation
  explaining why this port range was chosen (avoids 1024-5000 user ports,
  stays below 32768 ephemeral floor on most systems). Consider:
  - `const ADB_LOCAL_PORT_POOL_BASE: u16 = 19753;`
  - `const ADB_LOCAL_PORT_POOL_SIZE: u16 = 10000;`
  And document why 19753 (0x4d09) in a comment block.
```

```yaml
id: TRANS-003
label: A
location: src/transport/device_monitor.rs:654
title: device_monitor.rs at 654 lines is yellow but cohesive
evidence: |
  Four inline modules within one file:
  - DeviceTracker (shared helper, ~40 lines)
  - adb_source (adb track-devices logic, ~160 lines)
  - usbmuxd_source (Listen protocol + Attached/Detached, ~180 lines)
  - local_source (TCP probe + WS handshake, ~170 lines)
  
  Each source is self-contained and shares DeviceTracker pattern only.
  No cross-cutting logic. Header comments document the invariants
  ("every Added has matching Removed", "drain on disconnect").
risk: low
proposed_action: |
  Keep as-is. The 654-line total is justified because:
  1. Each source is a self-contained state machine (Add/Remove lifecycle)
  2. Shared DeviceTracker logic is minimal and benefits from proximity
  3. Splitting into separate files would scatter the "one device stream"
     abstraction and make the invariant harder to verify.
  In Phase 3, if UX changes require new discovery features, this becomes
  a natural split point: transport/adb.rs could contain adb_source,
  transport/usbmuxd_source.rs could contain the Listen logic, etc.
```

```yaml
id: TRANS-004
label: D
location: src/input/connector.rs:28-62
title: ConnectorHandle leaks message-type specifics; no abstraction for "send downstream message"
evidence: |
  pub fn send_mock_sync(&self, rules_json: String) { ... }
  pub fn send_replay(&self, method, url, headers, body) { ... }
  pub fn send_subscribe(&self) { ... }
  
  Each method constructs a ServerMessage variant, serializes, and sends.
  The connector knows about mock rules, replay, and subscribe semantics
  at the API level. If a new downstream message type is added (e.g.,
  ServerMessage::UpdateConfig), a new send_* method must be added here.
risk: low
proposed_action: |
  Extract a generic "send_downstream" or "send_server_message" method:
  
  impl ConnectorHandle {
      pub fn send(&self, msg: ServerMessage) -> bool {
          if let Ok(json) = serde_json::to_string(&msg) {
              self.tx.send(json).is_ok()
          } else {
              false
          }
      }
  }
  
  Then expose convenience methods (send_mock_sync, etc.) as thin wrappers.
  This reduces coupling between connector and protocol layer and makes
  the message types the source of truth for downstream API.
```

```yaml
id: TRANS-005
label: A
location: src/input/connector.rs:108-135
title: Hello timeout error message misattributes failure root cause
evidence: |
  let client_info = match tokio::time::timeout(
      std::time::Duration::from_secs(3), ws_read.next())
      .await
  {
      Ok(Some(Ok(Message::Text(text)))) => {
          match serde_json::from_str::<ClientMessage>(&text) {
              Ok(ClientMessage::Hello { ... }) => ClientInfo { ... },
              _ => return Err("First message was not Hello".into()),
          }
      }
      Ok(_) => return Err("No Hello received".into()),
      Err(_) => return Err("Hello timeout (not a flog server?)".into()),
  };
  
  The Err(_) branch conflates timeout with "no flog server". If a
  flog_dart instance takes > 3s to send Hello (e.g., device is slow
  or busy), the error message is misleading. Also, binary/non-text frames
  silently fail without distinguishing from timeout.
risk: medium
proposed_action: |
  Behavior is correct (3s timeout is intentional). Only the error message
  text is wrong. In Phase 3:
  - "Hello timeout (not a flog server?)" → just "Hello handshake timed out
    after 3s (port may not be a flog server)".
  - "First message was not Hello" → preserve type info: "Expected Hello,
    got {variant}".
  - Binary/non-text frames: separate error "Expected text frame, got binary".
  No timeout change. No retry logic change.
```

```yaml
id: TRANS-006
label: D
location: src/input/connector.rs:140-164
title: Reader/writer task spawn creates fire-and-forget async tasks with no monitoring
evidence: |
  // Spawn writer task
  tokio::spawn(async move {
      while let Some(json) = cmd_rx.recv().await {
          if ws_sink.send(Message::Text(json.into())).await.is_err() {
              break;
          }
      }
  });
  
  // Spawn reader task
  let event_tx_clone = event_tx.clone();
  tokio::spawn(async move {
      while let Some(msg_result) = ws_read.next().await {
          match msg_result {
              Ok(Message::Text(text)) => { ... }
              Ok(Message::Close(_)) => break,
              Err(_) => break,
              _ => {}
          }
      }
      let _ = event_tx_clone.send(ConnectorEvent::Disconnected);
  });
  
  If reader/writer tasks panic or encounter unhandled error types,
  there is no visibility. The Disconnected event is only sent by the
  reader; if the writer panics, the connection stays open from the
  reader's perspective.
risk: medium
proposed_action: |
  Capture JoinHandles from tokio::spawn and store them in ConnectorHandle
  or return them to caller, so abnormal termination can be detected.
  Alternatively: wrap both tasks in a select! that propagates panic/abort.
  At minimum: log errors before break to aid debugging connection issues.
```

```yaml
id: TRANS-007
label: B
location: src/transport/device_monitor.rs:560-566
title: tcp_open uses Ok(Ok(_)) pattern; logic correct but fragile
evidence: |
  async fn tcp_open(port: u16) -> Option<u16> {
      let addr = format!("127.0.0.1:{}", port);
      match tokio::time::timeout(TCP_TIMEOUT, tokio::net::TcpStream::connect(&addr)).await {
          Ok(Ok(_)) => Some(port),
          _ => None,
      }
  }
  
  This is the corrected version (from commit db32426). The pattern
  Ok(Ok(_)) correctly unwraps (timeout result, connect result) tuple.
  However, this is a common source of confusion in async code and
  the check passes without explicit comment or type annotation.
risk: low
proposed_action: |
  Add a doc comment explaining the two-level Ok unwrapping, or extract
  into a helper with a clearer name:
  
  /// Connection succeeded (timeout did not fire, stream created).
  async fn is_port_open(port: u16) -> bool {
      tokio::time::timeout(TCP_TIMEOUT, TcpStream::connect(...))
          .await
          .map_or(false, |r| r.is_ok())
  }
  
  This reads "is port open" rather than "did we get Ok(Ok)"
```

```yaml
id: TRANS-008
label: A
location: src/transport/device_monitor.rs:307-330
title: Reconnect delays are magic constants without justification
evidence: |
  const RECONNECT_DELAY: Duration = Duration::from_secs(3);
  const ADB_MISSING_DELAY: Duration = Duration::from_secs(30);
  const SOCKET_MISSING_DELAY: Duration = Duration::from_secs(10);
  const POLL_INTERVAL: Duration = Duration::from_secs(1);
  const TCP_TIMEOUT: Duration = Duration::from_millis(200);
  const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(3);
  
  Values are correct (3s reconnect is standard, 30s for missing adb
  avoids tight loops, 1s poll is responsive for localhost) but their
  reasoning is embedded only in the code, not in design docs.
risk: low
proposed_action: |
  Add a module-level comment documenting the timing strategy:
  - POLL_INTERVAL: balances responsiveness (user sees new device)
    vs CPU cost. 1s is typical for long-polling.
  - TCP_TIMEOUT: 200ms is aggressive but safe on loopback;
    parallel probes (10 ports × 200ms = 2s total worst case).
  - HANDSHAKE_TIMEOUT: 3s accounts for device lockdown prompt.
  - RECONNECT_DELAY: 3s avoids hammering crashed adb server.
  - ADB_MISSING_DELAY: 30s when adb binary not found (user likely
    installing or missing PATH; tight loop wastes CPU).
```

```yaml
id: TRANS-009
label: D
location: src/main.rs:240-275
title: Cross-platform transport paths are responsibility-asymmetric
evidence: |
  Three paths in match device.connection_method():
  
  1. Localhost: connect(&url) → ws://127.0.0.1:port (direct)
  
  2. AdbForward: setup_forward() → get local_port → connect(&url)
     to ws://127.0.0.1:local_port (tunnel setup required)
  
  3. Usbmuxd: connect_device(uid, port) → UnixStream tunnel →
     connect_stream(tunnel, url) (tunnel + WS over tunnel)
  
  Asymmetry: Localhost and AdbForward both use connect(), but only
  AdbForward tracks cleanup (adb_forward_info). Usbmuxd uses
  connect_stream() and the tunnel is implicitly held by the stream.
  If connect_stream fails after connect_device succeeds, the tunnel
  stream is dropped (closed), leaving no resource leak but also
  no explicit cleanup semantics. AdbForward has a symmetric cleanup
  path (remove_forward) but Usbmuxd does not.
risk: medium
proposed_action: |
  Extract a Connection abstraction:
  
  enum Connection {
      Localhost { url: String },
      AdbForward { serial: String, local_port: u16 },
      UsbmuxdTunnel { stream: UnixStream },
  }
  
  impl Connection {
      async fn cleanup(&mut self) { match self { ... } }
  }
  
  This makes cleanup responsibility explicit and symmetrical.
  Usbmuxd could log/track tunnel lifecycle; AdbForward cleanup
  moves into the enum. Phase 3 will need this when adding connection
  pooling or advanced device lifecycle features.
```

```yaml
id: TRANS-010
label: A
location: src/main.rs:277-333
title: Inactive-app message drop is intentional but undocumented
evidence: |
  ConnectorEvent::Message(msg) => {
      if a.active_app_id.as_deref() == Some(task_key_c.as_str()) {
          dispatch_client_message(&mut a, msg);
      }
  }
  
  Messages from inactive apps are silently discarded. Is this:
  A. Intentional optimization (buffer them on Dart side, only replay
     on subscribe)?
  B. A bug (passive loss of data from background apps)?
  C. Deferred buffering (messages are supposed to flow through
     FlogStore)?
  
  The comment refers to "main.rs owns the real long-lived session"
  but doesn't clarify whether inactive-app messages are dropped or
  buffered server-side.
risk: low
proposed_action: |
  Behavior is correct: per the Direct Socket architecture, each Dart app
  owns a FlogStore buffer; switching active app triggers subscribe() which
  replays buffered messages. TUI only dispatches from currently active app
  by design. No code change. In Phase 3 add a why-comment at the dispatch
  branch pointing to Direct Socket architecture doc:
    // Inactive-app messages are intentionally dropped here. Each flog_dart
    // instance buffers its own log/network entries via FlogStore; when the
    // user switches active_app_id we subscribe() which replays the buffer.
  This is a Phase 4 "why-comment" candidate (mark @COMMENT-WORTHY).
```

```yaml
id: TRANS-011
label: A
location: src/main.rs:235-345
title: Retry loop with exponential backoff is correct but unmonitored
evidence: |
  let mut retry_delay_secs: u64 = 2;
  loop {
      let ws_result = match device.connection_method() { ... };
      
      if let Ok((mut event_rx, handle)) = ws_result {
          retry_delay_secs = 2;
          while let Some(evt) = event_rx.recv().await { ... }
      }
      
      tokio::time::sleep(Duration::from_secs(retry_delay_secs)).await;
      retry_delay_secs = (retry_delay_secs * 2).min(30);
  }
  
  Backoff strategy is sound (2→4→8→16→30s cap). However, there is
  no visibility into how many retries occurred, whether a device is
  in a "stuck retrying" state, or metrics to trigger alerts. If a
  device becomes unavailable, the task silently loops forever.
risk: low
proposed_action: |
  Add logging and optional metrics:
  - Log retry attempt number and delay on each sleep.
  - Consider a max-retry threshold (e.g., 10 consecutive failures)
    to emit a warning or remove the device from UI.
  - Add a retry_count field to ConnectedApp for UI visibility.
  This is a Phase 3 observability improvement; current behavior is
  correct but lacks operational visibility.
```

```yaml
id: TRANS-012
label: D
location: src/input/protocol.rs:20-84
title: ServerMessage and ClientMessage variants are not validated for completeness
evidence: |
  pub enum ServerMessage {
      MockSync { rules: String },
      Replay { method, url, headers, body },
      Subscribe {},
  }
  
  pub enum ClientMessage {
      Hello { ... },
      Log { ... },
      Net { msg: FlogNetMessage },
  }
  
  New message types can be added to protocol without updating tests,
  handlers, or serialization code. serde(tag = "type") ensures JSON
  format is stable, but there is no exhaustiveness check at compile
  time to ensure all message handlers in main.rs are updated.
risk: low
proposed_action: |
  Phase 3: add #[non_exhaustive] to enum definitions (or leave off
  if intentionally allowing extension). Add a compile_fail test that
  forces a match to be exhaustive:
  
  #[test]
  fn all_server_messages_serializable() {
      let msgs = [
          ServerMessage::MockSync { ... },
          ServerMessage::Replay { ... },
          ServerMessage::Subscribe {},
      ];
      for msg in &msgs {
          let _json = serde_json::to_string(msg).unwrap();
      }
  }
  
  This catches missing handlers at compile time during Phase 3 refactor.
```

```yaml
id: TRANS-013
label: E
location: src/replay.rs:1-7
title: Replay module is marked dead_code but documentation explains why
evidence: |
  #![allow(dead_code)]
  
  //! HTTP request replay — resends a captured NetworkEntry via reqwest.
  //!
  //! Note: With the Direct Socket architecture, replay is handled by the
  //! flog_dart client via ServerMessage::Replay. This module is preserved
  //! for potential server-side replay functionality in the future.
  
  Module is intentionally preserved but unused. The #![allow(dead_code)]
  suppresses warnings correctly. However, the entire module being dead
  is the real finding — it's archived code that should be versioned
  in git history, not kept in the source tree.
risk: low
proposed_action: |
  Move src/replay.rs to docs/archived/replay.rs.v0.6.1 and remove from
  Cargo.toml. If server-side replay is needed in future, git history
  preserves the implementation. This unclutters the codebase and makes
  it clear to new contributors that this is not an active module.
```

```yaml
id: TRANS-014
label: D
location: src/input/protocol.rs:40-56
title: ClientInfo struct missing session/identity metadata
evidence: |
  pub struct ClientInfo {
      pub id: ClientId,
      pub app: String,
      pub app_version: String,
      pub os: String,
      pub package_name: String,
      pub port: u16,
      pub build_mode: String,
      pub connected_at: std::time::Instant,
  }
  
  No field for device_id, session_id, or protocol_version. If a
  flog_dart server version becomes incompatible in the future,
  or if the protocol needs versioning, there's no clean way to
  extract that from the Hello frame and store it for later
  reference (e.g., in device picker UI).
risk: low
proposed_action: |
  Add optional fields to ClientInfo:
  - `protocol_version: Option<String>` (from Hello, for compatibility checks)
  - `session_id: Option<String>` (if Dart generates one for restart detection)
  - `device_id: Option<String>` (from HelloDiscover or transport layer)
  
  These enable future features like "warn if flog_dart is newer"
  or "reconnect to same session on app restart" without breaking
  the protocol. Make them #[serde(default)] to maintain backward
  compatibility with older flog_dart versions.
```

```yaml
id: TRANS-015
label: A
location: src/main.rs:199-394
title: Device discovery task lacks error recovery for malformed device events
evidence: |
  while let Some(event) = device_rx.recv().await {
      match event {
          transport::DeviceEvent::Added(device) => { ... }
          transport::DeviceEvent::Removed(id) => { ... }
      }
  }
  
  If device_rx channel breaks unexpectedly (discovery task crashes),
  the match arm is never entered and the loop silently exits. The UI
  continues showing stale devices and new device events are never
  processed. No backpressure signal or error handling.
risk: low
proposed_action: |
  Wrap the while loop in error handling and optionally restart
  discovery:
  
  if device_rx.recv().is_none() {
      eprintln!("Device discovery channel closed unexpectedly");
      // Option 1: restart discovery (re-run start_discovery)
      // Option 2: emit UI error + give user retry button
      // Current: silent failure (acceptable if discovery task is
      // explicitly required to stay alive)
  }
  
  Add a doc comment clarifying the invariant: "discovery task must
  never exit; if it does, the app becomes unable to detect new devices."
```

```yaml
id: TRANS-100
label: D
location: src/transport/adb.rs:13-34
title: setup_forward mixes pure port allocation with adb shell-out
evidence: |
  setup_forward() computes a local port via PORT_COUNTER / PORT_BASE /
  PORT_RANGE, then shells out to `adb -s <serial> forward tcp:X tcp:Y`.
  Because the entire function is async + process-exec, the pure port
  allocation arithmetic cannot be unit-tested without running `adb`.
proposed_action: |
  Phase 2.5B Task 11 extracted `next_local_port(offset)` and
  `allocate_local_port()` as pure helpers. Phase 3 opportunity: go
  further and return a `Reservation { local_port, remove: impl FnOnce }`
  value so callers can mock the shell-out via an injected `AdbClient`
  trait. Keeps the real path ergonomic while enabling fault injection
  in higher-level transport tests.
```

```yaml
id: TRANS-101
label: D
location: src/transport/usbmuxd.rs:14-33
title: connect_device plist encoding is coupled to UnixStream I/O
evidence: |
  connect_device() both builds the Connect plist dict and writes it
  onto a UnixStream. Tests of the wire format previously had to
  duplicate the dict construction. Phase 2.5B Task 11 extracted
  build_connect_request(), connect_port_field(), encode_plist_frame(),
  and decode_plist_header() so frame layout is directly testable.
proposed_action: |
  Phase 3: similarly factor lockdown_get_value() into a pure
  `encode_get_value_request(key)` + `parse_get_value_response(bytes)`
  pair so the big-endian 4-byte lockdownd framing can be tested
  without a live device. Currently the lockdownd path is UNTESTABLE: PHYS.
```

```yaml
id: TRANS-102
label: D
location: src/transport/device_monitor.rs:385-423 (pre-refactor)
title: device_name couples getprop shell-out with display-name formatting
evidence: |
  device_name(serial) called getprop four times and interleaved the
  output formatting (emulator label, brand+model dedup) with I/O, so
  the formatting rules could only be tested by faking /usr/bin/adb.
  Phase 2.5B Task 11 extracted `emulator_name()` and
  `real_device_name()` as pure helpers; the async wrapper now only
  handles the getprop shell-out.
proposed_action: |
  No further action required for correctness. Pattern: treat the
  extracted helpers as the canonical spec for display-name rules;
  any future product changes (e.g., new emulator flavor, regional
  SKU) should modify the pure helper + its tests, not the async
  wrapper.
```

```yaml
id: TRANS-103
label: D
location: src/transport/device_monitor.rs:793-805 (pre-refactor)
title: read_message hardcoded to tokio::net::unix::OwnedReadHalf
evidence: |
  read_message() took a concrete &mut OwnedReadHalf, making it
  impossible to drive in-memory without a real usbmuxd socket. Phase
  2.5B Task 11 extracted `read_message_any<R: AsyncRead>()` and had
  the UnixStream wrapper delegate to it, enabling Cursor-backed
  unit tests of truncated/non-dict/valid-dict frames.
proposed_action: |
  Phase 3: consider applying the same generic-over-AsyncRead pattern
  to connector.rs (the WS reader task, TRANS-006) so reader task
  failure modes become unit-testable without a running server.
```

```yaml
id: TRANS-104
label: D
location: src/transport/device_monitor.rs:766-791 (pre-refactor)
title: send_listen mixes plist XML serialization with UnixStream writes
evidence: |
  send_listen() constructed the Listen request dict, serialized it to
  XML plist, composed the 16-byte header, and then wrote both in one
  async function. The wire-format assertions required parsing bytes
  back out of the socket. Phase 2.5B Task 11 extracted
  `encode_listen_frame()` so the header fields (length/version/type/tag)
  and body dict can be verified independently.
proposed_action: |
  No further action — the encode path is now fully covered by unit
  tests and the UnixStream write path is correctly annotated
  UNTESTABLE: PHYS.
```

```yaml
id: TRANS-105
label: D
location: src/transport/device_monitor.rs:631-656 (pre-refactor)
title: booted_simulator_name couples xcrun exec with JSON traversal
evidence: |
  booted_simulator_name() shelled out to `xcrun simctl list devices
  booted --json` and parsed the nested `devices → runtime → [devs]`
  structure inline. Phase 2.5B Task 11 extracted `parse_simctl_booted`
  as a pure helper, making the JSON traversal testable with fixture
  bytes (valid booted device, no booted, malformed JSON, non-array
  runtime entry).
proposed_action: |
  No further action. The shell-out path is correctly UNTESTABLE: PHYS.
```

## Summary

| label | count |
|---|---|
| A | 6 |
| B | 1 |
| C | 0 |
| D | 12 |
| E | 2 |
