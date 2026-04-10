/// Internal helper for flog_net protocol.
library;

import 'dart:convert';

const _tag = 'flog_net';

int _nextId = 1;

/// Get next unique request ID.
int nextNetId() => _nextId++;

/// Emit a flog_net protocol message.
/// Uses print() so it works with all flog data sources (VM Service, ADB, stdin).
void emitNet(Map<String, dynamic> data) {
  // ignore: avoid_print
  print('[INFO][$_tag] ${jsonEncode(data)}');
}
