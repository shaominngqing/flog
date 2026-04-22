# Unified Input Fields Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 给 Logs 和 Network 两个 tab 的过滤输入框做统一重构——新增 Exclude 过滤框，所有输入框实时生效（不用回车），移到第 1 行，三态背景样式，多项用 `|` 分隔 OR 匹配。

**Architecture:**
- 数据层先扩 `FilterState` 和 `NetworkFilter`，把 `set_search` 改造成支持 `|` 多项 + regex 模式，并新增对称的 `set_exclude`
- 再做共享 UI 组件 `ui/input_field.rs`（无状态纯渲染函数，三态样式 + 光标滚动窗口）
- 然后重构 `AppMode`：合并 `Search`/`TagFilter` 为 `InputActive(InputField)`，每个框独立 buffer
- `event.rs` 的键盘/鼠标分发改为按 `InputField` 类型分派，每次按键都 live-apply
- 最后 Logs 和 Network 的 toolbar 渲染接入新组件，布局调整

**Tech Stack:** Rust 2021, ratatui, crossterm, regex crate, unicode-width

---

## File Structure

| 文件 | 责任 |
|---|---|
| `src/domain/filter.rs` | FilterState 新增 exclude；set_search/set_exclude 支持 `|` 多项；`matches_multi` 小工具 |
| `src/domain/network_filter.rs` | NetworkFilter 新增 exclude；search 和 exclude 支持 `|` 多项 |
| `src/ui/input_field.rs` | **新建**。无状态渲染函数：给定 label/value/active/width/cursor_pos，输出 spans + 命中区域 |
| `src/app.rs` | AppMode 合并；InputBuffers 容器；Layout 新增 input_row_y / log_exclude_x / log_tag_x / net_exclude_x；新增 `enter_input_field` / `exit_input_field` / `apply_input_field` 方法 |
| `src/event.rs` | 合并 handle_search_key + handle_filter_key 为 handle_input_key(field)；鼠标分发按坐标判断点的是哪个框 |
| `src/ui/logs/mod.rs` | op1 变 3 个 input_field（search/exclude/tag）均分；op2 放 levels + bookmarks + count；no_matching_logs 加 exclude 行 |
| `src/ui/network/mod.rs` | row constraints 不变（仍是 7 行） |
| `src/ui/network/filter.rs` | op1 改 2 个 input_field（search/exclude）；op2 pills 不变 |
| `src/session.rs` | Session 持久化增加 exclude 字段（带 default 回退以兼容旧配置） |

---

## Task 1: 域层 — `matches_multi` helper 和测试

**Files:**
- Modify: `src/domain/filter.rs`
- Test: 同文件 `#[cfg(test)]` 模块

**背景**：现在 `FilterState::matches` 里 search 匹配是单个子串或单个正则；新语义是 plain 模式按 `|` 拆分 OR 匹配，regex 模式整串交给 regex。这个逻辑会被 search 和 exclude 共用，先抽出来。

- [ ] **Step 1: 写失败的测试**

