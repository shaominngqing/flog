# Spec: 断连噪声治理与 LIVE 语义修正

- **日期**：2026-04-29
- **范围**：flog TUI 主仓；不涉及 `flog_dart` 协议或行为
- **触发**：用户反馈"关闭应用或拔出设备后 flog 变得不可控"（两张截图 2026-04-29）
- **排除**：flog_dart v0.9 worktree 里 `FlogVmService.attach` 重试风暴（另留给 worktree 分支 smoke）；Dart FlogStore 跨 hot restart 的日志累积（属于 Dart 侧行为）

## 1. 问题陈述

### 1.1 症状

用户场景：Flutter 应用正常连接 flog，然后**关闭应用**或**拔出 USB 设备**。观察到：

1. **屏幕底部出现两行错误文字**："`connector reader task exiting: read error: WebSocket protocol error: Connection reset without closing handshake`"，堆叠显示，ratatui 不会重绘覆盖——即使用户切换 tab、滚动日志，这两行仍然粘在原位。
2. **LIVE 指示灯仍为绿色**：连接已断开，但状态栏左下角的 `LIVE` 一直亮着，让用户误以为仍在接收数据。
3. **没有任何"断开"提示**：用户无从感知连接状态的变化，只能从"日志不再流动"间接推断。

### 1.2 根因

**症状 1 — stderr 穿透 alternate screen**：

- `src/main.rs:73` 调用 `EnterAlternateScreen`，只重定向 **stdout**。
- `src/input/connector.rs:205 / 229 / 233` 的 writer/reader task 退出时用 `eprintln!`（走 **stderr**）记录原因（TRANS-006 当年的设计决策）。
- macOS Terminal / Alacritty 等都不会把 stderr 的输出路由到 alternate screen buffer——它直接写到当前可见的屏幕，且**不会被 ratatui 重绘覆盖**。
- 每次断开 reader + writer 两个 task 分别 `eprintln!`，造成两行堆叠。

**症状 2 — LIVE 语义错位**：

- 按 `docs/ARCHITECTURE.md §13`，`auto_scroll` 表达"用户是否在看 tail"，和连接状态解耦。
- UI 现状把 `auto_scroll` 直接渲染为 `LIVE`，连接断开后只要用户没上滚，`LIVE` 就继续亮。
- 这不是 bug，是**语义信息量不足**——单一指示承担了多重含义。

**症状 3 — 缺感知**：

- `connection_task` 收到 `Disconnected` 事件时，`run/server.rs:244` 只调用了 `show_status("Disconnected: <device>")`，toast 2 秒即消失。
- 若 toast 消失后用户才回头看屏幕，状态栏看不到任何异常——只有"卡在那里的 stderr 两行"（正是症状 1）。

### 1.3 为什么要一起修

三个症状共同制造了"失控"的体验：stderr 污染让用户**无法忽略**，LIVE 语义错位让用户**误判**，缺感知让用户**不确定**。任何单独修一个都治不好——修 stderr 但保留 LIVE 错位，用户还是会觉得"连接看起来是活的但没数据"；修 LIVE 但 stderr 还在，画面仍然脏。

## 2. 设计

### 2.1 连接生命周期状态机（核心模型）

引入**连接状态**作为从 `App` 纯函数导出的值（符合 CLAUDE.md 设计规则 #4：新状态应能由纯函数导出），不作为独立字段存储：

```
ConnectionState = fn(&App) -> enum {
    Live,         // active_app_id.is_some() && 对应 ConnectedApp 存在
    Reconnecting, // active_app_id.is_some() 但对应 ConnectedApp 不存在，且 connection_task 仍在活跃重试
    Offline,      // active_app_id.is_none() || 无任何 connection_task 在重试
}
```

**关键点**：`Reconnecting` 和 `Offline` 的区分需要知道 "connection_task 是否还在重试"。目前这个信息**不存在** App 里——connection_task 只在 `Connected` 成功时才 `add_connected_app`，retry loop 内部状态不外露。

**解法**：在 `add_connected_app` 后**不**清除 ConnectedApp，而是在 `Disconnected` 后**保留** ConnectedApp 并标记一个 `connection_status: ConnStatus` 字段：

```rust
pub enum ConnStatus {
    Live,          // 当前有活跃 WS 会话
    Reconnecting,  // WS 断开，connection_task retry loop 在跑
}

pub struct ConnectedApp {
    // ... 原有字段
    pub connection_status: ConnStatus,
}
```

`remove_connected_app` 语义变化：

