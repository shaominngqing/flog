# flog_dart WebSocket 握手失败覆盖 + back-compat 清理 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 `FlogWebSocket` 覆盖握手阶段失败，并清除 `flog_dart.dart` 里的 back-compat export 历史包袱。

**Architecture:** 删除无法 await 握手的同步构造器 `FlogWebSocket(Uri)`，新增 `FlogWebSocket.connect(Uri)` 和 `FlogWebSocket.wrap(fn, url:)` 两个异步静态方法，内部共享 `_connectAndWrap` 私有帮助函数处理 try/catch + emit 逻辑。同时从 `lib/flog_dart.dart` 删除 `nextNetId`/`emitNet`/`flogEnabled` 的 back-compat export。

**Tech Stack:** Dart, web_socket_channel, flog_dart 内部 `emitNet`/`nextNetId`/`flogEnabled`

---

## File Map

| 文件 | 操作 | 职责 |
|------|------|------|
| `~/.pub-cache/hosted/pub.flutter-io.cn/flog_dart-0.8.0/lib/src/flog_web_socket.dart` | Modify | 删旧构造器；加 `connect`/`wrap`/`_connectAndWrap` |
| `~/.pub-cache/hosted/pub.flutter-io.cn/flog_dart-0.8.0/lib/flog_dart.dart` | Modify | 删 back-compat export 行 |
| `~/.pub-cache/hosted/pub.flutter-io.cn/flog_dart-0.8.0/test/flog_web_socket_test.dart` | Modify/Create | 新增 connect/wrap 失败路径测试 |

> 注：flog_dart 是本地包，直接编辑 pub-cache 下的源码，然后在 aura-lang-flutter 里 `flutter pub get` 验证编译。

---

## Task 1: 删除同步构造器并清理 `fromChannel` 重复 emit

**Files:**
- Modify: `~/.pub-cache/hosted/pub.flutter-io.cn/flog_dart-0.8.0/lib/src/flog_web_socket.dart`

现有问题：`FlogWebSocket(Uri)` 同步构造器在 body 里 emit 了一次 open，然后 `_initFromChannel` 又 emit 一次 open（共两次）。`fromChannel` 只调 `_initFromChannel` 一次，没有重复问题。删掉同步构造器后，`_initFromChannel` 负责唯一的 open emit。

- [ ] **Step 1: 删除同步构造器**

将 `flog_web_socket.dart` 中的以下代码块整体删除（第 29–46 行）：

```dart
  /// Creates a [FlogWebSocket] that connects to [uri].
  ///
  /// Optional [protocols] are forwarded to [WebSocketChannel.connect].
  FlogWebSocket(Uri uri, {Iterable<String>? protocols})
      : _channel = WebSocketChannel.connect(uri, protocols: protocols),
        _id = nextNetId(),
        _start = DateTime.now() {
    if (flogEnabled) {
      emitNet({
        'id': _id,
        't': 'open',
        'p': 'ws',
        'url': uri.toString(),
      });
    }

    _initFromChannel(uri.toString());
  }
```

- [ ] **Step 2: 确认编译（预期失败，因为还没加新方法）**

```bash
cd ~/.pub-cache/hosted/pub.flutter-io.cn/flog_dart-0.8.0
dart analyze lib/src/flog_web_socket.dart
```

预期：无 `FlogWebSocket(Uri)` 相关错误，但业务侧调用点会有 unused import 等提示（正常）。

- [ ] **Step 3: Commit**

```bash
cd ~/.pub-cache/hosted/pub.flutter-io.cn/flog_dart-0.8.0
git add lib/src/flog_web_socket.dart
git commit -m "refactor(ws): delete sync FlogWebSocket(Uri) constructor — cannot await handshake"
```

---

## Task 2: 新增 `_connectAndWrap` 私有帮助函数 + `connect` / `wrap` 静态方法

**Files:**
- Modify: `~/.pub-cache/hosted/pub.flutter-io.cn/flog_dart-0.8.0/lib/src/flog_web_socket.dart`