在 `src/domain/filter.rs` 末尾追加：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_multi_plain_single() {
        let parts = vec!["timeout".to_string()];
        assert!(matches_multi(None, &parts, "connection timeout error"));
        assert!(!matches_multi(None, &parts, "connection ok"));
    }

    #[test]
    fn matches_multi_plain_or() {
        let parts = vec!["timeout".to_string(), "500".to_string(), "refused".to_string()];
        assert!(matches_multi(None, &parts, "got 500 from server"));
        assert!(matches_multi(None, &parts, "connection refused"));
        assert!(!matches_multi(None, &parts, "ok 200"));
    }

    #[test]
    fn matches_multi_case_insensitive_plain() {
        let parts = vec!["TiMeOuT".to_string()];
        assert!(matches_multi(None, &parts, "hit a Timeout"));
    }

    #[test]
    fn matches_multi_regex_owns_pipe() {
        let re = Regex::new("foo|bar").unwrap();
        assert!(matches_multi(Some(&re), &[], "hello foo"));
        assert!(matches_multi(Some(&re), &[], "bar world"));
        assert!(!matches_multi(Some(&re), &[], "baz"));
    }

    #[test]
    fn matches_multi_empty_parts_no_regex_is_false() {
        assert!(!matches_multi(None, &[], "anything"));
    }

    #[test]
    fn matches_multi_skips_empty_parts() {
        let parts = vec!["".to_string(), "hit".to_string(), "".to_string()];
        assert!(matches_multi(None, &parts, "go hit target"));
        assert!(!matches_multi(None, &parts, "miss"));
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --lib domain::filter::tests -- --nocapture`
Expected: `error[E0425]: cannot find function 'matches_multi' in this scope`

- [ ] **Step 3: 实现 matches_multi**

在 `src/domain/filter.rs` 的 `impl FilterState` 上方（紧跟 `impl Default for FilterState`）或文件合适位置插入**模块级**函数：

```rust
/// OR-match helper used by both Search and Exclude.
///
/// - If `regex` is `Some`, the regex owns the whole query (including `|`); `plain_parts` is ignored.
/// - Otherwise, return true if any non-empty entry in `plain_parts` is a case-insensitive
///   substring of `text`.
pub(crate) fn matches_multi(regex: Option<&Regex>, plain_parts: &[String], text: &str) -> bool {
    if let Some(re) = regex {
        return re.is_match(text);
    }
    if plain_parts.is_empty() {
        return false;
    }
    let text_lower = text.to_lowercase();
    for part in plain_parts {
        if part.is_empty() {
            continue;
        }
        if text_lower.contains(&part.to_lowercase()) {
            return true;
        }
    }
    false
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test --lib domain::filter::tests -- --nocapture`
Expected: 6 tests pass

- [ ] **Step 5: 提交**

```bash
git add src/domain/filter.rs
git commit -m "$(cat <<'EOF'
feat(filter): add matches_multi helper for OR-semantics filtering

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: 域层 — FilterState 加 exclude + set_search 走 `|` 多项

**Files:**
- Modify: `src/domain/filter.rs`
- Test: 同文件 tests 模块

**背景**：给 `FilterState` 加 `exclude_query` 等字段，改 `set_search` 在 plain 模式下拆 `|`，新增 `set_exclude` 同结构，`matches` 使用 `matches_multi`。

- [ ] **Step 1: 为新字段和 set_exclude 写失败测试**

在 `src/domain/filter.rs` 的 tests 模块追加：

```rust
    fn entry(tag: &str, msg: &str) -> LogEntry {
        LogEntry {
            timestamp: String::new(),
            level: LogLevel::Info,
            tag: tag.to_string(),
            message: msg.to_string(),
            extra_lines: Vec::new(),
            error: None,
            stacktrace: None,
            repeat_count: 1,
            raw: String::new(),
            source: super::super::entry::InputSource::Stdin,
        }
    }

    #[test]
    fn search_plain_multi_or() {
        let mut f = FilterState::default();
        f.set_search("timeout|500");
        assert!(f.matches(&entry("net", "connection timeout")));
        assert!(f.matches(&entry("net", "got 500 back")));
        assert!(!f.matches(&entry("net", "all good")));
    }

    #[test]
    fn search_regex_passes_pipe_through() {
        let mut f = FilterState::default();
        f.set_search("/foo|bar/");
        assert!(f.matches(&entry("t", "foo world")));
        assert!(f.matches(&entry("t", "bar world")));
        assert!(!f.matches(&entry("t", "baz")));
    }

    #[test]
    fn exclude_plain_removes_matches() {
        let mut f = FilterState::default();
        f.set_exclude("heartbeat|ping");
        assert!(f.matches(&entry("t", "real work")));
        assert!(!f.matches(&entry("t", "heartbeat tick")));
        assert!(!f.matches(&entry("t", "ping 30ms")));
    }

    #[test]
    fn exclude_regex_supported() {
        let mut f = FilterState::default();
        f.set_exclude("/^hb_/");
        assert!(!f.matches(&entry("t", "hb_start")));
        assert!(f.matches(&entry("t", "other_start")));
    }

    #[test]
    fn search_and_exclude_intersect() {
        let mut f = FilterState::default();
        f.set_search("error");
        f.set_exclude("heartbeat");
        assert!(f.matches(&entry("t", "got error 500")));
        assert!(!f.matches(&entry("t", "heartbeat error")));
        assert!(!f.matches(&entry("t", "all good")));
    }

    #[test]
    fn exclude_empty_does_nothing() {
        let mut f = FilterState::default();
        f.set_exclude("");
        assert!(f.matches(&entry("t", "anything")));
    }

    #[test]
    fn clear_resets_exclude() {
        let mut f = FilterState::default();
        f.set_exclude("noise");
        f.clear();
        assert!(f.matches(&entry("t", "noise was here")));
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --lib domain::filter::tests -- --nocapture`
Expected: 编译错误，`no method named 'set_exclude'`

- [ ] **Step 3: 扩展 FilterState 字段**

找到 struct 定义，改为：

```rust
#[derive(Debug, Clone)]
pub struct FilterState {
    pub min_level: LogLevel,
    pub tag_include: Vec<String>,
    pub tag_exclude: Vec<String>,
    pub search_query: String,
    pub search_regex: bool,
    compiled_regex: Option<Regex>,
    /// Plain-mode parts split by '|'. Empty when search is empty or in regex mode.
    compiled_search_plain: Vec<String>,
    pub exclude_query: String,
    pub exclude_regex: bool,
    compiled_exclude: Option<Regex>,
    compiled_exclude_plain: Vec<String>,
    pub tag_regex: bool,
    compiled_tag_include: Vec<Regex>,
    compiled_tag_exclude: Vec<Regex>,
}
```

`Default::default()` 补上新字段：

```rust
impl Default for FilterState {
    fn default() -> Self {
        Self {
            min_level: LogLevel::System,
            tag_include: Vec::new(),
            tag_exclude: Vec::new(),
            search_query: String::new(),
            search_regex: false,
            compiled_regex: None,
            compiled_search_plain: Vec::new(),
            exclude_query: String::new(),
            exclude_regex: false,
            compiled_exclude: None,
            compiled_exclude_plain: Vec::new(),
            tag_regex: false,
            compiled_tag_include: Vec::new(),
            compiled_tag_exclude: Vec::new(),
        }
    }
}
```

- [ ] **Step 4: 抽共享 compile_query helper，改 set_search，新增 set_exclude**

在 `impl FilterState` 块内新增/替换：

```rust
    fn compile_query(query: &str) -> (bool, Option<Regex>, Vec<String>) {
        // Regex mode: /pattern/ or /pattern/i
        if let Some(regex_body) = query.strip_prefix('/') {
            let (pattern, case_insensitive) = if let Some(p) = regex_body.strip_suffix("/i") {
                (p, true)
            } else if let Some(p) = regex_body.strip_suffix('/') {
                (p, false)
            } else {
                (regex_body, false)
            };
            let full = if case_insensitive {
                format!("(?i){}", pattern)
            } else {
                pattern.to_string()
            };
            let compiled = Regex::new(&full).ok();
            return (true, compiled, Vec::new());
        }
        // Plain multi-term mode: split by '|', trim, drop empties
        let parts: Vec<String> = query
            .split('|')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        (false, None, parts)
    }

    /// Set the Search query. Supports `/regex/` (optionally `/regex/i`) or `a|b|c` OR syntax.
    pub fn set_search(&mut self, query: &str) {
        let (is_regex, compiled, parts) = Self::compile_query(query);
        self.search_query = query.to_string();
        self.search_regex = is_regex;
        self.compiled_regex = compiled;
        self.compiled_search_plain = parts;
    }

    /// Set the Exclude query. Same syntax as set_search.
    pub fn set_exclude(&mut self, query: &str) {
        let (is_regex, compiled, parts) = Self::compile_query(query);
        self.exclude_query = query.to_string();
        self.exclude_regex = is_regex;
        self.compiled_exclude = compiled;
        self.compiled_exclude_plain = parts;
    }
```

**删除**原有 `set_search` 实现（已被上面替换）。

- [ ] **Step 5: 改 matches 使用 matches_multi，并加入 exclude 检查**

替换整个 `pub fn matches(&self, entry: &LogEntry) -> bool`：

```rust
    pub fn matches(&self, entry: &LogEntry) -> bool {
        // Separators always pass through filters
        if entry.tag == "────" {
            return true;
        }

        if entry.level < self.min_level {
            return false;
        }

        let tag = &entry.tag;

        // Tag 排除（使用预编译正则）
        if self.tag_regex {
            for re in &self.compiled_tag_exclude {
                if re.is_match(tag) {
                    return false;
                }
            }
        } else {
            let tag_lower = tag.to_lowercase();
            for exclude in &self.tag_exclude {
                if tag_lower == exclude.to_lowercase() {
                    return false;
                }
            }
        }

        // Tag 包含
        if !self.tag_include.is_empty() {
            let matched = if self.tag_regex {
                self.compiled_tag_include.iter().any(|re| re.is_match(tag))
            } else {
                let tag_lower = tag.to_lowercase();
                self.tag_include
                    .iter()
                    .any(|inc| tag_lower == inc.to_lowercase())
            };
            if !matched {
                return false;
            }
        }

        // Search (OR across message and tag)
        if !self.search_query.is_empty() {
            let full = entry.full_message();
            let hit = matches_multi(
                self.compiled_regex.as_ref(),
                &self.compiled_search_plain,
                &full,
            ) || matches_multi(
                self.compiled_regex.as_ref(),
                &self.compiled_search_plain,
                tag,
            );
            if !hit {
                return false;
            }
        }

        // Exclude (any hit on message or tag → drop)
        if !self.exclude_query.is_empty() {
            let full = entry.full_message();
            let kill = matches_multi(
                self.compiled_exclude.as_ref(),
                &self.compiled_exclude_plain,
                &full,
            ) || matches_multi(
                self.compiled_exclude.as_ref(),
                &self.compiled_exclude_plain,
                tag,
            );
            if kill {
                return false;
            }
        }

        true
    }
```

- [ ] **Step 6: 改 search_positions 使用 plain_parts 做高亮**

`search_positions` 用于高亮 —— 旧代码只高亮单个 query_lower。改为：

```rust
    pub fn search_positions(&self, text: &str) -> Vec<Range<usize>> {
        if self.search_query.is_empty() {
            return Vec::new();
        }

        if self.search_regex {
            if let Some(ref re) = self.compiled_regex {
                return re.find_iter(text).map(|m| m.start()..m.end()).collect();
            }
            return Vec::new();
        }

        let text_lower = text.to_lowercase();
        let mut positions = Vec::new();
        for part in &self.compiled_search_plain {
            if part.is_empty() {
                continue;
            }
            let needle = part.to_lowercase();
            let mut start = 0;
            while let Some(pos) = text_lower[start..].find(&needle) {
                let abs_start = start + pos;
                let abs_end = abs_start + needle.len();
                positions.push(abs_start..abs_end);
                start = abs_end;
            }
        }
        positions.sort_by_key(|r| r.start);
        positions
    }
```

- [ ] **Step 7: 改 clear() 清 exclude**

```rust
    pub fn clear(&mut self) {
        self.tag_include.clear();
        self.tag_exclude.clear();
        self.compiled_tag_include.clear();
        self.compiled_tag_exclude.clear();
        self.search_query.clear();
        self.search_regex = false;
        self.compiled_regex = None;
        self.compiled_search_plain.clear();
        self.exclude_query.clear();
        self.exclude_regex = false;
        self.compiled_exclude = None;
        self.compiled_exclude_plain.clear();
        self.tag_regex = false;
        self.min_level = LogLevel::System;
    }
```

- [ ] **Step 8: 运行全部测试**

Run: `cargo test --lib domain::filter -- --nocapture`
Expected: 所有 tests pass（原有 + 新增 7 个）

- [ ] **Step 9: 确认编译**

Run: `cargo build`
Expected: compile OK（旧的 set_search 调用方 —— `app::apply_search` —— 仍然只传一个 `&str`，签名不变）

- [ ] **Step 10: 提交**

```bash
git add src/domain/filter.rs
git commit -m "$(cat <<'EOF'
feat(filter): add exclude filter + pipe-separated OR matching

set_search and set_exclude both support:
- plain mode: a|b|c as OR of substrings
- regex mode: /pat/ and /pat/i passes pipe through to the engine

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: 域层 — NetworkFilter 加 exclude + 多项

**Files:**
- Modify: `src/domain/network_filter.rs`
- Test: 同文件 tests 模块

- [ ] **Step 1: 写失败测试**

在 `src/domain/network_filter.rs` 末尾加 tests 模块：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::network::{EntrySource, NetworkEntry, NetworkStatus, Protocol};

    fn e(url: &str, path: &str) -> NetworkEntry {
        NetworkEntry {
            id: "1".into(),
            url: url.into(),
            path: path.into(),
            method: "GET".into(),
            protocol: Protocol::Http,
            status: NetworkStatus::Completed,
            http_status: Some(200),
            started_ms: 0,
            duration_ms: Some(10),
            req_headers: Default::default(),
            req_query: Default::default(),
            req_body: None,
            res_headers: Default::default(),
            res_body: None,
            res_size: None,
            error: None,
            sse_chunks: Vec::new(),
            ws_messages: Vec::new(),
            source: EntrySource::App,
        }
    }

    #[test]
    fn search_plain_or() {
        let mut f = NetworkFilter::new();
        f.set_search("users|orders");
        assert!(f.matches(&e("https://x.com/api/users", "/api/users")));
        assert!(f.matches(&e("https://x.com/api/orders", "/api/orders")));
        assert!(!f.matches(&e("https://x.com/api/posts", "/api/posts")));
    }

    #[test]
    fn search_regex() {
        let mut f = NetworkFilter::new();
        f.set_search("/^/api/(users|orders)$/");
        assert!(f.matches(&e("https://x.com/api/users", "/api/users")));
        assert!(!f.matches(&e("https://x.com/api/posts", "/api/posts")));
    }

    #[test]
    fn exclude_plain() {
        let mut f = NetworkFilter::new();
        f.set_exclude("heartbeat|telemetry");
        assert!(f.matches(&e("https://x.com/api/users", "/api/users")));
        assert!(!f.matches(&e("https://x.com/api/heartbeat", "/api/heartbeat")));
        assert!(!f.matches(&e("https://x.com/api/telemetry", "/api/telemetry")));
    }

    #[test]
    fn reset_clears_exclude() {
        let mut f = NetworkFilter::new();
        f.set_exclude("noise");
        f.reset();
        assert!(f.matches(&e("https://x.com/noise", "/noise")));
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --lib domain::network_filter::tests -- --nocapture`
Expected: compile error `set_search` / `set_exclude` not found

- [ ] **Step 3: 扩展 NetworkFilter**

替换整个 struct + impl 块（保留枚举 ProtocolFilter/MethodFilter/StatusFilter 不动，只改 NetworkFilter 本身）：

```rust
use regex::Regex;

pub struct NetworkFilter {
    pub status: StatusFilter,
    pub method: MethodFilter,
    pub protocol: ProtocolFilter,
    pub search: String,
    pub exclude: String,
    search_regex: Option<Regex>,
    search_plain: Vec<String>,
    exclude_regex: Option<Regex>,
    exclude_plain: Vec<String>,
}

impl NetworkFilter {
    pub fn new() -> Self {
        Self {
            status: StatusFilter::All,
            method: MethodFilter::All,
            protocol: ProtocolFilter::All,
            search: String::new(),
            exclude: String::new(),
            search_regex: None,
            search_plain: Vec::new(),
            exclude_regex: None,
            exclude_plain: Vec::new(),
        }
    }

    fn compile_query(query: &str) -> (Option<Regex>, Vec<String>) {
        if let Some(body) = query.strip_prefix('/') {
            let (pattern, ci) = if let Some(p) = body.strip_suffix("/i") {
                (p, true)
            } else if let Some(p) = body.strip_suffix('/') {
                (p, false)
            } else {
                (body, false)
            };
            let full = if ci { format!("(?i){}", pattern) } else { pattern.to_string() };
            return (Regex::new(&full).ok(), Vec::new());
        }
        let parts: Vec<String> = query
            .split('|')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        (None, parts)
    }

    pub fn set_search(&mut self, query: &str) {
        self.search = query.to_string();
        let (re, parts) = Self::compile_query(query);
        self.search_regex = re;
        self.search_plain = parts;
    }

    pub fn set_exclude(&mut self, query: &str) {
        self.exclude = query.to_string();
        let (re, parts) = Self::compile_query(query);
        self.exclude_regex = re;
        self.exclude_plain = parts;
    }

    fn matches_any(regex: Option<&Regex>, plain: &[String], text: &str) -> bool {
        if let Some(re) = regex {
            return re.is_match(text);
        }
        if plain.is_empty() {
            return false;
        }
        let text_lower = text.to_lowercase();
        plain.iter().any(|p| !p.is_empty() && text_lower.contains(&p.to_lowercase()))
    }

    pub fn matches(&self, entry: &NetworkEntry) -> bool {
        if !self.status.matches(entry.status) {
            return false;
        }
        if !self.method.matches(&entry.method) {
            return false;
        }
        if !self.protocol.matches(entry.protocol) {
            return false;
        }
        if !self.search.is_empty() {
            let url_hit = Self::matches_any(self.search_regex.as_ref(), &self.search_plain, &entry.url);
            let path_hit = Self::matches_any(self.search_regex.as_ref(), &self.search_plain, &entry.path);
            if !url_hit && !path_hit {
                return false;
            }
        }
        if !self.exclude.is_empty() {
            let url_hit = Self::matches_any(self.exclude_regex.as_ref(), &self.exclude_plain, &entry.url);
            let path_hit = Self::matches_any(self.exclude_regex.as_ref(), &self.exclude_plain, &entry.path);
            if url_hit || path_hit {
                return false;
            }
        }
        true
    }

    #[allow(dead_code)]
    pub fn is_active(&self) -> bool {
        self.status != StatusFilter::All
            || self.method != MethodFilter::All
            || self.protocol != ProtocolFilter::All
            || !self.search.is_empty()
            || !self.exclude.is_empty()
    }

    pub fn reset(&mut self) {
        self.status = StatusFilter::All;
        self.method = MethodFilter::All;
        self.protocol = ProtocolFilter::All;
        self.search.clear();
        self.exclude.clear();
        self.search_regex = None;
        self.search_plain.clear();
        self.exclude_regex = None;
        self.exclude_plain.clear();
    }
}
```

- [ ] **Step 4: 运行测试**

Run: `cargo test --lib domain::network_filter::tests -- --nocapture`
Expected: 4 new tests pass

- [ ] **Step 5: 确认编译**

Run: `cargo build`
Expected: 可能有一处破坏 —— `event.rs:1296` 目前写的是 `app.network.filter.search = app.network.search_input.clone();`（直接赋 field，不走 setter）。改成：

```rust
// event.rs ~1296
app.network.filter.set_search(&app.network.search_input);
```

这是过渡修复，Task 5 会彻底重写这块。先保证编译通过。

- [ ] **Step 6: 提交**

```bash
git add src/domain/network_filter.rs src/event.rs
git commit -m "$(cat <<'EOF'
feat(network_filter): add exclude filter + pipe-separated OR matching

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: 新建 `ui/input_field.rs` 共享组件

**Files:**
- Create: `src/ui/input_field.rs`
- Modify: `src/ui/mod.rs` (加 pub mod)
- Test: 同 input_field.rs 文件

**背景**：无状态渲染函数，负责：(1) 三态样式，(2) 光标滚动窗口，(3) 返回命中区域。Logs/Network 两处共用。

- [ ] **Step 1: 在 ui/mod.rs 加模块声明**

读一下当前 `src/ui/mod.rs` 的 pub mod 位置，加一行 `pub mod input_field;`（和现有 `pub mod logs;` / `pub mod network;` 并列）。

- [ ] **Step 2: 写失败的测试（visible_window 滚动逻辑）**

创建 `src/ui/input_field.rs`，先写：

```rust
//! Shared input field renderer — stateless, three-state background, scroll window.

use ratatui::{
    style::{Modifier, Style},
    text::Span,
};
use unicode_width::UnicodeWidthChar;

use super::{MANTLE, OVERLAY0, SUBTEXT0, SURFACE0, SURFACE1, TEXT, YELLOW};

/// Inputs for rendering one input field.
pub struct InputFieldProps<'a> {
    pub label: &'a str,           // e.g., "Search"
    pub hint: &'a str,            // e.g., "(a|b)" shown when idle+empty
    pub value: &'a str,           // full buffer
    pub active: bool,
    /// Cursor byte offset into `value` (ignored when !active).
    pub cursor_byte: usize,
    /// Total width the field may consume (label + value box + 1-char gaps).
    pub total_width: u16,
}

/// Output from render_input_field.
pub struct RenderedInputField {
    pub spans: Vec<Span<'static>>,
    /// Click hit region (inclusive start, exclusive end) relative to caller's row.
    pub hit_x: (u16, u16),
    /// Number of columns consumed (should equal total_width when possible).
    pub used_width: u16,
}

/// Compute the substring (as a char slice) of `value` that fits in `box_width` columns,
/// keeping `cursor_byte` visible. Returns (display_text, ellipsis_left, ellipsis_right).
///
/// When `active = false`, always show from the start (with trailing ellipsis if needed).
/// When `active = true`, slide the window so the cursor position (right after cursor_byte) is visible,
/// with 1 column of right padding for the blinking "_".
pub fn visible_window(
    value: &str,
    cursor_byte: usize,
    box_width: usize,
    active: bool,
) -> (String, bool, bool) {
    if box_width == 0 {
        return (String::new(), false, false);
    }

    // Total display width of value
    let total: usize = value.chars().map(|c| c.width().unwrap_or(0)).sum();

    if total <= box_width {
        return (value.to_string(), false, false);
    }

    if !active {
        // Head + ellipsis suffix: take chars until box_width-1, add '…'
        let mut out = String::new();
        let mut used = 0usize;
        for ch in value.chars() {
            let w = ch.width().unwrap_or(0);
            if used + w > box_width.saturating_sub(1) {
                break;
            }
            out.push(ch);
            used += w;
        }
        return (out, false, true);
    }

    // Active: slide window to keep cursor visible.
    // Cursor column = width of value[..cursor_byte] + 1 (for the "_" indicator).
    let prefix_width: usize = value[..cursor_byte.min(value.len())]
        .chars()
        .map(|c| c.width().unwrap_or(0))
        .sum();
    // The displayed text plus trailing "_" must fit in box_width.
    // Target: cursor_col in range [1, box_width - 1] within the window.
    // Simplest rule: right-edge = prefix_width + 1; left-edge = right-edge - box_width.
    let right_edge = prefix_width + 1; // one column reserved for the "_" after cursor
    let left_edge = right_edge.saturating_sub(box_width);

    // Collect chars whose running width falls in [left_edge, right_edge).
    let mut out = String::new();
    let mut col = 0usize;
    let mut started = false;
    let mut ellipsis_left = left_edge > 0;
    for ch in value.chars() {
        let w = ch.width().unwrap_or(0);
        let ch_start = col;
        let ch_end = col + w;
        col = ch_end;
        if ch_end <= left_edge {
            continue;
        }
        if ch_start >= right_edge {
            break;
        }
        // Reserve 1 column for left '…' if we're truncating left
        if ellipsis_left && !started {
            out.push('…');
            started = true;
            // skip partial char that overlaps the ellipsis column
            if ch_end - left_edge <= 1 {
                continue;
            }
        }
        out.push(ch);
    }
    let ellipsis_right = right_edge < total;
    // Reserve 1 column for right '…' if we still have room overflow on the right
    // (rare because active keeps cursor at right; include if we truncated right).
    (out, ellipsis_left, ellipsis_right)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_window_short_fits() {
        let (out, l, r) = visible_window("hello", 5, 10, false);
        assert_eq!(out, "hello");
        assert!(!l && !r);
    }

    #[test]
    fn visible_window_idle_truncates_tail() {
        let (out, l, r) = visible_window("abcdefghij", 0, 5, false);
        // box_width=5 → keep 4 chars + '…' added by renderer via r=true
        assert_eq!(out, "abcd");
        assert!(!l);
        assert!(r);
    }

    #[test]
    fn visible_window_active_keeps_cursor_visible() {
        // value length 10, box=5, cursor at end (byte 10)
        let (out, l, r) = visible_window("abcdefghij", 10, 5, true);
        // Window right-edge = 11, left-edge = 6 → show fghij prefixed by '…'
        // With 1 col for ellipsis, 4 chars fit.
        assert!(out.starts_with('…'));
        assert!(out.ends_with('j'));
        assert!(l);
        assert!(!r);
    }

    #[test]
    fn visible_window_zero_box() {
        let (out, _, _) = visible_window("abc", 0, 0, false);
        assert_eq!(out, "");
    }
}
```

- [ ] **Step 3: 运行 visible_window 测试确认通过**

Run: `cargo test --lib ui::input_field::tests -- --nocapture`
Expected: 4 pass（如果某个 assertion 失败，检查实现与测试期望对齐）

- [ ] **Step 4: 实现 render_input_field**

追加到同一文件：

```rust
/// Render an input field. Layout (spans, left-to-right):
///   " LABEL: "  VALUE_BOX (box_width cols)  (no trailing space)
///
/// Three-state backgrounds:
///   idle + empty    → box bg BASE-surface (SURFACE0 dim)
///   idle + has text → box bg SURFACE0
///   active          → box bg SURFACE1 + blinking '_' cursor
///
/// When `idle + empty`, hint text is drawn inside the box.
pub fn render_input_field(props: InputFieldProps<'_>, x_offset: u16) -> RenderedInputField {
    use unicode_width::UnicodeWidthStr;

    // Label segment: "Search: " (with trailing space)
    let label_text = format!(" {}: ", props.label);
    let label_w = label_text.width() as u16;

    let box_width = props.total_width.saturating_sub(label_w) as usize;
    let box_width = box_width.max(4); // minimum usable

    let has_text = !props.value.is_empty();
    let (label_style, box_bg, text_fg) = if props.active {
        (
            Style::default().fg(YELLOW).bg(MANTLE).add_modifier(Modifier::BOLD),
            SURFACE1,
            TEXT,
        )
    } else if has_text {
        (
            Style::default().fg(SUBTEXT0).bg(MANTLE),
            SURFACE0,
            YELLOW,
        )
    } else {
        (
            Style::default().fg(OVERLAY0).bg(MANTLE),
            SURFACE0,
            OVERLAY0,
        )
    };

    let mut spans = Vec::new();
    spans.push(Span::styled(label_text, label_style));

    // Body: either hint (idle+empty) or scrolled value
    let (body_text, el_l, el_r) = if !has_text && !props.active {
        (props.hint.to_string(), false, false)
    } else {
        visible_window(props.value, props.cursor_byte, box_width.saturating_sub(if props.active { 1 } else { 0 }), props.active)
    };

    let _ = (el_l, el_r); // ellipsis markers are already embedded in body_text by visible_window

    let mut body = body_text;
    if props.active {
        body.push('_');
    }
    // Pad to box_width
    let body_w = body.width();
    if body_w < box_width {
        body.push_str(&" ".repeat(box_width - body_w));
    }

    spans.push(Span::styled(body, Style::default().fg(text_fg).bg(box_bg)));

    RenderedInputField {
        spans,
        hit_x: (x_offset + label_w, x_offset + label_w + box_width as u16),
        used_width: label_w + box_width as u16,
    }
}
```

- [ ] **Step 5: 编译检查**

Run: `cargo build`
Expected: 编译通过（这个模块不被任何人调用，只要结构/类型正确即可）

- [ ] **Step 6: 运行所有新测试**

Run: `cargo test --lib ui::input_field -- --nocapture`
Expected: 全部 pass

- [ ] **Step 7: 提交**

```bash
git add src/ui/input_field.rs src/ui/mod.rs
git commit -m "$(cat <<'EOF'
feat(ui): shared input_field component with 3-state bg and scroll window

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: app.rs — 重构 AppMode + InputBuffers

**Files:**
- Modify: `src/app.rs`
- Modify: `src/event.rs` (修复 compile，不做行为改动)

**背景**：AppMode 合并 Search/TagFilter 为 `InputActive(InputField)`。每个输入框独立 buffer + cursor。新增辅助方法 `enter_input_field`、`exit_input_field`、`apply_input_field`。

- [ ] **Step 1: 改 AppMode 和新增 InputField 枚举**

`src/app.rs`：

找到：
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    Normal,
    Search,
    TagFilter,
    Help,
    Stats,
    MockRuleEdit,
}
```

替换为：
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputField {
    LogSearch,
    LogExclude,
    LogTag,
    NetSearch,
    NetExclude,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    Normal,
    InputActive(InputField),
    Help,
    Stats,
    MockRuleEdit,
}
```

