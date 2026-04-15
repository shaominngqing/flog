# WS Chat View Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a directional "Chat" view to WS Messages in the Network detail panel — send left (green), recv right (blue), with type extraction, binary detection, and delta message grouping.

**Architecture:** New `ws_chat.rs` domain module handles type extraction, binary detection, and message grouping into `ChatGroup` structs. The detail renderer switches between Chat (default) and Raw mode via `[Chat] [Raw]` pills. Event handler wires pill clicks and group expand/collapse. No persistent rules needed — Chat mode is stateless.

**Tech Stack:** Rust, ratatui, serde_json, regex (for base64 detection)

---

## File Structure

| File | Responsibility |
|------|----------------|
| `src/domain/ws_chat.rs` (create) | Type extraction, binary detection, delta grouping — pure logic, no UI |
| `src/domain/mod.rs` (modify) | Add `pub mod ws_chat;` |
| `src/app.rs` (modify) | Add `ws_chat_mode: bool` to NetworkState, `ws_pill_line` to LayoutCache |
| `src/ui/network/detail.rs` (modify) | Replace WS Messages section with Chat/Raw dual-mode renderer |
| `src/event.rs` (modify) | Pill click handling, group expand/collapse |

---

### Task 1: Add WS Chat state fields

**Files:**
- Modify: `src/app.rs:160-191` (NetworkState struct + new())

- [ ] **Step 1: Add ws_chat_mode to NetworkState**

In `src/app.rs`, add after the `sse_merged_field_idx` field (line 188):

```rust
    /// Whether WS detail shows Chat view (true, default) or Raw view (false).
    pub ws_chat_mode: bool,
```

- [ ] **Step 2: Add ws_pill_line to LayoutCache**

In `src/app.rs`, add after the `sse_pill_line` field in the `LayoutCache` struct:

```rust
    /// WS pill line: (all_lines_index, header_text_width) for computing pill click positions.
    pub ws_pill_line: Option<(usize, usize)>,
```

- [ ] **Step 3: Initialize in NetworkState::new()**

Add after `sse_merged_field_idx: 0,` (line 254):

```rust
            ws_chat_mode: true,
```

- [ ] **Step 4: Build**

Run: `cargo build`
Expected: Compiles with unused field warnings.

- [ ] **Step 5: Commit**

```bash
git add src/app.rs
git commit -m "feat(ws): add ws_chat_mode state to NetworkState"
```

---

### Task 2: Implement WS Chat grouping logic

**Files:**
- Create: `src/domain/ws_chat.rs`
- Modify: `src/domain/mod.rs`

- [ ] **Step 1: Create ws_chat.rs with all logic and tests**

Create `src/domain/ws_chat.rs`:

