/// Internal helper for flog_net protocol.
library;

import 'flog_server.dart';

/// Master kill-switch.  `dart.vm.product` is true in release builds,
/// so flogEnabled becomes false and AOT tree-shaking removes all flog code.
const flogEnabled = bool.fromEnvironment(
  'FLOG_ENABLED',
  defaultValue: !bool.fromEnvironment('dart.vm.product'),
);

int _nextId = 1;

/// Get next unique request ID.
int nextNetId() => _nextId++;

/// Emit a flog_net protocol message via Direct Socket.
void emitNet(Map<String, dynamic> data) {
  if (!flogEnabled) return;
  data['type'] = 'net';
  data['ts'] = DateTime.now().millisecondsSinceEpoch;
  FlogServer.instance.send(data);
}
