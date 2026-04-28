# 代码风格指南

本文件给人类贡献者和 AI subagent 看。每一条规则都来自 2026-04 清理战役
的实际案例，不是从 Rust 通用风格搬的理论。所有规则都经过了特征化测试
的审视 —— 遵守它们就能让新代码通过 `cargo clippy -D warnings`、
通过 code review、通过未来的 UI 框架迁移。

本文件是 `docs/CONTRIBUTING.md` 的补充；CONTRIBUTING 讲流程，本文件
讲**代码本身**。

---

## 1. 命名约定

Rust 默认就是这套。本节只写**本项目的偏好**。

- `snake_case` —— 函数、方法、模块、文件名、字段
- `PascalCase` —— struct、enum、trait
- `SCREAMING_SNAKE_CASE` —— const、static
- 模块名用**单数**（`parser`、`transport`、`ui`），不用复数（不是
  `parsers` / `transports`）
- 文件名要**表达内容**，不要表达分类：
  - ✅ `filter.rs`, `mock_edit.rs`, `device_monitor/adb_source.rs`
  - ❌ `types.rs`, `utils.rs`, `helpers.rs`, `common.rs`
- 枚举变体不带类型前缀：
  - ✅ `enum ViewTab { Logs, Network }`
  - ❌ `enum ViewTab { TabLogs, TabNetwork }`
- 常量命名带**前缀表示领域**，避免混用：
  - ✅ `ADB_LOCAL_PORT_POOL_BASE`, `DOUBLE_CLICK_MS`, `SSE_EVENTS_PILL`
  - ❌ `BASE`, `MS`, `PILL`（没上下文）

## 2. 错误处理

### 2.1 按层分派

| 层 | 错误传播方式 | 例子 |
|---|---|---|
| `domain/`, `parser/` | `Result<T, DomainError>` 或 `Option<T>`，纯函数不 panic | `filter.rs::set_search` |
| `transport/`, `input/` | `Result<T, Box<dyn Error + Send + Sync>>`，I/O 都可能失败 | `connector::connect` |
| `app/` | 内部 unwrap 仅限于"刚刚 push 的 entry 一定存在"这类逻辑不变量 | `App::switch_to_app` |
| `ui/` | 不产生错误；渲染失败一律用空态 | `draw_empty_network` |
| `event/` | 不产生错误；未识别的输入 silently no-op，但加 `// UI-007` 注释说明 | `handle_normal_key` |
| `main.rs` / `run/` | 所有 `Result` 必须在此层解决 | `run::run_loop` |

### 2.2 `.unwrap()` 和 `.expect()`

- **允许** 在测试代码里随便用
- **允许** 在 `main.rs` / `run/` 的启动路径用（失败就挂，打印清晰信息）
- **禁止** 在 `domain/` `parser/` 的生产代码里使用 —— 这两层是纯函数，
  必须返回 `Option` / `Result`
- **`.expect(msg)` > `.unwrap()`** —— `expect` 至少写了 panic 原因

### 2.3 `panic!` 的合法用途

只有一种场景允许 `panic!`：**违反已文档化的结构性不变量**。例如
`App::switch_to_app` 里断言 "id 必须在 connected_apps 里"。
这类 panic 必须：

1. 在方法 dartdoc / rustdoc 明确记录
2. 有 `debug_assert!` 或 `assert!` 兜底
3. 有特征化测试覆盖反例（`should_panic` 或显式的不变量验证）

## 3. 注释

**默认不写注释。** 只在"为什么"非显然时写。

### 3.1 必须写的情况

1. **历史 bug 留下的非直观约束** —— 注释里引用 audit ID 或 commit SHA
   ```rust
   // UI-042: WS chat ↔ raw toggle must purge opposite-mode keys from
   // collapsed_sections, otherwise stale keys corrupt the neighbour pane render.
   ```
2. **和外部协议/平台交互的细节**
   ```rust
   // WHY: flog_dart hello is emitted synchronously on WS accept; 3s covers
   // typical slow iOS sim boot. Shorter values flake on cold simulator.
   const HELLO_TIMEOUT: Duration = Duration::from_secs(3);
   ```
