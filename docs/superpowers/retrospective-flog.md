# flog 清理战役 —— 复盘

**战役窗口：** 2026-04-22 → 2026-04-24（三个日历日，六个阶段，单一工作
分支 `master`）。
**仓库：** https://github.com/shaomingqing/flog —— 面向 Flutter 开发者
的终端原生日志查看器 + 网络检查器（Rust TUI），加一个发布在 pub.dev
的 Dart 伴生包 `flog_dart`。
**基本规则：** 每个阶段都有一个独立的出口 journal commit；下文每一条
论断都可以追溯到 `docs/superpowers/{audit,plans,journal}/` 某个文件，
或者 `git log`。

这份文档是 flog 项目专属的复盘。可推广的"这套模式是怎么运转起来的"
放在旁边的 `docs/superpowers/ai-long-workflow-methodology.md`。

## 1. 起点状态（2026-04-22，HEAD `5888264`）

快照记录在 `docs/superpowers/audit/.baseline.md`，是 Phase 1 审计开跑
之前的状态。

| 指标                                    | 入场值 |
|----------------------------------------|----------------|
| `git log` 总数（整个项目生命周期）       | 262 commits    |
| `cargo test`（lib + integration）       | 217 unit + 1 integration（218 绿）|
| `cargo clippy --all-targets -D warn`   | **失败** —— 1 个 error + 约 18 个 warning |
| `cargo fmt --check`                    | 干净          |
| `src/**/*.rs` 下的 Rust 行数            | 18 469         |
| `flog_dart/lib/**` 下的 Dart 行数       | 1 834          |
| 超过 800 行的文件（红）                  | **5 个** —— `event.rs` 1677, `ui/logs/mod.rs` 1358, `app.rs` 1167, `ui/network/detail.rs` 1109, `ui/source_select.rs` 898 |
| 500–800 行的文件（黄）                  | **6 个** —— `ui/json_viewer/render.rs`, `ui/network/mod.rs`, `domain/structured_parser.rs`, `transport/device_monitor.rs`, `main.rs`, `flog_dart/lib/src/flog_dio.dart` |
| 超 500 行的文件总数                      | **11**         |
| 工程文档（`docs/*.md`）                  | 0 —— 只有根目录的 README、README_EN、CLAUDE.md，和两份 flog_dart 文档 |
| 已知 bug 数                              | 未知 —— 还没跑过审计 |

入场的测试数是 "before" 快照里的关键：2 万行 Rust + 1.8 千行 Dart 只
有 217 条测试，按我们后来用的工具测是约 30% 覆盖率（`cargo llvm-cov`
在 Phase 2.5B Task 0 测出入场基线是 31.48% line / 31.30% region）。

## 2. 结束状态（2026-04-24，HEAD = Phase 6 journal commit）

| 指标                                    | 出场值         | 相对入场的 Δ   |
|----------------------------------------|----------------|----------------|
| 战役 commit 总数                        | 159 + 本文 3 = **162** | 不适用（见 §10）|
| `cargo test --all`                     | **2 166 绿，0 失败，0 ignored**（12 个测试二进制）| +1 948 |
| `flutter test`（flog_dart）            | **133 pass, 2 skip, 0 fail** | 从 84 pass / 3 skip / 1 fail（DART-001/002 编译期红）|
| `cargo clippy --all-targets -D warn`   | 干净          | 已修复         |
| `cargo fmt --check`                    | 干净          | 不变           |
| `src/**/*.rs` 下的 Rust 行数            | 28 050         | +9 581（主要是兄弟文件 `_tests.rs`，加上拆分的样板代码）|
| `flog_dart/lib/**` 下的 Dart 行数       | 2 233          | +399           |
| **超 500 行的生产文件**                  | **0**          | −11            |
| 包含 `_tests.rs` 在内超 500 行的文件     | 3              | 兄弟文件测试模块按设计豁免预算（见 CONTRIBUTING）|
| 行覆盖率（`cargo llvm-cov`）            | **90.54%**     | +59.06pp       |
| Region 覆盖率                           | 89.75%         | +58.45pp       |
| Function 覆盖率                         | 93.18%         | +43.69pp       |
| 工程文档（`docs/*.md`）                  | 4              | +4 —— `ARCHITECTURE.md`、`MODULES.md`、`PROTOCOL.md`、`CONTRIBUTING.md` |
| 更新的用户文档                           | 5              | `README.md`、`README_EN.md`、`CLAUDE.md`、`flog_dart/README.md`、`flog_dart/CHANGELOG.md` |
| 审计纸质追溯                             | 4 份 scope 文件 + 1 份索引 + 19 份 journal + 32 份 plan + 15 份 spec | 全新 |

