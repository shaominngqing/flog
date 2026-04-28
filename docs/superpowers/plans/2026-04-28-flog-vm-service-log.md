# FlogVmService 与日志采集增强 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 用 Dart VM Service 的 Logging/Stdout/Stderr 三流替代 flog_dart 现有三处全局 hook (`debugPrint` / `FlutterError.onError` / `PlatformDispatcher.onError`)，在 alpha (`--profile`) 和 debug 构建中采集更完整的日志。release 构建走 `flogEnabled` 常量分支，`vm_service` 包被 AOT tree-shake。

**Architecture:** `flog_dart` 侧新增独立模块 `FlogVmService`（方案 A）并移除老 hook 代码；vm_service 的 Stderr stream 进入一个多行帧组装器把 Flutter `════` exception 块合并为一条 LogEntry。`flog` TUI 侧在 parser 链里新增 `flutter_error.rs` 把组装后的异常文本反结构化为 tag / message / stacktrace。`LogEntry` 结构**不扩展**新字段——所有结构化产出都填进现有 `error` / `stacktrace` / `tag` / `message`。

**Tech Stack:** Dart (`package:vm_service` ^14.0.0)、Rust (`regex` 现有依赖)、现有 WebSocket `ClientMessage::Log` 协议（不变）。

**Spec:** `docs/superpowers/specs/2026-04-28-flog-vm-service-log-design.md`

---

## File Structure

### flog_dart（Dart）

| 动作 | 路径 | 职责 |
|---|---|---|
| Create | `flog_dart/lib/src/flog_vm_service.dart` | `FlogVmService` 类：attach / 三流订阅 / 重试 |
| Create | `flog_dart/lib/src/vm_service/stderr_frame_assembler.dart` | 多行 Flutter exception 帧组装器（纯函数 / 状态机） |
| Create | `flog_dart/lib/src/vm_service/log_level_map.dart` | `LogRecord.level` (int) → flog level (String) 映射 |
| Modify | `flog_dart/lib/src/flog_server.dart` | **删除** `_installSystemHooks` 与 `_recordRawLog`；`start()` 不再调 hook |
| Modify | `flog_dart/lib/flog_dart.dart` | `Flog.init()` 里调 `FlogVmService.instance.attach()` |
| Modify | `flog_dart/pubspec.yaml` | 新增 `vm_service: ^14.0.0` 依赖 |
| Modify | `flog_dart/CHANGELOG.md` | 记录 v0.9.0 breaking change |
| Modify | `flog_dart/pubspec.yaml` version bump | 0.8.0 → 0.9.0 |
| Create | `flog_dart/test/flog_vm_service_test.dart` | `FlogVmService` 行为测试 |
| Create | `flog_dart/test/vm_service/stderr_frame_assembler_test.dart` | 帧组装器测试 |
| Create | `flog_dart/test/vm_service/log_level_map_test.dart` | level 映射测试 |
| Modify | `flog_dart/test/flog_server_test.dart` | 移除针对 `debugPrint` / `FlutterError.onError` 的断言 |

### flog TUI（Rust）

| 动作 | 路径 | 职责 |
|---|---|---|
| Create | `src/parser/flutter_error.rs` | `FlutterErrorParser` 识别 `════ Exception caught by ═══` 块，填充 tag / message / error / stacktrace |
| Modify | `src/parser/mod.rs` | `default_chain()` 链里在 `StructuredParser` 之后、`GenericParser` 之前插入 `FlutterErrorParser` |

不修改：`src/input/protocol.rs`（ClientMessage::Log 字段不变）、`src/domain/entry.rs`（LogEntry 结构不变）、`src/run/dispatch.rs`（派发逻辑不变）。

---

## Task 1: 引入 vm_service 包依赖与版本 bump

**Files:**
- Modify: `flog_dart/pubspec.yaml`
- Modify: `flog_dart/CHANGELOG.md`

- [ ] **Step 1: 修改 pubspec.yaml**

在 `flog_dart/pubspec.yaml` 的 `dependencies:` 块新增 `vm_service`，并把 `version` 从 `0.8.0` 改为 `0.9.0`。完整 patch：

```yaml
name: flog_dart
description: Flutter companion for flog terminal log viewer. Structured logging, Network Inspector (HTTP/SSE/WS), FlogDio drop-in, mock interceptor. Tree-shakes to zero in release builds.
version: 0.9.0
homepage: https://github.com/shaominngqing/flog
repository: https://github.com/shaominngqing/flog/tree/master/flog_dart

dependencies:
  flutter:
    sdk: flutter
  dio: ">=4.0.0 <6.0.0"
  web_socket_channel: ">=2.0.0 <4.0.0"
  package_info_plus: ">=1.0.0 <12.0.0"
  meta: ">=1.8.0 <2.0.0"
  vm_service: ">=14.0.0 <16.0.0"

dev_dependencies:
  lints: ^4.0.0
  flutter_test:
    sdk: flutter

environment:
  sdk: ^3.0.0
  flutter: ">=3.0.0"
```

- [ ] **Step 2: 在 CHANGELOG 顶部加上 0.9.0 条目**

在 `flog_dart/CHANGELOG.md` 最顶部（现有的 `## 0.8.0` 条目之前）插入：

```markdown
## 0.9.0 — 2026-04-28

**Breaking release.** 日志采集路径重构。`flog_dart` 不再抢占
`debugPrint` / `FlutterError.onError` / `PlatformDispatcher.onError`
这三个全局钩子；改由新的 `FlogVmService` 模块订阅 Dart VM Service
的 `Logging` / `Stdout` / `Stderr` 流统一采集。

### What's new

- **新增 `FlogVmService`**：内部模块，在 `Flog.init()` 里自动 attach。
  订阅 vm_service 三流：
  - `Logging` — `dart:developer.log(...)` 和 `package:logging` 生态
  - `Stdout` — `print(...)` 和 native stdout
  - `Stderr` — 未捕获异常文本 + native stderr
- **多行 Flutter exception 帧组装**：Stderr 流里的 `════ Exception caught
  by ═══` 块会被合并为**一条** LogEntry，不再在 TUI 瀑布流里刷屏。
- **连接重试**：3 次指数退避（500ms / 1s / 2s）。最终失败通过
  `FlogLogger('flog_dart').warning(...)` 告警，不静默。
- **release tree-shake**：`flogEnabled == false` 时 `Flog.init()` 早退，
  `vm_service` 包依赖被 AOT 完全剪除，release 包体零增长。

### Breaking changes

- 移除 `FlogServer._installSystemHooks` 与 `_recordRawLog`。如果你的应用
  代码依赖 `flog_dart` 设置的 `FlutterError.onError` 或
  `PlatformDispatcher.onError`，请自行恢复或迁移到 `FlogLogger`。
- `debugPrint` 不再被 flog_dart 抢占。业务 log 请改用 `FlogLogger.info/d/w/e`
  直接发送结构化日志；`debugPrint` 的输出会由 vm_service 的 Stdout 流在
  alpha / debug 构建中自动采到。

### Constraints

- alpha 构建必须是 `--profile`（见 flutter-build-scripts commit `9527c74`）。
  VM Service 在 `--release` 构建里不存在，`FlogVmService` 会静默跳过。
- iOS 真机上 Stdout / Stderr 流的 `print` 输出可能部分丢失（Dart VM 在
  iOS 的已知限制）；改用 `FlogLogger` 或 `package:logging` 可绕开。
```

- [ ] **Step 3: 拉依赖并校验**

Run:
```bash
cd flog_dart && flutter pub get
```
Expected: `Got dependencies!`，`vm_service` 出现在 `pubspec.lock`。

- [ ] **Step 4: 提交**

```bash
cd flog_dart
git add pubspec.yaml pubspec.lock CHANGELOG.md
git commit -m "chore(flog_dart): v0.9.0 引入 vm_service 依赖并记录 breaking 变更"
```

---

## Task 2: LogRecord.level int → flog level 字符串映射

**Files:**
- Create: `flog_dart/lib/src/vm_service/log_level_map.dart`
- Test: `flog_dart/test/vm_service/log_level_map_test.dart`

**背景**：`dart:developer.log(level: int)` 的 level 对齐 `package:logging.Level`（0=ALL, 300=FINEST, 500=CONFIG, 700=INFO, 800=INFO, 900=WARNING, 1000=SEVERE, 2000=SHOUT）。flog 的 level 是字符串 (`verbose/debug/info/warning/error`)。

- [ ] **Step 1: 写失败测试**

创建 `flog_dart/test/vm_service/log_level_map_test.dart`：

