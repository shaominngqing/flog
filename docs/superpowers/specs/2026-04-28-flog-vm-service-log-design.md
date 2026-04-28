# Spec: FlogVmService 模块与日志采集增强

- **日期**：2026-04-28
- **范围**：仅 `flog_dart` Dart 侧改造 + flog TUI 解析侧配合；不含性能面板
- **前置条件**：`flutter-build-scripts` 已将 `BUILD_TYPE=alpha` 映射到 `flutter build --profile`（见 commit `9527c74`），alpha 包内 VM Service 可用
- **后续**：性能面板（FPS / 内存 / CPU / jank）另开独立 brainstorming

## 1. 目标与动机

### 1.1 当前日志采集的盲区

`flog_dart` v0.8 通过在 `Flog.init()` 里改写三个全局钩子来采集日志：`debugPrint`、`FlutterError.onError`、`PlatformDispatcher.instance.onError`。这套方案只覆盖了日志来源的一个子集：

| 来源 | 老方案能拿到 |
|---|---|
| `FlogLogger.info/d/w/e(...)` | ✅ 直接 emit |
| `debugPrint(...)` | ✅ hook |
| `FlutterError.onError` | ✅ hook |
| `PlatformDispatcher.onError` | ✅ hook |
| `print(...)` 裸打印 | ❌ 无 hook |
| `dart:developer` 的 `log(...)` | ❌ 无 hook |
| `package:logging` 的 `Logger("x").info("...")` | ❌（除非用户自己桥到 debugPrint） |
| 第三方 native SDK 往 stdout/stderr 的输出 | ❌ |

现状：量最大的第三方库日志和 native SDK 输出看不到，开发者调试时要在 IDE 终端 + flog TUI 之间切换；这与 flog "让开发者在一个 TUI 里拿到全部上下文" 的定位冲突。

### 1.2 为什么现在可以改

- `--profile` 构建里 Dart VM Service 默认启用，vm_service 的 `Logging` / `Stdout` / `Stderr` stream 能覆盖上表所有 ❌ 的来源
- alpha 包改 `--profile` 后，开发者日常使用的 `flutter run`（debug）和 alpha 内测包都能走 vm_service 这条路
- release 包里 flog_dart 整体 tree-shake，采集边界不变（和现在一致）

### 1.3 范围边界

**本 spec 覆盖**：
- `flog_dart` 新增 `FlogVmService` 模块（独立文件）
- 移除三处老 hook（`debugPrint` / `FlutterError.onError` / `PlatformDispatcher.onError`）
- vm_service `Logging` / `Stdout` / `Stderr` 三流订阅、连接失败重试
- stderr 多行 Flutter exception 的帧组装
- flog TUI 侧新增 `parser/flutter_error.rs`，把组装后的 exception 文本解析成结构化 LogEntry
- release 构建通过 conditional import + `flogEnabled` 常量分支彻底 tree-shake `vm_service` 包依赖

**本 spec 不覆盖**（将来另立 spec）：
- 性能面板（FPS / 内存 / CPU profile / Timeline）——基础设施在本 spec 打下后再做
- flog TUI detail 面板里的 "error viewer 小节高亮"（第二档优化，placeholder 留在未来增强章节）
- `package:logging` 以外的结构化日志生态对接

## 2. 架构

### 2.1 最终数据流

```
flog_dart 侧：
  FlogLogger.info/d/w/e(...)  ─────────────┐
                                           ↓
                                       FlogStore  ──→  FlogServer (WS)
                                           ↑
  vm_service:                              │
    ├─ Logging  stream       ──────────────┤   (dart:developer.log, package:logging)
    ├─ Stdout   stream       ──┐           │   (print, native stdout)
    └─ Stderr   stream       ──┤──→ stderr ┘   (uncaught exception text, native stderr)
                                frame assembler
                                (Flutter ══════ block → 1 LogEntry)

flog TUI 侧：
  WS ClientMessage::Log  →  parser chain:
                              structured.rs
                              generic.rs
                              keyword.rs
                              flutter_error.rs  ← 新增
                              network.rs
                            → LogStore → UI
```