覆盖率按模块分别记录 —— 每个热点模块都过了 Rule 2 的闸门，只有 4 个
被标记为 PHYS/D-ref 的缺口（`main.rs` 45.70%，`event.rs` 61.02%，
`transport/adb.rs` 49.18%，`transport/usbmuxd.rs` 73.02%；`replay.rs`
2.43%，作为 D-ref TRANS-013 归档）。完整的按模块表见
`docs/superpowers/journal/phase-2.5b.md`。

## 3. 六个阶段一览

每阶段的 commit 数是从上一阶段 journal 到当前阶段 journal 之间（含当
前 journal）的 `git log --oneline` 记录。

### Phase 0 —— 头脑风暴 & 划定范围（`f3b2a12`，1 个 commit）
产出 `specs/2026-04-22-project-cleanup-design.md`（667 行：六阶段路线
图、A/B/C/D/E 审计分类、500 行文件预算、测试密度/可观察性 Rules
1-11、`flog_dart` 发布流程规则）。还有
`journal/phase-0-brainstorming.md`（决策轨迹 —— 为什么是 500 行而不是
800，为什么 C 类必须在 Phase 2 之前解决，等等）。

### Phase 1 —— 审计（`5888264` 到 `a243f76`，4 个 commit）
4 个只读 subagent 并行跑 `transport/`、`domain/`、`ui/`、`flog_dart/`。
每个产出一份审计 markdown，每条发现都打 A/B/C/D/E 标签。汇总进
`audit/00-index.md`，Phase 2 前有一个用户闸门。初始 115 条发现（27 A，
13 B，0 C —— 全部在 Task 3 里被 reclassify 成 A/B/D/E，66 D，9 E）。
后续又增补了 DOM-025、UI-041、UI-042、DART-033，以及 Phase 2.5B 中抽
取出的新纯函数 TRANS-100..105。

### Phase 2 —— 机械清理（`dea1190` 到 `1c81e1e`，2 个 commit）
一次 subagent 跑完 9 条 E 类发现，修掉 clippy 的 error + warning，清
掉死代码（`LogStore::clear`、`adb::is_available`、`UsbDevice`），加上
clippy 要求的 `#[derive(Default)]` / `impl Default`。出场：clippy 0
warning，fmt 干净，测试数不变。

### Phase 2.5A —— 逻辑/渲染分离（`d95acd6` 到 `2322b62`，8 个 commit）
"可测性阶段"。从渲染器里抽出纯函数，这样渲染逻辑就可以不走
`TestBackend` 做单元测试：`compute_visible_entry_start`、
`entry_row_count`、`repeat_bar_normalized`、
`compute_visible_network_range`、`handle_sse_field_navigation`。过程
中发现了 UI-041：normal-mode 鼠标 handler 没法不做 Phase 3 重设计就
纯抽出来（作为增补记录进审计索引，直到 Phase 3 Step 3.6 解决之前
event.rs 覆盖率卡在 61%）。

### Phase 2.5B —— 特征化测试围栏（`0710387` 到 `8713a72`，16 个 commit）
整个战役里最大的一笔投入：搭一道让 Phase 3 安全的回归围栏。14 个 task
commit 通过 subagent 派发（大多数是串行 —— 并行尝试撞上了 worktree
锁问题）。交付物：
- `tests/support/{ui_inspect,fake_flog_server,fixtures,mod}.rs` ——
  共享测试脚手架（fake server 脚本的 `Behavior` enum、TUI 行匹配器、
  工厂函数）
- 8 个新的 integration crate，在 `tests/characterization_*.rs`
- 净增 +1 525 条测试（414 → 1 939 绿，加 3 条 `#[ignore = "bug: <id>"]`
  红测试用来锁 DOM-003 + DOM-018 × 2）
- 覆盖率从 31.48% 跳到 90.54%（行）

### Phase 3 —— 重新设计（`38cc1b9` 到 `8fe941e`，98 个 commit，10 个 step）
最长的一个阶段，也是吃 token 最多的一个阶段。每个 step 都是独立的
plan + subagent 派发 + journal。Step 3.1 到 3.10：