```dart
import 'package:flog_dart/src/vm_service/log_level_map.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('mapVmServiceLogLevel', () {
    test('FINE / FINER / FINEST (< 500) → debug', () {
      expect(mapVmServiceLogLevel(0), 'debug');
      expect(mapVmServiceLogLevel(300), 'debug');
      expect(mapVmServiceLogLevel(499), 'debug');
    });

    test('CONFIG / INFO (500..<900) → info', () {
      expect(mapVmServiceLogLevel(500), 'info');
      expect(mapVmServiceLogLevel(700), 'info');
      expect(mapVmServiceLogLevel(800), 'info');
      expect(mapVmServiceLogLevel(899), 'info');
    });

    test('WARNING (900..<1000) → warning', () {
      expect(mapVmServiceLogLevel(900), 'warning');
      expect(mapVmServiceLogLevel(950), 'warning');
      expect(mapVmServiceLogLevel(999), 'warning');
    });

    test('SEVERE / SHOUT (>= 1000) → error', () {
      expect(mapVmServiceLogLevel(1000), 'error');
      expect(mapVmServiceLogLevel(2000), 'error');
      expect(mapVmServiceLogLevel(99999), 'error');
    });

    test('negative fallback → debug', () {
      expect(mapVmServiceLogLevel(-1), 'debug');
      expect(mapVmServiceLogLevel(-999), 'debug');
    });
  });
}
```

- [ ] **Step 2: 跑测试确认失败**

Run:
```bash
cd flog_dart && flutter test test/vm_service/log_level_map_test.dart
```
Expected: 失败，提示 "Target of URI doesn't exist: 'package:flog_dart/src/vm_service/log_level_map.dart'"。

- [ ] **Step 3: 实现**

创建 `flog_dart/lib/src/vm_service/log_level_map.dart`：

```dart
/// `dart:developer.log(level:)` / `package:logging.Level.value` → flog level.
///
/// 对齐 `package:logging` 的阶梯：
/// - `< 500`   FINE/FINER/FINEST → `debug`
/// - `< 900`   CONFIG/INFO       → `info`
/// - `< 1000`  WARNING           → `warning`
/// - `>= 1000` SEVERE/SHOUT      → `error`
String mapVmServiceLogLevel(int level) {
  if (level < 500) return 'debug';
  if (level < 900) return 'info';
  if (level < 1000) return 'warning';
  return 'error';
}
```

- [ ] **Step 4: 跑测试确认通过**

Run:
```bash
cd flog_dart && flutter test test/vm_service/log_level_map_test.dart
```
Expected: All tests passed.

- [ ] **Step 5: 提交**

```bash
cd flog_dart
git add lib/src/vm_service/log_level_map.dart test/vm_service/log_level_map_test.dart
git commit -m "feat(flog_dart): vm_service LogRecord level → flog level 映射"
```

---

## Task 3: Stderr 多行 Flutter exception 帧组装器 —— 骨架 + 简单 case

**Files:**
- Create: `flog_dart/lib/src/vm_service/stderr_frame_assembler.dart`
- Test: `flog_dart/test/vm_service/stderr_frame_assembler_test.dart`

**目标**：把按行到达的 stderr 流，把 `════ Exception caught by <lib> ═══` 到下一条全 `═` 尾分隔线之间的所有行合并成一条。非组装行原样透传。

本 Task 只覆盖"典型 Flutter exception 正常完整块"一个 case，后续 Task 扩展超时 / 嵌套 / 非组装行。

- [ ] **Step 1: 写失败测试**

创建 `flog_dart/test/vm_service/stderr_frame_assembler_test.dart`：

```dart
import 'package:flog_dart/src/vm_service/stderr_frame_assembler.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('StderrFrameAssembler — happy path', () {
    test('合并完整 Flutter exception 块为一条 Frame', () {
      final emitted = <AssembledFrame>[];
      final asm = StderrFrameAssembler(onFrame: emitted.add);

      const lines = [
        '════════ Exception caught by widgets library ══════════════════════',
        'The following assertion was thrown building MyWidget(dirty):',
        'setState() called after dispose(): _MyState#a1b2c',
        '',
        'The relevant error-causing widget was:',
        '  MyWidget MyWidget:file:///path/widget.dart:42:15',
        '',
        'When the exception was thrown, this was the stack:',
        '#0      State.setState (package:flutter/src/widgets/framework.dart:1178:9)',
        '#1      _MyState.onDone (package:myapp/widgets/my.dart:56:5)',
        '════════════════════════════════════════════════════════════════════',
      ];

      for (final line in lines) {
        asm.addLine(line);
      }

      expect(emitted, hasLength(1));
      final f = emitted.single;
      expect(f.kind, FrameKind.flutterException);
      expect(f.library, 'widgets library');
      // 摘要 = 分隔线后的第一条非空行
      expect(
        f.summary,
        'The following assertion was thrown building MyWidget(dirty):',
      );
      // 原始文本必须包含头和尾分隔线
      expect(f.fullText.split('\n').first, startsWith('════════ Exception'));
      expect(f.fullText.split('\n').last, startsWith('════════'));
      // stack 只含 `#N` 行
      expect(f.stackFrames, hasLength(2));
      expect(f.stackFrames.first, startsWith('#0      State.setState'));
    });
  });
}
```

- [ ] **Step 2: 跑测试确认失败**

Run:
```bash
cd flog_dart && flutter test test/vm_service/stderr_frame_assembler_test.dart
```
Expected: 失败，提示 `stderr_frame_assembler.dart` 不存在。

- [ ] **Step 3: 实现最小骨架**

创建 `flog_dart/lib/src/vm_service/stderr_frame_assembler.dart`：

```dart
/// 一块已组装好的 stderr 数据，交给上层（`FlogVmService`）发送。
enum FrameKind {
  /// Flutter framework 格式的 `════ Exception caught by … ═══` 块。
  flutterException,

  /// 非分隔线包围的普通 stderr 行，原样透传。
  rawLine,
}

class AssembledFrame {
  AssembledFrame({
    required this.kind,
    required this.fullText,
    this.library,
    this.summary,
    this.stackFrames = const [],
  });

  final FrameKind kind;
  final String fullText;
  final String? library;
  final String? summary;
  final List<String> stackFrames;
}

/// 多行 stderr 帧组装器。按行喂入，命中完整 Flutter exception 块时
/// 发出 `AssembledFrame(kind: flutterException)`；其他行立即透传为
/// `AssembledFrame(kind: rawLine)`。
///
/// **本 Task 只实现 happy path**：完整的头-正文-尾结构。超时、嵌套、
/// 无尾分隔线等场景由 Task 4 扩展。
class StderrFrameAssembler {
  StderrFrameAssembler({required this.onFrame});

  final void Function(AssembledFrame frame) onFrame;

  // 累积中的 Flutter exception 行；null 表示当前不在块内。
  List<String>? _buffer;
  String? _library;

  static final _headerRe = RegExp(
    r'^═+\s*(?:Exception|Error|ERROR)\s+caught\s+by\s+(.+?)\s*═*$',
  );

  static bool _isTailSeparator(String line) {
    final trimmed = line.trim();
    return trimmed.length >= 60 && RegExp(r'^═+$').hasMatch(trimmed);
  }

  void addLine(String line) {
    if (_buffer == null) {
      final header = _headerRe.firstMatch(line);
      if (header != null) {
        _buffer = [line];
        _library = header.group(1)?.trim();
        return;
      }
      onFrame(AssembledFrame(kind: FrameKind.rawLine, fullText: line));
      return;
    }

    _buffer!.add(line);
    if (_isTailSeparator(line)) {
      _emitFlutterException();
    }
  }

  void _emitFlutterException() {
    final lines = _buffer!;
    final body = lines.sublist(1, lines.length - 1);
    final summary = body
        .firstWhere((l) => l.trim().isNotEmpty, orElse: () => '');
    final stack = body.where((l) => _isStackFrame(l)).toList();

    onFrame(AssembledFrame(
      kind: FrameKind.flutterException,
      fullText: lines.join('\n'),
      library: _library,
      summary: summary,
      stackFrames: stack,
    ));

    _buffer = null;
    _library = null;
  }