**两条采集路径，零重叠**：
- `FlogLogger` 是 flog_dart 自己的结构化 API，自己 emit，**不走** `dart:developer.log`，vm_service 看不到它
- 其他所有来源都由 vm_service 统一覆盖

没有去重需求（前一版设计需要去重是因为 debugPrint hook 和 vm_service stdout 都能抓 `debugPrint → print`，现在老 hook 全部移除，不再有重叠）。

### 2.2 模块布局（方案 A）

```
flog_dart/lib/src/
├── flog_server.dart              [保留] WS 服务端、connection、向 TUI 发消息
├── flog_store.dart               [保留] 本地 ring buffer，订阅回放
├── flog_logger.dart              [保留] FlogLogger 结构化 API
├── flog_http_interceptor.dart    [保留] 不变
├── flog_mock_interceptor.dart    [保留] 不变
├── flog_net.dart                 [保留] flogEnabled 常量
├── flog_vm_service.dart          [新增] 采集器入口：所有 vm_service 依赖都封装在此
└── vm_service/                   [新增] 帧组装器、level 映射等内部子模块（可选拆分）
flog_dart/lib/flog_dart.dart      [改] Flog.init() 逻辑瘦身
```

`FlogServer` 完全不感知 vm_service 存在。`FlogVmService` 通过 `FlogServer.send({'type': 'log', ...})` 把采集到的事件喂给下游——接口与 `FlogLogger` 走的路径完全一致。

**重要**：`flog_vm_service.dart` 是**无条件被 import** 的常规文件，tree-shake 靠 §4.2 的运行时 `const` 分支实现，**不用** conditional import（Dart 不支持基于用户常量的 conditional import）。

### 2.3 `Flog.init()` 的最终形态

```dart
static void init({int port = 9753}) {
  if (!flogEnabled) return;

  FlogServer.instance.start(port: port);
  FlogVmService.instance.attach();   // 连不上会静默重试，失败后 emit 一条 warn

  PackageInfo.fromPlatform().then((info) {
    FlogServer.instance.updateAppInfo(...);
  }).catchError(...);
}
```

对比现状，消失的部分是三处 hook 代码及其 `_emit` 管道（~30 行）。API 签名不变，v0.x 用户代码零改动。

## 3. `FlogVmService` 模块细节

### 3.1 职责

- 获取本进程 VM Service URL（`Service.getInfo()`）
- 建立 WebSocket 连接（通过 `vm_service` 包的 `vmServiceConnectUri`）
- 订阅 `Logging` / `Stdout` / `Stderr` 三个 stream
- 把 stream event 转换为 `type: 'log'` 消息格式，调 `FlogServer.instance.send(...)`
- stderr 多行 Flutter exception 的**帧组装**（下一节专门说）
- 连接失败指数退避重试，最终失败后通过 `FlogLogger` emit 一条 warn

### 3.2 三流的映射规则

| stream | event payload | → LogEntry |
|---|---|---|
| `Logging` | `LogRecord{ level, loggerName, message, error, stackTrace, time }` | level 映射（Dart int → flog level），tag = loggerName，message = message，error/stackTrace 直传 |
| `Stdout` | `Event{ bytes: base64 of line }` | level = `info`，tag = `stdout`，message = 解码后文本 |
| `Stderr` | `Event{ bytes: base64 of line }` | **走帧组装器**（下节），组装失败才走 fallback：level = `error`, tag = `stderr` |

**level 映射**（`dart:developer.log` 的 level 是任意 int）：

| `LogRecord.level` | flog level |
|---|---|
| ≤ 500 (FINE/FINER/FINEST) | `debug` |
| 500..<800 (CONFIG/INFO) | `info` |
| 800..<900 (WARNING) | `warning` |
| ≥ 900 (SEVERE/SHOUT) | `error` |

对齐 `package:logging` 的 `Level` 阶梯。

### 3.3 stderr 帧组装器

Flutter framework 打印未捕获异常的标准格式：

```
════════ Exception caught by <library> ════════════════════════════════════════
<summary>

<"The relevant error-causing widget was:" / 其他小节>
...

When the exception was thrown, this was the stack:
#0  ...
#1  ...
════════════════════════════════════════════════════════════════════════════════
```

**组装规则**：

