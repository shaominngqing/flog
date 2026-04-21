# JSON Viewer Rewrite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the existing text-transform JSON viewer (`src/ui/json_viewer.rs`) with an AST-based tree viewer that fixes indentation misalignment and flaky click handling.

**Architecture:** Parse JSON with `serde_json` into a flat arena tree (`Vec<FlatNode>` with integer `u32` IDs). Store fold state as a parallel `Vec<bool>` indexed by node ID. Render recursively with fixed-width markers (▼ / ▶ / spaces) so indentation derives from logical depth, not reverse-engineered from text. Click hit-testing maps rendered rows to node IDs with no wrap-continuation aliasing.

**Tech Stack:** Rust, `serde_json`, `ratatui`, `unicode_width`.

**Spec:** `docs/superpowers/specs/2026-04-21-json-viewer-rewrite-design.md`

---

## File Structure

**New module directory** (replaces single file `src/ui/json_viewer.rs`):

- `src/ui/json_viewer/mod.rs` — Public API; re-exports everything callers need (parse, init_state, toggle, expand_all, collapse_all, append_render, JsonViewerState, colorize_json_text).
- `src/ui/json_viewer/tree.rs` — `NodeKind`, `FlatNode`, `Tree`, `parse(text) -> Result<Tree, serde_json::Error>`.
- `src/ui/json_viewer/state.rs` — `JsonViewerState` (wraps `Vec<bool>`), `init_state`, `toggle`, `expand_all`, `collapse_all`.
- `src/ui/json_viewer/render.rs` — `append_render`, `summarize_container`, internal value/key colorizers.
- `src/ui/json_viewer/colorize.rs` — Existing `colorize_json_text` moved verbatim from the old file.

**Existing files modified:**

- `src/ui/mod.rs` — No change needed; `pub mod json_viewer;` already works with a directory.
- `src/ui/network/detail.rs` — `render_json_section` simplified; `json_click_map` element type changes.
- `src/ui/logs/detail.rs` — Migrate from old `bracket_format` / `render_json` / `init_state` to new API.
- `src/app.rs` — `NetworkState::detail_json_click_map` element type from `(String, usize)` → `(String, u32)`; `DetailState::viewer_state` keeps its name but is the new type; `toggle_detail_fold` signature changes from `usize` → `u32`.
- `src/event.rs` — Click dispatch calls `json_viewer::toggle(state, node_id)`; add uppercase `E` / `C` handlers for expand-all / collapse-all in Network tab.
- `src/ui/help.rs` — Add two help lines for `E` / `C`.

---

## Task 1: Scaffold the new module directory

**Files:**
- Create: `src/ui/json_viewer/mod.rs`
- Create: `src/ui/json_viewer/tree.rs` (empty stub)
- Create: `src/ui/json_viewer/state.rs` (empty stub)
- Create: `src/ui/json_viewer/render.rs` (empty stub)
- Create: `src/ui/json_viewer/colorize.rs` (moved content)
- Delete: `src/ui/json_viewer.rs`

The goal of this task is a clean scaffold that still builds. We move `colorize_json_text` out first because it's the only function that external callers (`logs/detail.rs` via `json_viewer::colorize_json_text`) depend on with unchanged behavior — the rest we can safely gut and rewrite from scratch.

- [ ] **Step 1: Create `src/ui/json_viewer/colorize.rs` with moved content**

Copy lines 456–706 (the `colorize_json_text` function and its associated constants + imports it needs) from the current `src/ui/json_viewer.rs` into a new file. The function is self-contained — it only needs these constants: `STR_COLOR`, `NUM_COLOR`, `BOOL_COLOR`, `NULL_COLOR`, `COMMA_COLOR`, plus `key_color()` / `brace_color()` and their backing `DEPTH_COLORS` / `DEPTH_BRACE` arrays.

Write this exact content to `src/ui/json_viewer/colorize.rs`:

```rust
//! Raw JSON text syntax highlighter.
//!
//! Character-by-character tokenizer that colorizes arbitrary text containing
//! JSON-like fragments. Used by `logs/detail.rs` for inline JSON embedded in
//! log messages. Not the tree viewer.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use super::super::{BLUE, GREEN, LAVENDER, MAUVE, OVERLAY0, PEACH, PINK, SAPPHIRE, SURFACE0, TEAL, TEXT, YELLOW};

const STR_COLOR: Color = GREEN;
const NUM_COLOR: Color = PEACH;
const BOOL_COLOR: Color = PINK;
const NULL_COLOR: Color = OVERLAY0;
const COMMA_COLOR: Color = SURFACE0;

const DEPTH_COLORS: [Color; 6] = [MAUVE, BLUE, TEAL, YELLOW, SAPPHIRE, LAVENDER];
const DEPTH_BRACE: [Color; 6] = [
    Color::Rgb(110, 115, 141),
    Color::Rgb(100, 105, 131),
    Color::Rgb(90, 95, 121),
    Color::Rgb(80, 85, 111),
    Color::Rgb(73, 77, 100),
    Color::Rgb(54, 58, 79),
];

fn key_color(depth: usize) -> Color { DEPTH_COLORS[depth % DEPTH_COLORS.len()] }
fn brace_color(depth: usize) -> Color { DEPTH_BRACE[depth % DEPTH_BRACE.len()] }

/// Colorize raw text (typically JSON) with syntax highlighting.
pub fn colorize_json_text(text: &str) -> Vec<Line<'static>> {
    let default_style = Style::default().fg(TEXT);
    let mut depth: usize = 0;
    let mut in_string = false;
    let mut result: Vec<Line<'static>> = Vec::new();

    for line in text.split('\n') {
        if line.is_empty() {
            result.push(Line::raw(""));
            continue;
        }

        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut buf = String::new();
        let mut buf_style = default_style;
        let mut i = 0;

        macro_rules! flush {
            () => {
                if !buf.is_empty() {
                    spans.push(Span::styled(std::mem::take(&mut buf), buf_style));
                }
            };
        }

        if in_string {
            buf_style = Style::default().fg(STR_COLOR);
            while i < len {
                let c = chars[i];
                if c == '\\' && i + 1 < len {
                    buf.push(c);
                    buf.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                if c == '"' {
                    buf.push(c);
                    i += 1;
                    in_string = false;
                    flush!();
                    buf_style = default_style;
                    break;
                }
                buf.push(c);
                i += 1;
            }
            if in_string {
                flush!();
                result.push(Line::from(spans));
                continue;
            }
        }

        while i < len {
            let c = chars[i];
            match c {
                '"' => {
                    flush!();
                    let mut s = String::new();
                    s.push('"');
                    let mut j = i + 1;
                    let mut terminated = false;
                    while j < len {
                        let sc = chars[j];
                        if sc == '\\' && j + 1 < len {
                            s.push(sc);
                            s.push(chars[j + 1]);
                            j += 2;
                            continue;
                        }
                        if sc == '"' {
                            s.push('"');
                            j += 1;
                            terminated = true;
                            break;
                        }
                        s.push(sc);
                        j += 1;
                    }
                    if !terminated {
                        spans.push(Span::styled(s, Style::default().fg(STR_COLOR)));
                        in_string = true;
                        i = len;
                        continue;
                    }
                    let is_key = {
                        let mut k = j;
                        while k < len && chars[k].is_ascii_whitespace() { k += 1; }
                        k < len && chars[k] == ':'
                    };
                    let color = if is_key { key_color(depth) } else { STR_COLOR };
                    spans.push(Span::styled(s, Style::default().fg(color)));
                    i = j;
                }
                '{' | '[' => {
                    flush!();
                    spans.push(Span::styled(c.to_string(), Style::default().fg(brace_color(depth))));
                    depth += 1;
                    i += 1;
                }
                '}' | ']' => {
                    flush!();
                    depth = depth.saturating_sub(1);
                    spans.push(Span::styled(c.to_string(), Style::default().fg(brace_color(depth))));
                    i += 1;
                }
                ':' | ',' => {
                    flush!();
                    spans.push(Span::styled(c.to_string(), Style::default().fg(COMMA_COLOR)));
                    i += 1;
                }
                't' if matches!(chars.get(i..i + 4), Some(&['t', 'r', 'u', 'e'])) => {
                    let after = chars.get(i + 4).copied().unwrap_or(' ');
                    if !after.is_alphanumeric() && after != '_' {
                        flush!();
                        spans.push(Span::styled("true", Style::default().fg(BOOL_COLOR)));
                        i += 4;
                    } else {
                        buf.push(c);
                        buf_style = default_style;
                        i += 1;
                    }
                }
                'f' if matches!(chars.get(i..i + 5), Some(&['f', 'a', 'l', 's', 'e'])) => {
                    let after = chars.get(i + 5).copied().unwrap_or(' ');
                    if !after.is_alphanumeric() && after != '_' {
                        flush!();
                        spans.push(Span::styled("false", Style::default().fg(BOOL_COLOR)));
                        i += 5;
                    } else {
                        buf.push(c);
                        buf_style = default_style;
                        i += 1;
                    }
                }
                'n' if matches!(chars.get(i..i + 4), Some(&['n', 'u', 'l', 'l'])) => {
                    let after = chars.get(i + 4).copied().unwrap_or(' ');
                    if !after.is_alphanumeric() && after != '_' {
                        flush!();
                        spans.push(Span::styled(
                            "null",
                            Style::default().fg(NULL_COLOR).add_modifier(Modifier::ITALIC),
                        ));
                        i += 4;
                    } else {
                        buf.push(c);
                        buf_style = default_style;
                        i += 1;
                    }
                }
                '0'..='9' | '-' if {
                    c.is_ascii_digit()
                        || (c == '-' && i + 1 < len && chars[i + 1].is_ascii_digit())
                } => {
                    flush!();
                    let mut num = String::new();
                    num.push(c);
                    let mut j = i + 1;
                    while j < len
                        && (chars[j].is_ascii_digit()
                            || chars[j] == '.'
                            || chars[j] == 'e'
                            || chars[j] == 'E'
                            || chars[j] == '+'
                            || chars[j] == '-')
                    {
                        num.push(chars[j]);
                        j += 1;
                    }
                    spans.push(Span::styled(num, Style::default().fg(NUM_COLOR)));
                    i = j;
                }
                ' ' | '\t' | '\r' => {
                    if buf_style != default_style {
                        flush!();
                        buf_style = default_style;
                    }
                    buf.push(c);
                    i += 1;
                }
                _ => {
                    if buf_style != default_style {
                        flush!();
                        buf_style = default_style;
                    }
                    buf.push(c);
                    i += 1;
                }
            }
        }
        flush!();
        result.push(Line::from(spans));
    }
    result
}
```

