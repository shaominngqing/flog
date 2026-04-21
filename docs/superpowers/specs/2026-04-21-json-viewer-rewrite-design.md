# JSON Viewer Rewrite — AST-Based Tree Viewer

**Date**: 2026-04-21
**Status**: Design approved, ready for implementation plan.

## Problem

The current `src/ui/json_viewer.rs` is a "half-parser": it string-reformats raw text by tracking brace/bracket depth and re-emits lines with `"  ".repeat(depth)` prefixes. Depth is then **reverse-engineered** from the leading whitespace of each emitted line. Click handling maps rendered rows back to source-line indices via a `row_to_source: Vec<usize>` table built during render.

This produces two visible bugs:

1. **Indentation misalignment** — array elements, folded placeholders, and sibling `{…}` nodes drift out of alignment when a line contains multiple structural tokens (e.g. `}, { "id": 929, …`). The flush logic splits by comma but the depth-from-text-width heuristic then disagrees with the logical nesting.
2. **Flaky clicks** — long string values are wrapped via `wrap_text`, and every wrap continuation line is pushed into `row_to_source` with the same source index. Clicking a continuation row triggers fold. Further, the click map stores only `(section_key, source_line)` with no column range, so any click anywhere on the line toggles (including accidental clicks on values).

## Root cause

The viewer is a **text transform**, not a tree. Depth, foldability, and click targets are all derived from rendered-text bookkeeping, which drifts from the actual JSON structure under edge cases.

## Design

Replace the text-transform viewer with an AST-based viewer: parse JSON into a flat arena tree, keep fold state as `Vec<bool>` indexed by node ID, render the tree directly. Every node has stable identity; indentation derives from logical depth; click hit-testing maps rendered rows to node IDs with no continuation-line aliasing because long values are **truncated with `…`, not wrapped**.

### Scope — what this replaces, what stays

**Replaced** (deleted from `src/ui/json_viewer.rs`):

- `FmtLine`, `bracket_format`, `init_state`, `toggle_fold`, `render_json`, `indent_brackets`, `flush`, `colorize_line_depth`, `colorize_content_depth`, `colorize_value`
- Old `JsonViewerState` fields: `collapsed`, `foldable`, `row_to_source`, `total_lines`

**Kept** (independent, different use case):

- `colorize_json_text` — raw-text JSON syntax highlighter used by `logs/detail.rs` for inline JSON in log messages. Moves to `ui/json_viewer/colorize.rs`, behavior unchanged.
- Color palette constants (`DEPTH_COLORS`, `DEPTH_BRACE`, `key_color`, `brace_color`, `STR_COLOR`, `NUM_COLOR`, `BOOL_COLOR`, `NULL_COLOR`, `COMMA_COLOR`, `FOLD_COLOR`). The new renderer reuses them.

### Module layout

`src/ui/json_viewer.rs` (single file) → `src/ui/json_viewer/` (module directory):

```
src/ui/json_viewer/
├── mod.rs       — public API: parse, init_state, toggle, expand_all,
│                  collapse_all, append_render. Re-exports colorize_json_text.
├── tree.rs      — FlatNode, Tree, parse()
├── state.rs     — FoldState + state mutations
├── render.rs    — Rendering + summarize + color application
└── colorize.rs  — Unchanged, moved from the old file
```

Each file stays ≤ ~300 lines with a single responsibility.

### Data structures

**Flat arena tree** (`tree.rs`):

```rust
#[derive(Clone, Debug)]
pub enum NodeKind {
    Null,
    Bool(bool),
    Number(String),     // keep original string form; avoid f64 lossy round-trip
    String(String),
    Object,             // children are key-value entries
    Array,              // children are positional entries
}

#[derive(Clone, Debug)]
pub struct FlatNode {
    pub kind: NodeKind,
    pub depth: u32,
    pub parent: Option<u32>,
    /// For Object/Array: child node IDs in order. Empty for leaves.
    pub children: Vec<u32>,
    /// Set on object entries (the key for this value). None for array entries
    /// and the root.
    pub key: Option<String>,
}

pub struct Tree {
    pub nodes: Vec<FlatNode>,   // root = nodes[0]
}

pub fn parse(text: &str) -> Result<Tree, serde_json::Error>;
```

Parsing: `serde_json::from_str::<serde_json::Value>(text)` then DFS-flatten into `nodes`. We don't roll our own parser — serde_json handles every JSON edge case and we reuse the exact same lenient number/string representation we already have.

**Fold state** (`state.rs`):

