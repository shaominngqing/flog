# Structured Parser Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore the logs-panel auto-format behavior for Dart `Map.toString()`-style messages by introducing a tolerant text-to-`serde_json::Value` parser, without contaminating the `json_viewer` module's JSON-only contract.

**Architecture:** New `src/domain/structured_parser.rs` tries strict JSON first, falls back to a tolerant Dart-Map parser, returns `Option<Value>`. `json_viewer::tree` gets a new `Tree::from_value(&Value)` entry point. Logs + Network detail panels switch from `json_viewer::parse(text)` to `structured_parser::parse(text).map(Tree::from_value)`.

**Tech Stack:** Rust, `serde_json`.

**Spec:** `docs/superpowers/specs/2026-04-21-structured-parser-design.md`

---

## File Structure

- `src/domain/structured_parser.rs` — NEW. Public `parse(&str) -> Option<Value>`. Private tolerant parser state machine.
- `src/domain/mod.rs` — add `pub mod structured_parser;` + the flat `parse` re-export.
- `src/ui/json_viewer/tree.rs` — refactor `parse(text)` to delegate to new `Tree::from_value(&Value)` helper; `from_value` is the new primary API.
- `src/ui/logs/detail.rs` — swap `json_viewer::parse(&full_msg)` for `structured_parser::parse(&full_msg).map(|v| Tree::from_value(&v))` flow.
- `src/ui/network/detail.rs::render_json_section` — same swap.

---

## Task 1: Refactor `Tree::parse` to expose `from_value`

**Files:**
- Modify: `src/ui/json_viewer/tree.rs`

The current `tree.rs::parse(text)` does two things in one call: `serde_json::from_str(text)` + `build(&value, …)`. Task 2 needs the second half as a standalone constructor. This task splits without changing behavior.

- [ ] **Step 1: Add `Tree::from_value` and refactor `parse` to delegate**

Open `src/ui/json_viewer/tree.rs`. Find the current `parse` function (around line 50):

```rust
pub fn parse(text: &str) -> Result<Tree, serde_json::Error> {
    let value: Value = serde_json::from_str(text)?;
    let mut nodes: Vec<FlatNode> = Vec::new();
    build(&value, None, None, 0, &mut nodes);
    Ok(Tree { nodes })
}
```

Replace with:

```rust
pub fn parse(text: &str) -> Result<Tree, serde_json::Error> {
    let value: Value = serde_json::from_str(text)?;
    Ok(Tree::from_value(&value))
}
```

And inside `impl Tree`, add (right after the existing methods):

```rust
    /// Construct a tree from an already-parsed `serde_json::Value`.
    ///
    /// Used by `parse(text)` (for strict JSON input) and by callers that
    /// obtain a `Value` by some other means (e.g. a tolerant parser for
    /// Dart `Map.toString()` output in log messages).
    pub fn from_value(value: &Value) -> Tree {
        let mut nodes: Vec<FlatNode> = Vec::new();
        build(value, None, None, 0, &mut nodes);
        Tree { nodes }
    }
```

- [ ] **Step 2: Run existing tree tests to verify behavior unchanged**

Run: `cargo test --lib ui::json_viewer::tree`
Expected: 7 tests pass (same as before). The refactor is behavior-preserving.

- [ ] **Step 3: Run full suite**

Run: `cargo test --lib`
Expected: 138 tests pass (unchanged).

- [ ] **Step 4: Commit**

```bash
git add src/ui/json_viewer/tree.rs
git commit -m "refactor(json_viewer): split tree::parse into parse + Tree::from_value"
```

---

## Task 2: Implement `structured_parser` module

**Files:**
- Create: `src/domain/structured_parser.rs`
- Modify: `src/domain/mod.rs`

A two-stage parser: strict JSON first, then a tolerant Dart-Map parser for the fallback path. Returns `Option<serde_json::Value>`.

- [ ] **Step 1: Write the module**

Create `src/domain/structured_parser.rs` with this exact content:

