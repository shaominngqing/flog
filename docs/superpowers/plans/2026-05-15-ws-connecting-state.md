# WebSocket Connecting State Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Emit a `connecting` flog_net frame the moment a WebSocket handshake begins so the TUI shows a Pending entry immediately, before the handshake succeeds or fails.

**Architecture:** Three changes in lockstep: (1) Dart emits `{t:'connecting'}` at the top of `_connectAndWrap`; (2) Rust deserializes it into a new `FlogNetKind::Connecting` variant and creates a `NetworkStatus::Pending` WS entry; (3) `handle_open` is updated to upgrade the existing Pending entry to Active instead of always pushing a new entry. `fromChannel` is unchanged (emits `open` directly — the channel is already established).

**Tech Stack:** Dart (flog_dart), Rust (flog TUI), `serde_json`, `cargo test`, `dart test`

---

## File Map

| File | Operation | Responsibility |
|------|-----------|----------------|
| `flog_dart/lib/src/flog_web_socket.dart` | Modify | Emit `connecting` frame at start of `_connectAndWrap` |
| `src/domain/network.rs` | Modify | Add `Connecting` variant to `FlogNetKind`; add arm to `id()` |
| `src/domain/network_store.rs` | Modify | Add `handle_connecting`; update `handle_open` to upsert |
| `src/domain/network_tests.rs` | Modify | Tests for `handle_connecting` and updated `handle_open` |
| `flog_dart/test/flog_web_socket_connect_test.dart` | Modify | Assert `connecting` frame emitted before `open`/`err` |

---

## Task 1: Add `Connecting` variant to `FlogNetKind` and update `id()`

**Files:**
- Modify: `src/domain/network.rs`

Context: `FlogNetKind` is a `#[serde(tag = "t", rename_all = "lowercase")]` enum at line ~260. The `Open` variant (line ~332) has the same fields we need. The `id()` method (line ~373) has one arm per variant.

- [ ] **Step 1: Add `Connecting` variant after `Open` in the enum**

In `src/domain/network.rs`, after the `Open` variant block:

```rust
    /// WebSocket open.
    Open {
        id: u64,
        #[serde(default)]
        url: Option<String>,
        #[serde(default)]
        ts: Option<u64>,
    },
```

Insert immediately after:

```rust
    /// WebSocket handshake started — not yet complete.
    /// TUI shows a Pending entry. Followed by `Open` (success) or `Err` (failure).
    Connecting {
        id: u64,
        #[serde(default)]
        url: Option<String>,
        #[serde(default)]
        ts: Option<u64>,
    },
```

- [ ] **Step 2: Add arm to `id()` method**

In the `id()` impl, the current final arm is `Self::Close { id, .. } => *id`. Add `Self::Connecting` to the chain. The full match should now end with:

```rust
            | Self::Open { id, .. }
            | Self::Connecting { id, .. }
            | Self::Send { id, .. }
            | Self::Recv { id, .. }
            | Self::Close { id, .. } => *id,
```

- [ ] **Step 3: Verify it compiles**

```bash
cd /Users/shaomingqing/FlutterProject/flog
cargo build 2>&1 | grep -E "error|warning.*unused"
```