```rust
//! WS Chat View utilities: type extraction, binary detection, and message grouping.

use crate::domain::network::WsDirection;

/// A group of consecutive WS messages with the same type and direction.
#[derive(Debug, Clone)]
pub struct ChatGroup {
    /// Direction of messages in this group.
    pub direction: WsDirection,
    /// Extracted type label (e.g., "session.update", "[message]", "[text]").
    pub type_label: String,
    /// Indices into the original ws_messages vec.
    pub msg_indices: Vec<usize>,
    /// If this is a delta group, the concatenated delta text.
    pub merged_delta: Option<String>,
    /// If all messages in this group are binary.
    pub is_binary: bool,
    /// Total size of all messages in this group.
    pub total_size: u64,
}

/// Extract the "type" label from a WS message's JSON data.
/// Scans common keys: type, event, action, op, cmd, method.
/// Returns "[message]" for JSON without a type key, "[text]" for non-JSON.
pub fn extract_type(data: &str) -> String {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
        for key in &["type", "event", "action", "op", "cmd", "method"] {
            if let Some(serde_json::Value::String(t)) = val.get(key) {
                return t.clone();
            }
        }
        "[message]".to_string()
    } else {
        "[text]".to_string()
    }
}

/// Check if a string looks like base64-encoded binary data.
/// Returns true if length > 1024 and content matches base64 charset.
pub fn is_base64_binary(s: &str) -> bool {
    s.len() > 1024 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=' || b == b'\n' || b == b'\r' || b == b' ')
}

/// Check if a JSON message contains any binary (large base64) field values.
pub fn has_binary_content(data: &str) -> bool {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
        check_binary_recursive(&val)
    } else {
        // Non-JSON: check if the raw data itself is binary
        is_base64_binary(data)
    }
}

fn check_binary_recursive(val: &serde_json::Value) -> bool {
    match val {
        serde_json::Value::String(s) => is_base64_binary(s),
        serde_json::Value::Object(map) => map.values().any(|v| check_binary_recursive(v)),
        serde_json::Value::Array(arr) => arr.iter().any(|v| check_binary_recursive(v)),
        _ => false,
    }
}

/// Extract the "delta" string field from a JSON message, if present.
pub fn extract_delta(data: &str) -> Option<String> {
    let val: serde_json::Value = serde_json::from_str(data).ok()?;
    val.get("delta")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Build a preview string for a message, replacing binary values with [binary N].
pub fn preview_message(data: &str, max_len: usize) -> String {
    if has_binary_content(data) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
            let replaced = replace_binary_in_json(&val);
            let s = replaced.to_string();
            if s.len() > max_len {
                format!("{}...", &s[..max_len.saturating_sub(3)])
            } else {
                s
            }
        } else {
            let decoded_size = data.len() * 3 / 4;
            format!("[binary {}]", format_binary_size(decoded_size))
        }
    } else if data.len() > max_len {
        format!("{}...", &data[..max_len.saturating_sub(3)])
    } else {
        data.to_string()
    }
}

fn replace_binary_in_json(val: &serde_json::Value) -> serde_json::Value {
    match val {
        serde_json::Value::String(s) if is_base64_binary(s) => {
            let decoded_size = s.len() * 3 / 4;
            serde_json::Value::String(format!("[binary {}]", format_binary_size(decoded_size)))
        }
        serde_json::Value::Object(map) => {
            let new_map: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), replace_binary_in_json(v)))
                .collect();
            serde_json::Value::Object(new_map)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(replace_binary_in_json).collect())
        }
        other => other.clone(),
    }
}

fn format_binary_size(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{}B", bytes)
    }
}

/// Group consecutive WS messages by (direction, type).
/// Delta messages with string delta fields are merged.
/// Binary-only groups are flagged.
pub fn group_messages(messages: &[(WsDirection, &str, u64)]) -> Vec<ChatGroup> {
    let mut groups: Vec<ChatGroup> = Vec::new();

    for (idx, (direction, data, size)) in messages.iter().enumerate() {
        let type_label = extract_type(data);
        let is_binary = has_binary_content(data);
        let delta = extract_delta(data);

        // Check if we can extend the current group
        let can_extend = if let Some(last) = groups.last() {
            last.direction == *direction && last.type_label == type_label
        } else {
            false
        };

        if can_extend {
            let last = groups.last_mut().unwrap();
            last.msg_indices.push(idx);
            last.total_size += size;
            if is_binary {
                // group stays binary only if ALL messages are binary
            } else {
                last.is_binary = false;
            }
            // Append delta if this is a delta group
            if let Some(d) = delta {
                if let Some(ref mut merged) = last.merged_delta {
                    merged.push_str(&d);
                }
                // else: first message had no delta, so this group doesn't merge
            }
        } else {
            // Start a new group
            let is_delta_type = type_label.to_lowercase().contains("delta");
            groups.push(ChatGroup {
                direction: *direction,
                type_label,
                msg_indices: vec![idx],
                merged_delta: if is_delta_type { delta.or_else(|| Some(String::new())) } else { None },
                is_binary,
                total_size: *size,
            });
        }
    }

    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_type_with_type_field() {
        let data = r#"{"type":"session.update","session":{}}"#;
        assert_eq!(extract_type(data), "session.update");
    }

    #[test]
    fn test_extract_type_with_event_field() {
        let data = r#"{"event":"message","data":"hello"}"#;
        assert_eq!(extract_type(data), "message");
    }

    #[test]
    fn test_extract_type_no_known_key() {
        let data = r#"{"foo":"bar"}"#;
        assert_eq!(extract_type(data), "[message]");
    }

    #[test]
    fn test_extract_type_non_json() {
        assert_eq!(extract_type("hello world"), "[text]");
    }

    #[test]
    fn test_is_base64_binary_short() {
        assert!(!is_base64_binary("SGVsbG8="));
    }

    #[test]
    fn test_is_base64_binary_long() {
        let long_b64 = "A".repeat(2048);
        assert!(is_base64_binary(&long_b64));
    }

    #[test]
    fn test_is_base64_binary_not_b64() {
        let long_text = "Hello, world! This is not base64. ".repeat(100);
        assert!(!is_base64_binary(&long_text));
    }

    #[test]
    fn test_has_binary_content_in_json() {
        let b64 = "A".repeat(2048);
        let data = format!(r#"{{"type":"audio","data":"{}"}}"#, b64);
        assert!(has_binary_content(&data));
    }

    #[test]
    fn test_has_binary_content_no_binary() {
        let data = r#"{"type":"session.update","session":{"model":"gpt-4"}}"#;
        assert!(!has_binary_content(data));
    }

    #[test]
    fn test_extract_delta() {
        let data = r#"{"type":"response.audio_transcript.delta","delta":"Hello"}"#;
        assert_eq!(extract_delta(data), Some("Hello".to_string()));
    }

    #[test]
    fn test_extract_delta_no_field() {
        let data = r#"{"type":"session.update"}"#;
        assert_eq!(extract_delta(data), None);
    }

    #[test]
    fn test_preview_message_binary() {
        let b64 = "A".repeat(2048);
        let data = format!(r#"{{"type":"audio","data":"{}"}}"#, b64);
        let preview = preview_message(&data, 80);
        assert!(preview.contains("[binary"));
        assert!(!preview.contains(&b64));
    }

    #[test]
    fn test_preview_message_normal() {
        let data = r#"{"type":"session.update","model":"gpt-4"}"#;
        assert_eq!(preview_message(data, 80), data);
    }

    #[test]
    fn test_group_messages_basic() {
        let msgs: Vec<(WsDirection, &str, u64)> = vec![
            (WsDirection::Send, r#"{"type":"session.update"}"#, 25),
            (WsDirection::Recv, r#"{"type":"session.created"}"#, 100),
            (WsDirection::Recv, r#"{"type":"session.created"}"#, 100),
        ];
        let groups = group_messages(&msgs);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].msg_indices.len(), 1);
        assert_eq!(groups[1].msg_indices.len(), 2); // two consecutive recv with same type
    }

    #[test]
    fn test_group_messages_delta_merge() {
        let msgs: Vec<(WsDirection, &str, u64)> = vec![
            (WsDirection::Recv, r#"{"type":"response.audio_transcript.delta","delta":"Hi"}"#, 50),
            (WsDirection::Recv, r#"{"type":"response.audio_transcript.delta","delta":" there"}"#, 50),
            (WsDirection::Recv, r#"{"type":"response.audio_transcript.delta","delta":"!"}"#, 50),
        ];
        let groups = group_messages(&msgs);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].merged_delta, Some("Hi there!".to_string()));
        assert_eq!(groups[0].msg_indices.len(), 3);
    }

    #[test]
    fn test_group_messages_direction_break() {
        let msgs: Vec<(WsDirection, &str, u64)> = vec![
            (WsDirection::Send, r#"{"type":"request"}"#, 10),
            (WsDirection::Recv, r#"{"type":"request"}"#, 10),
        ];
        let groups = group_messages(&msgs);
        assert_eq!(groups.len(), 2); // different direction = different group
    }

    #[test]
    fn test_group_messages_interleaved_types() {
        let msgs: Vec<(WsDirection, &str, u64)> = vec![
            (WsDirection::Recv, r#"{"type":"a.delta","delta":"x"}"#, 10),
            (WsDirection::Recv, r#"{"type":"a.delta","delta":"y"}"#, 10),
            (WsDirection::Recv, r#"{"type":"done"}"#, 10),
            (WsDirection::Recv, r#"{"type":"a.delta","delta":"z"}"#, 10),
        ];
        let groups = group_messages(&msgs);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].merged_delta, Some("xy".to_string()));
        assert_eq!(groups[2].merged_delta, Some("z".to_string()));
    }
}
```