```rust
//! Tolerant text-to-Value parser.
//!
//! Given arbitrary text, try to extract a `serde_json::Value`:
//!   1. Strict JSON via `serde_json::from_str`.
//!   2. Fallback: tolerant Dart `Map.toString()` format
//!      (unquoted keys, unquoted string values).
//!
//! Returns `None` if neither produces a value — caller falls back to
//! plain text rendering.
//!
//! Example tolerant inputs:
//!   `{code: 0, message: ok}` → `{"code": 0, "message": "ok"}`
//!   `{user: {id: 1, name: alice}, tags: [a, b, c]}`
//!
//! If the text embeds a structured value after a prefix
//! (e.g. `Response: {…}`), the first `{` or `[` in the text is the
//! start of the structured region.

use serde_json::{Map, Number, Value};

/// Best-effort parse. See module doc.
pub fn parse(text: &str) -> Option<Value> {
    // Locate the start of the structured region.
    let start = text.find(['{', '['])?;
    let payload = &text[start..];

    // 1. Strict JSON first.
    if let Ok(v) = serde_json::from_str::<Value>(payload) {
        return Some(v);
    }

    // 2. Tolerant fallback.
    let mut p = Parser::new(payload);
    let v = p.parse_value()?;
    p.skip_whitespace();
    if p.pos != p.src.len() {
        // Extra trailing junk → prefer plain-text fallback in caller.
        return None;
    }
    Some(v)
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(s: &'a str) -> Self {
        Parser { src: s.as_bytes(), pos: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.pos += 1;
        Some(b)
    }

    fn skip_whitespace(&mut self) {
        while let Some(b) = self.peek() {
            if b.is_ascii_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn expect(&mut self, b: u8) -> Option<()> {
        self.skip_whitespace();
        if self.peek() == Some(b) {
            self.pos += 1;
            Some(())
        } else {
            None
        }
    }

    fn parse_value(&mut self) -> Option<Value> {
        self.skip_whitespace();
        match self.peek()? {
            b'{' => self.parse_object(),
            b'[' => self.parse_array(),
            _ => None,
        }
    }

    fn parse_object(&mut self) -> Option<Value> {
        self.expect(b'{')?;
        let mut map = Map::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Some(Value::Object(map));
        }
        loop {
            self.skip_whitespace();
            let key = self.parse_key()?;
            self.expect(b':')?;
            let value = self.parse_entry_value()?;
            map.insert(key, value);
            self.skip_whitespace();
            match self.peek()? {
                b',' => {
                    self.pos += 1;
                    self.skip_whitespace();
                    // Allow trailing comma before closer.
                    if self.peek() == Some(b'}') {
                        self.pos += 1;
                        return Some(Value::Object(map));
                    }
                }
                b'}' => {
                    self.pos += 1;
                    return Some(Value::Object(map));
                }
                _ => return None,
            }
        }
    }

    fn parse_array(&mut self) -> Option<Value> {
        self.expect(b'[')?;
        let mut arr = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Some(Value::Array(arr));
        }
        loop {
            let value = self.parse_entry_value()?;
            arr.push(value);
            self.skip_whitespace();
            match self.peek()? {
                b',' => {
                    self.pos += 1;
                    self.skip_whitespace();
                    if self.peek() == Some(b']') {
                        self.pos += 1;
                        return Some(Value::Array(arr));
                    }
                }
                b']' => {
                    self.pos += 1;
                    return Some(Value::Array(arr));
                }
                _ => return None,
            }
        }
    }

    /// Parse an object key. Supports quoted (`"foo"`) and unquoted
    /// (Dart identifier chars + `.` and `-`) keys.
    fn parse_key(&mut self) -> Option<String> {
        self.skip_whitespace();
        if self.peek() == Some(b'"') {
            return self.parse_quoted_string();
        }
        let start = self.pos;
        while let Some(b) = self.peek() {
            if b == b':' || b.is_ascii_whitespace() {
                break;
            }
            if !is_key_char(b) {
                return None;
            }
            self.pos += 1;
        }
        if self.pos == start {
            return None;
        }
        Some(std::str::from_utf8(&self.src[start..self.pos]).ok()?.to_string())
    }

    /// Parse a value inside an object entry or array element. Delegates
    /// to nested object/array parsing or to `parse_bare_value`.
    fn parse_entry_value(&mut self) -> Option<Value> {
        self.skip_whitespace();
        match self.peek()? {
            b'{' => self.parse_object(),
            b'[' => self.parse_array(),
            b'"' => self.parse_quoted_string().map(Value::String),
            _ => self.parse_bare_value(),
        }
    }

    /// Parse a bare value up to the next `,` / `}` / `]` at the current
    /// nesting level. Recognizes `null`, `true`, `false`, integers, floats.
    /// Anything else becomes a trimmed `Value::String`.
    fn parse_bare_value(&mut self) -> Option<Value> {
        let start = self.pos;
        while let Some(b) = self.peek() {
            if matches!(b, b',' | b'}' | b']') {
                break;
            }
            self.pos += 1;
        }
        let raw = std::str::from_utf8(&self.src[start..self.pos]).ok()?.trim();
        if raw.is_empty() {
            return None;
        }
        Some(classify_bare(raw))
    }

    /// Parse a JSON-style quoted string with `\` escape handling.
    fn parse_quoted_string(&mut self) -> Option<String> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let b = self.bump()?;
            match b {
                b'"' => return Some(out),
                b'\\' => {
                    let esc = self.bump()?;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'n' => out.push('\n'),
                        b't' => out.push('\t'),
                        b'r' => out.push('\r'),
                        b'b' => out.push('\u{0008}'),
                        b'f' => out.push('\u{000C}'),
                        _ => {
                            // Unknown escape — keep literal.
                            out.push('\\');
                            out.push(esc as char);
                        }
                    }
                }
                _ => out.push(b as char),
            }
        }
    }
}

fn is_key_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'$' | b'.' | b'-')
}

fn classify_bare(raw: &str) -> Value {
    match raw {
        "null" => Value::Null,
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => {
            if let Ok(n) = raw.parse::<i64>() {
                return Value::Number(Number::from(n));
            }
            if let Ok(n) = raw.parse::<u64>() {
                return Value::Number(Number::from(n));
            }
            if let Ok(f) = raw.parse::<f64>() {
                if let Some(n) = Number::from_f64(f) {
                    return Value::Number(n);
                }
            }
            Value::String(raw.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_str(s: &str) -> Value {
        parse(s).unwrap_or_else(|| panic!("parse failed for: {:?}", s))
    }

    #[test]
    fn strict_json_still_parses() {
        let v = parse_str(r#"{"code": 0, "message": "ok"}"#);
        assert_eq!(v["code"], 0);
        assert_eq!(v["message"], "ok");
    }

    #[test]
    fn dart_map_unquoted_keys_and_strings() {
        let v = parse_str("{code: 0, message: ok}");
        assert_eq!(v["code"], 0);
        assert_eq!(v["message"], "ok");
    }

    #[test]
    fn nested_dart_map() {
        let v = parse_str("{user: {id: 1, name: alice}}");
        assert_eq!(v["user"]["id"], 1);
        assert_eq!(v["user"]["name"], "alice");
    }

    #[test]
    fn dart_array() {
        let v = parse_str("{items: [1, 2, 3]}");
        assert_eq!(v["items"][0], 1);
        assert_eq!(v["items"][2], 3);
    }

    #[test]
    fn empty_containers() {
        assert_eq!(parse_str("{}"), Value::Object(Map::new()));
        assert_eq!(parse_str("[]"), Value::Array(vec![]));
    }

    #[test]
    fn mixed_quoted_and_bare() {
        let v = parse_str(r#"{msg: "has, comma", count: 5}"#);
        assert_eq!(v["msg"], "has, comma");
        assert_eq!(v["count"], 5);
    }

    #[test]
    fn prefix_before_object() {
        let v = parse_str("Response: {code: 0}");
        assert_eq!(v["code"], 0);
    }

    #[test]
    fn gibberish_returns_none() {
        assert!(parse("not structured").is_none());
    }

    #[test]
    fn unbalanced_returns_none() {
        assert!(parse("{unclosed").is_none());
    }

    #[test]
    fn typed_bare_literals() {
        let v = parse_str("{a: null, b: true, c: false, d: 3.14}");
        assert!(v["a"].is_null());
        assert_eq!(v["b"], true);
        assert_eq!(v["c"], false);
        assert!((v["d"].as_f64().unwrap() - 3.14).abs() < 1e-9);
    }

    #[test]
    fn whitespace_tolerance() {
        let v = parse_str("{ foo :  bar  ,  baz : 2 }");
        assert_eq!(v["foo"], "bar");
        assert_eq!(v["baz"], 2);
    }

    #[test]
    fn trailing_garbage_returns_none() {
        // `{a: 1}` is valid, but trailing junk after the closer fails.
        assert!(parse("{a: 1} trailing").is_none());
    }

    #[test]
    fn text_without_brackets_returns_none() {
        assert!(parse("hello world").is_none());
    }
}
```

