# Unified Input Fields — Logs & Network 过滤框重构

**日期**: 2026-04-22
**状态**: Design

## 背景

当前 Logs 和 Network 两个 tab 的过滤输入框在多个维度上不一致、体验不够好：

1. **交互入口分散** — Logs 的 Search 和 Tag 是两个独立的 `AppMode`，Network 的 search 用 `search_active: bool`；三套机制做同一件事
2. **需要回车提交** — 所有输入框都要按 Enter 才 `apply_*()`，和现代习惯（即时过滤）相反
3. **缺少排除能力** — 只能通过 Tag 框的 `-tag` 排除 tag 字段，无法对 message / URL 做排除过滤
4. **布局不一致** — Logs 的 Tag 框在第 2 行（和 levels 挤在一起），Network 在第 1 行；全部输入框没有统一基线
5. **视觉态不够清晰** — 没有"空闲 / 有内容 / 激活"三种背景区分，用户不易知道当前哪些过滤已生效
6. **只 label 用 hint 文本** — `search...` / `tag...` 作为 placeholder 直接填在输入框内，和真实内容视觉冲突

## 目标

- 为 Logs 和 Network 提供统一的输入框组件
- 新增 Exclude 过滤能力（对 message / URL 的负向过滤）
- 所有输入框在第 1 行；选项 / pills 在第 2 行
- 输入即时生效，无需回车；点击框外失焦
- 输入框三态背景清晰可辨
- 支持多项 OR 语义（分隔符 `|`）
- 溢出时滚动窗口显示，功能不受影响

## 非目标

- 不改变 Search / Tag 现有的 regex 语法（`/pattern/`、`/pattern/i`）
- 不改变 Logs 的 level pills 和 Network 的 protocol/method/status pills 行为
- 不支持多行输入 / 粘贴时的多项编辑
- 不引入空格分隔 + 引号包裹的高级语法（只用 `|`）

## 设计概览

### 布局

**Logs**:
```
Row 1: [ Search (a|b): ______ ] [ Exclude (a|b): ______ ] [ Tag (+a|-b): ______ ]
Row 2:  S  V  D  I  W  E   │  ●3 bookmarks                         N/M count
```

**Network**:
```
Row 1: [ Search (a|b): ______ ] [ Exclude (a|b): ______ ]                  N/M count
Row 2: All HTTP SSE WS  │  GET POST PUT DEL PATCH  │  OK Fail Active Pending
```

- 输入框宽度按可用空间均分；最小宽度 16（label+括号+至少 8 字的值区域）
- 窄终端时 hint 会省略，保留纯 label（`Search:`）

### 输入框视觉（三态）

所有状态使用 `"Label: [ value ]"` 的形态（方括号是视觉 affordance，非字面显示；用背景块实现）。

| 状态 | Label 样式 | 输入区背景 | 输入区文字 | 备注 |
|---|---|---|---|---|
| Idle 空 | OVERLAY0 | BASE | OVERLAY0 dim | 显示 hint `(a\|b)` |
| Idle 有内容 | SUBTEXT0 | SURFACE0 | YELLOW | 表示过滤已激活 |
| Active | YELLOW + bold | SURFACE1 | TEXT | 显示 `_` 光标 |

### 多项语义

- **plain 模式**（不以 `/` 开头）：按 `|` 拆分为多项子串，任一匹配即命中（OR）
  - `timeout|500|refused` → 匹配包含 "timeout" OR "500" OR "refused" 的行
- **regex 模式**（以 `/` 开头，如 `/foo|bar/` / `/foo/i`）：整串交给 regex，`|` 由引擎处理
- **Tag 框**：plain 项可带 `+`/`-` 前缀指定 include/exclude，例 `+network|-flog_net|+http`

Search 和 Exclude 的 `|` 拆分和 regex 切换完全对称。

**只在 hint 里写最简示例**（`(a|b)` / `(+a|-b)`）—— 完整语法说明（regex `/pat/` / `/pat/i`、Tag `+`/`-` 前缀语义、失焦/激活规则、实时生效）写到 Help 页面 (`?`) 的 "Search & Filter" 小节，避免 hint 喧宾夺主。

### 输入框溢出

- **Active 态**：滚动窗口，光标始终可见
  - 光标距左右各留 1 字符上下文
  - 左侧溢出 → 显示 `…` 前缀
  - 右侧溢出 → 显示 `…` 后缀（但极少发生，光标大多在行尾）
- **Idle 态**：显示开头，尾部 `…` 省略号
- 内部 buffer 始终完整，过滤基于完整 buffer 执行，渲染只裁剪显示

### 交互

| 动作 | 效果 |
|---|---|
| 点击框内 | 激活该框（其他框失焦） |
| 点击框外 | 当前激活框失焦 |
| 任意字符输入 | 写入 buffer → 立即重算过滤 |
| Backspace | 删字符 → 立即重算过滤 |
| Enter | 失焦（保留内容） |
| Esc | 失焦（保留内容） |
| Tab | （未定义，保留不动）暂不切换焦点 |

