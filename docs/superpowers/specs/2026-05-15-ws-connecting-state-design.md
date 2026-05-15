# WebSocket "Connecting" 状态

**日期:** 2026-05-15  
**状态:** 待实现

---

## 背景与问题

WebSocket 连接当前的帧生命周期是：

```
(什么都没有) → open  (握手成功)
(什么都没有) → err   (握手失败)
```

握手期间 TUI 网络面板里没有任何条目，用户感知不到正在建连。HTTP 有 `req` 帧在 `onRequest` 立即 emit，TUI 立刻显示 `Pending` 状态；WS 缺少等价物。

## 目标

握手开始时立即在 TUI 里显示一条 WS 条目（Pending 状态），然后：
- 握手成功 → 更新为 Active（`open` 帧已有）
- 握手失败 → 更新为 Failed（`err` 帧已有）

`fromChannel` 构造器（服务端 HTTP upgrade，channel 已建立）不需要 `connecting` 帧，直接 emit `open`，行为不变。

---

## 设计

### 新增帧：`t: "connecting"`

字段与 `open` 完全一致，新增原因是语义不同：`connecting` 表示握手尚未完成，`open` 表示握手已成功。

```json
{ "type": "net", "t": "connecting", "p": "ws", "id": 42, "url": "wss://host/ws", "ts": 1715000000000 }
```

`fromChannel` 只 emit `open`，不 emit `connecting`。

### Dart 侧（`flog_web_socket.dart`）

在 `_connectAndWrap` 的 `id`/`start` 赋值之后、任何 `await` 之前，插入：

```dart
if (flogEnabled) {
  emitNet({'id': id, 't': 'connecting', 'p': 'ws', 'url': url});
}
```

完整 `_connectAndWrap` 帧顺序变为：

```
connecting  ← 新增，立即（进入方法时）
  await connect() + await channel.ready
    ├─ 成功 → open     (via _initFromChannel)
    └─ 失败 → err
```

### Rust 侧

#### `src/domain/network.rs`

1. `FlogNetKind` 加 `Connecting` variant（字段与 `Open` 一致）：

```rust
/// WebSocket 握手开始（尚未完成）。TUI 立即显示 Pending 条目。
/// 后续由 `open`（成功）或 `err`（失败）帧更新状态。
Connecting {
    id: u64,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    ts: Option<u64>,
},
```

2. `FlogNetKind::id()` 加 `Self::Connecting { id, .. } => *id` arm。

#### `src/domain/network_store.rs`

1. `process_message` 加分支：

```rust
FlogNetKind::Connecting { id, url, ts } => self.handle_connecting(id, url, ts),
```

2. 新增 `handle_connecting`：创建 `NetworkStatus::Pending` 的 WS 条目（与 `handle_open` 相同，但 status 为 Pending）：

```rust
fn handle_connecting(&mut self, id: u64, url: Option<String>, ts: Option<u64>) {
    self.ensure_capacity();
    let url = url.unwrap_or_default();
    let mut entry = NetworkEntry::new_ws(id, url, String::new());
    entry.status = NetworkStatus::Pending;
    if let Some(t) = ts {
        entry.timestamp = format_ts(t);
    }
    self.entries.push_back(entry);
}
```

3. 修改 `handle_open`：先查找已有条目（由 `connecting` 帧创建），存在则更新；不存在则 push 新条目（向后兼容无 `connecting` 帧的旧版本）：

```rust
fn handle_open(&mut self, id: u64, url: Option<String>, ts: Option<u64>) {
    if let Some(entry) = self.find_by_id_mut(id) {
        // connecting 帧已创建条目，升级为 Active
        entry.status = NetworkStatus::Active;
        if let Some(u) = url {
            if !u.is_empty() {
                entry.url = u;
            }
        }
        if let Some(t) = ts {
            entry.timestamp = format_ts(t);
        }
    } else {
        // 向后兼容：无 connecting 帧（fromChannel 或旧版 Dart）
        self.ensure_capacity();
        let url = url.unwrap_or_default();
        let mut entry = NetworkEntry::new_ws(id, url, String::new());
        if let Some(t) = ts {
            entry.timestamp = format_ts(t);
        }
        self.entries.push_back(entry);
    }
}
```

#### `src/ui/` — 无需改动

`NetworkStatus::Pending` 已有完整显示逻辑：表格显示 `"..."` + OVERLAY0 灰色，详情面板显示 `"Pending"`。

---

## 改动范围

| 文件 | 改动 |
|------|------|
| `flog_dart/lib/src/flog_web_socket.dart` | `_connectAndWrap` 加一次 `connecting` emit |
| `src/domain/network.rs` | `FlogNetKind` 加 `Connecting` variant；`id()` 加 arm |
| `src/domain/network_store.rs` | 加 `handle_connecting`；修改 `handle_open` 支持更新已有条目 |
| `src/domain/network_tests.rs`（若存在）| 新增 `handle_connecting` 单元测试；`handle_open` 测试覆盖更新路径 |
| `flog_dart/test/flog_web_socket_connect_test.dart` | 新增 `connecting` 帧先于 `open`/`err` 的断言 |

**不需要改动：** `src/ui/network/` 所有文件（`Pending` 状态已有完整 UI）。

---

## 验收标准

| # | 场景 | 期望 |
|---|------|------|
| 1 | TUI 已连接，`FlogWebSocket.wrap(factory, url:)` 被调用 | TUI 立即出现 WS 条目（Pending/`...`） |
| 2 | 握手成功 | 条目更新为 Active |
| 3 | 握手失败（DNS / TLS / refused） | 条目更新为 Failed，含 error + duration |
| 4 | `FlogWebSocket.fromChannel(channel, url:)` | 直接出现 Active 条目，无 Pending 过渡（行为不变）|
| 5 | 旧版 Dart 无 `connecting` 帧 | `handle_open` 仍正常创建 Active 条目（向后兼容）|
| 6 | 所有现有测试 | 全部通过，无回归 |
