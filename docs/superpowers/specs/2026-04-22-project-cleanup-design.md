# flog 项目健康化整理 Design

- **日期**：2026-04-22
- **范围**：整个 flog 仓库（Rust TUI + flog_dart 包）
- **状态**：Design approved, 待 writing-plans 拆分为可执行 plan

---

## 0. 背景与动机

flog 当前处于"功能基本完成、需要整理"的状态：217 个测试全绿、两个 tab 架构已稳定、flog_dart 已发布到 pub.dev。但代码层面累积了典型的迭代债务：

- 18 个 clippy warning + 1 个 test 编译错误（`PI` 近似值），`cargo clippy -- -D warnings` 挂
- 5 个源文件超过 800 行（最大 `event.rs` 1677 行、`logs/mod.rs` 1358、`app.rs` 1167、`network/detail.rs` 1109、`source_select.rs` 898）
- 多处死代码、魔法值散布、补丁式改动留下的痕迹
- UI 层逻辑与渲染耦合，测试网只覆盖 domain / parser 薄薄一层
- 已有文档：`CLAUDE.md` / `README.md` / `README_EN.md` / `flog_dart/README.md` / `flog_dart/CHANGELOG.md`；缺：架构总览 / 模块文档 / AI 协作规范 / 协议规范。且 Phase 3 改架构后，现有文档会过时需同步更新

本次整理的目标**不是加新 feature**，而是：

1. 把项目推到"长期可维护 + AI 可协作 + 回归自动可见"的状态
2. 本次工作本身同时作为一个"AI 长工作流实践"的样本案例沉淀下来

---

## 1. 总目标（硬指标）

交付完成时必须满足：

1. `cargo clippy --all-targets -- -D warnings` 零 warning
2. 零 `#[allow(dead_code)]`、零死代码（cargo 与人工双重确认）
3. **文件行数规则**（详见 §5.5）：
   - < 500 行绿灯
   - 500-800 黄灯，每个超标文件需在 Phase 3 step 文档中写一句为何可以不拆
   - \> 800 红灯，默认必拆，例外需白名单（大 match / 协议定义 / 纯常量表）
4. **测试达到"重构安全网"级别**：
   - `src/event.rs` / `src/app.rs` / `src/domain/filter.rs` / `src/domain/network_filter.rs` / `src/ui/logs/mod.rs` / `src/ui/network/mod.rs`（抽取后的逻辑函数部分）分支覆盖 ≥ 70%（前两个热点）/ ≥ 85%（两个纯逻辑 filter）
   - 其他模块整体分支覆盖 ≥ 60%
   - 总测试数从 217 预计增长到 400+（预估，非硬指标）
5. **文档体系齐全**：
   - 新增：`ARCHITECTURE.md` / `MODULES.md` / `CONTRIBUTING.md` / `PROTOCOL.md`
   - 更新：`README.md` / `README_EN.md` / `CLAUDE.md` / `flog_dart/README.md` / `flog_dart/CHANGELOG.md`
   - `CONTRIBUTING.md` 把"改动必须配测试"和"新文件遵守 500/800 行阈值"写成硬规矩
6. **过程资产沉淀**：
   - `docs/superpowers/audit/` 下 Audit 报告齐全
   - `docs/superpowers/plans/` 下 Phase 3 各 step 设计文档齐全
   - `docs/superpowers/journal/` 下每个 phase 的原始日志齐全
   - `docs/superpowers/retrospectives/` 下本次 flog 清理的专属复盘
   - `docs/superpowers/methodology/` 下抽象出的 AI 长工作流案例研究

### 非目标（显式排除，避免 scope 蔓延）

- 不加新 feature
- 不改 public CLI flag（`--port` / `--level` / `--tag`）
- 不改 WS 协议字段结构（除非 Audit B 类确认协议本身有 bug 且用户批准）
- 不改 `flog_dart` public API 签名（除非 Audit B 类且用户批准）
- 不加新依赖（crate / pub.dev package，除非 Audit 明确推荐）

---

## 2. Phase 总览

6 个 phase，每个 phase 1 个 commit，commit 之间可回滚。

