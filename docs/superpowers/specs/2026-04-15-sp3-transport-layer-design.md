# SP3: Transport Layer — adb reverse + usbmuxd

## Goal

flog 启动后自动检测已连接的设备，为 Android 设备执行 `adb reverse` 端口转发，为 iOS 真机通过 usbmuxd 协议建立 USB 端口转发。用户无需任何手动操作。

## Context

SP1 建了 Rust WS Server（localhost:9753），SP2 建了 Dart FlogClient（连接 localhost:9753）。当前状态：

- macOS / iOS 模拟器：localhost 直通，已可用
- Android：需要 `adb reverse` 让设备上的 localhost 到达 Mac
- iOS 真机：需要 usbmuxd 端口转发让设备上的 localhost 到达 Mac

## 设备发现

使用 `flutter devices --machine` 检测设备：

```json
[
  {"name": "23127PN0CC", "id": "1e0e87b2", "targetPlatform": "android-arm64", "emulator": false},
  {"name": "iPhone 15 Pro", "id": "00008120-xxxx", "targetPlatform": "ios", "emulator": false},
  {"name": "macOS", "id": "macos", "targetPlatform": "darwin", "emulator": false}
]
```

flog 后台每 5 秒轮询一次，根据 `targetPlatform` 和 `emulator` 决定行为：

| targetPlatform | emulator | 行为 |
|----------------|----------|------|
| `android-*` | any | `adb reverse tcp:9753 tcp:9753 -s {id}` |
| `ios` | false | usbmuxd 端口转发 {id}:9753 → localhost:9753 |
| `ios` | true | 无需操作（iOS 模拟器共享 Mac 网络） |
| `darwin` / `web-*` | any | 无需操作 |

## Android: adb reverse

简单的子进程调用：

```rust
Command::new("adb")
    .args(["-s", &device_id, "reverse", "tcp:9753", "tcp:9753"])
    .output()
```

- 每次发现 Android 设备时执行（幂等，重复执行无害）
- 设备消失时执行 `adb -s {id} reverse --remove tcp:9753` 清理
- 如果 `adb` 不在 PATH 中，静默跳过（用户可能只用 iOS）

## iOS 真机: usbmuxd

### 协议概述

usbmuxd 是 macOS 内置的 USB 多路复用守护进程。通过 Unix socket `/var/run/usbmuxd` 通信。

协议格式：每条消息 = 16 字节 header + plist XML body

Header:
```
u32 length     (整个消息包含 header)
u32 version    (1)
u32 type       (8 = plist)
u32 tag        (请求标识，响应会回传)
```

### 需要实现的命令

**1. Connect** — 连接到设备的指定端口

请求：
```xml
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>MessageType</key>
    <string>Connect</string>
    <key>DeviceID</key>
    <integer>{device_id_number}</integer>
    <key>PortNumber</key>
    <integer>{port_in_network_byte_order}</integer>
</dict>
</plist>
```

成功响应：socket 变成和设备端口的直通管道。

**注意：** `flutter devices` 返回的 id 是 UDID 字符串，但 usbmuxd Connect 需要的是数字 DeviceID。需要先通过 Listen 或 ListDevices 获取 UDID → DeviceID 的映射。

**2. Listen** — 监听设备连接/断开事件

请求：
```xml
<dict>
    <key>MessageType</key>
    <string>Listen</string>
</dict>
```

响应流：持续推送 Attached/Detached 事件，包含 DeviceID 和 SerialNumber(UDID)。

### 转发流程

1. 连接 `/var/run/usbmuxd`
2. 发送 Listen，获取设备列表（DeviceID ↔ UDID 映射）
3. 当 `flutter devices` 发现 iOS 真机时，根据 UDID 查找 DeviceID
4. 新开一个 usbmuxd 连接，发送 Connect(DeviceID, 9753)
5. 连接成功后，这个 socket 就是到设备 9753 端口的管道
6. 启动一个本地 TCP listener（随机端口或固定端口），将流量中继到 usbmuxd socket
7. 实际上不需要额外 listener — flog_dart 在 iOS 上连接 localhost:9753，我们需要让 9753 到达设备

