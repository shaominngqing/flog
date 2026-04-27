# Modules

Per-module index of the flog codebase at the Phase 4 exit state. Pair this
with [ARCHITECTURE.md](ARCHITECTURE.md) for the dependency graph and
[PROTOCOL.md](PROTOCOL.md) for wire-format detail.

Each entry lists the module's path, purpose, key types, key functions,
dependency fan-out, tests, and the audit IDs that touch it. Ordering is
bottom-up: `transport/` → `input/` → `domain/` → `parser/` → `app/` →
`event/` → `run/` → `ui/` → top-level → `flog_dart/`.

---

## Transport layer — `src/transport/`

### `src/transport/mod.rs`

- **Purpose:** layer entry point; re-exports `start_discovery`,
  `DeviceEvent`, `resolve_transport_addr`, `TransportAddr`.
- **Key types:** none (pure re-export layer).
- **Dependencies:** its own submodules only.
- **Tests:** none directly; all coverage lives under the submodules.
- **Audit:** TRANS-009.

### `src/transport/device_monitor/mod.rs`

- **Purpose:** event-driven device discovery — orchestrates adb / usbmuxd /
  local sources and emits unified `DeviceEvent`s on a single channel.
- **Key types:** `Device`, `DeviceKind` (`Android` / `IosUsb { device_id }` /
  `Local`), `ConnectionMethod` (`Localhost` / `AdbForward { serial }` /
  `Usbmuxd { device_id }`), `DeviceEvent` (`Added(Device)` / `Removed(String)`),
  `DeviceTracker` (internal helper).
- **Key functions:** `start_discovery(port) -> UnboundedReceiver<DeviceEvent>`,
  `Device::connection_method()`.
- **Dependencies:** tokio, submodules.
- **Tests:** `tracker_tests.rs` — `DeviceTracker` Add / Remove dedup +
  drain contract.
- **Audit:** TRANS-015.

### `src/transport/device_monitor/adb_source.rs` (+ `tests.rs`)

- **Purpose:** reads `adb track-devices` stream, emits `DeviceEvent` for
  every Android device that transitions to "device" state. Spawns a
  dedicated process so it never polls.
- **Key functions:** `track(tx)` — fire-and-forget task launched from
  `start_discovery`.
- **Dependencies:** tokio, `device_monitor::{DeviceTracker, Device, DeviceKind}`.
- **Tests:** `adb_source/tests.rs`.

### `src/transport/device_monitor/usbmuxd_source.rs` (+ `tests.rs`, macOS only)

- **Purpose:** `Listen` on `/var/run/usbmuxd` for iOS USB attach / detach
  plist events. macOS-only (`#[cfg(target_os = "macos")]`).
- **Key functions:** `track(tx)`.
- **Tests:** `usbmuxd_source/tests.rs`.

### `src/transport/device_monitor/local_source.rs` (+ `tests.rs`)

- **Purpose:** probes `localhost:port` via TCP and a WebSocket Hello
  handshake to detect macOS host apps + iOS simulators. Synthesises a
  `Device { kind: DeviceKind::Local }` on success.
- **Key functions:** `probe(tx, port)`.
- **Tests:** `local_source/tests.rs`.

### `src/transport/resolve.rs`

- **Purpose:** pure function from `Device` to `TransportAddr`; the only
  module that knows the branch structure of "which transport for which
  device kind".
- **Key types:** `TransportAddr { Localhost { port } | AdbForward { serial, port } | Usbmuxd { device_id, port } }`,
  `ResolveError` (`#[non_exhaustive]`, empty today).
- **Key functions:** `resolve_transport_addr(&Device, port) -> Result<TransportAddr, ResolveError>`.
- **Dependencies:** `device_monitor::{Device, ConnectionMethod}`.
- **Tests:** in-source `mod tests` — one case per `DeviceKind` variant.
- **Audit:** TRANS-009.

### `src/transport/adb.rs`

- **Purpose:** shell out to `adb -s <serial> forward tcp:<local> tcp:<port>`
  / `adb forward --remove` for the Android transport. Owns the
  local-port allocator (pool at 19753..29752) so no two forward rules
  ever collide.
- **Key constants:** `ADB_LOCAL_PORT_POOL_BASE = 19753`,
  `ADB_LOCAL_PORT_POOL_SIZE = 10000`.
- **Key functions:** `setup_forward(serial, device_port) -> Option<u16>`,
  `remove_forward(serial, local_port)`, `allocate_local_port()`.
- **Dependencies:** tokio::process::Command.
- **Tests:** in-source `mod tests`.
- **Audit:** TRANS-002.

### `src/transport/usbmuxd.rs`

- **Purpose:** macOS usbmuxd protocol client — plist over
  `/var/run/usbmuxd` Unix socket. Implements the Connect request that
  opens a TCP tunnel to an iOS device over USB.
