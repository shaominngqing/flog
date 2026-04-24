/// Internal helper for flog_net protocol.
library;

import 'package:meta/meta.dart';

import 'flog_server.dart';

/// Master kill-switch (compile-time constant for AOT tree-shaking).
///
/// Resolution order:
/// 1. Explicit `--dart-define=FLOG_ENABLED=true|false` wins.
/// 2. Otherwise infer from `--dart-define=APP_FLAVOR=...`:
///    - `release` -> disabled (tree-shaken away)
///    - `alpha` / anything else -> enabled
/// 3. Fallback: enabled in non-product builds, disabled in product builds.
const _appFlavor = String.fromEnvironment('APP_FLAVOR');
const _isProduct = bool.fromEnvironment('dart.vm.product');
// 注意：必须用 == '' 判空，String.isEmpty/isNotEmpty 不是 const 表达式。
const flogEnabled = bool.fromEnvironment(
  'FLOG_ENABLED',
  defaultValue: _appFlavor == '' ? !_isProduct : _appFlavor != 'release',
);

int _nextId = 1;

/// Get next unique request ID.
///
/// **Internal to flog_dart.** Still exported from
/// `package:flog_dart/flog_dart.dart` for back-compat, but users should not
/// call this directly: a future v1.0 release will remove the export. See
/// DART-021.
@internal
int nextNetId() => _nextId++;

/// Emit a flog_net protocol message via Direct Socket.
///
/// **Internal to flog_dart.** Still exported from
/// `package:flog_dart/flog_dart.dart` for back-compat, but users should not
/// call this directly: a future v1.0 release will remove the export. See
/// DART-021.
@internal
///
/// Copies [data] before decorating with `type` / `ts`, so callers that
/// inspect or reuse their payload after the call do not see protocol
/// keys leak back in. (DART-009.)
void emitNet(Map<String, dynamic> data) {
  if (!flogEnabled) return;
  final out = <String, dynamic>{
    ...data,
    'type': 'net',
    'ts': DateTime.now().millisecondsSinceEpoch,
  };
  FlogServer.instance.send(out);
}
