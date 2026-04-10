# flog

```
███████╗██╗      ██████╗  ██████╗
██╔════╝██║     ██╔═══██╗██╔════╝
█████╗  ██║     ██║   ██║██║  ███╗
██╔══╝  ██║     ██║   ██║██║   ██║
██║     ███████╗╚██████╔╝╚██████╔╝
╚═╝     ╚══════╝ ╚═════╝  ╚═════╝
```

**给 Flutter 开发者的终端日志查看器 + 网络调试器。**

### ▤ Logs — 实时日志流

![日志列表](docs/screenshot-logs.png)

![日志详情面板](docs/screenshot-logs-detail.png)

### ⇄ Network — 网络请求检查器

![网络请求列表](docs/screenshot-network.png)

![SSE 流式请求详情](docs/screenshot-network-sse.png)

```bash
curl -fsSL https://raw.githubusercontent.com/shaominngqing/flog/master/install.sh | bash
```

## 解决什么问题

Flutter 开发中看日志有两个烦的点：

**终端日志不可读** — `flutter run` 的输出里业务日志和系统日志混在一起，没有级别区分、没有颜色、没法过滤、JSON 挤成一行。要在一堆 `I/flutter`、`W/1.raster`、`D/TrafficStats` 里找到你关心的信息，全靠眼睛扫。

**网络请求难调试** — 想看请求详情要么加 print，要么开 DevTools，每次重启都要重连。没有一个轻量的方式在终端里直接看 HTTP/SSE/WebSocket 请求。

## flog 做了什么

flog 是一个独立运行的终端日志查看器 + 网络调试器。你把它开在一个终端窗口里，它自动连接你运行中的 Flutter 应用，实时显示结构化的日志和网络请求。

**两个标签页：**

- **▤ Logs** — 实时日志流，级别颜色区分，Tag 对齐，系统噪音过滤掉，JSON 可折叠展开
- **⇄ Network** — Flipper 风格的网络检查器，支持 HTTP/SSE/WebSocket，请求详情、Headers、Body 全部可查看

**不用重新打开** — flog 常驻运行，`flutter run` 重启后自动重连，不需要手动操作。

## 数据源

- **VM Service** — 通过 WebSocket 连接 Flutter VM，自动发现运行中的实例，通过 DDS 代理连接不影响 `flutter run`
- **ADB** — 通过 `adb logcat` 读取 Android 设备/模拟器日志，自动过滤 Flutter 相关 tag
- **stdin** — 管道模式，支持 `flutter run 2>&1 | flog --stdin`

## Logs 功能

- 按级别过滤（Verbose / Debug / Info / Warning / Error）
- 按 Tag 过滤（支持包含/排除，支持正则）
- 全文搜索（支持正则，高亮匹配，`n/N` 跳转）
- 详情面板（JSON 可折叠树，语法高亮，深度着色）
- 书签（右键标记，方便回看）
- 日志导出（导出过滤后的结果到文件）
- 统计视图（日志级别分布、Tag 排名）
- 时间线热力图（日志密度分布）
- 重复日志折叠
- 10 万条日志环形缓冲

## Network 功能

- HTTP / SSE / WebSocket 三种协议支持
- 请求列表（Protocol、Method、URL、Status、Duration、Size）
- 详情面板（可折叠 JSON 树）：
  - General（URL、Method、Status、Duration、Size）
  - Query Parameters（自动解析 URL 参数）
  - Request / Response Headers
  - Request / Response Body（JSON 语法高亮，逐层展开）
  - SSE Events（每个 chunk 可展开查看 JSON）
  - WebSocket Messages（发送/接收，方向标记）
- 过滤器（Protocol / Method / Status 行内 pill 切换）
- URL 搜索
- Copy as cURL（一键复制为 curl 命令）
- Copy Response（复制响应体）
- 自动滚动 + LIVE 指示器
- 1 万条请求缓冲

## 用法

```bash
# 自动发现模式（推荐）— 先开 flog，再 flutter run
flog

# ADB 模式
flog --adb
flog --adb -s emulator-5554

# 指定 VM Service 地址
flog --uri ws://127.0.0.1:8181/TOKEN=/ws

# 管道模式
flutter run 2>&1 | flog --stdin

# 启动时指定过滤
flog --level w
flog --tag Network
```

## 搭配 flog_dart

flog 能识别任何 Flutter 日志输出。搭配 [flog_dart](https://pub.dev/packages/flog_dart) 可以获得精确的级别和 Tag 解析 + Network Inspector：

```bash
# pubspec.yaml
dependencies:
  flog_dart: ^0.2.0
```

### 日志

```dart
import 'package:flog_dart/flog_logger.dart';

final log = FlogLogger('Network');
log.i('-> GET /api/users');
log.e('Connection failed: $e');
```

### Network Inspector

```dart
final dio = Dio();
dio.interceptors.addAll([
  FlogHttpInterceptor(),        // ← 必须放在最前面
  ApiResponseInterceptor(),     // 业务逻辑拦截器
  LoggingInterceptor(),
]);
```

> **注意：** `FlogHttpInterceptor` 必须添加在其他会修改或拦截响应的 interceptor **之前**。如果放在后面，当其他 interceptor 调用 `handler.reject()` 时，flog 看不到原始响应，请求会一直显示为 Pending 状态。

### SSE 流式请求

```dart
await for (final data in FlogSseParser.wrap(
  response.data!.stream,
  url: '/api/chat/completions',
  method: 'POST',
)) {
  final json = jsonDecode(data);
  // ...
}
```

### WebSocket

```dart
final ws = await FlogWebSocket.connect('wss://example.com/ws');
ws.send(jsonEncode({'type': 'hello'}));
ws.stream.listen((data) => print(data));
await ws.close();
```

## 快捷键

### Logs

| 按键 | 功能 |
|------|------|
| `1` / `2` | 切换 Logs / Network 标签页 |
| `/` | 搜索（支持 `/正则/i`） |
| `n` / `N` | 下一个/上一个匹配 |
| `j/k` 或方向键 | 移动选择 |
| `PgUp` / `PgDn` | 翻页 |
| `Enter` | 打开/关闭详情面板 |
| 右键 | 书签 |
| `c` | 复制日志 |
| `e` | 导出 |
| `S` | 统计 |
| `s` | 选择模式（终端文字选择） |
| `?` | 帮助 |
| `Esc` | 清除过滤 |
| `q` | 退出 |

### Network

| 按键 | 功能 |
|------|------|
| `/` | URL 搜索 |
| `c` | Copy as cURL |
| `y` | Copy Response |
| `Enter` | 打开/关闭详情面板 |
| `s` | 选择模式 |

## 安装

```bash
# 一键安装
curl -fsSL https://raw.githubusercontent.com/shaominngqing/flog/master/install.sh | bash

# 或通过 Cargo
cargo install flog
```

支持 macOS (Intel / Apple Silicon)、Linux (x86_64 / aarch64)、Windows。

## License

MIT

---

[English](README_EN.md)