```rust
pub struct FoldState {
    /// expanded[node_id] = true iff that container is expanded.
    /// Length == tree.nodes.len(). Leaves are always false (unused).
    pub expanded: Vec<bool>,
}

pub fn init(tree: &Tree, default_expand_depth: u32) -> FoldState;
pub fn toggle(state: &mut FoldState, node_id: u32);
pub fn expand_all(tree: &Tree, state: &mut FoldState);
pub fn collapse_all(tree: &Tree, state: &mut FoldState);
```

`init` expands every container with `depth <= default_expand_depth`. Default depth for detail panels = `1` (root expanded, immediate child containers expanded, grandchildren folded). Matches DevTools' default behavior.

### Rendering

```rust
pub fn append_render(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Option<(String, u32)>>,
    tree: &Tree,
    state: &FoldState,
    section_key: &str,
    indent_prefix: &str,    // outer indent applied by caller (e.g. "   ")
    max_width: usize,
);
```

Each rendered line has the fixed structure:

```
<indent_prefix><indent><marker><content>
```

- `indent` = `"  ".repeat(node.depth)` — 2 spaces per level, derived from **the actual tree depth**, not from text width.
- `marker` = 2 characters, fixed width, so keys always align:
  - Container (foldable): `▼ ` when expanded, `▶ ` when collapsed.
  - Leaf: `  ` (two spaces).
- `content` layout by node kind:

**Leaf (object entry or array item)**
```
"key": value,                 # object entry
value,                        # array item
```
Values colorized with existing palette (`STR_COLOR`, `NUM_COLOR`, `BOOL_COLOR`, `NULL_COLOR`). Trailing comma appended when the node is not the last sibling.

**Container, collapsed**
```
"key": {code: 0, message: "ok", …},
"key": [1, 2, 3, …] (12),
```
Summary comes from `summarize_container(tree, node_id, remaining_width) -> Vec<Span>`:
- Object: `{k1: v1, k2: v2, …}` — emit up to N entries until width is exhausted; string values styled `STR_COLOR`, numbers `NUM_COLOR`, keys with `key_color(depth)`. Nested containers render as `{…}` or `[…]`.
- Array: `[e1, e2, e3, …] (<length>)` — same rule for elements; always append `(<length>)` in `OVERLAY0`.
- Braces in the summary use `brace_color(depth)`.

**Container, expanded**

One line for the opener:
```
"key": {
```
(or `[` for arrays; root has no key). Then children render recursively, each on its own line at `depth + 1`. Finally one closing line:
```
},
```
(with trailing comma iff the container itself is not the last sibling). Marker on the closing line is `  ` (two spaces — the closing brace is not independently foldable).

**Long strings**

