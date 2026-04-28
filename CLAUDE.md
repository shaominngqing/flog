# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

**Before non-trivial changes, read `docs/ARCHITECTURE.md` first, then `docs/MODULES.md` to locate the right file, and `docs/PROTOCOL.md` for wire-format detail. Process rules live in `docs/CONTRIBUTING.md`; code-style rules in `docs/CODING_STYLE.md`; UI framework boundaries in `docs/UI_FRAMEWORK_BOUNDARY.md`.**

## 🛡️ Design rules that outlive the codebase

These rules guard against regressions that would make future work (including a planned GUI frontend) expensive. They apply to every commit, human or AI:

1. **UI layer only reads `App`, never writes business state.** Renderers may only write `LayoutCache`. Business mutations belong to `event/apply.rs` or `App` methods.
2. **`domain/` `parser/` `input/` `transport/` `app/` must NEVER `use ratatui` / `use crossterm` / any UI framework.** This is the UI-agnostic core; it must stay portable to a future GUI. `event/` may depend on exactly one input framework (currently `crossterm`), but must translate framework signals to neutral `ClickRegion` / (future) `KeyAction` enums before they reach `App` mutations.
3. **Input events must go through the two-phase detect/apply pattern.** `detect_click_region(&App, x, y) -> Option<ClickRegion>` is pure and testable. `apply_click_region(&mut App, region, class)` performs mutations. Never interleave the two. See Phase 3 Step 3.6.
4. **New state must be derivable by pure functions without assuming a TUI cell grid.** If your state requires a `Frame` or `Rect` to make sense, it belongs in `ui/` or `LayoutCache` (R5 of UI_FRAMEWORK_BOUNDARY.md), not in `App`.
5. **File size budget is a signal, not a law.** Under 500 lines is default. 500-800 yellow (document the reason). Over 800 red (must split). Test files (`*_tests.rs` sibling pattern) are exempt. Per `CONTRIBUTING.md §5.5`.
6. **Every B-class bug gets a red-locked characterization test.** `#[ignore = "bug: <audit-id>"]` test lands first; un-ignore in the same commit as the fix. No shadow TODOs, no silent fixes.

Violating any of these requires an audit entry and reviewer sign-off, not just a commit.

## What This Is

flog is a terminal-native log viewer + network inspector for Flutter developers, written in Rust. Flutter apps connect to flog over a direct WebSocket (port 9753+) via the `flog_dart` companion package and stream structured logs + HTTP / SSE / WebSocket traffic for display in an interactive TUI.

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
  - `network.rs` — `NetworkEntry`, `Protocol` (Http/Sse/Ws), `NetworkStatus` (Pending/Active/Completed/Failed/Orphan), `SseChunk`, `WsMessage`, `FlogNetKind` (typed `#[serde(tag="t")]` enum), `EntrySource` (App/Replay/Mocked)
  - `network_store.rs` — Network request storage (10K cap), processes flog_net protocol messages
  - `network_filter.rs` — `NetworkFilter` with `ProtocolFilter`, `MethodFilter`, `StatusFilter`
  - `mock.rs` — `MockRule`, `MockRuleStore` — interceptor-based mock system (URL pattern matching, method filter, status code, response body, delay, enable/toggle)
  - `sse_merge.rs` — SSE Merged View utilities: `extract_field_paths` (scans all chunks for leaf-string JSON paths), `resolve_path`, `auto_detect_field` (knows OpenAI/Claude streaming patterns), `merge_field` (concatenates a field across chunks)
  - `ws_chat.rs` — WS Chat View utilities: `extract_type` (scans type/event/action/op/cmd/method keys), `has_binary_content` (detects base64 >1KB), `group_messages` (groups consecutive same-type/direction messages, merges delta fields), `preview_message` (replaces binary with size labels)