- [ ] **Step 2: Add module to domain/mod.rs**

In `src/domain/mod.rs`, add after `pub mod sse_merge;`:

```rust
pub mod ws_chat;
```

- [ ] **Step 3: Run tests**

Run: `cargo test domain::ws_chat`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/domain/ws_chat.rs src/domain/mod.rs
git commit -m "feat(ws): add Chat view grouping logic with type extraction and binary detection"
```

---

### Task 3: Render WS Chat view in detail panel

**Files:**
- Modify: `src/ui/network/detail.rs:567-640` (WS Messages section)

- [ ] **Step 1: Add imports**

At the top of `src/ui/network/detail.rs`, add:

```rust
use crate::domain::ws_chat;
```

- [ ] **Step 2: Clear ws_pill_line at top of draw_network_detail**

After `app.layout.sse_pill_line = None;` (around line 67), add:

```rust
    app.layout.ws_pill_line = None;
```

- [ ] **Step 3: Replace WS Messages section rendering**

Replace the entire WS Messages section (from `// ── WebSocket Messages ──` at line 567 to the closing brace at line 640, just before `// ── Error ──`) with:

```rust
    // ── WebSocket Messages ──
    if entry.protocol == Protocol::Ws && !entry.ws_messages.is_empty() {
        let sent = entry.ws_messages.iter().filter(|m| m.direction == WsDirection::Send).count();
        let recv = entry.ws_messages.iter().filter(|m| m.direction == WsDirection::Recv).count();
        let sec_name = format!("Messages ({}\u{2191} {}\u{2193})", sent, recv);
        let sec_key = "WS Messages";
        let is_collapsed = app.network.collapsed_sections.contains(sec_key);

        // Header with Chat/Raw pills
        {
            let chat_pill = if app.network.ws_chat_mode {
                Span::styled(" Chat ", Style::default().fg(MANTLE).bg(SAPPHIRE).add_modifier(Modifier::BOLD))
            } else {
                Span::styled(" Chat ", Style::default().fg(OVERLAY0).bg(SURFACE0))
            };
            let raw_pill = if !app.network.ws_chat_mode {
                Span::styled(" Raw ", Style::default().fg(MANTLE).bg(SAPPHIRE).add_modifier(Modifier::BOLD))
            } else {
                Span::styled(" Raw ", Style::default().fg(OVERLAY0).bg(SURFACE0))
            };
            let icon = if is_collapsed { "\u{25b6}" } else { "\u{25bc}" };
            let header_text = format!(" {} {}  ", icon, sec_name);
            all_lines.push(Line::from(vec![
                Span::styled(header_text.clone(), Style::default().fg(SAPPHIRE).add_modifier(Modifier::BOLD)),
                chat_pill,
                Span::raw(" "),
                raw_pill,
            ]));
            app.layout.ws_pill_line = Some((all_lines.len() - 1, header_text.len()));
            section_line_map.push(Some(sec_key.to_string()));
            json_click_map.push(None);
        }

        if !is_collapsed {
            if app.network.ws_chat_mode {
                // ── Chat mode ──
                let msgs: Vec<(crate::domain::network::WsDirection, &str, u64)> = entry
                    .ws_messages
                    .iter()
                    .map(|m| (m.direction, m.data.as_str(), m.size))
                    .collect();
                let groups = ws_chat::group_messages(&msgs);

                for (gi, group) in groups.iter().enumerate() {
                    let group_key = format!("WS_GROUP#{}", gi);
                    let group_collapsed = app.network.collapsed_sections.contains(&group_key);
                    let (arrow, color) = match group.direction {
                        WsDirection::Send => ("\u{2192}", GREEN),  // →
                        WsDirection::Recv => ("\u{2190}", BLUE),   // ←
                    };
                    let is_recv = group.direction == WsDirection::Recv;
                    let indent = if is_recv { "          " } else { " " };
                    let count = group.msg_indices.len();

                    // Group header line
                    let count_str = if count > 1 { format!(" (\u{00d7}{})", count) } else { String::new() };

                    if group.is_binary && count > 1 {
                        // Binary group: single line summary
                        let size_str = ws_chat::preview_message("", 0); // not used, manual format
                        let total_kb = group.total_size as f64 / 1024.0;
                        all_lines.push(Line::from(Span::styled(
                            format!("{}{} {}{} [binary {:.1}KB]", indent, arrow, group.type_label, count_str, total_kb),
                            Style::default().fg(color),
                        )));
                        section_line_map.push(Some(group_key.clone()));
                        json_click_map.push(None);
                    } else if group.merged_delta.is_some() {
                        // Delta group: show type + count, then merged text
                        let toggle = if group_collapsed { "\u{25b6}" } else { "\u{25bc}" };
                        all_lines.push(Line::from(Span::styled(
                            format!("{}{} {} {}{}", indent, arrow, toggle, group.type_label, count_str),
                            Style::default().fg(color),
                        )));
                        section_line_map.push(Some(group_key.clone()));
                        json_click_map.push(None);

                        if !group_collapsed {
                            if let Some(ref merged) = group.merged_delta {
                                let text_indent = if is_recv { "            " } else { "   " };
                                if merged.is_empty() {
                                    all_lines.push(Line::from(Span::styled(
                                        format!("{}(no delta content)", text_indent),
                                        Style::default().fg(OVERLAY0),
                                    )));
                                    section_line_map.push(None);
                                    json_click_map.push(None);
                                } else {
                                    let wrap_w = inner_w.saturating_sub(text_indent.len());
                                    for wl in wrap_text(merged, wrap_w, 200) {
                                        all_lines.push(Line::from(Span::styled(
                                            format!("{}{}", text_indent, wl),
                                            Style::default().fg(TEXT),
                                        )));
                                        section_line_map.push(None);
                                        json_click_map.push(None);
                                    }
                                }
                            }
                        }
                    } else if count > 10 && group_collapsed {
                        // Large non-delta group: collapsed summary
                        all_lines.push(Line::from(Span::styled(
                            format!("{}{} \u{25b6} {}{}", indent, arrow, group.type_label, count_str),
                            Style::default().fg(color),
                        )));
                        section_line_map.push(Some(group_key.clone()));
                        json_click_map.push(None);
                    } else {
                        // Individual messages (small group or expanded)
                        if count > 10 {
                            // Collapsible header for large groups
                            all_lines.push(Line::from(Span::styled(
                                format!("{}{} \u{25bc} {}{}", indent, arrow, group.type_label, count_str),
                                Style::default().fg(color),
                            )));
                            section_line_map.push(Some(group_key.clone()));
                            json_click_map.push(None);
                        }

                        // Render individual messages
                        let render_indices = if count > 10 && group_collapsed {
                            &[] as &[usize]
                        } else if count > 10 {
                            &group.msg_indices[..]
                        } else {
                            &group.msg_indices[..]
                        };

                        for &mi in render_indices {
                            if let Some(msg) = entry.ws_messages.get(mi) {
                                let msg_key = format!("WS#{}", mi);
                                let msg_collapsed = app.network.collapsed_sections.contains(&msg_key);
                                let toggle = if msg_collapsed { "\u{25b6}" } else { "\u{25bc}" };
                                let preview_w = inner_w.saturating_sub(indent.len() + 6);

                                // Type + preview on one line
                                let type_label = ws_chat::extract_type(&msg.data);
                                let preview = if msg_collapsed {
                                    ws_chat::preview_message(&msg.data, preview_w.saturating_sub(type_label.len() + 3))
                                } else {
                                    String::new()
                                };

                                all_lines.push(Line::from(vec![
                                    Span::styled(
                                        format!("{}{} {} {} ", indent, arrow, toggle, type_label),
                                        Style::default().fg(color),
                                    ),
                                    Span::styled(preview, Style::default().fg(SUBTEXT0)),
                                ]));
                                section_line_map.push(Some(msg_key.clone()));
                                json_click_map.push(None);

                                if !msg_collapsed {
                                    let json_indent = if is_recv { "            " } else { "   " };
                                    // Replace binary in display
                                    let display_data = if ws_chat::has_binary_content(&msg.data) {
                                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&msg.data) {
                                            let replaced = ws_chat::preview_message(&msg.data, usize::MAX);
                                            replaced
                                        } else {
                                            msg.data.clone()
                                        }
                                    } else {
                                        msg.data.clone()
                                    };
                                    render_json_section(
                                        &mut all_lines,
                                        &mut section_line_map,
                                        &mut json_click_map,
                                        &display_data,
                                        &format!("ws_{}", mi),
                                        &mut app.network.json_viewer_states,
                                        inner_w,
                                    );
                                }
                            }
                        }
                    }
                }
            } else {
                // ── Raw mode (original behavior) ──
                for (i, msg) in entry.ws_messages.iter().enumerate() {
                    let (arrow, color) = match msg.direction {
                        WsDirection::Send => ("\u{2192}", GREEN),
                        WsDirection::Recv => ("\u{2190}", BLUE),
                    };
                    let msg_key = format!("WS#{}", i);
                    let msg_collapsed = app.network.collapsed_sections.contains(&msg_key);
                    let prefix = if msg_collapsed { "\u{25b6}" } else { "\u{25bc}" };
                    let ws_prefix_text = format!("  {} {} ", prefix, arrow);
                    let ws_preview_w = inner_w.saturating_sub(ws_prefix_text.len() + 1);
                    all_lines.push(Line::from(vec![
                        Span::styled(ws_prefix_text, Style::default().fg(color)),
                        Span::styled(
                            if msg_collapsed {
                                if msg.data.len() > ws_preview_w {
                                    format!("{}...", &msg.data[..ws_preview_w.saturating_sub(3)])
                                } else {
                                    msg.data.clone()
                                }
                            } else {
                                format!("({} bytes)", msg.size)
                            },
                            Style::default().fg(SUBTEXT0),
                        ),
                    ]));
                    section_line_map.push(Some(msg_key.clone()));
                    json_click_map.push(None);
                    if !msg_collapsed {
                        render_json_section(
                            &mut all_lines,
                            &mut section_line_map,
                            &mut json_click_map,
                            &msg.data,
                            &format!("ws_{}", i),
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
Expected: Compiles.

- [ ] **Step 5: Commit**

```bash
git add src/ui/network/detail.rs
git commit -m "feat(ws): render Chat view with directional layout, grouping, and binary detection"
```

---

### Task 4: Wire up WS pill clicks and group expand/collapse

**Files:**
- Modify: `src/event.rs`

- [ ] **Step 1: Add WS pill click handling**

In `src/event.rs`, in the detail panel left-click handler, after the SSE pill click block (the `if let Some((pill_line, header_w)) = app.layout.sse_pill_line` block), add:

```rust
                        // Check WS pill clicks (Chat/Raw toggle)
                        if let Some((pill_line, header_w)) = app.layout.ws_pill_line {
                            if line_idx == pill_line {
                                let click_x = (x.saturating_sub(app.layout.net_detail_x + 1)) as usize;
                                let chat_start = header_w;
                                let chat_end = chat_start + " Chat ".len();
                                let raw_start = chat_end + 1;
                                let raw_end = raw_start + " Raw ".len();
                                if click_x >= chat_start && click_x < chat_end {
                                    app.network.ws_chat_mode = true;
                                    return;
                                } else if click_x >= raw_start && click_x < raw_end {
                                    app.network.ws_chat_mode = false;
                                    return;
                                }
                                // Fall through to section toggle
                            }
                        }