1. 起始信号：`════════` 开头且包含 `Exception caught by` 或 `ERROR caught by`
2. 结束信号：**下一条** 以全 `═` 组成、长度 ≥ 60 的行
3. 起始和结束之间的所有行（含头尾分隔线）拼成**一条** LogEntry
4. 抽取：
   - `tag`: 从头行解析 `caught by XXX` 的 XXX（如 `widgets library` → `flutter.widgets`）
   - `message`: 摘要行（紧跟头分隔线之后的第一个非空行）
   - `stackTrace`: "When the exception was thrown" 之后直到尾分隔线的所有 `#N` 行
   - `error`: full 组装文本（保留所有小节，供 flog TUI 的 `flutter_error.rs` 进一步解析）
   - `level`: `error`
5. 超时保护：如果起始后 **500ms** 内没有收到结束分隔线，强制结束当前帧并 emit（避免 UI 无限等待）
6. 嵌套处理：组装期间遇到新的起始信号，说明上一块没正常收尾——立即 emit 当前缓冲区（带 warn 标记），然后以新信号重新开始
7. 非组装行：**原样转发**，level = `error`, tag = `stderr`（覆盖不走分隔线格式的 stderr，比如 native SDK 的裸 `fprintf`）

### 3.4 连接生命周期

```
attach()
  ├─→ if (!flogEnabled) return;                      // release 防御性早退
  ├─→ retry loop (3 attempts, backoff 500ms/1s/2s):
  │     ├─ info = await Service.getInfo()
  │     ├─ if (info.serverUri == null) → retry 下一次
  │     ├─ service = await vmServiceConnectUri(wsUri)
  │     ├─ await service.streamListen('Logging')
  │     ├─ await service.streamListen('Stdout')
  │     ├─ await service.streamListen('Stderr')
  │     ├─ 订阅 service.onLoggingEvent / onStdoutEvent / onStderrEvent
  │     └─ 成功 → 退出 retry loop
  └─→ 3 次都失败:
        FlogLogger('flog_dart').w(
          'FlogVmService: failed to attach after 3 retries; '
          'log stream will be limited to FlogLogger calls only'
        );
```

**不重试的场景**：
- `flogEnabled == false`（release 构建）——根本不调 `attach()`
- `Service.getInfo().serverUri` 返回 null 三次——VM Service 未启用（极罕见，通常只有 `--no-enable-vm-service` 强制关闭），告警后放弃
- 连接成功后运行时断开（WebSocket 异常 / isolate exit）——不重试，通常意味着 isolate 已挂，重试无意义

**重试时间**：指数退避，500ms → 1s → 2s。首轮 500ms 足以覆盖 `Flog.init()` 与 VM Service 服务端真正 ready 之间的启动竞争窗口。

### 3.5 错误与早期日志丢失

`Service.getInfo()` 和 stream subscribe 是异步的，从 `Flog.init()` 调用到三流开始接事件之间有**几十毫秒到几百毫秒**的窗口。这段时间的 stdout/stderr/log 事件会丢（vm_service 不做历史回放）。

**接受这个损失，理由**：
- `Flog.init()` 调用时机本来就要求在 `WidgetsFlutterBinding.ensureInitialized()` 之后立即，业务 log 还没开始
- 如果用户必须采到早期日志，可以主动用 `FlogLogger` 而不是 `print`，走进程内直发路径（不经 vm_service，零延迟）
- 这个损失在文档里明示

## 4. Release 构建的 tree-shake 方案

### 4.1 问题

`vm_service` 包是 Dart SDK 官方维护的大包（~20 KLoC 生成代码）。`import 'package:vm_service/vm_service.dart'` 这一行本身不会 tree-shake——AOT 编译器保守地保留一切被 import 的包。即便 `flogEnabled = false` 的运行时分支不会被执行，代码文本仍然会被编译进 release 二进制，膨胀包体。

### 4.2 方案：常量分支 + 无条件 import

`flog_vm_service.dart` 无条件 import `package:vm_service/vm_service.dart`。顶层入口通过 `flogEnabled` 常量的 const 分支剪枝掉所有引用：