```
Phase 1  Audit（只读，4 subagent 并行）
  产出：五分类发现清单 (A/B/C/D/E)，用户审阅 C 类
  commit: docs-only (audit reports)
                    ↓
Phase 2  Mechanical cleanup（subagent 按子系统并行）
  只做 0 风险、0 设计判断的改动
  clippy 零 warning / 死代码清除 / fmt / Default 补全
  commit: 1 个
                    ↓
Phase 2.5 Characterization tests（subagent 按模块并行）
  A/D 类 → 绿色 characterization test
  B 类 → ignored/should_panic 测试（Phase 3 要解除并变绿）
  UI 按"逻辑与渲染分离"原则,只测逻辑层
  覆盖率门槛：热点模块 ≥ 70% / 纯逻辑 ≥ 85% / 其他 ≥ 60%
  commit: 1 个
                    ↓
Phase 3  Redesign & Rebuild（串行,测试守护）
  - 拆巨型文件(按 §5.5 规则)
  - 重构职责/边界混乱的模块
  - 消除魔法值(带抽象化命名)
  - 消除补丁式代码还原成正确设计
  - 修 B 类 bug
  - 每子步骤必须附新测试
  commit: 1 个(内部多 commit 视情况 squash)
                    ↓
Phase 4  Comments
  只补 "why" 注释,不补 "what"
  commit: 1 个
                    ↓
Phase 5  Docs（subagent 并行,两波）
  第一波:ARCHITECTURE / MODULES / CONTRIBUTING / PROTOCOL(新增)
  第二波:README.md / README_EN.md / CLAUDE.md / flog_dart 文档(更新)
  commit: 1 个
                    ↓
Phase 6  Retrospective & Methodology（1 subagent 串行）
  产出 A: flog 专属复盘
  产出 B: AI 长工作流案例研究(方法论抽象)
  commit: 1 个
```

### 每个 phase 退出门槛（共用）

- `cargo test` 全绿（ignored 测试不算失败）
- `cargo clippy --all-targets -- -D warnings` 通过（Phase 2 之后强制）
- `cargo fmt --check` 通过
- 该 phase 的专属验收门槛（详见各节）

### worktree 策略

- Phase 1 只读 subagent 可不用 worktree（零冲突）
- Phase 2 / 2.5 / 5 并行 subagent 各自 worktree
- Phase 3 串行在主工作区
- Phase 6 单 subagent 无需 worktree

---

## 3. Phase 1 — Audit

### 3.1 总则

4 个 subagent 并行，**只读**（`Explore` 或 `general-purpose` agent，严禁写代码）。每个 subagent 产出一份结构化 Audit 报告 markdown，写到 `docs/superpowers/audit/` 下。

### 3.2 Audit 五分类

每条发现必须标一个 label：

| 标签 | 含义 | 去向 |
|---|---|---|
| **A. Correct-but-ugly** | 行为对，代码丑 / 不优雅 | Phase 3 重新设计（保行为） |
| **B. Confirmed bug** | 行为错（崩溃、数据错、UX 反直觉） | Phase 2.5 写红测 → Phase 3 修 |
| **C. Ambiguous** | 不确定是 feature 还是 bug | 暂停，问用户裁决 |
| **D. Architecture smell** | 抽象缺失 / 职责错位 / 补丁痕迹 / 状态机不合理 / 魔法值代表的概念没提取 | Phase 3 重新设计 |
| **E. Mechanical** | 真正 0 风险的机械修补（clippy 等价改写、死代码、拼写） | Phase 2 |

### 3.3 报告格式

每条发现包含字段：

```yaml
id: UI-007
label: D
location: src/event.rs:412-458
title: 输入模式状态机耦合 logs 与 network tab
evidence: |
  <3-10 行代码引用 + 观察到的行为>
proposed_action: |
  A/D → 重新设计思路
  B → 期望行为
  C → 待确认问题
  E → 具体修法
risk: low | medium | high
```

报告末尾附 summary 表：A/B/C/D/E 各多少条。

### 3.4 禁止词

报告里不得出现"TODO" / "待讨论" / "也许" / "可能"这种兜底词。要么归 C（主动问用户），要么归入其他确定类。

### 3.5 四个 subagent 的领域边界

**Agent 1 · Transport & Discovery**
- 范围：`src/transport/`、`src/input/connector.rs`、`src/input/protocol.rs`、`src/main.rs` 中涉及连接生命周期的部分、`src/replay.rs`
- 重点：并发 / 生命周期（ghost device、Hello timeout、ADB port cycling 这些最近刚修过的点）；跨平台三条路径（Localhost / AdbForward / Usbmuxd）职责是否对称；协议类型是否有隐式耦合；`ConnectorEvent` 错误分支完整性
- 产出：`docs/superpowers/audit/01-transport.md`

