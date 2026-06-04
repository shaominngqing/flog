# JSON Viewer Interactive Enhancements — Design

Date: 2026-05-12
Status: Draft (awaiting review)

## Goal

Three incremental UX upgrades to the shared `ui/json_viewer` component (used by both Logs detail and Network detail):

1. **Expand long values on demand** — strings truncated with `…` can be expanded to see the full content via Enter or click.
2. **Per-level copy button** — every collapsible container row gets a `⧉` icon that copies the subtree as pretty JSON.
3. **Clickable URLs** — JSON string values that are `http(s)://` URLs are underlined + colored and open in the system browser on click; a keyboard `o` shortcut also opens the URL under the viewer cursor.

These additions must preserve the module's current invariants (UI-agnostic `domain/`, two-phase detect/apply per CLAUDE.md §3, renderer-as-scroll-authority).

## Non-goals

- Multiple-URL-per-string detection (first URL only in v1).
- URL opening for `ws://`, `wss://`, `file://`, or other schemes (security-conservative).
- Search / selection inside the full-value overlay.
- Mouse wheel scrolling inside the overlay (PgUp/PgDn + j/k only).
- Cursor persistence across different detail entries.

## Approach summary

Approach B (selected over A: minimal hack, and C: full component abstraction). Extend the existing `click_map` semantics from "one action per row" to "N `(x_range, action)` segments per row". Add a `JsonAction` enum for the new action types, route through existing `ClickRegion` two-phase dispatch, add a `FullValueOverlay` app mode, and a lightweight row cursor (`viewer_cursor`) scoped to detail panels.

## §1 Data model

### `click_map` type change

```rust
// src/app/state_structs.rs — before
pub viewer_click_map: Vec<Option<u32>>;  // one foldable node per row

// after
pub struct JsonHotRegion {
    pub range: std::ops::Range<u16>,  // column span within the line
    pub action: JsonAction,
}
pub enum JsonAction {
    ToggleFold(u32),         // collapsible ▼/▶ marker or whole-row whitespace
    CopyNode(u32),           // ⧉ icon
    OpenUrl(String),         // http(s)://… span (full URL, even when display-truncated)
    ExpandFullValue(u32),    // truncated leaf string span
}
pub viewer_click_map: Vec<Vec<JsonHotRegion>>;
```

Invariants:
- Regions within a row are sorted by `range.start` ascending and **do not overlap**.
- Row whitespace falls back to the row's first `ToggleFold` region (if any).

### `append_render` signature change

```rust
pub fn append_render(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Vec<JsonHotRegion>>,  // was Vec<Option<(String, u32)>>
    tree: &Tree,
    state: &JsonViewerState,
    section_key: &str,
    outer_prefix: &str,
    max_width: usize,
);
```

The old `section_key` parameter is kept for caller symmetry but is **no longer embedded** in click_map entries — there is always exactly one JSON viewer per detail panel, and outer section folding is tracked separately via `NetworkDetailSectionToggle`. The parameter becomes informational only and may be removed in a later cleanup.

### New `ClickRegion` variants

```rust
// src/event/click_region.rs
LogsDetailJsonAction(JsonAction),
NetworkDetailJsonAction(JsonAction),
```

### New `AppMode` variant

```rust
// src/app/mod.rs
pub enum AppMode {
    Normal, InputActive(InputField), Help, Stats, MockRuleEdit,
    FullValueOverlay(FullValueOverlayState),
}

pub struct FullValueOverlayState {
    pub text: String,    // sanitized full string value
    pub node_id: u32,    // source node (for `y` reuse)
    pub scroll: usize,   // overlay scroll offset (rows)
}
```

### New cursor field

```rust
// src/app/state_structs.rs (DetailState)
pub viewer_cursor: Option<usize>,  // row index into viewer_click_map
```

Scoped to detail panels only. `None` = inactive (no highlight). Activated by any of j/k/Enter/o/y, reset to `None` on detail close or entry switch.

