/// Ring buffer that stores all flog messages (log + network) for replay.
///
/// When a new flog TUI client connects (or re-subscribes after a session
/// switch), [replayTo] iterates the buffer and sends every stored message.
/// Because Dart is single-threaded, no new messages can be produced during
/// the iteration — the transition from historical to live data is seamless.
library;

import 'dart:collection';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart' show visibleForTesting;

import 'flog_net.dart' show flogEnabled;

/// Singleton ring buffer for all outbound flog messages.
///
/// Messages are stored in insertion order regardless of type (`log` or `net`).
/// When the buffer reaches [capacity], the oldest entry is removed (true FIFO).
class FlogStore {
  static final FlogStore instance = FlogStore._();
  FlogStore._();

  /// Maximum number of messages retained in the ring buffer.
  ///
  /// Chosen to cover a typical Flutter dev-session of several hours
  /// without unbounded memory growth. At an average of ≈ 500 bytes per
  /// message (log line or flog_net frame), 50000 messages ≈ 25 MB of
  /// retained payload. The Rust TUI side uses a separate 100 000-entry
  /// log cap (see `src/domain/store.rs`); the Dart side is smaller on
  /// purpose because mobile devices are more memory-constrained than a
  /// developer workstation.
  ///
  /// Not currently tunable. A Phase 5 API may expose a constructor
  /// parameter if real apps need a larger window. (DART-020.)
  static const int defaultCapacity = 50000;

  /// Alias for [defaultCapacity] for back-compat with callers that still
  /// import `FlogStore.capacity`. New code should reference
  /// [defaultCapacity].
  static const int capacity = defaultCapacity;

  final Queue<Map<String, dynamic>> _buffer = ListQueue<Map<String, dynamic>>();

  /// Number of messages currently in the buffer.
  int get length => _buffer.length;

  /// Snapshot of all stored messages for tests.
  ///
  /// Returns an immutable list view of the internal buffer so tests can
  /// assert on emitted message shape without spinning up a WebSocket.
  /// Characterization-test-only; do not call from production code.
  @visibleForTesting
  List<Map<String, dynamic>> get snapshotForTesting =>
      List.unmodifiable(_buffer);

  /// Record a message into the ring buffer.
  ///
  /// Called by [FlogServer.send] for every log and network message.
  /// If the buffer is at capacity, the oldest message is evicted first.
  void record(Map<String, dynamic> message) {
    if (!flogEnabled) return;
    if (_buffer.length >= capacity) {
      _buffer.removeFirst();
    }
    _buffer.addLast(message);
  }

  /// Replay the entire buffer to [ws].
  ///
  /// Iterates all stored messages oldest-to-newest and sends each as JSON.
  /// Dart's single-threaded event loop guarantees that no new messages are
  /// produced (and therefore no buffer mutations occur) during this iteration.
  ///
  /// If the WebSocket throws during send (e.g. client disconnected), the
  /// replay stops early and the exception is silently caught.
  void replayTo(WebSocket ws) {
    for (final message in _buffer) {
      try {
        ws.add(jsonEncode(message));
      } catch (_) {
        break;
      }
    }
  }

  /// Clear all stored messages.
  void clear() {
    _buffer.clear();
  }
}