**Agent 2 · Domain Layer**
- 范围：`src/domain/` 全目录 + `src/parser/` 全目录 + `src/session.rs`
- 重点：`LogStore` 环形缓冲边界（100K / 10% drain / dup folding）；`FilterState` 正则预编译 + pipe-OR 退化；`NetworkFilter` 三套枚举可否统一；`network_store` 状态流转；Mock / SSE / WS 三块有无重复模式；parser chain 责任划分；`filter.rs` 420 行 / `structured_parser.rs` 693 行拆分点
- 产出：`docs/superpowers/audit/02-domain.md`

**Agent 3 · UI Layer (含 event dispatch)**
- 范围：`src/ui/` 全目录 + `src/app.rs` + `src/event.rs` + `src/cli.rs`
- 重点（最大一块）：`event.rs` 1677 行状态机/key 路由隐藏死路径；`app.rs` 1167 行里哪些 state 属于 UI 被误放顶层；`AppMode::InputActive(InputField)` 是否补丁式；`logs/mod.rs` 1358 / `network/detail.rs` 1109 / `source_select.rs` 898 拆点分析；滚动模型在两个 tab 是否真的统一；`json_viewer/` 作为 shared 是否泄漏；魔法值密度；**逻辑与渲染是否可分离**（直接决定 Phase 2.5 UI 测试怎么写）
- 产出：`docs/superpowers/audit/03-ui.md`

**Agent 4 · flog_dart**
- 范围：`flog_dart/lib/` 全目录 + `flog_dart/test/`
- 重点：`FlogDio` 自动插入 `FlogMockInterceptor` + `FlogHttpInterceptor` 的顺序保证；`FlogMockInterceptor` 匹配逻辑 / `FlogHttpInterceptor` response 时机；`FlogSseParser` / `FlogWebSocket` 在 binary / 异常流 / 中断场景；`flogEnabled` 是否全路径 tree-shake 干净；`ext.flog.syncMockRules` 错误路径；公开 API 的暴露与文档
- 产出：`docs/superpowers/audit/04-flog-dart.md`

### 3.6 合并 index

Phase 1 结束时由主协作者（人 / 主 Claude）合并：

`docs/superpowers/audit/00-index.md`：把 4 份报告的 **B 类（bug）+ C 类（问用户）** 条目按严重度排序。用户只需读这一份做裁决。

### 3.7 Phase 1 验收门槛

1. 4 份 audit 报告齐全，格式合规
2. 报告里无禁止词（见 §3.4）
3. 所有 C 类条目用户已裁决完，改判成 A/B/D/E 之一
4. `00-index.md` 合并完成
5. 1 个 docs-only commit

---

## 4. Phase 2 & Phase 2.5

### 4.1 Phase 2 — Mechanical cleanup

**硬约束**：只做 0 风险、0 设计判断的事。

**允许的改动白名单**：
1. Clippy warning 机械等价改写（`.into_iter()` 去掉、`saturating_sub`、`stripping a prefix manually`、`manual char comparison` 这类）
2. 编译器 `dead_code` 警告 —— **仅限** Audit 归入 E 类的
3. `cargo fmt` 一致化
4. `Default` trait 补全（前提：`new()` 无参且行为与 `Default` 一致）
5. 纯粹拼写 / 注释错别字 / 废弃 `#[allow(dead_code)]` 清理
6. test 编译错误修复（`PI` 近似值）

**不允许的事**（挪 Phase 3）：
- 提取魔法值为常量（涉及命名/归属设计判断）
- 合并重复代码成函数 / trait
- 改函数签名
- 任何涉及文件结构 / 模块边界的改动

**执行**：4 个 subagent 并行（transport / domain / ui+event / flog_dart），各自 worktree。合并次序：transport → domain → flog_dart → ui（ui 最大，最后吸收冲突）。

**验收门槛**：
- `cargo clippy --all-targets -- -D warnings` 通过（**Phase 2 存在的理由**）
- `cargo test` 全绿，测试数 ≥ 217
- `cargo fmt --check` 通过
- 1 个 commit

### 4.2 Phase 2.5 — Characterization tests

**定位**：整个计划的**风险枢纽**。Phase 3 的安全依赖于此。

#### 4.2.1 测试分类对应 Audit