- **Key functions:** `connect_device(device_id, port) -> UnixStream`.
- **Dependencies:** tokio, plist.
- **Tests:** none in-source; exercised via `transport_usbmuxd` integration.

### `src/transport/flutter_logs.rs`

- **Purpose:** thin wrapper around `flutter logs --clear` for reading
  Flutter app logs via the CLI. **Currently unreferenced** by the rest
  of the crate; kept compiled to avoid bit-rot. See "Audit trail gaps"
  at the bottom of this file.

---

## Input layer — `src/input/`

### `src/input/mod.rs`

- **Purpose:** layer entry point. Re-exports `connect`, `connect_stream`,
  `ConnectorEvent`, `ConnectorHandle`, and the `protocol` submodule.
- **Dependencies:** none outside the submodules.

### `src/input/protocol.rs` (+ `protocol_tests.rs`)

- **Purpose:** serde-backed wire types for the flog ↔ flog_dart
  WebSocket protocol.
- **Key types:**
  - `ClientId = u64`
  - `ClientInfo { id, app, app_version, os, package_name, port, build_mode, connected_at, session_id }`
  - `ClientMessage { Hello { …ClientInfo fields } | Log { level, tag, message, error, stack_trace, timestamp } | Net { msg: FlogNetKind } }`
  - `ServerMessage { MockSync { rules } | Replay { method, url, headers, body } | Subscribe {} }`
- **Dependencies:** serde, `domain::network::FlogNetKind`.
- **Tests:** `protocol_tests.rs` — round-trip deserialize for every variant.
- **Audit:** TRANS-012, TRANS-014.

### `src/input/connector.rs`

- **Purpose:** WebSocket client that performs the Hello handshake and
  shuttles `ClientMessage` / `ServerMessage` frames. Internally spawns
  dedicated reader + writer Tokio tasks.
- **Key types:** `ConnectorHandle { tx: UnboundedSender<String> }`,
  `ConnectorEvent { Connected(ClientInfo) | Disconnected | Message(ClientMessage) }`.
- **Key functions:** `connect(&str) -> Result<(UnboundedReceiver<ConnectorEvent>, ConnectorHandle), …>`,
  `connect_stream<S>(S, &str) -> Result<…>` (for usbmuxd tunnel),
  `ConnectorHandle::send(ServerMessage) -> bool`, `send_mock_sync`,
  `send_replay`, `send_subscribe`, `for_testing()`.
- **Handshake:** 3-second timeout on the first frame; errors on timeout,
  non-text, non-Hello.
- **Tests:** in-source `mod tests` + `tests/ws_server_test_direct.rs`
  integration.
- **Audit:** TRANS-004, TRANS-005, TRANS-006.

---

## Domain layer — `src/domain/`

### `src/domain/mod.rs`

- **Purpose:** layer entry point.
- **Re-exports:** `InputSource`, `LogEntry`, `LogLevel`, `ParseResult`,
  `FilterState`, `FilterVariant`, `MessageFilter`, `NetworkFilter`,
  `NetworkStore`, `LogStore`.

### `src/domain/entry.rs`

- **Purpose:** core log data types.
- **Key types:** `LogLevel` (`System` < `Verbose` < `Debug` < `Info` <
  `Warning` < `Error`), `InputSource::DirectSocket`, `LogEntry {
  timestamp, level, tag, message, extra_lines, repeat_count, source,
  error, stacktrace }`, `ParseResult { NewEntry(LogEntry) | Ignored }`.
- **Key functions:** `LogLevel::as_str`, `LogLevel::from_str`,
  `LogEntry::full_message`, `LogEntry::same_signature`.
- **Tests:** in-source `mod tests`.

### `src/domain/store.rs`

- **Purpose:** 100 K-entry ring buffer with push-time + drain-time
  consecutive-duplicate folding.
- **Key type:** `LogStore`.
- **Key constant:** `MAX_ENTRIES = 100_000`.
- **Key functions:** `add_entry(LogEntry) -> usize` (returns drained
  count), `len`, `is_empty`, `get`, `iter`.
- **Helpers:** `fold_consecutive_duplicates(&mut VecDeque<LogEntry>)`.
- **Tests:** in-source `mod tests` — covers DOM-011 fold on drain,
  capacity boundary, 200 K-identical regression.
- **Audit:** DOM-011.

### `src/domain/filter.rs` (+ `filter_tests.rs`)

- **Purpose:** combined log filter (level + tag include/exclude + search +
  exclude) with compiled regex and plain-OR modes per text field.
- **Key type:** `FilterState { min_level, tag_include, tag_exclude,
  search_query, search_regex, exclude_query, exclude_regex, tag_regex,
  … + 4 compiled-regex/plain-vector companions }`.