If the rendered line width exceeds `max_width`, string values (and only string values) are truncated with `…`. Example: `"Business Meetings in Eng…"`. Non-string content (numbers, booleans, nested summary) is never truncated — those never exceed a reasonable width in practice. Keys are never truncated either (they're always short JSON keys); if a key somehow overflows, the line overflows — we prefer structural integrity over width.

No word-wrap. No continuation lines. Every rendered line corresponds to exactly one node.

### Click hit-testing

Rendering pushes one entry into `click_map` per line. For container opener lines and collapsed-container lines: `Some((section_key, node_id))`. For leaf lines, closing-brace lines, and every line outside JSON viewer output: `None`.

The existing event handler in `src/event.rs` (line ~413) already does exactly the right lookup — we just change `toggle_fold(state, source_line)` to `json_viewer::toggle(state, node_id)`.

Type migration: `app::NetworkState::detail_json_click_map: Vec<Option<(String, usize)>>` → `Vec<Option<(String, u32)>>`.

### Keyboard: E / C

New event handlers in `src/event.rs` when focus is on the Network detail panel:

- `E` (uppercase, Shift+e): call `json_viewer::expand_all` for **every** `JsonViewerState` in `app.network.json_viewer_states` (there's one per section: `req_body`, `res_body`, `req_headers`, etc.).
- `C` (uppercase, Shift+c): same, but `collapse_all` (leaves root expanded to stay useful).

Uppercase is chosen to avoid collision with lowercase letters used by other handlers. Help overlay (`src/ui/help.rs`) gets two new lines.

### Caller integration (`src/ui/network/detail.rs`)

`render_json_section` becomes:

```rust
fn render_json_section(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    click_map: &mut Vec<Option<(String, u32)>>,
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
                lines, click_map, &tree, state, section_key,
                "   ", max_w.saturating_sub(3),
            );
            // section_map keeps pace with lines — fill None for every line added.
            for _ in base..lines.len() { section_map.push(None); }
        }
        Err(_) => {
            // Fallback: plain wrapped text for non-JSON bodies.
            for wl in wrap_text(json_text, max_w.saturating_sub(3), 100) {
                lines.push(Line::from(Span::styled(
                    format!("   {}", wl),
                    Style::default().fg(SUBTEXT0),
                )));
                section_map.push(None);
                click_map.push(None);
            }
        }
    }
}
```

`JsonViewerState` in `app.rs` stays as a named type; its internals change (now wraps `FoldState` + the `Tree` is rebuilt per render from the source text — we don't cache the tree because the tree is cheap to parse and network bodies are already in memory).

**Parse cost reasoning**: a typical response body is ≤ 100 KB; `serde_json::from_str` runs in microseconds. We re-parse on every frame only when the detail panel is visible for the currently selected entry. No performance concern. If profiling proves otherwise, cache `(source_text_hash, Tree)` on `JsonViewerState`.

### Color rules (recap)

Reused verbatim from the existing palette:

| Element | Color | Source |
|---|---|---|
| Object key | `key_color(depth)` (cycling MAUVE/BLUE/TEAL/YELLOW/SAPPHIRE/LAVENDER) | existing |
| Brace `{}[]` | `brace_color(depth)` (depth-dimming gray) | existing |
| String value | `STR_COLOR` = GREEN | existing |
| Number | `NUM_COLOR` = PEACH | existing |
| Bool | `BOOL_COLOR` = PINK | existing |
| Null | `NULL_COLOR` = OVERLAY0, italic | existing |
| Comma, colon | `COMMA_COLOR` = SURFACE0 | existing |
| Fold marker `▶`/`▼` | BLUE | existing |
| Summary count `(12)` | `OVERLAY0` | existing |

No visual changes for well-rendered cases — users only see the bugs go away.

### Edge cases

- **Empty object / array** `{}` / `[]`: rendered as a single leaf-looking line, marker = `  ` (not foldable), no click target. Consistent with DevTools.
- **Deep nesting** (> 50): render a `"(depth limit)"` placeholder at the cutoff. Rare in practice; protects against pathological inputs.
- **Non-JSON text**: `parse` returns `Err`; caller falls back to `wrap_text` as today.
- **Numeric precision**: keep serde_json's string form via `Value::to_string()` per node. No float precision loss.
- **Unicode in keys / strings**: width via `unicode_width::UnicodeWidthStr::width`, same as existing code.

### What this does NOT include (explicitly out of scope)

- Independent scroll for the JSON panel (still shares detail-panel scroll).
- Keyboard `h` / `l` / cursor-based node selection — no "currently-selected node" concept added.
- Click-to-expand-value for truncated strings (future work).
- Search within JSON tree.
- Copying a node's value / path (future work).

## Verification plan

- Open the screenshot's response body, confirm array element indentation is perfectly aligned, `{…}` placeholders line up with sibling object openers.
- Click `▶` on a folded node — expands. Click `▼` — folds. Click on a folded placeholder's text content — expands (whole foldable line is hot). Click on a leaf line — no-op.
- Click on a long truncated string — no-op (leaf). No accidental folds.
- Keyboard `E` / `C` — all sections expand / collapse.
- Large response (200+ keys, 5+ deep nesting) — no perf regression vs current.
- Non-JSON request/response body (HTML, plain text) — falls back gracefully.
- `logs/detail.rs` still compiles and renders inline JSON via the preserved `colorize_json_text`.

## Files changed

- `src/ui/json_viewer.rs` — DELETED (becomes directory)
- `src/ui/json_viewer/mod.rs` — NEW (public API)
- `src/ui/json_viewer/tree.rs` — NEW
- `src/ui/json_viewer/state.rs` — NEW
- `src/ui/json_viewer/render.rs` — NEW
- `src/ui/json_viewer/colorize.rs` — NEW (content moved from old file, unchanged)
- `src/ui/network/detail.rs` — `render_json_section` simplified; `json_click_map` type changed
- `src/ui/logs/detail.rs` — if it uses the old `JsonViewerState` / `bracket_format`, migrate to the new API
- `src/app.rs` — `NetworkState::detail_json_click_map` element type from `usize` → `u32`; `DetailState::viewer_state` field type updated if used by logs
- `src/event.rs` — click dispatch switches to `json_viewer::toggle(state, node_id)`; add `E`/`C` handlers
- `src/ui/help.rs` — add help lines for `E`/`C`
- `src/ui/mod.rs` — `pub mod json_viewer;` unchanged (still a module reference, directory vs file transparent)