| Audit 类 | 测试形态 | 状态 |
|---|---|---|
| A | 绿色 characterization test，锁行为 | 永久绿 |
| B | 断言期望行为的测试 | 写完是红，标 `#[ignore = "bug: AUDIT-xxx, fix in Phase 3"]` |
| D | 绿色 characterization test 覆盖当前行为 | 永久绿，Phase 3 重新设计时作为行为基线 |
| C | 不写 | — |
| E | 不写 | — |

**关键**：B 类测试故意留红 + ignore。它们是 Phase 3 修 bug 的作业清单。

#### 4.2.2 测试写在哪

- **Domain / Parser 层**：各自 `#[cfg(test)] mod tests`，补全边界用例
- **Transport 层**：`tests/transport_*.rs` 集成测试 + 模块内单测；可能需要引入测试替身（fake adb / fake ws），替身代码写在 `tests/support/`
- **UI + event 层（最难）**：
  - 前置动作：**把纯逻辑从 render 函数里抠出来成独立纯函数 / 方法**
    - 例：`logs/mod.rs` 里"给定 entries + viewport 高度 + scroll offset → 渲染哪些行" → 抽成 `compute_visible_range()` 纯函数 → 测
    - 例：`event.rs` key dispatch → 抽成 `handle_key(state, key) -> Vec<AppAction>` 返回动作而非直接修改 state → 测
  - ⚠ 这一步看起来像 Phase 3 的工作。规矩：**最小化抽取，只为让测试能写出来；不得重新设计**。抽出来的函数签名必须让 Audit D 类未来的重新设计还能做
  - 实在抽不出来（耦合太深）：用 `ratatui::backend::TestBackend` snapshot 小范围兜底，并在 Audit 加一条 D：`此处无法纯函数化 → Phase 3 必须重构`

#### 4.2.3 覆盖率量化

- 工具：**开工前定** —— `cargo-llvm-cov` 或 `cargo-tarpaulin`（详见 §7.2 开工前准备）
- 门槛：
  - `src/event.rs` 分支覆盖 ≥ 70%
  - `src/app.rs` 分支覆盖 ≥ 70%
  - `src/domain/filter.rs` 分支覆盖 ≥ 85%
  - `src/domain/network_filter.rs` 分支覆盖 ≥ 85%
  - `src/ui/logs/mod.rs`（抽取后的逻辑函数部分）≥ 70%
  - `src/ui/network/mod.rs`（抽取后的逻辑函数部分）≥ 70%
  - 其他模块**整体** ≥ 60%
- 生成覆盖率报告的命令进 `CONTRIBUTING.md`

#### 4.2.4 避免 pin-bug 陷阱

- subagent 写测试前必须读 Audit 报告，确认要冻结的是 A/D 而不是 B
- 凡觉得"这行为怪怪的但 Audit 没标 B"的地方，**不擅自冻结**，作为新的 C 类问题回报
- 每个 subagent 的测试 commit 要附 "testimony"：新增测试对应 Audit 哪些 A/B/D id

#### 4.2.5 执行

- 4 个 subagent 并行（domain+parser 合并一个 / transport 一个 / ui+event 一个 / flog_dart 一个），各自 worktree
- UI+event subagent 工作量最大，可能中途需要汇报

#### 4.2.6 Phase 2.5 验收门槛

1. 所有 A / D 条目都有对应绿色 characterization test
2. 所有 B 条目都有对应红色/ignored 测试，ignore 理由统一格式 `"bug: AUDIT-xxx, fix in Phase 3"`
3. 覆盖率量化门槛全部达到
4. `cargo test` 全部通过（ignored 不算失败）
5. 测试文件**不得依赖具体 UI 文本字符串**（断言语义不断言字面）
6. 1 个 commit

---

## 5. Phase 3 — Redesign & Rebuild（心脏）

### 5.1 指导原则

1. **测试是唯一真理**。每一步后：`cargo test` 全绿 + clippy 零 warning + fmt 干净。红 → 停 → 修或回滚；禁止在红色状态下推进
2. **A 类测试不得因重构变红**（变红 = 改坏了对的行为 → 回滚）
3. **B 类测试在 Phase 3 结束时必须全部从 ignored 变绿**
4. **每个重构子步骤必须附新测试**（新结构 / 新抽象的合约测试）
5. **一次一个模块**。跨模块耦合如属 Audit D，单独开子步骤
6. **设计优先于实现**：每个子步骤开工前，subagent 先在 step 文档里写 **"旧设计问题 → 新设计思路 → 迁移策略"**，这些段落直接进 Phase 5 ARCHITECTURE / MODULES
7. **代码洁癖**：每个子步骤结束必须回头 diff review，消除 orphan 代码 / 过时注释 / 半拉子抽象