- **Key functions:** `FilterState::matches(&LogEntry) -> bool`,
  `set_search`, `set_exclude`, `parse_tag_filter`,
  `search_positions`, `merge_overlapping_ranges` (pub(crate)).
- **Tests:** `filter_tests.rs` — exhaustive level / tag / search /
  exclude matrix + DOM-018 overlap regression.
- **Audit:** DOM-004, DOM-005, DOM-018.

### `src/domain/filter_traits.rs`

- **Purpose:** shared abstractions used by both log filters and network
  filters so OR-term parsing and pill-variant listing can be uniform.
- **Key types:** `trait FilterVariant` (`all`, `label`, `variants`),
  `trait MessageFilter` (checker-ish shape).
- **Dependencies:** none outside std.

### `src/domain/network.rs` (+ `network_tests.rs`)

- **Purpose:** network data types + the wire-facing `FlogNetKind` enum.
- **Key types:** `Protocol { Http | Sse | Ws }`,
  `NetworkStatus { Pending | Active | Completed | Failed | Orphan }`,
  `WsDirection { Send | Recv }`, `EntrySource { App | Replay | Mocked }`,
  `SseChunk { data }`, `WsMessage { direction, data, size }`,
  `NetworkEntry` (21-field record), `NetworkEntryBuilder`,
  `FlogNetKind { Req | Res | Err | Chunk | Done | Open | Send | Recv | Close }`
  (serde `#[serde(tag = "t", rename_all = "lowercase")]`).
- **Key functions:** `NetworkEntry::builder`, `new_http`, `new_sse`,
  `new_ws`, `new_orphan_response`, `display_size`; `FlogNetKind::id`.
- **Tests:** `network_tests.rs` — wire-format deserialization + id-dispatch.
- **Audit:** DOM-002, DOM-003, DOM-006, DOM-024, DOM-025.

### `src/domain/network_store.rs` (+ `network_store_tests.rs`)

- **Purpose:** 10 K-entry ring buffer; drives state transitions from
  `FlogNetKind` messages.
- **Key type:** `NetworkStore`.
- **Key functions:** `process_message(FlogNetKind)`, `len`, `iter`, `get`.
- **Key constant:** `MAX_ENTRIES = 10_000`.
- **Handlers:** `handle_req`, `handle_res`, `handle_err`, `handle_chunk`,
  `handle_done`, `handle_open`, `handle_ws_msg`, `handle_close`.
- **Tests:** `network_store_tests.rs` — every transition + orphan +
  Mocked / Replay source propagation.
- **Audit:** DOM-003 (orphan handling).

### `src/domain/network_filter.rs` (+ `network_filter_tests.rs`)

- **Purpose:** filter state for the Network tab.
- **Key types:** `ProtocolFilter`, `MethodFilter`, `StatusFilter` (each
  implements `FilterVariant`), `NetworkFilter { search_query, exclude_query,
  protocol, method, status, + compiled regex / plain parts }`.
- **Key functions:** `NetworkFilter::matches(&NetworkEntry) -> bool`,
  `set_search`, `set_exclude`, `search_positions`.
- **Tests:** `network_filter_tests.rs` — DOM-001 trait contract +
  every pill filter.
- **Audit:** DOM-001, DOM-019.

### `src/domain/mock.rs`

- **Purpose:** mock rule data types + their in-TUI store. Rules cross
  the wire as a JSON string on `ServerMessage::MockSync`.
- **Key types:** `MockRule { id, url_pattern, method, status_code,
  response_body, delay_ms, enabled, hit_count }`, `MockRuleStore`.
- **Key functions:** `MockRuleStore::add`, `update`, `remove`,
  `toggle`, `to_json_string`, `rules()`.
- **Audit:** DOM-007 (colocation decision), DART-013 (matching semantics).

### `src/domain/sse_merge.rs`

- **Purpose:** pure utilities for the SSE Merged View feature. UI calls
  these without side effects.
- **Key functions:** `extract_field_paths(&[SseChunk]) -> Vec<Vec<String>>`,
  `resolve_path(&Value, &[String]) -> Option<&str>`,
  `auto_detect_field(&[SseChunk]) -> Option<Vec<String>>`,
  `merge_field(&[SseChunk], &[String]) -> String`.

### `src/domain/ws_chat.rs`

- **Purpose:** pure utilities for the WS Chat View.
- **Key functions:** `extract_type(&str) -> Option<String>`,
  `has_binary_content(&str) -> bool`, `group_messages(&[WsMessage]) -> Vec<Group>`,
  `preview_message(&str) -> String`.

### `src/domain/json_tolerant.rs`

- **Purpose:** tolerant JSON parser for log message bodies that may be
  partially malformed (trailing commas, unquoted keys). Used by the
  JSON viewer colorizer for inline JSON preview in logs.

### `src/domain/structured_parser.rs`

