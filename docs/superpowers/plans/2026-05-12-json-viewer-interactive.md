# JSON Viewer Interactive Enhancements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add three interactive features to the shared `ui/json_viewer`: per-container ⧉ copy icon, clickable + keyboard-openable URLs, and an overlay that expands `…`-truncated string values on demand.

**Architecture:** Extend `viewer_click_map` from `Vec<Option<u32>>` to `Vec<Vec<JsonHotRegion>>` so each row can carry multiple typed `JsonAction` hot regions. Route actions through the existing two-phase `detect`/`apply` click dispatch. Add a `FullValueOverlay` `AppMode` and a `viewer_cursor` row cursor (scoped to detail panels). Keyboard: `J`/`K` move cursor, `Enter` triggers first non-fold action, `o` opens URL, `y` copies subtree, `Esc` closes overlay.

**Tech Stack:** Rust, ratatui, crossterm, serde_json, regex (all already in Cargo.toml).

**Spec reference:** `docs/superpowers/specs/2026-05-12-json-viewer-interactive-design.md`

---

## Task 1: Introduce `JsonAction` + `JsonHotRegion` types

This is a type-only change. We introduce the new types and propagate the shape through every Vec in the chain. Behavior is preserved: every row that previously had `Some(node_id)` now has a one-element Vec with `ToggleFold(node_id)`; every row that had `None` now has an empty Vec.

**Files:**
- Create: `src/ui/json_viewer/action.rs`
- Modify: `src/ui/json_viewer/mod.rs`
- Modify: `src/ui/json_viewer/render/mod.rs`
- Modify: `src/ui/json_viewer/render/lines.rs`
- Modify: `src/app/state_structs.rs:93`
- Modify: `src/ui/logs/detail/section.rs:37-45`
- Modify: `src/ui/logs/detail/renderers.rs` (all returns)
- Modify: `src/ui/logs/detail/mod.rs:155-160`
- Modify: `src/event/apply.rs:67-73,104-113`
- Modify: `tests/characterization_app_state.rs:759-763`

- [ ] **Step 1.1: Create action types**

Write `src/ui/json_viewer/action.rs`:

```rust
//! Action types for interactive hot regions in the JSON viewer.
//!
//! Each rendered row owns zero or more `JsonHotRegion`s — non-overlapping
//! column ranges, each mapped to a `JsonAction`. The detect phase looks
//! up (line_idx, x) in this table; the apply phase executes the action.

use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonAction {
    /// Toggle fold state for a container node.
    ToggleFold(u32),
    /// Copy the subtree rooted at this node as pretty JSON.
    CopyNode(u32),
    /// Open this URL in the system browser. Carries the FULL URL even when
    /// the displayed text is truncated with `…`.
    OpenUrl(String),
    /// Show the full string value of this leaf node in an overlay.
    ExpandFullValue(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonHotRegion {
    pub range: Range<u16>,
    pub action: JsonAction,
}
```

- [ ] **Step 1.2: Export from `json_viewer::mod`**

Modify `src/ui/json_viewer/mod.rs`:

```rust
mod action;
mod colorize;
mod palette;
mod render;
mod state;
mod tree;

pub use action::{JsonAction, JsonHotRegion};
pub use colorize::colorize_json_text;
pub use render::append_render;
pub use state::{init_state, toggle, JsonViewerState};
pub use tree::Tree;
```

- [ ] **Step 1.3: Update `append_render` signature**

Modify `src/ui/json_viewer/render/mod.rs` — change the `click_map` parameter type and the inner `lines::render_node` call (called with new Vec type). Replace the `append_render` definition body:

```rust
pub fn append_render(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Vec<super::action::JsonHotRegion>>,
    tree: &Tree,
    state: &JsonViewerState,
    section_key: &str,
    outer_prefix: &str,
    max_width: usize,
) {
    lines::render_node(
        out,
        click_map,
        tree,
        state,
        section_key,
        outer_prefix,
        max_width,
        0,
        false,
    );
}
```

And update `src/ui/json_viewer/render/lines.rs` — change every `&mut Vec<Option<(String, u32)>>` parameter to `&mut Vec<Vec<super::super::action::JsonHotRegion>>`. At every old `click_map.push(None)` site, replace with `click_map.push(Vec::new())`. At every old `click_map.push(Some((section_key.to_string(), id)))` site, replace with:

```rust
click_map.push(vec![super::super::action::JsonHotRegion {
    range: 0..u16::MAX,   // whole-row fallback; Task 2 refines to exclude ⧉ column
    action: super::super::action::JsonAction::ToggleFold(id),
}]);
```

Since `range.contains(&x)` will be true for any u16 x, any click on a foldable row still toggles fold — preserving current behavior during the Task 1 intermediate state.

Update imports at top of `lines.rs`:

```rust
use super::super::action::{JsonAction, JsonHotRegion};
```

Then simplify the push sites to use the imported names.

- [ ] **Step 1.4: Update existing `json_viewer` render tests**

In `src/ui/json_viewer/render/mod.rs`, the `render` helper in `#[cfg(test)] mod tests` uses `let mut cmap = Vec::new()`. That still compiles since the element type is inferred from the `append_render` signature. Update assertions:

- `collapsed_object_is_clickable`: change
  ```rust
  assert_eq!(cmap[0], Some(("sec".into(), 0)));
  ```
  to
  ```rust
  assert_eq!(cmap[0].len(), 1);
  assert!(matches!(cmap[0][0].action, crate::ui::json_viewer::JsonAction::ToggleFold(0)));
  ```
- `empty_object_not_foldable`: change
  ```rust
  assert_eq!(cmap, vec![None]);
  ```
  to
  ```rust
  assert_eq!(cmap.len(), 1);
  assert!(cmap[0].is_empty());
  ```
- `expanded_root_object_renders_children`: change
  ```rust
  assert_eq!(cmap[0], Some(("sec".into(), 0)));
  assert_eq!(cmap[1], None);
  assert_eq!(cmap[2], None);
  assert_eq!(cmap[3], None);
  ```
  to
  ```rust
  assert_eq!(cmap[0].len(), 1);
  assert!(matches!(cmap[0][0].action, crate::ui::json_viewer::JsonAction::ToggleFold(0)));
  assert!(cmap[1].is_empty());
  assert!(cmap[2].is_empty());
  assert!(cmap[3].is_empty());
  ```

- [ ] **Step 1.5: Update `DetailState::viewer_click_map` type**

Modify `src/app/state_structs.rs:93`:

```rust
pub viewer_click_map: Vec<Vec<crate::ui::json_viewer::JsonHotRegion>>,
```

- [ ] **Step 1.6: Update `RenderRow` and logs detail pipeline**

Modify `src/ui/logs/detail/section.rs:37-45`:

```rust
/// `hot_regions` carries zero or more interactive regions for this row
/// (fold toggle, copy, URL, expand). Empty for non-interactive rows.
pub struct RenderRow {
    pub line: ratatui::text::Line<'static>,
    pub hot_regions: Vec<crate::ui::json_viewer::JsonHotRegion>,
}

pub trait SectionRenderer {
    fn render(&self, section: &Section, inner_w: usize, state: &mut crate::app::DetailState) -> Vec<RenderRow>;
}
```

Remove the old `click_target: Option<u32>` field entirely. Update `src/ui/logs/detail/renderers.rs`:

```rust
fn row(line: Line<'static>) -> RenderRow {
    RenderRow {
        line,
        hot_regions: Vec::new(),
    }
}
```