- [ ] **Step 2: Register the module**

Open `src/domain/mod.rs`. Find the existing module declarations (e.g. `pub mod entry;`, `pub mod store;`, etc.) and add alphabetically-appropriate position:

```rust
pub mod structured_parser;
```

If `domain/mod.rs` has a `pub use` block that flattens its submodules' public API (check what's there), there's no need to add `structured_parser` to it — callers will use the qualified path `crate::domain::structured_parser::parse`.

- [ ] **Step 3: Run the new tests**

Run: `cargo test --lib domain::structured_parser`
Expected: 12 tests pass.

- [ ] **Step 4: Run full suite**

Run: `cargo test --lib`
Expected: 150 tests pass (138 + 12 new).

- [ ] **Step 5: Commit**

```bash
git add src/domain/structured_parser.rs src/domain/mod.rs
git commit -m "feat(domain): tolerant structured parser for Dart Map.toString inputs"
```

---

## Task 3: Wire the detail panels to the new pipeline

**Files:**
- Modify: `src/ui/logs/detail.rs`
- Modify: `src/ui/network/detail.rs`

Both detail renderers currently call `json_viewer::parse(text)` directly (strict JSON). Swap to `structured_parser::parse(text) → Tree::from_value(&value)` so Dart-Map inputs also work.