- [ ] **Step 1: 在 `fromChannel` 构造器之后、`_initFromChannel` 之前插入三个新方法**

在 `FlogWebSocket.fromChannel` 构造器（结尾 `}`）之后，`_initFromChannel` 方法之前，插入：

```dart
  /// Establishes a WebSocket connection and registers it with the flog network
  /// panel.
  ///
  /// On success, emits an `open` frame and returns the wrapped socket.
  /// On failure, emits an `err` frame with [uri], the error message, and the
  /// elapsed duration, then rethrows the original exception unchanged.
  static Future<FlogWebSocket> connect(
    Uri uri, {
    Iterable<String>? protocols,
  }) {
    return _connectAndWrap(
      () => WebSocketChannel.connect(uri, protocols: protocols),
      url: uri.toString(),
    );
  }

  /// Wraps any WebSocket connection factory so that flog can observe the
  /// handshake phase.
  ///
  /// [connect] is an async factory that must return an already-established
  /// [WebSocketChannel]. Use this when you build the channel yourself (e.g.
  /// `dart:io WebSocket.connect` with custom headers):
  ///
  /// ```dart
  /// final ws = await FlogWebSocket.wrap(
  ///   () async {
  ///     final socket = await WebSocket.connect(url, headers: {...});
  ///     return IOWebSocketChannel(socket);
  ///   },
  ///   url: url,
  /// );
  /// ```
  ///
  /// On success, emits an `open` frame and returns the wrapped socket.
  /// On failure, emits an `err` frame and rethrows the original exception.
  static Future<FlogWebSocket> wrap(
    Future<WebSocketChannel> Function() connect, {
    required String url,
  }) {
    return _connectAndWrap(connect, url: url);
  }

  /// Shared implementation for [connect] and [wrap].
  ///
  /// Calls [connect] to obtain a [WebSocketChannel], then awaits
  /// [WebSocketChannel.ready] to surface handshake errors. Emits an `open`
  /// frame on success and an `err` frame (with duration) on failure, then
  /// rethrows.
  static Future<FlogWebSocket> _connectAndWrap(
    Future<WebSocketChannel> Function() connect, {
    required String url,
  }) async {
    final id = nextNetId();
    final start = DateTime.now();

    WebSocketChannel channel;
    try {
      channel = await connect();
      await channel.ready;
    } catch (e) {
      if (flogEnabled) {
        emitNet({
          'id': id,
          't': 'err',
          'p': 'ws',
          'url': url,
          'error': e.toString(),
          'duration': DateTime.now().difference(start).inMilliseconds,
        });
      }
      rethrow;
    }

    final ws = FlogWebSocket._fromConnected(channel, id: id, start: start);
    ws._initFromChannel(url);
    return ws;
  }

  /// Private constructor used by [_connectAndWrap] after a successful
  /// handshake. Does NOT call [_initFromChannel] — the caller does that.
  FlogWebSocket._fromConnected(
    this._channel, {
    required int id,
    required DateTime start,
  })  : _id = id,
        _start = start;
```

- [ ] **Step 2: 确认 `_initFromChannel` 里的 open emit 仍然存在（不需要改）**

当前 `_initFromChannel` 第 63–70 行已有：
```dart
    if (flogEnabled) {
      emitNet({
        'id': _id,
        't': 'open',
        'p': 'ws',
        'url': url,
      });
    }