And in `JsonRenderer::render` where rows were built with `click_target: slot.map(|(_, id)| id)`:

```rust
.map(|(line, regions)| RenderRow { line, hot_regions: regions })
```

(After Task 1.3 the viewer returns a `Vec<Vec<JsonHotRegion>>` per rendered line; zip with `out` lines.)

In `src/ui/logs/detail/mod.rs:155-160`:

```rust
app.detail.viewer_click_map = body_rows
    .iter()
    .skip(scroll)
    .take(body_height)
    .map(|r| r.hot_regions.clone())
    .collect();
```

- [ ] **Step 1.7: Update `apply.rs` to read new type**

Modify `src/event/apply.rs:67-73` (logs detail):

```rust
ClickRegion::LogsDetailPanel { line_idx, x } => {
    if let Some(regions) = app.detail.viewer_click_map.get(line_idx) {
        for region in regions {
            if region.range.contains(&x) {
                if let crate::ui::json_viewer::JsonAction::ToggleFold(node_id) = region.action {
                    app.toggle_detail_fold(node_id);
                }
                return;
            }
        }
        // whitespace fallback: first ToggleFold
        for region in regions {
            if let crate::ui::json_viewer::JsonAction::ToggleFold(node_id) = region.action {
                app.toggle_detail_fold(node_id);
                return;
            }
        }
    }
}
```

(Task 4 will replace this block with a single `LogsDetailJsonAction` variant. For now preserve behavior exactly.)

Leave `NetworkDetailPanel` unchanged — it reads `detail_section_map`, not `viewer_click_map`.

- [ ] **Step 1.8: Update characterization test**

Modify `tests/characterization_app_state.rs:759-763`:

```rust
use crate::ui::json_viewer::{JsonAction, JsonHotRegion};
app.detail.viewer_click_map = vec![
    vec![JsonHotRegion { range: 0..u16::MAX, action: JsonAction::ToggleFold(1) }],
    vec![JsonHotRegion { range: 0..u16::MAX, action: JsonAction::ToggleFold(2) }],
];
app.reset_detail_for_selection();
assert!(app.detail.viewer_click_map.is_empty());
```

(Keep the test semantics: `reset_detail_for_selection` clears the map.)

- [ ] **Step 1.9: Run tests**

Run: `cargo test --lib json_viewer -- --nocapture`
Expected: PASS, including 12+ render tests.

Run: `cargo test --test characterization_app_state -- --nocapture`
Expected: PASS.

Run: `cargo build`
Expected: clean build.

- [ ] **Step 1.10: Commit**

```bash
git add src/ui/json_viewer src/app/state_structs.rs src/ui/logs/detail src/event/apply.rs tests/characterization_app_state.rs
git commit -m "refactor(json_viewer): replace click_map with typed JsonAction hot regions

No behavior change — each row's old Option<node_id> is now a Vec containing
either zero or one ToggleFold action. Sets up per-row multi-region dispatch
for upcoming copy/URL/expand features."
```

---

## Task 2: `⧉` copy icon + `CopyNode` action

Add the `⧉` glyph at the right end of every non-empty collapsible row, register a `CopyNode` region at its column range, and wire up the click path to copy the subtree to clipboard.

**Files:**
- Modify: `src/ui/json_viewer/render/lines.rs`
- Modify: `src/ui/json_viewer/render/summaries.rs`
- Modify: `src/event/click_region.rs`
- Modify: `src/event/detect.rs`
- Modify: `src/event/detect_net.rs` (if it handles JSON viewer clicks — verify)
- Modify: `src/event/apply.rs`
- Modify: `src/event/actions.rs`

- [ ] **Step 2.1: Write failing test — non-empty container has copy icon region**

Add to `src/ui/json_viewer/render/mod.rs` tests:

```rust
#[test]
fn collapsed_non_empty_container_has_copy_icon() {
    use crate::ui::json_viewer::JsonAction;
    let t = tree::parse(r#"{"a": 1}"#).unwrap();
    let mut s = state::init_state(&t, 0);
    s.expanded[0] = false;
    let mut out = Vec::new();
    let mut cmap = Vec::new();
    append_render(&mut out, &mut cmap, &t, &s, "sec", "", 80);
    // Two regions: ToggleFold (row body) + CopyNode (trailing ⧉).
    assert!(cmap[0].iter().any(|r| matches!(r.action, JsonAction::CopyNode(0))));
    let rendered: String = out[0].spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(rendered.trim_end().ends_with('⧉'), "missing ⧉ at end: {:?}", rendered);
}

#[test]
fn empty_container_has_no_copy_icon() {
    let t = tree::parse("{}").unwrap();
    let s = state::init_state(&t, 0);
    let mut out = Vec::new();
    let mut cmap = Vec::new();
    append_render(&mut out, &mut cmap, &t, &s, "sec", "", 80);
    let rendered: String = out[0].spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(!rendered.contains('⧉'), "empty container should not have copy icon: {:?}", rendered);
    assert!(cmap[0].is_empty());
}

#[test]
fn expanded_opener_has_copy_icon() {
    use crate::ui::json_viewer::JsonAction;
    let t = tree::parse(r#"{"a": 1}"#).unwrap();
    let s = state::init_state(&t, 1); // root expanded
    let mut out = Vec::new();
    let mut cmap = Vec::new();
    append_render(&mut out, &mut cmap, &t, &s, "sec", "", 80);
    // Opener row is line 0. It has ToggleFold + CopyNode.
    assert!(cmap[0].iter().any(|r| matches!(r.action, JsonAction::CopyNode(0))));
    let opener: String = out[0].spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(opener.trim_end().ends_with('⧉'), "opener missing ⧉: {:?}", opener);
}
```

- [ ] **Step 2.2: Run test to verify it fails**

Run: `cargo test --lib json_viewer::render::tests::collapsed_non_empty_container_has_copy_icon`
Expected: FAIL (⧉ not rendered).

- [ ] **Step 2.3: Append `⧉` and register region — opener rows**

In `src/ui/json_viewer/render/lines.rs`, modify `push_container_opener`. After the existing spans are built and before `out.push`, append the copy icon and build the region list:

```rust
fn push_container_opener(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Vec<JsonHotRegion>>,
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
            format!("\"{}\"", sanitize_for_cell(key)),
            Style::default().fg(key_color(depth as usize)),
        ));
        spans.push(Span::styled(": ", Style::default().fg(COMMA_COLOR)));
    }
    spans.push(Span::styled(
        open.to_string(),
        Style::default().fg(brace_color(depth as usize)),
    ));

    // Append " ⧉" at the row end.
    let before_icon_w: u16 = spans.iter()
        .map(|s| s.content.as_ref().width() as u16)
        .sum();
    spans.push(Span::styled(" ⧉", Style::default().fg(OVERLAY0)));
    let after_icon_w = before_icon_w + 2; // " " + "⧉" both width 1

    out.push(Line::from(spans));
    let _ = section_key; // reserved for future re-use
    click_map.push(vec![
        JsonHotRegion {
            range: 0..before_icon_w,
            action: JsonAction::ToggleFold(id),
        },
        JsonHotRegion {
            range: before_icon_w..after_icon_w,
            action: JsonAction::CopyNode(id),
        },
    ]);
}
```

Add `OVERLAY0` to the imports at top of file:

```rust
use crate::ui::{sanitize_for_cell, BLUE, OVERLAY0};
```

- [ ] **Step 2.4: Append `⧉` and register region — collapsed non-empty rows**

In `push_container_collapsed`, after the summary is appended and before `out.push`:

```rust
// (existing code builds spans for indent + marker + key + summary)

if empty {
    // existing empty-container branch — no icon, no CopyNode.
    if trailing_comma {
        spans.push(Span::styled(",", Style::default().fg(COMMA_COLOR)));
    }
    out.push(Line::from(spans));
    click_map.push(Vec::new());
    return;
}

// Non-empty: append " ⧉" at row end.
let before_icon_w: u16 = spans.iter()
    .map(|s| s.content.as_ref().width() as u16)
    .sum();
let comma_w: u16 = if trailing_comma { 1 } else { 0 };
if trailing_comma {
    spans.push(Span::styled(",", Style::default().fg(COMMA_COLOR)));
}
spans.push(Span::styled(" ⧉", Style::default().fg(OVERLAY0)));
let icon_start = before_icon_w + comma_w;
let icon_end = icon_start + 2;

out.push(Line::from(spans));
click_map.push(vec![
    JsonHotRegion {
        range: 0..icon_start,
        action: JsonAction::ToggleFold(id),
    },
    JsonHotRegion {
        range: icon_start..icon_end,
        action: JsonAction::CopyNode(id),
    },
]);
```

Rewrite `push_container_collapsed` end-to-end so the flow above replaces the old `if trailing_comma { ... } out.push(...); click_map.push(...)` tail. Remove the old tail.

- [ ] **Step 2.5: Reserve summary budget for trailing ` ⧉`**

Modify `src/ui/json_viewer/render/summaries.rs` — increase reservation so summaries don't overflow when the icon is appended. Change the `reserved` line:

```rust
// Old: let reserved = close.width() + count_suffix.width() + 3;
let reserved = close.width() + count_suffix.width() + 3 + 2; // +2 for trailing " ⧉"
```

Now re-run the width tests — they assert total line width including ⧉ fits within `max_width`. The `collapsed_array_fits_within_max_width` and `collapsed_object_fits_within_max_width` tests will measure the whole rendered line, so the ⧉ must be included in their budget calculation. The existing tests assert `w <= max_w`; with the extra 2 columns reserved, they should still pass.

- [ ] **Step 2.6: Run tests to verify Task 2 tests + preservation**

Run: `cargo test --lib json_viewer`
Expected: PASS for all existing tests + three new tests from 2.1. Width tests must still pass.

- [ ] **Step 2.7: Add `ClickRegion::LogsDetailJsonAction`**

Modify `src/event/click_region.rs` — inside the `#[derive(Debug, Clone, PartialEq, Eq)] pub(crate) enum ClickRegion` block, after `LogsDetailClose`:

```rust
LogsDetailJsonAction(crate::ui::json_viewer::JsonAction),
```

And after `NetworkDetailClose`:

```rust
NetworkDetailJsonAction(crate::ui::json_viewer::JsonAction),
```

- [ ] **Step 2.8: Route detect → `*JsonAction` when hot region hits**

In `src/event/detect.rs`, find the code that returns `ClickRegion::LogsDetailPanel { line_idx, x }`. Add a pure helper near it:

```rust
fn detect_json_action_in_detail(
    click_map: &[Vec<crate::ui::json_viewer::JsonHotRegion>],
    line_idx: usize,
    x_in_panel: u16,
) -> Option<crate::ui::json_viewer::JsonAction> {
    let row = click_map.get(line_idx)?;
    for r in row {
        if r.range.contains(&x_in_panel) {
            return Some(r.action.clone());
        }
    }
    // Whitespace fallback: first ToggleFold on the row.
    row.iter().find_map(|r| match &r.action {
        crate::ui::json_viewer::JsonAction::ToggleFold(_) => Some(r.action.clone()),
        _ => None,
    })
}
```

Before returning `LogsDetailPanel { line_idx, x }`, call:

```rust
if let Some(action) = detect_json_action_in_detail(&app.detail.viewer_click_map, line_idx, x) {
    return Some(ClickRegion::LogsDetailJsonAction(action));
}
return Some(ClickRegion::LogsDetailPanel { line_idx, x });
```

(If the current detect returns `LogsDetailPanel` anyway, preserve that fallback so existing section-toggle behavior still runs when no JSON hot region matches — though for logs panel the old variant was only consumed by the ToggleFold path, which is now covered by the new variant.)

Do the same in `src/event/detect_net.rs` for `NetworkDetailPanel` — but only when the click coordinate is inside the JSON viewer's region (the network detail has other content like `detail_section_map`; preserve that path by only delegating to `detect_json_action_in_detail` when the line falls within the JSON viewer's row range).

**Verify first:** grep for how `NetworkDetailPanel` is detected. If it covers the whole detail body indiscriminately, and the JSON viewer is only on some lines, use `app.detail.viewer_click_map.get(line_idx).map(|r| !r.is_empty()).unwrap_or(false)` as the precondition for the JSON route; otherwise fall through to `NetworkDetailPanel`.

- [ ] **Step 2.9: Apply `LogsDetailJsonAction` + `NetworkDetailJsonAction`**

Modify `src/event/apply.rs`. Remove the old ToggleFold logic added in Task 1.7 and replace with the new variants:

```rust
// Replace Task 1.7's LogsDetailPanel block.
ClickRegion::LogsDetailPanel { .. } => {
    // Reserved for future: anything in the panel that is NOT a JSON hot
    // region. JSON hot regions are dispatched via LogsDetailJsonAction.
}
ClickRegion::LogsDetailJsonAction(action) => apply_json_action(app, action),

// And near the Network section:
ClickRegion::NetworkDetailJsonAction(action) => apply_json_action(app, action),
```

Add the dispatch helper at the bottom of `apply.rs`:

```rust
fn apply_json_action(app: &mut App, action: crate::ui::json_viewer::JsonAction) {
    use crate::ui::json_viewer::JsonAction;
    match action {
        JsonAction::ToggleFold(id) => app.toggle_detail_fold(id),
        JsonAction::CopyNode(id) => {
            let text = super::actions::extract_node_json(&app.detail.viewer_tree, id);
            let msg = super::actions::copy_to_clipboard(&text);
            app.show_status(msg);
        }
        // OpenUrl / ExpandFullValue come online in later tasks.
        JsonAction::OpenUrl(_) => {}
        JsonAction::ExpandFullValue(_) => {}
    }
}
```

- [ ] **Step 2.10: Add `extract_node_json` helper**

Add to `src/event/actions.rs` (after the existing functions):

```rust
/// Rebuild a subtree as `serde_json::Value` and pretty-print it.
pub(super) fn extract_node_json(
    tree: &Option<crate::ui::json_viewer::Tree>,
    node_id: u32,
) -> String {
    let Some(tree) = tree else { return String::new(); };
    let value = subtree_to_value(tree, node_id);
    serde_json::to_string_pretty(&value).unwrap_or_default()
}

fn subtree_to_value(
    tree: &crate::ui::json_viewer::Tree,
    id: u32,
) -> serde_json::Value {
    use crate::ui::json_viewer::NodeKind;
    let node = tree.node(id);
    match &node.kind {
        NodeKind::Null => serde_json::Value::Null,
        NodeKind::Bool(b) => serde_json::Value::Bool(*b),
        NodeKind::Number(s) => serde_json::from_str(s)
            .unwrap_or(serde_json::Value::String(s.clone())),
        NodeKind::String(s) => serde_json::Value::String(s.clone()),
        NodeKind::Object => {
            let mut map = serde_json::Map::new();
            for &cid in &node.children {
                let child = tree.node(cid);
                let k = child.key.clone().unwrap_or_default();
                map.insert(k, subtree_to_value(tree, cid));
            }
            serde_json::Value::Object(map)
        }
        NodeKind::Array => {
            let arr: Vec<_> = node.children.iter().map(|&cid| subtree_to_value(tree, cid)).collect();
            serde_json::Value::Array(arr)
        }
    }
}
```