- **`parser/`** — Strategy-pattern log format parser chain, tried in order (see `MultiStrategyParser::default_chain`):
  1. `structured.rs` — Structured `[LEVEL][Tag] message` + pipe-format `HH:MM:SS.mmm │ LEVEL │ Tag │ msg`
  2. `generic.rs` — Flutter standard patterns (`I/flutter`, VM Service timestamps, exception blocks)
  3. `keyword.rs` — Fallback heuristic scanning for level keywords; accepts any non-empty input so lines never silently drop
  4. `network.rs` — `try_parse_network` for `flog_net`-tagged lines carrying `FlogNetKind` JSON; called directly by `run::dispatch_client_message`, NOT through the chain
  - `util.rs` — shared ANSI stripping + regex helpers

- **`input/`** — Direct Socket communication layer
  - `protocol.rs` — `ClientMessage` (Hello/Log/Net), `ServerMessage` (MockSync/Replay/Subscribe), `ClientInfo` (with `session_id`), serde-tagged JSON protocol
  - `connector.rs` — `connect()` / `connect_stream()` (tokio-tungstenite WS client, 3s Hello-handshake timeout), `ConnectorHandle` (downstream sender with `send_mock_sync` / `send_replay` / `send_subscribe` / generic `send`), `ConnectorEvent` (Connected/Disconnected/Message)

- **`transport/`** — Device discovery and platform-specific connectivity
  - `device_monitor/` — event-driven device discovery: `mod.rs` orchestrator, `adb_source.rs` (Android via `adb track-devices`), `usbmuxd_source.rs` (iOS over macOS usbmuxd), `local_source.rs` (macOS host + iOS simulator). Shared `DeviceTracker` guarantees matching Added/Removed.
  - `resolve.rs` — pure `resolve_transport_addr(device, port) -> TransportAddr` (Localhost / AdbForward / Usbmuxd). TRANS-009.
  - `adb.rs` — `setup_forward()` / `remove_forward()` + local-port pool allocator (19753..29752)
  - `usbmuxd.rs` — macOS usbmuxd protocol client for iOS real device USB connectivity (plist over Unix socket)

- **`ui/`** — ratatui-based TUI with Catppuccin Macchiato theme, dual-tab architecture. Every renderer takes `&App`; UI never writes to app state.
  - `mod.rs` — Top-level dispatcher, shared palette constants (BASE/MANTLE/…/MAUVE/PINK/LAVENDER), utility functions (wrap_text, safe_pad)
  - `tab_bar.rs` — Tab bar renderer (▤ Logs / ⇄ Network)
  - `json_viewer/` — **Shared** collapsible JSON tree component (AST-based)
    - `tree.rs` — flat-arena tree via `serde_json::Value` + DFS flatten
    - `state.rs` — parallel `Vec<bool>` fold state indexed by node ID
    - `render/` — split into `mod.rs` (orchestration), `lines.rs` (per-line rendering), `summaries.rs` (collapsed summaries); depth-aware; DevTools-style `{k: v, …}` / `[v, …] (N)`; fixed-width ▼/▶ markers; CJK-aware truncation
    - `palette.rs` — shared Catppuccin color constants (depth-cycling key/brace colors)
    - `colorize/` — independent raw-text JSON syntax highlighter for inline JSON in log messages
  - `logs/mod.rs` — Logs view orchestrator. Submodules: `toolbar.rs` (2-row toolbar), `list.rs` (log list), `status_bar.rs`, `detail/` (mod + renderers + section), `empty_states.rs`, `jump.rs` (Jump-to-Bottom pill), `stats.rs`, `highlight.rs` (auto-highlight URLs/status codes/durations)
  - `network/mod.rs` — Network view orchestrator. Submodules: `filter.rs` (2-line toolbar), `table.rs`, `status_bar.rs`, `detail/` (general + shared + http_body + sse + ws + error), `mock_rules.rs`, `stats.rs`
  - `input_field/` — **Shared** single-line input widget (three-state bg, viewport scroll). Used by the 5 unified input fields
  - `text_editor/` — **Shared** multi-line text editor state (cursor + viewport). Used by mock rule body editor
  - `device_picker/` — Device + app picker overlay (card / modal / row / click_map / palette)
  - `help/` — Help overlay. `mod.rs` + `content/logs.rs` + `content/network.rs`