- [ ] **Step 1: Update `src/ui/logs/detail.rs`**

Open the file. Find the body render block (around line 126):

```rust
    match json_viewer::parse(&full_msg) {
        Ok(tree) => {
            if entry_changed || app.detail.viewer_state.expanded.len() != tree.nodes.len() {
                app.detail.viewer_state = json_viewer::init_state(&tree, 1);
                app.detail.viewer_text_fingerprint = fingerprint;
            }
            // ... existing rendering code
```

Replace the `match` line and the `Ok(tree)` arm opener with the new pipeline. The full block becomes:

```rust
    match crate::domain::structured_parser::parse(&full_msg) {
        Some(value) => {
            let tree = json_viewer::Tree::from_value(&value);
            if entry_changed || app.detail.viewer_state.expanded.len() != tree.nodes.len() {
                app.detail.viewer_state = json_viewer::init_state(&tree, 1);
                app.detail.viewer_text_fingerprint = fingerprint;
            }

            let body_height = inner_h.saturating_sub(all_lines.len());
            let mut body_click_map: Vec<Option<(String, u32)>> = Vec::new();
            let mut body_lines: Vec<Line<'static>> = Vec::new();
            json_viewer::append_render(
                &mut body_lines,
                &mut body_click_map,
                &tree,
                &app.detail.viewer_state,
                "log_detail",
                "",
                inner_w,
            );

            let full_body_len = body_lines.len();
            let scroll = app.detail.scroll.min(full_body_len);
            app.detail.viewer_click_map = body_click_map
                .iter()
                .skip(scroll)
                .take(body_height)
                .map(|slot| slot.as_ref().map(|(_, id)| *id))
                .collect();

            let visible: Vec<Line<'static>> = body_lines
                .into_iter()
                .skip(scroll)
                .take(body_height)
                .collect();
            all_lines.extend(visible);

            app.detail.viewer_tree = Some(tree);
            total_content = app.detail.header_lines + full_body_len;
        }
        None => {
            for wl in crate::ui::wrap_text(&full_msg, inner_w, 500) {
                all_lines.push(Line::from(Span::styled(
                    wl,
                    Style::default().fg(TEXT),
                )));
            }
            app.detail.viewer_tree = None;
            total_content = all_lines.len();
        }
    }
```

The two changes from the old code are:
1. `match json_viewer::parse(&full_msg)` → `match crate::domain::structured_parser::parse(&full_msg)`
2. `Ok(tree)` / `Err(_)` → `Some(value)` / `None`
3. Added one line inside `Some(value)`: `let tree = json_viewer::Tree::from_value(&value);`

Everything else stays identical.

- [ ] **Step 2: Update `src/ui/network/detail.rs::render_json_section`**

Find the function (around line 920):