注意 `InputField` 派生 `Copy` 方便到处传值。

- [ ] **Step 2: 新增 InputBuffers 结构**

紧跟 `TagFilterInput` 定义之后，加：

```rust
/// Buffers + cursor for all 5 input fields.
#[derive(Default)]
pub struct InputBuffers {
    pub log_search: String,
    pub log_exclude: String,
    pub log_tag: String,
    pub net_search: String,
    pub net_exclude: String,
    /// Cursor byte offset per field (parallel to the String buffers).
    pub log_search_cursor: usize,
    pub log_exclude_cursor: usize,
    pub log_tag_cursor: usize,
    pub net_search_cursor: usize,
    pub net_exclude_cursor: usize,
}

impl InputBuffers {
    pub fn buffer_mut(&mut self, field: InputField) -> &mut String {
        match field {
            InputField::LogSearch => &mut self.log_search,
            InputField::LogExclude => &mut self.log_exclude,
            InputField::LogTag => &mut self.log_tag,
            InputField::NetSearch => &mut self.net_search,
            InputField::NetExclude => &mut self.net_exclude,
        }
    }

    pub fn buffer(&self, field: InputField) -> &str {
        match field {
            InputField::LogSearch => &self.log_search,
            InputField::LogExclude => &self.log_exclude,
            InputField::LogTag => &self.log_tag,
            InputField::NetSearch => &self.net_search,
            InputField::NetExclude => &self.net_exclude,
        }
    }

    pub fn cursor_mut(&mut self, field: InputField) -> &mut usize {
        match field {
            InputField::LogSearch => &mut self.log_search_cursor,
            InputField::LogExclude => &mut self.log_exclude_cursor,
            InputField::LogTag => &mut self.log_tag_cursor,
            InputField::NetSearch => &mut self.net_search_cursor,
            InputField::NetExclude => &mut self.net_exclude_cursor,
        }
    }

    pub fn cursor(&self, field: InputField) -> usize {
        match field {
            InputField::LogSearch => self.log_search_cursor,
            InputField::LogExclude => self.log_exclude_cursor,
            InputField::LogTag => self.log_tag_cursor,
            InputField::NetSearch => self.net_search_cursor,
            InputField::NetExclude => self.net_exclude_cursor,
        }
    }
}
```