### Key Controller Modules

- `app/` — Central state machine (Phase 4: `app.rs` exploded into a directory).
  - `mod.rs` — `App` root, `AppMode { Normal | InputActive(InputField) | Help | Stats | MockRuleEdit }`, `ViewTab { Logs | Network }`, `InputField { LogSearch | LogExclude | LogTag | NetSearch | NetExclude }` with `tab()` projection
  - `state_structs.rs` — `SearchState`, `InputBuffers`, `DetailState` (scroll, viewer_state, viewer_tree, viewer_click_map, viewer_text_fingerprint), `LogsViewState` (selected, scroll_offset, auto_scroll — Phase 4 UI-003 completion), `StatsSnapshot`
  - `network_state.rs` — `NetworkState` (owns its own `filtered_indices` write-through cache + auto_scroll; symmetry with `LogsViewState`)
  - `multi_app.rs` — `ConnectedApp` + add/remove/switch invariants (UI-040)
  - `mock_edit.rs` — `MockEditState` bundle
  - `sse_merge.rs` — `SseMergeRule`, `SsePathSegment`
  - `layout_cache.rs` — `LayoutCache` (per-frame rect snapshots for mouse detection)
  - `mode.rs` / `scroll.rs` / `detail.rs` / `input_fields.rs` — helpers
- `event/` — Keyboard/mouse event dispatch (Phase 3 Step 3.6: split `event.rs` into a directory).
  - `mod.rs` — top-level fanout on `AppMode`
  - `click_region.rs` — `ClickRegion` enum (semantic click targets), `ClickClass { Single | Double }`, `ScrollDir`, `Axis`
  - `detect.rs` + `detect_net.rs` — **pure** `detect_click_region(&App, x, y) -> Option<ClickRegion>` + `classify_click`
  - `apply.rs` + `apply_status.rs` — mutation phase: `apply_click_region(&mut App, …)`
  - `keys.rs` — per-mode key handlers
  - `actions.rs` — clipboard / cURL / replay / mock-from-selected
  - `pills.rs` / `sse_nav.rs` — shared hit-test + navigation helpers
- `run/` — Startup wiring + Tokio tasks (Phase 4: extracted from `main.rs`).
  - `server.rs` — device-discovery fanout, `connection_task` with 2s→30s exponential reconnect backoff, `spawn_switch_app_handler`
  - `dispatch.rs` — `dispatch_client_message` (Log → LogStore, Net → NetworkStore, Hello re-arrival)
  - `render_loop.rs` — 30 Hz main-thread render loop
- `cli.rs` — CLI argument parsing (clap): `--port`, `--level`, `--tag`
- `session.rs` — Session persistence to `$XDG_CONFIG_HOME/flog/session.toml` (active_tab, filters, bookmarks)
- `main.rs` — Tokio async entry point, 93 lines. CLI parse → Arc<Mutex<App>> → device discovery → panic hook → TUI enter → `run::run_loop` → TUI leave → save session.

### Data Flow

1. flog_dart starts WS server on port 9753..9762 (first free) inside the Flutter App
2. flog TUI discovers devices via three parallel event sources under `transport/device_monitor/`: `adb track-devices`, macOS usbmuxd `Listen`, and localhost TCP+WS probe. Emits `DeviceEvent::{Added,Removed}` on a single channel.
3. `run::spawn_device_discovery` fans each device out to `connection_task`s, one per (device, port) pair in `[port, port+9]`. For each, `transport::resolve_transport_addr` yields a `TransportAddr`:
   - `Localhost { port }` → `ws://localhost:{port}` (macOS host / iOS sim)
   - `AdbForward { serial, port }` → `adb -s <serial> forward tcp:<local> tcp:<port>` then `ws://localhost:<local>` (Android)
   - `Usbmuxd { device_id, port }` → usbmuxd Connect tunnel then WS upgrade over tunnel (iOS real device)