### 5.2 动作清单来源

Phase 3 不自由发挥，100% 来自 Audit 报告的 A/B/D：

| Audit 类别 | Phase 3 处理 |
|---|---|
| A | 重新设计实现，A 类测试守护行为不变 |
| B | 修复，ignored 测试解除并变绿 |
| D | 重新设计，D 类 characterization test 守护行为不变，新测试断言新结构 |

**Phase 3 途中发现新架构问题** → 不自作主张 → 加入 Audit 作为新条目 → 下次 planning 处理。

### 5.3 串行顺序（依赖倒序）

```
Step 3.1  Parser 层 redesign                              ← 最底层,无依赖者
Step 3.2  Domain 层 redesign (store/filter/network_*)
Step 3.3  Transport 层 redesign
Step 3.4  flog_dart redesign
          ─── 3.1-3.4 依赖图上互相独立,但仍强制串行 ───
              (并行 subagent 易留下风格不一致的边界)
Step 3.5  App 状态机 redesign (app.rs 拆 + AppMode 重新设计)
Step 3.6  Event dispatch redesign (event.rs 1677 → 分拆 + pure handler)
Step 3.7  UI Logs 视图 redesign (logs/mod.rs 1358 → 子模块)
Step 3.8  UI Network 视图 redesign
Step 3.9  UI shared (json_viewer / input_field / text_editor / source_select)
          ─── 3.5-3.9 必须串行,状态机是根 ───
Step 3.10 Cross-cutting cleanup pass-2
          回头消除 3.1-3.9 留下的接缝
```

### 5.4 每个 Step 的内部流程

```
(a) 读 Audit 报告中 scope 内的 A/B/D 条目
(b) 写 Step 设计文档(旧问题 → 新设计 → 迁移策略),保存为
    docs/superpowers/plans/YYYY-MM-DD-phase3-stepN-<module>.md
    → 设计未经用户批准不得开工
(c) 按新设计重写代码,逐条处理 Audit 条目
(d) 每处理一条: cargo test + clippy + fmt。红 → 停
(e) 补新测试断言"新结构的合约"(不是行为 - 行为已被 A/D 守护)
(f) 回头 diff review,检查 orphan 代码/过时注释/半拉子抽象
(g) 验收:
    - 所有 A 类测试绿
    - 本 step 涉及的 B 类测试从 ignored 变绿
    - 所有 D 类 characterization test 仍绿
    - 新结构测试齐全
    - 行数符合 §5.5 规则
    - 每个新模块在 step 文档里有"职责一句话"描述
(h) commit(step 内可多 commit,phase 完成时考虑 squash)
```

### 5.5 行数规则（信号,不是判决）

| 范围 | 对待 |
|---|---|
| < 300 行 | 舒适区 |
| 300-500 | 绿灯，无需关注 |
| 500-800 | 黄灯，Step 设计文档必须写一句"为什么可以不拆" |
| > 800 | 红灯，**默认必拆**，例外需白名单（大 match / 协议定义 / 纯常量表）并用户批准 |

**数字是信号，Audit 的设计判断高于数字**。Audit 把某处归入 D → 拆；归入 A 且文件处于黄灯区 → 看 step 文档的解释决定。

### 5.6 其他硬指标

- **循环依赖：零**（可用 `cargo modules dependencies` 或 `cargo-depgraph` 验证）
- **public API 最小化**：`pub` 暴露的每个符号在 step 文档里列清理由

### 5.7 Phase 3 总体验收门槛

1. 所有 Audit A/B/D 条目落地（B 全部解除 ignore 并绿）
2. 所有源文件符合 §5.5 行数规则
3. `cargo clippy --all-targets -- -D warnings` 通过
4. `cargo test` 全绿，总测试数相比 Phase 2.5 再增长
5. 覆盖率门槛不回退
6. `docs/superpowers/plans/*.md` step 设计文档齐全
7. 1 个 phase 级 commit

### 5.8 红线（不允许做的事）

- 改 `ClientMessage` / `ServerMessage` WS 协议字段（除非 Audit B 类确认协议有 bug + 用户批准）
- 改 `flog_dart` public API 签名（除非 Audit B 类 + 用户批准）
- 加新依赖
- 删除 / 改名用户可见 CLI flag