- [ ] **Step 3: App 结构加 inputs 字段**

在 `pub struct App { ... }` 里，找到 `pub search: SearchState,` 和 `pub tag_filter: TagFilterInput,` 保留（向后兼容逐步移除）；在下一行加：

```rust
    pub inputs: InputBuffers,
```

在 `App::new()` 的初始化块里（搜索 `search: SearchState::default(),`），加一行：
```rust
            inputs: InputBuffers::default(),
```

- [ ] **Step 4: Layout 加新字段**

找到 `pub struct LayoutCache { ... }`，在最后 `jump_to_bottom_rect` 前加：

```rust
    /// Y position of the row that holds all input fields (logs op1 / network op1).
    pub input_row_y: u16,
    /// Click hit regions per input field.
    pub log_search_x: (u16, u16),
    pub log_exclude_x: (u16, u16),
    pub log_tag_x: (u16, u16),
    pub net_exclude_x: (u16, u16),
```

（`net_search_x` 已存在；`search_x` 和 `filter_x` 是旧字段，暂不删，Task 7/8 会停用然后清理。）

- [ ] **Step 5: 新增输入框控制方法**

在 App impl 块里（替换原有 `enter_search`/`apply_search`/`cancel_search` 和 `enter_tag_filter`/`apply_tag_filter`/`cancel_tag_filter`），插入新方法：