4. `input::connect` / `connect_stream` performs the WS handshake + reads the first frame within 3 s; must be a `Hello`. Emits `ConnectorEvent::Connected(ClientInfo)` on success.
5. `ClientMessage::Log` → `run::dispatch_client_message` → `LogStore` (via parser chain if level/tag absent)
6. `ClientMessage::Net { msg: FlogNetKind }` → `dispatch_client_message` → `NetworkStore::process_message` (Req/Res/Err/Chunk/Done/Open/Send/Recv/Close)
7. Mock system: rules edited in the TUI → `MockRuleStore::to_json_string()` → `ConnectorHandle::send_mock_sync()` → `ServerMessage::MockSync` → `FlogMockInterceptor.updateRules()` in Dart
8. Replay: user hits Replay in Network detail → `ConnectorHandle::send_replay(method, url, headers, body)` → `ServerMessage::Replay` → Dart re-executes via the same Dio (may re-hit a mock); new entry tagged `EntrySource::Replay`
9. Session switch: picker click → `switch_app_tx` channel → `App::switch_to_app` → `ConnectorHandle::send_subscribe()` → Dart replays its `FlogStore` buffer as fresh Log/Net frames
10. SSE Merged View / WS Chat View: UI-only features (pure domain helpers in `domain/sse_merge.rs` + `domain/ws_chat.rs`), no transport involvement
11. Mouse events: `event::handle_mouse` → `detect::detect_click_region` (pure) → `apply::apply_click_region` (mutating). Two-phase dispatch per UI-009 / UI-041.
12. Renderer (`run::render_loop`) reads `App` at 30 Hz, clamps `scroll_offset`, renders to terminal. The **renderer is the scroll authority**.

### Concurrency Model

Tokio multi-threaded runtime. Device monitor + connector loop run in background task, producing `ConnectorEvent`s. Connector reader/writer tasks handle WS communication. Main thread polls terminal events. App state is behind `Arc<Mutex<App>>`.

### Scroll Model

Both Logs and Network use the same pattern. `auto_scroll` lives on `LogsViewState` (inside `app.logs`) and on `NetworkState` (inside `app.network`) — never on `App` directly.
- `move_up/down(n)` — viewport scroll (mouse wheel, PageUp/Down), moves offset + selected, disables auto_scroll
- `select_up/down(n)` — cursor move (j/k), moves only selected, disables auto_scroll
- `go_top/go_bottom` — Home/End
- **Renderer is the scroll authority** — clamps offset, detects bottom for auto_scroll resume
- Logcat-strict never-disturb: once the user scrolls off the tail, incoming logs never move the cursor; the Jump-to-Bottom pill + `new_logs_since_pause` counter surface buffered content waiting.

## flog_dart Dart Package