```

- [ ] **Step 2: Add WS group expand/collapse handling**

In the SSE-specific section keys block (before the generic section toggle), add handling for `WS_GROUP#N`:

```rust
                            // WS group expand/collapse in Chat mode
                            if let Some(idx_str) = section_key.strip_prefix("WS_GROUP#") {
                                if idx_str.parse::<usize>().is_ok() {
                                    let key = section_key.clone();
                                    if app.network.collapsed_sections.contains(&key) {
                                        app.network.collapsed_sections.remove(&key);
                                    } else {
                                        app.network.collapsed_sections.insert(key);
                                    }
                                    return;
                                }
                            }
```

- [ ] **Step 3: Pre-collapse delta groups on first render**

In `src/ui/network/detail.rs`, inside the Chat mode rendering, after building `groups`, add pre-collapse logic for delta groups. Insert right after `let groups = ws_chat::group_messages(&msgs);`:

```rust
                // Pre-collapse delta groups with >1 message on first render
                let ws_init_key = "_ws_chat_init";
                if !app.network.collapsed_sections.contains(ws_init_key) {
                    app.network.collapsed_sections.insert(ws_init_key.to_string());
                    // Don't pre-collapse anything — groups default to expanded
                    // (delta groups show merged text, which is the main value)
                }
```

