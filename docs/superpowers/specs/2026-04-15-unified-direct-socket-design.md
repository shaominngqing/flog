# Unified Direct Socket Architecture Design

## Goal

flog_dart 在 App 内启动 WS Server（:9753），flog TUI 作为 WS Client 主动发现并连接设备。所有平台（macOS、iOS 模拟器、Android、iOS 真机）统一模型，传输层差异全部在 flog（Rust）端解决。

## Architecture

```
Flutter App (flog_dart)                    flog TUI (Rust)
┌──────────────────────┐                  ┌──────────────────────┐
│  FlogServer           │                  │  DeviceConnector      │
│  WS Server :9753      │◄────────────────│  WS Client            │
│  - 推送 Log/Net 事件  │────────────────►│  - 接收 Log/Net       │
│  - 接收 Mock/Replay   │                  │  - 下发 Mock/Replay   │
└──────────────────────┘                  └──────────────────────┘
```

## Protocol

**不变。** SP1 定义的消息格式完全保留：

上行（Dart → flog）：`hello`, `log`, `net`
下行（flog → Dart）：`mock_sync`, `replay`

唯一区别：上行/下行的传输方向不变，但 TCP 连接的发起方反转了（flog 是 TCP client，App 是 TCP server）。

## 平台传输矩阵

| 平台 | flog 怎么连到设备的 9753 | 自动 |
|------|------------------------|------|
| macOS | `ws://localhost:9753` | 是 |
| iOS 模拟器 | `ws://localhost:9753` | 是 |
| Android 模拟器 | `adb forward tcp:{local} tcp:9753` → `ws://localhost:{local}` | 是 |
| Android 真机 | `adb forward tcp:{local} tcp:9753 -s {serial}` → `ws://localhost:{local}` | 是 |
| iOS 真机 | usbmuxd Connect(device_id, 9753) → TCP socket → WS upgrade | 是 |

**全部零配置。** 用户只需要 `flog` 一个命令。

## Dart 端：FlogServer

### `lib/src/flog_server.dart`（新，替代 flog_client.dart）

```dart
class FlogServer {
  static final FlogServer instance = FlogServer._();
  
  HttpServer? _httpServer;
  WebSocketChannel? _channel;
  bool _connected = false;
  Dio? _dio;

  void start({int port = 9753, Dio? dio});
  void send(Map<String, dynamic> data);
  void _onMessage(String json);  // mock_sync, replay
}
```

生命周期：
1. `FlogDio()` 构造时调用 `FlogServer.instance.start(dio: _inner)`
2. FlogServer 绑定 `0.0.0.0:9753`，等待 WebSocket 连接
3. flog TUI 连入 → FlogServer 发送 `hello`
4. 之后所有 `emitNet()` / `FlogLogger._log()` 通过 `FlogServer.instance.send()` 发送
5. 收到 `mock_sync` → `FlogMockInterceptor.updateRules()`
6. 收到 `replay` → `_dio.request(...)`
7. flog TUI 断开 → 等待重连（server 继续监听）

与旧 FlogClient 的区别：
- FlogClient 是 WS client，轮询重连
- FlogServer 是 WS server，被动等连接
- **更简单** — 不需要重连逻辑，server 一直在监听

### 其他 Dart 文件改动

| 文件 | 改动 |
|------|------|
| `flog_net.dart` | `FlogClient.instance.send()` → `FlogServer.instance.send()` |
| `flog_logger.dart` | 同上，export FlogServer 替代 FlogClient |
| `flog_dio.dart` | `FlogClient.instance.start()` → `FlogServer.instance.start()` |
| `flog_client.dart` | 删除，替换为 `flog_server.dart` |
| 其他文件 | 不变 |

## Rust 端：DeviceConnector

### `src/input/connector.rs`（新，替代 server.rs）

flog TUI 主动发现设备并连接。

```rust
pub struct DeviceConnector {
    event_rx: mpsc::UnboundedReceiver<ConnectorEvent>,
    handle: ConnectorHandle,
}

pub enum ConnectorEvent {
    Connected(DeviceInfo),
    Disconnected(DeviceId),
    Message(DeviceId, ClientMessage),
}

pub struct ConnectorHandle {
    // 发送下行消息给连接的设备
}
```

### 设备发现：`src/transport/device_monitor.rs`

后台轮询 `flutter devices --machine`（每 5 秒）：

```rust
struct FlutterDevice {
    name: String,
    id: String,
    platform: String,  // android-arm64, ios, darwin, web-javascript
    emulator: bool,
}
```

发现设备后根据平台类型选择连接方式。

### Android 连接：`src/transport/adb.rs`