- **Purpose:** structured-format parse helpers shared by the parser
  layer (DOM-008 split target; kept here because the helpers are pure
  domain concerns and the wrapping `Parser` trait impl lives in
  `parser/structured.rs`).

---

## Parser layer — `src/parser/`

### `src/parser/mod.rs`

- **Purpose:** multi-strategy parser chain.
- **Key types:** `trait LogLineParser` (`name`, `try_parse`),
  `MultiStrategyParser`.
- **Key functions:** `MultiStrategyParser::default_chain()`,
  `with_strategies(Vec<Box<dyn LogLineParser>>)`, `parse(line) -> ParseResult`.
- **Default order:** `[StructuredParser, GenericParser, KeywordParser]`.
- **Tests:** in-source `mod tests` — DOM-013 chain order + fall-through.
- **Audit:** DOM-013.

### `src/parser/structured.rs`

- **Purpose:** recognises `[LEVEL][Tag] msg` and
  `HH:MM:SS.mmm │ LEVEL │ Tag │ msg` pipe-format lines.
- **Key type:** `StructuredParser`.

### `src/parser/generic.rs` (+ `generic_tests.rs`)

- **Purpose:** recognises Flutter / VM Service patterns — `I/flutter (123): …`,
  `[INFO] [Tag] …`, timestamped DDS output, exception blocks.
- **Key type:** `GenericParser`.
- **Tests:** `generic_tests.rs`.

### `src/parser/keyword.rs`

- **Purpose:** fallback parser — infers level from keyword scan ("error"
  / "warn" / "debug"), tags everything `App`. Accepts any non-empty
  input so the chain never drops a line.
- **Key type:** `KeywordParser`.
- **Audit:** DOM-017.

### `src/parser/network.rs`

- **Purpose:** parses `flog_net`-tagged log lines as `FlogNetKind` JSON.
  Called by `run::dispatch_client_message` directly, not through the
  chain.
- **Key functions:** `try_parse_network(tag, message) -> Option<FlogNetKind>`.

### `src/parser/util.rs`

- **Purpose:** shared ANSI stripping + regex helpers used by the other
  parser strategies (DOM-015).

---

## Session + replay — top-level modules

### `src/session.rs` (+ `session_tests.rs`)

- **Purpose:** load / save `$XDG_CONFIG_HOME/flog/session.toml` —
  min_level, tag filter, search, exclude, bookmarks, active tab.
- **Key type:** `SessionData`.
- **Key functions:** `session_data_from_app(&App) -> SessionData`,
  `apply_session_data(&mut App, SessionData)`,
  `load_session(&mut App)`, `save_session(&App)`.
- **Tests:** `session_tests.rs` — level-byte round-trip + tag-filter
  reconstruction, DOM-021 / DOM-022.

### `src/replay.rs`

- **Purpose:** server-side HTTP replay. **Currently not wired up** —
  the active replay path uses `ConnectorHandle::send_replay` to ask
  the Dart side to re-execute. Kept for a possible future path where
  flog itself replays; entire module is `#[allow(dead_code)]`.

### `src/cli.rs`

- **Purpose:** clap-based CLI parser.
- **Flags:** `--port` (default 9753), `--level` (v/d/i/w/e), `--tag`.
- **Tests:** in-source `mod tests`.

### `src/lib.rs`

- **Purpose:** re-exports the minimum public surface needed by
  integration tests (`domain::*`, `parser::*`, `input::*`, `app::*`).

---

## App layer — `src/app/`

### `src/app/mod.rs` (+ sibling `src/app_tests.rs`)

- **Purpose:** `App` state machine root + public re-exports from submodules.
- **Key types:** `App` (the one mutable root), `AppMode { Normal |
  InputActive(InputField) | Help | Stats | MockRuleEdit }`, `ViewTab
  { Logs | Network }`, `InputField { LogSearch | LogExclude | LogTag |
  NetSearch | NetExclude }`.
- **Key functions:** `App::new`, `add_entry`, `filtered_indices`,
  `filtered_count`, `invalidate_filter`, `selected_store_index`,
  `compute_stats`, `clear_logs`, `insert_separator`, `export_logs`,
  `show_status`, `active_status`, `has_connected_client`;
  `InputField::tab`.
- **Invariants documented on `App`:** `active_app_id` ↔ `connected_apps`
  membership, device-picker scroll clamping, switch / remove semantics.
- **Audit:** UI-002, UI-003, UI-004, UI-006, UI-017, UI-040, UI-023.

### `src/app/state_structs.rs`

- **Purpose:** small sub-state structs.
- **Key types:** `SearchState`, `InputBuffers`, `StatsSnapshot`,
  `DetailState`, `LogsViewState`.

### `src/app/network_state.rs`