### 5.9 与 memory 规矩一致性

Project memory 里 `feedback_*.md` 的实战经验（UI aesthetics / 滚动行为 / 拦截器顺序 / 验证交付 / 主动思考 / 框架先于细节）在 Phase 3 subagent prompt 里会显式引用为约束。

---

## 6. Phase 4 & Phase 5

### 6.1 Phase 4 — Comments

**目标**：在 Phase 3 重构完成的**最终代码**上补"why"注释（不更早做，因为早写的注释会跟着代码被搬来搬去甚至变废话）。

**允许补的类型**（按 CLAUDE.md 注释规约）：
- 非显而易见的约束
- 隐藏的不变量
- 特定 bug 的 workaround（引用 Audit id 或 commit hash）
- 读者易误解的行为
- 模块级顶部 `//!` 一句话职责（配合 Phase 5 MODULES.md）

**不允许的类型**：
- 解释代码做什么（命名已做）
- 引用当前任务 / PR / 作者
- 多段 docstring（一行足够）
- emoji / 夸张语气

**执行**：1 个 subagent 扫全工程，参考 Phase 3 step 文档里标 `@COMMENT-WORTHY` 的点。单 worktree，纯追加。

**验收**：
1. 所有模块根部有 `//!` 一句话职责
2. `@COMMENT-WORTHY` 点均已补
3. 无"what"型注释（subagent 自检 + 用户 review）
4. 测试全绿 / clippy 零 warning
5. 1 个 commit

### 6.2 Phase 5 — Docs

**新增 4 份**（根目录）：

- **`ARCHITECTURE.md`**：分层图 / 数据流 / 并发模型 / 错误处理哲学。原材料 = Phase 3 step 设计文档的"新设计思路"
- **`MODULES.md`**：每个模块一个 section（职责/输入/输出/依赖/关键类型/关键测试）。原材料 = Phase 3 step 文档 + 模块级 `//!` 注释
- **`CONTRIBUTING.md`**：AI 协作硬规矩 —— 改动必须配测试 / 500-800 行阈值 / 新增 parser 流程 / 新增 UI 组件流程 / 新增 transport 后端流程 / 常用命令 / PR & commit 规范 / AI 读取顺序（CLAUDE.md → ARCHITECTURE → MODULES → 具体代码）
- **`PROTOCOL.md`**：Hello 握手 / 端口扫描 / identity verification / `ClientMessage` / `ServerMessage` / `FlogNetMessage` 字段表 + JSON 样例 / 版本协商策略 / Log tag 约定

**必须更新的现有文档**：
- `README.md`（中文）
- `README_EN.md`（英文）
- `CLAUDE.md`（Architecture 一节同步 + 顶部加"请读 ARCHITECTURE.md"引导）
- `flog_dart/README.md`（pub.dev 可见，**对外门面**）
- `flog_dart/CHANGELOG.md`（B 类 bug 修复 / 行为变化必须加条目；纯内部重构也加"internal refactor"条目）

**硬规矩**：
- 文档与代码事实**一致**，无旧段落残留
- 示例代码 / 命令必须跑得通（subagent 实测 `cargo install --path .` + 示例）
- **双语 README 保持一致**，不允许漂移

**执行（两波）**：
- 第一波：4 subagent 并行写新增文档（A/B/C/D）
- 第二波：2 subagent 并行更新现有文档（E: README × 2 + CLAUDE / F: flog_dart 双文档），必须**读第一波 draft** 后再写

**验收**：
1. 4 份新文档 + 5 份更新文档齐全
2. 交叉引用通畅（ARCHITECTURE → MODULES → 代码路径）
3. 无 TODO / 未来完善 挂起条目
4. 双语 README 语义 diff 一致
5. 示例实测通过
6. 1 个 commit

---

## 7. Phase 6 — Retrospective & Methodology

### 7.1 定位

本次工作是一个"典型的项目整理操作 + 长工作流 AI 实践样本"。Phase 6 产出**两份文档**，服务不同读者：

### 7.2 产出 A — flog 专属复盘

`docs/superpowers/retrospectives/2026-04-22-flog-cleanup-retrospective.md`

**读者**：未来改 flog 的人。