```dart
// lib/flog_dart.dart
import 'src/flog_net.dart' show flogEnabled;
import 'src/flog_vm_service.dart' as vms;

static void init({int port = 9753}) {
  if (!flogEnabled) return;                // const 分支
  FlogServer.instance.start(port: port);
  vms.FlogVmService.instance.attach();
  // ...
}
```

**原理**：`flogEnabled` 是 `const bool`（`bool.fromEnvironment('FLOG_ENABLED', defaultValue: ...)`，经 `APP_FLAVOR` 推导也是 const）。Dart AOT 编译器在 release 下：

1. 常量传播：`flogEnabled` 展开为 `false`
2. 不可达代码消除：`if (false) return;` 之后的代码全部剪除
3. 静态引用分析：`vms.FlogVmService.instance.attach()` 不可达，`vms` 的 import 变成 unused
4. 包级 tree-shake：`flog_vm_service.dart` 及其 `vm_service` 包依赖全部剪除

结果：release 二进制里 `vm_service` 包零字节。

> **为什么不用 conditional import（`if (dart.library.X)` 形式）**？Dart 的 conditional import 只支持 `dart.library.*` 这种 SDK 内置常量分支（用来区分 web / io / etc），不支持用户定义的 `flogEnabled`。强行用会触发分析错误。常量分支 + 顶层 import 是 Dart 对"用户 flag 控制的 tree-shake" 的标准做法，Flutter framework 的 `kReleaseMode` / `kDebugMode` 分支就是同样机制。

**验证方式**（写进 spec 的验收标准）：
- release 构建 APK 的 `libapp.so` 大小对比（加 FlogVmService 前后差异 < 50KB，证明 vm_service 包已被剪枝）
- `flutter build apk --release --analyze-size` 报告中搜索 `package:vm_service`，应无条目

### 4.3 `flogEnabled` 当前值域

现状（不改）：

| `APP_FLAVOR` dart-define | 是否 Flutter release 构建 | `flogEnabled` |
|---|---|---|
| `release` | 是 | false |
| `alpha` | 否（现在改成 --profile） | true |
| `alpha` | 是（误配置，不应发生） | false |
| 未设 | debug | true |
| 未设 | release | false |

Release 构建 + APP_FLAVOR=alpha 的组合物理上不会再出现（脚本已改为 `alpha → --profile`），但 `flogEnabled` 保留 `!kReleaseMode` 作为保护兜底。

## 5. flog TUI 侧：`flutter_error.rs` parser

### 5.1 位置

```
flog/src/parser/
├── structured.rs
├── generic.rs
├── keyword.rs
├── flutter_error.rs    [新增]
├── network.rs
├── util.rs
└── mod.rs   ←  MultiStrategyParser::default_chain 插入顺序
```

**链中位置**：`structured` 之后、`generic` 之前。理由：Flutter error 的外形（═ 分隔线 + "Exception caught by") 比 `generic.rs` 的启发式模式更明确，优先匹配避免被 `generic.rs` 截掉头部。

### 5.2 输入契约

`FlogVmService` 侧的帧组装器已经保证：一个 Flutter exception → 一个 `ClientMessage::Log` event → 一条 `LogEntry`，其中 `error` 字段承载**整个组装后的文本块**（含头尾分隔线和所有小节）。

`flutter_error.rs` 不需要自己做跨行合并——那是 flog_dart 的责任。它只需要在**单条 LogEntry** 里做结构化解析。

### 5.3 解析规则

输入：LogEntry 的 `error` 字段文本。

识别条件（所有命中才启用此 parser）：
- 第一行以 `════════` 开头且匹配正则 `^═+\s*(Exception|Error|ERROR)\s+caught\s+by`
- 存在尾行全 `═` 组成、长度 ≥ 60

产出结构（扩展 `LogEntry` 或新增 side-car 字段，见 §6）：

```rust
struct FlutterErrorParsed {
    library: String,              // "widgets library"
    summary: String,              // 摘要首段
    sections: Vec<ErrorSection>,  // 小节
    stack_frames: Vec<StackFrame>, // 已结构化的栈
}

struct ErrorSection {
    heading: String,   // "The relevant error-causing widget was"
    body: String,
    // 特殊 section：widget_locations 提取 file://... 路径，供 TUI 高亮
    kind: SectionKind,
}

enum SectionKind {
    Summary,
    WidgetContext { locations: Vec<SourceLocation> },
    Information,
    Stack,     // 冗余，stack_frames 是真数据源
    Unknown,
}

struct SourceLocation {
    file: String,       // "path/to/widget.dart"
    line: usize,
    column: usize,
}

struct StackFrame {
    // 复用 DART-008 已有的 stack frame 结构体
}
```