**Required**: expose `Tree::node` and `NodeKind` to the event module. Check what `Tree` exports via `pub use tree::Tree` — if `Tree::node` and `NodeKind` are not public, either add a `pub` to them in `src/ui/json_viewer/tree.rs` or introduce a narrow `pub fn walk(&self, id, visitor)` helper. Prefer making `node` and `NodeKind` `pub` (they're already used across module boundaries within `json_viewer`).

Update `src/ui/json_viewer/mod.rs` to add:

```rust
pub use tree::{NodeKind, Tree};
```

Also ensure `NodeKind` and `Node` / `Tree::node` are `pub` in `src/ui/json_viewer/tree.rs` (change `pub(super)` → `pub` or `pub(crate)` as needed so the `event` module can walk them).

- [ ] **Step 2.11: Write characterization test — CopyNode apply path**

Create `tests/characterization_ui_047_json_interactive.rs`:

```rust
//! Characterization tests for UI-047 (interactive JSON viewer:
//! ⧉ copy, URL open, expand full value).

use flog::app::App;
use flog::ui::json_viewer;

#[test]
fn copy_node_extracts_pretty_json_subtree() {
    let tree = json_viewer::parse_for_test(r#"{"a": 1, "b": {"c": 2}}"#);
    let text = flog::event::actions_public::extract_node_json(&Some(tree), 2);
    // Node 2 is the "b" sub-object (root=0, a=1, b=2, c=3 in DFS).
    // Pretty-print should yield lines like {\n  "c": 2\n}.
    assert!(text.contains("\"c\""), "text: {}", text);
}
```

Add test-visible re-exports **without** `#[cfg(test)]` (integration tests link against a non-test build of the lib crate, so `#[cfg(test)]` items are invisible from `tests/`). Use explicit `#[doc(hidden)]` + `pub` instead:

In `src/ui/json_viewer/mod.rs`:

```rust
#[doc(hidden)]
pub fn parse_for_test(text: &str) -> Tree {
    tree::parse(text).expect("parse")
}
```

And in `src/event/mod.rs`:

```rust
#[doc(hidden)]
pub mod actions_public {
    pub use super::actions::extract_node_json;
}
```

Then import: `use flog::event::actions_public;`.

- [ ] **Step 2.12: Run the new test**

Run: `cargo test --test characterization_ui_047_json_interactive copy_node_extracts_pretty_json_subtree -- --nocapture`
Expected: PASS.

- [ ] **Step 2.13: Run full test suite**

Run: `cargo build && cargo test`
Expected: PASS.

- [ ] **Step 2.14: Commit**

```bash
git add src/ui/json_viewer src/event tests/characterization_ui_047_json_interactive.rs
git commit -m "feat(json_viewer): ⧉ per-level copy icon

Non-empty collapsible containers show a trailing ⧉ icon; clicking copies
the subtree as pretty-printed JSON via pbcopy/xclip. New LogsDetailJsonAction
/ NetworkDetailJsonAction ClickRegion variants dispatch typed JsonActions
through the existing two-phase detect/apply. Width budget in summaries
reserves 2 additional columns for the icon."
```

---

## Task 3: URL detection + click/keyboard open

Detect `http(s)://` inside leaf string values; render as underlined LAVENDER; register `OpenUrl` hot region carrying the full URL even when display-truncated.

**Files:**
- Modify: `src/ui/json_viewer/render/lines.rs`
- Modify: `src/event/actions.rs`
- Create: `src/event/url_open.rs` (thin wrapper kept separate for tests)

- [ ] **Step 3.1: Write failing test — URL renders with underline**

Add to `src/ui/json_viewer/render/mod.rs` tests:

```rust
#[test]
fn url_in_string_is_underlined_and_registered() {
    use crate::ui::json_viewer::JsonAction;
    use ratatui::style::Modifier;
    let t = tree::parse(r#"{"url": "https://example.com/path"}"#).unwrap();
    let s = state::init_state(&t, 1); // root expanded
    let mut out = Vec::new();
    let mut cmap = Vec::new();
    append_render(&mut out, &mut cmap, &t, &s, "sec", "", 80);
    // Line 1 = the leaf "url": "https://..." row.
    let underlined = out[1].spans.iter().any(|s|
        s.content.as_ref().contains("example.com")
        && s.style.add_modifier.contains(Modifier::UNDERLINED));
    assert!(underlined, "URL span not underlined: {:?}",
        out[1].spans.iter().map(|s| (s.content.as_ref().to_string(), s.style)).collect::<Vec<_>>());
    assert!(cmap[1].iter().any(|r| matches!(&r.action, JsonAction::OpenUrl(u) if u == "https://example.com/path")));
}

#[test]
fn truncated_url_carries_full_url_in_action() {
    use crate::ui::json_viewer::JsonAction;
    let t = tree::parse(r#"{"url": "https://example.com/very/long/path/that/exceeds/the/width"}"#).unwrap();
    let s = state::init_state(&t, 1);
    let mut out = Vec::new();
    let mut cmap = Vec::new();
    append_render(&mut out, &mut cmap, &t, &s, "sec", "", 40); // narrow
    let full = cmap[1].iter().find_map(|r| match &r.action {
        JsonAction::OpenUrl(u) => Some(u.clone()),
        _ => None,
    }).expect("OpenUrl should be registered");
    assert_eq!(full, "https://example.com/very/long/path/that/exceeds/the/width");
}
```

- [ ] **Step 3.2: Run test to verify failure**

Run: `cargo test --lib json_viewer::render::tests::url_in_string_is_underlined_and_registered`
Expected: FAIL.

- [ ] **Step 3.3: Add regex + URL splitter in `render_leaf_value`**

At the top of `src/ui/json_viewer/render/lines.rs`:

```rust
use std::sync::OnceLock;
use regex::Regex;
use ratatui::style::Modifier;

use crate::ui::LAVENDER;

fn url_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"https?://[^\s"'<>]+"#).unwrap())
}
```

Extract the current String branch into a helper that returns spans + optional full URL + optional truncation flag:

```rust
/// Render a string leaf. Returns (spans, maybe_full_url, truncated).
/// - maybe_full_url: the FIRST http(s) URL in the string (never truncated).
/// - truncated: true if the display text was shortened with `…`.
fn render_string_leaf_inner(s_raw: &str, max_width: usize) -> (Vec<Span<'static>>, Option<String>, bool) {
    let s_safe = sanitize_for_cell(s_raw);
    let full_quoted_w = format!("\"{}\"", s_safe).width();
    let truncated = full_quoted_w > max_width && max_width >= 3;

    // Determine display text (possibly truncated).
    let display: String = if truncated {
        let budget = max_width.saturating_sub(3);
        let mut w = 0usize;
        let mut cut = 0usize;
        for (i, ch) in s_safe.char_indices() {
            let cw = ch.to_string().as_str().width();
            if w + cw > budget { break; }
            w += cw;
            cut = i + ch.len_utf8();
        }
        format!("\"{}…\"", &s_safe[..cut])
    } else {
        format!("\"{}\"", s_safe)
    };

    // Locate the FIRST URL match in the ORIGINAL sanitized string.
    let url_match = url_re().find(&s_safe);
    let full_url = url_match.map(|m| m.as_str().to_string());

    if full_url.is_none() {
        return (
            vec![Span::styled(display, Style::default().fg(STR_COLOR))],
            None,
            truncated,
        );
    }

    // Split display into [prefix][url-part][suffix]. Work on the display
    // string, find whatever portion of the URL is visible.
    let full_url_str = full_url.as_ref().unwrap();
    let display_match_start = display.find(full_url_str)
        .or_else(|| {
            // URL truncated by `…`; find the leading prefix of the URL.
            for take in (3..=full_url_str.len()).rev() {
                if !full_url_str.is_char_boundary(take) { continue; }
                let head = &full_url_str[..take];
                if display.contains(head) {
                    return display.find(head);
                }
            }
            None
        });

    let Some(url_start) = display_match_start else {
        // Fallback: no visible URL portion — just render plain string.
        return (
            vec![Span::styled(display, Style::default().fg(STR_COLOR))],
            Some(full_url_str.clone()),
            truncated,
        );
    };

    // Measure visible URL length in the display string.
    let rest = &display[url_start..];
    let url_visible_end = if let Some(idx) = rest.find(full_url_str) {
        url_start + idx + full_url_str.len()
    } else {
        // Truncated: URL visible portion ends at the `…` just before closing `"`.
        display.rfind('…').unwrap_or(display.len() - 1)
    };

    let prefix = &display[..url_start];
    let url_part = &display[url_start..url_visible_end];
    let suffix = &display[url_visible_end..];

    let mut spans = Vec::new();
    if !prefix.is_empty() {
        spans.push(Span::styled(prefix.to_string(), Style::default().fg(STR_COLOR)));
    }
    spans.push(Span::styled(
        url_part.to_string(),
        Style::default().fg(LAVENDER).add_modifier(Modifier::UNDERLINED),
    ));
    if !suffix.is_empty() {
        spans.push(Span::styled(suffix.to_string(), Style::default().fg(STR_COLOR)));
    }

    (spans, Some(full_url_str.clone()), truncated)
}
```

Replace the existing `NodeKind::String` branch inside `render_leaf_value` with a wrapper that calls `render_string_leaf_inner` and returns the spans (discarding `full_url` and `truncated` — those are consumed by `push_leaf_line` directly).

Update the leaf signatures so `push_leaf_line` can register URL/Expand hot regions:

```rust
fn push_leaf_line(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Vec<JsonHotRegion>>,
    tree: &Tree,
    _section_key: &str,
    outer_prefix: &str,
    max_width: usize,
    id: u32,
    trailing_comma: bool,
) {
    // ... existing prefix + key spans code ...

    let used: usize = spans.iter().map(|s| s.content.as_ref().width()).sum();
    let remaining = max_width
        .saturating_sub(used)
        .saturating_sub(if trailing_comma { 1 } else { 0 });

    let (value_spans, maybe_url, truncated) = if let NodeKind::String(s) = &node.kind {
        render_string_leaf_inner(s, remaining)
    } else {
        (render_leaf_value(&node.kind, remaining), None, false)
    };

    let prefix_w: u16 = spans.iter().map(|s| s.content.as_ref().width() as u16).sum();
    let value_w: u16 = value_spans.iter().map(|s| s.content.as_ref().width() as u16).sum();
    spans.extend(value_spans);

    if trailing_comma {
        spans.push(Span::styled(",", Style::default().fg(COMMA_COLOR)));
    }

    out.push(Line::from(spans));

    // Build hot regions for this leaf row.
    let mut regions: Vec<JsonHotRegion> = Vec::new();
    let value_start = prefix_w;
    let value_end = prefix_w + value_w;

    if let Some(full) = maybe_url {
        // We don't know the precise underlined column within the value span
        // cheaply. For v1, register OpenUrl over the whole value span; any
        // click inside the quoted string opens the URL. Refine later if needed.
        regions.push(JsonHotRegion {
            range: value_start..value_end,
            action: JsonAction::OpenUrl(full),
        });
    } else if truncated && matches!(node.kind, NodeKind::String(_)) {
        regions.push(JsonHotRegion {
            range: value_start..value_end,
            action: JsonAction::ExpandFullValue(id),
        });
    }

    click_map.push(regions);
}
```

Remove the old NodeKind::String branch from `render_leaf_value` (it's now in the helper).

**Note:** The "register over whole value span" simplification in this task is a deliberate v1 choice. Precise per-span column ranges (prefix | URL-only | suffix) can come later if users find mis-clicks confusing; v1 prioritizes correctness of the action payload.

- [ ] **Step 3.4: Add `open_url` helper**

Create `src/event/url_open.rs`:

```rust
//! Cross-platform URL opener. Extracted from `actions.rs` to keep the
//! `cfg!(target_os = ...)` noise isolated and swappable in tests.

