/// Internal helper for flog_net protocol.
library;

import 'dart:convert';
import 'dart:developer' as developer;

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
/// Uses both print() (for ADB/stdin) and developer.log() (for VM Service on all platforms).
void emitNet(Map<String, dynamic> data) {
  if (!flogEnabled) return;
  final json = jsonEncode(data);
  // print() for ADB logcat and stdin pipe
  // ignore: avoid_print
  print('[INFO][$_tag] $json');
  // developer.log() for VM Service Logging stream (works on iOS real device)
  developer.log(json, name: _tag);
}