Actually, delta groups should show their merged text by default (not collapsed). Large non-delta groups (>10 messages) should be collapsed. Add after building groups:

```rust
                // Pre-collapse large non-delta groups on first render
                let ws_init_key = "_ws_chat_init";
                if !app.network.collapsed_sections.contains(ws_init_key) {
                    app.network.collapsed_sections.insert(ws_init_key.to_string());
                    for (gi, group) in groups.iter().enumerate() {
                        if group.msg_indices.len() > 10 && group.merged_delta.is_none() && !group.is_binary {
                            app.network.collapsed_sections.insert(format!("WS_GROUP#{}", gi));
                        }
                    }
                }
```

- [ ] **Step 4: Build and test**

Run: `cargo build`
Expected: Compiles.

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/event.rs src/ui/network/detail.rs
git commit -m "feat(ws): wire up Chat/Raw pill clicks and group expand/collapse"
```

---

### Task 5: WS Copy Response support

**Files:**
- Modify: `src/event.rs:904-940` (copy_response function)

- [ ] **Step 1: Add WS copy response logic**

In `src/event.rs`, in the `copy_response` function, after the SSE block (before the `let body = entry.response_body...` line), add:

```rust
    // WS: copy chat summary (if in chat mode) or all message data
    if entry.protocol == crate::domain::network::Protocol::Ws && !entry.ws_messages.is_empty() {
        let text = if app.network.ws_chat_mode {
            let msgs: Vec<(crate::domain::network::WsDirection, &str, u64)> = entry
                .ws_messages
                .iter()
                .map(|m| (m.direction, m.data.as_str(), m.size))
                .collect();
            let groups = crate::domain::ws_chat::group_messages(&msgs);
            let mut lines = Vec::new();
            for group in &groups {
                let arrow = match group.direction {
                    crate::domain::network::WsDirection::Send => "→",
                    crate::domain::network::WsDirection::Recv => "←",
                };
                if group.is_binary {
                    let total_kb = group.total_size as f64 / 1024.0;
                    lines.push(format!("{} {} [binary {:.1}KB]", arrow, group.type_label, total_kb));
                } else if let Some(ref merged) = group.merged_delta {
                    lines.push(format!("{} {} (×{})", arrow, group.type_label, group.msg_indices.len()));
                    if !merged.is_empty() {
                        lines.push(merged.clone());
                    }
                } else {
                    for &mi in &group.msg_indices {
                        if let Some(msg) = entry.ws_messages.get(mi) {
                            let preview = crate::domain::ws_chat::preview_message(&msg.data, 200);
                            lines.push(format!("{} {}", arrow, preview));
                        }
                    }
                }
            }
            lines.join("\n")
        } else {
            entry.ws_messages.iter().map(|m| m.data.as_str()).collect::<Vec<_>>().join("\n")
        };
        if text.is_empty() {
            app.show_status("No WS data".to_string());
            return;
        }
        let msg = copy_to_clipboard(&text);
        app.show_status(format!("Response {}", msg));
        return;
    }
