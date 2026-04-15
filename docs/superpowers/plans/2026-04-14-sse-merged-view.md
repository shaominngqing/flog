# SSE Merged View Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a "Merged" mode to the SSE Events section in the Network detail panel that extracts a chosen JSON field path from all chunks and concatenates the values into a single readable text view, with per-path rule persistence within the session.

**Architecture:** New `SseMergeRule` map in `NetworkState` keyed by exact URL path (sans query params). The detail renderer switches between Events/Merged mode based on rule existence. Field path extraction uses `serde_json` path traversal on each chunk's data. A pill-style toggle `[Events]`/`[Merged]` in the section header controls the view, and `j/k` selects fields within Merged mode.

**Tech Stack:** Rust, ratatui, serde_json

---

### Task 1: Add SSE merge state to NetworkState

**Files:**
- Modify: `src/app.rs:145-169` (NetworkState struct)
- Modify: `src/app.rs:215-233` (NetworkState::new)

- [ ] **Step 1: Add SseMergeRule struct and merge state fields to NetworkState**

In `src/app.rs`, add the merge rule struct above `NetworkState` and add three new fields:

```rust
/// A saved SSE merge rule: which JSON field path to concatenate across chunks.
#[derive(Clone)]
pub struct SseMergeRule {
    /// JSON field path like `["choices", 0, "delta", "content"]`
    pub field_path: Vec<SsePathSegment>,
    /// Human-readable path string like `choices[0].delta.content`
    pub field_display: String,
}

/// A segment in a JSON field path.
#[derive(Clone, Debug, PartialEq)]
pub enum SsePathSegment {
    Key(String),
    Index(usize),
}
```

Add these fields to `NetworkState`:

```rust
    /// SSE merge rules: URL path (no query params) → merge rule.
    pub sse_merge_rules: std::collections::HashMap<String, SseMergeRule>,
    /// Whether the current SSE detail is showing Merged mode (true) or Events mode (false).
    pub sse_merged_mode: bool,
    /// Index of the currently selected field in Merged mode's field list.
    pub sse_merged_field_idx: usize,
```

- [ ] **Step 2: Initialize new fields in NetworkState::new()**

```rust
    sse_merge_rules: std::collections::HashMap::new(),
    sse_merged_mode: false,
    sse_merged_field_idx: 0,
```

- [ ] **Step 3: Build and run**

Run: `cargo build`
Expected: Compiles with no errors (new fields unused for now).

- [ ] **Step 4: Commit**

```bash
git add src/app.rs
git commit -m "feat(sse): add SseMergeRule state to NetworkState"
```

---

### Task 2: Implement SSE field path extraction utilities

**Files:**
- Create: `src/domain/sse_merge.rs`
- Modify: `src/domain/mod.rs` (add module)

- [ ] **Step 1: Write tests for field path extraction**

Create `src/domain/sse_merge.rs`:

```rust
//! SSE merge utilities: extract JSON field paths and concatenate values across chunks.

use crate::app::SsePathSegment;

/// Extract all unique leaf-string field paths from a JSON value.
/// Returns vec of (path_segments, display_string) pairs.
pub fn extract_field_paths(json: &serde_json::Value) -> Vec<(Vec<SsePathSegment>, String)> {
    let mut paths = Vec::new();
    collect_paths(json, &mut Vec::new(), &mut String::new(), &mut paths);
    paths
}

fn collect_paths(
    val: &serde_json::Value,
    segments: &mut Vec<SsePathSegment>,
    display: &mut String,
    out: &mut Vec<(Vec<SsePathSegment>, String)>,
) {
    match val {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                let prev_len = display.len();
                if !display.is_empty() {
                    display.push('.');
                }
                display.push_str(key);
                segments.push(SsePathSegment::Key(key.clone()));
                collect_paths(child, segments, display, out);
                segments.pop();
                display.truncate(prev_len);
            }
        }
        serde_json::Value::Array(arr) => {
            for (i, child) in arr.iter().enumerate() {
                let prev_len = display.len();
                display.push_str(&format!("[{}]", i));
                segments.push(SsePathSegment::Index(i));
                collect_paths(child, segments, display, out);
                segments.pop();
                display.truncate(prev_len);
            }
        }
        serde_json::Value::String(_) => {
            out.push((segments.clone(), display.clone()));
        }
        // Skip numbers, bools, nulls — only string leaves are useful for concatenation
        _ => {}
    }
}

/// Resolve a field path against a JSON value, returning the string value if found.
pub fn resolve_path(json: &serde_json::Value, path: &[SsePathSegment]) -> Option<String> {
    let mut current = json;
    for seg in path {
        match seg {
            SsePathSegment::Key(k) => {
                current = current.get(k)?;
            }
            SsePathSegment::Index(i) => {
                current = current.get(*i)?;
            }
        }
    }
    current.as_str().map(|s| s.to_string())
}

/// Auto-detect the best field path for SSE merge.
/// Priority: known LLM streaming patterns first, then first string field that appears in multiple chunks.
pub fn auto_detect_field(chunks_data: &[&str]) -> Option<(Vec<SsePathSegment>, String)> {
    if chunks_data.is_empty() {
        return None;
    }

    // Parse first chunk to get candidate paths
    let first: serde_json::Value = serde_json::from_str(chunks_data[0]).ok()?;
    let candidates = extract_field_paths(&first);

    if candidates.is_empty() {
        return None;
    }

    // Known LLM streaming patterns (check in order)
    let known_patterns = [
        "choices[0].delta.content",      // OpenAI / compatible
        "delta.text",                     // Claude API
        "output[0].delta.content",        // Some OpenAI variants
        "data",                           // Generic SSE
    ];

    for pattern in &known_patterns {
        if let Some(candidate) = candidates.iter().find(|(_, d)| d == pattern) {
            // Verify it resolves in at least the first chunk
            if resolve_path(&first, &candidate.0).is_some() {
                return Some(candidate.clone());
            }
        }
    }

    // Fallback: first string field that exists in at least 2 chunks (or 1 if only 1 chunk)
    let min_count = if chunks_data.len() > 1 { 2 } else { 1 };
    for (path, display) in &candidates {
        let mut count = 0;
        for cd in chunks_data.iter().take(5) {
            // Sample first 5 chunks
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(cd) {
                if resolve_path(&parsed, path).is_some() {
                    count += 1;
                }
            }
        }
        if count >= min_count {
            return Some((path.clone(), display.clone()));
        }
    }

    // Last resort: first candidate
    Some(candidates.into_iter().next().unwrap())
}

/// Concatenate a field across all chunks.
pub fn merge_field(chunks_data: &[&str], path: &[SsePathSegment]) -> String {
    let mut result = String::new();
    for cd in chunks_data {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(cd) {
            if let Some(val) = resolve_path(&parsed, path) {
                result.push_str(&val);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn openai_chunk(content: &str) -> String {
        serde_json::json!({
            "id": "chatcmpl-123",
            "object": "response.chunk",
            "choices": [{"delta": {"content": content}}],
            "model": "claude-sonnet-4.6"
        })
        .to_string()
    }

    #[test]
    fn test_extract_field_paths() {
        let json: serde_json::Value = serde_json::from_str(&openai_chunk("hello")).unwrap();
        let paths = extract_field_paths(&json);
        let displays: Vec<&str> = paths.iter().map(|(_, d)| d.as_str()).collect();
        assert!(displays.contains(&"id"));
        assert!(displays.contains(&"object"));
        assert!(displays.contains(&"choices[0].delta.content"));
        assert!(displays.contains(&"model"));
    }

    #[test]
    fn test_resolve_path() {
        let json: serde_json::Value = serde_json::from_str(&openai_chunk("hello")).unwrap();
        let path = vec![
            SsePathSegment::Key("choices".into()),
            SsePathSegment::Index(0),
            SsePathSegment::Key("delta".into()),
            SsePathSegment::Key("content".into()),
        ];
        assert_eq!(resolve_path(&json, &path), Some("hello".to_string()));
    }

    #[test]
    fn test_resolve_path_missing() {
        let json: serde_json::Value = serde_json::from_str(&openai_chunk("hello")).unwrap();
        let path = vec![SsePathSegment::Key("nonexistent".into())];
        assert_eq!(resolve_path(&json, &path), None);
    }

    #[test]
    fn test_auto_detect_openai() {
        let c1 = openai_chunk("Hello");
        let c2 = openai_chunk(" world");
        let chunks: Vec<&str> = vec![&c1, &c2];
        let result = auto_detect_field(&chunks);
        assert!(result.is_some());
        let (_, display) = result.unwrap();
        assert_eq!(display, "choices[0].delta.content");
    }

    #[test]
    fn test_merge_field() {
        let c1 = openai_chunk("Hello");
        let c2 = openai_chunk(" world");
        let c3 = openai_chunk("!");
        let chunks: Vec<&str> = vec![&c1, &c2, &c3];
        let path = vec![
            SsePathSegment::Key("choices".into()),
            SsePathSegment::Index(0),
            SsePathSegment::Key("delta".into()),
            SsePathSegment::Key("content".into()),
        ];
        assert_eq!(merge_field(&chunks, &path), "Hello world!");
    }

    #[test]
    fn test_merge_field_skips_missing() {
        let c1 = openai_chunk("Hello");
        // Chunk without content field
        let c2 = serde_json::json!({"id": "123", "object": "response.chunk"}).to_string();
        let c3 = openai_chunk(" world");
        let chunks: Vec<&str> = vec![&c1, &c2, &c3];
        let path = vec![
            SsePathSegment::Key("choices".into()),
            SsePathSegment::Index(0),
            SsePathSegment::Key("delta".into()),
            SsePathSegment::Key("content".into()),
        ];
        assert_eq!(merge_field(&chunks, &path), "Hello world");
    }

    #[test]
    fn test_auto_detect_empty_chunks() {
        let chunks: Vec<&str> = vec![];
        assert!(auto_detect_field(&chunks).is_none());
    }

    #[test]
    fn test_auto_detect_non_json() {
        let chunks: Vec<&str> = vec!["not json"];
        assert!(auto_detect_field(&chunks).is_none());
    }
}
```