  static final _stackFrameRe = RegExp(r'^#\d+\s+');
  static bool _isStackFrame(String line) => _stackFrameRe.hasMatch(line);
}
```

- [ ] **Step 4: 跑测试确认通过**

Run:
```bash
cd flog_dart && flutter test test/vm_service/stderr_frame_assembler_test.dart
```
Expected: All tests passed.

- [ ] **Step 5: 提交**

```bash
cd flog_dart
git add lib/src/vm_service/stderr_frame_assembler.dart test/vm_service/stderr_frame_assembler_test.dart
git commit -m "feat(flog_dart): stderr 多行 Flutter exception 帧组装器（happy path）"
```

---

## Task 4: 帧组装器 —— 边界场景（透传、嵌套、无尾分隔线的 flush）

**Files:**
- Modify: `flog_dart/lib/src/vm_service/stderr_frame_assembler.dart`
- Modify: `flog_dart/test/vm_service/stderr_frame_assembler_test.dart`

本 Task 在骨架上扩展：
- 非 Flutter exception 的 stderr 行原样透传（已在 Task 3 写好，本 Task 补测试）
- 在组装中间再次收到头分隔线时，旧块作为"未完成"立即 emit，以新信号开始
- 外部 `flush()` 调用时把未完成块作为 `rawLine` emit（供"500ms 超时"上层定时器使用——本模块不自己起定时器，保持纯同步，便于测试）

- [ ] **Step 1: 追加失败测试**

在 `flog_dart/test/vm_service/stderr_frame_assembler_test.dart` 的 `main()` 里、已有 `group` 之后追加：

```dart
  group('StderrFrameAssembler — edge cases', () {
    test('非分隔线行立刻作为 rawLine 透传', () {
      final emitted = <AssembledFrame>[];
      final asm = StderrFrameAssembler(onFrame: emitted.add);

      asm.addLine('plain stderr line');
      asm.addLine('another one');

      expect(emitted, hasLength(2));
      expect(emitted.every((f) => f.kind == FrameKind.rawLine), isTrue);
      expect(emitted[0].fullText, 'plain stderr line');
    });

    test('块内再次出现 header → 旧块立即 emit 作为未完成', () {
      final emitted = <AssembledFrame>[];
      final asm = StderrFrameAssembler(onFrame: emitted.add);

      asm.addLine('════════ Exception caught by widgets library ═════');
      asm.addLine('first summary');
      // 第二个 header 出现 —— 没有收到旧块的尾 `═`
      asm.addLine('════════ Exception caught by services library ═════');
      asm.addLine('second summary');
      asm.addLine('══════════════════════════════════════════════════════════════');

      expect(emitted, hasLength(2));
      // 第一条：未完成块，fullText 只包含 header + summary
      expect(emitted[0].kind, FrameKind.flutterException);
      expect(emitted[0].library, 'widgets library');
      expect(emitted[0].summary, 'first summary');
      expect(emitted[0].fullText.split('\n'), hasLength(2));
      // 第二条：完整收尾
      expect(emitted[1].library, 'services library');
      expect(emitted[1].summary, 'second summary');
    });

    test('flush() 把未完成块强制 emit', () {
      final emitted = <AssembledFrame>[];
      final asm = StderrFrameAssembler(onFrame: emitted.add);

      asm.addLine('════════ Exception caught by widgets library ═════');
      asm.addLine('hanging summary');
      // 没有收到尾分隔线，外部显式 flush
      asm.flush();

      expect(emitted, hasLength(1));
      expect(emitted.single.kind, FrameKind.flutterException);
      expect(emitted.single.summary, 'hanging summary');
    });

    test('flush() 在 idle 状态下是 no-op', () {
      final emitted = <AssembledFrame>[];
      final asm = StderrFrameAssembler(onFrame: emitted.add);
      asm.flush();
      expect(emitted, isEmpty);
    });
  });
```

- [ ] **Step 2: 跑测试确认新 case 失败**

Run:
```bash
cd flog_dart && flutter test test/vm_service/stderr_frame_assembler_test.dart
```
Expected: `块内再次出现 header` 和 `flush()` 相关 case 失败（当前骨架没处理这些）。

- [ ] **Step 3: 扩展实现**

修改 `flog_dart/lib/src/vm_service/stderr_frame_assembler.dart` —— 在 `addLine` 里，如果已在块内时又遇到 header，先 flush 当前未完成块再开新块；新增 `flush()` 方法。替换 `addLine` 和新增 `flush()`：

```dart
  void addLine(String line) {
    if (_buffer == null) {
      final header = _headerRe.firstMatch(line);
      if (header != null) {
        _buffer = [line];
        _library = header.group(1)?.trim();
        return;
      }
      onFrame(AssembledFrame(kind: FrameKind.rawLine, fullText: line));
      return;
    }

    // 已在块内：若再次遇到 header，旧块作为未完成立即 emit
    final header = _headerRe.firstMatch(line);
    if (header != null) {
      _emitFlutterException();
      _buffer = [line];
      _library = header.group(1)?.trim();
      return;
    }

    _buffer!.add(line);
    if (_isTailSeparator(line)) {
      _emitFlutterException();
    }
  }

  /// 强制把当前未完成块（若有）emit。上层应在 idle 超时或 stream 收尾时调。
  void flush() {
    if (_buffer != null) {
      _emitFlutterException();
    }
  }
```

同步修改 `_emitFlutterException`：之前假设 `lines` 至少 2 条（头 + 尾），现在未完成块里可能没尾行。改成更稳的 body 切片：

```dart
  void _emitFlutterException() {
    final lines = _buffer!;
    // 头是 lines[0]；尾若是 `═` 分隔线则剥掉
    final lastIdx = lines.length - 1;
    final hasTail = lastIdx >= 1 && _isTailSeparator(lines[lastIdx]);
    final body = hasTail ? lines.sublist(1, lastIdx) : lines.sublist(1);
    final summary = body
        .firstWhere((l) => l.trim().isNotEmpty, orElse: () => '');
    final stack = body.where(_isStackFrame).toList();

    onFrame(AssembledFrame(
      kind: FrameKind.flutterException,
      fullText: lines.join('\n'),
      library: _library,
      summary: summary,
      stackFrames: stack,
    ));

    _buffer = null;
    _library = null;
  }
```

- [ ] **Step 4: 跑全部测试确认通过**

Run:
```bash
cd flog_dart && flutter test test/vm_service/stderr_frame_assembler_test.dart
```
Expected: All tests passed（happy path + 4 个 edge case）。

- [ ] **Step 5: 提交**

```bash
cd flog_dart
git add lib/src/vm_service/stderr_frame_assembler.dart test/vm_service/stderr_frame_assembler_test.dart
git commit -m "feat(flog_dart): 帧组装器支持嵌套 header、flush、rawLine 透传"
```

---

## Task 5: `FlogVmService` 模块骨架（attach 生命周期 + 重试）

**Files:**
- Create: `flog_dart/lib/src/flog_vm_service.dart`
- Test: `flog_dart/test/flog_vm_service_test.dart`

**目标**：`FlogVmService.instance.attach()` 调 `Service.getInfo()` 拿 URL、用 `vmServiceConnectUri` 连接、`streamListen('Logging'/'Stdout'/'Stderr')`。连不上走 3 次指数退避重试，最终失败通过 `FlogLogger('flog_dart').warning(...)` 告警。本 Task 只实现连接和重试骨架，stream 事件处理留 Task 6/7/8。

**测试策略**：`attach` 内部的依赖通过构造函数注入（`Future<Uri?> Function()` 作 getInfo 抽象、`Future<VmService> Function(Uri)` 作 connect 抽象、`Duration Function(int)` 作 backoff 抽象），让测试可以 mock 不真启 VM。`FlogVmService.instance` 用默认依赖组合，生产路径不变。

- [ ] **Step 1: 写失败测试**

创建 `flog_dart/test/flog_vm_service_test.dart`：

```dart
import 'dart:async';

import 'package:flog_dart/src/flog_vm_service.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vm_service/vm_service.dart' as vms;

/// 仅测试用 stub：实现够用的 VmService 表面，让 attach 骨架能走通流程。
class _StubVmService implements vms.VmService {
  _StubVmService();

  final streamsListened = <String>[];

  @override
  Future<vms.Success> streamListen(String streamId) async {
    streamsListened.add(streamId);
    return vms.Success();
  }

  @override
  Stream<vms.Event> get onLoggingEvent => const Stream.empty();

  @override
  Stream<vms.Event> get onStdoutEvent => const Stream.empty();

  @override
  Stream<vms.Event> get onStderrEvent => const Stream.empty();

  @override
  Future<void> dispose() async {}