```rust
fn render_json_section(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Option<(String, u32)>>,
    json_text: &str,
    section_key: &str,
    viewer_states: &mut HashMap<String, JsonViewerState>,
    max_w: usize,
) {
    match json_viewer::parse(json_text) {
        Ok(tree) => {
            let state = viewer_states
                .entry(section_key.to_string())
                .or_insert_with(|| json_viewer::init_state(&tree, 1));
            let base = lines.len();
            json_viewer::append_render(
                lines,
                json_click_map,
                &tree,
                state,
                section_key,
                "   ",
                max_w.saturating_sub(3),
            );
            for _ in base..lines.len() {
                section_map.push(None);
            }
        }
        Err(_) => {
            for wl in wrap_text(json_text, max_w.saturating_sub(3), 100) {
                lines.push(Line::from(Span::styled(
                    format!("   {}", wl),
                    Style::default().fg(SUBTEXT0),
                )));
                section_map.push(None);
                json_click_map.push(None);
            }
        }
    }
}
```

Replace the function body (keeping the signature unchanged) with:

```rust
    match crate::domain::structured_parser::parse(json_text) {
        Some(value) => {
            let tree = json_viewer::Tree::from_value(&value);
            let state = viewer_states
                .entry(section_key.to_string())
                .or_insert_with(|| json_viewer::init_state(&tree, 1));
            let base = lines.len();
            json_viewer::append_render(
                lines,
                json_click_map,
                &tree,
                state,
                section_key,
                "   ",
                max_w.saturating_sub(3),
            );
            for _ in base..lines.len() {
                section_map.push(None);
            }
        }
        None => {
            for wl in wrap_text(json_text, max_w.saturating_sub(3), 100) {
                lines.push(Line::from(Span::styled(
                    format!("   {}", wl),
                    Style::default().fg(SUBTEXT0),
                )));
                section_map.push(None);
                json_click_map.push(None);
            }
        }
    }
```

Same three changes: `json_viewer::parse` → `structured_parser::parse`; `Ok/Err` → `Some/None`; add `Tree::from_value(&value)`.

- [ ] **Step 3: Build**

Run: `cargo build`
Expected: clean compile, same pre-existing warnings as before.

- [ ] **Step 4: Run full suite**

Run: `cargo test --lib`
Expected: 150 tests pass. No viewer test changes, no regressions.

- [ ] **Step 5: Manual verification**

Build the release binary and test against a Flutter app. Verify:

1. A log message like `{code: 0, message: ok, data: [1, 2, 3]}` (plain Dart Map dump) displays as a proper tree in the logs detail panel.
2. A log message with a JSON prefix like `Response body: {"user": "alice"}` displays the tree starting from the `{`.
3. A non-structured log message (e.g. `Application started in 120ms`) still falls back to plain wrapped text.
4. Network detail panels for real JSON responses render identically to before (no regression).

- [ ] **Step 6: Commit**

```bash
git add src/ui/logs/detail.rs src/ui/network/detail.rs
git commit -m "feat(ui): detail panels parse Dart Map.toString via structured_parser"
```

---

## Verification Summary

| Spec requirement | Verified by |
|---|---|
| Strict JSON still parses | Task 2 `strict_json_still_parses` |
| Dart Map with unquoted keys/values | Task 2 `dart_map_unquoted_keys_and_strings` |
| Nested Dart maps | Task 2 `nested_dart_map` |
| Dart arrays | Task 2 `dart_array` |
| Empty containers | Task 2 `empty_containers` |
| Mixed quoted + bare values | Task 2 `mixed_quoted_and_bare` |
| Prefix before object | Task 2 `prefix_before_object` |
| Gibberish → None | Task 2 `gibberish_returns_none` |
| Unbalanced → None | Task 2 `unbalanced_returns_none` |
| Typed literals (null/true/false/float) | Task 2 `typed_bare_literals` |
| Whitespace tolerance | Task 2 `whitespace_tolerance` |
| No over-consumption | Task 2 `trailing_garbage_returns_none`, `text_without_brackets_returns_none` |
| `Tree::from_value` split from `parse` | Task 1 + existing tree tests (7 pass) |
| Logs panel formats Dart dumps | Task 3 manual verification |
| Network panel regression-free | Task 3 manual verification + existing 21 render/state tests |