```rust
    // ── Unified input-field control ──

    pub fn enter_input_field(&mut self, field: InputField) {
        // Seed buffer from current filter state if buffer is empty.
        match field {
            InputField::LogSearch => {
                if self.inputs.log_search.is_empty() {
                    self.inputs.log_search = self.filter.search_query.clone();
                    self.inputs.log_search_cursor = self.inputs.log_search.len();
                }
            }
            InputField::LogExclude => {
                if self.inputs.log_exclude.is_empty() {
                    self.inputs.log_exclude = self.filter.exclude_query.clone();
                    self.inputs.log_exclude_cursor = self.inputs.log_exclude.len();
                }
            }
            InputField::LogTag => {
                if self.inputs.log_tag.is_empty() {
                    let tags: Vec<String> = self
                        .filter
                        .tag_include
                        .iter()
                        .cloned()
                        .chain(self.filter.tag_exclude.iter().map(|t| format!("-{}", t)))
                        .collect();
                    self.inputs.log_tag = tags.join("|");
                    self.inputs.log_tag_cursor = self.inputs.log_tag.len();
                }
            }
            InputField::NetSearch => {
                if self.inputs.net_search.is_empty() {
                    self.inputs.net_search = self.network.filter.search.clone();
                    self.inputs.net_search_cursor = self.inputs.net_search.len();
                }
            }
            InputField::NetExclude => {
                if self.inputs.net_exclude.is_empty() {
                    self.inputs.net_exclude = self.network.filter.exclude.clone();
                    self.inputs.net_exclude_cursor = self.inputs.net_exclude.len();
                }
            }
        }
        self.mode = AppMode::InputActive(field);
        self.layout.last_click = None;
    }

    pub fn exit_input_field(&mut self) {
        self.mode = AppMode::Normal;
        self.layout.last_click = None;
    }

    /// Push the active buffer into the filter and re-run filter.
    pub fn apply_input_field(&mut self, field: InputField) {
        match field {
            InputField::LogSearch => {
                self.filter.set_search(&self.inputs.log_search);
                self.invalidate_filter();
            }
            InputField::LogExclude => {
                self.filter.set_exclude(&self.inputs.log_exclude);
                self.invalidate_filter();
            }
            InputField::LogTag => {
                // Tag uses a custom parser already; still live-apply.
                // Translate '|' separators to ',' to reuse parse_tag_filter.
                let as_csv = self.inputs.log_tag.replace('|', ",");
                self.filter.parse_tag_filter(&as_csv);
                self.invalidate_filter();
            }
            InputField::NetSearch => {
                self.network.filter.set_search(&self.inputs.net_search);
                self.network.invalidate_filter();
            }
            InputField::NetExclude => {
                self.network.filter.set_exclude(&self.inputs.net_exclude);
                self.network.invalidate_filter();
            }
        }
    }
```

- [ ] **Step 6: 保留旧 enter_search/apply_search 等为薄 shim（防止 event.rs 编译崩）**

替换 `enter_search`、`apply_search`、`cancel_search`、`enter_tag_filter`、`apply_tag_filter`、`cancel_tag_filter` 为：

```rust
    // ── Legacy shims (will be removed in Task 6) ──

    pub fn enter_search(&mut self) {
        self.enter_input_field(InputField::LogSearch);
    }

    pub fn apply_search(&mut self) {
        self.apply_input_field(InputField::LogSearch);
        self.exit_input_field();
    }

    pub fn cancel_search(&mut self) {
        self.exit_input_field();
    }

    pub fn enter_tag_filter(&mut self) {
        self.enter_input_field(InputField::LogTag);
    }

    pub fn apply_tag_filter(&mut self) {
        self.apply_input_field(InputField::LogTag);
        self.exit_input_field();
    }

    pub fn cancel_tag_filter(&mut self) {
        self.exit_input_field();
    }
```

- [ ] **Step 7: 修复 event.rs 里引用旧 AppMode 变体的地方**

`src/event.rs` 需要这些替换：

替换 `handle_key` 的 match：
```rust
pub fn handle_key(app: &mut App, key: KeyEvent) {
    match app.mode.clone() {
        AppMode::Normal => handle_normal_key(app, key),
        AppMode::InputActive(field) => handle_input_key(app, field, key),
        AppMode::Help | AppMode::Stats => handle_overlay_key(app, key),
        AppMode::MockRuleEdit => handle_mock_edit_key(app, key),
    }
}
```

替换 `handle_mouse`：
```rust
pub fn handle_mouse(app: &mut App, mouse: MouseEvent) {
    match app.mode.clone() {
        AppMode::Normal => handle_normal_mouse(app, mouse),
        AppMode::InputActive(_) => handle_input_mouse(app, mouse),
        AppMode::Help | AppMode::Stats => handle_overlay_mouse(app, mouse),
        AppMode::MockRuleEdit => handle_mock_edit_mouse(app, mouse),
    }
}
```

在 event.rs 底部或合适位置增加：

```rust
fn handle_input_key(app: &mut App, field: crate::app::InputField, key: KeyEvent) {
    match key.code {
        KeyCode::Enter | KeyCode::Esc => app.exit_input_field(),
        KeyCode::Backspace => {
            let buf = app.inputs.buffer_mut(field);
            if buf.pop().is_some() {
                let c = app.inputs.cursor_mut(field);
                *c = (*c).min(app.inputs.buffer(field).len());
            }
            app.apply_input_field(field);
        }
        KeyCode::Char(c) => {
            app.inputs.buffer_mut(field).push(c);
            *app.inputs.cursor_mut(field) = app.inputs.buffer(field).len();
            app.apply_input_field(field);
        }
        _ => {}
    }
}
```

删除旧的 `handle_search_key` 和 `handle_filter_key` 函数（不再被调用）。

改 `handle_input_mouse` —— 之前引用了 `AppMode::Search`/`TagFilter`；改为调用 `app.exit_input_field()`：

```rust
fn handle_input_mouse(app: &mut App, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left)
        | MouseEventKind::Down(MouseButton::Right) => {
            // Any click while in InputActive → exit (buffer already applied on each keystroke).
            // Task 7/8 will improve this to click-through: clicking another input field switches focus.
            app.exit_input_field();
        }
        MouseEventKind::ScrollUp => app.move_up(SCROLL_LINES),
        MouseEventKind::ScrollDown => app.move_down(SCROLL_LINES),
        _ => {}
    }
}
```

Network 的 `/` 热键 和 `search_active` 机制先保留（Task 8 会统一替换），但 NetworkState 的 `search_active` 不再靠 AppMode 驱动。

- [ ] **Step 8: 编译**

Run: `cargo build`
Expected: 所有 rs 文件编译通过。可能遗漏的点：

- `session.rs`（如果 match AppMode 变体）
- 其他 ui 文件（logs/ 和 network/）如果有 `AppMode::Search` 或 `AppMode::TagFilter` 引用

用 grep 扫一下：

```bash
grep -rn "AppMode::Search\|AppMode::TagFilter" src/
```

若有残留，把匹配改为 `AppMode::InputActive(InputField::LogSearch)` 或 `AppMode::InputActive(_)` 的模式。

**注意**：`logs/mod.rs` 第 221 行 `let search_active = app.mode == AppMode::Search;` —— 改为：
```rust
let search_active = matches!(app.mode, AppMode::InputActive(crate::app::InputField::LogSearch));
```
类似地对 Tag filter 判断。

- [ ] **Step 9: 跑测试**

Run: `cargo test`
Expected: 全部 pass。测试里可能有 match AppMode 的地方，对应改。

- [ ] **Step 10: 提交**