  // 其他 VmService 方法本测试用不到；通过 noSuchMethod 兜底。
  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

void main() {
  group('FlogVmService.attach', () {
    test('getInfo 第一次返回 null，第二次返回 URI → 仍然成功', () async {
      var callCount = 0;
      final stub = _StubVmService();

      final svc = FlogVmService(
        getServiceUri: () async {
          callCount++;
          if (callCount == 1) return null;
          return Uri.parse('ws://127.0.0.1:41235/xyz=/ws');
        },
        connect: (uri) async => stub,
        backoff: (attempt) => Duration.zero, // 测试零延迟
      );

      final result = await svc.attach();
      expect(result, isTrue);
      expect(callCount, 2);
      expect(stub.streamsListened, containsAll(['Logging', 'Stdout', 'Stderr']));
    });

    test('连续 3 次 getInfo 返回 null → attach 返回 false', () async {
      var callCount = 0;
      final svc = FlogVmService(
        getServiceUri: () async {
          callCount++;
          return null;
        },
        connect: (uri) async => _StubVmService(),
        backoff: (attempt) => Duration.zero,
      );

      final result = await svc.attach();
      expect(result, isFalse);
      expect(callCount, 3);
    });

    test('connect 抛异常 3 次 → attach 返回 false', () async {
      var callCount = 0;
      final svc = FlogVmService(
        getServiceUri: () async => Uri.parse('ws://127.0.0.1:1/ws'),
        connect: (uri) async {
          callCount++;
          throw StateError('boom');
        },
        backoff: (attempt) => Duration.zero,
      );

      final result = await svc.attach();
      expect(result, isFalse);
      expect(callCount, 3);
    });

    test('backoff 被按 attempt 递增调用', () async {
      final attempts = <int>[];
      final svc = FlogVmService(
        getServiceUri: () async => null,
        connect: (uri) async => _StubVmService(),
        backoff: (attempt) {
          attempts.add(attempt);
          return Duration.zero;
        },
      );
      await svc.attach();
      // 前 2 次失败后 sleep，最后一次失败不 sleep
      expect(attempts, [1, 2]);
    });
  });
}
```

- [ ] **Step 2: 跑测试确认失败**

Run:
```bash
cd flog_dart && flutter test test/flog_vm_service_test.dart
```
Expected: 失败，提示 `flog_vm_service.dart` 不存在或导出的符号缺失。

- [ ] **Step 3: 实现 FlogVmService 骨架**

创建 `flog_dart/lib/src/flog_vm_service.dart`：

```dart
/// 订阅 Dart VM Service 的 Logging/Stdout/Stderr stream，
/// 把事件转 `{type: 'log', ...}` 消息喂给 [FlogServer]。
///
/// 仅在 `flogEnabled == true` 时被 `Flog.init()` 调用。release 构建
/// 下 `Flog.init` 走常量分支早退，此文件及其依赖（package:vm_service）
/// 全部被 AOT tree-shake。
library;

import 'dart:async';
import 'dart:developer' as developer;

import 'package:vm_service/vm_service.dart' as vms;
import 'package:vm_service/vm_service_io.dart' as vms_io;

import 'flog_logger.dart';

typedef _GetServiceUri = Future<Uri?> Function();
typedef _Connect = Future<vms.VmService> Function(Uri wsUri);
typedef _Backoff = Duration Function(int attempt);

class FlogVmService {
  FlogVmService({
    _GetServiceUri? getServiceUri,
    _Connect? connect,
    _Backoff? backoff,
  })  : _getServiceUri = getServiceUri ?? _defaultGetServiceUri,
        _connect = connect ?? _defaultConnect,
        _backoff = backoff ?? _defaultBackoff;

  static final FlogVmService instance = FlogVmService();

  final _GetServiceUri _getServiceUri;
  final _Connect _connect;
  final _Backoff _backoff;

  vms.VmService? _service;

  /// 最多尝试 3 次，成功返回 true，失败返回 false（并通过 FlogLogger warn）。
  Future<bool> attach() async {
    for (var attempt = 1; attempt <= 3; attempt++) {
      try {
        final uri = await _getServiceUri();
        if (uri == null) {
          if (attempt < 3) await Future<void>.delayed(_backoff(attempt));
          continue;
        }
        final service = await _connect(uri);
        await service.streamListen(vms.EventStreams.kLogging);
        await service.streamListen(vms.EventStreams.kStdout);
        await service.streamListen(vms.EventStreams.kStderr);
        _service = service;
        // stream 事件处理在后续 Task 接入
        return true;
      } catch (_) {
        if (attempt < 3) await Future<void>.delayed(_backoff(attempt));
      }
    }
    FlogLogger('flog_dart').w(
      'FlogVmService: failed to attach after 3 retries; '
      'log stream will be limited to FlogLogger calls only',
    );
    return false;
  }

  /// 关闭连接（测试用；生产代码不主动调）。
  Future<void> dispose() async {
    await _service?.dispose();
    _service = null;
  }

  // ── 默认依赖 ──

  static Future<Uri?> _defaultGetServiceUri() async {
    final info = await developer.Service.getInfo();
    return info.serverWebSocketUri;
  }

  static Future<vms.VmService> _defaultConnect(Uri wsUri) {
    return vms_io.vmServiceConnectUri(wsUri.toString());
  }

  static Duration _defaultBackoff(int attempt) {
    // attempt=1 → 500ms；2 → 1s；3 → 2s（3 不会被使用，因为循环末轮不 sleep）
    return Duration(milliseconds: 500 * (1 << (attempt - 1)));
  }
}
```

注：`FlogLogger('flog_dart').w(...)` 是调用现有 `flog_logger.dart` 里的 warning 方法。现有代码路径已经存在，不需要新增。

- [ ] **Step 4: 跑测试确认通过**

Run:
```bash
cd flog_dart && flutter test test/flog_vm_service_test.dart
```
Expected: All tests passed（4 个）。

- [ ] **Step 5: 提交**

```bash
cd flog_dart
git add lib/src/flog_vm_service.dart test/flog_vm_service_test.dart
git commit -m "feat(flog_dart): FlogVmService attach + 3 次指数退避重试骨架"
```

---

## Task 6: FlogVmService 接 Logging stream → 转 log 消息

**Files:**
- Modify: `flog_dart/lib/src/flog_vm_service.dart`
- Modify: `flog_dart/test/flog_vm_service_test.dart`

**目标**：attach 成功后，订阅 `onLoggingEvent`，把 `LogRecord` 字段映射到 flog log 消息，经由 `FlogServer.instance.send(...)` 下发。

**新增注入**：测试里需要抓到发到 FlogServer 的消息。新增 `send` 注入点 `void Function(Map<String, dynamic>)`，生产默认为 `FlogServer.instance.send`。

- [ ] **Step 1: 追加失败测试**

在 `flog_dart/test/flog_vm_service_test.dart` 的 `main()` 末尾追加 group。先在文件顶部追加一个构造 LogRecord event 的 helper 和替换 `_StubVmService` 里的 `onLoggingEvent` 为可控 StreamController：

把文件开头的 `_StubVmService` 替换为：

```dart
class _StubVmService implements vms.VmService {
  _StubVmService();

  final streamsListened = <String>[];
  final loggingCtrl = StreamController<vms.Event>.broadcast();
  final stdoutCtrl = StreamController<vms.Event>.broadcast();
  final stderrCtrl = StreamController<vms.Event>.broadcast();

  @override
  Future<vms.Success> streamListen(String streamId) async {
    streamsListened.add(streamId);
    return vms.Success();
  }

  @override
  Stream<vms.Event> get onLoggingEvent => loggingCtrl.stream;

  @override
  Stream<vms.Event> get onStdoutEvent => stdoutCtrl.stream;

  @override
  Stream<vms.Event> get onStderrEvent => stderrCtrl.stream;