- [ ] **Step 2: Add module to domain/mod.rs**

Find the `pub mod` declarations in `src/domain/mod.rs` and add:

```rust
pub mod sse_merge;
```

- [ ] **Step 3: Run tests**

Run: `cargo test --lib domain::sse_merge`
Expected: All 7 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/domain/sse_merge.rs src/domain/mod.rs
git commit -m "feat(sse): add field path extraction and merge utilities with tests"
```

---

### Task 3: Render Merged mode in SSE detail section

**Files:**
- Modify: `src/ui/network/detail.rs:343-413` (SSE Events rendering)

This task replaces the SSE Events section rendering with a mode-aware renderer that shows either Events (current behavior) or Merged view based on `app.network.sse_merged_mode`.

- [ ] **Step 1: Add imports to detail.rs**

At the top of `src/ui/network/detail.rs`, add to the existing `use crate::domain::network::` import:

```rust
use crate::domain::sse_merge;
```

- [ ] **Step 2: Add helper to strip query params from path**

Add this helper function near the other helpers at the bottom of `detail.rs` (after `render_json_section`):

```rust
/// Strip query params from URL path for merge rule matching.
fn path_without_query(path: &str) -> &str {
    path.split('?').next().unwrap_or(path)
}
```

- [ ] **Step 3: Replace SSE Events section rendering**

Replace the SSE section (lines 343-413 in `src/ui/network/detail.rs`) — from `// ── SSE Stream Events ──` up to and including the closing brace before `// ── WebSocket Messages ──` — with the following:

```rust
    // ── SSE Stream Events ──
    if entry.protocol == Protocol::Sse && !entry.sse_chunks.is_empty() {
        let rule_key = path_without_query(&entry.path).to_string();
        let has_rule = app.network.sse_merge_rules.contains_key(&rule_key);

        // Determine mode: if rule exists, default to merged; otherwise events only
        if has_rule && app.network.sse_merged_mode {
            // ── Merged mode ──
            let sec_key = "SSE Events";
            let is_collapsed = app.network.collapsed_sections.contains(sec_key);

            // Section header with pill toggle
            {
                let events_pill = Span::styled(
                    " Events ",
                    Style::default().fg(OVERLAY0).bg(SURFACE0),
                );
                let merged_pill = Span::styled(
                    " Merged ",
                    Style::default()
                        .fg(MANTLE)
                        .bg(SAPPHIRE)
                        .add_modifier(Modifier::BOLD),
                );
                let icon = if is_collapsed { "\u{25b6}" } else { "\u{25bc}" };
                let header_text = format!(
                    " {} SSE Events ({})  ",
                    icon,
                    entry.sse_chunks.len()
                );
                all_lines.push(Line::from(vec![
                    Span::styled(
                        header_text,
                        Style::default().fg(SAPPHIRE).add_modifier(Modifier::BOLD),
                    ),
                    events_pill,
                    Span::raw(" "),
                    merged_pill,
                ]));
                section_line_map.push(Some(sec_key.to_string()));
                json_click_map.push(None);
            }

            if !is_collapsed {
                let rule = app.network.sse_merge_rules.get(&rule_key).cloned();
                if let Some(rule) = rule {
                    // Collect all chunk data refs
                    let chunks_data: Vec<&str> =
                        entry.sse_chunks.iter().map(|c| c.data.as_str()).collect();

                    // Build candidate field list
                    let candidates = if let Some(first_json) =
                        chunks_data.first().and_then(|d| serde_json::from_str::<serde_json::Value>(d).ok())
                    {
                        sse_merge::extract_field_paths(&first_json)
                    } else {
                        vec![]
                    };

                    // Render field selector
                    let selected_idx = app.network.sse_merged_field_idx.min(
                        candidates.len().saturating_sub(1),
                    );
                    for (fi, (_, display)) in candidates.iter().enumerate() {
                        let is_active = fi == selected_idx;
                        let marker = if is_active { "\u{2023} " } else { "  " };
                        let style = if is_active {
                            Style::default().fg(SAPPHIRE).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(OVERLAY0)
                        };
                        all_lines.push(Line::from(Span::styled(
                            format!("  {}{}", marker, display),
                            style,
                        )));
                        section_line_map.push(Some(format!("SSE_FIELD#{}", fi)));
                        json_click_map.push(None);
                    }

                    // Divider
                    let divider_w = inner_w.saturating_sub(2);
                    all_lines.push(Line::from(Span::styled(
                        format!("  {}", "\u{2500}".repeat(divider_w)),
                        Style::default().fg(SURFACE0),
                    )));
                    section_line_map.push(None);
                    json_click_map.push(None);

                    // Merge and render concatenated text
                    let merged_text = sse_merge::merge_field(&chunks_data, &rule.field_path);
                    if merged_text.is_empty() {
                        all_lines.push(Line::from(Span::styled(
                            "   (no data for this field)",
                            Style::default().fg(OVERLAY0),
                        )));
                        section_line_map.push(None);
                        json_click_map.push(None);
                    } else {
                        for wl in wrap_text(&merged_text, inner_w.saturating_sub(3), 500) {
                            all_lines.push(Line::from(Span::styled(
                                format!("   {}", wl),
                                Style::default().fg(TEXT),
                            )));
                            section_line_map.push(None);
                            json_click_map.push(None);
                        }
                    }

                    // Clear rule button
                    all_lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            " Clear Rule ",
                            Style::default().fg(MANTLE).bg(RED).add_modifier(Modifier::BOLD),
                        ),
                    ]));
                    section_line_map.push(Some("SSE_CLEAR_RULE".to_string()));
                    json_click_map.push(None);
                }
            }
            all_lines.push(Line::raw(""));
            section_line_map.push(None);
            json_click_map.push(None);
        } else {
            // ── Events mode (original + pill toggle if rule exists) ──
            let sec_name = format!("SSE Events ({})", entry.sse_chunks.len());
            let sec_key = "SSE Events";
            let is_collapsed = app.network.collapsed_sections.contains(sec_key);

            if has_rule {
                // Show pills when rule exists
                let events_pill = Span::styled(
                    " Events ",
                    Style::default()
                        .fg(MANTLE)
                        .bg(SAPPHIRE)
                        .add_modifier(Modifier::BOLD),
                );
                let merged_pill = Span::styled(
                    " Merged ",
                    Style::default().fg(OVERLAY0).bg(SURFACE0),
                );
                let icon = if is_collapsed { "\u{25b6}" } else { "\u{25bc}" };
                all_lines.push(Line::from(vec![
                    Span::styled(
                        format!(" {} {}  ", icon, sec_name),
                        Style::default().fg(SAPPHIRE).add_modifier(Modifier::BOLD),
                    ),
                    events_pill,
                    Span::raw(" "),
                    merged_pill,
                ]));
                section_line_map.push(Some(sec_key.to_string()));
                json_click_map.push(None);
            } else {
                push_section_header(
                    &mut all_lines,
                    &mut section_line_map,
                    &mut json_click_map,
                    &sec_name,
                    is_collapsed,
                );
                // Override map entry to use fixed key (strip count)
                if let Some(last) = section_line_map.last_mut() {
                    *last = Some(sec_key.to_string());
                }
            }

            if !is_collapsed {
                // Pre-collapse SSE chunks by default on FIRST render only.
                let init_key = "_sse_init";
                if !app.network.collapsed_sections.contains(init_key) {
                    app.network.collapsed_sections.insert(init_key.to_string());
                    for i in 0..entry.sse_chunks.len() {
                        app.network.collapsed_sections.insert(format!("SSE#{}", i));
                    }
                }

                for (i, chunk) in entry.sse_chunks.iter().enumerate() {
                    let chunk_key = format!("SSE#{}", i);
                    let chunk_collapsed = app.network.collapsed_sections.contains(&chunk_key);
                    let prefix = if chunk_collapsed {
                        "  \u{25b6}"
                    } else {
                        "  \u{25bc}"
                    };
                    let prefix_text = format!("{} #{} ", prefix, i);
                    let preview_w = inner_w.saturating_sub(prefix_text.len() + 1);
                    all_lines.push(Line::from(vec![
                        Span::styled(prefix_text, Style::default().fg(OVERLAY0)),
                        Span::styled(
                            if chunk_collapsed {
                                if chunk.data.len() > preview_w {
                                    format!("{}...", &chunk.data[..preview_w.saturating_sub(3)])
                                } else {
                                    chunk.data.clone()
                                }
                            } else {
                                String::new()
                            },
                            Style::default().fg(SUBTEXT0),
                        ),
                    ]));
                    section_line_map.push(Some(chunk_key.clone()));
                    json_click_map.push(None);
                    if !chunk_collapsed {
                        render_json_section(
                            &mut all_lines,
                            &mut section_line_map,
                            &mut json_click_map,
                            &chunk.data,
                            &format!("sse_{}", i),
                            &mut app.network.json_viewer_states,
                            inner_w,
                        );
                    }
                }
            }
            all_lines.push(Line::raw(""));
            section_line_map.push(None);
            json_click_map.push(None);
        }
    }
```