```bash
git add src/app.rs src/event.rs src/ui/logs/mod.rs
git commit -m "$(cat <<'EOF'
refactor(app): merge Search/TagFilter into InputActive(InputField)

Introduces InputBuffers holding per-field buffer+cursor, plus enter_/apply_/exit_input_field
methods. Legacy enter_search etc. kept as thin shims to avoid churn in the keyboard
dispatch until later tasks.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: event.rs — 统一输入分发 + live apply + click 命中

**Files:**
- Modify: `src/event.rs`
- Modify: `src/app.rs` (删 legacy shims — 可选，留给 Task 9)

**背景**：Task 5 留下了过渡逻辑。这一步：
- Logs `/` / `T` 热键改为 `enter_input_field(LogSearch/LogTag)`（`E` 原本是 `export_logs`；为避免冲突，Exclude 框的热键用 `\`（反斜杠），和 Search 的 `/` 成对；若不方便也可以仅靠鼠标点击）
- Network `/` 热键改为 `enter_input_field(NetSearch)`
- 点击输入框：扩展 `handle_normal_mouse` 里的 toolbar 点击逻辑，按命中的 x 区间切到对应 `InputActive(field)`
- 点击外部：已在 Task 5 的 handle_input_mouse 做了（exit）
- Network 的 `search_active` bool 字段 **仍然保留**（Task 8 清理），但新逻辑写进 InputField 机制

- [ ] **Step 1: Logs 快捷键**

找到 `// Logs tab key handling` 下面的 match：

```rust
KeyCode::Char('/') => app.enter_search(),
```

改为：
```rust
KeyCode::Char('/') => app.enter_input_field(crate::app::InputField::LogSearch),
KeyCode::Char('\\') => app.enter_input_field(crate::app::InputField::LogExclude),
```

（原有 `Char('e') => app.export_logs()` 保留不动。）

检查是否还有其他处按 `T` 打开 tag filter：
```bash
grep -n "enter_tag_filter\|Char('t')\|Char('T')" src/event.rs
```
如有，改为 `app.enter_input_field(crate::app::InputField::LogTag)`。

- [ ] **Step 2: Network 快捷键**

找到：
```rust
KeyCode::Char('/') => {
    app.network.search_active = true;
    app.network.search_input = app.network.filter.search.clone();
}
```
替换为：
```rust
KeyCode::Char('/') => app.enter_input_field(crate::app::InputField::NetSearch),
KeyCode::Char('\\') => app.enter_input_field(crate::app::InputField::NetExclude),
```

在 Network key handling 的最开头（`// URL search input mode` 块）**整块删除**（那段 40 行：`if app.network.search_active { ... }`）—— 现在 InputActive 已在 `handle_key` 入口处分发到 `handle_input_key`，不会走到这个分支。

- [ ] **Step 3: Normal mode 鼠标 — Logs toolbar 点击命中**

找到 `handle_normal_mouse` 里的 Logs 分支。当前只有 Network 有 toolbar click 检测。需要加 Logs：

在 `// Check right-side buttons` 之前（或任意 Logs-specific 位置）插入：

```rust
    if app.active_tab == ViewTab::Logs {
        if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
            let x = mouse.column;
            let y = mouse.row;
            if y == app.layout.input_row_y {
                use crate::app::InputField;
                if x >= app.layout.log_search_x.0 && x < app.layout.log_search_x.1 {
                    app.enter_input_field(InputField::LogSearch);
                    return;
                }
                if x >= app.layout.log_exclude_x.0 && x < app.layout.log_exclude_x.1 {
                    app.enter_input_field(InputField::LogExclude);
                    return;
                }
                if x >= app.layout.log_tag_x.0 && x < app.layout.log_tag_x.1 {
                    app.enter_input_field(InputField::LogTag);
                    return;
                }
            }
        }
    }
```

- [ ] **Step 4: Normal mode 鼠标 — Network toolbar 扩 Exclude 命中**

找到 Network toolbar 已有的 click 块（约 137 行）：

```rust
if y == app.layout.net_toolbar_y
    && x >= app.layout.net_search_x.0
    && x < app.layout.net_search_x.1
{
    app.network.search_active = true;
    app.network.search_input = app.network.filter.search.clone();
    return;
}
```

替换为：
```rust
if y == app.layout.net_toolbar_y {
    use crate::app::InputField;
    if x >= app.layout.net_search_x.0 && x < app.layout.net_search_x.1 {
        app.enter_input_field(InputField::NetSearch);
        return;
    }
    if x >= app.layout.net_exclude_x.0 && x < app.layout.net_exclude_x.1 {
        app.enter_input_field(InputField::NetExclude);
        return;
    }
}
```

- [ ] **Step 5: handle_input_mouse — 点击其他输入框应切焦**

改进 Task 5 的 exit-on-any-click 逻辑：如果点在另一个输入框上，切焦而不是回 Normal。

替换 `handle_input_mouse`：

```rust
fn handle_input_mouse(app: &mut App, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let x = mouse.column;
            let y = mouse.row;
            if y == app.layout.input_row_y {
                use crate::app::InputField;
                if app.active_tab == ViewTab::Logs {
                    if x >= app.layout.log_search_x.0 && x < app.layout.log_search_x.1 {
                        app.enter_input_field(InputField::LogSearch);
                        return;
                    }
                    if x >= app.layout.log_exclude_x.0 && x < app.layout.log_exclude_x.1 {
                        app.enter_input_field(InputField::LogExclude);
                        return;
                    }
                    if x >= app.layout.log_tag_x.0 && x < app.layout.log_tag_x.1 {
                        app.enter_input_field(InputField::LogTag);
                        return;
                    }
                } else {
                    if x >= app.layout.net_search_x.0 && x < app.layout.net_search_x.1 {
                        app.enter_input_field(InputField::NetSearch);
                        return;
                    }
                    if x >= app.layout.net_exclude_x.0 && x < app.layout.net_exclude_x.1 {
                        app.enter_input_field(InputField::NetExclude);
                        return;
                    }
                }
            }
            // Click elsewhere → exit
            app.exit_input_field();
        }
        MouseEventKind::Down(MouseButton::Right) => app.exit_input_field(),
        MouseEventKind::ScrollUp => app.move_up(SCROLL_LINES),
        MouseEventKind::ScrollDown => app.move_down(SCROLL_LINES),
        _ => {}
    }
}
```

- [ ] **Step 6: 编译**

Run: `cargo build`
Expected: 编译通过。

- [ ] **Step 7: 手动冒烟（可选）**

Run: `cargo run -- --port 9753`（无需真实 Flutter 连接，只检测 UI 不崩）
在 logs tab 按 `/`、`\`、点击，看行为正常即可退出。

- [ ] **Step 8: 提交**

```bash
git add src/event.rs
git commit -m "$(cat <<'EOF'
feat(event): unified input dispatch with live apply and click-to-focus

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: Logs toolbar — 接入 input_field 组件，新布局

**Files:**
- Modify: `src/ui/logs/mod.rs`

**背景**：重写 `draw_toolbar_op1` 和 `draw_toolbar_op2`。op1 = 3 个输入框均分；op2 = levels + bookmarks + count 右对齐。`input_row_y` 记录到 layout；`log_search_x` / `log_exclude_x` / `log_tag_x` 也记录。

- [ ] **Step 1: 更新 draw_logs 的 rows 和 layout 写回**

找 `pub fn draw_logs` 里：
```rust
app.layout.toolbar_y = rows[1].y; // op row 1 (search)
app.layout.toolbar_op2_y = rows[2].y; // op row 2 (tag + levels)
```
改为：
```rust
app.layout.toolbar_y = rows[1].y;
app.layout.toolbar_op2_y = rows[2].y;
app.layout.input_row_y = rows[1].y;
```

（rows 结构不变：sep / op1 / op2 / sep / col_header / main / status。）

- [ ] **Step 2: 替换 draw_toolbar_op1**

全部替换为：

```rust
fn draw_toolbar_op1(f: &mut Frame, app: &mut App, area: Rect) {
    use crate::app::InputField;
    use crate::ui::input_field::{render_input_field, InputFieldProps};

    let bg = MANTLE;
    let w = area.width as u16;

    // Split width into 3 equal-ish slices for Search | Exclude | Tag, with 1-col gaps.
    let gap: u16 = 1;
    let inner = w.saturating_sub(gap * 2);
    let per = inner / 3;
    let rem = inner - per * 3;
    let widths = [per + (if rem > 0 { 1 } else { 0 }), per + (if rem > 1 { 1 } else { 0 }), per];

    let mut spans: Vec<Span> = Vec::new();
    let mut x: u16 = 0;

    let fields: [(InputField, &str, &str); 3] = [
        (InputField::LogSearch, "Search", "(a|b)"),
        (InputField::LogExclude, "Exclude", "(a|b)"),
        (InputField::LogTag, "Tag", "(+a|-b)"),
    ];

    for (i, (field, label, hint)) in fields.iter().enumerate() {
        let active = matches!(app.mode, AppMode::InputActive(f) if f == *field);
        let value = app.inputs.buffer(*field).to_string();
        let cursor_byte = app.inputs.cursor(*field);

        let out = render_input_field(
            InputFieldProps {
                label,
                hint,
                value: &value,
                active,
                cursor_byte,
                total_width: widths[i],
            },
            x,
        );

        // Store hit region
        match field {
            InputField::LogSearch => app.layout.log_search_x = out.hit_x,
            InputField::LogExclude => app.layout.log_exclude_x = out.hit_x,
            InputField::LogTag => app.layout.log_tag_x = out.hit_x,
            _ => {}
        }

        spans.extend(out.spans);
        x += out.used_width;

        if i < 2 {
            spans.push(Span::styled(" ".repeat(gap as usize), Style::default().bg(bg)));
            x += gap;
        }
    }

    // Pad remaining
    let used: u16 = spans.iter().map(|s| s.content.width() as u16).sum();
    if used < w {
        spans.push(Span::styled(
            " ".repeat((w - used) as usize),
            Style::default().bg(bg),
        ));
    }

    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)),
        area,
    );
}
```