**Tag 框**：
- Active 态：显示原始 buffer（`+network\|-flog_net`）
- Idle + 有内容态：显示解析后的 pills（和现在一样）
- 实时生效但不提交为 pills；失焦时才把 buffer 解析为 pills 显示

## 组件结构

### 新文件 `src/ui/input_field.rs`

```rust
pub struct InputFieldProps<'a> {
    pub label: &'a str,            // "Search", "Exclude", "Tag"
    pub hint: &'a str,             // "(a|b)", "(+a|-b)"
    pub value: &'a str,            // full buffer
    pub active: bool,
    pub width: u16,                // total width (label + bracket + value box)
    pub cursor_pos: usize,         // byte offset of cursor in value (active only)
}

pub struct RenderedInputField {
    pub spans: Vec<Span<'static>>,
    pub hit_x: (u16, u16),         // for click detection
}

pub fn render_input_field(props: InputFieldProps<'_>, x_offset: u16) -> RenderedInputField;
```

职责：
1. 根据 active + 是否有内容选 label/bg/text 样式
2. 计算滚动窗口（基于 cursor 位置或开头）
3. 返回 spans + 命中区域

### 数据层改动

#### `domain/filter.rs` (Logs)

新增：
```rust
pub struct FilterState {
    // ... existing fields ...
    pub exclude_query: String,
    pub exclude_regex: bool,
    compiled_exclude: Option<Regex>,
    compiled_exclude_plain: Vec<String>,   // lowercase, for plain OR
    // search 也补一份 plain multi
    compiled_search_plain: Vec<String>,
}
```

新方法：
- `set_search(&mut self, q: &str)` — 扩展为解析 `|` 多项
- `set_exclude(&mut self, q: &str)` — 对称逻辑

`matches()` 修改：
- search：有值则必须至少一项匹配（plain OR / regex 整串）
- 若 search 通过：**再检查 exclude**，任一项匹配就过滤掉

统一抽小工具：
```rust
fn matches_multi(
    query: &str,
    regex: Option<&Regex>,
    plain_parts: &[String],   // lowercase
    text: &str,
) -> bool;
```

#### `domain/network_filter.rs`

新增 `exclude: String` + 同样的 plain parts cache。

`matches()`：search 通过后检查 exclude（作用在 url+path）。

### `app.rs` 改动

**合并模式**：
```rust
pub enum AppMode {
    Normal,
    InputActive(InputField),
    Help,
    Stats,
    SourceSelect,
}

pub enum InputField {
    LogSearch,
    LogExclude,
    LogTag,
    NetSearch,
    NetExclude,
}
```

**Buffers**（每个输入框独立 buffer）：
```rust
pub struct InputBuffers {
    pub log_search: String,
    pub log_exclude: String,
    pub log_tag: String,
    pub net_search: String,
    pub net_exclude: String,
    pub cursors: [usize; 5],  // per-field cursor byte offset
}
```

**Layout 新增字段**：
```rust
pub struct Layout {
    // ... existing ...
    pub log_search_x: (u16, u16),
    pub log_exclude_x: (u16, u16),
    pub log_tag_x: (u16, u16),
    pub net_search_x: (u16, u16),   // rename from search_x if needed
    pub net_exclude_x: (u16, u16),
    pub input_row_y: u16,           // for click y-check
}
```

`apply_search()` / `apply_tag_filter()` 改为**每次按键都调用**，不再和 Enter 耦合。

### `event.rs` 改动

1. 删除 `handle_search_key` / `handle_filter_key` 分支 → 合并为 `handle_input_key(field: InputField, app, key)`
2. 按键逻辑：
   - 字符 / Backspace → 改 buffer → 立即 `apply_field(field)`
   - Enter / Esc → 切回 `AppMode::Normal`（保留内容）
3. 鼠标：
   - Click 命中某个 `input_row_y` + `*_x` 范围 → 切到该 `InputActive(field)`
   - Click 其他位置 → 若当前是 `InputActive(_)` → 切回 `Normal`（Tag 框自动解析为 pills，实际不需要额外动作，因为渲染态已经基于 active 状态切换）
4. `handle_input_mouse` 的"点击文本某处定位光标"保留（当前是否实现需对照；如未实现可留到 follow-up）

### UI 改动

**`ui/logs/mod.rs`** `draw_toolbar_op1` / `draw_toolbar_op2`:
- op1 变成 3 个 input_field 均分
- op2 改为 levels + bookmarks + 右对齐 count
- 计数（filtered/total）从 op1 移到 op2

