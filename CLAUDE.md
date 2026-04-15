# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

flog is a terminal-native log viewer + network inspector for Flutter developers, written in Rust. It connects to Flutter apps via VM Service WebSocket, ADB logcat, or stdin pipe and displays structured, filterable logs and network requests in an interactive TUI.

## Build & Test Commands

```bash
cargo build                    # Debug build
cargo build --release          # Release build
cargo test                     # Run all tests
cargo test --test ws_connect_test -- --nocapture  # Single test with output
cargo clippy                   # Lint
cargo fmt                      # Format
cargo install --path .         # Install to ~/.cargo/bin/
```

## Architecture

Four-layer architecture with strict dependency direction: `ui → app → domain ← parser/input`.

### Layers (all under `src/`)

- **`domain/`** — Pure data types with zero UI dependencies
  - `entry.rs` — `LogEntry`, `LogLevel`, `InputSource` types
  - `filter.rs` — `FilterState` with level/tag/search filtering, pre-compiled regex
  - `store.rs` — Ring-buffer log storage (100K cap, drains oldest 10% when full, folds consecutive duplicates)
  - `network.rs` — `NetworkEntry`, `Protocol` (Http/Sse/Ws), `NetworkStatus`, `SseChunk`, `WsMessage`, `FlogNetMessage`, `EntrySource` (App/Replay/Mocked)
  - `network_store.rs` — Network request storage (10K cap), processes flog_net protocol messages
  - `network_filter.rs` — `NetworkFilter` with `ProtocolFilter`, `MethodFilter`, `StatusFilter`
  - `mock.rs` — `MockRule`, `MockRuleStore` — interceptor-based mock system (URL pattern matching, method filter, status code, response body, delay, enable/toggle)
  - `sse_merge.rs` — SSE Merged View utilities: `extract_field_paths` (scans all chunks for leaf-string JSON paths), `resolve_path`, `auto_detect_field` (knows OpenAI/Claude streaming patterns), `merge_field` (concatenates a field across chunks)
  - `ws_chat.rs` — WS Chat View utilities: `extract_type` (scans type/event/action/op/cmd/method keys), `has_binary_content` (detects base64 >1KB), `group_messages` (groups consecutive same-type/direction messages, merges delta fields), `preview_message` (replaces binary with size labels)

- **`parser/`** — Strategy-pattern log format parser chain, tried in order:
  1. `structured.rs` — Structured `[LEVEL][Tag] message` format
  2. `generic.rs` — Flutter standard patterns (`I/flutter`, VM Service timestamps, exception blocks)
  3. `keyword.rs` — Fallback heuristic scanning for level keywords
  4. `network.rs` — Parses `[INFO][flog_net]` tagged lines as `FlogNetMessage` JSON
  - Unrecognized lines get SYSTEM level (never dropped)

- **`input/`** — Direct Socket communication layer
  - `protocol.rs` — `ClientMessage` (Hello/Log/Net), `ServerMessage` (MockSync/Replay), `ClientInfo`, serde-tagged JSON protocol
  - `connector.rs` — `connect()` (tokio-tungstenite WS client), `ConnectorHandle` (downstream message sender), `ConnectorEvent` (Connected/Disconnected/Message)

- **`transport/`** — Device discovery and platform-specific connectivity
  - `device_monitor.rs` — `DeviceMonitor` polls `flutter devices --machine`, `FlutterDevice`, `ConnectionMethod` (Localhost/AdbForward/Usbmuxd)
  - `adb.rs` — `setup_forward()` / `remove_forward()` for Android adb port forwarding
  - `usbmuxd.rs` — macOS usbmuxd protocol client for iOS real device USB connectivity (plist over Unix socket)

- **`ui/`** — ratatui-based TUI with Catppuccin Macchiato theme, dual-tab architecture
  - `mod.rs` — Top-level dispatcher, shared palette constants, utility functions
  - `tab_bar.rs` — Tab bar renderer (▤ Logs / ⇄ Network)
  - `json_viewer.rs` — **Shared** collapsible JSON tree component (bracket formatter, depth-aware coloring, fold/unfold)
  - `logs/mod.rs` — Logs view (toolbar, log list with level colors/tag pills, timeline, status bar)
  - `logs/detail.rs` — Log detail panel using json_viewer
  - `logs/highlight.rs` — Auto-highlight (HTTP methods, status codes, URLs, durations)
  - `logs/timeline.rs` — Timeline heatmap
  - `logs/stats.rs` — Statistics view
  - `network/mod.rs` — Network view (toolbar with filter pills, request table, status bar)
  - `network/detail.rs` — Network detail panel (General, Query Params, Headers, Body, SSE Events with Merged View, WS Messages)
  - `network/filter.rs` — Network toolbar renderer (2-line: search + protocol/method/status pills)
  - `network/stats.rs` — Network statistics overlay (latency percentiles, top-5 slowest, status distribution, per-domain breakdown)
  - `network/mock_rules.rs` — Mock rules side panel + edit overlay (create/edit/toggle/delete rules, JSON body editor)
  - `text_editor.rs` — Multi-line text editor component (cursor, editing, viewport scroll) — used by mock rule body editor
  - `source_select.rs` — Connection status display
  - `help.rs` — Comprehensive help overlay