- **Purpose:** the Network-tab view state.
- **Key type:** `NetworkState` (selected, scroll_offset, auto_scroll,
  show_detail, show_mock_rules_panel, filter, collapsed_sections,
  json_viewer_states, sse_merge_rules, sse_merged_mode,
  sse_merged_field_idx, ws_chat_mode, internal filtered_indices cache).
- **Key functions:** `move_up/down`, `select_up/down`, `go_top/go_bottom`,
  `filtered_indices`, `filtered_count`, `invalidate_filter`.
- **Audit:** UI-004, UI-006.

### `src/app/multi_app.rs`

- **Purpose:** multi-app connection management.
- **Key types:** `ConnectedApp { id, device_id, port, device_name,
  app_name, app_version, os, package_name, build_mode, handle }`.
- **Key functions (on `App`):** `add_connected_app`,
  `remove_connected_app`, `switch_to_app`.
- **Audit:** UI-040.

### `src/app/mode.rs`

- **Purpose:** mode-transition helpers (`enter_input`, `exit_input`,
  `toggle_help`, `toggle_stats`, etc.).

### `src/app/scroll.rs`

- **Purpose:** Logs-tab scroll primitives. These are thin wrappers that
  now delegate into `app.logs` (the Phase 4 UI-003 completion).
- **Key functions:** `logs_move_up/down`, `logs_select_up/down`,
  `logs_go_top/go_bottom`.

### `src/app/layout_cache.rs`

- **Purpose:** per-frame rect snapshot used by mouse click detection
  (`detect_click_region` reads it, renderers populate it).
- **Key type:** `LayoutCache`.

### `src/app/mock_edit.rs`

- **Purpose:** mock-rule editor bundle; keeps URL field, method field,
  status field, body editor (via `ui::text_editor::TextEditor`), and
  the edit-mode flag in one struct.
- **Key type:** `MockEditState`.
- **Audit:** UI-026, UI-028, UI-034.

### `src/app/sse_merge.rs`

- **Purpose:** SSE Merged View rule types kept on `NetworkState`.
- **Key types:** `SseMergeRule`, `SsePathSegment`.

### `src/app/detail.rs`

- **Purpose:** helpers shared by both tabs' detail panels — body parsing,
  viewer-state lookup, fingerprint invalidation.

### `src/app/input_fields.rs`

- **Purpose:** shared dispatch for the 5 unified input fields — buffer
  selection by `InputField`, commit-on-blur, cursor handling.

---

## Event layer — `src/event/`

### `src/event/mod.rs`

- **Purpose:** top-level keyboard + mouse dispatch, fans out by
  `AppMode`.
- **Key functions:** `handle_key(&mut App, KeyEvent)`,
  `handle_mouse(&mut App, MouseEvent)`; private `handle_normal_key`,
  `handle_input_key`, `handle_overlay_key`, `handle_mock_edit_key`,
  `handle_normal_mouse`, `handle_input_mouse`, `handle_overlay_mouse`,
  `handle_mock_edit_mouse`.
- **Audit:** UI-007, UI-009.

### `src/event/click_region.rs`

- **Purpose:** `ClickRegion` enum — the pure-function output of mouse
  click detection (Phase 3 Step 3.6).
- **Key types:** `ClickRegion` (tabs / toolbar / list / detail / mock /
  status / scrollbar variants), `ClickClass { Single | Double }`,
  `ScrollDir { Up | Down }`, `Axis { Vertical | Horizontal }`.
- **Audit:** UI-041.

### `src/event/detect.rs`

- **Purpose:** `detect_click_region(&App, x, y) -> Option<ClickRegion>`
  + `classify_click` (single vs. double based on elapsed time +
  position delta).
- **Tests:** characterization_event_mouse.rs — full coverage.
- **Audit:** UI-041.

### `src/event/detect_net.rs`

- **Purpose:** Network-tab-specific slice of `detect_click_region` —
  carved out to keep `detect.rs` under the 500-line budget.

### `src/event/apply.rs`

- **Purpose:** `apply_click_region(&mut App, ClickRegion, ClickClass, x, y)` —
  the mutation phase.

### `src/event/apply_status.rs`

- **Purpose:** status-bar click handling (Logs / Network). Extracted
  from `apply.rs` for focus.

### `src/event/actions.rs`

- **Purpose:** side effect helpers invoked from key handlers: clipboard
  copy, cURL rendering, replay trigger, mock-from-selected.

### `src/event/keys.rs`

- **Purpose:** per-mode key handlers (`handle_normal_key` variants,
  per-tab keys, mock editor keys, input field keys). Largest
  event-layer file after `apply.rs`.

### `src/event/pills.rs`

- **Purpose:** named pill labels (`SseEventsPill`, `SseMergedPill`, etc.)
  shared by detection + rendering so hit-testing and drawing can't drift.

### `src/event/sse_nav.rs`