```
`_connectAndWrap` 成功路径调用 `_initFromChannel`，open 帧由此 emit。无需额外改动。

- [ ] **Step 3: 验证分析无错误**

```bash
cd ~/.pub-cache/hosted/pub.flutter-io.cn/flog_dart-0.8.0
dart analyze lib/src/flog_web_socket.dart
```

预期：`No issues found!`（或仅有与本文件无关的提示）

- [ ] **Step 4: Commit**

```bash
git add lib/src/flog_web_socket.dart
git commit -m "feat(ws): add FlogWebSocket.connect() and .wrap() with handshake-failure coverage"
```

---

## Task 3: 删除 `flog_dart.dart` 的 back-compat export

**Files:**
- Modify: `~/.pub-cache/hosted/pub.flutter-io.cn/flog_dart-0.8.0/lib/flog_dart.dart`

- [ ] **Step 1: 删除 back-compat 注释块和 export 行**

将以下 7 行整体删除（第 14–20 行）：

```dart
// `nextNetId` / `emitNet` are marked `@internal` (DART-021) but we keep the
// export alive for v0.x back-compat. A future v1.0 release will drop them
// from the public surface; new code should import from
// `package:flog_dart/src/flog_net.dart` directly (and even that is
// discouraged).
// ignore: invalid_export_of_internal_element
export 'src/flog_net.dart' show nextNetId, emitNet, flogEnabled;
```

将第 12 行的 import 从：
```dart
import 'src/flog_net.dart' show flogEnabled;
```
保持不变（`flogEnabled` 仍在 `Flog.init()` 内部使用，只是不再 export）。

- [ ] **Step 2: 验证分析无错误**

```bash
cd ~/.pub-cache/hosted/pub.flutter-io.cn/flog_dart-0.8.0
dart analyze lib/flog_dart.dart
```

预期：`No issues found!`

- [ ] **Step 3: Commit**

```bash
git add lib/flog_dart.dart
git commit -m "chore: remove back-compat export of nextNetId/emitNet/flogEnabled (DART-021)"
```

---

## Task 4: 为 `connect` / `wrap` 失败路径写测试

**Files:**
- Modify/Create: `~/.pub-cache/hosted/pub.flutter-io.cn/flog_dart-0.8.0/test/flog_web_socket_connect_test.dart`

flog_dart 的测试用 `dart test`。失败路径测试的关键：mock 出一个会在 `channel.ready` 上抛异常的 `WebSocketChannel`，然后验证：
1. 原异常被 rethrow
2. `emitNet` 被调用且帧包含正确字段

由于 `emitNet` 直接调 `FlogServer.instance.send()`，测试里需要 `flogEnabled == false`（默认 debug 模式为 true）或者检查 `FlogStore`。最简单的方式：用 `wrap` 传入一个会抛异常的工厂，catch 异常并断言其类型，同时用 `FlogStore.instance.snapshotForTesting` 验证帧写入（`FlogStore.record` 在 `FlogServer.send` 内被调用）。

- [ ] **Step 1: 写测试文件**

```dart
// test/flog_web_socket_connect_test.dart
import 'dart:async';

import 'package:flog_dart/flog_dart.dart';
import 'package:flog_dart/src/flog_net.dart' show flogEnabled;
import 'package:test/test.dart';
import 'package:web_socket_channel/web_socket_channel.dart';