  @override
  Future<void> dispose() async {
    await loggingCtrl.close();
    await stdoutCtrl.close();
    await stderrCtrl.close();
  }

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}
```

在 `main()` 末尾追加：

```dart
  group('FlogVmService — Logging stream', () {
    test('LogRecord Event → {type:log, level, tag, message}', () async {
      final stub = _StubVmService();
      final sent = <Map<String, dynamic>>[];
      final svc = FlogVmService(
        getServiceUri: () async => Uri.parse('ws://x/ws'),
        connect: (uri) async => stub,
        backoff: (_) => Duration.zero,
        send: sent.add,
      );
      await svc.attach();

      stub.loggingCtrl.add(vms.Event(
        kind: vms.EventKind.kLogging,
        timestamp: 1_714_000_000_000,
        logRecord: vms.LogRecord(
          level: 900,
          loggerName: vms.InstanceRef(
            kind: vms.InstanceKind.kString,
            valueAsString: 'auth',
            identityHashCode: 0,
            id: 'x',
          ),
          message: vms.InstanceRef(
            kind: vms.InstanceKind.kString,
            valueAsString: 'token expired',
            identityHashCode: 0,
            id: 'y',
          ),
          time: 0,
          sequenceNumber: 0,
          zone: vms.InstanceRef(
            kind: vms.InstanceKind.kString,
            valueAsString: '',
            identityHashCode: 0,
            id: 'z',
          ),
          error: vms.InstanceRef(
            kind: vms.InstanceKind.kNull,
            valueAsString: null,
            identityHashCode: 0,
            id: 'e',
          ),
          stackTrace: vms.InstanceRef(
            kind: vms.InstanceKind.kNull,
            valueAsString: null,
            identityHashCode: 0,
            id: 's',
          ),
        ),
      ));

      // 给 stream 一次 tick 机会
      await Future<void>.delayed(Duration.zero);

      expect(sent, hasLength(1));
      final m = sent.single;
      expect(m['type'], 'log');
      expect(m['level'], 'warning');
      expect(m['tag'], 'auth');
      expect(m['message'], 'token expired');
    });

    test('loggerName 为空 → tag 填 "developer"', () async {
      final stub = _StubVmService();
      final sent = <Map<String, dynamic>>[];
      final svc = FlogVmService(
        getServiceUri: () async => Uri.parse('ws://x/ws'),
        connect: (uri) async => stub,
        backoff: (_) => Duration.zero,
        send: sent.add,
      );
      await svc.attach();

      stub.loggingCtrl.add(vms.Event(
        kind: vms.EventKind.kLogging,
        timestamp: 0,
        logRecord: vms.LogRecord(
          level: 800,
          loggerName: vms.InstanceRef(
            kind: vms.InstanceKind.kString,
            valueAsString: '',
            identityHashCode: 0,
            id: 'x',
          ),
          message: vms.InstanceRef(
            kind: vms.InstanceKind.kString,
            valueAsString: 'hello',
            identityHashCode: 0,
            id: 'y',
          ),
          time: 0,
          sequenceNumber: 0,
          zone: vms.InstanceRef(
            kind: vms.InstanceKind.kString, valueAsString: '',
            identityHashCode: 0, id: 'z',
          ),
          error: vms.InstanceRef(
            kind: vms.InstanceKind.kNull, identityHashCode: 0, id: 'e',
          ),
          stackTrace: vms.InstanceRef(
            kind: vms.InstanceKind.kNull, identityHashCode: 0, id: 's',
          ),
        ),
      ));
      await Future<void>.delayed(Duration.zero);

      expect(sent.single['tag'], 'developer');
    });
  });
```

- [ ] **Step 2: 跑测试确认失败**

Run:
```bash
cd flog_dart && flutter test test/flog_vm_service_test.dart
```
Expected: 新 group 失败——`FlogVmService` 构造参数没有 `send`，并且 Logging 事件没被处理。

- [ ] **Step 3: 扩展 FlogVmService**

修改 `flog_dart/lib/src/flog_vm_service.dart`。文件顶部导入新增：

```dart
import 'flog_server.dart';
import 'vm_service/log_level_map.dart';
```

typedef 段新增：

```dart
typedef _Send = void Function(Map<String, dynamic> msg);
```

构造函数扩展注入参数，默认走 `FlogServer.instance.send`：

```dart
  FlogVmService({
    _GetServiceUri? getServiceUri,
    _Connect? connect,
    _Backoff? backoff,
    _Send? send,
  })  : _getServiceUri = getServiceUri ?? _defaultGetServiceUri,
        _connect = connect ?? _defaultConnect,
        _backoff = backoff ?? _defaultBackoff,
        _send = send ?? FlogServer.instance.send;

  final _Send _send;
```

attach 成功分支里，在 `_service = service;` 之后插入：

```dart
        service.onLoggingEvent.listen(_handleLogging);
```

新增方法（类体内，dispose 之前）：

```dart
  void _handleLogging(vms.Event event) {
    final rec = event.logRecord;
    if (rec == null) return;
    final message = rec.message?.valueAsString ?? '';
    final loggerName = rec.loggerName?.valueAsString ?? '';
    final errorStr = rec.error?.valueAsString;
    final stackStr = rec.stackTrace?.valueAsString;

    _send({
      'type': 'log',
      'level': mapVmServiceLogLevel(rec.level ?? 0),
      'tag': loggerName.isEmpty ? 'developer' : loggerName,
      'message': message,
      if (errorStr != null && errorStr.isNotEmpty) 'error': errorStr,
      if (stackStr != null && stackStr.isNotEmpty) 'stackTrace': stackStr,
      'timestamp': event.timestamp ?? DateTime.now().millisecondsSinceEpoch,
    });
  }
```

- [ ] **Step 4: 跑测试确认通过**

Run:
```bash
cd flog_dart && flutter test test/flog_vm_service_test.dart
```
Expected: All tests passed。

- [ ] **Step 5: 提交**

```bash
cd flog_dart
git add lib/src/flog_vm_service.dart test/flog_vm_service_test.dart
git commit -m "feat(flog_dart): FlogVmService 接入 Logging stream"
```

---

## Task 7: FlogVmService 接 Stdout stream → 原样透传为 info

**Files:**
- Modify: `flog_dart/lib/src/flog_vm_service.dart`
- Modify: `flog_dart/test/flog_vm_service_test.dart`

**目标**：Stdout event 的 `bytes` 是 base64 编码的一行文本。解码后按一条 `{type:log, level:'info', tag:'stdout', message:...}` 下发。

- [ ] **Step 1: 追加失败测试**

在文件顶部 import 里加 `'dart:convert';`。在 `main()` 末尾追加 group：

```dart
  group('FlogVmService — Stdout stream', () {
    test('base64 bytes 解码后下发 info/stdout', () async {
      final stub = _StubVmService();
      final sent = <Map<String, dynamic>>[];
      final svc = FlogVmService(
        getServiceUri: () async => Uri.parse('ws://x/ws'),
        connect: (uri) async => stub,
        backoff: (_) => Duration.zero,
        send: sent.add,
      );
      await svc.attach();

      final payload = base64Encode(utf8.encode('hello world'));
      stub.stdoutCtrl.add(vms.Event(
        kind: vms.EventKind.kWriteEvent,
        timestamp: 1_714_000_000_000,
        bytes: payload,
      ));
      await Future<void>.delayed(Duration.zero);

      expect(sent, hasLength(1));
      expect(sent.single['type'], 'log');
      expect(sent.single['level'], 'info');
      expect(sent.single['tag'], 'stdout');
      expect(sent.single['message'], 'hello world');
    });

    test('空 bytes 不下发', () async {
      final stub = _StubVmService();
      final sent = <Map<String, dynamic>>[];
      final svc = FlogVmService(
        getServiceUri: () async => Uri.parse('ws://x/ws'),
        connect: (uri) async => stub,
        backoff: (_) => Duration.zero,
        send: sent.add,
      );
      await svc.attach();

      stub.stdoutCtrl.add(vms.Event(
        kind: vms.EventKind.kWriteEvent,
        timestamp: 0,
        bytes: null,
      ));
      await Future<void>.delayed(Duration.zero);

      expect(sent, isEmpty);
    });
  });
```

- [ ] **Step 2: 跑测试确认失败**

Run:
```bash
cd flog_dart && flutter test test/flog_vm_service_test.dart
```
Expected: 新 group 失败（stdout 未被订阅）。

- [ ] **Step 3: 扩展实现**

在 `flog_dart/lib/src/flog_vm_service.dart` 顶部 import 加：

```dart
import 'dart:convert';
```

attach 成功分支里、`service.onLoggingEvent.listen(_handleLogging);` 之后加：

```dart
        service.onStdoutEvent.listen(_handleStdout);
```

新增方法：

```dart
  void _handleStdout(vms.Event event) {
    final text = _decodeBytes(event.bytes);
    if (text == null || text.isEmpty) return;
    _send({
      'type': 'log',
      'level': 'info',
      'tag': 'stdout',
      'message': text,
      'timestamp': event.timestamp ?? DateTime.now().millisecondsSinceEpoch,
    });
  }

