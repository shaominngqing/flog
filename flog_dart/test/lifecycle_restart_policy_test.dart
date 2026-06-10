import 'package:flutter/widgets.dart' show AppLifecycleState;
import 'package:flutter_test/flutter_test.dart';

import 'package:flog_dart/src/lifecycle_restart_policy.dart';

void main() {
  group('FlogLifecycleRestartPolicy', () {
    test('resumed_without_prior_paused_does_not_restart', () {
      final policy = FlogLifecycleRestartPolicy();

      expect(policy.shouldRestart(AppLifecycleState.resumed), isFalse);
    });

    test('inactive_then_resumed_does_not_restart', () {
      final policy = FlogLifecycleRestartPolicy();

      expect(policy.shouldRestart(AppLifecycleState.inactive), isFalse);
      expect(policy.shouldRestart(AppLifecycleState.resumed), isFalse);
    });

    test('paused_then_resumed_restarts_once', () {
      final policy = FlogLifecycleRestartPolicy();

      expect(policy.shouldRestart(AppLifecycleState.paused), isFalse);
      expect(policy.shouldRestart(AppLifecycleState.resumed), isTrue);
      expect(policy.shouldRestart(AppLifecycleState.resumed), isFalse);
    });

    test('paused_survives_intermediate_inactive_until_resumed', () {
      final policy = FlogLifecycleRestartPolicy();

      expect(policy.shouldRestart(AppLifecycleState.paused), isFalse);
      expect(policy.shouldRestart(AppLifecycleState.inactive), isFalse);
      expect(policy.shouldRestart(AppLifecycleState.resumed), isTrue);
    });
  });
}
