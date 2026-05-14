# flog_dart WebSocket 握手失败覆盖 + back-compat 清理

**日期:** 2026-05-14  
**状态:** 待实现

---

## 背景与问题

`flog_dart` 当前对 WebSocket 连接建立阶段的失败没有覆盖：

- `FlogWebSocket(Uri)` 同步构造器内部调用 `WebSocketChannel.connect()`，但从不 `await channel.ready`。握手失败（DNS 解析失败、TLS 错误、4xx/5xx HTTP 响应、connection refused）时异常向上抛，flog 完全不知道这次连接发生过，network 面板看不到任何记录。
- HTTP/Dio 通过 `FlogHttpInterceptor.onError` 覆盖了连接层失败——这是三种协议里唯一覆盖完整的。
- SSE 的初始 HTTP 失败由 Dio 拦截器覆盖；流中断/parse 错误由 `FlogSseReporter` 捕获——覆盖完整。

此外，`flog_dart.dart` 仍对外 export `nextNetId`、`emitNet`（标注 `@internal`，注释说"v0.x back-compat，v1.0 删"）。当前处于开发阶段，一并清除。

---

## 目标

1. **WebSocket 握手失败进 network 面板**：无论连接方式，失败条目自动可见。
2. **使用方零样板**：调一个公开 API，成功/失败都自动上报，不需要手动 emit。
3. **覆盖第三方建连方式**：不要求使用方换掉 `dart:io WebSocket` 或其他 WS 客户端。
4. **清除 back-compat 历史包袱**：`nextNetId`/`emitNet` 不再对外 export。
5. **三种协议行为一致**：HTTP/SSE/WS 在 network 面板里的失败记录格式对齐。

---

## API 设计

### 删除

```dart
// 删除同步构造器——无法 await 握手，有覆盖盲区
FlogWebSocket(Uri uri, {Iterable<String>? protocols})
```

### 新增

```dart
/// 建立 WebSocket 连接并注册到 flog network 面板。
/// 握手成功 → 返回 FlogWebSocket，emit open 帧。
/// 握手失败 → emit err 帧（含 url、error、duration），rethrow 原异常。
static Future<FlogWebSocket> connect(
  Uri uri, {
  Iterable<String>? protocols,
})

/// 包裹任意建连方式（dart:io WebSocket、第三方客户端等）。
/// [connect] 是返回已建立 WebSocketChannel 的 async 工厂函数。
/// 成功/失败行为与 FlogWebSocket.connect 完全一致。
static Future<FlogWebSocket> wrap(
  Future<WebSocketChannel> Function() connect, {
  required String url,
})
```

### 保留

```dart
// 服务端 HTTP upgrade 场景：channel 已建立，只需包裹
FlogWebSocket.fromChannel(WebSocketChannel channel, {required String url})
```

### flog_dart.dart export 清理

删除以下三个 back-compat export：
- `nextNetId`
- `emitNet`  
- `flogEnabled`（若仅为 back-compat 导出）

`emitNet`/`nextNetId` 仍在 `src/flog_net.dart` 内部使用，只是不再公开。

---

## 数据流

`connect` 和 `wrap` 内部逻辑完全一致：

```
FlogWebSocket.connect(uri) / FlogWebSocket.wrap(fn, url: url)
  │
  ├─ 1. id = nextNetId()
  ├─ 2. start = DateTime.now()
  ├─ 3. channel = WebSocketChannel.connect(uri)  // connect
  │    或 channel = await fn()                   // wrap
  ├─ 4. await channel.ready
  │     │
  │     ├─ 成功 → emitNet({ t:'open', id, url, ts })
  │     │         return FlogWebSocket._fromConnected(channel, id, start, url)
  │     │
  │     └─ 失败 → emitNet({ t:'err', id, url,
  │                          error: e.toString(),
  │                          duration: elapsed_ms })
  │               rethrow e
  │
  └─ _fromConnected 复用现有 _initFromChannel 逻辑
```

**emit 格式与 `FlogHttpInterceptor.onError` 对齐**，TUI 无需改动。  
`duration` 在失败路径也记录，TUI 可显示"连了多久才失败"。  
`rethrow` 保证原始异常类型不变，业务侧 catch 逻辑不受影响。

---

## 业务侧迁移（aura-lang-flutter 参考）

当前模式（azure_stt_service / azure_tts_service / live_ws_transport）：

```dart
// 改前
final socket = await WebSocket.connect(url, headers: {...});
final channel = IOWebSocketChannel(socket);
final flogWs = FlogWebSocket.fromChannel(channel, url: url.toString());
```

两种等价迁移路径：

```dart
// 选项 A：换用 FlogWebSocket.connect（仅适用于无自定义 headers 的场景）
final flogWs = await FlogWebSocket.connect(Uri.parse(url));

// 选项 B：用 wrap 保留原有建连逻辑（推荐，兼容自定义 headers）
final flogWs = await FlogWebSocket.wrap(
  () async {
    final socket = await WebSocket.connect(url, headers: {'Authorization': 'Bearer $token'});
    return IOWebSocketChannel(socket);
  },
  url: url,
);
```

---

## 改动范围

### flog_dart 库内

| 文件 | 改动 |
|------|------|
| `lib/src/flog_web_socket.dart` | 删同步构造器；加 `connect` 静态方法；加 `wrap` 静态方法；提取 `_fromConnected` 私有工厂 |
| `lib/flog_dart.dart` | 删 `nextNetId`、`emitNet`、`flogEnabled` 的 back-compat export |

### TUI / flog Rust 侧

**无需改动。** `'t':'err'` 帧格式已有，network 面板直接显示。

---

## 验收标准

| # | 场景 | 期望 |
|---|------|------|
| 1 | `connect(uri)` 握手成功 | network 面板出现 open 条目；send/recv/close 均可见 |
| 2 | `connect(uri)` 握手失败（DNS/TLS/4xx/refused） | network 面板出现 err 条目，含 url、error message、duration；原异常正常向上抛 |
| 3 | `wrap(() => ..., url: ...)` 握手失败 | 同上 |
| 4 | `fromChannel` 行为不变 | 现有测试全部通过，无回归 |
| 5 | `nextNetId`/`emitNet` 不再公开 | `import 'package:flog_dart/flog_dart.dart'` 后访问这两个符号编译报错 |
| 6 | 旧同步构造器已删 | 调用 `FlogWebSocket(uri)` 编译报错 |
| 7 | HTTP/SSE 无回归 | 现有 HTTP/SSE 测试全部通过 |