- **Purpose:** pure navigation math for SSE Merged field list (j/k
  index wrapping).

---

## Run layer — `src/run/`

### `src/run/mod.rs`

- **Purpose:** startup wiring + top-level Tokio tasks. Split out of
  `main.rs` in Phase 4.
- **Re-exports:** `run_loop`, `spawn_device_discovery`,
  `spawn_switch_app_handler`. Test-only: `dispatch_client_message`,
  `format_ts`, `split_stacktrace`, `RAW_LOG_RE`, reconnect constants.

### `src/run/server.rs`

- **Purpose:** spawns device-discovery → per-connection fanout; runs
  one `connection_task` per (device, port) with the retry loop.
- **Key constants:** `RECONNECT_INITIAL_DELAY_SECS = 2`,
  `RECONNECT_MAX_DELAY_SECS = 30`, `RECONNECT_BACKOFF_FACTOR = 2`,
  `PORT_SCAN_RANGE = 10`.
- **Key functions:** `spawn_device_discovery(app, device_rx, base_port)`,
  `spawn_switch_app_handler(app, switch_app_rx)`,
  `connection_task(device, target_port, key, app, adb_fwd)`.
- **Audit:** TRANS-008, TRANS-010, TRANS-011, TRANS-015.

### `src/run/dispatch.rs`

- **Purpose:** `dispatch_client_message(&mut App, ClientMessage)` —
  routes `Log` to `LogStore`, `Net` to `NetworkStore`, handles
  `Hello` (re-arrives on reconnect).
- **Key helpers:** `format_ts`, `split_stacktrace`, `RAW_LOG_RE`.

### `src/run/render_loop.rs`

- **Purpose:** main-thread 30 Hz render loop.
- **Key function:** `run_loop(&mut Terminal, &Arc<Mutex<App>>)`.

### Sibling tests

- `src/main_tests.rs` is reached via `#[path = "../main_tests.rs"]` from
  `run/mod.rs` — exercises `dispatch_client_message` + reconnect constants.

---

## UI layer — `src/ui/`

### `src/ui/mod.rs`

- **Purpose:** top-level dispatcher + shared Catppuccin Macchiato
  palette (`BASE`, `MANTLE`, `SURFACE0`, `SURFACE1`, `OVERLAY0`,
  `TEXT`, `SUBTEXT0`, `BLUE`, `SAPPHIRE`, `TEAL`, `GREEN`, `YELLOW`,
  `PEACH`, `RED`, `MAUVE`, `PINK`, `LAVENDER`).
- **Key functions:** `draw(&mut Frame, &mut App)`, `wrap_text`, `safe_pad`.

### `src/ui/tab_bar.rs`

- **Purpose:** ▤ Logs / ⇄ Network tab bar renderer, including the
  connection pulse dot and the app/device context label.

### `src/ui/logs/mod.rs`

- **Purpose:** Logs view orchestration — lays out toolbar, list, status
  bar, detail panel, empty states.
- **Submodules:** `toolbar`, `list`, `status_bar`, `detail/`,
  `empty_states`, `jump` (Jump-to-Bottom pill), `stats`, `highlight`.

### `src/ui/logs/toolbar.rs`

- **Purpose:** 2-row Logs toolbar — Search / Exclude / Tag input fields
  on row 1, level buttons (S/V/D/I/W/E) on row 2.

### `src/ui/logs/list.rs`

- **Purpose:** the log list renderer. Handles variable-height rows
  (separators are 3 rows, entries may wrap), timeline heatmap sidebar,
  tag pill colouring, match highlighting.

### `src/ui/logs/status_bar.rs`

- **Purpose:** Logs status bar — filter summary, connection indicator,
  keyboard hints.

### `src/ui/logs/detail/mod.rs` (+ `renderers.rs`, `section.rs`)

- **Purpose:** Logs detail panel using the shared `json_viewer`.

### `src/ui/logs/empty_states.rs`

- **Purpose:** renderers for "not connected" / "waiting for logs" /
  "no matching logs" / "jump to bottom" empty / informational states.

### `src/ui/logs/jump.rs`

- **Purpose:** Jump-to-Bottom pill rendering + click region.

### `src/ui/logs/stats.rs`

- **Purpose:** Logs stats overlay — level distribution, tag ranking.

### `src/ui/logs/highlight.rs`

- **Purpose:** auto-highlight rules (HTTP methods, status codes, URLs,
  durations, JSON fragments) for the log list.

### `src/ui/network/mod.rs`

- **Purpose:** Network view orchestration.
- **Submodules:** `filter` (2-line toolbar), `table`, `status_bar`,
  `detail/`, `mock_rules`, `stats`.

### `src/ui/network/filter.rs`

- **Purpose:** Network toolbar — Search / Exclude inputs + Protocol /
  Method / Status pills.