### Key Top-Level Modules

- `app.rs` — Central state machine
  - `AppMode`: Normal, Search, TagFilter, Help, Stats, SourceSelect
  - `ViewTab`: Logs, Network
  - `SseMergeRule`, `SsePathSegment` — SSE Merged View rule types (field_path + display)
  - `NetworkState`: selected, scroll_offset, auto_scroll, filter, collapsed_sections, json_viewer_states, sse_merge_rules, sse_merged_mode, sse_merged_field_idx, ws_chat_mode
  - `DetailState`: scroll, header_lines, viewer_state (JsonViewerState)
- `event.rs` — Keyboard/mouse event dispatch (tab bar clicks, detail panel scroll, filter pill clicks)
- `cli.rs` — CLI argument parsing (clap): `--port`, `--level`, `--tag`
- `session.rs` — Session persistence (active_tab, filters, bookmarks)
- `main.rs` — Tokio async entry point, starts WS server, spawns server event processor, runs TUI render loop

### Data Flow

1. flog_dart starts WS server on port 9753 inside the Flutter App
2. flog TUI discovers devices via `flutter devices --machine` (every 5 seconds)
3. For each device, flog establishes connection via appropriate transport:
   - macOS / iOS sim → `ws://localhost:9753`
   - Android → `adb forward` then `ws://localhost:{local_port}`
   - iOS real → usbmuxd Connect then WS upgrade over tunnel
4. flog_dart sends `Hello` on connection → flog receives `ConnectorEvent::Connected`
5. flog_dart sends `Log` messages → `dispatch_client_message()` → `LogStore`
6. flog_dart sends `Net` messages → `dispatch_client_message()` → `NetworkStore`
7. Mock system: rules created in TUI → `ConnectorHandle.send_mock_sync()` → flog_dart `FlogMockInterceptor.updateRules()`
8. Replay: user triggers from detail → `ConnectorHandle.send_replay()` → flog_dart re-executes request
9. SSE Merged View / WS Chat View: UI-only features, no transport involvement
10. Renderer reads filtered indices, renders to terminal

### Concurrency Model

Tokio multi-threaded runtime. Device monitor + connector loop run in background task, producing `ConnectorEvent`s. Connector reader/writer tasks handle WS communication. Main thread polls terminal events. App state is behind `Arc<Mutex<App>>`.

### Scroll Model

Both Logs and Network use the same pattern:
- `move_up/down(n)` — viewport scroll (mouse wheel, PageUp/Down), moves offset + selected
- `select_up/down(n)` — cursor move (j/k), moves only selected
- `go_top/go_bottom` — Home/End
- **Renderer is the scroll authority** — clamps offset, detects bottom for auto_scroll

## flog_dart Dart Package

`flog_logger/` contains the Dart companion package published as [flog_dart](https://pub.dev/packages/flog_dart) on pub.dev.

- `FlogLogger` — Structured `[LEVEL][Tag] message` logging
- `FlogDio` — Drop-in `Dio` replacement that auto-instruments HTTP for Network Inspector. Inserts `FlogMockInterceptor` + `FlogHttpInterceptor` automatically. Also provides `sse()` convenience method for SSE streams.
- `FlogHttpInterceptor` — Dio interceptor for HTTP request/response logging (⚠ must be added BEFORE response-modifying interceptors)
- `FlogMockInterceptor` — Dio interceptor that intercepts requests matching mock rules synced from the flog TUI via VM Service extension (`ext.flog.syncMockRules`). Resolves with canned responses without hitting the network.
- `FlogSseParser` — SSE stream wrapper with chunk-level logging
- `FlogWebSocket` — WebSocket wrapper with send/recv logging
- Protocol: `[INFO][flog_net] {JSON}` via `print()` + `developer.log()` (for iOS real device via VM Service Logging stream)

### Tree-shaking / `flogEnabled`

`flogEnabled` is a compile-time constant: `true` in debug, `false` in release (`dart.vm.product`). When `false`, all flog code is eliminated by AOT tree-shaking — zero overhead in production. Can be overridden with `-DFL0G_ENABLED=true/false`.

### Mock System

Mock rules are created in the flog TUI (Network tab → `M` to open mock rules panel). Rules define URL pattern, optional method filter, status code, response body, and optional delay. The TUI syncs rules to the running Dart app via VM Service extension `ext.flog.syncMockRules`. `FlogMockInterceptor` (inserted automatically by `FlogDio`) intercepts matching requests and resolves with the canned response. Mocked requests are still logged and appear in the Network Inspector tagged as "Mocked".

## CI/CD

GitHub Actions (`release.yml`) builds on tag push (`v*`) for 5 targets: macOS x86_64/aarch64, Linux x86_64/aarch64, Windows x86_64. Artifacts are packaged and uploaded to GitHub Releases.