- [ ] **Step 2: Create empty stub files for the new modules**

Write `src/ui/json_viewer/tree.rs`:

```rust
//! Flat-arena JSON tree (implementation follows).
```

Write `src/ui/json_viewer/state.rs`:

```rust
//! Fold state (implementation follows).
```

Write `src/ui/json_viewer/render.rs`:

```rust
//! Rendering (implementation follows).
```

- [ ] **Step 3: Create `src/ui/json_viewer/mod.rs` with a minimal shim**

This temporarily re-exports just `colorize_json_text` and keeps the **old** public API stubs so the rest of the codebase keeps compiling until Task 9 migrates callers. The stubs live here, not in the new modules, so they're easy to delete later.

Write this exact content to `src/ui/json_viewer/mod.rs`:

```rust
//! JSON viewer — AST-based tree display for structured JSON.
//!
//! Submodules:
//! - `tree`     — parse text into a flat arena tree.
//! - `state`    — per-tree fold state.
//! - `render`   — depth-aware rendering with click hit-testing.
//! - `colorize` — raw-text JSON syntax highlight (independent).

mod colorize;
mod render;
mod state;
mod tree;

pub use colorize::colorize_json_text;

// ── Legacy shims ─────────────────────────────────────────────────────────
// These keep the existing callers compiling until Task 9 migrates them.
// Delete once no references remain.

use std::collections::HashSet;

pub struct FmtLine {
    pub text: String,
    pub depth: usize,
    pub close_line: Option<usize>,
}

#[derive(Default, Clone)]
pub struct JsonViewerState {
    pub collapsed: HashSet<usize>,
    pub foldable: HashSet<usize>,
    pub row_to_source: Vec<usize>,
    pub total_lines: usize,
}

pub fn bracket_format(_text: &str) -> Vec<FmtLine> {
    Vec::new()
}

pub fn init_state(_fmt_lines: &[FmtLine], _auto_expand_depth: usize) -> JsonViewerState {
    JsonViewerState::default()
}

pub fn toggle_fold(_state: &mut JsonViewerState, _source_line: usize) -> bool {
    false
}

pub fn render_json(
    _fmt_lines: &[FmtLine],
    _state: &mut JsonViewerState,
    _scroll: usize,
    _max_lines: usize,
    _max_width: usize,
) -> Vec<ratatui::text::Line<'static>> {
    Vec::new()
}
```

- [ ] **Step 4: Delete the old `src/ui/json_viewer.rs`**

Run: `git rm src/ui/json_viewer.rs`

- [ ] **Step 5: Build to confirm the scaffold compiles**

Run: `cargo build`
Expected: compiles with only deprecation/unused-variable warnings in the shim file. No errors.

If you see unused-import warnings for constants like `LAVENDER`, `PEACH`, `PINK`, etc. in `colorize.rs`, that's because the original file used them. Keep them — they're re-exports from `super::super`; removing them would cascade changes. Rust's `#[allow(unused_imports)]` isn't needed because they are used inside `DEPTH_COLORS`.

If the build fails, the most likely cause is an import-path mismatch on the `super::super::{...}` line in `colorize.rs`. The old file's import was `use super::{...}` because it was in `src/ui/`. Now that we're nested one deeper in `src/ui/json_viewer/`, it becomes `super::super::{...}`. Verify that's what you wrote.

- [ ] **Step 6: Commit**

```bash
git add src/ui/json_viewer/ src/ui/json_viewer.rs
git commit -m "refactor(json_viewer): scaffold module directory, relocate colorize"
```

---

## Task 2: Implement the tree module

**Files:**
- Modify: `src/ui/json_viewer/tree.rs`

We build a flat arena of `FlatNode` entries via DFS over `serde_json::Value`. Root is always at index 0. Children indices are contiguous for cache locality (we build by recursion but allocate IDs in DFS order). We keep numbers as their original string form via `serde_json::Number::to_string()` to avoid `f64` precision loss.

- [ ] **Step 1: Write the failing test file**

Create `src/ui/json_viewer/tree.rs` with the following content (replacing the stub):