| Step | 范围 | 关闭的审计 ID | commit 数 |
|------|-------|------------------|---------|
| 3.1  | parser/ | DOM-013/015/016/017 + LazyLock ack | 6 |
| 3.2  | domain/ | DOM-001/002/003/005/006/008/011/018/019/024/025（含 2 个 B 修复解锁 DOM-003 + DOM-018）| 11 |
| 3.3  | transport/ | TRANS-002/004/005/006/008/009/014 + A 类 ack | 10 |
| 3.4  | flog_dart | DART-001..009（全部 B），DART-010..027（D）| 16 |
| 3.5  | app 状态 | UI-002/004/006/017/026/028/034/040 | 8 |
| 3.6  | 事件派发 | UI-001/007/008/009/016/041 + UI-042 红锁 | 10 |
| 3.7  | ui/logs | UI-010（拆分）、UI-013、UI-014 | 8 |
| 3.8  | ui/network + UI-042 修复 | UI-037（detail 拆分）、UI-010 镜像、UI-042 B 修复 | 7 |
| 3.9  | ui/shared | UI-012（source_select → device_picker 改名 + 拆）、UI-014、UI-015、UI-030、UI-031、UI-038 | 8 |
| 3.10 | 横切 | UI-003（部分 —— LogsViewState 骨架）、UI-036（测试模块兄弟文件抽取）| 14 |

Phase 3 的 commit 数（98）远超其他阶段，是因为每个 step 都把工作拆
到 5–15 个中间 commit，确保单个 subagent 回合不超出运行时事件预算看
门狗（约 15–20 分钟）。

### Phase 4 —— 残余拆分 + "为什么"注释（`056d664` 到 `49102df`，6 个 commit）
扫尾 Phase 3 出场时还超 500 行的 3 个文件（`app.rs` 1506、
`device_monitor.rs` 743、`main.rs` 564）+ 完成 UI-003 LogsViewState
迁移（239 个调用点）+ 在指定 10 个热点加 "why" 注释 + 删掉过时的
TODO。出场：每个生产 Rust 文件 ≤ 500 行。

### Phase 5 —— 文档（`7aaed95` 到 `1b2cbdd`，8 个 commit）
4 份新工程文档（`ARCHITECTURE.md` 600 行、`MODULES.md` 842 行、
`PROTOCOL.md` 433 行、`CONTRIBUTING.md` 339 行）+ 刷新 5 份已有文档。
关闭 DART-024（README 缺）+ DART-025（CHANGELOG 回填）。加了
`docs/superpowers/README.md` 作为审计纸质追溯的索引。没有代码改动，
测试数不变。

### Phase 6 —— 复盘 + 方法论（当前阶段，3 个 commit）
产出本文 + `ai-long-workflow-methodology.md` + `journal/phase6.md`。
无代码改动，测试数不变。

## 4. bug 清单 —— 13 条 B 类条目

审计的 B 类定义是"已确认的 bug，一旦触发用户可观察"。Phase 2.5B 给
每条写了一个 `#[ignore = "bug: <id>"]` 的红测试；Phase 3 在修复的同
一个 commit 里取消 ignore。下面按 `audit/00-index.md` 的严重度排序。

