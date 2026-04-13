/// Internal helper for flog_net protocol.
library;

import 'dart:convert';

/// Master kill-switch.  `dart.vm.product` is true in release builds,
/// so flogEnabled becomes false and AOT tree-shaking removes all flog code.
const flogEnabled = bool.fromEnvironment(
  'FLOG_ENABLED',
  defaultValue: !bool.fromEnvironment('dart.vm.product'),
);

const _tag = 'flog_net';

int _nextId = 1;

/// Get next unique request ID.
int nextNetId() => _nextId++;

/// Emit a flog_net protocol message.
void emitNet(Map<String, dynamic> data) {
  if (!flogEnabled) return;
  // ignore: avoid_print
  print('[INFO][$_tag] ${jsonEncode(data)}');
}