- [ ] **Step 3: 替换 draw_toolbar_op2 — levels + bookmarks + count**

```rust
fn draw_toolbar_op2(f: &mut Frame, app: &mut App, area: Rect) {
    let bg = MANTLE;
    let mut spans: Vec<Span> = Vec::new();
    let mut x: u16 = 0;

    spans.push(Span::styled(" ", Style::default().bg(bg)));
    x += 1;

    app.layout.levels_x = x;
    for (label, level) in &[
        ("S", LogLevel::System),
        ("V", LogLevel::Verbose),
        ("D", LogLevel::Debug),
        ("I", LogLevel::Info),
        ("W", LogLevel::Warning),
        ("E", LogLevel::Error),
    ] {
        let (fg, bg_c, bold) = level_badge(*level);
        let style = if app.filter.min_level == *level {
            let mut s = Style::default().fg(fg).bg(if bg_c == Color::Reset { SURFACE1 } else { bg_c });
            if bold {
                s = s.add_modifier(Modifier::BOLD);
            }
            s
        } else if app.filter.min_level > *level {
            Style::default().fg(SURFACE0).bg(bg).add_modifier(Modifier::DIM)
        } else {
            Style::default().fg(level_color(*level)).bg(bg)
        };
        spans.push(Span::styled(format!(" {} ", label), style));
        x += 3;
    }

    spans.push(Span::styled("   │   ", Style::default().fg(SURFACE1).bg(bg)));
    x += 7;

    if !app.bookmarks.is_empty() {
        let bm = format!("●{}", app.bookmarks.len());
        x += bm.width() as u16;
        spans.push(Span::styled(bm, Style::default().fg(YELLOW).bg(bg)));
    }

    // Right-align count + sparkline
    let count_text = format!(" {}/{} ", app.filtered_count(), app.store.len());
    let cw = count_text.width() as u16;
    let pad = area.width.saturating_sub(x + cw);
    spans.push(Span::styled(" ".repeat(pad as usize), Style::default().bg(bg)));
    spans.push(Span::styled(count_text, Style::default().fg(SUBTEXT0).bg(bg)));

    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)),
        area,
    );
}
```

**注意**：原 op1 有 sparkline + match 导航（`<` `>` `N/match`），如果用户用 `n`/`N` 跳转匹配，仍在工作，只是可视元素去掉了。必要时后续任务再把 sparkline 搬到 op2 右侧。

- [ ] **Step 4: 清除对 `app.search.input` 的 placeholder 依赖**

原 op1 里绘制 `format!("{}_", app.search.input)`。新代码从 `app.inputs.buffer(LogSearch)` 拿值，`app.search.input` 不再使用（但 `app.search.matches` 跳转功能仍用）。

搜一下 `logs/mod.rs` 里是否还有 `AppMode::Search` / `AppMode::TagFilter` 字面量：
```bash
grep -n "AppMode::Search\|AppMode::TagFilter\|search\.input\|tag_filter\.input" src/ui/logs/mod.rs
```
将残留改为 `matches!(app.mode, AppMode::InputActive(crate::app::InputField::LogSearch))` 这种写法。

- [ ] **Step 5: 更新 no_matching_logs — 显示 exclude 行**

找 `draw_no_matching_logs`，在 `filter_rows` 构造块里（`if app.filter.min_level != LogLevel::System` 下面）加：

```rust
    if !app.filter.exclude_query.is_empty() {
        filter_rows.push(format!("    exclude: \"{}\"", app.filter.exclude_query));
    }
```

- [ ] **Step 6: 编译 + 运行测试**

Run: `cargo build && cargo test`
Expected: 全部 pass

- [ ] **Step 7: 手动验收 (optional but recommended)**

启动 `cargo run`，观察 Logs tab：
- op1 三个输入框并排
- 点击 Search 框激活（有光标 `_`）
- 敲字立即过滤
- 点击列表区域，取消激活
- 点击 Exclude 框切焦
- op2 应该显示 levels pills 和 count

- [ ] **Step 8: 提交**

```bash
git add src/ui/logs/mod.rs
git commit -m "$(cat <<'EOF'
feat(ui/logs): 3-input toolbar using input_field component

Row 1 now hosts Search / Exclude / Tag as three equal-width input fields,
row 2 moves to levels + bookmarks + count.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: Network toolbar — 接入 input_field 组件

**Files:**
- Modify: `src/ui/network/filter.rs`
- Modify: `src/app.rs` (删 NetworkState.search_active / search_input 可选)
- Modify: `src/ui/network/mod.rs` (input_row_y 写回)

- [ ] **Step 1: 在 network/mod.rs 写回 input_row_y**

找到 `app.layout.net_toolbar_y = rows[1].y;`，下一行加：
```rust
app.layout.input_row_y = rows[1].y;
```

- [ ] **Step 2: 替换 draw_network_op1**

全部替换：

```rust
pub fn draw_network_op1(f: &mut Frame, app: &mut App, area: Rect, count: usize, total: usize) {
    use crate::app::{AppMode, InputField};
    use crate::ui::input_field::{render_input_field, InputFieldProps};

    let bg = MANTLE;
    let w = area.width as u16;

    // Reserve right side for count text
    let count_text = format!(" {}/{} ", count, total);
    let cw = count_text.width() as u16;

    let avail = w.saturating_sub(cw + 1);
    let gap: u16 = 1;
    let inner = avail.saturating_sub(gap);
    let per = inner / 2;
    let widths = [per, inner - per];

    let mut spans: Vec<Span> = Vec::new();
    let mut x: u16 = 0;

    let fields: [(InputField, &str, &str); 2] = [
        (InputField::NetSearch, "Search", "(a|b)"),
        (InputField::NetExclude, "Exclude", "(a|b)"),
    ];

    for (i, (field, label, hint)) in fields.iter().enumerate() {
        let active = matches!(app.mode, AppMode::InputActive(f) if f == *field);
        let value = app.inputs.buffer(*field).to_string();
        let cursor_byte = app.inputs.cursor(*field);

        let out = render_input_field(
            InputFieldProps {
                label,
                hint,
                value: &value,
                active,
                cursor_byte,
                total_width: widths[i],
            },
            x,
        );

        match field {
            InputField::NetSearch => app.layout.net_search_x = out.hit_x,
            InputField::NetExclude => app.layout.net_exclude_x = out.hit_x,
            _ => {}
        }

        spans.extend(out.spans);
        x += out.used_width;

        if i < 1 {
            spans.push(Span::styled(" ".repeat(gap as usize), Style::default().bg(bg)));
            x += gap;
        }
    }

    // Pad then count
    let used: u16 = spans.iter().map(|s| s.content.width() as u16).sum();
    let pad = w.saturating_sub(used + cw);
    spans.push(Span::styled(" ".repeat(pad as usize), Style::default().bg(bg)));
    spans.push(Span::styled(count_text, Style::default().fg(SUBTEXT0).bg(bg)));

    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)),
        area,
    );
}
```

**注意**：原 signature 是 `draw_network_op1(f, app, area, count, total)` —— 保留不变。

- [ ] **Step 3: 清理未使用的 imports**

`use crate::app::App;` 保留；可能需要删除 `safe_pad`、`ProtocolFilter` 等在 op1 已不用的（如 clippy 警告再删）。

- [ ] **Step 4: 编译**

Run: `cargo build`
Expected: OK

- [ ] **Step 5: NetworkState.search_active/search_input 的处理**

这两个字段从 Task 6 开始不再被键盘/鼠标路径写入。渲染层（`draw_network_op1` 新版本）也不再读。**保留字段**（防止 breaking），但标记弃用可选。

先**不动**，等后续 simplify pass。

- [ ] **Step 6: 手动冒烟（可选）**

跑 `cargo run`，切到 Network tab，看 2 个输入框渲染正常，点击激活/输入生效。

- [ ] **Step 7: 提交**

```bash
git add src/ui/network/filter.rs src/ui/network/mod.rs
git commit -m "$(cat <<'EOF'
feat(ui/network): 2-input toolbar using input_field component (Search + Exclude)

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: Help 页面 — 补充 Search & Filter 详细语法

**Files:**
- Modify: `src/ui/help.rs`

**背景**：Hint 里只写最简提示（`(a|b)` / `(+a|-b)`）。Help 页面的 "Search & Filter" 小节要说清完整用法：regex 语法、Tag `+/-` 前缀、实时生效、点击/失焦行为、Exclude 的作用。