小节分割规则：
- 按**空行 + 下一个首字母大写的单行标题**切段（Flutter framework 的输出约定）
- 遇到 "When the exception was thrown, this was the stack:" 后进入 stack 模式，逐行解析 `#N ` 格式直到尾分隔线
- 任何不匹配已知小节的段 → `SectionKind::Unknown`，原样保留 body

### 5.4 解析失败的兜底

如果任一必需字段（library / stack frames）解析失败：
- **不丢数据**：LogEntry 的原始 `error` 字段文本照常透传给 UI
- parser 返回 "未解析成功"，后续 parser 继续尝试（`generic.rs` / `keyword.rs` 兜底）
- 为调试此 parser 的健壮性，在 `keyword.rs` 的统计里单独记一个 "flutter_error_fallback" 计数（可选，feature gated）

## 6. 协议与 `LogEntry` 结构

### 6.1 协议接口不变

`ClientMessage::Log` 的 JSON 格式不变：

```json
{
  "type": "log",
  "level": "error",
  "tag": "flutter.widgets",
  "message": "setState() called after dispose(): ...",
  "error": "════════ Exception caught by widgets library ════════...\n<完整原文>",
  "stackTrace": "#0 ...\n#1 ...",
  "timestamp": 1714000000000
}
```

现有 Rust 侧 `ClientMessage::Log` 的 serde 定义、`dispatch_client_message`、`LogStore` 全部不需要改——新 parser 消费的是 `error` 字段，这个字段本来就存在。

### 6.2 `LogEntry`（Rust 侧）扩展

`domain/entry.rs` 的 `LogEntry` 可以增加一个 optional side-car：

```rust
pub struct LogEntry {
    // 现有字段不变
    pub level: LogLevel,
    pub tag: String,
    pub message: String,
    pub error: Option<String>,
    pub stack_trace: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub source: InputSource,

    // 新增：flutter_error parser 解析成功时填充
    pub flutter_error: Option<FlutterErrorParsed>,
}
```

`FlutterErrorParsed` 为 None 时 UI 走现有渲染路径，无回归。为 Some 时未来 UI 可以用它做结构化展示（但**本 spec 不做** UI 渲染改造，放 §9 未来增强）。

## 7. 移除老代码清单

`flog_dart/lib/flog_dart.dart` 现状：

- `Flog.init()` 中三处 hook 设置 → **全部删除**
- 对应的 `FlutterError.onError` 保存 + 转发逻辑 → 删除
- `PlatformDispatcher.instance.onError` 设置 → 删除
- `debugPrint` 替换逻辑 + `originalDebugPrint` 保留 → 删除

`flog_dart/lib/src/flog_server.dart` 或其他文件如果有对应的 `_emit(details)` 帮助函数、`_installHooks()` 之类的私有方法 → **也删除**（具体文件位置见 flog_dart 源码审计结果）。

**副作用警告**（必须写进 CHANGELOG）：
- flog_dart v0.9 不再抢占 `debugPrint` / `FlutterError.onError` / `PlatformDispatcher.onError` 这三个全局钩子
- 如果用户应用代码**依赖** flog_dart 设置这些钩子（例如 App 里再链式调老实现），**将会失败**
- 升级指南：用 `FlogLogger` 替代业务场景的日志；框架 error 由 VM Service Stderr stream 自动捕获，不需手动调钩子

## 8. 依赖、打包、测试

### 8.1 pubspec 新增依赖

```yaml
dependencies:
  vm_service: ^14.0.0   # Dart SDK 官方包，锁住 major 即可
```

当前 `flog_dart` 依赖清单较小（`dio`、`package_info_plus` 等），增加 `vm_service` 会扩大**编译时**依赖面（生成代码多）。运行时在 release 下通过 §4.2 的常量分支 tree-shake，不增加 release 包体。

### 8.2 测试