Expected: `warning` about `Connecting` being unmatched in `process_message` (that's fine — we fix it in Task 2). No `error` lines.

- [ ] **Step 4: Commit**

```bash
git add src/domain/network.rs
git commit -m "feat(net): add FlogNetKind::Connecting variant for WS handshake-start"
```

---

## Task 2: Add `handle_connecting` + update `handle_open` in `network_store.rs`

**Files:**
- Modify: `src/domain/network_store.rs`

Context: `process_message` match block is at line ~29. `handle_open` is at line ~240 and currently always does `push_back` unconditionally. `find_by_id_mut` (line ~125) returns `Option<&mut NetworkEntry>`. `NetworkEntry::new_ws` creates an entry with `status = NetworkStatus::Active` by default (confirmed by test `dom_024_new_ws_defaults`).

- [ ] **Step 1: Write failing tests first** (in `src/domain/network_tests.rs`)

Add at the end of the file, before the closing `}`:

```rust
// ---- WS connecting state (2026-05-15 spec) -----------------------

#[test]
fn connecting_creates_pending_ws_entry() {
    let mut store = NetworkStore::new();
    store.process_message(FlogNetKind::Connecting {
        id: 99,
        url: Some("wss://host/ws".into()),
        ts: None,
    });
    assert_eq!(store.len(), 1);
    let e = store.get(0).unwrap();
    assert_eq!(e.id, 99);
    assert_eq!(e.protocol, Protocol::Ws);
    assert_eq!(e.status, NetworkStatus::Pending);
    assert_eq!(e.url, "wss://host/ws");
}

#[test]
fn open_after_connecting_upgrades_to_active() {
    let mut store = NetworkStore::new();
    store.process_message(FlogNetKind::Connecting {
        id: 99,
        url: Some("wss://host/ws".into()),
        ts: None,
    });
    store.process_message(FlogNetKind::Open {
        id: 99,
        url: Some("wss://host/ws".into()),
        ts: None,
    });
    // Must not create a second entry
    assert_eq!(store.len(), 1);
    let e = store.get(0).unwrap();
    assert_eq!(e.status, NetworkStatus::Active);
}

#[test]
fn open_without_prior_connecting_still_creates_active_entry() {
    // Backward-compat: fromChannel / old Dart that emits open without connecting.
    let mut store = NetworkStore::new();
    store.process_message(FlogNetKind::Open {
        id: 42,
        url: Some("wss://host/ws".into()),
        ts: None,
    });
    assert_eq!(store.len(), 1);
    let e = store.get(0).unwrap();
    assert_eq!(e.id, 42);
    assert_eq!(e.status, NetworkStatus::Active);
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cd /Users/shaomingqing/FlutterProject/flog
cargo test connecting_creates_pending_ws_entry open_after_connecting open_without_prior -- --nocapture 2>&1 | tail -20
```

Expected: compilation error (non-exhaustive match on `FlogNetKind::Connecting` in `process_message`). That confirms the tests are wired correctly.

- [ ] **Step 3: Add `Connecting` arm to `process_message`**

In `src/domain/network_store.rs`, in the `process_message` match block, after the `FlogNetKind::Open` arm:

```rust
            FlogNetKind::Open { id, url, ts } => self.handle_open(id, url, ts),
            FlogNetKind::Connecting { id, url, ts } => self.handle_connecting(id, url, ts),
```

- [ ] **Step 4: Add `handle_connecting` method**

After the `handle_open` method, insert:

```rust
    fn handle_connecting(&mut self, id: u64, url: Option<String>, ts: Option<u64>) {
        self.ensure_capacity();
        let url = url.unwrap_or_default();
        let mut entry = NetworkEntry::new_ws(id, url, String::new());
        entry.status = NetworkStatus::Pending;
        if let Some(t) = ts {
            entry.timestamp = format_ts(t);
        }
        self.entries.push_back(entry);
    }
```

- [ ] **Step 5: Update `handle_open` to upsert**

Replace the current `handle_open` body:

```rust
    fn handle_open(&mut self, id: u64, url: Option<String>, ts: Option<u64>) {
        self.ensure_capacity();

        let url = url.unwrap_or_default();
        let mut entry = NetworkEntry::new_ws(id, url, String::new());
        if let Some(t) = ts {
            entry.timestamp = format_ts(t);
        }
        self.entries.push_back(entry);
    }
```

With:

```rust
    fn handle_open(&mut self, id: u64, url: Option<String>, ts: Option<u64>) {
        if let Some(entry) = self.find_by_id_mut(id) {
            // Upgrade a Pending entry created by a prior `connecting` frame.
            entry.status = NetworkStatus::Active;
            if let Some(u) = url {
                if !u.is_empty() {
                    entry.url = u;
                }
            }
            if let Some(t) = ts {
                entry.timestamp = format_ts(t);
            }
        } else {
            // Backward-compat: `fromChannel` or old Dart that emits `open`
            // without a prior `connecting` frame.
            self.ensure_capacity();
            let url = url.unwrap_or_default();
            let mut entry = NetworkEntry::new_ws(id, url, String::new());
            if let Some(t) = ts {
                entry.timestamp = format_ts(t);
            }
            self.entries.push_back(entry);
        }
    }
```

- [ ] **Step 6: Run tests — expect all three to pass**

```bash
cd /Users/shaomingqing/FlutterProject/flog
cargo test connecting_creates_pending_ws_entry open_after_connecting open_without_prior -- --nocapture 2>&1 | tail -20
```

Expected:
```
test connecting_creates_pending_ws_entry ... ok
test open_after_connecting_upgrades_to_active ... ok
test open_without_prior_connecting_still_creates_active_entry ... ok
```

- [ ] **Step 7: Run full test suite**

```bash
cargo test 2>&1 | tail -10
```

Expected: `test result: ok. N passed; 0 failed`

- [ ] **Step 8: Commit**

```bash
git add src/domain/network_store.rs src/domain/network_tests.rs
git commit -m "feat(net): handle_connecting creates Pending WS entry; handle_open upserts"
```

---

## Task 3: Emit `connecting` frame in Dart `_connectAndWrap`

**Files:**
- Modify: `flog_dart/lib/src/flog_web_socket.dart`

Context: `_connectAndWrap` is at line ~87. `id` and `start` are assigned before the try block. `flogEnabled` and `emitNet` are imported from `flog_net.dart`. The `open` frame is emitted inside `_initFromChannel` (called after the try block on the success path).

- [ ] **Step 1: Write failing Dart test first**

In `flog_dart/test/flog_web_socket_connect_test.dart`, in the `'FlogWebSocket.wrap — failure path'` group, add a new test after the existing tests:

```dart
    test('emits connecting frame before open on success', () async {
      FlogServer.instance.start(port: 19754);

      // Use a channel that succeeds (ready completes normally).
      await FlogWebSocket.wrap(
        () async => _SucceedingChannel(),
        url: 'wss://success.example.com/ws',
      );

      final frames = FlogStore.instance.snapshotForTesting
          .where((f) => f['type'] == 'net' && f['p'] == 'ws')
          .toList();

      // Must have at least connecting + open
      expect(frames.length, greaterThanOrEqualTo(2));

      // First ws frame for this url must be connecting
      final wsFrames = frames
          .where((f) => f['url'] == 'wss://success.example.com/ws' ||
              (f['t'] == 'connecting' || f['t'] == 'open'))
          .toList();

      final connectingIdx =
          frames.indexWhere((f) => f['t'] == 'connecting');
      final openIdx = frames.indexWhere((f) => f['t'] == 'open');

      expect(connectingIdx, greaterThanOrEqualTo(0),
          reason: 'connecting frame must exist');
      expect(openIdx, greaterThanOrEqualTo(0),
          reason: 'open frame must exist');
      expect(connectingIdx, lessThan(openIdx),
          reason: 'connecting must come before open');
    });

    test('emits connecting frame before err on failure', () async {
      FlogServer.instance.start(port: 19755);
      final err = Exception('refused');

      await expectLater(
        FlogWebSocket.wrap(() async => throw err, url: 'wss://fail.example.com/ws'),
        throwsA(isA<Exception>()),
      );

      final frames = FlogStore.instance.snapshotForTesting
          .where((f) => f['type'] == 'net' && f['p'] == 'ws')
          .toList();

      final connectingIdx =
          frames.indexWhere((f) => f['t'] == 'connecting');
      final errIdx = frames.indexWhere((f) => f['t'] == 'err');

      expect(connectingIdx, greaterThanOrEqualTo(0),
          reason: 'connecting frame must exist');
      expect(errIdx, greaterThanOrEqualTo(0),
          reason: 'err frame must exist');
      expect(connectingIdx, lessThan(errIdx),
          reason: 'connecting must come before err');
    });
```

Also add the `_SucceedingChannel` helper at the bottom of the file (after `_NullSink`):

```dart
/// A [WebSocketChannel] whose [ready] future completes normally.
class _SucceedingChannel implements WebSocketChannel {
  @override
  Future<void> get ready => Future.value();

  @override
  Stream<dynamic> get stream => const Stream.empty();

  @override
  WebSocketSink get sink => _NullSink();

  @override
  String? get protocol => null;

  @override
  int? get closeCode => null;

  @override
  String? get closeReason => null;
}
```

- [ ] **Step 2: Run test to confirm it fails**

```bash
cd /Users/shaomingqing/FlutterProject/flog/flog_dart
dart test test/flog_web_socket_connect_test.dart --reporter expanded 2>&1 | grep -E "FAIL|PASS|Error" | head -20
```

Expected: the two new tests FAIL (connecting frame not yet emitted).

- [ ] **Step 3: Add `connecting` emit to `_connectAndWrap`**

In `flog_dart/lib/src/flog_web_socket.dart`, in `_connectAndWrap`, after `final start = DateTime.now();` and before `WebSocketChannel channel;`, insert:

```dart
    if (flogEnabled) {
      emitNet({'id': id, 't': 'connecting', 'p': 'ws', 'url': url});
    }
```

The complete top of `_connectAndWrap` should now read:

```dart
  static Future<FlogWebSocket> _connectAndWrap(
    Future<WebSocketChannel> Function() connect, {
    required String url,
  }) async {
    final id = nextNetId();
    final start = DateTime.now();

    if (flogEnabled) {
      emitNet({'id': id, 't': 'connecting', 'p': 'ws', 'url': url});
    }

    WebSocketChannel channel;
    try {
```

- [ ] **Step 4: Run Dart tests — expect new tests to pass**

```bash
cd /Users/shaomingqing/FlutterProject/flog/flog_dart
dart test test/flog_web_socket_connect_test.dart --reporter expanded 2>&1 | grep -E "FAIL|PASS|Error"
```

Expected: all tests PASS.

- [ ] **Step 5: Run full Dart test suite**

```bash
cd /Users/shaomingqing/FlutterProject/flog/flog_dart
dart test --reporter expanded 2>&1 | tail -15
```

Expected: `All tests passed!`

- [ ] **Step 6: Commit**

```bash
cd /Users/shaomingqing/FlutterProject/flog/flog_dart
git add lib/src/flog_web_socket.dart test/flog_web_socket_connect_test.dart
git commit -m "feat(ws): emit connecting frame at handshake start — TUI shows Pending immediately"
```

---

## Task 4: Full regression + build check

**Files:** None new — run existing suites.

- [ ] **Step 1: Full Rust test suite**

```bash
cd /Users/shaomingqing/FlutterProject/flog
cargo test 2>&1 | tail -10
```

Expected: `test result: ok. N passed; 0 failed`

- [ ] **Step 2: Rust release build**

```bash
cargo build --release 2>&1 | grep -E "^error"
```

Expected: no output (zero errors).

- [ ] **Step 3: Install updated flog binary**

```bash
cargo install --path . 2>&1 | tail -5
```

- [ ] **Step 4: aura-lang-flutter compile check**

```bash
cd /Users/shaomingqing/FlutterProject/aura-lang-flutter
flutter pub get && flutter analyze 2>&1 | tail -10
```

Expected: `No issues found!`

- [ ] **Step 5: Final commit (Rust side only if any cleanup needed)**

If no changes were needed, skip. If minor fixups landed since Task 2's commit:

```bash
cd /Users/shaomingqing/FlutterProject/flog
git add -p
git commit -m "chore: regression-pass cleanup after connecting-state feature"
```

---

## Self-Review

**Spec coverage:**

| Spec requirement | Task |
|-----------------|------|
| `connecting` frame emitted before any `await` in `_connectAndWrap` | Task 3 |
| `FlogNetKind::Connecting` variant deserialized by TUI | Task 1 |
| `handle_connecting` creates `NetworkStatus::Pending` WS entry | Task 2 |
| `handle_open` upgrades existing Pending entry (not push new) | Task 2 |
| `handle_open` still creates Active entry when no prior `connecting` (back-compat) | Task 2 |
| `fromChannel` unchanged (emits `open` directly) | No change needed — `_initFromChannel` path unmodified |
| Tests for `handle_connecting` and updated `handle_open` | Task 2 |
| Dart tests: `connecting` before `open`; `connecting` before `err` | Task 3 |
| All existing tests pass | Task 4 |

**Placeholder scan:** None found.

**Type consistency:** `FlogNetKind::Connecting { id, url, ts }` defined in Task 1 and matched in Task 2 `process_message` — field names match. `NetworkStatus::Pending` used in Task 2 `handle_connecting` and asserted in Task 2 test — consistent.