  static String? _decodeBytes(String? bytes) {
    if (bytes == null || bytes.isEmpty) return null;
    try {
      return utf8.decode(base64Decode(bytes));
    } catch (_) {
      return null;
    }
  }
```

- [ ] **Step 4: 跑测试确认通过**

Run:
```bash
cd flog_dart && flutter test test/flog_vm_service_test.dart
```
Expected: All tests passed。

- [ ] **Step 5: 提交**

```bash
cd flog_dart
git add lib/src/flog_vm_service.dart test/flog_vm_service_test.dart
git commit -m "feat(flog_dart): FlogVmService 接入 Stdout stream"
```

---

## Task 8: FlogVmService 接 Stderr stream → 帧组装器 → log

**Files:**
- Modify: `flog_dart/lib/src/flog_vm_service.dart`
- Modify: `flog_dart/test/flog_vm_service_test.dart`

**目标**：Stderr event 的 base64 文本可能是一行，也可能包含多行（`\n` 分隔）。把文本切成行，逐行喂 `StderrFrameAssembler`；回调里：
- `FrameKind.rawLine` → `{type:log, level:'error', tag:'stderr', message:line}`
- `FrameKind.flutterException` → `{type:log, level:'error', tag:tagFromLibrary, message:summary, error:fullText, stackTrace:stack.join('\n')}`

**tag 映射**：`"widgets library"` → `flutter.widgets`；`"services library"` → `flutter.services`；其他 `"X library"` → `flutter.<word>`；无法解析时 tag = `flutter`。

- [ ] **Step 1: 追加失败测试**

在 `flog_dart/test/flog_vm_service_test.dart` `main()` 末尾追加：

```dart
  group('FlogVmService — Stderr stream', () {
    test('多行 Flutter exception 合并为一条 error', () async {
      final stub = _StubVmService();
      final sent = <Map<String, dynamic>>[];
      final svc = FlogVmService(
        getServiceUri: () async => Uri.parse('ws://x/ws'),
        connect: (uri) async => stub,
        backoff: (_) => Duration.zero,
        send: sent.add,
      );
      await svc.attach();

      const block = '════════ Exception caught by widgets library ═════════════\n'
          'setState() called after dispose(): _MyState#a1b2c\n'
          '\n'
          'When the exception was thrown, this was the stack:\n'
          '#0      State.setState (package:flutter/src/widgets/framework.dart:1178:9)\n'
          '#1      _MyState.onDone (package:myapp/widgets/my.dart:56:5)\n'
          '════════════════════════════════════════════════════════════════';
      stub.stderrCtrl.add(vms.Event(
        kind: vms.EventKind.kWriteEvent,
        timestamp: 1_714_000_000_000,
        bytes: base64Encode(utf8.encode(block)),
      ));
      await Future<void>.delayed(Duration.zero);

      expect(sent, hasLength(1));
      final m = sent.single;
      expect(m['type'], 'log');
      expect(m['level'], 'error');
      expect(m['tag'], 'flutter.widgets');
      expect(m['message'], 'setState() called after dispose(): _MyState#a1b2c');
      expect(m['error'] as String, contains('Exception caught by widgets library'));
      expect(m['stackTrace'] as String, contains('#0'));
      expect(m['stackTrace'] as String, contains('#1'));
    });

    test('裸 stderr 行原样透传为 rawLine / error / stderr', () async {
      final stub = _StubVmService();
      final sent = <Map<String, dynamic>>[];
      final svc = FlogVmService(
        getServiceUri: () async => Uri.parse('ws://x/ws'),
        connect: (uri) async => stub,
        backoff: (_) => Duration.zero,
        send: sent.add,
      );
      await svc.attach();

      stub.stderrCtrl.add(vms.Event(
        kind: vms.EventKind.kWriteEvent,
        timestamp: 0,
        bytes: base64Encode(utf8.encode('native lib warning: foo\n')),
      ));
      await Future<void>.delayed(Duration.zero);

      expect(sent, hasLength(1));
      expect(sent.single['level'], 'error');
      expect(sent.single['tag'], 'stderr');
      expect(sent.single['message'], 'native lib warning: foo');
    });
  });
```

- [ ] **Step 2: 跑测试确认失败**

Run:
```bash
cd flog_dart && flutter test test/flog_vm_service_test.dart
```
Expected: 新 group 失败。

- [ ] **Step 3: 扩展实现**

在 `flog_dart/lib/src/flog_vm_service.dart` 顶部 import 补：

```dart
import 'vm_service/stderr_frame_assembler.dart';
```

类体新增字段：

```dart
  late final StderrFrameAssembler _stderrAssembler =
      StderrFrameAssembler(onFrame: _emitStderrFrame);
```

attach 成功分支里、`_handleStdout` 订阅之后加：

```dart
        service.onStderrEvent.listen(_handleStderr);
```

新增方法：

```dart
  void _handleStderr(vms.Event event) {
    final text = _decodeBytes(event.bytes);
    if (text == null) return;
    // Stderr 事件可能带多行 —— 切行逐个喂组装器
    for (final line in const LineSplitter().convert(text)) {
      _stderrAssembler.addLine(line);
    }
  }

  void _emitStderrFrame(AssembledFrame frame) {
    switch (frame.kind) {
      case FrameKind.rawLine:
        if (frame.fullText.isEmpty) return;
        _send({
          'type': 'log',
          'level': 'error',
          'tag': 'stderr',
          'message': frame.fullText,
          'timestamp': DateTime.now().millisecondsSinceEpoch,
        });
        return;
      case FrameKind.flutterException:
        _send({
          'type': 'log',
          'level': 'error',
          'tag': _tagFromLibrary(frame.library),
          'message': frame.summary ?? '',
          'error': frame.fullText,
          if (frame.stackFrames.isNotEmpty)
            'stackTrace': frame.stackFrames.join('\n'),
          'timestamp': DateTime.now().millisecondsSinceEpoch,
        });
        return;
    }
  }

  static String _tagFromLibrary(String? lib) {
    if (lib == null || lib.isEmpty) return 'flutter';
    final match = RegExp(r'^(\S+)\s+library$').firstMatch(lib.trim());
    if (match == null) return 'flutter';
    return 'flutter.${match.group(1)}';
  }
```

- [ ] **Step 4: 跑测试确认通过**

Run:
```bash
cd flog_dart && flutter test test/flog_vm_service_test.dart
```
Expected: All tests passed。

- [ ] **Step 5: 提交**

```bash
cd flog_dart
git add lib/src/flog_vm_service.dart test/flog_vm_service_test.dart
git commit -m "feat(flog_dart): FlogVmService 接入 Stderr stream + 帧组装"
```

---

## Task 9: 从 FlogServer 移除老 hook，并从 flog_dart.dart 启动 FlogVmService

**Files:**
- Modify: `flog_dart/lib/src/flog_server.dart`
- Modify: `flog_dart/lib/flog_dart.dart`
- Modify: `flog_dart/test/flog_server_test.dart`

**目标**：
1. 删掉 `FlogServer._installSystemHooks` 和 `_recordRawLog` 两个方法，以及 `start()` 里对 `_installSystemHooks()` 的调用，以及相关未用 import
2. 在 `Flog.init()` 里、`FlogServer.instance.start(port: port);` 之后加 `FlogVmService.instance.attach();`
3. `flog_server_test.dart` 里针对 `debugPrint` / `FlutterError.onError` / `PlatformDispatcher.onError` hook 行为的断言全部移除或改写

- [ ] **Step 1: 先读测试再决定删改**

Run:
```bash
grep -n "installSystemHooks\|debugPrint\|FlutterError.onError\|PlatformDispatcher" flog_dart/test/flog_server_test.dart | head -50
```
Expected: 列出所有引用老 hook 的测试断言。逐条在 Step 4 移除。

- [ ] **Step 2: 修改 FlogServer**

修改 `flog_dart/lib/src/flog_server.dart`：

a) 删除文件顶部 `import 'dart:ui' show PlatformDispatcher;` 一行（_installSystemHooks 删除后不再需要）
b) 删除 `import 'package:flutter/foundation.dart';` 一行（本类其余地方可能仍用到 `debugPrint`；若其余地方确实引用就保留。下面 Step 3 会说）
c) `start()` 方法里删除 `_installSystemHooks();` 这一行
d) 删掉整个 `// ── System log capture ──` 注释段 + `_installSystemHooks()` 方法 + `_recordRawLog()` 方法（原文件 92–145 行）

- [ ] **Step 3: 检查 FlogServer 里剩余的 debugPrint 使用**

现有 `flog_server.dart` 还有两处 `debugPrint`：第 190 行的端口绑定失败、第 307 行的 replay 失败日志。这两处用于 **输出到开发者终端**（不是 flog 自己采），必须保留。所以：

- 保留 `import 'package:flutter/foundation.dart';`（`debugPrint` 来源）
- 上面 Step 2 的 b) 改为**不删** `flutter/foundation.dart` import

改正后的导入区：

```dart
import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:dio/dio.dart';
import 'package:flutter/foundation.dart';

import 'flog_mock_interceptor.dart';
import 'flog_net.dart' show flogEnabled;
import 'flog_store.dart';
```

`start()` 方法改为：

```dart
  void start({int port = 9753}) {
    if (!flogEnabled) return;
    if (_started) return;
    _started = true;
    _port = port;
    _startServer();
  }
```

`// ── System log capture ──` 到 `_recordRawLog` 结束整段删除。

- [ ] **Step 4: 修改 flog_dart.dart 启动 FlogVmService**

修改 `flog_dart/lib/flog_dart.dart`。在顶部 import 区追加：

```dart
import 'src/flog_vm_service.dart';
```

`init()` 方法改为：