pub(super) fn open_url(url: &str) -> String {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return "Open failed (only http/https allowed)".into();
    }
    let result = if cfg!(target_os = "windows") {
        std::process::Command::new("cmd")
            .args(["/c", "start", "", url])
            .spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };
    match result {
        Ok(_) => format!("Opening {}", url),
        Err(_) => "Open failed (no opener)".into(),
    }
}
```

Register the module in `src/event/mod.rs`:

```rust
mod url_open;
```

And in `src/event/actions.rs`, add a re-export shim:

```rust
pub(super) fn open_url(url: &str) -> String {
    super::url_open::open_url(url)
}
```

- [ ] **Step 3.5: Wire `OpenUrl` into `apply_json_action`**

In `src/event/apply.rs`, update the `apply_json_action` helper:

```rust
JsonAction::OpenUrl(url) => {
    let msg = super::actions::open_url(&url);
    app.show_status(msg);
}
```

- [ ] **Step 3.6: Run tests**

Run: `cargo test --lib json_viewer::render`
Expected: PASS for `url_in_string_is_underlined_and_registered` and `truncated_url_carries_full_url_in_action`.

Run: `cargo build`
Expected: clean.

- [ ] **Step 3.7: Commit**

```bash
git add src/ui/json_viewer/render/lines.rs src/event
git commit -m "feat(json_viewer): clickable URLs in JSON string values