**`ui/network/filter.rs`** `draw_network_op1` / `draw_network_op2`:
- op1 变成 2 个 input_field + 右对齐 count
- op2 保持 pills（proto / method / status）

**`ui/logs/mod.rs::draw_no_matching_logs`**:
- "Active filters" 卡片新增 `exclude:` 行（当 exclude 非空）

## 数据流

```
keypress
  → event.rs handle_input_key
    → InputBuffers.log_search[cursor] += c
    → filter.set_search(&InputBuffers.log_search)
    → store.mark_dirty()
  → renderer reads filter + buffer
    → rebuild filtered indices
    → draw input_field with buffer + active state
```

失焦：
```
click outside active input
  → event.rs sees current mode = InputActive(_)
  → set mode = Normal
  (buffer and filter state retained; no re-apply needed)
```

## 错误处理

- Regex 解析失败（`set_search` 里）→ `compiled_regex = None`，回退到 plain 模式（`search_query` 仍保留用户输入的原文供渲染）；过滤对任意行都视为"非匹配"，不抛错不崩溃
- 空输入框 → 该维度不过滤
- `|` 两侧有空格（`foo | bar`）→ trim 后作为一项；全空段被跳过

## 测试

### 单元测试（`domain/filter.rs` / `network_filter.rs`）

- `matches_multi` plain 单项匹配
- `matches_multi` 多项 OR 匹配
- `matches_multi` regex 模式整串处理（含 `|`）
- `matches` 结合 search + exclude，交集行为
- Exclude 空串不影响结果
- Regex 解析失败回退 plain

### 手动验收

- Logs：3 个输入框均能点击激活
- Network：2 个输入框均能点击激活
- 任意字符变化即时更新列表（无需 Enter）
- 点击列表任意位置输入框失焦；Tag 框显示切回 pills
- Search 输入 `foo|bar` 过滤出含任一关键词的行
- Exclude 输入 `heartbeat` 过滤掉相关行
- Tag 输入 `+network|-flog_net` 只显示 network，不显示 flog_net
- 输入超长内容，光标可见，过滤基于完整内容工作
- 三态背景切换明显
- 窄终端（<100 col）不溢出、不报错

## 影响文件清单

| 文件 | 改动类型 |
|---|---|
| `src/ui/input_field.rs` | 新增 |
| `src/domain/filter.rs` | 增 exclude + 改 set_search + 抽 matches_multi |
| `src/domain/network_filter.rs` | 增 exclude + 改 matches |
| `src/app.rs` | 改 AppMode + 新增 InputBuffers + 扩 Layout |
| `src/ui/logs/mod.rs` | 改 op1/op2 + no_matching_logs |
| `src/ui/network/mod.rs` | 可能调整 row constraints |
| `src/ui/network/filter.rs` | 改 op1/op2 |
| `src/event.rs` | 合并 search/tag 输入处理；改鼠标点击分发 |
| `src/session.rs` | （如持久化 exclude）加字段 |

## 迁移注意

- `FilterState::clear` 要清 exclude
- `NetworkFilter::reset` 要清 exclude
- Session 持久化如果包含搜索状态，可能需要 bump schema 或向后兼容处理 `exclude` 缺失
- 现有键盘快捷键（如 `/` 打开 search，`T` 打开 tag）保留 —— 仍然切到 `InputActive(field)`

## 风险与 Trade-off

- **AppMode 重构工作量**：event.rs 涉及的改动较多（~80 行）。替代是保留现有双模式 + 再加两个新模式，但会很乱，长期代价更大
- **实时生效的性能**：每次按键重算过滤，对 100K 条 store 来说是 O(N)；现在已经有 `filter.dirty` 标记机制，延迟到下次渲染时重算，可接受。若实测发现抖动再加去抖
- **窄终端体验**：3 个输入框在 80 col 以下会非常挤，计划在宽度不够时隐藏 hint；极端情况 label 也省略（`S:` / `E:` / `T:`）
- **Tag 框双态显示**（active 显文本 / idle 显 pills）是现有行为，保留不变
- **Exclude 和 Tag `-` 前缀冗余？** —— 不冗余：前者作用于 message/URL，后者作用于 tag 字段，语义正交。保留两者并在 hint 里区分（Exclude 框提示 `(a|b)`，Tag 提示 `(+a|-b)`）

## 实施顺序建议

1. domain 层：`filter.rs` + `network_filter.rs` 增 exclude + matches_multi（含单测）
2. 新组件 `ui/input_field.rs`（不接入）
3. `app.rs`：AppMode 合并 + InputBuffers + Layout 扩展
4. `event.rs`：改键盘/鼠标分发为 InputField 驱动
5. `ui/logs/mod.rs`：接入新组件 + 布局重排
6. `ui/network/filter.rs`：接入新组件 + 布局重排
7. 调整空态文案（no_matching_logs 显示 exclude）
8. 手动验收 + 窄终端测试