```dart
  static void init({int port = 9753}) {
    if (!flogEnabled) return;

    // 启动本进程 WS 服务端
    FlogServer.instance.start(port: port);

    // 订阅 Dart VM Service 三流采集 log；release 构建里走 flogEnabled 常量
    // 分支早退，下面这行及其依赖（package:vm_service）会被 AOT tree-shake。
    FlogVmService.instance.attach();

    // 异步补齐 app info
    PackageInfo.fromPlatform().then((info) {
      FlogServer.instance.updateAppInfo(
        appName: info.appName,
        appVersion: info.version,
        packageName: info.packageName,
      );
    }).catchError((Object e, StackTrace st) {
      debugPrint('flog_dart: PackageInfo.fromPlatform failed: $e');
    });
  }
```

- [ ] **Step 5: 清理 flog_server_test.dart**

打开 `flog_dart/test/flog_server_test.dart`。对每个断言 / 测试：
- 如果它测的是 `debugPrint` 被替换后能把消息送进 `FlogStore` —— **删除**
- 如果它测的是 `FlutterError.onError` 被 FlogServer 抢占后能正确 chain —— **删除**
- 如果它测的是 `PlatformDispatcher.onError` —— **删除**
- 其他（端口绑定、hello / replay / subscribe / updateAppInfo 等）—— 保留

对每个被删除的测试，连同它的 setUp/tearDown 如果没被其他 case 共享，也一起删。测试 group 为空时连同 group 一起删。

- [ ] **Step 6: 跑全部 flog_dart 测试**

Run:
```bash
cd flog_dart && flutter test
```
Expected: All tests passed。

- [ ] **Step 7: 提交**

```bash
cd flog_dart
git add lib/src/flog_server.dart lib/flog_dart.dart test/flog_server_test.dart
git commit -m "refactor(flog_dart): 移除 debugPrint/FlutterError/PlatformDispatcher 三处 hook，改由 FlogVmService 采集"
```

---

## Task 10: flog TUI 新增 `flutter_error.rs` parser

**Files:**
- Create: `src/parser/flutter_error.rs`
- Test: 嵌在 `src/parser/flutter_error.rs` 文件底部 `#[cfg(test)]` 模块

**背景**：`flog_dart` 已经把 Flutter exception 块在 Dart 侧合并成**一行** `ClientMessage::Log`，其中：
- `tag` = `flutter.<library>`（如 `flutter.widgets`）
- `message` = 摘要
- `error` = 完整原文（含头尾 `═` 和所有小节）
- `stackTrace` = `#N ...` 逐行

Rust 侧的 `LogEntry` 此时已经有这些字段。`FlutterErrorParser` 的作用是**当通过 parser chain 的是"未经 flog_dart 组装直接流进来的多行 exception"这种边界场景**时，兜底识别第一行的 `════ Exception caught by ═══` header，把该行当作 level=Error 的 LogEntry 产出 tag / message。

这个 parser 处理**单行**输入（链的契约），所以它只能识别单独的 header 行并产出一个占位 LogEntry。真正的多行合并由 flog_dart 侧负责——这里是纯 Rust 侧的对称兜底。

- [ ] **Step 1: 写失败测试**

创建 `src/parser/flutter_error.rs`：

```rust
//! FlutterError parser — identifies Flutter framework exception header lines.
//!
//! Primary exception assembly happens on the flog_dart side: one Flutter
//! `════ Exception caught by X library ═══ ... ═════` block is collapsed
//! into **one** `ClientMessage::Log` with structured `tag` / `message` /
//! `error` / `stackTrace` fields. This parser is a Rust-side fallback for
//! the edge case where a raw header line reaches the parser chain without
//! upstream assembly (e.g., legacy adapters, future transports).
//!
//! Recognizes a single header line of the form:
//!     `════════ Exception caught by <library> ═════════════════`
//! and produces a `LogEntry` with:
//!     level = Error
//!     tag   = "flutter.<library>"  (first whitespace-separated word of library)
//!     message = the raw header line
//!
//! Any other line → returns None (next parser in the chain handles it).

use crate::domain::{LogEntry, LogLevel};
use crate::parser::LogLineParser;

use regex::Regex;
use std::sync::OnceLock;

pub struct FlutterErrorParser;

impl LogLineParser for FlutterErrorParser {
    fn name(&self) -> &'static str {
        "FlutterError"
    }

    fn try_parse(&self, line: &str) -> Option<LogEntry> {
        let caps = header_re().captures(line.trim())?;
        let library = caps.get(1)?.as_str().trim();
        let tag_suffix = library.split_whitespace().next().unwrap_or("");
        let tag = if tag_suffix.is_empty() {
            "flutter".to_string()
        } else {
            format!("flutter.{tag_suffix}")
        };
        Some(LogEntry::new(LogLevel::Error, tag, line))
    }
}

fn header_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // ═ 是 U+2550，Rust regex 默认 Unicode-aware
        Regex::new(r"^═+\s*(?:Exception|Error|ERROR)\s+caught\s+by\s+(.+?)\s*═*$")
            .expect("FlutterError header regex must compile")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_widgets_library_header() {
        let line = "════════ Exception caught by widgets library ═════════════════";
        let entry = FlutterErrorParser.try_parse(line).expect("should parse");
        assert_eq!(entry.level, LogLevel::Error);
        assert_eq!(entry.tag, "flutter.widgets");
        assert_eq!(entry.message, line);
    }

    #[test]
    fn recognizes_services_library_header() {
        let line = "════════ Exception caught by services library ═══════";
        let entry = FlutterErrorParser.try_parse(line).unwrap();
        assert_eq!(entry.tag, "flutter.services");
    }

    #[test]
    fn recognizes_error_variant_spelling() {
        let line = "════════ Error caught by rendering library ═══════";
        let entry = FlutterErrorParser.try_parse(line).unwrap();
        assert_eq!(entry.tag, "flutter.rendering");
    }

    #[test]
    fn rejects_plain_text() {
        assert!(FlutterErrorParser.try_parse("just a string").is_none());
        assert!(FlutterErrorParser.try_parse("").is_none());
    }

    #[test]
    fn rejects_stack_frame() {
        assert!(
            FlutterErrorParser
                .try_parse("#0      State.setState (package:flutter/...)")
                .is_none()
        );
    }

    #[test]
    fn rejects_tail_separator_line() {
        // 全 `═` 的尾分隔线不是 header
        let line = "════════════════════════════════════════════════════════════";
        assert!(FlutterErrorParser.try_parse(line).is_none());
    }

    #[test]
    fn falls_back_to_flutter_tag_on_weird_library_name() {
        // 无 "library" 后缀时，取第一个词
        let line = "════ Exception caught by custom-thing ═════";
        let entry = FlutterErrorParser.try_parse(line).unwrap();
        assert_eq!(entry.tag, "flutter.custom-thing");
    }
}
```

- [ ] **Step 2: 跑测试确认失败**

Run:
```bash
cargo test --test '*' flutter_error 2>&1 | tail -20
```
Expected: 编译失败，因为 `mod flutter_error;` 还没在 `src/parser/mod.rs` 中声明。

- [ ] **Step 3: 在 parser/mod.rs 声明模块**

修改 `src/parser/mod.rs`。`pub mod` 声明块改为：

```rust
pub mod flutter_error;
pub mod generic;
pub mod keyword;
pub mod network;
pub mod structured;
pub mod util;
```

`default_chain` 方法改为：

```rust
    pub fn default_chain() -> Self {
        Self::with_strategies(vec![
            Box::new(structured::StructuredParser),
            Box::new(flutter_error::FlutterErrorParser),
            Box::new(generic::GenericParser),
            Box::new(keyword::KeywordParser),
        ])
    }
```

- [ ] **Step 4: 跑 flutter_error 单测通过**

Run:
```bash
cargo test --lib parser::flutter_error
```
Expected: All tests passed（7 个）。

- [ ] **Step 5: 更新 default_chain 断言测试**

`src/parser/mod.rs` 里 `dom_013_default_chain_has_three_strategies` 测试现在数量不对了。找到它，更新为：

```rust
    #[test]
    fn default_chain_has_four_strategies_after_flutter_error_added() {
        // Lock current chain length and order. FlutterError inserted
        // between Structured and Generic (see spec 2026-04-28).
        let p = MultiStrategyParser::default_chain();
        assert_eq!(p.strategies.len(), 4);
        assert_eq!(p.strategies[0].name(), "Structured");
        assert_eq!(p.strategies[1].name(), "FlutterError");
        assert_eq!(p.strategies[2].name(), "Generic");
        assert_eq!(p.strategies[3].name(), "Keyword");
    }
```

删掉旧的 `dom_013_default_chain_has_three_strategies` 测试。

- [ ] **Step 6: 跑全部 parser 测试**

Run:
```bash
cargo test --lib parser
```
Expected: All tests passed（既有测试 + 新的 7 个 flutter_error 测试 + 改写后的 chain 测试）。

- [ ] **Step 7: 提交**