- [ ] **Step 1: 定位现有 Search & Filter 小节**

`src/ui/help.rs` 第 206 行附近是：
```rust
lines.push(subheading("\u{1f50d} Search & Filter"));
lines.push(Line::from(vec![
    Span::raw("    "),
    dim("Search:  "),
    key("/"),
    dim(" type query "),
    key("Enter"),
    dim("    /regex/i for case-insensitive regex"),
]));
// ... Tag 行类似，说 "comma-separated, - to exclude"
```

把它从第 206 行到 `blank()` 之前（约第 252 行）整段替换。

- [ ] **Step 2: 替换为新文案**

```rust
    lines.push(subheading("\u{1f50d} Search & Filter"));

    // Row 1 description
    lines.push(Line::from(vec![
        Span::raw("    "),
        dim("Row 1 hosts three input fields: "),
        Span::styled("Search", Style::default().fg(YELLOW)),
        dim(" / "),
        Span::styled("Exclude", Style::default().fg(YELLOW)),
        dim(" / "),
        Span::styled("Tag", Style::default().fg(YELLOW)),
        dim("."),
    ]));
    lines.push(blank());

    // Activation
    lines.push(Line::from(vec![
        Span::raw("    "),
        dim("Click a field or press "),
        key("/"),
        dim(" (Search) / "),
        key("\\"),
        dim(" (Exclude) / "),
        key("t"),
        dim(" (Tag) to activate."),
    ]));
    lines.push(Line::from(vec![
        Span::raw("    "),
        dim("Typing filters the list "),
        Span::styled("live", Style::default().fg(GREEN)),
        dim(" — no Enter needed. Click outside or press "),
        key("Esc"),
        dim(" to blur."),
    ]));
    lines.push(blank());

    // Syntax — Search / Exclude
    lines.push(Line::from(vec![
        Span::raw("    "),
        Span::styled("Search / Exclude syntax", Style::default().fg(SAPPHIRE)),
    ]));
    lines.push(Line::from(vec![
        Span::raw("      "),
        Span::styled("a|b|c", Style::default().fg(GREEN)),
        dim("            OR match — any of the terms (plain substring)"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("      "),
        Span::styled("/pattern/", Style::default().fg(GREEN)),
        dim("        regex mode — pipe is passed through to the engine"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("      "),
        Span::styled("/pattern/i", Style::default().fg(GREEN)),
        dim("       regex, case-insensitive"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("      "),
        dim("Exclude drops any row that matches (inverse of Search)."),
    ]));
    lines.push(blank());

    // Syntax — Tag
    lines.push(Line::from(vec![
        Span::raw("    "),
        Span::styled("Tag syntax", Style::default().fg(SAPPHIRE)),
    ]));
    lines.push(Line::from(vec![
        Span::raw("      "),
        Span::styled("+network|-flog_net", Style::default().fg(GREEN)),
        dim("   include network, exclude flog_net (pipe-separated)"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("      "),
        dim("Tag match is exact (case-insensitive); use regex via "),
        Span::styled("*", Style::default().fg(YELLOW)),
        dim(" or "),
        Span::styled(".", Style::default().fg(YELLOW)),
        dim(" in the pattern."),
    ]));
    lines.push(blank());

    // Level
    lines.push(Line::from(vec![
        Span::raw("    "),
        dim("Level:   click "),
        Span::styled(" S ", Style::default().fg(OVERLAY0).bg(SURFACE0)),
        Span::styled(" V ", Style::default().fg(OVERLAY0).bg(SURFACE0)),
        Span::styled(" D ", Style::default().fg(TEXT).bg(SURFACE0)),
        Span::styled(
            " I ",
            Style::default().fg(MANTLE).bg(BLUE).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " W ",
            Style::default().fg(MANTLE).bg(YELLOW).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " E ",
            Style::default().fg(MANTLE).bg(RED).add_modifier(Modifier::BOLD),
        ),
        dim("  (row 2) to set minimum level"),
    ]));
    lines.push(blank());
```

- [ ] **Step 3: 编译**

Run: `cargo build`
Expected: 编译通过；help.rs 里用的 Color/Style/Modifier/Span 常量都在作用域内。

- [ ] **Step 4: 手动看 help 页面**

Run: `cargo run`，在 app 里按 `?` 打开 Help，翻到 "Search & Filter" 小节确认可读、无乱码。

- [ ] **Step 5: 提交**

```bash
git add src/ui/help.rs
git commit -m "$(cat <<'EOF'
docs(help): expand Search & Filter section with full syntax + activation rules

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: Session 持久化 + cleanup

**Files:**
- Modify: `src/session.rs`
- Modify: `src/app.rs` (可选：删 legacy shim 和 SearchState.input / TagFilterInput)

**背景**：Session 加 `exclude` 字段。若之前的 session 文件没有这字段，serde 默认行为是报错；需用 `#[serde(default)]`。

- [ ] **Step 1: 看看 session.rs 的 filter 结构**

```bash
grep -n "search_query\|min_level\|tag_include\|exclude" src/session.rs
```

如果 session 有 `search_query` 字段，平行加 `exclude_query`。

- [ ] **Step 2: 增字段（示例）**

如 session 里有：
```rust
#[derive(Serialize, Deserialize)]
struct LogFilterSession {
    min_level: String,
    search_query: String,
    tag_include: Vec<String>,
    tag_exclude: Vec<String>,
}
```
改为：
```rust
#[derive(Serialize, Deserialize)]
struct LogFilterSession {
    min_level: String,
    search_query: String,
    #[serde(default)]
    exclude_query: String,
    tag_include: Vec<String>,
    tag_exclude: Vec<String>,
}
```

若 Network 的 session 也存 `search`，同样加 `#[serde(default)] exclude: String`。

在 save/load 函数里对应 get/set `self.filter.exclude_query` 和 `self.network.filter.exclude`。

**如果 session.rs 没有持久化 filter 的 search 字段**（可能只存了 bookmarks/active_tab），那就**跳过这步**。grep 结果为空表明不需要改。

- [ ] **Step 3: 编译 + 测试**

Run: `cargo build && cargo test`

- [ ] **Step 4: 跑 clippy**

Run: `cargo clippy --no-deps -- -D warnings`
Expected: 干净通过；如有 warning 简单修（例如 unused import）。

- [ ] **Step 5: 提交**

```bash
git add src/session.rs
git commit -m "$(cat <<'EOF'
feat(session): persist exclude filter with serde default for backward compat

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Task 11: 最终手动验收清单

**Files:** (无代码改动，只是清单)

- [ ] **Step 1: 启动 flog 连真实 Flutter 应用**

Run: `cargo run`
连一个跑着 flog_dart 的 app，确保日志/请求流入。

- [ ] **Step 2: Logs tab 验收**

- [ ] 3 个输入框在第 1 行并排
- [ ] 未激活 + 空：显示 hint `(a|b)` / `(+a|-b)`，bg 较暗
- [ ] 有内容 + 未激活：文字 YELLOW，bg SURFACE0
- [ ] 激活：bg SURFACE1，光标 `_` 可见
- [ ] 点击框 → 激活
- [ ] 点击列表 → 失焦
- [ ] 输入 `error` 即时过滤（不按 Enter）
- [ ] 输入 `timeout|500` 过滤包含两者之一的行
- [ ] 输入 `/^\[ERROR\]/` 启用 regex 模式
- [ ] Exclude 框输入 `heartbeat` 过滤掉相关行
- [ ] Search + Exclude 同时工作（交集）
- [ ] Tag 框 `+network|-flog_net` 只显示 network，排除 flog_net
- [ ] Esc / Enter 失焦但保留内容
- [ ] 超长内容：光标始终可见（active），idle 时显示 `abc…`

- [ ] **Step 3: Network tab 验收**

- [ ] 2 个输入框在第 1 行
- [ ] 三态背景正常
- [ ] Search `users|orders` OR 过滤
- [ ] Exclude `heartbeat` 过滤
- [ ] 点击切焦 Search ↔ Exclude

- [ ] **Step 4: 窄终端**

调终端宽度到 80 col，确认布局不溢出、不崩。

- [ ] **Step 5: 提交 QA 记录（可选）**

```bash
git commit --allow-empty -m "$(cat <<'EOF'
chore: manual QA pass on unified input fields

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## Self-Review Notes

- **Spec coverage**：5 个目标（统一组件、Exclude、第1行布局、实时生效、三态背景、多项OR）全部有对应 Task
- **Tasks 编号**：1–3 域层，4 新组件，5–6 状态/事件重构，7–8 UI 接入，9 Help 页，10 Session 收尾，11 验收
- **Type consistency**：`InputField` 枚举只在 Task 5 定义一次，后续都引用 `crate::app::InputField`；`apply_input_field` / `enter_input_field` / `exit_input_field` 三个方法名从 Task 5 开始一致
- **No placeholders**：每个 Step 都有可运行的代码或命令
- **风险已标**：窄终端、sparkline/match导航暂时从 op1 移除、Network.search_active 过渡保留 —— 都在相应 Task 说明