| ID         | 严重度 | 表面 | 红测试种下 | 修复 commit | 谁发现的 |
|------------|----------|-------------------------------------------------|------------------|------------|---------------|
| DOM-003    | HIGH     | 没有前置 request 的 HTTP response 被静默丢弃 | Phase 2.5B Task 12 `218e78f`（`#[ignore]`）| Phase 3 Step 3.2 `7e333a1` | 审计 subagent（02-domain）|
| DART-001   | HIGH     | SSE parser 在第一条 `data:` 后丢弃后续事件 | Phase 1（预先存在的 `flog_dart/test/`）| Phase 3 Step 3.4 `6179631` | 审计 subagent（04-flog-dart）|
| DART-002   | HIGH     | `FlogSseParser.wrapTyped` + `SseEvent` 被测试引用但 `lib/` 里不存在 | Phase 1（同一个文件 —— 编译 error）| Phase 3 Step 3.4 `6179631` | 审计 subagent（04-flog-dart）|
| DOM-018 (a)| MEDIUM   | `search_positions()` 在 OR term 重叠时会返回重叠区间 | Phase 2.5B Task 12 `218e78f` | Phase 3 Step 3.2 `3a4d9c1` | 审计 subagent（02-domain）|
| DOM-018 (b)| MEDIUM   | （第二个 ignored 用例 —— plain mode OR 重叠合并）| Phase 2.5B Task 12 `218e78f` | Phase 3 Step 3.2 `3a4d9c1` | 同上 |
| DART-004   | MEDIUM   | `flogEnabled=false` 时 `FlogMockInterceptor.onRequest` 仍然跑 | Phase 2.5B Task 13 `51320b7` | Phase 3 Step 3.4 `b0f1e55` | 审计 subagent（04-flog-dart）|
| DART-006   | MEDIUM   | `FlogWebSocket.stream` 文档说是 broadcast，实际是 single-subscription | Phase 2.5B Task 13 `51320b7` | Phase 3 Step 3.4 `c70d6f0` | 审计 subagent（04-flog-dart）|
| DART-008   | MEDIUM   | 前置 interceptor 短路时 `_idMap`/`_startMap` 泄漏 | Phase 2.5B Task 13 `51320b7` | Phase 3 Step 3.4 `77eabf8` | 审计 subagent（04-flog-dart）|
| TRANS-007  | LOW      | `tcp_open` 用 `Ok(Ok(_))` 模式 —— 对但脆弱 | Phase 2.5B Task 11 `b3f163f`（绿色，审计说"对但脆弱"）| Phase 3 Step 3.3 `65d6ab3`（is_port_open helper）| 审计 subagent（01-transport）|
| DART-003   | LOW      | 库 dartdoc 引用了不存在的顶层 `flog()` | Phase 2.5B Task 13 | Phase 3 Step 3.4 `ff4a710` | 审计 subagent（04-flog-dart）|
| DART-005   | LOW      | `ext.flog.syncMockRules` VM Service 扩展只有文档没注册 | Phase 2.5B Task 13 | Phase 3 Step 3.4 `804ebc0`（仅 doc）| 审计 subagent（04-flog-dart）|
| DART-007   | LOW      | `_truncate` 按字符数比字节预算（CJK UTF-8 会被截坏）| Phase 2.5B Task 13 | Phase 3 Step 3.4 `e09805d` | 审计 subagent（04-flog-dart）|
| DART-009   | LOW      | `emitNet` 修改调用方的 map | Phase 2.5B Task 13 | Phase 3 Step 3.4 `d72ceed` | 审计 subagent（04-flog-dart）|
| **UI-042** | MEDIUM（增补）| WS chat ↔ raw 切换泄漏 collapse-key 状态，污染相邻面板渲染 | Phase 3 Step 3.6 `95f97d7`（红锁）| Phase 3 Step 3.8 `133b631` | **用户**，战役中期（2026-04-24）|

**B 类 bug 共 14 条** —— 比审计最初报告的 13 条多一条，因为 UI-042
是中途作为增补补录进来的。

值得一提的处置：
- 审计把 **TRANS-007** 标成了"对但脆弱" —— 所以 Phase 2.5B 写的测试
  是绿色的，不是红的。Phase 3 的修复抽出 `is_port_open()` 给模式起
  了个名字，原来的绿测试继续锁住行为。
- **DART-002** 比较特殊：审计发现 `flog_dart/test/`（git 里没追踪，
  在用户的工作区里）引用了 `lib/` 里不存在的 API。Phase 1 Task 5 把
  测试文件原样提交，让它成了权威的 spec；Phase 3 Step 3.4 实现了
  `SseEvent` + `wrapTyped` 把编译失败变成绿色。这是"事后 TDD" ——
  测试是之前某次会话写的，反过来塑造了这次的实现。

## 5. 架构变化

重要的前后形态变化。大小按 `wc -l` 给出。