## §2 Rendering

### §2.1 `⧉` copy icon per collapsible row

- Added to **non-empty** containers only (both `▼` opener rows and `▶` collapsed-summary rows).
- Empty containers (`{}`, `[]`) and leaf rows do not get an icon.
- Appended as `Span::styled(" ⧉", Style::default().fg(OVERLAY0))` (2 display columns: leading space + glyph) at the **end** of the row.
- `summarize_container`'s `reserved` budget increases by 2 to account for the trailing ` ⧉`.
- click_map: appends `JsonHotRegion { range: (line_width-2)..line_width, action: CopyNode(node_id) }`.

### §2.2 URL detection & underline

In `render_leaf_value`'s `NodeKind::String` branch:
- Module-level `OnceLock<Regex>` with pattern `r"https?://[^\s\"'<>]+"`.
- If the sanitized string contains a URL match, split the string value into multiple `Span`s:
  - URL span: `Style::default().fg(LAVENDER).add_modifier(Modifier::UNDERLINED)`
  - Non-URL parts: current `STR_COLOR`.
- click_map: append `JsonHotRegion { range: url_x_start..url_x_end, action: OpenUrl(full_url) }`. **Even if the rendered URL is truncated** to `"https://x.com/very/lo…"`, the action carries the **full** URL string.
- Only the **first** URL in a string is recognized (v1 scope).
- `preview_child` (collapsed summary previews) does **not** do URL recognition — previews stay visually clean.

### §2.3 Truncated-string `ExpandFullValue` registration

Any leaf string whose rendered form ends with `…"` (i.e., actual truncation happened):
- Register `JsonHotRegion { range: <whole string span>, action: ExpandFullValue(node_id) }`.
- **Precedence with URL overlap** (since regions must not overlap):
  - If the string contains no URL → one `ExpandFullValue` region spans the whole string value.
  - If the string contains a URL and is truncated → the URL's columns get `OpenUrl`; the remaining string columns (quotes, non-URL text) get `ExpandFullValue`. If the URL fills the entire visible string span, `ExpandFullValue` is **omitted** (URL click is the only action). Rationale: users clicking a URL almost always want to open it; if they want the raw value they can use the keyboard (`y` or select the non-URL portion).

### §2.4 `viewer_cursor` highlight

After rendering, in `ui/logs/detail/mod.rs` and the network detail renderers:
- If `app.detail.viewer_cursor == Some(i)`, wrap `out[i]` spans so the whole line receives `Style::default().bg(SURFACE0)` (same tone as logs list selected row — Catppuccin consistency).
- Cursor out-of-bounds is clamped by app-state helpers (`detail_cursor_up/down`) before rendering; renderer does **not** mutate cursor.

### §2.5 Row whitespace default action

`detect::detect_json_action_in_detail` (pure fn):

```rust
fn detect_json_action_in_detail(
    click_map: &[Vec<JsonHotRegion>],
    line_idx: usize,
    x_in_panel: u16,
) -> Option<JsonAction> {
    let row = click_map.get(line_idx)?;
    for r in row {
        if r.range.contains(&x_in_panel) { return Some(r.action.clone()); }
    }
    row.iter().find_map(|r| matches!(r.action, JsonAction::ToggleFold(_)).then(|| r.action.clone()))
}
```

Preserves legacy behavior: clicking anywhere on a foldable row's whitespace still toggles fold.

## §3 Event dispatch

### §3.1 detect (pure)

`detect.rs` / `detect_net.rs` — the existing `LogsDetailPanel { line_idx, x }` / `NetworkDetailPanel { line_idx, x }` computation calls `detect_json_action_in_detail` first. If `Some(action)`, returns `LogsDetailJsonAction(action)` / `NetworkDetailJsonAction(action)`. Otherwise falls through to current paths (section toggle, etc.).

### §3.2 apply (mutation)