http(s) URLs in leaf string values render with UNDERLINED + LAVENDER and
register an OpenUrl hot region over the whole quoted-string span. The
OpenUrl action carries the full URL even when the displayed text is
truncated with … — clicking a truncated URL still opens the full address.
open_url dispatches via pbcopy/xdg-open/cmd start based on target_os."
```

---

## Task 4: `viewer_cursor` + keyboard navigation

Add a row cursor local to the detail panel. `J`/`K` (Shift+j/k) move it; `Enter` triggers the row's first-priority action; `o` opens URL on cursor row; `y` copies subtree on cursor row. Renderer draws a `SURFACE0` background on the cursor row.

j/k remain bound to list navigation (logs and network lists) — we use `J`/`K` to avoid conflicts.

**Files:**
- Modify: `src/app/state_structs.rs`
- Modify: `src/app/detail.rs`
- Modify: `src/ui/logs/detail/mod.rs` (and network equivalent)
- Modify: `src/event/keys.rs`
- Modify: `src/event/actions.rs`
- Modify: `src/ui/help/content/logs.rs`, `src/ui/help/content/network.rs`

- [ ] **Step 4.1: Write failing test — cursor up/down clamp**

Add to `tests/characterization_ui_047_json_interactive.rs`:

```rust
#[test]
fn viewer_cursor_j_k_clamp() {
    use flog::ui::json_viewer::{JsonAction, JsonHotRegion};
    let mut app = App::new(Default::default());
    app.detail.viewer_click_map = vec![
        vec![JsonHotRegion { range: 0..10, action: JsonAction::ToggleFold(0) }],
        vec![JsonHotRegion { range: 0..10, action: JsonAction::ToggleFold(1) }],
        vec![JsonHotRegion { range: 0..10, action: JsonAction::ToggleFold(2) }],
    ];
    assert_eq!(app.detail.viewer_cursor, None);
    app.detail_cursor_down();
    assert_eq!(app.detail.viewer_cursor, Some(0));
    app.detail_cursor_down();
    app.detail_cursor_down();
    app.detail_cursor_down(); // clamp at 2
    assert_eq!(app.detail.viewer_cursor, Some(2));
    app.detail_cursor_up();
    assert_eq!(app.detail.viewer_cursor, Some(1));
}
```

- [ ] **Step 4.2: Add cursor field + helpers**

Modify `src/app/state_structs.rs` — add to `DetailState`:

```rust
/// Interactive row cursor within the JSON viewer. `None` = inactive
/// (no highlight drawn). Set to `Some(0)` on first J/K press.
pub viewer_cursor: Option<usize>,
```

Modify `src/app/detail.rs` — add:

```rust
impl App {
    pub fn detail_cursor_down(&mut self) {
        let max = self.detail.viewer_click_map.len().saturating_sub(1);
        self.detail.viewer_cursor = Some(match self.detail.viewer_cursor {
            None => 0,
            Some(i) => (i + 1).min(max),
        });
    }

    pub fn detail_cursor_up(&mut self) {
        self.detail.viewer_cursor = Some(match self.detail.viewer_cursor {
            None => 0,
            Some(i) => i.saturating_sub(1),
        });
    }
}
```

Also extend `reset_detail_for_selection`:

```rust
pub fn reset_detail_for_selection(&mut self) {
    self.detail.scroll = 0;
    self.detail.viewer_state = crate::ui::json_viewer::JsonViewerState::default();
    self.detail.viewer_tree = None;
    self.detail.viewer_click_map.clear();
    self.detail.viewer_cursor = None;
}
```

- [ ] **Step 4.3: Run test from 4.1**

Run: `cargo test --test characterization_ui_047_json_interactive viewer_cursor_j_k_clamp`
Expected: PASS.

- [ ] **Step 4.4: Keyboard bindings J/K/Enter/o/y in detail-open Normal mode**

In `src/event/keys.rs`'s `handle_normal_key`, find the Logs/Network blocks. For **Logs** (after the Network block ends, in the `KeyCode::Char(...)` arms for Logs tab), add:

```rust
// JSON viewer interactive keys. Only active when detail panel is open.
KeyCode::Char('J') if app.show_detail_panel => {
    app.detail_cursor_down();
}
KeyCode::Char('K') if app.show_detail_panel => {
    app.detail_cursor_up();
}
KeyCode::Enter if app.show_detail_panel && app.detail.viewer_cursor.is_some() => {
    if let Some(action) = pick_cursor_action(&app.detail.viewer_click_map, app.detail.viewer_cursor) {
        super::apply::apply_json_action_public(app, action);
    }
}
KeyCode::Char('o') if app.show_detail_panel => {
    if let Some(url) = pick_cursor_url(&app.detail.viewer_click_map, app.detail.viewer_cursor) {
        let msg = super::actions::open_url(&url);
        app.show_status(msg);
    } else {
        app.show_status("No URL on this line".to_string());
    }
}
KeyCode::Char('y') if app.show_detail_panel => {
    if let Some(node_id) = pick_cursor_copy_node(&app.detail.viewer_click_map, app.detail.viewer_cursor) {
        let text = super::actions::extract_node_json(&app.detail.viewer_tree, node_id);
        let msg = super::actions::copy_to_clipboard(&text);
        app.show_status(msg);
    }
}
```

Repeat for **Network** tab inside its `match key.code` (after the existing arms).

Add the three helper functions at the bottom of `keys.rs`:

```rust
fn pick_cursor_action(
    click_map: &[Vec<crate::ui::json_viewer::JsonHotRegion>],
    cursor: Option<usize>,
) -> Option<crate::ui::json_viewer::JsonAction> {
    use crate::ui::json_viewer::JsonAction;
    let row = click_map.get(cursor?)?;
    // Precedence: ExpandFullValue > OpenUrl > CopyNode > ToggleFold.
    for priority in [
        |a: &JsonAction| matches!(a, JsonAction::ExpandFullValue(_)),
        |a: &JsonAction| matches!(a, JsonAction::OpenUrl(_)),
        |a: &JsonAction| matches!(a, JsonAction::CopyNode(_)),
        |a: &JsonAction| matches!(a, JsonAction::ToggleFold(_)),
    ] {
        if let Some(r) = row.iter().find(|r| priority(&r.action)) {
            return Some(r.action.clone());
        }
    }
    None
}

fn pick_cursor_url(
    click_map: &[Vec<crate::ui::json_viewer::JsonHotRegion>],
    cursor: Option<usize>,
) -> Option<String> {
    let row = click_map.get(cursor?)?;
    row.iter().find_map(|r| match &r.action {
        crate::ui::json_viewer::JsonAction::OpenUrl(u) => Some(u.clone()),
        _ => None,
    })
}

fn pick_cursor_copy_node(
    click_map: &[Vec<crate::ui::json_viewer::JsonHotRegion>],
    cursor: Option<usize>,
) -> Option<u32> {
    let row = click_map.get(cursor?)?;
    row.iter().find_map(|r| match &r.action {
        crate::ui::json_viewer::JsonAction::CopyNode(id) => Some(*id),
        _ => None,
    })
}
```

And in `src/event/apply.rs` expose the dispatch to sibling modules (`keys.rs`):

```rust
pub(super) fn apply_json_action_public(app: &mut App, action: crate::ui::json_viewer::JsonAction) {
    apply_json_action(app, action);
}
```

- [ ] **Step 4.5: Highlight cursor row in renderer**

In `src/ui/logs/detail/mod.rs`, after `all_lines.extend(visible)` but before the final block render, apply cursor highlight:

```rust
// viewer_cursor points into viewer_click_map, which is a body-only
// window. Map to all_lines by offsetting by header_lines.
if let Some(cursor_in_body) = app.detail.viewer_cursor {
    let target = app.detail.header_lines + cursor_in_body;
    if let Some(line) = all_lines.get_mut(target) {
        use ratatui::style::Style;
        use crate::ui::SURFACE0;
        let highlighted_spans: Vec<_> = line.spans.iter().cloned()
            .map(|mut s| { s.style = s.style.bg(SURFACE0); s })
            .collect();
        *line = ratatui::text::Line::from(highlighted_spans);
    }
}
```

Do the same in the network detail renderer at the equivalent location (grep for `viewer_click_map` in `src/ui/network/detail/` — set it where the body rows are assembled).

- [ ] **Step 4.6: Update help content**

Modify `src/ui/help/content/logs.rs` — add under the detail-panel keybinds section:

```rust
("J / K", "Move JSON viewer cursor"),
("Enter", "Trigger action on cursor row (expand / open / fold)"),
("o", "Open URL on cursor row"),
("y", "Copy subtree at cursor"),
```

Same additions in `src/ui/help/content/network.rs`.

- [ ] **Step 4.7: Run tests**

Run: `cargo test`
Expected: PASS.

- [ ] **Step 4.8: Commit**

```bash
git add src/app src/event/keys.rs src/event/apply.rs src/ui/logs/detail src/ui/network/detail src/ui/help/content
git commit -m "feat(json_viewer): row cursor + J/K/Enter/o/y keybindings