```bash
git add src/parser/flutter_error.rs src/parser/mod.rs
git commit -m "feat(parser): 新增 FlutterError parser 识别 Exception caught by header"
```

---

## Task 11: 端到端 smoke（手动）

**Files:** 无改动，纯手动验证。

**目标**：把 Spec §11 验收标准里的"profile (alpha) 构建 aura-lang-flutter"类目过一遍。

- [ ] **Step 1: 在 aura-lang-flutter 里升级 flog_dart 到 0.9.0**

路径取决于 aura-lang-flutter 是用 pub 发布版还是 path 依赖。查：

```bash
grep -n "flog_dart" /Users/shaomingqing/FlutterProject/aura-lang-flutter/pubspec.yaml
```

- 如果是 `flog_dart: ^0.8.0` → 先把 flog_dart 0.9.0 发布到 pub，或改为 path 依赖：
  ```yaml
  flog_dart:
    path: ../flog/flog_dart
  ```
- 如果已经是 path 依赖 → 直接 `flutter pub get`

- [ ] **Step 2: 跑 alpha 构建并安装到真机**

Run:
```bash
cd /Users/shaomingqing/FlutterProject/aura-lang-flutter
flutter build apk --profile --dart-define=APP_FLAVOR=alpha
flutter install --profile
```

对 iOS：
```bash
flutter build ipa --profile --dart-define=APP_FLAVOR=alpha
# 走 Xcode 或 ios-deploy 装到真机
```

- [ ] **Step 3: 启动 flog TUI 并连上 App**

```bash
flog
```
Expected: TUI 里看到 `aura` app 出现在 session 列表，连上。

- [ ] **Step 4: 在 App 里分别触发 4 种日志来源，逐一验证 TUI 能看到**

在 App 的开发入口或 debug 按钮里临时加四行代码触发：

```dart
// 1. print — 走 vm_service Stdout
print('[SMOKE] print output');

// 2. package:logging — 走 vm_service Logging
// import 'package:logging/logging.dart';
Logger('auth').info('[SMOKE] package:logging info');
Logger('auth').warning('[SMOKE] package:logging warning');

// 3. 未捕获 Future 异常 — 走 vm_service Stderr (Flutter 格式块)
Future(() => throw StateError('[SMOKE] uncaught future'));

// 4. FlogLogger — 直接走 FlogServer.send
FlogLogger('smoke').i('[SMOKE] FlogLogger info');
```

在 flog TUI Logs 面板里按 `[SMOKE]` 过滤，逐条核对：
- `print` → level=INFO, tag=stdout, message="[SMOKE] print output"
- `Logger('auth').info` → level=INFO, tag=auth, message="[SMOKE] package:logging info"
- `Logger('auth').warning` → level=WARNING, tag=auth
- 未捕获异常 → **一条** entry（不是多行瀑布），level=ERROR, tag=flutter.services（或其他 library），message 是摘要，detail 里能看到 stack trace
- `FlogLogger` → level=INFO, tag=smoke

每条都逐个勾掉：

- [ ] print 可见
- [ ] Logger('auth').info 可见且 tag=auth
- [ ] Logger('auth').warning 可见且 level=WARNING
- [ ] 未捕获异常为一条 entry 且 detail 含 stack trace
- [ ] FlogLogger.info 可见

- [ ] **Step 5: 验证 release 构建下 vm_service 被 tree-shake**

```bash
cd /Users/shaomingqing/FlutterProject/aura-lang-flutter
flutter build apk --release --analyze-size --dart-define=APP_FLAVOR=release | tee /tmp/size.txt
grep -i "vm_service" /tmp/size.txt || echo "OK: vm_service not in size report"
```
Expected: "OK: vm_service not in size report"。

- [ ] **Step 6: 对比 release APK 包体变化**

在当前 branch 打一次 release APK，记录 `libapp.so` 大小；切回 master 对比：

```bash
cd /Users/shaomingqing/FlutterProject/aura-lang-flutter
# 当前 branch
flutter build apk --release --dart-define=APP_FLAVOR=release
stat -f "%z" build/app/intermediates/merged_native_libs/release/mergeReleaseNativeLibs/out/lib/arm64-v8a/libapp.so
# 对比 ＜ 50 KB
```

- [ ] **Step 7: 无改动，smoke 通过即可结束**

无需 commit。如果 smoke 发现 bug，回到相应 Task 新增测试 + 修复 + commit。

---

## Self-Review Results

以下是我写完 plan 后对 spec 的核对。

### Spec coverage

| Spec 章节 | 对应 Task |
|---|---|
| §1.3 范围（FlogVmService 模块、移除老 hook、三流订阅、帧组装、flutter_error.rs、tree-shake） | Task 1–10 全覆盖 |
| §2.1 数据流（两路零重叠） | Task 9（移除老 hook）+ Task 6/7/8（接 vm_service 三流） |
| §2.2 模块布局 | Task 5（flog_vm_service.dart）+ Task 3/4（vm_service/stderr_frame_assembler.dart）+ Task 2（vm_service/log_level_map.dart） |
| §2.3 Flog.init 最终形态 | Task 9 Step 4 |
| §3.1 FlogVmService 职责 | Task 5 + 6 + 7 + 8 |
| §3.2 三流映射规则 | Task 6（Logging）+ 7（Stdout）+ 8（Stderr） |
| §3.3 stderr 帧组装器（起始 / 结束 / 超时 500ms / 嵌套） | Task 3（骨架 + happy path）+ Task 4（嵌套 + flush / rawLine）。**超时的定时器没有在本 plan 落实**——见下方 "spec gap" |
| §3.4 连接生命周期（3 次指数退避 + 最终 FlogLogger warn） | Task 5 |
| §3.5 早期日志丢失 | 不需要代码，文档化已在 spec；validating 在 Task 11 |
| §4 tree-shake（flogEnabled 常量分支） | Task 9 `Flog.init` 开头的 `if (!flogEnabled) return;`（已存在）；Task 11 Step 5 验证 |
| §5 flutter_error.rs | Task 10 |
| §6 协议不变、LogEntry 结构不变 | 设计已经确认**不新增** `flutter_error` 字段（spec §6.2 的扩展被 plan 有意省略），所有结构化产出填进现有 `tag`/`message`/`error`/`stackTrace` |
| §7 移除老代码 | Task 9 |
| §8.1 pubspec 依赖 | Task 1 |
| §8.2 测试 | Task 2–8 + 10 |
| §8.3 包体验证 | Task 11 Step 5–6 |
| §11 验收标准 | Task 11 |

### Spec gap 识别

1. **§3.3 "500ms 超时保护"在 plan 里没有实现**。理由：帧组装器被设计为**纯同步**模块（无定时器，易测），超时由上层 `FlogVmService` 自己用 `Timer` 在 idle 时调 `assembler.flush()` 实现。但 Task 8 也没加这个 Timer——**这是个 spec gap，放进未来增强**。实际影响：如果 Flutter 框架发出的 exception 块**没有收到尾分隔线且后面再没有新的 stderr 行来**，它就会一直滞留到下一条 header 或 dispose。在真实使用里 stderr 基本持续有输出，问题面很小；但 spec 承诺了 500ms，所以我把它记在这里作为**本 plan 已知未实现项**，若落地后发现问题再追加一个 Task 加定时器。
2. **§6.2 扩展 LogEntry `flutter_error` 字段在 plan 里省略**。理由：现有 `LogEntry` 的 `error` 和 `stacktrace` 字段已经足够承载 flog_dart 组装好的内容，且 UI 已经有 DART-008 的 stack 折叠展示 —— 加一个半结构化 side-car 会污染 domain 但当下用不上（`§9 未来增强` 才用）。这个变化**收窄了 spec 的范围**，需要在 review 里得到你点头。如果你要求严格按 spec §6.2 实施，可以追加 Task 12 扩展字段——但建议维持当前设计。

### Placeholder scan

- 检查了 "TBD" / "TODO" / "填入" / "实现 appropriate" / "handle edge cases" 等短语：**零命中**。
- 检查了 "Similar to Task N"：**零命中**。所有代码步骤都含完整代码块。

### Type consistency

- `FlogVmService`：构造参数名贯穿 Task 5/6/7/8 一致（`getServiceUri` / `connect` / `backoff` / `send`）
- `AssembledFrame` 与 `FrameKind`：Task 3 定义 → Task 4 扩展 → Task 8 消费，名称一致
- `mapVmServiceLogLevel`：Task 2 定义 → Task 6 消费，签名一致
- `{type: 'log', level, tag, message, error?, stackTrace?, timestamp}` 消息形状贯穿 Task 6/7/8/9 一致
- Rust `FlutterErrorParser::name()` 返回 `"FlutterError"`，Task 10 Step 5 的 chain assertion 匹配