3. **"当前看起来没用，不要删"的死代码**
   ```rust
   // Kept for future replay-by-id flow (TRANS-013 D-ref, archived).
   #[allow(dead_code)]
   pub fn rebuild_replay_url(...) { ... }
   ```

### 3.2 禁止写的情况

- 复述代码做什么 —— 函数名已经说了
- "此函数返回 X" —— 签名已经说了
- AI 生成的模板式 docstring（`Constructs a new instance of...`）
- `// TODO` 没有具体 ID / 时限 —— 要么现在就做，要么进 audit 里记

### 3.3 模块级 `//!` 块

每个非琐碎模块（>50 行）在顶部有一段 `//!` 块，结构：

```rust
//! 一行概要。
//!
//! 背景 / 关联的 audit ID / 历史决策 2-5 行。
//!
//! # Invariants
//! - 如果有重要不变量，列在这里
//!
//! # Dependencies
//! 只在和方向性有关时写（例如 "只被 event/ 读取，不被 ui/ 读取"）。
```

## 4. 模块边界

### 4.1 依赖方向

```
ui → app → domain ← parser/input/transport
         ↑
       event
```

**禁止反向**。具体的"不能 import 什么"规则见
[`docs/UI_FRAMEWORK_BOUNDARY.md`](UI_FRAMEWORK_BOUNDARY.md)。

### 4.2 公开度

默认原则：**越窄越好**。

- 新函数：先 `fn`（私有），有外部调用者再升 `pub(super)`，跨模块再升
  `pub(crate)`
- **`pub` 只给 crate 外部真正会用的 API**（几乎只有 `lib.rs` 的 re-exports）
- 测试内部细节用 `#[cfg(test)] pub(crate)` 暴露

### 4.3 文件大小预算

见 CONTRIBUTING §5.5。简述：

- **< 500 行生产代码**：默认目标
- **500-800 行（黄）**：必须在 `//!` 里解释为什么不能拆
- **> 800 行（红）**：必须拆
- **测试文件豁免**：`*_tests.rs` 不计入预算

## 5. async / 并发

### 5.1 什么时候用 tokio::spawn

- **长期后台任务**（device monitor、WebSocket reader/writer）—— 配 `#[allow(dead_code)]` 的 JoinHandle 是反模式，要么监控，要么明确声明 fire-and-forget + 留注释
- **一次性 I/O 超时** —— 用 `tokio::time::timeout`，不要开新 task

### 5.2 App 锁粒度

`Arc<Mutex<App>>` 是单把大锁。**只在需要改 App 的那段代码里拿锁**。
禁止：

- 拿着锁调 async I/O（会阻塞其他 task）
- 拿锁嵌套（lock ordering 在本项目未设计）

### 5.3 channel 选择

- `tokio::sync::mpsc::unbounded_channel` —— 生产者速率远小于消费者（discovery events）
- `tokio::sync::mpsc::channel(N)` —— 有明确 backpressure 需求
- `std::sync::mpsc` —— 禁止（会阻塞 tokio runtime）
- `crossbeam::channel` —— 禁止（project 已统一用 tokio）

## 6. 测试风格

### 6.1 三种测试

| 类型 | 放在哪里 | 写什么 |
|---|---|---|
| 内部单元测试 | `src/foo_tests.rs`（兄弟文件模式） | 单个 pub fn 的黑盒行为 |
| 特征化测试 | `tests/characterization_*.rs` | 冻结当前可观察行为，作回归围栏 |
| red-lock bug 测试 | `tests/characterization_bugs.rs` | `#[ignore = "bug: <id>"]` 锁 B 类 |

### 6.2 命名

测试函数名用**陈述句**表达"应当什么"，不是"是否什么"：

```rust
// ✅
fn filter_state_tag_exclude_rejects_matching() { ... }
fn sse_merged_j_wraps_to_zero_at_end() { ... }

// ❌
fn test_filter() { ... }
fn check_tag_filter() { ... }
```

### 6.3 断言

- **测试可观察特征，不测像素**（Rule 3）：断言文本存在、cell 颜色、
  span 数量，不做 buffer diff
- 一个测试**一个断言焦点**。要多个断言就分成多个 `#[test]`
- `assert_eq!` / `assert!` 配 **显式 failure message**（如果不明显）