### `src/ui/network/table.rs`

- **Purpose:** the request list table.

### `src/ui/network/status_bar.rs`

- **Purpose:** Network status bar.

### `src/ui/network/detail/mod.rs`

- **Purpose:** Flipper-style detail with collapsible sections.
- **Submodules:** `general`, `shared`, `http_body`, `sse` (events /
  Merged modes + field picker), `ws` (Chat / Raw modes), `error`.

### `src/ui/network/mock_rules.rs`

- **Purpose:** mock rules side panel + edit overlay. Uses
  `ui::text_editor::TextEditor` for the JSON body field.

### `src/ui/network/stats.rs`

- **Purpose:** Network stats overlay — latency percentiles, top-5
  slowest, status distribution, per-domain breakdown.

### `src/ui/json_viewer/mod.rs`

- **Purpose:** shared AST-based collapsible JSON tree. The flat arena
  tree lives in `tree.rs`, fold state in `state.rs`, rendering in
  `render/`, and palette in `palette.rs`. `colorize.rs` is a separate
  raw-text JSON syntax highlighter for inline snippets in log messages.
- **Key types:** `Tree`, `JsonViewerState`, `RenderOutput`.
- **Key functions:** `parse_and_render`, `colorize_json_text`.

### `src/ui/input_field/` (+ `tests.rs`)

- **Purpose:** shared single-line input field widget — three-state
  background (idle / hover / active), viewport scroll for overflow,
  cursor glyph.
- **Key types:** `InputFieldProps`.
- **Audit:** UI-015, UI-033.

### `src/ui/text_editor/` (cursor / viewport / mod)

- **Purpose:** multi-line text editor component (state only, no
  rendering). Used by mock-rule body editor.
- **Audit:** UI-014.

### `src/ui/device_picker/` (card / click_map / modal / palette / row / mod)

- **Purpose:** device + app picker overlay opened from the status bar.
- **Audit:** UI-038 mirror.

### `src/ui/help/mod.rs` + `help/content/{logs,network}.rs`

- **Purpose:** help overlay with per-tab keyboard / mouse / filter /
  detail sections. Content lives under `content/`.
- **Audit:** UI-013, UI-014.

---

## Top-level entry — `src/main.rs`

- **Purpose:** Tokio async entry — parses CLI, creates `Arc<Mutex<App>>`,
  starts device discovery, spawns the discovery + switch-app handlers,
  installs the panic hook, enters the TUI alt-screen, hands off to
  `run::run_loop`, and saves the session on exit.
- **Line count:** 93 lines (Phase 4 extraction target).

---

## Integration tests — `tests/`

| File                                           | Coverage                                                      |
|------------------------------------------------|---------------------------------------------------------------|
| `characterization_app_state.rs`                | App state machine transitions — AppMode / tabs / scroll       |
| `characterization_bugs.rs`                     | B-class regression fence from Phase 2.5B                      |
| `characterization_event_keys.rs`               | Every keyboard shortcut in every mode                         |
| `characterization_event_mouse.rs`              | Mouse click regions + double-click + scroll                   |
| `characterization_input.rs`                    | Unified input-field behaviour                                 |
| `characterization_ui_logs.rs`                  | Logs render snapshots + Jump-to-Bottom                        |
| `characterization_ui_network.rs`               | Network render snapshots + Mock + detail pills                |
| `characterization_ui_source_select_help.rs`    | Device picker + help overlay snapshots                        |
| `ws_server_test_direct.rs`                     | End-to-end Connector round-trip (Hello + Log + Net)           |
| `support/fake_flog_server.rs`                  | In-process fake flog_dart WS server for the test above        |
| `support/fixtures.rs`                          | `LogEntry` / `NetworkEntry` / `FlogNetKind` factories         |
| `support/ui_inspect.rs`                        | ratatui `TestBackend` helpers (buffer → string, colour extract) |

---

## flog_dart companion package — `flog_dart/lib/`

### `flog_dart/lib/flog_dart.dart`

- **Purpose:** library barrel + `Flog` entry-point class + `FlogLogger`.
- **Key classes:** `Flog` (with `init({int port = 9753})`), `FlogLogger`
  (tag-keyed structured logger with `verbose/debug/info/warning/error`
  + `v/d/i/w/e` shorthands, `error` + `stackTrace` named params).
- **Exports:** `FlogServer`, `FlogStore`, `FlogHttpInterceptor`,
  `FlogMockInterceptor`, `FlogSseParser`, `FlogWebSocket`, `FlogDio`,
  `FlogHttpConfig`, `SseResponse`, and `flogEnabled`. `nextNetId` +
  `emitNet` remain exported for v0.x compat but are marked
  `@internal` (DART-021).
- **Audit:** DART-022, DART-023.

### `flog_dart/lib/src/flog_server.dart`

