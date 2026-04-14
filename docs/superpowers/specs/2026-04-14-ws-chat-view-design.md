# WS Chat View Design Spec

## Goal

Add a "Chat" view to the WebSocket Messages section in the Network detail panel that presents WS messages as a directional conversation flow (send left, recv right), with intelligent grouping of delta messages and binary detection, while keeping the original "Raw" list as a switchable fallback.

## Problem

WebSocket connections (especially LLM Realtime APIs) generate hundreds of messages. The current flat list with individual expand/collapse is unusable when:
- Audio binary data (base64) floods the list with unreadable content
- Incremental delta messages (e.g., `audio_transcript.delta` × 52) need to be mentally reassembled
- Send/recv direction is only indicated by a small arrow icon, making it hard to follow the conversation flow

## Architecture

### Mode Switching

WS Messages section header shows `[Chat] [Raw]` pills (same pattern as SSE `[Events] [Merged]`):

- **Chat mode (default)**: Directional conversation flow with smart grouping
- **Raw mode**: Current behavior — flat list of individual collapsible messages

No persistent rules needed (unlike SSE Merged). Chat is always the default for WS entries; Raw is the fallback for inspecting individual messages.

### State

In `NetworkState`:
```
ws_chat_mode: bool  // true = Chat view (default), false = Raw view
```

In `LayoutCache`:
```
ws_pill_line: Option<(usize, usize)>  // same pattern as sse_pill_line
```

No rule persistence needed — Chat mode is stateless, always available for any WS entry.

## Chat View Rendering

### Message Layout

Send messages align left (green), recv messages align right (blue):

```
 → session.update
   {modalities: [audio, text], ...}

                    ← session.created
                    {id: sess_xxx, model: gpt-realtime...}

 → conversation.item.create
   {role: user, content: [...]}

                    ← audio_transcript.delta (×52)
                    Hi there! What can I get started
                    for you today?

                    ← response.done
                    {status: completed, usage: {...}}
```

Each message consists of:
1. **Type label line**: Direction arrow (`→`/`←`) + type value, colored by direction (GREEN for send, BLUE for recv)
2. **Content preview**: Indented, truncated to ~2 lines. Full JSON viewable by clicking to expand.

### Type Extraction

Scan each message's JSON for a "type" field. Check these keys in order:
1. `type`
2. `event`
3. `action`
4. `op`
5. `cmd`
6. `method`

Use the first match. If none found, display `[message]` as the type label.

If the message is not valid JSON, display `[text]` and show raw content.

### Binary Detection

A JSON string value is classified as binary if ALL of:
- Length > 1024 characters
- Matches base64 pattern: `^[A-Za-z0-9+/=\s]+$`

Binary values are replaced in preview with `[binary {size}]` where size is the decoded byte count (len * 3/4).

This applies to any field in the message, not just known fields like `audio` or `delta`. The detection is generic.

### Delta Message Grouping

When consecutive messages share the same `type` AND direction, they are grouped:

**Rule 1 — Delta concatenation**: If the type name contains "delta" (case-insensitive) AND the messages contain a field named `delta` with a string value:
- Group all consecutive messages into one entry
- Concatenate all `delta` field values
- Display: `← {type} (×{count})` followed by the concatenated text
- Click to expand individual messages

**Rule 2 — Binary group collapse**: If the grouped messages are all binary (detected by binary detection above):
- Display: `→ {type} (×{count}) [binary {total_size}]`
- Single line, no content preview
- Click to expand individual messages

**Rule 3 — Count-only collapse**: For other consecutive same-type messages (NOT delta, NOT binary):
- Display each message individually (don't concat — they are independent events)
- But if count > 10, collapse into `← {type} (×{count})` with expand option

### Non-JSON Messages

If a message is not valid JSON:
- Display direction arrow + `[text]`
- Show raw content as preview (truncated)
- No type extraction, no grouping

### Expand/Collapse

- Grouped messages (delta/binary/count) are collapsed by default
- Click the group header to expand into individual messages
- Each individual message can be further expanded to show full JSON (via existing json_viewer)
- Use `collapsed_sections` with keys like `"WS_GROUP#{start_idx}"` for group state

## Click Interactions

### Pill clicks (on header line)
- Click `[Chat]` → `ws_chat_mode = true`
- Click `[Raw]` → `ws_chat_mode = false`
- Same X-position detection pattern as SSE pills

### Message clicks (in Chat mode)
- Click group header → expand/collapse the group
- Click individual message (when expanded) → expand/collapse JSON detail

### Keyboard
- No j/k override needed (unlike SSE field selection) — normal scroll works fine for Chat view
- Esc: not needed (no rule to clear)

## Edge Cases

1. **WS with 0 messages**: Don't show Chat/Raw pills. Show "No messages yet" placeholder.
2. **All messages are binary**: Chat view shows only collapsed binary groups. Still useful to see the flow of send/recv.
3. **No `type` field in any message**: All messages show as `[message]`. Chat view still provides directional layout value.
4. **Non-JSON messages**: Displayed inline with `[text]` label. Grouping skipped for non-JSON.
5. **Mixed JSON and non-JSON**: JSON messages get type extraction, non-JSON get `[text]`. Both render in chronological order.
6. **Single very long message**: Content preview truncated to 2 lines. Click to expand.
7. **Active connection (messages arriving)**: Renderer re-runs on tick, new messages appear at bottom. Auto-scroll behavior same as current.
8. **Delta group still growing (active stream)**: Group count updates live. Concatenated text grows as new deltas arrive.
9. **base64 detection false positive**: Threshold at 1KB minimizes risk. Worst case: a long alphanumeric string shows as `[binary]` — user can switch to Raw mode to see actual content.
10. **Interleaved types**: `delta, delta, done, delta, delta` → two separate delta groups with `done` in between. Groups are strictly consecutive.

## Copy Response Behavior

For WS entries, Copy Response (`y` key):
- **Chat mode**: Copy a readable summary — all non-binary messages' content, preserving direction markers
- **Raw mode**: Copy all messages' raw data joined by newlines (current behavior)

## Files to Modify

- `src/app.rs` — Add `ws_chat_mode` field to `NetworkState`, `ws_pill_line` to `LayoutCache`
- `src/domain/ws_chat.rs` — New module: type extraction, binary detection, delta grouping logic
- `src/ui/network/detail.rs` — WS Messages section rendering (Chat mode + Raw mode with pills)
- `src/event.rs` — Pill click handling, group expand/collapse clicks