`flog_dart/` contains the Dart companion package published as [flog_dart](https://pub.dev/packages/flog_dart) on pub.dev.

- `Flog.init({int port = 9753})` — one-line bootstrap: starts `FlogServer` on the first free port in `[port, port+9]`, auto-detects app/package/version via `package_info_plus`, captures `debugPrint` + `FlutterError.onError` + `PlatformDispatcher.onError` (DART-023: `PackageInfo` errors now logged via `debugPrint`).
- `FlogLogger` — Structured `[LEVEL][Tag] message` logging. `verbose/debug/info/warning/error` + `v/d/i/w/e` shorthands, optional `error` + `stackTrace` params.
- `FlogDio` — Drop-in `Dio` replacement that auto-inserts `FlogMockInterceptor` + `FlogHttpInterceptor` at the front of the chain. `FlogDio.sse(...)` convenience returns `SseResponse` (`stream: Stream<String>` of parsed events) (DART-010 split out `flog_dio_sse.dart`).
- `FlogHttpInterceptor` — Dio interceptor for HTTP request/response logging (⚠ must be BEFORE response-modifying interceptors). DART-001/002 fixes: SSE parser now W3C-compliant; `SseEvent` + `FlogSseParser.wrapTyped` API added. DART-007 fixed byte-vs-char truncation; DART-008 fixed `_idMap` leak on early-reject; DART-027 added `_emitHttpCompletion` helper to dedupe ~30-line request-emit logic.
- `FlogMockInterceptor` — Dio interceptor that intercepts requests matching rules synced from the flog TUI over the WebSocket control channel (`{"type":"mock_sync","rules":"<json array>"}`). Matching is substring / first-match-wins / case-sensitive on URL; optional method filter (DART-013). DART-004 guard: onRequest is a no-op when `flogEnabled == false`.
- `FlogSseParser` — SSE stream wrapper with chunk-level logging. Typed API: `FlogSseParser.wrapTyped(stream, url:, method:, ...)` returning `Stream<SseEvent>`. See the flog_dart/test/ suite for the contract.
- `FlogWebSocket` — WebSocket wrapper with send/recv logging; stream is a true broadcast (DART-006).
- Protocol: direct WebSocket server on port 9753+ binding to the Flutter app itself. See [`docs/PROTOCOL.md`](docs/PROTOCOL.md) for the wire format.

### Tree-shaking / `flogEnabled`

`flogEnabled` is a compile-time constant. Default: `true` in debug, `false` in release (`!dart.vm.product`). Recognises `-DAPP_FLAVOR=release` as "off" and any other `APP_FLAVOR` as "on". Explicit `-DFLOG_ENABLED=true/false` wins over either derivation. When `false`, all flog code is eliminated by AOT tree-shaking — zero overhead in production.

### v0.8 forward reference

Audit DART-033 deferred seven SSE-subsystem design issues (layering mix, closure-variable state, UTF-8 decode cost, unbounded byte buffer, duplicate parser paths) to a dedicated `flog_dart v0.8` breaking release. See [`docs/PROTOCOL.md §9.1`](docs/PROTOCOL.md#91-flog_dart-v08-breaking-changes-dart-033-forward-ref). **Wire protocol will not change** — flog TUI 0.4.x will work against both v0.7.x and v0.8.x.

### Mock System

Mock rules are created in the flog TUI (Network tab → `M` to open mock rules panel). Rules define URL pattern, optional method filter, status code, response body, and optional delay. The TUI syncs rules to the running Dart app over the WebSocket control channel (`{"type":"mock_sync","rules":"<json array>"}`). `FlogMockInterceptor` (inserted automatically by `FlogDio`) intercepts matching requests and resolves with the canned response. Mocked requests are still logged and appear in the Network Inspector tagged as "Mocked".

## CI/CD

GitHub Actions (`release.yml`) builds on tag push (`v*`) for 5 targets: macOS x86_64/aarch64, Linux x86_64/aarch64, Windows x86_64. Artifacts are packaged and uploaded to GitHub Releases.

## Cleanup campaign (2026-04-22 → 2026-04-24)

The current repository shape is the product of a 5-phase cleanup campaign. Don't let the relatively small `main.rs` (93 lines) or the dense module directory tree mislead you — the design reasoning lives in `docs/superpowers/`:

- **Specs** — `docs/superpowers/specs/` — the design that led into each phase.
- **Plans** — `docs/superpowers/plans/` — the numbered task list per phase / step.
- **Audit** — `docs/superpowers/audit/00-index.md` — 115 findings classified A/B/D/E (all C resolved), with stable ids like `DOM-011`, `UI-009`, `TRANS-008`, `DART-033` that this file and source comments cite freely.
- **Journals** — `docs/superpowers/journal/phase*.md` — exit notes per phase.

When a code comment or doc line says `audit UI-041` / `DOM-003` / `TRANS-009`, that's the pointer to "why is the code shaped this way". Cross-reference before refactoring.