void main() {
  group('FlogWebSocket.wrap — failure path', () {
    setUp(() {
      FlogStore.instance.clear();
    });

    test('rethrows original exception when factory throws', () async {
      final original = Exception('dns failure');

      expect(
        () => FlogWebSocket.wrap(
          () async => throw original,
          url: 'wss://example.com/ws',
        ),
        throwsA(same(original)),
      );
    });

    test('rethrows original exception when channel.ready throws', () async {
      final original = WebSocketChannelException('handshake failed');

      expect(
        () => FlogWebSocket.wrap(
          () async => _FailingChannel(original),
          url: 'wss://example.com/ws',
        ),
        throwsA(same(original)),
      );
    });

    test('emits err frame with url, error, duration when flogEnabled', () async {
      // flogEnabled is true in test (debug mode). FlogServer may not be
      // started, but FlogStore.instance.record() is called directly via
      // emitNet → FlogServer.instance.send() → FlogStore.instance.record().
      // We start the server on a throwaway port to satisfy the instance.
      FlogServer.instance.start(port: 19753);

      final err = Exception('connection refused');

      await expectLater(
        FlogWebSocket.wrap(() async => throw err, url: 'wss://host/path'),
        throwsA(isA<Exception>()),
      );

      final frames = FlogStore.instance.snapshotForTesting
          .where((f) => f['type'] == 'net')
          .toList();

      expect(frames, isNotEmpty);
      final frame = frames.last['data'] as Map<String, dynamic>? ?? frames.last;
      // emitNet wraps payload in {type:'net', ts:...} and sends via FlogServer
      // which calls FlogStore.record. The stored message is the outer envelope.
      // Check the inner net payload fields:
      final netFrames = FlogStore.instance.snapshotForTesting
          .where((f) =>
              f.containsKey('t') && f['t'] == 'err' && f['p'] == 'ws')
          .toList();

      expect(netFrames, isNotEmpty, reason: 'expected an err ws frame');
      expect(netFrames.last['url'], equals('wss://host/path'));
      expect(netFrames.last['error'], contains('connection refused'));
      expect(netFrames.last['duration'], isA<int>());
    });
  });

  group('FlogWebSocket.connect — API exists', () {
    test('connect is a static method returning Future<FlogWebSocket>', () {
      // Compile-time check: if connect() doesn't exist, this file won't compile.
      // We don't actually connect — just verify the symbol exists and has the
      // right return type.
      Future<FlogWebSocket> Function(Uri, {Iterable<String>? protocols}) _ =
          FlogWebSocket.connect;
      expect(_, isNotNull);
    });

    test('wrap is a static method returning Future<FlogWebSocket>', () {
      Future<FlogWebSocket> Function(
        Future<WebSocketChannel> Function(), {
        required String url,
      }) _ = FlogWebSocket.wrap;
      expect(_, isNotNull);
    });
  });

  group('FlogWebSocket — old sync constructor removed', () {
    // This test documents intent. If FlogWebSocket(Uri) is accidentally
    // re-added, existing call sites won't compile, but this test also serves
    // as a reminder in CI.
    test('fromChannel still works (regression guard)', () {
      // We can't actually call fromChannel without a real channel, but we can
      // verify the symbol exists.
      expect(FlogWebSocket.fromChannel, isNotNull);
    });
  });
}

/// A [WebSocketChannel] whose [ready] future throws [_error].
class _FailingChannel extends WebSocketChannel {
  _FailingChannel(this._error)
      : super(
          StreamChannel(const Stream.empty(), _NullSink()),
        );

  final Object _error;

  @override
  Future<void> get ready => Future.error(_error);
}

class _NullSink implements StreamSink<dynamic> {
  const _NullSink();
  @override
  Future<dynamic> get done => Future.value();
  @override
  void add(dynamic data) {}
  @override
  void addError(Object error, [StackTrace? stackTrace]) {}
  @override
  Future<dynamic> addStream(Stream<dynamic> stream) => Future.value();
  @override
  Future<dynamic> close() => Future.value();
}
```

- [ ] **Step 2: 调整测试 — FlogStore 存储结构核对**

`emitNet` 的实现是：
```dart
void emitNet(Map<String, dynamic> data) {
  if (!flogEnabled) return;
  final out = <String, dynamic>{...data, 'type': 'net', 'ts': ...};
  FlogServer.instance.send(out);
}
```
`FlogServer.send` 调用 `FlogStore.instance.record(out)`。所以 `snapshotForTesting` 里的条目是 `{...originalFields, 'type': 'net', 'ts': ...}`。

把 Task 4 Step 1 里的测试断言替换成：

```dart
      final netErrFrames = FlogStore.instance.snapshotForTesting
          .where((f) =>
              f['type'] == 'net' &&
              f['t'] == 'err' &&
              f['p'] == 'ws')
          .toList();

      expect(netErrFrames, isNotEmpty, reason: 'expected a net err ws frame');
      expect(netErrFrames.last['url'], equals('wss://host/path'));
      expect(netErrFrames.last['error'], contains('connection refused'));
      expect(netErrFrames.last['duration'], isA<int>());