DetailState gains viewer_cursor: Option<usize>. Shift+J/K move it; Enter
triggers the row's first-priority action (Expand > OpenUrl > CopyNode >
ToggleFold); o opens URL on cursor row; y copies subtree. Cursor row is
highlighted with SURFACE0 bg. Small j/k remain bound to list navigation."
```

---

## Task 5: `FullValueOverlay` mode + `ExpandFullValue`

Add the `AppMode::FullValueOverlay` variant, the centered modal renderer, and wire up click-on-truncated-string + Enter-on-truncated-row to open the overlay.

**Files:**
- Modify: `src/app/mod.rs`
- Modify: `src/app/state_structs.rs` (FullValueOverlayState)
- Create: `src/ui/full_value_overlay.rs`
- Modify: `src/ui/mod.rs`
- Modify: `src/event/mod.rs`
- Modify: `src/event/keys.rs`
- Modify: `src/event/click_region.rs`
- Modify: `src/event/apply.rs`

- [ ] **Step 5.1: Write failing test — Enter opens overlay, Esc closes**

Add to `tests/characterization_ui_047_json_interactive.rs`:

```rust
#[test]
fn expand_full_value_enters_overlay() {
    use flog::app::AppMode;
    use flog::ui::json_viewer::{JsonAction, JsonHotRegion};

    let mut app = App::new(Default::default());
    // Simulate a detail panel with a truncated string leaf at row 0.
    app.show_detail_panel = true;
    // Parse a tiny tree so viewer_tree is populated.
    let tree = flog::ui::json_viewer::parse_for_test(r#""hello world""#);
    app.detail.viewer_tree = Some(tree);
    app.detail.viewer_click_map = vec![
        vec![JsonHotRegion {
            range: 0..u16::MAX,
            action: JsonAction::ExpandFullValue(0),
        }],
    ];
    app.detail.viewer_cursor = Some(0);

    // Press Enter.
    flog::event::test_helpers::press_enter(&mut app);
    assert!(matches!(app.mode, AppMode::FullValueOverlay(_)));

    // Press Esc.
    flog::event::test_helpers::press_esc(&mut app);
    assert!(matches!(app.mode, AppMode::Normal));
}
```

Add to `src/event/mod.rs` (no `#[cfg(test)]` — integration tests can only see items compiled into the normal lib):

```rust
#[doc(hidden)]
pub mod test_helpers {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    pub fn press_enter(app: &mut crate::app::App) {
        handle_key(app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    }

    pub fn press_esc(app: &mut crate::app::App) {
        handle_key(app, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    }
}
```

**Verify first:** grep for the top-level key entry in `src/event/mod.rs`. If it's named `handle_key`, the above is correct; if it's different (e.g., `handle_input`), substitute.

- [ ] **Step 5.2: Add `FullValueOverlayState` + `AppMode` variant**

In `src/app/state_structs.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullValueOverlayState {
    pub text: String,
    pub node_id: u32,
    pub scroll: usize,
}
```

Export it:

In `src/app/mod.rs`:

```rust
pub use state_structs::{
    DetailState, FullValueOverlayState, InputBuffers, LogsViewState, SearchState, StatsSnapshot,
};

// ...

#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    Normal,
    InputActive(InputField),
    Help,
    Stats,
    MockRuleEdit,
    FullValueOverlay(FullValueOverlayState),
}
```

Add the entry method on `impl App`:

```rust
pub fn enter_full_value_overlay(&mut self, text: String, node_id: u32) {
    self.mode = AppMode::FullValueOverlay(FullValueOverlayState {
        text,
        node_id,
        scroll: 0,
    });
}

pub fn exit_full_value_overlay(&mut self) {
    if matches!(self.mode, AppMode::FullValueOverlay(_)) {
        self.mode = AppMode::Normal;
    }
}
```

- [ ] **Step 5.3: Wire `ExpandFullValue` apply path**

Add to `src/event/actions.rs`:

```rust
/// Extract the raw string value of a leaf node.
pub(super) fn extract_node_string(
    tree: &Option<crate::ui::json_viewer::Tree>,
    node_id: u32,
) -> Option<String> {
    use crate::ui::json_viewer::NodeKind;
    let tree = tree.as_ref()?;
    match &tree.node(node_id).kind {
        NodeKind::String(s) => Some(s.clone()),
        _ => None,
    }
}
```

Update `apply_json_action` in `src/event/apply.rs`:

```rust
JsonAction::ExpandFullValue(id) => {
    if let Some(text) = super::actions::extract_node_string(&app.detail.viewer_tree, id) {
        app.enter_full_value_overlay(text, id);
    }
}
```

- [ ] **Step 5.4: Overlay renderer**

Create `src/ui/full_value_overlay.rs`:

```rust
//! Centered modal that shows the full text of a string value truncated
//! with `…` in the JSON viewer. Dismiss with Esc (no copy) or Enter /
//! click inside (copy + close).

use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::app::{AppMode, FullValueOverlayState};
use crate::ui::{LAVENDER, MANTLE, SURFACE0, TEXT};

pub fn render(f: &mut Frame, area: Rect, state: &FullValueOverlayState) {
    // Modal size: max 70% x 70%.
    let max_w = (area.width as f32 * 0.7) as u16;
    let max_h = (area.height as f32 * 0.7) as u16;
    let w = max_w.max(20).min(area.width.saturating_sub(2));
    let h = max_h.max(5).min(area.height.saturating_sub(2));
    let x = area.x + (area.width - w) / 2;
    let y = area.y + (area.height - h) / 2;
    let modal = Rect::new(x, y, w, h);

    f.render_widget(Clear, modal);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(LAVENDER))
        .style(Style::default().bg(MANTLE))
        .title(Span::styled(
            " Full value (Enter/click to copy, Esc to close) ",
            Style::default().fg(LAVENDER).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(modal);
    f.render_widget(block, modal);

    let p = Paragraph::new(state.text.clone())
        .style(Style::default().fg(TEXT).bg(SURFACE0))
        .wrap(Wrap { trim: false })
        .alignment(Alignment::Left)
        .scroll((state.scroll as u16, 0));
    f.render_widget(p, inner);
}

pub fn is_inside(area: Rect, x: u16, y: u16) -> bool {
    area.contains(ratatui::layout::Position { x, y })
}

/// Returns the modal rect for a given area — exported so detect can tell
/// inside-vs-outside without rendering.
pub fn modal_rect(area: Rect) -> Rect {
    let w = ((area.width as f32 * 0.7) as u16).max(20).min(area.width.saturating_sub(2));
    let h = ((area.height as f32 * 0.7) as u16).max(5).min(area.height.saturating_sub(2));
    let x = area.x + (area.width - w) / 2;
    let y = area.y + (area.height - h) / 2;
    Rect::new(x, y, w, h)
}
```

Register in `src/ui/mod.rs`:

```rust
pub mod full_value_overlay;
```

Then in the top-level UI dispatcher (the function that renders the whole frame), after all other layers are drawn:

```rust
if let AppMode::FullValueOverlay(ref state) = app.mode {
    full_value_overlay::render(f, f.size(), state);
}
```

Find the right render entry — probably in `src/run/render_loop.rs` or `src/ui/mod.rs::render`. Grep: `fn render(`.

- [ ] **Step 5.5: Overlay key handling**

In `src/event/mod.rs`'s `handle_key` dispatch:

```rust
match app.mode {
    AppMode::Normal => handle_normal_key(app, key),
    AppMode::InputActive(field) => ...,
    AppMode::Help => ...,
    AppMode::Stats => ...,
    AppMode::MockRuleEdit => ...,
    AppMode::FullValueOverlay(_) => handle_overlay_key(app, key),
}
```

Add handler in `src/event/keys.rs`:

```rust
pub(super) fn handle_overlay_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.exit_full_value_overlay(),
        KeyCode::Enter | KeyCode::Char('y') => {
            if let AppMode::FullValueOverlay(ref state) = app.mode {
                let text = state.text.clone();
                let msg = super::actions::copy_to_clipboard(&text);
                app.exit_full_value_overlay();
                app.show_status(msg);
            }
        }
        KeyCode::PageDown | KeyCode::Char('j') => {
            if let AppMode::FullValueOverlay(ref mut state) = app.mode {
                state.scroll = state.scroll.saturating_add(1);
            }
        }
        KeyCode::PageUp | KeyCode::Char('k') => {
            if let AppMode::FullValueOverlay(ref mut state) = app.mode {
                state.scroll = state.scroll.saturating_sub(1);
            }
        }
        _ => {}
    }
}
```

- [ ] **Step 5.6: Overlay mouse handling**

Add two `ClickRegion` variants in `src/event/click_region.rs`:

```rust
FullValueOverlayInside,
FullValueOverlayOutside,
```

**Step 5.6a — first add `term_size` to `LayoutCache`:**

Grep `src/app/layout_cache.rs` — if `term_size: (u16, u16)` (or equivalent `term_rect: Rect`) is not present, add:

```rust
pub term_size: (u16, u16),  // (width, height), set every frame by render loop
```

Initialize to `(0, 0)` in `Default`. Then in `src/run/render_loop.rs` (or wherever the frame render is dispatched), at the top of the render call:

```rust
app.layout.term_size = (f.size().width, f.size().height);
```

**Step 5.6b — then add detect branch:**

In `src/event/detect.rs`, at the top of the mouse detect function (before any normal region match), add:

```rust
if let AppMode::FullValueOverlay(_) = app.mode {
    let (w, h) = app.layout.term_size;
    let rect = crate::ui::full_value_overlay::modal_rect(ratatui::layout::Rect {
        x: 0, y: 0, width: w, height: h,
    });
    return Some(if rect.contains(ratatui::layout::Position { x, y }) {
        ClickRegion::FullValueOverlayInside
    } else {
        ClickRegion::FullValueOverlayOutside
    });
}
```

In `src/event/apply.rs`:

```rust
ClickRegion::FullValueOverlayInside => {
    if let AppMode::FullValueOverlay(ref state) = app.mode {
        let text = state.text.clone();
        let msg = super::actions::copy_to_clipboard(&text);
        app.exit_full_value_overlay();
        app.show_status(msg);
    }
}
ClickRegion::FullValueOverlayOutside => app.exit_full_value_overlay(),
```

- [ ] **Step 5.7: Run tests**

Run: `cargo test --test characterization_ui_047_json_interactive expand_full_value_enters_overlay`
Expected: PASS.

Run: `cargo build && cargo test`
Expected: PASS.

- [ ] **Step 5.8: Commit**

```bash
git add src/app src/ui/full_value_overlay.rs src/ui/mod.rs src/event tests/characterization_ui_047_json_interactive.rs
git commit -m "feat(json_viewer): FullValueOverlay for truncated strings

Long string values truncated with … can now be expanded: clicking the
value or pressing Enter on the cursor row opens a centered modal showing
the full text (70% max size, word-wrap). Esc closes; Enter/click-inside
copies to clipboard and closes; click-outside cancels. New
AppMode::FullValueOverlay variant + FullValueOverlayInside/Outside
ClickRegions."
```

---

## Task 6: Docs + help tests + final polish

**Files:**
- Modify: `docs/ARCHITECTURE.md`
- Modify: `docs/MODULES.md`
- Modify: `src/ui/help/content/logs.rs` (verify from Task 4.6)
- Modify: `src/ui/help/content/network.rs`

- [ ] **Step 6.1: Update `docs/ARCHITECTURE.md`**

Find the section describing the JSON viewer / click dispatch. Add a paragraph:

```markdown
### Interactive JSON actions (UI-047)

The JSON viewer's `click_map` is `Vec<Vec<JsonHotRegion>>` — each row
carries zero or more non-overlapping `(x_range, JsonAction)` segments.
`JsonAction` has four variants: `ToggleFold(id)`, `CopyNode(id)` (⧉
trailing icon), `OpenUrl(full_url)` (http/https in string values,
underlined), `ExpandFullValue(id)` (truncated string → centered modal).
Mouse detect looks up `(line, x)` in the row's regions, falling back
to the first `ToggleFold` on whitespace. Keyboard: `J`/`K` move a row
cursor, `Enter` triggers first-priority action, `o` opens URL, `y`
copies subtree, `Esc` closes the overlay.
```

- [ ] **Step 6.2: Update `docs/MODULES.md`**

Add `src/ui/full_value_overlay.rs` row and mention `json_viewer::action` submodule under the json_viewer entry.

- [ ] **Step 6.3: Run full suite**

Run: `cargo build && cargo test && cargo clippy`
Expected: all clean.

Run: `cargo fmt --check`
Expected: no changes needed (or run `cargo fmt` first).

- [ ] **Step 6.4: Smoke test UI manually**

Run: `cargo run`

With a connected Flutter app, in the TUI:
1. Select a log entry whose body contains a JSON object — ⧉ should show on collapsible rows, click copies.
2. Select a network request with a response containing `"url": "https://..."` — URL should be underlined LAVENDER, click opens browser.
3. Find a long string value that truncates to `…` — click it or press Enter on the cursor row — overlay opens with full text; Esc closes, Enter copies.
4. Press `J`/`K` in the detail panel — cursor row highlight moves.
5. Press `?` — help shows new keybindings.

Note: manual verification is expected to find issues that tests missed. Any fix here is a follow-up commit, not a task failure.

- [ ] **Step 6.5: Commit**

```bash
git add docs/ARCHITECTURE.md docs/MODULES.md
git commit -m "docs: UI-047 interactive JSON viewer (⧉ copy / URLs / expand)"
```

---

## Done criteria

- All six tasks merged.
- `cargo test` passes.
- `cargo clippy` clean.
- Manual smoke test of all three features passes on at least one platform (macOS).
- Help overlay shows new keybindings.
- `docs/ARCHITECTURE.md` + `docs/MODULES.md` describe the new shape.