- **旧**：设备 `Removed` 或 `Disconnected` 都调用它，直接从 `connected_apps` 移除。
- **新**：只在**设备 Removed**时真正移除；`Disconnected` 改为把 `connection_status = Reconnecting`。

这样 `App` 始终知道"这个 app 槽位还期望重连吗"，UI 也有足够信息渲染三态 LIVE/RECONNECTING/OFFLINE。

### 2.2 P1：connector 退出路径

**改动范围**：`src/input/connector.rs`、`src/run/server.rs`。

#### 2.2.1 ConnectorEvent 扩展

```rust
pub enum ConnectorEvent {
    Connected(ClientInfo),
    Disconnected { reason: DisconnectReason },  // 原无参数
    Message(ClientMessage),
}

pub enum DisconnectReason {
    PeerClosed,                    // reader 收到 Close 帧
    ReadError(String),             // reader 返回 Err
    WriteError(String),            // writer send 失败
    WriterChannelClosed,           // 所有 ConnectorHandle 被 drop
}
```

`DisconnectReason` 是 enum 而不是 `String`，让上层可以做结构化处理（例如 `PeerClosed` 是正常关闭，`ReadError` 才值得在状态栏 toast）。

#### 2.2.2 删除 eprintln

`connector.rs` 里三处 `eprintln!` 全部删除。reader/writer task 退出前把原因塞进 `DisconnectReason` 从 `event_tx` 送出。

#### 2.2.3 writer 先挂也能触发整条连接 teardown

现状：writer task 遇到 send 错误 break 掉，但 reader task 还挂在 `ws_read.next()` 上，只有当对端真正关闭才会走到 Disconnected 分支。这意味着 writer 失败时**整条连接状态"悬浮"**——UI 还显示 Live，但 mock_sync / replay 下发会 silently drop。

**改法**：writer 错误退出时，通过 `event_tx` 主动发一次 `Disconnected { reason: WriteError(..) }`，并通过 drop `ws_sink` 让 `ws_read` 的底层 stream 关闭（WebSocketStream 的 split 后两端共享底层 socket，drop sink 会关 socket，reader 随即解阻塞）。

这一点今天 writer 的 eprintln 后直接结束，并未通知 reader——**一并修正**。

#### 2.2.4 run/server.rs 消费 DisconnectReason

```rust
ConnectorEvent::Disconnected { reason } => {
    // 旧：a.remove_connected_app(&task_key_c);
    // 新：标记 reconnecting，而不是移除
    a.mark_app_reconnecting(&task_key_c);

    let msg = match reason {
        DisconnectReason::PeerClosed => format!("{} disconnected", device.name),
        DisconnectReason::ReadError(e) | DisconnectReason::WriteError(e) =>
            format!("{} connection lost: {}", device.name, short_reason(&e)),
        DisconnectReason::WriterChannelClosed =>
            format!("{} connection lost", device.name),
    };
    a.show_status(msg);
    // ...adb forward 清理...
    break;  // 跳出 event_rx.recv() 循环，进入 retry 分支
}
```

`short_reason(&str) -> String`：把底层错误文本压缩到 ≤ 40 字符，避免状态栏溢出。例如 `"WebSocket protocol error: Connection reset without closing handshake"` → `"connection reset"`。

### 2.3 P2：设备 Removed 的处理

`transport::DeviceEvent::Removed(id)` 到达时（`run/server.rs:100`），设备真的走了——此时**调用旧的** `remove_connected_app`，真正从 `connected_apps` 移除所有该设备的 ConnectedApp 条目，并清理 adb forward。

这和 2.1 的状态机一致：
- **短暂 WS 断开** → `Reconnecting`（ConnectedApp 保留）
- **设备拔出/adb kill-server** → `Offline`（ConnectedApp 移除）

### 2.4 App 侧新增接口

`src/app/multi_app.rs`：

```rust
impl App {
    /// WS 断开但 connection_task 仍在重试。不从 connected_apps 移除，
    /// 只翻转 connection_status。
    pub fn mark_app_reconnecting(&mut self, id: &str) {
        if let Some(app) = self.connected_apps.iter_mut().find(|a| a.id == id) {
            app.connection_status = ConnStatus::Reconnecting;
        }
    }

    /// add_connected_app 时：如果 id 已存在且 status 是 Reconnecting，
    /// 这是同一会话重连，保留 active_app_id、不 reset_session。
    /// 若 id 不存在，沿用现有首次连接逻辑。
    pub fn add_connected_app(&mut self, info: ConnectedApp) {
        // 见 §2.5
    }
}
```