| 条目 | 改前 | 改后 |
|--------------------------------------------|---------------------|------------------------------------------------|
| `src/event.rs` —— 单文件 TUI 派发 | 1 677 行 | `src/event/` —— 10 个文件，最大 495（apply.rs）|
| `src/app.rs` —— App struct + 状态机 | 1 506 行 | `src/app/` —— 11 个文件，最大 484（mod.rs）|
| `src/ui/logs/mod.rs` —— Logs 视图 | 1 358 行 | `src/ui/logs/` —— 拆成 toolbar/list/status_bar/empty_states/highlight/timeline/stats/detail/{mod,renderers,section}/jump |
| `src/ui/network/detail.rs` —— Network 详情 | 1 109 行 | `src/ui/network/detail/` —— 7 个文件（mod.rs 277、shared.rs 250、general.rs 95、http_body.rs 131、sse.rs 260、ws.rs 284、error.rs 44）|
| `src/ui/source_select.rs` | 898 行 | 改名为 `src/ui/device_picker/` —— 6 个文件（mod.rs 230、card.rs 352、row.rs 263、modal.rs 99、click_map.rs 119、palette.rs 16）|
| `src/transport/device_monitor.rs` | 743 行（Phase 3 出场时）| `src/transport/device_monitor/` —— 4 个文件（mod.rs 146、adb_source.rs 196、usbmuxd_source.rs 199、local_source.rs 197）|
| `src/ui/help.rs` | 534 行 | `src/ui/help/` —— mod.rs 278、content/{logs.rs 202, network.rs 119, mod.rs 8} |
| `src/main.rs` | 564 行 | `src/main.rs` 93 行 + `src/run/` —— dispatch.rs 140、server.rs 297、render_loop.rs 76 |
| `src/domain/structured_parser.rs` | 693 行 | `src/domain/structured_parser.rs` + `src/domain/json_tolerant.rs`（按 DOM-008 拆分）|
| `src/domain/network.rs` —— `FlogNetMessage` 松散 struct | — | `FlogNetKind` 强类型 enum（`#[serde(tag = "t")]`）—— DOM-002 + DOM-006 |

几个重设计不是单纯拆分，还引入了新抽象：
- `FilterVariant` trait（`domain/network_filter.rs`）统一了此前重复的
  3 个 filter enum（DOM-001）。
- `MessageFilter` trait + `FilterState` 封装（DOM-005 + DOM-019）。
- `NetworkEntry` builder 模式替掉了 factory 样板（DOM-024）。
- `ClickRegion` enum（`event/click_region.rs`）+ 两阶段
  `detect_click_region` / `apply_click_region` 拆分 —— "为可测性设计"
  的闪光时刻，解锁了 UI-041（审计原本标为"在当前形式下无法被纯函数
  化测试"）。
- `MockEditState` bundle 收拢了散落在 `App` 上的 `mock_edit_*` 字段
  （UI-026 + UI-034）。
- `LayoutCache` struct 把渲染布局状态从业务状态里分离（UI-017）。
- `MultiStrategyParser::with_strategies` 构造器允许自定义 parser 链，
  不需要改默认顺序（DOM-013）。

## 6. 文件大小轨迹

10 个生产文件带着超 500 行的预算超标进 Phase 1。到 Phase 4 出场时，
生产集已经干净：

```
Phase 1 基线（10 个超 500 行的生产文件）：
  src/event.rs                       1677
  src/ui/logs/mod.rs                 1358
  src/app.rs                         1167
  src/ui/network/detail.rs           1109
  src/ui/source_select.rs             898
  src/ui/json_viewer/render.rs        745
  src/ui/network/mod.rs               700
  src/domain/structured_parser.rs     693
  src/transport/device_monitor.rs     654
  src/main.rs                         546
  （flog_dart/lib/src/flog_dio.dart   504 —— Dart 侧）

Phase 4 出场（0 个超 500 行的生产文件）：
  —— 每个生产 .rs ≤ 500 行
  —— 兄弟 _tests.rs 文件在 500 行以上（filter_tests 606、
     network_store_tests 791、protocol_tests 526）—— 按设计；
     CONTRIBUTING.md 明确测试文件不受预算约束，因为它们随
     可观察场景线性增长
```

预算是信号，不是铁律。Phase 3 Step 3.9 和 Step 3.10 都做过"比计划多
拆一层"的战术判断（见 §8 "没奏效的 —— 过度拆分"）。

## 7. 测试轨迹

| 阶段 | `cargo test`（lib）| 覆盖率（行）| 备注 |
|-------------|--------------------|------------------|-------|
| 入场（Phase 0）| 217 | 31.48% | 1 个 `cargo test --test` 烟雾测试 |
| Phase 2 出场 | 217 | 31.48% | 无行为改动 |
| Phase 2.5A 出场 | 222（+5 个纯函数抽取）| 31.79% | 谨慎 —— helper 接上 |
| **Phase 2.5B 出场** | **640 lib + 1 299 integration = 1 939** | **90.54%** | 3 条 ignored（DOM-003 + 2×DOM-018）|
| Phase 3 出场 | 1 507 lib 等效（兄弟 _tests.rs 抽取后平衡）| 90.54%+（从未下降）| 0 ignored（所有红测试都翻绿）|
| Phase 4 出场 | 2 166 total | ≥90% | UI-003 迁移触及特征化测试，测试数增加 |
| Phase 5 出场 | 2 166 total | 不变 | 纯文档 |
| Phase 6 出场 | 2 166 total | 不变 | 纯文档 |

