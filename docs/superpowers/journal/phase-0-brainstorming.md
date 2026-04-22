# Phase 0 Journal — Brainstorming

## 入口
- 时间：2026-04-22
- 状态：flog 项目功能基本完成，用户发起"项目健康化整理"需求
- 协作者：用户 + Claude Code（主 Claude）
- 使用 skill：`superpowers:brainstorming`

## 用户原始请求（原文保留）

> 现在整个项目都是使用 Claude 实现的，现在功能基本实现的差不多了，我想拆分成几个 subagent 去 review 各个部分的代码，找到一些有 bug，实现不优雅，架构不合理的地方，还有死代码，都进行整理清理，我希望职责清晰，架构合理，代码优雅整洁，方便维护，然后加一些注释，最后整理一个项目工程文档，和项目的 spec 规范，方便后续 ai 工作，我想要得到一个完美的项目

## 开工前项目基线数据快照

- Rust 源代码总行数：18469 行（`find src -name "*.rs"`）
- Dart 源代码总行数：1834 行（`find flog_dart/lib -name "*.dart"`）
- 超过 800 行的源文件（红灯）：
  - `src/event.rs` 1677
  - `src/ui/logs/mod.rs` 1358
  - `src/app.rs` 1167
  - `src/ui/network/detail.rs` 1109
  - `src/ui/source_select.rs` 898
- 500-800 行之间（黄灯）：
  - `src/ui/json_viewer/render.rs` 745
  - `src/ui/network/mod.rs` 700
  - `src/domain/structured_parser.rs` 693
  - `src/transport/device_monitor.rs` 654
  - `src/main.rs` 546
  - `flog_dart/lib/src/flog_dio.dart` 504
- 测试状态：`cargo test` 217 个单测 + 1 个集成测试全绿
- Clippy 状态：18 warnings + 1 error（test 编不过：`PI` 近似值）
- 已知死代码：`MockRuleStore::enabled_count`、`LogStore::clear`、`adb::is_available`、`UsbDevice`、`list_devices`
- 已存在文档：`README.md` / `README_EN.md` / `CLAUDE.md` / `flog_dart/README.md` / `flog_dart/CHANGELOG.md`
- `docs/superpowers/specs/` 14 份旧 spec，`docs/superpowers/plans/` 15 份旧 plan
- 未跟踪文件：`flog_062120.log`、`flog_dart/test/`
- 当前分支：master

## Brainstorming 核心决策轨迹

以下每一条按"Claude 提议 → 用户选择 / 修正 → 修正后的版本"三段式记录。本次对话的所有关键修正都保留在这里，是 Phase 6 retrospective & methodology 的原材料。

---

### 决策 1 — 本次 brainstorming 的产出形态

- **Claude 提议**：一份总 spec 覆盖 5 phase / 只做 Phase 1 bug hunt / 跳过 bug hunt 直接做清理+重构
- **用户选择**：一份总 spec 覆盖全部
- **为什么**：用户要全局视图，不想 Claude 跑偏

### 决策 2 — Phase 3 重构尺度

- **Claude 提议**：保守（只拆文件不改行为）/ 适度（拆 + 清理明显职责混乱）/ 激进（重新设计模块边界）
- **用户选择**：适度
- **为什么**：保守流于形式，激进风险过高

### 决策 3 — Bug hunt 的 subagent 分工

- **Claude 提议**：4 块可选（Transport / Domain / UI / flog_dart）
- **用户选择**：**四块全要**，并明确附加期望："我都比较担心，我希望重构玩功能完整，且职责模块清晰"
- **影响**：这句附加期望变成了后续 Phase 3 redesign 的北极星

### 决策 4 — Phase 5 文档产出

- **Claude 提议**：ARCHITECTURE / MODULES / CONTRIBUTING / PROTOCOL 四选
- **用户选择**：**四份全要**
- **为什么**：这些文档服务不同读者层次，缺一不可

### 决策 5 — Commit 策略

- **Claude 提议**：每 phase 一个 commit / 每子任务一个 commit / 整体一个 branch 最后 squash
- **用户选择**：每 phase 一个 commit
- **为什么**：粒度适中，出错时最小损失单位是一个 phase