**等一下，方向反了。** 

flog server 在 Mac 上监听 9753。iOS App（flog_dart）要连 localhost:9753。在 iOS 真机上 localhost 是设备自己，不是 Mac。

所以需要的是：**让 iOS 设备上的 9753 端口转发到 Mac 上的 9753 端口。**

这正是 Flipper 用 `adb reverse` 做的事 — 但 iOS 没有 `adb reverse`。

usbmuxd 的 Connect 命令是 **Mac → 设备**，不是 **设备 → Mac**。它让 Mac 可以连接设备上的端口。

要实现设备 → Mac 的反向转发，需要：
1. 在 iOS App 里启动一个 TCP server 监听 9753
2. Mac 通过 usbmuxd Connect 连接到设备的 9753
3. 设备上的 server 接收到连接后，转发到 flog_dart 的 WebSocket 逻辑

**这太复杂了**，需要 Dart 端也改。

### 更好的方案

参考 Flipper 的做法 — Flipper 在 Mac 上启动 server，通过 usbmuxd 创建反向隧道：

**Flipper 的方式：**
1. Mac 上 Flipper server 监听 9088
2. iOS App 里 Flipper SDK 尝试连 localhost:9088
3. Flipper 用一个 **peertalk 辅助 App** 在设备上做端口转发

实际上 Flipper iOS 真机方案很 hacky — 它需要一个辅助 Mac App（PortForwardingMacApp）跑 peertalk。

### 务实方案

iOS 真机不做自动 USB 转发。改为：**flog_dart 支持通过 Wi-Fi 连接 Mac IP**。

```dart
// iOS 真机用法：指定 Mac 的局域网 IP
FlogDio(flogHost: '192.168.1.100');
```

这需要用户手动指定一次 IP，但：
- 零额外依赖
- 跨网络也能用
- flog_dart SP2 已经有 `flogHost` 参数

后续可以用 mDNS 自动发现来消除手动配置。

## 最终设计

| 平台 | 方式 | 自动 | 需要用户操作 |
|------|------|------|------------|
| macOS | localhost 直通 | 是 | 无 |
| iOS 模拟器 | localhost 直通 | 是 | 无 |
| Android 模拟器 | adb reverse | 是 | 无 |
| Android 真机 | adb reverse | 是 | 无 |
| iOS 真机 | Wi-Fi + flogHost | 否 | 指定 Mac IP |

## 实现

### 新文件

**`src/transport/mod.rs`** — 传输层模块

**`src/transport/device_monitor.rs`** — 设备监控

- 后台任务，每 5 秒执行 `flutter devices --machine`
- 解析 JSON，识别新设备/消失设备
- 对 Android 设备执行 adb reverse
- 清理消失设备的转发
- 将设备列表同步到 App state（UI 可以显示已连接设备）

**`src/transport/adb.rs`** — ADB 命令封装

- `adb_reverse(serial: &str, port: u16)` — 执行 adb reverse
- `adb_reverse_remove(serial: &str, port: u16)` — 清理
- `is_adb_available()` — 检查 adb 是否在 PATH 中

### 修改文件

**`src/main.rs`** — 启动 device_monitor 后台任务

**`src/app.rs`** — 添加 `devices: Vec<DeviceInfo>` 字段显示发现的设备

**`src/cli.rs`** — 可选：添加 `--no-forward` 禁用自动转发

### 不需要的

- usbmuxd 实现（iOS 真机走 Wi-Fi）
- 任何 Dart 端改动（flogHost 参数 SP2 已有）

## Edge Cases

1. **adb 不在 PATH** — 静默跳过 Android 转发，不报错
2. **flutter 不在 PATH** — 回退到不做设备发现，只等 client 连入
3. **多个 Android 设备** — 每个都执行 adb reverse（各自的 serial）
4. **adb reverse 失败** — 显示 status 提示，继续运行
5. **设备 USB 拔掉** — 下次轮询发现设备消失，清理转发
6. **flutter devices 很慢** — 超时 10 秒，超时就跳过本轮