特征化 crate 及其最终规模：

| Crate | 最终测试数 | 用途 |
|---------------------------------------------|------------------|---------|
| `tests/characterization_bugs.rs` | 7 | B 类锁定；Phase 3 Step 3.2 + 3.6 + 3.8 全部翻绿 |
| `tests/characterization_app_state.rs` | 157 | App 状态机转移、滚动、多 App、mock edit |
| `tests/characterization_event_keys.rs` | 107 | 按 AppMode × tab 的键盘派发 |
| `tests/characterization_event_mouse.rs` | 108 | TestBackend 鼠标路由（UI-041 被 Phase 3 Step 3.6 解锁）|
| `tests/characterization_input.rs` | 14 | FakeFlogServer 驱动 `input/connector.rs` 往返 |
| `tests/characterization_ui_logs.rs` | 84 | Logs 视图 TestBackend 快照 |
| `tests/characterization_ui_network.rs` | 128 | Network 视图 TestBackend 快照（含 UI-042 守护）|
| `tests/characterization_ui_source_select_help.rs` | 53 | picker + help 的 TestBackend |
| `tests/ws_server_test_direct.rs` | 1 | 战役前就有的烟雾测试 |

Dart 侧：`flutter test` 从 84 pass / 3 skip / 1 fail（DART-001 编译
error）变成 133 pass / 2 skip / 0 fail。

## 8. 延期事项

明确推出范围之外的事项，每一条都有转发引用。

- **DART-024 / DART-025** —— README + CHANGELOG 空缺。从 Phase 3
  Step 3.4 延期过来（计划选择 —— 内容工作，不是代码）。在 Phase 5
  Task 7（`364fb64`）关闭。
- **DART-033** —— flog_dart SSE 子系统架构债（分层混乱、闭包变量状
  态、parser 路径重复）。由外部审阅者 2026-04-24 作为 D 类增补提
  交。决策：延到 **Phase 5 之后的** flog_dart v0.8 breaking release
  —— v0.8 独立发布。Phase 5 写了迁移说明（`docs/PROTOCOL.md §9.1`、
  `flog_dart/CHANGELOG.md "Planned for v0.8"`）。不妥协：DART-001/002
  的正确性修复已经在 Phase 3 落地，剩下的是架构债，应当配迁移文档一
  起发布，不该硬塞进现在。
- **UI-011** —— JsonViewerPane 状态所有权 fingerprint —— 部分完成。
  Step 3.8 的预算被 detail/mod.rs 拆分 + UI-042 修复吃掉了；
  Step 3.9/3.10 只捡了一半（改名 + 组件搬迁）。剩下的另一半 —— 把
  pane fingerprint 明确到哪个 keyspace 拥有哪部分
  `collapsed_sections` —— 记在 Step 3.8 journal §"Deferred" 里。
- **TRANS-013** —— `src/replay.rs` 归档（只能通过 `pub mod` 到达的
  死模块）。Phase 2.5B Task 12 标为 UNTESTABLE D-ref。无限期延期
  —— 本来要用它的 replay 流程从来没接上，文件也就 50 行 no-op 代
  码。
- **TRANS-016 / TRANS-017** —— `src/transport/flutter_logs.rs` 能编
  译但没人调用；Phase 5 写 `MODULES.md` 验证时发现。记在
  `docs/MODULES.md "Audit trail gaps"`，如果不先被修掉，会在未来某次
  会话里迁移进 `audit/01-transport.md` 增补。Phase 5 的红线禁止内联
  修复。**（注：后续已在 DART-033 战役里顺手删掉，详见
  `chore(transport): remove unreferenced flutter_logs.rs (TRANS-016/017)`
  commit。）**
- **`src/event/mod.rs` 61% 覆盖率** —— 打了 PHYS 标签。未覆盖的 38%
  是 `handle_normal_mouse` 派发器驱动真实的
  `ratatui::backend::CrosstermBackend`。Phase 3 Step 3.6 的两阶段拆
  分原则上解锁了它；要把覆盖率再往上推，需要 TestBackend 变体来跑派
  发器的外壳，这超出本次战役范围。
- **`src/main.rs` + `src/run/server.rs` 引导路径 45–60%** —— 打了
  PHYS 标签。tokio runtime 启动、信号 handler、真实终端 enter/leave
  都是典型的没有纯函数缝的引导代码。

## 9. 让我们意外的事

