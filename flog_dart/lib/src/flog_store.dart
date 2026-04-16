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

import 'flog_net.dart' show flogEnabled;

/// Singleton ring buffer for all outbound flog messages.
///
/// Messages are stored in insertion order regardless of type (`log` or `net`).
/// When the buffer reaches [capacity], the oldest entry is removed (true FIFO).
class FlogStore {
  static final FlogStore instance = FlogStore._();
  FlogStore._();

  /// Maximum number of messages to retain.
  static const int capacity = 50000;

  final Queue<Map<String, dynamic>> _buffer = ListQueue<Map<String, dynamic>>();

  /// Number of messages currently in the buffer.
  int get length => _buffer.length;

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
