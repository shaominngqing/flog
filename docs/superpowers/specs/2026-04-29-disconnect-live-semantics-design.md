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
| S9 | 状态机 table-driven 测试 + 多步序列测试 | tests/ | 扩 `tests/characterization_app_state.rs` |
| S10 | UI 渲染 negative 断言补齐（Reconnecting / Offline / Paused 文案） | tests/ | 扩 `tests/characterization_ui_logs.rs` + `characterization_ui_network.rs` |
| S11 | `FakeFlogServer` test harness 扩展 + `tests/reconnect_test.rs` 端到端 | tests/ | `tests/support/fake_flog_server.rs`, 新增 `tests/reconnect_test.rs` |
| S12 | `tests/forbidden_patterns_test.rs` 新增类别 + `eprintln!` 规则 | tests/ | 新增 `tests/forbidden_patterns_test.rs` |

每单元 ≤ 80 行增量（估计），独立可 test。S12 必须在 S1（删除 eprintln）**之后** 才能通过，是最终的回归闸。

## 4. 测试策略

### 4.1 背景：为什么要补齐测试，不是单点添加

本次 bug 在覆盖率 ~90% 的代码库里漏掉，根因是**覆盖率 ≠ 路径覆盖 ≠ 负样本覆盖**。具体缺口：

| 缺口 | 证据 | 后果 |
|---|---|---|
| **断开事件端到端缺测** | `tests/characterization_input.rs` 有 24 处提到 `Disconnected` 但 **0 个 `#[test]`**（全是 fixture）；`tests/ws_server_test_direct.rs` 同样 0 个 `#[test]` | 真实 WS 断连的全路径（connector → server → App 状态）从未被自动化验证 |
| **UI negative 断言缺失** | `ui_010_status_bar_shows_live_pill_when_auto_scroll` 只验证 "auto_scroll=true → 显示 LIVE"；**从未**验证 "断开 + auto_scroll=true → LIVE 消失" | 语义错位的 bug 可以在所有正样本测试通过的情况下存在 |
| **多步状态转换无覆盖** | `app_state` 测试都是单步纯函数（"add 后状态 X" / "remove 后状态 Y"），没有"add→remove→add 后状态 Z"的连续序列 | `is_reconnect` 这种依赖"上一步状态"的判定错误无法被捕捉 |
| **输出通道污染无防线** | `tests/` 里 0 处 `eprintln` / `stderr` / `alternate_screen` 相关断言 | 任何走 stderr 的新代码默认合法，不存在"禁止引入"的测试闸 |

本次 spec 的测试章节不仅覆盖本次 bug，还**补齐这四类缺口的最小闭环**，让同类 bug（"状态机路径组合"、"屏幕污染"、"正反语义"）未来能被挡在 CI 前。

### 4.2 单元测试

#### 4.2.1 connector.rs

- `writer_failure_teardowns_reader` — 写失败后 reader 也必须解阻塞并发出 `Disconnected`（S2 的直接断言）
- `disconnect_reason_peer_close` / `disconnect_reason_read_error` / `disconnect_reason_writer_error` / `disconnect_reason_channel_closed` — 每个 `DisconnectReason` variant 的触发路径各一条
- `no_eprintln_on_disconnect` — 见 §4.5 的 forbidden-pattern 检查，不在此单元里重复

#### 4.2.2 multi_app.rs / live_state.rs

**关键新增**：table-driven 状态机测试。把 `LiveState` 四态和 ConnStatus + auto_scroll + active_app_id 的组合展开成表：

```rust
#[test]
fn live_state_matrix() {
    struct Case {
        name: &'static str,
        active_id: Option<&'static str>,
        connected: Vec<(&'static str, ConnStatus)>,
        auto_scroll: bool,
        expect: LiveState,
    }
    let cases = [
        Case { name: "live + tail", active_id: Some("a"),
               connected: vec![("a", ConnStatus::Live)],
               auto_scroll: true, expect: LiveState::Live },
        Case { name: "live + paused", /* ... */ expect: LiveState::LivePaused },
        Case { name: "reconnecting + tail", /* ... */ expect: LiveState::Reconnecting },
        Case { name: "reconnecting + paused", /* ... */ expect: LiveState::Reconnecting },
        Case { name: "offline empty", /* ... */ expect: LiveState::Offline },
        Case { name: "offline active_id dangles", /* ... */ expect: LiveState::Offline },
        // 共 ≥ 8 例，覆盖所有 {状态机} × {UI tab 输入} 组合
    ];
    for c in cases { /* assert */ }
}
```

原则：凡是**多维枚举组合**的纯函数，必须 table-driven 显式列举；不允许"代码行都跑到就算覆盖"。

**多步转换序列**：至少三条连续路径必须有测试：