---

### ⚠ 关键修正 A — "清理"这个词错了

这是本次 brainstorming 第一个大修正。

- **Claude 最初版本**：Phase 2 = "clippy / 死代码 / 魔法值清理"；Phase 3 = "拆巨型文件"。把"清理"和"重构"机械切分
- **用户修正**（原文保留）：

  > 我看写的是发现 bug，整理代码，但是我想要的是发现 bug，然后优化代码，很多地方不仅仅是清理能够解决的，可能在架构设计上就有问题了，需要重新上设计实现，我想要的是优雅美观的代码，不是能跑就行，我有代码洁癖，你懂吗

- **Claude 意识到的问题**：
  - "清理"这个词暗示小修小补，但用户要的是"发现问题 → 重新设计"
  - 魔法值背后往往是**抽象缺失**（不是提常量，是提类型）
  - 补丁代码背后往往是**状态机设计错误**
  - 这些不是"清理"能解决的
- **修正后的版本**：
  - Phase 2 重命名为 **Mechanical cleanup**，只做真正 0 风险 0 设计判断的事（clippy 等价改写 / 死代码删 / 格式化 / Default 补全）
  - Phase 3 重定位为 **Redesign & Rebuild**：明确授权"只要 Audit 报告把某处归入架构瑕疵 / 抽象缺失 / 补丁痕迹 / 设计错位，Phase 3 的 subagent 有权限重新设计它，而不只是搬运"
  - Audit 分类从三类（A/B/C）扩展为五类（A/B/C/D/E），新增 D = 架构瑕疵、E = 机械
  - Phase 2 和 Phase 3 的边界从"清理 vs 拆文件"改为"是否需要设计判断"

### ⚠ 关键修正 B — Characterization test 的 pin-bug 陷阱

- **Claude 最初版本**：Phase 2.5 把现有行为冻结为 characterization test → Phase 3 重构保持这些测试绿
- **用户修正**（原文保留）：

  > 哦对了，现在的里面可能也有 bug，之前说把现在的测试结果当作重构后的答案，我觉得是不是有点不妥

- **Claude 意识到的问题**：
  - Characterization testing 的经典陷阱："pinning down bugs"
  - 如果现有代码有 bug，把行为冻结会把 bug 也一起冻结
  - 之后想修 bug 还要先拆掉守护 bug 的测试
- **修正后的版本**：
  - Phase 2.5 按 Audit 分类区别对待：A/D 写绿色 characterization test（锁对的行为）；B 写**断言期望行为**的测试，写完是红的 → 标 `#[ignore = "bug: AUDIT-xxx, fix in Phase 3"]`
  - Phase 3 的验收门槛加一条：所有 B 类 ignored 测试必须解除 ignore 并变绿（= bug 修完）
  - 逻辑闭环：Characterization 冻结对的东西，TDD 修错的东西

### ⚠ 关键修正 C — 500 行硬上限不合理

- **Claude 最初版本**：Phase 3 硬规定"没有文件 > 500 行"
- **用户修正**（原文保留）：

  > 500 行的限制合理吗，我不知道，你觉得呢

- **Claude 反思**：
  - 500 是经验法则不是科学，来源是 Google / Airbnb 这类 style guide 的中位值
  - 当硬上限会产生假拆分（为过线而拆强内聚模块）和误伤合理大文件（大 match / 协议表 / 纯常量表）
- **修正后的版本**：
  - 行数是**信号，不是判决**
  - < 500 绿 / 500-800 黄要在 step 文档里解释 / > 800 红默认必拆（例外需白名单）
  - 最高原则：Audit 的设计判断高于数字

### ⚠ 关键修正 D — README 等现有文档也要更新

- **Claude 最初版本**：Phase 5 只规划新增 4 份文档
- **用户修正**（原文保留）：

  > 还有 README 等文档都需要做必要的更新