**内容骨架**：
1. Context 切片：开工前项目状态快照（测试数、clippy warning 数、超标文件清单）+ 开工动机（用户原始请求）
2. Brainstorming 轨迹：核心决策表 + 关键转折（本次对话中的修正：清理→redesign / pin-bug 陷阱 / 500 机械→黄红 / 文档漏 README / 案例研究要拆出来）
3. Phase 执行轨迹：每个 phase 入口 / 摘要 / 意外发现 / 退出 / 耗时
4. Audit 统计：A/B/C/D/E 各类最终数 + B 类修复前后对照 + D 类 top 5
5. 有效的做法
6. 坑 / 反面教材
7. 未来建议

### 7.3 产出 B — AI 长工作流案例研究

`docs/superpowers/methodology/ai-long-workflow-case-study.md`

**读者**：未来想用 AI 做大规模项目整理的人，flog 只是载体。

**内容骨架**：
1. 适用 / 不适用场景画像
2. 核心方法论：
   - 六阶段模型（Audit → Mechanical → Characterization → Redesign → Comments → Docs + Retrospective）每阶段不可替代作用
   - Audit 五分类（A/B/C/D/E）为什么穷尽、为什么互斥
   - Characterization + ignored-B-test 的 pin-bug 陷阱及其解法
   - "行为冻结 + 架构重做"的分工（A/D 守护行为，TDD 实现新抽象）
   - subagent 并行 vs 串行判据
3. Spec 与 Plan 生产线（brainstorming → spec → plans → subagent）
4. 多 subagent 协作模式（worktree 隔离 / 可追溯证据 / agent 类型选择 / prompt 模板）
5. 人机协作边界（哪些决策人必须做 / 哪些 AI 做 / brainstorming 阶段 AI 的催化剂职责 / 人的关键修正案例）
6. 验收 & 门槛设计
7. 时间线与成本观察（token / 日历时间 / phase 耗时占比 / 超时节约原因）
8. 反面教材 / 不要这样做
9. 可直接复用的模板集（Audit 报告 / Step 设计文档 / Journal 条目 / Subagent prompt 基础模板）

### 7.4 关系

- A 是原材料（事实）
- B 是方法论（抽象）
- B 引用 A 的具体数据作为论据，不允许空谈
- 同一个 subagent 写，先 A 后 B
- 写 B 时必须回溯 A 里每一条"有效做法 / 反面教材"，无法泛化的条目只留 A 不进 B

### 7.5 支撑资料 — Journal

前 5 个 phase 每个结束时同步写 `docs/superpowers/journal/phase-N.md`：原始日志（做了什么 / 遇到什么 / 怎么解决的），粗糙、按时间顺序、不修饰。Phase 6 的原材料。

**特别地，Phase 0**：立即写 `docs/superpowers/journal/phase-0-brainstorming.md`，把本次 brainstorming 对话的决策轨迹原样记下（修正前的版本 + 用户修正 + 修正后的版本）。

### 7.6 Phase 6 验收门槛

1. retrospective 文档 + methodology 文档 + 所有 `journal/phase-*.md` 齐全
2. 每个 phase journal 含：入口时间 / 退出时间 / 关键决策 / 意外发现
3. retrospective "有效做法" + "反面教材" 加起来 ≥ 10 条具体条目（不允许空话）
4. methodology 每一节都引用 retrospective 的具体事件 / 数据
5. 1 个 commit

---

## 8. 总体交付清单

### 代码侧

- `src/` 全部文件符合行数规则 + clippy 零 warning + 无死代码
- `flog_dart/lib/` 同上
- `cargo test` 全绿，测试数 217 → 预计 400+
- 覆盖率工具集成，命令进 `CONTRIBUTING.md`
- 所有 Audit B 类 bug 已修

### 文档侧（根目录）

- 新增：`ARCHITECTURE.md` / `MODULES.md` / `CONTRIBUTING.md` / `PROTOCOL.md`
- 更新：`README.md` / `README_EN.md` / `CLAUDE.md` / `flog_dart/README.md` / `flog_dart/CHANGELOG.md`

### 过程资产（`docs/superpowers/`）

- `specs/2026-04-22-project-cleanup-design.md`（本文件）
- `audit/00-index.md` + `01-transport.md` + `02-domain.md` + `03-ui.md` + `04-flog-dart.md`
- `plans/YYYY-MM-DD-phase3-stepN-*.md`
- `journal/phase-0-brainstorming.md` + `phase-1.md` ... `phase-5.md`
- `retrospectives/2026-04-22-flog-cleanup-retrospective.md`
- `methodology/ai-long-workflow-case-study.md`

