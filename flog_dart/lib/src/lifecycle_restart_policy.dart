/// Lifecycle restart policy for the embedded flog WebSocket server.
///
/// Mobile suspend/resume can leave app-owned sockets stale, but desktop
/// focus changes often produce `inactive -> resumed` without suspension.
/// Restart only after a real `paused` state so macOS focus does not drop
/// the active flog connection.
library;

import 'package:flutter/widgets.dart' show AppLifecycleState;

class FlogLifecycleRestartPolicy {
  bool _sawPaused = false;

  bool shouldRestart(AppLifecycleState state) {
    if (state == AppLifecycleState.paused) {
      _sawPaused = true;
      return false;
    }

    if (state == AppLifecycleState.resumed) {
      final shouldRestart = _sawPaused;
      _sawPaused = false;
      return shouldRestart;
    }

    return false;
  }
}