- `flog_dart/test/flog_vm_service_test.dart`：
  - 帧组装器：输入模拟的 Flutter exception 多行流，断言正确切帧
  - 帧组装器：输入跨帧边界、超时、嵌套场景，验证 fallback 逻辑
  - level 映射：各档 int level 映射到 flog level 正确
  - retry 逻辑：mock `Service.getInfo()` 返回 null/抛错，断言 backoff 时序
- `flog/tests/flutter_error_parser_test.rs`：
  - 典型 Flutter exception 文本 → 解析出预期结构
  - 尾分隔线缺失 → 返回未解析，交给后续 parser
  - widget context 段 → 提取到 `SourceLocation`
- 集成测试（现有 `tests/ws_connect_test.rs` 扩展）：
  - 构造带 ═ 头的 ClientMessage::Log → 最终 LogEntry 有 `flutter_error` Some

### 8.3 包体验证

release 构建用 `flutter build apk --release --analyze-size`，在 size report 中：
- `package:vm_service` 应**无条目**（证明 tree-shake 成功）
- `libapp.so` 体积相对改造前波动 < 50KB

## 9. 未来增强（非本 spec 范围）

- **flog TUI detail 面板 error viewer**：利用 `FlutterErrorParsed.sections` 做小节高亮、widget location 可复制（DART-008 stack trace 高亮扩展）
- **Logging stream 补结构化**：对 `loggerName` 做 alias 归一（`package:foo.Logger` → `foo`），在 TUI 侧做 tag 聚合
- **性能面板**：基于同一个 `FlogVmService` 模块扩展 `getCpuSamples` / `getAllocationProfile` / Timeline 订阅
- **自定义 error 格式**：如果业务自己的 framework 也用 ═ 分隔线格式，支持配置别名

## 10. 风险与取舍

| 风险 | 影响 | 缓解 |
|---|---|---|
| 结构化 error 降级为文本 | Widget `DiagnosticsNode` 树递归 / `InformationCollector` 回调信息丢失 | 接受；flog 现有 UI 也不用这些字段。未来需要时回到 hook 方案需要评估 |
| `Service.getInfo()` 异步启动窗口日志丢失 | 启动后几十到几百 ms 内的 stdout/stderr 看不到 | 文档说明；建议业务用 FlogLogger（直发，无窗口） |
| iOS 真机 stdout stream 部分事件丢失 | `print` 输出 / native stdout 可能不可靠 | 接受；业务 log 走 FlogLogger 不受影响；第三方库如果落到 `dart:developer.log` 也不受影响 |
| `vm_service` 包版本升级 breaking | 升级时 API 签名变化 | pin major 版本 `^14.0.0`；升级时专项测试 |
| 移除老 hook 影响现有 flog_dart 用户 | 依赖 `FlutterError.onError` 被 flog 设置的用户代码会失败 | flog_dart v0.9 major bump；CHANGELOG 明示迁移路径；aura-lang-flutter 是已知唯一用户，可同步升级 |

## 11. 验收标准

- [ ] flog_dart v0.9 发布：
  - [ ] `FlogVmService` 模块存在且独立成文件
  - [ ] `Flog.init()` 里原三处 hook 代码 grep 不到
  - [ ] 单元测试覆盖帧组装、level 映射、retry
- [ ] release 构建：
  - [ ] `--analyze-size` 报告无 `package:vm_service`
  - [ ] release APK 体积相比改造前变化 < 50KB
- [ ] profile (alpha) 构建的 aura-lang-flutter：
  - [ ] `print("hello")` 能在 flog TUI 里看到
  - [ ] `Logger("auth").info("ok")`（package:logging）能在 flog TUI 里看到，tag = `auth`
  - [ ] 主动 throw 一个未捕获 exception，flog TUI 里显示为**一条** LogEntry（不是几十行瀑布），level = error，stackTrace 字段填充，`flutter_error` side-car Some
  - [ ] `FlogLogger.info("x")` 照常工作
- [ ] 连接失败场景：
  - [ ] 手动 `--no-enable-vm-service` 跑，flog TUI 能看到 FlogLogger 发的 warn：`FlogVmService: failed to attach after 3 retries`
  - [ ] FlogLogger 自身 log 流不受影响