1. `Connect → Disconnect → Reconnect` 后 store 保留、active_app_id 保留、ConnStatus 回到 Live
2. `Connect → Disconnect → DeviceRemoved` 后 connected_apps 清空、active_app_id=None、LiveState=Offline
3. `Connect(app1) → Connect(app2) → Disconnect(app1)` 后 active_app_id 不动（只要 app2 不是当前 active），app1 标记 Reconnecting

#### 4.2.3 LogsViewState / NetworkState tab-aware 快照

`live_state_for(tab)` 在 Logs / Network 两个 tab 各自的 auto_scroll 被修改时只读对应 tab 的字段——每 tab 各一条测试。

### 4.3 UI 渲染测试：补齐 negative 断言

在 `tests/characterization_ui_logs.rs` / `characterization_ui_network.rs` 里，**每个正样本断言都要有一个对应的负样本**：

| 现有正样本 | 新增负样本 |
|---|---|
| `auto_scroll=true → 包含 "LIVE"` | `auto_scroll=true + connection_status=Reconnecting → 包含 "RECONNECTING"，不包含 "LIVE"` |
| 同上 | `active_app_id=None → 包含 "OFFLINE"，不包含 "LIVE" 和 "RECONNECTING"` |
| `auto_scroll=false → 显示 new pill` | `auto_scroll=false + Live → 显示 "PAUSED"` |

这套断言直接使用现有 `render_logs(&mut app, 120, 30)` + `full_text(&buf).contains()` 的工具链，无新基建。

### 4.4 端到端集成测试（本 spec 新增基础设施）

`tests/support/fake_flog_server.rs` 已有部分 fixture。本次扩展成完整的 test harness：

```rust
// tests/support/fake_flog_server.rs 新增方法
impl FakeFlogServer {
    pub async fn start_on(port: u16) -> Self { ... }
    pub async fn accept_one(&mut self) -> FakeSession { ... }
    pub async fn send_hello(&mut self, client_info: &ClientInfo) { ... }
    pub async fn send_log(&mut self, entry: &LogEntry) { ... }
    pub async fn close_peer(&mut self) { ... }  // 模拟"app 退出"
    pub fn kill_socket(&mut self) { ... }       // 模拟网络重置
}
```

`tests/reconnect_test.rs`（新增）包含 4 条端到端场景：

1. **e2e_peer_close**：fake server 发 Close 帧 → assert App 状态机变成 Reconnecting，status_message 包含 "disconnected"，LogStore 保留
2. **e2e_reconnect_same_session**：场景 1 后 fake server 重新 accept → assert ConnStatus 回 Live，active_app_id 不变
3. **e2e_writer_failure_cascade**：fake server accept 后立即关闭读端 → 触发 writer 错误 → assert reader 也解阻塞、Disconnected 事件送达（验证 S2）
4. **e2e_device_removed_vs_ws_dropped**：两个并行 fake server 分别触发 WS 断和模拟 DeviceRemoved → assert 前者 LiveState=Reconnecting、后者 LiveState=Offline

### 4.5 Forbidden-pattern 测试（新类别）

`tests/forbidden_patterns_test.rs`（新增）：

```rust
#[test]
fn no_eprintln_in_production_code() {
    let src = walkdir::WalkDir::new("src")
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension() == Some("rs".as_ref()))
        .filter(|e| !e.path().to_string_lossy().contains("_tests"));

    let mut offenders = vec![];
    for entry in src {
        let content = std::fs::read_to_string(entry.path()).unwrap();
        for (lineno, line) in content.lines().enumerate() {
            if line.contains("eprintln!") && !line.trim_start().starts_with("//") {
                offenders.push(format!("{}:{}", entry.path().display(), lineno + 1));
            }
        }
    }
    assert!(offenders.is_empty(),
        "eprintln! in production code pollutes alternate screen:\n  {}",
        offenders.join("\n  "));
}
```

目的：让本次修复**不可回滚**。任何人将来再加 `eprintln!` CI 就红。

此类别未来可扩展到其他反模式（例如 `println!` 在 TUI 路径、`unwrap()` 在 connector 路径等），本次先立类别、只加 `eprintln!` 一条。

### 4.6 手动冒烟

自动化之外的回归确认（保留为 checklist，不是门禁）：

- 用 aura-lang-flutter 连一次，关闭 app，观察：
  - 屏幕底部**无 eprintln 残留**
  - 状态栏左下角 chip 从 `LIVE` 变 `RECONNECTING`（黄底）
  - 拔掉设备后 chip 变 `OFFLINE`（暗灰）
- 滚动日志，观察 `LIVE` 变 `PAUSED`
- `go_bottom` 后回到 `LIVE`

### 4.7 CI 门禁

- `cargo test` 全绿（包含新增的 state matrix、e2e、forbidden-pattern）
- `cargo test --test forbidden_patterns_test` 单独可运行
- `cargo clippy -- -D warnings` 无新警告

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