### 2.5 add_connected_app 的新语义

| 进入条件 | 行为 |
|---|---|
| 相同 id 存在且 `connection_status == Reconnecting` | 替换条目，标记 `Live`。**不 reset_session**。Dart 自动 replay 的前半段和本地 store 重合——接受这种重复（Dart 的 FlogStore 行为，本次 spec 范围外） |
| 相同 id 存在且 `connection_status == Live` | 异常：不应发生（只会从 Reconnecting 过渡到 Live）。defensive：替换条目，reset_session |
| id 不存在 && `connected_apps.is_empty()` | 首次连接，激活 + reset_session |
| id 不存在 && 已有其他 active app | 新连接加入但不抢占，user 手动切换 |

注意"日志重复"在 2.5 的第一档里**不处理**——已和用户确认这是 Dart 侧行为，不在本次 spec 范围。本次改动保证**重连不再错误地"首次连接"**即可。

### 2.6 P4：LIVE 三态渲染

**衍生状态**（纯函数，不存）：

```rust
pub enum LiveState {
    Live,          // 有 active ConnectedApp 且 connection_status=Live 且 auto_scroll
    LivePaused,    // 有 active ConnectedApp 且 connection_status=Live 但 !auto_scroll
    Reconnecting,  // 有 active ConnectedApp 但 connection_status=Reconnecting
    Offline,       // 无 active ConnectedApp（active_app_id=None 或对应条目已从 connected_apps 移除）
}

impl App {
    pub fn live_state_for(&self, tab: ViewTab) -> LiveState { ... }
}
```

为什么按 tab 区分：`auto_scroll` 在 Logs 和 Network 两个 tab 上是独立的字段（`LogsViewState` vs `NetworkState`）。状态栏在哪个 tab 就用哪个 tab 的 `auto_scroll`。

**视觉映射**（Catppuccin Macchiato palette）：