- **Purpose:** WebSocket server that accepts connections from flog TUI.
  Supports multiple simultaneous TUI clients. Each new client gets a
  full replay of the buffered messages from `FlogStore`, then transitions
  to live.
- **Key class:** `FlogServer` (singleton via `.instance`).
- **Key methods:** `start({int port})`, `send(Map<String, dynamic>)`,
  `updateAppInfo(...)`.
- **Audit:** DART-015, DART-016, DART-017, DART-022, DART-005.

### `flog_dart/lib/src/flog_store.dart`

- **Purpose:** ring buffer (capacity 50 000) that stores all flog
  messages so newly-subscribing TUIs get full history.
- **Key class:** `FlogStore`.
- **Audit:** DART-020.

### `flog_dart/lib/src/flog_net.dart`

- **Purpose:** internal `nextNetId` + `emitNet` helpers +
  `flogEnabled` compile-time constant.
- **Key constant:** `flogEnabled` — derived from `dart.vm.product`
  with optional `-DFLOG_ENABLED=...` and `-DAPP_FLAVOR=...` overrides.
- **Audit:** DART-009, DART-021.

### `flog_dart/lib/src/flog_dio.dart`

- **Purpose:** `FlogDio` — drop-in `Dio` replacement. Automatically
  inserts `FlogMockInterceptor` + `FlogHttpInterceptor` at the front
  of the chain so business interceptors added later cannot hide
  responses from flog.
- **Key class:** `FlogDio` (extends / delegates to `Dio`);
  `FlogHttpConfig`; `SseResponse`.
- **Convenience:** `sse(path, {method, data, headers})` returns
  `SseResponse` with a `Stream<String>` of parsed events.
- **Audit:** DART-010.

### `flog_dart/lib/src/flog_dio_sse.dart`

- **Purpose:** SSE convenience split out of `flog_dio.dart` (DART-010).
- **Key function:** `flogSse(dio, path, …) -> SseResponse`.

### `flog_dart/lib/src/flog_http_interceptor.dart`

- **Purpose:** Dio interceptor that emits Req / Res / Err for every
  HTTP request. Must be inserted before response-modifying business
  interceptors.
- **Key class:** `FlogHttpInterceptor`.
- **Audit:** DART-007, DART-008, DART-027.

### `flog_dart/lib/src/flog_mock_interceptor.dart`

- **Purpose:** Dio interceptor that intercepts requests matching mock
  rules synced from the TUI over `mock_sync`. Matching is substring,
  first-match-wins, case-sensitive on the URL; optional method filter.
- **Key class:** `FlogMockInterceptor`.
- **Key function:** `FlogMockInterceptor.updateRules(jsonString)`.
- **Audit:** DART-004, DART-011, DART-012, DART-013, DART-014.

### `flog_dart/lib/src/flog_sse_parser.dart`

- **Purpose:** SSE parser — W3C-compliant multi-line `data:` join,
  typed `SseEvent` API, `wrapTyped` for structured consumption.
- **Key class:** `FlogSseParser`.
- **Key types:** `SseEvent { id, event, data, retry }`.
- **Key static:** `FlogSseParser.wrapTyped(stream, {url, method, requestId, requestHeaders})`.
- **Audit:** DART-001, DART-002 (parser rewrite).

### `flog_dart/lib/src/flog_web_socket.dart`

- **Purpose:** WebSocket wrapper that streams outbound `send`s and
  inbound frames to flog.
- **Key class:** `FlogWebSocket`.
- **Key methods:** `FlogWebSocket(uri)`, `FlogWebSocket.fromChannel(ch)`,
  `send`, `close`, `stream` (broadcast).
- **Audit:** DART-006, DART-018, DART-019.

### `flog_dart/test/`

- **Purpose:** `dart test`-driven test suite for the Dart package.
- **Committed per Phase 1 decision (DART-002 option A):** the test
  suite exists as the authoritative SSE parser contract; Phase 3
  Step 3.4 made it pass.

---

## Audit trail gaps

Written during Phase 5 doc pass; do NOT fix here.

- **TRANS-016 (new, Phase 5 doc review)** —
  `src/transport/flutter_logs.rs` defines `FlutterLogs` but is
  unreferenced by any other module. Either remove the file or wire it
  into a "read Flutter logs for a chosen device" source. Classified
  **E** (mechanical dead-code removal) once triaged.

- **TRANS-017 (new, Phase 5 doc review)** —
  `src/transport/mod.rs` does not re-export `flutter_logs` either, so
  the module is presently compiled-but-dead code reachable only via
  `crate::transport::flutter_logs::FlutterLogs` (pub path). Follow-up
  to TRANS-016.

Everything else referenced in this document exists at HEAD
(`7aaed95`) and is exercised by at least one test (or is doc-only
state like a palette constant).