- **Claude 意识到的问题**：Phase 3 改完后代码事实变了，README / CLAUDE.md / flog_dart 文档会过时
- **修正后的版本**：
  - Phase 5 执行拆两波：第一波新增 4 份（A/B/C/D subagent 并行）；第二波更新 5 份现有文档（E/F subagent 在读完第一波 draft 后跟写）
  - 硬规矩：双语 README 保持一致、示例代码必须实测、无旧段落残留

### ⚠ 关键修正 E — 本次工作本身是 AI 长工作流样本

这是本次 brainstorming 最深的一层洞察，也是第二重要的修正。

- **Claude 最初版本**：Phase 6 定义为"flog 专属复盘"
- **用户修正**（原文保留）：

  > 本次的操作会是一个比较典型的项目整理操作，也是一个长工作流的 AI 实践，所以需要记录一下

- **Claude 意识到的问题**：
  - 用户真正的意图不只是"给 flog 留档"，而是"把这次 AI 长流程实践本身沉淀成可复用样本"
  - 两个目标服务不同读者 —— 不能混
  - flog 专属复盘读者 = 未来改 flog 的人
  - AI 长工作流案例研究读者 = 未来想用 AI 做类似工程的人
- **修正后的版本**：
  - Phase 6 产出两份文档：
    - A：`retrospectives/2026-04-22-flog-cleanup-retrospective.md`（事实原材料）
    - B：`methodology/ai-long-workflow-case-study.md`（方法论抽象）
  - B 必须引用 A 的具体数据和事件作为论据
  - 写作顺序：先 A 后 B，同一个 subagent 保证一致性
  - 本文件（phase-0-brainstorming.md）直接成为 Phase 6 A/B 两份文档的核心原材料之一

---

## 意外发现（超出原计划的东西）

1. **217 测试全绿但 clippy 编译失败**：`cargo test` 跑得过（测试二进制不用 `-D warnings`），但 `cargo clippy -- -D warnings` 因为 `PI` 近似值变成 error。日常可能没察觉到，CI 门槛加 `-D warnings` 立刻暴露
2. **docs/superpowers/ 目录结构早已存在**：6 个子目录（audit / journal / methodology / plans / retrospectives / specs）已建好。这意味着用户之前跑过类似流程，本次不是首次
3. **`flog_dart/test/` 未跟踪**：开工前 §9 要确认意图

## Phase 0 关键规矩沉淀（直接输出给 Phase 6 方法论）

以下条目是本次 brainstorming 中验证过的、值得进方法论文档的做法：

1. **brainstorming 一次一个问题，用多选而非开放式** —— 效率更高、用户决策更清晰
2. **scope 判断比细节规划优先**：在细化计划前先判断"这需求是不是一份 spec 能装下"，本次发现是 5 个子项目（后扩为 6）
3. **phase 拆分按"意图"而非"类型"**：最初按"改动类型"（清理 vs 拆文件）→ 失败 → 改按"是否需要设计判断"成功
4. **"行为冻结 + 架构重做"的分工机制**：A/D 类 characterization 守护行为不变，TDD 实现新抽象，B 类 ignored-test 作为修 bug 作业清单
5. **信号 vs 判决**：数字指标（500 行）当信号要解释，不当硬判决
6. **人的关键修正值得原文保留**：本次五处修正每处都让 spec 更准。如果只记"达成共识的最终版本"，会丢失"为什么是这个版本"的因果链
7. **brainstorming 的 AI 职责是催化剂**：提出框架、指出盲点、强制选择；不是单向执行用户指令
8. **subagent 分工边界要在 spec 里写死**：临开工临时分容易切错
9. **worktree 隔离 + 原材料归档**：并行 subagent 的证据沉淀机制
10. **Phase 0 journal 必须立即写**：不立即写，开跑后记忆会淡，人的修正路径会丢失

## 出口
- 时间：2026-04-22
- 状态：
  - Spec 已落盘 `docs/superpowers/specs/2026-04-22-project-cleanup-design.md`
  - Phase 0 journal 本文件
  - 待用户 review spec
  - 待 spec self-review（placeholder / 矛盾 / 歧义 / scope 四项）
- 移交下一步：spec self-review → 用户 review → 交付 `writing-plans` skill 产出 Phase 1 可执行 plan