三个值得拿出来说的意外：

1. **一个外部审阅者发现了我们以为修掉的 bug（DART-001）。** Phase 3
   Step 3.4 跑完 parser 重写之后，外部审计指出我们的测试输入没覆盖
   W3C SSE 的"单 chunk 多事件"场景。我们重读了 spec —— 其实我们的
   parser 是正确处理的，但审阅者的 repro 输入是 `return` 分隔的
   （Mac Classic）流，spec 明确不支持。parser 正确地拒绝了它。我们
   还是加了一个回归守护测试（`06eccd4`
   `test(flog_dart/sse): DART-001 repro guards — W3C multi-line data
   + multi-event-per-chunk`），因为"bug 被上报了 → 我们需要证据证明
   不会复发"比"bug 被上报了 → 我们按 spec 辩论"便宜。诚实记账花了一
   个额外测试。

2. **`C = 0` 纪律的回报远超预期。** Phase 1 Task 3 强制每条 C 类
   （有歧义的）发现在 Phase 2 开始前和用户一起解决。本来以为是官僚
   主义开销；结果发现它是整个战役里"能并行派给 subagent 的工作"和
   "必须用户先定的工作"之间最清晰的分界线。Phase 2 之后每次 subagent
   误实现都能追溯回一条 A/B/D/E 发现，没有一次是 C 类缺口引起的。

3. **UI-042 是用户先发现的。** Step 3.5 到 Step 3.6 过渡期间，用户
   在 TUI 里点来点去，看到 WS chat → raw 切换污染了列表面板。我们
   108 条鼠标特征化测试一条都没抓到。bug 是 `ws_chat_mode` 字段被
   翻转时没清 `collapsed_sections` —— 旧 key 活过了模式切换。我们
   先写了红锁测试（Phase 3 Step 3.6 `95f97d7`），后写修复（Step 3.8
   `133b631`）。教训：90% 覆盖率不等于 100% 行为覆盖，因为覆盖率是
   "行被执行过"，bug 活在行与行之间的状态交互里。

## 10. commit 总数

战役的账：
- 战役第一个 commit：`f3b2a12`（Phase 0 设计 + journal），2026-04-22
  18:54
- 战役最后一个 commit（Phase 6 出场）：由本阶段 Task 3 添加
- 战役开始以来的 commit 数：**Phase 6 之前 159 + 新增 3 = 162 条
  commit**，横跨三天。

按阶段分布：
| 阶段 | commit 数 |
|-------|---------|
| 0     | 1       |
| 1     | 3       |
| 2     | 2       |
| 2.5A  | 8       |
| 2.5B  | 16      |
| 3     | 98（10 个 step 合计）|
| 4     | 6       |
| 5     | 8       |
| 6     | 3       |

Phase 3 占主导不是因为工作量更大，而是因为每个 step 的工作被拆成
5–15 个 commit（一个 audit cluster 一个，或一个文件拆分一个），目的
是让单个 commit 可 review，同时让任何一个 subagent 回合都控制在运行
时事件截断预算之内。同样量的工作用"一个 phase-step 一个 commit"打
包大概是 10 个 commit；进一步拆分是有意的 subagent 安全选择，不是官
僚主义。

## 11. 对这份代码的教训（未来工作，先读我）

如果你是下一个接手 flog 的贡献者：

1. **UI-012 改名是承重的。** 旧名 `ui/source_select` 是错的 —— 这个
   模块是设备选择器，不是日志源选择器。每一处文档、注释、测试名都
   在 Step 3.9（`0441135`）迁走了。如果你在任何地方看到
   `source_select`，那是过期残留。

2. **`collapsed_sections` 所有权按 pane 划分。** 经过 UI-042，约定
   是：任何改 `collapsed_sections` 的 pane，在模式切换时必须清自己
   的 keyspace。Key 按前缀命名空间（`WS#*` 给 raw 模式、`WS_GROUP#*`
   给 chat 模式，等等）。见 `src/app/mod.rs::purge_ws_collapse_keys`。

3. **两阶段鼠标派发是后续事件工作的接缝。**
   `event/detect.rs::detect_click_region` 是 (App ref, x, y) →
   Option<ClickRegion> 的纯函数。任何新的点击目标都进 `ClickRegion`
   （枚举在 `event/click_region.rs`）。任何新的 mutation 都进
   `event/apply.rs::apply_click_region`。派发器本身
   （`event/mod.rs::handle_normal_mouse`）应保持约 35 行。不要再把
   检测和 mutation 搅在一起 —— 会把 UI-041 覆盖率搞崩。