```

- [ ] **Step 2: Build and test**

Run: `cargo build`
Expected: Compiles.

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add src/event.rs
git commit -m "feat(ws): Copy Response copies chat summary in Chat mode"
```

---

### Task 6: Remove WS dump code and update docs

**Files:**
- Modify: `src/domain/network_store.rs` (remove dump)
- Modify: `README.md`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Remove WS dump code from network_store.rs**

In `src/domain/network_store.rs`, in `handle_ws_msg`, remove the entire `// Dump WS message to file for analysis` block (the `{ use std::io::Write; ... }` block).

- [ ] **Step 2: Update README.md**

In the Network 功能 section, update WebSocket Messages line:

```markdown
  - WebSocket Messages（Chat 对话流视图：按方向分列、type 标签、delta 消息自动拼接、binary 数据智能折叠；可切换 Raw 原始列表）
```

In the Network keyboard shortcuts table, add:

```markdown
| 点击 `[Chat]/[Raw]` | WS 消息：切换对话流视图和原始列表 |
```

- [ ] **Step 3: Update CLAUDE.md**

In the domain layer section, add after `sse_merge.rs`:

```markdown
  - `ws_chat.rs` — WS Chat View utilities: `extract_type` (scans type/event/action/op/cmd/method keys), `has_binary_content` (detects base64 >1KB), `group_messages` (groups consecutive same-type messages, merges delta fields), `preview_message` (replaces binary with size labels)
```

In NetworkState fields, add `ws_chat_mode`.

- [ ] **Step 4: Build and test**

Run: `cargo build`
Expected: Compiles.

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/domain/network_store.rs README.md CLAUDE.md
git commit -m "feat(ws): remove dump code, update README and CLAUDE.md with Chat View docs"
```