### Git 历史

6 个 phase commit：

```
chore(audit): Phase 1 — audit reports (4 subagents, A/B/C/D/E classified)
refactor: Phase 2 — mechanical cleanup, clippy zero-warning
test: Phase 2.5 — characterization tests + coverage baseline
refactor: Phase 3 — architecture redesign & bug fixes (all B-class resolved)
docs: Phase 4 — why-comments on final code
docs: Phase 5 — architecture / modules / contributing / protocol + README updates
docs: Phase 6 — retrospective + AI long-workflow methodology
```

---

## 9. 开工前准备（Phase 1 开始前必做）

1. **覆盖率工具定型**：选 `cargo-llvm-cov` 或 `cargo-tarpaulin`，跑当前 217 测试的覆盖率基线，记录。不定基线 → Phase 2.5 的 70% 门槛是空话
2. **`flog_dart/test/` 未跟踪**：确认用户意图 —— 加进 git 还是保留本地。影响 Phase 1 Agent 4 的范围
3. **worktree 清理策略**：约定每个 subagent 结束后 `ExitWorktree(remove)` 或主协作者统一管理
4. **Phase 0 journal 立即落**：`docs/superpowers/journal/phase-0-brainstorming.md`
5. **基线数据快照**：开工前 snapshot：代码行数 / 测试数 / clippy warning 数 / 超标文件清单 / 当前日期戳 —— retrospective 要用

---

## 10. 风险登记

| 风险 | 应对 |
|---|---|
| Audit 发现 B 类 bug 数量远超预期 | 分批：Phase 3 每 step 只处理本 scope 的 B；严重的单独列子 step |
| Phase 3 某次重构在测试通过下改坏未测边缘行为 | 接受。覆盖率门槛已是最大风险投入。剩下靠用户实测 + Phase 3.10 收尾 |
| UI 逻辑抽不出纯函数（耦合太深） | 退路：`TestBackend` snapshot 兜底 + 加入 Audit D 类 |
| 计划跑到一半改方向 | 每个 phase 入口 short update，随时叫停或调整 |
| Audit 分类判断有误 | 产出后用户全读一遍；C 类必须用户裁决 |
| Phase 3 串行过慢、用户想并行 | 3.1-3.4 依赖图独立，可以在极端情况下 fallback 并行，但代价是边界风格一致性 |

---

## 11. 工作节奏

- 每个 phase 入口 / 出口都与用户对齐后再继续，不一口气跑完
- 用户可在任何 phase 之间插入新需求或改方向
- 每个 phase 出口除了"与用户对齐"外，强制动作：subagent 写该 phase 的 journal
- 每个 phase 的 commit 允许回滚 —— 最小损失单位是一个 phase

---

## 附录 A — Audit 报告条目模板

```yaml
id: <SCOPE>-<N>
label: A | B | C | D | E
location: <file>:<line-range>
title: <一行概述>
evidence: |
  <3-10 行代码引用>
  <观察到的行为>
proposed_action: |
  <A/D: 重新设计思路>
  <B: 期望行为>
  <C: 待确认问题>
  <E: 具体修法>
risk: low | medium | high
```

## 附录 B — Phase 3 Step 设计文档模板

```markdown
# Phase 3 Step N — <module>

## 旧设计问题
- <Audit D-xxx: ...>
- <Audit A-xxx: ...>

## 新设计思路
<几段话>

## 迁移策略
<先动什么,后动什么,如何保持行为不变>

## 涉及的 Audit 条目
- A: [...]
- B: [...]
- D: [...]

## 新模块清单(每个一句话职责)
- `<path>`: <职责>

## 新增测试清单
- <test name>: <测什么合约>

## 为什么可以不拆(仅黄灯文件)
<文件名 + 理由>
```

## 附录 C — Journal 条目模板

```markdown
# Phase N Journal

## 入口
- 时间: YYYY-MM-DD HH:MM
- 状态: <前一 phase 出口快照>

## 时间线
- HH:MM <做了什么>
- HH:MM <遇到什么>
- HH:MM <怎么解决的>

## 意外发现
- <...>

## 出口
- 时间: YYYY-MM-DD HH:MM
- 状态: <该 phase 验收门槛是否全部达成>
- 移交下一 phase 的事项: <...>
```