4. **Ignored 测试带审计 ID。** 如果你在某个 commit 里看到
   `#[ignore = "bug: <id>"]`，那个 ID 指向一条 `audit/*.md` 里解释
   这个 bug 的条目。修它的时候在同一个 commit 里取消 ignore。不要
   留影子 TODO。

5. **兄弟文件测试模块是默认形式。** `src/foo.rs` 配对
   `src/foo_tests.rs`（同一个模块，`#[cfg(test)] mod tests;`）。
   UI-036 把每一个内联 `mod tests` 块都迁到了兄弟文件。新模块应该遵
   循同样的模式。500 行预算按设计不适用于 `*_tests.rs`
   （CONTRIBUTING §5.5）。

6. **`FlogNetKind` enum 就是 wire protocol。** `FlogNetMessage`
   （Phase 3 前的）没了。加新的 protocol variant 时，加到
   `src/domain/network.rs` 里 `FlogNetKind`（`#[serde(tag = "t")]`）。
   PROTOCOL.md 列了 wire 层级的例子。

7. **flog_dart v0.8 有计划但当时没发。** SSE 子系统相关的 Dart 侧工
   作需要和 DART-033 对齐 —— v0.8 plan 是"未决"事项。在 v0.8 发布
   之前，把 parser 当稳定，不要重构层级边界。**（补充：v0.8 已于
   2026-04-27 发布，DART-033 已关闭。）**

8. **`cargo llvm-cov` 是覆盖率的真相源。** 基线在
   `docs/superpowers/audit/.coverage-phase2-5b-final.txt`。Phase 3
   把"覆盖率不能掉"当作硬闸门，未来工作也应如此。

## 12. 诚实的成本

- **实际日历时间：** 三个日历日，大约 20-30 小时用户注意力（会话
  起：2026-04-22 早上，会话止：2026-04-24 晚上）。不是 3 个人日的连
  续工作 —— subagent 在后台跑，用户在监督、审、偶尔介入。
- **subagent 轮次：** 约 15-20 次，分布在 Phase 2.5B（14 个 task
  commit）+ Phase 3（10 个 step × 每个 1-2 次 subagent 轮）+ Phase
  4 + Phase 5 task subagent。不是每个 commit 都是 subagent —— 很多
  机械 task commit 是内联工作。
- **subagent 重试：** 至少 3 次 task commit 撞上"Truncated event
  message received"（Phase 2.5B 的 Task 5 被拆成 5a + 5b 之后
  subagent 才跑完）。修法永远是"缩小 task 范围"，从来不是"更用力相
  信 subagent"。
- **执行中途改计划：** Phase 2.5B 的初版计划被修订过
  （`4aab5f7 docs(superpowers): revise Phase 2.5B plan — stricter
  gates, parallel subagents`），因为 Task 3 回来时测试太浅，错过了
  Rule 9 的多场景密度。引入了更严格的数字闸门（Rule 2 每模块覆盖
  率、Rule 9 多场景、Rule 10 每公开函数密度），其余 task 重新派
  发。
- **用户介入：** 每个阶段边界都要显式批准计划；Phase 1 Task 3 的 C
  类裁决；Phase 3 Step 3.4 在 "这个常量叫 `defaultCapacity` 还是
  `FLOG_STORE_CAPACITY`" 之间犹豫时的 "加快进度" 轻推（用户选了一
  个）；UI-042 bug 报告（用户抓到了测试套件漏掉的 bug）；最后把
  Phase 6 推到底的 "不能因为不好做而妥协"（意思是诚实地把失败写进
  复盘，这份文件就是）。

## 13. 出场状态

每个阶段计划里的出口闸门都过了。见各 journal。特别是 Phase 6：

- ✅ `docs/superpowers/retrospective-flog.md`（本文）存在。
- ✅ `docs/superpowers/ai-long-workflow-methodology.md` 存在（兄弟
  篇）。
- ✅ `docs/superpowers/journal/phase6.md` 存在（战役关闭 journal）。
- ✅ `docs/superpowers/README.md` 更新过，有一个"Outcome"段指向两份
  新文档。
- ✅ `cargo test --all` 绿 —— 2 166 通过，0 失败，0 ignored
  （Phase 5 和 6 都是纯文档，数字不变）。
- ✅ Phase 6 没有引入代码改动。

战役关闭。flog 上任何后续工作都是新 spec。