```rust
// 为 Android 设备建立 adb forward 并连接
async fn connect_android(serial: &str, port: u16) -> Result<WebSocket> {
    // 1. adb -s {serial} forward tcp:{local_port} tcp:{port}
    // 2. 连接 ws://localhost:{local_port}
    // 3. 返回 WebSocket 连接
}
```

每个 Android 设备分配一个本地端口（避免多设备冲突）。

### iOS 真机连接：`src/transport/usbmuxd.rs`

```rust
// 通过 usbmuxd 连接 iOS 设备端口
async fn connect_ios(device_id: u32, port: u16) -> Result<TcpStream> {
    // 1. 连接 /var/run/usbmuxd Unix socket
    // 2. 发送 Connect(device_id, port) plist 请求
    // 3. 收到成功响应后，socket 变成直通管道
    // 4. 返回 TcpStream
}

// 通过 usbmuxd 列出设备获取 UDID → DeviceID 映射
async fn list_usb_devices() -> Result<Vec<UsbDevice>> {
    // 发送 ListDevices 请求，解析响应
}
```

在 usbmuxd TCP stream 上做 WebSocket upgrade → 得到 WS 连接。

### localhost 连接（macOS / iOS 模拟器）

直接 `tokio_tungstenite::connect_async("ws://localhost:9753")`。

### 连接管理

```rust
// 统一的连接流程
async fn connect_device(device: &FlutterDevice, port: u16) -> Result<WebSocket> {
    match device.platform.as_str() {
        p if p.starts_with("android") => connect_android(&device.id, port).await,
        "ios" if !device.emulator => connect_ios_usb(&device.id, port).await,
        _ => connect_localhost(port).await,  // macOS, iOS sim
    }
}
```

每个连接的设备对应一个后台任务，处理消息收发。断连后通过 device_monitor 的下一轮轮询重新发现并连接。

### Rust 文件变化

| 文件 | 变化 |
|------|------|
| `src/input/server.rs` | 删除 |
| `src/input/connector.rs` | 新建 — WS client + 连接管理 |
| `src/input/protocol.rs` | 不变 |
| `src/input/mod.rs` | 改 export |
| `src/transport/mod.rs` | 新建 — 传输层模块 |
| `src/transport/device_monitor.rs` | 新建 — flutter devices 轮询 |
| `src/transport/adb.rs` | 新建 — adb forward |
| `src/transport/usbmuxd.rs` | 新建 — usbmuxd 协议 |
| `src/main.rs` | 改 — 启动 connector 而非 server |
| `src/app.rs` | 小改 — server_handle → connector_handle |
| `src/event.rs` | 小改 — 同上 |
| `src/lib.rs` | 小改 — 加 transport 模块 |

### CLI

```
flog [--port PORT] [--level LEVEL] [--tag TAG]
```

`--port` 含义变了：不再是 server 监听端口，而是要连接的设备端口（默认 9753）。

## 启动顺序

| 顺序 | 行为 |
|------|------|
| flog 先启动 | device_monitor 轮询，发现设备后连接 |
| App 先启动 | FlogServer 监听 9753，等 flog 连入 |
| flog 重启 | App server 还在，flog 重新发现并连入 |
| App 热重载 | FlogServer 是 singleton，server 保持 |
| App 热重启 | FlogServer 重建，flog 检测到断连，下轮重连 |

## Edge Cases

1. **flutter 不在 PATH** — 回退到只尝试 localhost:9753（开发者本机直连场景）
2. **adb 不在 PATH** — 跳过 Android 设备连接
3. **多设备** — 每个设备独立连接，每个有自己的 WS 连接和后台任务
4. **设备拔掉** — WS 断连，connector 清理。下轮 device_monitor 不再发现该设备
5. **端口 9753 被占用（App 端）** — FlogServer.start() 失败，日志提示。flog_dart 功能不可用但 App 正常运行
6. **usbmuxd socket 不存在** — 跳过 iOS 真机连接
7. **adb forward 端口冲突** — 每个 Android 设备分配不同的本地端口（9753 + device_index）
8. **release 构建** — flogEnabled = false，FlogServer.start() 不执行，端口不占用

## 实施计划

由于 SP1 和 SP2 的 server/client 角色需要互换，实施分两步：

**Phase 1：角色互换**
- Dart：FlogClient → FlogServer（WS server）
- Rust：FlogServer → DeviceConnector（WS client）
- 先只支持 localhost 连接（macOS / iOS 模拟器）

**Phase 2：传输层**
- device_monitor（flutter devices 轮询）
- adb forward（Android）
- usbmuxd（iOS 真机）

Phase 1 完成后就可以端到端测试。Phase 2 增加平台支持。
