# Phase 1 Journal — Audit

## 入口
- 日期：2026-04-22
- Git HEAD at entry: `4b2c3b0` (baseline snapshot commit)
- 执行者：主 Claude + 4 audit subagents（read-only）
- 执行模式：Inline + 停机点（半自动）

## 时间线

- Phase 1 plan committed as `5888264`
- Task 0: baseline snapshot `.baseline.md` committed as `4b2c3b0`
- Task 1: 4 subagents dispatched in a single parallel message:
  - Subagent 1 (Explore, transport) — 15 findings
  - Subagent 2 (Explore, domain+parser) — 24 findings
  - Subagent 3 (Explore, UI+event+app) — 40 findings
  - Subagent 4 (general-purpose, flog_dart) — 32 findings
  - Zero code modifications across all four; `git status` confirmed no `src/` or `flog_dart/lib/` drift
- Task 2: format validation —
  - Forbidden-words scan: PASS
  - Required-fields counts: PASS
  - Label validity: PASS
  - Summary tables: 2 files had numeric drift in the summary (subagent
    self-count vs actual label count). Fixed inline: 02-domain and 03-ui
  - After fix: actual counts match summary tables on all 4 reports
- Task 3: 5 C-class entries adjudicated by user:
  - TRANS-005 → A (error message text, not design)
  - TRANS-010 → A (intentional, needs why-comment)
  - DOM-012 → E (confirmed dead — delete in Phase 2)
  - UI-022 → A (rename + merge in Phase 3)
  - UI-035 → D (migrate RGB constants to palette)
- Task 4: `00-index.md` assembled. User reviewed and approved option A for
  DART-002 (commit `flog_dart/test/` as authoritative red test)
- Task 5: this journal + phase commit

## 关键统计（Phase 1 出口）

| 类别 | Count | 去向 |
|---|---|---|
| A — Correct-but-ugly | 27 | Phase 3 redesign, A-class characterization test |
| B — Confirmed bug | 12 | Phase 2.5 red test → Phase 3 fix |
| C — Ambiguous | 0 | 全部裁决完毕 |
| D — Architecture smell | 63 | Phase 3 redesign, D-class characterization test |
| E — Mechanical | 9 | Phase 2 mechanical cleanup |
| **Total** | **111** | |

## 意外发现（Phase 6 methodology 原材料）

1. **flog_dart 是 B-class bug 的重灾区（9/12）**。Rust 侧只有 1 条 MEDIUM
   和 1 条 HIGH，flog_dart 侧 3 条 HIGH。反直觉：主代码库（Rust）比外部
   发布的 package（flog_dart）问题少。原因推测：Rust 侧有 217 个测试持续
   护着，flog_dart 侧没有 CI 测试运行。
2. **`flog_dart/test/` 的存在本身就是线索**。未跟踪的测试文件引用不存在
   的 API（`wrapTyped`, `SseEvent`）——说明"有人（很可能是早期 Claude
   会话）想做一次 SSE parser 重写，写了测试，但实现没跟上"。这是一个没写进
   CLAUDE.md / memory 的"历史意图"。Phase 3 把它恢复过来，是对历史的尊重。
3. **Subagent summary 表虚报**：两个 Explore subagent 在尾表的 A/B/C/D/E
   小计里口算了一个总数再分配，和文件里实际每条 label 计数对不上。Task 2
   的 awk 重新计数抓到。教训：**任何"subagent 自报"的统计都要独立核对**，
   不要信。
4. **audit 过程中零代码改动**：4 个只读 subagent 全部守规矩，`git status`
   没有 `src/` 或 `flog_dart/lib/` 变化。read-only prompt boilerplate
   起了作用。
5. **D-class 是大头（63）**，说明"代码能跑但架构不干净"是项目主要状态 ——
   这也印证了用户关键修正 A（"不仅仅是清理，架构有问题就重新设计"）的合理性。
   如果 Phase 2 和 Phase 3 没拆开、Phase 3 没拿到"可重新设计"的授权，
   这 63 条中大量条目会被错误分流到"机械清理"，结果就是改了一堆命名和
   样式但架构依然混乱。
6. **最危险的 bug 在协议边界而不是业务逻辑里**：DOM-003（HTTP 响应无 request
   被静默丢）+ DART-001（SSE 只保第一个 data:）—— 两个都是"边界情况被吞"，
   都没崩溃、都没日志、都是静默数据丢失。这种 bug 没有好的 UX 信号，只能靠
   仔细读代码发现，Phase 2.5 必须为这类写专门的测试。

## 出口
- 日期：2026-04-22
- Git HEAD at exit: Phase 1 commit（见 Task 5.3 commit hash，此 journal 内部暂不填写 —— 用 commit 前最后一轮校验）
- 验收门槛（spec §3.7）：
  - [x] 4 份 audit 报告齐全，格式合规
  - [x] 报告里无禁止词（forbidden-words scan PASS）
  - [x] 所有 C 类条目用户已裁决完（Task 3 done, C count = 0）
  - [x] `00-index.md` 合并完成并经用户 review
  - [x] `flog_dart/test/` disposition 决定：选 A（按 DART-002 恢复为权威测试）
  - [x] 1 个 docs-only commit（即本次 commit）

## 移交 Phase 2 的事项

- **E-class 分布**：transport 2 / domain 4 / ui 1 / flog-dart 2 —— 4 个
  Phase 2 subagent 各自认领。合并次序按 spec §4.1：
  transport → domain → flog_dart → ui
- **禁止在 Phase 2 改动的文件**：`flog_dart/test/`（由 Phase 2.5/3 处理）
- **额外提醒**：
  - DOM-012 归 E：Phase 2 domain subagent 负责删 `LogStore::append_continuation()`
    和 parser 的 `Continuation` 变体，确保 `cargo test` 仍绿
  - 03-ui 的 E-class 只有 1 条（`expand_all`/`collapse_all` 的
    `#[allow(dead_code)]`）
  - DART-024 / DART-025（README 与 CHANGELOG 残缺）归 D 类，不在 Phase 2
    动，归 Phase 5 文档更新 phase 处理
- **Phase 2.5 提醒**：
  - UI 层测试按"逻辑与渲染分离"原则，参考 03-ui.md 末尾每个红灯文件的
    "testability verdict"
  - `src/event.rs` 的 mouse routing **需要 TestBackend snapshot 兜底**
  - `flog_dart/test/flog_sse_parser_test.dart` 本身即 DART-001/002 的
    authoritative 红测 —— Phase 2.5 不为这俩再写 `#[ignore]`
- **Phase 3 step 规划的原材料**：`00-index.md` 的 "Phase 3 redesign scope
  — D-class by module" 节已按 spec §5.3 的 10 个 step 分好组