```rust
// src/event/apply.rs
ClickRegion::LogsDetailJsonAction(action) |
ClickRegion::NetworkDetailJsonAction(action) => apply_json_action(app, action),

fn apply_json_action(app: &mut App, action: JsonAction) {
    match action {
        JsonAction::ToggleFold(id) => app.toggle_detail_fold(id),
        JsonAction::CopyNode(id) => {
            let text = extract_node_json(&app.detail.viewer_tree, id);
            app.show_status(super::actions::copy_to_clipboard(&text));
        }
        JsonAction::OpenUrl(url) => {
            app.show_status(super::actions::open_url(&url));
        }
        JsonAction::ExpandFullValue(id) => {
            if let Some(text) = extract_node_string(&app.detail.viewer_tree, id) {
                app.enter_full_value_overlay(text, id);
            }
        }
    }
}
```

Helpers in `src/event/actions.rs`:

- `extract_node_json(&Tree, u32) -> String` — walks the subtree, rebuilds a `serde_json::Value`, returns `to_string_pretty`.
- `extract_node_string(&Tree, u32) -> Option<String>` — returns the raw string for a `NodeKind::String` node.

### §3.3 `open_url`

```rust
// src/event/actions.rs
pub(super) fn open_url(url: &str) -> String {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return "Open failed (only http/https allowed)".into();
    }
    let result = if cfg!(target_os = "windows") {
        std::process::Command::new("cmd").args(["/c", "start", "", url]).spawn()
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

Non-blocking (no `wait`). Scheme gate is a defense-in-depth guard in case `ExpandFullValue` ever gets misrouted here.

### §3.4 Keyboard (detail-visible, `AppMode::Normal`)

- `j` / `↓`: `app.detail_cursor_down()` (clamp to `viewer_click_map.len().saturating_sub(1)`). Activates cursor if currently `None`.
- `k` / `↑`: `app.detail_cursor_up()`.
- `Enter`: on `viewer_cursor` row, pick the **first action** with this precedence: `ExpandFullValue` > `OpenUrl` > `CopyNode` > `ToggleFold`. If the row has none of these (e.g., an `}` closer row), Enter is a no-op.
- `o`: find first `OpenUrl` on the cursor row; if none, `show_status("No URL on this line")`.
- `y`: find first `CopyNode` on the cursor row; if the row is a leaf (no CopyNode), copy the leaf's full string value.

**Pre-implementation check**: grep existing logs/network detail key handlers. If `j`/`k` are already bound to panel scroll, use `J`/`K` or reassess. (Most likely unbound in detail-visible mode — current detail panel has no row cursor.)

### §3.5 `FullValueOverlay` rendering

New `src/ui/full_value_overlay.rs`:
- Centered modal. Size = `min(content_wrapped_dims, 70% × 70%)`.
- Content wraps on panel width; overflowing content scrolls (PgUp/PgDn + j/k).
- Catppuccin: `SURFACE0` bg, `LAVENDER` border, title `Full value (Enter/click to copy, Esc to close)`.
- Rendered last in the top-level UI dispatcher so it overlays everything.

### §3.6 Overlay event handling

Top-level interception in `src/event/mod.rs` when `matches!(app.mode, AppMode::FullValueOverlay(_))`:

- Keyboard:
  - `Esc` → exit mode (no copy).
  - `Enter` / `y` → `copy_to_clipboard(&state.text)`, exit mode, `show_status("Copied")`.
  - `PgUp`/`PgDn`/`j`/`k` → scroll.
- Mouse:
  - `ClickRegion::FullValueOverlayInside` → copy + exit.
  - `ClickRegion::FullValueOverlayOutside` → exit (no copy).

### §3.7 Cleanup

- `LogsDetailClose` / `NetworkDetailClose`: clear `viewer_cursor`; existing `viewer_click_map.clear()` stays.
- Entry switch (new selection): reset cursor to `None`, rebuild click_map.

## §4 Testing

### Unit (in-module)

`src/ui/json_viewer/render/mod.rs` tests:
- `collapsed_container_has_copy_icon`
- `empty_container_no_copy_icon`
- `url_in_string_is_underlined_and_clickable`
- `truncated_url_action_carries_full_url`
- `truncated_string_registers_expand`
- `url_wins_over_expand_when_overlapping`

`src/event/detect.rs` tests:
- `detect_json_action_pinpoint`
- `detect_falls_back_to_fold_on_whitespace`
- `detect_leaf_row_whitespace_returns_none`

### Characterization

New `tests/characterization_ui_047_json_interactive.rs`:
- `apply_path_copy_node_reaches_clipboard_helper`
- `apply_path_open_url_reaches_opener_helper`
- `enter_opens_overlay_esc_closes`
- `viewer_cursor_j_k_clamp`
- `overlay_click_inside_copies_outside_cancels`

External side effects (`pbcopy`, `open`) are **not** asserted directly. Tests verify the apply path reached the correct helper (via a small trait indirection or by checking status bar text).

## §5 File changes

New:
- `src/ui/full_value_overlay.rs`
- `tests/characterization_ui_047_json_interactive.rs`

Modified:
- `src/ui/json_viewer/mod.rs` — export new types
- `src/ui/json_viewer/render/lines.rs` — ⧉ icon, URL, click_map shape
- `src/ui/json_viewer/render/summaries.rs` — `⧉` budget reservation
- `src/ui/json_viewer/render/mod.rs` — signature + tests
- `src/app/state_structs.rs` — `viewer_click_map` type, `viewer_cursor`
- `src/app/mod.rs` — `AppMode::FullValueOverlay`, `enter_full_value_overlay`
- `src/app/detail.rs` — cursor helpers, clear extension
- `src/event/click_region.rs` — `JsonAction` enum, `*JsonAction` variants, overlay variants
- `src/event/detect.rs` & `detect_net.rs` — hot-region detection
- `src/event/apply.rs` — `apply_json_action`
- `src/event/actions.rs` — `open_url`, `extract_node_json`, `extract_node_string`
- `src/event/keys.rs` — j/k/Enter/o/y bindings + overlay keys
- `src/event/mod.rs` — overlay mode interception
- `src/ui/logs/detail/mod.rs` — new `append_render` signature + cursor highlight
- `src/ui/network/detail/*.rs` — ditto
- `src/ui/mod.rs` — overlay renderer hook
- `src/ui/help/content/logs.rs` & `content/network.rs` — new keybindings
- `docs/ARCHITECTURE.md` & `docs/MODULES.md` — record `JsonAction` + overlay

Dependencies: `regex` (already in Cargo.toml for parser). No new crates.

## §6 Risks & YAGNI trimming

| Concern | Decision |
|---|---|
| Multiple URLs in one string | v1: first match only. |
| URLs containing spaces / CJK | Regex stops at whitespace/quote — robust enough. |
| Pretty JSON of very large subtrees | Acceptable; `serde_json::to_string_pretty` is fast. Overlay is for strings, not whole subtrees. |
| Overlay scroll without mouse wheel | Keyboard-only v1. |
| Cursor persistence across entries | Intentionally not persisted. |
| Key collision with existing j/k | Verify at implementation; fall back to `J`/`K` if needed. |

## §7 Migration plan

Six independently-mergeable steps:

1. Extend `click_map` data structure + `JsonAction` enum (type-only change; preserves current behavior).
2. Add `⧉` icon + `CopyNode` action (mouse + keyboard `y`).
3. URL detection + `OpenUrl` action + `open_url` implementation + keyboard `o`.
4. `viewer_cursor` + j/k navigation + Enter wiring.
5. `FullValueOverlay` mode + `ExpandFullValue` action + overlay event handling.
6. Help docs + characterization tests + `docs/ARCHITECTURE.md` + `docs/MODULES.md` updates.

Each step compiles and passes tests in isolation. Regression surface is bounded per step.