| State | 色块底 | 文字 | 文字色 |
|---|---|---|---|
| Live | `GREEN` (#a6da95) | `LIVE` | `BASE` (#24273a) |
| LivePaused | `OVERLAY0` (#6e738d) | `PAUSED` | `MANTLE` |
| Reconnecting | `YELLOW` (#eed49f) | `RECONNECTING` | `BASE` |
| Offline | `SURFACE0` (#363a4f) | `OFFLINE` | `SUBTEXT0` (#a5adcb) |

`PAUSED` 显式区分"连接正常但用户在看历史"vs"没连接"，解决 LIVE 语义的根本问题。

**状态转换触发**（谁让 LiveState 变化）：

- `Live ↔ LivePaused`：用户在对应 tab 内滚动 / go_bottom，翻转 `auto_scroll`
- `Live → Reconnecting`：`connection_task` 收到 `Disconnected`，调 `mark_app_reconnecting`
- `Reconnecting → Live`：`connection_task` 重连成功，调 `add_connected_app`（走新的"同 id 重连"分支）
- `Reconnecting → Offline`：**仅在设备真正 Removed 时**（DeviceEvent::Removed），`connected_apps` 被清空且 `active_app_id=None`
- `Offline → Live`：新设备被发现、首次连接成功
- `Offline → Reconnecting`：**不会发生**（必须先 Live 才能 Reconnecting）

`connection_task` 今天的实现**永不放弃重试**（2s→30s 指数退避后稳定在 30s）。所以只要设备仍在 `discovered_devices` 里，`Reconnecting` 就是持久状态；只有设备拔出才过渡到 `Offline`。这是期望行为——设备还在就继续试。

**新增辅助信息**（不阻塞本 spec 的可选增强）：LiveState 的 chip 后面跟一行小字展示 source_name 已经够用，不加额外字段。

### 2.7 视觉布局

状态栏第一格（最左）保留 LiveState chip（宽度自适应文本：LIVE=6，PAUSED=8，RECONNECTING=14，OFFLINE=9），其余格位不变。`ui/logs/status_bar.rs` 和 `ui/network/status_bar.rs` 共用一个新函数 `live_state_chip(state: LiveState) -> Span`，放到 `ui/mod.rs` 里。

## 3. 实现单元切片

| # | 单元 | 范围 | 文件 |
|---|---|---|---|
| S1 | `DisconnectReason` enum + `ConnectorEvent::Disconnected` 结构化 | input/ | `input/connector.rs`, 删除 3 处 eprintln |
| S2 | writer 挂掉也能 teardown reader | input/ | `input/connector.rs`（drop ws_sink 让 ws_read 解阻塞） |
| S3 | `ConnStatus` + `ConnectedApp::connection_status` 字段 | app/ | `app/multi_app.rs` |
| S4 | `mark_app_reconnecting` + `add_connected_app` 的"同 id 重连保留"分支 | app/ | `app/multi_app.rs` |
| S5 | `connection_task` 消费 `DisconnectReason`，区分 PeerClosed / ReadError / WriteError / WriterChannelClosed 的 status 文案；调用 mark_app_reconnecting 代替 remove | run/ | `run/server.rs` |
| S6 | 设备 Removed 走真正的 remove_connected_app | run/ | `run/server.rs`（原代码已是如此，确认无 regression） |
| S7 | `App::live_state_for(tab)` + `LiveState` enum | app/ | `app/mod.rs` 或新增 `app/live_state.rs` |
| S8 | `live_state_chip` 共享渲染 + 替换两个 status_bar.rs 的 LIVE 绘制 | ui/ | `ui/mod.rs`, `ui/logs/status_bar.rs`, `ui/network/status_bar.rs` |
| S9 | 集成测试：端到端 reconnect 场景断言无 eprintln 发生、状态转换正确 | tests/ | 新增或扩 `tests/reconnect_test.rs` |

每单元 ≤ 80 行增量（估计），独立可 test。

## 4. 测试策略

### 4.1 单元测试

- `connector.rs`：writer 失败能触发 Disconnected；writer 失败后 reader 不再阻塞
- `multi_app.rs`：
  - Reconnecting 状态下同 id 重连 → 不触发 reset_session（用 store.len() 或 bookmark 存在性断言）
  - Device Removed 清 ConnectedApp 以及其 adb forward 记录
  - `live_state_for` 四态覆盖（包括 active_app_id=None 但 connected_apps 非空的过渡态——走向 Offline）
- `app/live_state.rs`（若新建）：纯函数 table-driven 测试

### 4.2 集成测试

`tests/reconnect_test.rs`（新增）：启一个 mock WebSocket server，模拟三种场景：

1. Peer close → TUI 应 status_bar toast "disconnected"；状态机从 Live → Reconnecting
2. 设备 Removed → TUI 应从 Live → Offline（不经过 Reconnecting）
3. WS 断开再连 → 状态 Reconnecting → Live，store 保留，不触发 reset

### 4.3 回归断言

- `cargo test` 全绿
- 手动 smoke：用 aura-lang-flutter 连一次，关闭 app，观察屏幕底部**无 eprintln 残留**、状态栏显示 RECONNECTING 2s 内→ OFFLINE（如果 connection_task 真的放弃）或保持 RECONNECTING（retry loop 继续）

## 5. 不做的事

- 不引入 `tracing` 依赖（最小增量原则，当前 `eprintln` 调用点只有 3 个）
- 不新增环形日志或 debug 面板存储断连历史（已和用户确认"仅 toast，不留历史"）
- 不修 Dart 侧 FlogStore 跨 hot restart 的累积行为（Dart 侧问题）
- 不处理 flog_dart v0.9 worktree 的 FlogVmService attach 风暴（留给 worktree 分支）
- 不升级 `show_status` 的呈现（当前 2 秒 toast + 单值字段够用）

## 6. 风险

| 风险 | 缓解 |
|---|---|
| `Reconnecting` 状态下 `connected_apps` 不再是"活跃连接列表"的字面含义，触达该字段的其他代码可能误判 | 审计所有 `connected_apps` 读取点（估计 < 10 个）；必要处用 `connection_status == Live` 过滤 |
| 设备真正拔出时用户短暂看到 `RECONNECTING` 再变 `OFFLINE`（因 DeviceEvent::Removed 可能晚于 WS disconnect 到达） | 可接受——这就是事实：WS 断开时 TUI 还不知道设备是走了还是只是抖动；`adb track-devices` 的 Removed 本来就滞后 | 
| `drop(ws_sink)` 关闭底层 socket 的行为在 tokio-tungstenite 语义上需验证 | 查 crate 文档 + 写测试（S2 的 unit test） |

## 7. 参考

- 触发截图：2026-04-29 用户消息附件
- CLAUDE.md 设计规则 #4（状态应由纯函数导出）
- TRANS-006（连接退出日志决策，本次 spec 推翻其 `eprintln!` 实现但保留"让退出原因可见"的精神）
- TRANS-014（`session_id` 字段预留，本次不使用但相关）
- `docs/ARCHITECTURE.md §13`（auto_scroll 语义）
- `docs/ARCHITECTURE.md §15`（错误处理哲学）