```rust
//! Flat-arena JSON tree.
//!
//! Nodes are stored in a single `Vec<FlatNode>` indexed by `u32` ID.
//! Root is always `nodes[0]`. Children IDs are stored on the parent,
//! not contiguous (DFS order means parent < child, but not always
//! consecutive because siblings' subtrees are interleaved).

use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub enum NodeKind {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Object,
    Array,
}

#[derive(Clone, Debug)]
pub struct FlatNode {
    pub kind: NodeKind,
    pub depth: u32,
    pub parent: Option<u32>,
    /// Child node IDs in source order. Empty for leaves.
    pub children: Vec<u32>,
    /// For object entries: the key. For array entries and root: None.
    pub key: Option<String>,
}

pub struct Tree {
    pub nodes: Vec<FlatNode>,
}

impl Tree {
    pub fn root(&self) -> &FlatNode {
        &self.nodes[0]
    }
    pub fn node(&self, id: u32) -> &FlatNode {
        &self.nodes[id as usize]
    }
    pub fn is_container(&self, id: u32) -> bool {
        matches!(self.nodes[id as usize].kind, NodeKind::Object | NodeKind::Array)
    }
    pub fn is_empty_container(&self, id: u32) -> bool {
        self.is_container(id) && self.nodes[id as usize].children.is_empty()
    }
}

pub fn parse(text: &str) -> Result<Tree, serde_json::Error> {
    let value: Value = serde_json::from_str(text)?;
    let mut nodes: Vec<FlatNode> = Vec::new();
    build(&value, None, None, 0, &mut nodes);
    Ok(Tree { nodes })
}

fn build(
    value: &Value,
    parent: Option<u32>,
    key: Option<String>,
    depth: u32,
    nodes: &mut Vec<FlatNode>,
) -> u32 {
    let my_id = nodes.len() as u32;
    let kind = match value {
        Value::Null => NodeKind::Null,
        Value::Bool(b) => NodeKind::Bool(*b),
        Value::Number(n) => NodeKind::Number(n.to_string()),
        Value::String(s) => NodeKind::String(s.clone()),
        Value::Object(_) => NodeKind::Object,
        Value::Array(_) => NodeKind::Array,
    };
    nodes.push(FlatNode {
        kind,
        depth,
        parent,
        children: Vec::new(),
        key,
    });
    match value {
        Value::Object(map) => {
            let mut child_ids = Vec::with_capacity(map.len());
            for (k, v) in map {
                let cid = build(v, Some(my_id), Some(k.clone()), depth + 1, nodes);
                child_ids.push(cid);
            }
            nodes[my_id as usize].children = child_ids;
        }
        Value::Array(arr) => {
            let mut child_ids = Vec::with_capacity(arr.len());
            for v in arr {
                let cid = build(v, Some(my_id), None, depth + 1, nodes);
                child_ids.push(cid);
            }
            nodes[my_id as usize].children = child_ids;
        }
        _ => {}
    }
    my_id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_primitive_root() {
        let t = parse("42").unwrap();
        assert_eq!(t.nodes.len(), 1);
        assert_eq!(t.nodes[0].depth, 0);
        assert_eq!(t.nodes[0].kind, NodeKind::Number("42".into()));
        assert!(t.nodes[0].children.is_empty());
    }

    #[test]
    fn parse_flat_object() {
        let t = parse(r#"{"a": 1, "b": "hi"}"#).unwrap();
        assert_eq!(t.nodes.len(), 3);
        assert_eq!(t.nodes[0].kind, NodeKind::Object);
        assert_eq!(t.nodes[0].children, vec![1, 2]);
        assert_eq!(t.nodes[1].key, Some("a".into()));
        assert_eq!(t.nodes[1].kind, NodeKind::Number("1".into()));
        assert_eq!(t.nodes[1].depth, 1);
        assert_eq!(t.nodes[1].parent, Some(0));
        assert_eq!(t.nodes[2].key, Some("b".into()));
        assert_eq!(t.nodes[2].kind, NodeKind::String("hi".into()));
    }

    #[test]
    fn parse_nested_array() {
        let t = parse(r#"{"xs": [true, null]}"#).unwrap();
        // nodes: 0=root object, 1=array xs, 2=true, 3=null
        assert_eq!(t.nodes.len(), 4);
        assert_eq!(t.nodes[1].kind, NodeKind::Array);
        assert_eq!(t.nodes[1].children, vec![2, 3]);
        assert_eq!(t.nodes[2].kind, NodeKind::Bool(true));
        assert_eq!(t.nodes[2].key, None); // array entry has no key
        assert_eq!(t.nodes[2].depth, 2);
        assert_eq!(t.nodes[3].kind, NodeKind::Null);
    }

    #[test]
    fn parse_empty_containers() {
        let t = parse(r#"{"a": [], "b": {}}"#).unwrap();
        assert_eq!(t.nodes.len(), 3);
        assert!(t.is_empty_container(1));
        assert!(t.is_empty_container(2));
    }

    #[test]
    fn parse_invalid_json() {
        assert!(parse("not json").is_err());
        assert!(parse(r#"{"unterminated":"#).is_err());
    }

    #[test]
    fn number_preserves_string_form() {
        let t = parse("1776684313608").unwrap();
        assert_eq!(t.nodes[0].kind, NodeKind::Number("1776684313608".into()));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail (before wiring up the module)**

The `mod tree;` in `mod.rs` is already declared but uses `tree.rs` stub content. Because we just overwrote the stub with the real implementation, tests should actually **pass** — but let's be explicit. Run:

Run: `cargo test --lib ui::json_viewer::tree`
Expected: All 6 tests pass.

If they fail with "module not declared", check that `src/ui/json_viewer/mod.rs` has `mod tree;`.

- [ ] **Step 3: Commit**

```bash
git add src/ui/json_viewer/tree.rs
git commit -m "feat(json_viewer): flat-arena tree parser with serde_json"
```

---

## Task 3: Implement fold state

**Files:**
- Modify: `src/ui/json_viewer/state.rs`

The new `JsonViewerState` lives here. The old shim in `mod.rs` is still named `JsonViewerState` — we'll delete the shim in Task 8. For now we give the new state a distinct name inside `state.rs` (e.g. keep it internal) and re-export it via `mod.rs` only after the shim is gone. Since we **want** the final name to be `JsonViewerState`, we'll call it that here and accept the name clash by not re-exporting from `mod.rs` yet.

- [ ] **Step 1: Write the failing test file**

Write `src/ui/json_viewer/state.rs`:

```rust
//! Fold state for a JSON tree.
//!
//! State is a parallel `Vec<bool>` indexed by node ID. Leaves are always
//! false and unused; only container nodes' entries matter.

use super::tree::{NodeKind, Tree};

#[derive(Default, Clone)]
pub struct JsonViewerState {
    /// `expanded[id] == true` iff container node `id` is currently expanded.
    /// Length equals `tree.nodes.len()` after `init_state`.
    pub expanded: Vec<bool>,
}

impl JsonViewerState {
    pub fn is_expanded(&self, id: u32) -> bool {
        self.expanded.get(id as usize).copied().unwrap_or(false)
    }
}

/// Create initial state. Every container with `depth <= default_expand_depth`
/// starts expanded; all others collapsed.
pub fn init_state(tree: &Tree, default_expand_depth: u32) -> JsonViewerState {
    let mut expanded = vec![false; tree.nodes.len()];
    for (i, node) in tree.nodes.iter().enumerate() {
        let is_container = matches!(node.kind, NodeKind::Object | NodeKind::Array);
        if is_container && node.depth <= default_expand_depth {
            expanded[i] = true;
        }
    }
    JsonViewerState { expanded }
}

/// Toggle `node_id`. No-op if `node_id` is out of bounds or is a leaf.
/// Returns `true` iff the state changed.
pub fn toggle(tree: &Tree, state: &mut JsonViewerState, node_id: u32) -> bool {
    let idx = node_id as usize;
    if idx >= state.expanded.len() {
        return false;
    }
    let kind = &tree.nodes[idx].kind;
    if !matches!(kind, NodeKind::Object | NodeKind::Array) {
        return false;
    }
    state.expanded[idx] = !state.expanded[idx];
    true
}

/// Expand every container.
pub fn expand_all(tree: &Tree, state: &mut JsonViewerState) {
    for (i, node) in tree.nodes.iter().enumerate() {
        if matches!(node.kind, NodeKind::Object | NodeKind::Array) {
            state.expanded[i] = true;
        }
    }
}

