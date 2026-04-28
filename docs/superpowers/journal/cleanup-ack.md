# Cleanup acknowledgements — 战役收尾补丁

**日期：** 2026-04-28
**触发：** 战役关闭后（`campaign-closed` tag、v0.5.0 发布）的尾巴审查发现了 4 条审计 ID
既没在任何 journal 里被显式 ack，也没在代码里被处理。本文逐条定性，并更新
`audit/00-index.md` 的 Summary counts 以反映 UI-042 和 DART-033 这两条增补。
这是文档对齐补丁，**没有代码改动**。

## 1. 4 条未显式 ack 的 ID

### UI-021 — `mock_rules.rs` 规则显示与编辑混合（D 类）

**位置：** `src/ui/network/mock_rules.rs`（当前 493 行）

**状况：** 文件在 500 行预算之内，本身不触发文件大小闸门。表面"列表"和"编辑器"职
责交织在同一个文件里，但代码层已经干净：

- 入口 `pub fn draw_mock_rules_panel` 渲染列表
- 编辑器由 `text_editor` 组件承担（Phase 3 Step 3.9 已提取）

Phase 3 没把这个拆进独立文件，是因为拆完两边都会落在 250 行以下 —— 代价是
多一层模块树，收益是零。

**处置：** **不修复。** 保持现状；如果以后加新功能让文件涨到 600+，再拆。

### UI-025 — 三处 JSON viewer 状态无统一所有权（A 类）

**位置：** `src/ui/network/detail.rs`、`src/ui/logs/detail/mod.rs`、`src/ui/json_viewer/`

**状况：** Logs 一侧 `DetailState` 带 `viewer_text_fingerprint` 字段，在内容变化时
重建 state；Network 一侧 `json_viewer_states: HashMap<section_key, JsonViewerState>`
没有 fingerprint，切换 entry 时理论上 stale state 可能错位。

**和 UI-011 的关系：** UI-011 是 UI-025 的下游具体表现（Network 面板的 fingerprint
缺失），Step 3.8 的计划里标了 UI-011 partial。这两条本质上是同一个 A 类架构味。

**触发概率：** 低。需要在详情面板打开的同时切换 Network entry，且 JSON 树结构恰好
让某个 node_id 错位到另一个语义节点 —— 实际使用中很少见。

**处置：** **延到 v0.6**，和 UI-011 合并成一个 `JsonViewerPane` 抽象。不值得为 A
类重开一次战役。

### UI-033 — `input_field.rs` 魔术 padding/宽度计算（D 类）

**位置：** `src/ui/input_field/mod.rs`（267 行，已拆）

**状况：** 几处内联算术（`total_pad / 2`、`per + (if rem > 0 { 1 } else { 0 })`）
没抽成命名常量。Phase 3 Step 3.9 做了 UI-015（palette 解耦）但没顺手处理这条。

**处置：** **不修复。** 低信号，改起来一行常量声明，拖一个 commit 不值得。后人
在这个文件里下次动手时顺带做更合理。

### UI-039 — `show_status` toast 无统一 manager（A 类）

**位置：** `src/app/mod.rs`（`show_status` + `status_message` 字段）

**状况：** `status_message: Option<(String, u64)>` 是个裸 tuple，多个调用方
（clipboard 复制、export、mock 添加、replay 触发）都通过 `app.show_status(msg)`
写它。行为正确，只是没抽象成 `ToastManager { stack: Vec<Toast>, ... }`。

**处置：** **不修复。** A 类的典型 —— correct but ugly，无 bug，无闸门触发。
如果未来要加 toast 堆叠或按优先级排序再动。

## 2. 审计索引 Summary counts 更新

`audit/00-index.md` 曾在正文表里记录 115 条，增补 UI-042 + DART-033 之后在附
注里说明但没同步更新表格。现在同步：

```
before (表格):  A:27  B:13  C:0  D:66  E:9  Total:115
after  (表格):  A:27  B:14  C:0  D:67  E:9  Total:117
```

（UI-042 是 B，DART-033 是 D，总数 +2。）

## 3. TRANS-100..105 状态复核

之前尾巴审查怀疑这 6 条 ID 没 ack。复核结果：
- `phase-2.5b.md` §30 `TRANS-100..105 new audit entries`
- `phase3-step3.md` §206 `TRANS-100..105 inherited 2.5B b3f163f D (ack only)`
- `phase3-step10.md` §155 `TRANS-100..105 ack rows`

**已完成，不是遗漏** —— 我之前的脚本用纯 ID 字面匹配，错过了 `TRANS-100..105`
这种范围写法。

## 4. TRANS-016 / TRANS-017（`flutter_logs.rs`）状态复核

Phase 5 写 `MODULES.md` 时发现 `src/transport/flutter_logs.rs` 没被任何地方
引用（`mod.rs` 里 `pub mod` 缺失，0 call site）。作为 Phase 5 红线的延期项记
录进 `retrospective-flog.md §8`。

DART-033 战役开跑前，顺手一个 commit（`4786a5f chore(transport): remove
unreferenced flutter_logs.rs (TRANS-016/017)`）把它删掉了。`retrospective-flog.md §8`
已经在中文译本里加了补注说明此事。

**状态：closed。**

## 5. 战役 token 消耗

出自 `~/.claude/projects/-Users-shaomingqing-FlutterProject-flog/` 下 211 份
`.jsonl` 会话文件（207 份带 usage 元数据）。战役窗口从 2026-04-22 开始。

| 指标                      | 战役期（04-22 起）   | 项目全生命周期        |
|---------------------------|---------------------|----------------------|
| 输入（非缓存）            | 959,288             | 4,678,565            |
| 缓存写入（cache creation）| 70,173,258          | 164,308,820          |
| 缓存读取（cache read）    | 1,221,185,567       | 3,268,145,957        |
| 输出                      | 3,243,777           | 6,421,868            |
| **原始总和**              | **1,295,561,890**   | **3,443,555,210**    |
| 计费等效（cache_read ÷ 10）| ~196,494,879       | ~502,223,848         |

**观察：**
- 战役期消耗约 **1.30B raw tokens**，按 Anthropic cache read 折扣（0.1×）后约
  **196M billable tokens**。
- **cache read 占 94.3%**。长工作流的关键经济性在此 —— 重复读 CLAUDE.md /
  audit 索引 / plan 文件都走缓存，否则成本要翻 10 倍。
- 输出仅 3.24M tokens，对比 162 个 commit + 2 份方法论文档 + 14 份其他文档
  + 大量代码改动，平均每 commit ≈ 20K 输出 token。
- 非缓存输入极低（959K），说明每一轮 subagent 派发的新 context 都被妥善
  cache 住了。

**对方法论文档 §6 成本核算的校准：** 原文估计"15-20 个 subagent 回合 +
20-30 小时用户注意力"，这个数量级是对的；但没给具体 token 数。本文数字应
作为"约 2 万行代码库，3 天清理战役"的参考基准，写入
`ai-long-workflow-methodology.md §6` 的下一次修订。

## 6. 出场

- ✅ `audit/00-index.md` 的"User confirms Phase 2"闸门已打钩
- ✅ Summary counts 已更新到 117 条（含 UI-042 + DART-033）
- ✅ 4 条未 ack 的 ID 定性完毕（都不修复）
- ✅ TRANS-100..105 确认已 ack（之前误报）
- ✅ TRANS-016/017 确认已闭合（flutter_logs.rs 已删）
- ✅ 战役 token 消耗有数据基线

**战役真正彻底关闭。**