### 6.4 测试密度闸门

- **Rule 2**：核心模块覆盖率 ≥ 90%（`domain/`），≥ 85%（`ui/`）
- **Rule 9**：审计条目说 N 个场景，测试写 N 个用例，不合并
- **Rule 10**：核心模块每个 pub fn ≥ 5 个用例，平凡 getter 可豁免

## 7. 特定 Rust 模式

### 7.1 `Default` impl

能 `#[derive(Default)]` 就 derive。derive 不了就手写，理由写在 dartdoc。
**禁止** 在 `new()` 里写 `Self { field: Default::default(), ... }` —— 用 `..Default::default()` 语法。

### 7.2 `Clone` 和 `Copy`

- `Copy` —— 只给真正的 POD（枚举、坐标 struct）
- `Clone` —— 容器、数据 struct（`NetworkEntry`、`LogEntry`）
- **避免无意义的 Clone** —— 如果传引用可以解决就传引用

### 7.3 Builder vs 字段 literal

- **字段数 ≤ 3**：直接用 `struct literal`
- **字段数 4-7**：有默认值就用 `..Default::default()`
- **字段数 > 7 或参数含语义**（如 `http_entry` / `sse_entry`）：用 builder
  - 见 `NetworkEntry::builder()` / `NetworkEntry::http_builder()`

### 7.4 `&str` vs `String`

- **参数**：能用 `&str` 就用 `&str`；确实需要 own 再用 `String`
- **返回值**：返回 own 的 `String`，让调用方决定
- **const**：`const FOO: &str = "..."`（不是 `&'static str`）

### 7.5 `match` 穷尽性

```rust
// ✅ 显式列举所有变体
match kind {
    FlogNetKind::Req { .. } => ...,
    FlogNetKind::Res { .. } => ...,
    FlogNetKind::Err { .. } => ...,
    FlogNetKind::SseChunk { .. } => ...,
    FlogNetKind::WsMsg { .. } => ...,
}

// ❌ 用 _ 兜底——新增变体时编译器不提醒
match kind {
    FlogNetKind::Req { .. } => ...,
    _ => {}
}
```

例外：第三方类型（`KeyCode`、`MouseEventKind`）允许用 `_`，但要写注释。

## 8. UI 相关（详见 UI_FRAMEWORK_BOUNDARY.md）

### 8.1 渲染层铁律

- 渲染函数签名：`fn draw_xxx(f: &mut Frame, app: &mut App, area: Rect)` —— `app` 可以 mutable 是为了写 LayoutCache，但**不能改业务状态**
- **LayoutCache 之外的 App 字段，渲染函数禁止写**
- 渲染函数**永远不 panic**；异常态用空视图

### 8.2 事件层铁律

- 输入事件首先被翻译成 `ClickRegion` / `KeyAction`（中性枚举），
  **不得**让 `crossterm::KeyCode` 直接进 `apply.rs` 的 mutation 逻辑
- 见 Phase 3 Step 3.6 两阶段派发 —— `detect_*` 是纯函数，`apply_*` 负责 mutation

## 9. 给 AI 的额外建议

本项目主要由 AI + 人类协作开发。如果你是 AI，特别注意：

1. **不要加"解释做什么"的注释** —— 写了会被 review 删掉
2. **不要脑补 UI** —— 写测试前先读代码里真实的行为，见 Phase 2.5B
   §5.5 教训
3. **不要主动重命名现有 API** —— 除非有 audit 条目授权
4. **一个 task 一个 commit** —— 中途生成的调试代码要么 commit 要么删
5. **`cargo clippy --all-targets -D warnings` 必须 0 告警才能 commit**
6. **凭直觉加 `#[allow(dead_code)]` 前先问自己：真的没法把它删掉吗**

## 10. 偏离本指南的办法

本指南不是铁律。**偏离必须在 commit message 或审计条目里说清楚为什么**。
无理由的偏离在 code review 时会被要求调整或回滚。

如果你认为某条规则本身错了，**不要在 PR 里偷偷改**。开一个新的审计条目
（D 类），在 commit message 里引用，让下一次方法论迭代考虑。