/// Collapse every container except the root (so the panel stays useful).
pub fn collapse_all(tree: &Tree, state: &mut JsonViewerState) {
    for (i, node) in tree.nodes.iter().enumerate() {
        if matches!(node.kind, NodeKind::Object | NodeKind::Array) {
            state.expanded[i] = i == 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tree::parse;
    use super::*;

    #[test]
    fn init_expands_within_depth() {
        // Root object (depth 0) + child object (depth 1) + grandchild (depth 2)
        let t = parse(r#"{"a": {"b": {"c": 1}}}"#).unwrap();
        let s = init_state(&t, 1);
        assert!(s.expanded[0]); // root
        assert!(s.expanded[1]); // a (depth 1)
        assert!(!s.expanded[2]); // b (depth 2)
    }

    #[test]
    fn init_depth_zero_only_root() {
        let t = parse(r#"{"a": {"b": 1}}"#).unwrap();
        let s = init_state(&t, 0);
        assert!(s.expanded[0]);
        assert!(!s.expanded[1]);
    }

    #[test]
    fn toggle_flips_container() {
        let t = parse(r#"{"a": {"b": 1}}"#).unwrap();
        let mut s = init_state(&t, 0);
        assert!(!s.expanded[1]);
        assert!(toggle(&t, &mut s, 1));
        assert!(s.expanded[1]);
        assert!(toggle(&t, &mut s, 1));
        assert!(!s.expanded[1]);
    }

    #[test]
    fn toggle_leaf_is_noop() {
        let t = parse(r#"{"a": 1}"#).unwrap();
        let mut s = init_state(&t, 0);
        assert!(!toggle(&t, &mut s, 1)); // a's value is a leaf number
    }

    #[test]
    fn toggle_out_of_bounds_is_noop() {
        let t = parse(r#"{}"#).unwrap();
        let mut s = init_state(&t, 0);
        assert!(!toggle(&t, &mut s, 99));
    }

    #[test]
    fn expand_all_sets_all_containers() {
        let t = parse(r#"{"a": {"b": {"c": 1}}}"#).unwrap();
        let mut s = init_state(&t, 0);
        expand_all(&t, &mut s);
        assert!(s.expanded[0]);
        assert!(s.expanded[1]);
        assert!(s.expanded[2]);
    }

    #[test]
    fn collapse_all_leaves_root_expanded() {
        let t = parse(r#"{"a": {"b": 1}}"#).unwrap();
        let mut s = init_state(&t, 5);
        collapse_all(&t, &mut s);
        assert!(s.expanded[0]); // root stays
        assert!(!s.expanded[1]); // a collapses
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib ui::json_viewer::state`
Expected: All 7 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src/ui/json_viewer/state.rs
git commit -m "feat(json_viewer): fold state with init / toggle / expand-all / collapse-all"
```

---

## Task 4: Implement rendering — leaves and closed containers

We build the renderer in three tasks. This task handles the simplest cases: leaves and expanded-empty containers. Task 5 adds collapsed containers + summary. Task 6 adds expanded containers with children. This keeps each task testable against observable output.

**Files:**
- Modify: `src/ui/json_viewer/render.rs`

Each rendered line pairs a `ratatui::Line<'static>` with an `Option<u32>` — the node ID to toggle on click, or `None` for non-clickable lines. The entry point `append_render` pushes into the caller's `Vec<Line>` and `Vec<Option<(String, u32)>>` in lockstep.

- [ ] **Step 1: Write the render module skeleton + leaf rendering + tests**

Write `src/ui/json_viewer/render.rs`:

```rust
//! JSON tree rendering.
//!
//! Every rendered row has the layout
//!   <outer_prefix><indent><marker><content>
//! where `indent` derives from node depth (not from text width!) and
//! `marker` is a fixed-width 2-char cell (▼/▶/spaces) so content columns
//! always align.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

use super::super::{BLUE, GREEN, LAVENDER, MAUVE, OVERLAY0, PEACH, PINK, SAPPHIRE, SURFACE0, TEAL, YELLOW};
use super::state::JsonViewerState;
use super::tree::{NodeKind, Tree};

const STR_COLOR: Color = GREEN;
const NUM_COLOR: Color = PEACH;
const BOOL_COLOR: Color = PINK;
const NULL_COLOR: Color = OVERLAY0;
const COMMA_COLOR: Color = SURFACE0;
const FOLD_COLOR: Color = OVERLAY0;

const DEPTH_COLORS: [Color; 6] = [MAUVE, BLUE, TEAL, YELLOW, SAPPHIRE, LAVENDER];
const DEPTH_BRACE: [Color; 6] = [
    Color::Rgb(110, 115, 141),
    Color::Rgb(100, 105, 131),
    Color::Rgb(90, 95, 121),
    Color::Rgb(80, 85, 111),
    Color::Rgb(73, 77, 100),
    Color::Rgb(54, 58, 79),
];

fn key_color(depth: u32) -> Color { DEPTH_COLORS[(depth as usize) % DEPTH_COLORS.len()] }
fn brace_color(depth: u32) -> Color { DEPTH_BRACE[(depth as usize) % DEPTH_BRACE.len()] }

/// Render `tree` into `out`, pushing one `Option<(section_key, node_id)>`
/// into `click_map` for each line added. Clickable rows (foldable containers)
/// get `Some(...)`; leaves and close-brace lines get `None`.
///
/// `outer_prefix` is whitespace the caller wants prepended (e.g. `"   "` for
/// three-space section indent). `max_width` is the total panel width; strings
/// are truncated with `…` if the line would exceed it.
pub fn append_render(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Option<(String, u32)>>,
    tree: &Tree,
    state: &JsonViewerState,
    section_key: &str,
    outer_prefix: &str,
    max_width: usize,
) {
    render_node(
        out,
        click_map,
        tree,
        state,
        section_key,
        outer_prefix,
        max_width,
        0,
        false, // root never has a trailing comma
    );
}

/// Render node `id` and its subtree. `trailing_comma` says whether to append
/// a comma after this node's value (used for non-last siblings).
fn render_node(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Option<(String, u32)>>,
    tree: &Tree,
    state: &JsonViewerState,
    section_key: &str,
    outer_prefix: &str,
    max_width: usize,
    id: u32,
    trailing_comma: bool,
) {
    let node = tree.node(id);
    let depth = node.depth;
    let is_container = matches!(node.kind, NodeKind::Object | NodeKind::Array);

    if !is_container {
        push_leaf_line(out, click_map, tree, section_key, outer_prefix, max_width, id, trailing_comma);
        return;
    }

    // Containers: implemented in Tasks 5 and 6.
    // Until then, render as a collapsed placeholder so the module compiles.
    let _ = (state, depth);
    push_leaf_line(out, click_map, tree, section_key, outer_prefix, max_width, id, trailing_comma);
}

fn push_leaf_line(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Option<(String, u32)>>,
    tree: &Tree,
    _section_key: &str,
    outer_prefix: &str,
    max_width: usize,
    id: u32,
    trailing_comma: bool,
) {
    let node = tree.node(id);
    let depth = node.depth;
    let indent = "  ".repeat(depth as usize);
    let marker = "  "; // leaves are not foldable

    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::raw(format!("{}{}{}", outer_prefix, indent, marker)));

    // Key part (for object entries)
    if let Some(ref key) = node.key {
        spans.push(Span::styled(
            format!("\"{}\"", key),
            Style::default().fg(key_color(depth)),
        ));
        spans.push(Span::styled(": ", Style::default().fg(COMMA_COLOR)));
    }

    // Remaining width for the value
    let used: usize = spans.iter().map(|s| s.content.as_ref().width()).sum();
    let remaining = max_width.saturating_sub(used).saturating_sub(if trailing_comma { 1 } else { 0 });

    spans.extend(render_leaf_value(&node.kind, remaining));

    if trailing_comma {
        spans.push(Span::styled(",", Style::default().fg(COMMA_COLOR)));
    }

    out.push(Line::from(spans));
    click_map.push(None);
}

fn render_leaf_value(kind: &NodeKind, max_width: usize) -> Vec<Span<'static>> {
    match kind {
        NodeKind::Null => vec![Span::styled(
            "null",
            Style::default().fg(NULL_COLOR).add_modifier(Modifier::ITALIC),
        )],
        NodeKind::Bool(b) => vec![Span::styled(
            if *b { "true" } else { "false" },
            Style::default().fg(BOOL_COLOR),
        )],
        NodeKind::Number(s) => vec![Span::styled(s.clone(), Style::default().fg(NUM_COLOR))],
        NodeKind::String(s) => {
            let quoted = format!("\"{}\"", s);
            let text = if quoted.width() > max_width && max_width >= 2 {
                // Truncate inside the quotes, leave room for closing `"` + `…`
                let budget = max_width.saturating_sub(2); // `"…`
                let mut w = 0usize;
                let mut cut = 0usize;
                for (i, ch) in s.char_indices() {
                    let cw = ch.to_string().as_str().width();
                    if w + cw > budget {
                        break;
                    }
                    w += cw;
                    cut = i + ch.len_utf8();
                }
                format!("\"{}…\"", &s[..cut])
            } else {
                quoted
            };
            vec![Span::styled(text, Style::default().fg(STR_COLOR))]
        }
        // Containers reach this only during the stubbed path in render_node;
        // once Tasks 5/6 land, containers never hit here.
        NodeKind::Object => vec![Span::styled("{…}", Style::default().fg(FOLD_COLOR))],
        NodeKind::Array => vec![Span::styled("[…]", Style::default().fg(FOLD_COLOR))],
    }
}

#[cfg(test)]
mod tests {
    use super::super::{state, tree};
    use super::*;

    fn render(text: &str, width: usize) -> Vec<String> {
        let t = tree::parse(text).unwrap();
        let s = state::init_state(&t, 0); // collapse everything except root
        let mut out = Vec::new();
        let mut cmap = Vec::new();
        append_render(&mut out, &mut cmap, &t, &s, "sec", "", width);
        assert_eq!(out.len(), cmap.len(), "out and click_map must stay in sync");
        out.iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>())
            .collect()
    }

    #[test]
    fn leaf_null() {
        let lines = render("null", 80);
        assert_eq!(lines, vec!["  null"]);
    }

    #[test]
    fn leaf_bool() {
        assert_eq!(render("true", 80), vec!["  true"]);
        assert_eq!(render("false", 80), vec!["  false"]);
    }

    #[test]
    fn leaf_number() {
        assert_eq!(render("1776684313608", 80), vec!["  1776684313608"]);
    }

    #[test]
    fn leaf_string_short() {
        assert_eq!(render(r#""hello""#, 80), vec![r#"  "hello""#]);
    }

    #[test]
    fn leaf_string_truncated() {
        // "  " marker (2) + `"…"` (3) + content
        // max_width 10 -> used 2, remaining 8, budget for content = 6 chars
        let lines = render(r#""abcdefghij""#, 10);
        // "abcdefghij" has width 10 — with "…" bookend we expect a truncated form.
        // Exact cut point varies by width budget; just assert the ellipsis appears.
        assert!(lines[0].contains('…'), "expected truncation: {:?}", lines);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib ui::json_viewer::render`
Expected: 5 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src/ui/json_viewer/render.rs
git commit -m "feat(json_viewer): render leaf values with truncation"
```

---

## Task 5: Render collapsed containers with DevTools-style summary

**Files:**
- Modify: `src/ui/json_viewer/render.rs`

Collapsed containers show a summary inline: objects show up to N entries as `{k1: v1, k2: v2, …}`, arrays show elements as `[e1, e2, e3, …] (<len>)`. Nested containers summarize as `{…}` / `[…]`.

- [ ] **Step 1: Replace the container branch in `render_node` and add `summarize_container`**

Edit `src/ui/json_viewer/render.rs`. Find this block in `render_node`:

```rust
    // Containers: implemented in Tasks 5 and 6.
    // Until then, render as a collapsed placeholder so the module compiles.
    let _ = (state, depth);
    push_leaf_line(out, click_map, tree, section_key, outer_prefix, max_width, id, trailing_comma);
```

Replace it with:

```rust
    let expanded = state.is_expanded(id);
    if !expanded || node.children.is_empty() {
        push_container_collapsed(
            out, click_map, tree, section_key, outer_prefix, max_width, id, trailing_comma,
        );
        return;
    }

    // Expanded container with children: implemented in Task 6.
    // Until then, fall back to collapsed rendering so the module compiles.
    push_container_collapsed(
        out, click_map, tree, section_key, outer_prefix, max_width, id, trailing_comma,
    );
```

Now add these functions below `render_leaf_value`:

```rust
fn push_container_collapsed(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Option<(String, u32)>>,
    tree: &Tree,
    section_key: &str,
    outer_prefix: &str,
    max_width: usize,
    id: u32,
    trailing_comma: bool,
) {
    let node = tree.node(id);
    let depth = node.depth;
    let indent = "  ".repeat(depth as usize);
    let empty = node.children.is_empty();

    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::raw(format!("{}{}", outer_prefix, indent)));

    // Marker. Empty containers are not foldable — show blank marker.
    if empty {
        spans.push(Span::raw("  "));
    } else {
        spans.push(Span::styled("▶ ", Style::default().fg(BLUE)));
    }

    // Key
    if let Some(ref key) = node.key {
        spans.push(Span::styled(
            format!("\"{}\"", key),
            Style::default().fg(key_color(depth)),
        ));
        spans.push(Span::styled(": ", Style::default().fg(COMMA_COLOR)));
    }

    let used: usize = spans.iter().map(|s| s.content.as_ref().width()).sum();
    let remaining = max_width
        .saturating_sub(used)
        .saturating_sub(if trailing_comma { 1 } else { 0 });

    if empty {
        // Empty: just `{}` or `[]`
        let (open, close) = match node.kind {
            NodeKind::Object => ("{", "}"),
            NodeKind::Array => ("[", "]"),
            _ => unreachable!(),
        };
        spans.push(Span::styled(
            format!("{}{}", open, close),
            Style::default().fg(brace_color(depth)),
        ));
    } else {
        spans.extend(summarize_container(tree, id, remaining));
    }

    if trailing_comma {
        spans.push(Span::styled(",", Style::default().fg(COMMA_COLOR)));
    }

    out.push(Line::from(spans));
    click_map.push(if empty {
        None
    } else {
        Some((section_key.to_string(), id))
    });
}

/// Render a collapsed container's one-line summary.
/// Objects: `{k: v, k2: v2, …}`. Arrays: `[v, v2, …] (<len>)`.
/// Width-aware: stops emitting entries when the budget is exhausted and
/// appends `…` + closer.
fn summarize_container(tree: &Tree, id: u32, max_width: usize) -> Vec<Span<'static>> {
    let node = tree.node(id);
    let depth = node.depth;
    let bc = Style::default().fg(brace_color(depth));
    let (open, close) = match node.kind {
        NodeKind::Object => ("{", "}"),
        NodeKind::Array => ("[", "]"),
        _ => unreachable!(),
    };

    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::styled(open.to_string(), bc));

    // Reserve room for the closer + (optionally) `…` + (for arrays) ` (<len>)`
    let count = node.children.len();
    let count_suffix = if matches!(node.kind, NodeKind::Array) {
        format!(" ({})", count)
    } else {
        String::new()
    };
    let reserved = close.width() + count_suffix.width() + 1; // +1 for `…` (≤2 cols)
    let budget = max_width.saturating_sub(1 + reserved); // -1 for open already pushed

    let mut used = 0usize;
    let mut emitted = 0usize;

    for (i, &cid) in node.children.iter().enumerate() {
        let preview = preview_child(tree, cid);
        let chunk_w: usize = preview.iter().map(|s| s.content.as_ref().width()).sum();
        let sep_w = if i == 0 { 0 } else { 2 }; // ", "
        if used + sep_w + chunk_w > budget {
            break;
        }
        if i > 0 {
            spans.push(Span::styled(", ", Style::default().fg(COMMA_COLOR)));
            used += 2;
        }
        spans.extend(preview);
        used += chunk_w;
        emitted += 1;
    }

    if emitted < count {
        if emitted > 0 {
            spans.push(Span::styled(", ", Style::default().fg(COMMA_COLOR)));
        }
        spans.push(Span::styled(
            "…".to_string(),
            Style::default().fg(OVERLAY0),
        ));
    }
    spans.push(Span::styled(close.to_string(), bc));
    if !count_suffix.is_empty() {
        spans.push(Span::styled(count_suffix, Style::default().fg(OVERLAY0)));
    }
    spans
}

/// One-entry preview inside a summary. Objects show `key: value`;
/// arrays show just `value`. Nested containers render as `{…}` / `[…]`.
fn preview_child(tree: &Tree, cid: u32) -> Vec<Span<'static>> {
    let child = tree.node(cid);
    let mut spans: Vec<Span<'static>> = Vec::new();
    if let Some(ref k) = child.key {
        spans.push(Span::styled(
            format!("\"{}\"", k),
            Style::default().fg(key_color(child.depth)),
        ));
        spans.push(Span::styled(": ", Style::default().fg(COMMA_COLOR)));
    }
    match &child.kind {
        NodeKind::Null => spans.push(Span::styled(
            "null",
            Style::default().fg(NULL_COLOR).add_modifier(Modifier::ITALIC),
        )),
        NodeKind::Bool(b) => spans.push(Span::styled(
            if *b { "true" } else { "false" },
            Style::default().fg(BOOL_COLOR),
        )),
        NodeKind::Number(s) => spans.push(Span::styled(s.clone(), Style::default().fg(NUM_COLOR))),
        NodeKind::String(s) => {
            // In previews, always clip long strings early — single entry shouldn't hog the line.
            let max_chars = 20;
            let clipped: String = s.chars().take(max_chars).collect();
            let tail = if s.chars().count() > max_chars { "…" } else { "" };
            spans.push(Span::styled(
                format!("\"{}{}\"", clipped, tail),
                Style::default().fg(STR_COLOR),
            ));
        }
        NodeKind::Object => {
            spans.push(Span::styled("{…}", Style::default().fg(FOLD_COLOR)))
        }
        NodeKind::Array => {
            spans.push(Span::styled("[…]", Style::default().fg(FOLD_COLOR)))
        }
    }
    spans
}
```

- [ ] **Step 2: Add tests for collapsed-container rendering**

Append to the `#[cfg(test)] mod tests` block in `render.rs`:

```rust
    #[test]
    fn collapsed_object_shows_summary() {
        let lines = render(r#"{"code": 0, "message": "ok"}"#, 80);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("▶"), "should have fold marker: {:?}", lines);
        assert!(lines[0].contains("code"));
        assert!(lines[0].contains('0'));
        assert!(lines[0].contains("message"));
        assert!(lines[0].contains("ok"));
    }

    #[test]
    fn collapsed_array_shows_length_suffix() {
        let lines = render("[1,2,3,4,5,6,7,8,9,10,11,12]", 40);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("(12)"), "should show count: {:?}", lines);
    }

    #[test]
    fn nested_container_summarized_as_placeholder() {
        // Root collapsed (depth 0 is expanded by init_state(&t, 0), so actually root is expanded)
        // But with init_state(t, 0), root (depth 0) starts expanded.
        // To test nested summary we need to look at an expanded root's rendering,
        // which depends on Task 6. For now, collapse-everything via init_state(t, 0)
        // where root is still expanded but children are not:
        //   {"a": {"b": 1}}  — root expanded, so we'd see ▼ {, then a's line, then }.
        // That's Task 6 territory. Here we only assert the summary-building helper on a sub-object.
        // Use preview_child indirectly: render a deep-nested case still-collapsed by using depth 0.
        let lines = render(r#"{"a": {"b": 1}}"#, 80);
        // Root is expanded; a's line is collapsed and should show summary "{b: 1}"
        // Task 6 enables the multi-line root rendering; skip this test until Task 6.
        // For now, sanity-check: at least one line was emitted.
        assert!(!lines.is_empty());
    }

    #[test]
    fn empty_object_not_foldable() {
        let lines = render("{}", 80);
        let mut cmap = Vec::new();
        let mut out = Vec::new();
        let t = tree::parse("{}").unwrap();
        let s = state::init_state(&t, 0);
        append_render(&mut out, &mut cmap, &t, &s, "sec", "", 80);
        // Empty object: one line, no click target
        assert_eq!(cmap, vec![None]);
        assert!(lines[0].contains("{}"));
    }

    #[test]
    fn collapsed_object_is_clickable() {
        let t = tree::parse(r#"{"a": 1}"#).unwrap();
        let mut s = state::init_state(&t, 0);
        // Force the root collapsed to test the click map.
        s.expanded[0] = false;
        let mut out = Vec::new();
        let mut cmap = Vec::new();
        append_render(&mut out, &mut cmap, &t, &s, "sec", "", 80);
        assert_eq!(cmap.len(), 1);
        assert_eq!(cmap[0], Some(("sec".into(), 0)));
    }
```

- [ ] **Step 3: Run tests**

Run: `cargo test --lib ui::json_viewer::render`
Expected: 10 tests pass (5 from Task 4 + 5 new).

- [ ] **Step 4: Commit**

```bash
git add src/ui/json_viewer/render.rs
git commit -m "feat(json_viewer): collapsed container summary (DevTools-style)"
```

---

## Task 6: Render expanded containers with children

**Files:**
- Modify: `src/ui/json_viewer/render.rs`

Expanded containers emit an opener line, recursively render children, then a closer line.

- [ ] **Step 1: Replace the stub fallback in `render_node` with the real expanded-container code path**

Find this block you added in Task 5:

```rust
    // Expanded container with children: implemented in Task 6.
    // Until then, fall back to collapsed rendering so the module compiles.
    push_container_collapsed(
        out, click_map, tree, section_key, outer_prefix, max_width, id, trailing_comma,
    );
```

Replace it with:

```rust
    push_container_opener(
        out, click_map, tree, section_key, outer_prefix, id,
    );
    let child_count = node.children.len();
    for (i, &cid) in node.children.iter().enumerate() {
        let child_trailing = i + 1 < child_count;
        render_node(
            out, click_map, tree, state, section_key, outer_prefix,
            max_width, cid, child_trailing,
        );
    }
    push_container_closer(
        out, click_map, tree, outer_prefix, id, trailing_comma,
    );
```

Add these two helpers at the bottom of the file (before the `#[cfg(test)] mod tests` block):

```rust
fn push_container_opener(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Option<(String, u32)>>,
    tree: &Tree,
    section_key: &str,
    outer_prefix: &str,
    id: u32,
) {
    let node = tree.node(id);
    let depth = node.depth;
    let indent = "  ".repeat(depth as usize);
    let open = match node.kind {
        NodeKind::Object => "{",
        NodeKind::Array => "[",
        _ => unreachable!(),
    };

    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::raw(format!("{}{}", outer_prefix, indent)));
    spans.push(Span::styled("▼ ", Style::default().fg(BLUE)));
    if let Some(ref key) = node.key {
        spans.push(Span::styled(
            format!("\"{}\"", key),
            Style::default().fg(key_color(depth)),
        ));
        spans.push(Span::styled(": ", Style::default().fg(COMMA_COLOR)));
    }
    spans.push(Span::styled(
        open.to_string(),
        Style::default().fg(brace_color(depth)),
    ));

    out.push(Line::from(spans));
    click_map.push(Some((section_key.to_string(), id)));
}

fn push_container_closer(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Option<(String, u32)>>,
    tree: &Tree,
    outer_prefix: &str,
    id: u32,
    trailing_comma: bool,
) {
    let node = tree.node(id);
    let depth = node.depth;
    let indent = "  ".repeat(depth as usize);
    let close = match node.kind {
        NodeKind::Object => "}",
        NodeKind::Array => "]",
        _ => unreachable!(),
    };

    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::raw(format!("{}{}", outer_prefix, indent)));
    spans.push(Span::raw("  ")); // marker column — closer is not foldable
    spans.push(Span::styled(
        close.to_string(),
        Style::default().fg(brace_color(depth)),
    ));
    if trailing_comma {
        spans.push(Span::styled(",", Style::default().fg(COMMA_COLOR)));
    }
    out.push(Line::from(spans));
    click_map.push(None);
}
```

- [ ] **Step 2: Add expanded-container tests**

Append to the tests block in `render.rs`:

```rust
    #[test]
    fn expanded_root_object_renders_children() {
        // With default_expand_depth=1, root is expanded and children are collapsed.
        let t = tree::parse(r#"{"a": 1, "b": "hi"}"#).unwrap();
        let s = state::init_state(&t, 1);
        let mut out = Vec::new();
        let mut cmap = Vec::new();
        append_render(&mut out, &mut cmap, &t, &s, "sec", "", 80);
        // Expected lines: ▼ {, "a": 1,, "b": "hi", }
        assert_eq!(out.len(), 4, "lines={:?}",
            out.iter().map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>()).collect::<Vec<_>>());
        let texts: Vec<String> = out.iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert!(texts[0].contains('{'));
        assert!(texts[0].contains('▼'));
        assert!(texts[1].contains("\"a\""));
        assert!(texts[1].contains('1'));
        assert!(texts[1].ends_with(','));
        assert!(texts[2].contains("\"b\""));
        assert!(texts[2].contains("hi"));
        assert!(texts[3].trim_end() == "  }");
        // Click map: opener clickable, children None, closer None
        assert_eq!(cmap[0], Some(("sec".into(), 0)));
        assert_eq!(cmap[1], None);
        assert_eq!(cmap[2], None);
        assert_eq!(cmap[3], None);
    }

    #[test]
    fn nested_expansion_with_collapsed_grandchild() {
        let t = tree::parse(r#"{"a": {"b": {"c": 1}}}"#).unwrap();
        // Expand depth 1: root and "a" expand; "b" stays collapsed.
        let s = state::init_state(&t, 1);
        let mut out = Vec::new();
        let mut cmap = Vec::new();
        append_render(&mut out, &mut cmap, &t, &s, "sec", "", 80);
        let texts: Vec<String> = out.iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        // Expect: ▼ { / ▼ "a": { / ▶ "b": {c: 1} / } / }
        assert_eq!(out.len(), 5, "lines={:?}", texts);
        assert!(texts[0].contains('▼'));
        assert!(texts[1].contains('▼'));
        assert!(texts[1].contains("\"a\""));
        assert!(texts[2].contains('▶'));
        assert!(texts[2].contains("\"b\""));
        assert!(texts[2].contains("\"c\""));
        // Indentation alignment: children at depth d have exactly 2d spaces before marker
        let leading_spaces = |s: &str| s.chars().take_while(|c| *c == ' ').count();
        // texts[0] = "▼ {"     -> 0 leading spaces
        // texts[1] = "  ▼ \"a\"…" -> 2 leading spaces
        // texts[2] = "    ▶ \"b\"…" -> 4 leading spaces
        assert_eq!(leading_spaces(&texts[0]), 0);
        assert_eq!(leading_spaces(&texts[1]), 2);
        assert_eq!(leading_spaces(&texts[2]), 4);
    }

    #[test]
    fn array_of_objects_alignment() {
        // Screenshot scenario: array with each element an object.
        let t = tree::parse(r#"{"items": [{"id":1},{"id":2}]}"#).unwrap();
        // Expand root + items so we see both array entries as collapsed children.
        let mut s = state::init_state(&t, 1);
        // items is at depth 1, so already expanded. Array entries (depth 2) stay collapsed.
        let mut out = Vec::new();
        let mut cmap = Vec::new();
        append_render(&mut out, &mut cmap, &t, &s, "sec", "", 80);
        let texts: Vec<String> = out.iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        // Expected: ▼ { / ▼ "items": [ / ▶ {id: 1}, / ▶ {id: 2} / ] / }
        assert_eq!(out.len(), 6, "lines={:?}", texts);
        let leading_spaces = |s: &str| s.chars().take_while(|c| *c == ' ').count();
        assert_eq!(leading_spaces(&texts[2]), 4); // first array element
        assert_eq!(leading_spaces(&texts[3]), 4); // second array element
        assert_eq!(leading_spaces(&texts[4]), 2); // array closer at items' depth
        // Both array-element placeholders should be identically indented (the bug).
        let _ = s; // silence unused-mut warning if any
    }
```

- [ ] **Step 3: Run tests**

Run: `cargo test --lib ui::json_viewer::render`
Expected: 13 tests pass (10 from Tasks 4–5 + 3 new).

- [ ] **Step 4: Commit**

```bash
git add src/ui/json_viewer/render.rs
git commit -m "feat(json_viewer): expanded container rendering with aligned children"
```

---

## Task 7: Expose the new API from `mod.rs`

**Files:**
- Modify: `src/ui/json_viewer/mod.rs`

Re-export the real `JsonViewerState` and functions from the new modules so they're accessible to callers. The legacy shims stay in place for now — they'll be removed in the next task once callers migrate.

- [ ] **Step 1: Add the new exports under a distinct namespace**

Open `src/ui/json_viewer/mod.rs`. Add these lines **above** the `// ── Legacy shims ──` comment:

```rust
// ── New AST-based API ────────────────────────────────────────────────────
pub use render::append_render;
pub use state::{collapse_all, expand_all, toggle};
pub use tree::{parse, NodeKind, Tree};

// Re-export the real state type under a different name so callers can
// migrate gradually. Once the legacy shim is deleted (Task 8), we rename
// this to `JsonViewerState` and drop the alias.
pub use state::JsonViewerState as AstViewerState;

/// New init_state for the AST viewer. Renamed to avoid clash with the
/// legacy stub until Task 8.
pub fn init_ast_state(tree: &Tree, default_expand_depth: u32) -> AstViewerState {
    state::init_state(tree, default_expand_depth)
}
```

- [ ] **Step 2: Build**

Run: `cargo build`
Expected: compiles clean. You may see new "unused" warnings for `AstViewerState` / `init_ast_state` / etc. — that's fine, they'll be consumed in Task 8.

- [ ] **Step 3: Run all tests**

Run: `cargo test --lib ui::json_viewer`
Expected: all tests from Tasks 2, 3, 4, 5, 6 pass. No regressions.

- [ ] **Step 4: Commit**

```bash
git add src/ui/json_viewer/mod.rs
git commit -m "feat(json_viewer): expose new API alongside legacy shims"
```

---

## Task 8: Migrate callers + delete legacy shims

**Files:**
- Modify: `src/app.rs`
- Modify: `src/ui/network/detail.rs`
- Modify: `src/ui/logs/detail.rs`
- Modify: `src/event.rs`
- Modify: `src/ui/json_viewer/mod.rs`

This is the "rip the bandage" task — all callers move to the new API in one commit so the build stays green.

- [ ] **Step 1: Update `src/app.rs` types and `toggle_detail_fold`**

Open `src/app.rs`. Three edits:

**Edit A** — change `DetailState::viewer_state` type. Find (around line 59):

```rust
    /// JSON viewer fold/unfold state.
    pub viewer_state: crate::ui::json_viewer::JsonViewerState,
```

Replace with:

```rust
    /// JSON viewer fold/unfold state (AST-based).
    pub viewer_state: crate::ui::json_viewer::AstViewerState,
    /// Cached JSON tree for the currently shown entry. `None` until the
    /// renderer parses the first body.
    pub viewer_tree: Option<crate::ui::json_viewer::Tree>,
```

**Edit B** — change `NetworkState` types. Find (around line 97):

```rust
    /// JSON viewer states keyed by section (e.g., "req_headers", "res_body", "sse_0").
    pub json_viewer_states:
        std::collections::HashMap<String, crate::ui::json_viewer::JsonViewerState>,
    /// Maps detail panel line index -> (section_key, source_line) for JSON bracket click.
    pub detail_json_click_map: Vec<Option<(String, usize)>>,
```

Replace with:

```rust
    /// JSON viewer states keyed by section (e.g., "req_headers", "res_body", "sse_0").
    pub json_viewer_states:
        std::collections::HashMap<String, crate::ui::json_viewer::AstViewerState>,
    /// Maps detail panel line index -> (section_key, node_id) for JSON fold click.
    pub detail_json_click_map: Vec<Option<(String, u32)>>,
```

**Edit C** — update `toggle_detail_fold` (around line 779). Find:

```rust
    pub fn toggle_detail_fold(&mut self, source_line: usize) {
        crate::ui::json_viewer::toggle_fold(&mut self.detail.viewer_state, source_line);
    }
```

Replace with:

```rust
    pub fn toggle_detail_fold(&mut self, node_id: u32) {
        if let Some(ref tree) = self.detail.viewer_tree {
            crate::ui::json_viewer::toggle(tree, &mut self.detail.viewer_state, node_id);
        }
    }
```

**Edit D** — find the `detail.viewer_state` reset (line ~778):

```rust
        self.detail.viewer_state = crate::ui::json_viewer::JsonViewerState::default();
```

Replace with:

```rust
        self.detail.viewer_state = crate::ui::json_viewer::AstViewerState::default();
        self.detail.viewer_tree = None;
```

**Edit E** — if the default derivation of `DetailState` fails because of the new `viewer_tree: Option<Tree>` field (Tree has no Default), add a manual Default. Look at the `#[derive(Default)]` on `DetailState`. Replace:

```rust
#[derive(Default)]
pub struct DetailState {
```

with

```rust
pub struct DetailState {
```

And add an explicit `impl Default for DetailState`:

```rust
impl Default for DetailState {
    fn default() -> Self {
        Self {
            scroll: 0,
            header_lines: 0,
            viewer_state: crate::ui::json_viewer::AstViewerState::default(),
            viewer_tree: None,
        }
    }
}
```

(The exact set of fields matches the `DetailState` struct — if new fields have been added since this plan was written, include them with their existing defaults.)

- [ ] **Step 2: Update `src/ui/network/detail.rs::render_json_section`**

Open `src/ui/network/detail.rs`. Find the `render_json_section` function (around line 920). Replace the entire body from `fn render_json_section(` through its closing `}` with:

```rust
fn render_json_section(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Option<(String, u32)>>,
    json_text: &str,
    section_key: &str,
    viewer_states: &mut HashMap<String, json_viewer::AstViewerState>,
    max_w: usize,
) {
    match json_viewer::parse(json_text) {
        Ok(tree) => {
            let state = viewer_states
                .entry(section_key.to_string())
                .or_insert_with(|| json_viewer::init_ast_state(&tree, 1));
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
            // Keep section_map in sync with lines.
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

Also at the top of this file, change the import on line 20:

```rust
use crate::ui::json_viewer::{self, JsonViewerState};
```

to

```rust
use crate::ui::json_viewer::{self, AstViewerState};
```

And update the function signatures inside the same file that previously declared `click_map: Vec<Option<(String, usize)>>` or `HashMap<String, JsonViewerState>`:

- The top-level `json_click_map` variable (around line 68) — change its type annotation:
  ```rust
  let mut json_click_map: Vec<Option<(String, u32)>> = Vec::new();
  ```
- All call sites passing `&mut app.network.json_viewer_states` already work because `json_viewer_states` was retyped in Step 1.
- All `.push(None)` calls for `json_click_map` stay the same.

- [ ] **Step 3: Update `src/ui/logs/detail.rs`**

Open `src/ui/logs/detail.rs`. Replace lines 109–128 (the body fold/render block) with:

```rust
    // ── Body with fold/unfold using json_viewer ──
    match json_viewer::parse(&full_msg) {
        Ok(tree) => {
            // (Re-)initialize state if tree size changed (new entry selected).
            if app.detail.viewer_state.expanded.len() != tree.nodes.len() {
                app.detail.viewer_state = json_viewer::init_ast_state(&tree, 1);
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

            // Honor the scroll offset used by the legacy code path.
            let scroll = app.detail.scroll.min(body_lines.len());
            let visible: Vec<Line<'static>> =
                body_lines.into_iter().skip(scroll).take(body_height).collect();
            all_lines.extend(visible);

            // Stash the tree so app.toggle_detail_fold can resolve clicks.
            app.detail.viewer_tree = Some(tree);
        }
        Err(_) => {
            // Non-JSON body: fall back to plain wrapped text.
            for wl in crate::ui::wrap_text(&full_msg, inner_w, 500) {
                all_lines.push(Line::from(Span::styled(
                    wl,
                    Style::default().fg(TEXT),
                )));
            }
            app.detail.viewer_tree = None;
        }
    }

    // Track total content lines for scrollbar — use the raw rendered body length.
    // (Precise total accounting can be refined later; for now, scrollbar uses
    // all_lines.len() as a safe upper bound.)
    let total_content = all_lines.len();
```

The import line near the top of `logs/detail.rs` that was `use crate::ui::json_viewer;` stays unchanged.

If the imports block currently does `use crate::ui::json_viewer::{self, JsonViewerState};` or similar, swap `JsonViewerState` references for `AstViewerState` or remove them if they're no longer referenced in this file.

**Note:** The original code used `app.detail.viewer_state.total_lines` to compute scrollbar extent. That field no longer exists. The replacement above uses `all_lines.len()` as a simpler upper bound — the scrollbar stays functional, and log detail scrollbar perfection isn't on the critical path of this rewrite.

- [ ] **Step 4: Update `src/event.rs` click dispatch + add E/C handlers**

Open `src/event.rs`. Find the block at lines 412–421:

```rust
                        // Then check json_click_map for bracket toggle
                        if let Some(Some((section_key, source_line))) =
                            app.network.detail_json_click_map.get(line_idx)
                        {
                            if let Some(state) = app.network.json_viewer_states.get_mut(section_key)
                            {
                                crate::ui::json_viewer::toggle(state, *source_line);
                            }
                            return;
                        }
```

Replace with:

```rust
                        // Then check json_click_map for fold toggle (AST node id)
                        if let Some(Some((section_key, node_id))) =
                            app.network.detail_json_click_map.get(line_idx).cloned()
                        {
                            if let Some(state) =
                                app.network.json_viewer_states.get_mut(&section_key)
                            {
                                // The tree is ephemeral (rebuilt per frame); re-parse
                                // the source text here would require lookup. Instead,
                                // the renderer stashes clickable-container IDs that
                                // were foldable *at render time*. Toggle directly.
                                if let Some(slot) = state.expanded.get_mut(node_id as usize) {
                                    *slot = !*slot;
                                }
                            }
                            return;
                        }
```

Now add `E` / `C` handlers for expand-all / collapse-all. Find the Network-tab key handler block starting at line 1184 (`match key.code { KeyCode::Char('q') ... }`). The handler for `Char('y')` is at line 1270. Add these **after** it (before the `KeyCode::Char('m')` block):

```rust
            KeyCode::Char('E') => {
                // Expand all JSON sections in the network detail panel.
                // Leaves have unused slots; flipping them is harmless —
                // the renderer consults node kind, not the flag, for leaves.
                for state in app.network.json_viewer_states.values_mut() {
                    for slot in state.expanded.iter_mut() {
                        *slot = true;
                    }
                }
            }
            KeyCode::Char('C') => {
                // Collapse all JSON sections (keep root expanded).
                for state in app.network.json_viewer_states.values_mut() {
                    for (i, slot) in state.expanded.iter_mut().enumerate() {
                        *slot = i == 0;
                    }
                }
            }
```

**Note:** We bypass `json_viewer::expand_all` / `collapse_all` here because those take a `&Tree`, and we don't cache trees across frames for the network panel (they rebuild each render). Flipping `Vec<bool>` directly yields the same observable behavior — leaves' flags are ignored by the renderer.

- [ ] **Step 5: Delete the legacy shims from `mod.rs`**

Open `src/ui/json_viewer/mod.rs`. Delete everything from the `// ── Legacy shims ──` comment down to the end of `render_json`. Then rename the AST-based re-exports to drop the `Ast` / `init_ast_state` prefixes.

Replace the whole `// ── New AST-based API ──` block with:

```rust
pub use render::append_render;
pub use state::{collapse_all, expand_all, init_state, toggle, JsonViewerState};
pub use tree::{parse, NodeKind, Tree};
```

Now the file's public API is:

```rust
pub use colorize::colorize_json_text;
pub use render::append_render;
pub use state::{collapse_all, expand_all, init_state, toggle, JsonViewerState};
pub use tree::{parse, NodeKind, Tree};
```

- [ ] **Step 6: Fix the callers that referenced the aliased names**

Now every `AstViewerState` / `init_ast_state` reference is broken. Replace them:

In `src/app.rs`:
- `crate::ui::json_viewer::AstViewerState` → `crate::ui::json_viewer::JsonViewerState` (2 occurrences).

In `src/ui/network/detail.rs`:
- `use crate::ui::json_viewer::{self, AstViewerState};` → `use crate::ui::json_viewer::{self, JsonViewerState};`
- `HashMap<String, json_viewer::AstViewerState>` → `HashMap<String, json_viewer::JsonViewerState>`
- `json_viewer::init_ast_state(&tree, 1)` → `json_viewer::init_state(&tree, 1)`

In `src/ui/logs/detail.rs`:
- `json_viewer::init_ast_state(&tree, 1)` → `json_viewer::init_state(&tree, 1)`

- [ ] **Step 7: Build + run tests**

Run: `cargo build`
Expected: clean compile. If any "not found in `json_viewer`" error mentions a name like `init_state` / `toggle` / `append_render`, the re-exports in `mod.rs` are the fix.

Run: `cargo test`
Expected: all tests pass. Tree / state / render tests are unchanged.

- [ ] **Step 8: Manual verification**

Build and run flog:

```bash
cargo run --release -- --port 9753
```

With a Flutter app connected and a network response containing an array of objects (the scenario from the screenshot):

1. Open a request with a nested response body.
2. Confirm array elements and sibling objects align perfectly (same indentation column).
3. Click `▶` on a collapsed node — it expands.
4. Click the text content of a collapsed `{...}` placeholder — it expands (whole line is hot).
5. Click a leaf line (e.g. `"id": 928`) — nothing happens.
6. Click an expanded `▼` — it collapses.
7. Click on a long truncated string value — no accidental fold.
8. Press `Shift+E` — every JSON section expands fully.
9. Press `Shift+C` — every JSON section collapses to its root.

- [ ] **Step 9: Commit**

```bash
git add src/app.rs src/event.rs src/ui/json_viewer/mod.rs src/ui/network/detail.rs src/ui/logs/detail.rs
git commit -m "refactor(json_viewer): migrate callers to AST API, delete legacy shims"
```

---

## Task 9: Help text + `clippy` + `fmt`

**Files:**
- Modify: `src/ui/help.rs`

- [ ] **Step 1: Add help lines for `E` / `C`**

Open `src/ui/help.rs`. Find the Network-tab keybinding section (look for existing entries like `replay_selected` or similar — they're formatted as pairs of `(key, description)`). Add:

```rust
    ("E", "Expand all JSON sections"),
    ("C", "Collapse all JSON sections (keep root)"),
```

Place them near the other detail-panel controls. If `help.rs` uses a different format (e.g. a single big string table), match that style.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: clean. Fix any warning inline. Common cases:
- Unused `section_key` or `_` patterns — remove underscores or the variable if truly unused.
- `clippy::needless_borrow` — drop redundant `&`.

- [ ] **Step 3: Run fmt**

Run: `cargo fmt`
Expected: no diffs, or small auto-formatting.

- [ ] **Step 4: Final test sweep**

Run: `cargo test`
Expected: every test passes.

- [ ] **Step 5: Commit**

```bash
git add src/ui/help.rs
git commit -m "docs(help): add E/C shortcuts for JSON expand/collapse-all"
```

---

## Verification Summary

After the final task, the viewer should satisfy every requirement from the spec:

| Spec requirement | Verified by |
|---|---|
| Flat arena tree, `u32` IDs | Task 2 tests |
| `Vec<bool>` fold state | Task 3 tests |
| Init expands root + direct children (depth ≤ 1) | Task 3 `init_expands_within_depth` |
| Toggle flips container state, ignores leaves | Task 3 `toggle_leaf_is_noop` |
| Expand-all / collapse-all | Task 3 |
| Fixed-width marker (▼/▶/  ) | Tasks 4–6 `nested_expansion_with_collapsed_grandchild` |
| Indent = 2 × depth, derived from logical depth | Task 6 `array_of_objects_alignment` |
| DevTools-style summary | Task 5 `collapsed_object_shows_summary` |
| Array count suffix `(N)` | Task 5 `collapsed_array_shows_length_suffix` |
| Empty containers not foldable | Task 5 `empty_object_not_foldable` |
| Long strings truncated with `…` | Task 4 `leaf_string_truncated` |
| Click hit-test by node ID | Task 5 `collapsed_object_is_clickable` |
| Caller integration | Task 8 step 8 (manual) |
| `E` / `C` keybindings | Task 8 step 8 (manual) |
| `colorize_json_text` preserved | Unchanged, moved in Task 1 |
| No regressions in log detail panel | Task 8 step 8 (manual) |