```

（删除 Task 4 Step 1 中 `frames` / `frame` / `netFrames` 那三段冗余断言，只保留 `netErrFrames`）

- [ ] **Step 3: 运行测试，预期：失败（Task 2 还没做时）或通过**

```bash
cd ~/.pub-cache/hosted/pub.flutter-io.cn/flog_dart-0.8.0
dart test test/flog_web_socket_connect_test.dart --reporter expanded
```

Task 2 完成后预期全部 PASS。

- [ ] **Step 4: Commit**

```bash
git add test/flog_web_socket_connect_test.dart
git commit -m "test(ws): add connect/wrap failure-path and API-shape tests"
```

---

## Task 5: 全量测试 + aura-lang-flutter 编译验证

**Files:**
- No new files. Run existing test suite + compile check in consumer project.

- [ ] **Step 1: 运行 flog_dart 全量测试**

```bash
cd ~/.pub-cache/hosted/pub.flutter-io.cn/flog_dart-0.8.0
dart test --reporter expanded
```

预期：所有已有测试通过，Task 4 的新测试也通过。若有失败，修复后重新 commit。

- [ ] **Step 2: 在 aura-lang-flutter 验证编译**

```bash
cd /Users/shaomingqing/FlutterProject/aura-lang-flutter
flutter pub get
flutter analyze
```

预期：若业务代码里有 `FlogWebSocket(Uri)` 调用点或 `nextNetId`/`emitNet` 引用，这里会报编译错误——这是预期的，说明我们成功删掉了旧 API。

- [ ] **Step 3: 迁移 aura-lang-flutter 调用点（若 Step 2 有报错）**

涉及文件：
- `lib/data/services/azure_stt_service.dart`
- `lib/data/services/azure_tts_service.dart`
- `lib/data/services/live/live_ws_transport.dart`

当前模式：
```dart
final socket = await WebSocket.connect(url, headers: {...});
final channel = IOWebSocketChannel(socket);
final flogWs = FlogWebSocket.fromChannel(channel, url: url.toString());
```

改为：
```dart
final flogWs = await FlogWebSocket.wrap(
  () async {
    final socket = await WebSocket.connect(url, headers: {...});
    return IOWebSocketChannel(socket);
  },
  url: url.toString(),
);
```

- [ ] **Step 4: 重新验证编译**

```bash
cd /Users/shaomingqing/FlutterProject/aura-lang-flutter
flutter analyze
```

预期：`No issues found!`

- [ ] **Step 5: Commit aura-lang-flutter 迁移**

```bash
cd /Users/shaomingqing/FlutterProject/aura-lang-flutter
git add lib/data/services/azure_stt_service.dart \
        lib/data/services/azure_tts_service.dart \
        lib/data/services/live/live_ws_transport.dart
git commit -m "feat: migrate WebSocket to FlogWebSocket.wrap() for handshake-failure coverage"
```

---

## Self-Review

**Spec coverage check:**

| Spec 要求 | Task |
|-----------|------|
| 删除同步构造器 `FlogWebSocket(Uri)` | Task 1 |
| 新增 `connect(Uri)` 静态方法 | Task 2 |
| 新增 `wrap(fn, url:)` 静态方法 | Task 2 |
| 保留 `fromChannel` | Task 2（`_fromConnected` 仅供内部，`fromChannel` 原样保留）|
| 握手失败 emit `err` 帧含 url/error/duration | Task 2 `_connectAndWrap` |
| rethrow 原异常 | Task 2 `_connectAndWrap` |
| 删除 back-compat export | Task 3 |
| 测试失败路径 | Task 4 |
| 全量回归 + 业务侧迁移 | Task 5 |

**Placeholder scan:** 无 TBD / TODO / "similar to" 等。

**Type consistency:**
- `_fromConnected` 在 Task 2 定义并在 `_connectAndWrap` 内调用，字段名一致（`id`/`start`）。
- `_connectAndWrap` 返回 `Future<FlogWebSocket>`，`connect`/`wrap` 直接 return 它，类型一致。
- `emitNet` 字段 `'t':'err'`, `'p':'ws'`, `'url'`, `'error'`, `'duration'` 在 Task 2 和 Task 4 断言里完全对应。