- [ ] **Step 4: Build**

Run: `cargo build`
Expected: Compiles. (Merged mode not yet reachable — no event wiring yet.)

- [ ] **Step 5: Commit**

```bash
git add src/ui/network/detail.rs
git commit -m "feat(sse): render Merged mode with field selector and concatenated text"
```

---

### Task 4: Wire up click and keyboard events for Merged mode

**Files:**
- Modify: `src/event.rs:194-234` (detail panel click handling)
- Modify: `src/event.rs:982-1037` (network keyboard handling)

- [ ] **Step 1: Add pill click regions to LayoutCache**

In `src/app.rs`, add to the `LayoutCache` struct (after `detail_mock_btn` field around line 302):

```rust
    /// Click regions for SSE mode pills: (pill_name, y, x_start, x_end).
    pub sse_mode_pills: Vec<(String, u16, u16, u16)>,
```

Initialize in `LayoutCache::default()` — the `#[derive(Default)]` handles this since `Vec` defaults to empty.

- [ ] **Step 2: Record pill click regions in the renderer**

In `src/ui/network/detail.rs`, in the Merged mode section header rendering (where `events_pill` and `merged_pill` are created), add click region recording after pushing the header line. Insert this right after the `all_lines.push(Line::from(...))` call for the header, before `section_line_map.push(...)`:

First, clear previous pill regions at the top of `draw_network_detail` function (after `let mut json_click_map` line):

```rust
    app.layout.sse_mode_pills.clear();
```

Then, in both places where pills are rendered (Merged mode header and Events mode header with `has_rule`), record the pill positions. After each `all_lines.push(Line::from(vec![...]))` that includes pills, add:

```rust
                // Record pill click regions
                let pill_y = area.y + 1 + (all_lines.len() - 1) as u16; // approximate
                // We compute X positions from the header text width
                let header_w = header_text.width() as u16;
                let base_x = area.x + 1 + header_w;
                let events_w = " Events ".len() as u16;
                let merged_start = base_x + events_w + 1; // +1 for space
                let merged_w = " Merged ".len() as u16;
                app.layout.sse_mode_pills.push(("events".to_string(), pill_y, base_x, base_x + events_w));
                app.layout.sse_mode_pills.push(("merged".to_string(), pill_y, merged_start, merged_start + merged_w));
```

Note: The exact Y computation is tricky because of scroll. A simpler approach is to handle pill clicks via `detail_section_map` — we'll use section keys instead. See Step 3.

**Actually, simpler approach:** Instead of tracking exact pixel positions for pills, use the existing `section_line_map` mechanism. When the user clicks the SSE Events section header line:
- If the click X is in the "Events" pill region → switch to Events mode
- If the click X is in the "Merged" pill region → switch to Merged mode
- Otherwise → toggle collapse (existing behavior)

But since we don't track X within a section line, use dedicated section keys instead:

Register the pill lines as separate clickable entries in `section_line_map`. Replace the pill approach: instead of inline pills on the header, add two clickable lines below the header. **No — this changes the visual design.**

**Best approach:** Handle pill clicks in the mouse handler by checking `detail_section_map` for the `"SSE Events"` key and then checking X position against pill locations stored in `LayoutCache`. Let's keep the `sse_mode_pills` approach but compute positions more carefully.

In `src/ui/network/detail.rs`, after each pill header `all_lines.push(...)`, record the regions. The key insight is that the line index in `all_lines` tells us the display line, and we know `area` and `detail_scroll`:

```rust
                // Record pill click regions using the all_lines index
                let line_in_content = all_lines.len() - 1;
                app.layout.sse_mode_pills.clear();
                app.layout.sse_mode_pills.push(("events".to_string(), line_in_content as u16, 0, 0));
                app.layout.sse_mode_pills.push(("merged".to_string(), line_in_content as u16, 0, 0));
```

We store the `line_in_content` index and compute actual X in the click handler by comparing against the header text width. This is simpler. Let me revise to a cleaner approach:

Store: `pub sse_pill_line: Option<usize>` (the all_lines index of the header with pills), and `pub sse_pill_header_w: usize` (width of text before pills).

In `src/app.rs` LayoutCache:

```rust
    /// Line index (in all_lines) of SSE section header with pills, and the text width before pills.
    pub sse_pill_line: Option<(usize, usize)>,
```

- [ ] **Step 3: Revise LayoutCache — use sse_pill_line instead**

Remove the `sse_mode_pills` field added in Step 1. Instead add to `LayoutCache`:

```rust
    /// SSE pill line: (all_lines_index, header_text_width) for computing pill click positions.
    pub sse_pill_line: Option<(usize, usize)>,
```

In `draw_network_detail`, at the top (after clearing other state):

```rust
    app.layout.sse_pill_line = None;
```

In both pill-rendering locations (Merged header and Events-with-rule header), after `all_lines.push(...)`:

```rust
                app.layout.sse_pill_line = Some((all_lines.len() - 1, header_text.width()));
```

For the Events-with-rule case, `header_text` is `format!(" {} {}  ", icon, sec_name)`:

```rust
                let header_text = format!(" {} {}  ", icon, sec_name);
                // ... all_lines.push(...) ...
                app.layout.sse_pill_line = Some((all_lines.len() - 1, header_text.width()));
```

- [ ] **Step 4: Handle pill clicks in event.rs**

In `src/event.rs`, in the detail panel click handler (around line 210-221 where section_line_map is checked), add a check **before** the generic section toggle. After the `let line_idx = ...` computation (line 209):

```rust
                        // Check SSE pill clicks
                        if let Some((pill_line, header_w)) = app.layout.sse_pill_line {
                            if line_idx == pill_line {
                                let click_x = (x - area_x) as usize; // x relative to detail panel
                                let events_start = header_w;
                                let events_end = events_start + " Events ".len();
                                let merged_start = events_end + 1;
                                let merged_end = merged_start + " Merged ".len();
                                if click_x >= events_start && click_x < events_end {
                                    app.network.sse_merged_mode = false;
                                    return;
                                } else if click_x >= merged_start && click_x < merged_end {
                                    // Create rule if not exists
                                    let indices = app.network.filtered_indices(&app.network_store).to_vec();
                                    if let Some(&idx) = indices.get(app.network.selected) {
                                        if let Some(entry) = app.network_store.get(idx) {
                                            if entry.protocol == crate::domain::network::Protocol::Sse {
                                                let rule_key = entry.path.split('?').next().unwrap_or(&entry.path).to_string();
                                                if !app.network.sse_merge_rules.contains_key(&rule_key) {
                                                    // Auto-detect field and create rule
                                                    let chunks_data: Vec<&str> = entry.sse_chunks.iter().map(|c| c.data.as_str()).collect();
                                                    if let Some((path, display)) = crate::domain::sse_merge::auto_detect_field(&chunks_data) {
                                                        app.network.sse_merge_rules.insert(rule_key, crate::app::SseMergeRule {
                                                            field_path: path,
                                                            field_display: display,
                                                        });
                                                    }
                                                }
                                                app.network.sse_merged_mode = true;
                                                app.network.sse_merged_field_idx = 0;
                                            }
                                        }
                                    }
                                    return;
                                }
                                // If click is not on pills, fall through to section toggle
                            }
                        }
```

We need `area_x` — this is `app.layout.net_detail_x`. So use:

```rust
                                let click_x = (x.saturating_sub(app.layout.net_detail_x + 1)) as usize;
```

- [ ] **Step 5: Handle field selection clicks in Merged mode**

Still in the detail panel click handler, add handling for `SSE_FIELD#N` section keys:

```rust
                        // Check SSE field selection clicks
                        if let Some(Some(section_key)) = app.network.detail_section_map.get(line_idx) {
                            if let Some(idx_str) = section_key.strip_prefix("SSE_FIELD#") {
                                if let Ok(fi) = idx_str.parse::<usize>() {
                                    // Update selected field and rule
                                    app.network.sse_merged_field_idx = fi;
                                    // Update the rule with new field
                                    let indices = app.network.filtered_indices(&app.network_store).to_vec();
                                    if let Some(&store_idx) = indices.get(app.network.selected) {
                                        if let Some(entry) = app.network_store.get(store_idx) {
                                            let rule_key = entry.path.split('?').next().unwrap_or(&entry.path).to_string();
                                            let chunks_data: Vec<&str> = entry.sse_chunks.iter().map(|c| c.data.as_str()).collect();
                                            if let Some(first_json) = chunks_data.first().and_then(|d| serde_json::from_str::<serde_json::Value>(d).ok()) {
                                                let candidates = crate::domain::sse_merge::extract_field_paths(&first_json);
                                                if let Some((path, display)) = candidates.into_iter().nth(fi) {
                                                    app.network.sse_merge_rules.insert(rule_key, crate::app::SseMergeRule {
                                                        field_path: path,
                                                        field_display: display,
                                                    });
                                                }
                                            }
                                        }
                                    }
                                    return;
                                }
                            }
                            // Handle Clear Rule click
                            if section_key == "SSE_CLEAR_RULE" {
                                let indices = app.network.filtered_indices(&app.network_store).to_vec();
                                if let Some(&store_idx) = indices.get(app.network.selected) {
                                    if let Some(entry) = app.network_store.get(store_idx) {
                                        let rule_key = entry.path.split('?').next().unwrap_or(&entry.path).to_string();
                                        app.network.sse_merge_rules.remove(&rule_key);
                                        app.network.sse_merged_mode = false;
                                    }
                                }
                                return;
                            }
                        }
```

Place this **before** the existing generic section toggle code (the `if let Some(Some(section_key)) = app.network.detail_section_map.get(line_idx)` block). The existing block will need to be wrapped in an `else` or the new checks need early returns (which they already have).

- [ ] **Step 6: Handle j/k for field selection in Merged mode**

In the keyboard handler section for Network tab (around line 987), add a check for Merged mode before the existing j/k handling. Insert before the existing `KeyCode::Char('k') | KeyCode::Up` match:

```rust
                // SSE Merged mode: j/k switch fields
                KeyCode::Char('j') | KeyCode::Down if app.network.sse_merged_mode && app.network.show_detail => {
                    let indices = app.network.filtered_indices(&app.network_store).to_vec();
                    if let Some(&idx) = indices.get(app.network.selected) {
                        if let Some(entry) = app.network_store.get(idx) {
                            if entry.protocol == crate::domain::network::Protocol::Sse {
                                let chunks_data: Vec<&str> = entry.sse_chunks.iter().map(|c| c.data.as_str()).collect();
                                if let Some(first_json) = chunks_data.first().and_then(|d| serde_json::from_str::<serde_json::Value>(d).ok()) {
                                    let count = crate::domain::sse_merge::extract_field_paths(&first_json).len();
                                    if count > 0 {
                                        let new_idx = (app.network.sse_merged_field_idx + 1).min(count - 1);
                                        app.network.sse_merged_field_idx = new_idx;
                                        // Update rule
                                        let rule_key = entry.path.split('?').next().unwrap_or(&entry.path).to_string();
                                        let candidates = crate::domain::sse_merge::extract_field_paths(&first_json);
                                        if let Some((path, display)) = candidates.into_iter().nth(new_idx) {
                                            app.network.sse_merge_rules.insert(rule_key, crate::app::SseMergeRule {
                                                field_path: path,
                                                field_display: display,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                KeyCode::Char('k') | KeyCode::Up if app.network.sse_merged_mode && app.network.show_detail => {
                    app.network.sse_merged_field_idx = app.network.sse_merged_field_idx.saturating_sub(1);
                    // Update rule
                    let indices = app.network.filtered_indices(&app.network_store).to_vec();
                    if let Some(&idx) = indices.get(app.network.selected) {
                        if let Some(entry) = app.network_store.get(idx) {
                            if entry.protocol == crate::domain::network::Protocol::Sse {
                                let rule_key = entry.path.split('?').next().unwrap_or(&entry.path).to_string();
                                let chunks_data: Vec<&str> = entry.sse_chunks.iter().map(|c| c.data.as_str()).collect();
                                if let Some(first_json) = chunks_data.first().and_then(|d| serde_json::from_str::<serde_json::Value>(d).ok()) {
                                    let candidates = crate::domain::sse_merge::extract_field_paths(&first_json);
                                    if let Some((path, display)) = candidates.into_iter().nth(app.network.sse_merged_field_idx) {
                                        app.network.sse_merge_rules.insert(rule_key, crate::app::SseMergeRule {
                                            field_path: path,
                                            field_display: display,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
```

- [ ] **Step 7: Auto-enter Merged mode when selecting SSE entry with existing rule**

In `src/event.rs`, where selection changes reset detail state (around lines 286-295 where `collapsed_sections.clear()` is called), add after the clear:

```rust
                            // Auto-enter merged mode if SSE entry has a rule
                            if let Some(&new_idx) = app.network.filtered_indices(&app.network_store).to_vec().get(target) {
                                if let Some(entry) = app.network_store.get(new_idx) {
                                    if entry.protocol == crate::domain::network::Protocol::Sse {
                                        let rule_key = entry.path.split('?').next().unwrap_or(&entry.path).to_string();
                                        app.network.sse_merged_mode = app.network.sse_merge_rules.contains_key(&rule_key);
                                        if app.network.sse_merged_mode {
                                            if let Some(rule) = app.network.sse_merge_rules.get(&rule_key) {
                                                // Find matching field index
                                                let chunks_data: Vec<&str> = entry.sse_chunks.iter().map(|c| c.data.as_str()).collect();
                                                if let Some(first_json) = chunks_data.first().and_then(|d| serde_json::from_str::<serde_json::Value>(d).ok()) {
                                                    let candidates = crate::domain::sse_merge::extract_field_paths(&first_json);
                                                    app.network.sse_merged_field_idx = candidates.iter()
                                                        .position(|(_, d)| d == &rule.field_display)
                                                        .unwrap_or(0);
                                                }
                                            }
                                        }
                                    } else {
                                        app.network.sse_merged_mode = false;
                                    }
                                }
                            }
```

Do the same for the other selection change path (the `else` branch around line 290-295).

- [ ] **Step 8: Build and test**

Run: `cargo build`
Expected: Compiles with no errors.

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 9: Commit**

```bash
git add src/event.rs src/app.rs src/ui/network/detail.rs
git commit -m "feat(sse): wire up Merged mode clicks, j/k field selection, and auto-enter"
```

---

### Task 5: Manual integration testing

**Files:** None (testing only)

- [ ] **Step 1: Build release and test with real SSE data**

Run: `cargo build --release && cargo install --path .`

Test flow:
1. Connect to a Flutter app that makes SSE requests (e.g., LLM chat)
2. Trigger an SSE request
3. Open Network tab, select the SSE entry, press Enter to show detail
4. Verify SSE Events section shows the original chunk list (Events mode, no pills yet)
5. Click on [Merged] — should not appear yet (no rule)

Wait — the `[Merged]` pill only appears after a rule exists, and rules are created by clicking `[Merged]`. This is a chicken-and-egg problem. We need to show the `[Merged]` pill even without a rule for SSE entries.

- [ ] **Step 2: Fix — always show pills for SSE entries with JSON chunks**

In `src/ui/network/detail.rs`, change the Events-mode rendering. When the entry is SSE, always show pills (not just when `has_rule`). Replace the `if has_rule {` check in the Events section with:

```rust
            // Check if chunks contain JSON (merged mode is available)
            let has_json_chunks = entry.sse_chunks.first()
                .map(|c| serde_json::from_str::<serde_json::Value>(&c.data).is_ok())
                .unwrap_or(false);

            if has_json_chunks {
```

This means: if chunks are JSON, always show `[Events] [Merged]` pills. Clicking `[Merged]` creates the rule via auto-detect.

- [ ] **Step 3: Rebuild and re-test**

Run: `cargo build`

Test the full flow:
1. SSE entry with JSON chunks → should show `[Events] [Merged]` pills
2. Click `[Merged]` → auto-detects field, creates rule, switches to Merged view
3. See field list with `‣` marker on auto-detected field
4. See concatenated text below divider
5. Press `j/k` → field selector moves, merged text updates
6. Click field in list → same effect
7. Click `[Events]` → back to chunk list
8. Click `[Merged]` → returns to Merged view (rule persists)
9. Click `[Clear Rule]` → back to initial state (pills still visible for JSON chunks)
10. Select a different SSE entry with same path → should auto-enter Merged if rule exists
11. Select SSE entry with non-JSON data → no pills shown

- [ ] **Step 4: Commit any fixes**

```bash
git add -u
git commit -m "fix(sse): show Merged pill for all JSON SSE entries, not just those with rules"
```

---

### Task 6: Edge cases and polish

**Files:**
- Modify: `src/ui/network/detail.rs`
- Modify: `src/event.rs`

- [ ] **Step 1: Handle active SSE connections (new chunks arriving)**

No code change needed — the renderer re-runs on every tick, so new chunks are automatically included in `merge_field()` since it reads from the live `entry.sse_chunks` vec.

Verify: Start an SSE stream, enter Merged mode, watch text grow as chunks arrive.

- [ ] **Step 2: Handle Esc key to exit Merged mode**

In `src/event.rs` keyboard handler, in the Network section, add:

```rust
                KeyCode::Esc if app.network.sse_merged_mode && app.network.show_detail => {
                    app.network.sse_merged_mode = false;
                }
```

Place before other Esc handlers.

- [ ] **Step 3: Build and verify**

Run: `cargo build`
Expected: Compiles.

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add -u
git commit -m "feat(sse): add Esc to exit Merged mode, verify live chunk updates"
```
